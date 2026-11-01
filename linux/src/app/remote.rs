//! What the deployment says to the apps between their releases (the core's
//! api/remote.rs, `GET /api/apps/remote`): wording laid over the app's own
//! phrases, features switched off, numbers the app reads where it would use
//! its own, and a notice for the lobby. Remote.cs, Remote.kt and
//! Remote.swift, by the same rules and under the same names.
//!
//! The last answer is kept in the preferences and read back before the first
//! screen is drawn ([`restore`]), so that a feature switched off stays off
//! on a train and a phrase laid over does not flicker back to the app's own
//! at every start. Then the deployment is asked: at start, every half hour
//! after, and again whenever the app comes back to the lobby, at most once
//! a minute. An answer replaces what applied and is kept; a question that is
//! not answered changes nothing and says nothing, since what it would have
//! said is whatever was said last time.

use std::{
    cell::{Cell, RefCell},
    collections::{HashMap, HashSet},
    time::{Duration, Instant},
};

use tomoshibi_protocol::api::{
    parse_remote_config, remote_notice_text, remote_phrases, RemoteConfig,
};

use super::{log, words, AppModel, Change, Preferences, Screen};
use crate::i18n::{self, Language};

/// Every half hour (the owner's, in the spec of 2026-10-22).
pub const EVERY: Duration = Duration::from_secs(30 * 60);
/// How soon a question asked for by coming back may follow the one before.
pub const SOONEST: Duration = Duration::from_secs(60);
pub const KEPT_KEY: &str = "tomoshibi.remote";
pub const CLOSED_KEY: &str = "tomoshibi.remote.closed";
/// How many closed notices are remembered: an id is never put up again
/// once taken down, so the oldest only take up room.
pub const CLOSED_HELD: usize = 20;

/// The features the deployment can switch off, by its names.
pub mod feature {
    pub const SCREEN_SHARE: &str = "screen-share";
    pub const BACKGROUND_BLUR: &str = "background-blur";
    pub const WATCH_TOGETHER: &str = "watch-together";
    pub const MUSIC: &str = "music";
    pub const APP_UPDATES: &str = "app-updates";
}

/// A notice the lobby shows above everything else in it.
#[derive(Debug, Clone, PartialEq)]
pub struct Notice {
    pub id: String,
    /// Something to take care about, drawn as the app draws a problem.
    pub warning: bool,
    /// The words by language, worked out by the core when it was heard; a
    /// language missing here has nothing it can read, and is shown nothing.
    pub text: HashMap<Language, String>,
    /// Seconds since 1970 when it stops being shown.
    pub until: Option<i64>,
}

/// What the deployment said, as the app uses it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Answer {
    pub revision: String,
    pub phrases: HashMap<Language, HashMap<String, String>>,
    pub off: HashSet<String>,
    pub params: HashMap<String, f64>,
    pub notice: Option<Notice>,
    pub kept: String,
}

impl Answer {
    pub fn of(config: RemoteConfig) -> Answer {
        // Only the languages something is laid over in: an empty table for
        // the others read to the phrases as a change, and every screen was
        // built again for nothing a moment after it first showed.
        let phrases = Language::ALL
            .into_iter()
            .map(|language| (language, remote_phrases(config.clone(), language.code().into())))
            .filter(|(_, phrases)| !phrases.is_empty())
            .collect();
        let notice = config.notice.clone().map(|notice| Notice {
            id: notice.id.clone(),
            warning: notice.warning,
            until: notice.until,
            text: Language::ALL
                .into_iter()
                .filter_map(|language| {
                    Some((
                        language,
                        remote_notice_text(notice.clone(), language.code().into())?,
                    ))
                })
                .collect(),
        });
        Answer {
            revision: config.revision,
            phrases,
            off: config.off.into_iter().collect(),
            params: config.params,
            notice,
            kept: config.kept,
        }
    }

    pub fn allows(&self, feature: &str) -> bool {
        !self.off.contains(feature)
    }

    /// How long between looks for an update (update_check_hours), or none
    /// for the app's own six hours; never under ten minutes, the wait after
    /// a look that failed, so that a fraction of an hour written by mistake
    /// does not have every copy of the app asking every few seconds.
    pub fn update_interval(&self) -> Option<Duration> {
        let hours = *self.params.get("update_check_hours")?;
        if !hours.is_finite() || hours <= 0.0 {
            return None;
        }
        Some(Duration::from_secs_f64((hours * 3600.0).max(600.0)))
    }
}

/// What the model keeps of it (`AppModel::kept`).
#[derive(Default)]
pub struct RemoteState {
    pub answer: RefCell<Answer>,
    closed: RefCell<Vec<String>>,
    asking: Cell<bool>,
    asked: Cell<Option<Instant>>,
    started: Cell<bool>,
}

fn read_closed(preferences: &Preferences) -> Vec<String> {
    preferences
        .string(CLOSED_KEY)
        .and_then(|text| serde_json::from_str::<Vec<String>>(&text).ok())
        .unwrap_or_default()
}

/// The answer kept from before, applied before the first screen is drawn.
pub fn restore(model: &AppModel) {
    let state = model.kept::<RemoteState>();
    state.closed.replace(read_closed(model.preferences()));
    if let Some(config) = model
        .preferences()
        .string(KEPT_KEY)
        .and_then(parse_remote_config)
    {
        apply(model, Answer::of(config), false);
    }
}

