#if os(iOS)
import TomoshibiBasics
import UIKit

/// The iPhone's builds, which TestFlight installs: "Update" opens TestFlight on
/// this app's page, and nothing is downloaded or installed here (Updates
/// `elsewhere`). The beta's own address, which opens the TestFlight app, or
/// where TestFlight is not installed the same page on the web, which says how
/// to get it.
@MainActor
final class TestFlightInstaller: UpdateInstaller {
    /// The app's Apple ID in App Store Connect.
    static let app = "6823784130"

    var misplacement: Misplacement? { nil }
    var elsewhere: Bool { true }

    func openElsewhere() {
        guard let beta = URL(string: "itms-beta://beta.itunes.apple.com/v1/app/\(Self.app)"),
              let web = URL(string: "https://beta.itunes.apple.com/v1/app/\(Self.app)") else { return }
        UIApplication.shared.open(beta) { opened in
            if !opened { UIApplication.shared.open(web) }
        }
    }

    /// Never asked: Updates opens TestFlight instead of downloading.
    func install(_ file: URL, version: String, progress: @escaping @MainActor (Double) -> Void,
                 shown: @escaping @MainActor () async -> Void) async -> Installation {
        .stopped
    }

    func reveal() {}
    func stop() {}
}
#endif
