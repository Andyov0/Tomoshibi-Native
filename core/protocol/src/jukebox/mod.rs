//! The song desk: a queue the whole call can add to, played into the call by
//! one participant.
//!
//! Follows web/src/live/jukebox.ts, which is the whole protocol, and the
//! intents of web/src/components/room/MusicPanel.tsx (adding a song, asking
//! by name or by link, voting, the holder's own controls) and
//! web/src/routes/Room.tsx (when a page starts listening).
//!
//! Somebody who can reach the music library runs it -- holds the queue,
//! fetches and plays each track through the same output a single track plays
//! through -- and everybody else sees what is playing and what is next, and
//! adds to it: a track they found, a keyword the desk searches for them, a
//! playlist or song link as it was shared. A song ends and the next one plays;
//! anybody may vote to skip, and a majority of the others listening skips it.
//! The model is the Chinese "song request desk" the open-source Jusic made
//! familiar, kept to what a call needs.
//!
//! Everything travels as data, so in a sealed (encrypted) call -- where the
//! web's SDK options do not encrypt data -- the desk runs for its holder alone
//! and says nothing to the room: what is being listened to is not something
//! the relay is to be told in a call whose promise is that it hears nothing.
//!
//! # How the apps drive it
//!
//! [`JukeboxDesk`] is one room's desk, listener and holder in one. It plays
//! nothing and searches nothing: the music itself is the media crate's (fetch,
//! decode, publish the music track), and the library is the app's. The desk
//! says what to do as [`DeskAction`]s and is told what came of it:
//!
//! - `Prepare` a track (describe it, fetch it, decode it, at this
//!   participant's quality) and answer [`JukeboxDesk::prepared`]; the desk
//!   prepares the song after the one playing ahead of time, as the web does.
//! - `Play` what a request prepared and answer [`JukeboxDesk::played`] once
//!   it is published or has failed; say [`JukeboxDesk::track_ended`] when it
//!   ends by itself.
//! - `Abandon` a request (abort its fetch, drop what it decoded), `Stop` the
//!   music -- which must also make a play still being set up fail -- and
//!   `Pause` or resume it.
//! - `Search` the library for a keyword, or `ReadLink` a shared link, and
//!   answer [`JukeboxDesk::found`] with the songs. The library searched is the
//!   first one signed in, or else the first ([`desk_library`]).
//!
//! As with the theatre, every call takes the time and returns a
//! [`DeskOutput`]; messages on the `jukebox` topic go to `on_message`, `tick`
//! is called at least once a second, and `state` whenever the panel is drawn.

use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
};

use serde::Serialize;
use serde_json::Value;

use crate::watch::js;

/// The data topic (jukebox.ts, `TOPIC`).
pub const TOPIC: &str = "jukebox";

/// How often the holder says the desk is there, for anybody whose copy went
/// stale.
pub const ANNOUNCE_EVERY: u64 = 15_000;

/// How long a desk that has said nothing is believed to still exist: three
/// of its holder's words.
pub const STALE_MS: u64 = 45_000;

/// The most songs one addition carries.
pub const MOST_SONGS: usize = 500;

/// The longest keyword a desk searches for, in UTF-16 units.
pub const QUERY_LENGTH: usize = 100;

/// The longest link a desk reads, in UTF-16 units.
pub const LINK_LENGTH: usize = 2000;

/// A song as a library describes it, before it is anybody's request.
#[derive(Debug, Clone, PartialEq, Serialize, uniffi::Record)]
pub struct DeskSong {
    /// The library it is in.
    pub source: String,
    pub id: String,
    pub title: String,
    pub artists: Vec<String>,
    pub album: String,
    pub cover: String,
    /// Seconds.
    #[serde(serialize_with = "js::json_number")]
    pub duration: f64,
}

/// One song in the queue, and who asked for it.
#[derive(Debug, Clone, PartialEq, Serialize, uniffi::Record)]
pub struct DeskEntry {
    pub source: String,
    pub id: String,
    pub title: String,
    pub artists: Vec<String>,
    pub album: String,
    pub cover: String,
    #[serde(serialize_with = "js::json_number")]
    pub duration: f64,
    /// Distinct per addition, so the same song twice is two entries.
    pub key: String,
    /// The display name of whoever added it.
    pub by: String,
}

/// The desk as its holder says it.
#[derive(Debug, Clone, PartialEq, Serialize, uniffi::Record)]
pub struct DeskState {
    pub holder: String,
    #[serde(rename = "holderName")]
    pub holder_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub now: Option<DeskEntry>,
    pub queue: Vec<DeskEntry>,
    /// Who has voted to skip what is playing.
    pub votes: Vec<String>,
    /// How many votes skip it.
    pub needed: u32,
    /// The next song is being fetched.
    pub loading: bool,
    /// Which: off the queue already, and not playing yet.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next: Option<DeskEntry>,
    /// What is playing is paused, by whoever runs the desk.
    pub paused: bool,
    /// What is playing actually is: "FLAC · 24-bit · 44.1 kHz".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quality: Option<String>,
}

/// The desk as this participant knows it, for drawing.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct DeskSnapshot {
    /// Its own desk, or one heard and still believed; none otherwise.
    pub state: Option<DeskState>,
    /// The desk runs here.
    pub mine: bool,
    /// The desk is said to the room; false in a sealed call, where a desk
    /// runs for its holder alone.
    pub broadcast: bool,
}

