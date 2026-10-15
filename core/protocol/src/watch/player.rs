//! Keeping a player to the show.
//!
//! Follows `correction` and `expected` in web/src/live/watch.ts, and the loop
//! that applies them in web/src/components/room/WatchScreen.tsx (`keep`, run
//! every KEEP_MS). Each viewer plays the video themselves; what is shared is
//! only where the show is, and each player is nudged or moved to it. The
//! numbers are the web's, and each was measured in two browsers side by side;
//! the comments beside them say what went wrong with the number before.
//!
//! The app owns the player (AVPlayer, or YouTube's own in a web view) and
//! reads it four times a second into a [`WatchPlayerReading`]; the
//! [`WatchKeeper`] says what to do to it. One keeper per video, as the web
//! has one player component per video (keyed by the video's key), so the
//! memory of the last jump does not carry from one video to the next.

use std::sync::{Arc, Mutex};

use super::WatchShow;

/// Past this many seconds, a player jumps to where the show is rather than
/// catching up.
pub const SEEK_AT: f64 = 2.0;

/// A playing player starts closing a gap wider than this, and stops once it
/// is inside [`SETTLED`]: two lines rather than one, so a player does not sit
/// just inside a single line for good. Measured with one: after a resume, two
/// browsers stayed 0.22 s apart under a line at 0.25 for as long as they
/// played.
pub const CLOSE_ENOUGH: f64 = 0.12;
pub const SETTLED: f64 = 0.04;

/// A paused player further than this from the show is moved to it. Small,
/// because moving a still picture is not seen: at 0.3 two browsers paused by
/// one press sat a quarter of a second apart on different frames.
pub const PAUSED_CLOSE_ENOUGH: f64 = 0.1;

/// How much faster or slower a player runs to close a small gap.
pub const NUDGE: f64 = 0.05;

/// And a larger one, past [`FAR`]. A seek on a file fetched over the internet
/// lands about a second after it was asked for, by which time the show has
/// moved on a second: measured with a real video, a viewer arriving late
/// landed a second behind, and at five per cent took twenty seconds to close
/// it.
pub const FAR: f64 = 0.5;
pub const FAST_NUDGE: f64 = 0.12;

/// How often the app should read its player and ask the keeper
/// (WatchScreen.tsx, `KEEP_MS`).
pub const KEEP_MS: u64 = 250;

/// At most one jump in this long. A jump over the internet takes a while to
/// land, and YouTube's player goes on reporting where it was until it has:
/// held to the show every quarter second, it was asked to jump again and again
/// (WatchScreen.tsx, `SEEK_GAP_MS`).
pub const SEEK_GAP_MS: u64 = 1500;

/// How long YouTube's player is given to begin after being asked, before it
/// is taken to have been refused: iOS refuses every play not started by a
/// press inside YouTube's own frame (WatchScreen.tsx, `YOUTUBE_REFUSED_MS`).
pub const YOUTUBE_REFUSED_MS: u64 = 1500;

/// A player that cannot run a little fast or slow -- YouTube's, which takes
/// only a few fixed rates -- is jumped once it is this far out instead.
pub const YOUTUBE_JUMP_AT: f64 = 1.0;

/// What a player should do: jump, or run at this rate.
#[derive(Debug, Clone, Copy, PartialEq, uniffi::Record)]
pub struct WatchCorrection {
    pub seek: Option<f64>,
    pub rate: f64,
}

/// What a player should do, given where it is and where the show is
/// (watch.ts, `correction`).
///
/// A small gap is closed by running a little fast or slow, which nobody
/// notices; jumping instead would stutter every viewer every few seconds over
/// the jitter of the network. A large one -- somebody arriving late, a seek, a
/// player that stalled -- is jumped, because catching up five per cent at a
/// time would take minutes. `rate` is the rate the player runs at now: a gap
/// being closed is closed all the way to [`SETTLED`].
#[uniffi::export]
pub fn watch_correction(local: f64, expected: f64, playing: bool, rate: f64) -> WatchCorrection {
    let drift = local - expected;
    if !playing {
        let seek = (drift.abs() > PAUSED_CLOSE_ENOUGH).then_some(expected);
        return WatchCorrection { seek, rate: 1.0 };
    }
    if drift.abs() > SEEK_AT {
        return WatchCorrection { seek: Some(expected), rate: 1.0 };
    }
    let closing = rate != 1.0;
    if drift.abs() <= if closing { SETTLED } else { CLOSE_ENOUGH } {
        return WatchCorrection { seek: None, rate: 1.0 };
    }
    let by = if drift.abs() > FAR { FAST_NUDGE } else { NUDGE };
    WatchCorrection { seek: None, rate: if drift > 0.0 { 1.0 - by } else { 1.0 + by } }
}

/// Where the show is at `now_ms`, by what was last heard of it at
/// `heard_at_ms` (watch.ts, `expected`): it moves on while it plays, and
/// never past the end of a video whose length is known.
#[uniffi::export]
pub fn watch_expected(show: WatchShow, heard_at_ms: u64, now_ms: u64) -> f64 {
    expected(&show, heard_at_ms, now_ms)
}

pub(crate) fn expected(show: &WatchShow, heard_at_ms: u64, now_ms: u64) -> f64 {
    let moved = show.position + if show.playing { (now_ms as f64 - heard_at_ms as f64) / 1000.0 } else { 0.0 };
    match &show.now {
        Some(video) if video.duration > 0.0 => moved.min(video.duration),
        _ => moved,
    }
}

