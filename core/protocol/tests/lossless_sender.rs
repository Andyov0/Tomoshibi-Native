//! Sending a lossless share: web/src/live/lossless-sender.test.ts, case for case.
//!
//! The capture is fed in as a platform feeds it -- ten-millisecond chunks that
//! do not line up with the packet size -- and what is checked is what leaves:
//! that it goes only to people who asked, that it decodes to exactly the
//! samples fed in, that a hole in the capture is counted rather than smoothed
//! over, and that an upload that cannot keep up ends the stream instead of
//! queueing forever.
//!
//! The web tests' stand-in room publishes each packet as soon as it is handed
//! over, unless it is the slow one that never finishes; [`Room`] here drains
//! the queue the same two ways.

use std::collections::HashSet;

use tomoshibi_protocol::lossless::{
    to_int24, unpack, Block, LosslessSender, Outgoing, ASK_TOPIC, FRAMES, MUSIC, SHARED, TOPIC,
};

const START: u64 = 1_700_000_000_000;

struct Room {
    present: HashSet<String>,
    sent: Vec<Outgoing>,
    /// A connection that never finishes a send: one packet is taken and the
    /// rest wait behind it.
    slow: bool,
    in_flight: bool,
    gave_up: usize,
    now: u64,
}

impl Room {
    fn new(slow: bool) -> Self {
        Self {
            present: HashSet::new(),
            sent: Vec::new(),
            slow,
            in_flight: false,
            gave_up: 0,
            now: START,
        }
    }

    fn join(&mut self, identity: &str) {
        self.present.insert(identity.to_owned());
    }

    fn ask(&mut self, sender: &mut LosslessSender, identity: &str, yes: bool, topic: &str) {
        sender.on_ask(self.now, identity, topic, if yes { b"1" } else { b"0" });
    }

    /// One chunk of capture, then the queue drained as this room drains it.
    fn feed(&mut self, sender: &mut LosslessSender, planes: &[Vec<f32>], timestamp: i64) {
        let present = &self.present;
        let captured = sender
            .capture(self.now, 48_000, timestamp, planes, |identity| {
                present.contains(identity)
            })
            .expect("captures");
        self.gave_up += usize::from(captured.gave_up);

        if self.slow {
            if !self.in_flight {
                if let Some(packet) = sender.next_packet() {
                    self.in_flight = true;
                    self.sent.push(packet);
                }
            }
        } else {
            while let Some(packet) = sender.next_packet() {
                self.sent.push(packet);
            }
        }
    }

    /// Feed `chunks` ten-millisecond chunks, contiguous, starting at chunk
    /// `from`, with a hole before chunk `skip_at`.
    fn capture(
        &mut self,
        sender: &mut LosslessSender,
        chunks: usize,
        from: usize,
        skip_at: Option<usize>,
    ) {
        let mut timestamp = from as i64 * 10_000;
        for n in from..from + chunks {
            if Some(n) == skip_at {
                timestamp += 10_000;
            }
            self.feed(sender, &chunk(n * 480, 480), timestamp);
            timestamp += 10_000;
        }
    }

    fn sent_on(&self, topic: &str) -> Vec<&Outgoing> {
        self.sent
            .iter()
            .filter(|packet| packet.topic == topic)
            .collect()
    }
}

/// The web tests' capture: a sawtooth, different in each channel, as floats.
fn value(at: usize, c: usize) -> f32 {
    ((((at % 5000) as f64) - 2500.0) / if c == 1 { 4096.0 } else { 8192.0 }) as f32
}

fn chunk(start: usize, frames: usize) -> Vec<Vec<f32>> {
    (0..2)
        .map(|c| (0..frames).map(|i| value(start + i, c)).collect())
        .collect()
}

fn decode(packet: &Outgoing) -> Block {
    unpack(&packet.payload).expect("a packet")
}

#[test]
fn sends_nothing_until_somebody_asks_then_only_to_them() {
    let mut room = Room::new(false);
    room.join("gfriend-1");
    room.join("gother-2");
    let mut sender = LosslessSender::captured(&SHARED);

    room.capture(&mut sender, 4, 0, None);
    assert!(room.sent.is_empty());

    room.ask(&mut sender, "gfriend-1", true, ASK_TOPIC);
    room.capture(&mut sender, 4, 4, None);

    assert_eq!(room.sent.len(), 2);
    for packet in &room.sent {
        assert_eq!(packet.to, ["gfriend-1"]);
        assert_eq!(packet.topic, TOPIC);
    }
}

#[test]
fn sends_exactly_the_samples_it_was_given_in_consecutive_packets() {
    let mut room = Room::new(false);
    room.join("gfriend-1");
    let mut sender = LosslessSender::captured(&SHARED);
    room.ask(&mut sender, "gfriend-1", true, ASK_TOPIC);

    room.capture(&mut sender, 10, 0, None);

    assert_eq!(room.sent.len(), 5);
    for (b, packet) in room.sent.iter().enumerate() {
        let block = decode(packet);
        assert_eq!(block.seq, b as u32);
        assert_eq!(block.rate, 48_000);
        for (c, channel) in block.channels.iter().enumerate() {
            let expected: Vec<i32> = (0..FRAMES)
                .map(|i| to_int24(value(b * FRAMES + i, c)))
                .collect();
            assert!(*channel == expected, "block {b} channel {c}");
        }
    }
    assert_eq!(sender.gaps(), 0);
}

