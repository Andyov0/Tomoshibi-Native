//! What the screens before a call show beside their forms: the meetings this
//! account has arranged (Schedule.tsx), the rooms joined lately, and the
//! relays as measured from here (ServerPicker.tsx), as LobbyModel.cs and
//! Android's LobbyModel.kt keep them. Kept apart from the screens because
//! the sidebar, the stage and the sheets show the same lists, and a list
//! fetched twice is a list that disagrees with itself for a moment.

use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
};

use serde_json::Value;
use tomoshibi_protocol::{
    api::Arrangement,
    relays::{forget_relay_measurement, relay_timings, Relay},
};

use super::{words, AppModel, Change, Preferences};
use crate::i18n::{say, t};

/// The lobby's own change, for the screens that show its lists.
pub const CHANGED: Change = Change::Module("lobby");

pub const RECENTS_KEY: &str = "tomoshibi.recents";
pub const RECENTS_KEPT: usize = 24;

/// Where the list of arranged meetings has got to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Status {
    #[default]
    Loading,
    Ready,
    Failed,
    SignedOut,
}

/// A room this device joined, kept so that it can be joined again in one
/// press: a call app's recent list, which the web has no place for. Kept on
/// this device only and per deployment, and never anything but the name and
/// when.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recent {
    pub room: String,
    pub server: String,
    /// Milliseconds since 1970, as the other apps keep it.
    pub at_ms: i64,
}

/// What the model keeps (`AppModel::kept`).
#[derive(Default)]
pub struct LobbyState {
    pub arrangements: RefCell<Vec<Arrangement>>,
    pub status: Cell<Status>,
    reads: Cell<u64>,
    pub recents: RefCell<Vec<Recent>>,
    loaded: Cell<bool>,
    /// Round trips by relay name, none until measured; a relay that did not
    /// answer is present with no number.
    pub measured: RefCell<Option<HashMap<String, Option<f64>>>>,
    pub measuring: Cell<bool>,
    /// The order the first measurement settled on, so that a row does not
    /// move out from under a pointer already reaching for it.
    pub order: RefCell<Vec<String>>,
}

pub fn load_recents(preferences: &Preferences) -> Vec<Recent> {
    let Some(Value::Array(items)) = preferences
        .string(RECENTS_KEY)
        .and_then(|text| serde_json::from_str(&text).ok())
    else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| {
            let room = item.get("room")?.as_str()?.to_owned();
            let server = item.get("server")?.as_str()?.to_owned();
            if room.is_empty() || server.is_empty() {
                return None;
            }
            let at_ms = item.get("at").and_then(Value::as_i64).unwrap_or(0);
            Some(Recent { room, server, at_ms })
        })
        .collect()
}

/// Written field by field in the order the other apps write them (room,
/// server, at), whatever order serde_json's map would keep.
pub fn save_recents(preferences: &Preferences, recents: &[Recent]) {
    let items: Vec<String> = recents
        .iter()
        .map(|recent| {
            format!(
                "{{\"room\":{},\"server\":{},\"at\":{}}}",
                Value::from(recent.room.as_str()),
                Value::from(recent.server.as_str()),
                recent.at_ms
            )
        })
        .collect();
    preferences.set(RECENTS_KEY, format!("[{}]", items.join(",")));
}

/// The list with `room` on `server` at its head, once.
pub fn remembered(recents: &[Recent], room: &str, server: &str, at_ms: i64) -> Vec<Recent> {
    std::iter::once(Recent {
        room: room.to_owned(),
        server: server.to_owned(),
        at_ms,
    })
    .chain(
        recents
            .iter()
            .filter(|recent| !(recent.server == server && recent.room == room))
            .cloned(),
    )
    .take(RECENTS_KEPT)
    .collect()
}

fn state(model: &AppModel) -> std::rc::Rc<LobbyState> {
    let state = model.kept::<LobbyState>();
    if !state.loaded.replace(true) {
        state.recents.replace(load_recents(model.preferences()));
    }
    state
}

impl AppModel {
    /// The lobby's lists.
    pub fn lobby(&self) -> std::rc::Rc<LobbyState> {
        state(self)
    }

    /// The meetings still ahead or under way, soonest first.
    pub fn upcoming(&self) -> Vec<Arrangement> {
        let mut upcoming: Vec<Arrangement> = state(self)
            .arrangements
            .borrow()
            .iter()
            .filter(|one| !one.ended)
            .cloned()
            .collect();
        upcoming.sort_by_key(|one| words::instant(&one.at).unwrap_or(i64::MAX));
        upcoming
    }

