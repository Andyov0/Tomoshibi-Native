//! The connection meter (web/src/live/connection.test.ts, and the rest of what
//! connection.ts does with a stats report).
//!
//! What these guard is a light that says the wrong thing. A connection light is
//! only useful if it can be trusted at a glance, and the way it stops being
//! trustworthy is by being kind: three green bars on a call that is breaking up
//! sends somebody looking at their microphone, their browser and their
//! colleague before they doubt the indicator. So the bands are asserted at their
//! edges, the server's verdict may only make the reading worse, and the share
//! row is followed sample after sample, because its faults were in the merge.

use std::{
    net::{IpAddr, SocketAddr},
    sync::{Arc, Mutex},
    time::Duration,
};

use tokio::net::UdpSocket;
use tomoshibi_protocol::quality::{
    self, connection_grade as grade, ConnectionGrade::*, ConnectionMeter, ServerQuality, ShareLimitation,
    StatsEntry, TrackReport,
};

const FINE: ServerQuality = ServerQuality::Excellent;

#[test]
fn is_green_on_a_near_clean_connection() {
    assert_eq!(grade(FINE, Some(30.0), Some(0.0)), Good);
    assert_eq!(grade(FINE, Some(149.0), Some(1.9)), Good);
    // The edges themselves are still inside.
    assert_eq!(grade(FINE, Some(150.0), Some(2.0)), Good);
}

// Distance alone. Nothing is being lost, and the call is still hard work.
#[test]
fn turns_amber_on_distance_before_anything_is_lost() {
    assert_eq!(grade(FINE, Some(151.0), Some(0.0)), Fair);
    assert_eq!(grade(FINE, Some(399.0), Some(0.0)), Fair);
    assert_eq!(grade(FINE, Some(400.0), Some(0.0)), Fair);
}

#[test]
fn turns_red_when_a_conversation_would_stop_working() {
    assert_eq!(grade(FINE, Some(401.0), Some(0.0)), Poor);
    assert_eq!(grade(FINE, Some(20.0), Some(8.1)), Poor);
    assert_eq!(grade(FINE, Some(20.0), Some(8.0)), Fair);
}

// Loss matters at lower numbers than people expect: two per cent is audible.
#[test]
fn turns_amber_on_loss_a_long_way_below_what_sounds_like_a_lot() {
    assert_eq!(grade(FINE, Some(20.0), Some(2.1)), Fair);
}

// The worst of the two, not the average.
#[test]
fn takes_the_worse_of_distance_and_loss() {
    assert_eq!(grade(FINE, Some(30.0), Some(9.0)), Poor);
    assert_eq!(grade(FINE, Some(500.0), Some(0.0)), Poor);
    assert_eq!(grade(FINE, Some(200.0), Some(3.0)), Fair);
    assert_eq!(grade(FINE, Some(500.0), Some(3.0)), Poor);
}

#[test]
fn says_nothing_bad_before_anything_has_been_measured() {
    assert_eq!(grade(FINE, None, None), Good);

    let unmeasured = quality::connection_unmeasured();
    assert_eq!(unmeasured.grade, Good);
    assert!(!unmeasured.measured);
}

// The server can pull the reading down and must never push it up, or a broken
// call gets a clean bill of health.
#[test]
fn lets_the_server_make_it_worse_and_never_better() {
    assert_eq!(grade(ServerQuality::Poor, Some(20.0), Some(0.0)), Fair);
    assert_eq!(grade(ServerQuality::Lost, Some(20.0), Some(0.0)), Lost);

    // Already worse than the server thinks: the local numbers stand.
    assert_eq!(grade(ServerQuality::Excellent, Some(900.0), Some(20.0)), Poor);
    assert_eq!(grade(ServerQuality::Poor, Some(900.0), None), Poor);
    assert_eq!(grade(ServerQuality::Unknown, Some(20.0), None), Good);
}

fn outbound(id: &str, ssrc: u32) -> StatsEntry {
    StatsEntry::OutboundRtp {
        id: id.into(),
        ssrc: Some(ssrc),
        track_identifier: None,
        bytes_sent: Some(0.0),
        packets_sent: Some(0.0),
        frame_width: None,
        frame_height: None,
        frames_per_second: None,
        quality_limitation_reason: None,
    }
}

