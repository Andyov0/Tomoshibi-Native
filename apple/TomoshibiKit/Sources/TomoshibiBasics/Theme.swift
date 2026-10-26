import QuartzCore
import SwiftUI
#if canImport(UIKit)
import UIKit
#elseif canImport(AppKit)
import AppKit
#endif

/// The apps' palette and motion.
///
/// Two appearances, following the system's as it changes (the owner,
/// 2026-10-26: keep this interface and give it a day as well as a night,
/// light and dark as the system is). The night is what the apps always were:
/// the system's own dark greys (the owner found the web's warm browns muddy
/// under glass, 2026-10-15), so the glass over a picture reads as clear
/// rather than tinted. The day is the system's own light greys, with white
/// where the night lifts a surface towards white. Every platform draws the
/// same values, and every value of the night is exactly what it was.
///
/// The call itself does not follow: it stays in the night in either
/// appearance (`meeting`), as FaceTime's does, because a call is looked at
/// for an hour at a stretch and the pictures are the only thing in it that
/// should be bright.
///
/// There is one signal colour, the "tally" a studio camera lights when it is
/// live — speaking, sharing, unread all wear it, so only one thing on screen
/// is ever saying something — and the person chooses it (Appearance). Red
/// appears on leaving and on errors and nowhere else; green only on a
/// connection light.
public enum Theme {
    /// The appearance the call is drawn in whatever the system's is: the
    /// room, its bar, every panel and popover opened from inside it, and the
    /// small pictures of a call the settings draw. Nil lets the call follow
    /// the system as everything else does; this is the one place to change
    /// that. TOMOSHIBI_DEBUG_MEETING=system, in a debug build, draws it as
    /// if it said nil, to see the call by day before deciding to.
    public static let meeting: ColorScheme? = {
        #if DEBUG
        if ProcessInfo.processInfo.environment["TOMOSHIBI_DEBUG_MEETING"] == "system" { return nil }
        #endif
        return .dark
    }()

    public static let background = adaptive(dark: backgroundShades.dark, light: backgroundShades.light)
    public static let surface = adaptive(dark: Shade(0.110, 0.110, 0.118), light: Shade(1, 1, 1))
    public static let surfaceHigh = adaptive(dark: Shade(0.173, 0.173, 0.180), light: Shade(229 / 255, 229 / 255, 234 / 255))
    public static let border = adaptive(dark: Shade(1, 1, 1, 0.10), light: Shade(0, 0, 0, 0.10))
    public static let foreground = adaptive(dark: Shade(0.961, 0.961, 0.969), light: Shade(28 / 255, 28 / 255, 30 / 255))
    public static let muted = adaptive(dark: Shade(0.922, 0.922, 0.961, 0.6), light: Shade(60 / 255, 60 / 255, 67 / 255, 0.6))
    /// The colour the person chose, read where it is drawn so that a change
    /// redraws everything wearing it at once.
    @MainActor public static var tally: Color { Appearance.shared.accent.color }
    @MainActor public static var tallyForeground: Color { Appearance.shared.accent.onColor }
    /// The same colour as words and thin symbols on the page (Accent `ink`):
    /// itself at night, deeper by day. A fill wears `tally`.
    @MainActor public static var tallyInk: Color { Appearance.shared.accent.ink }
    public static let good = adaptive(dark: Shade(0.188, 0.820, 0.345), light: Shade(52 / 255, 199 / 255, 89 / 255))
    /// Green as words, a latency that is good: itself at night, and by day
    /// the green accent's own ink (Accent `ink`), since the light green reads
    /// at 2.2 to 1 as words on white. The connection's light keeps `good`.
    public static let goodInk = adaptive(dark: Shade(0.188, 0.820, 0.345), light: Shade(33 / 255, 126 / 255, 56 / 255))
    public static let danger = adaptive(dark: Shade(1.0, 0.271, 0.227), light: Shade(1, 59 / 255, 48 / 255))
    /// Red as words and symbols on the page, a problem said: itself at night,
    /// and by day a deeper red, since the light red reads at 3.2 to 1 as words
    /// on the day's grey. A red button or plate keeps `danger`.
    public static let dangerInk = adaptive(dark: Shade(1.0, 0.271, 0.227), light: Shade(215 / 255, 0, 21 / 255))
    public static let dangerForeground = Color.white

    private static let backgroundShades = (dark: Shade(0.043, 0.043, 0.051), light: Shade(242 / 255, 242 / 255, 247 / 255))

    #if canImport(UIKit)
    /// `background` as UIKit's own colour, for a view drawn under SwiftUI's
    /// (the Messages extension's). A SwiftUI colour handed to UIKit's
    /// initialiser is not promised to keep following the appearance.
    public static var backgroundUIColor: UIColor { platform(dark: backgroundShades.dark, light: backgroundShades.light) }
    #endif

