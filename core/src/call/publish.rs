//! What this participant sends, published exactly as the web client publishes
//! it, so that every browser in the room treats it the same way.
//!
//! The app captures (AVAudioEngine, AVCaptureSession, ScreenCaptureKit,
//! ReplayKit) and pushes into an input; the input owns the LiveKit track. The
//! numbers come from web/src/live/room.ts and sound.ts: H.264 only with no
//! backup codec, the camera's ladder, the share's rungs and bitrate formula,
//! and an audio profile per kind of sound. A sound is its own track with its
//! own source and name, never mixed into another: a browser turns a voice, a
//! screen's sound and the music up and down separately.

use std::{borrow::Cow, ffi::c_void, sync::Arc};

use livekit::{
    options::{AudioEncoding, TrackPublishOptions, VideoCodec, VideoEncoding, VideoPreset},
    prelude::*,
    webrtc::{
        audio_frame::AudioFrame,
        audio_source::{native::NativeAudioSource, AudioSourceOptions, RtcAudioSource},
        rtp_parameters::DegradationPreference,
        video_frame::{native::NativeBuffer, VideoFrame, VideoRotation},
        video_source::{native::NativeVideoSource, RtcVideoSource, VideoResolution},
    },
};
use tokio::sync::mpsc;

use tomoshibi_protocol::share::{
    camera_publish, share_publish, sound_profile, MediaSource, PublishLayer, ShareCapture, ShareQuality, SoundKind,
    VideoDegradation,
};

use super::{Call, CallError};
use crate::runtime::on_core;

const RATE: u32 = 48_000;
/// LiveKit's native audio source takes exactly 10 ms per frame when it is not
/// buffering, and buffering would add latency nobody asked for.
const FRAME: usize = (RATE / 100) as usize;

/// The kinds of picture a participant sends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum VideoKind {
    Camera,
    /// A shared screen at this frame rate and the share menu's size setting.
    Screen { fps: u32, quality: ShareQuality },
}

fn track_source(source: MediaSource) -> TrackSource {
    match source {
        MediaSource::Camera => TrackSource::Camera,
        MediaSource::Microphone => TrackSource::Microphone,
        MediaSource::ScreenShare => TrackSource::Screenshare,
        MediaSource::ScreenShareAudio => TrackSource::ScreenshareAudio,
        MediaSource::Unknown => TrackSource::Unknown,
    }
}

fn degradation(of: VideoDegradation) -> DegradationPreference {
    match of {
        VideoDegradation::MaintainResolution => DegradationPreference::MaintainResolution,
        VideoDegradation::MaintainFramerate => DegradationPreference::MaintainFramerate,
        VideoDegradation::Balanced => DegradationPreference::Balanced,
    }
}

fn layers(rungs: &[PublishLayer]) -> Vec<VideoPreset> {
    rungs.iter().map(|r| VideoPreset::new(r.width, r.height, r.max_bitrate, r.max_framerate)).collect()
}

/// Every number here is the protocol crate's (share.rs), which follows
/// room.ts and sound.ts and is tested against them; this only says it in the
/// SDK's terms. Simulcast is off when there is nothing beneath the top layer:
/// the Rust SDK unwraps a middle preset that an empty list does not have.
fn audio_options(kind: SoundKind) -> (String, u32, TrackPublishOptions) {
    let profile = sound_profile(kind);
    let options = TrackPublishOptions {
        source: track_source(profile.source),
        audio_encoding: Some(AudioEncoding { max_bitrate: profile.max_bitrate }),
        dtx: profile.dtx,
        red: profile.red,
        ..Default::default()
    };
    (profile.name, if profile.stereo { 2 } else { 1 }, options)
}

