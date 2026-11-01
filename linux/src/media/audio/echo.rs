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
//! microphone gives, with what the speakers played as its reverse stream,
//! the echo it is to take out.
//!
//! Each open microphone has its own processor (its settings follow what it
//! is published as); the playback writes what it plays into one history
//! here, and each processor takes its reverse stream from that history on
//! the microphone's own thread, one frame of it before each frame of the
//! microphone, so that the canceller is called in order from one thread.
//!
//! Alignment. The canceller finds the delay between the two itself, and
//! adapts while it holds; but where that delay moves (the speakers and the
//! microphone on two clocks, or a sound server resampling between them, as
//! PulseAudio's loopback does to hold its latency) it lost hold again and
//! again: measured through a 40 ms loopback, a voice leaked back at nearly
//! its own level in a quarter of the seconds after it had converged, where a
//! fixed delay leaked in none. So the delay is measured here as well, by
//! correlating the microphone with the history at 2 kHz four times a second
//! on a thread of its own; the drift between the last few measures says
//! where the echo is now, and the reverse stream is read from the history
//! that far back, [`MARGIN`] ahead of the echo, moving with the drift by
//! being resampled a little faster or slower. The canceller then sees the
//! same short delay all the time and stays converged; until a measure is
//! made, the playback's and the capture's latencies stand in for it.
//! Measured on a drift like a loopback's (a few hundred microseconds a
//! second, changing direction every few seconds), the echo was taken down
//! 59 dB this way against 29 dB with the reverse stream at a fixed place.

