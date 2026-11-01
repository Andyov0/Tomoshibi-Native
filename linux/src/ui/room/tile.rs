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
//! stage its own buttons (StageControls.tsx): the same person's other
//! picture ("Their screen", "Their camera") and filling the screen, shown
//! under the pointer or the keyboard's focus (36 drawn, scale::STAGE_BUTTON).
//! Somebody else's screen that is not on the stage is a card saying who is
//! sharing (ShareCard.tsx) and no picture at all: the room does not even
//! subscribe to it.
//!
//! Every mark comes and goes with a transition: the plate's marks slide in
//! beside the name, the rest grow or fade (the owner's rule, UI-SYNC).
//!
//! No mark saying that a press makes it larger: the owner had it taken away
//! on every platform (2026-10-26), the whole picture being what is pressed
//! (the stage, stage.rs, takes the press). A picture can be reached and
//! pressed from the keyboard too: it takes the focus, shows a ring for it,
//! and Enter or Space press it.

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use adw::prelude::*;
use gtk::{gdk, glib, graphene, gsk, pango, subclass::prelude::*};

use super::{
    glyph::{self, Glyph},
    motion::{Away, Motion},
    talk::PictureKind,
};
use crate::{
    i18n::{t, t_with},
    render::{Fit, VideoView},
    theme::{motion, scale},
};

/// What a picture says about whoever is in it.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Marks {
    /// What the plate says: the name, with "(you)" or "(screen)" where the
    /// picture is one's own or a screen (surface.ts `label`).
    pub label: String,
    /// The name alone, for the initials: "ann (you)" made "A(".
    pub name: String,
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
    /// The same person's other picture, offered beside the fill button as
    /// "Their screen" or "Their camera" (Room.tsx `counterpart`).
    pub other: Option<PictureKind>,
    /// The stage's buttons in the bottom corner rather than the top: on the
    /// other person of a call of two, whose top corner holds this
    /// participant's own picture as a card (StageControls.tsx `at`).
    pub controls_at_foot: bool,
    /// How far below the picture's top the buttons sit there: below the
    /// window's own buttons and the room's name where those reach across.
    pub controls_top: i32,
    /// How far in from the picture's trailing edge: clear of a panel open
    /// over it.
    pub controls_end: i32,
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
    /// Somebody else's screen while it is not on the stage: the card that
    /// stands in for it (ShareCard.tsx).
    pub share_card: bool,
    /// Their sealed frames cannot be read here: they typed another sealing
    /// word, and nothing of theirs will ever play.
    pub unreadable: bool,
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

/// A mark on the name plate that slides in beside the name and out again,
/// the plate growing and shrinking with it rather than jumping (the bar's
/// own buttons come and go the same way, page.rs `coming`).
fn plate_mark(child: &impl IsA<gtk::Widget>) -> gtk::Revealer {
    let revealer = gtk::Revealer::builder()
        .transition_type(gtk::RevealerTransitionType::SlideRight)
        .transition_duration(motion::SETTLE_MS)
        .reveal_child(false)
        .visible(false)
        .valign(gtk::Align::Center)
        .child(child)
        .build();
    // Gone once it has slid away, so that the plate's spacing does not keep
    // a gap for it.
    revealer.connect_child_revealed_notify(|revealer| {
        if !revealer.reveals_child() && !revealer.is_child_revealed() {
            revealer.set_visible(false);
        }
    });
    revealer
}

/// Brings a [`plate_mark`] in or takes it away: in over the settle time, out
/// over the shorter leave, as every piece of the call moves.
fn show_mark(revealer: &gtk::Revealer, on: bool) {
    if revealer.reveals_child() == on && revealer.is_visible() == on {
        return;
    }
    revealer.set_transition_duration(if on {
        motion::SETTLE_MS
    } else {
        motion::LEAVE_MS
    });
    if on {
        revealer.set_visible(true);
    }
    revealer.set_reveal_child(on);
    // Without animations the notify above may not come.
    if !on && !adw::is_animations_enabled(revealer) {
        revealer.set_visible(false);
    }
}

