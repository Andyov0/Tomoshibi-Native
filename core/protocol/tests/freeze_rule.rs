//! What these guard is a step down that is either never taken or taken for the
//! wrong reason.
//!
//! The cases are web/src/live/freeze.test.ts. The first fault is the one this
//! exists for and is invisible: a viewer whose server goes on sending the
//! original down a line that cannot carry it receives bytes and decodes
//! nothing, for a minute, and nothing reports it. The second is a picture made
//! worse by the very thing meant to help it, and the things that look like a
//! freeze from one reading are ordinary: a still screen sends nothing, a share
//! coming back from a pause arrives as bytes before frames, and the switch this
//! asks for takes a keyframe to show. Each is held apart here, reading by
//! reading.
//!
//! The room cases drive a `ShareFreezeWatch` the way the media crate does,
//! against a stand-in for the SDK's subscription that keeps what the web's real
//! publication keeps: a request only recorded while the track is here, and the
//! size sent as the smaller of the request and the picture's measurement.

use std::sync::Arc;

use tomoshibi_protocol::freeze::*;

const R720: FreezeRung = FreezeRung { width: 1280, height: 720 };
const R1080: FreezeRung = FreezeRung { width: 1920, height: 1080 };
const R1440: FreezeRung = FreezeRung { width: 2560, height: 1440 };
const LADDER: [FreezeRung; 3] = [R720, R1080, R1440];

/// The counters at one moment; `at` is filled in by whoever reads them.
#[derive(Clone, Copy, Default)]
struct Counters {
    frames: Option<f64>,
    bytes: Option<f64>,
    received: Option<f64>,
    lost: Option<f64>,
    freezes: Option<f64>,
}

impl Counters {
    fn at(self, at: f64) -> FreezeReading {
        FreezeReading {
            at,
            bytes: self.bytes,
            frames: self.frames,
            received: self.received,
            lost: self.lost,
            freezes: self.freezes,
        }
    }
}

type Act = (f64, FreezeAct);

/// Feed readings one second apart through the rule, and say what it did.
fn run(
    from: f64,
    to: f64,
    shape: impl Fn(f64) -> Counters,
    rungs: &[FreezeRung],
    watch: FreezeWatch,
    asked: Option<usize>,
) -> (Vec<Act>, FreezeWatch) {
    let mut acts = Vec::new();
    let mut current = watch;
    let mut at = from;
    while at <= to {
        let (next, act) = judge(&current, Some(&shape(at).at(at)), rungs, at, asked);
        current = next;
        if let Some(act) = act {
            acts.push((at, act));
        }
        at += FREEZE_EVERY_MS;
    }
    (acts, current)
}

fn plain(from: f64, to: f64, shape: impl Fn(f64) -> Counters) -> Vec<Act> {
    run(from, to, shape, &LADDER, fresh(0.0, None), None).0
}

fn cap(rung: FreezeRung) -> FreezeAct {
    FreezeAct::Cap { rung }
}

fn acts_only(acts: &[Act]) -> Vec<FreezeAct> {
    acts.iter().map(|(_, act)| *act).collect()
}

/// A share decoding thirty frames a second, megabits arriving, nothing frozen.
fn moving(at: f64) -> Counters {
    Counters {
        frames: Some(at / 1000.0 * 30.0),
        bytes: Some(at / 1000.0 * 2_000_000.0),
        received: Some(at / 1000.0 * 2000.0),
        lost: Some(0.0),
        freezes: Some(0.0),
    }
}

/// Frozen from `since` to `until`: bytes go on arriving and frames stand still.
fn frozen_between(since: f64, until: f64) -> impl Fn(f64) -> Counters {
    move |at| {
        let decoded = if at < since {
            at
        } else if at < until {
            since
        } else {
            at - (until - since)
        };
        Counters {
            frames: Some(decoded / 1000.0 * 30.0),
            bytes: Some(at / 1000.0 * 2_000_000.0),
            received: Some(at / 1000.0 * 2000.0),
            lost: Some(0.0),
            freezes: None,
        }
    }
}

/// Frozen from `since`: bytes go on arriving and frames stand still.
fn frozen_from(since: f64) -> impl Fn(f64) -> Counters {
    move |at| Counters {
        frames: Some(at.min(since) / 1000.0 * 30.0),
        bytes: Some(at / 1000.0 * 2_000_000.0),
        received: Some(at / 1000.0 * 2000.0),
        lost: Some(0.0),
        freezes: None,
    }
}

