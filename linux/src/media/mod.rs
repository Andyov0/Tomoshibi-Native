//! The devices: the boundary between the room (ui/room) and whatever opens a
//! microphone, a speaker, a camera or a screen on this machine.
//!
//! The core never opens a device on any platform (core-map: WebRTC runs in
//! synthetic mode, `enable_platform_adm` is never called). The app captures
//! and pushes into the inputs the core hands it (`AudioInput::push`,
//! `VideoInput::push_planes` / `push_bgra`), and plays and draws what it is
//! given through sinks (`Call::attach_pcm`, `attach_video`). Everything here
//! is that work for Linux: PulseAudio (which PipeWire also serves through
//! pipewire-pulse), the camera through the xdg-desktop-portal and PipeWire
//! with V4L2 behind it, the screen through the portal's ScreenCast.
//!
//! The room uses these traits and nothing below them, so that it can be
//! written and screenshotted against [`Quiet`] (no devices at all) while the
//! media module writes the real ones, and so that a test can hand the room a
//! fake. This file is the contract; the media module owns audio.rs,
//! camera.rs and screen.rs, which implement it, and changes here are agreed
//! with the room module.
//!
//! Threads. Every method is called on GTK's main thread and every callback
//! given here is called there. Capture runs on threads of its own and pushes
//! into the core from them, which the core allows.

pub mod audio;
pub mod camera;
pub mod screen;

use std::{future::Future, pin::Pin, rc::Rc, sync::Arc};

use tomoshibi_core::call::{Call, VideoSink};
use tomoshibi_protocol::share::{ShareQuality, SoundCapture};

/// A future the media module gives back, polled on GTK's main context
/// (`glib::spawn_future_local`); it may hold widgets and `Rc`s.
pub type Local<T> = Pin<Box<dyn Future<Output = T>>>;

/// What kind of device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DeviceKind {
    Microphone,
    Speaker,
    Camera,
}

/// A device somebody can choose, as the devices panel and the settings list
/// them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Device {
    /// Stable across runs where the system allows (a PulseAudio source or
    /// sink name, a PipeWire node's object.serial or a V4L2 path), so that
    /// a choice kept in the preferences finds the same device next time.
    pub id: String,
    /// What the system calls it, shown as it is.
    pub label: String,
    pub kind: DeviceKind,
    /// The system's default of its kind, which is what "no choice" means.
    pub is_default: bool,
}

/// Why a device could not be opened, for the room to say.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MediaError {
    /// This machine or build has no way to do it (no PipeWire, no portal).
    Unsupported,
    /// The person, or the portal on their behalf, said no.
    Denied,
    /// Nothing of the kind is there, or the chosen one has gone.
    NotFound,
    /// Something else holds it.
    Busy,
    /// It was opened and failed; the detail is for the log.
    Failed(String),
    /// The core refused the track (`CallError`), as its sentence for the log.
    Call(String),
}

impl std::fmt::Display for MediaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MediaError::Unsupported => f.write_str("not supported here"),
            MediaError::Denied => f.write_str("denied"),
            MediaError::NotFound => f.write_str("not found"),
            MediaError::Busy => f.write_str("in use"),
            MediaError::Failed(detail) => write!(f, "failed: {detail}"),
            MediaError::Call(detail) => write!(f, "the call refused it: {detail}"),
        }
    }
}

/// The voice captured from the microphone, before and during a call: the
/// pre-join's level meter needs it before there is any call to send it to.
pub trait Microphone {
    /// The level now, from nought to one, for a meter; read on a timer.
    fn level(&self) -> f32;
    /// Starts sending it into `call` as the voice (`SoundKind::Voice`),
    /// with the capture the web would ask for (echo cancellation, noise
    /// suppression, gain), or stops and starts again with new ones.
    fn publish(&self, call: Arc<Call>, capture: SoundCapture) -> Local<Result<(), MediaError>>;
    /// Stops sending, and keeps capturing for the meter.
    fn unpublish(&self) -> Local<()>;
    /// Mutes the track the others hear (the call's microphone button).
    fn set_muted(&self, muted: bool);
    /// Lets go of the device.
    fn close(&self);
}

