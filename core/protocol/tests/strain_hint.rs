//! What these guard is a hint that is wrong, which is worse than no hint.
//!
//! The cases are web/src/live/strain.test.ts, and the recorded readings are
//! web/src/live/strain.recorded.ts, carried as fixtures/strain/recorded.json
//! and replayed here exactly as the web replays them: each row put back into
//! the shape Chrome reports it in, read, and judged in order.
//!
//! The hint's predecessor, an automatic follower that was deleted, took a still
//! screen for a struggling one and remembered a bad line for ever, and both
//! faults were silent. The same two are possible here in a gentler form, a card
//! telling somebody with a perfectly good line to send less, so most of what is
//! here is the things that must not raise the card: the estimate's climb at the
//! start of a share, a still screen, a busy machine, a ceiling too low for the
//! content, a sender that reports less than Chrome. The rest is the card's
//! promises about itself: that it changes nothing until somebody presses for
//! it, that "Not now" lasts exactly one share, a reconnect included, and that a
//! change of setting starts the watch again and takes the card down with it.

use serde_json::Value;
use tomoshibi_protocol::share::*;
use tomoshibi_protocol::strain::*;

const M: f64 = 1_000_000.0;

/*
 * The owner's numbers, written out rather than imported. A test that took its
 * timings from the constants it checks would move along with them: set the
 * quiet time to nothing and the test would wait nothing and pass.
 */
const QUIET_START: f64 = 30_000.0;
const QUIET_CHANGE: f64 = 15_000.0;
const WINDOW: f64 = 20_000.0;
const EVERY: f64 = 2_000.0;

fn setting(quality: ShareQuality, frame_rate: u32) -> ShareSetting {
    ShareSetting { quality, frame_rate }
}

const AT_1440_60: ShareSetting = ShareSetting { quality: ShareQuality::P1440, frame_rate: 60 };
const AT_1080_60: ShareSetting = ShareSetting { quality: ShareQuality::P1080, frame_rate: 60 };
const AT_1440_120: ShareSetting = ShareSetting { quality: ShareQuality::P1440, frame_rate: 120 };

/* -------------------------------------------------------------------------- */
/* Reading what Chrome reports                                                */
/* -------------------------------------------------------------------------- */

fn s(text: &str) -> Option<String> {
    Some(text.to_string())
}

/// One outbound layer as Chrome 145 gave it, in the recording held at 8 Mb/s.
#[allow(clippy::too_many_arguments)]
fn outbound(
    id: &str,
    rid: &str,
    stamp: f64,
    bytes: f64,
    frames: f64,
    fps: f64,
    size: (f64, f64),
    target: f64,
) -> StrainOutbound {
    StrainOutbound {
        id: s(id),
        rid: s(rid),
        kind: s("video"),
        active: Some(true),
        timestamp: Some(stamp),
        bytes_sent: Some(bytes),
        frames_sent: Some(frames),
        frames_encoded: Some(frames),
        frames_per_second: Some(fps),
        frame_width: Some(size.0),
        frame_height: Some(size.1),
        target_bitrate: Some(target),
        quality_limitation_reason: s("none"),
        media_source_id: None,
    }
}

/*
 * Two consecutive readings exactly as Chrome 145 gave them. Kept whole because
 * their shape is the point: three outbound entries in no particular order, a
 * media-source linked from only one of them (the bottom rung, not the top),
 * and the estimate on a pair reached through the transport.
 */
fn chrome(before: bool) -> StrainReport {
    let stamp = if before { 1791884715847.0 } else { 1791884717849.0 };
    let pick = |early: f64, late: f64| if before { early } else { late };

    let mut q = outbound(
        "OT01V517873725",
        "q",
        stamp,
        pick(6137959.0, 6386541.0),
        pick(168.0, 175.0),
        pick(4.0, 3.0),
        (640.0, 360.0),
        pick(250004.0, 250010.0),
    );
    q.media_source_id = s("SV1");

    StrainReport {
        outbound: vec![
            outbound(
                "OT01V2131759287",
                "h",
                stamp,
                pick(22972777.0, 23894605.0),
                pick(168.0, 175.0),
                pick(4.0, 3.0),
                (1280.0, 720.0),
                pick(1250070.0, 1250081.0),
            ),
            outbound(
                "OT01V3993335387",
                "f",
                stamp,
                pick(17469907.0, 18283613.0),
                pick(84.0, 87.0),
                pick(2.0, 1.0),
                (1920.0, 1080.0),
                pick(2325206.0, 2325189.0),
            ),
            q,
        ],
        media_sources: vec![StrainMediaSource {
            id: s("SV1"),
            kind: s("video"),
            frames: Some(pick(2833.0, 2944.0)),
            frames_per_second: Some(pick(56.0, 54.0)),
            width: Some(1920.0),
            height: Some(1080.0),
        }],
        candidate_pairs: vec![StrainCandidatePair {
            id: s("CPmm4orUUS_2+XMyrOk"),
            state: s("succeeded"),
            nominated: Some(true),
            available_outgoing_bitrate: Some(8_000_000.0),
            bytes_sent: None,
        }],
        transports: vec![StrainTransport { id: s("T01"), selected_candidate_pair_id: s("CPmm4orUUS_2+XMyrOk") }],
    }
}

fn encoding(rid: &str, active: bool, max_bitrate: f64) -> StrainEncoding {
    StrainEncoding { rid: s(rid), active: Some(active), max_bitrate: Some(max_bitrate) }
}

fn chrome_encodings() -> Vec<StrainEncoding> {
    vec![encoding("q", true, 500_000.0), encoding("h", true, 2_500_000.0), encoding("f", true, 19_400_000.0)]
}

fn read(
    report: &StrainReport,
    encodings: &[StrainEncoding],
    previous: Option<&StrainCounters>,
    now: f64,
) -> (StrainSample, StrainCounters) {
    read_sample(report, encodings, previous, now)
}

fn close(value: Option<f64>, expected: f64, digits: i32) {
    let value = value.expect("a figure");
    assert!((value - expected).abs() < 0.5 * 10f64.powi(-digits), "{value} against {expected}");
}

#[test]
fn finds_the_top_layer_by_its_rid_and_the_capture_through_whichever_layer_links_it() {
    let (_, first) = read(&chrome(true), &chrome_encodings(), None, 0.0);
    let (sample, _) = read(&chrome(false), &chrome_encodings(), Some(&first), 2002.0);
    let seconds = 2.002;

    close(sample.source, (2944.0 - 2833.0) / seconds, 5);
    // The top layer's frames and bits, not the bottom rung's: "f" was the
    // middle entry of three, and the rung that carries the link is "q".
    close(sample.sent, (87.0 - 84.0) / seconds, 5);
    close(sample.spent, (18283613.0 - 17469907.0) * 8.0 / seconds, 0);
    close(sample.rest, (23894605.0 - 22972777.0 + 6386541.0 - 6137959.0) * 8.0 / seconds, 0);

    // The rungs one by one, in the order of their encodings, not the report's.
    let rungs = sample.rungs.clone().unwrap();
    assert_eq!(rungs.iter().map(|rung| rung.ceiling).collect::<Vec<_>>(), vec![Some(500_000.0), Some(2_500_000.0)]);
    close(rungs[0].spent, (6386541.0 - 6137959.0) * 8.0 / seconds, 0);
    close(rungs[1].spent, (23894605.0 - 22972777.0) * 8.0 / seconds, 0);

    assert_eq!(sample.target, Some(2325189.0));
    assert_eq!(sample.ceiling, Some(19_400_000.0));
    assert_eq!(sample.asked, Some(22_400_000.0));
    assert_eq!(sample.available, Some(8_000_000.0));
    assert_eq!(sample.reason.as_deref(), Some("none"));
    assert_eq!((sample.width, sample.height), (Some(1920), Some(1080)));

    // Read whole, this is the case the hint is for: 55 frames a second
    // captured, one and a half sent, a quarter of the ceiling allotted and all
    // of it spent, while the sender calls the limitation "none".
    assert_eq!(classify(&sample), StrainVerdict::Strained);
}

