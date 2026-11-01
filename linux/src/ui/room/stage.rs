//! The pictures' space: every tile placed by coordinates (plan.rs), moved
//! between places by an animation, never taken out and put back.
//!
//! A tile is a child of this one widget for as long as its picture is drawn,
//! whatever the arrangement: grid, stage and strip, a call of two, the
//! screen filled. A change of arrangement moves each tile from where it is
//! drawn now to its new place over the settle time; a change of the
//! window's size puts them straight where they belong, since a layout that
//! trails the window's edge as it is dragged reads as lag. A tile that
//! arrives grows into its place, and one that goes fades where it was.
//!
//! The larger picture is drawn under the smaller (the card in a call of two
//! over the person filling the screen), and since GTK draws and picks in
//! child order, the children are kept in that order: the last child is the
//! one on top, for drawing and for the pointer alike.

use std::cell::{Cell, RefCell};

use adw::prelude::*;
use gtk::{glib, subclass::prelude::*};

use super::{
    motion::Motion,
    plan::{Rect, Spots},
};
use crate::theme::motion;

pub struct Placed {
    pub id: String,
    pub widget: Motion,
    pub from: Rect,
    pub to: Rect,
    pub leaving: bool,
}

type Planner = Box<dyn Fn(f64, f64) -> Spots>;

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct Stage {
        pub placed: RefCell<Vec<Placed>>,
        pub progress: Cell<f64>,
        pub animation: RefCell<Option<adw::TimedAnimation>>,
        pub size: Cell<(i32, i32)>,
        pub planner: RefCell<Option<Planner>>,
        pub resized: RefCell<Option<Box<dyn Fn()>>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Stage {
        const NAME: &'static str = "TomoshibiRoomStage";
        type Type = super::Stage;
        type ParentType = gtk::Widget;
    }

    impl ObjectImpl for Stage {
        fn constructed(&self) {
            self.parent_constructed();
            self.progress.set(1.0);
            let stage = self.obj();
            stage.set_hexpand(true);
            stage.set_vexpand(true);
            stage.set_overflow(gtk::Overflow::Hidden);
        }

        fn dispose(&self) {
            if let Some(animation) = self.animation.take() {
                animation.pause();
            }
            for placed in self.placed.take() {
                placed.widget.unparent();
            }
        }
    }

    impl WidgetImpl for Stage {
        fn measure(&self, _: gtk::Orientation, _: i32) -> (i32, i32, i32, i32) {
            (0, 0, -1, -1)
        }

        fn compute_expand(&self, hexpand: &mut bool, vexpand: &mut bool) {
            *hexpand = true;
            *vexpand = true;
        }

        fn size_allocate(&self, width: i32, height: i32, _baseline: i32) {
            if self.size.replace((width, height)) != (width, height) {
                // A new size: straight to the new places.
                let spots = self
                    .planner
                    .borrow()
                    .as_ref()
                    .map(|plan| plan(width as f64, height as f64));
                if let Some(spots) = spots {
                    if let Some(animation) = self.animation.take() {
                        animation.pause();
                    }
                    self.progress.set(1.0);
                    for placed in self.placed.borrow_mut().iter_mut() {
                        if placed.leaving {
                            continue;
                        }
                        if let Some(spot) = spots.get(&placed.id) {
                            placed.to = *spot;
                        }
                        placed.from = placed.to;
                    }
                }
                // What is shown may change with the size (how many the strip
                // holds, how many pages), which is decided outside the
                // allocation: the room is told after it.
                let stage = self.obj().downgrade();
                glib::idle_add_local_once(move || {
                    if let Some(stage) = stage.upgrade() {
                        if let Some(resized) = stage.imp().resized.borrow().as_ref() {
                            resized();
                        }
                    }
                });
            }
            let progress = self.progress.get();
            for placed in self.placed.borrow().iter() {
                let at = placed.from.toward(&placed.to, progress);
                let widget = placed.widget.upcast_ref::<gtk::Widget>();
                // Measured before it is given its place, as GTK asks.
                let _ = widget.measure(gtk::Orientation::Horizontal, -1);
                let (x, y) = (at.x.round() as i32, at.y.round() as i32);
                let (w, h) = (
                    (at.max_x().round() as i32 - x).max(0),
                    (at.max_y().round() as i32 - y).max(0),
                );
                widget.size_allocate(&gtk::Allocation::new(x, y, w, h), -1);
            }
        }
    }
}

