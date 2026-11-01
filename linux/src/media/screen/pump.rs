//! What sends a share's picture into the call: the newest frame captured,
//! no faster than the share's rate, and the last one again twice a second
//! while nothing changes.
//!
//! The resend is Windows's (ScreenCapture.cs `Still`, after Android's
//! ScreenCapture.kt `STILL_NANOS`). A PipeWire screencast, like every
//! system's, sends a frame only when something on the screen changed, and
//! a key frame is only ever made from a frame that comes: somebody who
//! joins while a slide sits still would see a black picture until the
//! next change, and an encoder given nothing new lets a still picture's
//! quality stay where the last change left it.
//!
//! The pump is its own thread, so that the capture's callback (PipeWire's
//! loop) only copies a frame into the slot and returns, and the core's
//! conversion (libyuv's ARGBToI420 and its scaling, in `push_bgra`) runs
//! here rather than holding the compositor's buffer.

use std::{
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};

use tomoshibi_core::call::VideoInput;

use super::pixels::fitted;
use crate::app::log;

/// A still screen is sent again this often.
pub const STILL_MS: f64 = 500.0;

/// One captured picture: rows of blue, green, red and a fourth byte,
/// `width * 4` bytes a row.
pub struct Frame {
    pub bgra: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

/// The newest picture, written by the capture and read by the pump. Only
/// the newest is kept: a frame the pump has not taken before the next
/// arrives is one nobody needed.
#[derive(Default)]
pub struct FrameSlot {
    newest: Mutex<Option<Arc<Frame>>>,
    /// Counts the frames put in; the pump compares it with the last it sent
    /// to tell a new picture from the same one.
    count: AtomicU64,
}

impl FrameSlot {
    pub fn put(&self, frame: Frame) {
        *self.newest.lock().unwrap_or_else(|e| e.into_inner()) = Some(Arc::new(frame));
        self.count.fetch_add(1, Ordering::Release);
    }

    /// The newest picture and its number, nothing before the first.
    pub fn newest(&self) -> Option<(Arc<Frame>, u64)> {
        let newest = self
            .newest
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()?;
        Some((newest, self.count.load(Ordering::Acquire)))
    }

    /// The size of the newest picture.
    pub fn size(&self) -> Option<(u32, u32)> {
        self.newest().map(|(frame, _)| (frame.width, frame.height))
    }
}

/// When to send (ScreenCapture.cs `Pump`): on a deadline that moves on by
/// the interval, a wake that came late starting the next interval from now
/// rather than sending the frames it missed; and at each deadline either
/// the new picture or, once half a second has gone with nothing new, the
/// last one again.
#[derive(Debug, Clone)]
pub struct Pace {
    interval_ms: f64,
    due_ms: f64,
    last_sent_ms: f64,
}

impl Pace {
    pub fn new(fps: u32) -> Pace {
        Pace {
            interval_ms: 1000.0 / f64::from(fps.max(1)),
            due_ms: 0.0,
            last_sent_ms: -STILL_MS,
        }
    }

    pub fn set_fps(&mut self, fps: u32) {
        self.interval_ms = 1000.0 / f64::from(fps.max(1));
    }

    /// At `now`, with a new picture there or not: `Some(again)` when a
    /// picture is due, `again` when it is the last one sent once more.
    pub fn at(&mut self, now_ms: f64, fresh: bool) -> Option<bool> {
        if now_ms < self.due_ms {
            return None;
        }
        self.due_ms = if now_ms - self.due_ms > self.interval_ms {
            now_ms + self.interval_ms
        } else {
            self.due_ms + self.interval_ms
        };
        (fresh || now_ms - self.last_sent_ms >= STILL_MS).then_some(!fresh)
    }

    /// A picture went at `now`.
    pub fn sent(&mut self, now_ms: f64) {
        self.last_sent_ms = now_ms;
    }

    /// How long to sleep from `now`: to the next deadline, at least a
    /// millisecond and at most twenty, so that a change of input or a stop
    /// is seen soon.
    pub fn wait(&self, now_ms: f64) -> Duration {
        Duration::from_millis((self.due_ms - now_ms).clamp(1.0, 20.0) as u64)
    }
}

/// Where the pump sends, at what rate and size; changed when a share is
/// published again at another setting (`ScreenShare::change`).
struct Feed {
    input: Option<Arc<VideoInput>>,
    fps: u32,
    /// The size the share was published at, which each picture is fitted
    /// to in its own shape (`pixels::fitted`).
    size: (u32, u32),
}

struct Shared {
    stop: AtomicBool,
    feed: Mutex<Feed>,
    slot: Arc<FrameSlot>,
    pushed: AtomicU64,
}

/// The sending thread of one share.
pub struct Pump {
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
}

impl Pump {
    pub fn start(slot: Arc<FrameSlot>, input: Arc<VideoInput>, fps: u32, size: (u32, u32)) -> Pump {
        let shared = Arc::new(Shared {
            stop: AtomicBool::new(false),
            feed: Mutex::new(Feed {
                input: Some(input),
                fps,
                size,
            }),
            slot,
            pushed: AtomicU64::new(0),
        });
        let thread = std::thread::Builder::new()
            .name("tomoshibi-screen-pump".into())
            .spawn({
                let shared = shared.clone();
                move || run(&shared)
            })
            .map_err(|fault| log(&format!("the screen's pump did not start: {fault}")))
            .ok();
        Pump { shared, thread }
    }

