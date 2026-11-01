//! Watching a video together, layered over a call (web/src/live/watch.ts,
//! WatchScreen.tsx, WatchPanel.tsx, WatchTab.tsx; the Mac's Watch/).
//!
//! The protocol is the core's: `tomoshibi_protocol::watch::WatchTheatre` is
//! one room's show, viewer and holder in one, and says what to publish, what
//! to find out and what to say; `WatchKeeper` says what to do to the player
//! four times a second. This module wires the theatre to the call as its
//! header says ("How the apps drive it"): the `watch` data topic in and
//! out, in order, the asks answered from the deployment, the refusals said
//! in the person's own language, `tick` at least once a second and the
//! keeper's step every quarter of one. Nothing of the protocol is decided
//! here, which is what keeps a Linux participant's show the same bytes as a
//! browser's.
//!
//! The parts: player.rs (the GStreamer player, its sound into the call's own
//! mixer so that the echo canceller hears it), stage.rs (the video on the
//! stage, as one more picture of the room's plan), panel.rs (the panel:
//! a link to add, the queue, the media servers), and in a debug build
//! debug.rs (a stand-in for a deployment with no watch gateway, for tests
//! that must play only local files).
//!
//! Not offered at all in a sealed call: what everybody is watching is data,
//! which the web's SDK options do not encrypt (the core's own rule; the
//! theatre is made sealed and says and hears nothing).

#[cfg(debug_assertions)]
mod debug;
mod panel;
mod player;
mod stage;

use std::{
    cell::{Cell, RefCell},
    collections::{HashMap, VecDeque},
    rc::{Rc, Weak},
    sync::Arc,
    time::Duration,
};

use tomoshibi_core::call::{Call, CallEvent, CallState};
use tomoshibi_protocol::{
    api::{Deployment, MediaServer, ResolvedLink, ResolvedVideo, TicketVideo, UnresolvedReason},
    watch::{
        watch_expected, WatchAnswer, WatchAsk, WatchKeeper, WatchOutput, WatchPacket, WatchShow,
        WatchTheatre,
    },
};

pub use panel::panel;
pub use stage::{Place, StageView};

use super::{talk::Toast, Panel, RoomState, CHANGED};
use crate::{
    app::{log, AppModel, WeakModel},
    i18n::t,
};

/// The video's picture in the room's plan (the Mac's `watchStage`): first
/// among the pictures and first among those that take the stage.
pub const STAGE_ID: &str = "watch/stage";

/// The name the video's sound is mixed under (`Media::local_sound`).
const SOUND: &str = "watch";

/// How often the keeper's step is taken (WatchScreen.tsx `KEEP_MS`); the
/// theatre's tick, which wants at least one a second, rides on it.
const BEAT: Duration = Duration::from_millis(tomoshibi_protocol::watch::player::KEEP_MS);

/// How long a refusal is said in the corner (notices.ts `AT_MOST`).
const SAID_FOR_MS: u64 = 8000;

/// The one clock every call into the theatre and the keeper is given: a
/// monotonic count of milliseconds. The wall clock is not one, since it
/// jumps when the system sets it.
pub(crate) fn clock() -> u64 {
    (glib::monotonic_time() / 1000).max(0) as u64
}

/// What a ticket plays, as this app has learned it (watch-api.ts `infos`):
/// what the server said, or one of the two ways it can say nothing. Told
/// apart because "could not be read, try again" sent somebody retrying a
/// video that no retry would play.
// Unboxed although one side is much the larger, as the core's own
// ResolvedLink is: a few of these are kept per call.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq)]
pub enum Info {
    Known(ResolvedVideo),
    Unavailable,
    Failed,
}

impl Info {
    pub fn video(&self) -> Option<&ResolvedVideo> {
        match self {
            Info::Known(video) => Some(video),
            _ => None,
        }
    }
}

/// The code a refusal crosses the call as (watch.ts `REFUSALS`).
pub fn code(reason: UnresolvedReason) -> &'static str {
    match reason {
        UnresolvedReason::NotAVideo => "not_a_video",
        UnresolvedReason::Unavailable => "unavailable",
        UnresolvedReason::MembersOnly => "members_only",
        UnresolvedReason::CopyProtected => "copy_protected",
        UnresolvedReason::RegionLocked => "region_locked",
        UnresolvedReason::NotFound => "not_found",
        UnresolvedReason::NotAllowed => "not_allowed",
        UnresolvedReason::EmbyNotAllowed => "emby_not_allowed",
        UnresolvedReason::Failed => "failed",
    }
}

