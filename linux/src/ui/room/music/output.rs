//! The song desk's music going out (sound.ts `Output`; the Mac's
//! MusicOutput): one published track kept across songs, fed in real time
//! ten milliseconds at a time by a clock of its own, the lossless stream
//! beside it where one is offered, and the same samples into this app's own
//! mix, so that whoever plays the music hears it too (the web's `monitor`).
//!
//! Something always goes out, if only zeros: paused, between one song and
//! the next, or past the end of a song while the desk fetches the next. The
//! web keeps a constant source connected for the same reason, having
//! measured that a track with nothing playing into it stops being delivered
//! at all, and that a lossless stream with nothing in it for four seconds is
//! given up on by every listener for the rest of the track.
//!
//! The lossless stream is the file at its own rate: a song at another rate
//! starts a new stream on the same track. Its frames are counted against the
//! 10 ms blocks of Opus sent meanwhile, so a rate like 44.1 kHz, which does
//! not divide into 10 ms, stays at its rate without drifting.

use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread,
    time::{Duration, Instant},
};

use tomoshibi_core::call::{AudioInput, PcmSink};

use super::loader::DecodedSong;
use crate::app::log;

/// 10 ms at 48 kHz.
const FRAME: usize = 480;

struct Playing {
    song: Option<Arc<DecodedSong>>,
    key: Option<String>,
    /// Where the song is, in 48 kHz frames for Opus; the lossless stream's
    /// place in the file is its own, kept apart so neither repeats or skips
    /// a sample to round.
    position: usize,
    original_position: usize,
    paused: bool,
    told: bool,
    lossless_rate: u32,
    lossless_sent: i64,
    blocks_at_rate: i64,
}

/// The music's track and its clock.
pub struct Output {
    pub input: Arc<AudioInput>,
    /// Whether the track was published with a lossless stream beside it.
    pub lossless: bool,
    playing: Arc<Mutex<Playing>>,
    running: Arc<AtomicBool>,
}

impl Output {
    /// Starts the clock; `ended` is told (on the clock's thread) the key of
    /// a song that played to its end; `monitor` is this app's own mix.
    pub fn start(
        input: Arc<AudioInput>,
        lossless: bool,
        monitor: Option<Arc<dyn PcmSink>>,
        ended: impl Fn(String) + Send + 'static,
    ) -> Output {
        let playing = Arc::new(Mutex::new(Playing {
            song: None,
            key: None,
            position: 0,
            original_position: 0,
            paused: false,
            told: false,
            lossless_rate: 0,
            lossless_sent: 0,
            blocks_at_rate: 0,
        }));
        let running = Arc::new(AtomicBool::new(true));
        let (state, alive, sending) = (playing.clone(), running.clone(), input.clone());
        let spawned = thread::Builder::new()
            .name("tomoshibi music".into())
            .spawn(move || {
                let started = Instant::now();
                let mut blocks: u64 = 0;
                let mut block = vec![0i16; FRAME * 2];
                while alive.load(Ordering::Relaxed) {
                    // Every block that is due, then a sleep to the next.
                    let due = started.elapsed().as_millis() as u64 / 10 + 1;
                    if due > blocks + 50 {
                        // Far behind (the machine slept): start the clock
                        // again rather than send a burst.
                        blocks = due - 1;
                    }
                    while blocks < due {
                        let (original, finished) = fill(&state, lossless, &mut block);
                        sending.push(block.clone());
                        if let Some(monitor) = monitor.as_ref() {
                            let mut bytes = Vec::with_capacity(block.len() * 2);
                            for sample in &block {
                                bytes.extend_from_slice(&sample.to_le_bytes());
                            }
                            monitor.on_pcm(bytes, 48_000, 2);
                        }
                        if let Some((samples, rate, at)) = original {
                            sending.clone().push_lossless(samples, 2, rate, at);
                        }
                        if let Some(key) = finished {
                            ended(key);
                        }
                        blocks += 1;
                    }
                    let next = Duration::from_millis(blocks * 10);
                    let now = started.elapsed();
                    if next > now {
                        thread::sleep(next - now);
                    }
                }
            });
        if let Err(fault) = spawned {
            log(&format!("music: the clock did not start: {fault}"));
            running.store(false, Ordering::Relaxed);
        }
        Output {
            input,
            lossless,
            playing,
            running,
        }
    }

    /// Plays `song` from its start, in place of whatever was playing, and
    /// unpaused: the next song plays unpaused (jukebox.ts `Desk.pause`).
    pub fn play(&self, song: Arc<DecodedSong>, key: String) {
        let mut playing = self.playing.lock().unwrap_or_else(|p| p.into_inner());
        if self.lossless && song.original.is_some() && song.rate != playing.lossless_rate {
            playing.lossless_rate = song.rate;
            playing.lossless_sent = 0;
            playing.blocks_at_rate = 0;
        }
        playing.song = Some(song);
        playing.key = Some(key);
        playing.position = 0;
        playing.original_position = 0;
        playing.paused = false;
        playing.told = false;
    }

    pub fn pause(&self, paused: bool) {
        self.playing
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .paused = paused;
    }