/// Moving, losing `share` of its packets, and freezing once a second where
/// `freezing`: the receiver's own count of a frame that came far too late.
fn lossy(share: f64, freezing: bool) -> impl Fn(f64) -> Counters {
    move |at| Counters {
        lost: Some(at / 1000.0 * 2000.0 * share),
        freezes: Some(if freezing { at / 1000.0 } else { 0.0 }),
        ..moving(at)
    }
}

/* -------------------------------------------------------------------------- */
/* What counts as a freeze                                                    */
/* -------------------------------------------------------------------------- */

#[test]
fn steps_down_once_frames_have_stood_still_for_two_seconds_while_bytes_kept_coming() {
    let acts = plain(0.0, 10_000.0, frozen_from(4_000.0));

    assert_eq!(acts_only(&acts), vec![cap(R1080)]);
    // Not before two whole seconds of it: 4000 was the last reading to see a
    // frame, and 6000 is the first two seconds after.
    assert_eq!(acts[0].0, 4_000.0 + 2_000.0);
}

#[test]
fn says_nothing_about_a_share_that_is_moving() {
    assert!(plain(0.0, 120_000.0, moving).is_empty());
}

#[test]
fn does_not_take_a_still_screen_for_a_freeze() {
    // A still screen sends nothing and decodes nothing: both counters stand.
    let acts = plain(0.0, 120_000.0, |at| Counters {
        frames: Some(at.min(3_000.0) / 100.0),
        bytes: Some(at.min(3_000.0) * 100.0),
        received: Some(at.min(3_000.0)),
        lost: Some(0.0),
        freezes: None,
    });
    assert!(acts.is_empty());
}

#[test]
fn does_not_take_a_share_coming_back_from_a_pause_for_a_freeze() {
    // Paused at 3 s and resumed at 30 s, when bytes come a second before the
    // keyframe that lets frames be decoded again. Counted from the last frame,
    // that second is a stall of twenty-seven; it is a stall of one.
    let acts = plain(0.0, 60_000.0, |at| {
        let arrived = if at < 3_000.0 {
            at
        } else if at < 30_000.0 {
            3_000.0
        } else {
            at - 27_000.0
        };
        let decoded = if at < 3_000.0 {
            at
        } else if at <= 31_000.0 {
            3_000.0
        } else {
            at - 28_000.0
        };
        Counters {
            frames: Some(decoded / 1000.0 * 30.0),
            bytes: Some(arrived / 1000.0 * 2_000_000.0),
            ..Default::default()
        }
    });
    assert!(acts.is_empty());
}

#[test]
fn does_not_take_a_trickle_for_a_share_arriving() {
    // A few hundred bytes a second, RTCP's company, while nothing is decoded.
    let acts = plain(0.0, 60_000.0, |at| Counters {
        frames: Some(90.0),
        bytes: Some(50_000.0 + at / 5.0),
        ..Default::default()
    });
    assert!(acts.is_empty());
}

#[test]
fn steps_down_on_a_tenth_of_the_packets_lost_while_the_picture_freezes_even_as_frames_move() {
    assert_eq!(plain(0.0, 10_000.0, lossy(0.15, true)).first().map(|one| one.1), Some(cap(R1080)));
    assert!(plain(0.0, 60_000.0, lossy(0.05, true)).is_empty());
}

#[test]
fn does_not_step_down_on_loss_that_retransmission_brought_back() {
    // Twelve per cent lost and all of it recovered in time: the receiver goes
    // on counting the packets lost, and counts no freeze, because the picture
    // had none.
    assert!(plain(0.0, 120_000.0, lossy(0.12, false)).is_empty());
}

#[test]
fn does_not_take_loss_for_harm_in_a_receiver_that_does_not_count_freezes() {
    let acts = plain(0.0, 60_000.0, |at| Counters { freezes: None, ..lossy(0.3, false)(at) });
    assert!(acts.is_empty());
}

