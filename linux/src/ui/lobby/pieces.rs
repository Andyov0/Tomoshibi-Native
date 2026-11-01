//! The pieces the screens before a call share: symbols, the microphone and
//! camera switches, a person's initial, the problem at the foot of a screen,
//! the deployment's notice, the language menu, the server menu, the seal,
//! and the small ways things arrive and leave.

use std::{cell::RefCell, rc::Rc};

use adw::prelude::*;
use gtk::{glib, graphene, gsk, subclass::prelude::*};

use crate::{
    app::{self, remote, AppModel, Change},
    i18n::{self, t, t_with, Language},
    theme::motion,
    ui::kit,
};

// ---- symbols -----------------------------------------------------------

/// A symbol from the icon theme at a size; the freedesktop names, which
/// Adwaita and the other themes all draw.
pub fn icon(name: &str, size: i32) -> gtk::Image {
    let image = gtk::Image::from_icon_name(name);
    image.set_pixel_size(size);
    image
}

/// Symbols the freedesktop names do not have, drawn here on a 24-unit grid
/// in the widget's own colour, so that they follow the words beside them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Glyph {
    /// The language menu's.
    Globe,
    /// Join a meeting: a door and an arrow going in (UI-SYNC: not an eye).
    DoorIn,
    Key,
    Lock,
    LockOpen,
}

impl Glyph {
    fn path(self) -> &'static str {
        match self {
            Glyph::Globe => "M 21 12 A 9 9 0 1 1 3 12 A 9 9 0 1 1 21 12 M 3.5 9 L 20.5 9 M 3.5 15 L 20.5 15 M 12 3 C 8.5 6.5 8.5 17.5 12 21 M 12 3 C 15.5 6.5 15.5 17.5 12 21",
            Glyph::DoorIn => "M 13 3.5 L 19.5 3.5 L 19.5 20.5 L 13 20.5 M 3.5 12 L 14.5 12 M 10.5 8 L 14.5 12 L 10.5 16",
            Glyph::Key => "M 13 9.5 A 4.5 4.5 0 1 1 4 9.5 A 4.5 4.5 0 1 1 13 9.5 M 11.7 12.7 L 20 21 M 17.2 18.2 L 19.4 16 M 14.6 15.6 L 16.6 13.6",
            Glyph::Lock => "M 5.5 11 L 18.5 11 L 18.5 20.5 L 5.5 20.5 Z M 8.5 11 L 8.5 7.5 C 8.5 3 15.5 3 15.5 7.5 L 15.5 11",
            Glyph::LockOpen => "M 5.5 11 L 18.5 11 L 18.5 20.5 L 5.5 20.5 Z M 8.5 11 L 8.5 7.5 C 8.5 3.5 14.2 2.8 15.3 6.2",
        }
    }
}

mod glyph_imp {
    use super::*;
    use std::cell::Cell;

    #[derive(Default)]
    pub struct GlyphView {
        pub path: RefCell<Option<gsk::Path>>,
        pub size: Cell<i32>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for GlyphView {
        const NAME: &'static str = "TomoshibiGlyph";
        type Type = super::GlyphView;
        type ParentType = gtk::Widget;
    }

    impl ObjectImpl for GlyphView {}

    impl WidgetImpl for GlyphView {
        fn measure(&self, _: gtk::Orientation, _: i32) -> (i32, i32, i32, i32) {
            let size = self.size.get();
            (size, size, -1, -1)
        }

        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            let Some(path) = self.path.borrow().clone() else {
                return;
            };
            let obj = self.obj();
            let scale = obj.width().min(obj.height()) as f32 / 24.0;
            let stroke = gsk::Stroke::new(1.9);
            stroke.set_line_cap(gsk::LineCap::Round);
            stroke.set_line_join(gsk::LineJoin::Round);
            snapshot.save();
            snapshot.translate(&graphene::Point::new(
                (obj.width() as f32 - 24.0 * scale) / 2.0,
                (obj.height() as f32 - 24.0 * scale) / 2.0,
            ));
            snapshot.scale(scale, scale);
            snapshot.append_stroke(&path, &stroke, &obj.color());
            snapshot.restore();
        }
    }
}

