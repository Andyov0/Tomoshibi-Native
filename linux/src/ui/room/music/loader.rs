//! A song from the library, fetched and decoded (music.ts `load`; the Mac's
//! MusicLoader): what the track is (`Deployment::music_track`, which also
//! says when the library's account may not play it), the file itself from
//! the account-gated audio address with the session's cookie, and every
//! sample of it decoded, on a thread of its own.
//!
//! What comes out is the song at 48 kHz stereo for the Opus track, and,
//! where a lossless stream will carry it, the file's own samples at its own
//! rate as well, as the web sends them (`playDecoded` with the original).

use std::{io::Cursor, path::PathBuf, sync::Arc, thread};

use libwebrtc::native::audio_resampler::AudioResampler;
use symphonia::core::{
    audio::SampleBuffer,
    codecs::{DecoderOptions, CODEC_TYPE_NULL},
    errors::Error as Fault,
    formats::FormatOptions,
    io::MediaSourceStream,
    meta::MetadataOptions,
    probe::Hint,
};
use tokio::sync::oneshot;
use tomoshibi_protocol::api::{describe_music_audio, Deployment, MusicQuality};

/// The rate the Opus track is fed at.
pub const RATE: u32 = 48_000;

/// A song ready to play.
pub struct DecodedSong {
    /// Interleaved stereo at 48 kHz.
    pub samples: Vec<i16>,
    /// The file's own samples, interleaved stereo, at `rate`, for the
    /// lossless stream; none where none will carry it.
    pub original: Option<Vec<f32>>,
    pub rate: u32,
    /// "FLAC · 24-bit · 44.1 kHz", as the desk says it.
    pub quality: Option<String>,
}

impl DecodedSong {
    /// Frames at 48 kHz.
    pub fn frames(&self) -> usize {
        self.samples.len() / 2
    }
}

/// What came of a load.
pub enum Loaded {
    Ready(Arc<DecodedSong>),
    /// The library's account may not play it.
    Unavailable,
}

/// Describes, fetches and decodes a song of the deployment's library.
pub async fn load(
    deployment: Arc<Deployment>,
    source: String,
    id: String,
    quality: MusicQuality,
    keep_original: bool,
) -> Result<Loaded, String> {
    let audio = deployment
        .clone()
        .music_track(source.clone(), id.clone(), quality)
        .await
        .map_err(|fault| fault.to_string())?;
    let Some(audio) = audio else {
        return Ok(Loaded::Unavailable);
    };
    let url = deployment.music_audio_url(source, id, quality);
    let fetching = deployment.clone();
    let bytes = tomoshibi_protocol::runtime::on_core(async move {
        let answer = fetching
            .http()
            .get(url)
            .send()
            .await
            .map_err(|fault| fault.to_string())?;
        if !answer.status().is_success() {
            return Err(format!("the library answered {}", answer.status()));
        }
        answer
            .bytes()
            .await
            .map(|bytes| bytes.to_vec())
            .map_err(|fault| fault.to_string())
    })
    .await?;
    let format = audio.format.clone();
    let mut song = decode_off_thread(bytes, format, keep_original).await?;
    song.quality = Some(describe_music_audio(audio));
    Ok(Loaded::Ready(Arc::new(song)))
}

/// A file from a directory standing in for the library
/// (TOMOSHIBI_DEBUG_MUSIC_FILES, debug builds), decoded as a fetched track
/// is.
pub async fn load_file(path: PathBuf, keep_original: bool) -> Result<Loaded, String> {
    let said = path
        .extension()
        .map(|extension| extension.to_string_lossy().into_owned())
        .unwrap_or_default();
    let bytes =
        gio::spawn_blocking(move || std::fs::read(&path).map_err(|fault| fault.to_string()))
            .await
            .map_err(|_| "the file's reader stopped".to_owned())??;
    let mut song = decode_off_thread(bytes, said.clone(), keep_original).await?;
    song.quality = Some(format!(
        "{} · {} kHz",
        said.to_uppercase(),
        song.rate as f64 / 1000.0
    ));
    Ok(Loaded::Ready(Arc::new(song)))
}