fn because(entry: StatsEntry, reason: &str) -> StatsEntry {
    match entry {
        StatsEntry::OutboundRtp { id, ssrc, track_identifier, bytes_sent, packets_sent, frame_width, frame_height, frames_per_second, .. } => {
            StatsEntry::OutboundRtp {
                id,
                ssrc,
                track_identifier,
                bytes_sent,
                packets_sent,
                frame_width,
                frame_height,
                frames_per_second,
                quality_limitation_reason: Some(reason.into()),
            }
        }
        other => other,
    }
}

fn sized(entry: StatsEntry, width: f64, height: f64, fps: Option<f64>) -> StatsEntry {
    match entry {
        StatsEntry::OutboundRtp { id, ssrc, track_identifier, bytes_sent, packets_sent, quality_limitation_reason, .. } => {
            StatsEntry::OutboundRtp {
                id,
                ssrc,
                track_identifier,
                bytes_sent,
                packets_sent,
                frame_width: Some(width),
                frame_height: Some(height),
                frames_per_second: fps,
                quality_limitation_reason,
            }
        }
        StatsEntry::InboundRtp { id, ssrc, track_identifier, bytes_received, .. } => StatsEntry::InboundRtp {
            id,
            ssrc,
            track_identifier,
            bytes_received,
            frame_width: Some(width),
            frame_height: Some(height),
            frames_per_second: fps,
        },
        other => other,
    }
}

fn inbound(id: &str, ssrc: u32, bytes: f64) -> StatsEntry {
    StatsEntry::InboundRtp {
        id: id.into(),
        ssrc: Some(ssrc),
        track_identifier: None,
        bytes_received: Some(bytes),
        frame_width: None,
        frame_height: None,
        frames_per_second: None,
    }
}

fn sent(id: &str, bytes: f64, packets: f64) -> StatsEntry {
    StatsEntry::OutboundRtp {
        id: id.into(),
        ssrc: None,
        track_identifier: None,
        bytes_sent: Some(bytes),
        packets_sent: Some(packets),
        frame_width: None,
        frame_height: None,
        frames_per_second: None,
        quality_limitation_reason: None,
    }
}

fn far_end(id: &str, lost: f64, jitter: Option<f64>, rtt: Option<f64>) -> StatsEntry {
    StatsEntry::RemoteInboundRtp { id: id.into(), packets_lost: Some(lost), jitter, round_trip_time: rtt }
}

fn pair(id: &str, state: &str, nominated: bool, rtt: Option<f64>, local: &str) -> StatsEntry {
    StatsEntry::CandidatePair {
        id: id.into(),
        state: Some(state.into()),
        nominated: Some(nominated),
        current_round_trip_time: rtt,
        local_candidate_id: Some(local.into()),
    }
}

fn candidate(id: &str, address: &str, kind: &str, related: Option<&str>, url: Option<&str>) -> StatsEntry {
    StatsEntry::LocalCandidate {
        id: id.into(),
        address: Some(address.into()),
        candidate_type: Some(kind.into()),
        related_address: related.map(Into::into),
        url: url.map(Into::into),
        relay_protocol: (kind == "relay").then(|| "udp".to_owned()),
    }
}

/// This device's screen share, with one report.
fn sharing(entries: Vec<StatsEntry>) -> Vec<TrackReport> {
    vec![TrackReport { local: true, screen_share: true, entries }]
}

/// Takes the samples a meter would take every two seconds.
struct Clock {
    meter: Arc<ConnectionMeter>,
    at: u64,
}

impl Clock {
    fn new() -> Clock {
        Clock { meter: ConnectionMeter::new(), at: 1_000_000 }
    }

    fn sample(&mut self, reports: Vec<TrackReport>) -> quality::ConnectionReading {
        let reading = self.meter.fold(self.at, quality::gather(&reports), None, None);
        self.at += quality::connection_sample_every_ms();
        reading
    }

    fn limited(&mut self, entry: StatsEntry) -> Option<ShareLimitation> {
        self.sample(sharing(vec![entry])).share.expect("a share row").limited
    }
}

/*
 * What the share row says about why it is held back, sample after sample.
 *
 * The row carried the last reason forward whenever a sample had none to give,
 * which was meant for a sample taken while the encoder was reconfiguring, and
 * it treated "none" as no reason to give. So a share held back by the CPU for
 * ten seconds went on saying so for the rest of the call.
 */

