//! Hearing a lossless share: web/src/live/lossless-receiver.test.ts, case for case.
//!
//! Two promises are tested here, and each fails quietly if broken. The first is
//! that what reaches the player is exactly what was sent, in order. The second
//! is that whenever the lossless stream is not playing -- it never started, it
//! stopped, a packet went missing, the player could not start -- the Opus
//! track comes back. Holding Opus back is what keeps one song from playing
//! twice; if it is held back with nothing playing in its place, the listener
//! hears silence and nothing on the screen says why.
//!
//! The web tests run on fake timers with the receiver's one-second interval;
//! [`Harness::advance`] fires the same ticks at the same moments.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use tomoshibi_protocol::lossless::receiver::{MAX_UNDERRUNS, SLOW_WINDOW_MS, STALL_MS};
use tomoshibi_protocol::lossless::{
    from_int24, pack, Block, Hearing, LosslessReceiver, Outcome, Outgoing, Player, PlayoutCounts,
    Publication, Remote, Sound, TrackSource, ASK_TOPIC, FRAMES, LOSSLESS, MUSIC, SHARED, TOPIC,
};

#[derive(Default)]
struct PlayerState {
    blocks: Vec<Vec<Vec<f32>>>,
    closed: bool,
    level: f32,
    broken: bool,
    underruns: u32,
    rate: u32,
}

struct FakePlayer(Arc<Mutex<PlayerState>>);

impl Player for FakePlayer {
    fn push(&mut self, block: Vec<Vec<f32>>) {
        self.0.lock().unwrap().blocks.push(block);
    }
    fn volume(&mut self, level: f32) {
        self.0.lock().unwrap().level = level;
    }
    fn close(&mut self) {
        self.0.lock().unwrap().closed = true;
    }
    fn counts(&self) -> Option<PlayoutCounts> {
        let underruns = self.0.lock().unwrap().underruns;
        Some(PlayoutCounts {
            underruns,
            skipped: 0,
            held: 0,
            played: 0,
            playing: true,
        })
    }
    fn broken(&self) -> bool {
        self.0.lock().unwrap().broken
    }
}

#[derive(Default)]
struct Settings {
    blocked: HashSet<(String, Sound)>,
    volume: HashMap<(String, Sound), f32>,
}

impl Hearing for Settings {
    fn silenced(&self, identity: &str, sound: Sound) -> bool {
        let key = (identity.to_owned(), sound);
        self.blocked.contains(&key) || self.volume.get(&key) == Some(&0.0)
    }
    fn level(&self, identity: &str, sound: Sound) -> f32 {
        *self
            .volume
            .get(&(identity.to_owned(), sound))
            .unwrap_or(&1.0)
    }
}

struct Harness {
    receiver: LosslessReceiver,
    players: Arc<Mutex<Vec<Arc<Mutex<PlayerState>>>>>,
    room: Vec<Remote>,
    hearing: Settings,
    now: u64,
    next_tick: u64,
    sent: Vec<Outgoing>,
    changed: usize,
}

const START: u64 = 1_700_000_000_000;

impl Harness {
    /// The room as `receiveLossless` finds it, which refreshes at once.
    fn with(room: Vec<Remote>, encrypted: bool) -> Self {
        let players: Arc<Mutex<Vec<Arc<Mutex<PlayerState>>>>> = Arc::default();
        let made = players.clone();
        let receiver = LosslessReceiver::new(
            encrypted,
            Box::new(move |_from, _sound, rate, _channels| {
                let state = Arc::new(Mutex::new(PlayerState {
                    rate,
                    level: 1.0,
                    ..PlayerState::default()
                }));
                made.lock().unwrap().push(state.clone());
                Box::new(FakePlayer(state)) as Box<dyn Player>
            }),
        );
        let mut harness = Self {
            receiver,
            players,
            room,
            hearing: Settings::default(),
            now: START,
            next_tick: START + 1000,
            sent: Vec::new(),
            changed: 0,
        };
        harness.refresh();
        harness
    }

    fn new() -> Self {
        Self::with(Vec::new(), false)
    }

    fn apply(&mut self, outcome: Outcome) {
        self.sent.extend(outcome.send);
        self.changed += usize::from(outcome.changed);
    }