#[test]
fn reads_the_encodings_in_either_order() {
    // This client's own: the Rust SDK lists its encodings top first.
    let (_, first) = read(&chrome(true), &chrome_encodings(), None, 0.0);
    let mut reversed = chrome_encodings();
    reversed.reverse();
    let (sample, _) = read(&chrome(false), &reversed, Some(&first), 2002.0);

    assert_eq!(sample.ceiling, Some(19_400_000.0));
    assert_eq!(classify(&sample), StrainVerdict::Strained);
}

#[test]
fn says_nothing_about_a_rate_it_has_only_one_reading_of() {
    let (sample, _) = read(&chrome(false), &chrome_encodings(), None, 0.0);
    assert_eq!(sample.spent, None);
    assert_eq!(classify(&sample), StrainVerdict::Unknown);
}

#[test]
fn takes_a_counter_that_went_backwards_as_a_new_stream_not_a_negative_rate() {
    let (_, first) = read(&chrome(false), &chrome_encodings(), None, 0.0);
    let mut later = chrome(true);
    for entry in &mut later.outbound {
        entry.timestamp = Some(1791884719851.0);
    }
    let (sample, _) = read(&later, &chrome_encodings(), Some(&first), 0.0);

    assert_eq!(sample.sent, Some(2.0)); // fell back to the sender's own figure
    assert_eq!(sample.spent, None);
    assert_eq!(classify(&sample), StrainVerdict::Unknown);
}

#[test]
fn counts_the_whole_line_where_the_sender_does_and_only_against_the_same_pair() {
    // The pair's own count ran ahead of the share's layers by a second sender's
    // worth: the camera and the voice go out on the same connection.
    let counted = |before: bool, bytes: f64, id: &str| {
        let mut report = chrome(before);
        report.candidate_pairs[0].id = s(id);
        report.candidate_pairs[0].bytes_sent = Some(bytes);
        report.transports[0].selected_candidate_pair_id = s(id);
        report
    };

    let (_, first) = read(&counted(true, 50_000_000.0, "CPmm4orUUS_2+XMyrOk"), &chrome_encodings(), None, 0.0);
    let (sample, _) =
        read(&counted(false, 52_000_000.0, "CPmm4orUUS_2+XMyrOk"), &chrome_encodings(), Some(&first), 2002.0);
    close(sample.carried, 2_000_000.0 * 8.0 / 2.002, 0);

    // Moved to another pair: two counts of two things.
    let (moved, _) = read(&counted(false, 52_000_000.0, "CPelsewhere"), &chrome_encodings(), Some(&first), 2002.0);
    assert_eq!(moved.carried, None);

    // And Chrome's own report as recorded, which kept no count, says nothing.
    let (_, plain) = read(&chrome(true), &chrome_encodings(), None, 0.0);
    assert_eq!(read(&chrome(false), &chrome_encodings(), Some(&plain), 2002.0).0.carried, None);
}

#[test]
fn says_nothing_about_the_rest_of_the_share_where_one_rung_cannot_be_counted() {
    // A partial sum would understate what the rest of the share takes, and
    // offer a setting that does not fit.
    let (mut before, mut after) = (chrome(true), chrome(false));
    for report in [&mut before, &mut after] {
        report.outbound.iter_mut().filter(|one| one.rid.as_deref() == Some("h")).for_each(|one| one.bytes_sent = None);
    }

    let (_, first) = read(&before, &chrome_encodings(), None, 0.0);
    let (sample, _) = read(&after, &chrome_encodings(), Some(&first), 2002.0);

    assert_eq!(sample.rest, None);
    assert!(sample.spent.is_some());
}

#[test]
fn reads_a_top_layer_nobody_is_watching_as_no_evidence() {
    // dynacast pauses the top rung when nobody has the share at full size. Read
    // as strained, it would be the strongest evidence there is.
    let paused =
        vec![encoding("q", true, 500_000.0), encoding("h", true, 2_500_000.0), encoding("f", false, 19_400_000.0)];
    let (_, first) = read(&chrome(true), &paused, None, 0.0);
    let (sample, _) = read(&chrome(false), &paused, Some(&first), 2002.0);

    assert!(!sample.active);
    assert_eq!(classify(&sample), StrainVerdict::Unknown);
}

#[test]
fn tells_firefox_nothing_because_it_reports_too_little_to_be_sure() {
    // Firefox 157: it drops frames too, fifteen sent of nineteen captured, but
    // has no target, no limitation reason and no estimate of the line.
    let report = |step: f64| StrainReport {
        outbound: vec![StrainOutbound {
            id: s("dfc7d826"),
            kind: s("video"),
            timestamp: Some(1791887135722.0 + step * 2000.0),
            bytes_sent: Some(2749786.0 + step * 950006.0),
            frames_sent: Some(83.0 + step * 30.0),
            frames_encoded: Some(83.0 + step * 30.0),
            frames_per_second: Some(15.0),
            frame_width: Some(1280.0),
            frame_height: Some(720.0),
            ..Default::default()
        }],
        media_sources: vec![StrainMediaSource {
            id: s("mediasource_video_0"),
            kind: s("video"),
            frames: Some(132.0 + step * 38.0),
            frames_per_second: Some(20.0),
            width: Some(1280.0),
            height: Some(720.0),
        }],
        candidate_pairs: vec![StrainCandidatePair {
            id: s("684970da"),
            state: s("succeeded"),
            nominated: Some(true),
            ..Default::default()
        }],
        transports: vec![StrainTransport { id: s("41e5c119"), selected_candidate_pair_id: s("684970da") }],
    };
    let encodings = vec![StrainEncoding { rid: None, active: Some(true), max_bitrate: Some(4_000_000.0) }];

    let mut counters = read(&report(0.0), &encodings, None, 0.0).1;
    let mut samples = Vec::new();
    for step in 1..=45 {
        let (sample, next) = read(&report(step as f64), &encodings, Some(&counters), step as f64 * 2000.0);
        counters = next;
        samples.push(sample);
    }

    close(samples[0].source, 19.0, 0);
    close(samples[0].sent, 15.0, 0);
    assert!(samples.iter().all(|sample| classify(sample) == StrainVerdict::Unknown));
    assert!(replay(&samples, setting(ShareQuality::P1080, 30), QUIET_START).is_empty());
}

