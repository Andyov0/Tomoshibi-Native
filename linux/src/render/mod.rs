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
//! A camera is framed as the Mac and Android frame it ([`framing_fills`],
//! TomoshibiBasics Framing.swift): it fills its place while the two shapes
//! are close, and is shown whole otherwise, on a blurred and darkened copy
//! of itself rather than between bars ([`backdrop_pixels`], the Mac's
//! PictureBackdrop). A change between the two is animated, the picture
//! growing to fill or shrinking to be whole as the copy fades.
//!
//! Owned by the render module.

use std::{
    cell::{Cell, RefCell},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
};

use adw::prelude::AnimationExt as _;
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
    /// Filling the view, cut where the shapes differ (the lobby's preview).
    Cover,
    /// A camera in the call: filling while the shapes are close, whole on a
    /// blurred copy of itself where filling would cut most of it away
    /// ([`framing_fills`]).
    Framed,
}

/// Whether a camera's picture, as shown (after its rotation), fills a view
/// of this size or is shown whole in it (TomoshibiBasics Framing.swift, and
/// Android's copy, whose cases the tests below repeat one for one).
///
/// The share of the picture a fill keeps along the side it cuts decides:
/// three quarters or more and it fills; half or more and it still fills
/// when the two are the same way round, since a tall picture in a tall
/// place loses only the room around a face; anything less is shown whole.
/// A laptop's landscape camera in an upright place kept a little over half
/// its width, and whoever sat beside the person was cut out of the call
/// (the owner's test on a phone, 2026-10-19). A size not yet known fills, as
/// every camera did before, so that a first frame does not jump between the
/// two before its shape has arrived.
pub fn framing_fills(picture: (f32, f32), view: (f32, f32)) -> bool {
    let ((pw, ph), (vw, vh)) = (picture, view);
    if pw <= 0.0 || ph <= 0.0 || vw <= 0.0 || vh <= 0.0 {
        return true;
    }
    let (a, b) = (pw / ph, vw / vh);
    let kept = a.min(b) / a.max(b);
    if kept >= 0.75 {
        return true;
    }
    // An exact square is neither way round.
    let same_way = (a > 1.0 && b > 1.0) || (a < 1.0 && b < 1.0);
    same_way && kept >= 0.5
}

/// The long side of a backdrop's picture, in pixels (PictureBackdrop
/// `side`): shrunk this far it is colour and light and nothing that could
/// be read as a face beside the face.
pub const BACKDROP_SIDE: u32 = 48;
/// The blur, in those pixels (PictureBackdrop `sigma`).
const BACKDROP_SIGMA: f32 = 4.5;
/// What a pixel keeps of its light: the picture under black at about 35 per
/// cent, so that the picture in front stays the brightest thing in its
/// place (PictureBackdrop `kept`).
const BACKDROP_KEPT: f32 = 0.65;
/// How often a backdrop is taken again from the picture it is behind, and
/// how long a new one takes to fade in over the last.
const BACKDROP_EVERY_MS: u64 = 1000;
const BACKDROP_CROSSFADE_MS: u32 = 600;

