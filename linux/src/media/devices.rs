//! Which [`Media`] the app runs with: the real devices of this machine,
//! put together from audio.rs, camera.rs and screen.rs.
//!
//! Owned by the media module, like those three. The sound comes from
//! [`Sound`] (PulseAudio, through pipewire-pulse where PipeWire runs), the
//! camera from [`Cameras`] (the camera portal and PipeWire, V4L2 where there
//! is no portal), the screen from [`Screens`] (the portal's ScreenCast and
//! PipeWire).
//!
//! A debug run given `TOMOSHIBI_SOURCE` (app::Launch) sends a test's sound
//! and picture in place of the microphone and the camera, so that a test
//! never captures a room; given `TOMOSHIBI_MUTE` it plays nothing
//! (`speakers`: silence to the real speakers), as on Windows; given
//! `TOMOSHIBI_SCREEN=synthetic` it shares a generated screen and chord
//! rather than asking the portal for one.

use std::{cell::RefCell, rc::Rc, sync::Arc};

use tomoshibi_core::call::Call;

use super::{
    audio::{Sound, SoundOptions, Speakers, TestSound},
    camera::{CameraOptions, Cameras, TestPicture},
    screen::{Screens, Source},
    Camera, Device, DeviceKind, Local, Media, MediaError, Microphone, ScreenShare, ShareChoice,
};
use crate::app::{log, Launch};

/// The media this machine has.
pub fn standard() -> Rc<dyn Media> {
    let launch = Launch::from_environment();
    let (sound, camera) = test_sources(launch.source.as_deref());
    let output = match launch.mute.as_deref() {
        Some("speakers") => Speakers::Silent,
        Some(_) => Speakers::Nowhere,
        None => Speakers::Speakers,
    };
    let no_echo_canceller =
        cfg!(debug_assertions) && std::env::var_os("TOMOSHIBI_DEBUG_NO_ECHO_CANCELLER").is_some();
    if sound.is_some() || camera.is_some() {
        log(&format!(
            "media: test sources in place of the devices: sound {sound:?}, picture {camera:?}"
        ));
    }
    Machine::open(
        SoundOptions {
            output,
            test_sound: sound,
            no_echo_canceller,
        },
        CameraOptions {
            test_picture: camera,
        },
        Source::from_environment(),
    )
}

/// `TOMOSHIBI_SOURCE`, a comma-separated list of what to send in place of
/// the devices: a `.wav` file or `tone[:<hz>]` for the microphone, a `.y4m`
/// file or `pattern[:i420|nv12|yuyv]` for the camera, or `synthetic` for a
/// tone and a pattern.
pub fn test_sources(given: Option<&str>) -> (Option<TestSound>, Option<TestPicture>) {
    let (mut sound, mut picture) = (None, None);
    for item in given
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|i| !i.is_empty())
    {
        let lower = item.to_ascii_lowercase();
        if lower == "synthetic" {
            sound = Some(TestSound::Tone(440.0));
            picture = Some(TestPicture::Pattern(super::camera::PixelFormat::Nv12));
        } else if lower.ends_with(".wav") {
            sound = Some(TestSound::Wav(item.into()));
        } else if lower.ends_with(".y4m") {
            picture = Some(TestPicture::Y4m(item.into()));
        } else if let Some(rest) = lower.strip_prefix("tone") {
            let hz = rest
                .strip_prefix(':')
                .and_then(|hz| hz.parse().ok())
                .unwrap_or(440.0);
            sound = Some(TestSound::Tone(hz));
        } else if let Some(rest) = lower.strip_prefix("pattern") {
            let format = match rest.strip_prefix(':') {
                Some("i420") => super::camera::PixelFormat::I420,
                Some("yuyv") => super::camera::PixelFormat::Yuyv,
                _ => super::camera::PixelFormat::Nv12,
            };
            picture = Some(TestPicture::Pattern(format));
        } else {
            log(&format!(
                "media: TOMOSHIBI_SOURCE names something unknown: {item}"
            ));
        }
    }
    (sound, picture)
}

