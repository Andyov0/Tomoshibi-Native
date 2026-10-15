//! Watching a video together.
//!
//! Follows web/src/live/watch.ts, which is the whole protocol, and the parts
//! of the pages around it that decide what crosses the call: adding a video
//! (`addVideo` in web/src/components/room/WatchTab.tsx), when a page starts
//! and stops listening (web/src/routes/Room.tsx), and the loop that holds a
//! player to the show (web/src/components/room/WatchScreen.tsx; see
//! [`player`]).
//!
//! Modelled, as the web says, on the video players people build into VRChat
//! worlds: somebody pastes a link, everybody plays it from wherever it comes
//! from, and what is shared is only which video, how far in, and whether it
//! is playing. One participant runs the show -- whoever started it, who must
//! be able to read links, which the deployment does on somebody's behalf --
//! and says where it is: on every change, every two seconds while it plays,
//! every few seconds while it is still, when somebody arrives, and when
//! somebody says hello. Everybody may drive it: a link, play, pause, a seek, a
//! skip, all sent to the one running it and applied there, so there is one
//! timeline and not one per person.
//!
//! There is one timeline and everybody follows it, the holder included. The
//! web's first version told the room where the holder's own player was, and
//! the two disagreed by however long the video took to load at the holder; so
//! a video now starts when the holder's player can play it, or
//! [`STARTING_MS`] after it was chosen if it never can, and from then on the
//! timeline is the only clock.
//!
//! Whoever adds a video reads its link themselves where they can, and hands
//! the show only the ticket it was filed under; the holder reads a link only
//! for somebody who cannot, and says to the server that it reads for somebody
//! else. A film from a media server is relayed only for the person given that
//! who chose it, so a link read at the holder on a guest's behalf must not be
//! taken for the holder's choice; and a relayed film's ticket is queued from
//! somebody else only where their identity carries the signature the server
//! names as the film's grantor, or a ticket copied out of one room would be
//! relayed to another on the first person's grant.
//!
//! Not offered at all in a sealed call. What everybody is watching is data,
//! which the web's SDK options do not encrypt, and it is not something the
//! relay is to be told in a call whose promise is that it hears nothing.
//!
//! # How the apps drive it
//!
//! [`WatchTheatre`] is one room's show, viewer and holder in one, as the web
//! keeps both per room. It reads no clock and sends nothing itself: every
//! call takes the time (`now_ms`, any monotonic millisecond clock, the same
//! one for every call) and returns a [`WatchOutput`] whose packets the media
//! crate publishes as they are, whose `asks` the app answers with
//! [`WatchTheatre::answer`], and whose `refusals` the app says in the
//! person's own language. Messages arriving on the `watch` topic go to
//! [`WatchTheatre::on_message`] with the sender's identity and name; only
//! messages from a participant, never from the server. `tick` is called at
//! least once a second, and `state` whenever the screen is drawn.
//!
//! What the web does with `addVideo`, the app does in three steps: somebody
//! who cannot read links calls `add_link`; somebody who can, with no show
//! running anywhere, calls `start` and then `add_link`, and the holder reads
//! it; somebody who can, in a show somebody else runs, reads the link
//! themselves and calls `add_ticket` with what came back, passing the link
//! beside the ticket except for a media server's film. `add_ticket` starts a
//! show here if the one running ended while the link was being read.

use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
};

pub(crate) mod js;
pub mod player;
pub mod show;

pub use player::{
    watch_correction, watch_expected, WatchCorrection, WatchKeepStep, WatchKeeper, WatchPlayerReading,
};
pub use show::{is_ticket, read_show, WatchPacket, WatchShow, WatchVideo, REFUSALS};

use show::Say;

/// The data topic (watch.ts, `TOPIC`).
pub const TOPIC: &str = "watch";

/// How often the one running the show says where it is, while it plays.
pub const TELL_EVERY: u64 = 2000;

/// And while it is still: paused, waiting, or between videos. Never longer
/// than a third of [`STALE_MS`], or a show paused for a conversation would be
/// taken for one that had ended -- it was, for every viewer, twenty seconds
/// into a pause. Counted in the web's two-second steps, so a still show is in
/// fact said every six seconds.
pub const TELL_STILL: u64 = 5000;

/// How long a new video waits for the holder's player before its timeline
/// runs anyway.
pub const STARTING_MS: u64 = 8000;