    /// Sends to `input` from now on, at this rate and size.
    pub fn feed(&self, input: Arc<VideoInput>, fps: u32, size: (u32, u32)) {
        let mut feed = self.shared.feed.lock().unwrap_or_else(|e| e.into_inner());
        *feed = Feed {
            input: Some(input),
            fps,
            size,
        };
    }

    /// How many pictures have gone into the call.
    pub fn pushed(&self) -> u64 {
        self.shared.pushed.load(Ordering::Relaxed)
    }

    /// Stops sending; returns the thread to wait for, which ends within a
    /// sleep (at most 20 ms) and whatever push it is in.
    pub fn stop(mut self) -> Option<JoinHandle<()>> {
        self.shared.stop.store(true, Ordering::Release);
        self.shared
            .feed
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .input = None;
        self.thread.take()
    }
}

impl Drop for Pump {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::Release);
    }
}

fn run(shared: &Shared) {
    let clock = Instant::now();
    let mut pace = Pace::new(30);
    let mut sent: Option<u64> = None;
    while !shared.stop.load(Ordering::Acquire) {
        let (input, fps, size) = {
            let feed = shared.feed.lock().unwrap_or_else(|e| e.into_inner());
            (feed.input.clone(), feed.fps, feed.size)
        };
        pace.set_fps(fps);
        let now = clock.elapsed().as_secs_f64() * 1000.0;
        if let (Some(input), Some((frame, number))) = (input, shared.slot.newest()) {
            let fresh = sent != Some(number);
            if pace.at(now, fresh).is_some() {
                let (out_width, out_height) = fitted(size, (frame.width, frame.height));
                // push_bgra takes the rows by value, and the frame stays in
                // the slot for the next resend: one copy per picture sent.
                input.push_bgra(
                    frame.bgra.clone(),
                    frame.width * 4,
                    frame.width,
                    frame.height,
                    out_width,
                    out_height,
                    (now * 1000.0) as i64,
                );
                sent = Some(number);
                pace.sent(now);
                let pushed = shared.pushed.fetch_add(1, Ordering::Relaxed) + 1;
                if pushed == 1 || pushed.is_multiple_of(300) {
                    log(&format!(
                        "screen sent {pushed} pictures, {}x{} captured as {out_width}x{out_height}",
                        frame.width, frame.height
                    ));
                }
            }
        }
        std::thread::sleep(pace.wait(clock.elapsed().as_secs_f64() * 1000.0));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Runs a pace for `ms` milliseconds a millisecond at a time, a new
    /// picture arriving whenever `fresh_at` says, and returns when it sent
    /// and whether each was a resend.
    fn drive(fps: u32, ms: u32, fresh_at: impl Fn(u32) -> bool) -> Vec<(u32, bool)> {
        let mut pace = Pace::new(fps);
        let mut fresh = false;
        let mut sent = Vec::new();
        for now in 0..ms {
            fresh |= fresh_at(now);
            if let Some(again) = pace.at(f64::from(now), fresh) {
                pace.sent(f64::from(now));
                sent.push((now, again));
                fresh = false;
            }
        }
        sent
    }

    #[test]
    fn a_still_screen_is_sent_again_twice_a_second() {
        // One picture at the start, then nothing new for three seconds.
        let sent = drive(30, 3000, |now| now == 0);
        assert_eq!(sent.first(), Some(&(0, false)));
        let resends: Vec<u32> = sent
            .iter()
            .filter(|(_, again)| *again)
            .map(|(at, _)| *at)
            .collect();
        assert_eq!(resends.len(), 5, "{sent:?}");
        for pair in resends.windows(2) {
            let gap = pair[1] - pair[0];
            assert!(
                (500..=534).contains(&gap),
                "resent {gap} ms apart: {sent:?}"
            );
        }
    }

    #[test]
    fn a_moving_screen_goes_no_faster_than_the_rate() {
        // Something new every millisecond, at 30 fps for two seconds.
        let sent = drive(30, 2000, |_| true);
        assert!(sent.iter().all(|(_, again)| !again));
        assert!((59..=61).contains(&sent.len()), "{} sent", sent.len());
    }

    #[test]
    fn a_late_wake_does_not_send_what_it_missed() {
        let mut pace = Pace::new(60);
        assert_eq!(pace.at(0.0, true), Some(false));
        pace.sent(0.0);
        // Woken 200 ms late: one picture, and the next is an interval on.
        assert_eq!(pace.at(200.0, true), Some(false));
        pace.sent(200.0);
        assert_eq!(pace.at(201.0, true), None);
        assert_eq!(pace.at(217.0, true), Some(false));
    }

    #[test]
    fn the_slot_keeps_the_newest_and_counts() {
        let slot = FrameSlot::default();
        assert!(slot.newest().is_none());
        slot.put(Frame {
            bgra: vec![0; 16],
            width: 2,
            height: 2,
        });
        slot.put(Frame {
            bgra: vec![0; 24],
            width: 3,
            height: 2,
        });
        let (frame, number) = slot.newest().unwrap();
        assert_eq!((frame.width, number), (3, 2));
        assert_eq!(slot.size(), Some((3, 2)));
    }
}