/// What to say about a video that was not added (WatchTab.tsx `refusal`,
/// the Mac's WatchWords). The two refusals of a person are said as what the
/// person seeing them was not given, because by the time one reaches
/// somebody it is about them.
pub fn refusal(why: &str) -> String {
    match why {
        "not_a_video" => t("That link is not a video that can be played here."),
        "unavailable" => t("That video cannot be played here."),
        "members_only" => {
            t("That video is for paying members only, and plays only on its own site.")
        }
        "copy_protected" => t("That video is copy-protected, and plays only on its own site."),
        "region_locked" => t("That video is not available in this server's region."),
        "not_found" => t(
            "That episode could not be found: it may have been taken down, or not be offered in this server's region.",
        ),
        "not_allowed" => t("Watching together has not been opened to you."),
        "emby_not_allowed" => t("The media servers have not been opened to you."),
        _ => t("The video could not be read. Try again."),
    }
}

/// The site a link is on, as people say it: "youtube.com", not
/// "www.youtube.com" (WatchScreen.tsx `siteOf`).
pub fn site(link: &str) -> String {
    let rest = link
        .strip_prefix("https://")
        .or_else(|| link.strip_prefix("http://"))
        .or_else(|| link.strip_prefix("HTTPS://"))
        .or_else(|| link.strip_prefix("HTTP://"));
    let Some(rest) = rest else {
        return String::new();
    };
    let host = rest
        .split(['/', '?', '#'])
        .next()
        .unwrap_or_default()
        .rsplit('@')
        .next()
        .unwrap_or_default();
    let host = host.split(':').next().unwrap_or_default().to_lowercase();
    host.strip_prefix("www.").unwrap_or(&host).to_owned()
}

/// Minutes and seconds, or hours too (the Mac's `musicClock`).
pub fn clock_text(seconds: f64) -> String {
    let whole = seconds.max(0.0).floor() as u64;
    let (hours, minutes, seconds) = (whole / 3600, (whole / 60) % 60, whole % 60);
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    }
}

/// At most `count` UTF-16 units, as a JavaScript `slice(0, count)` cuts.
fn cut(text: &str, count: usize) -> String {
    let units: Vec<u16> = text.encode_utf16().take(count).collect();
    String::from_utf16_lossy(&units)
}

/// What the room keeps for watching together, in the model beside the
/// room's own state (`AppModel::kept::<WatchState>()`): the engine of the
/// call it was made for.
#[derive(Default)]
pub struct WatchState {
    engine: RefCell<Option<Rc<Engine>>>,
}

/// The engine of the call joined now, if there is one and it is not over.
pub(crate) fn engine(model: &AppModel) -> Option<Rc<Engine>> {
    model
        .kept::<WatchState>()
        .engine
        .borrow()
        .clone()
        .filter(|engine| !engine.over.get())
}

/// Every event of the call, before the room reads it: the engine is made
/// for a call the first time one of its events arrives.
pub(crate) fn follow(model: &AppModel, call: &Arc<Call>, event: &CallEvent) {
    let state = model.kept::<WatchState>();
    let current = state
        .engine
        .borrow()
        .clone()
        .filter(|engine| engine.is_for(call));
    let engine = match current {
        Some(engine) => engine,
        None => {
            if let Some(old) = state.engine.borrow_mut().take() {
                old.end();
            }
            let Some(engine) = Engine::new(model, call) else {
                return;
            };
            state.engine.replace(Some(engine.clone()));
            engine
        }
    };
    engine.hear(event);
}

/// Whether there is a show, and so a picture of it on the stage.
pub(crate) fn on_stage(model: &AppModel) -> bool {
    engine(model).is_some_and(|engine| engine.shared.borrow().show.is_some())
}

/// Whether the bar offers the button (Room.tsx): to whoever can start a
/// show, and to everybody once there is one; never in a sealed call.
pub(crate) fn offered(model: &AppModel) -> bool {
    engine(model).is_some_and(|engine| {
        !engine.sealed && {
            let shared = engine.shared.borrow();
            shared.ready || shared.show.is_some()
        }
    })
}

/// Whether a video is playing, for the dot on the bar's button.
pub(crate) fn playing(model: &AppModel) -> bool {
    engine(model).is_some_and(|engine| {
        engine
            .shared
            .borrow()
            .show
            .as_ref()
            .is_some_and(|show| show.playing && show.now.is_some())
    })
}

/// The call is about to be left: a holder closes the show and waits for
/// that word to go out.
pub(crate) async fn leaving(model: &AppModel) {
    let Some(engine) = engine(model) else { return };
    if engine.shared.borrow().mine {
        engine.handle(engine.theatre.close(clock()), true);
        engine.flush().await;
    }
    engine.end();
}