/// How far past its end, in seconds, the timeline may run before the video is
/// taken to be over without the holder's player saying so: a holder who cannot
/// play it would otherwise leave everybody at its last frame, with the queue
/// waiting behind.
pub const OVERRUN_S: f64 = 5.0;

/// How long a show that has said nothing is believed to still exist.
pub const STALE_MS: u64 = 20_000;

/// The longest link a show reads, in UTF-16 units, as the web's field allows.
pub const LINK_LENGTH: usize = 2000;

/// Something the app has to find out before the show can go on, and answer
/// with [`WatchTheatre::answer`] under the same `request`.
#[derive(Debug, Clone, PartialEq, uniffi::Enum)]
pub enum WatchAsk {
    /// What a link plays (watch-api.ts, `resolveVideo`). `for_other` where
    /// somebody else pasted it and this holder reads it on their behalf,
    /// which the request to the server has to say.
    Resolve { request: u64, text: String, for_other: bool },
    /// What a ticket somebody else read plays, as the server says now
    /// (watch-api.ts, `readTicket`), with the grantor it names.
    Read { request: u64, ticket: String },
}

/// The app's answer to a [`WatchAsk`].
#[derive(Debug, Clone, PartialEq, uniffi::Enum)]
pub enum WatchAnswer {
    /// It plays. `grantor` is the signature in the server's X-Watch-Grantor,
    /// for a film relayed under somebody's grant, and nothing otherwise.
    Found { ticket: String, duration: f64, live: bool, grantor: Option<String> },
    /// It does not, and why: one of [`REFUSALS`].
    Refused { why: String },
}

/// What a call into the theatre came to.
#[derive(Debug, Clone, Default, PartialEq, uniffi::Record)]
pub struct WatchOutput {
    /// To publish, in this order.
    pub packets: Vec<WatchPacket>,
    /// To find out and answer.
    pub asks: Vec<WatchAsk>,
    /// Videos this person tried to add that were not added, as refusal codes
    /// for the app to say.
    pub refusals: Vec<String>,
    /// What [`WatchTheatre::state`] returns is different now.
    pub changed: bool,
    /// For an intent: it was taken, here or by being sent to whoever runs the
    /// show. False where there is no show to take it, or the call is sealed.
    pub accepted: bool,
}

/// The show as this participant knows it, for drawing.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct WatchSnapshot {
    /// Its own show, or one heard and still believed; none otherwise.
    pub show: Option<WatchShow>,
    /// When `show` was heard, or for one's own, when this version of it was
    /// first read: what [`watch_expected`] reckons from.
    pub heard_at_ms: u64,
    /// The show runs here.
    pub mine: bool,
    /// Watching together is offered in this call at all; false in a sealed
    /// one, where the panel says so instead.
    pub offered: bool,
    /// Where the timeline is at the time `state` was asked.
    pub expected: f64,
}

/// One room's show, as viewer and as holder (watch.ts: `Theatre`, and the
/// per-room `startTheatre`, `closeTheatre`, `command`, `seenShow` and
/// `hearShows`).
#[derive(Debug, uniffi::Object)]
pub struct WatchTheatre {
    inner: Mutex<Inner>,
}

#[uniffi::export]
impl WatchTheatre {
    /// This participant's identity and display name, and whether the call is
    /// sealed (encrypted), in which case nothing is ever said or heard.
    #[uniffi::constructor]
    pub fn new(identity: String, name: String, sealed: bool) -> Arc<Self> {
        Arc::new(Self {
            inner: Mutex::new(Inner {
                identity,
                name,
                sealed,
                present: HashSet::new(),
                heard: None,
                holder: None,
                own: None,
                requests: 1,
                out: WatchOutput::default(),
                visible: false,
            }),
        })
    }

    /// The room is joined: asks once whoever runs a show to say where it is,
    /// so somebody arriving partway through a film is told at once rather than
    /// at the next word (watch.ts, `hearShows`).
    pub fn joined(&self, now_ms: u64) -> WatchOutput {
        self.with(|inner| {
            let _ = now_ms;
            if !inner.sealed {
                inner.out.packets.push(Say::Hello.to(Vec::new()));
            }
        })
    }

    /// A message on the `watch` topic from the participant `sender`, whose
    /// display name is `sender_name`.
    pub fn on_message(&self, sender: String, sender_name: String, payload: Vec<u8>, now_ms: u64) -> WatchOutput {
        self.with(|inner| inner.on_message(sender, sender_name, &payload, now_ms))
    }

