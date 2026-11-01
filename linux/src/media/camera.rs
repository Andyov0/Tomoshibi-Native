//! The camera, through the xdg-desktop-portal's Camera interface and
//! PipeWire, with V4L2 directly where there is no portal.
//!
//! What this does (core-map section 3): lists the cameras and says when one
//! comes or goes; opens the chosen one at 1280x720 and thirty frames a
//! second, as the web asks (`camera_capture`, the computer's rule) and as
//! Windows opens it (Devices.cs); makes each frame I420 (frame.rs, from
//! NV12, YUYV, I420 or MJPEG); blurs the background behind a switch with
//! the web's own segmenter (blur.rs); pushes the frame into the camera's
//! `VideoInput` with `push_planes`; and hands the same frame to every
//! preview as a `RemoteFrame` with `i420` filled (feed.rs).
//!
//! Where the cameras come from, in order: the camera portal's PipeWire
//! remote, which is how a sandboxed app reaches a camera and how libcamera's
//! cameras are reached at all; else, where no portal answers, the session's
//! PipeWire directly; else, and for as long as PipeWire lists no camera,
//! the V4L2 nodes (v4l2.rs). One of the two is listed at a time, since a
//! PipeWire camera is the same device as its V4L2 node. The portal is asked
//! the first time the cameras are wanted, not at start: an app that is
//! never shown a camera never asks for one.
//!
//! A debug run given a test picture (`TOMOSHIBI_SOURCE`, pattern.rs) lists
//! that alone and sends it, so that a test never opens a real camera.

mod blur;
mod feed;
pub mod frame;
mod modes;
mod pattern;
mod pipewire;
mod v4l2;

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

use gio::prelude::*;
use tokio::sync::{mpsc, oneshot};
use tomoshibi_core::call::{Call, VideoInput, VideoKind, VideoSink};
use tomoshibi_protocol::share::{camera_capture, CameraDevice};

use self::{feed::Feed, modes::Wanted};
use super::{Camera, Device, DeviceKind, Local, MediaError};
use crate::app::log;

pub use self::{frame::PixelFormat, pattern::TestPicture};

/// Makes PipeWire safe to call in this binary, in place of
/// `pipewire::init()` and before anything else of pipewire-rs: false where
/// libpipewire is not there. One function for the whole app, the screen
/// share's (media/screen/linkage.rs says why it is needed at all).
pub fn pipewire_ready() -> bool {
    pipewire::ready()
}

/// How the camera side is set up: by devices.rs, from the launch.
#[derive(Debug, Clone, Default)]
pub struct CameraOptions {
    /// A test's picture in place of every camera.
    pub test_picture: Option<TestPicture>,
}

/// What the threads below tell the main thread.
enum Event {
    /// PipeWire's cameras changed.
    PipeWire,
    /// A /dev/video node came or went.
    Nodes,
    /// A camera stopped by itself: the capture started under this token.
    Ended { label: String, token: u64 },
}

/// Whom to tell that a camera stopped by itself, once.
type EndedSlot = RefCell<Option<Box<dyn Fn()>>>;

/// The camera half of the app's `Media`.
pub struct Cameras {
    options: CameraOptions,
    changed: Rc<dyn Fn()>,
    events: mpsc::UnboundedSender<Event>,
    started: Cell<bool>,
    /// The portal and PipeWire have been asked, and have answered or not.
    settled: Cell<bool>,
    waiting: RefCell<Vec<oneshot::Sender<()>>>,
    v4l2: RefCell<Vec<v4l2::Node>>,
    pipewire: RefCell<Option<Rc<pipewire::Session>>>,
    monitor: RefCell<Option<gio::FileMonitor>>,
    next_token: Cell<u64>,
    /// The captures running, by token, and whom each tells when it stops by
    /// itself. A capture stopped on purpose (closed, or changed for another)
    /// is taken out first: closing a PipeWire stream reports it unconnected
    /// like an unplugged camera does.
    running: RefCell<std::collections::HashMap<u64, std::rc::Weak<EndedSlot>>>,
    /// Test cameras pretended unplugged, gone from the list.
    gone: RefCell<std::collections::HashSet<String>>,
}

fn wanted() -> Wanted {
    let capture = camera_capture(CameraDevice::Computer);
    Wanted {
        width: capture.width,
        height: capture.height,
        fps: capture.frame_rate,
    }
}

