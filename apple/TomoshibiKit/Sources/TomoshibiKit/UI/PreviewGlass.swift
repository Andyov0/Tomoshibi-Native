import SwiftUI
import TomoshibiBasics

// The phone lobby's camera card with the camera off: a pane of glass with
// cartoon children drawn on it in lines of glass, one after another.
//
// It was the grey of a camera-off tile, an empty card the size of the
// picture that was not there. The owner asked for it to be a large piece of
// glass instead, with an animation of different people's heads drawn on it
// in frosted-glass lines, drawn the way the iPhone writes its first "hello"
// (2026-10-26); then, finding realistic heads and shoulders unsettling, for
// cartoon children, boys and girls (2026-10-27). What is drawn, and when each
// line is, comes from art/portraits.json, which every client plays
// (TomoshibiBasics Portraits.swift); here is only how it looks. It is the one
// surface in the content layer that is glass (Glass.swift: the rest of the
// page is plain), because it stands where a picture will be, as the glass
// over a call does; the card's buttons over it are the content layer's plain
// ones, so there is no glass on glass.
//
// The drawing is decoration. It is hidden from VoiceOver, it is the first
// child standing finished when Reduce Motion is on, and it moves only while
// it can be seen: it holds still while the app is not in front, while the
// card is scrolled out of sight or covered by a sheet, and stops with the
// card. While a child is held finished nothing is redrawn at all: the
// timeline that draws them is paused between the moments of writing and
// fading.

struct PreviewGlass: View {
    let cornerRadius: CGFloat
    /// The card is covered (a sheet over the lobby): the drawing holds still.
    var held = false

    var body: some View {
        let shape = RoundedRectangle(cornerRadius: cornerRadius, style: .continuous)
        ZStack {
            PaneOfGlass(shape: shape)
            PortraitCycle(held: held)
                .padding(.horizontal, 12)
        }
        .clipShape(shape)
        // Nothing on it is pressed, and a tap on the card belongs to the page
        // under it, which puts the keyboard away (`putsKeyboardAway`): the
        // grey that was here took the tap itself, and the keyboard stayed up
        // (UITests/LobbyTouchTests `testKeyboardGoesOnATapOnTheCameraCard`).
        .allowsHitTesting(false)
        .accessibilityHidden(true)
    }
}

/// The card's glass: Liquid Glass where the system has it, the system's thin
/// material with a light edge before it, as `glassSurface` draws a control.
/// Not `glassSurface` itself, which inside the lobby's content layer draws a
/// plain surface, and this card is the exception to that.
private struct PaneOfGlass<S: Shape>: View {
    let shape: S

    var body: some View {
        if #available(macOS 26.0, iOS 26.0, *), !GlassSupport.forcedFallback {
            Color.clear
                .glassEffect(Glass.regular, in: shape)
        } else {
            shape.fill(.ultraThinMaterial)
                .overlay(shape.stroke(Theme.foreground.opacity(0.16), lineWidth: 1))
        }
    }
}

// MARK: - The children, one after another

/// The children on the card in turn (PortraitRound, as every client plays
/// them), drawn by a timeline that runs only while something on it moves.
struct PortraitCycle: View {
    var held = false
    /// When the cycle's second zero was, while it runs; how far it had got
    /// while it does not.
    @State private var origin: Date?
    @State private var stopped = 0.0
    /// Something is being written or fading.
    @State private var moving = false
    @State private var inSight = true
    @Environment(\.scenePhase) private var scenePhase
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    private let round = PortraitRound.art

    private var running: Bool {
        !held && inSight && scenePhase == .active && !reduceMotion && PortraitDebug.held == nil && round.length > 0
    }