/// What a test asks of watching together (script.rs `watch=`): `add
/// <link>`, `play`, `pause`, `seek <seconds>`, `skip`, `remove <index>`,
/// `stop`, `panel`, `media`.
#[cfg(debug_assertions)]
pub(crate) fn script(model: &AppModel, value: &str) {
    let (word, rest) = value.split_once(' ').unwrap_or((value, ""));
    let Some(engine) = engine(model) else {
        log("watch script: no call to watch in");
        return;
    };
    match word {
        "add" => {
            let (engine, link) = (engine.clone(), rest.to_owned());
            glib::spawn_future_local(async move {
                let added = engine.add(&link).await;
                log(&format!("watch script: added {added}"));
            });
        }
        "play" => engine.set_playing(true),
        "pause" => engine.set_playing(false),
        "seek" => {
            if let Ok(to) = rest.parse::<f64>() {
                engine.seek(to);
            }
        }
        "skip" => {
            let key = engine
                .shared
                .borrow()
                .show
                .as_ref()
                .and_then(|show| show.now.as_ref().map(|video| video.key.clone()));
            if let Some(key) = key {
                engine.skip(&key);
            }
        }
        "remove" => {
            let key = rest.parse::<usize>().ok().and_then(|at| {
                engine
                    .shared
                    .borrow()
                    .show
                    .as_ref()
                    .and_then(|show| show.queue.get(at).map(|video| video.key.clone()))
            });
            if let Some(key) = key {
                engine.remove(&key);
            }
        }
        "stop" => engine.stop(),
        "panel" | "media" => {
            if word == "media" {
                engine.shared.borrow_mut().tab_media = true;
            }
            let open = model.kept::<RoomState>().view.borrow().panel;
            if open != Some(Panel::Watch) {
                super::toggle_panel(model, Panel::Watch);
            }
        }
        "report" => {
            let shared = engine.shared.borrow();
            log(&format!(
                "watch report: offered {}, ready {}, mine {}, show {:?}",
                !engine.sealed && (shared.ready || shared.show.is_some()),
                shared.ready,
                shared.mine,
                shared.show.as_ref().map(|show| (
                    show.holder.clone(),
                    show.now.as_ref().map(|video| video.ticket.clone()),
                    show.queue.len(),
                    show.playing,
                    show.position,
                    show.version
                )),
            ));
            if let Some(player) = engine.player.borrow().as_ref() {
                log(&format!("watch report: player {}", player.describe()));
            }
        }
        other => log(&format!("watch script: no such press {other:?}")),
    }
}

/// What the screens read of the show.
#[derive(Default)]
pub(crate) struct Shared {
    /// The show as this participant knows it, and when it was heard (or for
    /// the holder, when this version of it was first read): what the
    /// timeline is reckoned from.
    pub show: Option<WatchShow>,
    pub heard_at_ms: u64,
    /// The show runs here.
    pub mine: bool,
    /// This person may read links and start a show: signed in, given
    /// watching together, on a deployment with a gateway (`watch_ready`).
    pub ready: bool,
    /// What each ticket plays, asked of the server once and kept.
    pub infos: HashMap<String, Info>,
    /// The media servers this person may browse, asked when the panel opens.
    pub servers: Vec<MediaServer>,
    /// The panel is to open at the media servers (a test's `watch=media`).
    pub tab_media: bool,
}

/// One call's show, as viewer and as holder: the theatre wired to the call,
/// the videos' descriptions asked of the deployment, and the player for
/// whatever is on (the Mac's WatchEngine).
pub(crate) struct Engine {
    model: WeakModel,
    call: std::sync::Weak<Call>,
    theatre: Arc<WatchTheatre>,
    pub deployment: Arc<Deployment>,
    pub sealed: bool,
    pub shared: RefCell<Shared>,
    pub player: RefCell<Option<Rc<player::Player>>>,
    /// Packets waiting to be published, one at a time and in order: a state
    /// overtaken by the word that the show closed would have everybody
    /// believe in it again.
    sending: RefCell<VecDeque<WatchPacket>>,
    sending_now: Cell<bool>,
    /// Tickets being asked about now, so that each is asked once.
    asking: RefCell<std::collections::HashSet<String>>,
    reconnecting: Cell<bool>,
    /// For good: what every late answer looks at first. A video's
    /// description fetched after the call was left went on to build a
    /// player and start the film again on the Mac.
    over: Cell<bool>,
    timer: RefCell<Option<glib::SourceId>>,
    /// The keeper of the video playing now, one per video as the web keys
    /// its player by the video's key.
    keeper: RefCell<Option<(String, Arc<WatchKeeper>)>>,
    /// The ids of the refusals said in the corner, apart from the room's
    /// own notices'.
    next_notice: Cell<u64>,
    me: Weak<Engine>,
}