impl Cameras {
    pub fn new(options: CameraOptions, changed: Rc<dyn Fn()>) -> Rc<Cameras> {
        let (events, mut heard) = mpsc::unbounded_channel::<Event>();
        let cameras = Rc::new(Cameras {
            options,
            changed,
            events,
            started: Cell::new(false),
            settled: Cell::new(false),
            waiting: RefCell::new(Vec::new()),
            v4l2: RefCell::new(Vec::new()),
            pipewire: RefCell::new(None),
            monitor: RefCell::new(None),
            next_token: Cell::new(1),
            running: RefCell::new(std::collections::HashMap::new()),
            gone: RefCell::new(std::collections::HashSet::new()),
        });
        let weak = Rc::downgrade(&cameras);
        glib::spawn_future_local(async move {
            while let Some(event) = heard.recv().await {
                let Some(cameras) = weak.upgrade() else {
                    return;
                };
                match event {
                    Event::PipeWire => (cameras.changed)(),
                    Event::Nodes => {
                        // udev sets a new node's permissions a moment after
                        // it appears.
                        glib::timeout_future(Duration::from_millis(500)).await;
                        while heard.try_recv().is_ok() {}
                        cameras.v4l2.replace(v4l2::nodes());
                        (cameras.changed)();
                    }
                    Event::Ended { label, token } => {
                        log(&format!("camera: {label} stopped by itself"));
                        let slot = cameras.running.borrow_mut().remove(&token);
                        let ended = slot
                            .and_then(|slot| slot.upgrade())
                            .and_then(|slot| slot.borrow_mut().take());
                        if let Some(ended) = ended {
                            ended();
                        }
                    }
                }
            }
        });
        cameras
    }

