//! The pieces every screen is made of, so that a button looks and moves the
//! same wherever it is: the mark, the primary and plain buttons, and the one
//! way things arrive and leave.
//!
//! Every element enters and leaves with a transition (the owner's rule;
//! `open &&`-style appearing and vanishing is what is not wanted). An
//! [`Arrival`] wraps anything that comes and goes on its own; a whole screen
//! changes through the window's stack, which crossfades. Both are skipped
//! when `gtk-enable-animations` is off, which is how reduced motion is
//! respected: libadwaita's animations and GTK's transitions both read it.
//!
//! Shared by the screens. Add to it, at the end, rather than changing what a
//! piece already does: other screens use it.

use std::cell::{Cell, RefCell};

use adw::prelude::*;
use gtk::{glib, graphene, subclass::prelude::*};

use crate::theme::{motion, scale};

/// The product's mark: the four tiles of a call, one of them lit (Windows'
/// Pieces.cs `Mark`, on a 32-unit grid).
pub fn mark(size: i32) -> gtk::DrawingArea {
    let area = gtk::DrawingArea::builder()
        .content_width(size)
        .content_height(size)
        .halign(gtk::Align::Center)
        .can_target(false)
        .build();
    area.update_property(&[gtk::accessible::Property::Label("Tomoshibi")]);
    area.set_draw_func(|_, cr, width, height| {
        let size = width.min(height) as f64;
        let unit = size / 32.0;
        rounded(cr, 0.0, 0.0, size, size, 8.0 * unit);
        cr.set_source_rgb(
            0x0F as f64 / 255.0,
            0x0E as f64 / 255.0,
            0x0D as f64 / 255.0,
        );
        let _ = cr.fill();
        let inner = unit * 0.86;
        let offset = (size - 32.0 * inner) / 2.0;
        for (index, (x, y)) in [(6.0, 8.0), (17.0, 8.0), (6.0, 17.0), (17.0, 17.0)]
            .into_iter()
            .enumerate()
        {
            rounded(
                cr,
                offset + x * inner,
                offset + y * inner,
                9.0 * inner,
                7.0 * inner,
                2.0 * inner,
            );
            if index == 3 {
                cr.set_source_rgb(1.0, 0xA2 as f64 / 255.0, 0x3B as f64 / 255.0);
            } else {
                cr.set_source_rgb(
                    0x55 as f64 / 255.0,
                    0x52 as f64 / 255.0,
                    0x4E as f64 / 255.0,
                );
            }
            let _ = cr.fill();
        }
    });
    area
}

fn rounded(cr: &gtk::cairo::Context, x: f64, y: f64, w: f64, h: f64, r: f64) {
    use std::f64::consts::{FRAC_PI_2, PI};
    cr.new_sub_path();
    cr.arc(x + w - r, y + r, r, -FRAC_PI_2, 0.0);
    cr.arc(x + w - r, y + h - r, r, 0.0, FRAC_PI_2);
    cr.arc(x + r, y + h - r, r, FRAC_PI_2, PI);
    cr.arc(x + r, y + r, r, PI, 3.0 * FRAC_PI_2);
    cr.close_path();
}

/// The primary action: amber with dark ink, a neutral grey plate with grey
/// words when it cannot be pressed (UI-SYNC decision 4), 44 high.
pub fn primary_button(label: &str) -> gtk::Button {
    let button = gtk::Button::with_label(label);
    button.add_css_class("tmsb-primary");
    button.set_height_request(scale::CONTROL_PRIMARY);
    button
}

/// A secondary action: a quiet plate, 36 high.
pub fn plain_button(label: &str) -> gtk::Button {
    let button = gtk::Button::with_label(label);
    button.add_css_class("tmsb-plain");
    button.set_height_request(scale::CONTROL_LARGE);
    button
}

/// Words in the muted colour.
pub fn muted(text: &str) -> gtk::Label {
    let label = gtk::Label::builder()
        .label(text)
        .wrap(true)
        .justify(gtk::Justification::Center)
        .build();
    label.add_css_class("tmsb-muted");
    label
}

/// A screen's heading.
pub fn title(text: &str) -> gtk::Label {
    let label = gtk::Label::builder()
        .label(text)
        .wrap(true)
        .justify(gtk::Justification::Center)
        .build();
    label.add_css_class("tmsb-title");
    label
}

/// A hairline.
pub fn divider() -> gtk::Box {
    let line = gtk::Box::builder()
        .height_request(1)
        .valign(gtk::Align::Center)
        .hexpand(true)
        .build();
    line.add_css_class("tmsb-divider");
    line
}

