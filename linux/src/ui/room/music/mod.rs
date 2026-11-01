//! The song desk, layered over a call (jukebox.ts, music.ts, sound.ts,
//! duck.ts, MusicPanel.tsx; the Mac's Music module).
//!
//! The desk itself is the protocol crate's `JukeboxDesk`: this tells it what
//! the call says (who is here, the `jukebox` topic) and the time at least
//! once a second, and does what it asks: fetch and decode a song
//! (loader.rs), publish and play it (output.rs, as `SoundKind::LibraryMusic`,
//! heard by whoever plays it through this app's own mix), pause it, stop it,
//! search the library, read a link, and answer with what came of each.
//! Everything it says to the room goes out on the topic in the order it was
//! said, exactly as the web writes it. In a sealed call the desk plays for
//! its holder alone and says nothing to the room (the core's rule: the
//! desk's data would cross the relay unsealed); the music track itself is
//! sealed like any other.
//!
//! Music anybody plays is turned down under speech here (duck.rs,
//! `meet-live.music-duck`, on unless off), the listener's own choice, by the
//! voices this app hears and its own microphone.
//!
//! The panel is panel.rs; the bar's button comes and goes with
//! [`available`].

mod duck;
mod loader;
mod output;
mod panel;

use std::{
    cell::{Cell, RefCell},
    collections::{HashMap, HashSet},
    rc::Rc,
    sync::Arc,
    time::{Duration, Instant},
};

use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};
use tomoshibi_core::call::{Call, CallEvent, CallState, MediaSource, PcmSink, TrackInfo};
use tomoshibi_protocol::{
    api::{MusicLibrary, MusicQuality, MusicTrack},
    jukebox::{
        desk_library, DeskAction, DeskLibrary, DeskOutput, DeskPacket, DeskPrepared, DeskSnapshot,
        DeskSong, JukeboxDesk,
    },
    share::{sound_lossless_offered, SoundKind},
};

pub(super) use panel::panel;

use self::{
    duck::Duck,
    loader::{DecodedSong, Loaded},
    output::Output,
};
use super::RoomState;
use crate::{
    app::{log, AppModel, WeakModel},
    i18n::{t, t_with},
};

/// preferences.ts `meet-live.music-quality`: "best", or nothing for
/// lossless; the settings' music page writes the same key.
const QUALITY_KEY: &str = "meet-live.music-quality";
/// duck.ts `meet-live.music-duck`: on unless "off"; the settings' music
/// page writes the same key.
const DUCK_KEY: &str = "meet-live.music-duck";
/// sound.ts `meet-live.music-monitor`: how loud the music playing from here
/// is in this machine's own speakers, nothing kept being full.
const MONITOR_KEY: &str = "meet-live.music-monitor";
/// The identity this app's own music is filed under in the mix
/// (media::Media `local_sound`).
const MONITOR: &str = "local:music";
/// A library of files for a test (TOMOSHIBI_DEBUG_MUSIC_FILES).
const FILES: &str = "files";

/// The song desk of the call there is now, made the first time the call
/// says anything.
#[derive(Default)]
pub struct Music {
    desk: RefCell<Option<Rc<SongDesk>>>,
    /// A desk is being made: anything that asks for it meanwhile (the bar,
    /// redrawn by something the new desk said) is told there is none yet,
    /// rather than making a second.
    making: Cell<bool>,
}

/// Which part of the panel is showing (MusicPanel.tsx `Tab`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tab {
    #[default]
    Desk,
    Search,
}

/// One call's desk: the protocol's desk, and everything it asks for.
pub struct SongDesk {
    model: WeakModel,
    call: Arc<Call>,
    desk: Arc<JukeboxDesk>,
    outbox: UnboundedSender<DeskPacket>,
    /// The libraries this participant can reach; `None` where there is none
    /// or nobody is signed in, which is most people.
    pub libraries: RefCell<Option<Vec<MusicLibrary>>>,
    pub snapshot: RefCell<DeskSnapshot>,
    loads: RefCell<HashMap<u64, glib::JoinHandle<()>>>,
    prepared: RefCell<HashMap<u64, Arc<DecodedSong>>>,
    output: RefCell<Option<Output>>,
    /// The track is being published.
    opening: Cell<bool>,
    /// Moved on by every stop, so a track still being published when the
    /// music was stopped finds out and takes itself down.
    generation: Cell<u64>,
    duck: RefCell<Duck>,
    ducking: Cell<bool>,
    duck_ticks: Cell<u64>,
    /// What was last handed to the mix for each person's music.
    ducked: RefCell<HashMap<String, f32>>,
    started: Instant,
    over: Cell<bool>,
    /// An ask is out, looked up in the library here.
    pub asking: Cell<bool>,
    asked: Cell<Option<u64>>,
    // What the panel shows, kept for the call.
    pub tab: Cell<Tab>,
    pub search_results: RefCell<Option<Vec<MusicTrack>>>,
    pub searching: Cell<bool>,
    pub search_problem: RefCell<Option<String>>,
    pub added: RefCell<HashSet<String>>,
    /// The listener's own level for each person's music, and whether it is
    /// stopped at the server (hearing.ts).
    pub hearing: RefCell<HashMap<String, (f32, bool)>>,
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.as_millis() as u64)
        .unwrap_or_default()
}

