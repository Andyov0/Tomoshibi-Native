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
        // Expands as its child does. Without this a wrapped column that asks
        // to fill the height and centre itself (the unreachable screen's)
        // was given only its own height at the top of the window: a widget
        // of our own does not take its child's expand flags by itself.
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

// ---- menus drawn in the window ------------------------------------------------

/// Which side of what it hangs from a [`Menu`] opens on, when there is room
/// there; it opens on the other side when there is more room there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuSide {
    Below,
    Above,
}

/// How a [`Menu`] lines up with what it hangs from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuAlign {
    /// Its left edge on the anchor's.
    Start,
    /// Its right edge on the anchor's (a menu off a button in a corner).
    End,
    /// As wide as the anchor (the suggestions under a field).
    Fill,
}

/// How far a menu opens from what it hangs from, and from the window's
/// edges.
const MENU_GAP: f32 = 6.0;
const MENU_MARGIN: i32 = 8;
/// How far a menu travels as it arrives and leaves, towards its anchor.
const MENU_TRAVEL: f32 = 8.0;

mod menu_imp {
    use super::*;

    pub struct Menu {
        /// What is drawn: the menu's own ground, edge and shadow are this
        /// box's, not the menu's, because GTK draws a widget's CSS ground
        /// before and outside its snapshot, where the fade could not reach
        /// it (the card stood there at full strength, empty, for a frame).
        pub frame: gtk::Box,
        pub scroller: gtk::ScrolledWindow,
        pub column: gtk::Box,
        pub progress: Cell<f64>,
        pub animation: RefCell<Option<adw::TimedAnimation>>,
        pub anchor: glib::WeakRef<gtk::Widget>,
        pub side: Cell<MenuSide>,
        pub align: Cell<MenuAlign>,
        /// Where it was put the last time, to arrive from that side.
        pub above: Cell<bool>,
        pub open: Cell<bool>,
        /// A modal menu takes the keyboard, and a press outside it closes it
        /// and goes no further, as a system menu's does; a field's
        /// suggestions leave the keyboard in the field and let the press
        /// through.
        pub modal: Cell<bool>,
        pub keys: RefCell<Option<(glib::WeakRef<gtk::Window>, gtk::EventControllerKey)>>,
        pub follow: RefCell<Option<gtk::TickCallbackId>>,
        pub anchor_bounds: Cell<Option<(f32, f32, f32, f32)>>,
        pub returns_focus_to: glib::WeakRef<gtk::Widget>,
        pub opened: RefCell<Vec<Box<dyn Fn()>>>,
    }

