//! The show as it crosses the call: what a video and a show are, how a show
//! heard from somebody else is read, and the exact JSON every message is.
//!
//! Follows the types and `readShow` in web/src/live/watch.ts, and the shape
//! of each message as the web client writes it with `JSON.stringify`, field
//! order included, so that a native holder's word and a browser holder's are
//! the same bytes.

use serde::Serialize;
use serde_json::Value;

use super::{js, TOPIC};

/// The most videos a show heard from elsewhere may queue before it is taken
/// for something other than a show (watch.ts, `readShow`).
pub const MOST_QUEUED: usize = 500;

/// Every refusal there is (watch.ts, `REFUSALS`): why a video was not added,
/// as a code the app turns into a sentence in the person's own language.
pub const REFUSALS: [&str; 9] = [
    "not_a_video",
    "unavailable",
    "members_only",
    "copy_protected",
    "region_locked",
    "not_found",
    "not_allowed",
    "emby_not_allowed",
    "failed",
];

/// One video in the show, as the call knows it: the ticket it plays, who put
/// it there, and the two facts the timeline needs. Everything else -- the
/// title, the picture, where it plays from -- each participant asks the
/// server for by the ticket, so nobody in the call can tell everybody else's
/// player to fetch an address of their own.
#[derive(Debug, Clone, PartialEq, Serialize, uniffi::Record)]
pub struct WatchVideo {
    /// Distinct per addition, so the same video twice is two entries, and a
    /// skip names exactly the one it meant.
    pub key: String,
    /// The display name of whoever added it.
    pub by: String,
    pub ticket: String,
    /// Seconds; nought where unknown.
    #[serde(serialize_with = "js::json_number")]
    pub duration: f64,
    pub live: bool,
}

/// What everybody is told: the show, as its holder says it.
#[derive(Debug, Clone, PartialEq, Serialize, uniffi::Record)]
pub struct WatchShow {
    /// The identity of the participant running it.
    pub holder: String,
    #[serde(rename = "holderName")]
    pub holder_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub now: Option<WatchVideo>,
    pub queue: Vec<WatchVideo>,
    pub playing: bool,
    /// Seconds into `now` when this was said.
    #[serde(serialize_with = "js::json_number")]
    pub position: f64,
    /// Grows with every change, so a message arriving late never undoes a
    /// newer one.
    pub version: u64,
}

/// One packet for the media crate to publish on the `watch` topic, exactly as
/// a browser would: `destinations` empty is everybody.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct WatchPacket {
    pub topic: String,
    pub payload: Vec<u8>,
    pub destinations: Vec<String>,
    pub reliable: bool,
}