/// The backdrop of a picture in GDK's default layout (blue, green, red,
/// alpha bytes, `stride` a row): shrunk by averaging to [`BACKDROP_SIDE`] on
/// its long side, blurred, and darkened, in the same layout. Returns the
/// bytes and their width and height. Done once a second on a few thousand
/// pixels, so that nothing runs per frame and nothing is decoded twice: the
/// GPU's blur over a whole stage at thirty frames a second was the
/// alternative.
pub fn backdrop_pixels(
    bgra: &[u8],
    width: u32,
    height: u32,
    stride: usize,
) -> Option<(Vec<u8>, u32, u32)> {
    if width == 0 || height == 0 || bgra.len() < stride * (height as usize - 1) + width as usize * 4
    {
        return None;
    }
    let long = width.max(height) as f32;
    let shrink = (BACKDROP_SIDE as f32 / long).min(1.0);
    let (out_w, out_h) = (
        ((width as f32 * shrink).round() as u32).max(1),
        ((height as f32 * shrink).round() as u32).max(1),
    );
    // Averaged over the whole of each output pixel's area, so that a
    // picture shrunk thirty times is its colours and not a sample of them.
    let mut small = vec![0f32; (out_w * out_h * 3) as usize];
    for oy in 0..out_h {
        let y0 = (oy as u64 * height as u64 / out_h as u64) as usize;
        let y1 = (((oy + 1) as u64 * height as u64).div_ceil(out_h as u64) as usize).max(y0 + 1);
        for ox in 0..out_w {
            let x0 = (ox as u64 * width as u64 / out_w as u64) as usize;
            let x1 = (((ox + 1) as u64 * width as u64).div_ceil(out_w as u64) as usize).max(x0 + 1);
            let mut sum = [0u64; 3];
            for y in y0..y1.min(height as usize) {
                let row = &bgra[y * stride..];
                for x in x0..x1.min(width as usize) {
                    let at = x * 4;
                    sum[0] += row[at] as u64;
                    sum[1] += row[at + 1] as u64;
                    sum[2] += row[at + 2] as u64;
                }
            }
            let count =
                ((y1.min(height as usize) - y0) * (x1.min(width as usize) - x0)).max(1) as f32;
            let at = ((oy * out_w + ox) * 3) as usize;
            for channel in 0..3 {
                small[at + channel] = sum[channel] as f32 / count;
            }
        }
    }
    let blurred = gaussian(&small, out_w as usize, out_h as usize, BACKDROP_SIGMA);
    let mut out = vec![255u8; (out_w * out_h * 4) as usize];
    for index in 0..(out_w * out_h) as usize {
        for channel in 0..3 {
            out[index * 4 + channel] = (blurred[index * 3 + channel] * BACKDROP_KEPT)
                .round()
                .clamp(0.0, 255.0) as u8;
        }
    }
    Some((out, out_w, out_h))
}