impl Engine {
    fn new(model: &AppModel, call: &Arc<Call>) -> Option<Rc<Engine>> {
        let entry = model.call_entry()?;
        let sealed = entry.options().sealing_word.is_some();
        let theatre = WatchTheatre::new(call.local_identity(), call.local_name(), sealed);
        let engine = Rc::new_cyclic(|me| Engine {
            model: model.downgrade(),
            call: Arc::downgrade(call),
            theatre,
            deployment: entry.deployment.clone(),
            sealed,
            shared: RefCell::new(Shared::default()),
            player: RefCell::new(None),
            sending: RefCell::new(VecDeque::new()),
            sending_now: Cell::new(false),
            asking: RefCell::new(Default::default()),
            reconnecting: Cell::new(false),
            over: Cell::new(false),
            timer: RefCell::new(None),
            keeper: RefCell::new(None),
            next_notice: Cell::new(0),
            me: me.clone(),
        });
        // Not offered at all in a sealed call: nothing is listened to, said
        // or asked, and the server is not even asked whether this person
        // could start a show.
        if sealed {
            return Some(engine);
        }
        let now = clock();
        let everybody: Vec<String> = call
            .participants()
            .into_iter()
            .map(|participant| participant.identity)
            .collect();
        engine.handle(engine.theatre.roster(everybody, now), true);
        engine.handle(engine.theatre.joined(now), true);

        let weak = engine.me.clone();
        let timer = glib::timeout_add_local(BEAT, move || {
            let Some(engine) = weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            if engine.over.get() {
                return glib::ControlFlow::Break;
            }
            engine.beat();
            glib::ControlFlow::Continue
        });
        engine.timer.replace(Some(timer));

        let weak = engine.me.clone();
        let deployment = engine.deployment.clone();
        glib::spawn_future_local(async move {
            #[cfg(debug_assertions)]
            let stand_in = debug::links();
            #[cfg(not(debug_assertions))]
            let stand_in = false;
            let ready = stand_in || deployment.watch_ready().await.unwrap_or(false);
            if let Some(engine) = weak.upgrade() {
                if !engine.over.get() {
                    engine.shared.borrow_mut().ready = ready;
                    engine.announce();
                }
            }
        });
        Some(engine)
    }

    fn is_for(&self, call: &Arc<Call>) -> bool {
        self.call
            .upgrade()
            .is_some_and(|mine| Arc::ptr_eq(&mine, call))
    }

    fn model(&self) -> Option<AppModel> {
        self.model.upgrade()
    }

    /// Tells the room that what it draws of the show changed.
    fn announce(&self) {
        if let Some(model) = self.model() {
            model.announce(CHANGED);
        }
    }

    fn hear(&self, event: &CallEvent) {
        if self.over.get() || self.sealed {
            return;
        }
        let now = clock();
        match event {
            CallEvent::ParticipantJoined { participant } => {
                self.handle(
                    self.theatre
                        .participant_connected(participant.identity.clone(), now),
                    true,
                );
            }
            CallEvent::ParticipantLeft { identity } => {
                self.handle(
                    self.theatre.participant_disconnected(identity.clone(), now),
                    true,
                );
            }
            CallEvent::StateChanged { state } => match state {
                CallState::Reconnecting => self.reconnecting.set(true),
                CallState::Connected if self.reconnecting.get() => {
                    // A reconnect replaces the roster without saying who
                    // left.
                    self.reconnecting.set(false);
                    if let Some(call) = self.call.upgrade() {
                        let everybody = call
                            .participants()
                            .into_iter()
                            .map(|participant| participant.identity)
                            .collect();
                        self.handle(self.theatre.roster(everybody, now), true);
                    }
                }
                CallState::Disconnected => self.end(),
                _ => {}
            },
            // Only from a participant: the server says nothing on this
            // topic, and anything without a sender is not a show's word.
            CallEvent::Data {
                topic,
                identity: Some(identity),
                payload,
            } if topic == tomoshibi_protocol::watch::TOPIC => {
                let name = self
                    .model()
                    .and_then(|model| {
                        let room = model.kept::<RoomState>();
                        let talk = room.talk.borrow();
                        talk.as_ref().and_then(|talk| {
                            talk.person(identity).map(|person| person.name.clone())
                        })
                    })
                    .unwrap_or_default();
                self.handle(
                    self.theatre
                        .on_message(identity.clone(), name, payload.clone(), now),
                    true,
                );
            }
            CallEvent::Ended { .. } => self.end(),
            _ => {}
        }
    }

    /// The call is over: the player stops at once, for good.
    fn end(&self) {
        if self.over.replace(true) {
            return;
        }
        if let Some(timer) = self.timer.take() {
            timer.remove();
        }
        if let Some(player) = self.player.take() {
            player.stop();
        }
        let _ = self.theatre.close(clock());
        self.shared.borrow_mut().show = None;
        self.announce();
    }