fn camera_options(width: u32, height: u32) -> Result<TrackPublishOptions, CallError> {
    let plan = camera_publish(width, height).map_err(|e| CallError::Publish { message: e.to_string() })?;
    Ok(TrackPublishOptions {
        source: TrackSource::Camera,
        video_codec: VideoCodec::H264,
        video_encoding: Some(VideoEncoding { max_bitrate: plan.encoding.max_bitrate, max_framerate: plan.encoding.max_framerate }),
        simulcast: plan.simulcast && !plan.rungs.is_empty(),
        simulcast_layers: Some(layers(&plan.rungs)),
        degradation_preference: Some(degradation(plan.degradation)),
        ..Default::default()
    })
}

fn screen_options(width: u32, height: u32, fps: u32, quality: ShareQuality) -> TrackPublishOptions {
    let captured = ShareCapture { width: Some(width), height: Some(height), frame_rate: Some(fps as f64) };
    let plan = share_publish(fps, quality, captured);
    // Settled before publishing: the web publishes at the setting and lowers
    // the top layer once the capture's real size is known, which an app knows
    // from the start.
    let bitrate = plan.settled_bitrate.unwrap_or(plan.encoding.max_bitrate);
    TrackPublishOptions {
        source: TrackSource::Screenshare,
        video_codec: VideoCodec::H264,
        video_encoding: Some(VideoEncoding { max_bitrate: bitrate, max_framerate: plan.encoding.max_framerate }),
        simulcast: plan.simulcast && !plan.rungs.is_empty(),
        simulcast_layers: Some(layers(&plan.rungs)),
        degradation_preference: Some(degradation(plan.degradation)),
        ..Default::default()
    }
}

/// A sound being sent. The app pushes 48 kHz 16-bit PCM in whatever lengths
/// its capture delivers; the input cuts it into the 10 ms frames the source
/// takes, in order, on the core's runtime.
#[derive(uniffi::Object)]
pub struct AudioInput {
    sid: String,
    channels: u32,
    track: LocalAudioTrack,
    feed: mpsc::UnboundedSender<Vec<i16>>,
    call: Arc<Call>,
}

#[uniffi::export]
impl AudioInput {
    pub fn sid(&self) -> String {
        self.sid.clone()
    }

    /// Interleaved samples at 48 kHz, in the channel count this input was
    /// published with.
    pub fn push(&self, samples: Vec<i16>) {
        let _ = self.feed.send(samples);
    }

    /// Muted at the track: the server and everybody else see it muted, as
    /// they do when a browser turns its microphone off.
    pub fn set_muted(&self, muted: bool) {
        if muted {
            self.track.mute()
        } else {
            self.track.unmute()
        }
    }

    pub fn channels(&self) -> u32 {
        self.channels
    }

    pub async fn unpublish(self: Arc<Self>) -> Result<(), CallError> {
        on_core(async move { unpublish(&self.call, &self.sid).await }).await
    }
}

/// A picture being sent. The app pushes each captured CVPixelBuffer, retained
/// once for the core, which hands it to the hardware encoder without a copy.
#[derive(uniffi::Object)]
pub struct VideoInput {
    sid: String,
    source: NativeVideoSource,
    track: LocalVideoTrack,
    call: Arc<Call>,
}

impl Drop for VideoInput {
    fn drop(&mut self) {
        self.call.strain.stopped(&self.sid);
    }
}

#[uniffi::export]
impl VideoInput {
    pub fn sid(&self) -> String {
        self.sid.clone()
    }

    /// `pixel_buffer` is a CVPixelBufferRef the caller has retained once
    /// (`Unmanaged.passRetained(buffer).toOpaque()`); the core releases it.
    /// `rotation` is clockwise degrees: 0, 90, 180 or 270.
    pub fn push(&self, pixel_buffer: u64, timestamp_us: i64, rotation: u32) {
        if pixel_buffer == 0 {
            return;
        }
        let buffer = unsafe { NativeBuffer::from_cv_pixel_buffer(pixel_buffer as *mut c_void) };
        let rotation = match rotation {
            90 => VideoRotation::VideoRotation90,
            180 => VideoRotation::VideoRotation180,
            270 => VideoRotation::VideoRotation270,
            _ => VideoRotation::VideoRotation0,
        };
        self.source.capture_frame(&VideoFrame { rotation, timestamp_us, frame_metadata: None, buffer });
    }