async fn decode_off_thread(
    bytes: Vec<u8>,
    format: String,
    keep_original: bool,
) -> Result<DecodedSong, String> {
    let (told, answer) = oneshot::channel();
    thread::Builder::new()
        .name("tomoshibi music decoder".into())
        .spawn(move || {
            let _ = told.send(decode(bytes, &format, keep_original));
        })
        .map_err(|fault| fault.to_string())?;
    answer.await.map_err(|_| "the decoder stopped".to_owned())?
}

/// Every sample of a file, whatever it is by its first bytes (the format the
/// library said is a hint, not a promise), as stereo.
pub fn decode(bytes: Vec<u8>, said: &str, keep_original: bool) -> Result<DecodedSong, String> {
    let stream = MediaSourceStream::new(Box::new(Cursor::new(bytes)), Default::default());
    let mut hint = Hint::new();
    if !said.is_empty() {
        hint.with_extension(said);
    }
    let probed = symphonia::default::get_probe()
        .format(
            &hint,
            stream,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .map_err(|fault| format!("not a file this can read: {fault}"))?;
    let mut reader = probed.format;
    let track = reader
        .tracks()
        .iter()
        .find(|track| track.codec_params.codec != CODEC_TYPE_NULL)
        .ok_or("no sound in the file")?;
    let track_id = track.id;
    let mut rate = track.codec_params.sample_rate.unwrap_or(0);
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(|fault| format!("no decoder: {fault}"))?;
    let mut stereo: Vec<f32> = Vec::new();
    let mut buffer: Option<SampleBuffer<f32>> = None;
    loop {
        let packet = match reader.next_packet() {
            Ok(packet) => packet,
            Err(Fault::IoError(fault)) if fault.kind() == std::io::ErrorKind::UnexpectedEof => {
                break
            }
            Err(Fault::ResetRequired) => break,
            Err(fault) => return Err(format!("the file stopped reading: {fault}")),
        };
        if packet.track_id() != track_id {
            continue;
        }
        let decoded = match decoder.decode(&packet) {
            Ok(decoded) => decoded,
            // A damaged frame is skipped, as a player skips it.
            Err(Fault::DecodeError(_)) => continue,
            Err(fault) => return Err(format!("the file did not decode: {fault}")),
        };
        let spec = *decoded.spec();
        rate = spec.rate;
        let channels = spec.channels.count().max(1);
        let wanted = decoded.capacity() as u64;
        if buffer
            .as_ref()
            .is_none_or(|buffer| buffer.capacity() < decoded.capacity() * channels)
        {
            buffer = Some(SampleBuffer::new(wanted, spec));
        }
        let Some(buffer) = buffer.as_mut() else {
            continue;
        };
        buffer.copy_interleaved_ref(decoded);
        for frame in buffer.samples().chunks_exact(channels) {
            let (left, right) = if channels == 1 {
                (frame[0], frame[0])
            } else {
                (frame[0], frame[1])
            };
            stereo.push(left);
            stereo.push(right);
        }
    }
    if stereo.is_empty() || rate == 0 {
        return Err("no samples in the file".into());
    }
    let samples = to_48k(&stereo, rate);
    Ok(DecodedSong {
        samples,
        original: keep_original.then_some(stereo),
        rate,
        quality: None,
    })
}

fn to_i16(sample: f32) -> i16 {
    (sample.clamp(-1.0, 1.0) * 32767.0).round() as i16
}

/// Stereo floats at `rate` to 16-bit stereo at 48 kHz: WebRTC's resampler
/// where the rate divides into ten milliseconds (every rate a library
/// serves), between neighbouring samples otherwise.
pub fn to_48k(stereo: &[f32], rate: u32) -> Vec<i16> {
    if rate == RATE {
        return stereo.iter().map(|s| to_i16(*s)).collect();
    }
    if rate.is_multiple_of(100) && (8_000..=384_000).contains(&rate) {
        let pcm: Vec<i16> = stereo.iter().map(|s| to_i16(*s)).collect();
        let per = (rate / 100) as usize;
        let mut resampler = AudioResampler::default();
        let mut out = Vec::with_capacity(stereo.len() * RATE as usize / rate as usize + 960);
        for block in pcm.chunks(per * 2) {
            let mut padded;
            let block = if block.len() < per * 2 {
                padded = block.to_vec();
                padded.resize(per * 2, 0);
                &padded[..]
            } else {
                block
            };
            out.extend_from_slice(
                resampler.remix_and_resample(block, per as u32, 2, rate, 2, RATE),
            );
        }
        return out;
    }
    let frames = stereo.len() / 2;
    let out_frames = (frames as u64 * u64::from(RATE) / u64::from(rate)) as usize;
    let mut out = Vec::with_capacity(out_frames * 2);
    for index in 0..out_frames {
        let position = index as f64 * f64::from(rate) / f64::from(RATE);
        let below = position.floor() as usize;
        let above = (below + 1).min(frames - 1);
        let part = (position - below as f64) as f32;
        for channel in 0..2 {
            let a = stereo[below * 2 + channel];
            let b = stereo[above * 2 + channel];
            out.push(to_i16(a + (b - a) * part));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A WAV file of a tone, as a test stands a song in with.
    fn wav(rate: u32, channels: u16, seconds: f32, hz: f32) -> Vec<u8> {
        let frames = (rate as f32 * seconds) as u32;
        let mut data = Vec::new();
        for n in 0..frames {
            let value = (0.5
                * (2.0 * std::f32::consts::PI * hz * n as f32 / rate as f32).sin()
                * 32767.0) as i16;
            for _ in 0..channels {
                data.extend_from_slice(&value.to_le_bytes());
            }
        }
        let mut file = Vec::new();
        file.extend_from_slice(b"RIFF");
        file.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
        file.extend_from_slice(b"WAVEfmt ");
        file.extend_from_slice(&16u32.to_le_bytes());
        file.extend_from_slice(&1u16.to_le_bytes());
        file.extend_from_slice(&channels.to_le_bytes());
        file.extend_from_slice(&rate.to_le_bytes());
        file.extend_from_slice(&(rate * channels as u32 * 2).to_le_bytes());
        file.extend_from_slice(&(channels * 2).to_le_bytes());
        file.extend_from_slice(&16u16.to_le_bytes());
        file.extend_from_slice(b"data");
        file.extend_from_slice(&(data.len() as u32).to_le_bytes());
        file.extend_from_slice(&data);
        file
    }

    #[test]
    fn a_file_at_another_rate_comes_out_at_48k_stereo_with_its_original() {
        let song = decode(wav(44_100, 1, 1.0, 1000.0), "", true).unwrap();
        assert_eq!(song.rate, 44_100);
        assert!(
            (song.frames() as i64 - 48_000).abs() < 600,
            "{} frames",
            song.frames()
        );
        let original = song.original.as_ref().unwrap();
        assert_eq!(
            original.len(),
            44_100 * 2,
            "the file's own samples, made stereo"
        );
        // Half scale, still, after resampling.
        let middle = &song.samples[20_000..20_960];
        let peak = middle.iter().map(|s| i32::from(*s).abs()).max().unwrap();
        assert!((peak - 16_383).abs() < 600, "peak {peak}");
        // Both channels carry the mono file.
        assert_eq!(song.samples[20_000], song.samples[20_001]);
    }

    #[test]
    fn a_file_kept_without_its_original_and_a_broken_one_refused() {
        let song = decode(wav(48_000, 2, 0.5, 440.0), "wav", false).unwrap();
        assert!(song.original.is_none());
        assert_eq!(song.frames(), 24_000);
        assert!(decode(b"not sound at all".to_vec(), "", false).is_err());
    }
}