/// What opens a picture's menu, at a point in the tile.
type MenuAt = Rc<dyn Fn(f64, f64)>;

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
    proven: gtk::Revealer,
    unverified: gtk::Revealer,
    mic_off: gtk::Revealer,
    silenced: gtk::Revealer,
    hand: gtk::Box,
    hand_motion: Motion,
    small_hand_motion: Motion,
    controls: Motion,
    switch_button: gtk::Button,
    switch_icon: gtk::Image,
    switch_words: gtk::Label,
    switch_motion: Motion,
    fill_button: gtk::Button,
    share_face: Motion,
    share_stop: gtk::Button,
    share_card: Motion,
    share_card_said: gtk::Label,
    share_card_watch: gtk::Box,
    unreadable: Motion,
    unreadable_words: gtk::Label,
    hovering: Cell<bool>,
    focused: Cell<bool>,
    marks: RefCell<Marks>,
    /// The track whose frames the view is attached to, if any.
    pub attached: RefCell<Option<String>>,
    on_fill: RefCell<Option<Rc<dyn Fn()>>>,
    on_switch: RefCell<Option<Rc<dyn Fn()>>>,
    on_stop: RefCell<Option<Rc<dyn Fn()>>>,
    on_press: RefCell<Option<Rc<dyn Fn()>>>,
    on_menu: RefCell<Option<MenuAt>>,
    floats: Cell<usize>,
}

