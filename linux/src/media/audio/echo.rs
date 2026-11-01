//! WebRTC's audio processing on the microphone, fed what the speakers play.
//!
//! A browser's getUserMedia runs the microphone through this very module
//! (echo cancellation, noise suppression, gain control, as the web asks for
//! them: share.rs `voice_capture`). The core sends what it is pushed as it
//! is, since its audio device is synthetic and nothing of WebRTC's own
//! capture path runs, and Linux has no system canceller to lean on (a
//! PipeWire echo-cancel module is something a person may have loaded, not
//! something the app can count on). Without one, anybody on speakers sends
//! the room back to itself a moment later. So the app runs WebRTC's own
//! `AudioProcessingModule` from the vendored libwebrtc over every 10 ms the
//! microphone gives, and feeds it every 10 ms the mixer plays as its reverse
//! stream, the echo it is to take out.
//!
//! Each open microphone has its own processor (its settings follow what it
//! is published as), and the playback feeds all of them; the processors are
//! held weakly here, so a microphone closed is a processor gone.
//!
//! The canceller is told how far apart the two are: the playback stream's
//! latency, written here by the playback thread, plus the capture stream's,
//! which the capture thread adds. AEC3 finds the delay itself as well; the
//! figure is where it starts looking.

use std::sync::{
    atomic::{AtomicBool, AtomicI32, Ordering},
    Arc, Mutex, Weak,
};

use libwebrtc::native::apm::AudioProcessingModule;
use tomoshibi_protocol::share::SoundCapture;

use super::mixer::{FRAME, RATE};

/// What a processor does, from the capture the web asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Processing {
    pub echo_cancellation: bool,
    pub noise_suppression: bool,
    pub gain_control: bool,
}

impl Processing {
    /// The web's constraints for a kind of sound (`sound_profile(..).capture`).
    pub fn of(capture: &SoundCapture) -> Processing {
        Processing {
            echo_cancellation: capture.echo_cancellation,
            noise_suppression: capture.noise_suppression,
            gain_control: capture.auto_gain_control,
        }
    }

    /// Nothing at all: a test source that is to arrive exactly as it was
    /// generated.
    pub const NONE: Processing = Processing {
        echo_cancellation: false,
        noise_suppression: false,
        gain_control: false,
    };

    fn any(self) -> bool {
        self.echo_cancellation || self.noise_suppression || self.gain_control
    }
}

/// One microphone's processing.
pub struct Processor {
    apm: AudioProcessingModule,
    processing: Processing,
    scratch: Vec<i16>,
}

impl Processor {
    pub fn new(processing: Processing) -> Processor {
        // The high-pass filter goes with any processing, as in a browser,
        // where it follows echo cancellation (Chrome's goog_highpass_filter).
        let apm = AudioProcessingModule::new(
            processing.echo_cancellation,
            processing.gain_control,
            processing.any(),
            processing.noise_suppression,
        );
        Processor {
            apm,
            processing,
            scratch: vec![0; FRAME],
        }
    }

    /// Ten milliseconds of the microphone, mono, processed in place;
    /// `delay_ms` is how long ago what the speakers played reached them.
    pub fn capture(&mut self, frame: &mut [i16], delay_ms: i32) {
        if !self.processing.any() || frame.len() != FRAME {
            return;
        }
        if self.processing.echo_cancellation {
            let _ = self.apm.set_stream_delay_ms(delay_ms.clamp(0, 500));
        }
        let _ = self.apm.process_stream(frame, RATE as i32, 1);
    }

    fn render(&mut self, mono: &[i16]) {
        if !self.processing.echo_cancellation || mono.len() != FRAME {
            return;
        }
        // The reverse stream is processed in place too; what comes back is
        // not played, so it is processed in a copy.
        self.scratch.copy_from_slice(mono);
        let _ = self
            .apm
            .process_reverse_stream(&mut self.scratch, RATE as i32, 1);
    }
}

/// What the playback and every microphone share.
#[derive(Default)]
pub struct Echo {
    processors: Mutex<Vec<Weak<Mutex<Processor>>>>,
    /// The playback stream's latency, in milliseconds.
    render_delay_ms: AtomicI32,
    /// `TOMOSHIBI_DEBUG_NO_ECHO_CANCELLER`: processors are made without the
    /// canceller, for a test that shows what it takes out.
    disabled: AtomicBool,
}

impl Echo {
    pub fn new() -> Arc<Echo> {
        Arc::new(Echo::default())
    }

    /// A processor for a microphone, fed the playback from now on.
    pub fn processor(&self, mut processing: Processing) -> Arc<Mutex<Processor>> {
        if self.disabled.load(Ordering::Relaxed) {
            processing.echo_cancellation = false;
        }
        let made = Arc::new(Mutex::new(Processor::new(processing)));
        let mut processors = self.processors.lock().unwrap_or_else(|p| p.into_inner());
        processors.retain(|p| p.strong_count() > 0);
        processors.push(Arc::downgrade(&made));
        made
    }

