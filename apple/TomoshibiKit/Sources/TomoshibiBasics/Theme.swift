import QuartzCore
import SwiftUI

/// The apps' palette and motion.
///
/// Dark in every system appearance, on purpose: a call is looked at for an
/// hour at a stretch and the pictures are the only thing in it that should be
/// bright. The neutrals are the system's own dark greys (the owner found the
/// web's warm browns muddy under glass, 2026-10-15), so the glass over a
/// picture reads as clear rather than tinted. There is one signal colour,
/// the "tally" a studio camera lights when it is live — speaking, sharing,
/// unread all wear it, so only one thing on screen is ever saying something —
/// and the person chooses it (Appearance). Red appears on leaving and on
/// errors and nowhere else; green only on a connection light.
public enum Theme {
    public static let background = Color(red: 0.043, green: 0.043, blue: 0.051)
    public static let surface = Color(red: 0.110, green: 0.110, blue: 0.118)
    public static let surfaceHigh = Color(red: 0.173, green: 0.173, blue: 0.180)
    public static let border = Color.white.opacity(0.10)
    public static let foreground = Color(red: 0.961, green: 0.961, blue: 0.969)
    public static let muted = Color(red: 0.922, green: 0.922, blue: 0.961).opacity(0.6)
    /// The colour the person chose, read where it is drawn so that a change
    /// redraws everything wearing it at once.
    @MainActor public static var tally: Color { Appearance.shared.accent.color }
    @MainActor public static var tallyForeground: Color { Appearance.shared.accent.onColor }
    public static let good = Color(red: 0.188, green: 0.820, blue: 0.345)
    public static let danger = Color(red: 1.0, green: 0.271, blue: 0.227)
    public static let dangerForeground = Color.white

    /// The tile's corner, 0.5rem.
    public static let tileRadius: CGFloat = 8

    /// Arrivals decelerate into place; departures need no such courtesy
    /// (--ease-settle and --ease-leave).
    public static let settle = Animation.timingCurve(0.2, 0.9, 0.3, 1, duration: settleSeconds)
    public static let leave = Animation.timingCurve(0.4, 0, 1, 1, duration: leaveSeconds)

    /// The same two curves for Core Animation, for what moves inside a layer
    /// SwiftUI does not draw: a remote picture going from filling its place
    /// to being shown whole and back (VideoSurface), which is a layer's
    /// scale and a layer's opacity under a view SwiftUI only places.
    public static let settleSeconds: Double = 0.32
    public static let leaveSeconds: Double = 0.2
    public static var settleTiming: CAMediaTimingFunction { CAMediaTimingFunction(controlPoints: 0.2, 0.9, 0.3, 1) }
    public static var leaveTiming: CAMediaTimingFunction { CAMediaTimingFunction(controlPoints: 0.4, 0, 1, 1) }

    /// Every appearance and disappearance uses these: the person who owns this
    /// product does not accept a view that simply pops in or out.
    public static let arrive = AnyTransition.asymmetric(
        insertion: .opacity.combined(with: .scale(scale: 0.97)).animation(settle),
        removal: .opacity.animation(leave)
    )
    public static let slideUp = AnyTransition.asymmetric(
        insertion: .move(edge: .bottom).combined(with: .opacity).animation(settle),
        removal: .move(edge: .bottom).combined(with: .opacity).animation(leave)
    )
    public static let slideIn = AnyTransition.asymmetric(
        insertion: .move(edge: .trailing).combined(with: .opacity).animation(settle),
        removal: .move(edge: .trailing).combined(with: .opacity).animation(leave)
    )
}