#[test]
fn clears_the_warning_when_the_stack_says_the_limitation_is_gone() {
    let mut clock = Clock::new();

    assert_eq!(clock.limited(because(outbound("o1", 1), "cpu")), Some(ShareLimitation::Cpu));
    assert_eq!(clock.limited(because(outbound("o1", 1), "none")), None);
}

#[test]
fn switches_from_one_reason_to_another() {
    let mut clock = Clock::new();

    assert_eq!(clock.limited(because(outbound("o1", 1), "bandwidth")), Some(ShareLimitation::Bandwidth));
    assert_eq!(clock.limited(because(outbound("o1", 1), "cpu")), Some(ShareLimitation::Cpu));
    assert_eq!(clock.limited(because(outbound("o1", 1), "other")), Some(ShareLimitation::Other));
    assert_eq!(clock.limited(because(outbound("o1", 1), "none")), None);
}

#[test]
fn keeps_a_reason_across_a_sample_that_did_not_say_and_not_for_ever() {
    let mut clock = Clock::new();

    assert_eq!(clock.limited(because(outbound("o1", 1), "cpu")), Some(ShareLimitation::Cpu));

    // A gap in the measurement, not a recovery: two samples of it.
    assert_eq!(clock.limited(outbound("o1", 1)), Some(ShareLimitation::Cpu));
    assert_eq!(clock.limited(outbound("o1", 1)), Some(ShareLimitation::Cpu));

    // But a stack that has stopped saying is not still saying "cpu".
    assert_eq!(clock.limited(outbound("o1", 1)), None);
    assert_eq!(clock.limited(outbound("o1", 1)), None);
}

#[test]
fn does_not_hand_one_shares_reason_to_the_next_one() {
    let mut clock = Clock::new();

    assert_eq!(clock.limited(because(outbound("o1", 1), "cpu")), Some(ShareLimitation::Cpu));
    // A different track, which happens to have no reason on its first report.
    assert_eq!(clock.limited(outbound("o1", 2)), None);
}

#[test]
fn starts_clean_in_a_new_room() {
    let mut clock = Clock::new();
    assert_eq!(clock.limited(because(outbound("o1", 1), "cpu")), Some(ShareLimitation::Cpu));

    clock.meter.reset();
    let reading = clock.sample(sharing(vec![outbound("o1", 1)]));

    assert_eq!(reading.share.unwrap().limited, None);
    // And the first sample in the new room is a baseline again.
    assert!(!reading.measured);
}

#[test]
fn keeps_a_shares_size_across_a_sample_that_lacked_it() {
    let mut clock = Clock::new();

    clock.sample(sharing(vec![sized(outbound("o1", 1), 1920.0, 1080.0, Some(30.0))]));
    let gap = clock.sample(sharing(vec![outbound("o1", 1)])).share.unwrap();
    assert_eq!((gap.width, gap.height, gap.fps), (Some(1920.0), Some(1080.0), Some(30.0)));

    // Not into another track, though.
    let other = clock.sample(sharing(vec![outbound("o1", 2)])).share.unwrap();
    assert_eq!((other.width, other.height, other.fps), (None, None, None));
}

/*
 * The reading when a share is a ladder. A share is published as three
 * encodings, so a sender's report holds three outbound rows, and taking
 * whichever came last once showed somebody sharing at 1440p as "360p". The
 * rows are in the order that used to produce the wrong answer.
 */

#[test]
fn says_the_size_that_was_chosen_not_whichever_rung_came_last() {
    let mut clock = Clock::new();
    let share = clock
        .sample(sharing(vec![
            sized(outbound("o0", 3), 2560.0, 1440.0, Some(60.0)),
            sized(outbound("o1", 2), 1280.0, 720.0, Some(30.0)),
            sized(outbound("o2", 1), 640.0, 360.0, Some(15.0)),
        ]))
        .share
        .unwrap();

    assert_eq!((share.width, share.height, share.fps), (Some(2560.0), Some(1440.0), Some(60.0)));
}

// The bottom rung is never the one being squeezed; reading its reason would say
// nothing is wrong on a share the encoder is visibly struggling with.
#[test]
fn takes_the_reason_from_the_rung_it_is_reporting() {
    let mut clock = Clock::new();
    let share = clock
        .sample(sharing(vec![
            because(sized(outbound("o0", 3), 2560.0, 1440.0, None), "cpu"),
            because(sized(outbound("o1", 1), 640.0, 360.0, None), "none"),
        ]))
        .share
        .unwrap();

    assert_eq!(share.limited, Some(ShareLimitation::Cpu));
}

