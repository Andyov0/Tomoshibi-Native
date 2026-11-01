//! What the interface says, in the language of whoever is reading it: the
//! web's i18n.ts, as on Apple (Phrases.swift), Android (Localization.kt) and
//! Windows (Localization.cs).
//!
//! Every phrase is keyed by its English, from the same catalogue the other
//! apps build from: apple/i18n/generate.mjs writes Apple's strings from the
//! web's dictionaries and native.json, and android/i18n.mjs writes the
//! phrases.json that Android and Windows read and that is built into this
//! binary below. A phrase with no translation falls back to its key, a whole
//! readable sentence. A phrase this app adds goes into native.json like any
//! other app's, in all four languages, never into a table of its own here.
//!
//! The deployment may lay phrases of its own over these (the core's
//! api/remote.rs, `remote_phrases`), so that wording can change between
//! releases without anybody starting the app again; the app sets them with
//! [`set_laid_over`] and everything that said something says it again.

use std::{cell::RefCell, collections::HashMap, sync::OnceLock};

/// The catalogue, as android/i18n.mjs writes it: one object per language
/// code, keyed by the English.
const PHRASES: &str = include_str!("../../android/app/src/main/assets/phrases.json");

/// The web's own storage name for a chosen language (i18n.ts).
pub const STORAGE_KEY: &str = "meet-live.locale";

/// The web client's four languages (i18n.ts `LOCALES`), with their codes as
/// stored values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Language {
    En,
    ZhHans,
    ZhHant,
    Ja,
}

impl Language {
    pub const ALL: [Language; 4] = [
        Language::En,
        Language::ZhHans,
        Language::ZhHant,
        Language::Ja,
    ];

    pub fn code(self) -> &'static str {
        match self {
            Language::En => "en",
            Language::ZhHans => "zh-Hans",
            Language::ZhHant => "zh-Hant",
            Language::Ja => "ja",
        }
    }

    pub fn of(code: &str) -> Option<Language> {
        Self::ALL
            .into_iter()
            .find(|language| language.code() == code)
    }

    /// What the language calls itself. Never translated: a language picker
    /// is read by somebody who cannot yet read the language on screen.
    pub fn display_name(self) -> &'static str {
        match self {
            Language::En => "English",
            Language::ZhHans => "简体中文",
            Language::ZhHant => "繁體中文",
            Language::Ja => "日本語",
        }
    }

    /// Which language to open in, given the system's preference list: the
    /// web's `detect()` rule for rule, read in order, with an explicit script
    /// subtag deciding before a region (simplified Chinese with the region
    /// set to Hong Kong arrives as zh-Hans-HK, which the region rule alone
    /// would open in traditional). POSIX names (zh_TW.UTF-8) are read the
    /// same way.
    pub fn detect<'a>(preferred: impl IntoIterator<Item = &'a str>) -> Language {
        for tag in preferred {
            let lower = tag.to_ascii_lowercase();
            let lower = lower.split(['.', '@']).next().unwrap_or_default();
            if lower.starts_with("ja") {
                return Language::Ja;
            }
            if lower.starts_with("zh") {
                let subtags: Vec<&str> = lower.split(['-', '_']).collect();
                if subtags.contains(&"hant") {
                    return Language::ZhHant;
                }
                if subtags.contains(&"hans") {
                    return Language::ZhHans;
                }
                let traditional = ["tw", "hk", "mo"]
                    .iter()
                    .any(|region| subtags.contains(region));
                return if traditional {
                    Language::ZhHant
                } else {
                    Language::ZhHans
                };
            }
            if lower.starts_with("en") {
                return Language::En;
            }
        }
        Language::En
    }
}

type Table = HashMap<String, String>;

fn tables() -> &'static HashMap<Language, Table> {
    static TABLES: OnceLock<HashMap<Language, Table>> = OnceLock::new();
    TABLES.get_or_init(|| parse(PHRASES))
}

/// The catalogue's text read into a table per language; a language missing
/// from it is an empty table, whose every phrase is its English.
pub fn parse(json: &str) -> HashMap<Language, Table> {
    let root: serde_json::Value = serde_json::from_str(json).unwrap_or_default();
    Language::ALL
        .into_iter()
        .map(|language| {
            let table = root
                .get(language.code())
                .and_then(serde_json::Value::as_object)
                .map(|entries| {
                    entries
                        .iter()
                        .filter_map(|(key, value)| Some((key.clone(), value.as_str()?.to_owned())))
                        .collect()
                })
                .unwrap_or_default();
            (language, table)
        })
        .collect()
}

