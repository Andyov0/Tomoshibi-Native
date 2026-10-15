//! Hearing somebody's shared sound without loss. Follows
//! web/src/live/lossless-receiver.ts.
//!
//! For each person whose shared sound or music is offered losslessly (its Opus
//! track is named [`Channel::lossless`]), this asks for the stream, plays it
//! through a [`Player`] of its own, and has the Opus track held back at the
//! media server so the same song is neither downloaded twice nor heard twice a
//! second apart. [`LosslessReceiver::holds_back`] is how the code that applies
//! volumes and blocks learns which Opus tracks to leave off: the web client
//! disables such a publication exactly as it does a blocked one
//! (Audible.tsx), and puts every setting back whenever [`Outcome::changed`].
//!
//! It is lossless or it is nothing. A stream that has not started
//! [`STALL_MS`] after asking, has stopped for that long, has a packet missing
//! from its sequence, keeps running dry, arrives too slowly, or whose player
//! could not start, is given up on for the rest of that share, and the Opus
//! track comes back. A missing packet should not happen on a reliable channel,
//! but if one does, twenty milliseconds of the song have gone, and carrying on
//! would be presenting a stream with a hole in it as the lossless one.
//!
//! Not offered in an encrypted call: the media tracks are encrypted there and
//! the data packets are not, so the stream would cross the relay as plain
//! samples in a call whose promise is that the relay cannot hear it.
//!
//! The media crate drives it: [`LosslessReceiver::tick`] once a second and
//! whenever a track is published or unpublished, a participant joins or
//! leaves, or a hearing setting changes (the events the web client refreshes
//! on); [`LosslessReceiver::on_data`] for every data packet on a lossless
//! topic; [`LosslessReceiver::relevel_music`] whenever the music's duck moves.
//! What comes back in [`Outcome::send`] is published as it says.

use super::format::{from_int24, unpack};
use super::playout::PlayoutCounts;
use super::{Channel, Outgoing, Sound, TrackSource, ASK_EVERY_MS, CHANNELS};

/// Silence after asking, or between packets, that counts as the stream having failed.
pub const STALL_MS: u64 = 4000;

/// Times playout may run dry in one share before the stream is given up on.
///
/// One can be a burst of loss the retransmissions took longer than the buffer
/// to repair. Two means the path cannot carry the stream in real time: from a
/// machine reaching a mainland relay over the open internet the reliable
/// channel delivered about an eighth of the stream's rate, every sample
/// correct and the sound arriving in pieces between long gaps. Packets kept
/// coming, so the stall never fired, and the listener heard that instead of
/// the Opus track, which is built for exactly such a path.
pub const MAX_UNDERRUNS: u32 = 2;

/// The share of real time a stream must arrive at, measured over
/// [`SLOW_WINDOW_MS`].
///
/// The quicker of the two verdicts on a path that cannot carry the stream: a
/// second underrun took ten seconds and more of sound in pieces to arrive on
/// that path, while a stream arriving at an eighth of its rate is plain in
/// five. Half is far below what a working path delivers even through a
/// retransmission stall, so a hiccup is not mistaken for a path that cannot
/// do it.
pub const SLOW_RATIO: f64 = 0.5;
pub const SLOW_WINDOW_MS: u64 = 5000;

/// How often to ask until the first packet comes.
///
/// Far more often than the keep-alive, because an unanswered first ask is the
/// likeliest failure there is: it can cross the publication on the way and
/// arrive before the sender is listening, and waiting the full
/// [`ASK_EVERY_MS`] to repeat it would outlast [`STALL_MS`] and give up on a
/// stream that was fine.
pub const ASK_UNTIL_ANSWERED_MS: u64 = 1000;

/// What plays a stream: the web client's seam between the receiver and its
/// audio context, and the tests' way in.
///
/// The real one plays at the stream's own rate, so nothing between the packet
/// and the sound card resamples it, through a [`super::Worklet`]; eases its
/// gain towards each [`Player::volume`] with a time constant of 30 ms rather
/// than stepping it, because under the music's duck the level moves every few
/// tens of milliseconds and a stepped gain is heard as clicks; and reports
/// [`super::Worklet::counts`], the once-a-second counts, not live ones.
pub trait Player: Send {
    /// One block, one vector per channel.
    fn push(&mut self, block: Vec<Vec<f32>>);
    fn volume(&mut self, level: f32);
    fn close(&mut self);
    /// The counts last reported, if any have been.
    fn counts(&self) -> Option<PlayoutCounts>;
    /// Whether it could not start.
    fn broken(&self) -> bool;
}

/// Makes a player for a stream at a rate, with a number of channels.
pub type MakePlayer = Box<dyn FnMut(u32, usize) -> Box<dyn Player> + Send>;

/// What the listener has decided about hearing people, which this consults on
/// every refresh: the web client's hearing.ts, and duck.ts for the music.
pub trait Hearing {
    /// Blocked, or at volume nought: such a sound is not asked for at all.
    fn silenced(&self, identity: &str, sound: Sound) -> bool;
    /// How loud a stream plays: the listener's volume, and for music the
    /// duck's factor as well.
    fn level(&self, identity: &str, sound: Sound) -> f32;
}