    /// Looks for cameras, the first time they are wanted.
    fn start(self: &Rc<Self>) {
        if self.started.replace(true) {
            return;
        }
        if self.options.test_picture.is_some() {
            self.settle();
            return;
        }
        self.v4l2.replace(v4l2::nodes());
        let monitor = gio::File::for_path("/dev")
            .monitor_directory(gio::FileMonitorFlags::NONE, gio::Cancellable::NONE);
        match monitor {
            Ok(monitor) => {
                let events = self.events.clone();
                monitor.connect_changed(move |_, file, _, event| {
                    let video = file
                        .basename()
                        .is_some_and(|name| name.to_string_lossy().starts_with("video"));
                    if video
                        && matches!(
                            event,
                            gio::FileMonitorEvent::Created | gio::FileMonitorEvent::Deleted
                        )
                    {
                        let _ = events.send(Event::Nodes);
                    }
                });
                self.monitor.replace(Some(monitor));
            }
            Err(fault) => log(&format!(
                "camera: /dev is not watched for cameras coming and going: {fault}"
            )),
        }
        let weak = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            look_for_pipewire(weak.clone()).await;
            if let Some(cameras) = weak.upgrade() {
                cameras.settle();
            }
        });
    }

    /// The portal has answered or is not there, and PipeWire has been
    /// connected to or not: every camera there is now is listed.
    fn settle(&self) {
        self.settled.set(true);
        for waiting in self.waiting.borrow_mut().drain(..) {
            let _ = waiting.send(());
        }
    }

    /// Waits until the cameras are settled, five seconds at most: a portal
    /// that never answers leaves the V4L2 nodes.
    async fn settled(self: &Rc<Self>) {
        self.start();
        if self.settled.get() {
            return;
        }
        let (told, tell) = oneshot::channel();
        self.waiting.borrow_mut().push(told);
        let _ = glib::future_with_timeout(Duration::from_secs(5), tell).await;
    }

    /// PipeWire's cameras, if it lists any.
    fn pipewire_nodes(&self) -> Option<(Rc<pipewire::Session>, Vec<pipewire::Node>)> {
        let session = self.pipewire.borrow().clone()?;
        let nodes = session
            .nodes
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone();
        (!nodes.is_empty()).then_some((session, nodes))
    }

    /// The cameras there are, the first one first.
    pub fn devices(self: &Rc<Self>) -> Vec<Device> {
        if let Some(picture) = &self.options.test_picture {
            let label = match picture {
                TestPicture::Pattern(format) => format!("Test picture ({format:?})"),
                TestPicture::Y4m(path) => format!("Test picture ({})", path.display()),
            };
            // Two, so that a test can move the call from one to the other
            // and take either away (`pretend_unplugged`).
            let gone = self.gone.borrow();
            return [("test", label), ("test-2", "Test picture 2".to_owned())]
                .into_iter()
                .filter(|(id, _)| !gone.contains(*id))
                .enumerate()
                .map(|(index, (id, label))| Device {
                    id: id.into(),
                    label,
                    kind: DeviceKind::Camera,
                    is_default: index == 0,
                })
                .collect();
        }
        self.start();
        let devices: Vec<(String, String)> = match self.pipewire_nodes() {
            Some((_, nodes)) => nodes
                .into_iter()
                .map(|n| (format!("pipewire:{}", n.name), n.label))
                .collect(),
            None => self
                .v4l2
                .borrow()
                .iter()
                .map(|n| (n.id.clone(), n.label.clone()))
                .collect(),
        };
        devices
            .into_iter()
            .enumerate()
            .map(|(index, (id, label))| Device {
                id,
                label,
                kind: DeviceKind::Camera,
                is_default: index == 0,
            })
            .collect()
    }

    /// Opens the camera `choice` (`None`, or one that has gone: the first),
    /// once the portal and PipeWire have answered, so that a camera asked
    /// for as the app starts is not looked for before it can be listed.
    pub fn open_camera(
        self: &Rc<Self>,
        choice: Option<String>,
    ) -> Local<Result<Rc<dyn Camera>, MediaError>> {
        let cameras = self.clone();
        Box::pin(async move {
            cameras.settled().await;
            let feed = Feed::new();
            let stop = Arc::new(AtomicBool::new(false));
            let (opened, open) = oneshot::channel();
            let (backend, device, token) = match cameras.begin(choice, &feed, &stop, opened) {
                Ok(begun) => begun,
                Err(fault) => {
                    feed.close();
                    return Err(fault);
                }
            };
            let result = open
                .await
                .unwrap_or_else(|_| Err(MediaError::Failed("the camera's thread ended".into())));
            match result {
                Ok(()) => {
                    let ended: Rc<EndedSlot> = Rc::new(RefCell::new(None));
                    cameras
                        .running
                        .borrow_mut()
                        .insert(token, Rc::downgrade(&ended));
                    Ok(Rc::new(Opened {
                        cameras: cameras.clone(),
                        feed,
                        capture: Rc::new(RefCell::new(Capture {
                            stop,
                            backend,
                            device,
                            token,
                        })),
                        ended,
                        published: Rc::new(RefCell::new(None)),
                        closed: Rc::new(Cell::new(false)),
                    }) as Rc<dyn Camera>)
                }
                Err(fault) => {
                    stop.store(true, Ordering::Relaxed);
                    if let Backend::PipeWire(session, token) = &backend {
                        session.close(*token);
                    }
                    feed.close();
                    Err(fault)
                }
            }
        })
    }

    /// Starts capturing the camera chosen into `feed`; `opened` is told
    /// how the device's own open went. The capture, the device it is (as
    /// listed) and the token it reports its own end under.
    fn begin(
        &self,
        choice: Option<String>,
        feed: &Arc<Feed>,
        stop: &Arc<AtomicBool>,
        opened: oneshot::Sender<Result<(), MediaError>>,
    ) -> Result<(Backend, String, u64), MediaError> {
        let token = self.next_token.get();
        self.next_token.set(token + 1);
        if let Some(picture) = self.options.test_picture.clone() {
            // The second test camera draws the pattern from other planes,
            // which proves the frames come from another capture.
            let second = choice.as_deref() == Some("test-2");
            let picture = match picture {
                TestPicture::Pattern(PixelFormat::Nv12) if second => {
                    TestPicture::Pattern(PixelFormat::I420)
                }
                TestPicture::Pattern(_) if second => TestPicture::Pattern(PixelFormat::Nv12),
                other => other,
            };
            pattern::start(picture, feed.clone(), stop.clone(), opened);
            let id = if second { "test-2" } else { "test" };
            return Ok((Backend::Thread, id.into(), token));
        }
        let choice = choice.filter(|c| !c.is_empty());
        let events = self.events.clone();
        if let Some((session, nodes)) = self.pipewire_nodes() {
            let node = choice
                .as_deref()
                .and_then(|c| c.strip_prefix("pipewire:"))
                .and_then(|name| nodes.iter().find(|n| n.name == name))
                .unwrap_or(&nodes[0])
                .clone();
            let label = node.label.clone();
            let device = format!("pipewire:{}", node.name);
            let ended = Box::new(move || {
                let _ = events.send(Event::Ended { label, token });
            });
            if !session.open(token, node, wanted(), feed.clone(), opened, ended) {
                return Err(MediaError::Failed("PipeWire's loop is gone".into()));
            }
            return Ok((Backend::PipeWire(session, token), device, token));
        }
        let nodes = self.v4l2.borrow().clone();
        let node = choice
            .as_deref()
            .and_then(|c| v4l2::find(c, &nodes))
            .or_else(|| nodes.first().cloned())
            .ok_or(MediaError::NotFound)?;
        let label = node.label.clone();
        let device = node.id.clone();
        v4l2::start(
            node,
            wanted(),
            feed.clone(),
            stop.clone(),
            opened,
            move || {
                let _ = events.send(Event::Ended { label, token });
            },
        );
        Ok((Backend::Thread, device, token))
    }
}