    fn refresh(&mut self) {
        let outcome = self.receiver.tick(self.now, &self.room, &self.hearing);
        self.apply(outcome);
    }

    /// The person's only publication becomes this one, as the web tests' share does.
    fn share(&mut self, identity: &str, name: &str, sid: &str) {
        let publication = Publication {
            sid: sid.into(),
            source: TrackSource::ScreenShareAudio,
            name: name.into(),
        };
        self.room.retain(|remote| remote.identity != identity);
        self.room.push(Remote {
            identity: identity.into(),
            publications: vec![publication],
        });
        self.refresh();
    }

    fn data(&mut self, identity: &str, topic: &str, block: Block) {
        let payload = pack(&block).unwrap();
        let outcome = self.receiver.on_data(
            self.now,
            identity,
            topic,
            &payload,
            &self.room,
            &self.hearing,
        );
        self.apply(outcome);
    }

    fn send(&mut self, identity: &str, seq: u32) {
        self.data(
            identity,
            TOPIC,
            Block {
                rate: 48_000,
                stream: 1,
                seq,
                channels: samples(seq),
            },
        );
    }

    fn advance(&mut self, ms: u64) {
        let until = self.now + ms;
        while self.next_tick <= until {
            self.now = self.next_tick;
            self.next_tick += 1000;
            self.refresh();
        }
        self.now = until;
    }

    fn set_blocked(&mut self, identity: &str, sound: Sound, blocked: bool) {
        let key = (identity.to_owned(), sound);
        if blocked {
            self.hearing.blocked.insert(key);
        } else {
            self.hearing.blocked.remove(&key);
        }
        self.refresh();
    }

    /// The asks on the shared sound's topic, as the web tests read them.
    fn asks(&self) -> Vec<(String, bool)> {
        self.sent
            .iter()
            .filter(|packet| packet.topic == ASK_TOPIC)
            .map(|packet| (packet.to[0].clone(), packet.payload == b"1"))
            .collect()
    }

    fn holds_back(&self, identity: &str) -> bool {
        self.receiver.holds_back(identity, Sound::Screen)
    }

    fn player(&self, index: usize) -> Arc<Mutex<PlayerState>> {
        self.players.lock().unwrap()[index].clone()
    }

    fn players_made(&self) -> usize {
        self.players.lock().unwrap().len()
    }
}

/// A block whose samples say which block they belong to.
fn samples(seq: u32) -> Vec<Vec<i32>> {
    (0..2)
        .map(|c| {
            (0..FRAMES)
                .map(|i| (seq as i32 * FRAMES as i32 + i as i32) * if c == 1 { -1 } else { 1 })
                .collect()
        })
        .collect()
}

fn as_floats(channel: &[i32]) -> Vec<f32> {
    channel.iter().map(|&sample| from_int24(sample)).collect()
}

fn yes(to: &str) -> (String, bool) {
    (to.to_owned(), true)
}

fn no(to: &str) -> (String, bool) {
    (to.to_owned(), false)
}

#[test]
fn a_lossless_share_is_asked_for_and_its_opus_track_held_back() {
    let mut h = Harness::new();
    h.share("gfriend-1", LOSSLESS, "TR_one");

    assert_eq!(h.asks(), [yes("gfriend-1")]);
    assert_eq!(h.sent[0].to, ["gfriend-1"]);
    assert!(h.holds_back("gfriend-1"));
    assert!(h.changed > 0);
}

#[test]
fn a_lossless_share_reaches_the_player_sample_for_sample_in_order() {
    let mut h = Harness::new();
    h.share("gfriend-1", LOSSLESS, "TR_one");

    for seq in 40..45 {
        h.send("gfriend-1", seq);
    }

    assert_eq!(h.players_made(), 1);
    let player = h.player(0);
    let player = player.lock().unwrap();
    assert_eq!(player.blocks.len(), 5);
    for (b, block) in player.blocks.iter().enumerate() {
        let expected = samples(40 + b as u32);
        for (c, channel) in block.iter().enumerate() {
            assert!(*channel == as_floats(&expected[c]), "block {b} channel {c}");
        }
    }
}

