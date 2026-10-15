import SwiftUI
import TomoshibiBasics

// Liquid Glass, and what stands in for it before the systems that have it.
//
// The owner asked for the apps to look like Apple's own (2026-10-15), which
// on macOS 26 and iOS 26 means Liquid Glass: controls and panels as glass
// floating over the content, refracting what is under them. Those systems
// draw it with `glassEffect`; on macOS 14–15 and iOS 17–18, which the apps
// still support, the same shapes are drawn in the system's thin material with
// a light edge, which is the nearest those systems come and is what their own
// apps look like.

enum GlassSupport {
    /// TOMOSHIBI_NO_GLASS, in a debug build, draws the stand-in on a system
    /// that has glass, so the older look can be checked without an older Mac.
    static let forcedFallback: Bool = {
        #if DEBUG
        ProcessInfo.processInfo.environment["TOMOSHIBI_NO_GLASS"] != nil
        #else
        false
        #endif
    }()

    static var available: Bool {
        if #available(macOS 26.0, iOS 26.0, *) { return !forcedFallback }
        return false
    }
}

extension View {
    /// Glass in `shape`, tinted where it says something (the primary action,
    /// leaving), interactive where it is pressed.
    @ViewBuilder
    func glassSurface<S: Shape>(_ shape: S, tint: Color? = nil, interactive: Bool = false) -> some View {
        if #available(macOS 26.0, iOS 26.0, *), !GlassSupport.forcedFallback {
            // A tint alone is barely there on macOS 27's glass, and dark text
            // on it could not be read: the primary button looked grey. Where
            // the glass says something it is filled with the colour, and the
            // glass gives it its edge and its light.
            if let tint {
                background(shape.fill(tint))
                    .glassEffect(Glass.regular.tint(tint).interactive(interactive), in: shape)
            } else {
                glassEffect(Glass.regular.interactive(interactive), in: shape)
            }
        } else {
            background {
                ZStack {
                    shape.fill(.ultraThinMaterial)
                    if let tint { shape.fill(tint) }
                }
            }
            .overlay(shape.stroke(Color.white.opacity(tint == nil ? 0.16 : 0.28), lineWidth: 1))
            .shadow(color: .black.opacity(0.28), radius: 14, y: 6)
        }
    }

    /// A larger pane of glass: a sidebar, a sheet, a panel beside a call.
    func glassPane(cornerRadius: CGFloat = 22) -> some View {
        glassSurface(RoundedRectangle(cornerRadius: cornerRadius, style: .continuous))
    }
}

/// Several pieces of glass that should merge and morph as one, where the
/// system can (a control bar's buttons). Before it, simply the content.
struct GlassGroup<Content: View>: View {
    var spacing: CGFloat = 8
    @ViewBuilder var content: Content

    var body: some View {
        if #available(macOS 26.0, iOS 26.0, *), !GlassSupport.forcedFallback {
            GlassEffectContainer(spacing: spacing) { content }
        } else {
            content
        }
    }
}

/// A button made of glass: a circle around a symbol, or a capsule around
/// words. `prominent` wears the person's chosen colour; `destructive` is red,
/// for leaving and nothing else.
struct GlassButtonStyle: ButtonStyle {
    enum Shape {
        case circle(CGFloat)
        case capsule(height: CGFloat)
    }

    var shape: Shape = .capsule(height: 36)
    var prominent = false
    var destructive = false
    @Environment(\.isEnabled) private var enabled

    func makeBody(configuration: Configuration) -> some View {
        let tint: Color? = destructive ? Theme.danger : prominent ? Theme.tally : nil
        let ink: Color = destructive ? Theme.dangerForeground : prominent ? Theme.tallyForeground : Theme.foreground
        Group {
            switch shape {
            case let .circle(size):
                configuration.label
                    .frame(width: size, height: size)
                    .contentShape(Circle())
                    .glassSurface(Circle(), tint: tint, interactive: true)
            case let .capsule(height):
                configuration.label
                    .lineLimit(1)
                    .padding(.horizontal, height * 0.45)
                    .frame(minHeight: height)
                    .contentShape(Capsule())
                    .glassSurface(Capsule(), tint: tint, interactive: true)
            }
        }
        .foregroundStyle(ink)
        .font(.system(size: 13.5, weight: .semibold))
        .scaleEffect(configuration.isPressed && !GlassSupport.available ? 0.96 : 1)
        .opacity(enabled ? 1 : 0.45)
        .animation(Theme.settle, value: configuration.isPressed)
        .animation(Theme.settle, value: enabled)
    }
}

extension ButtonStyle where Self == GlassButtonStyle {
    static func glassCircle(_ size: CGFloat = 44, prominent: Bool = false, destructive: Bool = false) -> GlassButtonStyle {
        GlassButtonStyle(shape: .circle(size), prominent: prominent, destructive: destructive)
    }

    static func glassCapsule(_ height: CGFloat = 36, prominent: Bool = false, destructive: Bool = false) -> GlassButtonStyle {
        GlassButtonStyle(shape: .capsule(height: height), prominent: prominent, destructive: destructive)
    }
}

/// What the screens before a call stand on when there is no picture yet: an
/// evening room, a cool wall with one warm lamp in it, the light Tomoshibi is
/// named for. Glass needs something under it to be glass, and a flat dark
/// ground makes every pane look like a grey box.
struct Backdrop: View {
    var body: some View {
        GeometryReader { geometry in
            let size = geometry.size
            ZStack {
                LinearGradient(colors: [Color(red: 0.122, green: 0.165, blue: 0.216),
                                        Color(red: 0.071, green: 0.094, blue: 0.129),
                                        Color(red: 0.035, green: 0.047, blue: 0.067)],
                               startPoint: .topLeading, endPoint: .bottomTrailing)
                glow(Color(red: 0.204, green: 0.384, blue: 0.549), opacity: 0.75,
                     at: CGPoint(x: size.width * 0.28, y: size.height * 0.82), radius: max(size.width, size.height) * 0.55)
                glow(Color(red: 0.369, green: 0.361, blue: 0.902), opacity: 0.32,
                     at: CGPoint(x: size.width * 0.95, y: size.height * 0.95), radius: max(size.width, size.height) * 0.4)
                glow(Color(red: 1.0, green: 0.80, blue: 0.627), opacity: 0.5,
                     at: CGPoint(x: size.width * 0.74, y: size.height * 0.26), radius: max(size.width, size.height) * 0.3)
            }
            .ignoresSafeArea()
        }
        .ignoresSafeArea()
        .allowsHitTesting(false)
        .accessibilityHidden(true)
    }

    private func glow(_ colour: Color, opacity: Double, at point: CGPoint, radius: CGFloat) -> some View {
        Circle()
            .fill(RadialGradient(colors: [colour.opacity(opacity), colour.opacity(0)], center: .center,
                                 startRadius: 0, endRadius: radius))
            .frame(width: radius * 2, height: radius * 2)
            .position(point)
    }
}