/// Asks the portal for its PipeWire remote and connects to it, or to the
/// session's PipeWire where no portal answers; the cameras hold the
/// connection once it is up.
async fn look_for_pipewire(weak: std::rc::Weak<Cameras>) {
    let fd = match portal().await {
        Portal::Remote(fd) => Some(fd),
        Portal::Absent(why) => {
            log(&format!(
                "camera: no camera portal ({why}); PipeWire directly"
            ));
            None
        }
        Portal::Refused(why) => {
            log(&format!("camera: the portal refused the camera ({why})"));
            return;
        }
    };
    let Some(events) = weak.upgrade().map(|cameras| cameras.events.clone()) else {
        return;
    };
    let (connected, connection) = oneshot::channel();
    pipewire::start(
        fd,
        move || {
            let _ = events.send(Event::PipeWire);
        },
        connected,
    );
    match connection.await {
        Ok(Ok(session)) => {
            if let Some(cameras) = weak.upgrade() {
                cameras.pipewire.replace(Some(Rc::new(session)));
                (cameras.changed)();
            }
        }
        Ok(Err(fault)) => log(&format!("camera: no PipeWire ({fault}); V4L2 alone")),
        Err(_) => {}
    }
}

/// What the portal said.
enum Portal {
    Remote(std::os::fd::OwnedFd),
    Absent(String),
    Refused(String),
}

/// Asks the camera portal for its PipeWire remote. A host app is let in
/// without a dialog; a sandboxed one is asked. A remote is asked for even
/// when no camera is present yet, so that one plugged in later appears on
/// it.
async fn portal() -> Portal {
    use ashpd::desktop::camera::Camera as CameraPortal;
    let proxy = match CameraPortal::new().await {
        Ok(proxy) => proxy,
        Err(fault) => return Portal::Absent(fault.to_string()),
    };
    match proxy
        .request_access(Default::default())
        .await
        .and_then(|request| request.response())
    {
        Ok(()) => {}
        Err(ashpd::Error::Response(response)) => return Portal::Refused(format!("{response:?}")),
        Err(fault) => return Portal::Absent(fault.to_string()),
    }
    match proxy.open_pipe_wire_remote(Default::default()).await {
        Ok(fd) => Portal::Remote(fd),
        Err(fault) => Portal::Absent(fault.to_string()),
    }
}

enum Backend {
    /// A thread of ours (V4L2, a test's picture), stopped by its flag.
    Thread,
    /// A stream on the PipeWire thread, closed by its token.
    PipeWire(Rc<pipewire::Session>, u64),
}

/// One device's capture into an open camera's feed.
struct Capture {
    stop: Arc<AtomicBool>,
    backend: Backend,
    /// The device, as listed.
    device: String,
    token: u64,
}

impl Capture {
    /// Stops it on purpose: whoever waits for it to stop by itself is not
    /// told.
    fn stop(&self, cameras: &Cameras) {
        cameras.running.borrow_mut().remove(&self.token);
        self.stop.store(true, Ordering::Relaxed);
        if let Backend::PipeWire(session, token) = &self.backend {
            session.close(*token);
        }
    }
}

/// An open camera.
struct Opened {
    cameras: Rc<Cameras>,
    feed: Arc<Feed>,
    /// The device's capture now; another after `use_device`.
    capture: Rc<RefCell<Capture>>,
    ended: Rc<EndedSlot>,
    published: Rc<RefCell<Option<Arc<VideoInput>>>>,
    closed: Rc<Cell<bool>>,
}

impl Opened {
    fn stop_capture(&self) {
        if self.closed.replace(true) {
            return;
        }
        self.capture.borrow().stop(&self.cameras);
        self.ended.borrow_mut().take();
        *self.feed.input.lock().unwrap_or_else(|p| p.into_inner()) = None;
        self.feed.close();
    }
}

impl Camera for Opened {
    fn add_preview(&self, sink: Arc<dyn VideoSink>) -> u64 {
        self.feed.add_preview(sink)
    }

    fn remove_preview(&self, id: u64) {
        self.feed.remove_preview(id);
    }