/// The song desk's music, in either form (sound.ts `isMusic`).
fn is_music(track: &TrackInfo) -> bool {
    track.source == MediaSource::Unknown && track.name.starts_with("music")
}

/// The call's desk, made the first time it is wanted.
pub(super) fn desk_of(model: &AppModel) -> Option<Rc<SongDesk>> {
    let call = super::call_of(model)?;
    let music = model.kept::<Music>();
    if let Some(desk) = music.desk.borrow().as_ref() {
        if Arc::ptr_eq(&desk.call, &call) && !desk.over.get() {
            return Some(desk.clone());
        }
    }
    if music.making.replace(true) {
        return None;
    }
    let desk = SongDesk::begin(model, call);
    music.desk.replace(Some(desk.clone()));
    music.making.set(false);
    Some(desk)
}

/// Everything the call says, for the desk: who comes and goes, the topic,
/// a reconnect's roster, music arriving to be turned down, the end.
pub(super) fn follow(model: &AppModel, call: &Arc<Call>, event: &CallEvent) {
    if let CallEvent::Ended { .. } = event {
        if let Some(desk) = model.kept::<Music>().desk.take() {
            desk.end();
        }
        return;
    }
    let Some(desk) = desk_of(model).filter(|desk| Arc::ptr_eq(&desk.call, call)) else {
        return;
    };
    let now = now_ms();
    match event {
        CallEvent::ParticipantJoined { participant } => {
            desk.apply(
                desk.desk
                    .participant_connected(participant.identity.clone(), now),
            );
        }
        CallEvent::ParticipantLeft { identity } => {
            desk.apply(desk.desk.participant_disconnected(identity.clone(), now));
            desk.duck.borrow_mut().left(identity);
            desk.ducked.borrow_mut().remove(identity);
        }
        CallEvent::StateChanged {
            state: CallState::Connected,
        } => {
            desk.apply(desk.desk.roster(desk.identities(), now));
        }
        CallEvent::Data {
            topic,
            identity: Some(identity),
            payload,
        } if topic == tomoshibi_protocol::jukebox::TOPIC => {
            // Only from somebody in the room: the server's own messages
            // are not the desk's (jukebox.ts `onData`).
            let name = desk.name_of(model, identity);
            desk.apply(
                desk.desk
                    .on_message(identity.clone(), name, payload.clone(), now),
            );
        }
        CallEvent::TrackSubscribed { identity, track } if is_music(track) => {
            desk.arrived(identity);
            desk.hear(model, identity);
        }
        CallEvent::TrackUnpublished { identity, .. }
        | CallEvent::TrackUnsubscribed { identity, .. } => {
            let still = model
                .kept::<RoomState>()
                .talk
                .borrow()
                .as_ref()
                .is_some_and(|talk| {
                    talk.person(identity)
                        .is_some_and(|person| person.music_track().is_some())
                });
            if !still {
                desk.duck.borrow_mut().left(identity);
            }
        }
        _ => {}
    }
}

/// Whether the bar offers the panel (Room.tsx ~280): to somebody who can
/// reach the library before there is any music, and to everybody once there
/// is a desk or music to turn down or ask for.
pub(super) fn available(model: &AppModel) -> bool {
    let Some(desk) = desk_of(model) else {
        return false;
    };
    let music_here = model
        .kept::<RoomState>()
        .talk
        .borrow()
        .as_ref()
        .is_some_and(|talk| {
            talk.people
                .iter()
                .any(|person| person.music_track().is_some())
        });
    desk.libraries.borrow().is_some()
        || desk.snapshot.borrow().state.is_some()
        || desk.playing_here()
        || music_here
}

/// Whether music is playing from anybody, for the bar's dot.
pub(super) fn playing(model: &AppModel) -> bool {
    let Some(desk) = desk_of(model) else {
        return false;
    };
    let music_here = model
        .kept::<RoomState>()
        .talk
        .borrow()
        .as_ref()
        .is_some_and(|talk| {
            talk.people
                .iter()
                .any(|person| person.music_track().is_some())
        });
    desk.snapshot
        .borrow()
        .state
        .as_ref()
        .is_some_and(|state| state.now.is_some())
        || desk.playing_here()
        || music_here
}