glib::wrapper! {
    pub struct GlyphView(ObjectSubclass<glyph_imp::GlyphView>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

pub fn glyph(kind: Glyph, size: i32) -> GlyphView {
    let view: GlyphView = glib::Object::new();
    view.imp().size.set(size);
    view.imp().path.replace(gsk::Path::parse(kind.path()).ok());
    view.set_can_target(false);
    view.set_valign(gtk::Align::Center);
    view.set_halign(gtk::Align::Center);
    view
}

// ---- buttons -----------------------------------------------------------

/// A round plain button with a symbol, `size` across, its symbol four
/// tenths of it (the shared scale), and the label for the screen reader and
/// the tooltip.
pub fn round_button(symbol: &impl IsA<gtk::Widget>, size: i32, label: &str) -> gtk::Button {
    let button = gtk::Button::builder()
        .child(symbol)
        .width_request(size)
        .height_request(size)
        .halign(gtk::Align::Center)
        .valign(gtk::Align::Center)
        .tooltip_text(label)
        .build();
    button.add_css_class("tmsb-round");
    button.update_property(&[gtk::accessible::Property::Label(label)]);
    button
}

/// A symbol and words on a capsule.
pub fn labelled(symbol: Option<&gtk::Widget>, words: &str) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    row.set_halign(gtk::Align::Center);
    if let Some(symbol) = symbol {
        row.append(symbol);
    }
    let label = gtk::Label::new(Some(words));
    label.set_ellipsize(gtk::pango::EllipsizeMode::End);
    row.append(&label);
    row
}

/// The microphone and the camera as they will be on joining. Off is the
/// foreground's own plate with the symbol in the page's colour, as on the
/// call's bar (UI-SYNC decision 1), not red; forty-four wherever they stand.
pub fn device_switches(model: &AppModel, size: i32) -> gtk::Box {
    let capsule = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    capsule.add_css_class("tmsb-capsule");
    capsule.set_halign(gtk::Align::Center);
    let microphone = gtk::Button::new();
    let camera = gtk::Button::new();
    for button in [&microphone, &camera] {
        button.add_css_class("tmsb-switch");
        button.set_size_request(size, size);
        capsule.append(button);
    }
    microphone.set_widget_name("lobby.microphone");
    camera.set_widget_name("lobby.camera");
    let glyph_size = (size as f32 * crate::theme::scale::ICON_RATIO).round() as i32;
    let fill = {
        let (microphone, camera) = (microphone.downgrade(), camera.downgrade());
        move |model: &AppModel| {
            let (Some(microphone), Some(camera)) = (microphone.upgrade(), camera.upgrade()) else {
                return;
            };
            let on = model.start_with_microphone();
            microphone.set_child(Some(&icon(
                if on {
                    "audio-input-microphone-symbolic"
                } else {
                    "microphone-disabled-symbolic"
                },
                glyph_size,
            )));
            set_class(&microphone, "off", !on);
            let said = if on {
                t("Mute microphone")
            } else {
                t("Unmute microphone")
            };
            microphone.set_tooltip_text(Some(&said));
            microphone.update_property(&[gtk::accessible::Property::Label(&said)]);
            let on = model.start_with_camera();
            camera.set_child(Some(&icon(
                if on {
                    "camera-video-symbolic"
                } else {
                    "camera-disabled-symbolic"
                },
                glyph_size,
            )));
            set_class(&camera, "off", !on);
            let said = if on {
                t("Turn camera off")
            } else {
                t("Turn camera on")
            };
            camera.set_tooltip_text(Some(&said));
            camera.update_property(&[gtk::accessible::Property::Label(&said)]);
        }
    };
    fill(model);
    let weak = model.downgrade();
    microphone.connect_clicked(move |_| {
        if let Some(model) = weak.upgrade() {
            model.set_start_with_microphone(!model.start_with_microphone());
        }
    });
    let weak = model.downgrade();
    camera.connect_clicked(move |_| {
        if let Some(model) = weak.upgrade() {
            model.set_start_with_camera(!model.start_with_camera());
        }
    });
    model.subscribe_while(&capsule, move |model, change| {
        if change == app::CHANGED {
            fill(model);
        }
    });
    capsule
}

pub fn set_class(widget: &impl IsA<gtk::Widget>, class: &str, on: bool) {
    if on {
        widget.add_css_class(class);
    } else {
        widget.remove_css_class(class);
    }
}

/// The person, as a circle with their initial in it, the same colour for
/// the same name everywhere it appears (Lobby.swift `Initial`).
pub fn initial(name: &str, size: i32) -> gtk::Label {
    let letter: String = name
        .chars()
        .next()
        .map(|c| c.to_uppercase().collect())
        .unwrap_or_default();
    let label = gtk::Label::new(Some(&letter));
    label.set_size_request(size, size);
    label.set_halign(gtk::Align::Center);
    label.set_valign(gtk::Align::Center);
    label.add_css_class("tmsb-initial");
    label.add_css_class(&format!("tmsb-initial-{}", initial_colour(name)));
    let attributes = gtk::pango::AttrList::new();
    attributes.insert(gtk::pango::AttrSize::new_size_absolute(
        (size as f64 * 0.42 * gtk::pango::SCALE as f64) as i32,
    ));
    label.set_attributes(Some(&attributes));
    label.set_accessible_role(gtk::AccessibleRole::Presentation);
    label
}

/// The account's picture over its initial (the Mac's `AccountAvatar`): the
/// initial at once, the picture fading in over it once the deployment has
/// sent it, and again whenever the account changes it. A picture that does
/// not come leaves the initial.
pub fn avatar(model: &AppModel, name: &str, size: i32) -> gtk::Overlay {
    let overlay = gtk::Overlay::new();
    overlay.set_size_request(size, size);
    overlay.set_halign(gtk::Align::Center);
    overlay.set_valign(gtk::Align::Center);
    overlay.set_child(Some(&initial(name, size)));
    let picture = gtk::Picture::builder()
        .content_fit(gtk::ContentFit::Cover)
        .can_shrink(true)
        .width_request(size)
        .height_request(size)
        .build();
    picture.add_css_class("tmsb-avatar-picture");
    picture.set_overflow(gtk::Overflow::Hidden);
    let slot = fading(&picture, false);
    slot.set_can_target(false);
    overlay.add_overlay(&slot);
    overlay.set_accessible_role(gtk::AccessibleRole::Presentation);

    let shown: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));
    let fill = {
        let (picture, slot) = (picture.downgrade(), slot.downgrade());
        move |model: &AppModel| {
            let address = model.avatar_address();
            if *shown.borrow() == address {
                return;
            }
            shown.replace(address.clone());
            let (Some(picture), Some(slot)) = (picture.upgrade(), slot.upgrade()) else {
                return;
            };
            let Some(address) = address else {
                slot.set_reveal_child(false);
                return;
            };
            if let Some(texture) = PICTURES.with(|kept| kept.borrow().get(&address).cloned()) {
                picture.set_paintable(Some(&texture));
                slot.set_reveal_child(true);
                return;
            }
            let (picture, slot, shown) = (picture.downgrade(), slot.downgrade(), shown.clone());
            glib::spawn_future_local(async move {
                let Some(bytes) = crate::app::account::fetch_picture(address.clone()).await else {
                    return;
                };
                // Decoded off the main thread, at twice the size drawn for a
                // sharp picture on a doubled display.
                let decoded = gtk::gio::spawn_blocking(move || decode_picture(&bytes, size * 2)).await;
                let Ok(Some(texture)) = decoded.map(|pixels| pixels.map(|p| p.texture())) else {
                    return;
                };
                PICTURES.with(|kept| kept.borrow_mut().insert(address.clone(), texture.clone()));
                // Not over a newer picture asked for meanwhile.
                if shown.borrow().as_deref() != Some(address.as_str()) {
                    return;
                }
                if let (Some(picture), Some(slot)) = (picture.upgrade(), slot.upgrade()) {
                    picture.set_paintable(Some(&texture));
                    slot.set_reveal_child(true);
                }
            });
        }
    };
    fill(model);
    model.subscribe_while(&overlay, move |model, change| {
        if matches!(change, Change::Account) {
            fill(model);
        }
    });
    overlay
}