/// One packet for the media crate to publish on the `jukebox` topic, exactly
/// as a browser would: `destinations` empty is everybody.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct DeskPacket {
    pub topic: String,
    pub payload: Vec<u8>,
    pub destinations: Vec<String>,
    pub reliable: bool,
}

/// What the desk needs done, by the media crate or the app.
#[derive(Debug, Clone, PartialEq, uniffi::Enum)]
pub enum DeskAction {
    /// Describe, fetch and decode this song, and answer `prepared`.
    Prepare { request: u64, entry: DeskEntry },
    /// Abort that request's fetch and drop whatever it decoded.
    Abandon { request: u64 },
    /// Publish what that request prepared, and answer `played`.
    Play { request: u64, entry: DeskEntry },
    /// Stop the music, and fail any play still being set up.
    Stop,
    /// Pause the music, or go on with it.
    Pause { paused: bool },
    /// Search the library for this keyword and answer `found` with the hits,
    /// best first. `mine` where this participant asked, who is told if
    /// nothing was found.
    Search { request: u64, query: String, mine: bool },
    /// Read the songs a shared playlist or song link names, and answer
    /// `found`.
    ReadLink { request: u64, text: String, mine: bool },
}

/// What came of a `Prepare`.
#[derive(Debug, Clone, PartialEq, uniffi::Enum)]
pub enum DeskPrepared {
    /// Decoded and ready to play; `quality` is how it is described.
    Ready { quality: Option<String> },
    /// The library's account may not play it.
    Unavailable,
    /// It could not be fetched or decoded.
    Failed,
}

/// A library this participant can reach.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct DeskLibrary {
    pub id: String,
    pub signed_in: bool,
}

/// What a call into the desk came to.
#[derive(Debug, Clone, Default, PartialEq, uniffi::Record)]
pub struct DeskOutput {
    /// To publish, in this order.
    pub packets: Vec<DeskPacket>,
    /// To do, in this order.
    pub actions: Vec<DeskAction>,
    /// Songs skipped because they could not be played, for the app to say.
    pub skipped: Vec<DeskEntry>,
    /// What [`JukeboxDesk::state`] returns is different now.
    pub changed: bool,
    /// For an intent: it was taken. False where nobody runs a desk to take
    /// it; for `found`, where nothing was added.
    pub accepted: bool,
}

/// The library a desk searches: the first one signed in, or else the first
/// (jukebox.ts, `Desk.request`).
#[uniffi::export]
pub fn desk_library(libraries: Vec<DeskLibrary>) -> Option<String> {
    libraries
        .iter()
        .find(|library| library.signed_in)
        .or(libraries.first())
        .map(|library| library.id.clone())
}

/// Whether what somebody typed is a link rather than a keyword (MusicPanel.tsx,
/// `LINK`).
pub fn is_link(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("http://") || lower.contains("https://")
}

/// One room's desk, as listener and as holder (jukebox.ts: `Desk`, and the
/// per-room `startDesk`, `closeDesk`, `deskState`, `addToDesk`,
/// `requestSong`, `sendLink`, `voteToSkip` and `hearDesks`).
#[derive(Debug, uniffi::Object)]
pub struct JukeboxDesk {
    inner: Mutex<Inner>,
}

#[uniffi::export]
impl JukeboxDesk {
    /// This participant's identity and display name, and whether the call is
    /// sealed, in which case a desk here plays for its holder alone and none
    /// is heard.
    #[uniffi::constructor]
    pub fn new(identity: String, name: String, sealed: bool) -> Arc<Self> {
        Arc::new(Self {
            inner: Mutex::new(Inner {
                identity,
                name,
                sealed,
                present: HashSet::new(),
                heard: None,
                desk: None,
                requests: 1,
                out: DeskOutput::default(),
                visible: false,
            }),
        })
    }

    /// The room is joined: asks once whoever runs a desk to say what is
    /// playing, so somebody arriving with music already on sees the queue
    /// without waiting for the next announcement.
    pub fn joined(&self, now_ms: u64) -> DeskOutput {
        self.with(now_ms, |inner| {
            if !inner.sealed {
                inner.out.packets.push(Say::Hello.to(Vec::new()));
            }
        })
    }

    /// A message on the `jukebox` topic from the participant `sender`, whose
    /// display name is `sender_name`.
    pub fn on_message(&self, sender: String, sender_name: String, payload: Vec<u8>, now_ms: u64) -> DeskOutput {
        self.with(now_ms, |inner| inner.on_message(sender, sender_name, &payload, now_ms))
    }

    /// Somebody arrived: the holder tells them what is playing, and the
    /// majority a skip needs grows.
    pub fn participant_connected(&self, identity: String, now_ms: u64) -> DeskOutput {
        self.with(now_ms, |inner| {
            inner.present.insert(identity);
            let remotes = inner.present.len();
            if let Some(desk) = inner.desk.as_mut() {
                desk.remotes = remotes;
                desk.announce(now_ms);
            }
        })
    }