#[test]
fn says_nothing_without_each_figure_the_rule_needs() {
    type Strip = fn(&mut StrainReport);
    let strip: [(&str, Strip); 5] = [
        ("the line's estimate", |report| {
            report.candidate_pairs.iter_mut().for_each(|pair| pair.available_outgoing_bitrate = None)
        }),
        ("the limitation reason", |report| {
            report.outbound.iter_mut().for_each(|one| one.quality_limitation_reason = None)
        }),
        ("the target", |report| report.outbound.iter_mut().for_each(|one| one.target_bitrate = None)),
        // The capture's rate has a second source in the sender's own figure;
        // take that away too, or the reading is not missing anything.
        ("the capture's frames", |report| {
            report.media_sources.iter_mut().for_each(|one| {
                one.frames = None;
                one.frames_per_second = None;
            })
        }),
        ("the layer's bytes", |report| report.outbound.iter_mut().for_each(|one| one.bytes_sent = None)),
    ];

    for (name, without) in strip {
        let (mut before, mut after) = (chrome(true), chrome(false));
        without(&mut before);
        without(&mut after);

        let (_, first) = read(&before, &chrome_encodings(), None, 0.0);
        let (sample, _) = read(&after, &chrome_encodings(), Some(&first), 2002.0);
        assert_eq!(classify(&sample), StrainVerdict::Unknown, "without {name}");
    }
}

/* -------------------------------------------------------------------------- */
/* Replaying a share                                                          */
/* -------------------------------------------------------------------------- */

/// What the judge said, and when: the hint, or None where it took it down.
type Said = Vec<(f64, Option<StrainHint>)>;

/// Feed readings through the judge the way the watch does, keeping what it said.
fn replay(samples: &[StrainSample], setting: ShareSetting, settled: f64) -> Said {
    let mut said = Vec::new();
    let mut showing = false;
    for index in 1..=samples.len() {
        let at = samples[index - 1].at;
        match judge(&samples[..index], settled, Some(setting), showing) {
            StrainSaid::TakeDown => {
                showing = false;
                said.push((at, None));
            }
            StrainSaid::Show { hint } => {
                showing = true;
                said.push((at, Some(hint)));
            }
            StrainSaid::LeaveAsIs => {}
        }
    }
    said
}

fn shown(said: &Said) -> Vec<bool> {
    said.iter().map(|one| one.1.is_some()).collect()
}

fn number(value: &Value) -> Option<f64> {
    value.as_f64()
}

/// One recorded row back into the shape Chrome reports it in.
fn chrome_shaped(row: &Value) -> (StrainReport, Vec<StrainEncoding>) {
    let at = row[0].as_f64().unwrap();
    let available = row[1].as_f64().unwrap();
    let source = &row[2];

    let outbound = row[3]
        .as_array()
        .unwrap()
        .iter()
        .map(|layer| {
            let rid = layer[0].as_str().unwrap();
            StrainOutbound {
                id: Some(format!("OT-{rid}")),
                rid: s(rid),
                kind: s("video"),
                active: Some(true),
                timestamp: Some(at),
                bytes_sent: number(&layer[1]),
                frames_sent: number(&layer[2]),
                frames_encoded: number(&layer[2]),
                frames_per_second: number(&layer[3]),
                frame_width: number(&layer[4]),
                frame_height: number(&layer[4]).and(number(&layer[5])),
                target_bitrate: number(&layer[6]),
                quality_limitation_reason: layer[7].as_str().map(str::to_string),
                // As Chrome does it: the link from the bottom rung only.
                media_source_id: (rid == "q").then(|| "SV1".to_string()),
            }
        })
        .collect();

    let report = StrainReport {
        outbound,
        media_sources: vec![StrainMediaSource {
            id: s("SV1"),
            kind: s("video"),
            frames: number(&source[0]),
            frames_per_second: number(&source[1]),
            width: number(&source[2]),
            height: number(&source[3]),
        }],
        candidate_pairs: vec![StrainCandidatePair {
            id: s("CP1"),
            state: s("succeeded"),
            nominated: Some(true),
            available_outgoing_bitrate: (available >= 0.0).then_some(available),
            bytes_sent: None,
        }],
        transports: vec![StrainTransport { id: s("T01"), selected_candidate_pair_id: s("CP1") }],
    };

    let encodings = row[4]
        .as_array()
        .unwrap()
        .iter()
        .map(|one| StrainEncoding {
            rid: one[0].as_str().map(str::to_string),
            active: one[1].as_bool(),
            max_bitrate: number(&one[2]),
        })
        .collect();

    (report, encodings)
}

fn recording(name: &str) -> Vec<StrainSample> {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/strain/recorded.json")).unwrap();
    let rows = fixture["recordings"][name]["rows"].as_array().unwrap_or_else(|| panic!("no recording {name}"));

    let mut counters: Option<StrainCounters> = None;
    rows.iter()
        .map(|row| {
            let (report, encodings) = chrome_shaped(row);
            let (sample, next) = read_sample(&report, &encodings, counters.as_ref(), row[0].as_f64().unwrap());
            counters = Some(next);
            sample
        })
        .collect()
}

#[test]
fn says_so_on_a_line_that_genuinely_cannot_carry_the_share_once_and_not_early() {
    // 1440p at sixty asking 34.4 Mb/s on a line held at 25, with content that
    // costs what a game costs: 36 of 57 frames sent, for a minute.
    let said = replay(&recording("limit1440Noise"), AT_1440_60, QUIET_START);
    assert_eq!(said.len(), 1);

    let (at, hint) = said[0];
    let hint = hint.expect("nothing was said");

    // Thirty seconds of quiet and then twenty of evidence: never before.
    assert!(at >= QUIET_START + WINDOW - EVERY, "{at}");
    assert_eq!(strain_megabits(hint.have), "25");
    assert_eq!(strain_megabits(hint.need), "37");
    assert_eq!(hint.to, setting(ShareQuality::P1080, 30));
}

#[test]
fn takes_it_back_once_the_line_is_lifted() {
    // The cap removed by renegotiating at 56 seconds. The top layer still drops
    // frames, but that is the ceiling's doing, and a smaller setting would make
    // it worse, so the card must go.
    let said = replay(&recording("recover1440Noise"), AT_1440_60, QUIET_START);
    assert_eq!(shown(&said), vec![true, false]);

    assert!(said[0].0 < 56_000.0);
    assert!(said[1].0 > 56_000.0 + WINDOW * 0.75);
    assert!(said[1].0 < 56_000.0 + WINDOW + 2.0 * EVERY);
}

#[test]
fn is_not_fooled_by_the_estimates_climb_at_the_start_of_a_share() {
    // Unconstrained, asking 48.7: the estimate takes thirty-six seconds to reach
    // the ask, and reading by reading that is indistinguishable from a full line.
    let samples = recording("ramp1440Noise");
    assert!(samples.iter().filter(|sample| classify(sample) == StrainVerdict::Strained).count() > 5);
    assert!(replay(&samples, AT_1440_120, QUIET_START).is_empty());
}

#[test]
fn says_nothing_about_a_still_screen_however_starved_its_estimate() {
    // The trap the deleted follower fell into: every figure but the capture's
    // says "struggling".
    let samples = recording("static1440Capped");
    assert!(samples.iter().all(|sample| classify(sample) == StrainVerdict::Unknown));
    assert!(replay(&samples, AT_1440_60, QUIET_START).is_empty());
}

#[test]
fn says_nothing_about_cheap_content_on_an_open_line() {
    assert!(replay(&recording("cheap1080Free"), AT_1080_60, QUIET_START).is_empty());
}

#[test]
fn says_nothing_when_not_even_the_lightest_setting_would_fit() {
    // Held at 8 Mb/s: plainly struggling, and 1080p at fifteen alone has a
    // ceiling of 9.7 before the rest of the share.
    let samples = recording("limitSimCheap");
    assert!(samples[samples.len() - 10..].iter().all(|sample| classify(sample) == StrainVerdict::Strained));
    assert!(replay(&samples, AT_1080_60, QUIET_START).is_empty());
}

