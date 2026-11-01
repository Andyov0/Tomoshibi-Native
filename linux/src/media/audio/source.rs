//! What a test sends in place of the microphone (`TOMOSHIBI_SOURCE`, debug
//! builds only): a WAV file, looped, or a steady tone. Both come out as the
//! microphone would, 48 kHz mono ten milliseconds at a time, and go through
//! the same processing and the same input, so that a test of the app's
//! sending is a test of what a person's microphone takes, not of a side
//! road. A tone is a frequency a browser's spectrum finds at once
//! (interop/channels.js), and nobody's voice.

use std::{f32::consts::PI, path::Path};

use libwebrtc::native::audio_resampler::AudioResampler;

use super::mixer::{FRAME, RATE};

/// A generated or loaded sound, looped.
pub enum Sound {
    Tone {
        hz: f32,
        phase: f32,
    },
    /// 48 kHz mono, played from `at` and round again.
    Samples {
        samples: Vec<i16>,
        at: usize,
    },
}

impl Sound {
    /// A sine at `hz`, a quarter of full scale.
    pub fn tone(hz: f32) -> Sound {
        Sound::Tone { hz, phase: 0.0 }
    }

    /// The next ten milliseconds.
    pub fn next(&mut self, frame: &mut [i16]) {
        match self {
            Sound::Tone { hz, phase } => {
                let step = 2.0 * PI * *hz / RATE as f32;
                for sample in frame.iter_mut() {
                    *sample = (phase.sin() * 8192.0) as i16;
                    *phase += step;
                    if *phase > 2.0 * PI {
                        *phase -= 2.0 * PI;
                    }
                }
            }
            Sound::Samples { samples, at } => {
                for sample in frame.iter_mut() {
                    *sample = samples[*at];
                    *at = (*at + 1) % samples.len();
                }
            }
        }
    }

    /// A WAV file as 48 kHz mono: 16-bit integer or 32-bit float PCM, any
    /// rate and any number of channels (the channels averaged).
    pub fn wav(path: &Path) -> Result<Sound, String> {
        let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let (rate, mono) = read_wav(&bytes)?;
        let samples = to_rate(&mono, rate);
        if samples.is_empty() {
            return Err(format!("{}: no samples", path.display()));
        }
        Ok(Sound::Samples { samples, at: 0 })
    }
}

/// The rate and the samples, mixed to one channel, of a WAV file's bytes.
pub fn read_wav(bytes: &[u8]) -> Result<(u32, Vec<i16>), String> {
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err("not a WAV file".into());
    }
    let u16_at = |at: usize| u16::from_le_bytes([bytes[at], bytes[at + 1]]);
    let u32_at =
        |at: usize| u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]]);
    let mut at = 12;
    let mut format: Option<(u16, u16, u32, u16)> = None;
    while at + 8 <= bytes.len() {
        let id = &bytes[at..at + 4];
        let size = u32_at(at + 4) as usize;
        let body = at + 8;
        let end = body.saturating_add(size).min(bytes.len());
        match id {
            b"fmt " if size >= 16 => {
                let mut tag = u16_at(body);
                // WAVE_FORMAT_EXTENSIBLE names the real format in its GUID's
                // first two bytes.
                if tag == 0xFFFE && size >= 26 {
                    tag = u16_at(body + 24);
                }
                format = Some((tag, u16_at(body + 2), u32_at(body + 4), u16_at(body + 14)));
            }
            b"data" => {
                let (tag, channels, rate, bits) = format.ok_or("data before fmt")?;
                let channels = channels.max(1) as usize;
                let data = &bytes[body..end];
                let frames: Vec<f32> = match (tag, bits) {
                    (1, 16) => data
                        .as_chunks::<2>()
                        .0
                        .iter()
                        .map(|b| f32::from(i16::from_le_bytes(*b)) / 32768.0)
                        .collect(),
                    (3, 32) => data
                        .as_chunks::<4>()
                        .0
                        .iter()
                        .map(|b| f32::from_le_bytes(*b))
                        .collect(),
                    _ => return Err(format!("unsupported WAV format {tag} at {bits} bits")),
                };
                let mono = frames
                    .chunks_exact(channels)
                    .map(|frame| {
                        let mean = frame.iter().sum::<f32>() / channels as f32;
                        (mean.clamp(-1.0, 1.0) * 32767.0).round() as i16
                    })
                    .collect();
                return Ok((rate, mono));
            }
            _ => {}
        }
        // Chunks are padded to an even length.
        at = body + size + (size & 1);
    }
    Err("no data in the WAV file".into())
}