#[test]
fn still_draws_a_row_before_any_rung_has_produced_a_frame() {
    let mut clock = Clock::new();
    let share = clock.sample(sharing(vec![outbound("o0", 1), outbound("o1", 3)])).share;

    assert!(share.as_ref().is_some_and(|share| share.sending), "{share:?}");
}

// The person watching has the same question as the person sharing. Not asked
// while this device is sharing, though: then the question is its own encoder.
#[test]
fn reads_somebody_elses_share_unless_this_device_is_sending_one() {
    let theirs = TrackReport {
        local: false,
        screen_share: true,
        entries: vec![sized(inbound("i1", 9, 0.0), 1280.0, 720.0, Some(24.0))],
    };

    let mut clock = Clock::new();
    let share = clock.sample(vec![theirs.clone()]).share.unwrap();
    assert!(!share.sending);
    assert_eq!((share.width, share.fps, share.limited), (Some(1280.0), Some(24.0), None));

    // Handed over in either order, ours is the one reported.
    let ours = TrackReport {
        local: true,
        screen_share: true,
        entries: vec![sized(outbound("o1", 1), 1920.0, 1080.0, None)],
    };
    let mut clock = Clock::new();
    let share = clock.sample(vec![theirs, ours]).share.unwrap();
    assert!(share.sending);
    assert_eq!(share.width, Some(1920.0));
}

#[test]
fn has_no_share_row_when_nothing_is_shared() {
    let camera = TrackReport { local: true, screen_share: false, entries: vec![sized(outbound("o1", 1), 640.0, 480.0, None)] };

    assert_eq!(Clock::new().sample(vec![camera]).share, None);
}

/*
 * Rates and loss, which are changes over the interval and not totals since the
 * call began: a call that lost badly for ten seconds an hour ago is fine now.
 */

#[test]
fn reads_rates_and_loss_over_the_interval_after_a_baseline() {
    let report = |bytes_out: f64, packets: f64, lost: f64, bytes_in: f64| {
        vec![TrackReport {
            local: true,
            screen_share: false,
            entries: vec![
                sent("o1", bytes_out, packets),
                far_end("r1", lost, Some(0.0124), None),
                inbound("i1", 4, bytes_in),
                pair("p1", "succeeded", true, Some(0.0426), "c1"),
            ],
        }]
    };

    let mut clock = Clock::new();

    // The first sample has nothing to compare against.
    let first = clock.sample(report(1000.0, 100.0, 10.0, 500.0));
    assert!(!first.measured);
    assert_eq!((first.up_kbps, first.down_kbps, first.loss_percent), (None, None, None));
    assert_eq!((first.rtt_ms, first.jitter_ms), (Some(43.0), Some(12.0)));

    // Two seconds later: 250 kB out is 1000 kbit/s, 50 lost of 1050 is 4.76%.
    let second = clock.sample(report(251_000.0, 1100.0, 60.0, 125_500.0));
    assert!(second.measured);
    assert_eq!(second.up_kbps, Some(1000.0));
    assert_eq!(second.down_kbps, Some(500.0));
    let loss = second.loss_percent.unwrap();
    assert!((loss - 50.0 / 1050.0 * 100.0).abs() < 1e-9, "{loss}");
    assert_eq!(second.grade, Fair);

    // Counters that went backwards are no negative rate and no negative loss.
    let third = clock.sample(report(1000.0, 1200.0, 0.0, 0.0));
    assert_eq!((third.up_kbps, third.down_kbps, third.loss_percent), (Some(0.0), Some(0.0), Some(0.0)));

    // Nothing sent in the interval is no loss reading at all.
    let fourth = clock.sample(report(1000.0, 1200.0, 0.0, 0.0));
    assert_eq!(fourth.loss_percent, None);
}

// A track's report describes its whole peer connection, so two tracks on one
// connection hold the same entries; counting both doubled every byte.
#[test]
fn counts_an_entry_two_reports_share_once() {
    let shared = vec![sent("o1", 10_000.0, 10.0), far_end("r1", 1.0, None, None)];
    let reports = vec![
        TrackReport { local: true, screen_share: false, entries: shared.clone() },
        TrackReport { local: true, screen_share: false, entries: shared },
    ];

    let gathered = quality::gather(&reports);
    assert_eq!((gathered.bytes_sent, gathered.packets_sent, gathered.packets_lost), (10_000.0, 10.0, 1.0));

    // Entries without an id cannot be told apart, and are all counted.
    let anonymous = vec![TrackReport { local: true, screen_share: false, entries: vec![sent("", 5.0, 1.0), sent("", 5.0, 1.0)] }];
    assert_eq!(quality::gather(&anonymous).bytes_sent, 10.0);
}

