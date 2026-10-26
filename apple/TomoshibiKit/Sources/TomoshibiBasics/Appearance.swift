import Foundation
import Observation
import SwiftUI

/// The colour the interface spends, chosen by the person using it.
///
/// The product has one signal colour, the amber a studio camera lights when it
/// is live, and everything that says "this is happening" wears it: the primary
/// button, the person speaking, a raised hand, an unread dot. The owner asked
/// for people to be able to choose it (2026-10-15), so it is a setting rather
/// than a constant; everything else in the palette stays fixed, because the
/// neutrals are what keep a call's pictures the brightest thing on screen.
/// Red for leaving and errors and green for a connection light stay as they
/// are whatever is chosen: they are read for their meaning, not noticed.
@MainActor
@Observable
public final class Appearance {
    public static let shared = Appearance()

    /// Where the choice is kept.
    public static let storageKey = "tomoshibi.accent"

    /// Whether a choice is written down; a test driving the app
    /// (AppModel.rehearsal) shows its choices for as long as it runs and
    /// keeps none of them, because the defaults it would write are the
    /// owner's.
    @ObservationIgnored public var keeps = true

    /// Where the choices are kept; a test's own suite in the tests.
    @ObservationIgnored private let defaults: UserDefaults

    public var accent: Accent {
        didSet { keep(accent.rawValue, forKey: Self.storageKey) }
    }

    /// How the connection is shown in a call: the bars alone unless the
    /// person asks for the figures, or nothing at all (the owner's rule,
    /// 2026-10-15: simple by default, details and hiding in the settings).
    public var connectionBadge: ConnectionBadge {
        didSet { keep(connectionBadge.rawValue, forKey: Self.badgeKey) }
    }

    public static let badgeKey = "tomoshibi.connection-badge"

    /// Where the call's controls sit (web/src/live/controls.ts), under the
    /// web's own storage name; "always" is kept as nothing, as the web does.
    public var controlsPlacement: ControlsPlacement {
        didSet { keep(controlsPlacement == .always ? nil : controlsPlacement.rawValue, forKey: Self.controlsKey) }
    }

    public static let controlsKey = "meet-live.controls"

    init(defaults: UserDefaults = .standard) {
        self.defaults = defaults
        accent = defaults.string(forKey: Self.storageKey).flatMap(Accent.init(rawValue:)) ?? .amber
        connectionBadge = defaults.string(forKey: Self.badgeKey).flatMap(ConnectionBadge.init(rawValue:)) ?? .signal
        controlsPlacement = defaults.string(forKey: Self.controlsKey).flatMap(ControlsPlacement.init(rawValue:)) ?? .always
    }

    private func keep(_ value: String?, forKey key: String) {
        guard keeps else { return }
        if let value { defaults.set(value, forKey: key) } else { defaults.removeObject(forKey: key) }
    }
}

/// The three places the web lets the controls sit (controls.ts).
public enum ControlsPlacement: String, CaseIterable, Identifiable, Sendable {
    /// Over the pictures at the foot, always shown.
    case always
    /// Over the pictures, gone after a few still seconds, back on a move.
    case idle
    /// Down the side, where they cost width instead of height.
    case side

    public var id: String { rawValue }

    public var name: String {
        switch self {
        case .always: t("Always shown")
        case .idle: t("Hide when nothing is happening")
        case .side: t("At the side")
        }
    }
}

/// The three ways the connection badge can be shown.
public enum ConnectionBadge: String, CaseIterable, Identifiable, Sendable {
    /// The bars, and nothing to open.
    case signal
    /// The bars and the round trip, opening onto the figures the web shows.
    case detailed
    case hidden

    public var id: String { rawValue }

    public var name: String {
        switch self {
        case .signal: t("Signal only")
        case .detailed: t("With details")
        case .hidden: t("Hidden")
        }
    }
}

/// The colours on offer, each with the colour drawn on top of it.
///
/// Apple's own system colours apart from the first, which is the product's:
/// they are what a person already recognises from the system's accent
/// setting. Each comes in the system's dark-appearance shade, tuned to sit on
/// dark grey, and its light-appearance one, tuned to sit on white; the
/// product's amber has a deeper shade for the day of its own. The four
/// clients draw the same values.
public enum Accent: String, CaseIterable, Identifiable, Sendable {
    case amber, orange, pink, purple, indigo, blue, teal, green, graphite

