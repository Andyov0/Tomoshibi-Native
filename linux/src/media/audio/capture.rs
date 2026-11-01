//! The microphone's thread: ten milliseconds at a time from PulseAudio (or
//! a test's sound), through the echo canceller and the rest of WebRTC's
//! processing, into the meter, and into the voice's `AudioInput` while it
//! is published and not muted.
//!
//! The thread is the microphone's for as long as it is open, before a call
//! (the pre-join's meter) and during it; publishing only hands it an input
//! to push into. Nothing is pushed while muted: the track is muted at the
//! server too (`AudioInput::set_muted`), and a microphone that is off sends
//! nothing at all, not silence that a bug could turn back into sound.

use std::{
    sync::{
        atomic::{AtomicBool, AtomicU32, Ordering},
        Arc, Mutex,
    },
    thread,
    time::{Duration, Instant},
};

use tokio::sync::oneshot;
use tomoshibi_core::call::AudioInput;

use super::{
    echo::{Echo, Processing, Processor},
    mixer::FRAME,
    pulse,
    source::Sound,
};
use crate::{app::log, media::MediaError};

/// What the microphone's thread and its handle share.
pub struct Shared {
    /// The level, an RMS over the last tenth of a second from nought to full
    /// scale, as an f32's bits.
    level: AtomicU32,
    /// Where the voice goes while it is published.
    pub input: Mutex<Option<Arc<AudioInput>>>,
    /// What the voice is processed with: the web's constraints for what it
    /// is published as (the default voice before it is).
    pub processing: Mutex<Processing>,
    pub muted: AtomicBool,
    pub stop: AtomicBool,
}

impl Shared {
    pub fn new(processing: Processing, muted: bool) -> Arc<Shared> {
        Arc::new(Shared {
            level: AtomicU32::new(0),
            input: Mutex::new(None),
            processing: Mutex::new(processing),
            muted: AtomicBool::new(muted),
            stop: AtomicBool::new(false),
        })
    }

    pub fn level(&self) -> f32 {
        f32::from_bits(self.level.load(Ordering::Relaxed))
    }
}

/// Where the sound comes from.
pub enum From {
    /// A PulseAudio source by name, or the default one.
    Device(Option<String>),
    /// A test's sound, paced by the clock rather than a device.
    Test(Sound),
}

/// How loud the last tenth of a second was (Windows' SoundMeter).
struct Meter {
    squares: [f64; 10],
    counts: [usize; 10],
    next: usize,
}

impl Meter {
    fn new() -> Meter {
        Meter {
            squares: [0.0; 10],
            counts: [0; 10],
            next: 0,
        }
    }

    fn add(&mut self, frame: &[i16]) -> f32 {
        self.squares[self.next] = frame.iter().map(|&s| f64::from(s) * f64::from(s)).sum();
        self.counts[self.next] = frame.len();
        self.next = (self.next + 1) % self.squares.len();
        let count: usize = self.counts.iter().sum();
        if count == 0 {
            return 0.0;
        }
        ((self.squares.iter().sum::<f64>() / count as f64).sqrt() / 32768.0) as f32
    }
}

enum Stream {
    Pulse(libpulse_simple_binding::Simple),
    Test { sound: Sound, next: Instant },
}

/// Starts the microphone's thread; `opened` is told whether the first open
/// worked.
pub fn start(
    shared: Arc<Shared>,
    echo: Arc<Echo>,
    from: From,
    opened: oneshot::Sender<Result<(), MediaError>>,
) {
    let spawned = thread::Builder::new()
        .name("tomoshibi microphone".into())
        .spawn(move || run(shared, echo, from, opened));
    if let Err(fault) = spawned {
        log(&format!(
            "sound: the microphone's thread did not start: {fault}"
        ));
    }
}

