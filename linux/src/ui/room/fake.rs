//! Devices that are not there, for a debug run (TOMOSHIBI_DEBUG_FAKE_MEDIA):
//! a camera that draws a test picture, a microphone that beeps quietly, a
//! screen that shares a test page, and a speaker that only counts what it
//! is given.
//!
//! What the room is proved with in a container that has no camera, no
//! sound server and no portal: everything the room does with devices goes
//! through the `media::Media` traits, so the same room code publishes these
//! as it will publish the real ones (media/, another module's), and a
//! browser in the call sees an ordinary camera, voice and share. The
//! counting speaker is what shows remote sound arriving at all; its log line
//! ("first sound from ...") is what a test reads.
//!
//! The switch's value says what is on at the join, comma-separated:
//! `camera`, `microphone`, `share`; `1` alone gives the devices and turns
//! nothing on. Debug builds only.

use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    rc::Rc,
    sync::{
        atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

use tomoshibi_core::call::{
    AudioInput, Call, PcmSink, RemoteFrame, VideoInput, VideoKind, VideoSink,
};
use tomoshibi_protocol::share::SoundKind;

use crate::{
    app::log,
    media::{
        Camera, Device, DeviceKind, Local, Media, MediaError, Microphone, ScreenShare, ShareChoice,
    },
};

/// The switch, read once.
pub fn wanted() -> Option<String> {
    std::env::var("TOMOSHIBI_DEBUG_FAKE_MEDIA")
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

/// Whether the switch turns `what` on at the join.
pub fn on_at_join(what: &str) -> bool {
    wanted().is_some_and(|value| value.split(',').any(|one| one.trim() == what))
}

#[derive(Default)]
pub struct FakeMedia {
    playing: RefCell<HashMap<String, Arc<Counted>>>,
    volumes: RefCell<HashMap<String, f32>>,
    silent: Cell<bool>,
}

impl FakeMedia {
    pub fn new() -> Rc<FakeMedia> {
        Rc::new(FakeMedia::default())
    }
}

fn device(id: &str, label: &str, kind: DeviceKind) -> Device {
    Device {
        id: id.to_owned(),
        label: label.to_owned(),
        kind,
        is_default: true,
    }
}

impl Media for FakeMedia {
    fn devices(&self, kind: DeviceKind) -> Vec<Device> {
        match kind {
            DeviceKind::Microphone => vec![device("test-microphone", "Test microphone", kind)],
            DeviceKind::Speaker => vec![device("test-speaker", "Test speaker", kind)],
            DeviceKind::Camera => vec![device("test-camera", "Test camera", kind)],
        }
    }

    fn on_devices_changed(&self, _: Box<dyn Fn()>) {}

    fn open_microphone(&self, _: Option<String>) -> Local<Result<Rc<dyn Microphone>, MediaError>> {
        Box::pin(async { Ok(Rc::new(FakeMicrophone::default()) as Rc<dyn Microphone>) })
    }

    fn open_camera(&self, _: Option<String>) -> Local<Result<Rc<dyn Camera>, MediaError>> {
        Box::pin(async { Ok(FakeCamera::start(Picture::Camera, 640, 360, 15) as Rc<dyn Camera>) })
    }

    fn share_screen(
        &self,
        call: Arc<Call>,
        choice: ShareChoice,
    ) -> Local<Result<Rc<dyn ScreenShare>, MediaError>> {
        Box::pin(async move {
            let (width, height) = (1280, 720);
            let input = call
                .publish_video(
                    VideoKind::Screen {
                        fps: choice.fps,
                        quality: choice.quality,
                    },
                    width,
                    height,
                )
                .await
                .map_err(|fault| MediaError::Call(fault.to_string()))?;
            let source = FakeCamera::start(Picture::Screen, width, height, choice.fps.min(15));
            source.shared.input.lock().unwrap().replace(input);
            log("the test screen is shared");
            Ok(Rc::new(FakeShare {
                source,
                ended: RefCell::new(None),
            }) as Rc<dyn ScreenShare>)
        })
    }

    fn can_share_screen(&self) -> bool {
        true
    }

    fn attach_call(&self, call: &Arc<Call>) {
        log(&format!(
            "test media attached to the call as {}",
            call.local_identity()
        ));
    }

    fn detach_call(&self) {
        self.playing.borrow_mut().clear();
    }

    fn play(&self, call: &Arc<Call>, track_sid: &str, identity: &str) {
        let counted = Arc::new(Counted {
            sid: track_sid.to_owned(),
            identity: identity.to_owned(),
            bytes: AtomicU64::new(0),
        });
        call.attach_pcm(track_sid.to_owned(), counted.clone());
        self.playing
            .borrow_mut()
            .insert(track_sid.to_owned(), counted);
        log(&format!("playing {identity}'s sound {track_sid}"));
    }

    fn stop_playing(&self, call: &Arc<Call>, track_sid: &str) {
        if let Some(counted) = self.playing.borrow_mut().remove(track_sid) {
            call.detach(track_sid.to_owned());
            log(&format!(
                "stopped {}'s sound {}: {} bytes heard",
                counted.identity,
                track_sid,
                counted.bytes.load(Ordering::Relaxed)
            ));
        }
    }

    fn set_speaker(&self, choice: Option<String>) {
        log(&format!("speaker chosen: {choice:?}"));
    }

    fn set_volume(&self, identity: &str, level: f32) {
        self.volumes.borrow_mut().insert(identity.to_owned(), level);
        log(&format!("volume of {identity} set to {level:.2}"));
    }

    fn set_silent(&self, silent: bool) {
        self.silent.set(silent);
    }
}

/// A speaker that counts.
struct Counted {
    sid: String,
    identity: String,
    bytes: AtomicU64,
}

impl PcmSink for Counted {
    fn on_pcm(&self, pcm: Vec<u8>, sample_rate: u32, channels: u32) {
        if self.bytes.fetch_add(pcm.len() as u64, Ordering::Relaxed) == 0 {
            log(&format!(
                "first sound from {} on {}: {} Hz, {} channel(s)",
                self.identity, self.sid, sample_rate, channels
            ));
        }
    }
}

/// A microphone that beeps for a fifth of a second every second, quietly,
/// as a browser's fake microphone does.
#[derive(Default)]
struct FakeMicrophone {
    shared: Arc<MicShared>,
}

#[derive(Default)]
struct MicShared {
    input: Mutex<Option<Arc<AudioInput>>>,
    running: AtomicBool,
    level: AtomicU32,
}

impl Microphone for FakeMicrophone {
    fn level(&self) -> f32 {
        f32::from_bits(self.shared.level.load(Ordering::Relaxed))
    }

    fn publish(&self, call: Arc<Call>, original: bool) -> Local<Result<(), MediaError>> {
        let shared = self.shared.clone();
        Box::pin(async move {
            let old = shared.input.lock().unwrap().take();
            if let Some(old) = old {
                let _ = old.unpublish().await;
            }
            let input = call
                .publish_audio(SoundKind::Voice { original })
                .await
                .map_err(|fault| MediaError::Call(fault.to_string()))?;
            shared.input.lock().unwrap().replace(input);
            if !shared.running.swap(true, Ordering::SeqCst) {
                let shared = shared.clone();
                std::thread::Builder::new()
                    .name("test-microphone".into())
                    .spawn(move || beep(shared))
                    .map_err(|fault| MediaError::Failed(fault.to_string()))?;
            }
            log("the test microphone is published");
            Ok(())
        })
    }

    fn unpublish(&self) -> Local<()> {
        let shared = self.shared.clone();
        Box::pin(async move {
            let input = shared.input.lock().unwrap().take();
            if let Some(input) = input {
                let _ = input.unpublish().await;
            }
        })
    }

    fn set_muted(&self, muted: bool) {
        if let Some(input) = self.shared.input.lock().unwrap().as_ref() {
            input.set_muted(muted);
        }
    }

    fn close(&self) {
        self.shared.running.store(false, Ordering::SeqCst);
    }
}

fn beep(shared: Arc<MicShared>) {
    let started = Instant::now();
    let mut sent = 0u64;
    while shared.running.load(Ordering::SeqCst) {
        let samples: Vec<i16> = (0..480)
            .map(|n| {
                let at = sent * 480 + n;
                let second = (at % 48_000) as f32 / 48_000.0;
                if second < 0.2 {
                    ((at as f32 * 440.0 * std::f32::consts::TAU / 48_000.0).sin() * 2400.0) as i16
                } else {
                    0
                }
            })
            .collect();
        let loud = samples.iter().map(|s| s.unsigned_abs()).max().unwrap_or(0) as f32 / 32768.0;
        shared
            .level
            .store((loud * 4.0).min(1.0).to_bits(), Ordering::Relaxed);
        if let Some(input) = shared.input.lock().unwrap().as_ref() {
            input.push(samples);
        }
        sent += 1;
        let due = started + Duration::from_millis(sent * 10);
        if let Some(wait) = due.checked_duration_since(Instant::now()) {
            std::thread::sleep(wait);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Picture {
    Camera,
    Screen,
}

/// A picture source on a thread of its own: the test camera, and the test
/// screen.
struct FakeCamera {
    shared: Arc<CameraShared>,
}

struct CameraShared {
    kind: Picture,
    width: u32,
    height: u32,
    input: Mutex<Option<Arc<VideoInput>>>,
    previews: Mutex<Vec<(u64, Arc<dyn VideoSink>)>>,
    next: AtomicU64,
    running: AtomicBool,
    muted: AtomicBool,
}

impl FakeCamera {
    fn start(kind: Picture, width: u32, height: u32, fps: u32) -> Rc<FakeCamera> {
        let shared = Arc::new(CameraShared {
            kind,
            width,
            height,
            input: Mutex::new(None),
            previews: Mutex::new(Vec::new()),
            next: AtomicU64::new(1),
            running: AtomicBool::new(true),
            muted: AtomicBool::new(false),
        });
        let running = shared.clone();
        let _ = std::thread::Builder::new()
            .name("test-camera".into())
            .spawn(move || draw(running, fps.max(1)));
        Rc::new(FakeCamera { shared })
    }
}

impl Camera for FakeCamera {
    fn add_preview(&self, sink: Arc<dyn VideoSink>) -> u64 {
        let id = self.shared.next.fetch_add(1, Ordering::Relaxed);
        self.shared.previews.lock().unwrap().push((id, sink));
        id
    }

    fn remove_preview(&self, id: u64) {
        self.shared
            .previews
            .lock()
            .unwrap()
            .retain(|(kept, _)| *kept != id);
    }

    fn publish(&self, call: Arc<Call>) -> Local<Result<(), MediaError>> {
        let shared = self.shared.clone();
        Box::pin(async move {
            let input = call
                .publish_video(VideoKind::Camera, shared.width, shared.height)
                .await
                .map_err(|fault| MediaError::Call(fault.to_string()))?;
            shared.input.lock().unwrap().replace(input);
            log("the test camera is published");
            Ok(())
        })
    }

    fn unpublish(&self) -> Local<()> {
        let shared = self.shared.clone();
        Box::pin(async move {
            let input = shared.input.lock().unwrap().take();
            if let Some(input) = input {
                let _ = input.unpublish().await;
            }
        })
    }

    fn set_muted(&self, muted: bool) {
        self.shared.muted.store(muted, Ordering::Relaxed);
    }

    fn set_blur(&self, _: bool) {}

    fn close(&self) {
        self.shared.running.store(false, Ordering::SeqCst);
    }
}

impl Drop for FakeCamera {
    fn drop(&mut self) {
        self.shared.running.store(false, Ordering::SeqCst);
    }
}

struct FakeShare {
    source: Rc<FakeCamera>,
    ended: RefCell<Option<Box<dyn Fn()>>>,
}

impl ScreenShare for FakeShare {
    fn stop(&self) -> Local<()> {
        let shared = self.source.shared.clone();
        Box::pin(async move {
            shared.running.store(false, Ordering::SeqCst);
            let input = shared.input.lock().unwrap().take();
            if let Some(input) = input {
                let _ = input.unpublish().await;
            }
            log("the test screen stopped");
        })
    }

    fn on_ended(&self, ended: Box<dyn Fn()>) {
        self.ended.replace(Some(ended));
    }
}

/// Draws the test picture until the source is closed: colour bars with a
/// white block sweeping across them for the camera, a light page with a
/// moving stripe for the screen, so that a frozen picture is told from a
/// live one at a glance.
fn draw(shared: Arc<CameraShared>, fps: u32) {
    let (w, h) = (shared.width as usize, shared.height as usize);
    let (cw, ch) = (w.div_ceil(2), h.div_ceil(2));
    let started = Instant::now();
    let mut frame = 0u64;
    // BT.601 limited range: white, yellow, cyan, green, magenta, red, blue.
    const BARS: [(u8, u8, u8); 7] = [
        (235, 128, 128),
        (210, 16, 146),
        (170, 166, 16),
        (145, 54, 34),
        (106, 202, 222),
        (81, 90, 240),
        (41, 240, 110),
    ];
    // The page under the moving block is drawn once; a debug build drew
    // every pixel of a 720p screen at a few frames a second.
    let mut base_y = vec![16u8; w * h];
    let mut base_u = vec![128u8; cw * ch];
    let mut base_v = vec![128u8; cw * ch];
    for row in 0..h {
        for column in 0..w {
            let (ly, lu, lv) = match shared.kind {
                Picture::Camera => BARS[column * BARS.len() / w],
                Picture::Screen if (row / 40) % 2 == 0 => (220, 128, 128),
                Picture::Screen => (200, 128, 128),
            };
            base_y[row * w + column] = ly;
            if row % 2 == 0 && column % 2 == 0 {
                base_u[(row / 2) * cw + column / 2] = lu;
                base_v[(row / 2) * cw + column / 2] = lv;
            }
        }
    }
    let (block_w, rows, (by, bu, bv)) = match shared.kind {
        Picture::Camera => (w / 10, h / 3..2 * h / 3, (235u8, 128u8, 128u8)),
        Picture::Screen => (12, 0..h, (81, 90, 240)),
    };
    while shared.running.load(Ordering::SeqCst) {
        let (mut y, mut u, mut v) = (base_y.clone(), base_u.clone(), base_v.clone());
        let sweep = (frame as usize * 6) % w;
        let columns = sweep..(sweep + block_w).min(w);
        for row in rows.clone() {
            y[row * w + columns.start..row * w + columns.end].fill(by);
            if row % 2 == 0 {
                let at = (row / 2) * cw;
                u[at + columns.start / 2..at + columns.end.div_ceil(2)].fill(bu);
                v[at + columns.start / 2..at + columns.end.div_ceil(2)].fill(bv);
            }
        }
        let timestamp_us = started.elapsed().as_micros() as i64;
        let muted = shared.muted.load(Ordering::Relaxed);
        let previews: Vec<Arc<dyn VideoSink>> = shared
            .previews
            .lock()
            .unwrap()
            .iter()
            .map(|(_, sink)| sink.clone())
            .collect();
        if !previews.is_empty() {
            let mut packed = Vec::with_capacity(y.len() + u.len() + v.len());
            packed.extend_from_slice(&y);
            packed.extend_from_slice(&u);
            packed.extend_from_slice(&v);
            for sink in previews {
                sink.on_frame(RemoteFrame {
                    pixel_buffer: 0,
                    width: shared.width,
                    height: shared.height,
                    rotation: 0,
                    timestamp_us,
                    i420: Some(packed.clone()),
                });
            }
        }
        if !muted {
            if let Some(input) = shared.input.lock().unwrap().as_ref() {
                input.push_planes(
                    y,
                    u,
                    v,
                    shared.width,
                    cw as u32,
                    1,
                    shared.width,
                    shared.height,
                    timestamp_us,
                    0,
                );
            }
        }
        frame += 1;
        let due = started + Duration::from_micros(frame * 1_000_000 / fps as u64);
        if let Some(wait) = due.checked_duration_since(Instant::now()) {
            std::thread::sleep(wait);
        }
    }
}
