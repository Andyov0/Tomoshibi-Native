//! Every remote sound mixed into one 48 kHz stereo stream, ten milliseconds
//! at a time, for the one playback stream the speakers get.
//!
//! The sounds arrive on the core's threads at WebRTC's pace (`attach_pcm`
//! hands over 10 ms whenever the core's audio clock ticks) and are taken out
//! at the sound card's, which is a different clock. So each sound waits in a
//! queue of its own: it starts only once two frames are queued, which rides
//! over the two clocks' ticks not lining up. A queue that runs dry plays
//! what it has, then silence, and waits for two frames again, rather than
//! clicking in and out a frame at a time.
//!
//! A queue that grows stays grown unless something takes it back: one stall
//! of the card (a busy machine) left three remote voices fifteen frames deep
//! for the rest of a call, 150 ms later than they need be, because the old
//! rule cut a queue only past twenty frames, as Windows' Playback.cs does.
//! So the shallowest each queue has been over the last half second is
//! watched, and whatever of it stands above [`TARGET`] is taken back
//! gradually: a frame that is all but silent (the gap between two words) is
//! dropped whole, which nobody hears, and otherwise every frame is played a
//! hundredth faster than it came until the excess is gone, which nobody hears
//! either. A queue past [`CEILING`] is still cut back at once, for the burst
//! that arrives after the network itself stalled.
//!
//! The lossless streams (`LosslessSink`) come at the sender's own rate, as
//! floats. Each is taken to 48 kHz stereo by WebRTC's own resampler, one per
//! stream since it keeps the filter's state from one block to the next, and
//! then queued like any other sound.
//!
//! Levels. Each sound is a voice, a shared screen's sound or music, and is
//! played at its person's level for that kind (hearing.ts keeps one per
//! person and kind; the sound panel has a row each): `set_level` is the
//! person's level for everything of theirs, `set_kind_level` the level of
//! their screen's sound or their music, which stands in place of the
//! person's where it is set. Music is further turned down under speech
//! (`set_duck`, duck.ts). A gain is eased across a frame rather than
//! stepped, since a gain stepped under speech clicks.
//!
//! The sum is limited rather than clipped. Four people talking at once at the
//! levels a browser's gain control sends summed to full scale for 15% of the
//! samples, and truncating them to 16 bits put a buzz of odd harmonics 39 dB
//! up on everything. The limiter brings the whole mix down, fast when it
//! would go over and slowly back, and a soft knee catches what the first
//! moment of a frame still overshoots.

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
/// Frames a queue settles at once it has been taken back gradually: 40 ms,
/// two frames above the start, which rides over the ordinary unevenness of
/// both clocks.
const TARGET: usize = 4;
/// Mixes over which the shallowest a queue has been is measured: half a
/// second.
const WINDOW: u32 = 50;
/// Frames past which a queue is cut back at once: 200 ms.
const CEILING: usize = 20;
/// Frames a queue is cut back to: 40 ms.
const SETTLE: usize = 4;
/// Samples of one channel taken out of every frame while a queue is being
/// taken back: a hundredth faster.
const SHRINK: usize = FRAME / 100;
/// A frame whose loudest sample is below this (about -54 dBFS) is the
/// silence between words, and is dropped whole when a queue is too long.
const QUIET: i32 = 64;
/// Frames in a row with no silence to drop before a queue owed back is
/// played faster instead: a quarter of a second, longer than any word, so
/// that speech is taken back in its gaps and only music, which has none, is
/// sped up.
const NO_GAP: u32 = 25;

/// The loudest anybody may make somebody else: three times as loud as it was
/// sent (hearing.ts `MAX_VOLUME`; the owner asked for it on 2026-11-05). Past
/// one the slider says it can harm hearing.
pub const MAX_VOLUME: f32 = 3.0;

/// The level the limiter holds the mix under: -1 dBFS.
const CEILING_LEVEL: f32 = 0.891 * 32767.0;
/// Where the soft knee begins, for what overshoots inside a frame.
const KNEE: f32 = 0.95 * 32767.0;
/// How much of the way back to full gain the limiter goes each frame once
/// the mix is quieter: about 150 ms to recover.
const RELEASE: f32 = 0.065;