/// The player as the app finds it, a few times a second.
#[derive(Debug, Clone, Copy, PartialEq, uniffi::Record)]
pub struct WatchPlayerReading {
    /// It has the video loaded and can say where it is; false as well where
    /// it has been refused for good (the site will not play it here), so the
    /// keeper does not go on asking it.
    pub ready: bool,
    /// Seconds into the video.
    pub time: f64,
    pub playing: bool,
    /// The rate it runs at now.
    pub rate: f64,
    /// It can run a little fast or slow. AVPlayer can; YouTube's cannot.
    pub nudges: bool,
    /// It is YouTube's own player, whose play is asked by a message and
    /// answered only by its state changing, and whose position reads where it
    /// was until a jump has landed.
    pub youtube: bool,
}

/// What to do to the player now. Everything is to be applied in the order of
/// the fields: play or pause, then jump, then rate.
#[derive(Debug, Clone, Copy, PartialEq, uniffi::Record)]
pub struct WatchKeepStep {
    /// Nothing to hold to yet: the player is not ready. The app shows a
    /// spinner.
    pub waiting: bool,
    /// Where the clock under the video should read, or `None` to leave it:
    /// something live has no position to show while it waits.
    pub position: Option<f64>,
    pub play: bool,
    pub pause: bool,
    pub seek: Option<f64>,
    /// The rate to run at, for a player that nudges; `None` for one that
    /// cannot, which is never asked to.
    pub rate: Option<f64>,
    /// YouTube's player was asked to play and is still not playing after
    /// [`YOUTUBE_REFUSED_MS`]: the platform wants a press inside the player
    /// itself, and the app says so.
    pub refused: bool,
}

#[derive(Debug, Default)]
struct Memory {
    /// When the player was last made to jump, by the keeper or by the viewer.
    jumped: Option<u64>,
    /// When it was asked to play and has not yet been seen playing.
    asked: Option<u64>,
}

/// Holds one player to the show (WatchScreen.tsx, the `keep` loop).
#[derive(Debug, uniffi::Object)]
pub struct WatchKeeper {
    memory: Mutex<Memory>,
}

#[uniffi::export]
impl WatchKeeper {
    #[uniffi::constructor]
    pub fn new() -> Arc<Self> {
        Arc::new(Self { memory: Mutex::new(Memory::default()) })
    }

    /// The viewer moved the player themselves (the position slider): counted
    /// as a jump, so the keeper does not jump it back before the show has
    /// heard the seek and moved there too.
    pub fn seeked(&self, now_ms: u64) {
        self.memory.lock().unwrap_or_else(|e| e.into_inner()).jumped = Some(now_ms);
    }

    /// What to do to the player, given what it reads, whether the show is
    /// playing and where it is ([`watch_expected`]), and whether the video is
    /// live.
    pub fn keep(&self, player: WatchPlayerReading, show_playing: bool, expected: f64, live: bool, now_ms: u64) -> WatchKeepStep {
        let mut memory = self.memory.lock().unwrap_or_else(|e| e.into_inner());
        keep(&mut memory, player, show_playing, expected, live, now_ms)
    }
}

fn keep(memory: &mut Memory, player: WatchPlayerReading, playing: bool, want: f64, live: bool, now: u64) -> WatchKeepStep {
    let mut step = WatchKeepStep {
        waiting: false,
        position: None,
        play: false,
        pause: false,
        seek: None,
        rate: None,
        refused: false,
    };
    if !player.ready {
        step.waiting = true;
        // Where everybody is, for a viewer with nothing playing here.
        if !live {
            step.position = Some(want);
        }
        return step;
    }

    // The player as it will read once told: a video element, and AVPlayer
    // with it, reports a jump's target and a play at once, so what follows in
    // the same step sees them; YouTube's answers neither until it has acted.
    let mut time = player.time;
    let mut running = player.playing;
    let jump = |memory: &mut Memory, step: &mut WatchKeepStep, time: &mut f64, to: f64| {
        if memory.jumped.is_some_and(|at| now.saturating_sub(at) < SEEK_GAP_MS) {
            return;
        }
        memory.jumped = Some(now);
        step.seek = Some(to);
        if !player.youtube {
            *time = to;
        }
    };

    if playing && !running {
        // Started from where the show is, rather than from where this player
        // stood and then chased: measured, starting on the next tick put every
        // player about 0.4 s behind and five per cent fast for the next eight
        // seconds.
        if !live && (time - want).abs() > CLOSE_ENOUGH {
            jump(memory, &mut step, &mut time, want);
        }
        if memory.asked.is_none() {
            memory.asked = Some(now);
        }
        step.play = true;
        if !player.youtube {
            running = true;
        }
        // YouTube's answer is its state, not the request: asked and still not
        // playing a moment later is refused.
        if player.youtube && memory.asked.is_some_and(|at| now.saturating_sub(at) > YOUTUBE_REFUSED_MS) {
            step.refused = true;
        }
    } else if !playing && running {
        step.pause = true;
        running = false;
    }
    if running || !playing {
        memory.asked = None;
    }

    // Something live has no moment to agree on; only playing or not.
    if !live {
        let fix = watch_correction(time, want, playing, player.rate);
        if let Some(to) = fix.seek {
            jump(memory, &mut step, &mut time, to);
        } else if !player.nudges && playing && (time - want).abs() > YOUTUBE_JUMP_AT {
            jump(memory, &mut step, &mut time, want);
        }
        if player.nudges {
            step.rate = Some(fix.rate);
        }
    }

    step.position = Some(time);
    step
}