#[test]
fn a_lossless_share_is_asked_for_again_while_it_lasts_so_the_sender_keeps_sending() {
    let mut h = Harness::new();
    h.share("gfriend-1", LOSSLESS, "TR_one");

    let mut seq = 0;
    for _ in 0..11 {
        for _ in 0..50 {
            h.send("gfriend-1", seq);
            seq += 1;
        }
        h.advance(1000);
    }

    assert!(h.asks().iter().filter(|(_, yes)| *yes).count() >= 3);
    assert!(h.holds_back("gfriend-1"));
}

#[test]
fn gives_up_on_a_packet_missing_from_the_sequence() {
    let mut h = Harness::new();
    h.share("gfriend-1", LOSSLESS, "TR_one");
    let before = h.changed;

    h.send("gfriend-1", 1);
    h.send("gfriend-1", 3);

    assert!(!h.holds_back("gfriend-1"));
    assert!(h.player(0).lock().unwrap().closed);
    assert_eq!(h.asks().last(), Some(&no("gfriend-1")));
    assert!(h.changed > before, "giving Opus back is announced");
}

#[test]
fn gives_up_when_nothing_arrives_after_asking() {
    let mut h = Harness::new();
    h.share("gfriend-1", LOSSLESS, "TR_one");

    h.advance(STALL_MS - 1000);
    assert!(h.holds_back("gfriend-1"));

    h.advance(2000);
    assert!(!h.holds_back("gfriend-1"));
}

#[test]
fn gives_up_when_the_stream_stops() {
    let mut h = Harness::new();
    h.share("gfriend-1", LOSSLESS, "TR_one");

    h.send("gfriend-1", 0);
    h.advance(STALL_MS + 1500);

    assert!(!h.holds_back("gfriend-1"));
}

#[test]
fn gives_up_when_packets_arrive_but_the_player_could_not_start() {
    let mut h = Harness::new();
    h.share("gfriend-1", LOSSLESS, "TR_one");

    h.send("gfriend-1", 0);
    h.player(0).lock().unwrap().broken = true;
    h.send("gfriend-1", 1);
    h.advance(1000);

    assert!(!h.holds_back("gfriend-1"));
}

#[test]
fn gives_up_when_playout_keeps_running_dry_though_packets_still_come() {
    let mut h = Harness::new();
    h.share("gfriend-1", LOSSLESS, "TR_one");
    h.send("gfriend-1", 0);
    let player = h.player(0);

    player.lock().unwrap().underruns = MAX_UNDERRUNS - 1;
    h.send("gfriend-1", 1);
    h.advance(1000);
    assert!(h.holds_back("gfriend-1"));

    player.lock().unwrap().underruns = MAX_UNDERRUNS;
    h.send("gfriend-1", 2);
    h.advance(1000);
    assert!(!h.holds_back("gfriend-1"));
}

#[test]
fn gives_up_when_it_arrives_at_a_fraction_of_real_time_without_waiting_for_it_to_run_dry() {
    let mut h = Harness::new();
    h.share("gfriend-1", LOSSLESS, "TR_one");

    // A packet is 20 ms; one every 200 ms is a tenth of real time.
    let mut seq = 0;
    let mut t = 0;
    while t < SLOW_WINDOW_MS + 1000 {
        h.send("gfriend-1", seq);
        seq += 1;
        h.advance(200);
        t += 200;
    }

    assert!(!h.holds_back("gfriend-1"));
}

#[test]
fn does_not_give_up_when_it_arrives_at_real_time_in_bursts() {
    let mut h = Harness::new();
    h.share("gfriend-1", LOSSLESS, "TR_one");

    // A second's worth at once, every second: lumpy, and all of it.
    let mut seq = 0;
    for _ in 0..12 {
        for _ in 0..50 {
            h.send("gfriend-1", seq);
            seq += 1;
        }
        h.advance(1000);
    }

    assert!(h.holds_back("gfriend-1"));
}

#[test]
fn starts_afresh_when_the_person_shares_again() {
    let mut h = Harness::new();
    h.share("gfriend-1", LOSSLESS, "TR_one");
    h.send("gfriend-1", 1);
    h.send("gfriend-1", 3);
    assert!(!h.holds_back("gfriend-1"));

    h.share("gfriend-1", LOSSLESS, "TR_two");

    assert!(h.holds_back("gfriend-1"));
}