#[test]
fn takes_the_round_trip_from_the_pair_in_use() {
    let gathered = |entries: Vec<StatsEntry>| quality::gather(&[TrackReport { local: true, screen_share: false, entries }]);

    // The pairs that lost the race have stale timings.
    let read = gathered(vec![
        pair("p1", "succeeded", false, Some(0.9), "c1"),
        pair("p2", "in-progress", true, Some(0.8), "c2"),
        pair("p3", "succeeded", true, Some(0.0805), "c3"),
    ]);
    assert_eq!(read.rtt_ms, Some(81.0));

    // Some stacks report it only from the far end, and only there is it read.
    let read = gathered(vec![far_end("r1", 0.0, None, Some(0.2)), far_end("r2", 0.0, None, Some(0.3))]);
    assert_eq!(read.rtt_ms, Some(200.0));

    // The pair wins whichever order they come in.
    let read = gathered(vec![far_end("r1", 0.0, None, Some(0.2)), pair("p1", "succeeded", true, Some(0.05), "c1")]);
    assert_eq!(read.rtt_ms, Some(50.0));

    // Rounded the way JavaScript rounds a half.
    let read = gathered(vec![pair("p1", "succeeded", true, Some(0.0125), "c1")]);
    assert_eq!(read.rtt_ms, Some(13.0));
}

/*
 * This device's own address: the useful half of the pair in use, the network
 * somebody is actually leaving from. Never the relay's, and never a stranger's.
 */

fn address_of(entries: Vec<StatsEntry>) -> (Option<String>, Option<String>) {
    let read = quality::gather(&[TrackReport { local: true, screen_share: false, entries }]);
    (read.own_address, read.ask_at)
}

#[test]
fn says_the_reflexive_address_on_a_direct_path() {
    let (own, ask) = address_of(vec![
        pair("p1", "succeeded", true, Some(0.02), "c2"),
        candidate("c1", "192.168.1.24", "host", None, None),
        candidate("c2", "203.0.113.9", "srflx", Some("192.168.1.24"), Some("stun:stun.example.com:3478")),
    ]);

    assert_eq!((own.as_deref(), ask), (Some("203.0.113.9"), None));
}

#[test]
fn says_nothing_for_a_private_address() {
    let (own, ask) = address_of(vec![
        candidate("c1", "192.168.1.24", "host", None, None),
        pair("p1", "succeeded", true, Some(0.02), "c1"),
    ]);

    assert_eq!((own, ask), (None, None));
}

// Through a relay the candidate in use is the relay's address, or one learned
// at the far end of the relay's path; that was shown as somebody's own and
// read, correctly, as a stranger's.
#[test]
fn says_what_the_relay_saw_on_a_relayed_path() {
    let turn = Some("turn:relay.example.com:3478?transport=udp");

    let (own, _) = address_of(vec![
        candidate("c1", "198.51.100.4", "relay", Some("203.0.113.9"), turn),
        pair("p1", "succeeded", true, Some(0.02), "c1"),
    ]);
    assert_eq!(own.as_deref(), Some("203.0.113.9"));

    // A peer-reflexive candidate names the relay candidate it went out through.
    let (own, _) = address_of(vec![
        pair("p1", "succeeded", true, Some(0.02), "c9"),
        StatsEntry::LocalCandidate {
            id: "c9".into(),
            address: Some("100.64.0.7".into()),
            candidate_type: Some("prflx".into()),
            related_address: Some("198.51.100.4".into()),
            url: None,
            relay_protocol: Some("udp".into()),
        },
        candidate("c1", "198.51.100.4", "relay", Some("203.0.113.9"), turn),
        candidate("c2", "198.51.100.4", "relay", Some("203.0.113.77"), turn),
    ]);
    assert_eq!(own.as_deref(), Some("203.0.113.9"));
}

