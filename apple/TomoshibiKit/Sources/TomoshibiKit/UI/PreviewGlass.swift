import SwiftUI
import TomoshibiBasics

// The phone lobby's camera card with the camera off: a pane of glass with
// people drawn on it in lines of glass, one after another.
//
// It was the grey of a camera-off tile, an empty card the size of the
// picture that was not there. The owner asked for it to be a large piece of
// glass instead, with an animation of different people's heads drawn on it
// in frosted-glass lines, drawn the way the iPhone writes its first "hello"
// (2026-10-26). It is the one surface in the content layer that is glass
// (Glass.swift: the rest of the page is plain), because it stands where a
// picture will be, as the glass over a call does; the card's buttons over it
// are the content layer's plain ones, so there is no glass on glass.
//
// The drawing is decoration. It is hidden from VoiceOver, it is one person
// standing finished when Reduce Motion is on, and it moves only while it can
// be seen: it holds still while the app is not in front, while the card is
// scrolled out of sight or covered by a sheet, and stops with the card.
// While a person is held finished nothing is redrawn at all: the timeline
// that draws them is paused between the moments of writing and fading.

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

// MARK: - The people, one after another

/// When each person is drawn, held and let go, round and round: a moment of
/// empty glass, the drawing at the pen's one speed, a second finished, and
/// a fade, then the next.
enum Choreography {
    static let lead = 0.25
    static let hold = 1.0
    static let fade = 0.6

    /// How long one person's turn is.
    static func turn(_ person: Int) -> Double {
        lead + Portrait.everybody[person].writing + hold + fade
    }

    /// The turns of one round, as when each starts.
    private static let starts: [Double] = {
        var at = 0.0
        return Portrait.everybody.indices.map { person in
            defer { at += turn(person) }
            return at
        }
    }()

    static let round: Double = Portrait.everybody.indices.reduce(0) { $0 + turn($1) }

    /// Whose turn it is `t` seconds into the cycle, and how far into it.
    private static func place(_ t: Double) -> (person: Int, into: Double, start: Double) {
        let base = (t / round).rounded(.down) * round
        let inRound = t - base
        let person = starts.lastIndex { $0 <= inRound } ?? 0
        return (person, inRound - starts[person], base + starts[person])
    }

    /// Who is on the card `t` seconds in, how many seconds of them are
    /// drawn, and how much of them is still there to see.
    static func moment(at t: Double) -> (person: Int, written: Double, shown: Double) {
        let (person, into, _) = place(max(t, 0))
        let writing = Portrait.everybody[person].writing
        let written = into - lead
        let fading = (into - lead - writing - hold) / fade
        let shown = 1 - min(max(fading, 0), 1)
        // Eased out as a light dims, quickly at first.
        return (person, written, shown * shown * (3 - 2 * shown))
    }

    /// Whether anything is moving `t` seconds in, and until when that stays
    /// so: the writing and the fade move, the empty glass before a person
    /// and the finished drawing do not.
    static func motion(at t: Double) -> (moving: Bool, until: Double) {
        let (person, into, start) = place(max(t, 0))
        let writing = Portrait.everybody[person].writing
        let marks = [lead, lead + writing, lead + writing + hold, turn(person)]
        let moving = [false, true, false, true]
        let phase = marks.firstIndex { into < $0 } ?? 3
        return (moving[phase], start + marks[phase])
    }
}