/* -------------------------------------------------------------------------- */
/* Readings made up to order                                                  */
/* -------------------------------------------------------------------------- */

/// What the rungs beneath the top take, as Chrome allots them on the ladder of
/// 2026-10-14 with a full-size viewer: 2.5 and about 8.5.
const RUNGS: f64 = 11.0 * M;

/// The friend's share that prompted this: 1440p at 120 asking 48.7 Mb/s for the
/// top layer, on a home uplink that carried about 37.
fn strained(at: f64) -> StrainSample {
    StrainSample {
        at,
        active: true,
        source: Some(118.0),
        sent: Some(64.0),
        spent: Some(29.0 * M),
        rest: Some(RUNGS),
        rungs: None,
        carried: None,
        target: Some(29.0 * M),
        ceiling: Some(48_660_000.0),
        // The top's ceiling and both rungs': 48.66 + 2.5 + 11.31.
        asked: Some(62_473_708.0),
        available: Some(37.0 * M),
        reason: s("none"),
        width: Some(2560),
        height: Some(1440),
    }
}

/// Readings every two seconds from `from` to `to`, each the friend's share as
/// `shape` changes it.
fn readings(from: f64, to: f64, shape: impl Fn(f64, &mut StrainSample)) -> Vec<StrainSample> {
    let mut out = Vec::new();
    let mut at = from;
    while at <= to {
        let mut sample = strained(at);
        shape(at, &mut sample);
        out.push(sample);
        at += EVERY;
    }
    out
}

fn as_is(_: f64, _: &mut StrainSample) {}

#[test]
fn tells_the_friend_what_his_line_carries_and_what_would_fit_it() {
    let said = replay(&readings(2_000.0, 60_000.0, as_is), AT_1440_120, QUIET_START);
    let hint = said[0].1.expect("a hint");

    assert_eq!(strain_megabits(hint.have), "37");
    assert_eq!(strain_megabits(hint.need), "62");
    // Four fifths of 37 is 29.6, less the rungs' eleven: 18.6 for the top.
    // 1080p at sixty asks 19.4 and does not fit; 1440p at fifteen asks 17.2 and
    // is worth more than 1080p at thirty's 13.7.
    assert_eq!(hint.to, setting(ShareQuality::P1440, 15));
    assert!(share_ask_of(AT_1080_60, ShareCapture::default()) as f64 > 0.8 * 37.0 * M - RUNGS);
    assert!(share_ask_of(setting(ShareQuality::P1440, 15), ShareCapture::default()) as f64 <= 0.8 * 37.0 * M - RUNGS);
}

#[test]
fn waits_out_a_climb_that_outlasts_the_quiet_time() {
    // Every reading strained and the line used to the full all the way up,
    // which is what a climb looks like; only the slope says otherwise.
    let climbing = |at: f64, sample: &mut StrainSample| {
        let available = (10.0 + 35.0 * at / 56_000.0) * M;
        sample.available = Some(available);
        sample.target = Some(0.7 * available);
        sample.spent = Some(0.85 * available);
    };
    let samples = readings(2_000.0, 56_000.0, climbing);

    assert!(samples.iter().all(|sample| classify(sample) == StrainVerdict::Strained));
    assert!(samples.iter().all(|sample| sample.available.unwrap() < sample.asked.unwrap()));
    assert!(replay(&samples, AT_1440_120, QUIET_START).is_empty());

    // Once the climb levels off below the ask, the same share is told, once the
    // window holds more of the level than of the climb.
    let mut level = strained(56_000.0);
    climbing(56_000.0, &mut level);
    let mut longer = samples.clone();
    longer.extend(readings(58_000.0, 100_000.0, |_, sample| {
        sample.available = level.available;
        sample.target = level.target;
        sample.spent = level.spent;
    }));

    let said = replay(&longer, AT_1440_120, QUIET_START);
    assert_eq!(said.len(), 1);
    assert!(said[0].0 > 56_000.0);
}

#[test]
fn takes_a_sawtooth_for_the_ceiling_it_is_not_a_climb() {
    let tooth = |at: f64, sample: &mut StrainSample| sample.available = Some((33.0 + ((at / EVERY) % 4.0) * 2.5) * M);
    assert_eq!(replay(&readings(2_000.0, 60_000.0, tooth), AT_1440_120, QUIET_START).len(), 1);
}

#[test]
fn says_nothing_about_an_estimate_that_stopped_climbing_because_nothing_was_testing_it() {
    // 1440p at sixty on a 60 Mb/s line, the encoder starved on the way up and
    // the estimate standing at 20 from 36 to 52 seconds: strained reading by
    // reading, flat across the window, short of the ask, and the line two
    // thirds used, which is not a line that is full.
    let stall = |at: f64, sample: &mut StrainSample| {
        sample.source = Some(60.0);
        sample.ceiling = Some(34_400_000.0);
        sample.asked = Some(48_220_000.0);

        let available = if at < 36_000.0 {
            (5.0 + 15.0 * at / 36_000.0) * M
        } else if at < 52_000.0 {
            20.0 * M
        } else {
            (20.0 + 2.0 * (at - 52_000.0) / 1000.0).min(60.0) * M
        };
        sample.available = Some(available);

        if available >= 40.0 * M {
            sample.sent = Some(60.0);
            sample.target = Some(34.4 * M);
            sample.spent = Some(30.0 * M);
        } else {
            let out = available / 1.5;
            sample.sent = Some(8.0);
            sample.target = Some(out - RUNGS);
            sample.spent = Some(out - RUNGS);
        }
    };
    let samples = readings(2_000.0, 90_000.0, stall);

    assert!(samples
        .iter()
        .filter(|sample| sample.at >= 36_000.0 && sample.at < 52_000.0)
        .all(|sample| classify(sample) == StrainVerdict::Strained));
    assert!(replay(&samples, AT_1440_60, QUIET_START).is_empty());
}

#[test]
fn says_nothing_about_a_stalled_estimate_even_where_a_smaller_setting_would_fit() {
    // This client's own. The stall above stands at 20 Mb/s, where nothing would
    // fit, so it passes with the fullness test removed, in the web as here. The
    // web's header describes the stall that prompted the test as one that
    // "offered 1080p at fifteen"; at 30 Mb/s, still short of the ask, that is
    // what is offered without it.
    let stall = |at: f64, sample: &mut StrainSample| {
        sample.source = Some(60.0);
        sample.ceiling = Some(34_400_000.0);
        sample.asked = Some(48_220_000.0);

        let available = if at < 36_000.0 {
            (5.0 + 25.0 * at / 36_000.0) * M
        } else if at < 52_000.0 {
            30.0 * M
        } else {
            (30.0 + 2.0 * (at - 52_000.0) / 1000.0).min(60.0) * M
        };
        sample.available = Some(available);

        if available >= 40.0 * M {
            sample.sent = Some(60.0);
            sample.target = Some(34.4 * M);
            sample.spent = Some(30.0 * M);
        } else {
            let out = available / 1.5;
            sample.sent = Some(8.0);
            sample.target = Some(out - RUNGS);
            sample.spent = Some(out - RUNGS);
        }
    };
    let samples = readings(2_000.0, 90_000.0, stall);

    assert!(samples
        .iter()
        .filter(|sample| sample.at >= 36_000.0 && sample.at < 52_000.0)
        .all(|sample| classify(sample) == StrainVerdict::Strained));
    assert!(replay(&samples, AT_1440_60, QUIET_START).is_empty());
}