    fn beat(&self) {
        let now = clock();
        self.handle(self.theatre.tick(now), true);
        self.refresh(now);
        // The clock read again: the tick or the refresh may have just built
        // a player, whose source began after `now`.
        let now = clock();
        let (player, show, heard_at) = {
            let shared = self.shared.borrow();
            (
                self.player.borrow().clone(),
                shared.show.clone(),
                shared.heard_at_ms,
            )
        };
        if let (Some(player), Some(show)) = (player, show) {
            self.keep(&player, &show, heard_at, now);
        }
    }

    /// Holds the player to the show: the keeper's step, applied in the order
    /// of its fields.
    fn keep(&self, player: &player::Player, show: &WatchShow, heard_at: u64, now: u64) {
        let keeper = {
            let mut kept = self.keeper.borrow_mut();
            match kept.as_ref() {
                Some((key, keeper)) if *key == player.key => keeper.clone(),
                _ => {
                    let keeper = WatchKeeper::new();
                    *kept = Some((player.key.clone(), keeper.clone()));
                    keeper
                }
            }
        };
        let expected = watch_expected(show.clone(), heard_at, now);
        let live = player.video.live;
        let reading = player.reading();
        let step = keeper.keep(reading, show.playing, expected, live, now);
        player.apply(&step);
        #[cfg(debug_assertions)]
        debug::log_step(&reading, show.playing, expected);
    }

    /// Reads the show again and follows it: a new video gets a player of its
    /// own, and a show that ended takes its player with it.
    fn refresh(&self, now: u64) {
        if self.over.get() {
            return;
        }
        let snapshot = self.theatre.state(now);
        let before = self.outline();
        {
            let mut shared = self.shared.borrow_mut();
            shared.show = snapshot.show;
            shared.heard_at_ms = snapshot.heard_at_ms;
            shared.mine = snapshot.mine;
        }
        // A video that stopped being playable here takes its player with it
        // too, and the stage says so in its place.
        let (video, playable) = {
            let shared = self.shared.borrow();
            let video = shared.show.as_ref().and_then(|show| show.now.clone());
            let playable = video.as_ref().and_then(|video| {
                shared
                    .infos
                    .get(&video.ticket)
                    .and_then(Info::video)
                    .cloned()
            });
            (video, playable)
        };
        let current = self.player.borrow().clone();
        if let Some(current) = current {
            if Some(&current.key) != video.as_ref().map(|video| &video.key) || playable.is_none() {
                current.stop();
                self.player.replace(None);
            }
        }
        if let Some(video) = video {
            if self.player.borrow().is_none() {
                match playable {
                    Some(resolved) => {
                        let made = player::Player::new(self.me.clone(), video, resolved);
                        self.player.replace(Some(made));
                    }
                    None => {
                        // Asked once; the answer reads the show again.
                        self.info(&video.ticket);
                    }
                }
            }
        }
        if self.outline() != before {
            self.announce();
        }
    }

    /// What the room draws of the show, beyond where it is: when this
    /// changes the room is told, and not every quarter second.
    fn outline(&self) -> String {
        let shared = self.shared.borrow();
        let show = shared.show.as_ref();
        format!(
            "{} {} {} {:?} {:?} {} {} {} {}",
            shared.ready,
            shared.mine,
            show.is_some(),
            show.and_then(|show| show.now.as_ref().map(|video| video.key.clone())),
            show.map(|show| show
                .queue
                .iter()
                .map(|video| video.key.clone())
                .collect::<Vec<_>>()),
            show.is_some_and(|show| show.playing),
            shared.infos.len(),
            shared.servers.len(),
            self.player
                .borrow()
                .as_ref()
                .map_or(0, |player| player.generation()),
        )
    }

    /// What the theatre said: the packets published in order, the refusals
    /// said, the asks answered.
    fn handle(&self, out: WatchOutput, answering: bool) {
        // Nothing the theatre says once the call is over is said: its words
        // would go into a call already left, and its asks start the fetches
        // that brought the show back.
        if self.over.get() {
            return;
        }
        for packet in out.packets {
            self.send(packet);
        }
        for why in &out.refusals {
            self.say(&refusal(why));
        }
        if answering {
            for ask in out.asks {
                let weak = self.me.clone();
                glib::spawn_future_local(async move {
                    let Some(engine) = weak.upgrade() else { return };
                    let answered = engine.answer(ask).await;
                    engine.handle(answered, true);
                });
            }
        }
        if out.changed {
            self.refresh(clock());
        }
    }