    /// Somebody arrived: the holder tells them where the show is.
    pub fn participant_connected(&self, identity: String, now_ms: u64) -> WatchOutput {
        self.with(|inner| {
            inner.present.insert(identity);
            if let Some(holder) = inner.holder.as_mut() {
                holder.announce(now_ms);
            }
            inner.notice_visibility(now_ms);
        })
    }

    /// Somebody left. A show they ran is no longer believed.
    pub fn participant_disconnected(&self, identity: String, now_ms: u64) -> WatchOutput {
        self.with(|inner| {
            inner.present.remove(&identity);
            if inner.heard.as_ref().is_some_and(|seen| seen.show.holder == identity) {
                inner.heard = None;
                inner.out.changed = true;
            }
            inner.notice_visibility(now_ms);
        })
    }

    /// Everybody else in the room, after a reconnect: the web checks the
    /// holder against the roster on every read, because a reconnect replaces
    /// the roster without saying who left.
    pub fn roster(&self, identities: Vec<String>, now_ms: u64) -> WatchOutput {
        self.with(|inner| {
            inner.present = identities.into_iter().collect();
            inner.notice_visibility(now_ms);
        })
    }

    /// Time passing: the holder's word every two seconds, a video run past
    /// its end, a show heard of that has gone quiet.
    pub fn tick(&self, now_ms: u64) -> WatchOutput {
        self.with(|inner| {
            if let Some(holder) = inner.holder.as_mut() {
                holder.tick(now_ms);
            }
            inner.notice_visibility(now_ms);
        })
    }

    /// The show as this participant knows it now.
    ///
    /// Reading the holder's own show can end its wait for the holder's player
    /// ([`STARTING_MS`]), as reading it does in the web; what that tells the
    /// room goes out with the next call that returns an output.
    pub fn state(&self, now_ms: u64) -> WatchSnapshot {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let seen = inner.seen(now_ms);
        WatchSnapshot {
            expected: seen.as_ref().map_or(0.0, |seen| player::expected(&seen.show, seen.at, now_ms)),
            heard_at_ms: seen.as_ref().map_or(0, |seen| seen.at),
            show: seen.map(|seen| seen.show),
            mine: inner.holder.is_some(),
            offered: !inner.sealed,
        }
    }

    /// Start running a show here. Refused while another is heard, and in a
    /// sealed call (watch.ts, `startTheatre`); accepted, doing nothing, where
    /// one runs here already.
    pub fn start(&self, now_ms: u64) -> WatchOutput {
        self.with(|inner| {
            inner.out.accepted = inner.start(now_ms);
        })
    }

    /// Stop running the show here, saying so last of all (`closeTheatre`).
    pub fn close(&self, now_ms: u64) -> WatchOutput {
        self.with(|inner| inner.close(now_ms))
    }

    /// Add what a link plays: read by the holder, here if the show runs here
    /// (as this person's own choice), otherwise by whoever runs it (as
    /// somebody else's).
    pub fn add_link(&self, text: String, now_ms: u64) -> WatchOutput {
        self.with(|inner| {
            let text = js::cut(&text, LINK_LENGTH);
            let by = inner.by();
            inner.out.accepted = inner.command(now_ms, |holder, requests| holder.add(&text, by, None, requests), Say::AddText(&text));
        })
    }

    /// Add a video this person already read, by its ticket. `link` is the
    /// link as pasted, sent beside the ticket for a holder whose page is older
    /// than tickets -- such a holder read only the text, and handed a ticket
    /// alone said every link from a newer page was not a video -- and left
    /// out for a media server's film, which an older holder would read with
    /// its own session and so as its own choice (WatchTab.tsx, `addVideo`).
    ///
    /// Where the show ended while the link was being read, what was read is
    /// still this person's to play, so it starts the next show here.
    pub fn add_ticket(&self, ticket: String, link: Option<String>, now_ms: u64) -> WatchOutput {
        self.with(|inner| {
            let link = link.map(|text| js::cut(&text, LINK_LENGTH));
            let by = inner.by();
            let taken = inner.command(
                now_ms,
                |holder, requests| holder.add_ticket(&ticket, by.clone(), None, requests),
                Say::AddTicket(&ticket, link.as_deref()),
            );
            inner.out.accepted = taken || {
                let started = !inner.sealed && inner.start(now_ms);
                if started {
                    let mut requests = inner.requests;
                    if let Some(holder) = inner.holder.as_mut() {
                        holder.add_ticket(&ticket, by, None, &mut requests);
                    }
                    inner.requests = requests;
                }
                started
            };
        })
    }