#[test]
fn counts_the_camera_and_the_voice_where_the_sender_counts_the_line() {
    // The share's own layers take 29 of the 37, which on their own are not a
    // full line; the line's count, with the camera and the voice in it, says it
    // is.
    let shared = readings(2_000.0, 60_000.0, |_, sample| {
        sample.spent = Some(18.0 * M);
        sample.target = Some(18.0 * M);
    });
    assert!(replay(&shared, AT_1440_120, QUIET_START).is_empty());

    let counted: Vec<StrainSample> =
        shared.into_iter().map(|sample| StrainSample { carried: Some(34.0 * M), ..sample }).collect();
    assert_eq!(replay(&counted, AT_1440_120, QUIET_START).len(), 1);
}

#[test]
fn says_nothing_while_the_encoder_carries_something_other_than_the_setting() {
    // A reconnect published the share again with its first options, 48.7 at
    // the top, under a menu that says 1440p at thirty.
    let samples = readings(2_000.0, 90_000.0, as_is);
    assert!(replay(&samples, setting(ShareQuality::P1440, 30), QUIET_START).is_empty());
    assert_eq!(replay(&samples, AT_1440_120, QUIET_START).len(), 1);
}

#[test]
fn says_nothing_about_a_still_screen() {
    let said = replay(
        &readings(2_000.0, 90_000.0, |_, sample| {
            sample.source = Some(2.0);
            sample.sent = Some(2.0);
            sample.available = Some(4.0 * M);
            sample.target = Some(2.0 * M);
            sample.spent = Some(2.0 * M);
        }),
        AT_1440_120,
        QUIET_START,
    );
    assert!(said.is_empty());
}

#[test]
fn says_nothing_when_the_machine_is_the_limit() {
    let said = replay(&readings(2_000.0, 90_000.0, |_, sample| sample.reason = s("cpu")), AT_1440_120, QUIET_START);
    assert!(said.is_empty());
}

#[test]
fn says_nothing_about_a_ceiling_too_low_for_the_content() {
    // Allotted its whole ceiling and still dropping frames: the content wants
    // more than the setting allows, and a smaller setting allows less.
    let said = replay(
        &readings(2_000.0, 90_000.0, |_, sample| {
            sample.ceiling = Some(34_400_000.0);
            sample.asked = Some(48_220_000.0);
            sample.target = Some(34_000_000.0);
            sample.spent = Some(34_000_000.0);
            sample.available = Some(47.0 * M);
        }),
        AT_1440_60,
        QUIET_START,
    );
    assert!(said.is_empty());
}

#[test]
fn says_nothing_about_an_allotment_the_encoder_is_not_using() {
    let said =
        replay(&readings(2_000.0, 90_000.0, |_, sample| sample.spent = Some(12.0 * M)), AT_1440_120, QUIET_START);
    assert!(said.is_empty());

    // This client's own. The case above also fails the line being full, and so
    // passes with the spending test removed, in the web as here. On a line the
    // camera and the voice do fill, only the spending test says the share is
    // not what is short.
    let full = replay(
        &readings(2_000.0, 90_000.0, |_, sample| {
            sample.spent = Some(12.0 * M);
            sample.carried = Some(34.0 * M);
        }),
        AT_1440_120,
        QUIET_START,
    );
    assert!(full.is_empty());
}

#[test]
fn says_nothing_about_a_stutter_that_passed_before_twenty_seconds_of_it() {
    let said = replay(
        &readings(2_000.0, 90_000.0, |at, sample| {
            if !(at > 40_000.0 && at <= 52_000.0) {
                sample.sent = Some(116.0);
            }
        }),
        AT_1440_120,
        QUIET_START,
    );
    assert!(said.is_empty());
}

#[test]
fn does_not_speak_once_the_stutter_has_stopped_however_recent_it_was() {
    // Strained until the very moment the window fills, clear from then on:
    // nine in ten at that moment, and the one that matters is the last.
    let said = replay(
        &readings(2_000.0, 90_000.0, |at, sample| {
            if at >= QUIET_START + WINDOW {
                sample.sent = Some(116.0);
            }
        }),
        AT_1440_120,
        QUIET_START,
    );
    assert!(said.is_empty());
}

#[test]
fn wants_four_readings_in_five_not_most_of_them() {
    let said = replay(
        &readings(2_000.0, 90_000.0, |at, sample| {
            if (at / EVERY) % 10.0 < 3.0 {
                sample.sent = Some(116.0);
            }
        }),
        AT_1440_120,
        QUIET_START,
    );
    assert!(said.is_empty());
}

#[test]
fn does_not_count_readings_from_before_the_share_settled() {
    // A change of setting at 40 seconds moves the start of counting to 55.
    let settled = 40_000.0 + QUIET_CHANGE;
    let said = replay(&readings(2_000.0, 100_000.0, as_is), AT_1440_120, settled);

    assert_eq!(said.len(), 1);
    assert!(said[0].0 >= settled + WINDOW - EVERY);
}

#[test]
fn does_not_reach_a_verdict_across_gaps_in_the_readings() {
    // An app the system stopped running for a while: one reading in three.
    let sparse: Vec<StrainSample> = readings(2_000.0, 120_000.0, as_is)
        .into_iter()
        .enumerate()
        .filter(|(index, _)| index % 3 == 0)
        .map(|(_, one)| one)
        .collect();
    assert!(replay(&sparse, AT_1440_120, QUIET_START).is_empty());
}

#[test]
fn does_not_tell_somebody_their_line_is_short_of_what_it_carries() {
    let said =
        replay(&readings(2_000.0, 90_000.0, |_, sample| sample.available = Some(60.0 * M)), AT_1440_120, QUIET_START);
    assert!(said.is_empty());

    // This client's own. The case above is also a line two thirds used, and so
    // passes with this test removed, in the web as here. Something else on the
    // line taking it, the line full and its estimate above the ask, is the case
    // where only this test keeps the sentence from being false.
    let taken = replay(
        &readings(2_000.0, 90_000.0, |_, sample| {
            sample.available = Some(70.0 * M);
            sample.carried = Some(65.0 * M);
        }),
        AT_1440_120,
        QUIET_START,
    );
    assert!(taken.is_empty());
}

#[test]
fn is_not_taken_down_by_the_screen_going_still() {
    let said = replay(
        &readings(2_000.0, 140_000.0, |at, sample| {
            if at > 60_000.0 {
                sample.source = Some(1.0);
                sample.sent = Some(1.0);
                sample.spent = Some(0.2 * M);
            }
        }),
        AT_1440_120,
        QUIET_START,
    );
    assert_eq!(shown(&said), vec![true]);
}

#[test]
fn is_taken_down_when_the_line_recovers() {
    let said = replay(
        &readings(2_000.0, 140_000.0, |at, sample| {
            if at > 70_000.0 {
                sample.sent = Some(118.0);
                sample.target = Some(48_000_000.0);
                sample.available = Some(80.0 * M);
            }
        }),
        AT_1440_120,
        QUIET_START,
    );

    assert_eq!(shown(&said), vec![true, false]);
    assert!(said[1].0 >= 70_000.0 + WINDOW * 0.8 - EVERY);
}

/* -------------------------------------------------------------------------- */
/* What the rungs give back                                                   */
/* -------------------------------------------------------------------------- */