thread_local! {
    /// The pictures fetched, by address (which carries the picture's
    /// version), so that a lobby built again shows it at once.
    static PICTURES: RefCell<std::collections::HashMap<String, gtk::gdk::Texture>> =
        RefCell::new(std::collections::HashMap::new());
}

/// A decoded picture's pixels, which can cross threads where a GdkPixbuf
/// is better not to.
pub struct Pixels {
    bytes: glib::Bytes,
    width: i32,
    height: i32,
    stride: usize,
    alpha: bool,
}

impl Pixels {
    pub fn texture(&self) -> gtk::gdk::Texture {
        gtk::gdk::MemoryTexture::new(
            self.width,
            self.height,
            if self.alpha {
                gtk::gdk::MemoryFormat::R8g8b8a8
            } else {
                gtk::gdk::MemoryFormat::R8g8b8
            },
            &self.bytes,
            self.stride,
        )
        .upcast()
    }
}

/// `bytes` decoded and scaled to fit `size` square; on any thread.
pub fn decode_picture(bytes: &[u8], size: i32) -> Option<Pixels> {
    let stream = gtk::gio::MemoryInputStream::from_bytes(&glib::Bytes::from(bytes));
    let pixbuf = gtk::gdk_pixbuf::Pixbuf::from_stream_at_scale(
        &stream,
        size,
        size,
        true,
        gtk::gio::Cancellable::NONE,
    )
    .ok()?;
    Some(Pixels {
        bytes: pixbuf.read_pixel_bytes(),
        width: pixbuf.width(),
        height: pixbuf.height(),
        stride: pixbuf.rowstride() as usize,
        alpha: pixbuf.has_alpha(),
    })
}

/// Which of the six colours a name gets: the sum of its code points.
pub fn initial_colour(name: &str) -> usize {
    name.chars().fold(0u32, |sum, c| sum.wrapping_add(c as u32)) as usize % 6
}

// ---- the identity box ------------------------------------------------------

/// A field of the box that holds a name and a passphrase, with its symbol
/// before it: the person for the name, the key for the passphrase, as the
/// web's two fields have. The symbol is drawn beside the field rather than
/// in it, because GtkPasswordEntry has no place for one: with the name's
/// symbol inside its field and none beside the passphrase, the two lines
/// of words started at different places.
pub fn identity_field(symbol: &impl IsA<gtk::Widget>, field: &impl IsA<gtk::Widget>) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    row.add_css_class("tmsb-field-row");
    let symbol = symbol.as_ref();
    symbol.add_css_class("tmsb-muted");
    symbol.set_valign(gtk::Align::Center);
    row.append(symbol);
    field.as_ref().set_hexpand(true);
    row.append(field);
    // A press on the symbol is a press on its field.
    let press = gtk::GestureClick::new();
    let weak = field.as_ref().downgrade();
    press.connect_released(move |_, _, _, _| {
        if let Some(field) = weak.upgrade() {
            field.grab_focus();
        }
    });
    symbol.add_controller(press);
    row
}

/// The name's symbol and the passphrase's.
pub fn name_symbol() -> gtk::Widget {
    icon("avatar-default-symbolic", 16).upcast()
}

pub fn passphrase_symbol() -> gtk::Widget {
    glyph(Glyph::Key, 16).upcast()
}

// ---- coming and going ----------------------------------------------------

