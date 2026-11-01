//! The speakers' thread: the mixer's next ten milliseconds, handed to the
//! echo canceller as what is about to be heard and written to PulseAudio,
//! whose blocking write is what paces the thread at the sound card's clock.
//!
//! A change of speakers (the devices panel, `Media::set_speaker`) is picked
//! up between two frames: the stream is closed and the new one opened, and
//! what is queued in the mixer plays on there. Speakers picked that will
//! not open, or that went away, give way to the system's own, as on Windows
//! (Playback.cs `Open`). A stream that fails is opened again.
//!
//! In a muted launch (`TOMOSHIBI_MUTE`, a test) there are no speakers at
//! all: the thread keeps the mixer's time with the clock instead, so every
//! sound is still received, queued and measured, and nothing can reach a
//! real device by any path.

use std::{
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
    thread,
    time::{Duration, Instant},
};

use super::{echo::Echo, mixer::Mixer, mixer::FRAME, pulse};
use crate::app::log;

/// Where the mix goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Output {
    /// The speakers.
    Speakers,
    /// The speakers, written silence (`TOMOSHIBI_MUTE=speakers`): the
    /// stream runs as it would, nothing is heard.
    Silent,
    /// Nowhere; the clock keeps time (`TOMOSHIBI_MUTE`).
    Nowhere,
}

/// What the playback thread and its owner share.
pub struct Shared {
    pub stop: AtomicBool,
    /// The speakers wanted, and how many times that has changed.
    pub speaker: Mutex<Option<String>>,
    pub generation: AtomicU64,
    /// Frames written since the start, for the debug log.
    pub frames: AtomicU64,
}

impl Shared {
    pub fn new(speaker: Option<String>) -> Arc<Shared> {
        Arc::new(Shared {
            stop: AtomicBool::new(false),
            speaker: Mutex::new(speaker),
            generation: AtomicU64::new(0),
            frames: AtomicU64::new(0),
        })
    }

    pub fn set_speaker(&self, speaker: Option<String>) {
        *self.speaker.lock().unwrap_or_else(|p| p.into_inner()) = speaker;
        self.generation.fetch_add(1, Ordering::AcqRel);
    }
}

pub fn start(shared: Arc<Shared>, mixer: Arc<Mixer>, echo: Arc<Echo>, output: Output) {
    let spawned = thread::Builder::new()
        .name("tomoshibi speakers".into())
        .spawn(move || run(shared, mixer, echo, output));
    if let Err(fault) = spawned {
        log(&format!(
            "sound: the speakers' thread did not start: {fault}"
        ));
    }
}

fn run(shared: Arc<Shared>, mixer: Arc<Mixer>, echo: Arc<Echo>, output: Output) {
    let mut mix = vec![0i16; FRAME * 2];
    let mut bytes = vec![0u8; FRAME * 4];
    let mut failures = 0u32;
    while !shared.stop.load(Ordering::Relaxed) {
        let generation = shared.generation.load(Ordering::Acquire);
        let wanted = shared
            .speaker
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone();
        let stream = if output == Output::Nowhere {
            None
        } else {
            match pulse::playback(wanted.as_deref()) {
                Ok(stream) => {
                    failures = 0;
                    log(&format!(
                        "sound: playing to {}{}",
                        wanted.as_deref().unwrap_or("the system's speakers"),
                        if output == Output::Silent {
                            ", silently"
                        } else {
                            ""
                        }
                    ));
                    Some(stream)
                }
                Err(fault) if wanted.is_some() => {
                    log(&format!(
                        "sound: the speakers picked would not open ({fault}); using the system's"
                    ));
                    *shared.speaker.lock().unwrap_or_else(|p| p.into_inner()) = None;
                    continue;
                }
                Err(fault) => {
                    failures += 1;
                    if failures == 1 {
                        log(&format!("sound: the speakers would not open ({fault})"));
                    }
                    // Keep the mixer moving meanwhile, so that what arrives
                    // is not all played at once when they come back.
                    for _ in 0..100 {
                        if shared.stop.load(Ordering::Relaxed) {
                            return;
                        }
                        mixer.mix(&mut mix);
                        thread::sleep(Duration::from_millis(10));
                    }
                    continue;
                }
            }
        };
        let mut next = Instant::now();
        let mut written: u64 = 0;
        loop {
            if shared.stop.load(Ordering::Relaxed) {
                return;
            }
            if shared.generation.load(Ordering::Acquire) != generation {
                break;
            }
            mixer.mix(&mut mix);
            if output == Output::Silent {
                mix.iter_mut().for_each(|s| *s = 0);
            }
            match stream.as_ref() {
                Some(simple) => {
                    echo.render(&mix);
                    for (pair, sample) in bytes.as_chunks_mut::<2>().0.iter_mut().zip(mix.iter()) {
                        pair.copy_from_slice(&sample.to_le_bytes());
                    }
                    if let Err(fault) = simple.write(&bytes) {
                        log(&format!("sound: the speakers stopped: {fault}"));
                        thread::sleep(Duration::from_millis(200));
                        break;
                    }
                    if written.is_multiple_of(50) {
                        let latency = simple.get_latency().map(|l| (l.0 / 1000) as i32);
                        echo.set_render_delay_ms(
                            latency.unwrap_or(pulse::PLAYBACK_LATENCY_MS as i32),
                        );
                    }
                }
                None => {
                    next += Duration::from_millis(10);
                    let now = Instant::now();
                    if next > now {
                        thread::sleep(next - now);
                    } else if now - next > Duration::from_millis(200) {
                        next = now;
                    }
                }
            }
            written += 1;
            shared.frames.fetch_add(1, Ordering::Relaxed);
        }
    }
}