/// The camera, before and during a call.
pub trait Camera {
    /// Draws what the camera sees into `sink` too (a [`crate::render::VideoView`]'s
    /// sink, for the self view and the pre-join's preview); returns a number
    /// to remove it with.
    fn add_preview(&self, sink: Arc<dyn VideoSink>) -> u64;
    fn remove_preview(&self, id: u64);
    /// Starts sending it into `call` as the camera (`VideoKind::Camera`).
    fn publish(&self, call: Arc<Call>) -> Local<Result<(), MediaError>>;
    fn unpublish(&self) -> Local<()>;
    fn set_muted(&self, muted: bool);
    /// Background blur (MediaPipe's selfie model, as on Windows), where the
    /// deployment offers it.
    fn set_blur(&self, on: bool);
    fn close(&self);
}

/// What a shared screen is sent as: the share menu's choices.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShareChoice {
    pub fps: u32,
    pub quality: ShareQuality,
    /// The screen's sound too (`SoundKind::ScreenSound`), the default output's
    /// monitor without this app's own playback where PipeWire allows.
    pub with_sound: bool,
}

/// A screen being shared.
pub trait ScreenShare {
    /// Stops sharing and closes the portal's stream.
    fn stop(&self) -> Local<()>;
    /// Called once on the main thread when the share ends by itself: the
    /// person stopped it from the system's own indicator, or the stream
    /// failed.
    fn on_ended(&self, ended: Box<dyn Fn()>);
}

/// Every device the app opens. One per app; the room and the settings
/// borrow it.
pub trait Media {
    /// The devices of a kind, the system's default among them.
    fn devices(&self, kind: DeviceKind) -> Vec<Device>;
    /// Called on the main thread whenever a device comes or goes.
    fn on_devices_changed(&self, changed: Box<dyn Fn()>);

    /// Opens the microphone (`None`: the system's default).
    fn open_microphone(
        &self,
        choice: Option<String>,
    ) -> Local<Result<Rc<dyn Microphone>, MediaError>>;
    /// Opens the camera (`None`: the first one).
    fn open_camera(&self, choice: Option<String>) -> Local<Result<Rc<dyn Camera>, MediaError>>;
    /// Asks the system (the portal's picker) for a screen or window and
    /// starts sending it into `call`.
    fn share_screen(
        &self,
        call: Arc<Call>,
        choice: ShareChoice,
    ) -> Local<Result<Rc<dyn ScreenShare>, MediaError>>;
    /// Whether sharing is possible here at all, for the share button.
    fn can_share_screen(&self) -> bool;

    /// Plays a remote sound track (`call.attach_pcm` into the mixer), as the
    /// participant `identity`'s voice, screen sound or music.
    fn play(&self, call: &Arc<Call>, track_sid: &str, identity: &str);
    /// Stops playing it (`call.detach`).
    fn stop_playing(&self, call: &Arc<Call>, track_sid: &str);
    /// Where everything is played (`None`: the system's default).
    fn set_speaker(&self, choice: Option<String>);
    /// One person's volume, from nought to one (the sound panel's sliders).
    fn set_volume(&self, identity: &str, level: f32);
    /// Silences everything played (`TOMOSHIBI_MUTE=speakers`, deafen).
    fn set_silent(&self, silent: bool);
}

/// No devices at all: what the room is written and screenshotted against
/// until the real ones exist, and what a test hands it. Lists nothing, opens
/// nothing, plays nothing.
#[derive(Default)]
pub struct Quiet;

impl Media for Quiet {
    fn devices(&self, _: DeviceKind) -> Vec<Device> {
        Vec::new()
    }
    fn on_devices_changed(&self, _: Box<dyn Fn()>) {}
    fn open_microphone(&self, _: Option<String>) -> Local<Result<Rc<dyn Microphone>, MediaError>> {
        Box::pin(async { Err(MediaError::Unsupported) })
    }
    fn open_camera(&self, _: Option<String>) -> Local<Result<Rc<dyn Camera>, MediaError>> {
        Box::pin(async { Err(MediaError::Unsupported) })
    }
    fn share_screen(
        &self,
        _: Arc<Call>,
        _: ShareChoice,
    ) -> Local<Result<Rc<dyn ScreenShare>, MediaError>> {
        Box::pin(async { Err(MediaError::Unsupported) })
    }
    fn can_share_screen(&self) -> bool {
        false
    }
    fn play(&self, _: &Arc<Call>, _: &str, _: &str) {}
    fn stop_playing(&self, _: &Arc<Call>, _: &str) {}
    fn set_speaker(&self, _: Option<String>) {}
    fn set_volume(&self, _: &str, _: f32) {}
    fn set_silent(&self, _: bool) {}
}

/// The media this machine has: the real devices once the media module has
/// written them; [`Quiet`] until then.
pub fn standard() -> Rc<dyn Media> {
    Rc::new(Quiet)
}