/// The curve arrivals settle on.
fn settle() -> adw::Easing {
    // libadwaita has no cubic-bezier easing to give the exact (0.2, 0.9,
    // 0.3, 1); ease-out-cubic is the closest of its curves, decelerating
    // into place as the web's --ease-settle does.
    let _ = motion::SETTLE_CURVE;
    adw::Easing::EaseOutCubic
}

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct Arrival {
        pub child: RefCell<Option<gtk::Widget>>,
        /// From nought (not there) to one (in place).
        pub progress: Cell<f64>,
        pub lift: Cell<f32>,
        pub animation: RefCell<Option<adw::TimedAnimation>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Arrival {
        const NAME: &'static str = "TomoshibiArrival";
        type Type = super::Arrival;
        type ParentType = gtk::Widget;

        fn class_init(class: &mut Self::Class) {
            class.set_layout_manager_type::<gtk::BinLayout>();
        }
    }

    impl ObjectImpl for Arrival {
        fn constructed(&self) {
            self.parent_constructed();
            self.progress.set(1.0);
            self.lift.set(motion::RISE_LIFT);
        }

        fn dispose(&self) {
            if let Some(child) = self.child.take() {
                child.unparent();
            }
        }
    }

    impl WidgetImpl for Arrival {
        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            let progress = self.progress.get().clamp(0.0, 1.0) as f32;
            let Some(child) = self.child.borrow().clone() else {
                return;
            };
            if progress <= 0.0 {
                return;
            }
            snapshot.save();
            snapshot.translate(&graphene::Point::new(
                0.0,
                (1.0 - progress) * self.lift.get(),
            ));
            snapshot.push_opacity(progress as f64);
            self.obj().snapshot_child(&child, snapshot);
            snapshot.pop();
            snapshot.restore();
        }
    }
}

glib::wrapper! {
    /// Something that arrives (rising fourteen points into place and fading
    /// up, as the screens before a call bring their pieces in) and leaves
    /// (fading out and dropping back) rather than appearing or vanishing.
    pub struct Arrival(ObjectSubclass<imp::Arrival>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Arrival {
    /// `child` wrapped, in place and showing.
    pub fn new(child: &impl IsA<gtk::Widget>) -> Arrival {
        let arrival: Arrival = glib::Object::new();
        child.as_ref().set_parent(&arrival);
        arrival.imp().child.replace(Some(child.as_ref().clone()));
        arrival
    }

    /// Hidden now, rising into place after `delay_ms`.
    pub fn rises(self, delay_ms: u32) -> Arrival {
        self.imp().progress.set(0.0);
        let arrival = self.clone();
        glib::timeout_add_local_once(
            std::time::Duration::from_millis(delay_ms as u64),
            move || {
                arrival.animate(1.0, motion::RISE_MS, settle(), None);
            },
        );
        self
    }

    /// Fades out and drops back over the leaving time, then calls `gone`
    /// (to remove it, or hide it).
    pub fn leave(&self, gone: impl FnOnce() + 'static) {
        self.animate(
            0.0,
            motion::LEAVE_MS,
            adw::Easing::EaseInCubic,
            Some(Box::new(gone)),
        );
    }

    /// Comes back from having left.
    pub fn arrive(&self) {
        self.animate(1.0, motion::SETTLE_MS, settle(), None);
    }

    fn animate(
        &self,
        to: f64,
        duration_ms: u32,
        easing: adw::Easing,
        then: Option<Box<dyn FnOnce()>>,
    ) {
        // One that is running is paused, not skipped, and the new one starts
        // from wherever it had got to. Skipping it would finish it at once,
        // and a leave interrupted by an arrival would call its `gone` (and
        // remove what had just been asked to arrive).
        if let Some(running) = self.imp().animation.take() {
            running.pause();
        }
        let from = self.imp().progress.get();
        let target = adw::CallbackAnimationTarget::new(glib::clone!(
            #[weak(rename_to = arrival)]
            self,
            move |value| {
                arrival.imp().progress.set(value);
                arrival.queue_draw();
            }
        ));
        let animation = adw::TimedAnimation::builder()
            .widget(self)
            .value_from(from)
            .value_to(to)
            .duration(duration_ms)
            .easing(easing)
            .target(&target)
            .build();
        if let Some(then) = then {
            let then = RefCell::new(Some(then));
            animation.connect_done(move |_| {
                if let Some(then) = then.take() {
                    then();
                }
            });
        }
        self.imp().animation.replace(Some(animation.clone()));
        animation.play();
    }
}

/// Gives a window the app's ground and words, which follow the system's day
/// and night through the stylesheet (theme.rs); the main window has it, and
/// any other window (a settings window, a dialog's own) adds it.
pub fn follow_appearance(window: &impl IsA<gtk::Window>) {
    window.as_ref().add_css_class("tmsb-window");
}

/// The bar across the top of a screen: no title and no fill, only the
/// window's own buttons, so that the screen reaches the top of the window as
/// the Mac's hidden title bar lets it. Every screen puts one at its head
/// (the window has none of its own, so that a screen can put buttons in
/// it), and it is where the window is dragged from.
pub fn bar() -> adw::HeaderBar {
    let bar = adw::HeaderBar::builder().show_title(false).build();
    bar.add_css_class("tmsb-bar");
    bar.add_css_class("flat");
    bar
}