/// Mono samples at `rate`, taken to 48 kHz by WebRTC's resampler, ten
/// milliseconds at a time (a last part short of ten is left out).
fn to_rate(samples: &[i16], rate: u32) -> Vec<i16> {
    if rate == RATE {
        return samples.to_vec();
    }
    if !(8_000..=384_000).contains(&rate) {
        return Vec::new();
    }
    let per = (rate / 100) as usize;
    let mut resampler = AudioResampler::default();
    let mut out = Vec::with_capacity(samples.len() / per * FRAME);
    for block in samples.chunks_exact(per) {
        out.extend_from_slice(resampler.remix_and_resample(block, per as u32, 1, rate, 1, RATE));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wav(rate: u32, channels: u16, samples: &[i16]) -> Vec<u8> {
        let data: Vec<u8> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();
        let mut out = Vec::new();
        out.extend_from_slice(b"RIFF");
        out.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
        out.extend_from_slice(b"WAVEfmt ");
        out.extend_from_slice(&16u32.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&channels.to_le_bytes());
        out.extend_from_slice(&rate.to_le_bytes());
        out.extend_from_slice(&(rate * u32::from(channels) * 2).to_le_bytes());
        out.extend_from_slice(&(channels * 2).to_le_bytes());
        out.extend_from_slice(&16u16.to_le_bytes());
        // A chunk the reader must step over, of odd length.
        out.extend_from_slice(b"LIST");
        out.extend_from_slice(&3u32.to_le_bytes());
        out.extend_from_slice(&[1, 2, 3, 0]);
        out.extend_from_slice(b"data");
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(&data);
        out
    }

    #[test]
    fn a_stereo_wav_is_read_as_one_channel() {
        let bytes = wav(48_000, 2, &[1000, 3000, -2000, -4000]);
        let (rate, mono) = read_wav(&bytes).unwrap();
        assert_eq!(rate, 48_000);
        assert_eq!(mono.len(), 2);
        assert!(
            (mono[0] - 2000).abs() <= 1 && (mono[1] + 3000).abs() <= 1,
            "{mono:?}"
        );
    }

    #[test]
    fn a_wav_at_another_rate_comes_out_at_48k() {
        let samples: Vec<i16> = (0..16_000)
            .map(|i| ((i as f32 / 16.0).sin() * 8000.0) as i16)
            .collect();
        let (rate, mono) = read_wav(&wav(16_000, 1, &samples)).unwrap();
        let out = to_rate(&mono, rate);
        assert_eq!(out.len(), 48_000);
    }

    #[test]
    fn not_a_wav_is_refused() {
        assert!(read_wav(b"RIFF\0\0\0\0AVI ").is_err());
        assert!(read_wav(b"nothing").is_err());
    }

    #[test]
    fn the_tone_is_the_frequency_it_says() {
        let mut tone = Sound::tone(440.0);
        let mut second = vec![0i16; RATE as usize];
        for frame in second.chunks_mut(FRAME) {
            tone.next(frame);
        }
        // Crossings upward in one second: one per cycle.
        let ups = second.windows(2).filter(|w| w[0] < 0 && w[1] >= 0).count();
        assert!((439..=441).contains(&ups), "{ups} cycles");
    }
}
