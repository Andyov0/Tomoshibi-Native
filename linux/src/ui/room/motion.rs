//! How the call's pieces come and go: every one enters and leaves with a
//! transition (the owner's rule), as the Mac's `Theme.arrive`, `slideIn`
//! and `slideUp` bring them.
//!
//! A [`Motion`] wraps one piece and draws it part of the way between not
//! there (moved by an offset, a little smaller, transparent) and in place.
//! It is kit::Arrival's idea with a direction of its own per use: a tile
//! grows into its place, a panel slides in from the side, a popover rises
//! off its button, a toast comes up from the foot. libadwaita skips its
//! animations when `gtk-enable-animations` is off, which is how reduced
//! motion is respected here as everywhere else in the app.

use std::cell::{Cell, RefCell};

use adw::prelude::*;
use gtk::{glib, graphene, subclass::prelude::*};

use crate::theme::motion;

/// Where a piece comes from and goes back to.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Away {
    pub dx: f32,
    pub dy: f32,
    pub scale: f32,
}

impl Away {
    /// Grows into place (Theme.arrive: 0.97 and a fade).
    pub const GROW: Away = Away {
        dx: 0.0,
        dy: 0.0,
        scale: motion::ARRIVE_SCALE,
    };
    /// Fades, nothing more: a picture arriving inside its own tile.
    pub const FADE: Away = Away {
        dx: 0.0,
        dy: 0.0,
        scale: 1.0,
    };
    /// Rises a little off what it hangs from (a popover over the bar).
    pub const RISE: Away = Away {
        dx: 0.0,
        dy: 8.0,
        scale: motion::ARRIVE_SCALE,
    };
    /// In from the side, as a panel beside the call comes (the web's,
    /// ui-polish 2026-10-29).
    pub const SIDE: Away = Away {
        dx: 24.0,
        dy: 0.0,
        scale: 1.0,
    };
    /// Up from the foot.
    pub const FOOT: Away = Away {
        dx: 0.0,
        dy: 16.0,
        scale: 1.0,
    };
    /// Down from the top.
    pub const TOP: Away = Away {
        dx: 0.0,
        dy: -12.0,
        scale: 1.0,
    };
}

mod imp {
    use super::*;

    pub struct Motion {
        pub child: RefCell<Option<gtk::Widget>>,
        pub progress: Cell<f64>,
        pub from: Cell<super::Away>,
        pub shown: Cell<bool>,
        pub animation: RefCell<Option<adw::TimedAnimation>>,
    }

