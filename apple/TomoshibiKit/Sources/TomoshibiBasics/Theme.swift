import SwiftUI

/// The web client's palette and motion (web/src/index.css), so the apps look
/// like the same product.
///
/// Dark in every system appearance, on purpose: a call is looked at for an
/// hour at a stretch and the pictures are the only thing in it that should be
/// bright. The neutrals lean warm, because a blue-grey ground turns skin
/// green. There is one signal colour, the amber "tally" a studio camera lights
/// when it is live — speaking, sharing, unread all wear it, so only one thing
/// on screen is ever saying something. Red appears on leaving and on errors
/// and nowhere else; green only on a connection light.
public enum Theme {
    public static let background = Color(red: 0.0596, green: 0.0552, blue: 0.0508)
    public static let surface = Color(red: 0.1012, green: 0.098, blue: 0.0949)
    public static let surfaceHigh = Color(red: 0.141, green: 0.1329, blue: 0.1247)
    public static let border = Color(red: 0.1804, green: 0.172, blue: 0.1634)
    public static let foreground = Color(red: 0.9617, green: 0.953, blue: 0.9401)
    public static let muted = Color(red: 0.5922, green: 0.572, blue: 0.5474)
    /// The amber is a little outside sRGB; Display P3 holds it.
    public static let tally = Color(.displayP3, red: 0.98, green: 0.64, blue: 0.25)
    public static let tallyForeground = Color(red: 0.1236, green: 0.0743, blue: 0.0274)
    public static let good = Color(red: 0.3736, green: 0.8422, blue: 0.5293)
    public static let danger = Color(red: 0.8856, green: 0.3146, blue: 0.232)
    public static let dangerForeground = Color(red: 0.9868, green: 0.9868, blue: 0.9868)

    /// The tile's corner, 0.5rem.
    public static let tileRadius: CGFloat = 8

    /// Arrivals decelerate into place; departures need no such courtesy
    /// (--ease-settle and --ease-leave).
    public static let settle = Animation.timingCurve(0.2, 0.9, 0.3, 1, duration: 0.32)
    public static let leave = Animation.timingCurve(0.4, 0, 1, 1, duration: 0.2)

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