#[test]
fn does_not_judge_loss_over_a_handful_of_packets() {
    // Twenty packets a second, a third of them lost: a still screen's
    // keep-alives on a bad second, not a share that cannot be carried.
    let acts = plain(0.0, 60_000.0, |at| Counters {
        frames: Some(at / 100.0),
        bytes: Some(at * 10.0),
        received: Some(at / 1000.0 * 20.0),
        lost: Some(at / 1000.0 * 10.0),
        freezes: Some(at / 1000.0),
    });
    assert!(acts.is_empty());
}

#[test]
fn takes_a_counter_that_went_backwards_for_a_new_stream_not_a_freeze() {
    // Resubscribed at 10 s: everything starts again from nothing.
    let acts = plain(0.0, 30_000.0, |at| if at < 10_000.0 { moving(at) } else { moving(at - 10_000.0) });
    assert!(acts.is_empty());
}

#[test]
fn does_not_judge_a_replaced_stream_on_what_the_old_one_lost() {
    // This client's own. The case above passes with the replacement guard
    // removed, in the web as here, because a counter that went backwards
    // already gains nothing. What the guard keeps out is the old stream's loss:
    // here it lost 40 of 90 packets and froze once, too few packets to judge,
    // and a tick that ran late puts the replacement half a second after it.
    // Kept, those 90 packets would be judged together with the new stream's
    // first 60 as 27% lost and a picture that froze, and step down a stream
    // that has lost nothing.
    let readings = [
        FreezeReading {
            at: 0.0,
            frames: Some(0.0),
            bytes: Some(0.0),
            received: Some(0.0),
            lost: Some(0.0),
            freezes: Some(0.0),
        },
        FreezeReading {
            at: 1_000.0,
            frames: Some(30.0),
            bytes: Some(90_000.0),
            received: Some(50.0),
            lost: Some(40.0),
            freezes: Some(1.0),
        },
        FreezeReading {
            at: 1_500.0,
            frames: Some(1.0),
            bytes: Some(100.0),
            received: Some(5.0),
            lost: Some(0.0),
            freezes: Some(0.0),
        },
        FreezeReading {
            at: 2_000.0,
            frames: Some(16.0),
            bytes: Some(60_000.0),
            received: Some(65.0),
            lost: Some(0.0),
            freezes: Some(0.0),
        },
    ];

    let mut watch = fresh(0.0, None);
    for reading in readings {
        let (next, act) = judge(&watch, Some(&reading), &LADDER, reading.at, None);
        assert_eq!(act, None, "at {}", reading.at);
        watch = next;
    }
}

/* -------------------------------------------------------------------------- */
/* What is asked for instead                                                  */
/* -------------------------------------------------------------------------- */

#[test]
fn is_the_rung_beneath_the_one_this_viewers_subscription_asks_for() {
    // A picture sized for the 1080p rung freezes: the step is to 720.
    let (acts, _) = run(0.0, 10_000.0, frozen_from(4_000.0), &LADDER, fresh(0.0, None), Some(1));
    assert_eq!(acts[0].1, cap(R720));
}

#[test]
fn is_the_rung_beneath_the_top_for_a_viewer_at_full_size_whatever_height_its_frames_arrive_at() {
    // A share published at 1440p and set to 1080p afterwards sends its top at
    // 1080 lines. The request says the top, and the frames are not read.
    let (acts, _) = run(0.0, 6_000.0, frozen_from(2_000.0), &LADDER, fresh(0.0, None), Some(2));
    assert_eq!(acts[0].1, cap(R1080));
    assert_eq!(plain(0.0, 6_000.0, frozen_from(2_000.0))[0].1, cap(R1080));
}

#[test]
fn is_the_rung_beneath_the_original_when_nothing_has_been_decoded_at_all() {
    // A minute of bytes and not one frame, from the start.
    let acts = plain(0.0, 10_000.0, |at| Counters {
        frames: Some(0.0),
        bytes: Some(at / 1000.0 * 2_000_000.0),
        ..Default::default()
    });
    assert_eq!(acts[0].1, cap(R1080));
}

#[test]
fn is_nothing_at_all_for_a_share_with_no_rungs_beneath_it() {
    // A window of 720 lines or less is published without rungs; asking anyway
    // would be asking for the same picture every second.
    let (acts, _) = run(0.0, 60_000.0, frozen_from(4_000.0), &[R720], fresh(0.0, None), None);
    assert!(acts.is_empty());
}