    pub fn refresh_arrangements(&self) {
        let (Some(deployment), Some(_)) = (self.deployment(), self.account()) else {
            return;
        };
        let lobby = state(self);
        let mine = lobby.reads.get() + 1;
        lobby.reads.set(mine);
        lobby.status.set(Status::Loading);
        self.announce(CHANGED);
        let weak = self.downgrade();
        glib::spawn_future_local(async move {
            let found = deployment.arrangements().await;
            let Some(model) = weak.upgrade() else {
                return;
            };
            let lobby = state(&model);
            if lobby.reads.get() != mine {
                return;
            }
            match found {
                Ok(found) => {
                    lobby.arrangements.replace(found);
                    lobby.status.set(Status::Ready);
                }
                Err(fault) => lobby.status.set(
                    if words::code(&fault) == Some("not_signed_in") {
                        Status::SignedOut
                    } else {
                        Status::Failed
                    },
                ),
            }
            model.announce(CHANGED);
        });
    }

    /// Arranges a meeting for `at` (seconds since 1970); the sentence why
    /// not where it was refused.
    pub async fn arrange(&self, room: String, at: i64, relay: Option<String>) -> Result<Arrangement, String> {
        let Some(deployment) = self.deployment() else {
            return Err(t("Something went wrong. Try again."));
        };
        match deployment
            .arrange_meeting(room, at * 1000, relay.unwrap_or_default())
            .await
        {
            Ok(made) => {
                let lobby = state(self);
                lobby.reads.set(lobby.reads.get() + 1);
                lobby.arrangements.borrow_mut().push(made.clone());
                lobby.status.set(Status::Ready);
                self.announce(CHANGED);
                Ok(made)
            }
            Err(fault) => Err(words::explain_arranging(&fault)),
        }
    }

    /// Calls a meeting off; the sentence to say where that did not take.
    pub async fn cancel_meeting(&self, id: String) -> Result<(), String> {
        let Some(deployment) = self.deployment() else {
            return Ok(());
        };
        match deployment.cancel_meeting(id.clone()).await {
            Ok(()) => {
                let lobby = state(self);
                lobby.reads.set(lobby.reads.get() + 1);
                lobby.arrangements.borrow_mut().retain(|one| one.id != id);
                self.announce(CHANGED);
                Ok(())
            }
            Err(fault) => Err(words::explain_arranging(&fault)),
        }
    }

    /// meeting.ts `linkFor`: the core writes it as the web does.
    pub fn meeting_link(&self, one: &Arrangement) -> Option<String> {
        let deployment = self.deployment()?;
        Some(deployment.meeting_link(one.token.clone()?, one.room.clone()))
    }

    /// calendar.ts `icsFor`: the meeting as a calendar file's text.
    pub fn calendar_text(&self, one: &Arrangement) -> Option<String> {
        let link = self.meeting_link(one)?;
        let start = words::instant(&one.at)?;
        Some(calendar_text(&one.id, &one.room, &link, start, words::now()))
    }

    /// The rooms joined on the deployment showing now, latest first.
    pub fn recent_here(&self) -> Vec<Recent> {
        let here = self
            .deployment()
            .map(|deployment| deployment.base_url())
            .unwrap_or_default();
        let mut recents: Vec<Recent> = state(self)
            .recents
            .borrow()
            .iter()
            .filter(|recent| recent.server == here)
            .cloned()
            .collect();
        recents.sort_by(|a, b| b.at_ms.cmp(&a.at_ms));
        recents
    }

    pub fn remember_room(&self, room: &str) {
        let Some(server) = self.deployment().map(|deployment| deployment.base_url()) else {
            return;
        };
        let lobby = state(self);
        let next = remembered(&lobby.recents.borrow(), room, &server, words::now() * 1000);
        save_recents(self.preferences(), &next);
        lobby.recents.replace(next);
        self.announce(CHANGED);
    }

    /// Takes a room off the recent list, which is the person's own record of
    /// where they have been and theirs to tidy.
    pub fn forget_recent(&self, recent: &Recent) {
        let lobby = state(self);
        lobby.recents.borrow_mut().retain(|kept| kept != recent);
        save_recents(self.preferences(), &lobby.recents.borrow());
        self.announce(CHANGED);
    }

    /// Measures every relay from here, for the server section.
    pub fn measure_relays(&self) {
        let lobby = state(self);
        let Some(relays) = self.relays().filter(|list| !list.relays.is_empty()) else {
            return;
        };
        if lobby.measuring.replace(true) {
            return;
        }
        self.announce(CHANGED);
        let weak = self.downgrade();
        glib::spawn_future_local(async move {
            let timings = relay_timings(relays.relays).await;
            let Some(model) = weak.upgrade() else {
                return;
            };
            let lobby = state(&model);
            if lobby.order.borrow().is_empty() {
                let mut ranked: Vec<(String, f64)> = timings
                    .iter()
                    .map(|one| (one.name.clone(), one.millis.unwrap_or(f64::INFINITY)))
                    .collect();
                ranked.sort_by(|a, b| a.1.total_cmp(&b.1));
                lobby
                    .order
                    .replace(ranked.into_iter().map(|(name, _)| name).collect());
            }
            lobby.measured.replace(Some(
                timings.into_iter().map(|one| (one.name, one.millis)).collect(),
            ));
            lobby.measuring.set(false);
            model.announce(CHANGED);
        });
    }