/// `child` in a revealer that fades it in and out over the settling time,
/// which GTK skips when animations are off.
pub fn fading(child: &impl IsA<gtk::Widget>, shown: bool) -> gtk::Revealer {
    gtk::Revealer::builder()
        .transition_type(gtk::RevealerTransitionType::Crossfade)
        .transition_duration(motion::SETTLE_MS)
        .reveal_child(shown)
        .child(child)
        .build()
}

/// `child` in a revealer that opens downward and fades, for something that
/// comes into a column and moves what is under it.
pub fn unfolding(child: &impl IsA<gtk::Widget>, shown: bool) -> gtk::Revealer {
    gtk::Revealer::builder()
        .transition_type(gtk::RevealerTransitionType::SlideDown)
        .transition_duration(motion::SETTLE_MS)
        .reveal_child(shown)
        .child(child)
        .build()
}

// ---- lists that change ------------------------------------------------------

/// The rows of a list kept by what they show, so that a change to the list
/// moves only what changed: a row that is new unfolds into place, a row
/// that went folds away before it is removed, a row whose words changed is
/// changed where it is, and the rest stay as they are. The lobby's lists
/// were emptied and built again on every change, so every row vanished and
/// reappeared at once whenever one of them changed.
#[derive(Default)]
pub struct KeptRows {
    rows: RefCell<Vec<KeptRow>>,
}

struct KeptRow {
    key: String,
    looks: String,
    row: gtk::ListBoxRow,
    slot: gtk::Revealer,
    leaving: Rc<std::cell::Cell<bool>>,
}

impl KeptRows {
    /// Makes `list` show `wanted` in order: each a key (what the row is),
    /// what it looks like now (a row whose looks change is built again in
    /// place), and how to build it.
    pub fn show(&self, list: &gtk::ListBox, wanted: Vec<(String, String, Box<dyn Fn() -> gtk::Widget>)>) {
        let mut rows = self.rows.borrow_mut();
        // What goes.
        for kept in rows.iter() {
            if !kept.leaving.get() && !wanted.iter().any(|(key, _, _)| *key == kept.key) {
                kept.leaving.set(true);
                kept.slot.set_reveal_child(false);
            }
        }
        // What comes back, stays or arrives, in order among the rows that
        // are not leaving.
        let mut place = 0;
        for (key, looks, build) in wanted {
            let found = rows.iter().position(|kept| kept.key == key);
            match found {
                Some(index) => {
                    let kept = &mut rows[index];
                    kept.leaving.set(false);
                    kept.slot.set_reveal_child(true);
                    if kept.looks != looks {
                        kept.slot.set_child(Some(&build()));
                        kept.looks = looks;
                    }
                    let row = kept.row.clone();
                    if row.index() != place {
                        list.remove(&row);
                        list.insert(&row, place);
                    }
                }
                None => {
                    let slot = unfolding(&build(), false);
                    let row = gtk::ListBoxRow::builder()
                        .child(&slot)
                        .activatable(false)
                        .build();
                    list.insert(&row, place);
                    let leaving = Rc::new(std::cell::Cell::new(false));
                    // Removed once folded away, unless it came back meanwhile.
                    let (weak_list, weak_row, still) =
                        (list.downgrade(), row.downgrade(), leaving.clone());
                    slot.connect_child_revealed_notify(move |slot| {
                        if slot.is_child_revealed() || !still.get() {
                            return;
                        }
                        if let (Some(list), Some(row)) = (weak_list.upgrade(), weak_row.upgrade()) {
                            if row.parent().as_ref() == Some(list.upcast_ref()) {
                                list.remove(&row);
                            }
                        }
                    });
                    // Unfolds once it is in the list, so that it is seen to.
                    slot.set_reveal_child(true);
                    rows.push(KeptRow {
                        key,
                        looks,
                        row,
                        slot,
                        leaving,
                    });
                }
            }
            place += 1;
        }
        // Forget the rows that have been removed.
        rows.retain(|kept| kept.row.parent().is_some());
    }
}

/// A line under a heading in the muted voice.
pub fn note(text: &str) -> gtk::Label {
    let label = gtk::Label::builder()
        .label(text)
        .wrap(true)
        .wrap_mode(gtk::pango::WrapMode::WordChar)
        .justify(gtk::Justification::Center)
        .build();
    label.add_css_class("tmsb-note");
    label
}

pub fn heading(text: &str, mono: bool) -> gtk::Label {
    let label = gtk::Label::builder()
        .label(text)
        .wrap(true)
        .wrap_mode(gtk::pango::WrapMode::WordChar)
        .justify(gtk::Justification::Center)
        .lines(2)
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .build();
    label.add_css_class(if mono {
        "tmsb-room-heading"
    } else {
        "tmsb-heading"
    });
    label
}

// ---- what went wrong -------------------------------------------------------

