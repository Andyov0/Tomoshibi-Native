//! Every remote sound mixed into one 48 kHz stereo stream, ten milliseconds
//! at a time, for the one playback stream the speakers get.
//!
//! The sounds arrive on the core's threads at WebRTC's pace (`attach_pcm`
//! hands over 10 ms whenever the core's audio clock ticks) and are taken out
//! at the sound card's, which is a different clock. So each sound waits in a
//! queue of its own: it starts only once two frames are queued, which rides
//! over the two clocks' ticks not lining up, and a queue that has grown past
//! a fifth of a second, which is the card running slower than the network
//! for long enough to talk over somebody, is cut back to forty milliseconds
//! (Windows' Playback.cs drops at the same fifth of a second). A queue that
//! runs dry plays what it has, then silence, and waits for two frames again,
//! rather than clicking in and out a frame at a time.
//!
//! The lossless streams (`LosslessSink`) come at the sender's own rate, as
//! floats. Each is taken to 48 kHz stereo by WebRTC's own resampler, one per
//! stream since it keeps the filter's state from one block to the next, and
//! then queued like any other sound.
//!
//! Each person's level (the sound panel's sliders, `Media::set_volume`)
//! applies to everything of theirs, voice, screen sound, music and the
//! lossless streams standing in for the last two; it is eased across a
//! frame rather than stepped, since a gain stepped under speech clicks.

use std::{
    collections::{HashMap, VecDeque},
    sync::Mutex,
};

use libwebrtc::native::audio_resampler::AudioResampler;

/// The rate everything is mixed and played at.
pub const RATE: u32 = 48_000;
/// Ten milliseconds of one channel at [`RATE`].
pub const FRAME: usize = (RATE / 100) as usize;

/// Frames queued before a sound starts, or starts again after running dry.
const START: usize = 2;
/// Frames past which a queue is cut back: 200 ms.
const CEILING: usize = 20;
/// Frames a queue is cut back to: 40 ms.
const SETTLE: usize = 4;

/// One sound waiting to be played.
struct Source {
    identity: String,
    /// The channels its samples are interleaved in: 1 or 2.
    channels: usize,
    /// Interleaved 48 kHz samples not yet played.
    queue: VecDeque<i16>,
    /// Whether it is past its start (or restart) cushion.
    playing: bool,
    /// The gain it was played at at the end of the last frame, eased toward
    /// the person's level.
    gain: f32,
    /// A lossless stream that has ended: it fades out to nothing over the
    /// next frame and is then taken away.
    ending: bool,
    /// For a lossless stream not at 48 kHz stereo: the resampler that takes
    /// it there, and the rate and channels it was made for.
    resampler: Option<(AudioResampler, u32, usize)>,
    /// What has arrived and been played, for the log.
    reading: Reading,
}

#[derive(Default, Clone, Copy)]
struct Reading {
    frames_in: u64,
    frames_out: u64,
    dropped: u64,
    squares: f64,
    samples: u64,
}

impl Source {
    fn new(identity: &str, channels: usize) -> Source {
        Source {
            identity: identity.to_owned(),
            channels,
            queue: VecDeque::with_capacity(FRAME * 2 * (CEILING + 1)),
            playing: false,
            gain: 0.0,
            ending: false,
            resampler: None,
            reading: Reading::default(),
        }
    }

    fn queued_frames(&self) -> usize {
        self.queue.len() / (FRAME * self.channels)
    }

    fn take(&mut self, samples: &[i16], channels: usize) {
        if channels != self.channels {
            // The same track never changes its channels; a key reused for
            // another shape starts again.
            self.channels = channels;
            self.queue.clear();
            self.playing = false;
        }
        let mut squares = 0f64;
        for &sample in samples {
            squares += f64::from(sample) * f64::from(sample);
        }
        self.reading.squares += squares;
        self.reading.samples += samples.len() as u64;
        self.reading.frames_in += (samples.len() / (FRAME * channels).max(1)) as u64;
        self.queue.extend(samples.iter().copied());
        if self.queued_frames() > CEILING {
            let keep = SETTLE * FRAME * self.channels;
            let cut = self.queue.len() - keep;
            self.queue.drain(..cut);
            self.reading.dropped += (cut / (FRAME * self.channels)) as u64;
        }
    }
}