/// A 1440p share at sixty, top layer starved, the two rungs sending `small` and
/// `middle`.
fn under_sixty(available: f64, spent: f64, small: f64, middle: f64) -> Vec<StrainSample> {
    let asks = share_ask_of(AT_1440_60, ShareCapture::default()) as f64;
    let two_rungs = (share_rung_small().max_bitrate + share_rung_middle_fast().max_bitrate) as f64;

    readings(2_000.0, 60_000.0, |_, sample| {
        sample.source = Some(60.0);
        sample.sent = Some(30.0);
        sample.spent = Some(spent);
        sample.target = Some(spent);
        sample.ceiling = Some(asks);
        sample.asked = Some(asks + two_rungs);
        sample.available = Some(available);
        sample.rest = Some(small + middle);
        sample.rungs = Some(vec![
            StrainRung { ceiling: Some(share_rung_small().max_bitrate as f64), spent: Some(small) },
            StrainRung { ceiling: Some(share_rung_middle_fast().max_bitrate as f64), spent: Some(middle) },
        ]);
    })
}

fn ask(quality: ShareQuality, frame_rate: u32) -> f64 {
    share_ask_of(setting(quality, frame_rate), ShareCapture::default()) as f64
}

#[test]
fn counts_the_megabits_the_1080p_rung_gives_back_at_thirty_frames() {
    // 30 Mb/s: four fifths is 24, less the rungs' 10.9 leaves 13.1, and 1080p
    // at thirty asks 13.7. At thirty the 1080p rung gives back the 2.4 it sent
    // above its allotment: 15.5, which holds 1080p at thirty and not 1440p at
    // fifteen.
    let samples = under_sixty(30.0 * M, 18.0 * M, 2.5 * M, 8.4 * M);

    assert!(ask(ShareQuality::P1080, 30) > 0.8 * 30.0 * M - 10.9 * M);
    assert!(ask(ShareQuality::P1080, 30) <= 0.8 * 30.0 * M - 10.9 * M + 2.4 * M);
    assert_eq!(replay(&samples, AT_1440_60, QUIET_START)[0].1.unwrap().to, setting(ShareQuality::P1080, 30));

    // Read without the rungs one by one, the same line is offered half the
    // frames: what the hint said before this was counted.
    let blind: Vec<StrainSample> = samples.into_iter().map(|sample| StrainSample { rungs: None, ..sample }).collect();
    assert_eq!(replay(&blind, AT_1440_60, QUIET_START)[0].1.unwrap().to, setting(ShareQuality::P1080, 15));
}

#[test]
fn credits_nothing_for_a_rung_already_sending_less_than_it_would_be_allotted() {
    let said = replay(&under_sixty(30.0 * M, 18.0 * M, 2.5 * M, 5.5 * M), AT_1440_60, QUIET_START);

    assert!(ask(ShareQuality::P1440, 15) > 0.8 * 30.0 * M - 8.0 * M);
    assert_eq!(said[0].1.unwrap().to, setting(ShareQuality::P1080, 30));
}

#[test]
fn gives_nothing_back_to_a_step_that_keeps_sixty_frames_nor_for_the_720p_rung() {
    let said = replay(&under_sixty(37.0 * M, 24.0 * M, 2.5 * M, 8.0 * M), AT_1440_60, QUIET_START);

    assert!(ask(ShareQuality::P1080, 60) > 0.8 * 37.0 * M - 10.5 * M);
    assert!(ask(ShareQuality::P1080, 60) <= 0.8 * 37.0 * M - 10.5 * M + 0.6 * M);
    assert_eq!(said[0].1.unwrap().to, setting(ShareQuality::P1440, 15));
}

/* -------------------------------------------------------------------------- */
/* What is offered                                                            */
/* -------------------------------------------------------------------------- */

fn area(setting: &ShareSetting) -> u64 {
    let size = share_size_of(setting.quality);
    size.width as u64 * size.height as u64
}

fn rate(setting: &ShareSetting) -> u32 {
    if setting.quality == ShareQuality::Auto {
        60
    } else {
        setting.frame_rate
    }
}

fn nothing_back(_: &ShareSetting) -> f64 {
    0.0
}

fn display(width: u32, height: u32, frame_rate: Option<f64>) -> ShareCapture {
    ShareCapture { width: Some(width), height: Some(height), frame_rate }
}

#[test]
fn is_never_a_larger_picture_or_more_frames_than_were_chosen() {
    let chosen = [
        setting(ShareQuality::P4k, 60),
        setting(ShareQuality::P4k, 15),
        AT_1440_120,
        setting(ShareQuality::P1080, 240),
        setting(ShareQuality::P1080, 30),
        setting(ShareQuality::Auto, 240),
    ];

    for from in chosen {
        let mut budget = 0.0;
        while budget <= 100.0 * M {
            if let Some(to) = suggest(from, budget, display(3840, 2160, None), nothing_back) {
                assert!(area(&to) <= area(&from));
                assert!(rate(&to) <= rate(&from));
                assert_ne!(to, from);
                assert!(share_ask_of(to, display(3840, 2160, None)) as f64 <= budget);
            }
            budget += 0.5 * M;
        }
    }
}

#[test]
fn is_the_most_the_line_will_hold() {
    let capture = display(2560, 1440, Some(118.0));
    assert_eq!(suggest(AT_1440_120, 26.6 * M, capture, nothing_back), Some(setting(ShareQuality::P1440, 30)));
    assert_eq!(suggest(AT_1440_120, 21.0 * M, capture, nothing_back), Some(AT_1080_60));
}

#[test]
fn is_nothing_at_all_when_nothing_fits() {
    assert_eq!(suggest(AT_1440_120, 9.0 * M, display(2560, 1440, Some(118.0)), nothing_back), None);
}

#[test]
fn never_offers_a_rate_the_display_was_not_giving_anyway() {
    // 120 chosen on a sixty-hertz display: "1440p at sixty" would change the
    // label and nothing that is sent.
    let to = suggest(AT_1440_120, 40.0 * M, display(2560, 1440, Some(60.0)), nothing_back);
    assert_ne!(to, Some(AT_1440_60));
    assert_eq!(to, Some(setting(ShareQuality::P1440, 30)));
}

#[test]
fn names_the_smaller_picture_when_two_would_send_the_same_one() {
    let screen = display(1920, 1080, Some(60.0));

    assert_eq!(
        suggest(setting(ShareQuality::Auto, 60), 13.0 * M, screen, nothing_back),
        Some(setting(ShareQuality::P1080, 15))
    );
    assert_eq!(suggest(AT_1440_60, 17.0 * M, screen, nothing_back), Some(setting(ShareQuality::P1080, 30)));

    // Where the display has the larger picture to give, the larger name is
    // what would be sent, and it stays.
    assert_eq!(
        suggest(AT_1440_60, 18.0 * M, display(2560, 1440, Some(60.0)), nothing_back),
        Some(setting(ShareQuality::P1440, 15))
    );
}

#[test]
fn lists_the_steps_down_largest_first_and_fastest_first() {
    let steps = share_steps_down(AT_1440_120);

    assert_eq!(steps.first(), Some(&AT_1440_60));
    assert_eq!(steps.last(), Some(&setting(ShareQuality::P1080, 15)));
    assert!(!steps.iter().any(|step| step.quality == ShareQuality::P4k));
}

/*
 * The price used to decide whether a setting fits has to be the price the
 * retune will actually set. They are one function; this is what notices if
 * somebody makes them two again.
 */