/// The problem at the foot of a screen (Notices.tsx), over `content`: one
/// that happened and is over fades by itself after ten seconds; one about
/// why somebody is not in a meeting stays, with a way to close it.
pub fn with_toast(model: &AppModel, content: &impl IsA<gtk::Widget>) -> gtk::Overlay {
    let overlay = gtk::Overlay::new();
    overlay.set_child(Some(content));
    let symbol = gtk::Image::new();
    symbol.set_pixel_size(14);
    let words = gtk::Label::builder()
        .wrap(true)
        .wrap_mode(gtk::pango::WrapMode::WordChar)
        .xalign(0.0)
        .max_width_chars(60)
        .build();
    words.add_css_class("tmsb-toast-words");
    let close = round_button(&icon("window-close-symbolic", 10), 24, &t("Close"));
    close.add_css_class("tmsb-toast-close");
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    row.add_css_class("tmsb-toast");
    row.append(&symbol);
    row.append(&words);
    row.append(&close);
    row.set_widget_name("lobby.toast");
    let revealer = fading(&row, false);
    revealer.set_halign(gtk::Align::Center);
    revealer.set_valign(gtk::Align::End);
    revealer.set_margin_bottom(28);
    revealer.set_margin_start(20);
    revealer.set_margin_end(20);
    revealer.set_can_target(false);
    overlay.add_overlay(&revealer);
    let weak = model.downgrade();
    close.connect_clicked(move |_| {
        if let Some(model) = weak.upgrade() {
            model.clear_problem();
        }
    });
    // A press on a toast that fades puts it away.
    let click = gtk::GestureClick::new();
    let weak = model.downgrade();
    click.connect_released(move |_, _, _, _| {
        if let Some(model) = weak.upgrade() {
            if !model.problem_stays() {
                model.clear_problem();
            }
        }
    });
    row.add_controller(click);

    let fill = {
        let (revealer, words, symbol, close) = (
            revealer.downgrade(),
            words.downgrade(),
            symbol.downgrade(),
            close.downgrade(),
        );
        move |model: &AppModel| {
            let (Some(revealer), Some(words), Some(symbol), Some(close)) = (
                revealer.upgrade(),
                words.upgrade(),
                symbol.upgrade(),
                close.upgrade(),
            ) else {
                return;
            };
            match model.problem() {
                Some(problem) => {
                    let stays = model.problem_stays();
                    words.set_label(&problem);
                    symbol.set_icon_name(Some(if stays {
                        "dialog-warning-symbolic"
                    } else {
                        "dialog-information-symbolic"
                    }));
                    set_class(&symbol, "tmsb-tally-text", stays);
                    set_class(&symbol, "tmsb-muted", !stays);
                    close.set_visible(stays);
                    revealer.set_can_target(true);
                    revealer.set_reveal_child(true);
                }
                None => {
                    revealer.set_can_target(false);
                    revealer.set_reveal_child(false);
                }
            }
        }
    };
    fill(model);
    model.subscribe_while(&overlay, move |model, change| {
        if matches!(change, Change::Problem) {
            fill(model);
        }
    });
    overlay
}

// ---- the deployment's notice ---------------------------------------------

/// The deployment's notice (the core's remote config), above everything
/// else in the lobby and the sign-in, with its close; it comes and goes as
/// the deployment says, and stays closed here once closed.
pub fn notice_card(model: &AppModel) -> gtk::Revealer {
    let symbol = gtk::Image::new();
    symbol.set_pixel_size(16);
    symbol.set_valign(gtk::Align::Start);
    let words = gtk::Label::builder()
        .wrap(true)
        .wrap_mode(gtk::pango::WrapMode::WordChar)
        .xalign(0.0)
        .hexpand(true)
        .selectable(true)
        .build();
    words.add_css_class("tmsb-notice-words");
    let close = round_button(&icon("window-close-symbolic", 11), 28, &t("Dismiss"));
    close.set_valign(gtk::Align::Start);
    let card = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    card.add_css_class("tmsb-notice");
    card.set_widget_name("lobby.notice");
    card.append(&symbol);
    card.append(&words);
    card.append(&close);
    let revealer = unfolding(&card, false);
    let shown_id = Rc::new(RefCell::new(None::<String>));
    let fill = {
        let (revealer, words, symbol, card, shown_id) = (
            revealer.downgrade(),
            words.downgrade(),
            symbol.downgrade(),
            card.downgrade(),
            shown_id.clone(),
        );
        move |model: &AppModel| {
            let (Some(revealer), Some(words), Some(symbol), Some(card)) = (
                revealer.upgrade(),
                words.upgrade(),
                symbol.upgrade(),
                card.upgrade(),
            ) else {
                return;
            };
            match remote::notice(model) {
                Some((notice, text)) => {
                    words.set_label(&text);
                    symbol.set_icon_name(Some(if notice.warning {
                        "dialog-warning-symbolic"
                    } else {
                        "dialog-information-symbolic"
                    }));
                    set_class(&card, "warning", notice.warning);
                    shown_id.replace(Some(notice.id));
                    revealer.set_reveal_child(true);
                }
                None => revealer.set_reveal_child(false),
            }
        }
    };
    fill(model);
    let weak = model.downgrade();
    close.connect_clicked(move |_| {
        if let (Some(model), Some(id)) = (weak.upgrade(), shown_id.borrow().clone()) {
            remote::close(&model, &id);
        }
    });
    model.subscribe_while(&revealer, move |model, change| {
        if matches!(change, Change::Remote | Change::Screen) {
            fill(model);
        }
    });
    revealer
}

// ---- menus ---------------------------------------------------------------