    fn publish(&self, call: Arc<Call>) -> Local<Result<(), MediaError>> {
        if self.published.borrow().is_some() {
            return Box::pin(async { Ok(()) });
        }
        let (feed, published) = (self.feed.clone(), self.published.clone());
        Box::pin(async move {
            // Published at the size the camera sends, which the core's
            // ladder is worked out from (`camera_publish`): the first frame
            // says it. A camera that has sent nothing in three seconds is
            // published at the size asked for.
            let mut waited = 0;
            let size = loop {
                let size = *feed.size.lock().unwrap_or_else(|p| p.into_inner());
                if size.0 >= 2 || waited >= 60 {
                    break size;
                }
                glib::timeout_future(Duration::from_millis(50)).await;
                waited += 1;
            };
            let (width, height) = if size.0 >= 2 {
                size
            } else {
                (wanted().width, wanted().height)
            };
            let input = call
                .publish_video(VideoKind::Camera, width, height)
                .await
                .map_err(|fault| MediaError::Call(fault.to_string()))?;
            input.set_muted(feed.muted.load(Ordering::Relaxed));
            *feed.input.lock().unwrap_or_else(|p| p.into_inner()) = Some(input.clone());
            published.replace(Some(input));
            log(&format!("camera: published at {width}x{height}"));
            Ok(())
        })
    }

    fn unpublish(&self) -> Local<()> {
        let input = self.published.borrow_mut().take();
        *self.feed.input.lock().unwrap_or_else(|p| p.into_inner()) = None;
        Box::pin(async move {
            if let Some(input) = input {
                if let Err(fault) = input.unpublish().await {
                    log(&format!("camera: did not unpublish: {fault}"));
                }
            }
        })
    }

    fn set_muted(&self, muted: bool) {
        self.feed.muted.store(muted, Ordering::Relaxed);
        if let Some(input) = self.published.borrow().as_ref() {
            input.set_muted(muted);
        }
    }

    fn set_blur(&self, on: bool) {
        self.feed.blur_wanted.store(on, Ordering::Relaxed);
    }

    fn device(&self) -> Option<String> {
        Some(self.capture.borrow().device.clone())
    }

    #[cfg(debug_assertions)]
    fn pretend_unplugged(&self) {
        if self.cameras.options.test_picture.is_none() {
            return;
        }
        let capture = self.capture.borrow();
        log(&format!(
            "camera: pretending {} was pulled out",
            capture.device
        ));
        self.cameras
            .gone
            .borrow_mut()
            .insert(capture.device.clone());
        capture.stop.store(true, Ordering::Relaxed);
        let _ = self.cameras.events.send(Event::Ended {
            label: capture.device.clone(),
            token: capture.token,
        });
        (self.cameras.changed)();
    }

    fn on_ended(&self, ended: Box<dyn Fn()>) {
        if !self.closed.get() {
            self.ended.replace(Some(ended));
        }
    }

    /// Another device into the same feed: the frames it sends go to the
    /// same input and the same previews, and the old capture is stopped
    /// only once the new one has opened, so the picture never stops for
    /// those watching. A camera of another size is simply a picture of
    /// another size from then on, which the encoder takes as a browser's
    /// does after `switchActiveDevice`.
    fn use_device(&self, choice: Option<String>) -> Local<Result<(), MediaError>> {
        if self.closed.get() {
            return Box::pin(async { Err(MediaError::NotFound) });
        }
        let cameras = self.cameras.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let (opened, open) = oneshot::channel();
        let (backend, device, token) = match cameras.begin(choice, &self.feed, &stop, opened) {
            Ok(begun) => begun,
            Err(fault) => return Box::pin(async move { Err(fault) }),
        };
        let fresh = Capture {
            stop,
            backend,
            device,
            token,
        };
        let (capture, closed, ended) = (
            self.capture.clone(),
            self.closed.clone(),
            Rc::downgrade(&self.ended),
        );
        Box::pin(async move {
            let result = open
                .await
                .unwrap_or_else(|_| Err(MediaError::Failed("the camera's thread ended".into())));
            if let Err(fault) = result {
                fresh.stop(&cameras);
                return Err(fault);
            }
            // Closed while the new one was opening: it is nobody's.
            if closed.get() {
                fresh.stop(&cameras);
                return Err(MediaError::NotFound);
            }
            cameras.running.borrow_mut().insert(fresh.token, ended);
            let old = capture.replace(fresh);
            old.stop(&cameras);
            log(&format!(
                "camera: now {}, into the same picture",
                capture.borrow().device
            ));
            Ok(())
        })
    }

    fn close(&self) {
        self.stop_capture();
        if let Some(input) = self.published.borrow_mut().take() {
            glib::spawn_future_local(async move {
                let _ = input.unpublish().await;
            });
        }
    }
}

impl Drop for Opened {
    fn drop(&mut self) {
        self.stop_capture();
    }
}