/// Whoever asked to hear of devices coming and going.
type Listeners = Rc<RefCell<Vec<Box<dyn Fn()>>>>;

/// The sound, the camera and the screen together.
struct Machine {
    sound: Rc<Sound>,
    cameras: Rc<Cameras>,
    screens: Screens,
    listeners: Listeners,
}

impl Machine {
    /// Called on GTK's main thread once the application has started: the
    /// screen asks the portal in the background whether it can share.
    fn open(sound: SoundOptions, camera: CameraOptions, screen: Source) -> Rc<dyn Media> {
        let listeners: Listeners = Rc::new(RefCell::new(Vec::new()));
        let changed: Rc<dyn Fn()> = {
            let listeners = Rc::downgrade(&listeners);
            Rc::new(move || {
                if let Some(listeners) = listeners.upgrade() {
                    for listener in listeners.borrow().iter() {
                        listener();
                    }
                }
            })
        };
        Rc::new(Machine {
            sound: Sound::new(sound, changed.clone()),
            cameras: Cameras::new(camera, changed),
            screens: Screens::new(screen),
            listeners,
        })
    }
}

impl Media for Machine {
    fn devices(&self, kind: DeviceKind) -> Vec<Device> {
        match kind {
            DeviceKind::Camera => self.cameras.devices(),
            _ => self.sound.devices(kind),
        }
    }

    fn on_devices_changed(&self, changed: Box<dyn Fn()>) {
        self.listeners.borrow_mut().push(changed);
    }

    fn open_microphone(
        &self,
        choice: Option<String>,
    ) -> Local<Result<Rc<dyn Microphone>, MediaError>> {
        self.sound.open_microphone(choice)
    }

    fn open_camera(&self, choice: Option<String>) -> Local<Result<Rc<dyn Camera>, MediaError>> {
        self.cameras.open_camera(choice)
    }

    fn share_screen(
        &self,
        call: Arc<Call>,
        choice: ShareChoice,
    ) -> Local<Result<Rc<dyn ScreenShare>, MediaError>> {
        self.screens.share(call, choice)
    }

    fn can_share_screen(&self) -> bool {
        self.screens.can_share()
    }

    fn attach_call(&self, call: &Arc<Call>) {
        self.sound.attach_call(call);
    }

    fn detach_call(&self) {
        self.sound.detach_call();
    }

    fn play(&self, call: &Arc<Call>, track_sid: &str, identity: &str) {
        self.sound.play(call, track_sid, identity);
    }

    fn stop_playing(&self, call: &Arc<Call>, track_sid: &str) {
        self.sound.stop_playing(call, track_sid);
    }

    fn set_speaker(&self, choice: Option<String>) {
        self.sound.set_speaker(choice);
    }

    fn set_volume(&self, identity: &str, level: f32) {
        self.sound.set_volume(identity, level);
    }

    fn set_silent(&self, silent: bool) {
        self.sound.set_silent(silent);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::media::camera::PixelFormat;

    #[test]
    fn test_sources_are_read_from_the_switch() {
        assert_eq!(test_sources(None), (None, None));
        assert_eq!(
            test_sources(Some("tone:660")),
            (Some(TestSound::Tone(660.0)), None)
        );
        assert_eq!(
            test_sources(Some(" /t/voice.wav , pattern:yuyv ")),
            (
                Some(TestSound::Wav("/t/voice.wav".into())),
                Some(TestPicture::Pattern(PixelFormat::Yuyv))
            )
        );
        assert_eq!(
            test_sources(Some("synthetic")),
            (
                Some(TestSound::Tone(440.0)),
                Some(TestPicture::Pattern(PixelFormat::Nv12))
            )
        );
        assert_eq!(
            test_sources(Some("/t/clip.Y4M")).1,
            Some(TestPicture::Y4m("/t/clip.Y4M".into()))
        );
    }
}
