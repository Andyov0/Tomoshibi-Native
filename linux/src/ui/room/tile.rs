//! The frame every picture appears in (the Mac's TileFrame, Tile.tsx): a
//! camera, a shared screen and a person with their camera off sit in the
//! same box with the same marks, so the grid never jumps as cameras go on
//! and off. The person's initials are under the picture, and the picture
//! fades in over them when its first frame arrives and out when it goes.
//!
//! What is on it: the name plate at the foot (a capsule about 24 high, 8
//! from the edge, black at seven tenths, the name always white, the muted
//! microphone white beside it; UI-SYNC decision 5), a raised hand at the top
//! left, the tally round a speaker, reactions rising off it, and on the
//! stage the button that fills the screen with it, shown under the pointer
//! (36 drawn, scale::STAGE_BUTTON).
//!
//! No mark saying that a press makes it larger: the owner had it taken away
//! on every platform (2026-10-26), the whole picture being what is pressed
//! (the stage, stage.rs, takes the press).

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use adw::prelude::*;
use gtk::{glib, graphene, gsk, pango, subclass::prelude::*};

use super::{
    glyph::{self, Glyph},
    motion::{Away, Motion},
    talk::PictureKind,
};
use crate::{
    i18n::t,
    render::{Fit, VideoView},
    theme::scale,
};

/// What a picture says about whoever is in it.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Marks {
    pub label: String,
    /// Whose it is, for the colour behind their initials.
    pub identity: String,
    /// A name somebody proved with a passphrase.
    pub proven: bool,
    /// Wearing a name somebody else in the room proved.
    pub unverified: bool,
    pub speaking: bool,
    /// Their microphone is off, or they have none.
    pub muted: bool,
    /// Turned all the way down by whoever is looking.
    pub silenced: bool,
    pub hand: bool,
    /// Small: a filmstrip's or the card in a call of two.
    pub compact: bool,
    /// Laid over another picture (the card in a call of two).
    pub floating: bool,
    /// Filling the screen: square edges on a black ground.
    pub filled: bool,
    /// The fill button: offered (`Some`), and whether it is the one that
    /// leaves the full screen (`Some(true)`).
    pub fill_button: Option<bool>,
    pub kind: Option<PictureKind>,
    /// One's own camera, drawn as a mirror is.
    pub mirrored: bool,
    /// No name drawn, for a picture that names itself.
    pub unlabelled: bool,
    /// The hand drawn at the start of the name rather than at the top left,
    /// where the room's own name and signal would cover it.
    pub hand_beside_name: bool,
    /// One's own shared screen, which has no picture here: a face saying
    /// so, with the way to stop (the Mac's MyShareFace).
    pub my_share: bool,
    /// How far the name plate is lifted off the foot: a picture that runs
    /// under the bar keeps its name above it.
    pub lift: i32,
}

/// The six warm colours behind initials, close to the palette in
/// web/src/live/avatar.ts (the Mac's `Avatar`), steady for one identity.
const AVATAR_COLOURS: [(f32, f32, f32); 6] = [
    (0.55, 0.33, 0.24),
    (0.42, 0.36, 0.27),
    (0.33, 0.38, 0.33),
    (0.48, 0.30, 0.33),
    (0.36, 0.33, 0.40),
    (0.50, 0.42, 0.25),
];

/// Which of the colours an identity has: FNV-1a over its bytes, as the Mac
/// hashes it, so that one person is one colour on both.
pub fn avatar_colour(identity: &str) -> usize {
    let mut hash: u64 = 1469598103934665603;
    for byte in identity.bytes() {
        hash = (hash ^ byte as u64).wrapping_mul(1099511628211);
    }
    (hash % AVATAR_COLOURS.len() as u64) as usize
}

/// The first letters of the first two words, upper case; a question mark
/// for no name.
pub fn initials(name: &str) -> String {
    let letters: String = name
        .split_whitespace()
        .take(2)
        .filter_map(|word| word.chars().next())
        .collect();
    if letters.is_empty() {
        "?".to_owned()
    } else {
        letters.to_uppercase()
    }
}

mod avatar {
    use super::*;

    #[derive(Default)]
    pub struct Avatar {
        pub name: RefCell<String>,
        pub identity: RefCell<String>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Avatar {
        const NAME: &'static str = "TomoshibiRoomAvatar";
        type Type = super::Avatar;
        type ParentType = gtk::Widget;
    }

