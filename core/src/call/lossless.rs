//! Hearing a browser's shared sound or music without loss
//! (web/src/live/lossless-receiver.ts, through the protocol crate's
//! `LosslessReceiver`).
//!
//! The receiver decides when to ask for a stream, when to give up on it and
//! which Opus tracks to hold back at the server meanwhile; this module gives it
//! the room, the listener's hearing settings and a clock, carries out what it
//! says, and plays each stream: decoded blocks go into the web's own playout
//! buffer (`Worklet`), which a task drains ten milliseconds at a time on a real
//! clock and hands to the app as samples at the stream's own rate. The buffer
//! is kept here rather than in the app so that its underruns are counted the
//! way a browser counts them, which is what the receiver's verdicts rest on.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex, Weak},
    time::Duration,
};

use livekit::prelude::*;
use tomoshibi_protocol::lossless::{
    receiver::{Hearing, LosslessReceiver, Outcome, Player, Publication, Remote},
    PlayoutCounts, Sound, TrackSource as LosslessSource, Worklet,
};

use super::{freeze::now_ms, Call};

/// Where lossless sound goes in the app: one stream per person and sound,
/// interleaved 32-bit float at the stream's own rate.
#[uniffi::export(with_foreign)]
pub trait LosslessSink: Send + Sync {
    fn on_lossless(&self, identity: String, music: bool, samples: Vec<f32>, rate: u32, channels: u32);
    /// The stream is over, or given up on; its Opus track is back.
    fn on_lossless_end(&self, identity: String, music: bool);
}

/// What the listener set for each person's sounds (hearing.ts), which decides
/// whether a stream is asked for at all and how loud it plays.
#[derive(Default)]
pub(super) struct Settings {
    levels: HashMap<(String, bool), (f32, bool)>,
}

impl Settings {
    pub(super) fn levels_set(&mut self, identity: String, music: bool, level: f32, blocked: bool) {
        self.levels.insert((identity, music), (level.clamp(0.0, 1.0), blocked));
    }
}

impl Hearing for Settings {
    fn silenced(&self, identity: &str, sound: Sound) -> bool {
        self.levels.get(&(identity.to_string(), sound == Sound::Music)).is_some_and(|(_, blocked)| *blocked)
    }

    fn level(&self, identity: &str, sound: Sound) -> f32 {
        self.levels.get(&(identity.to_string(), sound == Sound::Music)).map_or(1.0, |(level, _)| *level)
    }
}

pub(super) struct Lossless {
    receiver: Mutex<LosslessReceiver>,
    pub(super) settings: Mutex<Settings>,
    sink: Arc<Mutex<Option<Arc<dyn LosslessSink>>>>,
}

impl Lossless {
    pub(super) fn new(encrypted: bool) -> Self {
        let sink: Arc<Mutex<Option<Arc<dyn LosslessSink>>>> = Arc::new(Mutex::new(None));
        let for_players = sink.clone();
        let make = Box::new(move |identity: &str, sound: Sound, rate: u32, channels: usize| -> Box<dyn Player> {
            Box::new(NativePlayer::start(identity.to_string(), sound == Sound::Music, rate, channels, for_players.clone()))
        });
        Self {
            receiver: Mutex::new(LosslessReceiver::new(encrypted, make)),
            settings: Mutex::new(Settings::default()),
            sink,
        }
    }

    pub(super) fn set_sink(&self, sink: Arc<dyn LosslessSink>) {
        *super::held(&self.sink) = Some(sink);
    }
}

fn source(of: TrackSource) -> LosslessSource {
    match of {
        TrackSource::Camera => LosslessSource::Camera,
        TrackSource::Microphone => LosslessSource::Microphone,
        TrackSource::Screenshare => LosslessSource::ScreenShare,
        TrackSource::ScreenshareAudio => LosslessSource::ScreenShareAudio,
        _ => LosslessSource::Unknown,
    }
}

fn room(call: &Call) -> Vec<Remote> {
    call.room
        .remote_participants()
        .values()
        .map(|p| Remote {
            identity: p.identity().to_string(),
            publications: p
                .track_publications()
                .values()
                .map(|publication| Publication {
                    sid: publication.sid().to_string(),
                    source: source(publication.source()),
                    name: publication.name(),
                })
                .collect(),
        })
        .collect()
}

/// Whether a lossless packet's rate is one this app can play.
///
/// The format takes any rate a sender writes, as the web's does, byte for
/// byte; what refuses a rate in a browser is the AudioContext the receiver
/// then asks for, so the stream is simply never played. Nothing refused it
/// here: a block saying 0 crashed every native listener at once (there is no
/// audio format without a rate), 1 to 4 gave a playback buffer of no length,
/// and a huge rate asked for gigabytes. Such a packet is dropped before the
/// receiver sees it, as if it had not come. Every rate audio is really made at
/// lies inside these bounds. The rate is bytes 4 to 8 of the header
/// (format.rs `unpack`); a packet too short to have one is the format's to
/// call malformed.
fn playable_rate(payload: &[u8]) -> bool {
    match payload.get(4..8) {
        Some(b) => (8_000..=384_000).contains(&u32::from_le_bytes([b[0], b[1], b[2], b[3]])),
        None => true,
    }
}

impl Call {
    /// Runs the receiver once, now: on the second, and whenever the room or a
    /// hearing setting changed.
    pub(super) async fn lossless_tick(self: &Arc<Self>) {
        let room = room(self);
        let outcome = {
            let settings = super::held(&self.lossless.settings);
            super::held(&self.lossless.receiver).tick(now_ms() as u64, &room, &*settings)
        };
        self.lossless_apply(outcome, &room).await;
    }