/// A row of a menu: words, maybe a detail, and a check where it is the one
/// chosen (menus: radius 14, rows 32 high with radius 10).
pub fn menu_row(title: &str, detail: Option<&str>, checked: bool) -> gtk::Button {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let words = gtk::Label::new(Some(title));
    words.set_xalign(0.0);
    words.set_hexpand(true);
    words.set_ellipsize(gtk::pango::EllipsizeMode::End);
    row.append(&words);
    let said = gtk::Label::new(None);
    said.add_css_class("tmsb-muted");
    said.add_css_class("tmsb-small");
    row.append(&said);
    let check = icon("object-select-symbolic", 14);
    check.add_css_class("tmsb-tally-text");
    row.append(&check);
    // Read out as one of a menu's choices, the chosen one checked.
    let button = gtk::Button::builder()
        .child(&row)
        .accessible_role(gtk::AccessibleRole::MenuItemRadio)
        .build();
    button.add_css_class("tmsb-menu-row");
    button.update_property(&[gtk::accessible::Property::Label(title)]);
    set_menu_row(&button, detail, checked);
    button
}

/// A row of [`menu_row`]'s changed in place: its detail and its check. A
/// menu that is open while what it lists changes (the servers' times
/// arriving) keeps its rows, and the keyboard on the one it was on, rather
/// than being built again under the pointer.
pub fn set_menu_row(row: &gtk::Button, detail: Option<&str>, checked: bool) {
    let Some(line) = row.child() else {
        return;
    };
    let said = line
        .first_child()
        .and_then(|words| words.next_sibling())
        .and_downcast::<gtk::Label>();
    if let Some(said) = said {
        said.set_label(detail.unwrap_or_default());
        said.set_visible(detail.is_some());
        if let Some(check) = said.next_sibling() {
            check.set_opacity(if checked { 1.0 } else { 0.0 });
        }
    }
    row.update_state(&[gtk::accessible::State::Checked(if checked {
        gtk::AccessibleTristate::True
    } else {
        gtk::AccessibleTristate::False
    })]);
}

/// A menu `width` wide hanging from `anchor` (kit::Menu: drawn in the
/// window, arriving and leaving with a transition), and the column its rows
/// go in.
pub fn menu_for(
    anchor: &impl IsA<gtk::Widget>,
    width: i32,
    side: kit::MenuSide,
    align: kit::MenuAlign,
) -> (kit::Menu, gtk::Box) {
    let menu = kit::Menu::new(anchor, width, side, align);
    let column = menu.column();
    (menu, column)
}

pub fn menu_rule() -> gtk::Separator {
    let rule = gtk::Separator::new(gtk::Orientation::Horizontal);
    rule.add_css_class("tmsb-menu-rule");
    rule
}

/// The language, before a call as on every web page; following the system
/// until somebody chooses. The choice is kept under the web's own name.
pub fn language_button(model: &AppModel) -> gtk::Button {
    let button = kit::menu_button(&glyph(Glyph::Globe, 15));
    button.set_tooltip_text(Some(&t("Language")));
    button.set_valign(gtk::Align::Center);
    button.add_css_class("tmsb-round");
    button.add_css_class("tmsb-language");
    button.set_size_request(32, 32);
    button.set_widget_name("lobby.language");
    button.update_property(&[gtk::accessible::Property::Label(&t("Language"))]);
    let (menu, column) = menu_for(&button, 230, kit::MenuSide::Below, kit::MenuAlign::End);
    let system = Language::detect(i18n::system_languages().iter().map(String::as_str));
    let follow = menu_row(
        &t("Same as the system"),
        Some(system.display_name()),
        !i18n::chosen(),
    );
    column.append(&follow);
    column.append(&menu_rule());
    let weak = model.downgrade();
    let menu_weak = menu.downgrade();
    follow.connect_clicked(move |_| {
        if let Some(menu) = menu_weak.upgrade() {
            menu.popdown();
        }
        if let Some(model) = weak.upgrade() {
            model.preferences().remove(i18n::STORAGE_KEY);
        }
        i18n::choose(None);
    });
    for language in Language::ALL {
        let row = menu_row(
            language.display_name(),
            None,
            i18n::chosen() && i18n::language() == language,
        );
        let weak = model.downgrade();
        let menu_weak = menu.downgrade();
        row.connect_clicked(move |_| {
            if let Some(menu) = menu_weak.upgrade() {
                menu.popdown();
            }
            if let Some(model) = weak.upgrade() {
                model.preferences().set(i18n::STORAGE_KEY, language.code());
            }
            i18n::choose(Some(language));
        });
        column.append(&row);
    }
    button
}

