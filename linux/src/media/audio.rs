//! The microphone and the speakers, through PulseAudio, which PipeWire
//! serves as well (pipewire-pulse), so one client covers both kinds of
//! system.
//!
//! What this does (core-map section 2):
//! - captures the microphone at 48 kHz mono, ten milliseconds at a time,
//!   and pushes it into the voice's `AudioInput` (capture.rs);
//! - runs it through WebRTC's audio processing first, as a browser's
//!   getUserMedia does, with the speakers' mix as the echo canceller's
//!   reverse stream (echo.rs): the core's audio device is synthetic, so
//!   nothing of WebRTC's own capture path runs, and Linux has no system
//!   canceller to lean on;
//! - plays every remote sound the room hands to [`Sound::play`] through one
//!   mixer fed by `Call::attach_pcm` (10 ms at 48 kHz, one channel for a
//!   voice and two for screen sound and music) and by the lossless side
//!   (`Call::set_lossless_sink`), to the speakers chosen (mixer.rs,
//!   playback.rs);
//! - lists the sources and sinks, the system's defaults among them, and
//!   says when they change (pulse.rs).
//!
//! [`Sound`] is the audio half of `Media`; devices.rs puts it together with
//! the camera's. Everything here is called on GTK's main thread; the
//! threads below it (the device watch, the microphone, the speakers) touch
//! nothing of GTK's and reach the main thread through a channel.

mod capture;
mod echo;
pub mod mixer;
mod playback;
mod pulse;
pub mod source;

use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    path::PathBuf,
    rc::Rc,
    sync::{atomic::Ordering, Arc, Mutex},
};

use tokio::sync::{mpsc, oneshot};
use tomoshibi_core::call::{AudioInput, Call, LosslessSink, MediaSource, PcmSink};
use tomoshibi_protocol::share::{sound_profile, SoundKind};

use self::{
    capture::From,
    echo::{Echo, Processing},
    mixer::{Kind, Mixer},
    playback::Output,
};
use super::{Device, DeviceKind, Local, MediaError, Microphone};
use crate::app::log;

pub use playback::Output as Speakers;

/// What a test sends in place of the microphone (`TOMOSHIBI_SOURCE`).
#[derive(Debug, Clone, PartialEq)]
pub enum TestSound {
    Wav(PathBuf),
    Tone(f32),
}

/// How the sound side is set up: by devices.rs, from the launch.
#[derive(Debug, Clone)]
pub struct SoundOptions {
    /// Where the mix goes; `Nowhere` and `Silent` in a muted launch.
    pub output: Output,
    /// A test's sound in place of every microphone.
    pub test_sound: Option<TestSound>,
    /// `TOMOSHIBI_DEBUG_NO_ECHO_CANCELLER`: for a test that shows what the
    /// canceller takes out.
    pub no_echo_canceller: bool,
}

impl Default for SoundOptions {
    fn default() -> Self {
        SoundOptions {
            output: Output::Speakers,
            test_sound: None,
            no_echo_canceller: false,
        }
    }
}

/// The audio half of the app's `Media`.
pub struct Sound {
    options: SoundOptions,
    lists: Arc<Mutex<pulse::Lists>>,
    echo: Arc<Echo>,
    mixer: Arc<Mixer>,
    /// The speakers chosen (`None`: the system's), for a playback started
    /// later.
    speaker: RefCell<Option<String>>,
    playback: RefCell<Option<Arc<playback::Shared>>>,
    call: RefCell<Option<Arc<Call>>>,
    /// The tracks being played, by SID, with whose they are.
    playing: RefCell<HashMap<String, String>>,
    reading: Cell<bool>,
}