fn run(
    shared: Arc<Shared>,
    echo: Arc<Echo>,
    mut from: From,
    opened: oneshot::Sender<Result<(), MediaError>>,
) {
    let mut opened = Some(opened);
    // The processor, and what it was asked for (which the echo hub may have
    // made without the canceller, for a test).
    let mut processor: Option<(Processing, Arc<Mutex<Processor>>)> = None;
    let mut meter = Meter::new();
    let mut frame = vec![0i16; FRAME];
    let mut bytes = vec![0u8; FRAME * 2];
    let mut capture_delay_ms = 0;
    let mut frames: u64 = 0;
    let mut failures = 0u32;
    while !shared.stop.load(Ordering::Relaxed) {
        let mut stream = match &mut from {
            From::Device(name) => {
                match pulse::record(name.as_deref()) {
                    Ok(stream) => Stream::Pulse(stream),
                    Err(fault) if name.is_some() => {
                        // The one picked is gone or will not open: the system's
                        // own, as the speakers do (Windows Playback.cs).
                        log(&format!("sound: the microphone picked would not open ({fault}); using the system's"));
                        *name = None;
                        continue;
                    }
                    Err(fault) => {
                        if let Some(opened) = opened.take() {
                            let _ = opened.send(Err(MediaError::Failed(fault)));
                            return;
                        }
                        failures += 1;
                        if failures == 1 {
                            log(&format!(
                                "sound: the microphone stopped and would not open again ({fault})"
                            ));
                        }
                        thread::sleep(Duration::from_secs(1));
                        continue;
                    }
                }
            }
            From::Test(sound) => Stream::Test {
                sound: std::mem::replace(sound, Sound::tone(0.0)),
                next: Instant::now(),
            },
        };
        failures = 0;
        if let Some(opened) = opened.take() {
            let _ = opened.send(Ok(()));
        }
        loop {
            if shared.stop.load(Ordering::Relaxed) {
                break;
            }
            match &mut stream {
                Stream::Pulse(simple) => {
                    if let Err(fault) = simple.read(&mut bytes) {
                        log(&format!("sound: the microphone stopped: {fault}"));
                        thread::sleep(Duration::from_millis(200));
                        break;
                    }
                    for (sample, pair) in frame.iter_mut().zip(bytes.as_chunks::<2>().0) {
                        *sample = i16::from_le_bytes([pair[0], pair[1]]);
                    }
                    if frames.is_multiple_of(50) {
                        capture_delay_ms = simple
                            .get_latency()
                            .map(|l| (l.0 / 1000) as i32)
                            .unwrap_or(0);
                    }
                }
                Stream::Test { sound, next } => {
                    *next += Duration::from_millis(10);
                    let now = Instant::now();
                    if *next > now {
                        thread::sleep(*next - now);
                    } else if now - *next > Duration::from_millis(200) {
                        // Far behind (the machine slept): start the clock again
                        // rather than sending a burst.
                        *next = now;
                    }
                    sound.next(&mut frame);
                }
            }
            frames += 1;
            let wanted = *shared.processing.lock().unwrap_or_else(|p| p.into_inner());
            if processor.as_ref().map(|(asked, _)| *asked) != Some(wanted) {
                processor = Some((wanted, echo.processor(wanted)));
            }
            if let Some((_, processor)) = processor.as_ref() {
                if let Ok(mut processor) = processor.lock() {
                    processor.capture(&mut frame, echo.render_delay_ms() + capture_delay_ms);
                }
            }
            let level = meter.add(&frame);
            shared.level.store(level.to_bits(), Ordering::Relaxed);
            if !shared.muted.load(Ordering::Relaxed) {
                let input = shared
                    .input
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .clone();
                if let Some(input) = input {
                    input.push(frame.clone());
                }
            }
        }
        if let Stream::Test { sound, .. } = stream {
            from = From::Test(sound);
        }
    }
    shared.level.store(0f32.to_bits(), Ordering::Relaxed);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_meter_is_an_rms_over_a_tenth_of_a_second() {
        let mut meter = Meter::new();
        let loud = vec![16384i16; FRAME];
        let quiet = vec![0i16; FRAME];
        let level = meter.add(&loud);
        assert!((level - 0.5).abs() < 1e-3, "{level}");
        for _ in 0..9 {
            meter.add(&quiet);
        }
        let level = meter.add(&quiet);
        assert_eq!(level, 0.0, "a tenth of a second later it is forgotten");
    }
}
