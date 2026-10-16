import CoreGraphics
import Foundation

// The rules for the corner of a call, where notices and what people said
// appear, kept apart from the views that draw them so that they can be held
// to the web's numbers by a test.

/// How long the call's notices stay (web/src/live/notices.ts).
public enum NoticeTimes {
    /// An ordinary notice: long enough to read four words (`BRIEFLY`).
    public static let briefly: Double = 2.2
    /// A share, because the picture is about to change under you (`A_MOMENT`).
    public static let aMoment: Double = 3
    /// The longest any notice stays (`AT_MOST`). Ten seconds has been seen;
    /// for ever has been ignored.
    public static let atMost: Double = 10
    /// What fits above the controls without becoming a column of its own;
    /// the fourth pushes the oldest out (Notices.tsx `visibleToasts`).
    public static let shown = 3
    /// The least a held notice is given once let go. Sonner resumes with
    /// whatever was left, so a notice read to its last tenth of a second
    /// vanished the instant the pointer moved off it, which looked like the
    /// pointer had knocked it away.
    public static let afterHolding: Double = 1

    /// Which notices to drop so that no more than `shown` remain, oldest
    /// first: only ones that go by themselves (`lasting` false). One that
    /// stays until it is dealt with, somebody at the door, is never pushed out
    /// by somebody arriving.
    public static func overflow(_ lasting: [Bool], limit: Int = shown) -> [Int] {
        var excess = lasting.count - limit
        var drop: [Int] = []
        for (index, stays) in lasting.enumerated() where excess > 0 && !stays {
            drop.append(index)
            excess -= 1
        }
        return drop
    }
}

/// A notice's countdown, which waits while the corner is being read (sonner
/// pauses every timer while its stack is expanded under the pointer).
public struct NoticeClock: Equatable, Sendable {
    /// Seconds still to go, as of `since` when running.
    public private(set) var left: Double
    /// When it last started running, on whatever clock the caller keeps.
    public private(set) var since: Double?

    public init(lasts: Double, now: Double, running: Bool) {
        left = lasts
        since = running ? now : nil
    }

    public var running: Bool { since != nil }

    /// Stops it, keeping what was left.
    public mutating func hold(now: Double) {
        guard let since else { return }
        left = max(0, left - (now - since))
        self.since = nil
    }

    /// Starts it again, with at least `NoticeTimes.afterHolding` to go.
    public mutating func run(now: Double) {
        guard since == nil else { return }
        left = max(left, NoticeTimes.afterHolding)
        since = now
    }

    /// Seconds from `now` until it is due; nil while it is held.
    public func due(now: Double) -> Double? {
        guard let since else { return nil }
        return max(0, left - (now - since))
    }
}

/// Where each notice in the corner sits: piled, the newest in front and the
/// others peeking above it, until the corner is read, when they spread into a
/// column (sonner's stack, which the web's notices use as it comes).
public enum NoticePile {
    /// Between notices spread into a column (Notices.tsx `gap`).
    public static let gap: CGFloat = 8
    /// How much of each notice behind the front one shows above it.
    public static let peek: CGFloat = 9
    /// How much smaller each one behind is drawn, per place.
    public static let shrink: CGFloat = 0.05

    /// How far up from the bottom of the corner each notice's bottom edge
    /// sits, newest first (index 0 is the front), given each one's height in
    /// the same order. Piled, each sits `peek` above the one in front, all of
    /// them drawn the front one's height; spread, each sits on the one below.
    public static func lifts(_ heights: [CGFloat], spread: Bool) -> [CGFloat] {
        var lifts: [CGFloat] = []
        var below: CGFloat = 0
        for (place, height) in heights.enumerated() {
            if spread {
                lifts.append(below)
                below += height + gap
            } else {
                // Each is drawn the front one's height, so lifting its bottom
                // edge by `peek` a place lifts its top edge by the same.
                lifts.append(CGFloat(place) * peek)
            }
        }
        return lifts
    }

    /// The height the whole corner takes.
    public static func height(_ heights: [CGFloat], spread: Bool) -> CGFloat {
        guard let front = heights.first else { return 0 }
        if spread { return heights.reduce(0, +) + gap * CGFloat(heights.count - 1) }
        return front + peek * CGFloat(heights.count - 1)
    }

    /// The scale a notice is drawn at in its place.
    public static func scale(place: Int, spread: Bool) -> CGFloat {
        spread ? 1 : max(0.5, 1 - shrink * CGFloat(place))
    }
}

/// Keeping the notices and what people said out of each other's way.
///
/// Messages sit in the bottom-leading corner and notices in the bottom-
/// trailing one. On a wide screen they never meet; on a phone, or a window
/// narrowed to a strip, both columns cover the width, and a notice drawn over
/// somebody's words hides both. There the notices rise above the messages,
/// and not the other way about: a message stays six seconds and a notice two,
/// so the one that comes and goes is the one that moves.
public enum CornerLanes {
    /// Space kept between the two.
    public static let gap: CGFloat = 8

    /// How far the notices must rise, given where the messages are drawn and
    /// the leading edge of the notices' own column, both in one coordinate
    /// space; nothing while the two do not share any width.
    public static func lift(notices leading: CGFloat, said: CGRect?) -> CGFloat {
        guard let said, !said.isEmpty, said.maxX + gap > leading else { return 0 }
        return said.height + gap
    }
}

/// The parts of the web's chat that are rules rather than drawing.
public enum ChatRules {
    /// The longest message the box takes (ChatPanel.tsx `maxLength`).
    public static let longest = 2000

    /// `text` cut to `longest`, counted as a browser counts `maxLength`, in
    /// UTF-16 code units, but never through the middle of a character: a
    /// pasted emoji at the limit is left out whole rather than sent as half
    /// a surrogate pair.
    public static func clamp(_ text: String, to limit: Int = longest) -> String {
        guard text.utf16.count > limit else { return text }
        var kept = ""
        var used = 0
        for character in text {
            let size = String(character).utf16.count
            if used + size > limit { break }
            kept.append(character)
            used += size
        }
        return kept
    }

    /// Whether the line at `index` carries on a run by the same person, and
    /// so is drawn without their name (ChatPanel.tsx, Said.tsx): the column is
    /// narrow and repeating it says nothing new.
    public static func continues(_ speakers: [String], at index: Int) -> Bool {
        index > 0 && index < speakers.count && speakers[index - 1] == speakers[index]
    }
}