/// What a sound is, for its person's level of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    Voice,
    /// A shared screen's sound, or sound shared on its own.
    Screen,
    /// The song desk's music.
    Music,
}

/// One sound waiting to be played.
struct Source {
    identity: String,
    kind: Kind,
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
    /// The shallowest the queue has been in this window, in frames, and how
    /// many mixes the window has run.
    shallowest: usize,
    mixes: u32,
    /// Samples (of every channel) still to be taken back from the queue.
    excess: usize,
    /// Frames played in a row, while something was owed, with no silence
    /// to drop.
    without_gap: u32,
    /// How loud the last few frames played were, before any gain, as an RMS
    /// from nought to one: what the duck listens to.
    recent: [f32; 4],
    recent_at: usize,
    /// What has arrived and been played, for the log.
    reading: Reading,
}

#[derive(Default, Clone, Copy)]
struct Reading {
    frames_in: u64,
    frames_out: u64,
    dropped: u64,
    trimmed: u64,
    squares: f64,
    samples: u64,
}

impl Source {
    fn new(identity: &str, kind: Kind, channels: usize) -> Source {
        Source {
            identity: identity.to_owned(),
            kind,
            channels,
            queue: VecDeque::with_capacity(FRAME * 2 * (CEILING + 1)),
            playing: false,
            gain: 0.0,
            ending: false,
            resampler: None,
            shallowest: usize::MAX,
            mixes: 0,
            excess: 0,
            without_gap: 0,
            recent: [0.0; 4],
            recent_at: 0,
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
            self.excess = 0;
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
            self.excess = 0;
        }
    }

    /// Watches how shallow the queue gets, and once a window says it stood
    /// above the target the whole time, owes that much back.
    fn watch_depth(&mut self) {
        self.shallowest = self.shallowest.min(self.queued_frames());
        self.mixes += 1;
        if self.mixes < WINDOW {
            return;
        }
        if self.shallowest != usize::MAX && self.shallowest > TARGET {
            let owed = (self.shallowest - TARGET) * FRAME * self.channels;
            self.excess = self.excess.max(owed);
        }
        self.shallowest = usize::MAX;
        self.mixes = 0;
    }

    /// The next frame of this sound, one channel or two interleaved, into
    /// `frame` (`FRAME * channels`); how much of it was there. Takes back
    /// what is owed of a queue that stood too long on the way.
    fn next_frame(&mut self, frame: &mut [i16]) -> usize {
        let per = FRAME * self.channels;
        // The silence between two words, dropped whole while there is
        // something to take back and something after it to play.
        while self.excess >= per && self.queue.len() >= per * 2 {
            let quiet = self
                .queue
                .iter()
                .take(per)
                .all(|sample| i32::from(*sample).abs() < QUIET);
            if !quiet {
                break;
            }
            self.queue.drain(..per);
            self.excess -= per;
            self.without_gap = 0;
            self.reading.trimmed += 1;
        }
        let shrink = SHRINK * self.channels;
        if self.excess < shrink {
            self.without_gap = 0;
        } else {
            self.without_gap += 1;
        }
        if self.without_gap > NO_GAP && self.queue.len() >= per + shrink {
            // A hundredth faster: FRAME + SHRINK samples of each channel
            // read into FRAME, by linear interpolation.
            let read = FRAME + SHRINK;
            let channels = self.channels;
            let (first, second) = self.queue.as_slices();
            let at = |index: usize| -> f32 {
                let value = if index < first.len() {
                    first[index]
                } else {
                    second[index - first.len()]
                };
                f32::from(value)
            };
            for out in 0..FRAME {
                let position = out as f32 * (read - 1) as f32 / (FRAME - 1) as f32;
                let below = position.floor() as usize;
                let above = (below + 1).min(read - 1);
                let part = position - below as f32;
                for channel in 0..channels {
                    let a = at(below * channels + channel);
                    let b = at(above * channels + channel);
                    frame[out * channels + channel] = (a + (b - a) * part).round() as i16;
                }
            }
            self.queue.drain(..read * channels);
            self.excess -= shrink;
            return FRAME;
        }
        let available = (self.queue.len() / self.channels).min(FRAME);
        for (slot, sample) in frame
            .iter_mut()
            .zip(self.queue.drain(..available * self.channels))
        {
            *slot = sample;
        }
        for slot in frame.iter_mut().skip(available * self.channels) {
            *slot = 0;
        }
        available
    }

