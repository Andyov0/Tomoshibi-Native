import Foundation
import Observation
import os

/// The language in use, and the phrases in it.
///
/// Held here rather than in the SwiftUI environment for the reason the web
/// keeps it outside React: notices are raised from places no view owns, such as
/// a person arriving, a device refused, the core reporting a failure from its
/// own thread. Those need the same words as everything else, and an
/// environment value cannot reach them. So `t()` works from any thread, with or
/// without SwiftUI.
///
/// It is also observable, and reading a phrase is what registers a view's
/// interest. `t()` reads `language` through the observation registrar, so a
/// view whose body says anything re-renders when the language changes, without
/// having to remember to watch for it: the web's `useT()` makes subscribing
/// and translating one call for the same reason, so that they cannot come
/// apart.
///
/// The state sits behind a lock rather than in a macro-generated stored
/// property, because the property is read from whatever thread a notice comes
/// from while a picker on the main thread writes it.
public final class Localization: Observable, @unchecked Sendable {
    /// The one the app speaks from, and the one the free functions use.
    public static let shared = Localization()

    /// Where a person's choice is kept. The web's key, old name and all: the
    /// web keeps it so that renaming the product did not forget everybody's
    /// language, and the apps use the same so that the two read alike.
    public static let storageKey = "meet-live.locale"

    private let registrar = ObservationRegistrar()
    private let current: OSAllocatedUnfairLock<Language>
    private let defaults: UserDefaults

    /// Settle on a language before anything is drawn in the wrong one.
    ///
    /// A choice already made outranks everything; a stored value that is not
    /// one of the four codes exactly counts as no choice, as on the web.
    /// Failing that, the preference list is asked, in order.
    public init(defaults: UserDefaults = .standard, preferredLanguages: [String] = Locale.preferredLanguages) {
        self.defaults = defaults
        let chosen = defaults.string(forKey: Self.storageKey).flatMap(Language.init(rawValue:))
        current = OSAllocatedUnfairLock(initialState: chosen ?? Language.detect(from: preferredLanguages))
    }

    /// The language in use. Setting it is a person's choice, and is kept.
    ///
    /// The web keeps a choice only when it differs from the language already
    /// showing, so somebody who picks English while English was merely detected
    /// has chosen nothing, and follows the system to Japanese later. Here every
    /// choice is kept, since a person who picked a language from a list meant
    /// it. Watchers are told only when the language actually changes.
    public var language: Language {
        get {
            registrar.access(self, keyPath: \.language)
            return current.withLock { $0 }
        }
        set {
            defaults.set(newValue.rawValue, forKey: Self.storageKey)
            guard current.withLock({ $0 }) != newValue else { return }
            registrar.withMutation(of: self, keyPath: \.language) {
                current.withLock { $0 = newValue }
            }
        }
    }

    /// Say something: the phrase in the current language, values in place.
    public func t(_ phrase: String, _ values: PhraseValues? = nil) -> String {
        substitute(Phrasebook.template(phrase, in: language), values)
    }

    /// Translate text that only exists at runtime: the web's `say()`.
    ///
    /// For what the deployment supplies rather than the interface, a relay's
    /// label typed into a management page. One that matches a phrase the
    /// interface already says, ignoring case because it was typed by a person,
    /// comes back in the reader's language; anything else comes back exactly as
    /// typed. Kept apart from `t()` so that `t()` stays the call every phrase in
    /// the source goes through, which LocalizationTests checks by reading the
    /// source.
    public func say(_ text: String) -> String {
        let language = language
        if let exact = Phrasebook.tables[language]?[text] { return exact }
        if let folded = Phrasebook.folded[language]?[text.lowercased()] { return folded }
        return text
    }

    /// The finished translation of a phrase, cut at its placeholders, for a
    /// phrase that carries something other than text.
    public func phraseSegments(_ phrase: String) -> [PhraseSegment] {
        PhraseSegment.split(Phrasebook.template(phrase, in: language))
    }
}

/// Say something, in the language the app is speaking.
///
///     Text(t("Show {name} larger", ["name": person.name]))
///
/// Every phrase the interface shows goes through here, keyed by its English,
/// and is in the catalog in all four languages (apple/i18n/README.md).
public func t(_ phrase: String, _ values: PhraseValues? = nil) -> String {
    Localization.shared.t(phrase, values)
}

/// Translate text the deployment supplies, such as a relay's label.
public func say(_ text: String) -> String {
    Localization.shared.say(text)
}

/// A phrase in the current language, cut at its placeholders.
public func phraseSegments(_ phrase: String) -> [PhraseSegment] {
    Localization.shared.phraseSegments(phrase)
}
