import Observation
import SwiftUI

/// One window's day and night on the iPhone: the system's appearance as the
/// window reports it, kept where SwiftUI sees it change, and what the window
/// itself is told to take while a call holds it.
///
/// The screens around a call are told their appearance outright (AppRoot
/// `outside`), so that the screen arriving after a call is right from its
/// first frame rather than drawn in the call's dark until the window lets
/// go of it. That appearance used to be read from the screen's traits, in a
/// computed property SwiftUI could not observe, and so only when the
/// window's own appearance happened to redraw the root. On its way into the
/// background the app is photographed in the other appearance as well, for
/// the app switcher and for coming back after the system has changed: the
/// system turns the scene and the window to the other appearance for that
/// photograph and leaves the screen's alone, so the screens were drawn in
/// the old appearance in it. Coming back after the system had turned dark,
/// the lobby slid in light and only then went dark, and the other way about
/// (recorded on the simulator, iOS 27.1). The screen's traits were read
/// because the scene's were no use during a call: SwiftUI's
/// `preferredColorScheme` takes the call's dark by overriding the scene's
/// appearance, and the scene then says the call's rather than the system's.
///
/// Now the window is told the call's appearance itself
/// (WindowLookReader), which leaves the scene saying the system's; the
/// window and its scene report every change, the window's word taken while
/// it follows the system and the scene's while the call holds it; and this
/// is observable, so a change redraws the screens in the same pass that
/// changed the window, the photograph's among them.
@MainActor
@Observable
public final class WindowLook {
    /// The system's appearance as this window last reported it; nil until
    /// the window has been put on the screen.
    public private(set) var system: ColorScheme?

    public init() {}

    /// What the window is told to take: the call's appearance while the call
    /// holds the window, and nothing otherwise, which is the system's. Never
    /// the system's appearance written out: a window told "light" because
    /// the system was light when it was told stays light when the system
    /// turns dark.
    public nonisolated static func window(holdsCall: Bool, meeting: ColorScheme?) -> ColorScheme? {
        holdsCall ? meeting : nil
    }

    /// What the window says of itself and of its scene, each nil where it
    /// says neither (an unspecified style), and whether the window has been
    /// told an appearance of its own.
    ///
    /// The window's own word while it follows the system: it is what the
    /// keyboard and the status bar over the screens are drawn in, and it
    /// turns with the scene for the photograph in the other appearance,
    /// which the screens have to be drawn in for that photograph to be
    /// right. While the window takes the call's appearance its word is the
    /// call's, and the scene's is the system's. A report that says nothing
    /// leaves the last word standing, rather than falling to the dark an
    /// unspecified style resolves the palette to.
    public func report(window: ColorScheme?, scene: ColorScheme?, windowTold: Bool) {
        guard let now = Self.system(window: window, scene: scene, windowTold: windowTold), now != system else { return }
        system = now
    }

    nonisolated static func system(window: ColorScheme?, scene: ColorScheme?, windowTold: Bool) -> ColorScheme? {
        windowTold ? scene : (window ?? scene)
    }

    /// The appearance of the screens around a call: the system's as the
    /// window reported it, or, before it has, what the window was drawn in,
    /// which is the system's until a call has held it.
    public func outside(window: ColorScheme) -> ColorScheme {
        system ?? window
    }
}