glib::wrapper! {
    /// The call's pictures. See the module's comment.
    pub struct Stage(ObjectSubclass<imp::Stage>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Default for Stage {
    fn default() -> Self {
        Self::new()
    }
}

impl Stage {
    pub fn new() -> Stage {
        glib::Object::new()
    }

    /// Where the tiles go at a size: asked whenever the stage is given a new
    /// one.
    pub fn set_planner(&self, plan: impl Fn(f64, f64) -> Spots + 'static) {
        self.imp().planner.replace(Some(Box::new(plan)));
    }

    /// Called (from the main loop, after the allocation) when the size
    /// changed.
    pub fn connect_resized(&self, resized: impl Fn() + 'static) {
        self.imp().resized.replace(Some(Box::new(resized)));
    }

    pub fn size(&self) -> (f64, f64) {
        let (width, height) = self.imp().size.get();
        (width as f64, height as f64)
    }

    pub fn has(&self, id: &str) -> bool {
        self.imp()
            .placed
            .borrow()
            .iter()
            .any(|placed| placed.id == id && !placed.leaving)
    }

    pub fn ids(&self) -> Vec<String> {
        self.imp()
            .placed
            .borrow()
            .iter()
            .filter(|placed| !placed.leaving)
            .map(|placed| placed.id.clone())
            .collect()
    }

    /// The place a tile is drawn at now.
    pub fn spot(&self, id: &str) -> Option<Rect> {
        let progress = self.imp().progress.get();
        self.imp()
            .placed
            .borrow()
            .iter()
            .find(|placed| placed.id == id && !placed.leaving)
            .map(|placed| placed.from.toward(&placed.to, progress))
    }

    /// Adds a tile at `spot`, growing into it.
    pub fn add(&self, id: &str, widget: &Motion, spot: Rect) {
        // One still fading out under the same id goes at once: the picture
        // came back, and two of it would be drawn.
        self.drop_leaving(id);
        widget.set_parent(self);
        self.imp().placed.borrow_mut().push(Placed {
            id: id.to_owned(),
            widget: widget.clone(),
            from: spot,
            to: spot,
            leaving: false,
        });
        widget.show(true);
        self.queue_allocate();
    }

    /// Takes away at once a tile of `id` still fading out. Off the list
    /// before it is unparented: unparenting unmaps it, libadwaita skips an
    /// animation whose widget is unmapped, and the skipped fade's end (the
    /// `remove` below) then ran while GTK was still inside this unparent,
    /// unparented the same widget a second time from within the first, and
    /// the app died in gtk_widget_unparent. That was a call losing its
    /// default route: everybody left and came back within the fade.
    fn drop_leaving(&self, id: &str) {
        let mut gone = Vec::new();
        self.imp().placed.borrow_mut().retain(|placed| {
            let keep = !(placed.leaving && placed.id == id);
            if !keep {
                gone.push(placed.widget.clone());
            }
            keep
        });
        for widget in gone {
            if widget.parent().as_ref() == Some(self.upcast_ref()) {
                widget.unparent();
            }
        }
    }

    /// Fades a tile out where it is, then takes it away.
    pub fn remove(&self, id: &str) {
        let widget = {
            let mut placed = self.imp().placed.borrow_mut();
            let Some(found) = placed
                .iter_mut()
                .find(|placed| placed.id == id && !placed.leaving)
            else {
                return;
            };
            found.leaving = true;
            // It stays where it is drawn now while it fades.
            let progress = self.imp().progress.get();
            found.from = found.from.toward(&found.to, progress);
            found.to = found.from;
            found.widget.clone()
        };
        let (stage, gone) = (self.downgrade(), widget.downgrade());
        widget.leave(move || {
            let (Some(stage), Some(gone)) = (stage.upgrade(), gone.upgrade()) else {
                return;
            };
            // Only a tile still on the list is this fade's to take away. One
            // already taken (the same picture coming back, the stage going)
            // is being unparented by whoever took it, and this can be called
            // from inside that unparent (`drop_leaving` says how).
            let was_here = {
                let mut placed = stage.imp().placed.borrow_mut();
                let before = placed.len();
                placed.retain(|placed| placed.widget != gone);
                placed.len() != before
            };
            if was_here && gone.parent().as_ref() == Some(stage.upcast_ref()) {
                gone.unparent();
            }
        });
    }

    /// Sends every tile to its place in `spots`: moved there over the settle
    /// time when `animate`, put there otherwise. Tiles `spots` does not name
    /// stay where they are (they are on their way out).
    pub fn set_spots(&self, spots: &Spots, animate: bool) {
        let imp = self.imp();
        let progress = imp.progress.get();
        let moved = imp.placed.borrow().iter().any(|placed| {
            !placed.leaving && spots.get(&placed.id).is_some_and(|spot| *spot != placed.to)
        });
        // Nothing going anywhere new: a move under way is left to finish.
        // The room is brought up to date on every event of the call, and
        // each time it was put at its end: the call's picture leaving the
        // stage for a show was in the strip one frame after it set off,
        // since the show's start brought the next update a frame later.
        if !moved {
            self.restack();
            return;
        }
        for placed in imp.placed.borrow_mut().iter_mut() {
            if placed.leaving {
                continue;
            }
            let Some(spot) = spots.get(&placed.id) else {
                continue;
            };
            let now = placed.from.toward(&placed.to, progress);
            placed.from = if animate { now } else { *spot };
            placed.to = *spot;
        }
        self.restack();
        if let Some(animation) = imp.animation.take() {
            animation.pause();
        }
        if moved && animate && adw::is_animations_enabled(self) {
            imp.progress.set(0.0);
            let target = adw::CallbackAnimationTarget::new(glib::clone!(
                #[weak(rename_to = stage)]
                self,
                move |value| {
                    stage.imp().progress.set(value);
                    stage.queue_allocate();
                }
            ));
            let animation = adw::TimedAnimation::builder()
                .widget(self)
                .value_from(0.0)
                .value_to(1.0)
                .duration(motion::SETTLE_MS)
                .easing(adw::Easing::EaseOutCubic)
                .target(&target)
                .build();
            imp.animation.replace(Some(animation.clone()));
            animation.play();
        } else {
            imp.progress.set(1.0);
            for placed in imp.placed.borrow_mut().iter_mut() {
                placed.from = placed.to;
            }
        }
        // Only a move lays the window out again: the room is brought up to
        // date on every event of the call (a level, a reading every two
        // seconds), and a layout each time was a window always waiting for
        // one.
        self.queue_allocate();
    }

    /// The larger picture under the smaller, in drawing and in picking.
    fn restack(&self) {
        let mut order: Vec<(f64, bool, Motion)> = self
            .imp()
            .placed
            .borrow()
            .iter()
            .map(|placed| (placed.to.area(), placed.leaving, placed.widget.clone()))
            .collect();
        // Leaving ones lowest, then by area, largest first.
        order.sort_by(|a, b| {
            b.1.cmp(&a.1)
                .then(b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal))
        });
        for (_, _, widget) in order {
            widget.insert_before(self, None::<&gtk::Widget>);
        }
    }

    /// Which tile the point is on, the one on top first.
    pub fn tile_at(&self, x: f64, y: f64) -> Option<String> {
        let progress = self.imp().progress.get();
        let placed = self.imp().placed.borrow();
        let mut hits: Vec<&Placed> = placed
            .iter()
            .filter(|placed| !placed.leaving)
            .filter(|placed| {
                let at = placed.from.toward(&placed.to, progress);
                x >= at.x && x < at.max_x() && y >= at.y && y < at.max_y()
            })
            .collect();
        hits.sort_by(|a, b| {
            a.to.area()
                .partial_cmp(&b.to.area())
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        hits.first().map(|placed| placed.id.clone())
    }
}