    /// The app's answer to an ask.
    pub fn answer(&self, request: u64, answer: WatchAnswer, now_ms: u64) -> WatchOutput {
        self.with(|inner| {
            if let Some(holder) = inner.holder.as_mut() {
                holder.answer(request, answer, now_ms);
            }
        })
    }

    /// Play, for everybody.
    pub fn play(&self, now_ms: u64) -> WatchOutput {
        self.with(|inner| inner.out.accepted = inner.command(now_ms, |holder, _| holder.play(now_ms), Say::Play))
    }

    /// Pause, for everybody.
    pub fn pause(&self, now_ms: u64) -> WatchOutput {
        self.with(|inner| inner.out.accepted = inner.command(now_ms, |holder, _| holder.pause(now_ms), Say::Pause))
    }

    /// Move everybody to `to` seconds. The app moves its own player at once
    /// too, and tells the keeper ([`WatchKeeper::seeked`]).
    pub fn seek(&self, to: f64, now_ms: u64) -> WatchOutput {
        self.with(|inner| {
            // A position that is not a number is nothing a person chose; sent,
            // a browser holder would read it as nought and go to the start.
            if !to.is_finite() {
                return;
            }
            inner.out.accepted = inner.command(now_ms, |holder, _| holder.seek(to, now_ms), Say::Seek(to));
        })
    }

    /// Past the video named, and only that one: two presses do not skip two.
    /// `None` skips whatever is playing.
    pub fn skip(&self, key: Option<String>, now_ms: u64) -> WatchOutput {
        self.with(|inner| {
            inner.out.accepted =
                inner.command(now_ms, |holder, _| holder.skip(key.as_deref(), now_ms), Say::Skip(key.as_deref()));
        })
    }

    /// Take a video out of the queue.
    pub fn remove(&self, key: String, now_ms: u64) -> WatchOutput {
        self.with(|inner| {
            inner.out.accepted = inner.command(now_ms, |holder, _| holder.remove(&key, now_ms), Say::Remove(&key));
        })
    }

    /// The holder's own player can play the video `key`: its timeline starts
    /// now, for everybody. Nothing anywhere but at the holder.
    pub fn player_ready(&self, key: String, now_ms: u64) -> WatchOutput {
        self.with(|inner| {
            if let Some(holder) = inner.holder.as_mut() {
                holder.ready(&key, now_ms);
            }
        })
    }

    /// The holder's own player reached the end of the video `key`.
    pub fn player_ended(&self, key: String, now_ms: u64) -> WatchOutput {
        self.with(|inner| {
            if let Some(holder) = inner.holder.as_mut() {
                holder.ended(&key, now_ms);
            }
        })
    }
}

impl WatchTheatre {
    fn with(&self, work: impl FnOnce(&mut Inner)) -> WatchOutput {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        work(&mut inner);
        inner.drain()
    }
}

/// A show as this participant knows it, and when it was told.
#[derive(Debug, Clone)]
struct Seen {
    show: WatchShow,
    at: u64,
}

#[derive(Debug)]
struct Inner {
    identity: String,
    name: String,
    sealed: bool,
    /// Everybody else in the room, as far as the app has said.
    present: HashSet<String>,
    /// The show last heard from somebody else (watch.ts, `heard`).
    heard: Option<Seen>,
    /// The show, where it runs here.
    holder: Option<Holder>,
    /// The holder's own view of its show, kept until the show changes
    /// (watch.ts, `own`): a fresh one on every read is a picture that never
    /// settles, since the position moves while it plays.
    own: Option<Seen>,
    /// The next ask's number, kept across shows so a late answer to a show
    /// that closed is never taken for one asked by the next.
    requests: u64,
    out: WatchOutput,
    /// Whether a show was visible when last looked, so a show going stale or
    /// its holder vanishing from the roster is said as a change.
    visible: bool,
}

impl Inner {
    fn by(&self) -> String {
        if self.name.is_empty() { self.identity.clone() } else { self.name.clone() }
    }