use std::{
    collections::VecDeque,
    sync::{
        atomic::{AtomicBool, AtomicI32, AtomicU64, Ordering},
        mpsc, Arc, Mutex,
    },
    thread,
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

/// Samples of the speakers kept: 2.7 s at 48 kHz, a power of two.
const HISTORY: usize = 1 << 17;
/// The measure works at 2 kHz: every 24th sample's place, the average of
/// the 24.
const DECIMATE: usize = 24;
/// How much of the microphone each measure looks at: half a second, long
/// enough for a phrase, short enough not to lag a drift by much.
const WINDOW: usize = 1000;
/// The longest delay looked for: 400 ms.
const LONGEST: usize = 800;
/// Kept of the speakers at 2 kHz.
const HISTORY_SLOW: usize = 1 << 13;
/// How far ahead of its echo the reverse stream is put: 60 ms. Ahead, not
/// behind: a reverse stream that came after its echo (the sign once written
/// the other way round) is cancelled not at all. And well ahead: at 10 ms,
/// through a real loopback, any measure a few milliseconds off put part of
/// the echo before its reference, and the voice leaked back in most
/// seconds; at 60 ms the canceller has the room it has when the reverse
/// stream is fed the moment it is played.
const MARGIN: f64 = 2880.0;
/// Frames between measures: a quarter of a second.
const MEASURE_EVERY: u64 = 25;
/// Measures a drift is worked out from: the last second and a half's.
const KEPT_MEASURES: usize = 6;
/// The most the reverse stream moves in one frame, in samples (2%): a
/// drift is far slower, and anything faster is a measure gone wrong.
const STEP: f64 = 10.0;
/// A measure this far from where the reverse stream is (10 ms) is jumped
/// to at once: the path moved (a new device, a stall, a loopback that
/// dropped or added a chunk to hold its latency), and gliding there at the
/// drift's pace left the canceller a second without its echo.
const JUMP: f64 = 480.0;
/// The correlation a measure needs to be believed.
const BELIEVED: f32 = 0.3;
/// The least the reverse stream is put ahead of its echo (10 ms), and the
/// most (120 ms), whatever else moves it.
const LEAST_MARGIN: f64 = 480.0;
const MOST_MARGIN: f64 = 5760.0;
/// What the reverse stream keeps behind the newest the speakers were
/// given, when its place is first chosen: 10 ms.
const HEADROOM: f64 = 480.0;
/// Frames looked back over (a second) for whether the speakers keep coming
/// short of where the reverse stream is read, and how many of them must
/// for the margin to be brought in.
const SHORT_SPAN: u32 = 100;
const SHORT_TOO_OFTEN: u32 = 30;

/// What the speakers played, mono, as the playback wrote it.
struct History {
    ring: Vec<i16>,
    /// Samples written since the start.
    written: u64,
    slow: Vec<f32>,
}

impl History {
    fn new() -> History {
        History {
            ring: vec![0; HISTORY],
            written: 0,
            slow: vec![0.0; HISTORY_SLOW],
        }
    }

    fn sample(&self, index: u64) -> f32 {
        f32::from(self.ring[(index as usize) & (HISTORY - 1)])
    }

    /// The speakers at `position` (fractional), linearly between samples.
    fn at(&self, position: f64) -> f32 {
        let below = position.floor();
        let part = (position - below) as f32;
        let below = below as u64;
        let a = self.sample(below);
        let b = self.sample(below + 1);
        a + (b - a) * part
    }
}

/// One microphone's processing.
pub struct Processor {
    apm: AudioProcessingModule,
    processing: Processing,
    echo: Arc<Echo>,
    scratch: Vec<i16>,
    align: Align,
}

/// Where the reverse stream is read from, and the measure of where it
/// should be.
struct Align {
    /// Samples of the microphone processed so far.
    captured: u64,
    /// Where the reverse stream is read from: the speakers' sample at
    /// `captured + feed` goes with the microphone's at `captured`.
    feed: Option<f64>,
    /// Where the echo is, from the last measure believed: the offset, with
    /// no margin.
    target: Option<f64>,
    /// How far ahead of its echo the reverse stream is read: [`MARGIN`] at
    /// first, then wherever keeps the reverse stream still (see `believe`).
    margin: f64,
    /// Of the frames since the last look, how many found the speakers short
    /// of where the reverse stream is read, and by how much at most.
    span_frames: u32,
    short_frames: u32,
    short_most: f64,
    /// The measures believed lately: where in the microphone each window's
    /// middle was, and the offset found there. The drift between them is
    /// what moves the reverse stream from one frame to the next, so that it
    /// follows a delay that moves rather than lagging a measure behind it.
    measures: VecDeque<(f64, f64)>,
    /// A measure off the line, waiting for the next to say whether the
    /// path moved.
    suspect: Option<(f64, f64)>,
    /// Measures off the line in a row.
    astray: u32,
    /// The microphone at 2 kHz, the last second of it.
    slow: VecDeque<f32>,
    frames: u64,
    /// The measuring thread, or measures made in place (a test's).
    measurer: Option<Measurer>,
    /// Measures believed, for the log.
    pub believed: u64,
    pub last: Option<Measure>,
}

struct Measurer {
    jobs: mpsc::Sender<Job>,
    measures: mpsc::Receiver<Option<Measure>>,
    /// A job is out.
    out: bool,
}

/// One measure's material: the microphone's last half second and the
/// speakers' last 0.9 s, at 2 kHz, and where each ends.
struct Job {
    microphone: Vec<f32>,
    microphone_end: u64,
    speakers: Vec<f32>,
    speakers_end: u64,
}

/// What a measure found: where the speakers' sample that goes with the
/// microphone's sample `c` is, as `c + offset`, and how sure it is.
#[derive(Debug, Clone, Copy)]
pub struct Measure {
    pub offset: f64,
    /// Where in the microphone the middle of the window measured was.
    pub at: f64,
    pub correlation: f32,
    /// The echo's delay behind the speakers' newest sample, in ms.
    pub delay_ms: f64,
}

impl Processor {
    fn new(processing: Processing, echo: Arc<Echo>, in_place: bool) -> Processor {
        // The high-pass filter goes with any processing, as in a browser,
        // where it follows echo cancellation (Chrome's goog_highpass_filter).
        let apm = AudioProcessingModule::new(
            processing.echo_cancellation,
            processing.gain_control,
            processing.any(),
            processing.noise_suppression,
        );
        let measurer = (processing.echo_cancellation && !in_place).then(start_measurer);
        Processor {
            apm,
            processing,
            echo,
            scratch: vec![0; FRAME],
            align: Align {
                captured: 0,
                feed: None,
                target: None,
                margin: MARGIN,
                span_frames: 0,
                short_frames: 0,
                short_most: 0.0,
                measures: VecDeque::with_capacity(KEPT_MEASURES + 1),
                suspect: None,
                astray: 0,
                slow: VecDeque::with_capacity(WINDOW + 1),
                frames: 0,
                measurer: measurer.flatten(),
                believed: 0,
                last: None,
            },
        }
    }

    /// What it actually does: what was asked for, less the canceller where
    /// a test turned it off (`Echo::disable_canceller`).
    pub fn processing(&self) -> Processing {
        self.processing
    }

    /// The last measure of the echo's delay, and how many were believed.
    pub fn measured(&self) -> (Option<Measure>, u64) {
        (self.align.last, self.align.believed)
    }

    /// Ten milliseconds of the microphone, mono, processed in place;
    /// `delay_ms` is how long ago what the speakers played reached them, by
    /// the two streams' latencies, which stands in until the delay has been
    /// measured.
    pub fn capture(&mut self, frame: &mut [i16], delay_ms: i32) {
        if !self.processing.any() || frame.len() != FRAME {
            return;
        }
        if self.processing.echo_cancellation {
            self.listen(frame);
            self.reverse(delay_ms);
            let _ = self.apm.set_stream_delay_ms((self.align.margin / 48.0) as i32);
        }
        let _ = self.apm.process_stream(frame, RATE as i32, 1);
        self.align.captured += FRAME as u64;
    }

    /// The microphone, before anything is taken out of it, into the measure;
    /// a measure asked for every half second, and the last one's answer
    /// taken up.
    fn listen(&mut self, frame: &[i16]) {
        let align = &mut self.align;
        for block in frame.as_chunks::<DECIMATE>().0 {
            let sum: f32 = block.iter().map(|&s| f32::from(s)).sum();
            align.slow.push_back(sum / DECIMATE as f32);
        }
        while align.slow.len() > WINDOW {
            align.slow.pop_front();
        }
        align.frames += 1;
        let answered = match align.measurer.as_mut() {
            Some(measurer) if measurer.out => match measurer.measures.try_recv() {
                Ok(measure) => {
                    measurer.out = false;
                    Some(measure)
                }
                Err(mpsc::TryRecvError::Empty) => None,
                Err(mpsc::TryRecvError::Disconnected) => {
                    align.measurer = None;
                    None
                }
            },
            _ => None,
        };
        if let Some(measure) = answered.flatten() {
            align.believe(measure);
        }
        if !align.frames.is_multiple_of(MEASURE_EVERY) || align.slow.len() < WINDOW {
            return;
        }
        let job = {
            let history = self.echo.history.lock().unwrap_or_else(|p| p.into_inner());
            let speakers_end = history.written / DECIMATE as u64;
            let wanted = WINDOW + LONGEST;
            if speakers_end < wanted as u64 {
                return;
            }
            let speakers: Vec<f32> = (speakers_end - wanted as u64..speakers_end)
                .map(|index| history.slow[(index as usize) & (HISTORY_SLOW - 1)])
                .collect();
            Job {
                microphone: align.slow.iter().copied().collect(),
                microphone_end: (align.captured + FRAME as u64) / DECIMATE as u64,
                speakers,
                speakers_end,
            }
        };
        match align.measurer.as_mut() {
            Some(measurer) => {
                if !measurer.out && measurer.jobs.send(job).is_ok() {
                    measurer.out = true;
                }
            }
            None => {
                if let Some(measure) = measure(&job) {
                    align.believe(measure);
                }
            }
        }
    }

    /// One frame of the speakers, from the history where the measure says
    /// it will be heard, the margin ahead of that, into the canceller.
    fn reverse(&mut self, delay_ms: i32) {
        let history = self.echo.history.lock().unwrap_or_else(|p| p.into_inner());
        let align = &mut self.align;
        let captured = align.captured as f64;
        let written = history.written as f64;
        if history.written < FRAME as u64 * 2 {
            return;
        }
        // Until a measure: the newest the speakers have, less the two
        // streams' latencies, is what this frame of the microphone hears;
        // the margin no more than those latencies leave behind the newest.
        if align.feed.is_none() {
            let room = f64::from(delay_ms.clamp(0, 500)) * 48.0 - HEADROOM;
            align.margin = MARGIN.min(room).clamp(LEAST_MARGIN, MOST_MARGIN);
        }
        let margin = align.margin;
        let guess = || {
            written - f64::from(delay_ms.clamp(0, 500)) * 48.0 - FRAME as f64 - captured + margin
        };
        let from = align.feed.unwrap_or_else(guess);
        let wanted = align
            .predict(captured + FRAME as f64)
            .or(align.target)
            .map(|offset| offset + margin)
            .unwrap_or(from);
        let mut to = if (wanted - from).abs() > JUMP {
            wanted
        } else {
            from + (wanted - from).clamp(-STEP, STEP)
        };
        let mut from = if (wanted - from).abs() > JUMP {
            to
        } else {
            from
        };
        // Never further back than the history holds.
        let newest = written - 2.0;
        let oldest = written - (HISTORY - 2 * FRAME) as f64;
        if captured + from < oldest {
            from = oldest - captured;
            to = to.max(from);
        }
        // Where the speakers have not been given this much yet, what they
        // have not been given is silence to the canceller, and the reverse
        // stream stays where it is. It was pulled back to what they had
        // instead, and kept there, gliding forward again over the next
        // tenth of a second: the playback writes in bursts and the
        // microphone reads in bursts, and through a 40 ms loopback, with the
        // reverse stream 60 ms ahead of its echo and 23 ms behind the
        // newest, a burst arriving late now and then moved it 2.6 ms and
        // back, and the canceller, which had held the echo, lost it for a
        // second (a voice back at -8 dBFS). Only where the speakers keep
        // coming short (a system whose buffers are smaller than the margin
        // allows for) is the margin brought in, once, by what was missing.
        let short = captured + FRAME as f64 + to - newest;
        align.span_frames += 1;
        if short > 0.0 {
            align.short_frames += 1;
            align.short_most = align.short_most.max(short);
        }
        if align.span_frames >= SHORT_SPAN {
            if align.short_frames >= SHORT_TOO_OFTEN && align.margin > LEAST_MARGIN {
                let fewer = (align.short_most + FRAME as f64).min(align.margin - LEAST_MARGIN);
                align.margin -= fewer;
                align.note(format_args!(
                    "the speakers came short in {} frames of {}: the margin is now {:.1} ms",
                    align.short_frames,
                    align.span_frames,
                    align.margin / 48.0
                ));
            }
            align.span_frames = 0;
            align.short_frames = 0;
            align.short_most = 0.0;
        }
        align.feed = Some(to);
        let start = captured + from;
        let span = FRAME as f64 + (to - from);
        for (index, slot) in self.scratch.iter_mut().enumerate() {
            let position = start + span * index as f64 / FRAME as f64;
            *slot = if position > newest {
                0
            } else {
                history.at(position).round().clamp(-32768.0, 32767.0) as i16
            };
        }
        drop(history);
        let _ = self
            .apm
            .process_reverse_stream(&mut self.scratch, RATE as i32, 1);
    }
}

impl Align {
    /// A line in the debug log, where TOMOSHIBI_DEBUG_ECHO_LOG asks for it.
    fn note(&self, said: std::fmt::Arguments) {
        if cfg!(debug_assertions) && std::env::var_os("TOMOSHIBI_DEBUG_ECHO_LOG").is_some() {
            crate::app::log(&format!("echo: {said}"));
        }
    }

    fn believe(&mut self, measure: Measure) {
        self.last = Some(measure);
        self.note(format_args!(
            "measured {:.2} ms behind at {:.0}, offset {:.1}, correlation {:.2}; feeding at {:?}",
            measure.delay_ms, measure.at, measure.offset, measure.correlation, self.feed
        ));
        if measure.correlation < BELIEVED
            || (cfg!(debug_assertions) && std::env::var_os("TOMOSHIBI_DEBUG_ECHO_FIXED").is_some())
        {
            return;
        }
        self.believed += 1;
        // The first measure puts the reverse stream where the echo is, the
        // margin ahead of it, as far as the speakers' newest allows: on a
        // system whose buffers are short, a margin of 60 ms would be read
        // from samples the speakers have not been given yet.
        if self.measures.is_empty() && self.target.is_none() {
            let room = measure.delay_ms * 48.0 - HEADROOM;
            self.margin = MARGIN.min(room).clamp(LEAST_MARGIN, MOST_MARGIN);
        }
        // A measure far from where the drift says it would be is either the
        // voice fooling the measure (a pitch period off, which through a
        // resampling loopback came every few seconds) or the path having
        // moved (another device, a stall). One alone is taken for the
        // first: followed, it moved the reverse stream 6 ms for a moment and
        // the canceller lost hold. Two in a row that agree are the second,
        // and what came before is forgotten.
        // After a stall of the machine the path can move a second at once,
        // and the measures that follow scatter while it settles: three off
        // the line in a row, and the next one sure of itself is taken,
        // rather than the reverse stream staying where nothing is.
        if let Some(expected) = self.predict(measure.at) {
            if (expected - measure.offset).abs() > 240.0 {
                let moved = match self.suspect.take() {
                    Some((at, offset)) if (offset - measure.offset).abs() < 96.0 => {
                        self.measures.clear();
                        self.measures.push_back((at, offset));
                        true
                    }
                    _ if self.astray >= 3 && measure.correlation >= 0.6 => {
                        self.measures.clear();
                        true
                    }
                    _ => false,
                };
                if !moved {
                    self.suspect = Some((measure.at, measure.offset));
                    self.astray += 1;
                    return;
                }
                self.astray = 0;
                self.moved(measure);
            } else {
                self.suspect = None;
                self.astray = 0;
            }
        }
        self.target = Some(measure.offset);
        self.measures.push_back((measure.at, measure.offset));
        while self.measures.len() > KEPT_MEASURES {
            self.measures.pop_front();
        }
    }

    /// The path moved, by a few milliseconds most often: PulseAudio's
    /// loopback, holding its latency, now and then drops or adds a chunk
    /// and the echo arrives that much sooner or later from then on.
    ///
    /// The reverse stream stays where it is, and what is ahead of the echo
    /// by is what changes, within [`LEAST_MARGIN`] and [`MOST_MARGIN`]. It
    /// was moved to the new place instead, the margin ahead of the echo as
    /// before; but by the time two measures agree on a move (a second) the
    /// canceller has found the echo at its new delay itself, and moving the
    /// reverse stream then moved it a second time: through a 40 ms
    /// loopback the echo came back for a second as the path moved (-16
    /// dBFS) and again, louder, as the reverse stream followed it (-5).
    fn moved(&mut self, measure: Measure) {
        let Some(feed) = self.feed else { return };
        let margin = (feed - measure.offset).clamp(LEAST_MARGIN, MOST_MARGIN);
        self.note(format_args!(
            "the path moved: offset {:.1}, the margin {:.1} ms from {:.1} ms",
            measure.offset,
            margin / 48.0,
            self.margin / 48.0
        ));
        self.margin = margin;
    }

    /// The offset at the microphone's sample `at`: the line through the
    /// measures kept, or the one measure there is.
    fn predict(&self, at: f64) -> Option<f64> {
        let count = self.measures.len();
        if count == 0 {
            return None;
        }
        let (last_at, last) = *self.measures.back()?;
        if count < 3 {
            return Some(last);
        }
        let n = count as f64;
        let mean_at = self.measures.iter().map(|(x, _)| x).sum::<f64>() / n;
        let mean = self.measures.iter().map(|(_, y)| y).sum::<f64>() / n;
        let spread: f64 = self
            .measures
            .iter()
            .map(|(x, _)| (x - mean_at).powi(2))
            .sum();
        if spread <= 0.0 {
            return Some(last);
        }
        let together: f64 = self
            .measures
            .iter()
            .map(|(x, y)| (x - mean_at) * (y - mean))
            .sum();
        // A drift of more than two thousandths is no clock's (two sound
        // cards are a hundred millionths or so apart, a loopback's
        // resampler about as much): held to that.
        let slope = (together / spread).clamp(-0.002, 0.002);
        let _ = last_at;
        Some(mean + slope * (at - mean_at))
    }
}

/// A thread that measures, one job at a time; none where it would not
/// start, and the reverse stream then stays where the latencies put it.
fn start_measurer() -> Option<Measurer> {
    let (jobs, asked) = mpsc::channel::<Job>();
    let (answers, measures) = mpsc::channel();
    thread::Builder::new()
        .name("tomoshibi echo delay".into())
        .spawn(move || {
            while let Ok(job) = asked.recv() {
                if answers.send(measure(&job)).is_err() {
                    return;
                }
            }
        })
        .ok()?;
    Some(Measurer {
        jobs,
        measures,
        out: false,
    })
}

/// Where the microphone's last half second is best matched in the
/// speakers' last 0.9 s: the normalised correlation at every delay up to
/// [`LONGEST`], its peak refined between samples. Nothing where the
/// speakers were silent.
fn measure(job: &Job) -> Option<Measure> {
    let microphone = &job.microphone;
    let speakers = &job.speakers;
    let n = microphone.len();
    let m = speakers.len();
    if n == 0 || m < n {
        return None;
    }
    let longest = m - n;
    let energy_microphone: f32 = microphone.iter().map(|v| v * v).sum();
    if energy_microphone <= 0.0 {
        return None;
    }
    // The speakers' energy under the window at each delay, slid along.
    let mut window: f64 = speakers[m - n..].iter().map(|v| f64::from(v * v)).sum();
    if window < (n as f64) * 4.0 {
        return None;
    }
    let mut best = (0usize, f32::MIN);
    let mut values = vec![0f32; longest + 1];
    for (delay, slot) in values.iter_mut().enumerate() {
        let start = m - n - delay;
        if delay > 0 {
            let entering = f64::from(speakers[start]);
            let leaving = f64::from(speakers[start + n]);
            window += entering * entering - leaving * leaving;
        }
        let dot: f32 = microphone
            .iter()
            .zip(&speakers[start..start + n])
            .map(|(a, b)| a * b)
            .sum();
        let value = dot / ((energy_microphone as f64 * window.max(1.0)).sqrt() as f32);
        *slot = value;
        if value > best.1 {
            best = (delay, value);
        }
    }
    let (peak, correlation) = best;
    // Between samples: a parabola through the peak and its neighbours.
    let refined = if peak > 0 && peak < longest {
        let (a, b, c) = (values[peak - 1], values[peak], values[peak + 1]);
        let bend = a - 2.0 * b + c;
        if bend < 0.0 {
            peak as f64 + f64::from(0.5 * (a - c) / bend)
        } else {
            peak as f64
        }
    } else {
        peak as f64
    };
    let offset_slow = job.speakers_end as f64 - job.microphone_end as f64 - refined;
    Some(Measure {
        offset: offset_slow * DECIMATE as f64,
        at: (job.microphone_end as f64 - n as f64 / 2.0) * DECIMATE as f64,
        correlation,
        delay_ms: refined * DECIMATE as f64 / 48.0,
    })
}

/// What the playback and every microphone share.
pub struct Echo {
    history: Mutex<History>,
    /// The playback stream's latency, in milliseconds.
    render_delay_ms: AtomicI32,
    /// Frames of the speakers written into the history, for the debug log.
    pub rendered: AtomicU64,
    /// `TOMOSHIBI_DEBUG_NO_ECHO_CANCELLER`: processors are made without the
    /// canceller, for a test that shows what it takes out.
    disabled: AtomicBool,
}

impl Default for Echo {
    fn default() -> Self {
        Echo {
            history: Mutex::new(History::new()),
            render_delay_ms: AtomicI32::new(0),
            rendered: AtomicU64::new(0),
            disabled: AtomicBool::new(false),
        }
    }
}

impl Echo {
    pub fn new() -> Arc<Echo> {
        Arc::new(Echo::default())
    }

    /// A processor for a microphone, its reverse stream read from the
    /// playback's history from now on.
    pub fn processor(self: &Arc<Self>, processing: Processing) -> Arc<Mutex<Processor>> {
        Arc::new(Mutex::new(self.processor_here(processing, false)))
    }

    fn processor_here(self: &Arc<Self>, mut processing: Processing, in_place: bool) -> Processor {
        if self.disabled.load(Ordering::Relaxed) {
            processing.echo_cancellation = false;
        }
        Processor::new(processing, self.clone(), in_place)
    }

    pub fn disable_canceller(&self) {
        self.disabled.store(true, Ordering::Relaxed);
    }

    /// Ten milliseconds of what the speakers are about to play, stereo
    /// interleaved, as the playback writes it.
    pub fn render(&self, stereo: &[i16]) {
        let mut history = self.history.lock().unwrap_or_else(|p| p.into_inner());
        let mut sum = 0f32;
        let mut count = 0;
        for pair in stereo.as_chunks::<2>().0 {
            let mono = ((i32::from(pair[0]) + i32::from(pair[1])) / 2) as i16;
            let at = (history.written as usize) & (HISTORY - 1);
            history.ring[at] = mono;
            sum += f32::from(mono);
            count += 1;
            history.written += 1;
            if count == DECIMATE {
                let slow_at =
                    ((history.written / DECIMATE as u64 - 1) as usize) & (HISTORY_SLOW - 1);
                history.slow[slow_at] = sum / DECIMATE as f32;
                sum = 0.0;
                count = 0;
            }
        }
        self.rendered.fetch_add(1, Ordering::Relaxed);
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

    /// Something like a voice (as the echo rig's make_voice.py makes one):
    /// a harmonic series on a gliding fundamental, in syllables of 120 to
    /// 300 ms with phrases and pauses between, which a canceller treats as
    /// it treats speech. White noise, tried first, AEC3 takes for a
    /// stationary sound and hardly cancels at all.
    fn speech(samples: usize, seed: u32) -> Vec<i16> {
        let mut state = seed;
        let mut random = move || {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (state >> 8) as f32 / (1u32 << 24) as f32
        };
        let mut phase = 0f32;
        let mut low = 0f32;
        let mut syllable_left = 0usize;
        let mut syllable_len = 1usize;
        let mut gap_left = 0usize;
        let mut brightness = 0.3f32;
        (0..samples)
            .map(|i| {
                let t = i as f32 / RATE as f32;
                let f0 = 150.0
                    + 40.0 * (2.0 * std::f32::consts::PI * 0.7 * t).sin()
                    + 15.0 * (2.0 * std::f32::consts::PI * 5.3 * t).sin();
                phase += 2.0 * std::f32::consts::PI * f0 / RATE as f32;
                let mut source = 0f32;
                for k in 1..30 {
                    source += (k as f32 * phase).sin() / k as f32;
                }
                low += (source - low) * brightness;
                if syllable_left == 0 && gap_left == 0 {
                    syllable_len = (RATE as f32 * (0.12 + 0.18 * random())) as usize;
                    syllable_left = syllable_len;
                    gap_left = (RATE as f32 * (0.04 + 0.06 * random())) as usize;
                    brightness = 0.1 + 0.4 * random();
                }
                let talking = t % 3.0 < 2.2;
                let envelope = if syllable_left > 0 {
                    syllable_left -= 1;
                    let x = 1.0 - syllable_left as f32 / syllable_len as f32;
                    (std::f32::consts::PI * x).sin().powf(0.6)
                } else {
                    gap_left -= 1;
                    0.0
                };
                let level = if talking { envelope } else { 0.0 };
                (low * level * 6000.0) as i16
            })
            .collect()
    }

    /// The canceller takes out what the speakers played: the microphone
    /// hears the playback (an echo of it, ten milliseconds late) and its
    /// processed output is far quieter than what it heard. Broken (the
    /// reverse stream not fed), the echo comes through: the second half of
    /// this test.
    #[test]
    fn the_canceller_takes_out_what_was_played() {
        let echo = Echo::new();
        let mut processor = echo.processor_here(
            Processing {
                echo_cancellation: true,
                noise_suppression: false,
                gain_control: false,
            },
            true,
        );
        let mut unfed = Processor::new(
            Processing {
                echo_cancellation: true,
                noise_suppression: false,
                gain_control: false,
            },
            Echo::new(),
            true,
        );
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
            processor.capture(&mut mic, 10);
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

    /// The measure finds where the echo is: speech played, heard 63.5 ms
    /// later, is found at 63.5 ms to within half a millisecond.
    #[test]
    fn the_delay_of_the_echo_is_measured() {
        let echo = Echo::new();
        let mut processor = echo.processor_here(
            Processing {
                echo_cancellation: true,
                noise_suppression: false,
                gain_control: false,
            },
            true,
        );
        let delay = 3048; // 63.5 ms
        let played = speech(400 * FRAME, 7);
        for block in 0..300 {
            let speakers = &played[block * FRAME..(block + 1) * FRAME];
            let stereo: Vec<i16> = speakers.iter().flat_map(|&s| [s, s]).collect();
            echo.render(&stereo);
            let mut mic: Vec<i16> = (0..FRAME)
                .map(|i| {
                    let at = (block * FRAME + i) as i64 - delay;
                    if at >= 0 {
                        played[at as usize] / 3
                    } else {
                        0
                    }
                })
                .collect();
            processor.capture(&mut mic, 0);
        }
        let (last, believed) = processor.measured();
        let last = last.expect("measured");
        assert!(believed >= 3, "{believed} measures believed");
        // The speakers' newest frame and the microphone's newest frame were
        // given together, so the echo is 63.5 ms behind the speakers.
        assert!(
            (last.delay_ms - 63.5).abs() < 0.5,
            "measured {:.2} ms (correlation {:.2})",
            last.delay_ms,
            last.correlation
        );
        assert!(last.correlation > 0.9);
        assert!(
            ((-last.offset) - delay as f64).abs() < 12.0,
            "{}",
            last.offset
        );
    }

    /// How far behind the speakers the echo is at second `t`, as through
    /// PulseAudio's loopback: it resamples to hold its latency, at a rate
    /// it changes every few seconds, so the delay drifts one way, then the
    /// other, by a few hundred microseconds a second.
    fn drifting(t: f64) -> f64 {
        let rates = [0.3, -0.4, 0.25, -0.3];
        // Behind the newest the speakers were given: their own buffer
        // (50 ms) and the loopback's 40 ms, as in the rig.
        let mut delay = 95.0;
        let mut at = 0.0;
        for rate in rates {
            let span = (t - at).clamp(0.0, 8.0);
            delay += rate * span;
            at += 8.0;
        }
        delay
    }

    /// The fault this alignment is for: an echo whose delay drifts, as a
    /// resampling loopback's does, a few milliseconds over half a minute.
    /// With the reverse stream following the measure the canceller holds on
    /// and takes the echo out (about 59 dB here); the same canceller fed the
    /// speakers at a fixed place leaves far more of it (about 29 dB).
    /// Broken (the measure ignored), the two are the same and the
    /// comparison fails.
    #[test]
    fn a_wandering_echo_is_followed() {
        let processing = Processing {
            echo_cancellation: true,
            noise_suppression: false,
            gain_control: false,
        };
        let seconds = 30;
        let played = speech(seconds * 100 * FRAME + FRAME, 11);
        let run = |follow: bool| -> (f64, f64) {
            let echo = Echo::new();
            let mut processor = echo.processor_here(processing, true);
            let mut raw_energy = 0.0;
            let mut left_energy = 0.0;
            for block in 0..seconds * 100 {
                let speakers = &played[block * FRAME..(block + 1) * FRAME];
                let stereo: Vec<i16> = speakers.iter().flat_map(|&s| [s, s]).collect();
                echo.render(&stereo);
                let mut mic: Vec<i16> = (0..FRAME)
                    .map(|i| {
                        let n = (block * FRAME + i) as f64;
                        let at = n - drifting(n / 48_000.0) * 48.0;
                        if at < 1.0 {
                            return 0;
                        }
                        let below = at.floor() as usize;
                        let part = at - below as f64;
                        let value = f64::from(played[below]) * (1.0 - part)
                            + f64::from(played[below + 1]) * part;
                        (value * 0.4) as i16
                    })
                    .collect();
                if !follow {
                    processor.align.target = None;
                    processor.align.measures.clear();
                }
                let raw = energy(&mic);
                processor.capture(&mut mic, 50);
                if block >= 1000 {
                    raw_energy += raw;
                    left_energy += energy(&mic);
                }
            }
            (raw_energy, left_energy)
        };
        let (raw, followed) = run(true);
        let (_, fixed) = run(false);
        let followed_db = 10.0 * (raw / followed.max(1.0)).log10();
        let fixed_db = 10.0 * (raw / fixed.max(1.0)).log10();
        assert!(
            followed_db > fixed_db + 10.0 && followed_db > 40.0,
            "followed {followed_db:.1} dB, fixed {fixed_db:.1} dB"
        );
    }

    /// Runs speech through an echo `delay(block)` samples late, the
    /// speakers given their frames as `given(block)` says (how many frames
    /// to write before that block's microphone frame), and hands back where
    /// the reverse stream was read from at every frame, and the margin at
    /// the end.
    fn run_path(
        seconds: usize,
        delay: impl Fn(usize) -> usize,
        given: impl Fn(usize) -> usize,
    ) -> (Vec<Option<f64>>, f64, f64, f64) {
        let echo = Echo::new();
        let mut processor = echo.processor_here(
            Processing {
                echo_cancellation: true,
                noise_suppression: false,
                gain_control: false,
            },
            true,
        );
        let played = speech((seconds * 100 + 16) * FRAME, 5);
        let mut written_frames = 0usize;
        let mut feeds = Vec::new();
        let (mut raw_late, mut left_late) = (0.0, 0.0);
        for block in 0..seconds * 100 {
            for _ in 0..given(block) {
                let speakers = &played[written_frames * FRAME..(written_frames + 1) * FRAME];
                let stereo: Vec<i16> = speakers.iter().flat_map(|&s| [s, s]).collect();
                echo.render(&stereo);
                written_frames += 1;
            }
            let lag = delay(block);
            let mut mic: Vec<i16> = (0..FRAME)
                .map(|i| {
                    let n = block * FRAME + i;
                    if n >= lag {
                        played[n - lag] / 3
                    } else {
                        0
                    }
                })
                .collect();
            let raw = energy(&mic);
            processor.capture(&mut mic, 60);
            if block >= (seconds - 3) * 100 {
                raw_late += raw;
                left_late += energy(&mic);
            }
            feeds.push(processor.align.feed);
        }
        let margin = processor.align.margin;
        (feeds, margin, raw_late, left_late)
    }

    /// The path moving once, by 6.2 ms later, as PulseAudio's loopback
    /// moves it when it drops a chunk to hold its latency: the reverse
    /// stream stays where it was and the margin takes the move, so that the
    /// canceller, which finds the echo at its new delay itself, is not made
    /// to find it a second time. Broken (the reverse stream moved to the
    /// new place, as it was), it moves by the 296 samples.
    #[test]
    fn a_path_that_moves_leaves_the_reverse_stream_where_it_was() {
        let before = 80 * 48;
        let moved_at = 1200;
        let (feeds, margin, raw, left) = run_path(
            20,
            |block| if block < moved_at { before } else { before + 296 },
            |block| if block == 0 { 2 } else { 1 },
        );
        let settled = feeds[moved_at - 10].expect("fed");
        let wandered = feeds[moved_at..]
            .iter()
            .map(|feed| (feed.expect("fed") - settled).abs())
            .fold(0.0, f64::max);
        assert!(
            wandered < 40.0,
            "the reverse stream moved {wandered:.0} samples after the path did"
        );
        assert!(
            (margin - (MARGIN + 296.0)).abs() < 40.0,
            "the margin is {margin:.0}, not the move's {}",
            MARGIN + 296.0
        );
        let taken = 10.0 * (raw / left.max(1.0)).log10();
        assert!(taken > 25.0, "only {taken:.1} dB taken out after the move");
    }

    /// The speakers' frames arriving late now and then (the playback held
    /// up for 30 ms, then writing what it owed at once): the reverse stream
    /// stays where it is, and what it has not been given yet is silence to
    /// the canceller. Broken (pulled back to what the speakers had, as it
    /// was), it moves by what was missing and glides back after.
    #[test]
    fn speakers_late_now_and_then_do_not_move_the_reverse_stream() {
        let held = |block: usize| block % 200 == 150;
        let (feeds, _, raw, left) = run_path(
            12,
            |_| 75 * 48,
            move |block| {
                if block == 0 {
                    2
                } else if held(block) || held(block.wrapping_sub(1)) || held(block.wrapping_sub(2)) {
                    0
                } else if held(block.wrapping_sub(3)) {
                    4
                } else {
                    1
                }
            },
        );
        let settled = feeds[340].expect("fed");
        let wandered = feeds[340..]
            .iter()
            .map(|feed| (feed.expect("fed") - settled).abs())
            .fold(0.0, f64::max);
        assert!(
            wandered < 40.0,
            "the reverse stream moved {wandered:.0} samples as the speakers came late"
        );
        let taken = 10.0 * (raw / left.max(1.0)).log10();
        assert!(taken > 25.0, "only {taken:.1} dB taken out");
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