    /// Somebody left: their vote no longer counts, the majority is of
    /// whoever is still here, and a desk they held is no longer believed.
    pub fn participant_disconnected(&self, identity: String, now_ms: u64) -> DeskOutput {
        self.with(now_ms, |inner| {
            inner.present.remove(&identity);
            let remotes = inner.present.len();
            if let Some(desk) = inner.desk.as_mut() {
                desk.remotes = remotes;
                if desk.broadcast {
                    desk.left(&identity, now_ms);
                }
            }
            if inner.heard.as_ref().is_some_and(|heard| heard.state.holder == identity) {
                inner.heard = None;
                inner.out.changed = true;
            }
        })
    }

    /// Everybody else in the room, after a reconnect, which replaces the
    /// roster without saying who left.
    pub fn roster(&self, identities: Vec<String>, now_ms: u64) -> DeskOutput {
        self.with(now_ms, |inner| {
            inner.present = identities.into_iter().collect();
            let remotes = inner.present.len();
            if let Some(desk) = inner.desk.as_mut() {
                desk.remotes = remotes;
            }
        })
    }

    /// Time passing: the holder's word every fifteen seconds, a desk heard of
    /// that has gone quiet.
    pub fn tick(&self, now_ms: u64) -> DeskOutput {
        self.with(now_ms, |inner| {
            if let Some(desk) = inner.desk.as_mut() {
                desk.tick(now_ms);
            }
        })
    }

    /// The desk as this participant knows it now.
    pub fn state(&self, now_ms: u64) -> DeskSnapshot {
        let inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        DeskSnapshot {
            state: inner.desk_state(now_ms),
            mine: inner.desk.is_some(),
            broadcast: !inner.sealed,
        }
    }

    /// Start running the desk here. Refused while another is heard;
    /// accepted, doing nothing, where one runs here already.
    pub fn start(&self, now_ms: u64) -> DeskOutput {
        self.with(now_ms, |inner| inner.out.accepted = inner.start(now_ms))
    }

    /// Stop running the desk here, saying so last of all, and stop the music.
    pub fn close(&self, now_ms: u64) -> DeskOutput {
        self.with(now_ms, |inner| {
            if let Some(mut desk) = inner.desk.take() {
                desk.close(now_ms);
                inner.absorb(desk.out);
                inner.requests = desk.requests;
                inner.out.changed = true;
            }
        })
    }

    /// Add songs this participant chose: to its own desk, or sent to whoever
    /// runs the one heard. The app starts a desk here first where none runs
    /// anywhere and this participant can reach a library (MusicPanel.tsx,
    /// `add`).
    pub fn add_songs(&self, songs: Vec<DeskSong>, now_ms: u64) -> DeskOutput {
        self.with(now_ms, |inner| {
            let by = inner.by();
            if let Some(desk) = inner.desk.as_mut() {
                desk.add(songs, &by, now_ms);
                inner.out.accepted = true;
                return;
            }
            let songs: Vec<DeskSong> = songs.into_iter().take(MOST_SONGS).collect();
            inner.out.accepted = inner.tell(Say::Add(&songs), now_ms);
        })
    }

    /// Ask for a song by what was typed: a link names a playlist or a song,
    /// anything else is a keyword whose top hit is played. Asked of this
    /// participant's own desk, or sent to whoever runs the one heard
    /// (MusicPanel.tsx, `ask`). The app starts a desk here first where none
    /// runs anywhere and this participant can reach a library.
    pub fn ask(&self, text: String, now_ms: u64) -> DeskOutput {
        self.with(now_ms, |inner| {
            let by = inner.by();
            let link = is_link(&text);
            if let Some(desk) = inner.desk.as_mut() {
                inner.out.accepted =
                    if link { desk.link(&text, by, true) } else { desk.request(&text, by, true) };
                return;
            }
            inner.out.accepted = if link {
                inner.tell(Say::Link(&js::cut(&text, LINK_LENGTH)), now_ms)
            } else {
                inner.tell(Say::Request(&js::cut(&text, QUERY_LENGTH)), now_ms)
            };
        })
    }

    /// Vote to skip what is playing; the holder's own vote is a skip.
    pub fn vote(&self, now_ms: u64) -> DeskOutput {
        self.with(now_ms, |inner| {
            if let Some(desk) = inner.desk.as_mut() {
                desk.next(now_ms);
                inner.out.accepted = true;
                return;
            }
            inner.out.accepted = inner.tell(Say::Vote, now_ms);
        })
    }

    /// Skip what is playing, at the holder; nothing anywhere else.
    pub fn skip(&self, now_ms: u64) -> DeskOutput {
        self.holding(now_ms, |desk| desk.next(now_ms))
    }

    /// Pause what is playing, or go on with it, at the holder. The next song
    /// plays unpaused.
    pub fn pause(&self, paused: bool, now_ms: u64) -> DeskOutput {
        self.holding(now_ms, |desk| desk.pause(paused, now_ms))
    }

    /// Take a song out, at the holder: one in the queue, or the one on its way
    /// to playing.
    pub fn remove(&self, key: String, now_ms: u64) -> DeskOutput {
        self.holding(now_ms, |desk| desk.remove(&key, now_ms))
    }

    /// Move a song to the top of the queue, at the holder.
    pub fn to_top(&self, key: String, now_ms: u64) -> DeskOutput {
        self.holding(now_ms, |desk| desk.lift(&key, now_ms))
    }