impl Sound {
    /// Starts watching the system's devices; `changed` is called on the main
    /// thread whenever they change.
    pub fn new(options: SoundOptions, changed: Rc<dyn Fn()>) -> Rc<Sound> {
        let lists = Arc::new(Mutex::new(pulse::Lists::default()));
        let (told, mut tells) = mpsc::unbounded_channel::<()>();
        pulse::watch(lists.clone(), move || {
            let _ = told.send(());
        });
        glib::spawn_future_local(async move {
            while tells.recv().await.is_some() {
                // A burst of changes (a headset is a sink and a source at
                // once) is said once.
                while tells.try_recv().is_ok() {}
                changed();
            }
        });
        let echo = Echo::new();
        if options.no_echo_canceller {
            echo.disable_canceller();
            log(
                "sound: the echo canceller is off for this run (TOMOSHIBI_DEBUG_NO_ECHO_CANCELLER)",
            );
        }
        Rc::new(Sound {
            options,
            lists,
            echo,
            mixer: Arc::new(Mixer::new()),
            speaker: RefCell::new(None),
            playback: RefCell::new(None),
            call: RefCell::new(None),
            playing: RefCell::new(HashMap::new()),
            reading: Cell::new(false),
        })
    }

    /// The microphones or the speakers, the system's default first.
    pub fn devices(&self, kind: DeviceKind) -> Vec<Device> {
        if kind == DeviceKind::Microphone {
            if let Some(test) = &self.options.test_sound {
                let label = match test {
                    TestSound::Wav(path) => format!("Test sound ({})", path.display()),
                    TestSound::Tone(hz) => format!("Test tone ({hz} Hz)"),
                };
                return vec![Device {
                    id: "test".into(),
                    label,
                    kind,
                    is_default: true,
                }];
            }
        }
        let lists = self.lists.lock().unwrap_or_else(|p| p.into_inner()).clone();
        let (endpoints, default) = match kind {
            DeviceKind::Microphone => (lists.sources, lists.default_source),
            DeviceKind::Speaker => (lists.sinks, lists.default_sink),
            DeviceKind::Camera => return Vec::new(),
        };
        let mut devices: Vec<Device> = endpoints
            .into_iter()
            .map(|endpoint| Device {
                is_default: default.as_deref() == Some(endpoint.name.as_str()),
                id: endpoint.name,
                label: endpoint.description,
                kind,
            })
            .collect();
        devices.sort_by_key(|device| !device.is_default);
        devices
    }

    /// Opens the microphone `choice` (`None`: the system's default) and
    /// starts capturing it, for the meter until it is published.
    pub fn open_microphone(
        &self,
        choice: Option<String>,
    ) -> Local<Result<Rc<dyn Microphone>, MediaError>> {
        let default_voice =
            Processing::of(&sound_profile(SoundKind::Voice { original: false }).capture);
        let (from, processing) = match &self.options.test_sound {
            Some(TestSound::Tone(hz)) => (From::Test(source::Sound::tone(*hz)), Processing::NONE),
            Some(TestSound::Wav(path)) => match source::Sound::wav(path) {
                Ok(sound) => (From::Test(sound), default_voice),
                Err(fault) => return Box::pin(async move { Err(MediaError::Failed(fault)) }),
            },
            None => (
                From::Device(choice.filter(|c| !c.is_empty())),
                default_voice,
            ),
        };
        // A generated tone goes out as it was made: a test reads its
        // frequency and level in the browser, and the noise suppressor
        // takes a steady tone for noise.
        let raw = matches!(self.options.test_sound, Some(TestSound::Tone(_)));
        let test = self.options.test_sound.is_some();
        let shared = capture::Shared::new(processing, false);
        let (opened, open) = oneshot::channel();
        let echo = self.echo.clone();
        capture::start(shared.clone(), echo.clone(), from, opened);
        Box::pin(async move {
            match open.await {
                Ok(Ok(())) => Ok(Rc::new(Mic {
                    shared: Rc::new(RefCell::new(shared)),
                    echo,
                    test,
                    raw,
                    published: Rc::new(RefCell::new(None)),
                    busy: Rc::new(Cell::new(false)),
                }) as Rc<dyn Microphone>),
                Ok(Err(fault)) => Err(fault),
                Err(_) => Err(MediaError::Failed("the microphone's thread ended".into())),
            }
        })
    }

    /// The call's own: the lossless streams into the mixer, and the speakers
    /// opened.
    pub fn attach_call(&self, call: &Arc<Call>) {
        call.set_lossless_sink(Arc::new(LosslessToMixer(self.mixer.clone())));
        self.call.replace(Some(call.clone()));
        self.start_playback();
        self.start_reading();
    }