struct State {
    language: Language,
    /// Whether the person chose a language rather than following the system.
    chosen: bool,
    laid_over: HashMap<Language, Table>,
    listeners: Vec<Box<dyn Fn(Language)>>,
}

thread_local! {
    // The interface's language lives on the main thread, as everything that
    // draws does; a phrase wanted elsewhere is asked for there and sent.
    static STATE: RefCell<State> = RefCell::new(State {
        language: Language::detect(system_languages().iter().map(String::as_str)),
        chosen: false,
        laid_over: HashMap::new(),
        listeners: Vec::new(),
    });
}

/// The system's language preference, as GLib reads it from LANGUAGE,
/// LC_ALL, LC_MESSAGES and LANG, most wanted first.
pub fn system_languages() -> Vec<String> {
    glib::language_names()
        .iter()
        .map(|name| name.to_string())
        .collect()
}

/// The language on screen.
pub fn language() -> Language {
    STATE.with(|state| state.borrow().language)
}

/// Whether the language was chosen rather than followed from the system.
pub fn chosen() -> bool {
    STATE.with(|state| state.borrow().chosen)
}

/// Opens on a language kept from before (the preferences' [`STORAGE_KEY`]),
/// or the system's when there is none. Says nothing to the listeners: it is
/// called before anything is on screen.
pub fn start(kept: Option<&str>) {
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        match kept.and_then(Language::of) {
            Some(language) => {
                state.language = language;
                state.chosen = true;
            }
            None => {
                state.language = Language::detect(system_languages().iter().map(String::as_str));
                state.chosen = false;
            }
        }
    });
}

/// Shows the interface in `language`, or the system's when `None`; the
/// caller keeps the choice (or removes it) under [`STORAGE_KEY`].
pub fn choose(language: Option<Language>) {
    let (now, changed) = STATE.with(|state| {
        let mut state = state.borrow_mut();
        let next = language
            .unwrap_or_else(|| Language::detect(system_languages().iter().map(String::as_str)));
        state.chosen = language.is_some();
        let changed = next != state.language;
        state.language = next;
        (next, changed)
    });
    if changed {
        tell(now);
    }
}

/// The deployment's phrases, by language and then by the English they are
/// keyed by. A change to what is laid over in the language on screen is said
/// as a change of language is.
pub fn set_laid_over(laid_over: HashMap<Language, Table>) {
    let (now, changed) = STATE.with(|state| {
        let mut state = state.borrow_mut();
        let language = state.language;
        let changed = state.laid_over.get(&language) != laid_over.get(&language);
        state.laid_over = laid_over;
        (language, changed)
    });
    if changed {
        tell(now);
    }
}

/// Calls `listener` on the main thread whenever the language on screen, or
/// what is laid over it, changes. For screens that are not rebuilt anyway.
pub fn on_change(listener: impl Fn(Language) + 'static) {
    STATE.with(|state| state.borrow_mut().listeners.push(Box::new(listener)));
}

fn tell(language: Language) {
    // Taken out while they run, so that a listener may itself ask for a
    // phrase or add a listener without the state being borrowed twice.
    let listeners = STATE.with(|state| std::mem::take(&mut state.borrow_mut().listeners));
    for listener in &listeners {
        listener(language);
    }
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        let added = std::mem::take(&mut state.listeners);
        state.listeners = listeners;
        state.listeners.extend(added);
    });
}

/// The template for a phrase in `language`: the deployment's where it laid
/// one over, the app's own otherwise, the English key last.
pub fn template_in(phrase: &str, language: Language) -> String {
    let laid = STATE.with(|state| {
        state
            .borrow()
            .laid_over
            .get(&language)
            .and_then(|table| table.get(phrase).cloned())
    });
    laid.or_else(|| {
        tables()
            .get(&language)
            .and_then(|table| table.get(phrase).cloned())
    })
    .unwrap_or_else(|| phrase.to_owned())
}

/// A phrase in the language on screen.
pub fn t(phrase: &str) -> String {
    template_in(phrase, language())
}