    /// What came of a `Prepare`.
    pub fn prepared(&self, request: u64, outcome: DeskPrepared, now_ms: u64) -> DeskOutput {
        self.holding(now_ms, |desk| desk.prepared(request, outcome, now_ms))
    }

    /// What came of a `Play`: published, or failed.
    pub fn played(&self, request: u64, ok: bool, now_ms: u64) -> DeskOutput {
        self.holding(now_ms, |desk| desk.played(request, ok, now_ms))
    }

    /// The song `key` ended by itself.
    pub fn track_ended(&self, key: String, now_ms: u64) -> DeskOutput {
        self.holding(now_ms, |desk| desk.advance(&key, now_ms))
    }

    /// The songs a `Search` or `ReadLink` found: best first, each with the
    /// library it is in. Empty where nothing was, or the library did not
    /// answer.
    pub fn found(&self, request: u64, songs: Vec<DeskSong>, now_ms: u64) -> DeskOutput {
        self.with(now_ms, |inner| {
            if let Some(desk) = inner.desk.as_mut() {
                inner.out.accepted = desk.found(request, songs, now_ms);
            }
        })
    }
}

impl JukeboxDesk {
    fn with(&self, now: u64, work: impl FnOnce(&mut Inner)) -> DeskOutput {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        work(&mut inner);
        if let Some(desk) = inner.desk.as_mut() {
            desk.resume(now);
            let said = std::mem::take(&mut desk.out);
            inner.absorb(said);
        }
        inner.notice_visibility(now);
        let mut out = std::mem::take(&mut inner.out);
        // Asked twice in one breath -- a song taken out while it was being
        // published stops the music, and so does putting the desk away -- the
        // music is stopped once.
        out.actions.dedup_by(|a, b| matches!((a, b), (DeskAction::Stop, DeskAction::Stop)));
        out
    }

    /// Something only the holder does: accepted where the desk runs here.
    fn holding(&self, now: u64, work: impl FnOnce(&mut Desk)) -> DeskOutput {
        self.with(now, |inner| {
            if let Some(desk) = inner.desk.as_mut() {
                work(desk);
                inner.out.accepted = true;
            }
        })
    }
}

/// A desk heard from somebody else, and when.
#[derive(Debug, Clone)]
struct Heard {
    state: DeskState,
    at: u64,
}

#[derive(Debug)]
struct Inner {
    identity: String,
    name: String,
    sealed: bool,
    present: HashSet<String>,
    heard: Option<Heard>,
    desk: Option<Desk>,
    /// The next request's number, carried from one desk to the next so a late
    /// answer to a desk put away is never taken for one asked by the next.
    requests: u64,
    out: DeskOutput,
    visible: bool,
}

impl Inner {
    fn by(&self) -> String {
        if self.name.is_empty() { self.identity.clone() } else { self.name.clone() }
    }

    fn absorb(&mut self, said: DeskOutput) {
        self.out.packets.extend(said.packets);
        self.out.actions.extend(said.actions);
        self.out.skipped.extend(said.skipped);
        self.out.changed |= said.changed;
    }

    /// A desk heard from somebody else, while it is believed: said something
    /// within [`STALE_MS`], and its holder still in the room.
    fn heard_now(&self, now: u64) -> Option<&Heard> {
        let last = self.heard.as_ref()?;
        if now.saturating_sub(last.at) > STALE_MS || !self.present.contains(&last.state.holder) {
            return None;
        }
        Some(last)
    }

    fn desk_state(&self, now: u64) -> Option<DeskState> {
        match &self.desk {
            Some(desk) => Some(desk.state()),
            None => self.heard_now(now).map(|heard| heard.state.clone()),
        }
    }

    fn notice_visibility(&mut self, now: u64) {
        let visible = self.desk.is_some() || self.heard_now(now).is_some();
        if visible != self.visible {
            self.out.changed = true;
        }
        self.visible = visible;
    }

    fn start(&mut self, now: u64) -> bool {
        if self.desk.is_some() {
            return true;
        }
        if self.desk_state(now).is_some() {
            return false;
        }
        let mut desk = Desk::new(self.identity.clone(), self.name.clone(), !self.sealed, now, self.requests);
        desk.remotes = self.present.len();
        desk.announce(now);
        self.desk = Some(desk);
        self.out.changed = true;
        true
    }

    /// Send something to whoever holds the desk heard (jukebox.ts, `tell`).
    fn tell(&mut self, message: Say<'_>, now: u64) -> bool {
        if self.desk.is_some() {
            return false;
        }
        let Some(heard) = self.heard_now(now) else { return false };
        let packet = message.to(vec![heard.state.holder.clone()]);
        self.out.packets.push(packet);
        true
    }