impl SongDesk {
    fn begin(model: &AppModel, call: Arc<Call>) -> Rc<SongDesk> {
        let sealed = super::sealed(model);
        let desk = JukeboxDesk::new(call.local_identity(), call.local_name(), sealed);
        let (outbox, mut outgoing) = unbounded_channel::<DeskPacket>();
        let enabled = model.preferences().string(DUCK_KEY).as_deref() != Some("off");
        let song_desk = Rc::new(SongDesk {
            model: model.downgrade(),
            call: call.clone(),
            desk,
            outbox,
            libraries: RefCell::new(None),
            snapshot: RefCell::new(DeskSnapshot {
                state: None,
                mine: false,
                broadcast: !sealed,
            }),
            loads: RefCell::new(HashMap::new()),
            prepared: RefCell::new(HashMap::new()),
            output: RefCell::new(None),
            opening: Cell::new(false),
            generation: Cell::new(0),
            duck: RefCell::new(Duck::new(enabled)),
            ducking: Cell::new(false),
            duck_ticks: Cell::new(0),
            ducked: RefCell::new(HashMap::new()),
            started: Instant::now(),
            over: Cell::new(false),
            asking: Cell::new(false),
            asked: Cell::new(None),
            tab: Cell::new(Tab::Desk),
            search_results: RefCell::new(None),
            searching: Cell::new(false),
            search_problem: RefCell::new(None),
            added: RefCell::new(HashSet::new()),
            hearing: RefCell::new(HashMap::new()),
        });
        // One sender, so the room hears the desk's words in the order it
        // said them.
        glib::spawn_future_local(async move {
            while let Some(packet) = outgoing.recv().await {
                if let Err(fault) = call
                    .clone()
                    .send_data(
                        packet.topic,
                        packet.payload,
                        packet.destinations,
                        packet.reliable,
                    )
                    .await
                {
                    log(&format!("music: the desk's word was not sent: {fault}"));
                }
            }
        });
        // Whoever is here already, and a word to any desk running.
        let now = now_ms();
        song_desk.apply(song_desk.desk.roster(song_desk.identities(), now));
        song_desk.apply(song_desk.desk.joined(now));
        if let Some(talk) = model.kept::<RoomState>().talk.borrow().as_ref() {
            for person in &talk.people {
                if person.music_track().is_some() {
                    song_desk.duck.borrow_mut().arrived(&person.id, 0.0);
                }
            }
        }
        // The desk wants the time at least once a second.
        let weak = Rc::downgrade(&song_desk);
        glib::timeout_add_local(Duration::from_secs(1), move || {
            let Some(desk) = weak.upgrade().filter(|desk| !desk.over.get()) else {
                return glib::ControlFlow::Break;
            };
            desk.apply(desk.desk.tick(now_ms()));
            glib::ControlFlow::Continue
        });
        song_desk.find_libraries(model);
        song_desk.ensure_ducking();
        #[cfg(debug_assertions)]
        song_desk.drive(model);
        song_desk
    }

    fn identities(&self) -> Vec<String> {
        self.call
            .participants()
            .into_iter()
            .map(|person| person.identity)
            .collect()
    }

    fn name_of(&self, model: &AppModel, identity: &str) -> String {
        model
            .kept::<RoomState>()
            .talk
            .borrow()
            .as_ref()
            .and_then(|talk| talk.person(identity).map(|person| person.name.clone()))
            .unwrap_or_default()
    }

    pub fn playing_here(&self) -> bool {
        self.output.borrow().is_some() || self.opening.get()
    }

    pub fn progress(&self) -> Option<(f64, f64)> {
        self.output.borrow().as_ref().and_then(Output::progress)
    }

