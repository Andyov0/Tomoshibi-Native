#if os(macOS)
@testable import TomoshibiBasics
import XCTest

/*
 * What a Mac build must be to replace the running app (UpdateTrust): the
 * team's Developer ID, for this app, intact, notarized; and in a release
 * build nothing else, whatever the environment says. What these guard is an
 * update that installs an app nobody here made, or a release build that a
 * variable left in somebody's shell talks into accepting a development one.
 *
 * The released app is the real thing to check against: /Applications/Tomoshibi.app
 * where it is installed (read, never changed), or TOMOSHIBI_TEST_RELEASED_APP.
 * Run the release half with
 *     swift test -c release -Xswiftc -enable-testing --filter UpdateTrustTests
 */
final class UpdateTrustTests: XCTestCase {
    let id = "app.tomoshibi.mac"
    var scratch: URL!

    override func setUpWithError() throws {
        scratch = FileManager.default.temporaryDirectory.appendingPathComponent("trust-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: scratch, withIntermediateDirectories: true)
    }

    override func tearDownWithError() throws {
        try? FileManager.default.removeItem(at: scratch)
    }

    func testOnlyADebugBuildCanBeToldToAcceptADevelopmentSignature() {
        let told = ["TOMOSHIBI_DEBUG_UPDATE_DEVELOPMENT_SIGNED": "1"]
        XCTAssertEqual(UpdateTrust.signing(environment: [:]), .developerID)
        XCTAssertEqual(UpdateTrust.signing(environment: ["TOMOSHIBI_DEBUG_UPDATE_DEVELOPMENT_SIGNED": "yes"]), .developerID)
        #if DEBUG
        XCTAssertEqual(UpdateTrust.signing(environment: told), .development)
        #else
        XCTAssertEqual(UpdateTrust.signing(environment: told), .developerID, "a release build was talked out of Developer ID")
        #endif
    }

    func testTheRequirementIsTheTeamsDeveloperIDForThisApp() {
        let said = UpdateTrust.requirement(.developerID, identifier: id)
        XCTAssertTrue(said.contains("identifier \"app.tomoshibi.mac\""))
        XCTAssertTrue(said.contains("certificate 1[field.1.2.840.113635.100.6.2.6] exists"), "not the Developer ID intermediate")
        XCTAssertTrue(said.contains("certificate leaf[field.1.2.840.113635.100.6.1.13] exists"), "not a Developer ID Application leaf")
        XCTAssertTrue(said.contains("certificate leaf[subject.OU] = \"SM86NB3CPZ\""))
        var requirement: SecRequirement?
        XCTAssertEqual(SecRequirementCreateWithString(said as CFString, [], &requirement), errSecSuccess)
    }

    func testAnAppWithoutTheTeamsSignatureIsRefused() throws {
        let unsigned = try bundle("Unsigned")
        try run("/usr/bin/codesign", "--remove-signature", unsigned.appendingPathComponent("Contents/MacOS/Tomoshibi").path)
        XCTAssertEqual(UpdateTrust.refusal(of: unsigned, identifier: id, signing: .developerID), .unsigned)

        let adHoc = try bundle("AdHoc")
        try run("/usr/bin/codesign", "--force", "--sign", "-", "--identifier", id, adHoc.path)
        XCTAssertEqual(UpdateTrust.refusal(of: adHoc, identifier: id, signing: .developerID), .notOurs)
        XCTAssertFalse(UpdateTrust.notarized(adHoc))

        // Apple's own signature is Apple's, not this team's Developer ID.
        let apple = URL(fileURLWithPath: "/System/Applications/Calculator.app")
        XCTAssertEqual(UpdateTrust.refusal(of: apple, identifier: "com.apple.calculator", signing: .developerID), .notOurs)
        // Accepted by Gatekeeper, as the system's own: not notarized, which
        // is what this is asked.
        XCTAssertFalse(UpdateTrust.notarized(apple), "accepted was taken for notarized")
    }

    func testTheReleasedAppIsAcceptedAndOnlyAsItself() throws {
        let released = try releasedApp()
        XCTAssertNil(UpdateTrust.refusal(of: released, identifier: id, signing: .developerID))
        XCTAssertTrue(UpdateTrust.notarized(released))
        XCTAssertEqual(UpdateTrust.refusal(of: released, identifier: "app.tomoshibi.other", signing: .developerID), .notOurs,
                       "another app of the same team was taken for this one")
        #if DEBUG
        XCTAssertEqual(UpdateTrust.refusal(of: released, identifier: id, signing: .development), .notOurs,
                       "a Developer ID build met the development requirement")
        #endif
    }

    func testAnotherTeamsDeveloperIDAppIsRefusedEvenAsItself() throws {
        // Any app on this Mac signed with somebody else's Developer ID.
        let apps = (try? FileManager.default.contentsOfDirectory(at: URL(fileURLWithPath: "/Applications"), includingPropertiesForKeys: nil)) ?? []
        for app in apps where app.pathExtension == "app" {
            guard let (identifier, team) = signer(of: app), team != UpdateTrust.team,
                  UpdateTrust.notarized(app) else { continue }
            XCTAssertEqual(UpdateTrust.refusal(of: app, identifier: identifier, signing: .developerID), .notOurs,
                           "\(app.lastPathComponent), signed by \(team), was accepted")
            return
        }
        throw XCTSkip("no other team's notarized app in /Applications")
    }

    func testAReleasedAppChangedAfterSigningIsRefused() throws {
        let copy = scratch.appendingPathComponent("Tomoshibi.app")
        try run("/usr/bin/ditto", try releasedApp().path, copy.path)
        let resource = try XCTUnwrap(FileManager.default.enumerator(at: copy.appendingPathComponent("Contents/Resources"), includingPropertiesForKeys: nil)?
            .compactMap { $0 as? URL }.first { !$0.hasDirectoryPath })
        let handle = try FileHandle(forWritingTo: resource)
        try handle.seekToEnd()
        try handle.write(contentsOf: Data([0]))
        try handle.close()
        XCTAssertEqual(UpdateTrust.refusal(of: copy, identifier: id, signing: .developerID), .broken)
    }

    // MARK: -

    private func releasedApp() throws -> URL {
        let path = ProcessInfo.processInfo.environment["TOMOSHIBI_TEST_RELEASED_APP"] ?? "/Applications/Tomoshibi.app"
        guard FileManager.default.fileExists(atPath: path) else { throw XCTSkip("no released Tomoshibi.app at \(path)") }
        return URL(fileURLWithPath: path)
    }

    private func signer(of app: URL) -> (String, String)? {
        var code: SecStaticCode?
        guard SecStaticCodeCreateWithPath(app as CFURL, [], &code) == errSecSuccess, let code else { return nil }
        var info: CFDictionary?
        guard SecCodeCopySigningInformation(code, SecCSFlags(rawValue: kSecCSSigningInformation), &info) == errSecSuccess,
              let info = info as? [String: Any],
              let identifier = info[kSecCodeInfoIdentifier as String] as? String,
              let team = info[kSecCodeInfoTeamIdentifier as String] as? String else { return nil }
        return (identifier, team)
    }

    /// A one-file app bundle whose executable is a copy of /usr/bin/true.
    private func bundle(_ name: String) throws -> URL {
        let app = scratch.appendingPathComponent("\(name)/Tomoshibi.app")
        let macOS = app.appendingPathComponent("Contents/MacOS")
        try FileManager.default.createDirectory(at: macOS, withIntermediateDirectories: true)
        try FileManager.default.copyItem(atPath: "/usr/bin/true", toPath: macOS.appendingPathComponent("Tomoshibi").path)
        let info: [String: Any] = ["CFBundleIdentifier": id, "CFBundleExecutable": "Tomoshibi", "CFBundlePackageType": "APPL"]
        let plist = try PropertyListSerialization.data(fromPropertyList: info, format: .xml, options: 0)
        try plist.write(to: app.appendingPathComponent("Contents/Info.plist"))
        return app
    }

    private func run(_ tool: String, _ arguments: String...) throws {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: tool)
        process.arguments = arguments
        process.standardOutput = FileHandle.nullDevice
        process.standardError = FileHandle.nullDevice
        try process.run()
        process.waitUntilExit()
        XCTAssertEqual(process.terminationStatus, 0, "\(tool) \(arguments.joined(separator: " "))")
    }
}
#endif