/// A phrase with its `{name}` placeholders filled. A placeholder given no
/// value becomes nothing, as on the web.
pub fn t_with(phrase: &str, values: &[(&str, &dyn std::fmt::Display)]) -> String {
    substitute(&t(phrase), values)
}

/// Text the deployment supplies rather than the interface, such as a relay's
/// label: translated if it matches a phrase exactly or ignoring case, shown as
/// typed otherwise.
pub fn say(text: &str) -> String {
    let language = language();
    let exact = template_in(text, language);
    if exact != text {
        return exact;
    }
    let lower = text.to_lowercase();
    tables()
        .get(&language)
        .and_then(|table| {
            let mut keys: Vec<&String> = table
                .keys()
                .filter(|key| key.to_lowercase() == lower)
                .collect();
            keys.sort();
            keys.first().and_then(|key| table.get(*key)).cloned()
        })
        .unwrap_or_else(|| text.to_owned())
}

/// `{name}` replaced by its value (Localization.cs `Substitute`); a brace
/// that does not open a well-formed placeholder is kept as written.
pub fn substitute(template: &str, values: &[(&str, &dyn std::fmt::Display)]) -> String {
    let mut built = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        built.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let name_len = after
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .unwrap_or(after.len());
        if name_len > 0 && after[name_len..].starts_with('}') {
            let name = &after[..name_len];
            if let Some((_, value)) = values.iter().find(|(key, _)| *key == name) {
                built.push_str(&value.to_string());
            }
            rest = &after[name_len + 1..];
        } else {
            built.push('{');
            rest = after;
        }
    }
    built.push_str(rest);
    built
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_catalogue_built_in_has_all_four_languages() {
        let tables = tables();
        for language in Language::ALL {
            assert!(
                !tables[&language].is_empty() || language == Language::En,
                "{language:?} is empty"
            );
        }
        // Compared with the catalogue rather than written out, so that the
        // test holds whatever wording the translators settle on.
        for language in [Language::ZhHans, Language::ZhHant, Language::Ja] {
            let translated = template_in("Sign in", language);
            assert_eq!(translated, tables[&language]["Sign in"]);
            assert_ne!(
                translated, "Sign in",
                "{language:?} has no translation of a phrase every app shows"
            );
        }
    }

    #[test]
    fn a_phrase_with_no_translation_is_its_english() {
        assert_eq!(
            template_in("A phrase nobody wrote", Language::Ja),
            "A phrase nobody wrote"
        );
    }

    #[test]
    fn the_language_is_detected_as_the_web_detects_it() {
        assert_eq!(Language::detect(["zh_TW.UTF-8"]), Language::ZhHant);
        assert_eq!(
            Language::detect(["zh-Hans-HK"]),
            Language::ZhHans,
            "the script decides before the region"
        );
        assert_eq!(Language::detect(["zh_HK"]), Language::ZhHant);
        assert_eq!(Language::detect(["zh_CN.UTF-8"]), Language::ZhHans);
        assert_eq!(Language::detect(["ja_JP.UTF-8"]), Language::Ja);
        assert_eq!(Language::detect(["fr_FR", "C"]), Language::En);
        assert_eq!(
            Language::detect(["fr_FR", "ja"]),
            Language::Ja,
            "read in order"
        );
    }

    #[test]
    fn placeholders_are_filled_and_a_missing_one_is_nothing() {
        let three = 3;
        assert_eq!(
            substitute("{count} people", &[("count", &three)]),
            "3 people"
        );
        assert_eq!(substitute("Hello {name}.", &[]), "Hello .");
        assert_eq!(substitute("a { b {} c", &[]), "a { b {} c");
    }

    #[test]
    fn laid_over_phrases_win_in_their_language_only() {
        start(Some("ja"));
        let own = tables()[&Language::Ja]["Sign in"].clone();
        let mut japanese = Table::new();
        japanese.insert("Sign in".into(), "laid over".into());
        set_laid_over(HashMap::from([(Language::Ja, japanese)]));
        assert_eq!(t("Sign in"), "laid over");
        assert_eq!(
            template_in("Sign in", Language::ZhHans),
            tables()[&Language::ZhHans]["Sign in"]
        );
        set_laid_over(HashMap::new());
        assert_eq!(t("Sign in"), own);
    }
}