    /// The fastest relay that answered, which is what "automatic" settles
    /// on: never one kept for administrators, never one out of service
    /// (relays.ts `fastest`).
    pub fn fastest_relay(&self) -> Option<(String, f64)> {
        let lobby = state(self);
        let measured = lobby.measured.borrow();
        let relays = self.relays()?;
        relays
            .relays
            .iter()
            .filter(|relay| !relay.admin_only && !relay.maintenance)
            .filter_map(|relay| Some((relay.name.clone(), (*measured.as_ref()?.get(&relay.name)?)?)))
            .min_by(|a, b| a.1.total_cmp(&b.1))
    }

    /// What a relay is called to a person: its label, translated where it is
    /// a phrase.
    pub fn relay_label(&self, name: &str) -> String {
        self.relays()
            .and_then(|list| list.relays.into_iter().find(|relay| relay.name == name))
            .map(|relay| say(relay.label.as_deref().unwrap_or(&relay.name)))
            .unwrap_or_else(|| name.to_owned())
    }

    /// The relays under their region, in the settled order.
    pub fn relays_by_region(&self) -> Vec<(String, Vec<Relay>)> {
        let relays = self.relays().map(|list| list.relays).unwrap_or_default();
        let order = state(self).order.borrow().clone();
        let mut ranked = relays;
        if !order.is_empty() {
            ranked.sort_by_key(|relay| {
                order
                    .iter()
                    .position(|name| *name == relay.name)
                    .unwrap_or(usize::MAX)
            });
        }
        let mut groups: Vec<(String, Vec<Relay>)> = Vec::new();
        for relay in ranked {
            let region = relay
                .region
                .as_deref()
                .unwrap_or_default()
                .split('/')
                .map(str::trim)
                .filter(|part| !part.is_empty())
                .map(say)
                .collect::<Vec<_>>()
                .join(" · ");
            match groups.iter_mut().find(|(name, _)| *name == region) {
                Some((_, members)) => members.push(relay),
                None => groups.push((region, vec![relay])),
            }
        }
        groups
    }

    /// A relay that has just dropped a call is the one thing known about the
    /// fleet; without forgetting the measurement it would go on being named
    /// the fastest for minutes (App.tsx, the lost connection).
    pub fn forget_relay_measurement(&self) {
        forget_relay_measurement();
    }
}

/// The calendar file of one meeting, as calendar.ts writes it.
pub fn calendar_text(id: &str, room: &str, link: &str, start: i64, now: i64) -> String {
    let stamp = |at: i64| {
        let wall = words::Wall::at(at, 0);
        let seconds = at.rem_euclid(60);
        format!(
            "{:04}{:02}{:02}T{:02}{:02}{:02}Z",
            wall.year, wall.month, wall.day, wall.hour, wall.minute, seconds
        )
    };
    let escape = |text: &str| {
        text.replace('\\', "\\\\")
            .replace(';', "\\;")
            .replace(',', "\\,")
            .replace('\n', "\\n")
    };
    let lines = [
        "BEGIN:VCALENDAR".to_owned(),
        "VERSION:2.0".into(),
        "PRODID:-//Tomoshibi//Arranged meeting//EN".into(),
        "CALSCALE:GREGORIAN".into(),
        "METHOD:PUBLISH".into(),
        "BEGIN:VEVENT".into(),
        format!("UID:{id}@tomoshibi"),
        format!("DTSTAMP:{}", stamp(now)),
        format!("DTSTART:{}", stamp(start)),
        format!("SUMMARY:{}", escape(room)),
        format!("URL:{link}"),
        format!("DESCRIPTION:{}", escape(link)),
        "END:VEVENT".into(),
        "END:VCALENDAR".into(),
    ];
    lines.join("\r\n") + "\r\n"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recents_are_kept_once_latest_first_and_survive_a_restart() {
        let preferences = Preferences::in_memory();
        let mut recents = remembered(&[], "a", "http://h/", 1);
        recents = remembered(&recents, "b", "http://h/", 2);
        recents = remembered(&recents, "a", "http://h/", 3);
        assert_eq!(
            recents.iter().map(|r| r.room.as_str()).collect::<Vec<_>>(),
            ["a", "b"]
        );
        save_recents(&preferences, &recents);
        assert_eq!(
            preferences.string(RECENTS_KEY).as_deref(),
            Some(r#"[{"room":"a","server":"http://h/","at":3},{"room":"b","server":"http://h/","at":2}]"#)
        );
        assert_eq!(load_recents(&preferences), recents);
        let many = (0..30).fold(Vec::new(), |list, n| remembered(&list, &format!("r{n}"), "s", n));
        assert_eq!(many.len(), RECENTS_KEPT);
    }

    #[test]
    fn a_meeting_is_a_calendar_file() {
        let text = calendar_text("m1", "team, weekly", "https://x/?meeting=t#/team", 1_793_527_200, 1_793_491_200);
        assert!(text.contains("DTSTART:20261101T100000Z\r\n"));
        assert!(text.contains("SUMMARY:team\\, weekly\r\n"));
        assert!(text.ends_with("END:VCALENDAR\r\n"));
    }
}
