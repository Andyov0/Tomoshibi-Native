//! Remote pictures on screen: a widget the room puts wherever a picture
//! goes, and the sink the core delivers that picture's frames to.
//!
//! On Linux the app draws every remote picture itself (core-map section 4;
//! the core draws only on Windows and Android). A [`VideoView`] hands the
//! room a sink ([`VideoView::sink`]) for `Call::attach_video(sid, sink)` or a
//! camera's preview. Frames arrive on the core's own threads
//! ("tomoshibi-core") as packed I420. Each is turned there into a finished
//! GdkMemoryTexture (libyuv's I420ToARGB, through the vendored libwebrtc the
//! core already links, into GDK's own default layout so that GTK uploads it
//! as it is) and left in a one-frame slot, newest wins. The slot then wakes
//! the view's one waiting future on the main thread, which swaps the texture
//! in and asks for a draw. That swap is all the main thread does per frame:
//! no conversion, no copy, and nothing at all while no frames come.
//!
//! The first version took the frame on every tick of the widget's frame
//! clock, which kept the clock running, and the window redrawing at the
//! display's rate, for as long as any picture was on screen, frames or no
//! frames; a camera that had stopped still cost sixty wakeups a second.
//!
//! GdkTexture is immutable and thread-safe (GDK's own documentation, and
//! gdk4-rs marks MemoryTexture `Send + Sync`), which is what lets the core's
//! thread make it. Nothing else of GTK is touched off the main thread.
//!
//! The room keeps the core sending the size it shows: [`VideoView::
//! connect_size_wanted`] says the size in device pixels whenever the
//! allocation changes, for `Call::request_video_size`.
//!
//! Owned by the render module.

use std::{
    cell::{Cell, RefCell},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
};

use gtk::{gdk, glib, graphene, gsk, prelude::*, subclass::prelude::*};
use libwebrtc::native::yuv_helper;
use tokio::sync::Notify;
use tomoshibi_core::call::{RemoteFrame, VideoSink};

/// How a picture fills its view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Fit {
    /// Whole, with bars where the shapes differ (a shared screen).
    #[default]
    Contain,
    /// Filling the view, cut where the shapes differ (a camera in a tile).
    Cover,
}

/// One picture converted and ready for the main thread.
struct Picture {
    texture: gdk::Texture,
    /// Clockwise degrees the picture is to be turned when shown.
    rotation: u32,
}

/// Where the core leaves the newest frame. Shared between the core's
/// threads, which write it, and the view, which takes it.
#[derive(Default)]
pub struct FrameSlot {
    newest: Mutex<Option<Picture>>,
    arrived: AtomicU64,
    /// What the view's future on the main thread waits on. A Notify keeps
    /// one permit when nobody is waiting, so frames arriving faster than the
    /// main thread takes them are one wakeup, not a queue of them.
    wake: Notify,
    /// The view is gone; its future stops at the next wakeup.
    closed: AtomicBool,
}

impl FrameSlot {
    /// How many frames have arrived, drawn or not; for tests and the
    /// statistics overlay.
    pub fn arrived(&self) -> u64 {
        self.arrived.load(Ordering::Relaxed)
    }

    fn take(&self) -> Option<Picture> {
        self.newest
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .take()
    }

    fn put(&self, picture: Picture) {
        // A lock a panic elsewhere poisoned is still a usable one: the slot
        // holds one whole value at a time, never half of one.
        *self
            .newest
            .lock()
            .unwrap_or_else(|poison| poison.into_inner()) = Some(picture);
        self.arrived.fetch_add(1, Ordering::Relaxed);
        self.wake.notify_one();
    }
}

impl VideoSink for FrameSlot {
    fn on_frame(&self, frame: RemoteFrame) {
        // A frame with no I420 is an Apple CVPixelBuffer, which no Linux
        // core makes (remote.rs fills `i420` off Apple).
        let Some(i420) = frame.i420.as_deref() else {
            return;
        };
        if self.closed.load(Ordering::Relaxed) {
            return;
        }
        let Some(texture) = texture_of(i420, frame.width, frame.height) else {
            return;
        };
        self.put(Picture {
            texture,
            rotation: frame.rotation % 360,
        });
    }
}