#[test]
fn stops_sending_to_somebody_who_withdraws() {
    let mut room = Room::new(false);
    room.join("gfriend-1");
    let mut sender = LosslessSender::captured(&SHARED);
    room.ask(&mut sender, "gfriend-1", true, ASK_TOPIC);
    room.capture(&mut sender, 2, 0, None);
    assert_eq!(room.sent.len(), 1);

    room.ask(&mut sender, "gfriend-1", false, ASK_TOPIC);
    room.capture(&mut sender, 4, 2, None);
    assert_eq!(room.sent.len(), 1);
}

#[test]
fn stops_sending_to_somebody_who_leaves() {
    let mut room = Room::new(false);
    room.join("gfriend-1");
    let mut sender = LosslessSender::captured(&SHARED);
    room.ask(&mut sender, "gfriend-1", true, ASK_TOPIC);
    room.capture(&mut sender, 2, 0, None);

    sender.on_left("gfriend-1");
    room.capture(&mut sender, 4, 2, None);
    assert_eq!(room.sent.len(), 1);
}

#[test]
fn counts_a_hole_in_the_capture_rather_than_passing_over_it() {
    let mut room = Room::new(false);
    room.join("gfriend-1");
    let mut sender = LosslessSender::captured(&SHARED);
    room.ask(&mut sender, "gfriend-1", true, ASK_TOPIC);

    room.capture(&mut sender, 6, 0, Some(3));

    assert_eq!(sender.gaps(), 1);
}

#[test]
fn gives_up_once_when_the_upload_cannot_keep_up() {
    let mut room = Room::new(true);
    room.join("gfriend-1");
    let mut sender = LosslessSender::captured(&SHARED);
    room.ask(&mut sender, "gfriend-1", true, ASK_TOPIC);

    // Four seconds of capture into a connection that never finishes a send.
    room.capture(&mut sender, 400, 0, None);

    assert_eq!(room.gave_up, 1);
    assert!(sender.stopped());
    assert_eq!(sender.queued(), 0);
}

#[test]
fn gives_up_past_three_seconds_waiting_and_not_at_three() {
    // 150 packets of 20 ms waiting is three seconds at 48 kHz: still going.
    // The 151st behind the one in flight is past it.
    let mut room = Room::new(true);
    room.join("gfriend-1");
    let mut sender = LosslessSender::captured(&SHARED);
    room.ask(&mut sender, "gfriend-1", true, ASK_TOPIC);

    room.capture(&mut sender, 2 * 151, 0, None);
    assert_eq!((room.gave_up, sender.queued()), (0, 150));

    room.capture(&mut sender, 2, 2 * 151, None);
    assert_eq!(room.gave_up, 1);
}

#[test]
fn keeps_going_on_an_upload_that_keeps_up() {
    let mut room = Room::new(false);
    room.join("gfriend-1");
    let mut sender = LosslessSender::captured(&SHARED);
    room.ask(&mut sender, "gfriend-1", true, ASK_TOPIC);
    room.capture(&mut sender, 400, 0, None);
    assert_eq!((room.gave_up, room.sent.len()), (0, 200));
}

#[test]
fn stops_sending_to_somebody_no_longer_in_the_room_and_to_an_ask_not_repeated() {
    let mut room = Room::new(false);
    room.join("gfriend-1");
    room.join("gother-2");
    let mut sender = LosslessSender::captured(&SHARED);
    room.ask(&mut sender, "gfriend-1", true, ASK_TOPIC);
    room.ask(&mut sender, "gother-2", true, ASK_TOPIC);

    // Gone without a word: no disconnect event, just no longer listed.
    room.present.remove("gother-2");
    room.capture(&mut sender, 2, 0, None);
    assert_eq!(
        room.sent.last().map(|p| p.to.clone()),
        Some(vec!["gfriend-1".to_owned()])
    );

    // Fifteen seconds and more since the last ask.
    room.now += 16_000;
    room.capture(&mut sender, 2, 2, None);
    assert_eq!(room.sent.len(), 1);
}

#[test]
fn an_ask_lasts_fifteen_seconds_and_not_a_moment_more() {
    let mut room = Room::new(false);
    room.join("gfriend-1");
    let mut sender = LosslessSender::captured(&SHARED);
    room.ask(&mut sender, "gfriend-1", true, ASK_TOPIC);

    room.now += 15_000;
    room.capture(&mut sender, 2, 0, None);
    assert_eq!(room.sent.len(), 1);

    room.now += 1;
    room.capture(&mut sender, 2, 2, None);
    assert_eq!(room.sent.len(), 1);
}