impl Tile {
    pub fn new(id: &str) -> Rc<Tile> {
        // A button to whoever reads the screen to somebody: pressing it is
        // what the picture does.
        let root = gtk::Overlay::builder()
            .name(format!("tile:{id}"))
            .overflow(gtk::Overflow::Hidden)
            .accessible_role(gtk::AccessibleRole::Button)
            .focusable(true)
            .build();
        root.add_css_class("tmsb-tile");

        let avatar = Avatar::new();
        root.set_child(Some(&avatar));

        let video = VideoView::new();
        let video_motion = Motion::hidden(&video, Away::FADE);
        video_motion.set_can_target(false);
        root.add_overlay(&video_motion);
        root.set_measure_overlay(&video_motion, false);

        // Somebody else's screen while it is not on the stage: who is
        // sharing, and the word that the whole card is pressed to watch it
        // (the Mac's ShareCardFace; flat, a step lighter than a person's
        // tile, so that it is not taken for a camera that is off).
        let card = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(10)
            .halign(gtk::Align::Center)
            .valign(gtk::Align::Center)
            .margin_start(12)
            .margin_end(12)
            .can_target(false)
            .build();
        card.add_css_class("tmsb-share-card");
        let card_mark = gtk::Image::from_icon_name("screen-shared-symbolic");
        card_mark.add_css_class("tmsb-share-card-mark");
        card_mark.set_halign(gtk::Align::Center);
        let share_card_said = gtk::Label::builder()
            .wrap(true)
            .lines(2)
            .ellipsize(pango::EllipsizeMode::End)
            .justify(gtk::Justification::Center)
            .build();
        share_card_said.add_css_class("tmsb-share-said");
        let share_card_watch = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(5)
            .halign(gtk::Align::Center)
            .build();
        share_card_watch.add_css_class("tmsb-watch-pill");
        share_card_watch.append(&gtk::Image::from_icon_name("media-playback-start-symbolic"));
        let watch_words = gtk::Label::new(Some(&t("Watch")));
        share_card_watch.append(&watch_words);
        card.append(&card_mark);
        card.append(&share_card_said);
        card.append(&share_card_watch);
        let share_card = Motion::hidden(&card, Away::FADE);
        share_card.set_can_target(false);
        root.add_overlay(&share_card);

        let edge = gtk::Box::builder().can_target(false).build();
        edge.add_css_class("tmsb-tile-edge");
        root.add_overlay(&edge);
        let ring = gtk::Box::builder().can_target(false).build();
        ring.add_css_class("tmsb-ring");
        root.add_overlay(&ring);
        // The keyboard's focus, drawn as a ring of its own inside the edge,
        // since the tile clips whatever is drawn outside it.
        let focus_ring = gtk::Box::builder().can_target(false).build();
        focus_ring.add_css_class("tmsb-focus-ring");
        root.add_overlay(&focus_ring);

        let name = gtk::Label::builder()
            .ellipsize(pango::EllipsizeMode::End)
            .single_line_mode(true)
            .build();
        let proven_dot = gtk::Box::builder()
            .valign(gtk::Align::Center)
            .tooltip_text(t("Only this person can use this name"))
            .build();
        proven_dot.add_css_class("tmsb-proven");
        let proven = plate_mark(&proven_dot);
        let unverified_label = gtk::Label::builder()
            .label(t("unverified"))
            .valign(gtk::Align::Center)
            .tooltip_text(t("Someone else has proved this name"))
            .build();
        unverified_label.add_css_class("tmsb-unverified");
        let unverified = plate_mark(&unverified_label);
        let mic_off_image = gtk::Image::from_icon_name("microphone-disabled-symbolic");
        mic_off_image.update_property(&[gtk::accessible::Property::Label(&t("microphone off"))]);
        let mic_off = plate_mark(&mic_off_image);
        let silenced_image = gtk::Image::from_icon_name("audio-volume-muted-symbolic");
        silenced_image.add_css_class("silenced");
        silenced_image.set_tooltip_text(Some(&t("Muted by you")));
        let silenced = plate_mark(&silenced_image);
        let plate = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(6)
            .valign(gtk::Align::Center)
            .build();
        plate.add_css_class("tmsb-plate");
        // The hand at the start of the name: an amber circle with the hand
        // in its middle. Not stretched by the plate (it was drawn as a
        // pill 24 wide with the hand against its left end), and the glyph
        // centred in it as the large one is.
        let small_hand = gtk::Box::builder()
            .halign(gtk::Align::Center)
            .valign(gtk::Align::Center)
            .build();
        small_hand.add_css_class("tmsb-hand");
        small_hand.add_css_class("small");
        let small_glyph = Glyph::new(glyph::HAND, 10);
        small_glyph.set_hexpand(true);
        small_glyph.set_vexpand(true);
        small_hand.append(&small_glyph);
        let small_hand_motion = Motion::hidden(&small_hand, Away::GROW);
        small_hand_motion.set_halign(gtk::Align::Center);
        small_hand_motion.set_valign(gtk::Align::Center);
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

        // A sealed call whose word this participant does not share with
        // them: nothing of theirs can be read, and without a word for it
        // their tile looked like a camera that was off and a microphone
        // that was quiet.
        let unreadable_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(6)
            .build();
        unreadable_box.add_css_class("tmsb-unreadable");
        unreadable_box.append(&gtk::Image::from_icon_name("changes-prevent-symbolic"));
        let unreadable_words = gtk::Label::builder()
            .ellipsize(pango::EllipsizeMode::End)
            .single_line_mode(true)
            .build();
        unreadable_box.append(&unreadable_words);
        let unreadable = Motion::hidden(&unreadable_box, Away::GROW);
        unreadable.set_halign(gtk::Align::Center);
        unreadable.set_valign(gtk::Align::Start);
        unreadable.set_margin_top(10);
        unreadable.set_margin_start(44);
        unreadable.set_margin_end(44);
        root.add_overlay(&unreadable);

        // The stage's own buttons: the same person's other picture, and
        // filling the screen.
        let switch_icon = gtk::Image::from_icon_name("screen-shared-symbolic");
        let switch_words = gtk::Label::new(None);
        let switch_inner = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        switch_inner.set_halign(gtk::Align::Center);
        switch_inner.append(&switch_icon);
        switch_inner.append(&switch_words);
        let switch_button = gtk::Button::builder().child(&switch_inner).build();
        switch_button.add_css_class("tmsb-stage-button");
        switch_button.add_css_class("words");
        switch_button.set_widget_name("tile.switch");
        let switch_motion = Motion::hidden(&switch_button, Away::GROW);
        let fill_button = gtk::Button::from_icon_name("view-fullscreen-symbolic");
        fill_button.add_css_class("tmsb-stage-button");
        fill_button.set_widget_name("tile.fill");
        let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        buttons.append(&switch_motion);
        buttons.append(&fill_button);
        let controls = Motion::hidden(&buttons, Away::FADE);
        controls.set_halign(gtk::Align::End);
        controls.set_valign(gtk::Align::Start);
        controls.set_margin_end(12);
        controls.set_margin_top(12);
        controls.set_widget_name("tile.controls");
        root.add_overlay(&controls);

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
            controls,
            switch_button,
            switch_icon,
            switch_words,
            switch_motion,
            fill_button,
            share_face,
            share_stop,
            share_card,
            share_card_said,
            share_card_watch,
            unreadable,
            unreadable_words,
            hovering: Cell::new(false),
            focused: Cell::new(false),
            marks: RefCell::new(Marks::default()),
            attached: RefCell::new(None),
            on_fill: RefCell::new(None),
            on_switch: RefCell::new(None),
            on_stop: RefCell::new(None),
            on_press: RefCell::new(None),
            on_menu: RefCell::new(None),
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

        // The keyboard's focus on the picture or on one of its buttons shows
        // the buttons, as the web's `focus-within` does: they were reachable
        // only under a pointer.
        let focus = gtk::EventControllerFocus::new();
        let weak = Rc::downgrade(&tile);
        focus.connect_contains_focus_notify(move |focus| {
            if let Some(tile) = weak.upgrade() {
                tile.focused.set(focus.contains_focus());
                tile.show_buttons();
            }
        });
        tile.root.add_controller(focus);

        // Enter and Space press the picture itself, never a button on it,
        // which takes its own keys first; the menu key and Shift+F10 open
        // its menu, as a right press does.
        let keys = gtk::EventControllerKey::new();
        let weak = Rc::downgrade(&tile);
        keys.connect_key_pressed(move |_, key, _, state| {
            let Some(tile) = weak.upgrade() else {
                return glib::Propagation::Proceed;
            };
            if !tile.root.has_focus() {
                return glib::Propagation::Proceed;
            }
            let shift_f10 = key == gdk::Key::F10 && state.contains(gdk::ModifierType::SHIFT_MASK);
            if key == gdk::Key::Menu || shift_f10 {
                let menu = tile.on_menu.borrow().clone();
                if let Some(menu) = menu {
                    let (width, height) = (tile.root.width() as f64, tile.root.height() as f64);
                    menu(width / 2.0, height / 2.0);
                    return glib::Propagation::Stop;
                }
                return glib::Propagation::Proceed;
            }
            if matches!(
                key,
                gdk::Key::Return | gdk::Key::KP_Enter | gdk::Key::ISO_Enter | gdk::Key::space
            ) {
                let press = tile.on_press.borrow().clone();
                if let Some(press) = press {
                    press();
                    return glib::Propagation::Stop;
                }
            }
            glib::Propagation::Proceed
        });
        tile.root.add_controller(keys);

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
        let weak = Rc::downgrade(&tile);
        tile.switch_button.connect_clicked(move |_| {
            let switch = weak
                .upgrade()
                .and_then(|tile| tile.on_switch.borrow().clone());
            if let Some(switch) = switch {
                switch();
            }
        });
        tile.reword();
        tile
    }