/// The packed planes' lengths for a picture of this size, or `None` for a
/// buffer shorter than the size says. Checked here because libyuv's
/// wrappers assert on a short plane, and an assertion on the core's thread
/// is a panic inside the core's video stream.
fn planes(i420: &[u8], width: u32, height: u32) -> Option<(&[u8], &[u8], &[u8])> {
    let (w, h) = (width as usize, height as usize);
    let (cw, ch) = (w.div_ceil(2), h.div_ceil(2));
    if w == 0 || h == 0 || i420.len() < w * h + 2 * cw * ch {
        return None;
    }
    let (y, rest) = i420.split_at(w * h);
    let (u, rest) = rest.split_at(cw * ch);
    Some((y, u, &rest[..cw * ch]))
}

/// A packed I420 picture as a texture in GDK's default memory layout
/// (blue, green, red, alpha bytes, premultiplied, which an opaque picture
/// is trivially): libyuv's "ARGB" is that byte order on a little-endian
/// machine, and every machine this app is built for is one.
fn texture_of(i420: &[u8], width: u32, height: u32) -> Option<gdk::Texture> {
    let (y, u, v) = planes(i420, width, height)?;
    let stride = width * 4;
    let mut bgra = vec![0u8; (stride * height) as usize];
    let chroma = width.div_ceil(2);
    yuv_helper::i420_to_argb(
        y,
        width,
        u,
        chroma,
        v,
        chroma,
        &mut bgra,
        stride,
        width as i32,
        height as i32,
    );
    let bytes = glib::Bytes::from_owned(bgra);
    Some(
        gdk::MemoryTexture::new(
            width as i32,
            height as i32,
            gdk::MemoryFormat::B8g8r8a8Premultiplied,
            &bytes,
            stride as usize,
        )
        .upcast(),
    )
}

/// A packed I420 picture (Y, then U, then V, each without padding, as
/// `RemoteFrame::i420` is) as RGBA bytes, eight bits a channel, with
/// BT.601's limited range, which is what libwebrtc's own I420ToARGB
/// assumes. `None` for a buffer shorter than the size says. For tests and
/// anything that wants the pixels rather than a texture.
pub fn i420_to_rgba(i420: &[u8], width: u32, height: u32) -> Option<Vec<u8>> {
    let (y, u, v) = planes(i420, width, height)?;
    let stride = width * 4;
    let mut rgba = vec![0u8; (stride * height) as usize];
    let chroma = width.div_ceil(2);
    // libyuv names formats by the word, low byte last: its ABGR is red,
    // green, blue, alpha in memory.
    yuv_helper::i420_to_abgr(
        y,
        width,
        u,
        chroma,
        v,
        chroma,
        &mut rgba,
        stride,
        width as i32,
        height as i32,
    );
    Some(rgba)
}

/// A clockwise turn's effect on a picture's shape: the size it is shown at.
pub fn turned_size(width: u32, height: u32, rotation: u32) -> (u32, u32) {
    if rotation % 180 == 90 {
        (height, width)
    } else {
        (width, height)
    }
}