/// A separable Gaussian blur of three-channel pixels, the edges held.
fn gaussian(pixels: &[f32], width: usize, height: usize, sigma: f32) -> Vec<f32> {
    let reach = (sigma * 3.0).ceil() as isize;
    let weights: Vec<f32> = (-reach..=reach)
        .map(|at| (-((at * at) as f32) / (2.0 * sigma * sigma)).exp())
        .collect();
    let total: f32 = weights.iter().sum();
    let pass = |from: &[f32], horizontal: bool| {
        let mut to = vec![0f32; from.len()];
        for y in 0..height {
            for x in 0..width {
                let mut sum = [0f32; 3];
                for (index, weight) in weights.iter().enumerate() {
                    let offset = index as isize - reach;
                    let (sx, sy) = if horizontal {
                        (
                            (x as isize + offset).clamp(0, width as isize - 1) as usize,
                            y,
                        )
                    } else {
                        (
                            x,
                            (y as isize + offset).clamp(0, height as isize - 1) as usize,
                        )
                    };
                    let at = (sy * width + sx) * 3;
                    for channel in 0..3 {
                        sum[channel] += from[at + channel] * weight;
                    }
                }
                let at = (y * width + x) * 3;
                for channel in 0..3 {
                    to[at + channel] = sum[channel] / total;
                }
            }
        }
        to
    };
    let across = pass(pixels, true);
    pass(&across, false)
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
        Fit::Framed if framing_fills(picture, view) => (vw / pw).max(vh / ph),
        Fit::Framed => (vw / pw).min(vh / ph),
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
        /// How far a framed camera is toward filling its place: 1 fills,
        /// 0 is whole on its backdrop, and between while it changes.
        pub filling: Cell<f64>,
        /// What the framing was last decided as, and whether the picture's
        /// shape was known then: the first decision made with it is taken
        /// at once, since a picture that arrives already shown whole should
        /// not first be seen shrinking to it.
        pub fills: Cell<Option<bool>>,
        pub shaped: Cell<bool>,
        pub framing: RefCell<Option<adw::TimedAnimation>>,
        /// The backdrop behind a camera shown whole, the one before it while
        /// a new one fades in, how far it has, and when it was taken.
        pub backdrop: RefCell<Option<gdk::Texture>>,
        pub backdrop_before: RefCell<Option<gdk::Texture>>,
        pub backdrop_fade: Cell<f64>,
        pub backdrop_at: Cell<Option<std::time::Instant>>,
        pub crossfade: RefCell<Option<adw::TimedAnimation>>,
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
            // Filling until decided otherwise, as every camera did before.
            self.filling.set(1.0);
            self.backdrop_fade.set(1.0);
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
            for animation in [self.framing.take(), self.crossfade.take()]
                .into_iter()
                .flatten()
            {
                animation.pause();
            }
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
            let scale = match self.fit.get() {
                // Between whole and filling by how far the framing has got,
                // so that a change between them is a picture growing or
                // shrinking rather than a jump.
                Fit::Framed => {
                    let whole = (view_w / shown_w).min(view_h / shown_h);
                    let fill = (view_w / shown_w).max(view_h / shown_h);
                    whole + (fill - whole) * self.filling.get().clamp(0.0, 1.0) as f32
                }
                fit => fitted((view_w, view_h), (shown_w, shown_h), fit).0 / shown_w,
            };
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
            // Behind a camera shown whole, its blurred copy filling the
            // place, faded out as the picture grows to fill it.
            let behind = 1.0 - self.filling.get().clamp(0.0, 1.0);
            if self.fit.get() == Fit::Framed && behind > 0.0 {
                let cover = (view_w / shown_w).max(view_h / shown_h);
                let (cover_w, cover_h) = (pic_w * cover, pic_h * cover);
                let fade = self.backdrop_fade.get().clamp(0.0, 1.0);
                let layers = [
                    (self.backdrop_before.borrow().clone(), behind * (1.0 - fade)),
                    (self.backdrop.borrow().clone(), behind * fade),
                ];
                for (layer, opacity) in layers {
                    let Some(layer) = layer else { continue };
                    if opacity <= 0.0 {
                        continue;
                    }
                    snapshot.push_opacity(opacity);
                    snapshot.save();
                    snapshot.translate(&graphene::Point::new(view_w / 2.0, view_h / 2.0));
                    if self.mirrored.get() {
                        snapshot.scale(-1.0, 1.0);
                    }
                    if rotation != 0 {
                        snapshot.rotate(rotation as f32);
                    }
                    snapshot.append_scaled_texture(
                        &layer,
                        gsk::ScalingFilter::Linear,
                        &graphene::Rect::new(-cover_w / 2.0, -cover_h / 2.0, cover_w, cover_h),
                    );
                    snapshot.restore();
                    snapshot.pop();
                }
            }
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
            self.decide();
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
            self.decide();
            self.take_backdrop();
            let view = self.obj();
            view.queue_draw();
            if first {
                self.tell_picture(true);
            }
        }

        /// Fills or shows whole, by the picture's shape and the view's,
        /// again whenever either changes: the sender turning their phone,
        /// the window being resized, the picture going to the stage.
        pub(super) fn decide(&self) {
            let view = self.obj();
            let framed = self.fit.get() == Fit::Framed;
            let picture = self.texture.borrow().as_ref().map(|texture| {
                turned_size(
                    texture.width() as u32,
                    texture.height() as u32,
                    self.rotation.get(),
                )
            });
            let size = (view.width() as f32, view.height() as f32);
            let fill = !framed
                || framing_fills(
                    picture.map_or((0.0, 0.0), |(w, h)| (w as f32, h as f32)),
                    size,
                );
            let known = picture.is_some() && size.0 > 0.0 && size.1 > 0.0;
            if self.fills.get() == Some(fill) && (!known || self.shaped.get()) {
                return;
            }
            let animated = self.fills.get().is_some() && self.shaped.get() && known;
            if known {
                self.shaped.set(true);
            }
            self.fills.set(Some(fill));
            let to = if fill { 1.0 } else { 0.0 };
            if let Some(running) = self.framing.take() {
                running.pause();
            }
            if !fill {
                // The copy is wanted now, not at the next second.
                self.backdrop_at.set(None);
                self.take_backdrop();
            }
            if !animated || !adw::is_animations_enabled(&*view) {
                self.filling.set(to);
                view.queue_draw();
                return;
            }
            let target = adw::CallbackAnimationTarget::new(glib::clone!(
                #[weak]
                view,
                move |value| {
                    view.imp().filling.set(value);
                    view.queue_draw();
                }
            ));
            let animation = adw::TimedAnimation::builder()
                .widget(&*view)
                .value_from(self.filling.get())
                .value_to(to)
                .duration(crate::theme::motion::SETTLE_MS)
                .easing(adw::Easing::EaseOutCubic)
                .target(&target)
                .build();
            self.framing.replace(Some(animation.clone()));
            animation.play();
        }

        /// A new backdrop from the picture showing, once a second while a
        /// camera is shown whole and never otherwise; it fades in over the
        /// last one.
        fn take_backdrop(&self) {
            if self.fit.get() != Fit::Framed || self.fills.get() != Some(false) {
                return;
            }
            let now = std::time::Instant::now();
            if self
                .backdrop_at
                .get()
                .is_some_and(|at| now.duration_since(at).as_millis() < BACKDROP_EVERY_MS as u128)
            {
                return;
            }
            let Some(texture) = self.texture.borrow().clone() else {
                return;
            };
            self.backdrop_at.set(Some(now));
            let (width, height) = (texture.width() as u32, texture.height() as u32);
            let stride = width as usize * 4;
            let mut bytes = vec![0u8; stride * height as usize];
            texture.download(&mut bytes, stride);
            let Some((pixels, w, h)) = backdrop_pixels(&bytes, width, height, stride) else {
                return;
            };
            let made: gdk::Texture = gdk::MemoryTexture::new(
                w as i32,
                h as i32,
                gdk::MemoryFormat::B8g8r8a8Premultiplied,
                &glib::Bytes::from_owned(pixels),
                w as usize * 4,
            )
            .upcast();
            let before = self.backdrop.replace(Some(made));
            let view = self.obj();
            if let Some(running) = self.crossfade.take() {
                running.pause();
            }
            if before.is_none() || !adw::is_animations_enabled(&*view) {
                self.backdrop_before.replace(None);
                self.backdrop_fade.set(1.0);
                view.queue_draw();
                return;
            }
            self.backdrop_before.replace(before);
            self.backdrop_fade.set(0.0);
            let target = adw::CallbackAnimationTarget::new(glib::clone!(
                #[weak]
                view,
                move |value| {
                    view.imp().backdrop_fade.set(value);
                    view.queue_draw();
                }
            ));
            let animation = adw::TimedAnimation::builder()
                .widget(&*view)
                .value_from(0.0)
                .value_to(1.0)
                .duration(BACKDROP_CROSSFADE_MS)
                .easing(adw::Easing::Linear)
                .target(&target)
                .build();
            self.crossfade.replace(Some(animation.clone()));
            animation.play();
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
            self.imp().decide();
            self.queue_draw();
        }
    }

    /// Whether a framed camera fills its place now (rather than being shown
    /// whole on its backdrop), once it has been decided; for tests.
    pub fn framed_fills(&self) -> Option<bool> {
        self.imp().fills.get()
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
        // The next picture is framed afresh, by its own shape, and is not
        // seen against the last one's backdrop.
        imp.shaped.set(false);
        imp.backdrop.replace(None);
        imp.backdrop_before.replace(None);
        imp.backdrop_at.set(None);
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
        assert_eq!(
            pixel(&taken.texture, 0, 0)[0],
            255,
            "the newer, white frame"
        );
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

    /// The Mac's FramingTests, case for case, and Android's with them: a
    /// case added there is added here.
    #[test]
    fn a_camera_fills_or_is_shown_whole_by_the_two_shapes() {
        type Case = ((f32, f32), (f32, f32), bool);
        let cases: [Case; 13] = [
            ((640.0, 480.0), (300.0, 400.0), false),
            ((640.0, 480.0), (1856.0, 2160.0), false),
            ((720.0, 960.0), (1856.0, 2160.0), true),
            ((720.0, 960.0), (968.0, 2376.0), true),
            ((640.0, 480.0), (968.0, 2376.0), false),
            ((1280.0, 720.0), (1440.0, 900.0), true),
            ((720.0, 960.0), (1440.0, 900.0), false),
            ((480.0, 480.0), (300.0, 400.0), true),
            ((1280.0, 720.0), (500.0, 500.0), false),
            ((720.0, 960.0), (1080.0, 2340.0), true),
            ((720.0, 1280.0), (968.0, 2376.0), true),
            ((640.0, 480.0), (1600.0, 900.0), true),
            ((720.0, 1280.0), (400.0, 1600.0), false),
        ];
        for (picture, view, fills) in cases {
            assert_eq!(
                framing_fills(picture, view),
                fills,
                "{picture:?} in {view:?}"
            );
        }
    }

    #[test]
    fn an_unknown_size_fills() {
        assert!(framing_fills((0.0, 0.0), (300.0, 400.0)));
        assert!(framing_fills((640.0, 480.0), (0.0, 0.0)));
        assert!(framing_fills((640.0, 0.0), (300.0, 400.0)));
        assert!(framing_fills((640.0, 480.0), (300.0, 0.0)));
    }

    #[test]
    fn an_upright_phone_in_a_wide_tile_is_drawn_whole() {
        // A phone held upright (720 by 1280) on the stage of a wide window:
        // filling it kept less than a third of the picture.
        let (w, h) = fitted((1200.0, 675.0), (720.0, 1280.0), Fit::Framed);
        assert_eq!(h.round(), 675.0);
        assert!(w < 1200.0);
        // And a laptop's camera in the same tile fills it.
        let (w, _) = fitted((1200.0, 675.0), (1280.0, 720.0), Fit::Framed);
        assert_eq!(w.round(), 1200.0);
    }

    #[test]
    fn the_backdrop_is_small_dark_and_the_picture_s_colour() {
        // A plain red picture, 640 by 360, in GDK's order.
        let (width, height) = (640u32, 360u32);
        let mut bgra = Vec::new();
        for _ in 0..width * height {
            bgra.extend_from_slice(&[0, 0, 200, 255]);
        }
        let (pixels, w, h) = backdrop_pixels(&bgra, width, height, width as usize * 4).unwrap();
        assert_eq!((w, h), (48, 27));
        assert_eq!(pixels.len(), (w * h * 4) as usize);
        // Darkened to about two thirds, and still red all over: a blur of
        // one colour is that colour.
        for pixel in pixels.chunks(4) {
            assert_eq!(pixel, &[0, 0, 130, 255]);
        }
        assert!(backdrop_pixels(&[0; 10], 640, 360, 2560).is_none());
        assert!(backdrop_pixels(&[], 0, 0, 0).is_none());
    }

    #[test]
    fn the_backdrop_blurs_an_edge_into_a_gradient() {
        // Left half black, right half white: blurred, the middle is grey
        // and the ends keep their own.
        let (width, height) = (96u32, 48u32);
        let mut bgra = Vec::new();
        for _ in 0..height {
            for x in 0..width {
                let v = if x < width / 2 { 0 } else { 255 };
                bgra.extend_from_slice(&[v, v, v, 255]);
            }
        }
        let (pixels, w, _) = backdrop_pixels(&bgra, width, height, width as usize * 4).unwrap();
        let at = |x: u32| pixels[(x * 4) as usize];
        assert!(at(0) < 10, "{}", at(0));
        assert!(at(w - 1) > 155, "{}", at(w - 1));
        let middle = at(w / 2);
        assert!(middle > 40 && middle < 130, "{middle}");
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