#[test]
fn steps_down_again_when_the_lower_rung_freezes_too_but_not_before_it_has_had_time_to_arrive() {
    let acts = plain(0.0, 30_000.0, frozen_from(4_000.0));

    assert_eq!(acts_only(&acts), vec![cap(R1080), cap(R720)]);
    assert!(acts[1].0 - acts[0].0 >= FREEZE_SETTLE_MS + FREEZE_STALL_MS);
}

#[test]
fn does_not_judge_loss_again_until_the_rung_it_asked_for_has_had_time_to_arrive() {
    let acts = plain(0.0, 20_000.0, lossy(0.15, true));

    assert_eq!(acts[0].1, cap(R1080));
    assert_eq!(acts[1].1, cap(R720));
    assert!(acts[1].0 - acts[0].0 >= FREEZE_SETTLE_MS);
}

#[test]
fn never_goes_below_the_bottom_rung() {
    let acts = plain(0.0, 6_000.0 + FREEZE_HOLD_MS - FREEZE_EVERY_MS, frozen_from(4_000.0));
    assert_eq!(acts_only(&acts), vec![cap(R1080), cap(R720)]);
}

/* -------------------------------------------------------------------------- */
/* Asking for the original again                                              */
/* -------------------------------------------------------------------------- */

#[test]
fn asks_for_the_original_twenty_seconds_after_the_step_down_and_not_before() {
    // Frozen until the step down at 6 s takes; then the lower rung moves.
    let acts = plain(0.0, 60_000.0, frozen_between(4_000.0, 7_000.0));

    assert_eq!(acts_only(&acts), vec![cap(R1080), FreezeAct::Release]);
    assert_eq!(acts[1].0 - acts[0].0, FREEZE_HOLD_MS);
    // The owner's number, written out: a test that took it from the constant
    // it checks would move with it.
    assert_eq!(FREEZE_HOLD_MS, 20_000.0);
}

#[test]
fn asks_for_the_original_whether_or_not_anybody_is_watching() {
    // Capped, then off the stage: no readings at all. The clock still runs.
    let (_, capped) = run(0.0, 6_000.0, frozen_from(4_000.0), &LADDER, fresh(0.0, None), None);

    assert_eq!(judge(&capped, None, &LADDER, 6_000.0 + FREEZE_HOLD_MS - 1.0, None).1, None);
    assert_eq!(judge(&capped, None, &LADDER, 6_000.0 + FREEZE_HOLD_MS, None).1, Some(FreezeAct::Release));
}

/// A viewer driven second by second on a line described by `carries(at)`:
/// where it says no, the original stands still `after` ms after it was asked
/// for while any lower rung moves. How long each cap was held, in order.
fn holds(to: f64, carries: impl Fn(f64) -> bool, after: f64) -> Vec<f64> {
    let mut watch = fresh(0.0, None);
    let mut frames = 0.0;
    let mut asked = 0.0;
    let mut capped: Option<f64> = None;
    let mut held = Vec::new();

    let mut at = 0.0;
    while at <= to {
        if watch.cap.is_some() || carries(at) || at - asked < after {
            frames += 30.0;
        }

        let reading = FreezeReading {
            at,
            frames: Some(frames),
            bytes: Some(at * 2_000.0),
            received: Some(at * 2.0),
            lost: Some(0.0),
            freezes: None,
        };
        let (next, act) = judge(&watch, Some(&reading), &LADDER, at, None);
        watch = next;

        match act {
            Some(FreezeAct::Cap { .. }) if capped.is_none() => capped = Some(at),
            Some(FreezeAct::Release) => {
                if let Some(since) = capped.take() {
                    held.push(at - since);
                }
                asked = at;
            }
            _ => {}
        }
        at += FREEZE_EVERY_MS;
    }
    held
}

/// Readings one second apart from `from` until the rule first acts.
fn first_step(watch: FreezeWatch, from: f64, shape: impl Fn(f64) -> Counters) -> (Vec<Act>, FreezeWatch) {
    let mut acts = Vec::new();
    let mut current = watch;
    let mut at = from;
    while at <= from + 60_000.0 && acts.is_empty() {
        let (next, act) = judge(&current, Some(&shape(at).at(at)), &LADDER, at, None);
        current = next;
        if let Some(act) = act {
            acts.push((at, act));
        }
        at += FREEZE_EVERY_MS;
    }
    (acts, current)
}