    fn send(&self, packet: WatchPacket) {
        self.sending.borrow_mut().push_back(packet);
        if self.sending_now.replace(true) {
            return;
        }
        let weak = self.me.clone();
        glib::spawn_future_local(async move {
            loop {
                let Some(engine) = weak.upgrade() else { return };
                let next = engine.sending.borrow_mut().pop_front();
                let Some(packet) = next else {
                    engine.sending_now.set(false);
                    return;
                };
                let Some(call) = engine.call.upgrade() else {
                    engine.sending.borrow_mut().clear();
                    engine.sending_now.set(false);
                    return;
                };
                drop(engine);
                if let Err(fault) = call
                    .send_data(
                        packet.topic,
                        packet.payload,
                        packet.destinations,
                        packet.reliable,
                    )
                    .await
                {
                    log(&format!("a watch message was not sent: {fault}"));
                }
            }
        });
    }

    /// Waits until everything said so far has gone out.
    async fn flush(&self) {
        for _ in 0..40 {
            if !self.sending_now.get() && self.sending.borrow().is_empty() {
                return;
            }
            glib::timeout_future(Duration::from_millis(50)).await;
        }
    }

    /// Said in the call's corner, as the web says a refusal (Room.tsx
    /// `hearShows`, `actionFailed`), wherever the panel is.
    fn say(&self, text: &str) {
        let Some(model) = self.model() else { return };
        let room = model.kept::<RoomState>();
        // Ids of their own, above any the room counts to, so that the
        // room's notices and these never take each other's place.
        let id = (1u64 << 62) + self.next_notice.get();
        self.next_notice.set(self.next_notice.get() + 1);
        {
            let mut view = room.view.borrow_mut();
            view.toasts
                .retain(|(_, kept)| kept.key.as_deref() != Some("watch"));
            view.toasts.push((
                id,
                Toast {
                    key: Some("watch".into()),
                    closable: true,
                    lasts_ms: Some(SAID_FOR_MS),
                    ..Toast::failure(text.to_owned())
                },
            ));
            let over = view.toasts.len().saturating_sub(4);
            view.toasts.drain(..over);
        }
        let weak = model.downgrade();
        glib::timeout_add_local_once(Duration::from_millis(SAID_FOR_MS), move || {
            if let Some(model) = weak.upgrade() {
                super::dismiss(&model, id);
            }
        });
        model.announce(CHANGED);
    }

    /// Finds out what the theatre asked and gives it the answer.
    async fn answer(&self, ask: WatchAsk) -> WatchOutput {
        match ask {
            WatchAsk::Resolve {
                request,
                text,
                for_other,
            } => {
                let answer = match self.resolve(&text, for_other).await {
                    Ok(ResolvedLink::Playable { video }) => {
                        let found = WatchAnswer::Found {
                            ticket: video.ticket.clone(),
                            duration: video.duration,
                            live: video.live,
                            grantor: video.grantor.clone(),
                        };
                        self.remember(video);
                        found
                    }
                    Ok(ResolvedLink::NotPlayable { reason }) => WatchAnswer::Refused {
                        why: code(reason).to_owned(),
                    },
                    // What the web's resolveVideo makes of anything thrown.
                    Err(_) => WatchAnswer::Refused {
                        why: "failed".to_owned(),
                    },
                };
                self.theatre.answer(request, answer, clock())
            }
            WatchAsk::Read { request, ticket } => {
                let answer = match self.read_info(&ticket).await {
                    Info::Known(video) => WatchAnswer::Found {
                        ticket: video.ticket,
                        duration: video.duration,
                        live: video.live,
                        grantor: video.grantor,
                    },
                    Info::Unavailable => WatchAnswer::Refused {
                        why: "unavailable".to_owned(),
                    },
                    Info::Failed => WatchAnswer::Refused {
                        why: "failed".to_owned(),
                    },
                };
                self.theatre.answer(request, answer, clock())
            }
        }
    }

    /// A link read through the deployment's gateway, or the stand-in a test
    /// gives this app where the deployment has none.
    async fn resolve(
        &self,
        text: &str,
        for_other: bool,
    ) -> Result<ResolvedLink, tomoshibi_protocol::ProtocolError> {
        #[cfg(debug_assertions)]
        if debug::links() {
            return Ok(debug::resolve(text));
        }
        self.deployment
            .clone()
            .resolve_video(text.to_owned(), for_other)
            .await
    }

    async fn fetch_info(&self, ticket: &str) -> Info {
        #[cfg(debug_assertions)]
        if let Some(known) = debug::info(ticket) {
            return known;
        }
        match self.deployment.clone().video_info(ticket.to_owned()).await {
            Ok(TicketVideo::Known { video }) => Info::Known(video),
            Ok(TicketVideo::Unavailable) => Info::Unavailable,
            _ => Info::Failed,
        }
    }

