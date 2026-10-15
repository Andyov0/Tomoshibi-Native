//! Remote pictures and sounds, handed to whatever the app draws and plays them
//! with.
//!
//! The app attaches a sink to a track by its sid whenever it likes, before or
//! after the track is subscribed; frames flow while both are true and stop
//! when either stops. A picture arrives as the CVPixelBuffer the hardware
//! decoder produced, retained once for the app to take, so nothing is copied
//! on the way to the screen. Sound arrives as 10 ms of 48 kHz PCM.

use std::{collections::HashMap, ffi::c_void, sync::Arc, sync::Mutex};

use futures_util::StreamExt;
use livekit::{
    prelude::*,
    webrtc::{
        audio_stream::native::NativeAudioStream,
        video_frame::{VideoBuffer, VideoRotation},
        video_stream::native::NativeVideoStream,
    },
};
use tokio::task::JoinHandle;

/// One decoded picture.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct RemoteFrame {
    /// A CVPixelBufferRef retained once for the receiver, which must take it
    /// (`Unmanaged.fromOpaque(...).takeRetainedValue()`), or 0 when the
    /// decoder produced something else and `i420` carries the picture.
    pub pixel_buffer: u64,
    pub width: u32,
    pub height: u32,
    /// Clockwise degrees the picture is to be turned when shown.
    pub rotation: u32,
    pub timestamp_us: i64,
    /// Y, then U, then V, each plane packed without padding.
    pub i420: Option<Vec<u8>>,
}

#[uniffi::export(with_foreign)]
pub trait VideoSink: Send + Sync {
    fn on_frame(&self, frame: RemoteFrame);
}

#[uniffi::export(with_foreign)]
pub trait AudioSink: Send + Sync {
    /// Interleaved 16-bit samples.
    fn on_audio(&self, samples: Vec<i16>, sample_rate: u32, channels: u32);
}

extern "C" {
    fn CVPixelBufferRetain(buffer: *mut c_void) -> *mut c_void;
}

const RATE: i32 = 48_000;

enum Wanted {
    Video(Arc<dyn VideoSink>),
    Audio(Arc<dyn AudioSink>, u32),
}

struct Entry {
    wanted: Wanted,
    flowing: Option<JoinHandle<()>>,
}

#[derive(Default)]
pub(super) struct Sinks {
    entries: Mutex<HashMap<String, Entry>>,
}

impl Sinks {
    pub(super) fn attach_video(&self, sid: &str, sink: Arc<dyn VideoSink>, track: Option<RemoteTrack>) {
        self.attach(sid, Wanted::Video(sink), track);
    }

    pub(super) fn attach_audio(&self, sid: &str, sink: Arc<dyn AudioSink>, track: Option<RemoteTrack>, channels: u32) {
        self.attach(sid, Wanted::Audio(sink, channels), track);
    }

    fn attach(&self, sid: &str, wanted: Wanted, track: Option<RemoteTrack>) {
        let mut entries = self.entries.lock().unwrap();
        if let Some(old) = entries.remove(sid) {
            if let Some(task) = old.flowing {
                task.abort();
            }
        }
        let flowing = track.and_then(|track| flow(&wanted, track));
        entries.insert(sid.to_string(), Entry { wanted, flowing });
    }

    /// A track the app may already be waiting for has arrived.
    pub(super) fn subscribed(&self, sid: &str, track: RemoteTrack) {
        let mut entries = self.entries.lock().unwrap();
        if let Some(entry) = entries.get_mut(sid) {
            if let Some(task) = entry.flowing.take() {
                task.abort();
            }
            entry.flowing = flow(&entry.wanted, track);
        }
    }

    /// The track went away but the app still wants it: keep the wish, so a
    /// resubscription (after a reconnect, say) picks it up again.
    pub(super) fn unsubscribed(&self, sid: &str) {
        if let Some(entry) = self.entries.lock().unwrap().get_mut(sid) {
            if let Some(task) = entry.flowing.take() {
                task.abort();
            }
        }
    }

    pub(super) fn detach(&self, sid: &str) {
        if let Some(entry) = self.entries.lock().unwrap().remove(sid) {
            if let Some(task) = entry.flowing {
                task.abort();
            }
        }
    }

    pub(super) fn detach_all(&self) {
        for (_, entry) in self.entries.lock().unwrap().drain() {
            if let Some(task) = entry.flowing {
                task.abort();
            }
        }
    }
}

fn flow(wanted: &Wanted, track: RemoteTrack) -> Option<JoinHandle<()>> {
    match (wanted, track) {
        (Wanted::Video(sink), RemoteTrack::Video(video)) => {
            let sink = sink.clone();
            let mut stream = NativeVideoStream::new(video.rtc_track());
            Some(tokio::spawn(async move {
                while let Some(frame) = stream.next().await {
                    sink.on_frame(picture(&frame.buffer, frame.rotation, frame.timestamp_us));
                }
            }))
        }
        (Wanted::Audio(sink, channels), RemoteTrack::Audio(audio)) => {
            let (sink, channels) = (sink.clone(), *channels);
            let mut stream = NativeAudioStream::new(audio.rtc_track(), RATE, channels as i32);
            Some(tokio::spawn(async move {
                while let Some(frame) = stream.next().await {
                    sink.on_audio(frame.data.to_vec(), frame.sample_rate, frame.num_channels);
                }
            }))
        }
        _ => None,
    }
}

fn picture(buffer: &Box<dyn VideoBuffer>, rotation: VideoRotation, timestamp_us: i64) -> RemoteFrame {
    let rotation = match rotation {
        VideoRotation::VideoRotation0 => 0,
        VideoRotation::VideoRotation90 => 90,
        VideoRotation::VideoRotation180 => 180,
        VideoRotation::VideoRotation270 => 270,
    };
    let (width, height) = (buffer.width(), buffer.height());

    if let Some(native) = buffer.as_native() {
        let pixels = native.get_cv_pixel_buffer();
        if !pixels.is_null() {
            // The decoder keeps its own reference; this one is the app's.
            let retained = unsafe { CVPixelBufferRetain(pixels) };
            return RemoteFrame { pixel_buffer: retained as u64, width, height, rotation, timestamp_us, i420: None };
        }
    }

    let i420 = buffer.to_i420();
    let (sy, su, sv) = i420.strides();
    let (y, u, v) = i420.data();
    let (cw, ch) = ((width + 1) / 2, (height + 1) / 2);
    let mut packed = Vec::with_capacity((width * height + 2 * cw * ch) as usize);
    for row in 0..height {
        packed.extend_from_slice(&y[(row * sy) as usize..(row * sy + width) as usize]);
    }
    for (plane, stride) in [(u, su), (v, sv)] {
        for row in 0..ch {
            packed.extend_from_slice(&plane[(row * stride) as usize..(row * stride + cw) as usize]);
        }
    }
    RemoteFrame { pixel_buffer: 0, width, height, rotation, timestamp_us, i420: Some(packed) }
}