#[test]
fn does_not_ask_in_an_encrypted_call() {
    let mut h = Harness::with(Vec::new(), true);
    h.share("gfriend-1", LOSSLESS, "TR_one");

    assert_eq!(h.asks(), []);
    assert!(!h.holds_back("gfriend-1"));
}

#[test]
fn does_not_ask_for_a_share_that_is_not_offered_losslessly() {
    let mut h = Harness::new();
    h.share("gfriend-1", SHARED.opus, "TR_one");

    assert_eq!(h.asks(), []);
    assert!(!h.holds_back("gfriend-1"));
}

#[test]
fn does_not_ask_for_somebody_whose_sound_this_person_has_turned_off() {
    let mut h = Harness::new();
    h.set_blocked("gfriend-1", Sound::Screen, true);
    h.share("gfriend-1", LOSSLESS, "TR_one");

    assert_eq!(h.asks(), []);
    assert!(!h.holds_back("gfriend-1"));
}

#[test]
fn does_not_ask_for_somebody_at_volume_nought() {
    let mut h = Harness::new();
    h.hearing
        .volume
        .insert(("gfriend-1".into(), Sound::Screen), 0.0);
    h.share("gfriend-1", LOSSLESS, "TR_one");

    assert_eq!(h.asks(), []);
}

#[test]
fn asks_again_every_second_until_the_first_packet_in_case_the_first_ask_arrived_too_early() {
    let mut h = Harness::new();
    h.share("gfriend-1", LOSSLESS, "TR_one");

    h.advance(3000);
    assert!(h.asks().iter().filter(|(_, yes)| *yes).count() >= 3);

    h.send("gfriend-1", 0);
    let before = h.asks().len();
    h.advance(3000);
    h.send("gfriend-1", 1);
    assert_eq!(h.asks().len(), before);
}

#[test]
fn asks_every_five_seconds_once_answered() {
    let mut h = Harness::new();
    h.share("gfriend-1", LOSSLESS, "TR_one");
    h.send("gfriend-1", 0);
    let before = h.asks().len();

    let mut seq = 1;
    for _ in 0..10 {
        for _ in 0..50 {
            h.send("gfriend-1", seq);
            seq += 1;
        }
        h.advance(1000);
    }

    // Asked at the start; then at five and ten seconds.
    assert_eq!(h.asks().len() - before, 2);
}

/// Somebody sharing an application's sound and playing music at once.
fn both() -> Harness {
    let room = vec![Remote {
        identity: "gfriend-1".into(),
        publications: vec![
            Publication {
                sid: "TR_app".into(),
                source: TrackSource::ScreenShareAudio,
                name: LOSSLESS.into(),
            },
            Publication {
                sid: "TR_music".into(),
                source: TrackSource::Unknown,
                name: MUSIC.lossless.into(),
            },
        ],
    }];
    let mut h = Harness::new();
    h.room = room;
    h.refresh();
    h
}

#[test]
fn music_beside_a_shared_sound_is_asked_for_on_its_own_topic_and_played_by_its_own_player() {
    let mut h = both();

    let topics: Vec<&str> = h.sent.iter().map(|packet| packet.topic).collect();
    assert_eq!(topics, [ASK_TOPIC, MUSIC.ask]);

    h.data(
        "gfriend-1",
        MUSIC.topic,
        Block {
            rate: 44_100,
            stream: 7,
            seq: 0,
            channels: samples(0),
        },
    );
    h.data(
        "gfriend-1",
        TOPIC,
        Block {
            rate: 48_000,
            stream: 8,
            seq: 0,
            channels: samples(1),
        },
    );

    assert_eq!(h.players_made(), 2);
    assert!(h.player(0).lock().unwrap().blocks[0][0] == as_floats(&samples(0)[0]));
    assert!(h.player(1).lock().unwrap().blocks[0][0] == as_floats(&samples(1)[0]));
    assert_eq!(h.player(0).lock().unwrap().rate, 44_100);
    assert!(h.receiver.holds_back("gfriend-1", Sound::Music));
    assert!(h.receiver.holds_back("gfriend-1", Sound::Screen));
}

#[test]
fn music_is_let_go_of_on_its_own_when_turned_off_and_the_shared_sound_is_not() {
    let mut h = both();

    h.set_blocked("gfriend-1", Sound::Music, true);

    assert!(!h.receiver.holds_back("gfriend-1", Sound::Music));
    assert!(h.receiver.holds_back("gfriend-1", Sound::Screen));
    assert_eq!(
        h.sent.last().map(|p| (p.topic, p.payload.clone())),
        Some((MUSIC.ask, b"0".to_vec()))
    );
}