/// Where the call is held: the fastest automatically, or a relay somebody
/// picks (ServerPicker.tsx), as a menu that measures when it opens.
pub fn server_menu(model: &AppModel) -> gtk::Button {
    let words = gtk::Label::new(None);
    words.set_ellipsize(gtk::pango::EllipsizeMode::End);
    words.set_max_width_chars(18);
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    row.append(&icon("network-server-symbolic", 13));
    row.append(&words);
    row.append(&icon("pan-down-symbolic", 11));
    let button = kit::menu_button(&row);
    button.set_valign(gtk::Align::Center);
    button.set_tooltip_text(Some(&t("Server")));
    button.add_css_class("tmsb-pill");
    button.set_widget_name("lobby.server");
    let (menu, column) = menu_for(&button, 260, kit::MenuSide::Below, kit::MenuAlign::End);
    let fill_words = {
        let words = words.downgrade();
        move |model: &AppModel| {
            if let Some(words) = words.upgrade() {
                words.set_label(&match model.relay_choice() {
                    Some(name) => model.relay_label(&name),
                    None => t("Automatic"),
                });
            }
        }
    };
    fill_words(model);
    // The rows, by relay (None for Automatic), and what they were built
    // for: the same relays are changed in place.
    let rows: Rc<RefCell<(String, Vec<(Option<String>, gtk::Button)>)>> =
        Rc::new(RefCell::new((String::new(), Vec::new())));
    let fill_menu = {
        let (column, menu) = (column.downgrade(), menu.downgrade());
        move |model: &AppModel| {
            let (Some(column), Some(menu)) = (column.upgrade(), menu.upgrade()) else {
                return;
            };
            let fastest = model.fastest_relay();
            let automatic_detail = fastest
                .as_ref()
                .map(|(name, millis)| format!("{} · {:.0} ms", model.relay_label(name), millis));
            let measured = model.lobby().measured.borrow().clone();
            let detail_of = |name: &str| {
                measured
                    .as_ref()
                    .and_then(|measured| measured.get(name).copied().flatten())
                    .map(|millis| format!("{millis:.0} ms"))
            };
            let regions = model.relays_by_region();
            let built_for = regions
                .iter()
                .map(|(region, relays)| {
                    let names: Vec<String> = relays
                        .iter()
                        .map(|relay| {
                            format!("{}:{}:{}", relay.name, model.relay_label(&relay.name), relay.maintenance)
                        })
                        .collect();
                    format!("{region}[{}]", names.join(","))
                })
                .collect::<Vec<_>>()
                .join(";");
            let chosen = model.relay_choice();
            {
                let kept = rows.borrow();
                if kept.0 == built_for && !kept.1.is_empty() {
                    for (relay, row) in &kept.1 {
                        match relay {
                            None => set_menu_row(row, automatic_detail.as_deref(), chosen.is_none()),
                            Some(name) => set_menu_row(
                                row,
                                detail_of(name).as_deref(),
                                chosen.as_deref() == Some(name.as_str()),
                            ),
                        }
                    }
                    return;
                }
            }
            while let Some(child) = column.first_child() {
                column.remove(&child);
            }
            let mut built = Vec::new();
            let automatic = menu_row(&t("Automatic"), automatic_detail.as_deref(), chosen.is_none());
            let (weak, pop) = (model.downgrade(), menu.downgrade());
            automatic.connect_clicked(move |_| {
                if let Some(model) = weak.upgrade() {
                    model.set_relay_choice(None);
                }
                if let Some(pop) = pop.upgrade() {
                    pop.popdown();
                }
            });
            column.append(&automatic);
            built.push((None, automatic));
            column.append(&menu_rule());
            for (region, relays) in regions {
                if !region.is_empty() {
                    let label = gtk::Label::new(Some(&region));
                    label.set_xalign(0.0);
                    label.add_css_class("tmsb-menu-heading");
                    column.append(&label);
                }
                for relay in relays {
                    let row = menu_row(
                        &model.relay_label(&relay.name),
                        detail_of(&relay.name).as_deref(),
                        chosen.as_deref() == Some(&relay.name),
                    );
                    row.set_sensitive(!relay.maintenance);
                    let (weak, pop, name) =
                        (model.downgrade(), menu.downgrade(), relay.name.clone());
                    row.connect_clicked(move |_| {
                        if let Some(model) = weak.upgrade() {
                            model.set_relay_choice(Some(name.clone()));
                        }
                        if let Some(pop) = pop.upgrade() {
                            pop.popdown();
                        }
                    });
                    column.append(&row);
                    built.push((Some(relay.name.clone()), row));
                }
            }
            rows.replace((built_for, built));
        }
    };
    fill_menu(model);
    let weak = model.downgrade();
    menu.connect_opened(move || {
        if let Some(model) = weak.upgrade() {
            model.measure_relays();
        }
    });
    model.subscribe_while(&button, move |model, change| {
        if change == app::CHANGED || change == app::lobby::CHANGED {
            fill_words(model);
            fill_menu(model);
        }
    });
    button
}

// ---- the seal ------------------------------------------------------------

