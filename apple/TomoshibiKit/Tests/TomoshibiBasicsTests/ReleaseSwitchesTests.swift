import XCTest

/*
 * Every switch a test drives the apps by is read from the environment, and a
 * release build must read none of them: the released Mac app can be started
 * with any environment by whoever can start it, and a switch it honoured
 * there once let a process with no permissions of its own invite a stranger
 * into the person's meeting and click the app's own share and camera
 * buttons (HostDebug), load YouTube's API from anywhere into a page with the
 * deployment's origin (WatchDebug), or have the person hear nobody in any
 * call (TOMOSHIBI_MUTE).
 *
 * So the rule, held here over the sources: every reading of the process's
 * environment sits inside `#if DEBUG`, where the release build does not
 * compile it, and is then gated on `Rehearsal.running` as well. The one
 * exception is the simulator's own SIMULATOR_MODEL_IDENTIFIER, which says
 * what a simulator pretends to be and exists nowhere else.
 * apple/check-release.sh holds the release binary itself to the same rule,
 * by the names left in it.
 */
final class ReleaseSwitchesTests: XCTestCase {
    private static let apple = URL(fileURLWithPath: #filePath)
        .deletingLastPathComponent() // TomoshibiBasicsTests
        .deletingLastPathComponent() // Tests
        .deletingLastPathComponent() // TomoshibiKit
        .deletingLastPathComponent() // apple

    /// What the apps ship: the package's sources and the targets' own.
    private static let roots = ["TomoshibiKit/Sources", "App", "Messages", "Broadcast"]

    func testEveryReadingOfTheEnvironmentIsCompiledIntoADebugBuildAlone() throws {
        var read = 0
        var outside: [String] = []
        for root in Self.roots {
            let folder = Self.apple.appendingPathComponent(root)
            guard let walk = FileManager.default.enumerator(at: folder, includingPropertiesForKeys: nil) else { continue }
            for case let file as URL in walk where file.pathExtension == "swift" {
                let source = try String(contentsOf: file, encoding: .utf8)
                let found = Self.readings(in: source)
                read += found.all
                outside += found.outside.map { "\(root)/\(file.path.dropFirst(folder.path.count + 1)):\($0)" }
            }
        }
        // The scan found something to judge, so a path that moved does not
        // pass by reading nothing.
        XCTAssertGreaterThan(read, 40, "the sources were not where this looked")
        XCTAssertEqual(outside, [], "read from the environment outside #if DEBUG, so a release build reads it too")
    }

    /// The scanner itself, on sources whose answer is known: a guard that
    /// passes whatever it is shown is worse than none.
    func testTheScanSeesEveryWayOutOfADebugBlock() {
        let source = """
        let a = ProcessInfo.processInfo.environment["ONE"]
        #if DEBUG
        let b = ProcessInfo.processInfo.environment["TWO"]
        #if os(macOS)
        let c = ProcessInfo.processInfo.environment["THREE"]
        #endif
        #else
        let d = ProcessInfo.processInfo.environment["FOUR"]
        #endif
        #if !DEBUG
        let e = ProcessInfo.processInfo.environment["FIVE"]
        #endif
        #if DEBUG || os(iOS)
        let f = ProcessInfo.processInfo.environment["SIX"]
        #elseif DEBUG
        let g = ProcessInfo.processInfo.environment["SEVEN"]
        #endif
        #if DEBUG && os(macOS)
        let h = getenv("EIGHT")
        #endif
        let i = getenv("NINE")
        // ProcessInfo.processInfo.environment in a comment is no reading
        /* nor ProcessInfo.processInfo.environment
           in a block */
        let j = ProcessInfo.processInfo.environment["SIMULATOR_MODEL_IDENTIFIER"]
        func k(_ environment: [String: String] = ProcessInfo.processInfo.environment) {}
        """
        let found = Self.readings(in: source)
        XCTAssertEqual(found.all, 11)
        XCTAssertEqual(found.outside, [1, 8, 11, 14, 21, 26])
    }

    /// The lines that read the environment, and those of them a release
    /// build compiles: outside every branch whose condition requires DEBUG.
    ///
    /// A branch requires DEBUG when its condition is `DEBUG` or a chain of
    /// `&&` with `DEBUG` in it; an `#else`, a `!DEBUG` or an `||` does not,
    /// which errs towards calling a reading unguarded rather than guarded.
    static func readings(in source: String) -> (all: Int, outside: [Int]) {
        var branches: [Bool] = []
        var inComment = false
        var all = 0
        var outside: [Int] = []
        for (index, raw) in source.components(separatedBy: "\n").enumerated() {
            var line = raw
            if inComment {
                guard let end = line.range(of: "*/") else { continue }
                line = String(line[end.upperBound...])
                inComment = false
            }
            while let start = line.range(of: "/*") {
                if let end = line.range(of: "*/", range: start.upperBound..<line.endIndex) {
                    line.removeSubrange(start.lowerBound..<end.upperBound)
                } else {
                    line = String(line[..<start.lowerBound])
                    inComment = true
                }
            }
            if let comment = line.range(of: "//") { line = String(line[..<comment.lowerBound]) }
            let said = line.trimmingCharacters(in: .whitespaces)
            if said.hasPrefix("#if ") {
                branches.append(requiresDebug(String(said.dropFirst(4))))
            } else if said.hasPrefix("#elseif "), !branches.isEmpty {
                branches[branches.count - 1] = requiresDebug(String(said.dropFirst(8)))
            } else if said.hasPrefix("#else"), !branches.isEmpty {
                branches[branches.count - 1] = false
            } else if said.hasPrefix("#endif"), !branches.isEmpty {
                branches.removeLast()
            } else if line.contains("processInfo.environment") || line.contains("getenv(") {
                all += 1
                if !branches.contains(true), !line.contains("SIMULATOR_MODEL_IDENTIFIER") { outside.append(index + 1) }
            }
        }
        return (all, outside)
    }

    private static func requiresDebug(_ condition: String) -> Bool {
        guard !condition.contains("||") else { return false }
        return condition.components(separatedBy: "&&").contains {
            $0.trimmingCharacters(in: .whitespaces.union(CharacterSet(charactersIn: "()"))) == "DEBUG"
        }
    }
}