/// The people on the card in turn, drawn by a timeline that runs only while
/// something on it moves.
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

    private var running: Bool {
        !held && inSight && scenePhase == .active && !reduceMotion && PortraitDebug.held == nil
    }

    var body: some View {
        Group {
            if let still = PortraitDebug.held {
                PortraitLines(person: still.person, written: still.written)
            } else if reduceMotion {
                PortraitLines(person: Portrait.everybody[0], written: .infinity)
            } else {
                TimelineView(.animation(minimumInterval: 1.0 / 60, paused: !(running && moving))) { context in
                    let now = Choreography.moment(at: origin.map { context.date.timeIntervalSince($0) } ?? stopped)
                    PortraitLines(person: Portrait.everybody[now.person], written: now.written)
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
                let next = Choreography.motion(at: t)
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

/// One person, drawn `written` seconds into their drawing, fitted to the
/// height of the card and standing on its foot, in lines of glass.
///
/// A line is Liquid Glass in the line's own shape, tinted faintly with the
/// colour of the interface's text, with the light caught along its upper
/// side. Five ways were set side by side on the card, by day and by night:
/// the glass alone, the glass with the light, the glass filled underneath,
/// the system's thin material in the line's shape, and a faint fill with an
/// edge. The material and the fill drew a double edge wherever two strokes
/// cross, at an ear or the bridge of the glasses, where the glass merges
/// them into one line. The glass alone was frosted glass but faint, and by
/// day white on a white pane, a relief with nothing to read; the tint is
/// what makes it a grey rod of glass there and a lit one at night, and the
/// light shows on it in both.
struct PortraitLines: View {
    let person: Portrait
    let written: Double

    var body: some View {
        GeometryReader { space in
            let side = min(space.size.width, space.size.height)
            let width = side * 0.027
            let centre = PortraitPath(person: person, written: written)
            let outline = PortraitPath(person: person, written: written, stroked: width)
            ZStack {
                GlassLine(shape: outline)
                centre
                    .stroke(Color.white.opacity(0.62), style: StrokeStyle(lineWidth: max(width * 0.16, 0.8), lineCap: .round, lineJoin: .round))
                    .offset(x: -width * 0.17, y: -width * 0.2)
                    .mask(outline)
            }
            .frame(width: side, height: side)
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .bottom)
        }
        .allowsHitTesting(false)
    }
}

/// The body of a line: glass where the system has it, and before it a faint
/// fill with an edge, which is what the glass looks like without its depth.
private struct GlassLine: View {
    let shape: PortraitPath

    var body: some View {
        if #available(macOS 26.0, iOS 26.0, *), !GlassSupport.forcedFallback {
            Color.clear.glassEffect(Glass.regular.tint(Theme.foreground.opacity(0.16)), in: shape)
        } else {
            shape.fill(Theme.foreground.opacity(0.12))
            shape.stroke(Theme.foreground.opacity(0.34), lineWidth: 0.75)
        }
    }
}

/// A person's lines fitted to the square they are drawn in, along their
/// middle, or as the outline of a line `stroked` wide.
struct PortraitPath: Shape {
    let person: Portrait
    let written: Double
    var stroked: CGFloat?

    func path(in rect: CGRect) -> Path {
        let side = min(rect.width, rect.height)
        let fitted = person.path(after: written)
            .applying(CGAffineTransform(scaleX: side / 100, y: side / 100)
                .concatenating(CGAffineTransform(translationX: rect.midX - side / 2, y: rect.maxY - side)))
        guard let stroked else { return fitted }
        return fitted.strokedPath(StrokeStyle(lineWidth: stroked, lineCap: .round, lineJoin: .round))
    }
}

enum PortraitDebug {
    /// TOMOSHIBI_DEBUG_PORTRAIT=<name>[@<seconds>], in a debug build: that
    /// person alone, standing still, finished or that far into their
    /// drawing, to photograph each of them.
    static let held: (person: Portrait, written: Double)? = {
        #if DEBUG
        guard let said = ProcessInfo.processInfo.environment["TOMOSHIBI_DEBUG_PORTRAIT"] else { return nil }
        let parts = said.split(separator: "@").map(String.init)
        guard let person = Portrait.everybody.first(where: { $0.name == parts.first }) else { return nil }
        return (person, parts.count > 1 ? Double(parts[1]) ?? .infinity : .infinity)
        #else
        return nil
        #endif
    }()
}
