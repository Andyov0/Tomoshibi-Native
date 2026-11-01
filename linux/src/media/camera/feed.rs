//! Where every camera's frames go, whichever way they were captured: made
//! into I420 (frame.rs), the background blurred when that is on (blur.rs),
//! pushed into the camera's `VideoInput` while it is published and not
//! muted, and shown to every preview.
//!
//! The capture thread does the first three itself, frame by frame, since the
//! encoder wants every frame and wants it in order. The previews are drawn
//! from a thread of their own, through a slot that holds only the newest
//! frame: a preview sink converts to RGBA on the thread that calls it
//! (render's FrameSlot), and a view drawing slowly (a debug build, a busy
//! machine) must cost the preview a frame, never the call.
//!
//! The previews are handed what is sent, after the blur, so that what a
//! person sees of themselves is what the room sees, as on Windows
//! (CameraBlur.cs).

use std::{
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Condvar, Mutex,
    },
    thread,
    time::Instant,
};

use tomoshibi_core::call::{RemoteFrame, VideoInput, VideoSink};

use super::{
    blur::Blur,
    frame::{self, Planes, Raw},
};
use crate::app::log;

/// A frame for the previews.
struct Shown {
    width: u32,
    height: u32,
    timestamp_us: i64,
    i420: Vec<u8>,
}

#[derive(Default)]
struct Previews {
    sinks: Vec<(u64, Arc<dyn VideoSink>)>,
    newest: Option<Shown>,
    closed: bool,
}

/// One open camera's pipeline.
pub struct Feed {
    pub input: Mutex<Option<Arc<VideoInput>>>,
    pub muted: AtomicBool,
    /// Whether the background is to be blurred.
    pub blur_wanted: AtomicBool,
    /// The blur, made when it is first wanted and let go when it is turned
    /// off; the capture thread's alone.
    blur: Mutex<Option<Blur>>,
    planes: Mutex<Planes>,
    previews: Arc<(Mutex<Previews>, Condvar)>,
    next_preview: AtomicU64,
    /// The size of the last frame, which the camera is published at.
    pub size: Mutex<(u32, u32)>,
    pub frames: AtomicU64,
    pub pushed: AtomicU64,
    failures: AtomicU64,
    epoch: Instant,
}

impl Feed {
    pub fn new() -> Arc<Feed> {
        let previews: Arc<(Mutex<Previews>, Condvar)> =
            Arc::new((Mutex::new(Previews::default()), Condvar::new()));
        let feed = Arc::new(Feed {
            input: Mutex::new(None),
            muted: AtomicBool::new(false),
            blur_wanted: AtomicBool::new(false),
            blur: Mutex::new(None),
            planes: Mutex::new(Planes::default()),
            previews: previews.clone(),
            next_preview: AtomicU64::new(1),
            size: Mutex::new((0, 0)),
            frames: AtomicU64::new(0),
            pushed: AtomicU64::new(0),
            failures: AtomicU64::new(0),
            epoch: Instant::now(),
        });
        let spawned = thread::Builder::new()
            .name("tomoshibi camera preview".into())
            .spawn(move || show(previews));
        if let Err(fault) = spawned {
            log(&format!(
                "camera: the preview thread did not start: {fault}"
            ));
        }
        feed
    }

    /// Microseconds on one steady clock, which WebRTC's timestamp aligner
    /// takes to its own.
    pub fn now_us(&self) -> i64 {
        self.epoch.elapsed().as_micros() as i64
    }

    pub fn add_preview(&self, sink: Arc<dyn VideoSink>) -> u64 {
        let id = self.next_preview.fetch_add(1, Ordering::Relaxed);
        let (previews, _) = &*self.previews;
        previews
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .sinks
            .push((id, sink));
        id
    }

    pub fn remove_preview(&self, id: u64) {
        let (previews, _) = &*self.previews;
        previews
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .sinks
            .retain(|(at, _)| *at != id);
    }