    fn heard(&mut self, frame: &[i16]) {
        let squares: f64 = frame.iter().map(|&s| f64::from(s) * f64::from(s)).sum();
        let rms = (squares / frame.len().max(1) as f64).sqrt() / 32768.0;
        self.recent[self.recent_at] = rms as f32;
        self.recent_at = (self.recent_at + 1) % self.recent.len();
    }

    fn loudness(&self) -> f32 {
        self.recent.iter().copied().fold(0.0, f32::max)
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
    kind_levels: HashMap<(String, Kind), f32>,
    ducks: HashMap<String, f32>,
    silent: bool,
    /// The limiter's gain at the end of the last frame.
    limit: f32,
    /// Frames the limiter turned down, for the log.
    limited: u64,
}

impl State {
    fn level_of(&self, identity: &str, kind: Kind) -> f32 {
        let person = self.levels.get(identity).copied().unwrap_or(1.0);
        let level = match kind {
            Kind::Voice => person,
            _ => self
                .kind_levels
                .get(&(identity.to_owned(), kind))
                .copied()
                .unwrap_or(person),
        };
        if kind == Kind::Music {
            level * self.ducks.get(identity).copied().unwrap_or(1.0)
        } else {
            level
        }
    }
}

/// The mixer: shared by the sinks the core feeds and the playback thread.
pub struct Mixer {
    state: Mutex<State>,
}

impl Default for Mixer {
    fn default() -> Self {
        Mixer {
            state: Mutex::new(State {
                limit: 1.0,
                ..State::default()
            }),
        }
    }
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

    /// Starts expecting a track's sound, `identity`'s voice.
    pub fn add(&self, sid: &str, identity: &str) {
        self.add_kind(sid, identity, Kind::Voice);
    }