#[test]
fn waits_twice_as_long_each_time_the_original_freezes_again_up_to_five_minutes() {
    assert_eq!(
        holds(1_400_000.0, |_| false, 10_000.0),
        vec![20_000.0, 40_000.0, 80_000.0, 160_000.0, 300_000.0, 300_000.0, 300_000.0]
    );
    assert_eq!(FREEZE_LONGEST_HOLD_MS, 300_000.0);
}

#[test]
fn starts_again_from_twenty_seconds_once_the_original_has_held_for_two_minutes() {
    // Bad for the first 200 s, good until 600 s, bad again after: the trouble
    // at 600 s is met with twenty seconds, not five minutes.
    let line = |at: f64| (200_000.0..600_000.0).contains(&at);
    assert_eq!(
        holds(800_000.0, line, 10_000.0),
        vec![20_000.0, 40_000.0, 80_000.0, 160_000.0, 20_000.0, 40_000.0, 80_000.0]
    );
}

#[test]
fn counts_a_freeze_long_after_a_return_as_a_new_trouble_rather_than_a_failed_return() {
    assert_eq!(holds(1_000_000.0, |_| false, FREEZE_TRIED_MS + 30_000.0), vec![20_000.0; 5]);
}

#[test]
fn does_not_take_a_second_step_under_one_cap_for_a_second_failed_return() {
    // One failed return and two steps: the hold stays forty, counted from the
    // second step, rather than doubling again to eighty.
    let (_, capped) = run(0.0, 6_000.0, frozen_from(4_000.0), &LADDER, fresh(0.0, None), None);
    let mut watch = judge(&capped, None, &LADDER, 26_000.0, None).0;

    let mut acts = Vec::new();
    let mut at = 30_000.0;
    while at <= 120_000.0 {
        let reading = frozen_from(6_000.0)(at - 30_000.0).at(at);
        let (next, act) = judge(&watch, Some(&reading), &LADDER, at, None);
        watch = next;
        if let Some(act) = act {
            acts.push((at, act));
        }
        at += FREEZE_EVERY_MS;
    }

    assert_eq!(acts_only(&acts[..3]), vec![cap(R1080), cap(R720), FreezeAct::Release]);
    assert_eq!(acts[2].0 - acts[1].0, 2.0 * FREEZE_HOLD_MS);
}

#[test]
fn remembers_a_failed_return_through_a_share_taken_off_the_stage() {
    let (_, capped) = run(0.0, 6_000.0, frozen_from(4_000.0), &LADDER, fresh(0.0, None), None);
    let (released, act) = judge(&capped, None, &LADDER, 26_000.0, None);
    assert_eq!(act, Some(FreezeAct::Release));

    let watch = judge(&released, None, &LADDER, 27_000.0, None).0;
    let (acts, after) = first_step(watch, 30_000.0, |at| frozen_from(6_000.0)(at - 30_000.0));

    assert!(matches!(acts[0].1, FreezeAct::Cap { .. }));
    assert_eq!(after.cap.unwrap().until, acts[0].0 + 2.0 * FREEZE_HOLD_MS);
}

#[test]
fn remembers_a_failed_return_through_a_stream_that_was_replaced() {
    let (_, capped) = run(0.0, 6_000.0, frozen_from(4_000.0), &LADDER, fresh(0.0, None), None);
    let reading = frozen_between(4_000.0, 7_000.0)(26_000.0).at(26_000.0);
    let (released, act) = judge(&capped, Some(&reading), &LADDER, 26_000.0, None);
    assert_eq!(act, Some(FreezeAct::Release));

    let (acts, after) = first_step(released, 30_000.0, |at| frozen_from(6_000.0)(at - 30_000.0));

    assert!(matches!(acts[0].1, FreezeAct::Cap { .. }));
    assert_eq!(after.cap.unwrap().until, acts[0].0 + 2.0 * FREEZE_HOLD_MS);
}

/* -------------------------------------------------------------------------- */
/* Reading the receiver                                                       */
/* -------------------------------------------------------------------------- */