/// Whether the call is sealed, and its word (secrecy.ts): a switch, and the
/// word's field with what it does under it once the switch is on.
pub fn seal_control(model: &AppModel, on: bool) -> (gtk::Box, Rc<std::cell::Cell<bool>>) {
    let sealing = Rc::new(std::cell::Cell::new(on));
    let symbol_box = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    symbol_box.set_size_request(18, -1);
    let title = gtk::Label::new(None);
    title.set_xalign(0.0);
    title.set_hexpand(true);
    let switch = gtk::Switch::builder()
        .active(on)
        .valign(gtk::Align::Center)
        .build();
    switch.update_property(&[gtk::accessible::Property::Label(&t("Seal this call"))]);
    switch.set_widget_name("lobby.seal");
    let head = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    head.append(&symbol_box);
    head.append(&title);
    head.append(&switch);

    let word = gtk::PasswordEntry::builder()
        .placeholder_text(t("Sealing word"))
        .show_peek_icon(true)
        .build();
    word.add_css_class("tmsb-input");
    word.set_text(&model.sealing_word());
    word.update_property(&[gtk::accessible::Property::Label(&t("Sealing word"))]);
    let said = gtk::Label::builder()
        .label(t(
            "Everybody who types the same word can hear each other, and the server carrying the call cannot. Tell people the word some other way.",
        ))
        .wrap(true)
        .xalign(0.0)
        .build();
    said.add_css_class("tmsb-muted");
    said.add_css_class("tmsb-small");
    let below = gtk::Box::new(gtk::Orientation::Vertical, 8);
    below.set_margin_top(10);
    below.append(&word);
    below.append(&said);
    let revealer = unfolding(&below, on);

    let column = gtk::Box::new(gtk::Orientation::Vertical, 0);
    column.append(&head);
    column.append(&revealer);

    let fill = {
        let (symbol_box, title, switch, word) = (
            symbol_box.downgrade(),
            title.downgrade(),
            switch.downgrade(),
            word.downgrade(),
        );
        move || {
            let (Some(symbol_box), Some(title), Some(switch), Some(word)) = (
                symbol_box.upgrade(),
                title.upgrade(),
                switch.upgrade(),
                word.upgrade(),
            ) else {
                return;
            };
            let sealed = switch.is_active() && !word.text().is_empty();
            if let Some(child) = symbol_box.first_child() {
                symbol_box.remove(&child);
            }
            let symbol = glyph(if sealed { Glyph::Lock } else { Glyph::LockOpen }, 16);
            set_class(&symbol, "tmsb-tally-text", sealed);
            set_class(&symbol, "tmsb-muted", !sealed);
            symbol_box.append(&symbol);
            title.set_label(&if sealed {
                t("This call is sealed")
            } else {
                t("Seal this call")
            });
            set_class(&title, "tmsb-tally-text", sealed);
            set_class(&title, "tmsb-strong", sealed);
        }
    };
    fill();
    {
        let (fill, sealing, revealer, word, weak) = (
            fill.clone(),
            sealing.clone(),
            revealer.downgrade(),
            word.downgrade(),
            model.downgrade(),
        );
        switch.connect_active_notify(move |switch| {
            let on = switch.is_active();
            sealing.set(on);
            if let Some(revealer) = revealer.upgrade() {
                revealer.set_reveal_child(on);
            }
            if !on {
                if let Some(word) = word.upgrade() {
                    word.set_text("");
                }
                if let Some(model) = weak.upgrade() {
                    model.set_sealing_word("");
                }
            }
            fill();
        });
    }
    {
        let weak = model.downgrade();
        word.connect_changed(move |word| {
            if let Some(model) = weak.upgrade() {
                model.set_sealing_word(&word.text());
            }
            fill();
        });
    }
    (column, sealing)
}

// ---- the clipboard -------------------------------------------------------

/// Puts `text` on the clipboard.
pub fn copy(text: &str) {
    if let Some(display) = gtk::gdk::Display::default() {
        display.clipboard().set_text(text);
    }
}

/// A round button that copies a link, and says so for a moment.
pub fn copy_button(link: &str, size: i32) -> gtk::Button {
    let glyph_size = (size as f32 * 0.42) as i32;
    let button = round_button(
        &icon("edit-copy-symbolic", glyph_size),
        size,
        &t("Copy link"),
    );
    let link = link.to_owned();
    button.connect_clicked(move |button| {
        copy(&link);
        button.set_child(Some(&icon("object-select-symbolic", glyph_size)));
        button.add_css_class("tmsb-tally-text");
        button.set_tooltip_text(Some(&t("Link copied")));
        let weak = button.downgrade();
        glib::timeout_add_local_once(std::time::Duration::from_millis(1600), move || {
            if let Some(button) = weak.upgrade() {
                button.set_child(Some(&icon("edit-copy-symbolic", glyph_size)));
                button.remove_css_class("tmsb-tally-text");
                button.set_tooltip_text(Some(&t("Copy link")));
            }
        });
    });
    button
}

// ---- the choice of two -----------------------------------------------------

/// Two (or more) choices on one capsule, one of them chosen (the sheet's
/// Start new meeting / Join a meeting).
pub fn segmented(options: &[String], chosen: usize, changed: impl Fn(usize) + 'static) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 2);
    row.add_css_class("tmsb-segmented");
    row.set_homogeneous(true);
    let changed = Rc::new(changed);
    let mut first: Option<gtk::ToggleButton> = None;
    for (index, option) in options.iter().enumerate() {
        let button = gtk::ToggleButton::with_label(option);
        button.set_active(index == chosen);
        if let Some(first) = &first {
            button.set_group(Some(first));
        } else {
            first = Some(button.clone());
        }
        let changed = changed.clone();
        button.connect_toggled(move |button| {
            if button.is_active() {
                changed(index);
            }
        });
        row.append(&button);
    }
    row
}

// ---- the person's words where a person is named ----------------------------

/// "Hello, {name}".
pub fn greeting(name: &str) -> String {
    t_with("Hello, {name}", &[("name", &name)])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_has_one_colour_everywhere() {
        assert_eq!(initial_colour("ann"), initial_colour("ann"));
        assert_eq!(initial_colour(""), 0);
        // The Mac's sum of code points: "ab" = 97 + 98 = 195, 195 % 6 = 3.
        assert_eq!(initial_colour("ab"), 3);
    }
}
