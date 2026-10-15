//! The playout buffer: web/src/live/playout.test.ts, and the worklet's report.
//!
//! What it must never do is put out a frame that did not arrive, put one out
//! twice, or out of order: any of those is the loss this path exists to avoid,
//! done locally. So the frames are numbered and the output is checked to be the
//! numbers in order -- across block boundaries the requests do not line up
//! with, across a stall, and across a skip. What it may do, and must say it
//! did, is wait and drop.

use tomoshibi_protocol::lossless::{
    ceiling_frames, target_frames, Playout, PlayoutCounts, Worklet,
};

/// A block whose samples are their own frame numbers, scaled into range.
fn numbered(from: usize, frames: usize, channels: usize) -> Vec<Vec<f32>> {
    (0..channels)
        .map(|c| {
            (0..frames)
                .map(|i| ((from + i) as f64 / 1e6 + if c > 0 { 0.5 } else { 0.0 }) as f32)
                .collect()
        })
        .collect()
}

fn pull(playout: &mut Playout, frames: usize) -> Vec<Vec<f32>> {
    let mut out = vec![vec![9.0; frames], vec![9.0; frames]];
    playout.pull(&mut out);
    out
}

fn frame_of(sample: f32) -> i64 {
    (f64::from(sample) * 1e6).round() as i64
}

#[test]
fn waits_for_its_target_then_plays_every_frame_in_order() {
    let mut playout = Playout::new(2, 1920, 48_000);

    playout.push(numbered(0, 960, 2));
    assert!(pull(&mut playout, 128)[0].iter().all(|&s| s == 0.0));

    playout.push(numbered(960, 960, 2));
    playout.push(numbered(1920, 960, 2));

    let mut heard = Vec::new();
    for _ in 0..20 {
        let out = pull(&mut playout, 128);
        for (&left, &right) in out[0].iter().zip(&out[1]) {
            heard.push(frame_of(left));
            // The channels stay together, and each is its own.
            assert!((right - (left + 0.5)).abs() < 1e-6);
        }
    }

    assert_eq!(heard, (0..2560).collect::<Vec<_>>());
    let counts = playout.counts();
    assert_eq!(
        (counts.underruns, counts.skipped, counts.played),
        (0, 0, 2560)
    );
}

#[test]
fn goes_quiet_when_it_runs_dry_says_so_and_waits_for_its_target_again() {
    let mut playout = Playout::new(2, 960, 48_000);
    playout.push(numbered(0, 960, 2));

    for _ in 0..7 {
        pull(&mut playout, 128);
    }
    let last = pull(&mut playout, 128);
    // 7 * 128 = 896 played, 64 left: the rest of this request is silence.
    assert_eq!(frame_of(last[0][63]), 959);
    assert_eq!(last[0][64], 0.0);
    assert!(last
        .iter()
        .all(|channel| channel[64..].iter().all(|&s| s == 0.0)));
    let counts = playout.counts();
    assert_eq!((counts.underruns, counts.playing), (1, false));

    playout.push(numbered(960, 480, 2));
    assert!(pull(&mut playout, 128)[0].iter().all(|&s| s == 0.0));

    playout.push(numbered(1440, 480, 2));
    assert_eq!(frame_of(pull(&mut playout, 128)[0][0]), 960);
}

#[test]
fn drops_whole_blocks_from_the_front_past_its_ceiling_back_to_its_target_and_counts_them() {
    let mut playout = Playout::new(2, 1920, 4800);
    for block in 0..6 {
        playout.push(numbered(block * 960, 960, 2));
    }

    // 5760 held is past 4800: blocks go from the front until 1920 remain.
    let counts = playout.counts();
    assert_eq!((counts.held, counts.skipped), (1920, 3840));
    assert_eq!(frame_of(pull(&mut playout, 128)[0][0]), 3840);
}

#[test]
fn drops_the_rest_of_a_block_part_played_and_counts_only_what_was_not_heard() {
    let mut playout = Playout::new(2, 960, 2000);
    playout.push(numbered(0, 960, 2));
    pull(&mut playout, 100);
    playout.push(numbered(960, 960, 2));
    // 860 + 960 + 960 = 2780 is past 2000: blocks go from the front back to
    // 960, the part-played one counting the 860 frames nobody heard.
    playout.push(numbered(1920, 960, 2));

    let counts = playout.counts();
    assert_eq!((counts.held, counts.skipped), (960, 860 + 960));
    assert_eq!(frame_of(pull(&mut playout, 1)[0][0]), 1920);
}

#[test]
fn plays_a_mono_stream_out_of_both_speakers() {
    let mut playout = Playout::new(1, 128, 48_000);
    playout.push(numbered(0, 128, 1));

    let out = pull(&mut playout, 128);
    assert_eq!(out[0], out[1]);
    assert_eq!(frame_of(out[0][127]), 127);
}

#[test]
fn pulls_any_length_including_ten_milliseconds_across_blocks() {
    let mut playout = Playout::new(2, 960, 48_000);
    for block in 0..3 {
        playout.push(numbered(block * 960, 960, 2));
    }

    let mut heard = Vec::new();
    for size in [480, 480, 441, 1, 1000, 478] {
        let out = pull(&mut playout, size);
        heard.extend(out[0].iter().map(|&s| frame_of(s)));
    }
    assert_eq!(heard, (0..2880).collect::<Vec<_>>());
    assert_eq!(playout.counts().underruns, 0);
}

#[test]
fn ignores_an_empty_block() {
    let mut playout = Playout::new(2, 1, 10);
    playout.push(vec![vec![], vec![]]);
    playout.push(vec![]);
    assert_eq!(playout.counts(), PlayoutCounts::default());
}

#[test]
fn has_the_web_clients_target_and_ceiling() {
    assert_eq!(
        (target_frames(48_000), ceiling_frames(48_000)),
        (28_800, 144_000)
    );
    assert_eq!(
        (target_frames(44_100), ceiling_frames(44_100)),
        (26_460, 132_300)
    );
    let worklet = Worklet::new(48_000, 2);
    assert_eq!(
        (worklet.playout().target(), worklet.playout().ceiling()),
        (28_800, 144_000)
    );
}

#[test]
fn the_worklet_reports_its_counts_once_a_second_and_not_before() {
    let mut worklet = Worklet::new(48_000, 2);
    worklet.push(numbered(0, 28_800, 2));
    let mut out = vec![vec![0.0f32; 128], vec![0.0f32; 128]];

    // 374 quanta of 128 is 47872 frames: not yet a second.
    for _ in 0..374 {
        worklet.pull(&mut out);
    }
    assert_eq!(worklet.counts(), None);
    assert_eq!(worklet.playout().counts().underruns, 1);

    // The 375th makes 48000, and the report carries the underrun, a second late.
    worklet.pull(&mut out);
    let report = worklet.counts().expect("a report");
    assert_eq!((report.underruns, report.played), (1, 28_800));

    // Between reports, what the page knows stands still.
    worklet.push(numbered(28_800, 28_800, 2));
    worklet.pull(&mut out);
    assert_eq!(worklet.counts(), Some(report));
}