    /// The libraries, asked once as Room.tsx asks: a deployment without one,
    /// or somebody not signed in, is no library.
    fn find_libraries(self: &Rc<Self>, model: &AppModel) {
        #[cfg(debug_assertions)]
        if files_directory().is_some() {
            self.libraries.replace(Some(vec![MusicLibrary {
                id: FILES.into(),
                name: "Files".into(),
                signed_in: true,
            }]));
            return;
        }
        let Some(entry) = model.call_entry() else {
            return;
        };
        let weak = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            let found = entry.deployment.music_sources().await.unwrap_or_default();
            if let Some(desk) = weak.upgrade() {
                log(&format!("music: {} libraries", found.len()));
                desk.libraries.replace((!found.is_empty()).then_some(found));
                desk.changed();
            }
        });
    }

    fn changed(&self) {
        if let Some(model) = self.model.upgrade() {
            super::changed(&model);
        }
    }

    fn say(&self, words: String) {
        if let Some(model) = self.model.upgrade() {
            super::failed(&model, words);
        }
    }

    /// What the desk said and asked for, done in order.
    fn apply(self: &Rc<Self>, out: DeskOutput) {
        if self.over.get() {
            return;
        }
        for packet in out.packets {
            let _ = self.outbox.send(packet);
        }
        for action in out.actions {
            self.perform(action);
        }
        for entry in out.skipped {
            self.say(t_with(
                "Skipped {title}: the library's account cannot play it.",
                &[("title", &entry.title)],
            ));
        }
        if out.changed {
            self.refresh();
        }
    }

    fn refresh(&self) {
        let snapshot = self.desk.state(now_ms());
        if *self.snapshot.borrow() != snapshot {
            self.snapshot.replace(snapshot);
            self.changed();
        }
    }

    fn perform(self: &Rc<Self>, action: DeskAction) {
        log(&format!("music: {}", describe(&action)));
        match action {
            DeskAction::Prepare { request, entry } => self.prepare(request, entry.source, entry.id),
            DeskAction::Abandon { request } => {
                if let Some(load) = self.loads.borrow_mut().remove(&request) {
                    load.abort();
                }
                self.prepared.borrow_mut().remove(&request);
            }
            DeskAction::Play { request, entry } => self.play(request, entry.key),
            DeskAction::Stop => self.stop_music(),
            DeskAction::Pause { paused } => {
                if let Some(output) = self.output.borrow().as_ref() {
                    output.pause(paused);
                }
            }
            DeskAction::Search {
                request,
                query,
                mine,
            } => self.search(request, query, mine),
            DeskAction::ReadLink {
                request,
                text,
                mine,
            } => self.read_link(request, text, mine),
        }
    }

    /// Whether the music goes out with a lossless stream beside it: where it
    /// is wanted, and never in a sealed call.
    fn lossless_offered(&self) -> bool {
        let Some(model) = self.model.upgrade() else {
            return false;
        };
        sound_lossless_offered(super::lossless_wanted(&model), super::sealed(&model), true)
    }

    fn quality(&self) -> MusicQuality {
        let best = self.model.upgrade().is_some_and(|model| {
            model.preferences().string(QUALITY_KEY).as_deref() == Some("best")
        });
        if best {
            MusicQuality::Best
        } else {
            MusicQuality::Lossless
        }
    }

    /// Describe, fetch and decode; the desk is told what came of it unless
    /// it abandoned the request meanwhile.
    fn prepare(self: &Rc<Self>, request: u64, source: String, id: String) {
        let keep_original = self
            .output
            .borrow()
            .as_ref()
            .map(|output| output.lossless)
            .unwrap_or_else(|| self.lossless_offered());
        let quality = self.quality();
        let deployment = self
            .model
            .upgrade()
            .and_then(|model| model.call_entry())
            .map(|entry| entry.deployment);
        let weak = Rc::downgrade(self);
        let load = glib::spawn_future_local(async move {
            let loaded = if source == FILES {
                match files_directory() {
                    Some(directory) => loader::load_file(directory.join(&id), keep_original).await,
                    None => Err("no files".into()),
                }
            } else {
                match deployment {
                    Some(deployment) => {
                        loader::load(deployment, source, id, quality, keep_original).await
                    }
                    None => Err("no deployment".into()),
                }
            };
            let Some(desk) = weak.upgrade() else { return };
            if desk.loads.borrow_mut().remove(&request).is_none() || desk.over.get() {
                return;
            }
            let outcome = match loaded {
                Ok(Loaded::Ready(song)) => {
                    let quality = song.quality.clone();
                    desk.prepared.borrow_mut().insert(request, song);
                    DeskPrepared::Ready { quality }
                }
                Ok(Loaded::Unavailable) => DeskPrepared::Unavailable,
                Err(fault) => {
                    log(&format!("music: a song did not load: {fault}"));
                    DeskPrepared::Failed
                }
            };
            desk.apply(desk.desk.prepared(request, outcome, now_ms()));
        });
        self.loads.borrow_mut().insert(request, load);
    }

    fn play(self: &Rc<Self>, request: u64, key: String) {
        let song = self.prepared.borrow_mut().remove(&request);
        let started = self.generation.get();
        let weak = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            let Some(desk) = weak.upgrade() else { return };
            let mut ok = false;
            if let Some(song) = song {
                if desk.open_output(started).await && started == desk.generation.get() {
                    if let Some(output) = desk.output.borrow().as_ref() {
                        output.play(song, key);
                        ok = true;
                    }
                }
            }
            if !desk.over.get() {
                desk.apply(desk.desk.played(request, ok, now_ms()));
            }
        });
    }

    /// The music's own track, published the first time a song plays and
    /// kept across the songs that follow until the music stops.
    async fn open_output(self: &Rc<Self>, started: u64) -> bool {
        if self.output.borrow().is_some() {
            return true;
        }
        if self.opening.replace(true) {
            // Another play is publishing it: wait for that.
            for _ in 0..200 {
                glib::timeout_future(Duration::from_millis(50)).await;
                if !self.opening.get() {
                    break;
                }
            }
            return self.output.borrow().is_some();
        }
        let offered = self.lossless_offered();
        let published = self
            .call
            .clone()
            .publish_audio(SoundKind::LibraryMusic {
                lossless_offered: offered,
            })
            .await;
        self.opening.set(false);
        let input = match published {
            Ok(input) => input,
            Err(fault) => {
                log(&format!("music: the track was not published: {fault}"));
                return false;
            }
        };
        // Stopped while it was being published: taken down again rather
        // than left playing silence into the room (sound.ts).
        if started != self.generation.get() || self.over.get() {
            let _ = input.unpublish().await;
            return false;
        }
        let model = self.model.upgrade();
        let monitor: Option<Arc<dyn PcmSink>> = model
            .as_ref()
            .and_then(super::media_of)
            .and_then(|media| media.local_sound("music"));
        let weak_desk = Rc::downgrade(self);
        let (ended_tell, mut ended_heard) = unbounded_channel::<String>();
        glib::spawn_future_local(async move {
            while let Some(key) = ended_heard.recv().await {
                let Some(desk) = weak_desk.upgrade() else {
                    return;
                };
                if !desk.over.get() {
                    log(&format!("music: {key} ended"));
                    desk.apply(desk.desk.track_ended(key, now_ms()));
                }
            }
        });
        let output = Output::start(input, offered, monitor, move |key| {
            let _ = ended_tell.send(key);
        });
        log(&format!("music: published, lossless offered {offered}"));
        self.output.replace(Some(output));
        if let Some(model) = model.as_ref() {
            self.set_monitor_volume(model, monitor_volume(model));
        }
        self.arrived(duck::LOCAL);
        self.changed();
        true
    }

    fn stop_music(&self) {
        self.generation.set(self.generation.get() + 1);
        let output = self.output.borrow_mut().take();
        if let Some(output) = output {
            output.stop();
            let input = output.input.clone();
            glib::spawn_future_local(async move {
                let _ = input.unpublish().await;
            });
        }
        self.duck.borrow_mut().left(duck::LOCAL);
        self.changed();
    }

    /// The top hit for a keyword, from the library the desk searches.
    fn search(self: &Rc<Self>, request: u64, query: String, mine: bool) {
        let libraries: Vec<DeskLibrary> = self
            .libraries
            .borrow()
            .iter()
            .flatten()
            .map(|library| DeskLibrary {
                id: library.id.clone(),
                signed_in: library.signed_in,
            })
            .collect();
        let library = desk_library(libraries);
        let weak = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            let Some(desk) = weak.upgrade() else { return };
            let mut songs = Vec::new();
            let mut failed = false;
            if let Some(library) = library {
                match desk.searched(&library, &query).await {
                    Ok(found) => {
                        songs = found
                            .into_iter()
                            .map(|track| song(track, &library))
                            .collect()
                    }
                    Err(()) => failed = true,
                }
            }
            if desk.over.get() {
                return;
            }
            let out = desk.desk.found(request, songs, now_ms());
            let accepted = out.accepted;
            desk.apply(out);
            if mine {
                desk.answered(
                    request,
                    if failed {
                        Some(t("The library did not answer. Try again."))
                    } else if accepted {
                        None
                    } else {
                        Some(t("Nothing found."))
                    },
                );
            }
        });
    }

    fn read_link(self: &Rc<Self>, request: u64, text: String, mine: bool) {
        let deployment = self
            .model
            .upgrade()
            .and_then(|model| model.call_entry())
            .map(|entry| entry.deployment);
        let weak = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            let mut songs = Vec::new();
            let mut failed = false;
            if let Some(deployment) = deployment {
                match deployment.music_link(text).await {
                    Ok(Some(link)) => {
                        songs = link
                            .tracks
                            .into_iter()
                            .map(|track| song(track, &link.source))
                            .collect()
                    }
                    Ok(None) => {}
                    Err(_) => failed = true,
                }
            }
            let Some(desk) = weak.upgrade() else { return };
            if desk.over.get() {
                return;
            }
            let out = desk.desk.found(request, songs, now_ms());
            let accepted = out.accepted;
            desk.apply(out);
            if mine {
                desk.answered(
                    request,
                    if failed {
                        Some(t("The library did not answer. Try again."))
                    } else if accepted {
                        None
                    } else {
                        Some(t("That link names no playlist or song."))
                    },
                );
            }
        });
    }

    fn answered(&self, request: u64, problem: Option<String>) {
        if self.asked.get() == Some(request) {
            self.asked.set(None);
            self.asking.set(false);
        }
        if let Some(problem) = problem {
            self.say(problem);
        }
        self.changed();
    }

    /// A library's answer to a search: the deployment's, or the files a
    /// test stands in for one with.
    pub async fn searched(&self, library: &str, query: &str) -> Result<Vec<MusicTrack>, ()> {
        if library == FILES {
            return Ok(files_matching(query));
        }
        let Some(entry) = self.model.upgrade().and_then(|model| model.call_entry()) else {
            return Err(());
        };
        entry
            .deployment
            .music_search(library.to_owned(), query.to_owned(), 1)
            .await
            .map_err(|_| ())
    }

    // ---- what the person does ------------------------------------------

    /// Where nobody runs a desk and this participant can reach a library, it
    /// starts here first (MusicPanel.tsx `add`, `ask`).
    fn start_if_nobody_runs_one(self: &Rc<Self>) {
        if self.snapshot.borrow().state.is_none() && self.libraries.borrow().is_some() {
            self.apply(self.desk.start(now_ms()));
        }
    }

    pub fn add(self: &Rc<Self>, songs: Vec<DeskSong>) {
        self.start_if_nobody_runs_one();
        let out = self.desk.add_songs(songs, now_ms());
        let accepted = out.accepted;
        self.apply(out);
        if !accepted {
            self.say(t("Nobody is running the song desk."));
        }
    }

    /// Asks for a song by what was typed: a song's name, or a playlist or
    /// song link.
    pub fn ask(self: &Rc<Self>, typed: &str) {
        let text = typed.trim();
        if text.is_empty() || self.asking.get() {
            return;
        }
        self.start_if_nobody_runs_one();
        let out = self.desk.ask(text.to_owned(), now_ms());
        for action in &out.actions {
            if let DeskAction::Search {
                request,
                mine: true,
                ..
            }
            | DeskAction::ReadLink {
                request,
                mine: true,
                ..
            } = action
            {
                self.asked.set(Some(*request));
                self.asking.set(true);
            }
        }
        let accepted = out.accepted;
        self.apply(out);
        if !accepted {
            let mine = self.snapshot.borrow().mine;
            self.say(if mine {
                t("Nothing found.")
            } else {
                t("Nobody is running the song desk.")
            });
        }
        self.changed();
    }

    pub fn vote(self: &Rc<Self>) {
        self.apply(self.desk.vote(now_ms()));
    }
    pub fn skip(self: &Rc<Self>) {
        self.apply(self.desk.skip(now_ms()));
    }
    pub fn pause(self: &Rc<Self>, paused: bool) {
        self.apply(self.desk.pause(paused, now_ms()));
    }
    pub fn remove(self: &Rc<Self>, key: &str) {
        self.apply(self.desk.remove(key.to_owned(), now_ms()));
    }
    pub fn to_top(self: &Rc<Self>, key: &str) {
        self.apply(self.desk.to_top(key.to_owned(), now_ms()));
    }
    pub fn close(self: &Rc<Self>) {
        self.apply(self.desk.close(now_ms()));
    }

    /// A search of the panel's search tab.
    pub fn search_library(self: &Rc<Self>, query: &str) {
        let asked = query.trim().to_owned();
        if asked.is_empty() || self.searching.get() {
            return;
        }
        let Some(library) = self.libraries.borrow().as_ref().and_then(|all| {
            all.iter()
                .find(|library| library.signed_in)
                .or(all.first())
                .map(|library| library.id.clone())
        }) else {
            return;
        };
        self.searching.set(true);
        self.search_problem.replace(None);
        self.changed();
        let weak = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            let Some(desk) = weak.upgrade() else { return };
            match desk.searched(&library, &asked).await {
                Ok(found) => {
                    desk.search_results.replace(Some(found));
                }
                Err(()) => {
                    desk.search_problem
                        .replace(Some(t("The library did not answer. Try again.")));
                }
            }
            desk.added.borrow_mut().clear();
            desk.searching.set(false);
            desk.changed();
        });
    }

    /// The library a search result is in: the one the search tab searched.
    pub fn search_library_id(&self) -> Option<String> {
        self.libraries.borrow().as_ref().and_then(|all| {
            all.iter()
                .find(|library| library.signed_in)
                .or(all.first())
                .map(|library| library.id.clone())
        })
    }

    // ---- how loud music is here ----------------------------------------

    /// The listener's own setting for somebody's music: their level of it in
    /// the mix, and at the server, where a muted song is not even sent
    /// (hearing.ts `setBlocked`) and a silenced one's lossless stream not
    /// asked for.
    pub fn set_hearing(&self, model: &AppModel, identity: &str, level: f32, blocked: bool) {
        self.hearing
            .borrow_mut()
            .insert(identity.to_owned(), (level, blocked));
        self.hear(model, identity);
        self.changed();
    }

    pub fn hearing_of(&self, identity: &str) -> (f32, bool) {
        self.hearing
            .borrow()
            .get(identity)
            .copied()
            .unwrap_or((1.0, false))
    }

    fn hear(&self, model: &AppModel, identity: &str) {
        let (level, blocked) = self.hearing_of(identity);
        if let Some(media) = super::media_of(model) {
            media.set_kind_volume(identity, true, if blocked { 0.0 } else { level });
        }
        let call = self.call.clone();
        let identity = identity.to_owned();
        let silenced = blocked || level <= 0.0;
        glib::spawn_future_local(async move {
            call.set_hearing(identity, true, level, silenced).await;
        });
    }

    /// How loud the music from here is in this machine's own speakers, which
    /// changes nothing sent.
    pub fn set_monitor_volume(&self, model: &AppModel, level: f32) {
        let level = level.clamp(0.0, 1.0);
        if level >= 1.0 {
            model.preferences().remove(MONITOR_KEY);
        } else {
            model.preferences().set(MONITOR_KEY, level.to_string());
        }
        if let Some(media) = super::media_of(model) {
            media.set_kind_volume(MONITOR, true, level);
        }
        self.changed();
    }

    pub fn set_ducks(&self, model: &AppModel, on: bool) {
        if on {
            model.preferences().remove(DUCK_KEY);
        } else {
            model.preferences().set(DUCK_KEY, "off");
        }
        self.duck.borrow_mut().enabled = on;
        self.apply_duck(model, self.clock());
        self.changed();
    }

    pub fn ducks(&self) -> bool {
        self.duck.borrow().enabled
    }

    // ---- turning the music down under speech ---------------------------

    fn clock(&self) -> f64 {
        self.started.elapsed().as_secs_f64() * 1000.0
    }

    /// A music stream has started from somebody, or from here: faded in, and
    /// the voices listened to while there is music to turn down.
    fn arrived(self: &Rc<Self>, key: &str) {
        let now = self.clock();
        self.duck.borrow_mut().arrived(key, now);
        self.ensure_ducking();
    }

    fn ensure_ducking(self: &Rc<Self>) {
        if self.ducking.replace(true) {
            return;
        }
        let weak = Rc::downgrade(self);
        let mut last = self.clock();
        glib::timeout_add_local(Duration::from_millis(duck::TICK_MS as u64), move || {
            let Some(desk) = weak.upgrade().filter(|desk| !desk.over.get()) else {
                return glib::ControlFlow::Break;
            };
            let Some(model) = desk.model.upgrade() else {
                return glib::ControlFlow::Break;
            };
            let now = desk.clock();
            let elapsed = now - last;
            last = now;
            if desk.duck_tick(&model, now, elapsed) {
                glib::ControlFlow::Continue
            } else {
                desk.ducking.set(false);
                glib::ControlFlow::Break
            }
        });
    }

    /// One reading of every voice, this participant's own included, and the
    /// music moved (duck.ts `tick`). False once there is no music and
    /// nothing still moving.
    fn duck_tick(&self, model: &AppModel, now: f64, elapsed: f64) -> bool {
        let room = model.kept::<RoomState>();
        let music = self.playing_here()
            || room.talk.borrow().as_ref().is_some_and(|talk| {
                talk.people
                    .iter()
                    .any(|person| person.music_track().is_some())
            });
        let mut levels: Vec<f64> = super::media_of(model)
            .map(|media| media.voice_levels())
            .unwrap_or_default()
            .into_iter()
            .map(|(_, level)| f64::from(level))
            .collect();
        if room.local.microphone_on.get() {
            if let Some(microphone) = room.local.microphone.borrow().as_ref() {
                levels.push(f64::from(microphone.level()));
            }
        }
        let moved = self.duck.borrow_mut().tick(&levels, now, elapsed);
        if moved {
            self.apply_duck(model, now);
        }
        // Every two seconds in a debug run, for the tests that read the log:
        // how loud the loudest voice was and how far the music is down.
        if cfg!(debug_assertions) {
            let ticks = self.duck_ticks.get() + 1;
            self.duck_ticks.set(ticks);
            if ticks.is_multiple_of(50) {
                let loudest = levels.iter().copied().fold(0.0, f64::max);
                log(&format!(
                    "music: duck {:.2}, {} voices, the loudest {:.4}",
                    self.duck.borrow().level,
                    levels.len(),
                    loudest
                ));
            }
        }
        music || !self.duck.borrow().settled(now)
    }

    fn apply_duck(&self, model: &AppModel, now: f64) {
        let Some(media) = super::media_of(model) else {
            return;
        };
        let mut keys: Vec<String> = model
            .kept::<RoomState>()
            .talk
            .borrow()
            .as_ref()
            .map(|talk| {
                talk.people
                    .iter()
                    .filter(|person| person.music_track().is_some())
                    .map(|person| person.id.clone())
                    .collect()
            })
            .unwrap_or_default();
        if self.playing_here() {
            keys.push(duck::LOCAL.to_owned());
        }
        let duck = self.duck.borrow();
        let mut ducked = self.ducked.borrow_mut();
        for key in keys {
            let factor = duck.factor(&key, now) as f32;
            if ducked
                .get(&key)
                .is_some_and(|was| (was - factor).abs() < 0.002)
            {
                continue;
            }
            ducked.insert(key.clone(), factor);
            let identity = if key == duck::LOCAL {
                MONITOR
            } else {
                key.as_str()
            };
            media.set_duck(identity, factor);
        }
    }

    /// The call is over: everything stops, nothing more is said.
    fn end(&self) {
        self.over.set(true);
        for (_, load) in self.loads.borrow_mut().drain() {
            load.abort();
        }
        self.prepared.borrow_mut().clear();
        if let Some(output) = self.output.borrow_mut().take() {
            output.stop();
        }
    }
}