/// Where a picture of `picture` (already turned) is drawn in a view of
/// `view`: centred, scaled to fit whole or to cover. Returns the drawn
/// width and height.
pub fn fitted(view: (f32, f32), picture: (f32, f32), fit: Fit) -> (f32, f32) {
    let (vw, vh) = view;
    let (pw, ph) = picture;
    if pw <= 0.0 || ph <= 0.0 {
        return (0.0, 0.0);
    }
    let scale = match fit {
        Fit::Contain => (vw / pw).min(vh / ph),
        Fit::Cover => (vw / pw).max(vh / ph),
    };
    (pw * scale, ph * scale)
}

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct VideoView {
        pub slot: Arc<FrameSlot>,
        pub texture: RefCell<Option<gdk::Texture>>,
        pub rotation: Cell<u32>,
        pub fit: Cell<Fit>,
        pub mirrored: Cell<bool>,
        pub drawn: Cell<u64>,
        pub wanted: Cell<(u32, u32)>,
        #[allow(clippy::type_complexity)]
        pub size_wanted: RefCell<Vec<Box<dyn Fn(&super::VideoView, u32, u32)>>>,
        #[allow(clippy::type_complexity)]
        pub picture_changed: RefCell<Vec<Box<dyn Fn(&super::VideoView, bool)>>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for VideoView {
        const NAME: &'static str = "TomoshibiVideoView";
        type Type = super::VideoView;
        type ParentType = gtk::Widget;
    }

    impl ObjectImpl for VideoView {
        fn constructed(&self) {
            self.parent_constructed();
            let view = self.obj();
            view.set_hexpand(true);
            view.set_vexpand(true);
            view.set_overflow(gtk::Overflow::Hidden);
            view.set_can_target(false);
            // The one future that takes frames for this view, woken by the
            // slot. It holds the view weakly, so a view thrown away is not
            // kept alive by it, and stops when the view is disposed.
            let (slot, weak) = (self.slot.clone(), view.downgrade());
            glib::spawn_future_local(async move {
                loop {
                    slot.wake.notified().await;
                    if slot.closed.load(Ordering::Relaxed) {
                        return;
                    }
                    let Some(view) = weak.upgrade() else {
                        return;
                    };
                    view.imp().take_frame();
                }
            });
        }

        fn dispose(&self) {
            self.slot.closed.store(true, Ordering::Relaxed);
            self.slot.wake.notify_one();
        }
    }

    impl WidgetImpl for VideoView {
        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            let Some(texture) = self.texture.borrow().clone() else {
                return;
            };
            let view = self.obj();
            let (view_w, view_h) = (view.width() as f32, view.height() as f32);
            if view_w <= 0.0 || view_h <= 0.0 {
                return;
            }
            let rotation = self.rotation.get();
            let (pic_w, pic_h) = (texture.width() as f32, texture.height() as f32);
            let (shown_w, shown_h) = {
                let (w, h) = turned_size(texture.width() as u32, texture.height() as u32, rotation);
                (w as f32, h as f32)
            };
            let (fit_w, _) = fitted((view_w, view_h), (shown_w, shown_h), self.fit.get());
            let scale = fit_w / shown_w;
            let (draw_w, draw_h) = (pic_w * scale, pic_h * scale);
            // A share shown much smaller than it was sent is drawn from the
            // texture's mipmaps, so that its words stay readable rather than
            // shimmering; anything else is filtered linearly.
            let factor = view.scale_factor().max(1) as f32;
            let filter = if scale * factor < 0.5 {
                gsk::ScalingFilter::Trilinear
            } else {
                gsk::ScalingFilter::Linear
            };

            snapshot.push_clip(&graphene::Rect::new(0.0, 0.0, view_w, view_h));
            snapshot.save();
            snapshot.translate(&graphene::Point::new(view_w / 2.0, view_h / 2.0));
            if self.mirrored.get() {
                snapshot.scale(-1.0, 1.0);
            }
            if rotation != 0 {
                snapshot.rotate(rotation as f32);
            }
            snapshot.append_scaled_texture(
                &texture,
                filter,
                &graphene::Rect::new(-draw_w / 2.0, -draw_h / 2.0, draw_w, draw_h),
            );
            snapshot.restore();
            snapshot.pop();
        }

        fn size_allocate(&self, width: i32, height: i32, baseline: i32) {
            self.parent_size_allocate(width, height, baseline);
            let factor = self.obj().scale_factor().max(1);
            let wanted = (
                (width * factor).max(0) as u32,
                (height * factor).max(0) as u32,
            );
            if wanted != self.wanted.get() && wanted.0 > 0 && wanted.1 > 0 {
                self.wanted.set(wanted);
                let view = self.obj();
                for listener in self.size_wanted.borrow().iter() {
                    listener(&view, wanted.0, wanted.1);
                }
            }
        }
    }

    impl VideoView {
        pub(super) fn take_frame(&self) {
            let Some(picture) = self.slot.take() else {
                return;
            };
            let first = self.texture.replace(Some(picture.texture)).is_none();
            self.rotation.set(picture.rotation);
            self.drawn.set(self.drawn.get() + 1);
            let view = self.obj();
            view.queue_draw();
            if first {
                self.tell_picture(true);
            }
        }

        pub(super) fn tell_picture(&self, has: bool) {
            let view = self.obj();
            for listener in self.picture_changed.borrow().iter() {
                listener(&view, has);
            }
        }
    }
}