#[test]
fn sends_to_everybody_who_asked_in_the_order_they_first_asked() {
    let mut room = Room::new(false);
    room.join("gfriend-1");
    room.join("gother-2");
    let mut sender = LosslessSender::captured(&SHARED);
    room.ask(&mut sender, "gother-2", true, ASK_TOPIC);
    room.ask(&mut sender, "gfriend-1", true, ASK_TOPIC);
    room.ask(&mut sender, "gother-2", true, ASK_TOPIC);

    room.capture(&mut sender, 2, 0, None);
    assert_eq!(room.sent[0].to, ["gother-2", "gfriend-1"]);
}

#[test]
fn takes_an_ask_with_a_byte_order_mark_as_a_browser_does() {
    let mut room = Room::new(false);
    room.join("gfriend-1");
    let mut sender = LosslessSender::captured(&SHARED);
    sender.on_ask(START, "gfriend-1", ASK_TOPIC, b"\xEF\xBB\xBF1");
    room.capture(&mut sender, 2, 0, None);
    assert_eq!(room.sent.len(), 1);

    // Anything else is a no.
    sender.on_ask(START, "gfriend-1", ASK_TOPIC, b"1 ");
    room.capture(&mut sender, 2, 2, None);
    assert_eq!(room.sent.len(), 1);
}

#[test]
fn turns_floats_into_samples_the_way_it_was_told_to() {
    let mut room = Room::new(false);
    room.join("gfriend-1");
    let mut sender = LosslessSender::new(&SHARED, |_| 4242);
    room.ask(&mut sender, "gfriend-1", true, ASK_TOPIC);

    room.capture(&mut sender, 2, 0, None);

    let block = decode(&room.sent[0]);
    assert!(block.channels[0].iter().all(|&sample| sample == 4242));
}

#[test]
fn a_sample_it_cannot_send_ends_the_reading_as_in_the_browser() {
    let mut room = Room::new(false);
    room.join("gfriend-1");
    let mut sender = LosslessSender::new(&SHARED, |_| 1 << 24);
    room.ask(&mut sender, "gfriend-1", true, ASK_TOPIC);

    let present = room.present.clone();
    let first = sender.capture(START, 48_000, 0, &chunk(0, 960), |id| present.contains(id));
    assert!(first.is_err());
    let next = sender.capture(START, 48_000, 20_000, &chunk(960, 960), |id| {
        present.contains(id)
    });
    assert_eq!(next.map(|c| c.queued).ok(), Some(0));
}

#[test]
fn as_the_song_desks_music_answers_only_asks_for_the_music_and_only_on_the_musics_topic() {
    let mut room = Room::new(false);
    room.join("gfriend-1");
    let mut sender = LosslessSender::captured(&MUSIC);

    room.ask(&mut sender, "gfriend-1", true, ASK_TOPIC);
    room.capture(&mut sender, 2, 0, None);
    assert!(room.sent.is_empty());

    room.ask(&mut sender, "gfriend-1", true, MUSIC.ask);
    room.capture(&mut sender, 2, 2, None);
    assert_eq!(room.sent.len(), 1);
    assert_eq!(room.sent_on(MUSIC.topic).len(), 1);
    assert!(room.sent_on(TOPIC).is_empty());
}

#[test]
fn a_change_of_format_is_a_new_stream_from_the_start() {
    let mut room = Room::new(false);
    room.join("gfriend-1");
    let mut ids = vec![7u32, 8].into_iter();
    let mut sender = LosslessSender::captured(&SHARED).with_streams(move || ids.next().unwrap());
    room.ask(&mut sender, "gfriend-1", true, ASK_TOPIC);
    room.capture(&mut sender, 3, 0, None);

    // Half a block made at 48 kHz is dropped, not sent at 44.1 kHz.
    let present = room.present.clone();
    let planes = chunk(0, 960);
    let captured = sender
        .capture(START, 44_100, 0, &planes, |id| present.contains(id))
        .unwrap();
    assert_eq!(captured.queued, 1);
    room.sent.push(sender.next_packet().unwrap());

    let blocks: Vec<Block> = room.sent.iter().map(decode).collect();
    assert_eq!(
        blocks
            .iter()
            .map(|b| (b.stream, b.seq, b.rate))
            .collect::<Vec<_>>(),
        [(7, 0, 48_000), (8, 0, 44_100)]
    );
    assert_eq!(blocks[1].channels[0][0], to_int24(value(0, 0)));
}

#[test]
fn stopped_it_sends_and_hears_nothing() {
    let mut room = Room::new(false);
    room.join("gfriend-1");
    let mut sender = LosslessSender::captured(&SHARED);
    room.ask(&mut sender, "gfriend-1", true, ASK_TOPIC);
    sender.stop();
    room.ask(&mut sender, "gfriend-1", true, ASK_TOPIC);
    room.capture(&mut sender, 4, 0, None);
    assert!(room.sent.is_empty());
}