    pub fn disable_canceller(&self) {
        self.disabled.store(true, Ordering::Relaxed);
    }

    /// Ten milliseconds of what the speakers are about to play, stereo
    /// interleaved, as the playback writes it.
    pub fn render(&self, stereo: &[i16]) {
        let live: Vec<Arc<Mutex<Processor>>> = {
            let processors = self.processors.lock().unwrap_or_else(|p| p.into_inner());
            processors.iter().filter_map(Weak::upgrade).collect()
        };
        if live.is_empty() {
            return;
        }
        let mono: Vec<i16> = stereo
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| ((i32::from(pair[0]) + i32::from(pair[1])) / 2) as i16)
            .collect();
        for processor in live {
            if let Ok(mut processor) = processor.lock() {
                processor.render(&mono);
            }
        }
    }

    pub fn set_render_delay_ms(&self, delay: i32) {
        self.render_delay_ms.store(delay, Ordering::Relaxed);
    }

    pub fn render_delay_ms(&self) -> i32 {
        self.render_delay_ms.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(hz: f32, frames: usize, offset: usize, amplitude: f32) -> Vec<i16> {
        (0..frames * FRAME)
            .map(|i| {
                let t = (i + offset) as f32 / RATE as f32;
                (amplitude * (2.0 * std::f32::consts::PI * hz * t).sin()) as i16
            })
            .collect()
    }

    fn energy(samples: &[i16]) -> f64 {
        samples
            .iter()
            .map(|&s| f64::from(s) * f64::from(s))
            .sum::<f64>()
            / samples.len() as f64
    }

    /// The canceller takes out what the speakers played: the microphone
    /// hears the playback (an echo of it, ten milliseconds late) and its
    /// processed output is far quieter than what it heard. Broken (the
    /// reverse stream not fed), the echo comes through: the second half of
    /// this test.
    #[test]
    fn the_canceller_takes_out_what_was_played() {
        let echo = Echo::new();
        let processor = echo.processor(Processing {
            echo_cancellation: true,
            noise_suppression: false,
            gain_control: false,
        });
        let mut unfed = Processor::new(Processing {
            echo_cancellation: true,
            noise_suppression: false,
            gain_control: false,
        });
        // Speech-like: two tones beating, so the canceller has something
        // that is not a steady line to lock on to.
        let seconds = 6;
        let played: Vec<i16> = sine(330.0, seconds * 100, 0, 6000.0)
            .iter()
            .zip(sine(523.0, seconds * 100, 0, 5000.0))
            .map(|(a, b)| a.saturating_add(b))
            .collect();
        let mut heard_after = Vec::new();
        let mut unfed_after = Vec::new();
        let mut raw_after = Vec::new();
        for block in 0..seconds * 100 {
            let speakers = &played[block * FRAME..(block + 1) * FRAME];
            let stereo: Vec<i16> = speakers.iter().flat_map(|&s| [s, s]).collect();
            echo.render(&stereo);
            // The room returns it a frame later at half the level.
            let source = if block == 0 {
                vec![0; FRAME]
            } else {
                played[(block - 1) * FRAME..block * FRAME].to_vec()
            };
            let mut mic: Vec<i16> = source.iter().map(|&s| s / 2).collect();
            let mut mic_unfed = mic.clone();
            let raw = mic.clone();
            processor.lock().unwrap().capture(&mut mic, 10);
            unfed.capture(&mut mic_unfed, 10);
            if block >= (seconds - 2) * 100 {
                heard_after.extend_from_slice(&mic);
                unfed_after.extend_from_slice(&mic_unfed);
                raw_after.extend_from_slice(&raw);
            }
        }
        let (raw, cancelled, uncancelled) = (
            energy(&raw_after),
            energy(&heard_after),
            energy(&unfed_after),
        );
        let suppression_db = 10.0 * (raw / cancelled.max(1.0)).log10();
        assert!(
            suppression_db > 20.0,
            "only {suppression_db:.1} dB taken out (raw {raw:.0}, after {cancelled:.0})"
        );
        let unfed_db = 10.0 * (raw / uncancelled.max(1.0)).log10();
        assert!(
            unfed_db < 6.0,
            "without the reverse stream nothing should be cancelled, but {unfed_db:.1} dB was"
        );
    }

    #[test]
    fn no_processing_leaves_the_samples_alone() {
        let echo = Echo::new();
        let processor = echo.processor(Processing::NONE);
        let original = sine(440.0, 1, 0, 8000.0);
        let mut frame = original.clone();
        processor.lock().unwrap().capture(&mut frame, 0);
        assert_eq!(frame, original);
    }
}