#[test]
fn prices_a_setting_the_way_the_retune_applies_it() {
    for (width, height, frame_rate) in [(2560, 1440, 120.0), (1920, 1080, 60.0), (1366, 768, 60.0)] {
        let capture = display(width, height, Some(frame_rate));
        for to in share_steps_down(setting(ShareQuality::P4k, 60)) {
            // The capture as a display source answers after being narrowed.
            let asked = share_retune_capture(to.frame_rate, to.quality);
            let settled = display(width.min(asked.max_width), height.min(asked.max_height), Some(frame_rate));
            let retuned = share_retune_encodings(to.frame_rate, to.quality, settled, 3);

            assert_eq!(retuned.encodings[2].max_bitrate, share_ask_of(to, capture), "{to:?} on {width}x{height}");
        }
    }
}

#[test]
fn says_megabits_the_way_the_card_does() {
    assert_eq!(strain_megabits(37_000_000.0), "37");
    assert_eq!(strain_megabits(62_473_708.0), "62");
    assert_eq!(strain_megabits(9_677_294.0), "9.7");
    // An exact half goes up, as toFixed has it.
    assert_eq!(strain_megabits(2_250_000.0), "2.3");
    assert_eq!(strain_megabits(350_000.0), "0.3");
}

/* -------------------------------------------------------------------------- */
/* The watch, over a share                                                    */
/* -------------------------------------------------------------------------- */

/// What the sender's figures are, given the top layer's ceiling as it stands.
type Shaping = Box<dyn Fn(f64) -> Box<dyn Fn(&mut StrainSample)>>;

/// One share and the watch over it, driven the way the media crate drives
/// them: the record of the share, a sender whose report says whatever the
/// friend's share says, a reading every two seconds while sharing, and the
/// retune the menu and the card both go through.
struct Sharing {
    now: f64,
    run: std::sync::Arc<ShareRun>,
    watch: std::sync::Arc<ShareStrainWatch>,
    capture: (u32, u32, f64),
    /// What the capture was last narrowed to.
    asked: (u32, u32),
    encodings: Vec<StrainEncoding>,
    sid: String,
    sharing: bool,
    next: Option<f64>,
    last_stats: f64,
    totals: (f64, f64, f64, f64),
    shaping: Shaping,
    writes: usize,
}

impl Sharing {
    fn new(shaping: Shaping) -> Self {
        let small = share_rung_small().max_bitrate as f64;
        let middle = share_rung_middle_fast().max_bitrate as f64;
        Sharing {
            now: 0.0,
            run: ShareRun::new(),
            watch: ShareStrainWatch::new(),
            capture: (2560, 1440, 120.0),
            asked: (2560, 1440),
            encodings: vec![encoding("q", true, small), encoding("h", true, middle), encoding("f", true, 48_660_000.0)],
            sid: "TR_1".into(),
            sharing: false,
            next: None,
            last_stats: 0.0,
            totals: (0.0, 0.0, 0.0, 0.0),
            shaping,
            writes: 0,
        }
    }

    fn friend() -> Self {
        Self::new(Box::new(|_| Box::new(|_| {})))
    }

    /// The share starts, as `share()` records it.
    fn share(&mut self, quality: ShareQuality, frame_rate: u32) {
        self.run.begin(quality, frame_rate, self.now);
        self.watch.running_changed(self.run.current());
    }

    /// The share's picture is in the room (or gone from it), which starts or
    /// stops the reading every two seconds.
    fn set_sharing(&mut self, on: bool) {
        self.sharing = on;
        if on {
            self.next = Some(self.now + EVERY);
        } else {
            self.next = None;
            self.watch.sharing_stopped();
        }
    }

    fn stats(&mut self) -> StrainReport {
        let seconds = (self.now - self.last_stats) / 1000.0;
        self.last_stats = self.now;

        let mut shaped = strained(self.now);
        (self.shaping)(self.encodings[2].max_bitrate.unwrap())(&mut shaped);
        self.totals.0 += shaped.source.unwrap() * seconds;
        self.totals.1 += shaped.sent.unwrap() * seconds;
        self.totals.2 += shaped.spent.unwrap() / 8.0 * seconds;
        self.totals.3 += shaped.rest.unwrap() / 8.0 * seconds;

        let now = Some(self.now);
        StrainReport {
            transports: vec![StrainTransport { id: s("T"), selected_candidate_pair_id: s("P") }],
            candidate_pairs: vec![StrainCandidatePair {
                id: s("P"),
                available_outgoing_bitrate: shaped.available,
                ..Default::default()
            }],
            media_sources: vec![StrainMediaSource {
                id: s("S"),
                kind: s("video"),
                frames: Some(self.totals.0.round()),
                frames_per_second: None,
                width: Some(self.capture.0 as f64),
                height: Some(self.capture.1 as f64),
            }],
            outbound: vec![
                StrainOutbound {
                    id: s("Q"),
                    rid: s("q"),
                    kind: s("video"),
                    timestamp: now,
                    bytes_sent: Some(self.totals.3.round()),
                    frames_sent: Some(0.0),
                    target_bitrate: Some(share_rung_small().max_bitrate as f64),
                    quality_limitation_reason: shaped.reason.clone(),
                    media_source_id: s("S"),
                    ..Default::default()
                },
                StrainOutbound {
                    id: s("F"),
                    rid: s("f"),
                    kind: s("video"),
                    timestamp: now,
                    bytes_sent: Some(self.totals.2.round()),
                    frames_sent: Some(self.totals.1.round()),
                    target_bitrate: shaped.target,
                    quality_limitation_reason: shaped.reason.clone(),
                    ..Default::default()
                },
            ],
        }
    }

    fn look(&mut self) {
        let begun = self.watch.begin(self.run.current(), Some(self.sid.clone()), self.now);
        if begun.read {
            let report = self.stats();
            self.watch.reading(report, self.encodings.clone(), self.now);
        }
    }

    fn pass(&mut self, ms: f64) {
        let end = self.now + ms;
        while let Some(next) = self.next.filter(|next| *next <= end) {
            self.now = next;
            self.next = Some(next + EVERY);
            self.look();
        }
        self.now = end;
    }

    /// The retune the menu and the card both go through.
    fn retune(&mut self, frame_rate: u32, quality: ShareQuality) -> bool {
        let asked = share_retune_capture(frame_rate, quality);
        self.asked = (asked.max_width, asked.max_height);
        let settled = display(self.capture.0.min(self.asked.0), self.capture.1.min(self.asked.1), Some(self.capture.2));

        let retuned = share_retune_encodings(frame_rate, quality, settled, self.encodings.len() as u32);
        for update in &retuned.encodings {
            self.encodings[update.index as usize].max_bitrate = Some(update.max_bitrate as f64);
        }
        self.writes += 1;

        self.run.change(quality, frame_rate, self.now);
        self.watch.running_changed(self.run.current());
        true
    }

    /// The card's button, pressed.
    fn press(&mut self) -> StrainAccept {
        let said = self.watch.accept(self.run.current());
        if let StrainAccept::Apply { setting } = said {
            let took = self.retune(setting.frame_rate, setting.quality);
            self.watch.accepted(took);
        }
        said
    }

    fn hint(&self) -> Option<StrainHint> {
        self.watch.hint()
    }

    fn setting(&self) -> ShareSetting {
        self.run.current().unwrap().setting()
    }
}

fn started() -> Sharing {
    let mut sharing = Sharing::friend();
    sharing.share(ShareQuality::P1440, 120);
    sharing.set_sharing(true);
    sharing
}

#[test]
fn says_nothing_to_somebody_who_is_not_sharing() {
    let mut sharing = Sharing::friend();
    sharing.share(ShareQuality::P1440, 120);
    sharing.pass(120_000.0);
    assert_eq!(sharing.hint(), None);
}