    /// Starts expecting a sound of this kind under `key`, `identity`'s.
    pub fn add_kind(&self, key: &str, identity: &str, kind: Kind) {
        let mut state = self.state();
        let source = state
            .sources
            .entry(key.to_owned())
            .or_insert_with(|| Source::new(identity, kind, 1));
        source.identity = identity.to_owned();
        source.kind = kind;
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
        let kind = if music { Kind::Music } else { Kind::Screen };
        let mut state = self.state();
        let source = state
            .sources
            .entry(key)
            .or_insert_with(|| Source::new(identity, kind, 2));
        if source.ending {
            *source = Source::new(identity, kind, 2);
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

    /// One person's level for everything of theirs, from nought to three
    /// times as sent ([`MAX_VOLUME`]). Past one is only a larger number here:
    /// the samples are multiplied by it, and the limiter holds the sum under
    /// full scale as it holds two loud voices together.
    pub fn set_level(&self, identity: &str, level: f32) {
        self.state()
            .levels
            .insert(identity.to_owned(), level.clamp(0.0, MAX_VOLUME));
    }

    /// One person's level for their screen's sound (`music` false) or their
    /// music, in place of their level for everything.
    pub fn set_kind_level(&self, identity: &str, music: bool, level: f32) {
        let kind = if music { Kind::Music } else { Kind::Screen };
        self.state()
            .kind_levels
            .insert((identity.to_owned(), kind), level.clamp(0.0, MAX_VOLUME));
    }

    /// How far a person's music is turned down under speech, from nought to
    /// one (one: not at all).
    pub fn set_duck(&self, identity: &str, factor: f32) {
        let mut state = self.state();
        if factor >= 1.0 {
            state.ducks.remove(identity);
        } else {
            state
                .ducks
                .insert(identity.to_owned(), factor.clamp(0.0, 1.0));
        }
    }

    /// How loud each person's voice was over the last 40 ms as it was
    /// played, before their level: an RMS from nought to one, the loudest of
    /// their voices.
    pub fn voice_levels(&self) -> Vec<(String, f32)> {
        let state = self.state();
        let mut levels: HashMap<String, f32> = HashMap::new();
        for source in state.sources.values() {
            if source.kind != Kind::Voice {
                continue;
            }
            let entry = levels.entry(source.identity.clone()).or_insert(0.0);
            *entry = entry.max(source.loudness());
        }
        levels.into_iter().collect()
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
        let mut sum = [0f32; FRAME * 2];
        let mut frame = [0i16; FRAME * 2];
        let mut state = self.state();
        let targets: Vec<(String, f32)> = state
            .sources
            .iter()
            .map(|(key, source)| {
                let target = if source.ending {
                    0.0
                } else {
                    state.level_of(&source.identity, source.kind)
                };
                (key.clone(), target)
            })
            .collect();
        let mut finished = Vec::new();
        for (key, target) in targets {
            let Some(source) = state.sources.get_mut(&key) else {
                continue;
            };
            source.watch_depth();
            if !source.playing {
                if source.queued_frames() >= START || (source.ending && !source.queue.is_empty()) {
                    source.playing = true;
                } else {
                    if source.ending {
                        finished.push(key);
                    }
                    source.heard(&[]);
                    continue;
                }
            }
            let channels = source.channels;
            let used = &mut frame[..FRAME * channels];
            let available = source.next_frame(used);
            source.heard(&used[..available * channels]);
            let from = source.gain;
            for index in 0..FRAME {
                let gain = from + (target - from) * ((index + 1) as f32 / FRAME as f32);
                let (left, right) = if channels == 2 {
                    (used[index * 2], used[index * 2 + 1])
                } else {
                    (used[index], used[index])
                };
                sum[index * 2] += f32::from(left) * gain;
                sum[index * 2 + 1] += f32::from(right) * gain;
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
                finished.push(key);
            }
        }
        for key in finished {
            state.sources.remove(&key);
        }
        let silent = state.silent;
        let (gain, limited) = limit(&mut sum, state.limit);
        state.limit = gain;
        state.limited += u64::from(limited);
        drop(state);
        for (slot, value) in out.iter_mut().zip(sum.iter()) {
            *slot = if silent { 0 } else { *value as i16 };
        }
    }

    /// One line per sound since the last reading, for the debug log: what
    /// arrived, what was played, what was cut or taken back, and how loud it
    /// was in dBFS; and how often the limiter turned the mix down.
    pub fn reading(&self) -> Vec<String> {
        let mut state = self.state();
        let limited = std::mem::take(&mut state.limited);
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
                    "{key} ({} {:?}) in={} out={} cut={} trimmed={} queued={} level={dbfs:.1} dBFS",
                    source.identity,
                    source.kind,
                    reading.frames_in,
                    reading.frames_out,
                    reading.dropped,
                    reading.trimmed,
                    source.queued_frames(),
                )
            })
            .collect();
        lines.sort();
        if limited > 0 {
            lines.push(format!("the limiter turned {limited} frames down"));
        }
        lines
    }

    /// How many sounds are being mixed, lossless streams among them.
    pub fn sounds(&self) -> usize {
        self.state().sources.len()
    }
}

/// Holds one frame of the mix under [`CEILING_LEVEL`]: the gain `from` the
/// last frame eases down to what this frame's loudest sample needs, or back
/// up toward one; whatever still goes past the knee is bent under full scale
/// rather than cut at it. The gain at the end, and whether it turned the
/// frame down.
fn limit(sum: &mut [f32], from: f32) -> (f32, bool) {
    let peak = sum.iter().fold(0f32, |peak, value| peak.max(value.abs()));
    let needed = if peak > CEILING_LEVEL {
        CEILING_LEVEL / peak
    } else {
        1.0
    };
    let to = if needed < from {
        needed
    } else {
        let back = from + (1.0 - from) * RELEASE;
        // All the way back once it is within a fiftieth of a decibel, rather
        // than for ever a hair under.
        (if back > 0.998 { 1.0 } else { back }).min(needed)
    };
    let frames = sum.len() / 2;
    for (index, pair) in sum.as_chunks_mut::<2>().0.iter_mut().enumerate() {
        let gain = from + (to - from) * ((index + 1) as f32 / frames as f32);
        for value in pair {
            *value = knee(*value * gain);
        }
    }
    (to, to < 0.999)
}

