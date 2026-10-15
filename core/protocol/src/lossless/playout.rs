//! Playing lossless sound out: the buffer between the network and the speakers.
//! Follows web/src/live/playout.ts, and playout.worklet.ts for [`Worklet`].
//!
//! Packets arrive in bursts -- a retransmission holds up everything behind it,
//! and then it all arrives at once -- and the sound card asks for frames at a
//! steady pace. The jitter buffer every audio track runs absorbs that by
//! stretching and squeezing the sound and inventing what never arrived, which
//! is the thing this whole path exists to avoid. So this one only ever waits:
//!
//! - It does not start until it holds `target` frames, and while it plays,
//!   every frame it puts out is the next frame that arrived, unaltered.
//! - If it runs dry it says so, goes quiet, and waits for `target` again. A gap,
//!   never an invention.
//! - Past `ceiling` frames it drops whole packets from the front, back to
//!   `target`. Two machines' clocks never agree exactly, and a sender a hair
//!   faster than this sound card would otherwise build a backlog for as long
//!   as the music played. At the usual tens of parts per million that is one
//!   skip in some hours; resampling to track the drift would alter every
//!   sample to avoid altering a few.
//!
//! It never resamples, so whatever plays it has to run at the stream's own
//! rate: the web client opens an audio context at that rate for each stream,
//! and leaves any conversion to the operating system, which treats every
//! sound on the machine alike.

use std::collections::VecDeque;

/// Seconds held before playing: enough for a retransmission across a border.
/// The web client keeps it in lossless-receiver.ts.
pub const TARGET_SECONDS: f64 = 0.6;

/// Past this many seconds held, whole packets are dropped back to the target.
pub const CEILING_SECONDS: f64 = 3.0;

/// The target, in frames, for a stream at `rate`.
pub fn target_frames(rate: u32) -> usize {
    (TARGET_SECONDS * f64::from(rate)).round() as usize
}

/// The ceiling, in frames, for a stream at `rate`.
pub fn ceiling_frames(rate: u32) -> usize {
    (CEILING_SECONDS * f64::from(rate)).round() as usize
}

/// How a playout is doing.
///
/// Exported for the apps, which may show it: the web client's panel does.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, uniffi::Record)]
pub struct PlayoutCounts {
    /// Times it ran dry while playing.
    pub underruns: u32,
    /// Frames dropped to stay within the ceiling.
    pub skipped: u64,
    /// Frames it holds now.
    pub held: u64,
    /// Frames played since it started.
    pub played: u64,
    pub playing: bool,
}

/// The buffer itself. Planar: a block is one vector per channel, all the same
/// length, and so is what it fills.
#[derive(Debug, Clone)]
pub struct Playout {
    channels: usize,
    target: usize,
    ceiling: usize,
    queue: VecDeque<Vec<Vec<f32>>>,
    /// How far into the first block of the queue playback has got.
    offset: usize,
    held: usize,
    playing: bool,
    underruns: u32,
    skipped: u64,
    played: u64,
}

impl Playout {
    /// `channels` is the stream's; `target` and `ceiling` are in frames.
    pub fn new(channels: usize, target: usize, ceiling: usize) -> Self {
        Self {
            channels: channels.max(1),
            target,
            ceiling,
            queue: VecDeque::new(),
            offset: 0,
            held: 0,
            playing: false,
            underruns: 0,
            skipped: 0,
            played: 0,
        }
    }

    pub fn target(&self) -> usize {
        self.target
    }

    pub fn ceiling(&self) -> usize {
        self.ceiling
    }

    /// Add a block: one vector per channel, all the same length.
    pub fn push(&mut self, block: Vec<Vec<f32>>) {
        let frames = block.first().map_or(0, Vec::len);
        if frames == 0 {
            return;
        }

        self.queue.push_back(block);
        self.held += frames;

        if self.held > self.ceiling {
            while self.held > self.target && self.queue.len() > 1 {
                let first = self
                    .queue
                    .pop_front()
                    .expect("more than one block is queued");
                let dropped = first.first().map_or(0, Vec::len) - self.offset;
                self.offset = 0;
                self.held -= dropped;
                self.skipped += dropped as u64;
            }
        }

        if !self.playing && self.held >= self.target {
            self.playing = true;
        }
    }