/// The key a lossless stream is kept under, beside the tracks' SIDs.
fn lossless_key(identity: &str, music: bool) -> String {
    format!(
        "lossless/{identity}/{}",
        if music { "music" } else { "screen" }
    )
}

#[derive(Default)]
struct State {
    sources: HashMap<String, Source>,
    levels: HashMap<String, f32>,
    silent: bool,
}

/// The mixer: shared by the sinks the core feeds and the playback thread.
#[derive(Default)]
pub struct Mixer {
    state: Mutex<State>,
}

impl Mixer {
    pub fn new() -> Mixer {
        Mixer::default()
    }

    fn state(&self) -> std::sync::MutexGuard<'_, State> {
        // A panic on another thread while it held the lock leaves the state
        // as it was, which is still a mixer; playing on beats stopping.
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Starts expecting a track's sound, `identity`'s.
    pub fn add(&self, sid: &str, identity: &str) {
        let mut state = self.state();
        state
            .sources
            .entry(sid.to_owned())
            .or_insert_with(|| Source::new(identity, 1))
            .identity = identity.to_owned();
    }

    /// Stops playing a track's sound, whatever is still queued of it.
    pub fn remove(&self, sid: &str) {
        self.state().sources.remove(sid);
    }

    /// Everything goes: the call is over.
    pub fn clear(&self) {
        self.state().sources.clear();
    }

    /// Ten milliseconds (or any whole number of them) of a track's sound,
    /// interleaved, at 48 kHz: what `PcmSink::on_pcm` is given. A track not
    /// added, or already removed, is not played.
    pub fn push(&self, sid: &str, samples: &[i16], channels: usize) {
        if channels == 0 || channels > 2 {
            return;
        }
        let mut state = self.state();
        if let Some(source) = state.sources.get_mut(sid) {
            source.take(samples, channels);
        }
    }

    /// Ten milliseconds of a lossless stream at its own rate, interleaved
    /// floats (`LosslessSink::on_lossless`). A stream that is new, or whose
    /// rate or channels changed, starts afresh.
    pub fn push_lossless(
        &self,
        identity: &str,
        music: bool,
        samples: &[f32],
        rate: u32,
        channels: usize,
    ) {
        // The core drops rates nobody records at before they get here
        // (lossless.rs `playable_rate`); this is the second wall, as on
        // Windows, since a rate of nought has no frames to resample.
        if !(8_000..=384_000).contains(&rate) || !(1..=2).contains(&channels) {
            return;
        }
        let key = lossless_key(identity, music);
        let mut state = self.state();
        let source = state
            .sources
            .entry(key)
            .or_insert_with(|| Source::new(identity, 2));
        if source.ending {
            *source = Source::new(identity, 2);
        }
        let pcm: Vec<i16> = samples
            .iter()
            .map(|sample| (sample.clamp(-1.0, 1.0) * 32767.0).round() as i16)
            .collect();
        if rate == RATE && channels == 2 {
            source.take(&pcm, 2);
            return;
        }
        let fresh = !matches!(&source.resampler, Some((_, r, c)) if *r == rate && *c == channels);
        if fresh {
            source.resampler = Some((AudioResampler::default(), rate, channels));
        }
        let Some((resampler, _, _)) = source.resampler.as_mut() else {
            return;
        };
        // WebRTC's resampler takes ten milliseconds at a time, which is what
        // the core's player drains (`rate / 100` frames a tick).
        let per = (rate / 100) as usize;
        let mut converted = Vec::with_capacity(FRAME * 2);
        for block in pcm.chunks_exact(per * channels) {
            let out =
                resampler.remix_and_resample(block, per as u32, channels as u32, rate, 2, RATE);
            converted.extend_from_slice(out);
        }
        source.take(&converted, 2);
    }

    /// A lossless stream is over (`on_lossless_end`): it fades out.
    pub fn end_lossless(&self, identity: &str, music: bool) {
        if let Some(source) = self.state().sources.get_mut(&lossless_key(identity, music)) {
            source.ending = true;
        }
    }