    impl ObjectImpl for Avatar {}

    impl WidgetImpl for Avatar {
        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            let widget = self.obj();
            let (width, height) = (widget.width() as f32, widget.height() as f32);
            let side = width.min(height) * 0.36;
            if side < 2.0 {
                return;
            }
            let (red, green, blue) = AVATAR_COLOURS[avatar_colour(&self.identity.borrow())];
            let circle =
                graphene::Rect::new((width - side) / 2.0, (height - side) / 2.0, side, side);
            snapshot.push_rounded_clip(&gsk::RoundedRect::from_rect(circle, side / 2.0));
            snapshot.append_color(&gtk::gdk::RGBA::new(red, green, blue, 1.0), &circle);
            snapshot.pop();

            let layout = widget.create_pango_layout(Some(&initials(&self.name.borrow())));
            let mut font = pango::FontDescription::new();
            font.set_absolute_size((side * 0.38) as f64 * pango::SCALE as f64);
            font.set_weight(pango::Weight::Semibold);
            layout.set_font_description(Some(&font));
            let (text_w, text_h) = layout.pixel_size();
            snapshot.save();
            snapshot.translate(&graphene::Point::new(
                (width - text_w as f32) / 2.0,
                (height - text_h as f32) / 2.0,
            ));
            snapshot.append_layout(&layout, &widget.color());
            snapshot.restore();
        }
    }
}