    /// The call is over: everything it played stops, and the speakers are let
    /// go.
    pub fn detach_call(&self) {
        let call = self.call.borrow_mut().take();
        let playing: Vec<String> = self
            .playing
            .borrow_mut()
            .drain()
            .map(|(sid, _)| sid)
            .collect();
        if let Some(call) = call {
            for sid in &playing {
                call.detach(sid.clone());
            }
        }
        self.mixer.clear();
        if let Some(playback) = self.playback.borrow_mut().take() {
            playback.stop.store(true, Ordering::Relaxed);
        }
    }

    /// Plays a remote sound track, `identity`'s: a voice, a screen's sound
    /// or music, as its publication says, for the listener's level of each.
    pub fn play(&self, call: &Arc<Call>, track_sid: &str, identity: &str) {
        let kind = call
            .participants()
            .iter()
            .flat_map(|person| person.tracks.iter())
            .find(|track| track.sid == track_sid)
            .map(|track| kind_of(track.source, &track.name))
            .unwrap_or(Kind::Voice);
        self.mixer.add_kind(track_sid, identity, kind);
        self.playing
            .borrow_mut()
            .insert(track_sid.to_owned(), identity.to_owned());
        call.attach_pcm(
            track_sid.to_owned(),
            Arc::new(PcmToMixer {
                mixer: self.mixer.clone(),
                sid: track_sid.to_owned(),
            }),
        );
        self.start_playback();
    }

    pub fn stop_playing(&self, call: &Arc<Call>, track_sid: &str) {
        call.detach(track_sid.to_owned());
        self.mixer.remove(track_sid);
        self.playing.borrow_mut().remove(track_sid);
    }

    /// Where everything is played (`None`: the system's speakers).
    pub fn set_speaker(&self, choice: Option<String>) {
        let choice = choice.filter(|c| !c.is_empty());
        self.speaker.replace(choice.clone());
        if let Some(playback) = self.playback.borrow().as_ref() {
            playback.set_speaker(choice);
        }
    }

    pub fn set_volume(&self, identity: &str, level: f32) {
        self.mixer.set_level(identity, level);
    }

    pub fn set_kind_volume(&self, identity: &str, music: bool, level: f32) {
        self.mixer.set_kind_level(identity, music, level);
    }

    pub fn set_duck(&self, identity: &str, factor: f32) {
        self.mixer.set_duck(identity, factor);
    }

    pub fn voice_levels(&self) -> Vec<(String, f32)> {
        self.mixer.voice_levels()
    }

    /// A sound this app plays itself, into the same mix as the call (the
    /// song desk's music heard by whoever plays it; a video watched
    /// together): so that the echo canceller hears it as it hears the call,
    /// and the speakers chosen and the silence of a muted run apply to it.
    /// Filed under the identity `local:<name>`, for its level; music when
    /// `name` is "music". It plays until the sink is let go of.
    pub fn local_sound(&self, name: &str) -> Arc<dyn PcmSink> {
        let key = format!("local/{name}");
        let kind = if name == "music" {
            Kind::Music
        } else {
            Kind::Screen
        };
        self.mixer.add_kind(&key, &format!("local:{name}"), kind);
        self.start_playback();
        Arc::new(LocalToMixer {
            mixer: self.mixer.clone(),
            key,
        })
    }

    pub fn set_silent(&self, silent: bool) {
        self.mixer.set_silent(silent);
    }

    fn start_playback(&self) {
        if self.playback.borrow().is_some() {
            return;
        }
        let shared = playback::Shared::new(self.speaker.borrow().clone());
        playback::start(
            shared.clone(),
            self.mixer.clone(),
            self.echo.clone(),
            self.options.output,
        );
        self.playback.replace(Some(shared));
    }

    /// In a debug build, every five seconds of a call: what each sound
    /// received and played, for the tests that read the log.
    fn start_reading(&self) {
        if !cfg!(debug_assertions) || self.reading.replace(true) {
            return;
        }
        let mixer = Arc::downgrade(&self.mixer);
        glib::timeout_add_seconds_local(5, move || {
            let Some(mixer) = mixer.upgrade() else {
                return glib::ControlFlow::Break;
            };
            for line in mixer.reading() {
                log(&format!("sound: {line}"));
            }
            glib::ControlFlow::Continue
        });
    }
}

/// One remote track's sound into the mixer, on the core's thread.
struct PcmToMixer {
    mixer: Arc<Mixer>,
    sid: String,
}