    fn drain(&mut self) -> WatchOutput {
        if let Some(holder) = self.holder.as_mut() {
            let said = std::mem::take(&mut holder.out);
            self.out.packets.extend(said.packets);
            self.out.asks.extend(said.asks);
            self.out.refusals.extend(said.refusals);
            self.out.changed |= said.changed;
        }
        std::mem::take(&mut self.out)
    }

    /// A show heard from somebody else, while it is still believed: said
    /// something within [`STALE_MS`], and its holder still in the room.
    fn heard_now(&self, now: u64) -> Option<&Seen> {
        let last = self.heard.as_ref()?;
        if now.saturating_sub(last.at) > STALE_MS || !self.present.contains(&last.show.holder) {
            return None;
        }
        Some(last)
    }

    fn seen(&mut self, now: u64) -> Option<Seen> {
        if let Some(holder) = self.holder.as_mut() {
            let show = holder.state(now);
            if let Some(kept) = self.own.as_ref().filter(|kept| kept.show.version == show.version) {
                return Some(kept.clone());
            }
            let seen = Seen { show, at: now };
            self.own = Some(seen.clone());
            return Some(seen);
        }
        self.heard_now(now).cloned()
    }

    fn notice_visibility(&mut self, now: u64) {
        let visible = self.holder.is_some() || self.heard_now(now).is_some();
        if visible != self.visible {
            self.out.changed = true;
        }
        self.visible = visible;
    }

    fn start(&mut self, now: u64) -> bool {
        if self.holder.is_some() {
            return true;
        }
        if self.sealed || self.seen(now).is_some() {
            return false;
        }
        let mut holder = Holder::new(self.identity.clone(), self.name.clone(), now);
        holder.announce(now);
        self.holder = Some(holder);
        self.own = None;
        self.visible = true;
        self.out.changed = true;
        true
    }

    fn close(&mut self, now: u64) {
        let Some(mut holder) = self.holder.take() else { return };
        holder.close(now);
        let said = std::mem::take(&mut holder.out);
        self.out.packets.extend(said.packets);
        self.own = None;
        self.out.changed = true;
        self.notice_visibility(now);
    }

    /// Ask the show to do something: here if it runs here, otherwise of
    /// whoever runs it, and of nobody else (watch.ts, `command`).
    fn command(&mut self, now: u64, here: impl FnOnce(&mut Holder, &mut u64), there: Say<'_>) -> bool {
        if self.sealed {
            return false;
        }
        if let Some(holder) = self.holder.as_mut() {
            here(holder, &mut self.requests);
            return true;
        }
        let Some(seen) = self.heard_now(now) else { return false };
        let packet = there.to(vec![seen.show.holder.clone()]);
        self.out.packets.push(packet);
        true
    }

    fn on_message(&mut self, sender: String, sender_name: String, payload: &[u8], now: u64) {
        // In a sealed call nothing is heard, as nothing is said.
        if self.sealed {
            return;
        }
        // Data arrives only from somebody in the room.
        self.present.insert(sender.clone());
        let Some(message) = js::decode(payload) else { return };

        if let Some(holder) = self.holder.as_mut() {
            let by = if sender_name.is_empty() { sender.clone() } else { sender_name };
            holder.on_message(&message, sender, by, &mut self.requests, now);
            return;
        }

        match js::kind(&message) {
            "state" => {
                let Some(mut show) = message.get("show").and_then(read_show) else { return };
                // Believed only from whoever runs it: anybody may say anything.
                if show.holder != sender {
                    return;
                }
                // The same holder's older word, arriving after a newer one, is
                // not news.
                if let Some(last) = &self.heard {
                    if last.show.holder == sender && show.version < last.show.version {
                        return;
                    }
                }
                if !sender_name.is_empty() {
                    show.holder_name = sender_name;
                }
                self.heard = Some(Seen { show, at: now });
                self.out.changed = true;
                self.visible = true;
            }
            "refused" if self.heard.as_ref().is_some_and(|seen| seen.show.holder == sender) => {
                let why = message.get("why").and_then(|why| why.as_str()).unwrap_or_default();
                self.out.refusals.push(show::refusal(why).to_string());
            }
            "closed" if self.heard.as_ref().is_some_and(|seen| seen.show.holder == sender) => {
                self.heard = None;
                self.out.changed = true;
                self.notice_visibility(now);
            }
            _ => {}
        }
    }
}