    /// What a ticket plays, if this app knows yet; asks the server once if
    /// not, and the answer reads the show again.
    pub fn info(&self, ticket: &str) -> Option<Info> {
        if let Some(known) = self.shared.borrow().infos.get(ticket) {
            return Some(known.clone());
        }
        if self.asking.borrow_mut().insert(ticket.to_owned()) {
            let (weak, ticket) = (self.me.clone(), ticket.to_owned());
            glib::spawn_future_local(async move {
                let Some(engine) = weak.upgrade() else { return };
                let found = engine.fetch_info(&ticket).await;
                engine.asking.borrow_mut().remove(&ticket);
                engine.shared.borrow_mut().infos.insert(ticket, found);
                engine.refresh(clock());
                engine.announce();
            });
        }
        None
    }

    /// The same question, waited for (watch-api.ts `readTicket`).
    async fn read_info(&self, ticket: &str) -> Info {
        if let Some(known) = self.shared.borrow().infos.get(ticket) {
            return known.clone();
        }
        let found = self.fetch_info(ticket).await;
        self.shared
            .borrow_mut()
            .infos
            .insert(ticket.to_owned(), found.clone());
        found
    }

    /// What this person read themselves, so it is not asked for again.
    fn remember(&self, video: ResolvedVideo) {
        self.shared
            .borrow_mut()
            .infos
            .insert(video.ticket.clone(), Info::Known(video));
    }

    // ---- what the person does ------------------------------------------

    /// Adds what a link plays to the show; true if it was added, or sent to
    /// be (WatchTab.tsx `addVideo`, the Mac's `add`).
    ///
    /// Read here by anybody who may read links, and the show is handed only
    /// the ticket; started here with it when nobody runs one. Somebody who
    /// may not hands the link itself to whoever runs the show, who reads it
    /// on their behalf. Read here rather than by the holder because of whose
    /// choice a video is: a film from a media server is relayed only for the
    /// person given that who chose it.
    pub async fn add(&self, pasted: &str) -> bool {
        let text = pasted.trim().to_owned();
        if text.is_empty() || self.sealed || self.over.get() {
            return false;
        }
        let now = clock();
        let ready = self.shared.borrow().ready;
        if !ready {
            let out = self.theatre.add_link(text, now);
            let accepted = out.accepted;
            self.handle(out, true);
            return accepted;
        }
        self.refresh(now);
        let (show, mine) = {
            let shared = self.shared.borrow();
            (shared.show.is_some(), shared.mine)
        };
        if !show {
            let started = self.theatre.start(now);
            let accepted = started.accepted;
            self.handle(started, true);
            if !accepted {
                return false;
            }
            return self.own(self.theatre.add_link(text, clock())).await;
        }
        if mine {
            return self.own(self.theatre.add_link(text, now)).await;
        }
        let found = match self.resolve(&cut(&text, 2000), false).await {
            Ok(found) => found,
            Err(_) => {
                self.say(&refusal("failed"));
                return false;
            }
        };
        // Left, or ended, while the link was being read: nothing more.
        if self.over.get() {
            return false;
        }
        match found {
            ResolvedLink::NotPlayable { reason } => {
                self.say(&refusal(code(reason)));
                false
            }
            ResolvedLink::Playable { video } => {
                // The link beside its ticket, for a holder whose page
                // predates tickets; never for a media server's film, which
                // such a holder would read with its own session and so as
                // its own choice.
                let link = (video.play.kind != tomoshibi_protocol::api::ResolvedPlayKind::Relay)
                    .then_some(text);
                let ticket = video.ticket.clone();
                self.remember(video);
                self.own(self.theatre.add_ticket(ticket, link, clock()))
                    .await
            }
        }
    }

    /// An output whose asks are this person's own adding: answered before
    /// saying whether it worked, so the field empties only for a video that
    /// went in.
    async fn own(&self, out: WatchOutput) -> bool {
        let asks = out.asks.clone();
        let mut added = out.accepted && out.refusals.is_empty();
        self.handle(out, false);
        for ask in asks {
            let answered = self.answer(ask).await;
            if self.over.get() {
                return false;
            }
            if !answered.refusals.is_empty() {
                added = false;
            }
            self.handle(answered, true);
        }
        added && !self.over.get()
    }

    /// Play or pause for everybody, felt here at once: the player is told
    /// too, so the press does not wait for the show's answer.
    pub fn toggle(&self) {
        let playing = self.shared.borrow().show.as_ref().map(|show| show.playing);
        let Some(playing) = playing else { return };
        let player = self.player.borrow().clone();
        if playing {
            if let Some(player) = player {
                player.pause_now();
            }
            self.handle(self.theatre.pause(clock()), true);
        } else {
            if let Some(player) = player {
                player.play_now();
            }
            self.handle(self.theatre.play(clock()), true);
        }
    }

    pub fn set_playing(&self, playing: bool) {
        let now = self.shared.borrow().show.as_ref().map(|show| show.playing);
        if now.is_some_and(|now| now != playing) {
            self.toggle();
        }
    }

