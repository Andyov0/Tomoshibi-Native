//! A screen and a sound that are not there, for tests: what a debug run
//! shares when told to (`Source::Synthetic`), so that no test ever sends a
//! picture of a desktop or the sound of a room, and so that the share's
//! publishing, pacing and conversion can be checked where there is no
//! portal at all (the plain weston of run-headless.sh has none).
//!
//! The same as Windows's (windows/Tomoshibi/Media/ScreenCapture.cs
//! `SyntheticScreen`, SoundCapture.cs `SyntheticSound`), but drawn as
//! BGRA rows into the same slot the portal's capture fills, so that it goes
//! through `push_bgra` exactly as a real screen does: a light page with
//! eight colour bars along its top (white, yellow, cyan, green, magenta,
//! red, blue, black, the order a test reads them back in), lines of
//! "text" written a word at a time, and a pointer that wanders. Mostly
//! still, as a shared document is, with a change eight times a second. The
//! sound is a chord whose two sides differ, 523.25 Hz on the left and
//! 659.25 Hz on the right at -20 dBFS, so that a browser can tell it
//! arrived in stereo.

use std::{
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc,
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};

use super::{
    pump::{Frame, FrameSlot},
    sound::SoundSink,
};

pub const WIDTH: u32 = 1920;
pub const HEIGHT: u32 = 1080;
pub const LEFT_HZ: f64 = 523.25;
pub const RIGHT_HZ: f64 = 659.25;

/// The bars' colours as blue, green, red, left to right.
pub const BARS: [[u8; 3]; 8] = [
    [235, 235, 235],
    [16, 235, 235],
    [235, 235, 16],
    [16, 235, 16],
    [235, 16, 235],
    [16, 16, 235],
    [235, 16, 16],
    [16, 16, 16],
];

/// A thread drawing into a slot, or making sound into a sink.
pub struct Generator {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    pub made: Arc<AtomicU64>,
}

impl Generator {
    pub fn stop(mut self) -> Option<JoinHandle<()>> {
        self.stop.store(true, Ordering::Release);
        self.thread.take()
    }
}

impl Drop for Generator {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
    }
}

fn spawn(
    name: &str,
    body: impl FnOnce(Arc<AtomicBool>, Arc<AtomicU64>) + Send + 'static,
) -> Generator {
    let stop = Arc::new(AtomicBool::new(false));
    let made = Arc::new(AtomicU64::new(0));
    let thread = std::thread::Builder::new()
        .name(name.into())
        .spawn({
            let (stop, made) = (stop.clone(), made.clone());
            move || body(stop, made)
        })
        .ok();
    Generator { stop, thread, made }
}

/// Draws the generated screen into `slot`, a new picture eight times a
/// second.
pub fn screen(slot: Arc<FrameSlot>) -> Generator {
    spawn("tomoshibi-synthetic-screen", move |stop, made| {
        let clock = Instant::now();
        let mut drawn = u64::MAX;
        while !stop.load(Ordering::Acquire) {
            let step = clock.elapsed().as_millis() as u64 / 125;
            if step != drawn {
                slot.put(Frame {
                    bgra: paint(WIDTH, HEIGHT, step),
                    width: WIDTH,
                    height: HEIGHT,
                });
                drawn = step;
                made.fetch_add(1, Ordering::Relaxed);
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    })
}

/// One picture of the generated screen at `step`.
pub fn paint(width: u32, height: u32, step: u64) -> Vec<u8> {
    let (w, h) = (width as usize, height as usize);
    let mut bgra = vec![0u8; w * h * 4];
    let mut fill = |x0: usize, y0: usize, x1: usize, y1: usize, colour: [u8; 3]| {
        for y in y0.min(h)..y1.min(h) {
            for x in x0.min(w)..x1.min(w) {
                let at = (y * w + x) * 4;
                bgra[at..at + 4].copy_from_slice(&[colour[0], colour[1], colour[2], 255]);
            }
        }
    };
    fill(0, 0, w, h, [225, 225, 225]);
    // The bars, along the top fifth.
    for (index, colour) in BARS.iter().enumerate() {
        fill(index * w / 8, 0, (index + 1) * w / 8, h / 5, *colour);
    }
    // Lines of "text", one more word every step.
    let line = (h / 40).max(4);
    let written = (step % 400) as usize;
    for row in 0..24 {
        let top = h / 4 + row * line * 3 / 2;
        if top + line >= h {
            break;
        }
        let words = written.saturating_sub(row * 12).min(12);
        for word in 0..words {
            let left = w / 12 + word * w / 15;
            let length = w / 22 + (row * 7 + word * 13) % (w / 40 + 1);
            fill(left, top, left + length, top + line / 2, [40, 40, 40]);
        }
    }
    // The pointer, wandering.
    let x = (w as f64 / 2.0 + w as f64 / 3.0 * (step as f64 / 9.0).sin()) as usize;
    let y = (h as f64 / 2.0 + h as f64 / 3.0 * (step as f64 / 13.0).cos()) as usize;
    for row in 0..h / 30 {
        fill(x, y + row, x + row / 2 + 2, y + row + 1, [16, 16, 16]);
    }
    bgra
}

/// Makes the chord into `sink`, ten milliseconds at a time.
pub fn sound(sink: SoundSink) -> Generator {
    spawn("tomoshibi-synthetic-sound", move |stop, made| {
        const RATE: f64 = 48_000.0;
        const CHUNK: usize = 480;
        let clock = Instant::now();
        let (mut left, mut right) = (0.0f64, 0.0f64);
        let mut chunks: u64 = 0;
        while !stop.load(Ordering::Acquire) {
            while chunks * 10 <= clock.elapsed().as_millis() as u64 {
                let mut samples = Vec::with_capacity(CHUNK * 2);
                for _ in 0..CHUNK {
                    samples.push((left.sin() * 0.1 * 32767.0) as i16);
                    samples.push((right.sin() * 0.1 * 32767.0) as i16);
                    left = (left + std::f64::consts::TAU * LEFT_HZ / RATE) % std::f64::consts::TAU;
                    right =
                        (right + std::f64::consts::TAU * RIGHT_HZ / RATE) % std::f64::consts::TAU;
                }
                if let Some(input) = sink.lock().unwrap_or_else(|e| e.into_inner()).clone() {
                    input.push(samples);
                    made.fetch_add(1, Ordering::Relaxed);
                }
                chunks += 1;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bars_are_where_a_test_reads_them() {
        let (width, height) = (320, 180);
        let picture = paint(width, height, 0);
        for (index, colour) in BARS.iter().enumerate() {
            let x = (index as u32 * width / 8 + width / 16) as usize;
            let y = (height / 10) as usize;
            let at = (y * width as usize + x) * 4;
            assert_eq!(&picture[at..at + 3], colour, "bar {index}");
        }
    }

    #[test]
    fn the_page_changes_between_steps() {
        assert_ne!(paint(320, 180, 3), paint(320, 180, 4));
    }
}