#[test]
fn reads_the_video_streams_counters_and_nothing_that_is_not_a_number() {
    let entries = vec![
        FreezeInbound {
            kind: "audio".into(),
            bytes_received: Some(9e9),
            frames_decoded: Some(0.0),
            ..Default::default()
        },
        FreezeInbound {
            kind: "video".into(),
            bytes_received: Some(123.0),
            frames_decoded: Some(4.0),
            packets_received: Some(5.0),
            packets_lost: Some(1.0),
            freeze_count: Some(2.0),
        },
    ];

    assert_eq!(
        freeze_read_inbound(entries, 7.0),
        FreezeReading {
            at: 7.0,
            bytes: Some(123.0),
            frames: Some(4.0),
            received: Some(5.0),
            lost: Some(1.0),
            freezes: Some(2.0)
        }
    );

    let unreadable = vec![FreezeInbound { kind: "video".into(), bytes_received: Some(f64::NAN), ..Default::default() }];
    assert_eq!(freeze_read_inbound(unreadable, 0.0).bytes, None);
}

#[test]
fn takes_the_rungs_from_the_publication_smallest_first_one_per_height() {
    let layers = vec![
        FreezeRung { width: 2560, height: 1440 },
        FreezeRung { width: 1280, height: 720 },
        FreezeRung { width: 1920, height: 1080 },
        FreezeRung { width: 1920, height: 1080 },
        FreezeRung { width: 0, height: 0 },
    ];
    assert_eq!(freeze_rungs_of(layers), LADDER.to_vec());
}

#[test]
fn names_the_rung_a_request_is_answered_with_by_the_servers_rule() {
    let asked = |width, height| freeze_rung_asked(LADDER.to_vec(), Some(FreezeRung { width, height }), false);

    // The smallest rung at least nine tenths as tall as asked.
    assert_eq!(asked(1400, 790), Some(0));
    assert_eq!(asked(1500, 810), Some(1));
    assert_eq!(asked(2134, 1200), Some(1));
    assert_eq!(asked(2504, 1408), Some(2));
    assert_eq!(asked(9000, 5000), Some(2));
    assert_eq!(freeze_rung_asked(LADDER.to_vec(), None, false), None);

    // Taller than wide: the request's width.
    assert_eq!(freeze_rung_asked(LADDER.to_vec(), Some(FreezeRung { width: 790, height: 1400 }), true), Some(0));
}

#[test]
fn sends_the_smaller_of_the_request_and_the_measurement() {
    let measured = FreezeRung { width: 2504, height: 1408 };

    assert_eq!(freeze_subscription_size(Some(R1080), Some(measured)), Some(R1080));
    assert_eq!(freeze_subscription_size(Some(FREEZE_UNCAPPED), Some(measured)), Some(measured));
    assert_eq!(freeze_subscription_size(None, Some(measured)), Some(measured));
    assert_eq!(freeze_subscription_size(Some(R720), None), Some(R720));
}

/* -------------------------------------------------------------------------- */
/* In a room                                                                  */
/* -------------------------------------------------------------------------- */

enum Shape {
    Fixed(Box<dyn Fn(f64) -> Counters>),
    /// A line that carries the 1080p rung and not the original: every time the
    /// original is asked for it moves for eight seconds and then stands still
    /// while bytes go on arriving.
    CannotCarryTheOriginal {
        frames: f64,
        bytes: f64,
        original: f64,
        was_original: bool,
    },
}

/// One remote share as the media crate drives it, ticking once a second from
/// one second in, with the SDK's subscription as the web's tests have it.
struct Room {
    now: f64,
    watch: Option<Arc<ShareFreezeWatch>>,
    measured: Option<FreezeRung>,
    requested: Option<FreezeRung>,
    /// What the subscription sent the server, and when.
    sent: Vec<(f64, FreezeRung)>,
    /// Every size the watch asked the subscription for.
    told: Vec<FreezeRung>,
    subscribed: bool,
    /// When the current track arrived, if there is one.
    track: Option<f64>,
    hidden: bool,
    shape: Shape,
}

impl Room {
    /// A share on a large screen at full size.
    fn watching(shape: Shape) -> Self {
        let mut room = Room {
            now: 0.0,
            watch: None,
            measured: None,
            requested: None,
            sent: Vec::new(),
            told: Vec::new(),
            subscribed: true,
            track: Some(0.0),
            hidden: false,
            shape,
        };
        room.measure(2504, 1408);
        room
    }

    fn fixed(shape: impl Fn(f64) -> Counters + 'static) -> Self {
        Self::watching(Shape::Fixed(Box::new(shape)))
    }

