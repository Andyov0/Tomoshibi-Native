//! A call made from an app process rather than a command line: joins a room,
//! publishes the probe's three tones and a moving picture for a while, and
//! reports what it did. It exists to prove that the bindings, the async
//! runtime and the media engine all work inside a sandboxed app on each
//! platform before anything is built on them, and goes when the real session
//! arrives.

use std::{borrow::Cow, f32::consts::TAU, time::Duration};

use livekit::{
    options::{AudioEncoding, TrackPublishOptions, VideoCodec},
    prelude::*,
    webrtc::{
        audio_frame::AudioFrame,
        audio_source::{native::NativeAudioSource, AudioSourceOptions, RtcAudioSource},
        video_frame::{I420Buffer, VideoFrame, VideoRotation},
        video_source::{native::NativeVideoSource, RtcVideoSource, VideoResolution},
    },
};
use serde::Deserialize;

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum SpikeError {
    #[error("{0}")]
    Failed(String),
}

#[derive(uniffi::Record)]
pub struct SpikeReport {
    pub identity: String,
    pub room: String,
    pub published: u32,
    pub others_seen: u32,
}

#[derive(Deserialize)]
struct Grant {
    url: String,
    token: String,
    identity: String,
    room: String,
}

const RATE: u32 = 48_000;

async fn tone(source: NativeAudioSource, channels: u32, hz: [f32; 2], until: tokio::time::Instant) {
    let per = (RATE / 100) as usize;
    let mut phase = [0f32; 2];
    let mut tick = tokio::time::interval(Duration::from_millis(10));
    while tokio::time::Instant::now() < until {
        tick.tick().await;
        let mut data = Vec::with_capacity(per * channels as usize);
        for _ in 0..per {
            for c in 0..channels as usize {
                phase[c] = (phase[c] + TAU * hz[c] / RATE as f32) % TAU;
                data.push((phase[c].sin() * 6000.0) as i16);
            }
        }
        let frame = AudioFrame { data: Cow::Owned(data), sample_rate: RATE, num_channels: channels, samples_per_channel: per as u32 };
        if source.capture_frame(&frame).await.is_err() {
            return;
        }
    }
}

/// Joins `room` on the deployment at `base_url` and stays for `seconds`.
#[uniffi::export]
pub async fn spike_join(base_url: String, room: String, name: String, seconds: u32) -> Result<SpikeReport, SpikeError> {
    crate::runtime::on_core(join(base_url, room, name, seconds)).await
}

async fn join(base_url: String, room: String, name: String, seconds: u32) -> Result<SpikeReport, SpikeError> {
    let fail = |e: String| SpikeError::Failed(e);
    let grant: Grant = reqwest::Client::new()
        .post(format!("{base_url}/api/rooms/{room}/join"))
        .json(&serde_json::json!({"name": name, "passphrase": "", "relay": ""}))
        .send()
        .await
        .map_err(|e| fail(format!("join: {e}")))?
        .json()
        .await
        .map_err(|e| fail(format!("join answer: {e}")))?;

    let mut options = RoomOptions::default();
    options.adaptive_stream = true;
    options.dynacast = true;
    let (session, mut events) =
        Room::connect(&grant.url, &grant.token, options).await.map_err(|e| fail(format!("connect: {e}")))?;

    let until = tokio::time::Instant::now() + Duration::from_secs(seconds as u64);
    let mut published = 0;
    for (name, channels, hz, source, bitrate, red) in [
        ("", 1, [440.0, 440.0], TrackSource::Microphone, 64_000, true),
        ("", 2, [660.0, 880.0], TrackSource::ScreenshareAudio, 160_000, true),
        ("music", 2, [330.0, 495.0], TrackSource::Unknown, 510_000, false),
    ] {
        let native = NativeAudioSource::new(AudioSourceOptions::default(), RATE, channels, 0);
        let track = LocalAudioTrack::create_audio_track(name, RtcAudioSource::Native(native.clone()));
        let options = TrackPublishOptions {
            source,
            audio_encoding: Some(AudioEncoding { max_bitrate: bitrate }),
            dtx: false,
            red,
            ..Default::default()
        };
        if session.local_participant().publish_track(LocalTrack::Audio(track), options).await.is_ok() {
            published += 1;
        }
        tokio::spawn(tone(native, channels, hz, until));
    }

    let camera = NativeVideoSource::new(VideoResolution { width: 640, height: 360 }, false);
    let track = LocalVideoTrack::create_video_track("", RtcVideoSource::Native(camera.clone()));
    let options = TrackPublishOptions { source: TrackSource::Camera, video_codec: VideoCodec::H264, ..Default::default() };
    if session.local_participant().publish_track(LocalTrack::Video(track), options).await.is_ok() {
        published += 1;
    }
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_millis(33));
        let mut n: u32 = 0;
        while tokio::time::Instant::now() < until {
            tick.tick().await;
            n = n.wrapping_add(1);
            let mut buffer = I420Buffer::new(640, 360);
            let (sy, _, _) = buffer.strides();
            let (y, u, v) = buffer.data_mut();
            for row in 0..360u32 {
                for col in 0..640u32 {
                    y[(row * sy + col) as usize] = ((col + row + n * 4) % 256) as u8;
                }
            }
            u.fill(90);
            v.fill(200);
            camera.capture_frame(&VideoFrame { rotation: VideoRotation::VideoRotation0, timestamp_us: 0, frame_metadata: None, buffer });
        }
    });

    let mut others = std::collections::HashSet::new();
    loop {
        tokio::select! {
            _ = tokio::time::sleep_until(until) => break,
            Some(event) = events.recv() => if let RoomEvent::ParticipantConnected(p) = event {
                others.insert(p.identity().to_string());
            }
        }
    }
    for p in session.remote_participants().keys() {
        others.insert(p.to_string());
    }
    let _ = session.close().await;
    Ok(SpikeReport { identity: grant.identity, room: grant.room, published, others_seen: others.len() as u32 })
}