    /// A packet on a lossless topic, from a participant, read at once and in
    /// the order the room delivered it; what it leads to (a message to send,
    /// a track to switch at the server) is the caller's to wait on.
    ///
    /// Each packet used to be read in a task of its own, and the runtime ran
    /// those tasks in whatever order it liked: the receiver took a packet that
    /// overtook another for a gap and gave the stream up, so a native listener
    /// stopped after anything from none to seventeen blocks (2026-10-17).
    pub(super) fn lossless_heard(self: &Arc<Self>, from: &str, topic: &str, payload: &[u8]) -> (Outcome, Vec<Remote>) {
        let room = room(self);
        if !playable_rate(payload) {
            return (Outcome::default(), room);
        }
        let outcome = {
            let settings = super::held(&self.lossless.settings);
            super::held(&self.lossless.receiver).on_data(now_ms() as u64, from, topic, payload, &room, &*settings)
        };
        (outcome, room)
    }

    pub(super) async fn lossless_apply(self: &Arc<Self>, outcome: Outcome, room: &[Remote]) {
        for out in outcome.send {
            let packet = DataPacket {
                payload: out.payload,
                topic: Some(out.topic.to_string()),
                reliable: true,
                destination_identities: out.to.into_iter().map(ParticipantIdentity).collect(),
                ..Default::default()
            };
            let _ = self.room.local_participant().publish_data(packet).await;
        }
        if !outcome.changed {
            return;
        }
        // Every screen sound and music track as Audible.tsx sets it: off at
        // the server when the listener blocked it or a lossless stream is
        // standing in for it, on otherwise.
        // Settings before the receiver, as everywhere else that holds both:
        // taken the other way round here, a packet being read on the room's
        // event pump and this could each hold one and wait for the other,
        // and every event of the call stopped with them.
        let settings = super::held(&self.lossless.settings);
        let receiver = super::held(&self.lossless.receiver);
        let sink = super::held(&self.lossless.sink).clone();
        for remote in room {
            for publication in &remote.publications {
                let sound = match publication.source {
                    LosslessSource::ScreenShareAudio => Sound::Screen,
                    LosslessSource::Unknown if publication.name.starts_with("music") => Sound::Music,
                    _ => continue,
                };
                let held = receiver.holds_back(&remote.identity, sound);
                let blocked = settings.silenced(&remote.identity, sound);
                if let Some(found) = self.publication(&publication.sid) {
                    found.set_enabled(!(blocked || held));
                }
                if !held {
                    if let Some(sink) = sink.as_ref() {
                        sink.on_lossless_end(remote.identity.clone(), sound == Sound::Music);
                    }
                }
            }
        }
    }
}

/// Ticks the receiver once a second until the call goes away.
pub(super) async fn keep(call: Weak<Call>) {
    let mut tick = tokio::time::interval(Duration::from_secs(1));
    loop {
        tick.tick().await;
        let Some(call) = call.upgrade() else { return };
        call.lossless_tick().await;
    }
}

/// One stream played: the web's playout buffer, drained on a real clock.
struct NativePlayer {
    worklet: Arc<Mutex<Worklet>>,
    pump: tokio::task::JoinHandle<()>,
    alive: Arc<Mutex<bool>>,
}

impl NativePlayer {
    fn start(identity: String, music: bool, rate: u32, channels: usize, sink: Arc<Mutex<Option<Arc<dyn LosslessSink>>>>) -> Self {
        let worklet = Arc::new(Mutex::new(Worklet::new(rate, channels)));
        let alive = Arc::new(Mutex::new(true));
        let (drained, flag) = (worklet.clone(), alive.clone());
        let pump = tokio::spawn(async move {
            let per = (rate / 100).max(1) as usize;
            let mut planes = vec![vec![0f32; per]; channels.max(1)];
            let mut tick = tokio::time::interval(Duration::from_millis(10));
            tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Burst);
            while *super::held(&flag) {
                tick.tick().await;
                super::held(&drained).pull(&mut planes);
                let mut interleaved = Vec::with_capacity(per * planes.len());
                for frame in 0..per {
                    for plane in &planes {
                        interleaved.push(plane[frame]);
                    }
                }
                let target = super::held(&sink).clone();
                if let Some(target) = target {
                    target.on_lossless(identity.clone(), music, interleaved, rate, planes.len() as u32);
                }
            }
        });
        Self { worklet, pump, alive }
    }
}

impl Player for NativePlayer {
    fn push(&mut self, block: Vec<Vec<f32>>) {
        super::held(&self.worklet).push(block);
    }

    fn volume(&mut self, _level: f32) {}

    fn close(&mut self) {
        *super::held(&self.alive) = false;
        self.pump.abort();
    }

    fn counts(&self) -> Option<PlayoutCounts> {
        super::held(&self.worklet).counts()
    }

    fn broken(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::playable_rate;
    use tomoshibi_protocol::lossless::{pack, Block};

    fn packet(rate: u32) -> Vec<u8> {
        pack(&Block { rate, stream: 7, seq: 1, channels: vec![vec![0; 480]] }).expect("packs")
    }

    #[test]
    fn a_rate_nobody_records_at_is_never_played() {
        for rate in [0, 1, 4, 7_999, 384_001, u32::MAX] {
            assert!(!playable_rate(&packet(rate)), "rate {rate} would be played");
        }
        for rate in [8_000, 44_100, 48_000, 96_000, 192_000, 384_000] {
            assert!(playable_rate(&packet(rate)), "rate {rate} would be dropped");
        }
    }
}