    var body: some View {
        Group {
            if let still = PortraitDebug.held {
                PortraitLines(person: still.person, written: still.written)
            } else if reduceMotion || round.people.isEmpty {
                if !round.people.isEmpty { PortraitLines(person: 0, written: .infinity) }
            } else {
                TimelineView(.animation(minimumInterval: 1.0 / 60, paused: !(running && moving))) { context in
                    let now = round.moment(at: origin.map { context.date.timeIntervalSince($0) } ?? stopped)
                    PortraitLines(person: now.person, written: now.written)
                        .opacity(now.shown)
                }
            }
        }
        .whenInSight { inSight = $0 }
        .onDisappear { stop() }
        .task(id: running) {
            guard running else {
                stop()
                return
            }
            let origin = Date().addingTimeInterval(-stopped)
            self.origin = origin
            while !Task.isCancelled {
                let t = Date().timeIntervalSince(origin)
                let next = round.motion(at: t)
                if moving != next.moving { moving = next.moving }
                // A little past the change, so that the last frame drawn
                // before the timeline pauses is the finished drawing (or the
                // faded one) rather than one a frame short of it.
                try? await Task.sleep(for: .seconds(max(next.until - t, 0) + 0.04))
            }
        }
    }

    private func stop() {
        if let origin { stopped = Date().timeIntervalSince(origin) }
        origin = nil
        moving = false
    }
}

extension View {
    /// Whether this is on the screen, as far as a scroll view can say: on
    /// iOS 17 and macOS 14, which cannot, it is taken to be.
    @ViewBuilder
    fileprivate func whenInSight(_ changed: @escaping (Bool) -> Void) -> some View {
        if #available(macOS 15.0, iOS 18.0, *) {
            onScrollVisibilityChange(threshold: 0.05, changed)
        } else {
            self
        }
    }
}

// MARK: - The lines

/// One child, drawn `written` seconds into their drawing, placed on the card
/// as the art says (`bottom-center`: as large as fits the height and the
/// width less the card's side inset, standing on its foot; art/README.md),
/// in lines of glass.
///
/// A line is Liquid Glass in the line's own shape, tinted faintly with the
/// colour of the interface's text, with the light caught along its upper
/// side. Five ways were set side by side on the card, by day and by night:
/// the glass alone, the glass with the light, the glass filled underneath,
/// the system's thin material in the line's shape, and a faint fill with an
/// edge. The material and the fill drew a double edge wherever two strokes
/// cross, where the glass merges them into one line. The glass alone was
/// frosted glass but faint, and by day white on a white pane, a relief with
/// nothing to read; the tint is what makes it a rod of glass there and a lit
/// one at night, and the light shows on it in both.
///
/// The glass is the clear kind, not the regular. The regular casts a soft
/// grey shadow round everything it is drawn in, which round a face's worth
/// of lines on the day's white pane was a grey haze: the drawing looked
/// dirty (2026-10-27, set side by side with tints from 8 to 28 % and with
/// flat lines). The clear glass has no shadow; by day it takes a little more
/// tint and a brighter light to read as clean glass rather than as nothing.
/// The numbers are in art/README.md for the other platforms to draw the
/// same.
struct PortraitLines: View {
    let person: Int
    let written: Double
    @Environment(\.colorScheme) private var scheme

    var body: some View {
        GeometryReader { space in
            let width = PortraitPlace(space.size).scale * PortraitArt.lineWidth
            let look = LineLook(scheme)
            let outline = PortraitPath(person: person, written: written, outline: true)
            ZStack {
                GlassLine(shape: outline, look: look)
                PortraitPath(person: person, written: written, outline: false)
                    .stroke(Color.white.opacity(look.light), style: StrokeStyle(lineWidth: max(width * 0.16, 0.8), lineCap: .round, lineJoin: .round))
                    .offset(x: -width * 0.17, y: -width * 0.2)
                    .mask(outline)
            }
        }
        .allowsHitTesting(false)
    }
}

/// How much of the text's colour a line takes, and how bright its light is,
/// by night and by day.
private struct LineLook {
    /// The glass's tint.
    let tint: Double
    /// The light along the line's upper side.
    let light: Double
    /// Without glass: the line's body and its edge.
    let fill: Double
    let edge: Double

    init(_ scheme: ColorScheme) {
        if scheme == .dark {
            (tint, light, fill, edge) = (0.16, 0.6, 0.12, 0.30)
        } else {
            (tint, light, fill, edge) = (0.20, 0.9, 0.14, 0.30)
        }
    }
}