glib::wrapper! {
    /// A remote (or local preview) picture. See the module's comment.
    pub struct VideoView(ObjectSubclass<imp::VideoView>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Default for VideoView {
    fn default() -> Self {
        Self::new()
    }
}

impl VideoView {
    pub fn new() -> Self {
        glib::Object::new()
    }

    /// The sink to attach: `call.attach_video(sid, view.sink())`. The same
    /// sink for the view's whole life.
    pub fn sink(&self) -> Arc<dyn VideoSink> {
        self.imp().slot.clone()
    }

    /// The slot itself, for its counters.
    pub fn slot(&self) -> Arc<FrameSlot> {
        self.imp().slot.clone()
    }

    pub fn set_fit(&self, fit: Fit) {
        if self.imp().fit.replace(fit) != fit {
            self.queue_draw();
        }
    }

    pub fn fit(&self) -> Fit {
        self.imp().fit.get()
    }

    /// Mirrored, as one's own camera is shown.
    pub fn set_mirrored(&self, mirrored: bool) {
        if self.imp().mirrored.replace(mirrored) != mirrored {
            self.queue_draw();
        }
    }

    /// Whether a picture has arrived since the view was made or cleared.
    pub fn has_picture(&self) -> bool {
        self.imp().texture.borrow().is_some()
    }

    /// The picture's size as shown (turned), once one has arrived.
    pub fn picture_size(&self) -> Option<(u32, u32)> {
        let texture = self.imp().texture.borrow();
        let texture = texture.as_ref()?;
        Some(turned_size(
            texture.width() as u32,
            texture.height() as u32,
            self.imp().rotation.get(),
        ))
    }

    /// Frames put on screen.
    pub fn frames_drawn(&self) -> u64 {
        self.imp().drawn.get()
    }

    /// Forgets the picture shown, for a track that went away. A frame that
    /// had arrived and not yet been taken goes with it.
    pub fn clear(&self) {
        let imp = self.imp();
        imp.slot.take();
        if imp.texture.replace(None).is_some() {
            imp.tell_picture(false);
        }
        self.queue_draw();
    }

    /// Called with the view's size in device pixels whenever it changes, for
    /// `call.request_video_size(sid, width, height)`.
    pub fn connect_size_wanted(&self, listener: impl Fn(&VideoView, u32, u32) + 'static) {
        self.imp().size_wanted.borrow_mut().push(Box::new(listener));
    }

    /// The size last said to [`connect_size_wanted`](Self::connect_size_wanted)'s
    /// listeners, (0, 0) before the first allocation: for a sink attached
    /// after the view was laid out, which asks for its size at once.
    pub fn size_wanted(&self) -> (u32, u32) {
        self.imp().wanted.get()
    }

    /// Called when the first picture arrives (true) and when the view is
    /// cleared of it (false): for a tile that fades from the person's
    /// initials to their camera rather than cutting.
    pub fn connect_picture_changed(&self, listener: impl Fn(&VideoView, bool) + 'static) {
        self.imp()
            .picture_changed
            .borrow_mut()
            .push(Box::new(listener));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain(width: u32, height: u32, y: u8, u: u8, v: u8) -> Vec<u8> {
        let (cw, ch) = (width.div_ceil(2) as usize, height.div_ceil(2) as usize);
        let mut frame = vec![y; (width * height) as usize];
        frame.extend(std::iter::repeat_n(u, cw * ch));
        frame.extend(std::iter::repeat_n(v, cw * ch));
        frame
    }

    fn frame(width: u32, height: u32, y: u8, rotation: u32) -> RemoteFrame {
        RemoteFrame {
            pixel_buffer: 0,
            width,
            height,
            rotation,
            timestamp_us: 0,
            i420: Some(plain(width, height, y, 128, 128)),
        }
    }

    fn pixel(texture: &gdk::Texture, x: usize, y: usize) -> [u8; 4] {
        let (w, h) = (texture.width() as usize, texture.height() as usize);
        let mut bytes = vec![0u8; w * h * 4];
        // download() writes GDK's default layout: blue, green, red, alpha.
        texture.download(&mut bytes, w * 4);
        let at = (y * w + x) * 4;
        [bytes[at + 2], bytes[at + 1], bytes[at], bytes[at + 3]]
    }

    #[test]
    fn the_limited_range_ends_are_black_and_white() {
        let black = i420_to_rgba(&plain(4, 2, 16, 128, 128), 4, 2).unwrap();
        assert_eq!(&black[..4], &[0, 0, 0, 255]);
        let white = i420_to_rgba(&plain(4, 2, 235, 128, 128), 4, 2).unwrap();
        assert_eq!(&white[..4], &[255, 255, 255, 255]);
    }

    #[test]
    fn red_comes_out_red() {
        // BT.601 red: Y 81, U 90, V 240.
        let red = i420_to_rgba(&plain(2, 2, 81, 90, 240), 2, 2).unwrap();
        assert!(
            red[0] > 240 && red[1] < 10 && red[2] < 10,
            "{:?}",
            &red[..4]
        );
    }

    #[test]
    fn the_texture_is_the_same_colour_in_gdk_order() {
        // The texture's bytes are blue first; a picture handed to GDK in the
        // wrong order shows a red shirt as blue, which no test of the RGBA
        // converter alone would see.
        let red = texture_of(&plain(4, 4, 81, 90, 240), 4, 4).unwrap();
        let [r, g, b, a] = pixel(&red, 1, 1);
        assert!(r > 240 && g < 10 && b < 10 && a == 255, "{r} {g} {b} {a}");
        assert_eq!((red.width(), red.height()), (4, 4));
    }

    #[test]
    fn odd_sizes_and_short_buffers() {
        assert!(i420_to_rgba(&plain(3, 3, 16, 128, 128), 3, 3).is_some());
        assert!(texture_of(&plain(5, 3, 16, 128, 128), 5, 3).is_some());
        assert!(i420_to_rgba(&[0; 10], 4, 4).is_none());
        assert!(texture_of(&[0; 10], 4, 4).is_none());
        assert!(i420_to_rgba(&[], 0, 0).is_none());
    }

    #[test]
    fn the_slot_keeps_only_the_newest_frame() {
        let slot = FrameSlot::default();
        for y in [16, 235] {
            slot.on_frame(frame(2, 2, y, 0));
        }
        assert_eq!(slot.arrived(), 2);
        let taken = slot.take().unwrap();
        assert_eq!(pixel(&taken.texture, 0, 0)[0], 255, "the newer, white frame");
        assert!(slot.take().is_none(), "taken once");
    }

    #[test]
    fn a_closed_slot_converts_nothing() {
        let slot = FrameSlot::default();
        slot.closed.store(true, Ordering::Relaxed);
        slot.on_frame(frame(2, 2, 16, 0));
        assert_eq!(slot.arrived(), 0);
        assert!(slot.take().is_none());
    }

    #[test]
    fn frames_wake_the_view_once_however_many_arrive() {
        // The wakeup is a permit, not a queue: three frames before the main
        // thread looks are one wakeup and one picture.
        let slot = FrameSlot::default();
        for y in [16, 100, 235] {
            slot.on_frame(frame(2, 2, y, 90));
        }
        use std::{future::Future, pin::pin, task};
        let mut context = task::Context::from_waker(task::Waker::noop());
        let mut first = pin!(slot.wake.notified());
        assert!(first.as_mut().poll(&mut context).is_ready(), "woken");
        let mut second = pin!(slot.wake.notified());
        assert!(
            second.as_mut().poll(&mut context).is_pending(),
            "one permit for three frames"
        );
        assert_eq!(slot.take().map(|picture| picture.rotation), Some(90));
    }

    #[test]
    fn a_turned_picture_is_shown_on_its_side() {
        assert_eq!(turned_size(1280, 720, 90), (720, 1280));
        assert_eq!(turned_size(1280, 720, 270), (720, 1280));
        assert_eq!(turned_size(1280, 720, 180), (1280, 720));
        assert_eq!(turned_size(1280, 720, 0), (1280, 720));
    }

    #[test]
    fn contain_fits_whole_and_cover_fills() {
        // A 16:9 camera in a square view.
        let (w, h) = fitted((100.0, 100.0), (160.0, 90.0), Fit::Contain);
        assert_eq!((w.round(), h.round()), (100.0, 56.0));
        let (w, h) = fitted((100.0, 100.0), (160.0, 90.0), Fit::Cover);
        assert_eq!((w.round(), h.round()), (178.0, 100.0));
        assert_eq!(fitted((100.0, 100.0), (0.0, 90.0), Fit::Cover), (0.0, 0.0));
    }
}