/// One remote participant as the room has them now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Remote {
    pub identity: String,
    pub publications: Vec<Publication>,
}

/// One of a remote participant's track publications.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Publication {
    /// The track's sid: a new share is a new publication and a fresh start.
    pub sid: String,
    pub source: TrackSource,
    pub name: String,
}

/// What a call into the receiver wants done.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Outcome {
    /// Asks to publish, in order.
    pub send: Vec<Outgoing>,
    /// Which Opus tracks to hold back has changed: put the settings back.
    pub changed: bool,
}

struct Source {
    identity: String,
    channel: &'static Channel,
    publication: String,
    wanted: bool,
    failed: bool,
    asked_at: u64,
    last_ask: u64,
    last_packet: Option<u64>,
    stream: Option<u32>,
    next_seq: Option<u64>,
    player: Option<Box<dyn Player>>,
    /// Frames arrived since `window_start`, against the rate they should arrive at.
    window_start: Option<u64>,
    window_frames: u64,
    rate: u32,
}

impl Source {
    fn new(identity: &str, channel: &'static Channel, publication: &str) -> Self {
        Self {
            identity: identity.to_owned(),
            channel,
            publication: publication.to_owned(),
            wanted: false,
            failed: false,
            asked_at: 0,
            last_ask: 0,
            last_packet: None,
            stream: None,
            next_seq: None,
            player: None,
            window_start: None,
            window_frames: 0,
            rate: 0,
        }
    }

    fn close(&mut self) {
        if let Some(mut player) = self.player.take() {
            player.close();
        }
    }

    fn ask(&self, yes: bool) -> Outgoing {
        Outgoing {
            topic: self.channel.ask,
            to: vec![self.identity.clone()],
            payload: if yes { b"1".to_vec() } else { b"0".to_vec() },
        }
    }

    fn give_up(&mut self, send: &mut Vec<Outgoing>) {
        self.failed = true;
        self.close();
        send.push(self.ask(false));
    }
}

/// Every lossless stream this participant hears in one room.
pub struct LosslessReceiver {
    /// Off in an encrypted call, and once stopped: nothing is asked for and
    /// nothing held back.
    enabled: bool,
    /// By person and by channel, in the order they were first seen, which is
    /// the web client's Map order.
    sources: Vec<Source>,
    make_player: MakePlayer,
}

impl LosslessReceiver {
    /// A receiver for one room. `encrypted` is whether the call is: then this
    /// never asks and never holds anything back.
    pub fn new(encrypted: bool, make_player: MakePlayer) -> Self {
        Self {
            enabled: !encrypted,
            sources: Vec::new(),
            make_player,
        }
    }

    fn position(&self, identity: &str, sound: Sound) -> Option<usize> {
        self.sources
            .iter()
            .position(|source| source.identity == identity && source.channel.sound == sound)
    }

    /// Whether this person's Opus track for a sound should be held back,
    /// because the lossless one is playing.
    pub fn holds_back(&self, identity: &str, sound: Sound) -> bool {
        self.position(identity, sound)
            .is_some_and(|i| self.sources[i].wanted)
    }

    /// How a person's lossless stream is doing, if one is being played.
    pub fn counts(&self, identity: &str, sound: Sound) -> Option<PlayoutCounts> {
        let source = &self.sources[self.position(identity, sound)?];
        source.player.as_ref()?.counts()
    }

    /// Look at the room again: who offers a lossless stream, who is to be
    /// asked, which streams have failed. `now` is in milliseconds.
    pub fn tick(&mut self, now: u64, room: &[Remote], hearing: &dyn Hearing) -> Outcome {
        let mut outcome = Outcome::default();
        if self.enabled {
            self.refresh(now, room, hearing, &mut outcome);
        }
        outcome
    }

    /// A data packet arrived from `from`, who the caller has found in the
    /// room; packets with no known sender are dropped before this, as the web
    /// client drops them.
    pub fn on_data(
        &mut self,
        now: u64,
        from: &str,
        topic: &str,
        payload: &[u8],
        room: &[Remote],
        hearing: &dyn Hearing,
    ) -> Outcome {
        let mut outcome = Outcome::default();
        if !self.enabled {
            return outcome;
        }
        let Some(channel) = Channel::by_topic(topic) else {
            return outcome;
        };
        let Some(i) = self.position(from, channel.sound) else {
            return outcome;
        };
        if !self.sources[i].wanted {
            return outcome;
        }

        // A packet that is not one of ours is dropped, and the stream goes on.
        let Ok(block) = unpack(payload) else {
            return outcome;
        };

        let source = &mut self.sources[i];
        if source.stream != Some(block.stream) {
            source.close();
            let mut player = (self.make_player)(block.rate, block.channels.len());
            player.volume(hearing.level(from, channel.sound));
            source.player = Some(player);
            source.stream = Some(block.stream);
            source.next_seq = Some(u64::from(block.seq));
            source.rate = block.rate;
            source.window_start = Some(now);
            source.window_frames = 0;
        }

        if source.next_seq != Some(u64::from(block.seq)) {
            source.give_up(&mut outcome.send);
            self.refresh(now, room, hearing, &mut outcome);
            return outcome;
        }

        // A JavaScript number in the web client, so it does not wrap; a
        // stream long enough to reach 2^32 packets would end there in both.
        source.next_seq = Some(u64::from(block.seq) + 1);
        source.last_packet = Some(now);
        source.window_frames += block.channels.first().map_or(0, Vec::len) as u64;
        if let Some(player) = source.player.as_mut() {
            player.push(
                block
                    .channels
                    .iter()
                    .map(|channel| channel.iter().map(|&sample| from_int24(sample)).collect())
                    .collect(),
            );
        }

        outcome
    }

