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

type Listener = Box<dyn Fn(Language)>;

struct State {
    language: Language,
    /// Whether the person chose a language rather than following the system.
    chosen: bool,
    laid_over: HashMap<Language, Table>,
    /// By the number [`on_change`] gave them, for [`on_change_while`] to
    /// take one away again.
    listeners: Vec<(u64, Listener)>,
    next: u64,
    /// Listeners taken away while [`tell`] had the list out, so that it is
    /// not put back with them in it.
    removed: Vec<u64>,
}

thread_local! {
    // The interface's language lives on the main thread, as everything that
    // draws does; a phrase wanted elsewhere is asked for there and sent.
    static STATE: RefCell<State> = RefCell::new(State {
        language: Language::detect(system_languages().iter().map(String::as_str)),
        chosen: false,
        laid_over: HashMap::new(),
        listeners: Vec::new(),
        next: 1,
        removed: Vec::new(),
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
/// what is laid over it, changes, for the life of the app: for what lives
/// that long (the window). Returns the number [`forget`] takes.
pub fn on_change(listener: impl Fn(Language) + 'static) -> u64 {
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        let id = state.next;
        state.next += 1;
        state.listeners.push((id, Box::new(listener)));
        id
    })
}

/// [`on_change`] for as long as `widget` lives: taken away when it is
/// destroyed. Every call's screen and every ending added one for good, which
/// did nothing once its screen had gone and was run again at every change of
/// language for the rest of the app's life, one more for every call.
pub fn on_change_while(
    widget: &impl glib::prelude::IsA<gtk::Widget>,
    listener: impl Fn(Language) + 'static,
) {
    use gtk::prelude::WidgetExt as _;
    let id = on_change(listener);
    widget.as_ref().connect_destroy(move |_| forget(id));
}

/// Takes away the listener [`on_change`] numbered `id`.
pub fn forget(id: u64) {
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        let before = state.listeners.len();
        state.listeners.retain(|(kept, _)| *kept != id);
        if state.listeners.len() == before {
            // Out with `tell`, or never there: either way not to come back.
            state.removed.push(id);
        }
    });
}

/// How many listeners there are, for the tests.
#[cfg(test)]
fn listening() -> usize {
    STATE.with(|state| state.borrow().listeners.len())
}

fn tell(language: Language) {
    // Taken out while they run, so that a listener may itself ask for a
    // phrase, add a listener or take one away (a screen destroyed by what
    // another listener did) without the state being borrowed twice.
    let listeners = STATE.with(|state| std::mem::take(&mut state.borrow_mut().listeners));
    for (_, listener) in &listeners {
        listener(language);
    }
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        let added = std::mem::take(&mut state.listeners);
        let removed = std::mem::take(&mut state.removed);
        state.listeners = listeners;
        state.listeners.extend(added);
        state.listeners.retain(|(id, _)| !removed.contains(id));
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

    /// The phrases this app's code asks for: every literal given to `t` or
    /// `t_with` in src/, outside the tests.
    fn phrases_asked_for() -> Vec<(String, String)> {
        fn walk(dir: &std::path::Path, found: &mut Vec<std::path::PathBuf>) {
            for entry in std::fs::read_dir(dir).expect("src is readable").flatten() {
                let path = entry.path();
                if path.is_dir() {
                    walk(&path, found);
                } else if path.extension().is_some_and(|ext| ext == "rs") {
                    found.push(path);
                }
            }
        }
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut files = Vec::new();
        walk(&root, &mut files);
        let mut asked = Vec::new();
        for file in files {
            let text = std::fs::read_to_string(&file).expect("a source file");
            let code = text.split("#[cfg(test)]").next().unwrap_or_default();
            for opener in ["t(\"", "t_with(\""] {
                let mut rest = code;
                while let Some(at) = rest.find(opener) {
                    let before = rest[..at].chars().last();
                    rest = &rest[at + opener.len()..];
                    // `t(` as a word of its own, not the end of `print(`.
                    if before.is_some_and(|c| c.is_alphanumeric() || c == '_') {
                        continue;
                    }
                    let mut phrase = String::new();
                    let mut chars = rest.chars();
                    while let Some(c) = chars.next() {
                        match c {
                            '\\' => {
                                if let Some(next) = chars.next() {
                                    phrase.push(next);
                                }
                            }
                            '"' => break,
                            c => phrase.push(c),
                        }
                    }
                    asked.push((phrase, file.display().to_string()));
                }
            }
        }
        asked
    }

    #[test]
    fn every_phrase_asked_for_is_in_all_four_languages() {
        // As LocalizationCatalogTests does for the Swift sources: a phrase
        // misspelt, or added here and not to native.json, would be shown in
        // English to everybody else.
        let asked = phrases_asked_for();
        assert!(
            asked.len() > 100,
            "the scan found only {} phrases",
            asked.len()
        );
        let missing: Vec<String> = asked
            .iter()
            .filter(|(phrase, _)| {
                [Language::ZhHans, Language::ZhHant, Language::Ja]
                    .iter()
                    .any(|language| !tables()[language].contains_key(phrase))
            })
            .map(|(phrase, file)| format!("{phrase:?} in {file}"))
            .collect();
        assert!(
            missing.is_empty(),
            "phrases not in the catalogue: {missing:#?}"
        );
    }

    #[test]
    fn a_listener_taken_away_is_not_told_again() {
        use std::{cell::Cell, rc::Rc};
        start(Some("en"));
        let told = Rc::new(Cell::new(0));
        let before = listening();
        let counted = told.clone();
        let id = on_change(move |_| counted.set(counted.get() + 1));
        assert_eq!(listening(), before + 1);
        choose(Some(Language::Ja));
        assert_eq!(told.get(), 1);
        forget(id);
        assert_eq!(listening(), before, "the list is back where it was");
        choose(Some(Language::En));
        assert_eq!(told.get(), 1, "not told after it was taken away");
        // One taking itself away while the list is out with `tell`.
        let slot: Rc<Cell<u64>> = Rc::default();
        let (counted, own) = (told.clone(), slot.clone());
        slot.set(on_change(move |_| {
            counted.set(counted.get() + 1);
            forget(own.get());
        }));
        choose(Some(Language::Ja));
        assert_eq!(told.get(), 2);
        choose(Some(Language::En));
        assert_eq!(told.get(), 2, "gone once it had taken itself away");
        assert_eq!(listening(), before);
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