impl PcmSink for PcmToMixer {
    fn on_pcm(&self, pcm: Vec<u8>, _sample_rate: u32, channels: u32) {
        let samples: Vec<i16> = pcm
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| i16::from_le_bytes(*pair))
            .collect();
        self.mixer.push(&self.sid, &samples, channels as usize);
    }
}

/// What a remote sound is, from its publication (sound.ts `isMusic`,
/// lossless.ts): the song desk's music is a track of no known source named
/// "music" or "music-lossless"; sound shared on its own is a screen's sound.
fn kind_of(source: MediaSource, name: &str) -> Kind {
    match source {
        MediaSource::ScreenShareAudio => Kind::Screen,
        MediaSource::Unknown if name.starts_with("music") => Kind::Music,
        _ => Kind::Voice,
    }
}

/// A sound of this app's own into the mixer, from whichever thread makes it.
struct LocalToMixer {
    mixer: Arc<Mixer>,
    key: String,
}

impl PcmSink for LocalToMixer {
    fn on_pcm(&self, pcm: Vec<u8>, _sample_rate: u32, channels: u32) {
        let samples: Vec<i16> = pcm
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| i16::from_le_bytes(*pair))
            .collect();
        self.mixer.push(&self.key, &samples, channels as usize);
    }
}

impl Drop for LocalToMixer {
    fn drop(&mut self) {
        self.mixer.remove(&self.key);
    }
}

/// The call's lossless streams into the mixer, on the core's thread.
struct LosslessToMixer(Arc<Mixer>);

impl LosslessSink for LosslessToMixer {
    fn on_lossless(
        &self,
        identity: String,
        music: bool,
        samples: Vec<f32>,
        rate: u32,
        channels: u32,
    ) {
        self.0
            .push_lossless(&identity, music, &samples, rate, channels as usize);
    }

    fn on_lossless_end(&self, identity: String, music: bool) {
        self.0.end_lossless(&identity, music);
    }
}

/// The voice's input while it is published, and whether as original sound.
type Published = Rc<RefCell<Option<(Arc<AudioInput>, bool)>>>;

/// An open microphone.
struct Mic {
    /// The capture now; another when the device is changed
    /// (`use_device`), the voice's input handed from the one to the other.
    shared: Rc<RefCell<Arc<capture::Shared>>>,
    echo: Arc<Echo>,
    /// A test's sound in place of the device: there is nothing to change to.
    test: bool,
    /// A generated tone, sent unprocessed whatever it is published as.
    raw: bool,
    published: Published,
    /// A publish is out; another one is refused until it is done, since the
    /// room never publishes a second voice.
    busy: Rc<Cell<bool>>,
}

impl Mic {
    fn set_input(shared: &capture::Shared, input: Option<Arc<AudioInput>>) {
        *shared.input.lock().unwrap_or_else(|p| p.into_inner()) = input;
    }
}

impl Mic {
    fn now(&self) -> Arc<capture::Shared> {
        self.shared.borrow().clone()
    }
}

impl Microphone for Mic {
    fn level(&self) -> f32 {
        self.now().level()
    }