/// The body of a line: glass where the system has it, and before it a faint
/// fill with an edge, which is what the glass looks like without its depth.
/// The edge is drawn round the lines made one shape first, as the glass
/// merges them; round the strokes as they are, it drew every crossing.
private struct GlassLine: View {
    let shape: PortraitPath
    let look: LineLook

    var body: some View {
        if #available(macOS 26.0, iOS 26.0, *), !GlassSupport.forcedFallback {
            Color.clear.glassEffect(Glass.clear.tint(Theme.foreground.opacity(look.tint)), in: shape)
        } else {
            shape.fill(Theme.foreground.opacity(look.fill))
            United(shape: shape).stroke(Theme.foreground.opacity(look.edge), lineWidth: 0.75)
        }
    }

    private struct United: Shape {
        let shape: PortraitPath

        func path(in rect: CGRect) -> Path {
            shape.path(in: rect).normalized(eoFill: false)
        }
    }
}

/// Where the canvas sits in a space: scaled to fit it and standing on its
/// foot, in the middle across.
struct PortraitPlace {
    let scale: CGFloat
    let transform: CGAffineTransform

    init(_ size: CGSize) {
        let canvas = PortraitArt.canvas
        scale = max(min(size.width / canvas.width, size.height / canvas.height), 0)
        transform = CGAffineTransform(scaleX: scale, y: scale)
            .concatenating(CGAffineTransform(translationX: (size.width - canvas.width * scale) / 2,
                                             y: size.height - canvas.height * scale))
    }
}

/// A child's lines placed in the space, along their middles or as the shape
/// of the glass: the lines stroked at the art's width, all of them one shape.
struct PortraitPath: Shape {
    let person: Int
    let written: Double
    let outline: Bool

    func path(in rect: CGRect) -> Path {
        let drawn = outline ? PortraitOutlines.shape(person, at: written) : PortraitOutlines.middles(person, at: written)
        return drawn.applying(PortraitPlace(rect.size).transform.concatenating(CGAffineTransform(translationX: rect.minX, y: rect.minY)))
    }
}

/// Every stroke's shape as glass, worked out once in the canvas's units:
/// a frame then strokes only the line the pen is on, where stroking the
/// whole drawing again on every frame is what a drawing with a face's worth
/// of small lines would cost.
private enum PortraitOutlines {
    static let style = StrokeStyle(lineWidth: PortraitArt.lineWidth, lineCap: .round, lineJoin: .round)
    static let whole: [[Path]] = PortraitArt.portraits.map { person in
        person.strokes.map { $0.path.strokedPath(style) }
    }

    static func shape(_ person: Int, at seconds: Double) -> Path {
        var all = Path()
        for (i, stroke) in PortraitArt.portraits[person].strokes.enumerated() {
            let done = Portrait.drawn(stroke, at: seconds)
            if done >= 1 {
                all.addPath(whole[person][i])
            } else if done > 0 {
                all.addPath(stroke.path.trimmedPath(from: 0, to: done).strokedPath(style))
            }
        }
        return all
    }

    static func middles(_ person: Int, at seconds: Double) -> Path {
        var all = Path()
        for stroke in PortraitArt.portraits[person].strokes {
            let done = Portrait.drawn(stroke, at: seconds)
            if done >= 1 {
                all.addPath(stroke.path)
            } else if done > 0 {
                all.addPath(stroke.path.trimmedPath(from: 0, to: done))
            }
        }
        return all
    }
}

enum PortraitDebug {
    /// TOMOSHIBI_DEBUG_PORTRAIT=<name>[@<seconds>], in a debug build: that
    /// child alone, standing still, finished or that far into their
    /// drawing, to photograph each of them.
    static let held: (person: Int, written: Double)? = {
        #if DEBUG
        guard let said = ProcessInfo.processInfo.environment["TOMOSHIBI_DEBUG_PORTRAIT"] else { return nil }
        let parts = said.split(separator: "@").map(String.init)
        guard let person = PortraitArt.portraits.firstIndex(where: { $0.name == parts.first }) else { return nil }
        return (person, parts.count > 1 ? Double(parts[1]) ?? .infinity : .infinity)
        #else
        return nil
        #endif
    }()
}
