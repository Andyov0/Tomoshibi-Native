import SwiftUI
import TomoshibiBasics

// The phone lobby's camera card with the camera off: a pane of glass with
// little animals drawn on it in thin lines, one after another.
//
// It was the grey of a camera-off tile, an empty card the size of the
// picture that was not there. The owner asked for a large piece of glass
// instead, with drawings on it drawn the way the iPhone writes its first
// "hello" (2026-10-26). People came first, then cartoon children; the owner
// found realistic heads unsettling, and then faces drawn in lines of glass,
// with ringed eyes and a light along every line, eerie (2026-10-27), and
// chose cartoon animals in plain thin lines with two dots for eyes. What is
// drawn, and when each line is, comes from art/portraits.json, which every
// client plays (TomoshibiBasics Portraits.swift); here is only how it looks,
// and art/README.md has the same numbers for the other platforms. The card
// is the one surface in the content layer that is glass (Glass.swift: the
// rest of the page is plain), because it stands where a picture will be, as
// the glass over a call does; the card's buttons over it are the content
// layer's plain ones, so there is no glass on glass.
//
// The drawing is decoration. It is hidden from VoiceOver, it is the first
// animal standing finished when Reduce Motion is on, and it moves only while
// it can be seen: it holds still while the app is not in front, while the
// card is scrolled out of sight or covered by a sheet, and stops with the
// card. While an animal is held finished nothing is redrawn at all: the
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
///
/// The clear kind of Liquid Glass, not the regular: the regular casts a soft
/// grey shadow round what is drawn in it, which by day read as dirt on the
/// white page round the card and round the lines that were glass before
/// these (2026-10-27, the two set side by side by day and by night).
private struct PaneOfGlass<S: Shape>: View {
    let shape: S

    var body: some View {
        if #available(macOS 26.0, iOS 26.0, *), !GlassSupport.forcedFallback {
            Color.clear.glassEffect(Glass.clear, in: shape)
        } else {
            shape.fill(.ultraThinMaterial)
                .overlay(shape.stroke(Theme.foreground.opacity(0.16), lineWidth: 1))
        }
    }
}

// MARK: - The animals, one after another

/// The animals in turn (PortraitRound, as every client plays them), drawn by
/// a timeline that runs only while something on it moves.
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

/// One animal, drawn `written` seconds into their drawing, placed as the art
/// says (`bottom-center`: as large as fits the height and the width less the
/// card's side inset, standing on its foot), in thin lines.
///
/// A line is one plain stroke at the art's width with round ends, in the
/// text's colour taken down to 60 % by night and 50 % by day: soft on the
/// glass, and still clear on it, where 40 % by day was faint and 72 % by
/// night was hard (set side by side on the card, 2026-10-27). No glass in
/// the lines, no light along them, no shadow: lines of glass, each a rod
/// with two edges and a highlight, were what made the faces before these
/// eerie. The lines are one shape, so where two cross the colour does not
/// double.
struct PortraitLines: View {
    let person: Int
    let written: Double
    @Environment(\.colorScheme) private var scheme

    var body: some View {
        GeometryReader { space in
            let width = PortraitPlace(space.size).scale * PortraitArt.lineWidth
            PortraitPath(person: person, written: written)
                .stroke(Theme.foreground.opacity(scheme == .dark ? 0.6 : 0.5),
                        style: StrokeStyle(lineWidth: width, lineCap: .round, lineJoin: .round))
        }
        .allowsHitTesting(false)
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

/// An animal's lines, as far as they are drawn, placed in the space.
struct PortraitPath: Shape {
    let person: Int
    let written: Double

    func path(in rect: CGRect) -> Path {
        var drawn = Path()
        for stroke in PortraitArt.portraits[person].strokes {
            let done = Portrait.drawn(stroke, at: written)
            if done >= 1 {
                drawn.addPath(stroke.path)
            } else if done > 0 {
                drawn.addPath(stroke.path.trimmedPath(from: 0, to: done))
            }
        }
        return drawn.applying(PortraitPlace(rect.size).transform.concatenating(CGAffineTransform(translationX: rect.minX, y: rect.minY)))
    }
}

enum PortraitDebug {
    /// TOMOSHIBI_DEBUG_PORTRAIT=<name>[@<seconds>], in a debug build: that
    /// animal alone, standing still, finished or that far into their
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