/// Whether `ticket` looks like one the gateway issues (watch-api.ts,
/// `TICKET`: `^[\w-]{16,64}$`, where JavaScript's `\w` is ASCII only).
pub fn is_ticket(ticket: &str) -> bool {
    (16..=64).contains(&ticket.len())
        && ticket.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

/// A refusal as it is passed on: one of [`REFUSALS`], or "failed" for
/// anything this client does not know, as the web's `hearShows` reads one.
pub fn refusal(why: &str) -> &str {
    REFUSALS.iter().copied().find(|known| *known == why).unwrap_or("failed")
}

/// A show as told by somebody else, if it is one: every field the type
/// promises, of the kind it promises, within reason (watch.ts, `readShow`).
/// Anything else is dropped whole. It arrives from another participant, and a
/// field that is not what the page expects -- a queue that is not a list --
/// took every viewer's page down in the web client; here it would be a
/// timeline built on a value nobody vouched for.
///
/// Fields a show does not carry are left behind, an address above all: what
/// a video plays is the server's to say, by the ticket, never the holder's.
pub fn read_show(value: &Value) -> Option<WatchShow> {
    let show = value.as_object()?;
    let holder = show.get("holder")?.as_str()?;
    let playing = show.get("playing")?.as_bool()?;
    let position = count(show.get("position"))?;
    let version = count(show.get("version"))?;
    let queue = show.get("queue")?.as_array()?;
    if queue.len() > MOST_QUEUED {
        return None;
    }
    // Absent is no video; present and not a video -- `null` included, as in
    // the web, where only `undefined` means none -- is not a show.
    let now = match show.get("now") {
        None => None,
        Some(one) => Some(read_video(one)?),
    };
    let queue = queue.iter().map(read_video).collect::<Option<Vec<_>>>()?;

    Some(WatchShow {
        holder: holder.to_string(),
        holder_name: show.get("holderName").and_then(Value::as_str).map(|name| js::cut(name, 100)).unwrap_or_default(),
        now,
        queue,
        playing,
        position,
        // A browser's counter is a whole number; one that is not is taken at
        // its floor, which no honest holder can tell apart.
        version: version as u64,
    })
}

fn read_video(value: &Value) -> Option<WatchVideo> {
    let one = value.as_object()?;
    let key = one.get("key")?.as_str()?;
    if js::length(key) > 64 {
        return None;
    }
    let by = one.get("by")?.as_str()?;
    let ticket = one.get("ticket")?.as_str()?;
    if !is_ticket(ticket) {
        return None;
    }
    Some(WatchVideo {
        key: key.to_string(),
        by: js::cut(by, 100),
        ticket: ticket.to_string(),
        duration: count(one.get("duration")).unwrap_or(0.0),
        live: one.get("live") == Some(&Value::Bool(true)),
    })
}

/// A number that is finite and not negative (`readShow`'s `number`).
fn count(value: Option<&Value>) -> Option<f64> {
    value.and_then(Value::as_f64).filter(|n| n.is_finite() && *n >= 0.0)
}

/*
 * The messages, each as the web writes it. One struct per shape rather than
 * a tagged enum, so the field order -- `t` first, then the rest as the web's
 * object literal has them -- is fixed by the declaration and not by serde.
 */

#[derive(Serialize)]
struct Bare {
    t: &'static str,
}

#[derive(Serialize)]
struct State<'a> {
    t: &'static str,
    show: &'a WatchShow,
}

#[derive(Serialize)]
struct Refused<'a> {
    t: &'static str,
    why: &'a str,
}

#[derive(Serialize)]
struct AddText<'a> {
    t: &'static str,
    text: &'a str,
}

#[derive(Serialize)]
struct AddTicket<'a> {
    t: &'static str,
    ticket: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    text: Option<&'a str>,
}

#[derive(Serialize)]
struct Seek {
    t: &'static str,
    #[serde(serialize_with = "js::json_number")]
    to: f64,
}

#[derive(Serialize)]
struct Keyed<'a> {
    t: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    key: Option<&'a str>,
}

/// What the theatre says, in the words the web uses.
pub(crate) enum Say<'a> {
    State(&'a WatchShow),
    Hello,
    Closed,
    Refused(&'a str),
    AddText(&'a str),
    AddTicket(&'a str, Option<&'a str>),
    Play,
    Pause,
    Seek(f64),
    Skip(Option<&'a str>),
    Remove(&'a str),
}

impl Say<'_> {
    pub(crate) fn bytes(&self) -> Vec<u8> {
        let written = match self {
            Say::State(show) => serde_json::to_vec(&State { t: "state", show }),
            Say::Hello => serde_json::to_vec(&Bare { t: "hello" }),
            Say::Closed => serde_json::to_vec(&Bare { t: "closed" }),
            Say::Refused(why) => serde_json::to_vec(&Refused { t: "refused", why }),
            Say::AddText(text) => serde_json::to_vec(&AddText { t: "add", text }),
            Say::AddTicket(ticket, text) => serde_json::to_vec(&AddTicket { t: "add", ticket, text: *text }),
            Say::Play => serde_json::to_vec(&Bare { t: "play" }),
            Say::Pause => serde_json::to_vec(&Bare { t: "pause" }),
            Say::Seek(to) => serde_json::to_vec(&Seek { t: "seek", to: *to }),
            Say::Skip(key) => serde_json::to_vec(&Keyed { t: "skip", key: *key }),
            Say::Remove(key) => serde_json::to_vec(&Keyed { t: "remove", key: Some(key) }),
        };
        // Nothing here can fail to serialize: strings, booleans and numbers.
        written.unwrap_or_default()
    }

    /// As a packet: reliable, as every message on this topic is in the web.
    pub(crate) fn to(&self, destinations: Vec<String>) -> WatchPacket {
        WatchPacket { topic: TOPIC.to_string(), payload: self.bytes(), destinations, reliable: true }
    }
}