    pub fn video(&self) -> &VideoView {
        &self.video
    }

    pub fn marks(&self) -> Marks {
        self.marks.borrow().clone()
    }

    /// What the face's Stop does.
    pub fn on_stop_sharing(&self, stop: impl Fn() + 'static) {
        self.on_stop.replace(Some(Rc::new(stop)));
    }

    /// What the fill button does.
    pub fn on_fill(&self, fill: impl Fn() + 'static) {
        self.on_fill.replace(Some(Rc::new(fill)));
    }

    /// What "Their screen" or "Their camera" does.
    pub fn on_switch(&self, switch: impl Fn() + 'static) {
        self.on_switch.replace(Some(Rc::new(switch)));
    }

    /// What Enter or Space on the focused picture does: what a press on it
    /// does.
    pub fn on_press(&self, press: impl Fn() + 'static) {
        self.on_press.replace(Some(Rc::new(press)));
    }

    /// What the menu key does on the focused picture: its menu, at a point
    /// in the tile's own coordinates.
    pub fn on_menu(&self, menu: impl Fn(f64, f64) + 'static) {
        self.on_menu.replace(Some(Rc::new(menu)));
    }

    /// Says again what the tile says once, in the language now.
    pub fn reword(&self) {
        self.unreadable_words.set_label(&t("Another sealing word"));
        self.unreadable.set_tooltip_text(Some(&t(
            "This person sealed the call with another word, so nothing of theirs can be seen or heard here.",
        )));
        if let Some(words) = self
            .share_card_watch
            .last_child()
            .and_downcast::<gtk::Label>()
        {
            words.set_label(&t("Watch"));
        }
        // What depends on the marks is said again on the next set_marks.
        let marks = self.marks.replace(Marks {
            label: String::from("\u{0}"),
            ..Marks::default()
        });
        self.set_marks(marks);
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
        // The initials are the person's, not the plate's: "ann (you)" was
        // drawn as "A(".
        let initials_of = if marks.name.trim().is_empty() {
            &marks.label
        } else {
            &marks.name
        };
        self.avatar.set(initials_of, &marks.identity);
        self.name.set_label(&marks.label);
        show_mark(&self.proven, marks.proven);
        show_mark(&self.unverified, marks.unverified);
        show_mark(&self.silenced, marks.silenced);
        show_mark(&self.mic_off, marks.muted && !marks.silenced);
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
        self.share_card.show(marks.share_card);
        self.share_card_said
            .set_label(&t_with("{name} is sharing", &[("name", &marks.name)]));
        self.share_card_watch.set_visible(!marks.compact);
        toggle(self.share_card.upcast_ref(), "compact", marks.compact);
        self.avatar
            .set_visible(!marks.my_share && !marks.share_card);
        self.share_stop.set_visible(!marks.compact);
        toggle(self.share_face.upcast_ref(), "compact", marks.compact);
        toggle(root.upcast_ref(), "mine-shared", marks.my_share);
        toggle(root.upcast_ref(), "share-card", marks.share_card);
        self.small_hand_motion
            .show(marks.hand && marks.hand_beside_name);
        self.unreadable.show(marks.unreadable);
        self.unreadable_words.set_visible(!marks.compact);
        toggle(self.ring.upcast_ref(), "on", marks.speaking);
        toggle(self.edge.upcast_ref(), "floating", marks.floating);
        toggle(root.upcast_ref(), "filled", marks.filled);
        // A screen is always whole, since whatever is at its edge is what
        // somebody is pointing at; somebody else's camera fills its place
        // or is shown whole on a blurred copy of itself, by the two shapes
        // (Framing, the Mac's VideoSurface); one's own camera fills its
        // place as the Mac's own preview does, since it is a mirror and not
        // anybody's view of the room.
        self.video.set_fit(match marks.kind {
            Some(PictureKind::Screen) => Fit::Contain,
            _ if marks.mirrored => Fit::Cover,
            _ => Fit::Framed,
        });
        self.video.set_mirrored(marks.mirrored);

        // The stage's buttons: in the top corner below whatever reaches
        // across it, or in the bottom corner clear of the bar.
        self.controls.set_valign(if marks.controls_at_foot {
            gtk::Align::End
        } else {
            gtk::Align::Start
        });
        self.controls.set_margin_top(if marks.controls_at_foot {
            0
        } else {
            marks.controls_top.max(12)
        });
        self.controls.set_margin_bottom(if marks.controls_at_foot {
            10 + marks.lift
        } else {
            0
        });
        self.controls.set_margin_end(marks.controls_end.max(10));
        let fill_label = if marks.fill_button == Some(true) {
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
        self.fill_button.set_tooltip_text(Some(&fill_label));
        self.fill_button
            .update_property(&[gtk::accessible::Property::Label(&fill_label)]);
        if let Some(other) = marks.other {
            let (icon, words) = match other {
                PictureKind::Screen => ("screen-shared-symbolic", t("Their screen")),
                PictureKind::Camera => ("avatar-default-symbolic", t("Their camera")),
            };
            self.switch_icon.set_icon_name(Some(icon));
            self.switch_words.set_label(&words);
            self.switch_button.set_tooltip_text(Some(&words));
            self.switch_button
                .update_property(&[gtk::accessible::Property::Label(&words)]);
        }
        // The words where the picture has room for them, the symbol alone on
        // a small one, still said to a reader (StageControls.tsx).
        self.switch_words.set_visible(!marks.compact);
        toggle(self.switch_button.upcast_ref(), "words", !marks.compact);
        self.switch_motion.show(marks.other.is_some());
        root.update_property(&[gtk::accessible::Property::Label(&marks.label)]);
        self.marks.replace(marks);
        self.show_buttons();
    }

    /// The stage's buttons, under the pointer or the keyboard's focus.
    fn show_buttons(&self) {
        let offered = self.marks.borrow().fill_button.is_some();
        self.controls
            .show(offered && (self.hovering.get() || self.focused.get()));
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

    /// Whether the stage's buttons are showing, and "Their ..." among them,
    /// for a test to read.
    pub fn controls_shown(&self) -> (bool, bool) {
        (self.controls.is_shown(), self.switch_motion.is_shown())
    }

    /// Shows the stage's buttons as the pointer would, for a test that has
    /// no pointer: a headless run moves none.
    #[cfg(debug_assertions)]
    pub fn hover_for_test(&self, on: bool) {
        self.hovering.set(on);
        self.show_buttons();
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
        assert_eq!(initials("émile zola"), "ÉZ", "upper case beyond ASCII");
        // The plate's label is not a name: its "(you)" gave "A(".
        assert_eq!(initials("ann"), "A");
    }

    #[test]
    fn one_identity_is_one_colour() {
        assert_eq!(avatar_colour("tabc-1"), avatar_colour("tabc-1"));
        let spread: std::collections::HashSet<usize> =
            (0..40).map(|n| avatar_colour(&format!("g{n}"))).collect();
        assert!(spread.len() > 3, "the colours are spread: {spread:?}");
    }
}