    /// Where the song is and how long it is, in seconds.
    pub fn progress(&self) -> Option<(f64, f64)> {
        let playing = self.playing.lock().unwrap_or_else(|p| p.into_inner());
        let song = playing.song.as_ref()?;
        Some((
            playing.position as f64 / 48_000.0,
            song.frames() as f64 / 48_000.0,
        ))
    }

    /// Ends the clock; the caller takes the track down.
    pub fn stop(&self) {
        self.running.store(false, Ordering::Relaxed);
    }
}

impl Drop for Output {
    fn drop(&mut self) {
        self.stop();
    }
}

/// The lossless stream's share of one block: samples, rate, and where they
/// start on the stream's clock in microseconds.
type Original = (Vec<f32>, u32, i64);

/// The next 10 ms of both streams: the song where it is playing, silence
/// anywhere else; and the key of a song that has just run out.
fn fill(
    state: &Mutex<Playing>,
    lossless: bool,
    block: &mut [i16],
) -> (Option<Original>, Option<String>) {
    let mut playing = state.lock().unwrap_or_else(|p| p.into_inner());
    let now_playing = playing.song.is_some() && !playing.paused;
    let original = next_original(&mut playing, lossless, now_playing);
    block.iter_mut().for_each(|sample| *sample = 0);
    let Some(song) = playing.song.clone().filter(|_| now_playing) else {
        return (original, None);
    };
    let frames = song.frames();
    if playing.position >= frames {
        return (original, None);
    }
    let count = FRAME.min(frames - playing.position);
    let start = playing.position * 2;
    block[..count * 2].copy_from_slice(&song.samples[start..start + count * 2]);
    playing.position += count;
    if playing.position >= frames && !playing.told {
        playing.told = true;
        return (original, playing.key.clone());
    }
    (original, None)
}

fn next_original(playing: &mut Playing, lossless: bool, now_playing: bool) -> Option<Original> {
    if !lossless || playing.lossless_rate == 0 {
        return None;
    }
    let rate = playing.lossless_rate;
    playing.blocks_at_rate += 1;
    let due = playing.blocks_at_rate * i64::from(rate) / 100;
    let count = (due - playing.lossless_sent).max(0) as usize;
    if count == 0 {
        return None;
    }
    let mut samples = vec![0f32; count * 2];
    if now_playing {
        if let Some(song) = playing.song.as_ref().filter(|song| song.rate == rate) {
            if let Some(file) = song.original.as_ref() {
                let from = playing.original_position * 2;
                let available = file.len().saturating_sub(from).min(count * 2);
                samples[..available].copy_from_slice(&file[from..from + available]);
                playing.original_position += available / 2;
            }
        }
    }
    let at = playing.lossless_sent * 1_000_000 / i64::from(rate);
    playing.lossless_sent = due;
    Some((samples, rate, at))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn song(frames: usize, rate: u32) -> Arc<DecodedSong> {
        Arc::new(DecodedSong {
            samples: vec![1000; frames * 2],
            original: Some(vec![
                0.25;
                (frames as u64 * u64::from(rate) / 48_000) as usize * 2
            ]),
            rate,
            quality: None,
        })
    }

    fn playing() -> Mutex<Playing> {
        Mutex::new(Playing {
            song: None,
            key: None,
            position: 0,
            original_position: 0,
            paused: false,
            told: false,
            lossless_rate: 0,
            lossless_sent: 0,
            blocks_at_rate: 0,
        })
    }

    /// A song plays to its end and says so once, then silence follows; the
    /// lossless stream at 44.1 kHz carries 441 frames a block, never more
    /// or fewer over a second.
    #[test]
    fn a_song_plays_out_and_the_lossless_stream_keeps_its_rate() {
        let state = playing();
        {
            let mut playing = state.lock().unwrap();
            playing.song = Some(song(48_000, 44_100));
            playing.key = Some("k1".into());
            playing.lossless_rate = 44_100;
        }
        let mut block = vec![0i16; FRAME * 2];
        let mut ended = Vec::new();
        let mut lossless_frames = 0;
        for _ in 0..150 {
            let (original, finished) = fill(&state, true, &mut block);
            if let Some((samples, rate, _)) = original {
                assert_eq!(rate, 44_100);
                lossless_frames += samples.len() / 2;
            }
            ended.extend(finished);
        }
        assert_eq!(ended, vec!["k1".to_owned()], "said once");
        assert_eq!(lossless_frames, 66_150, "a second and a half at 44.1 kHz");
        assert!(block.iter().all(|s| *s == 0), "silence after the end");
    }

    #[test]
    fn paused_it_sends_silence_and_goes_on_from_where_it_was() {
        let state = playing();
        state.lock().unwrap().song = Some(song(48_000, 48_000));
        let mut block = vec![0i16; FRAME * 2];
        fill(&state, false, &mut block);
        assert_eq!(block[0], 1000);
        state.lock().unwrap().paused = true;
        fill(&state, false, &mut block);
        assert!(block.iter().all(|s| *s == 0));
        assert_eq!(state.lock().unwrap().position, FRAME);
        state.lock().unwrap().paused = false;
        fill(&state, false, &mut block);
        assert_eq!(state.lock().unwrap().position, FRAME * 2);
    }
}
