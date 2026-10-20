import os
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

    /// TOMOSHIBI_ALL_GLASS, in a debug build, draws glass in the content
    /// layer too, as the apps did before it, to set the two side by side.
    static let everywhere: Bool = {
        #if DEBUG
        ProcessInfo.processInfo.environment["TOMOSHIBI_ALL_GLASS"] != nil
        #else
        false
        #endif
    }()

    static var available: Bool {
        if #available(macOS 26.0, iOS 26.0, *) { return !forcedFallback }
        return false
    }
}

// Glass for the controls and navigation, not for what they are over.
//
// Apple's guidance (Human Interface Guidelines, Materials): "Liquid Glass
// forms a distinct functional layer for controls and navigation elements
// ... that floats above the content layer", "Don't use Liquid Glass in the
// content layer ... use standard materials for elements in the content
// layer", and "Use Liquid Glass effects sparingly"; and SwiftUI's "Applying
// Liquid Glass to custom views": "Creating too many Liquid Glass effect
// containers and applying too many effects to views outside of containers
// can degrade performance. Limit the use of Liquid Glass effects onscreen at
// the same time." The apps had glass everywhere: every card of the lobby and
// the settings, every button and field on those cards (glass on glass), and
// the name on every picture of a call, each one worked out again from the
// moving video under it on every frame. The owner found too much of it, and
// a phone in a call hot (2026-10-17).
//
// So a part of the screen is marked as content (`contentLayer()`), and every
// surface drawn inside it, the cards, buttons, fields and switches that all
// come through `glassSurface`, is drawn as a plain surface instead: a light
// translucent fill and a hairline, with no glass, no blur and no shadow. What
// is left as glass is what floats over the content: the call's bar and the
// row above it, the sidebars, the window's own controls, and the shell of a
// panel or sheet over a call (whose insides are content again).
private struct ContentLayerKey: EnvironmentKey {
    static let defaultValue = false
}

extension EnvironmentValues {
    var inContentLayer: Bool {
        get { self[ContentLayerKey.self] }
        set { self[ContentLayerKey.self] = newValue }
    }
}

/// A content-layer surface: what `glassSurface` draws inside `contentLayer()`.
private struct PlainSurface<S: Shape>: ViewModifier {
    let shape: S
    let tint: Color?

    func body(content: Content) -> some View {
        content
            .background(shape.fill(tint ?? Color.white.opacity(0.07)))
            .overlay(shape.stroke(Color.white.opacity(tint == nil ? 0.09 : 0), lineWidth: 1))
    }
}

/// `glassSurface`, deciding by the layer it is drawn in.
private struct LayeredSurface<S: Shape>: ViewModifier {
    let shape: S
    let tint: Color?
    let interactive: Bool
    @Environment(\.inContentLayer) private var inContent

    func body(content: Content) -> some View {
        if inContent && !GlassSupport.everywhere {
            content.modifier(PlainSurface(shape: shape, tint: tint))
        } else {
            content.liquidGlass(shape, tint: tint, interactive: interactive)
        }
    }
}

extension View {
    /// Marks this part of the screen as content: surfaces inside it are plain.
    func contentLayer() -> some View {
        environment(\.inContentLayer, true)
    }

    /// A control floating over content inside a content layer (a button over
    /// a picture), which is glass again.
    func controlLayer() -> some View {
        environment(\.inContentLayer, false)
    }

    /// Glass in `shape`, tinted where it says something (the primary action,
    /// leaving), interactive where it is pressed; a plain surface instead
    /// inside `contentLayer()`.
    func glassSurface<S: Shape>(_ shape: S, tint: Color? = nil, interactive: Bool = false) -> some View {
        modifier(LayeredSurface(shape: shape, tint: tint, interactive: interactive))
    }

    @ViewBuilder
    fileprivate func liquidGlass<S: Shape>(_ shape: S, tint: Color?, interactive: Bool) -> some View {
        #if DEBUG
        liquidGlassDrawn(shape, tint: tint, interactive: interactive).modifier(GlassCount())
        #else
        liquidGlassDrawn(shape, tint: tint, interactive: interactive)
        #endif
    }

