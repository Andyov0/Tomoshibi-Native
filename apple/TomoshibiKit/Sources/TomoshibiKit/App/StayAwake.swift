import Foundation
#if os(iOS)
import UIKit
#endif

/// Keeps the screen lit and the machine awake for as long as a call is on,
/// and lets go the moment it is not.
///
/// Nothing did: an iPhone whose owner only watched somebody's share locked
/// itself after its auto-lock time, and the camera and every picture stopped
/// with it, leaving the sound; a Mac's display went to sleep in the middle of
/// a call, and with the window closed and nothing playing, App Nap could slow
/// the app until the call's heartbeat was missed and it joined all over
/// again. Android keeps the screen on and Windows asks the system the same
/// for a call, as a browser holds a wake lock for a page playing a call.
///
/// On an iPhone or an iPad the idle timer is turned off, which only holds
/// while the app is in front, as it should; on a Mac an activity is begun
/// that keeps the display awake and the app out of App Nap.
@MainActor
final class StayAwake {
    private(set) var holding = false
    #if os(macOS)
    private var activity: NSObjectProtocol?
    #endif

    func hold(_ wanted: Bool) {
        guard wanted != holding else { return }
        holding = wanted
        #if os(iOS)
        UIApplication.shared.isIdleTimerDisabled = wanted
        #else
        if wanted {
            activity = ProcessInfo.processInfo.beginActivity(options: [.userInitiated, .idleDisplaySleepDisabled],
                                                             reason: "A call is on")
        } else if let activity {
            ProcessInfo.processInfo.endActivity(activity)
            self.activity = nil
        }
        #endif
    }
}