    /// One person's level for everything of theirs, from nought to one.
    pub fn set_level(&self, identity: &str, level: f32) {
        self.state()
            .levels
            .insert(identity.to_owned(), level.clamp(0.0, 1.0));
    }

    /// Plays silence in place of everything, keeping every queue moving.
    pub fn set_silent(&self, silent: bool) {
        self.state().silent = silent;
    }

    pub fn silent(&self) -> bool {
        self.state().silent
    }

    /// The next ten milliseconds of the mix, stereo interleaved, into `out`
    /// (`2 * FRAME` samples).
    pub fn mix(&self, out: &mut [i16]) {
        let mut sum = [0i32; FRAME * 2];
        let mut state = self.state();
        let State {
            sources,
            levels,
            silent,
        } = &mut *state;
        let mut finished = Vec::new();
        for (key, source) in sources.iter_mut() {
            if !source.playing {
                if source.queued_frames() >= START || (source.ending && !source.queue.is_empty()) {
                    source.playing = true;
                } else {
                    if source.ending {
                        finished.push(key.clone());
                    }
                    continue;
                }
            }
            let target = if source.ending {
                0.0
            } else {
                levels.get(&source.identity).copied().unwrap_or(1.0)
            };
            let from = source.gain;
            let channels = source.channels;
            let available = (source.queue.len() / channels).min(FRAME);
            for frame in 0..FRAME {
                let gain = from + (target - from) * ((frame + 1) as f32 / FRAME as f32);
                let (left, right) = if frame < available {
                    let left = source.queue.pop_front().unwrap_or(0);
                    let right = if channels == 2 {
                        source.queue.pop_front().unwrap_or(0)
                    } else {
                        left
                    };
                    (left, right)
                } else {
                    (0, 0)
                };
                sum[frame * 2] += (f32::from(left) * gain) as i32;
                sum[frame * 2 + 1] += (f32::from(right) * gain) as i32;
            }
            source.gain = target;
            source.reading.frames_out += 1;
            if available < FRAME || source.queue.is_empty() {
                // Ran dry, or will on the next tick: wait for the cushion
                // again rather than play the next frame the moment it
                // comes and have nothing after it.
                source.playing = false;
            }
            if source.ending && (source.gain == 0.0 || source.queue.is_empty()) {
                finished.push(key.clone());
            }
        }
        for key in finished {
            sources.remove(&key);
        }
        let silent = *silent;
        drop(state);
        for (slot, value) in out.iter_mut().zip(sum.iter()) {
            *slot = if silent {
                0
            } else {
                (*value).clamp(i16::MIN as i32, i16::MAX as i32) as i16
            };
        }
    }

    /// One line per sound since the last reading, for the debug log: what
    /// arrived, what was played, what was cut, and how loud it was in dBFS.
    pub fn reading(&self) -> Vec<String> {
        let mut state = self.state();
        let mut lines: Vec<String> = state
            .sources
            .iter_mut()
            .map(|(key, source)| {
                let reading = std::mem::take(&mut source.reading);
                let rms = if reading.samples > 0 {
                    (reading.squares / reading.samples as f64).sqrt() / 32768.0
                } else {
                    0.0
                };
                let dbfs = if rms > 0.0 {
                    20.0 * rms.log10()
                } else {
                    f64::NEG_INFINITY
                };
                format!(
                    "{key} ({}) in={} out={} cut={} queued={} level={dbfs:.1} dBFS",
                    source.identity,
                    reading.frames_in,
                    reading.frames_out,
                    reading.dropped,
                    source.queued_frames(),
                )
            })
            .collect();
        lines.sort();
        lines
    }

