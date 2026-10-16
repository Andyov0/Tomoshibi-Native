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

    public var accent: Accent {
        didSet { UserDefaults.standard.set(accent.rawValue, forKey: Self.storageKey) }
    }

    /// How the connection is shown in a call: the bars alone unless the
    /// person asks for the figures, or nothing at all (the owner's rule,
    /// 2026-10-15: simple by default, details and hiding in the settings).
    public var connectionBadge: ConnectionBadge {
        didSet { UserDefaults.standard.set(connectionBadge.rawValue, forKey: Self.badgeKey) }
    }

    public static let badgeKey = "tomoshibi.connection-badge"

    /// Where the call's controls sit (web/src/live/controls.ts), under the
    /// web's own storage name; "always" is kept as nothing, as the web does.
    public var controlsPlacement: ControlsPlacement {
        didSet {
            UserDefaults.standard.set(controlsPlacement == .always ? nil : controlsPlacement.rawValue, forKey: Self.controlsKey)
        }
    }

    public static let controlsKey = "meet-live.controls"

    private init() {
        accent = UserDefaults.standard.string(forKey: Self.storageKey).flatMap(Accent.init(rawValue:)) ?? .amber
        connectionBadge = UserDefaults.standard.string(forKey: Self.badgeKey).flatMap(ConnectionBadge.init(rawValue:)) ?? .signal
        controlsPlacement = UserDefaults.standard.string(forKey: Self.controlsKey).flatMap(ControlsPlacement.init(rawValue:)) ?? .always
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
/// Apple's own dark-appearance system colours apart from the first, which is
/// the product's: they are what a person already recognises from the system's
/// accent setting, and they were tuned to sit on dark grey.
public enum Accent: String, CaseIterable, Identifiable, Sendable {
    case amber, orange, pink, purple, indigo, blue, teal, green, graphite

    public var id: String { rawValue }

    public var color: Color {
        switch self {
        case .amber: Color(red: 1.0, green: 0.635, blue: 0.231)
        case .orange: Color(red: 1.0, green: 0.624, blue: 0.039)
        case .pink: Color(red: 1.0, green: 0.216, blue: 0.373)
        case .purple: Color(red: 0.749, green: 0.353, blue: 0.949)
        case .indigo: Color(red: 0.369, green: 0.361, blue: 0.902)
        case .blue: Color(red: 0.039, green: 0.518, blue: 1.0)
        case .teal: Color(red: 0.251, green: 0.784, blue: 0.878)
        case .green: Color(red: 0.188, green: 0.820, blue: 0.345)
        case .graphite: Color(red: 0.596, green: 0.596, blue: 0.616)
        }
    }

    /// Text and symbols on the colour: dark on the light ones, white on the
    /// deep ones, whichever reads at 4.5 to 1.
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