    fn last_sent(&self) -> Option<FreezeRung> {
        self.sent.last().map(|(_, size)| *size)
    }

    /// The SDK's `emitTrackUpdate`.
    fn emit(&mut self) {
        if let Some(size) = freeze_subscription_size(self.requested, self.measured) {
            self.sent.push((self.now, size));
        }
    }

    /// Adaptive streaming's measurement of the picture changed.
    fn measure(&mut self, width: u32, height: u32) {
        self.measured = Some(FreezeRung { width, height });
        self.emit();
    }

    /// The SDK's `setVideoDimensions`: refused while the share is not wanted,
    /// recorded only while its track is here, and sent either way.
    fn set_video_dimensions(&mut self, size: FreezeRung) {
        self.told.push(size);
        if !self.subscribed || self.requested == Some(size) {
            return;
        }
        if self.track.is_some() {
            self.requested = Some(size);
        }
        self.emit();
    }

    fn counters(&mut self) -> Counters {
        let now = self.now;
        let on_original = self.last_sent().map(|size| size.height).unwrap_or(0) > R1080.height;
        match &mut self.shape {
            Shape::Fixed(shape) => shape(now - self.track.unwrap_or(0.0)),
            Shape::CannotCarryTheOriginal { frames, bytes, original, was_original } => {
                if on_original && !*was_original {
                    *original = now;
                }
                *was_original = on_original;
                *bytes += 2_000_000.0;
                if !on_original || now - *original < 8_000.0 {
                    *frames += 30.0;
                }
                Counters {
                    frames: Some(*frames),
                    bytes: Some(*bytes),
                    received: Some(*bytes / 1000.0),
                    lost: Some(0.0),
                    freezes: None,
                }
            }
        }
    }

    fn watch(&mut self) -> Arc<ShareFreezeWatch> {
        let now = self.now;
        self.watch.get_or_insert_with(|| ShareFreezeWatch::new(now)).clone()
    }

    fn can_tell(&self) -> bool {
        self.subscribed && self.track.is_some()
    }

    fn tick(&mut self) {
        let watch = self.watch();
        let reading = (!self.hidden && self.subscribed && self.track.is_some()).then(|| self.counters().at(self.now));
        let can_tell = self.can_tell();
        if let Some(size) = watch.tick(reading, LADDER.to_vec(), self.measured, false, self.now, can_tell) {
            self.set_video_dimensions(size);
        }
    }

    /// The room says the track arrived.
    fn track_subscribed(&mut self) {
        let watch = self.watch();
        let can_tell = self.can_tell();
        if let Some(size) = watch.subscribed(can_tell) {
            self.set_video_dimensions(size);
        }
    }

    fn advance(&mut self, ms: f64) {
        let end = self.now + ms;
        loop {
            let next = (self.now / FREEZE_EVERY_MS).floor() * FREEZE_EVERY_MS + FREEZE_EVERY_MS;
            if next > end {
                break;
            }
            self.now = next;
            self.tick();
        }
        self.now = end;
    }

    /// Changes of what was asked for, capped or not, in order.
    fn changes(&self) -> Vec<(f64, bool)> {
        let mut out: Vec<(f64, bool)> = Vec::new();
        for (when, size) in &self.sent {
            let capped = size.height <= R1080.height;
            if out.last().map(|one| one.1) != Some(capped) {
                out.push((*when, capped));
            }
        }
        out
    }

    fn caps(&self) -> usize {
        self.changes().iter().filter(|one| one.1).count()
    }

    /// How long each cap was held before the original was asked for again.
    fn waits(&self) -> Vec<f64> {
        let all = self.changes();
        all.windows(2).filter(|pair| pair[0].1).map(|pair| pair[1].0 - pair[0].0).collect()
    }
}

const MEASURED: FreezeRung = FreezeRung { width: 2504, height: 1408 };

#[test]
fn asks_for_the_lower_rung_through_this_viewers_own_subscription_then_for_the_original() {
    let mut room = Room::fixed(frozen_between(4_000.0, 7_000.0));

    room.advance(4_000.0 + FREEZE_STALL_MS + FREEZE_EVERY_MS);
    assert_eq!(room.told.len(), 1);
    assert_eq!(room.last_sent(), Some(R1080));

    // Lifted: the server is asked for what the picture measures again.
    room.advance(FREEZE_HOLD_MS);
    assert_eq!(room.told.last(), Some(&FREEZE_UNCAPPED));
    assert_eq!(room.last_sent(), Some(MEASURED));
}