    /// Lets the preview thread go.
    pub fn close(&self) {
        let (previews, woken) = &*self.previews;
        let mut previews = previews.lock().unwrap_or_else(|p| p.into_inner());
        previews.closed = true;
        previews.sinks.clear();
        woken.notify_all();
    }

    /// A frame from the device, on its capture thread.
    pub fn deliver(&self, raw: &Raw, timestamp_us: i64) {
        let mut planes = self.planes.lock().unwrap_or_else(|p| p.into_inner());
        if let Err(fault) = frame::convert(raw, &mut planes) {
            // A frame that does not read is dropped; a camera that sends
            // nothing else is said once, then every thousandth time.
            let failures = self.failures.fetch_add(1, Ordering::Relaxed);
            if failures.is_multiple_of(1000) {
                log(&format!(
                    "camera: a frame did not read ({fault:?}, {} so far)",
                    failures + 1
                ));
            }
            return;
        }
        self.deliver_planes(&mut planes, timestamp_us);
    }

    /// A frame already in I420 (a test's), on its thread.
    pub fn deliver_planes(&self, planes: &mut Planes, timestamp_us: i64) {
        let (width, height) = (planes.width, planes.height);
        if width < 2 || height < 2 {
            return;
        }
        *self.size.lock().unwrap_or_else(|p| p.into_inner()) = (width, height);
        self.frames.fetch_add(1, Ordering::Relaxed);
        self.blur(planes);

        if !self.muted.load(Ordering::Relaxed) {
            let input = self.input.lock().unwrap_or_else(|p| p.into_inner()).clone();
            if let Some(input) = input {
                let (y, u, v) = planes.split();
                let cw = width / 2;
                input.push_planes(
                    y.to_vec(),
                    u.to_vec(),
                    v.to_vec(),
                    width,
                    cw,
                    1,
                    width,
                    height,
                    timestamp_us,
                    0,
                );
                self.pushed.fetch_add(1, Ordering::Relaxed);
            }
        }

        let (previews, woken) = &*self.previews;
        let mut previews = previews.lock().unwrap_or_else(|p| p.into_inner());
        if !previews.sinks.is_empty() {
            previews.newest = Some(Shown {
                width,
                height,
                timestamp_us,
                i420: planes.data.clone(),
            });
            woken.notify_one();
        }
    }

    fn blur(&self, planes: &mut Planes) {
        let wanted = self.blur_wanted.load(Ordering::Relaxed);
        let mut blur = self.blur.lock().unwrap_or_else(|p| p.into_inner());
        match (wanted, blur.is_some()) {
            (true, false) => *blur = Some(Blur::new()),
            (false, true) => *blur = None,
            _ => {}
        }
        if let Some(blur) = blur.as_mut() {
            let (width, height) = (planes.width as usize, planes.height as usize);
            let (y, u, v) = planes.split_mut();
            blur.process(y, u, v, width, height);
        }
    }
}

/// The preview thread: the newest frame to every preview, until closed.
fn show(previews: Arc<(Mutex<Previews>, Condvar)>) {
    let (lock, woken) = &*previews;
    loop {
        let (frame, sinks) = {
            let mut held = lock.lock().unwrap_or_else(|p| p.into_inner());
            loop {
                if held.closed {
                    return;
                }
                if held.newest.is_some() {
                    break;
                }
                held = woken.wait(held).unwrap_or_else(|p| p.into_inner());
            }
            let frame = held.newest.take();
            (
                frame,
                held.sinks
                    .iter()
                    .map(|(_, sink)| sink.clone())
                    .collect::<Vec<_>>(),
            )
        };
        let Some(frame) = frame else { continue };
        for sink in sinks {
            sink.on_frame(RemoteFrame {
                pixel_buffer: 0,
                width: frame.width,
                height: frame.height,
                rotation: 0,
                timestamp_us: frame.timestamp_us,
                i420: Some(frame.i420.clone()),
            });
        }
    }
}