#[test]
fn speaks_after_the_quiet_time_and_the_evidence_and_changes_nothing_by_itself() {
    let mut sharing = started();
    let encodings = sharing.encodings.clone();

    sharing.pass(QUIET_START + WINDOW - 4_000.0);
    assert_eq!(sharing.hint(), None);

    sharing.pass(8_000.0);
    assert_eq!(sharing.hint().map(|hint| hint.to), Some(setting(ShareQuality::P1440, 15)));

    // A hint: nothing about the share has been touched.
    assert_eq!(sharing.encodings, encodings);
    assert_eq!(sharing.writes, 0);
    assert_eq!(sharing.setting(), AT_1440_120);
}

#[test]
fn applies_the_setting_offered_and_only_when_asked() {
    let mut sharing = started();
    sharing.pass(60_000.0);

    let offered = sharing.hint().expect("a hint").to;
    assert!(matches!(sharing.press(), StrainAccept::Apply { setting } if setting == offered));

    assert_eq!(sharing.writes, 1);
    assert_eq!(sharing.encodings[2].max_bitrate, Some(share_ask_of(offered, display(2560, 1440, Some(120.0))) as f64));
    assert_eq!(sharing.setting(), offered);
    assert_eq!(sharing.hint(), None);
}

#[test]
fn keeps_the_card_up_beside_a_change_that_did_not_take() {
    let mut sharing = started();
    sharing.pass(60_000.0);

    assert!(matches!(sharing.watch.accept(sharing.run.current()), StrainAccept::Apply { .. }));
    // A second press while the first is under way is not a second change.
    assert_eq!(sharing.watch.accept(sharing.run.current()), StrainAccept::Ignored);

    assert!(!sharing.watch.accepted(false));
    assert!(sharing.hint().is_some());
    assert!(matches!(sharing.watch.accept(sharing.run.current()), StrainAccept::Apply { .. }));
}

#[test]
fn puts_the_question_away_for_the_rest_of_the_share_and_not_for_the_next_one() {
    let mut sharing = started();
    sharing.pass(60_000.0);
    assert!(sharing.hint().is_some());

    sharing.watch.decline();
    assert_eq!(sharing.hint(), None);

    // The line is just as bad for the next five minutes of this share.
    sharing.pass(300_000.0);
    assert_eq!(sharing.hint(), None);

    // A new share is a new question, with its own quiet time first.
    sharing.sid = "TR_2".into();
    sharing.share(ShareQuality::P1440, 120);

    sharing.pass(QUIET_START);
    assert_eq!(sharing.hint(), None);

    sharing.pass(WINDOW + 4_000.0);
    assert!(sharing.hint().is_some());
}

#[test]
fn does_not_bring_the_card_back_for_a_reading_under_way_when_not_now_was_said() {
    // This client's own, and a departure from the web: there a reading already
    // under way when "Not now" was pressed was still judged, and could put the
    // card straight back up for the rest of the share. Here it is dropped.
    let mut sharing = started();
    sharing.pass(60_000.0);
    assert!(sharing.hint().is_some());

    sharing.now += 1_000.0;
    let begun = sharing.watch.begin(sharing.run.current(), Some(sharing.sid.clone()), sharing.now);
    assert!(begun.read);

    sharing.watch.decline();
    let report = sharing.stats();
    assert_eq!(sharing.watch.reading(report, sharing.encodings.clone(), sharing.now), StrainUpdate::Keep);
    assert_eq!(sharing.hint(), None);
}

/*
 * A full reconnect, as the SDK does it: every local track unpublished and
 * published again, the same capture under a new sid, and the share never
 * started again. The picture leaves the room and comes back.
 */
#[test]
fn keeps_not_now_through_a_reconnect_which_is_the_same_share_published_again() {
    let mut sharing = started();
    sharing.pass(60_000.0);
    assert!(sharing.hint().is_some());

    sharing.watch.decline();

    sharing.set_sharing(false);
    sharing.sid = "TR_after_reconnect".into();
    sharing.pass(EVERY);
    sharing.set_sharing(true);

    sharing.pass(300_000.0);
    assert_eq!(sharing.hint(), None);
}

#[test]
fn owes_a_share_published_again_the_quiet_time_a_new_share_is_owed() {
    // The new connection's estimate starts from nothing and climbs, exactly as
    // a new share's does.
    let mut sharing = started();
    sharing.pass(60_000.0);
    assert!(sharing.hint().is_some());

    sharing.sid = "TR_after_reconnect".into();
    sharing.pass(EVERY);

    // Down, because it was about a connection that is gone.
    assert_eq!(sharing.hint(), None);

    sharing.pass(QUIET_START + WINDOW - 4_000.0);
    assert_eq!(sharing.hint(), None);

    sharing.pass(8_000.0);
    assert!(sharing.hint().is_some());
}

#[test]
fn takes_the_card_down_the_moment_the_menu_changes_the_share_and_a_press_then_does_nothing() {
    let mut sharing = started();
    sharing.pass(60_000.0);
    assert_eq!(sharing.hint().map(|hint| hint.to), Some(setting(ShareQuality::P1440, 15)));

    // Somebody picks 1080p at fifteen from the menu instead. No time passes:
    // the card goes with the change, not at the next reading.
    sharing.retune(15, ShareQuality::P1080);
    assert_eq!(sharing.hint(), None);

    // And the press already on its way changes nothing, least of all upward.
    let writes = sharing.writes;
    assert_eq!(sharing.press(), StrainAccept::Ignored);
    assert_eq!(sharing.writes, writes);
    assert_eq!(sharing.setting(), setting(ShareQuality::P1080, 15));
}

#[test]
fn drops_a_press_about_a_share_that_moved_before_the_card_went() {
    // The menu's change recorded but the card not yet told: the press is
    // about a setting no longer being sent.
    let mut sharing = started();
    sharing.pass(60_000.0);
    assert!(sharing.hint().is_some());

    sharing.run.change(ShareQuality::P1080, 15, sharing.now);
    assert_eq!(sharing.watch.accept(sharing.run.current()), StrainAccept::Dropped);
    assert_eq!(sharing.hint(), None);
    assert_eq!(sharing.writes, 0);
}

#[test]
fn starts_again_from_nothing_when_the_setting_is_changed_from_the_menu() {
    // Short of whatever ceiling it has, and sending as much as the line takes
    // whatever the ceiling, so the line is as full after the change as before.
    let mut sharing = Sharing::new(Box::new(|ceiling| {
        Box::new(move |sample: &mut StrainSample| {
            sample.target = Some(0.6 * ceiling);
            sample.spent = Some(30.0 * M);
        })
    }));
    sharing.share(ShareQuality::P1440, 120);
    sharing.set_sharing(true);

    sharing.pass(60_000.0);
    assert!(sharing.hint().is_some());

    // The menu, not the card: what the card said was about a setting no longer
    // being sent.
    sharing.retune(60, ShareQuality::P1440);
    sharing.pass(EVERY);
    assert_eq!(sharing.hint(), None);

    // Fifteen seconds of quiet and twenty of evidence, about the new setting.
    sharing.pass(QUIET_CHANGE + WINDOW - 4_000.0);
    assert_eq!(sharing.hint(), None);

    sharing.pass(8_000.0);
    assert_eq!(sharing.hint().map(|hint| hint.to), Some(setting(ShareQuality::P1440, 15)));
}
