//! Remote pictures on screen: a widget the room puts wherever a picture
//! goes, and the sink the core delivers that picture's frames to.
//!
//! On Linux the app draws every remote picture itself (core-map section 4;
//! the core draws only on Windows and Android). A [`VideoView`] hands the
//! room a sink ([`VideoView::sink`]) for `Call::attach_video(sid, sink)` or a
//! camera's preview. Frames arrive on the core's own threads
//! ("tomoshibi-core") as I420; each is converted there to RGBA and left in a
//! one-frame slot, newest wins, and the widget takes it on the next tick of
//! its frame clock, on the main thread, as a GdkMemoryTexture. Nothing GTK
//! is touched off the main thread, and a picture arriving faster than the
//! screen draws costs a conversion, never a queue.
//!
//! The room keeps the core sending the size it shows: [`VideoView::
//! connect_size_wanted`] says the size in device pixels whenever the
//! allocation changes, for `Call::request_video_size`.
//!
//! Owned by the render module. What is here is a working first version
//! (a straightforward converter on the core's thread); the render module may
//! replace the insides (libyuv through the vendored libwebrtc, GL upload of
//! the planes) and keeps the API.

use std::{
    cell::{Cell, RefCell},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
};

use gtk::{gdk, glib, graphene, prelude::*, subclass::prelude::*};
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
    width: u32,
    height: u32,
    /// Clockwise degrees the picture is to be turned when shown.
    rotation: u32,
    rgba: glib::Bytes,
}

/// Where the core leaves the newest frame. Shared between the core's
/// threads, which write it, and the view, which takes it.
#[derive(Default)]
pub struct FrameSlot {
    newest: Mutex<Option<Picture>>,
    fresh: AtomicBool,
    arrived: AtomicU64,
}

impl FrameSlot {
    /// How many frames have arrived, drawn or not; for tests and the
    /// statistics overlay.
    pub fn arrived(&self) -> u64 {
        self.arrived.load(Ordering::Relaxed)
    }

    fn take(&self) -> Option<Picture> {
        if !self.fresh.swap(false, Ordering::AcqRel) {
            return None;
        }
        self.newest.lock().ok()?.take()
    }
}

impl VideoSink for FrameSlot {
    fn on_frame(&self, frame: RemoteFrame) {
        // A frame with no I420 is an Apple CVPixelBuffer, which no Linux
        // core makes (remote.rs fills `i420` off Apple).
        let Some(i420) = frame.i420.as_deref() else {
            return;
        };
        let Some(rgba) = i420_to_rgba(i420, frame.width, frame.height) else {
            return;
        };
        let picture = Picture {
            width: frame.width,
            height: frame.height,
            rotation: frame.rotation % 360,
            rgba: glib::Bytes::from_owned(rgba),
        };
        if let Ok(mut newest) = self.newest.lock() {
            *newest = Some(picture);
        }
        self.arrived.fetch_add(1, Ordering::Relaxed);
        self.fresh.store(true, Ordering::Release);
    }
}