/// What a lookup was for: who asked, and from where.
#[derive(Debug)]
struct Pending {
    /// A link to read, rather than a ticket somebody else read.
    link: bool,
    by: String,
    /// The participant who sent it, where it came from elsewhere; told if it
    /// cannot be added, and held to the grant of a relayed film.
    from: Option<String>,
}

/// The show, as run here (watch.ts, `Theatre`).
#[derive(Debug)]
struct Holder {
    identity: String,
    name: String,
    now: Option<WatchVideo>,
    queue: Vec<WatchVideo>,
    playing: bool,
    position: f64,
    at: u64,
    version: u64,
    /// A video chosen and not yet playable at the holder, and since when: its
    /// timeline waits.
    starting: Option<(String, u64)>,
    closed: bool,
    /// The web's two-second timer: when it next fires, and how long the show
    /// has been still since it last spoke on it.
    next_fire: u64,
    still: u64,
    keys: u64,
    pending: HashMap<u64, Pending>,
    out: WatchOutput,
}

impl Holder {
    fn new(identity: String, name: String, now: u64) -> Self {
        Self {
            identity,
            name,
            now: None,
            queue: Vec::new(),
            playing: false,
            position: 0.0,
            at: now,
            version: 0,
            starting: None,
            closed: false,
            next_fire: now + TELL_EVERY,
            still: 0,
            keys: 0,
            pending: HashMap::new(),
            out: WatchOutput::default(),
        }
    }

    /// Whether the timeline is moving: playing, and not waiting for the video
    /// to start.
    fn running(&self) -> bool {
        self.playing && self.starting.is_none()
    }

    fn elapsed(&self, now: u64) -> f64 {
        (now as f64 - self.at as f64) / 1000.0
    }

    /// Gives up waiting for the holder's player once [`STARTING_MS`] has
    /// passed: from when the wait ran out, not from whenever this was next
    /// asked.
    fn settle(&mut self, now: u64) {
        let Some((_, since)) = &self.starting else { return };
        if now.saturating_sub(*since) >= STARTING_MS {
            let ran = since + STARTING_MS;
            self.starting = None;
            self.at = ran;
            self.changed(now);
        }
    }

    /// Where the timeline is now.
    fn current(&mut self, now: u64) -> f64 {
        let Some(duration) = self.now.as_ref().map(|video| video.duration) else { return 0.0 };
        self.settle(now);
        let at = self.position + if self.running() { self.elapsed(now) } else { 0.0 };
        if duration > 0.0 { at.min(duration) } else { at }
    }

    /// Whether the timeline has run [`OVERRUN_S`] past the end of what is
    /// playing.
    fn overran(&mut self, now: u64) -> bool {
        let Some(video) = &self.now else { return false };
        if video.duration <= 0.0 || video.live {
            return false;
        }
        let duration = video.duration;
        self.settle(now);
        if !self.running() {
            return false;
        }
        self.position + self.elapsed(now) >= duration + OVERRUN_S
    }

    fn ready(&mut self, key: &str, now: u64) {
        if self.starting.as_ref().map(|(waiting, _)| waiting.as_str()) != Some(key) {
            return;
        }
        self.starting = None;
        self.mark(self.position, now);
        self.changed(now);
    }

    fn state(&mut self, now: u64) -> WatchShow {
        let position = self.current(now);
        WatchShow {
            holder: self.identity.clone(),
            holder_name: self.name.clone(),
            now: self.now.clone(),
            queue: self.queue.clone(),
            // Not yet, while the video is still on its way at the holder:
            // everybody waits at the start and then begins together.
            playing: self.running() && self.now.is_some(),
            position,
            version: self.version,
        }
    }

    fn tick(&mut self, now: u64) {
        if self.closed {
            return;
        }
        // Read as often as the app ticks, as the web's page reads its show on
        // every render, so the wait for the holder's player ends on time.
        self.settle(now);
        if now < self.next_fire {
            return;
        }
        // One firing however late the tick: a timer the platform held back
        // fires once in the web too, not once for every period it missed.
        while self.next_fire <= now {
            self.next_fire += TELL_EVERY;
        }
        if self.overran(now) {
            self.next(now);
            return;
        }
        self.still += TELL_EVERY;
        if self.running() || self.still >= TELL_STILL {
            self.still = 0;
            self.announce(now);
        }
    }