fn song(track: MusicTrack, source: &str) -> DeskSong {
    DeskSong {
        source: source.to_owned(),
        id: track.id,
        title: track.title,
        artists: track.artists,
        album: track.album,
        cover: track.cover,
        duration: track.duration,
    }
}

fn monitor_volume(model: &AppModel) -> f32 {
    // Nothing kept is full, and so is anything that is not a volume:
    // sound.ts once read "nothing" as nought and played the music to
    // everybody but its own speakers.
    model
        .preferences()
        .string(MONITOR_KEY)
        .and_then(|kept| kept.parse::<f32>().ok())
        .filter(|level| (0.0..=1.0).contains(level))
        .unwrap_or(1.0)
}

pub(super) fn monitor_level(model: &AppModel) -> f32 {
    monitor_volume(model)
}

fn describe(action: &DeskAction) -> String {
    match action {
        DeskAction::Prepare { request, entry } => format!("prepare {request} {}", entry.title),
        DeskAction::Abandon { request } => format!("abandon {request}"),
        DeskAction::Play { request, entry } => format!("play {request} {}", entry.title),
        DeskAction::Stop => "stop".into(),
        DeskAction::Pause { paused } => format!("pause {paused}"),
        DeskAction::Search { request, .. } => format!("search {request}"),
        DeskAction::ReadLink { request, .. } => format!("read a link {request}"),
    }
}