/// A packed I420 picture (Y, then U, then V, each without padding, as
/// `RemoteFrame::i420` is) as RGBA, eight bits a channel, with BT.601's
/// limited range, which is what libwebrtc's own I420ToARGB assumes. `None`
/// for a buffer shorter than the size says.
pub fn i420_to_rgba(i420: &[u8], width: u32, height: u32) -> Option<Vec<u8>> {
    let (w, h) = (width as usize, height as usize);
    let (cw, ch) = (w.div_ceil(2), h.div_ceil(2));
    if w == 0 || h == 0 || i420.len() < w * h + 2 * cw * ch {
        return None;
    }
    let (y_plane, rest) = i420.split_at(w * h);
    let (u_plane, v_plane) = rest.split_at(cw * ch);
    let mut rgba = vec![0u8; w * h * 4];
    for row in 0..h {
        for column in 0..w {
            let y = y_plane[row * w + column] as i32 - 16;
            let chroma = (row / 2) * cw + column / 2;
            let u = u_plane[chroma] as i32 - 128;
            let v = v_plane[chroma] as i32 - 128;
            // Fixed point, eight fractional bits: 1.164, 1.596, 0.391,
            // 0.813 and 2.018.
            let luma = 298 * y + 128;
            let red = (luma + 409 * v) >> 8;
            let green = (luma - 100 * u - 208 * v) >> 8;
            let blue = (luma + 516 * u) >> 8;
            let at = (row * w + column) * 4;
            rgba[at] = red.clamp(0, 255) as u8;
            rgba[at + 1] = green.clamp(0, 255) as u8;
            rgba[at + 2] = blue.clamp(0, 255) as u8;
            rgba[at + 3] = 255;
        }
    }
    Some(rgba)
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
            view.add_tick_callback(|view, _| {
                view.imp().take_frame();
                glib::ControlFlow::Continue
            });
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
            let turned = rotation == 90 || rotation == 270;
            let (pic_w, pic_h) = (texture.width() as f32, texture.height() as f32);
            let (shown_w, shown_h) = if turned {
                (pic_h, pic_w)
            } else {
                (pic_w, pic_h)
            };
            let scale = match self.fit.get() {
                Fit::Contain => (view_w / shown_w).min(view_h / shown_h),
                Fit::Cover => (view_w / shown_w).max(view_h / shown_h),
            };
            let (draw_w, draw_h) = (pic_w * scale, pic_h * scale);

            snapshot.push_clip(&graphene::Rect::new(0.0, 0.0, view_w, view_h));
            snapshot.save();
            snapshot.translate(&graphene::Point::new(view_w / 2.0, view_h / 2.0));
            if self.mirrored.get() {
                snapshot.scale(-1.0, 1.0);
            }
            if rotation != 0 {
                snapshot.rotate(rotation as f32);
            }
            snapshot.append_texture(
                &texture,
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
        fn take_frame(&self) {
            let Some(picture) = self.slot.take() else {
                return;
            };
            let texture = gdk::MemoryTexture::new(
                picture.width as i32,
                picture.height as i32,
                gdk::MemoryFormat::R8g8b8a8,
                &picture.rgba,
                picture.width as usize * 4,
            );
            self.texture.replace(Some(texture.upcast()));
            self.rotation.set(picture.rotation);
            self.drawn.set(self.drawn.get() + 1);
            self.obj().queue_draw();
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
        self.imp().fit.set(fit);
        self.queue_draw();
    }

    pub fn fit(&self) -> Fit {
        self.imp().fit.get()
    }

    /// Mirrored, as one's own camera is shown.
    pub fn set_mirrored(&self, mirrored: bool) {
        self.imp().mirrored.set(mirrored);
        self.queue_draw();
    }

    /// Whether a picture has arrived since the view was made or cleared.
    pub fn has_picture(&self) -> bool {
        self.imp().texture.borrow().is_some()
    }

    /// Frames put on screen.
    pub fn frames_drawn(&self) -> u64 {
        self.imp().drawn.get()
    }

    /// Forgets the picture shown, for a track that went away.
    pub fn clear(&self) {
        self.imp().texture.replace(None);
        self.queue_draw();
    }

    /// Called with the view's size in device pixels whenever it changes, for
    /// `call.request_video_size(sid, width, height)`.
    pub fn connect_size_wanted(&self, listener: impl Fn(&VideoView, u32, u32) + 'static) {
        self.imp().size_wanted.borrow_mut().push(Box::new(listener));
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
    fn odd_sizes_and_short_buffers() {
        assert!(i420_to_rgba(&plain(3, 3, 16, 128, 128), 3, 3).is_some());
        assert!(i420_to_rgba(&[0; 10], 4, 4).is_none());
        assert!(i420_to_rgba(&[], 0, 0).is_none());
    }

    #[test]
    fn the_slot_keeps_only_the_newest_frame() {
        let slot = FrameSlot::default();
        for y in [16, 235] {
            slot.on_frame(RemoteFrame {
                pixel_buffer: 0,
                width: 2,
                height: 2,
                rotation: 0,
                timestamp_us: 0,
                i420: Some(plain(2, 2, y, 128, 128)),
            });
        }
        assert_eq!(slot.arrived(), 2);
        let taken = slot.take().unwrap();
        assert_eq!(taken.rgba[0], 255, "the newer, white frame");
        assert!(slot.take().is_none(), "taken once");
    }
}