    /// What the night draws in white over its greys and the day in black
    /// over its own: hairlines, rules, a row lit under the pointer, the faint
    /// plate behind a symbol. Always taken at an opacity, the night's.
    public static let ink = adaptive(dark: Shade(1, 1, 1), light: Shade(0, 0, 0))

    /// Words fainter than `muted`, a step back from it: `muted` at `opacity`
    /// at night; by day `muted` itself, since anything fainter falls under 3
    /// to 1 on the day's grey and white.
    public static func faint(_ opacity: Double) -> Color {
        adaptive(dark: Shade(0.922, 0.922, 0.961, 0.6 * opacity), light: Shade(60 / 255, 60 / 255, 67 / 255, 0.6))
    }

    /// Something that floats over a surface rather than being part of it (a
    /// card in a sidebar, a list dropped from a field): the raised grey at
    /// night, white by day, where the light grey is for what is set into a
    /// surface (a field, a track, a chip) and white is what stands above one.
    public static let floating = adaptive(dark: Shade(0.173, 0.173, 0.180), light: Shade(1, 1, 1))

    /// The ground that is black at night, under every screen and the phone's
    /// lobby: the day's background there.
    public static let page = adaptive(dark: Shade(0, 0, 0), light: Shade(242 / 255, 242 / 255, 247 / 255))

    /// A surface lifted off what is behind it, a card or a pane in the content
    /// layer: white at `opacity` at night, which over the dark greys is a
    /// lighter grey; white by day, the way the system's own cards stand on its
    /// light grey. Over a surface that is already lifted (`nested`), the day
    /// draws a faint grey instead, since white on white is no surface at all.
    public static func lifted(_ opacity: Double, nested: Bool = false) -> Color {
        adaptive(dark: Shade(1, 1, 1, opacity), light: nested ? Shade(0, 0, 0, 0.05) : Shade(1, 1, 1, 1))
    }

    /// A well sunk into what is around it, a field or a track: black at
    /// `opacity` at night; by day a quarter of it, a grey that reads as a well
    /// on white and on the day's glass without becoming a dark patch.
    public static func sunk(_ opacity: Double) -> Color {
        adaptive(dark: Shade(0, 0, 0, opacity), light: Shade(0, 0, 0, opacity * 0.25))
    }

    /// A shadow under something that floats over the page: black at
    /// `opacity` at night, and under half of that by day, where a night's
    /// shadow on a light page is a smudge.
    public static func shadow(_ opacity: Double) -> Color {
        adaptive(dark: Shade(0, 0, 0, opacity), light: Shade(0, 0, 0, opacity * 0.4))
    }

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

    /// One colour in sRGB, its components as the palette writes them.
    public struct Shade: Sendable, Equatable {
        public let red, green, blue, alpha: Double

        public init(_ red: Double, _ green: Double, _ blue: Double, _ alpha: Double = 1) {
            self.red = red
            self.green = green
            self.blue = blue
            self.alpha = alpha
        }
    }

    /// A colour that is `dark` in the dark appearance and `light` in the
    /// light one, decided where it is drawn: by the system's appearance, or
    /// by the one a view is told to take (`meeting`), so that the room drawn
    /// dark inside a light app is dark all through.
    ///
    /// Made through the platform's own colour, which SwiftUI resolves against
    /// the appearance it draws in, with the night's components exactly as
    /// SwiftUI's `Color(red:green:blue:)` took them: the night's pixels are
    /// the same as before there was a day (ThemeTests holds both).
    public static func adaptive(dark: Shade, light: Shade) -> Color {
        #if canImport(UIKit)
        Color(uiColor: platform(dark: dark, light: light))
        #elseif canImport(AppKit)
        Color(nsColor: platform(dark: dark, light: light))
        #endif
    }

    #if canImport(UIKit)
    static func platform(dark: Shade, light: Shade) -> UIColor {
        UIColor { traits in
            let shade = traits.userInterfaceStyle == .light ? light : dark
            return UIColor(red: shade.red, green: shade.green, blue: shade.blue, alpha: shade.alpha)
        }
    }
    #elseif canImport(AppKit)
    static func platform(dark: Shade, light: Shade) -> NSColor {
        NSColor(name: nil) { appearance in
            let shade = appearance.bestMatch(from: [.darkAqua, .aqua]) == .aqua ? light : dark
            return NSColor(srgbRed: shade.red, green: shade.green, blue: shade.blue, alpha: shade.alpha)
        }
    }
    #endif
}