    fn on_message(&mut self, sender: String, sender_name: String, payload: &[u8], now: u64) {
        // In a sealed call the desk neither speaks nor listens.
        if self.sealed {
            return;
        }
        // Data arrives only from somebody in the room.
        if self.present.insert(sender.clone()) {
            if let Some(desk) = self.desk.as_mut() {
                desk.remotes = self.present.len();
            }
        }
        let Some(message) = js::decode(payload) else { return };

        if let Some(desk) = self.desk.as_mut() {
            let by = if sender_name.is_empty() { sender.clone() } else { sender_name };
            desk.on_message(&message, &sender, by, now);
            return;
        }

        match js::kind(&message) {
            "state" => {
                // Believed only from whoever it says holds it.
                let Some(mut state) = message.get("state").and_then(read_state) else { return };
                if state.holder != sender {
                    return;
                }
                if !sender_name.is_empty() {
                    state.holder_name = sender_name;
                }
                self.heard = Some(Heard { state, at: now });
                self.out.changed = true;
            }
            "closed" if self.heard.as_ref().is_some_and(|heard| heard.state.holder == sender) => {
                self.heard = None;
                self.out.changed = true;
            }
            _ => {}
        }
    }
}

/// A desk's state as heard from somebody else.
///
/// The web takes it as it comes and draws whatever is there. Here it becomes a
/// typed record, so each field is read for the kind it should be and anything
/// else falls back to what a desk with nothing in it says: a song that is not
/// one is left out of the list, a count that is not a number is one.
fn read_state(value: &Value) -> Option<DeskState> {
    let state = value.as_object()?;
    let text = |field: &str| state.get(field).and_then(Value::as_str).map(str::to_string);
    let entries = |field: &str| {
        state
            .get(field)
            .and_then(Value::as_array)
            .map(|list| list.iter().filter_map(read_entry).collect())
            .unwrap_or_default()
    };
    Some(DeskState {
        holder: text("holder")?,
        holder_name: text("holderName").unwrap_or_default(),
        now: state.get("now").and_then(read_entry),
        queue: entries("queue"),
        votes: state
            .get("votes")
            .and_then(Value::as_array)
            .map(|list| list.iter().filter_map(Value::as_str).map(str::to_string).collect())
            .unwrap_or_default(),
        needed: state
            .get("needed")
            .and_then(Value::as_f64)
            .filter(|n| n.is_finite() && *n >= 0.0)
            .map_or(1, |n| n.min(f64::from(u32::MAX)) as u32),
        loading: state.get("loading") == Some(&Value::Bool(true)),
        next: state.get("next").and_then(read_entry),
        paused: state.get("paused") == Some(&Value::Bool(true)),
        quality: text("quality"),
    })
}

fn read_entry(value: &Value) -> Option<DeskEntry> {
    let entry = value.as_object()?;
    let song = read_song(value)?;
    Some(DeskEntry {
        source: song.source,
        id: song.id,
        title: song.title,
        artists: song.artists,
        album: song.album,
        cover: song.cover,
        duration: song.duration,
        key: entry.get("key")?.as_str()?.to_string(),
        by: entry.get("by").and_then(Value::as_str).unwrap_or_default().to_string(),
    })
}

/// A song somebody sent, if it is one (jukebox.ts, `validSong`): a source, an
/// id of under sixty-four units, a title, and a list of artists.
///
/// The web checks those four and keeps the object whole, extra fields and all,
/// to be said again in every state it announces. Here the song is built from
/// the fields a song has, each of its kind, and the rest is left behind.
fn read_song(value: &Value) -> Option<DeskSong> {
    let song = value.as_object()?;
    let source = song.get("source")?.as_str()?;
    let id = song.get("id")?.as_str()?;
    let title = song.get("title")?.as_str()?;
    let artists = song.get("artists")?.as_array()?;
    if id.is_empty() || js::length(id) >= 64 {
        return None;
    }
    let text = |field: &str| song.get(field).and_then(Value::as_str).unwrap_or_default().to_string();
    Some(DeskSong {
        source: source.to_string(),
        id: id.to_string(),
        title: title.to_string(),
        artists: artists.iter().filter_map(Value::as_str).map(str::to_string).collect(),
        album: text("album"),
        cover: text("cover"),
        duration: song.get("duration").and_then(Value::as_f64).filter(|n| n.is_finite()).unwrap_or(0.0),
    })
}

/*
 * The messages, each as the web writes it, `t` first.
 */

#[derive(Serialize)]
struct Bare {
    t: &'static str,
}

#[derive(Serialize)]
struct State<'a> {
    t: &'static str,
    state: &'a DeskState,
}

#[derive(Serialize)]
struct Add<'a> {
    t: &'static str,
    songs: &'a [DeskSong],
}

#[derive(Serialize)]
struct Request<'a> {
    t: &'static str,
    query: &'a str,
}

#[derive(Serialize)]
struct Link<'a> {
    t: &'static str,
    text: &'a str,
}