#[test]
fn steps_a_picture_sized_for_the_1080p_rung_straight_to_720p() {
    let mut room = Room::fixed(frozen_from(2_000.0));
    room.measure(1800, 1012);

    room.advance(2_000.0 + FREEZE_STALL_MS + FREEZE_EVERY_MS);
    assert_eq!(room.last_sent(), Some(R720));
}

#[test]
fn never_touches_a_share_that_does_not_freeze() {
    // The subscription is then exactly what it would be without this: the
    // measurement and nothing else.
    let mut room = Room::fixed(moving);
    room.advance(120_000.0);
    assert!(room.told.is_empty());
}

#[test]
fn reads_nothing_in_the_background() {
    let mut room = Room::fixed(frozen_from(2_000.0));
    room.hidden = true;
    room.advance(30_000.0);
    assert!(room.told.is_empty());
}

/// Capped, then shrunk to its card past the end of the hold, and put back:
/// wanted again at once, the track a moment later. A size told in between is
/// dropped by the SDK; told only then and recorded as told, the lift was never
/// told again and the viewer stayed a rung down until the share ended.
#[test]
fn lifts_a_cap_left_on_a_share_that_went_to_its_card() {
    for gap in [0.0, FREEZE_EVERY_MS, 3.0 * FREEZE_EVERY_MS] {
        let mut room = Room::fixed(frozen_from(2_000.0));

        room.advance(2_000.0 + FREEZE_STALL_MS + FREEZE_EVERY_MS);
        assert_eq!(room.last_sent(), Some(R1080));

        room.subscribed = false;
        room.track = None;
        room.advance(FREEZE_HOLD_MS + 5_000.0);

        room.subscribed = true;
        room.advance(gap);

        room.track = Some(room.now);
        room.shape = Shape::Fixed(Box::new(moving));
        room.track_subscribed();
        room.measure(2504, 1408);
        room.advance(FREEZE_EVERY_MS);

        assert_eq!(room.last_sent(), Some(MEASURED), "track back {gap} ms after it was wanted");
    }
}

#[test]
fn lifts_it_as_soon_as_the_track_is_here_before_the_room_says_so() {
    let mut room = Room::fixed(frozen_from(2_000.0));

    room.advance(2_000.0 + FREEZE_STALL_MS + FREEZE_EVERY_MS);
    room.subscribed = false;
    room.track = None;
    room.advance(FREEZE_HOLD_MS + 5_000.0);

    room.subscribed = true;
    room.advance(2.0 * FREEZE_EVERY_MS);
    room.track = Some(room.now);
    room.shape = Shape::Fixed(Box::new(moving));
    room.measure(2504, 1408);
    room.advance(FREEZE_EVERY_MS);

    assert_eq!(room.last_sent(), Some(MEASURED));
}

fn cannot_carry_the_original() -> Room {
    Room::watching(Shape::CannotCarryTheOriginal { frames: 0.0, bytes: 0.0, original: 0.0, was_original: true })
}

#[test]
fn asks_for_the_original_again_later_each_time_it_freezes_again() {
    let mut room = cannot_carry_the_original();
    room.advance(200_000.0);

    assert_eq!(room.waits()[..3], [FREEZE_HOLD_MS, 2.0 * FREEZE_HOLD_MS, 4.0 * FREEZE_HOLD_MS]);
}

#[test]
fn remembers_how_the_original_fared_through_a_share_taken_to_its_card_and_back() {
    // Taken to its card during the second cap and put back: a new subscription,
    // a new track, the readings thrown away. How often the original failed is
    // about the line, not the subscription.
    let mut room = cannot_carry_the_original();

    while room.caps() < 2 {
        room.advance(FREEZE_EVERY_MS);
    }
    room.advance(5_000.0);

    room.subscribed = false;
    room.track = None;
    room.advance(3_000.0);

    room.subscribed = true;
    room.track = Some(room.now);
    room.track_subscribed();
    room.measure(2504, 1408);

    room.advance(200_000.0);
    assert_eq!(room.waits()[..3], [FREEZE_HOLD_MS, 2.0 * FREEZE_HOLD_MS, 4.0 * FREEZE_HOLD_MS]);
}