    pub fn set_muted(&self, muted: bool) {
        if muted {
            self.track.mute()
        } else {
            self.track.unmute()
        }
    }

    pub async fn unpublish(self: Arc<Self>) -> Result<(), CallError> {
        self.call.strain.stopped(&self.sid);
        on_core(async move { unpublish(&self.call, &self.sid).await }).await
    }
}

async fn unpublish(call: &Call, sid: &str) -> Result<(), CallError> {
    call.room
        .local_participant()
        .unpublish_track(&TrackSid::try_from(sid.to_string()).map_err(|_| CallError::NoSuchTrack { sid: sid.into() })?)
        .await
        .map(|_| ())
        .map_err(|e| CallError::Failed { message: e.to_string() })
}

#[uniffi::export]
impl Call {
    /// Starts sending a sound of this kind and returns the input to push it
    /// into.
    pub async fn publish_audio(self: Arc<Self>, kind: SoundKind) -> Result<Arc<AudioInput>, CallError> {
        on_core(async move {
            let (name, channels, options) = audio_options(kind);
            let source = NativeAudioSource::new(AudioSourceOptions::default(), RATE, channels, 0);
            let track = LocalAudioTrack::create_audio_track(&name, RtcAudioSource::Native(source.clone()));
            let publication = self
                .room
                .local_participant()
                .publish_track(LocalTrack::Audio(track.clone()), options)
                .await
                .map_err(|e| CallError::Publish { message: e.to_string() })?;

            let (feed, mut fed) = mpsc::unbounded_channel::<Vec<i16>>();
            tokio::spawn(async move {
                let per = FRAME * channels as usize;
                let mut pending: Vec<i16> = Vec::with_capacity(per * 4);
                while let Some(samples) = fed.recv().await {
                    pending.extend_from_slice(&samples);
                    let mut at = 0;
                    while pending.len() - at >= per {
                        let frame = AudioFrame {
                            data: Cow::Borrowed(&pending[at..at + per]),
                            sample_rate: RATE,
                            num_channels: channels,
                            samples_per_channel: FRAME as u32,
                        };
                        if source.capture_frame(&frame).await.is_err() {
                            return;
                        }
                        at += per;
                    }
                    pending.drain(..at);
                }
            });

            Ok(Arc::new(AudioInput { sid: publication.sid().to_string(), channels, track, feed, call: self.clone() }))
        })
        .await
    }

    /// Starts sending a picture of this kind at this capture size.
    pub async fn publish_video(
        self: Arc<Self>,
        kind: VideoKind,
        width: u32,
        height: u32,
    ) -> Result<Arc<VideoInput>, CallError> {
        on_core(async move {
            let screencast = matches!(kind, VideoKind::Screen { .. });
            let source = NativeVideoSource::new(VideoResolution { width, height }, screencast);
            let track = LocalVideoTrack::create_video_track("", RtcVideoSource::Native(source.clone()));
            let options = match kind {
                VideoKind::Camera => camera_options(width, height)?,
                VideoKind::Screen { fps, quality } => screen_options(width, height, fps, quality),
            };
            let publication = self
                .room
                .local_participant()
                .publish_track(LocalTrack::Video(track.clone()), options)
                .await
                .map_err(|e| CallError::Publish { message: e.to_string() })?;
            let sid = publication.sid().to_string();
            if let VideoKind::Screen { fps, quality } = kind {
                self.strain.started(sid.clone(), track.clone(), quality, fps);
            }
            Ok(Arc::new(VideoInput { sid, source, track, call: self.clone() }))
        })
        .await
    }
}
