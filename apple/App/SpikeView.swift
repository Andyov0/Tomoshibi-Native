import SwiftUI
import TomoshibiCore

/// The first screen of the apps for now: proves the core links and runs in an
/// app process by joining a room and sending test media from it.
///
/// TOMOSHIBI_AUTOJOIN=<room> (and optionally TOMOSHIBI_BASE) joins on launch,
/// so a script can drive it without a pointer; what happens is printed too.
struct SpikeView: View {
    @State private var base = ProcessInfo.processInfo.environment["TOMOSHIBI_BASE"] ?? "http://127.0.0.1:8080"
    @State private var room = ProcessInfo.processInfo.environment["TOMOSHIBI_AUTOJOIN"] ?? "spike"
    @State private var status = "core \(coreVersion())"
    @State private var busy = false

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Tomoshibi").font(.title.bold())
            TextField("Deployment", text: $base).textFieldStyle(.roundedBorder)
            TextField("Room", text: $room).textFieldStyle(.roundedBorder)
            Button(busy ? "In the room…" : "Join for 30 seconds", action: join)
                .disabled(busy)
            Text(status).font(.callout.monospaced()).textSelection(.enabled)
        }
        .padding(24)
        .frame(minWidth: 360)
        .task {
            if ProcessInfo.processInfo.environment["TOMOSHIBI_AUTOJOIN"] != nil { join() }
        }
    }

    private func join() {
        busy = true
        say("joining \(room) at \(base)")
        Task {
            do {
                let seconds = UInt32(ProcessInfo.processInfo.environment["TOMOSHIBI_SECONDS"] ?? "") ?? 30
                let report = try await spikeJoin(baseUrl: base, room: room, name: "Native App", seconds: seconds)
                say("left \(report.room) as \(report.identity), published \(report.published), saw \(report.othersSeen)")
            } catch {
                say("failed: \(error)")
            }
            busy = false
        }
    }

    private func say(_ line: String) {
        status = line
        // Standard error is unbuffered, so a script watching a launched app
        // sees each line as it happens rather than when the process exits.
        FileHandle.standardError.write(Data("SPIKE \(line)\n".utf8))
    }
}