    /// How many sounds are being mixed, lossless streams among them.
    pub fn sounds(&self) -> usize {
        self.state().sources.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(frames: usize, channels: usize, value: i16) -> Vec<i16> {
        vec![value; FRAME * frames * channels]
    }

    fn mixed(mixer: &Mixer) -> Vec<i16> {
        let mut out = vec![0i16; FRAME * 2];
        mixer.mix(&mut out);
        out
    }

    #[test]
    fn a_sound_waits_for_its_cushion_then_plays_in_both_channels() {
        let mixer = Mixer::new();
        mixer.add("TR_a", "ann");
        mixer.push("TR_a", &tone(1, 1, 1000), 1);
        assert!(
            mixed(&mixer).iter().all(|&s| s == 0),
            "one frame is not yet enough to start"
        );
        mixer.push("TR_a", &tone(1, 1, 1000), 1);
        let out = mixed(&mixer);
        // Eased in from silence across the first frame, then at full level.
        assert!(out[0].abs() < 10);
        assert_eq!(out[FRAME * 2 - 2], 1000);
        assert_eq!(out[FRAME * 2 - 1], 1000);
        let out = mixed(&mixer);
        assert!(out.iter().all(|&s| s == 1000));
    }

    #[test]
    fn a_track_not_added_or_removed_is_not_played() {
        let mixer = Mixer::new();
        mixer.push("TR_x", &tone(4, 1, 1000), 1);
        assert!(mixed(&mixer).iter().all(|&s| s == 0));
        mixer.add("TR_x", "ann");
        mixer.push("TR_x", &tone(4, 1, 1000), 1);
        mixer.remove("TR_x");
        assert!(mixed(&mixer).iter().all(|&s| s == 0));
    }

    #[test]
    fn sounds_add_and_clip_and_levels_follow_the_person() {
        let mixer = Mixer::new();
        mixer.add("TR_a", "ann");
        mixer.add("TR_b", "bob");
        mixer.set_level("bob", 0.5);
        for _ in 0..4 {
            mixer.push("TR_a", &tone(1, 1, 25_000), 1);
            mixer.push("TR_b", &tone(1, 2, 25_000), 2);
        }
        mixed(&mixer);
        let out = mixed(&mixer);
        // 25000 + 12500 clipped to the 16-bit range.
        assert!(out.iter().all(|&s| s == i16::MAX), "{:?}", &out[..4]);
        mixer.set_level("ann", 0.0);
        mixed(&mixer);
        let out = mixed(&mixer);
        assert!(out.iter().all(|&s| s == 12_500), "{:?}", &out[..4]);
    }

    #[test]
    fn a_long_queue_is_cut_back_and_a_dry_one_waits_again() {
        let mixer = Mixer::new();
        mixer.add("TR_a", "ann");
        mixer.push("TR_a", &tone(CEILING + 5, 1, 100), 1);
        let reading = mixer.reading();
        assert!(
            reading[0].contains(&format!("queued={SETTLE}")),
            "{reading:?}"
        );
        for _ in 0..SETTLE {
            mixed(&mixer);
        }
        // Dry: the next frame alone does not restart it.
        mixer.push("TR_a", &tone(1, 1, 100), 1);
        assert!(mixed(&mixer).iter().all(|&s| s == 0));
    }

    #[test]
    fn silence_keeps_the_queues_moving() {
        let mixer = Mixer::new();
        mixer.add("TR_a", "ann");
        mixer.push("TR_a", &tone(3, 1, 500), 1);
        mixer.set_silent(true);
        assert!(mixed(&mixer).iter().all(|&s| s == 0));
        assert!(mixer.reading()[0].contains("out=1"));
    }

    #[test]
    fn lossless_at_another_rate_is_resampled_and_fades_out_when_it_ends() {
        let mixer = Mixer::new();
        // 44.1 kHz mono, a steady half scale, ten milliseconds at a time.
        let block = vec![0.5f32; 441];
        for _ in 0..6 {
            mixer.push_lossless("ann", true, &block, 44_100, 1);
        }
        let mut last = Vec::new();
        for _ in 0..4 {
            last = mixed(&mixer);
        }
        let middle = last[FRAME];
        assert!((middle - 16_384).abs() < 400, "resampled level {middle}");
        mixer.end_lossless("ann", true);
        mixed(&mixer);
        assert_eq!(
            mixer.sounds(),
            0,
            "an ended stream is taken away once faded"
        );
    }

    #[test]
    fn lossless_refuses_rates_nobody_records_at() {
        let mixer = Mixer::new();
        mixer.push_lossless("ann", false, &[0.5; 10], 0, 1);
        mixer.push_lossless("ann", false, &[0.5; 10], 4, 1);
        mixer.push_lossless("ann", false, &[0.5; 30], 48_000, 3);
        assert_eq!(mixer.sounds(), 0);
    }
}