    /// Add what a link plays, read by the app. `from` is whoever pasted it
    /// elsewhere: told if it cannot be added, and for whom it is read as
    /// somebody else's.
    fn add(&mut self, text: &str, by: String, from: Option<String>, requests: &mut u64) {
        let request = take(requests);
        let for_other = from.is_some();
        self.pending.insert(request, Pending { link: true, by, from });
        self.out.asks.push(WatchAsk::Resolve { request, text: js::cut(text, LINK_LENGTH), for_other });
    }

    /// Add a video somebody already read, by its ticket: queued only once the
    /// server says what it plays. Asked rather than believed, because a ticket
    /// arriving from another participant is only a string: one that is
    /// expired, mistyped, or a film the server will not relay reads as
    /// nothing, and a queue holding it would play nothing for everybody until
    /// the timeline ran past it. One that is not a ticket at all is never
    /// asked about.
    fn add_ticket(&mut self, ticket: &str, by: String, from: Option<String>, requests: &mut u64) {
        if !is_ticket(ticket) {
            self.refuse("failed", from.as_deref());
            return;
        }
        let request = take(requests);
        self.pending.insert(request, Pending { link: false, by, from });
        self.out.asks.push(WatchAsk::Read { request, ticket: ticket.to_string() });
    }

    fn answer(&mut self, request: u64, answer: WatchAnswer, now: u64) {
        let Some(Pending { link, by, from }) = self.pending.remove(&request) else { return };
        if self.closed {
            return;
        }
        let (ticket, duration, live, grantor) = match answer {
            WatchAnswer::Refused { why } => {
                // "not_allowed" for a link read on somebody else's behalf is
                // about this holder's session, not the asker: a holder whose
                // watching together was taken away partway through a show.
                // Passed on, it would tell a guest something had not been
                // opened to them. For the asker the video cannot be played
                // here, which is true.
                let why = if link && from.is_some() && why == "not_allowed" { "unavailable" } else { &why };
                self.refuse(why, from.as_deref());
                return;
            }
            WatchAnswer::Found { ticket, duration, live, grantor } => (ticket, duration, live, grantor),
        };
        // What the server answered for is what the show will carry to every
        // browser, whose readShow drops a whole show over one bad ticket.
        if !is_ticket(&ticket) {
            self.refuse("failed", from.as_deref());
            return;
        }
        // A relayed film is queued from somebody else only where the
        // signature in their identity -- proven, and the same -- is the one the
        // server names as its grantor. A signature drawn from nothing is not a
        // person, and is never anybody's grant. Said back as the media servers
        // not being opened to the sender, which is what is true of them for
        // this film. A film the holder adds is not checked: nothing here can
        // stop the show's own owner.
        let grantor = grantor.filter(|grantor| !grantor.is_empty());
        if let (Some(from), Some(grantor), false) = (from.as_deref(), grantor.as_deref(), link) {
            if proven_signature(from) != Some(grantor) {
                self.refuse("emby_not_allowed", Some(from));
                return;
            }
        }
        let duration = if duration.is_finite() && duration > 0.0 { duration } else { 0.0 };
        let key = js::new_key(now, &mut self.keys);
        self.queue.push(WatchVideo { key, by, ticket, duration, live });
        if self.now.is_none() {
            self.next(now);
        } else {
            self.changed(now);
        }
    }

    /// Said back to whoever asked, where that was elsewhere; here otherwise.
    fn refuse(&mut self, why: &str, from: Option<&str>) {
        let why = show::refusal(why);
        match from {
            Some(from) => self.out.packets.push(Say::Refused(why).to(vec![from.to_string()])),
            None => self.out.refusals.push(why.to_string()),
        }
    }

    fn play(&mut self, now: u64) {
        if self.now.is_none() || self.playing {
            return;
        }
        let at = self.current(now);
        self.mark(at, now);
        self.playing = true;
        self.changed(now);
    }

    fn pause(&mut self, now: u64) {
        if self.now.is_none() || !self.playing {
            return;
        }
        let at = self.current(now);
        self.mark(at, now);
        self.playing = false;
        self.changed(now);
    }

    fn seek(&mut self, to: f64, now: u64) {
        let Some(video) = &self.now else { return };
        if !to.is_finite() {
            return;
        }
        let end = if video.duration > 0.0 { video.duration } else { f64::INFINITY };
        self.mark(to.min(end).max(0.0), now);
        self.changed(now);
    }