enum Say<'a> {
    State(&'a DeskState),
    Closed,
    Hello,
    Add(&'a [DeskSong]),
    Request(&'a str),
    Link(&'a str),
    Vote,
}

impl Say<'_> {
    fn bytes(&self) -> Vec<u8> {
        let written = match self {
            Say::State(state) => serde_json::to_vec(&State { t: "state", state }),
            Say::Closed => serde_json::to_vec(&Bare { t: "closed" }),
            Say::Hello => serde_json::to_vec(&Bare { t: "hello" }),
            Say::Add(songs) => serde_json::to_vec(&Add { t: "add", songs }),
            Say::Request(query) => serde_json::to_vec(&Request { t: "request", query }),
            Say::Link(text) => serde_json::to_vec(&Link { t: "link", text }),
            Say::Vote => serde_json::to_vec(&Bare { t: "vote" }),
        };
        written.unwrap_or_default()
    }

    fn to(&self, destinations: Vec<String>) -> DeskPacket {
        DeskPacket { topic: TOPIC.to_string(), payload: self.bytes(), destinations, reliable: true }
    }
}

/// The song fetched to play after the one playing, while it plays.
#[derive(Debug)]
struct Ahead {
    key: String,
    request: u64,
    /// What came of it, once something has.
    result: Option<DeskPrepared>,
}

#[derive(Debug, PartialEq)]
enum Stage {
    /// Being fetched and decoded. `retry` where the request was the one made
    /// ahead of time, whose failure is tried once more before the song is
    /// given up, as the web retries a prefetch that failed.
    Preparing { retry: bool },
    /// Being published, which waits on the media server.
    Publishing,
}

/// The song on its way to playing: off the queue, and not playing yet.
#[derive(Debug)]
struct Fetching {
    entry: DeskEntry,
    request: u64,
    aborted: bool,
    stage: Stage,
    quality: Option<String>,
}

/// The desk, as run here (jukebox.ts, `Desk`).
///
/// The web's `next` is one long async function that awaits the fetch and
/// then the publish; here it is the same steps as a state the answers move
/// on. Where the web went on once an awaited fetch was abandoned, the desk
/// goes on at the end of the call that abandoned it, which is the same order:
/// the rest of that call happens first.
#[derive(Debug)]
struct Desk {
    identity: String,
    name: String,
    broadcast: bool,
    queue: Vec<DeskEntry>,
    now: Option<DeskEntry>,
    quality: Option<String>,
    paused: bool,
    /// Who voted, in the order they did.
    votes: Vec<String>,
    ahead: Option<Ahead>,
    fetching: Option<Fetching>,
    /// A song is being started: from the moment `next` is asked until the
    /// song plays or there is none.
    starting: bool,
    closed: bool,
    /// The loop owes a step: the song on its way was abandoned while it was
    /// being fetched.
    resuming: bool,
    /// Everybody else in the room, which the majority is of.
    remotes: usize,
    next_announce: u64,
    keys: u64,
    requests: u64,
    /// Searches and links being read: who asked, and whether it was here.
    lookups: HashMap<u64, (String, bool)>,
    out: DeskOutput,
}

impl Desk {
    fn new(identity: String, name: String, broadcast: bool, now: u64, requests: u64) -> Self {
        Self {
            identity,
            name,
            broadcast,
            queue: Vec::new(),
            now: None,
            quality: None,
            paused: false,
            votes: Vec::new(),
            ahead: None,
            fetching: None,
            starting: false,
            closed: false,
            resuming: false,
            remotes: 0,
            next_announce: now + ANNOUNCE_EVERY,
            keys: 0,
            requests,
            lookups: HashMap::new(),
            out: DeskOutput::default(),
        }
    }

    fn request_id(&mut self) -> u64 {
        let request = self.requests;
        self.requests += 1;
        request
    }

    /// A majority of everybody else listening.
    fn needed(&self) -> u32 {
        (self.remotes.div_ceil(2) as u32).max(1)
    }

    fn state(&self) -> DeskState {
        DeskState {
            holder: self.identity.clone(),
            holder_name: self.name.clone(),
            now: self.now.clone(),
            queue: self.queue.clone(),
            votes: self.votes.clone(),
            needed: self.needed(),
            loading: self.starting,
            next: self.fetching.as_ref().map(|fetching| fetching.entry.clone()),
            paused: self.now.is_some() && self.paused,
            quality: self.now.as_ref().and(self.quality.clone()),
        }
    }

    fn tick(&mut self, now: u64) {
        if !self.broadcast || self.closed || now < self.next_announce {
            return;
        }
        while self.next_announce <= now {
            self.next_announce += ANNOUNCE_EVERY;
        }
        self.announce(now);
    }

    /// Add songs, by whoever is named; playing starts if nothing is.
    fn add(&mut self, songs: Vec<DeskSong>, by: &str, now: u64) {
        if self.closed || songs.is_empty() {
            return;
        }
        for song in songs.into_iter().take(MOST_SONGS) {
            let key = js::new_key(now, &mut self.keys);
            self.queue.push(DeskEntry {
                source: song.source,
                id: song.id,
                title: song.title,
                artists: song.artists,
                album: song.album,
                cover: song.cover,
                duration: song.duration,
                key,
                by: by.to_string(),
            });
        }
        self.changed(now);
        if self.now.is_none() && !self.starting {
            self.next(now);
        } else {
            self.prefetch();
        }
    }

    /// The top result for a keyword. False where there is no keyword to ask.
    fn request(&mut self, query: &str, by: String, mine: bool) -> bool {
        let asked = js::cut(js::trim(query), QUERY_LENGTH);
        if asked.is_empty() {
            return false;
        }
        let request = self.request_id();
        self.lookups.insert(request, (by, true));
        self.out.actions.push(DeskAction::Search { request, query: asked, mine });
        true
    }

    /// Every song a shared playlist or song link names.
    fn link(&mut self, text: &str, by: String, mine: bool) -> bool {
        let request = self.request_id();
        self.lookups.insert(request, (by, false));
        self.out.actions.push(DeskAction::ReadLink { request, text: js::cut(text, LINK_LENGTH), mine });
        true
    }

    /// What a search or a link found. True where it added something.
    fn found(&mut self, request: u64, songs: Vec<DeskSong>, now: u64) -> bool {
        let Some((by, search)) = self.lookups.remove(&request) else { return false };
        let songs: Vec<DeskSong> = if search { songs.into_iter().take(1).collect() } else { songs };
        let added = !songs.is_empty();
        self.add(songs, &by, now);
        added
    }

    fn vote(&mut self, identity: &str, now: u64) {
        // Not while the next song is on its way. What everybody still sees
        // then is the song already being skipped, and a vote against it would
        // skip the one coming as well.
        if self.now.is_none() || self.starting {
            return;
        }
        if !self.votes.iter().any(|voter| voter == identity) {
            self.votes.push(identity.to_string());
        }
        if self.votes.len() as u32 >= self.needed() {
            self.next(now);
        } else {
            self.changed(now);
        }
    }

    fn pause(&mut self, paused: bool, now: u64) {
        if self.now.is_none() || self.paused == paused {
            return;
        }
        self.paused = paused;
        self.out.actions.push(DeskAction::Pause { paused });
        self.changed(now);
    }

    fn remove(&mut self, key: &str, now: u64) {
        self.queue.retain(|one| one.key != key);
        if self.ahead.as_ref().is_some_and(|ahead| ahead.key == key) {
            self.drop_ahead();
        }
        // The song already on its way to playing is taken out too: it was in
        // the list a moment ago, and taking it out has to mean it never plays.
        // Before, it left the list when it was fetched and played regardless.
        if self.fetching.as_ref().is_some_and(|fetching| fetching.entry.key == key) {
            self.abort();
        }
        self.changed(now);
        self.prefetch();
    }

    fn lift(&mut self, key: &str, now: u64) {
        let Some(at) = self.queue.iter().position(|one| one.key == key) else { return };
        let found = self.queue.remove(at);
        self.queue.insert(0, found);
        self.changed(now);
        self.prefetch();
    }

    fn close(&mut self, now: u64) {
        if self.closed {
            return;
        }
        self.closed = true;
        self.drop_ahead();
        self.abort();
        self.fetching = None;
        self.resuming = false;
        self.starting = false;
        self.lookups.clear();
        self.queue.clear();
        self.now = None;
        self.say(Say::Closed);
        self.out.actions.push(DeskAction::Stop);
        self.changed(now);
    }

    fn left(&mut self, identity: &str, now: u64) {
        // The majority is of whoever is still here.
        self.votes.retain(|voter| voter != identity);
        self.changed(now);
    }

    /// On to the next song.
    ///
    /// One at a time. Asked again while a song is on its way -- a second
    /// press, or a skip voted through -- it is that song that is skipped:
    /// abandoned, and the one after fetched in its place. Before, the web did
    /// nothing at all here, and a song slow to arrive could not be got past.
    fn next(&mut self, now: u64) {
        if self.closed {
            return;
        }
        if self.starting {
            self.abort();
            return;
        }
        self.starting = true;
        self.votes.clear();
        self.step(now);
    }

    /// The song playing ended by itself: on to the next, unless one is on its
    /// way already. Not `next`, which takes being asked while a song loads as a
    /// second press and skips that one too: a song that ended while somebody's
    /// skip was loading the next took the next with it.
    fn advance(&mut self, key: &str, now: u64) {
        if self.starting || self.now.as_ref().map(|entry| entry.key.as_str()) != Some(key) {
            return;
        }
        self.next(now);
    }

    /// Takes the next song off the queue and sets it on its way, or stops the
    /// music when there is none.
    fn step(&mut self, now: u64) {
        if self.queue.is_empty() {
            self.now = None;
            self.changed(now);
            self.out.actions.push(DeskAction::Stop);
            self.finish(now);
            return;
        }
        let entry = self.queue.remove(0);

        // Said at once. The song left the queue here and the list on
        // everybody's screen did not change until it began to play, so it sat
        // at the top of the queue while it loaded, and taking it out there took
        // out nothing: it loaded and played regardless.
        self.fetching = Some(Fetching {
            entry: entry.clone(),
            request: 0,
            aborted: false,
            stage: Stage::Preparing { retry: false },
            quality: None,
        });
        self.changed(now);

        if let Some(ahead) = self.ahead.take_if(|ahead| ahead.key == entry.key) {
            if let Some(fetching) = self.fetching.as_mut() {
                fetching.request = ahead.request;
                fetching.stage = Stage::Preparing { retry: true };
            }
            if let Some(result) = ahead.result {
                self.decoded(result, now);
            }
            return;
        }
        let request = self.request_id();
        if let Some(fetching) = self.fetching.as_mut() {
            fetching.request = request;
        }
        self.out.actions.push(DeskAction::Prepare { request, entry });
    }

    /// The song on its way is done with -- played, or there was none -- and
    /// nothing is starting any more.
    fn finish(&mut self, now: u64) {
        self.starting = false;
        self.changed(now);
    }

    /// Abandons the song on its way (the web's `fetching.abort.abort()`).
    fn abort(&mut self) {
        let Some(fetching) = self.fetching.as_mut().filter(|fetching| !fetching.aborted) else { return };
        fetching.aborted = true;
        match fetching.stage {
            Stage::Preparing { .. } => {
                let request = fetching.request;
                self.out.actions.push(DeskAction::Abandon { request });
                self.resuming = true;
            }
            // Still the song on its way while it is published, which waits on
            // the media server, and still to be abandoned there: the music is
            // stopped, and the play that was setting up finds it so, fails,
            // and the desk goes on when that is said.
            Stage::Publishing => self.out.actions.push(DeskAction::Stop),
        }
    }

    /// Goes on from a song abandoned while it was being fetched, once the
    /// call that abandoned it is done.
    fn resume(&mut self, now: u64) {
        while std::mem::take(&mut self.resuming) {
            if self.closed {
                return;
            }
            if self.fetching.as_ref().is_some_and(|fetching| fetching.aborted) {
                self.fetching = None;
                self.step(now);
            }
        }
    }

    fn prepared(&mut self, request: u64, outcome: DeskPrepared, now: u64) {
        if let Some(ahead) = self.ahead.as_mut().filter(|ahead| ahead.request == request) {
            ahead.result = Some(outcome);
            return;
        }
        let waiting = self.fetching.as_ref().is_some_and(|fetching| {
            fetching.request == request && !fetching.aborted && matches!(fetching.stage, Stage::Preparing { .. })
        });
        if waiting && !self.closed {
            self.decoded(outcome, now);
        }
    }

    fn decoded(&mut self, outcome: DeskPrepared, now: u64) {
        let Some(fetching) = self.fetching.as_mut() else { return };
        let retry = fetching.stage == Stage::Preparing { retry: true };
        match outcome {
            DeskPrepared::Ready { quality } => {
                fetching.stage = Stage::Publishing;
                fetching.quality = quality;
                let action = DeskAction::Play { request: fetching.request, entry: fetching.entry.clone() };
                self.out.actions.push(action);
            }
            // Fetched ahead and failed: tried once more now.
            DeskPrepared::Failed if retry => {
                let entry = fetching.entry.clone();
                let request = self.request_id();
                if let Some(fetching) = self.fetching.as_mut() {
                    fetching.request = request;
                    fetching.stage = Stage::Preparing { retry: false };
                }
                self.out.actions.push(DeskAction::Prepare { request, entry });
            }
            DeskPrepared::Unavailable | DeskPrepared::Failed => {
                if let Some(fetching) = self.fetching.take() {
                    self.out.skipped.push(fetching.entry);
                }
                self.step(now);
            }
        }
    }

    fn played(&mut self, request: u64, ok: bool, now: u64) {
        let publishing = self.fetching.as_ref().is_some_and(|fetching| {
            fetching.request == request && fetching.stage == Stage::Publishing
        });
        if !publishing || self.closed {
            return;
        }
        let Some(fetching) = self.fetching.take() else { return };
        if !ok {
            if !fetching.aborted {
                self.out.skipped.push(fetching.entry);
            }
            self.step(now);
            return;
        }
        self.now = Some(fetching.entry);
        self.paused = false;
        self.quality = fetching.quality;
        self.changed(now);
        self.prefetch();
        self.finish(now);
    }

    /// The next song in the queue, fetched and decoded while this one plays.
    fn prefetch(&mut self) {
        let Some(following) = self.queue.first() else { return };
        if self.closed || self.ahead.as_ref().is_some_and(|ahead| ahead.key == following.key) {
            return;
        }
        let entry = following.clone();
        self.drop_ahead();
        let request = self.request_id();
        self.ahead = Some(Ahead { key: entry.key.clone(), request, result: None });
        self.out.actions.push(DeskAction::Prepare { request, entry });
    }

    fn drop_ahead(&mut self) {
        if let Some(ahead) = self.ahead.take() {
            self.out.actions.push(DeskAction::Abandon { request: ahead.request });
        }
    }

    fn changed(&mut self, now: u64) {
        self.out.changed = true;
        self.announce(now);
    }

    fn announce(&mut self, now: u64) {
        let _ = now;
        // Nothing after the word that it closed: a state sent behind it -- the
        // listeners are told the desk changed as it closes -- would have
        // everybody believe in it again until it went stale.
        if self.closed {
            return;
        }
        let state = self.state();
        self.say(Say::State(&state));
    }

    fn say(&mut self, message: Say<'_>) {
        if self.broadcast {
            self.out.packets.push(message.to(Vec::new()));
        }
    }

    fn on_message(&mut self, message: &Value, sender: &str, by: String, now: u64) {
        if self.closed || !self.broadcast {
            return;
        }
        match js::kind(message) {
            "add" => {
                // A list, or nothing: the web's `songs.filter` on anything else
                // throws in the handler, and nothing is added.
                if let Some(songs) = message.get("songs").and_then(Value::as_array) {
                    let songs = songs.iter().filter_map(read_song).take(MOST_SONGS).collect();
                    self.add(songs, &by, now);
                }
            }
            "request" => {
                self.request(&js::string(message.get("query")), by, false);
            }
            "link" => {
                self.link(&js::string(message.get("text")), by, false);
            }
            "vote" => self.vote(sender, now),
            "hello" => self.announce(now),
            _ => {}
        }
    }
}