/// TOMOSHIBI_DEBUG_MUSIC_FILES: a directory standing in for the library, in
/// a debug build; never in a release build.
fn files_directory() -> Option<std::path::PathBuf> {
    #[cfg(debug_assertions)]
    {
        std::env::var_os("TOMOSHIBI_DEBUG_MUSIC_FILES").map(std::path::PathBuf::from)
    }
    #[cfg(not(debug_assertions))]
    {
        None
    }
}

/// The files whose names match a search (`*` all of them), as songs.
fn files_matching(query: &str) -> Vec<MusicTrack> {
    let Some(directory) = files_directory() else {
        return Vec::new();
    };
    let wanted = query.trim().to_lowercase();
    let mut names: Vec<String> = std::fs::read_dir(&directory)
        .map(|entries| {
            entries
                .flatten()
                .filter_map(|entry| entry.file_name().into_string().ok())
                .filter(|name| !name.starts_with('.'))
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
        .into_iter()
        .filter(|name| wanted == "*" || name.to_lowercase().contains(&wanted))
        .map(|name| MusicTrack {
            title: name
                .rsplit_once('.')
                .map(|(stem, _)| stem.to_owned())
                .unwrap_or(name.clone()),
            id: name,
            artists: vec!["Files".into()],
            album: String::new(),
            cover: String::new(),
            duration: 0.0,
        })
        .collect()
}

/// A room script's `music=<what>`: `ask:<words>`, `pause`, `play`, `skip`,
/// `vote`, `close`, `duck:<on|off>`, `monitor:<0-100>`, `volume:<0-100>`
/// (the first person's music), `tab:<desk|search>`, `search:<words>`.
pub(super) fn script(model: &AppModel, value: &str) {
    let Some(desk) = desk_of(model) else { return };
    let (verb, rest) = value.split_once(':').unwrap_or((value, ""));
    match verb {
        "ask" => desk.ask(rest),
        "pause" => desk.pause(true),
        "play" => desk.pause(false),
        "skip" => desk.skip(),
        "vote" => desk.vote(),
        "close" => desk.close(),
        "duck" => desk.set_ducks(model, rest != "off"),
        "monitor" => desk.set_monitor_volume(model, rest.parse::<f32>().unwrap_or(100.0) / 100.0),
        "volume" => {
            let first = model
                .kept::<RoomState>()
                .talk
                .borrow()
                .as_ref()
                .and_then(|talk| {
                    talk.people
                        .iter()
                        .find(|person| person.music_track().is_some())
                        .map(|person| person.id.clone())
                });
            if let Some(first) = first {
                desk.set_hearing(
                    model,
                    &first,
                    rest.parse::<f32>().unwrap_or(100.0) / 100.0,
                    false,
                );
            }
        }
        "tab" => {
            desk.tab.set(if rest == "search" {
                Tab::Search
            } else {
                Tab::Desk
            });
            desk.changed();
        }
        "search" => {
            desk.tab.set(Tab::Search);
            desk.search_library(rest);
        }
        _ => log(&format!("music: no such step {value:?}")),
    }
}

#[cfg(debug_assertions)]
impl SongDesk {
    /// TOMOSHIBI_DEBUG_MUSIC_ASK=first|second: asks for each in turn once
    /// the libraries are known, as if typed into the desk's box.
    fn drive(self: &Rc<Self>, _model: &AppModel) {
        let Ok(asks) = std::env::var("TOMOSHIBI_DEBUG_MUSIC_ASK") else {
            return;
        };
        let weak = Rc::downgrade(self);
        glib::timeout_add_local_once(Duration::from_secs(2), move || {
            if let Some(desk) = weak.upgrade() {
                for ask in asks.split('|').filter(|ask| !ask.trim().is_empty()) {
                    desk.asking.set(false);
                    desk.ask(ask);
                }
            }
        });
    }
}