    /// Past what is playing; given a key, only if that is still what is
    /// playing.
    fn skip(&mut self, key: Option<&str>, now: u64) {
        if key.is_some() && self.now.as_ref().map(|video| video.key.as_str()) != key {
            return;
        }
        self.next(now);
    }

    fn remove(&mut self, key: &str, now: u64) {
        let before = self.queue.len();
        self.queue.retain(|one| one.key != key);
        if self.queue.len() != before {
            self.changed(now);
        }
    }

    fn ended(&mut self, key: &str, now: u64) {
        if self.now.as_ref().is_some_and(|video| video.key == key) {
            self.next(now);
        }
    }

    fn close(&mut self, now: u64) {
        if self.closed {
            return;
        }
        self.closed = true;
        self.pending.clear();
        self.out.packets.push(Say::Closed.to(Vec::new()));
        self.now = None;
        self.queue.clear();
        self.changed(now);
    }

    fn next(&mut self, now: u64) {
        self.now = (!self.queue.is_empty()).then(|| self.queue.remove(0));
        self.mark(0.0, now);
        self.playing = self.now.is_some();
        self.starting = self.now.as_ref().map(|video| (video.key.clone(), now));
        self.changed(now);
    }

    fn mark(&mut self, position: f64, now: u64) {
        self.position = position;
        self.at = now;
    }

    fn changed(&mut self, now: u64) {
        self.version += 1;
        self.out.changed = true;
        self.announce(now);
    }

    fn announce(&mut self, now: u64) {
        // Nothing after the word that it closed: a state sent behind it would
        // have everybody believe in the show again until it went stale.
        if self.closed {
            return;
        }
        let show = self.state(now);
        self.out.packets.push(Say::State(&show).to(Vec::new()));
    }

    fn on_message(&mut self, message: &serde_json::Value, sender: String, by: String, requests: &mut u64, now: u64) {
        if self.closed {
            return;
        }
        match js::kind(message) {
            "hello" => self.announce(now),
            "add" => {
                // Either shape, from anybody: what arrives is only what
                // somebody said it was. A ticket present in any form is the
                // ticket shape, and one that is not a string is no ticket.
                if let Some(ticket) = message.get("ticket") {
                    self.add_ticket(ticket.as_str().unwrap_or_default(), by, Some(sender), requests);
                } else {
                    self.add(&js::string(message.get("text")), by, Some(sender), requests);
                }
            }
            "play" => self.play(now),
            "pause" => self.pause(now),
            "seek" => self.seek(js::number(message.get("to")), now),
            "skip" => self.skip(message.get("key").and_then(|key| key.as_str()), now),
            "remove" => self.remove(&js::string(message.get("key")), now),
            _ => {}
        }
    }
}

fn take(requests: &mut u64) -> u64 {
    let request = *requests;
    *requests += 1;
    request
}

/// The signature an identity carries, where it was derived from a passphrase
/// (web/src/live/name.ts, `signatureOf`, `proven`): an identity is
/// `<t|g|a><ten base-32 letters>-...`, `t` and `a` proven, `g` drawn from
/// nothing. Kept here rather than borrowed from the identity module because
/// it is the one question this module asks of an identity.
fn proven_signature(identity: &str) -> Option<&str> {
    let bytes = identity.as_bytes();
    if !matches!(bytes.first(), Some(b't' | b'a')) || bytes.get(11) != Some(&b'-') {
        return None;
    }
    let trip = &bytes[1..11];
    trip.iter()
        .all(|b| b.is_ascii_lowercase() || (b'2'..=b'7').contains(b))
        .then(|| &identity[1..11])
}

#[cfg(test)]
mod tests {
    use super::proven_signature;

    #[test]
    fn reads_a_signature_only_where_it_was_earned() {
        assert_eq!(proven_signature("aeeeeefffff-1"), Some("eeeeefffff"));
        assert_eq!(proven_signature("tggggghhhhh-2"), Some("ggggghhhhh"));
        assert_eq!(proven_signature("geeeeefffff-3"), None);
        assert_eq!(proven_signature("teeeeefffff"), None);
        assert_eq!(proven_signature("teeeee1ffff-1"), None);
        assert_eq!(proven_signature("tEEEEEFFFFF-1"), None);
        assert_eq!(proven_signature(""), None);
    }
}