    /// Moves everybody to `to`, and this player at once.
    pub fn seek(&self, to: f64) {
        let player = self.player.borrow().clone();
        if let Some(player) = player {
            player.seek_now(to);
            if let Some((key, keeper)) = self.keeper.borrow().as_ref() {
                if *key == player.key {
                    keeper.seeked(clock());
                }
            }
        }
        self.handle(self.theatre.seek(to, clock()), true);
    }

    /// Past the video named, and only that one: two presses do not skip two.
    pub fn skip(&self, key: &str) {
        self.handle(self.theatre.skip(Some(key.to_owned()), clock()), true);
    }

    pub fn remove(&self, key: &str) {
        self.handle(self.theatre.remove(key.to_owned(), clock()), true);
    }

    /// Stop running the show here, saying so last of all.
    pub fn stop(&self) {
        self.handle(self.theatre.close(clock()), true);
    }

    fn player_ready(&self, key: &str) {
        self.handle(self.theatre.player_ready(key.to_owned(), clock()), true);
    }

    fn player_ended(&self, key: &str) {
        self.handle(self.theatre.player_ended(key.to_owned(), clock()), true);
    }

    /// The media servers, asked when the panel opens and only of somebody
    /// who could start a show: browsing is for choosing, and choosing is
    /// theirs (WatchPanel.tsx). The web reads every refusal here as none.
    pub fn load_servers(&self) {
        if !self.shared.borrow().ready || self.sealed || self.over.get() {
            return;
        }
        #[cfg(debug_assertions)]
        if debug::links() {
            return;
        }
        let weak = self.me.clone();
        let deployment = self.deployment.clone();
        glib::spawn_future_local(async move {
            let found = deployment.media_servers().await.unwrap_or_default();
            if let Some(engine) = weak.upgrade() {
                if !engine.over.get() && engine.shared.borrow().servers != found {
                    engine.shared.borrow_mut().servers = found;
                    engine.announce();
                }
            }
        });
    }

    /// The account's session for the deployment's own relay, which plays a
    /// film only to the account it was chosen by.
    fn session_cookie(&self) -> Option<String> {
        self.deployment
            .account_cookie()
            .map(|value| format!("meet-live.account={value}"))
    }

    /// Whether the video's sound may be played at all here: a test run with
    /// TOMOSHIBI_MUTE plays nothing, as the call's own sound plays nothing.
    fn sound_sink(&self) -> player::Sound {
        let Some(model) = self.model() else {
            return player::Sound::Nowhere;
        };
        let room = model.kept::<RoomState>();
        let media = room.media.borrow().clone();
        if let Some(sink) = media.and_then(|media| media.local_sound(SOUND)) {
            return player::Sound::Mixer(sink);
        }
        if model.launch().mute.is_some() {
            player::Sound::Nowhere
        } else {
            player::Sound::System
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_site_is_said_as_people_say_it() {
        assert_eq!(site("https://www.youtube.com/watch?v=x"), "youtube.com");
        assert_eq!(site("http://Example.org:8080/a"), "example.org");
        assert_eq!(site("ftp://example.org"), "");
        assert_eq!(site("not a link"), "");
    }

    #[test]
    fn a_clock_reads_as_a_player_reads() {
        assert_eq!(clock_text(0.0), "0:00");
        assert_eq!(clock_text(65.4), "1:05");
        assert_eq!(clock_text(3725.0), "1:02:05");
        assert_eq!(clock_text(-3.0), "0:00");
    }

    #[test]
    fn a_link_is_cut_as_javascript_cuts_it() {
        assert_eq!(cut("abcdef", 3), "abc");
        // An emoji is two units: cut between them, the half is lost.
        assert_eq!(cut("a\u{1F600}b", 2), "a\u{FFFD}");
    }

    #[test]
    fn every_refusal_has_its_own_sentence() {
        let mut said = std::collections::HashSet::new();
        for why in tomoshibi_protocol::watch::REFUSALS {
            said.insert(refusal(why));
        }
        assert_eq!(said.len(), tomoshibi_protocol::watch::REFUSALS.len());
        assert_eq!(refusal("anything else"), refusal("failed"));
    }

    #[test]
    fn the_codes_are_the_protocol_s() {
        for reason in [
            UnresolvedReason::NotAVideo,
            UnresolvedReason::Unavailable,
            UnresolvedReason::MembersOnly,
            UnresolvedReason::CopyProtected,
            UnresolvedReason::RegionLocked,
            UnresolvedReason::NotFound,
            UnresolvedReason::NotAllowed,
            UnresolvedReason::EmbyNotAllowed,
            UnresolvedReason::Failed,
        ] {
            assert!(tomoshibi_protocol::watch::REFUSALS.contains(&code(reason)));
        }
    }
}