    public var id: String { rawValue }

    public var color: Color {
        switch self {
        case .amber: Self.shade(dark: (1.0, 0.635, 0.231), light: (242, 144, 30))
        case .orange: Self.shade(dark: (1.0, 0.624, 0.039), light: (255, 149, 0))
        case .pink: Self.shade(dark: (1.0, 0.216, 0.373), light: (255, 45, 85))
        case .purple: Self.shade(dark: (0.749, 0.353, 0.949), light: (175, 82, 222))
        case .indigo: Self.shade(dark: (0.369, 0.361, 0.902), light: (88, 86, 214))
        case .blue: Self.shade(dark: (0.039, 0.518, 1.0), light: (0, 122, 255))
        case .teal: Self.shade(dark: (0.251, 0.784, 0.878), light: (48, 176, 199))
        case .green: Self.shade(dark: (0.188, 0.820, 0.345), light: (52, 199, 89))
        case .graphite: Self.shade(dark: (0.596, 0.596, 0.616), light: (142, 142, 147))
        }
    }

    /// The colour as words and thin symbols are drawn in it, on the page
    /// rather than as a fill: a link, a tick, the colour's own name, a
    /// latency. At night the colour itself; by day a deeper shade, since the
    /// day's shades are for filling and several read at under 3 to 1 as words
    /// on the day's grey (amber at about 2.3), where these read at 4.5 or
    /// better. A fill keeps `color` in either appearance.
    public var ink: Color {
        switch self {
        case .amber: Self.shade(dark: (1.0, 0.635, 0.231), light: (163, 92, 9))
        case .orange: Self.shade(dark: (1.0, 0.624, 0.039), light: (161, 94, 0))
        case .pink: Self.shade(dark: (1.0, 0.216, 0.373), light: (223, 0, 43))
        case .purple: Self.shade(dark: (0.749, 0.353, 0.949), light: (163, 56, 217))
        case .indigo: Self.shade(dark: (0.369, 0.361, 0.902), light: (88, 86, 214))
        case .blue: Self.shade(dark: (0.039, 0.518, 1.0), light: (0, 107, 224))
        case .teal: Self.shade(dark: (0.251, 0.784, 0.878), light: (33, 121, 137))
        case .green: Self.shade(dark: (0.188, 0.820, 0.345), light: (33, 126, 56))
        case .graphite: Self.shade(dark: (0.596, 0.596, 0.616), light: (110, 110, 115))
        }
    }

    /// The night's components as they always were, the day's out of 255 as
    /// the palette gives them.
    private static func shade(dark: (Double, Double, Double), light: (Double, Double, Double)) -> Color {
        Theme.adaptive(dark: Theme.Shade(dark.0, dark.1, dark.2),
                       light: Theme.Shade(light.0 / 255, light.1 / 255, light.2 / 255))
    }

    /// Text and symbols on the colour: dark on the light ones, white on the
    /// deep ones, whichever reads better. The same in both appearances: the
    /// day's shades are a little deeper, which costs the dark words on the
    /// light colours a little (amber's 9.1 to 1 becomes 7.7) and gains the
    /// white on the deep ones a little (blue's 3.6 becomes 4.0), and none of
    /// them would read better the other way.
    public var onColor: Color {
        switch self {
        case .amber, .orange: Color(red: 0.122, green: 0.071, blue: 0.0)
        case .teal: Color(red: 0.0, green: 0.15, blue: 0.18)
        case .green: Color(red: 0.0, green: 0.17, blue: 0.055)
        case .graphite: Color(red: 0.07, green: 0.07, blue: 0.08)
        case .pink, .purple, .indigo, .blue: .white
        }
    }

    /// The colour's name, in the person's language.
    public var name: String {
        switch self {
        case .amber: t("Lamplight")
        case .orange: t("Orange")
        case .pink: t("Pink")
        case .purple: t("Purple")
        case .indigo: t("Indigo")
        case .blue: t("Blue")
        case .teal: t("Teal")
        case .green: t("Green")
        case .graphite: t("Graphite")
        }
    }
}