/// Starts asking: now, and every half hour after. Once.
pub fn start(model: &AppModel) {
    let state = model.kept::<RemoteState>();
    if state.started.replace(true) {
        return;
    }
    ask(model);
    let weak = model.downgrade();
    glib::timeout_add_local(EVERY, move || match weak.upgrade() {
        Some(model) => {
            ask(&model);
            glib::ControlFlow::Continue
        }
        None => glib::ControlFlow::Break,
    });
    // Coming back to the lobby asks again: a call can be long, and the
    // lobby is where what was said shows.
    let weak = model.downgrade();
    model.subscribe(move |model, change| {
        if change == Change::Screen && matches!(model.screen(), Screen::Lobby | Screen::SignIn) {
            if let Some(model) = weak.upgrade() {
                let state = model.kept::<RemoteState>();
                let recent = state
                    .asked
                    .get()
                    .is_some_and(|asked| asked.elapsed() < SOONEST);
                if !recent {
                    ask(&model);
                }
            }
        }
    });
    // A notice that runs out is taken down then, without anything else
    // having to draw the lobby again.
    let weak = model.downgrade();
    glib::timeout_add_seconds_local(30, move || match weak.upgrade() {
        Some(model) => {
            let state = model.kept::<RemoteState>();
            let ran_out = state
                .answer
                .borrow()
                .notice
                .as_ref()
                .and_then(|notice| notice.until)
                .is_some_and(|until| until <= words::now());
            if ran_out {
                state.answer.borrow_mut().notice = None;
                model.announce(Change::Remote);
            }
            glib::ControlFlow::Continue
        }
        None => glib::ControlFlow::Break,
    });
}

/// Asks the deployment once, unless a question is out.
pub fn ask(model: &AppModel) {
    let Some(deployment) = model.deployment() else {
        return;
    };
    let state = model.kept::<RemoteState>();
    if state.asking.replace(true) {
        return;
    }
    state.asked.set(Some(Instant::now()));
    let weak = model.downgrade();
    glib::spawn_future_local(async move {
        let heard = deployment.remote_config().await;
        let Some(model) = weak.upgrade() else {
            return;
        };
        model.kept::<RemoteState>().asking.set(false);
        match heard {
            Ok(config) => apply(&model, Answer::of(config), true),
            Err(fault) => log(&format!("the deployment's word to the apps was not heard: {fault}")),
        }
    });
}

fn apply(model: &AppModel, answer: Answer, heard: bool) {
    let state = model.kept::<RemoteState>();
    if heard && !answer.kept.is_empty() {
        model.preferences().set(KEPT_KEY, &answer.kept);
    }
    if *state.answer.borrow() == answer {
        return;
    }
    if heard {
        log(&format!(
            "the deployment's word to the apps: revision {}, {} off, {}",
            if answer.revision.is_empty() { "none" } else { &answer.revision },
            answer.off.len(),
            if answer.notice.is_some() { "a notice" } else { "no notice" },
        ));
    }
    i18n::set_laid_over(answer.phrases.clone());
    state.answer.replace(answer);
    model.announce(Change::Remote);
}

/// Whether a feature is offered (the answer's `off`).
pub fn allows(model: &AppModel, feature: &str) -> bool {
    model.kept::<RemoteState>().answer.borrow().allows(feature)
}

/// The notice for the lobby and the sign-in, in the language on screen:
/// none once it has run out, none once closed here, none in a language it
/// has no words for.
pub fn notice(model: &AppModel) -> Option<(Notice, String)> {
    let state = model.kept::<RemoteState>();
    let notice = state.answer.borrow().notice.clone()?;
    if notice.until.is_some_and(|until| until <= words::now()) {
        return None;
    }
    if state.closed.borrow().contains(&notice.id) {
        return None;
    }
    let text = notice.text.get(&i18n::language())?.clone();
    Some((notice, text))
}

/// The notice's close: this id is not shown again on this device, even
/// after the app is started again.
pub fn close(model: &AppModel, id: &str) {
    let state = model.kept::<RemoteState>();
    {
        let mut closed = state.closed.borrow_mut();
        closed.retain(|kept| kept != id);
        closed.push(id.to_owned());
        let excess = closed.len().saturating_sub(CLOSED_HELD);
        closed.drain(..excess);
        if let Ok(text) = serde_json::to_string(&*closed) {
            model.preferences().set(CLOSED_KEY, text);
        }
    }
    model.announce(Change::Remote);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_update_interval_is_never_under_ten_minutes() {
        let mut answer = Answer::default();
        assert_eq!(answer.update_interval(), None);
        answer.params.insert("update_check_hours".into(), 0.001);
        assert_eq!(answer.update_interval(), Some(Duration::from_secs(600)));
        answer.params.insert("update_check_hours".into(), 12.0);
        assert_eq!(answer.update_interval(), Some(Duration::from_secs(12 * 3600)));
        answer.params.insert("update_check_hours".into(), -1.0);
        assert_eq!(answer.update_interval(), None);
    }

    #[test]
    fn a_kept_answer_is_read_back_whole() {
        let kept = r#"{"revision":"r7","off":["music"],"params":{"update_check_hours":24},
            "phrases":{"Sign in":{"ja":"入る"}},
            "notice":{"id":"n1","level":"warning","text":{"en":"Down at nine.","ja":"九時に停止します。"}}}"#;
        let answer = Answer::of(parse_remote_config(kept.into()).expect("readable"));
        assert_eq!(answer.revision, "r7");
        assert!(!answer.allows(feature::MUSIC));
        assert!(answer.allows(feature::WATCH_TOGETHER));
        assert_eq!(answer.phrases[&Language::Ja]["Sign in"], "入る");
        assert!(!answer.phrases.contains_key(&Language::ZhHans), "nothing laid over there");
        let notice = answer.notice.expect("a notice");
        assert_eq!(notice.text[&Language::Ja], "九時に停止します。");
        assert_eq!(notice.text[&Language::ZhHans], "Down at nine.", "English stands in");
    }
}