    fn publish(&self, call: Arc<Call>, original: bool) -> Local<Result<(), MediaError>> {
        if self.busy.get() {
            return Box::pin(async { Err(MediaError::Busy) });
        }
        if matches!(self.published.borrow().as_ref(), Some((_, current)) if *current == original) {
            return Box::pin(async { Ok(()) });
        }
        // Already published as the other kind of voice: the capture changes
        // in place, on the same track (sound.ts `applyOriginalSound` restarts
        // the track it has). Published again, the voice was gone for
        // everybody for a second or two while the new track was subscribed,
        // measured from a browser beside it. The web also changes the
        // sender's bitrate; the core has no call for that on a published
        // voice, so the track keeps the bitrate it was published with.
        let current = self.published.borrow().clone();
        if let Some((input, _)) = current {
            if !self.raw {
                let kind = SoundKind::Voice { original };
                *self
                    .now()
                    .processing
                    .lock()
                    .unwrap_or_else(|p| p.into_inner()) =
                    Processing::of(&sound_profile(kind).capture);
            }
            self.published.replace(Some((input, original)));
            log(&format!(
                "sound: the voice is now captured as {}, on the same track",
                if original {
                    "original sound"
                } else {
                    "a processed voice"
                }
            ));
            return Box::pin(async { Ok(()) });
        }
        self.busy.set(true);
        let (shared, published, busy, raw) = (
            self.shared.clone(),
            self.published.clone(),
            self.busy.clone(),
            self.raw,
        );
        Box::pin(async move {
            // The new voice first, then the old one taken down: taken down
            // first, the voice was gone for everybody until the new one was
            // up, a silence as long as a publish takes. The web changes the
            // track in place (sound.ts `applyOriginalSound`: restartTrack and
            // the sender's bitrate), which the core has no call for: a
            // published voice's bitrate is fixed when it is published. So for
            // the moment between the two both are published, and only the new
            // one is fed from the moment it is up, as on the Mac
            // (CallModel.setOriginalSound).
            let kind = SoundKind::Voice { original };
            let result = call.publish_audio(kind).await;
            busy.set(false);
            let input = result.map_err(|fault| MediaError::Call(fault.to_string()))?;
            let capture = shared.borrow().clone();
            input.set_muted(capture.muted.load(Ordering::Relaxed));
            if !raw {
                *capture.processing.lock().unwrap_or_else(|p| p.into_inner()) =
                    Processing::of(&sound_profile(kind).capture);
            }
            Mic::set_input(&capture, Some(input.clone()));
            let old = published.replace(Some((input, original)));
            log(&format!(
                "sound: the voice is published{}",
                if original { " as original sound" } else { "" }
            ));
            if let Some((old, _)) = old {
                if let Err(fault) = old.unpublish().await {
                    log(&format!("sound: the old voice did not unpublish: {fault}"));
                }
            }
            Ok(())
        })
    }

    fn unpublish(&self) -> Local<()> {
        let shared = self.now();
        let old = self.published.borrow_mut().take();
        Box::pin(async move {
            Mic::set_input(&shared, None);
            if let Some((input, _)) = old {
                if let Err(fault) = input.unpublish().await {
                    log(&format!("sound: the voice did not unpublish: {fault}"));
                }
            }
        })
    }

    fn set_muted(&self, muted: bool) {
        self.now().muted.store(muted, Ordering::Relaxed);
        if let Some((input, _)) = self.published.borrow().as_ref() {
            input.set_muted(muted);
        }
    }

    fn close(&self) {
        let shared = self.now();
        shared.stop.store(true, Ordering::Relaxed);
        let old = self.published.borrow_mut().take();
        Mic::set_input(&shared, None);
        if let Some((input, _)) = old {
            glib::spawn_future_local(async move {
                let _ = input.unpublish().await;
            });
        }
    }

    fn use_device(&self, choice: Option<String>) -> Local<Result<(), MediaError>> {
        if self.test {
            return Box::pin(async { Ok(()) });
        }
        let current = self.now();
        let processing = *current.processing.lock().unwrap_or_else(|p| p.into_inner());
        let fresh = capture::Shared::new(processing, current.muted.load(Ordering::Relaxed));
        let (opened, open) = oneshot::channel();
        capture::start(
            fresh.clone(),
            self.echo.clone(),
            From::Device(choice.filter(|c| !c.is_empty())),
            opened,
        );
        let shared = self.shared.clone();
        Box::pin(async move {
            match open.await {
                Ok(Ok(())) => {}
                Ok(Err(fault)) => {
                    fresh.stop.store(true, Ordering::Relaxed);
                    return Err(fault);
                }
                Err(_) => return Err(MediaError::Failed("the microphone's thread ended".into())),
            }
            // The voice's input moves from the old capture to the new one
            // between two of their frames: at most ten milliseconds of the
            // old device are not sent, and nothing is sent twice.
            let old = shared.replace(fresh.clone());
            let input = old.input.lock().unwrap_or_else(|p| p.into_inner()).take();
            Mic::set_input(&fresh, input);
            old.stop.store(true, Ordering::Relaxed);
            log("sound: the voice now comes from another microphone");
            Ok(())
        })
    }
}

impl Drop for Mic {
    fn drop(&mut self) {
        self.now().stop.store(true, Ordering::Relaxed);
    }
}