    @ViewBuilder
    fileprivate func liquidGlassDrawn<S: Shape>(_ shape: S, tint: Color?, interactive: Bool) -> some View {
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

    /// Glass in `shape` drawn behind the view rather than around it, for a
    /// surface whose children reach past its edge: the call's bar, whose
    /// popovers hang above it.
    ///
    /// On iOS 26 a view inside `glassEffect` is touched only within the glass
    /// shape. The bar's popovers are drawn above the capsule, and every one of
    /// them, the More menu's items, the shares', the reactions and Leave's,
    /// was drawn on an iPhone and did nothing when pressed: a real tap on the
    /// More menu's Sound never reached its action (UITests/BarMenuTests), and
    /// the same tap with the glass turned off did. The Mac's glass lets them
    /// through. Behind the content, the glass is the same glass and touches
    /// go to what is drawn over it.
    func glassBacking<S: Shape>(_ shape: S) -> some View {
        modifier(GlassBacking(shape: shape))
    }

    /// A larger pane of glass: a sidebar, a sheet, a panel beside a call.
    func glassPane(cornerRadius: CGFloat = 22) -> some View {
        glassSurface(RoundedRectangle(cornerRadius: cornerRadius, style: .continuous))
    }

    /// A panel, sheet or menu floating over the content: glass itself, and
    /// content inside, so that nothing in it is glass on glass ("avoid ...
    /// layering Liquid Glass elements on top of each other", Adopting Liquid
    /// Glass). Glass wherever it is put, even inside a content layer.
    func glassShell(cornerRadius: CGFloat = 22) -> some View {
        contentLayer()
            .glassPane(cornerRadius: cornerRadius)
            .controlLayer()
    }
}

#if DEBUG
/// TOMOSHIBI_DEBUG_GLASS_COUNT: how many pieces of glass are on the screen,
/// logged under the app's `glass` category whenever it settles, to count what
/// the content layer took away.
private struct GlassCount: ViewModifier {
    @MainActor static var live = 0
    @MainActor static var pending: Task<Void, Never>?
    static let on = ProcessInfo.processInfo.environment["TOMOSHIBI_DEBUG_GLASS_COUNT"] != nil

    func body(content: Content) -> some View {
        if Self.on {
            content
                .onAppear { Self.live += 1; Self.report() }
                .onDisappear { Self.live -= 1; Self.report() }
        } else {
            content
        }
    }

    @MainActor static func report() {
        pending?.cancel()
        pending = Task {
            try? await Task.sleep(for: .seconds(1))
            guard !Task.isCancelled else { return }
            Logger(subsystem: "app.tomoshibi", category: "glass").notice("GLASS live \(live, privacy: .public)")
        }
    }
}
#endif

private struct GlassBacking<S: Shape>: ViewModifier {
    let shape: S
    @Environment(\.inContentLayer) private var inContent

    func body(content: Content) -> some View {
        if inContent && !GlassSupport.everywhere {
            content.modifier(PlainSurface(shape: shape, tint: nil))
        } else if #available(macOS 26.0, iOS 26.0, *), !GlassSupport.forcedFallback {
            content.background { Color.clear.glassEffect(Glass.regular, in: shape) }
        } else {
            content.liquidGlass(shape, tint: nil, interactive: false)
        }
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
    @Environment(\.inContentLayer) private var inContent

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
        // Glass answers a press itself; a plain surface (no glass, or the
        // content layer) is pressed down a little instead.
        .scaleEffect(configuration.isPressed && (!GlassSupport.available || inContent) ? 0.96 : 1)
        .brightness(configuration.isPressed && inContent ? 0.06 : 0)
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

/// What the screens before a call stand on when there is no picture yet: the
/// call's own ground, flat.
///
/// It was an evening room, a dark blue wash with three coloured glows in it,
/// on the argument that glass needs something under it to be glass. The
/// owner found the gradients cheap (2026-10-20) and asked for fewer of them;
/// the glow behind the sign-in card and the lobby was the largest. The
/// darkest colour of the palette is what the call itself stands on, so the
/// screens before it and the call now read as one place, and the panes over
/// it are told apart by their own fill and edge rather than by what they
/// blur. Android's Backdrop (ui/kit/Glass.kt) is the same colour.
struct Backdrop: View {
    var body: some View {
        Theme.background
            .ignoresSafeArea()
            .allowsHitTesting(false)
            .accessibilityHidden(true)
    }
}