    impl Default for Menu {
        fn default() -> Self {
            let column = gtk::Box::new(gtk::Orientation::Vertical, 2);
            let scroller = gtk::ScrolledWindow::builder()
                .hscrollbar_policy(gtk::PolicyType::Never)
                .propagate_natural_height(true)
                .propagate_natural_width(true)
                .child(&column)
                .build();
            let frame = gtk::Box::new(gtk::Orientation::Vertical, 0);
            frame.add_css_class("tmsb-menu");
            frame.append(&scroller);
            Menu {
                frame,
                scroller,
                column,
                progress: Cell::new(0.0),
                animation: RefCell::new(None),
                anchor: glib::WeakRef::new(),
                side: Cell::new(MenuSide::Below),
                align: Cell::new(MenuAlign::Start),
                above: Cell::new(false),
                open: Cell::new(false),
                modal: Cell::new(true),
                keys: RefCell::new(None),
                follow: RefCell::new(None),
                anchor_bounds: Cell::new(None),
                returns_focus_to: glib::WeakRef::new(),
                opened: RefCell::new(Vec::new()),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Menu {
        const NAME: &'static str = "TomoshibiMenu";
        type Type = super::Menu;
        type ParentType = gtk::Widget;

        fn class_init(class: &mut Self::Class) {
            class.set_layout_manager_type::<gtk::BinLayout>();
            class.set_css_name("menu-card");
            class.set_accessible_role(gtk::AccessibleRole::Menu);
        }
    }

    impl ObjectImpl for Menu {
        fn constructed(&self) {
            self.parent_constructed();
            self.frame.set_parent(&*self.obj());
        }

        fn dispose(&self) {
            self.obj().stop_listening();
            self.frame.unparent();
        }
    }

    impl WidgetImpl for Menu {
        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            let progress = self.progress.get().clamp(0.0, 1.0) as f32;
            if progress <= 0.0 {
                return;
            }
            // Towards the anchor while it is not all there: a menu below its
            // button comes down off it, one above rises off it.
            let toward = if self.above.get() { 1.0 } else { -1.0 };
            snapshot.save();
            snapshot.translate(&graphene::Point::new(
                0.0,
                toward * (1.0 - progress) * MENU_TRAVEL,
            ));
            snapshot.push_opacity(progress as f64);
            self.parent_snapshot(snapshot);
            snapshot.pop();
            snapshot.restore();
        }
    }
}

glib::wrapper! {
    /// A menu drawn inside the window rather than in a surface of its own:
    /// it fades in and slides a few points off what it hangs from as it
    /// opens, and back as it closes (GtkPopover appears and vanishes in one
    /// frame, and GTK 4 gives it no transition to ask for), and it is kept
    /// inside the window, where a compositor that does not move popups
    /// clear of the screen's edge left a popover's right half off screen.
    /// It lives in the nearest [`menu_layer`] above its anchor.
    pub struct Menu(ObjectSubclass<menu_imp::Menu>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Menu {
    /// A menu `width` wide hanging from `anchor`. Its rows go in
    /// [`column`](Self::column).
    pub fn new(anchor: &impl IsA<gtk::Widget>, width: i32, side: MenuSide, align: MenuAlign) -> Menu {
        let menu: Menu = glib::Object::new();
        let imp = menu.imp();
        imp.column.set_width_request(width);
        imp.anchor.set(Some(anchor.as_ref()));
        imp.side.set(side);
        imp.align.set(align);
        menu.set_visible(false);
        // Held by the anchor, which is what keeps it alive until it is first
        // opened into a layer (the menu holds the anchor only weakly), and
        // so that whoever has the anchor can open it (the debug switch that
        // opens a menu for a picture).
        // SAFETY: the key is only ever set here, with this type.
        unsafe {
            anchor.as_ref().set_data("tmsb-menu", menu.clone());
        }
        // The menu goes with what it hangs from.
        let weak = menu.downgrade();
        anchor.as_ref().connect_destroy(move |_| {
            if let Some(menu) = weak.upgrade() {
                menu.stop_listening();
                if let Some(layer) = menu.parent().and_then(|p| p.downcast::<gtk::Overlay>().ok()) {
                    layer.remove_overlay(&menu);
                }
            }
        });
        menu
    }

    /// The menu kept on `anchor` by [`Menu::new`], if there is one.
    pub fn of(anchor: &impl IsA<gtk::Widget>) -> Option<Menu> {
        // SAFETY: set in `new` with this type.
        unsafe {
            anchor
                .as_ref()
                .data::<Menu>("tmsb-menu")
                .map(|menu| menu.as_ref().clone())
        }
    }

    pub fn column(&self) -> gtk::Box {
        self.imp().column.clone()
    }

    /// Whether a press outside closes it and goes no further, and whether
    /// it takes the keyboard (true unless said otherwise).
    pub fn set_modal(&self, modal: bool) {
        self.imp().modal.set(modal);
    }

    pub fn is_open(&self) -> bool {
        self.imp().open.get()
    }

    /// Called each time it opens (the server menu measures then).
    pub fn connect_opened(&self, opened: impl Fn() + 'static) {
        self.imp().opened.borrow_mut().push(Box::new(opened));
    }

    pub fn toggle(&self) {
        if self.is_open() {
            self.popdown();
        } else {
            self.popup();
        }
    }

    pub fn popup(&self) {
        let imp = self.imp();
        let Some(anchor) = imp.anchor.upgrade() else {
            return;
        };
        let Some(layer) = layer_for(&anchor) else {
            crate::app::log("a menu has no layer to open in");
            return;
        };
        if self.parent().as_ref() != Some(layer.upcast_ref()) {
            if let Some(old) = self.parent().and_then(|p| p.downcast::<gtk::Overlay>().ok()) {
                old.remove_overlay(self);
            }
            layer.add_overlay(self);
        }
        let was_open = imp.open.replace(true);
        let was_shown = self.is_visible();
        self.set_visible(true);
        self.set_can_target(true);
        anchor.add_css_class("open");
        anchor.update_state(&[gtk::accessible::State::Expanded(Some(true))]);
        if was_shown {
            self.animate(1.0, motion::SETTLE_MS, settle());
        } else {
            // Its clock starts once it has been laid out and drawn once, at
            // nothing: the animation is timed, and the first frame of a menu
            // just shown (its rows measured, its styles worked out) took
            // 180 ms in a debug build drawn in software, so the first frame
            // anybody saw was already 92 per cent of the way and the menu
            // seemed to appear at once.
            let frames = Cell::new(0);
            self.add_tick_callback(move |menu, _| {
                frames.set(frames.get() + 1);
                if frames.get() < 2 {
                    return glib::ControlFlow::Continue;
                }
                if menu.is_open() {
                    menu.animate(1.0, motion::SETTLE_MS, settle());
                }
                glib::ControlFlow::Break
            });
        }
        if was_open {
            return;
        }
        self.listen(&anchor);
        for opened in imp.opened.borrow().iter() {
            opened();
        }
        if imp.modal.get() {
            let root = anchor.root();
            let focus = root.as_ref().and_then(|root| root.focus());
            imp.returns_focus_to.set(focus.as_ref());
            // The first row, so that the arrows and Enter work at once, as
            // a menu opened from the keyboard does.
            self.child_focus(gtk::DirectionType::TabForward);
        }
    }

    pub fn popdown(&self) {
        let imp = self.imp();
        if !imp.open.replace(false) {
            return;
        }
        self.stop_listening();
        if let Some(anchor) = imp.anchor.upgrade() {
            anchor.remove_css_class("open");
            anchor.update_state(&[gtk::accessible::State::Expanded(Some(false))]);
        }
        // The keyboard goes back where it was, where it was in the menu.
        let focus_inside = self
            .root()
            .and_then(|root| root.focus())
            .is_some_and(|focus| focus.is_ancestor(self));
        if focus_inside {
            if let Some(back) = imp
                .returns_focus_to
                .upgrade()
                .or_else(|| imp.anchor.upgrade())
            {
                back.grab_focus();
            }
        }
        self.set_can_target(false);
        let weak = self.downgrade();
        self.animate_then(0.0, motion::LEAVE_MS, adw::Easing::EaseInCubic, move || {
            if let Some(menu) = weak.upgrade() {
                if !menu.is_open() {
                    menu.set_visible(false);
                }
            }
        });
    }

    fn animate(&self, to: f64, duration_ms: u32, easing: adw::Easing) {
        self.animate_then(to, duration_ms, easing, || {});
    }

    fn animate_then(&self, to: f64, duration_ms: u32, easing: adw::Easing, then: impl FnOnce() + 'static) {
        let imp = self.imp();
        // Paused rather than skipped, as Arrival's: a close interrupted by
        // an open goes on from where it had got to.
        if let Some(running) = imp.animation.take() {
            running.pause();
        }
        let target = adw::CallbackAnimationTarget::new(glib::clone!(
            #[weak(rename_to = menu)]
            self,
            move |value| {
                menu.imp().progress.set(value);
                menu.queue_draw();
            }
        ));
        let animation = adw::TimedAnimation::builder()
            .widget(self)
            .value_from(imp.progress.get())
            .value_to(to)
            .duration(duration_ms)
            .easing(easing)
            .target(&target)
            .build();
        let then = RefCell::new(Some(then));
        animation.connect_done(move |_| {
            if let Some(then) = then.take() {
                then();
            }
        });
        imp.animation.replace(Some(animation.clone()));
        animation.play();
    }

    /// Escape closes it from anywhere in the window, and it follows its
    /// anchor while it is open (an anchor in a list that scrolls).
    fn listen(&self, anchor: &gtk::Widget) {
        let imp = self.imp();
        if let Some(window) = anchor.root().and_then(|root| root.downcast::<gtk::Window>().ok()) {
            let keys = gtk::EventControllerKey::new();
            keys.set_propagation_phase(gtk::PropagationPhase::Capture);
            let weak = self.downgrade();
            keys.connect_key_pressed(move |_, key, _, _| {
                if key != gtk::gdk::Key::Escape {
                    return glib::Propagation::Proceed;
                }
                match weak.upgrade() {
                    Some(menu) if menu.is_open() => {
                        menu.popdown();
                        glib::Propagation::Stop
                    }
                    _ => glib::Propagation::Proceed,
                }
            });
            window.add_controller(keys.clone());
            let held = glib::WeakRef::new();
            held.set(Some(&window));
            imp.keys.replace(Some((held, keys)));
        }
        imp.anchor_bounds.set(None);
        let id = self.add_tick_callback(|menu, _| {
            let imp = menu.imp();
            let (Some(anchor), Some(layer)) = (
                imp.anchor.upgrade(),
                menu.parent().and_then(|p| p.downcast::<gtk::Overlay>().ok()),
            ) else {
                return glib::ControlFlow::Continue;
            };
            // An anchor taken off the screen (its page went) takes the menu.
            if !anchor.is_mapped() {
                menu.popdown();
                return glib::ControlFlow::Continue;
            }
            let bounds = anchor
                .compute_bounds(&layer)
                .map(|b| (b.x(), b.y(), b.width(), b.height()));
            if bounds != imp.anchor_bounds.get() {
                imp.anchor_bounds.set(bounds);
                layer.queue_allocate();
            }
            glib::ControlFlow::Continue
        });
        imp.follow.replace(Some(id));
    }

    fn stop_listening(&self) {
        let imp = self.imp();
        if let Some((window, keys)) = imp.keys.take() {
            if let Some(window) = window.upgrade() {
                window.remove_controller(&keys);
            }
        }
        if let Some(id) = imp.follow.take() {
            id.remove();
        }
    }

    /// Where it goes in `layer`: on its side of the anchor where it fits,
    /// on the other where that has more room, as tall as there is room for
    /// (its rows scroll beyond that), and inside the window's edges.
    fn place(&self, layer: &gtk::Overlay) -> Option<gtk::gdk::Rectangle> {
        let imp = self.imp();
        let anchor = imp.anchor.upgrade()?.compute_bounds(layer)?;
        let (minimum, natural) = self.preferred_size();
        let (whole_width, whole_height) = (layer.width(), layer.height());
        let width = match imp.align.get() {
            MenuAlign::Fill => (anchor.width().round() as i32).max(minimum.width()),
            _ => natural.width(),
        }
        .min(whole_width - 2 * MENU_MARGIN)
        .max(minimum.width());
        let below_room =
            whole_height - (anchor.y() + anchor.height() + MENU_GAP).round() as i32 - MENU_MARGIN;
        let above_room = (anchor.y() - MENU_GAP).round() as i32 - MENU_MARGIN;
        let wanted = natural.height();
        let above = match imp.side.get() {
            MenuSide::Below => wanted > below_room && above_room > below_room,
            MenuSide::Above => !(wanted > above_room && below_room > above_room),
        };
        imp.above.set(above);
        let room = if above { above_room } else { below_room };
        let height = wanted.min(room).max(minimum.height());
        let x = match imp.align.get() {
            MenuAlign::Start | MenuAlign::Fill => anchor.x(),
            MenuAlign::End => anchor.x() + anchor.width() - width as f32,
        };
        let x = (x.round() as i32).clamp(MENU_MARGIN, (whole_width - width - MENU_MARGIN).max(MENU_MARGIN));
        let y = if above {
            (anchor.y() - MENU_GAP).round() as i32 - height
        } else {
            (anchor.y() + anchor.height() + MENU_GAP).round() as i32
        };
        Some(gtk::gdk::Rectangle::new(x, y.max(0), width, height))
    }
}

/// The layer [`Menu`]s open in: `child` with room over it. A screen with a
/// menu wraps itself in one, and so does a dialog with one, since a dialog
/// is drawn over the screen's layer.
pub fn menu_layer(child: &impl IsA<gtk::Widget>) -> gtk::Overlay {
    let layer = gtk::Overlay::new();
    layer.set_child(Some(child));
    layer.add_css_class("tmsb-menu-layer");
    layer.connect_get_child_position(|layer, child| child.downcast_ref::<Menu>()?.place(layer));
    // A press outside an open menu closes it; a modal menu's press goes no
    // further, so that the press that put the menu away does not also press
    // whatever was under it.
    let press = gtk::GestureClick::new();
    press.set_button(0);
    press.set_propagation_phase(gtk::PropagationPhase::Capture);
    press.connect_pressed(|gesture, _, x, y| {
        let Some(layer) = gesture.widget().and_downcast::<gtk::Overlay>() else {
            return;
        };
        let point = graphene::Point::new(x as f32, y as f32);
        let inside = |widget: &gtk::Widget| {
            widget
                .compute_bounds(&layer)
                .is_some_and(|bounds| bounds.contains_point(&point))
        };
        let mut child = layer.first_child();
        while let Some(widget) = child {
            child = widget.next_sibling();
            let Ok(menu) = widget.downcast::<Menu>() else {
                continue;
            };
            if !menu.is_open() || inside(menu.upcast_ref()) {
                continue;
            }
            // A press on the anchor is the anchor's: it closes the menu
            // by toggling it.
            if menu.imp().anchor.upgrade().is_some_and(|anchor| inside(&anchor)) {
                continue;
            }
            menu.popdown();
            if menu.imp().modal.get() {
                gesture.set_state(gtk::EventSequenceState::Claimed);
            }
        }
    });
    layer.add_controller(press);
    layer
}

/// The nearest menu layer holding `widget`.
fn layer_for(widget: &gtk::Widget) -> Option<gtk::Overlay> {
    let mut parent = widget.parent();
    while let Some(next) = parent {
        if next.has_css_class("tmsb-menu-layer") {
            return next.downcast().ok();
        }
        parent = next.parent();
    }
    None
}

/// A button that opens `menu`'s kind of menu: pressed, it opens or closes
/// it, and it reads as open while the menu is (the `.open` class).
pub fn menu_button(child: &impl IsA<gtk::Widget>) -> gtk::Button {
    let button = gtk::Button::builder().child(child).build();
    button.update_property(&[gtk::accessible::Property::HasPopup(true)]);
    button.update_state(&[gtk::accessible::State::Expanded(Some(false))]);
    button.connect_clicked(|button| {
        if let Some(menu) = Menu::of(button) {
            menu.toggle();
        }
    });
    button
}

/// Shows or hides `widget` with a fade rather than at once, for a widget
/// that cannot be put in a revealer (a row of a preferences list is the
/// list's own child): fading in from wherever it is, or fading out and then
/// hidden. Skipped when animations are off, as every timed animation is.
pub fn fade_visible(widget: &impl IsA<gtk::Widget>, shown: bool) {
    let widget = widget.as_ref();
    // SAFETY: the key is only ever set here, with this type.
    let running = unsafe { widget.steal_data::<adw::TimedAnimation>("tmsb-fade") };
    if let Some(running) = running {
        running.pause();
    }
    let visible = widget.is_visible();
    if shown == visible && (!shown || widget.opacity() >= 1.0) {
        return;
    }
    if shown && !visible {
        widget.set_opacity(0.0);
        widget.set_visible(true);
    }
    let target = adw::PropertyAnimationTarget::new(widget, "opacity");
    let animation = adw::TimedAnimation::builder()
        .widget(widget)
        .value_from(widget.opacity())
        .value_to(if shown { 1.0 } else { 0.0 })
        .duration(if shown { motion::SETTLE_MS } else { motion::LEAVE_MS })
        .easing(if shown {
            settle()
        } else {
            adw::Easing::EaseInCubic
        })
        .target(&target)
        .build();
    if !shown {
        let weak = widget.downgrade();
        animation.connect_done(move |animation| {
            if let Some(widget) = weak.upgrade() {
                if animation.value() <= 0.0 {
                    widget.set_visible(false);
                    widget.set_opacity(1.0);
                }
            }
        });
    }
    // SAFETY: read back above with the same type.
    unsafe {
        widget.set_data("tmsb-fade", animation.clone());
    }
    animation.play();
}