    /// Fill `out`, one slice per output channel, with the next frames, or
    /// silence. Any length: a sound card's 128 frames, ten milliseconds, or
    /// whatever the output asks for.
    ///
    /// A mono stream goes to every output channel; a stereo one to its own,
    /// and a third output channel or more repeats the last. The output
    /// channels are meant to be the same length; if they are not, the
    /// shortest decides how much is taken, rather than one channel running
    /// ahead of another.
    pub fn pull<C: AsMut<[f32]>>(&mut self, out: &mut [C]) {
        let frames = out
            .iter_mut()
            .map(|channel| channel.as_mut().len())
            .min()
            .unwrap_or(0);
        let mut written = 0;

        if self.playing {
            while written < frames {
                let Some(first) = self.queue.front() else {
                    break;
                };
                let length = first.first().map_or(0, Vec::len);
                let take = (frames - written).min(length - self.offset);

                for (c, channel) in out.iter_mut().enumerate() {
                    let index = c.min(self.channels - 1).min(first.len() - 1);
                    let source = &first[index][self.offset..self.offset + take];
                    channel.as_mut()[written..written + take].copy_from_slice(source);
                }

                written += take;
                self.offset += take;
                self.held -= take;
                if self.offset == length {
                    self.queue.pop_front();
                    self.offset = 0;
                }
            }

            self.played += written as u64;

            if written < frames {
                self.playing = false;
                self.underruns += 1;
            }
        }

        for channel in out.iter_mut() {
            let channel = channel.as_mut();
            let from = written.min(channel.len());
            channel[from..].fill(0.0);
        }
    }

    pub fn counts(&self) -> PlayoutCounts {
        PlayoutCounts {
            underruns: self.underruns,
            skipped: self.skipped,
            held: self.held as u64,
            played: self.played,
            playing: self.playing,
        }
    }
}

/// The audio-thread side of the web client's playout: a shell around
/// [`Playout`] that reports its counts about once a second.
///
/// The once a second matters. The receiver decides a stream is starved from
/// the counts it was last told, so it learns of an underrun up to a second
/// after the fact; a player that handed over live counts would give up on a
/// stream sooner than a browser in the same room does. [`Worklet::counts`] is
/// that last report, and nothing until the first.
#[derive(Debug, Clone)]
pub struct Worklet {
    playout: Playout,
    rate: u32,
    since_report: u64,
    latest: Option<PlayoutCounts>,
}

impl Worklet {
    /// A playout for a stream at `rate` with `channels`, with the target and
    /// ceiling the web client's receiver gives its worklet.
    pub fn new(rate: u32, channels: usize) -> Self {
        Self {
            playout: Playout::new(channels, target_frames(rate), ceiling_frames(rate)),
            rate,
            since_report: 0,
            latest: None,
        }
    }

    pub fn push(&mut self, block: Vec<Vec<f32>>) {
        self.playout.push(block);
    }

    /// What the sound card asks for. Reports once at least a second of frames
    /// has been asked for since the last report, counting a request with no
    /// channels as a render quantum of 128, as the worklet does.
    pub fn pull<C: AsMut<[f32]>>(&mut self, out: &mut [C]) {
        if !out.is_empty() {
            self.playout.pull(out);
        }

        let frames = out
            .first_mut()
            .map_or(128, |channel| channel.as_mut().len());
        self.since_report += frames as u64;
        if self.since_report >= u64::from(self.rate) {
            self.since_report = 0;
            self.latest = Some(self.playout.counts());
        }
    }

    /// The counts as last reported.
    pub fn counts(&self) -> Option<PlayoutCounts> {
        self.latest
    }

    /// The buffer, with its live counts.
    pub fn playout(&self) -> &Playout {
        &self.playout
    }

    pub fn rate(&self) -> u32 {
        self.rate
    }
}