glib::wrapper! {
    /// A person with no picture: their initials on a colour of their own.
    pub struct Avatar(ObjectSubclass<avatar::Avatar>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Avatar {
    fn new() -> Avatar {
        let avatar: Avatar = glib::Object::new();
        avatar.set_hexpand(true);
        avatar.set_vexpand(true);
        avatar.set_can_target(false);
        avatar
    }

    fn set(&self, name: &str, identity: &str) {
        let imp = self.imp();
        if *imp.name.borrow() != name || *imp.identity.borrow() != identity {
            imp.name.replace(name.to_owned());
            imp.identity.replace(identity.to_owned());
            self.queue_draw();
        }
    }
}

/// One picture's frame and what is on it.
pub struct Tile {
    pub id: String,
    pub root: gtk::Overlay,
    video: VideoView,
    video_motion: Motion,
    avatar: Avatar,
    edge: gtk::Box,
    ring: gtk::Box,
    plate: gtk::Box,
    plate_motion: Motion,
    name: gtk::Label,
    proven: gtk::Box,
    unverified: gtk::Label,
    mic_off: gtk::Image,
    silenced: gtk::Image,
    hand: gtk::Box,
    hand_motion: Motion,
    small_hand_motion: Motion,
    fill_button: gtk::Button,
    fill_motion: Motion,
    share_face: Motion,
    share_stop: gtk::Button,
    hovering: Cell<bool>,
    marks: RefCell<Marks>,
    /// The track whose frames the view is attached to, if any.
    pub attached: RefCell<Option<String>>,
    on_fill: RefCell<Option<Rc<dyn Fn()>>>,
    on_stop: RefCell<Option<Rc<dyn Fn()>>>,
    floats: Cell<usize>,
}

impl Tile {
    pub fn new(id: &str) -> Rc<Tile> {
        let root = gtk::Overlay::builder()
            .name(format!("tile:{id}"))
            .overflow(gtk::Overflow::Hidden)
            .build();
        root.add_css_class("tmsb-tile");

        let avatar = Avatar::new();
        root.set_child(Some(&avatar));

        let video = VideoView::new();
        let video_motion = Motion::hidden(&video, Away::FADE);
        video_motion.set_can_target(false);
        root.add_overlay(&video_motion);
        root.set_measure_overlay(&video_motion, false);

        let edge = gtk::Box::builder().can_target(false).build();
        edge.add_css_class("tmsb-tile-edge");
        root.add_overlay(&edge);
        let ring = gtk::Box::builder().can_target(false).build();
        ring.add_css_class("tmsb-ring");
        root.add_overlay(&ring);

        let name = gtk::Label::builder()
            .ellipsize(pango::EllipsizeMode::End)
            .single_line_mode(true)
            .build();
        let proven = gtk::Box::builder()
            .valign(gtk::Align::Center)
            .visible(false)
            .tooltip_text(t("Only this person can use this name"))
            .build();
        proven.add_css_class("tmsb-proven");
        let unverified = gtk::Label::builder()
            .label(t("unverified"))
            .visible(false)
            .valign(gtk::Align::Center)
            .tooltip_text(t("Someone else has proved this name"))
            .build();
        unverified.add_css_class("tmsb-unverified");
        let mic_off = gtk::Image::from_icon_name("microphone-disabled-symbolic");
        mic_off.set_visible(false);
        mic_off.update_property(&[gtk::accessible::Property::Label(&t("microphone off"))]);
        let silenced = gtk::Image::from_icon_name("audio-volume-muted-symbolic");
        silenced.add_css_class("silenced");
        silenced.set_visible(false);
        silenced.set_tooltip_text(Some(&t("Muted by you")));
        let plate = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(6)
            .valign(gtk::Align::Center)
            .build();
        plate.add_css_class("tmsb-plate");
        let small_hand = gtk::Box::builder().valign(gtk::Align::Center).build();
        small_hand.add_css_class("tmsb-hand");
        small_hand.add_css_class("small");
        small_hand.append(&Glyph::new(glyph::HAND, 10));
        let small_hand_motion = Motion::hidden(&small_hand, Away::GROW);
        small_hand_motion.set_margin_start(-6);
        plate.append(&small_hand_motion);
        plate.append(&name);
        plate.append(&proven);
        plate.append(&unverified);
        plate.append(&silenced);
        plate.append(&mic_off);
        let plate_motion = Motion::new(&plate, Away::FADE);
        plate_motion.set_halign(gtk::Align::Start);
        plate_motion.set_valign(gtk::Align::End);
        plate_motion.set_margin_start(scale::NAME_PLATE_INSET);
        plate_motion.set_margin_bottom(scale::NAME_PLATE_INSET);
        plate_motion.set_margin_end(scale::NAME_PLATE_INSET);
        plate_motion.set_can_target(false);
        root.add_overlay(&plate_motion);

        let hand = gtk::Box::builder()
            .halign(gtk::Align::Center)
            .valign(gtk::Align::Center)
            .build();
        hand.add_css_class("tmsb-hand");
        let hand_glyph = Glyph::new(glyph::HAND, 15);
        hand_glyph.set_hexpand(true);
        hand.append(&hand_glyph);
        hand.update_property(&[gtk::accessible::Property::Label("\u{270B}")]);
        let hand_motion = Motion::hidden(&hand, Away::GROW);
        hand_motion.set_halign(gtk::Align::Start);
        hand_motion.set_valign(gtk::Align::Start);
        hand_motion.set_margin_start(8);
        hand_motion.set_margin_top(8);
        hand_motion.set_can_target(false);
        root.add_overlay(&hand_motion);

        let fill_button = gtk::Button::from_icon_name("view-fullscreen-symbolic");
        fill_button.add_css_class("tmsb-stage-button");
        fill_button.set_widget_name("tile.fill");
        let fill_motion = Motion::hidden(&fill_button, Away::FADE);
        fill_motion.set_halign(gtk::Align::End);
        fill_motion.set_valign(gtk::Align::End);
        fill_motion.set_margin_end(10);
        fill_motion.set_margin_bottom(10);
        root.add_overlay(&fill_motion);

        // One's own share: the screen's symbol on the tally's tint, the
        // sentence, and Stop where there is room for it.
        let face = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(8)
            .halign(gtk::Align::Center)
            .valign(gtk::Align::Center)
            .build();
        face.add_css_class("tmsb-share-face");
        let mark = gtk::Image::from_icon_name("screen-shared-symbolic");
        mark.add_css_class("tmsb-share-mark");
        mark.set_halign(gtk::Align::Center);
        let said = gtk::Label::builder()
            .label(t("You are sharing your screen"))
            .wrap(true)
            .justify(gtk::Justification::Center)
            .build();
        said.add_css_class("tmsb-share-said");
        let share_stop = gtk::Button::with_label(&t("Stop sharing"));
        share_stop.add_css_class("tmsb-panel-button");
        share_stop.set_halign(gtk::Align::Center);
        share_stop.set_widget_name("tile.stop-sharing");
        face.append(&mark);
        face.append(&said);
        face.append(&share_stop);
        let share_face = Motion::hidden(&face, Away::FADE);
        root.add_overlay(&share_face);

        let tile = Rc::new(Tile {
            id: id.to_owned(),
            root,
            video,
            video_motion,
            avatar,
            edge,
            ring,
            plate,
            plate_motion,
            name,
            proven,
            unverified,
            mic_off,
            silenced,
            hand,
            hand_motion,
            small_hand_motion,
            fill_button,
            fill_motion,
            share_face,
            share_stop,
            hovering: Cell::new(false),
            marks: RefCell::new(Marks::default()),
            attached: RefCell::new(None),
            on_fill: RefCell::new(None),
            on_stop: RefCell::new(None),
            floats: Cell::new(0),
        });

        // The picture fades in over the initials when its first frame comes,
        // and out again when it is cleared, rather than cutting.
        let motion = tile.video_motion.downgrade();
        tile.video.connect_picture_changed(move |_, has| {
            if let Some(motion) = motion.upgrade() {
                motion.show(has);
            }
        });

        let hover = gtk::EventControllerMotion::new();
        let weak = Rc::downgrade(&tile);
        hover.connect_enter(move |_, _, _| {
            if let Some(tile) = weak.upgrade() {
                tile.hovering.set(true);
                tile.show_buttons();
            }
        });
        let weak = Rc::downgrade(&tile);
        hover.connect_leave(move |_| {
            if let Some(tile) = weak.upgrade() {
                tile.hovering.set(false);
                tile.show_buttons();
            }
        });
        tile.root.add_controller(hover);

        let weak = Rc::downgrade(&tile);
        tile.share_stop.connect_clicked(move |_| {
            let stop = weak
                .upgrade()
                .and_then(|tile| tile.on_stop.borrow().clone());
            if let Some(stop) = stop {
                stop();
            }
        });
        let weak = Rc::downgrade(&tile);
        tile.fill_button.connect_clicked(move |_| {
            let fill = weak
                .upgrade()
                .and_then(|tile| tile.on_fill.borrow().clone());
            if let Some(fill) = fill {
                fill();
            }
        });
        tile
    }

    pub fn video(&self) -> &VideoView {
        &self.video
    }

    pub fn marks(&self) -> Marks {
        self.marks.borrow().clone()
    }

    /// What the fill button does.
    /// What the face's Stop does.
    pub fn on_stop_sharing(&self, stop: impl Fn() + 'static) {
        self.on_stop.replace(Some(Rc::new(stop)));
    }

    pub fn on_fill(&self, fill: impl Fn() + 'static) {
        self.on_fill.replace(Some(Rc::new(fill)));
    }

    /// Shows what is true of the picture now; only what changed is touched.
    pub fn set_marks(&self, marks: Marks) {
        if *self.marks.borrow() == marks {
            return;
        }
        let root = &self.root;
        let toggle = |widget: &gtk::Widget, class: &str, on: bool| {
            if on {
                widget.add_css_class(class);
            } else {
                widget.remove_css_class(class);
            }
        };
        self.avatar.set(&marks.label, &marks.identity);
        self.name.set_label(&marks.label);
        self.proven.set_visible(marks.proven);
        self.unverified.set_visible(marks.unverified);
        self.silenced.set_visible(marks.silenced);
        self.mic_off.set_visible(marks.muted && !marks.silenced);
        self.plate_motion.show(!marks.unlabelled);
        toggle(self.plate.upcast_ref(), "compact", marks.compact);
        toggle(self.hand.upcast_ref(), "compact", marks.compact);
        let inset = if marks.compact {
            4
        } else {
            scale::NAME_PLATE_INSET
        };
        self.plate_motion.set_margin_start(inset);
        self.plate_motion.set_margin_bottom(inset + marks.lift);
        self.hand_motion
            .set_margin_start(if marks.compact { 5 } else { 8 });
        self.hand_motion
            .set_margin_top(if marks.compact { 5 } else { 8 });
        self.hand_motion.show(marks.hand && !marks.hand_beside_name);
        self.share_face.show(marks.my_share);
        self.avatar.set_visible(!marks.my_share);
        self.share_stop.set_visible(!marks.compact);
        toggle(root.upcast_ref(), "mine-shared", marks.my_share);
        self.small_hand_motion
            .show(marks.hand && marks.hand_beside_name);
        toggle(self.ring.upcast_ref(), "on", marks.speaking);
        toggle(self.edge.upcast_ref(), "floating", marks.floating);
        toggle(root.upcast_ref(), "filled", marks.filled);
        self.video.set_fit(match marks.kind {
            Some(PictureKind::Screen) => Fit::Contain,
            _ => Fit::Cover,
        });
        self.video.set_mirrored(marks.mirrored);
        let label = if marks.fill_button == Some(true) {
            t("Leave fullscreen")
        } else {
            t("Fill the screen")
        };
        self.fill_button
            .set_icon_name(if marks.fill_button == Some(true) {
                "view-restore-symbolic"
            } else {
                "view-fullscreen-symbolic"
            });
        self.fill_button.set_tooltip_text(Some(&label));
        self.fill_button
            .update_property(&[gtk::accessible::Property::Label(&label)]);
        root.update_property(&[gtk::accessible::Property::Label(&marks.label)]);
        self.marks.replace(marks);
        self.show_buttons();
    }

    fn show_buttons(&self) {
        let offered = self.marks.borrow().fill_button.is_some();
        self.fill_motion.show(offered && self.hovering.get());
    }

    /// A reaction rising off the picture (Reacting.tsx): low on the picture,
    /// centred, each new one beside the last rather than on top of it,
    /// rising and gone in the web's four seconds.
    pub fn float(&self, what: &str) {
        let compact = self.marks.borrow().compact;
        let label = gtk::Label::builder()
            .label(what)
            .halign(gtk::Align::Center)
            .valign(gtk::Align::End)
            .can_target(false)
            .build();
        label.add_css_class("tmsb-float");
        if compact {
            label.add_css_class("compact");
        }
        // Reacting.tsx: `(at % 3) * 28 - 28` from the middle, so that two of
        // the same at once are two things. A centred child moves by half the
        // difference of its margins.
        let shift = (self.floats.get() % 3) as i32 * 28 - 28;
        self.floats.set(self.floats.get() + 1);
        label.set_margin_start((2 * shift).max(0));
        label.set_margin_end((-2 * shift).max(0));
        let base = if compact { 12 } else { 40 } + self.marks.borrow().lift;
        label.set_margin_bottom(base);
        self.root.add_overlay(&label);
        let rise = if compact { 40.0 } else { 90.0 };
        let shown = tomoshibi_protocol::hands::reaction_shown_for_ms() as u32;
        let target = adw::CallbackAnimationTarget::new(glib::clone!(
            #[weak]
            label,
            move |value| {
                label.set_margin_bottom(base + (value * rise) as i32);
                // In over the first tenth, out over the last third.
                let opacity = (value / 0.1).min((1.0 - value) / 0.33).clamp(0.0, 1.0);
                label.set_opacity(opacity);
            }
        ));
        let animation = adw::TimedAnimation::builder()
            .widget(&label)
            .value_from(0.0)
            .value_to(1.0)
            .duration(shown)
            .easing(adw::Easing::EaseOutCubic)
            .target(&target)
            .build();
        let (root, weak_label) = (self.root.downgrade(), label.downgrade());
        animation.connect_done(move |_| {
            if let (Some(root), Some(label)) = (root.upgrade(), weak_label.upgrade()) {
                root.remove_overlay(&label);
            }
        });
        // An animation that is skipped (reduced motion) ends at once, and
        // the reaction would never be seen; it is held its four seconds by a
        // timer then, still.
        if adw::is_animations_enabled(&label) {
            animation.play();
        } else {
            label.set_opacity(1.0);
            let (root, label) = (self.root.downgrade(), label.downgrade());
            glib::timeout_add_local_once(
                std::time::Duration::from_millis(shown as u64),
                move || {
                    if let (Some(root), Some(label)) = (root.upgrade(), label.upgrade()) {
                        root.remove_overlay(&label);
                    }
                },
            );
        }
    }

    /// The speaking ring's own timing, for a test to read.
    pub fn speaking(&self) -> bool {
        self.ring.has_css_class("on")
    }

    pub fn hand_shown(&self) -> bool {
        self.hand_motion.is_shown()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initials_are_the_first_two_words() {
        assert_eq!(initials("ann lee smith"), "AL");
        assert_eq!(initials("  "), "?");
        assert_eq!(initials("Web Peer"), "WP");
        assert_eq!(initials("灯"), "灯");
    }

    #[test]
    fn one_identity_is_one_colour() {
        assert_eq!(avatar_colour("tabc-1"), avatar_colour("tabc-1"));
        let spread: std::collections::HashSet<usize> =
            (0..40).map(|n| avatar_colour(&format!("g{n}"))).collect();
        assert!(spread.len() > 3, "the colours are spread: {spread:?}");
    }
}