    impl Default for Motion {
        fn default() -> Self {
            Motion {
                child: RefCell::new(None),
                progress: Cell::new(1.0),
                from: Cell::new(super::Away::GROW),
                shown: Cell::new(true),
                animation: RefCell::new(None),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Motion {
        const NAME: &'static str = "TomoshibiRoomMotion";
        type Type = super::Motion;
        type ParentType = gtk::Widget;

        fn class_init(class: &mut Self::Class) {
            class.set_layout_manager_type::<gtk::BinLayout>();
        }
    }

    impl ObjectImpl for Motion {
        fn dispose(&self) {
            if let Some(animation) = self.animation.take() {
                animation.pause();
            }
            if let Some(child) = self.child.take() {
                child.unparent();
            }
        }
    }

    impl WidgetImpl for Motion {
        // A widget of our own does not take its child's expand flags by
        // itself (kit::Arrival found it first).
        fn compute_expand(&self, hexpand: &mut bool, vexpand: &mut bool) {
            let child = self.child.borrow();
            *hexpand = child
                .as_ref()
                .is_some_and(|child| child.compute_expand(gtk::Orientation::Horizontal));
            *vexpand = child
                .as_ref()
                .is_some_and(|child| child.compute_expand(gtk::Orientation::Vertical));
        }

        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            let progress = self.progress.get().clamp(0.0, 1.0) as f32;
            let Some(child) = self.child.borrow().clone() else {
                return;
            };
            if progress <= 0.0 {
                return;
            }
            let from = self.from.get();
            let widget = self.obj();
            let (width, height) = (widget.width() as f32, widget.height() as f32);
            let away = 1.0 - progress;
            let scale = from.scale + (1.0 - from.scale) * progress;
            snapshot.save();
            snapshot.translate(&graphene::Point::new(
                away * from.dx + width / 2.0,
                away * from.dy + height / 2.0,
            ));
            if scale != 1.0 {
                snapshot.scale(scale, scale);
            }
            snapshot.translate(&graphene::Point::new(-width / 2.0, -height / 2.0));
            snapshot.push_opacity(progress as f64);
            widget.snapshot_child(&child, snapshot);
            snapshot.pop();
            snapshot.restore();
        }
    }
}

glib::wrapper! {
    /// A piece of the call that arrives and leaves rather than appearing and
    /// vanishing. See the module's comment.
    pub struct Motion(ObjectSubclass<imp::Motion>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Motion {
    /// `child` wrapped, in place and showing.
    pub fn new(child: &impl IsA<gtk::Widget>, from: Away) -> Motion {
        let motion: Motion = glib::Object::new();
        child.as_ref().set_parent(&motion);
        motion.imp().child.replace(Some(child.as_ref().clone()));
        motion.imp().from.set(from);
        motion
    }

    /// `child` wrapped and not there yet: [`show`](Self::show) brings it.
    pub fn hidden(child: &impl IsA<gtk::Widget>, from: Away) -> Motion {
        let motion = Motion::new(child, from);
        motion.imp().progress.set(0.0);
        motion.imp().shown.set(false);
        motion.set_visible(false);
        motion
    }

    /// Where it comes from and goes back to from now on: a panel slides in
    /// from the side beside the call, and up from the foot over it.
    pub fn set_from(&self, from: Away) {
        self.imp().from.set(from);
    }

    pub fn child(&self) -> Option<gtk::Widget> {
        self.imp().child.borrow().clone()
    }

    pub fn is_shown(&self) -> bool {
        self.imp().shown.get()
    }

    /// Brings it in, or takes it away; nothing when it is already going
    /// that way. A piece that has gone is hidden, so that it takes neither
    /// room nor presses.
    pub fn show(&self, on: bool) {
        if self.imp().shown.replace(on) == on {
            return;
        }
        if on {
            self.set_visible(true);
            self.set_can_target(true);
            self.animate(1.0, motion::SETTLE_MS, adw::Easing::EaseOutCubic, None);
        } else {
            // No presses on a piece on its way out: a second press on a
            // panel's close button as it slid away pressed what was under.
            self.set_can_target(false);
            let weak = self.downgrade();
            self.animate(
                0.0,
                motion::LEAVE_MS,
                adw::Easing::EaseInCubic,
                Some(Box::new(move || {
                    if let Some(motion) = weak.upgrade() {
                        if !motion.is_shown() {
                            motion.set_visible(false);
                        }
                    }
                })),
            );
        }
    }

    /// Takes it away, then calls `gone` (to remove it).
    pub fn leave(&self, gone: impl FnOnce() + 'static) {
        self.imp().shown.set(false);
        self.set_can_target(false);
        self.animate(
            0.0,
            motion::LEAVE_MS,
            adw::Easing::EaseInCubic,
            Some(Box::new(gone)),
        );
    }

    fn animate(
        &self,
        to: f64,
        duration_ms: u32,
        easing: adw::Easing,
        then: Option<Box<dyn FnOnce()>>,
    ) {
        // One running is paused rather than skipped, and the next starts
        // where it had got to: skipping would finish it, and a leave cut
        // short by an arrival would call its `then` and remove what had just
        // been asked to come back (kit::Arrival's reasoning).
        if let Some(running) = self.imp().animation.take() {
            running.pause();
        }
        let from = self.imp().progress.get();
        if (from - to).abs() < f64::EPSILON {
            if let Some(then) = then {
                then();
            }
            return;
        }
        let target = adw::CallbackAnimationTarget::new(glib::clone!(
            #[weak(rename_to = motion)]
            self,
            move |value| {
                motion.imp().progress.set(value);
                motion.queue_draw();
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