    /// The music's level moves under speech many times a second; only the
    /// players' volumes follow it, not the whole of a refresh.
    pub fn relevel_music(&mut self, hearing: &dyn Hearing) {
        if !self.enabled {
            return;
        }
        for source in &mut self.sources {
            if source.channel.sound == Sound::Music {
                if let Some(player) = source.player.as_mut() {
                    player.volume(hearing.level(&source.identity, Sound::Music));
                }
            }
        }
    }

    /// Stop listening: every player closed, every stream still wanted told no.
    pub fn stop(&mut self) -> Vec<Outgoing> {
        let mut send = Vec::new();
        for source in &mut self.sources {
            source.close();
            if source.wanted {
                send.push(source.ask(false));
            }
        }
        self.sources.clear();
        self.enabled = false;
        send
    }

    fn refresh(&mut self, now: u64, room: &[Remote], hearing: &dyn Hearing, outcome: &mut Outcome) {
        let mut moved = false;

        for participant in room {
            for channel in &CHANNELS {
                let identity = participant.identity.as_str();
                let share = participant.publications.iter().find(|publication| {
                    publication.source == channel.source && publication.name == channel.lossless
                });
                let existing = self.position(identity, channel.sound);

                let Some(share) = share else {
                    if let Some(i) = existing {
                        let mut source = self.sources.remove(i);
                        source.close();
                        moved |= source.wanted;
                    }
                    continue;
                };

                let i = match existing {
                    Some(i) if self.sources[i].publication == share.sid => i,
                    // Replaced where it stands, as a Map keeps a key's place.
                    Some(i) => {
                        self.sources[i].close();
                        self.sources[i] = Source::new(identity, channel, &share.sid);
                        i
                    }
                    None => {
                        self.sources
                            .push(Source::new(identity, channel, &share.sid));
                        self.sources.len() - 1
                    }
                };
                let source = &mut self.sources[i];

                let mut want = !source.failed && !hearing.silenced(identity, channel.sound);

                // Packets arriving for a player that cannot play is the same
                // failure as packets not arriving, and must come back to Opus
                // just as surely: otherwise the Opus track stays held back and
                // nothing is heard.
                let since = source.asked_at.max(source.last_packet.unwrap_or(0));
                let stalled = now.saturating_sub(since) > STALL_MS;
                let starved = source
                    .player
                    .as_ref()
                    .and_then(|player| player.counts())
                    .map_or(0, |counts| counts.underruns)
                    >= MAX_UNDERRUNS;

                let mut slow = false;
                if let Some(start) = source.window_start {
                    let elapsed = now.saturating_sub(start);
                    if elapsed >= SLOW_WINDOW_MS {
                        let due = (elapsed as f64 / 1000.0) * f64::from(source.rate);
                        slow = (source.window_frames as f64) < SLOW_RATIO * due;
                        source.window_start = Some(now);
                        source.window_frames = 0;
                    }
                }

                let broken = source.player.as_ref().is_some_and(|player| player.broken());
                let repeat = if source.last_packet.is_some() {
                    ASK_EVERY_MS
                } else {
                    ASK_UNTIL_ANSWERED_MS
                };

                if want && source.wanted && (stalled || starved || slow || broken) {
                    source.give_up(&mut outcome.send);
                    want = false;
                } else if want && !source.wanted {
                    source.asked_at = now;
                    source.last_ask = now;
                    outcome.send.push(source.ask(true));
                } else if want && now.saturating_sub(source.last_ask) >= repeat {
                    source.last_ask = now;
                    outcome.send.push(source.ask(true));
                } else if !want && source.wanted && !source.failed {
                    source.close();
                    source.stream = None;
                    outcome.send.push(source.ask(false));
                }

                if want != source.wanted {
                    source.wanted = want;
                    moved = true;
                }

                if let Some(player) = source.player.as_mut() {
                    player.volume(hearing.level(identity, channel.sound));
                }
            }
        }

        self.sources.retain_mut(|source| {
            if room
                .iter()
                .any(|participant| participant.identity == source.identity)
            {
                return true;
            }
            source.close();
            moved |= source.wanted;
            false
        });

        outcome.changed |= moved;
    }
}