// Under a relay-only policy the stack withholds every related address, so the
// relay is asked; and only over plain STUN, never in the clear to a relay the
// call was told to reach over TLS.
#[test]
fn asks_the_relay_where_the_candidates_will_not_say() {
    let (own, ask) = address_of(vec![
        candidate("c1", "198.51.100.4", "relay", Some("0.0.0.0"), Some("turn:relay.example.com:3478?transport=udp")),
        pair("p1", "succeeded", true, Some(0.02), "c1"),
    ]);
    assert_eq!((own, ask.as_deref()), (None, Some("stun:relay.example.com:3478")));

    let (own, ask) = address_of(vec![
        candidate("c1", "198.51.100.4", "relay", None, Some("turns:relay.example.com:443?transport=tcp")),
        pair("p1", "succeeded", true, Some(0.02), "c1"),
    ]);
    assert_eq!((own, ask), (None, None));
}

/// A relay's TURN port answering STUN, as internal/rtc/stun.go answers it, or
/// refusing with a binding error.
async fn relay_stun(refuse: bool) -> (SocketAddr, Arc<Mutex<usize>>) {
    let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let address = socket.local_addr().unwrap();
    let asked = Arc::new(Mutex::new(0usize));

    let count = asked.clone();
    tokio::spawn(async move {
        let mut buffer = [0u8; 1500];
        while let Ok((read, from)) = socket.recv_from(&mut buffer).await {
            *count.lock().unwrap() += 1;
            let request = &buffer[..read];

            let mut reply = Vec::new();
            reply.extend_from_slice(if refuse { &[0x01, 0x11] } else { &[0x01, 0x01] });
            if refuse {
                reply.extend_from_slice(&[0, 0]);
            } else {
                reply.extend_from_slice(&12u16.to_be_bytes());
            }
            reply.extend_from_slice(&request[4..20]);

            if !refuse {
                let IpAddr::V4(ip) = from.ip() else { continue };
                reply.extend_from_slice(&[0x00, 0x20, 0x00, 0x08, 0x00, 0x01]);
                reply.extend_from_slice(&(from.port() ^ 0x2112).to_be_bytes());
                for (byte, cookie) in ip.octets().iter().zip([0x21, 0x12, 0xA4, 0x42]) {
                    reply.push(byte ^ cookie);
                }
            }

            let _ = socket.send_to(&reply, from).await;
        }
    });

    (address, asked)
}

fn relayed_through(stun: SocketAddr) -> Vec<TrackReport> {
    vec![TrackReport {
        local: true,
        screen_share: false,
        entries: vec![
            candidate("c1", "198.51.100.4", "relay", None, Some(&format!("turn:{stun}?transport=udp"))),
            pair("p1", "succeeded", true, Some(0.02), "c1"),
        ],
    }]
}

#[tokio::test]
async fn asks_a_relay_once_and_believes_its_answer() {
    let (stun, asked) = relay_stun(false).await;
    let meter = ConnectionMeter::new();

    let first = meter.clone().sample(1000, relayed_through(stun), Some("tokyo-1".into())).await;
    assert_eq!(first.own_address.as_deref(), Some("127.0.0.1"));
    assert_eq!(first.holding.as_deref(), Some("tokyo-1"));

    let second = meter.clone().sample(3000, relayed_through(stun), Some(String::new())).await;
    assert_eq!(second.own_address.as_deref(), Some("127.0.0.1"));
    assert_eq!(second.holding, None);

    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(*asked.lock().unwrap(), 1, "asked again on the second reading");
}

// A relay that will not say is asked once, not on every reading.
#[tokio::test]
async fn remembers_a_relay_that_would_not_say() {
    let (stun, asked) = relay_stun(true).await;
    let meter = ConnectionMeter::new();

    assert_eq!(meter.clone().sample(1000, relayed_through(stun), None).await.own_address, None);
    assert_eq!(meter.clone().sample(3000, relayed_through(stun), None).await.own_address, None);

    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(*asked.lock().unwrap(), 1);
}

#[test]
fn takes_the_servers_verdict_as_a_floor() {
    let clean = || vec![TrackReport { local: true, screen_share: false, entries: vec![pair("p1", "succeeded", true, Some(0.02), "c1")] }];
    let mut clock = Clock::new();

    assert_eq!(clock.sample(clean()).grade, Good);
    clock.meter.heard(ServerQuality::Poor);
    assert_eq!(clock.sample(clean()).grade, Fair);
    clock.meter.heard(ServerQuality::Lost);
    assert_eq!(clock.sample(clean()).grade, Lost);
    clock.meter.heard(ServerQuality::Good);
    assert_eq!(clock.sample(clean()).grade, Good);
}
