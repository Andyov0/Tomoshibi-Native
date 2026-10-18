//! Meetings arranged ahead of time.
//!
//! Follows web/src/live/meeting.ts; the server is internal/app/meeting.go.
//! Arranging, listing and cancelling are for somebody signed in; reading one
//! by its token is for anybody holding the link, who asks it whether the
//! meeting has begun at the pace `meeting_ask_every_ms` sets.

use std::sync::Arc;

use reqwest::Method;
use serde::Serialize;
use serde_json::Value;

use super::{
    read::{some_text, text, yes},
    text::{encode_uri_component, iso_from_ms, ms_from_rfc3339},
    unexpected, Deployment,
};
use crate::{runtime::on_core, ProtocolError};

/// One arranged meeting, as host and guest alike are told it (meeting.ts
/// `Arrangement`).
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct Arrangement {
    pub id: String,
    pub room: String,
    /// When, as an instant; shown in the reader's own zone.
    pub at: String,
    /// The relay it will be held on, where the host chose one.
    pub relay: Option<String>,
    pub started: bool,
    pub ended: bool,
    /// When the host may begin it, which is before `at`.
    pub from: Option<String>,
    /// The secret in the link, said only to the host.
    pub token: Option<String>,
    /// Whether whoever asked arranged it.
    pub mine: bool,
    /// The way in, said only once it has begun and while it is good.
    pub invite: Option<String>,
}

impl Arrangement {
    fn read(said: &Value) -> Self {
        Self {
            id: text(said.get("id")),
            room: text(said.get("room")),
            at: text(said.get("at")),
            relay: some_text(said.get("relay")),
            started: yes(said.get("started")),
            ended: yes(said.get("ended")),
            from: some_text(said.get("from")),
            token: some_text(said.get("token")),
            mine: yes(said.get("mine")),
            invite: some_text(said.get("invite")),
        }
    }
}

/// How often to ask whether a meeting at `at` has begun, at `now_ms`
/// (meeting.ts `askEvery`).
///
/// Every three seconds inside the last ten minutes, and every thirty before:
/// a link opened an hour early is a tab left open, and thirty people behind one
/// office's address asking every three seconds for an hour would trip the rate
/// limit for all of them. A time that cannot be read is NaN in the web client,
/// which is never "far off", so it asks quickly.
#[uniffi::export]
pub fn meeting_ask_every_ms(at: String, now_ms: i64) -> u64 {
    match ms_from_rfc3339(&at) {
        Some(at) if at - now_ms > 10 * 60_000 => 30_000,
        _ => 3_000,
    }
}

/// meeting.ts `HOST_NOT_COMING_AFTER`: how long after the arranged time to keep
/// asking before concluding the host is not coming. Longer than anybody is
/// late, shorter than the day the server keeps the record.
#[uniffi::export]
pub fn host_not_coming_after_ms() -> u64 {
    2 * 60 * 60_000
}

#[derive(Serialize)]
struct Arranging<'a> {
    room: &'a str,
    at: String,
    relay: &'a str,
}

#[uniffi::export]
impl Deployment {
    /// Arranges a meeting on `room` at `at_ms` (milliseconds since the epoch),
    /// on `relay` or wherever suits when it is empty.
    ///
    /// The time is sent as `toISOString` sends it, an instant in UTC with
    /// milliseconds: a time with no zone is a time in a zone somebody guessed,
    /// and the server refuses it (`bad_time`).
    pub async fn arrange_meeting(
        self: Arc<Self>,
        room: String,
        at_ms: i64,
        relay: String,
    ) -> Result<Arrangement, ProtocolError> {
        on_core(async move {
            let at = iso_from_ms(at_ms).ok_or_else(|| ProtocolError::Invalid {
                detail: "that time is out of range".into(),
            })?;
            let body = serde_json::to_string(&Arranging {
                room: &room,
                at,
                relay: &relay,
            })
            .map_err(|fault| {
                unexpected(format!("the arrangement could not be written: {fault}"))
            })?;
            let request = self
                .api
                .json_request(Method::POST, "/api/meetings")?
                .body(body);
            let said = self
                .api
                .send(request)
                .await?
                .accepted("")?
                .json::<Value>("the arrangement")?;
            Ok(Arrangement::read(&said))
        })
        .await
    }

    /// The signed-in host's own arrangements, soonest first, each with its token.
    pub async fn arrangements(self: Arc<Self>) -> Result<Vec<Arrangement>, ProtocolError> {
        on_core(async move {
            let said = self
                .api
                .send(self.api.json_request(Method::GET, "/api/meetings")?)
                .await?
                .accepted("")?
                .json::<Value>("the arrangements")?;
            Ok(said
                .get("meetings")
                .and_then(Value::as_array)
                .map(|all| all.iter().map(Arrangement::read).collect())
                .unwrap_or_default())
        })
        .await
    }

    /// What a meeting link points at; `no_such_meeting` for a dead one.
    pub async fn arranged(self: Arc<Self>, token: String) -> Result<Arrangement, ProtocolError> {
        on_core(async move {
            let path = format!("/api/meetings/{}", encode_uri_component(&token));
            let said = self
                .api
                .send(self.api.json_request(Method::GET, &path)?)
                .await?
                .accepted("")?
                .json::<Value>("the arrangement")?;
            Ok(Arrangement::read(&said))
        })
        .await
    }

    /// Cancels one of the signed-in host's arrangements, by its id.
    pub async fn cancel_meeting(self: Arc<Self>, id: String) -> Result<(), ProtocolError> {
        on_core(async move {
            let path = format!("/api/meetings/{}", encode_uri_component(&id));
            self.api
                .send(self.api.json_request(Method::DELETE, &path)?)
                .await?
                .accepted("")
                .map(drop)
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // meeting.test.ts, "how often to ask".
    #[test]
    fn asks_slowly_while_the_meeting_is_far_off() {
        let at = "2026-09-02T10:00:00.000Z".to_owned();
        let t = ms_from_rfc3339(&at).unwrap();

        assert_eq!(meeting_ask_every_ms(at.clone(), t - 60 * 60_000), 30_000);
        assert_eq!(meeting_ask_every_ms(at, t - 11 * 60_000), 30_000);
    }

    #[test]
    fn asks_quickly_inside_the_last_ten_minutes_and_after_the_time() {
        let at = "2026-09-02T10:00:00.000Z".to_owned();
        let t = ms_from_rfc3339(&at).unwrap();

        assert_eq!(meeting_ask_every_ms(at.clone(), t - 9 * 60_000), 3_000);
        assert_eq!(meeting_ask_every_ms(at.clone(), t), 3_000);
        assert_eq!(meeting_ask_every_ms(at, t + 30 * 60_000), 3_000);
        assert_eq!(meeting_ask_every_ms("not a time".into(), 0), 3_000);
    }

    #[test]
    fn reads_an_arrangement_leniently() {
        let said = serde_json::json!({
            "id": "m1", "room": "standup", "at": "2026-09-02T10:00:00Z", "relay": "",
            "started": true, "ended": "no", "from": "2026-09-02T09:45:00Z", "mine": true, "added": 1
        });
        let read = Arrangement::read(&said);

        assert_eq!(read.relay, None);
        assert!(read.started && !read.ended && read.mine);
        assert_eq!(read.token, None);
    }
}