/// Full scale approached rather than reached: linear to the knee, then bent
/// toward full scale along a curve that never gets there.
fn knee(value: f32) -> f32 {
    let size = value.abs();
    if size <= KNEE {
        return value;
    }
    let room = 32767.0 - KNEE;
    let over = (size - KNEE) / room;
    let bent = KNEE + room * (over / (1.0 + over));
    bent.copysign(value)
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

    fn queued(mixer: &Mixer) -> usize {
        let reading = mixer.reading();
        let line = reading
            .iter()
            .find(|line| line.contains("queued="))
            .unwrap();
        line.split("queued=")
            .nth(1)
            .unwrap()
            .split(' ')
            .next()
            .unwrap()
            .parse()
            .unwrap()
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

    /// Two loud voices together are limited, not truncated: the mix stays
    /// under full scale, and no sample sits clipped at it. Without the
    /// limiter (`limit` passing the sum through) every sample was 32767.
    #[test]
    fn sounds_add_and_are_limited_and_levels_follow_the_person() {
        let mixer = Mixer::new();
        mixer.add("TR_a", "ann");
        mixer.add("TR_b", "bob");
        mixer.set_level("bob", 0.5);
        let feed = |mixer: &Mixer| {
            mixer.push("TR_a", &tone(1, 1, 25_000), 1);
            mixer.push("TR_b", &tone(1, 2, 25_000), 2);
            mixed(mixer)
        };
        feed(&mixer);
        feed(&mixer);
        let out = feed(&mixer);
        // 25000 + 12500 held at the limiter's ceiling.
        let ceiling = CEILING_LEVEL as i16;
        assert!(
            out.iter().all(|&s| (s - ceiling).abs() <= 2),
            "{:?}",
            &out[..4]
        );
        assert!(out.iter().all(|&s| s < i16::MAX));
        // Quieter again, the limiter lets go over a few frames and the
        // sound comes through as it is.
        mixer.set_level("ann", 0.0);
        for _ in 0..100 {
            feed(&mixer);
        }
        let out = feed(&mixer);
        assert!(
            out.iter().all(|&s| (s - 12_500).abs() <= 2),
            "{:?}",
            &out[..4]
        );
    }

    /// A person turned up past as sent comes out that much louder, up to
    /// three times and no further. With the old clamp at one, 2.5 played as
    /// sent.
    #[test]
    fn a_level_past_one_makes_the_sound_louder_up_to_three_times() {
        let mixer = Mixer::new();
        mixer.add("TR_a", "ann");
        mixer.set_level("ann", 2.5);
        let feed = |mixer: &Mixer| {
            mixer.push("TR_a", &tone(1, 1, 4000), 1);
            mixed(mixer)
        };
        for _ in 0..4 {
            feed(&mixer);
        }
        let out = feed(&mixer);
        assert!(
            out.iter().all(|&s| (s - 10_000).abs() <= 2),
            "{:?}",
            &out[..4]
        );
        mixer.set_level("ann", 7.0);
        for _ in 0..4 {
            feed(&mixer);
        }
        let out = feed(&mixer);
        assert!(
            out.iter().all(|&s| (s - 12_000).abs() <= 2),
            "{:?}",
            &out[..4]
        );
    }

    #[test]
    fn the_knee_bends_and_never_reaches_full_scale() {
        assert_eq!(knee(1000.0), 1000.0);
        assert_eq!(knee(-1000.0), -1000.0);
        let bent = knee(60_000.0);
        assert!(bent < 32767.0 && bent > KNEE, "{bent}");
        assert!(knee(-60_000.0) > -32767.0);
        assert!(knee(40_000.0) < knee(50_000.0), "it still rises");
    }

    #[test]
    fn levels_of_screens_and_music_stand_apart_and_music_ducks() {
        let mixer = Mixer::new();
        mixer.add_kind("TR_v", "ann", Kind::Voice);
        mixer.add_kind("TR_m", "ann", Kind::Music);
        mixer.set_level("ann", 0.5);
        let state = mixer.state();
        assert_eq!(state.level_of("ann", Kind::Voice), 0.5);
        assert_eq!(
            state.level_of("ann", Kind::Music),
            0.5,
            "the person's, until set"
        );
        drop(state);
        mixer.set_kind_level("ann", true, 0.8);
        mixer.set_duck("ann", 0.3);
        let state = mixer.state();
        assert_eq!(state.level_of("ann", Kind::Voice), 0.5);
        assert!((state.level_of("ann", Kind::Music) - 0.24).abs() < 1e-6);
        assert_eq!(state.level_of("ann", Kind::Screen), 0.5);
        drop(state);
        mixer.set_duck("ann", 1.0);
        assert!((mixer.state().level_of("ann", Kind::Music) - 0.8).abs() < 1e-6);
    }

    #[test]
    fn a_long_queue_is_cut_back_and_a_dry_one_waits_again() {
        let mixer = Mixer::new();
        mixer.add("TR_a", "ann");
        mixer.push("TR_a", &tone(CEILING + 5, 1, 100), 1);
        assert_eq!(queued(&mixer), SETTLE);
        for _ in 0..SETTLE {
            mixed(&mixer);
        }
        // Dry: the next frame alone does not restart it.
        mixer.push("TR_a", &tone(1, 1, 100), 1);
        assert!(mixed(&mixer).iter().all(|&s| s == 0));
    }

    /// The fault of the test calls: a stall pushes a queue to fifteen frames
    /// and nothing ever brings it back. Kept fed a frame a mix, it now
    /// comes back to the target within seconds, by playing a hundredth
    /// faster where there is no silence to drop. Broken (no taking back),
    /// the queue is still fifteen at the end.
    #[test]
    fn a_queue_that_stands_too_long_is_taken_back_gradually() {
        let mixer = Mixer::new();
        mixer.add("TR_a", "ann");
        mixer.push("TR_a", &tone(15, 1, 8000), 1);
        let mut deepest = 0;
        for _ in 0..2000 {
            mixer.push("TR_a", &tone(1, 1, 8000), 1);
            let out = mixed(&mixer);
            assert!(out[FRAME] != 0, "never runs dry meanwhile");
            deepest = deepest.max(queued(&mixer));
        }
        assert!(deepest <= 16);
        let now = queued(&mixer);
        assert!(now <= TARGET + 1, "still {now} frames queued");
    }

    /// Speech has gaps, and a gap is dropped whole: far quicker than playing
    /// faster, and nothing of a word is changed.
    #[test]
    fn silence_between_words_is_dropped_first() {
        let mixer = Mixer::new();
        mixer.add("TR_a", "ann");
        // Fifteen frames: a word, a gap, a word, again and again.
        let pattern = |n: usize| if n % 3 == 1 { 0 } else { 6000 };
        for n in 0..15 {
            mixer.push("TR_a", &tone(1, 1, pattern(n)), 1);
        }
        for n in (15..).take(300) {
            mixer.push("TR_a", &tone(1, 1, pattern(n)), 1);
            mixed(&mixer);
        }
        assert!(queued(&mixer) <= TARGET + 1, "{:?}", mixer.reading());
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
    fn voices_are_heard_for_the_duck_and_music_is_not() {
        let mixer = Mixer::new();
        mixer.add("TR_a", "ann");
        mixer.add_kind("TR_m", "bob", Kind::Music);
        for _ in 0..4 {
            mixer.push("TR_a", &tone(1, 1, 3277), 1);
            mixer.push("TR_m", &tone(1, 2, 3277), 2);
        }
        mixed(&mixer);
        let levels = mixer.voice_levels();
        assert_eq!(levels.len(), 1);
        assert_eq!(levels[0].0, "ann");
        assert!((levels[0].1 - 0.1).abs() < 0.01, "{levels:?}");
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