#[test]
fn music_turned_back_on_is_asked_for_again() {
    let mut h = both();
    h.set_blocked("gfriend-1", Sound::Music, true);
    h.set_blocked("gfriend-1", Sound::Music, false);

    assert!(h.receiver.holds_back("gfriend-1", Sound::Music));
    assert_eq!(
        h.sent.last().map(|p| (p.topic, p.payload.clone())),
        Some((MUSIC.ask, b"1".to_vec()))
    );
}

#[test]
fn the_musics_level_follows_the_duck_without_a_refresh() {
    let mut h = both();
    h.data(
        "gfriend-1",
        MUSIC.topic,
        Block {
            rate: 48_000,
            stream: 7,
            seq: 0,
            channels: samples(0),
        },
    );
    h.data(
        "gfriend-1",
        TOPIC,
        Block {
            rate: 48_000,
            stream: 8,
            seq: 0,
            channels: samples(0),
        },
    );

    h.hearing
        .volume
        .insert(("gfriend-1".into(), Sound::Music), 0.3);
    h.hearing
        .volume
        .insert(("gfriend-1".into(), Sound::Screen), 0.5);
    h.receiver.relevel_music(&h.hearing);

    assert_eq!(h.player(0).lock().unwrap().level, 0.3);
    assert_eq!(h.player(1).lock().unwrap().level, 1.0);
}

#[test]
fn a_packet_that_is_not_one_is_dropped_and_the_stream_goes_on() {
    let mut h = Harness::new();
    h.share("gfriend-1", LOSSLESS, "TR_one");
    h.send("gfriend-1", 0);

    let outcome = h.receiver.on_data(
        h.now,
        "gfriend-1",
        TOPIC,
        b"not a packet",
        &h.room,
        &h.hearing,
    );
    assert_eq!(outcome, Outcome::default());
    h.send("gfriend-1", 1);

    assert!(h.holds_back("gfriend-1"));
    assert_eq!(h.player(0).lock().unwrap().blocks.len(), 2);
}

#[test]
fn a_packet_nobody_asked_for_is_ignored() {
    let mut h = Harness::new();
    h.room.push(Remote {
        identity: "gfriend-1".into(),
        publications: Vec::new(),
    });
    h.send("gfriend-1", 0);
    assert_eq!(h.players_made(), 0);
}

#[test]
fn somebody_who_leaves_is_let_go_of() {
    let mut h = Harness::new();
    h.share("gfriend-1", LOSSLESS, "TR_one");
    h.send("gfriend-1", 0);
    let before = h.changed;

    h.room.clear();
    h.refresh();

    assert!(!h.holds_back("gfriend-1"));
    assert!(h.player(0).lock().unwrap().closed);
    assert!(h.changed > before);
}

#[test]
fn stopping_closes_every_player_and_withdraws_every_ask() {
    let mut h = both();
    h.data(
        "gfriend-1",
        TOPIC,
        Block {
            rate: 48_000,
            stream: 8,
            seq: 0,
            channels: samples(0),
        },
    );

    let sent = h.receiver.stop();

    assert_eq!(
        sent.iter()
            .map(|p| (p.topic, p.payload.clone()))
            .collect::<Vec<_>>(),
        [(ASK_TOPIC, b"0".to_vec()), (MUSIC.ask, b"0".to_vec())]
    );
    assert!(h.player(0).lock().unwrap().closed);
    assert!(!h.receiver.holds_back("gfriend-1", Sound::Screen));
    h.refresh();
    assert_eq!(h.sent.len(), 2, "nothing more is asked once stopped");
}

#[test]
fn reports_the_players_counts() {
    let mut h = Harness::new();
    h.share("gfriend-1", LOSSLESS, "TR_one");
    assert_eq!(h.receiver.counts("gfriend-1", Sound::Screen), None);
    h.send("gfriend-1", 0);
    h.player(0).lock().unwrap().underruns = 1;
    assert_eq!(
        h.receiver
            .counts("gfriend-1", Sound::Screen)
            .map(|c| c.underruns),
        Some(1)
    );
}
