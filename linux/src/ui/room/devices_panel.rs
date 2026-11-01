//! The devices panel (DeviceMenu.tsx; the Mac's DevicesPanel): which
//! microphone, speakers and camera, how the voice is sent, the background
//! blurred or not, and the language beside them, in the Mac's order.
//!
//! Built once when the panel opens and brought up to date in place: a device
//! plugged in slides into its list, one taken away slides out, and the tick
//! moves by fading, rather than every list being emptied and made again on
//! each change of the room (which also lost a press made just as a row was
//! made again under the pointer). A press moves the tick at once; the
//! microphone and camera move to the device in place (local.rs
//! `choose_device`).
//!
//! The microphone's level is drawn under its list while it is on, with what
//! it means in words (MicLevel.tsx): "Say something" until anything has been
//! heard, "Hearing you" once it has, and "No sound from it" for a device
//! that has given nothing at all two seconds after it was turned on, which a
//! meter that never moves cannot tell from a quiet room.

use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    rc::Rc,
    time::{Duration, Instant},
};

use adw::prelude::*;
use gtk::glib;

use super::{
    panels::{frame, muted, section, PanelView},
    Panel, RoomState,
};
use crate::{
    app::{remote, AppModel},
    i18n::{self, t, Language},
    media::{Device, DeviceKind, Media},
};

/// The CSS of the pieces the media module adds to the call's screen: the
/// devices panel's switches and meter, the share menu, the uplink card and
/// the music panel. In the call's palette, like everything in a call.
pub(super) const MEDIA_CSS: &str = r#"
.tmsb-panel switch, .tmsb-menu switch {
  background-color: alpha(@tmsb_call_wash, 0.18); border: none; box-shadow: none;
  transition: background-color 160ms ease-out;
}
.tmsb-panel switch:checked, .tmsb-menu switch:checked { background-color: @tmsb_call_tally; }
.tmsb-panel switch slider, .tmsb-menu switch slider { background-color: @tmsb_call_fg; box-shadow: none; border: none; }
.tmsb-panel switch:disabled, .tmsb-menu switch:disabled { opacity: 0.4; }
.tmsb-setting { min-height: 40px; }
.tmsb-setting .title { font-size: 13px; }
.tmsb-setting .detail { font-size: 11.5px; color: @tmsb_call_muted; }
image.tmsb-tick { opacity: 0; transition: opacity 160ms ease-out; color: @tmsb_call_fg; }
image.tmsb-tick.on { opacity: 1; }
.tmsb-level-words { font-size: 11.5px; color: @tmsb_call_muted; transition: color 160ms ease-out; }
.tmsb-level-words.heard { color: @tmsb_call_fg; }
button.tmsb-chip {
  min-height: 32px; min-width: 44px; padding: 0 10px; border-radius: 10px; box-shadow: none;
  border: 1px solid @tmsb_call_border; background-color: transparent; color: @tmsb_call_muted;
  font-size: 13px; font-feature-settings: "tnum";
  transition: background-color 120ms ease-out, color 120ms ease-out;
}
button.tmsb-chip:hover { background-color: alpha(@tmsb_call_wash, 0.10); color: @tmsb_call_fg; }
button.tmsb-chip:active { background-color: alpha(@tmsb_call_wash, 0.16); }
button.tmsb-chip.held { border-color: alpha(@tmsb_call_tally, 0.5); background-color: alpha(@tmsb_call_tally, 0.15); color: @tmsb_call_fg; }
.tmsb-menu-describe { font-size: 11.5px; color: @tmsb_call_muted; padding: 2px 10px 0 10px; }
.tmsb-strain {
  background-color: @tmsb_call_surface_high; border-radius: 14px; padding: 14px;
  box-shadow: inset 0 0 0 1px alpha(@tmsb_call_wash, 0.10), 0 8px 24px rgba(0, 0, 0, 0.35);
}
.tmsb-strain .said { font-size: 12.5px; color: @tmsb_call_fg; }
.tmsb-strain .ask { font-size: 12.5px; color: @tmsb_call_muted; }
.tmsb-strain image.gauge { color: @tmsb_call_tally; }
.tmsb-music-now .title { font-size: 14px; font-weight: 600; }
.tmsb-music-now .artists, .tmsb-music-row .artists { font-size: 12px; color: @tmsb_call_muted; }
.tmsb-music-row { min-height: 40px; }
.tmsb-music-row .title { font-size: 13px; }
.tmsb-music-quality { font-size: 11px; color: @tmsb_call_muted; }
.tmsb-music-tabs button.tmsb-chip { min-width: 0; }
.tmsb-music-entry {
  background-color: alpha(@tmsb_call_wash, 0.06); border-radius: 10px;
  border: 1px solid @tmsb_call_border; min-height: 34px; padding: 0 10px;
}
.tmsb-music-entry entry, .tmsb-music-entry entry:focus-within {
  background: none; border: none; box-shadow: none; outline: none; min-height: 32px; padding: 0;
}
.tmsb-music-entry text { background: none; color: @tmsb_call_fg; }
.tmsb-music-progress trough { background-color: alpha(@tmsb_call_wash, 0.14); min-height: 4px; border-radius: 999px; }
.tmsb-music-progress progress { background-color: @tmsb_call_fg; min-height: 4px; border-radius: 999px; }
"#;

/// Says once, for the whole app, when the room should look at the devices
/// again: a device that comes or goes changes the room, and the panel then
/// sees it.
#[derive(Default)]
struct Watched {
    media: RefCell<Option<std::rc::Weak<dyn Media>>>,
}

fn watch_devices(model: &AppModel, media: &Rc<dyn Media>) {
    let watched = model.kept::<Watched>();
    let already = watched
        .media
        .borrow()
        .as_ref()
        .and_then(std::rc::Weak::upgrade)
        .is_some_and(|kept| Rc::ptr_eq(&kept, media));
    if already {
        return;
    }
    watched.media.replace(Some(Rc::downgrade(media)));
    let weak = model.downgrade();
    media.on_devices_changed(Box::new(move || {
        if let Some(model) = weak.upgrade() {
            super::changed(&model);
        }
    }));
}

/// One list of devices, its rows kept by device id.
struct List {
    column: gtk::Box,
    rows: RefCell<Vec<Row>>,
    empty: gtk::Revealer,
}

struct Row {
    id: String,
    revealer: gtk::Revealer,
    label: gtk::Label,
    tick: gtk::Image,
    button: gtk::Button,
}

/// A piece that slides in and out of a column rather than appearing.
fn slide(child: &impl IsA<gtk::Widget>, shown: bool) -> gtk::Revealer {
    gtk::Revealer::builder()
        .transition_type(gtk::RevealerTransitionType::SlideDown)
        .transition_duration(crate::theme::motion::SETTLE_MS)
        .reveal_child(shown)
        .child(child)
        .build()
}

/// One of a list to pick from: the chosen one ticked in white, as the web's
/// menu ticks it (UI sync; it was amber).
pub(super) fn choice_row(title: &str) -> (gtk::Button, gtk::Label, gtk::Image) {
    let line = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let label = gtk::Label::builder()
        .label(title)
        .xalign(0.0)
        .hexpand(true)
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .build();
    let tick = gtk::Image::from_icon_name("object-select-symbolic");
    tick.add_css_class("tick");
    tick.add_css_class("tmsb-tick");
    line.append(&label);
    line.append(&tick);
    let button = gtk::Button::builder().child(&line).build();
    button.add_css_class("tmsb-choice");
    (button, label, tick)
}

fn set_ticked(button: &gtk::Button, tick: &gtk::Image, ticked: bool) {
    if ticked == tick.has_css_class("on") {
        return;
    }
    if ticked {
        tick.add_css_class("on");
    } else {
        tick.remove_css_class("on");
    }
    button.update_state(&[gtk::accessible::State::Checked(if ticked {
        gtk::AccessibleTristate::True
    } else {
        gtk::AccessibleTristate::False
    })]);
}

impl List {
    fn new(empty_words: &str) -> List {
        let column = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let empty_label = muted(empty_words);
        empty_label.set_margin_top(4);
        empty_label.set_margin_bottom(4);
        let empty = slide(&empty_label, false);
        column.append(&empty);
        List {
            column,
            rows: RefCell::new(Vec::new()),
            empty,
        }
    }

    /// Brings the rows to `devices`, ticking `ticked`; `pressed` is called
    /// with a row's device when it is pressed.
    fn show(&self, devices: &[Device], ticked: Option<&str>, pressed: &Rc<dyn Fn(&str)>) {
        let mut rows = self.rows.borrow_mut();
        rows.retain(|row| {
            if devices.iter().any(|device| device.id == row.id) {
                return true;
            }
            let (column, gone) = (self.column.downgrade(), row.revealer.downgrade());
            row.revealer.connect_child_revealed_notify(move |revealer| {
                if !revealer.is_child_revealed() {
                    if let (Some(column), Some(gone)) = (column.upgrade(), gone.upgrade()) {
                        if gone.parent().as_ref() == Some(column.upcast_ref()) {
                            column.remove(&gone);
                        }
                    }
                }
            });
            row.revealer.set_reveal_child(false);
            if !adw::is_animations_enabled(&row.revealer) {
                self.column.remove(&row.revealer);
            }
            false
        });
        for device in devices {
            if let Some(row) = rows.iter().find(|row| row.id == device.id) {
                if row.label.label() != device.label {
                    row.label.set_label(&device.label);
                }
                continue;
            }
            let (button, label, tick) = choice_row(&device.label);
            button.set_widget_name(&format!("device:{}", device.id));
            let (id, pressed) = (device.id.clone(), pressed.clone());
            button.connect_clicked(move |_| pressed(&id));
            let revealer = slide(&button, false);
            self.column.append(&revealer);
            revealer.set_reveal_child(true);
            rows.push(Row {
                id: device.id.clone(),
                revealer,
                label,
                tick,
                button,
            });
        }
        for row in rows.iter() {
            set_ticked(&row.button, &row.tick, ticked == Some(row.id.as_str()));
        }
        self.empty.set_reveal_child(devices.is_empty());
    }
}

/// A setting with words beside it and a switch: the original sound, the
/// blur.
fn setting(title: &str, detail: Option<&str>) -> (gtk::Box, gtk::Label, gtk::Label, gtk::Switch) {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    row.add_css_class("tmsb-setting");
    row.set_margin_start(10);
    row.set_margin_end(6);
    let words = gtk::Box::new(gtk::Orientation::Vertical, 1);
    words.set_valign(gtk::Align::Center);
    words.set_hexpand(true);
    let heading = gtk::Label::builder()
        .label(title)
        .xalign(0.0)
        .wrap(true)
        .build();
    heading.add_css_class("title");
    let said = gtk::Label::builder()
        .label(detail.unwrap_or_default())
        .xalign(0.0)
        .wrap(true)
        .build();
    said.add_css_class("detail");
    said.set_visible(detail.is_some());
    words.append(&heading);
    words.append(&said);
    let switch = gtk::Switch::builder().valign(gtk::Align::Center).build();
    switch.update_property(&[gtk::accessible::Property::Label(title)]);
    row.append(&words);
    row.append(&switch);
    (row, heading, said, switch)
}

/// The microphone's level, drawn in segments as the Mac draws it.
fn meter() -> (gtk::DrawingArea, Rc<Cell<f64>>) {
    let shown = Rc::new(Cell::new(0.0));
    let area = gtk::DrawingArea::builder()
        .content_height(8)
        .hexpand(true)
        .valign(gtk::Align::Center)
        .build();
    area.update_property(&[gtk::accessible::Property::Label(&t("Microphone level"))]);
    let level = shown.clone();
    area.set_draw_func(move |area, cairo, width, height| {
        const SEGMENTS: i32 = 18;
        let gap = 3.0;
        let each = ((width as f64 - gap * (SEGMENTS - 1) as f64) / SEGMENTS as f64).max(1.0);
        let lit = (level.get() * SEGMENTS as f64).round() as i32;
        let fg = area.color();
        for index in 0..SEGMENTS {
            let x = index as f64 * (each + gap);
            let alpha = if index < lit { fg.alpha() as f64 } else { 0.14 };
            cairo.set_source_rgba(fg.red() as f64, fg.green() as f64, fg.blue() as f64, alpha);
            let radius = (height as f64 / 2.0).min(each / 2.0);
            cairo.new_sub_path();
            cairo.arc(
                x + radius,
                height as f64 / 2.0,
                radius,
                std::f64::consts::FRAC_PI_2,
                3.0 * std::f64::consts::FRAC_PI_2,
            );
            cairo.arc(
                x + each - radius,
                height as f64 / 2.0,
                radius,
                -std::f64::consts::FRAC_PI_2,
                std::f64::consts::FRAC_PI_2,
            );
            cairo.close_path();
            let _ = cairo.fill();
        }
    });
    (area, shown)
}

/// How full the meter is for an RMS level (the Mac's `MicrophoneLevel`):
/// in decibels from -60 to -10, so that a voice is in the upper half.
fn shown_level(rms: f32) -> f64 {
    if rms <= 0.0 {
        return 0.0;
    }
    ((20.0 * f64::from(rms).log10() + 60.0) / 50.0).clamp(0.0, 1.0)
}

/// MicLevel.tsx's 0.06: anything above it has been heard.
const HEARD: f64 = 0.06;

pub(super) fn build(model: &AppModel) -> PanelView {
    crate::theme::add_module_css("room-media", MEDIA_CSS);
    let (outer, body) = frame(model, Panel::Devices, &t("Devices"), &t("Close"));
    if let Some(media) = super::media_of(model) {
        watch_devices(model, &media);
    }

    let pick = |kind: DeviceKind| -> Rc<dyn Fn(&str)> {
        let weak = model.downgrade();
        Rc::new(move |id: &str| {
            if let Some(model) = weak.upgrade() {
                super::choose_device(&model, kind, Some(id.to_owned()));
            }
        })
    };

    // The microphone: its list, its level while it is on, and the original
    // sound beneath, a property of the voice as the blur is of the picture.
    let microphone_heading = section(&t("Microphone"));
    let microphones = List::new(&t("No devices found"));
    let (meter_area, meter_level) = meter();
    let meter_words = gtk::Label::builder().xalign(1.0).build();
    meter_words.add_css_class("tmsb-level-words");
    let meter_row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    meter_row.set_margin_start(10);
    meter_row.set_margin_end(10);
    meter_row.set_margin_top(6);
    meter_row.set_margin_bottom(6);
    let wave = gtk::Image::from_icon_name("audio-input-microphone-symbolic");
    wave.add_css_class("dim-label");
    meter_row.append(&wave);
    meter_row.append(&meter_area);
    meter_row.append(&meter_words);
    let meter_slide = slide(&meter_row, false);
    let (original_row, original_title, original_detail, original_switch) = setting(
        &t("Original sound"),
        Some(&t(
            "No noise suppression or automatic volume. For headphones and music.",
        )),
    );
    original_switch.set_widget_name("devices.original");
    body.append(&microphone_heading);
    body.append(&microphones.column);
    body.append(&meter_slide);
    body.append(&original_row);

    let speakers_heading = section(&t("Speakers"));
    let speakers = List::new(&t("No devices found"));
    body.append(&speakers_heading);
    body.append(&speakers.column);

    let camera_heading = section(&t("Camera"));
    let cameras = List::new(&t("No devices found"));
    let (blur_row, blur_title, _, blur_switch) = setting(&t("Blur my background"), None);
    blur_switch.set_widget_name("devices.blur");
    let blur_slide = slide(&blur_row, false);
    body.append(&camera_heading);
    body.append(&cameras.column);
    body.append(&blur_slide);

    // The language, as the Mac's panel has it beside the devices.
    let language_heading = section(&t("Language"));
    body.append(&language_heading);
    let languages = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let mut language_rows = Vec::new();
    for language in Language::ALL {
        let (button, _, tick) = choice_row(language.display_name());
        button.set_widget_name(&format!("language:{}", language.code()));
        let weak = model.downgrade();
        button.connect_clicked(move |_| {
            if let Some(model) = weak.upgrade() {
                model.preferences().set(i18n::STORAGE_KEY, language.code());
                i18n::choose(Some(language));
                super::changed(&model);
            }
        });
        languages.append(&button);
        language_rows.push((language, button, tick));
    }
    body.append(&languages);

    // The switches, each told apart from a change made here by `quiet`.
    let quiet = Rc::new(Cell::new(false));
    {
        let (weak, quiet) = (model.downgrade(), quiet.clone());
        original_switch.connect_active_notify(move |switch| {
            if quiet.get() {
                return;
            }
            if let Some(model) = weak.upgrade() {
                super::set_original_sound(&model, switch.is_active());
            }
        });
    }
    {
        let (weak, quiet) = (model.downgrade(), quiet.clone());
        blur_switch.connect_active_notify(move |switch| {
            if quiet.get() {
                return;
            }
            if let Some(model) = weak.upgrade() {
                model.set_blur(switch.is_active());
            }
        });
    }

    // The meter, read thirty times a second while the panel is there.
    let heard = Rc::new(Cell::new(false));
    let opened: Rc<Cell<Option<Instant>>> = Rc::new(Cell::new(None));
    {
        let (weak, area, words) = (
            model.downgrade(),
            meter_area.downgrade(),
            meter_words.downgrade(),
        );
        let (level_cell, heard, opened) = (meter_level.clone(), heard.clone(), opened.clone());
        glib::timeout_add_local(Duration::from_millis(33), move || {
            let (Some(model), Some(area), Some(words)) =
                (weak.upgrade(), area.upgrade(), words.upgrade())
            else {
                return glib::ControlFlow::Break;
            };
            let room = model.kept::<RoomState>();
            let on = room.local.microphone_on.get();
            if !on {
                opened.set(None);
                heard.set(false);
                return glib::ControlFlow::Continue;
            }
            let since = opened.get().unwrap_or_else(Instant::now);
            opened.set(Some(since));
            let rms = room
                .local
                .microphone
                .borrow()
                .as_ref()
                .map(|microphone| microphone.level())
                .unwrap_or(0.0);
            let shown = shown_level(rms);
            if shown > HEARD {
                heard.set(true);
            }
            if (shown - level_cell.get()).abs() > 0.01 {
                level_cell.set(shown);
                area.queue_draw();
            }
            // Nothing at all two seconds after it came on: a device that
            // gives nothing, not a quiet room.
            let silent = rms == 0.0 && since.elapsed() > Duration::from_secs(2);
            let said = if silent {
                t("No sound from it")
            } else if heard.get() {
                t("Hearing you")
            } else {
                t("Say something")
            };
            if words.label() != said {
                words.set_label(&said);
            }
            if heard.get() && !silent {
                words.add_css_class("heard");
            } else {
                words.remove_css_class("heard");
            }
            glib::ControlFlow::Continue
        });
    }

    let pick_microphone = pick(DeviceKind::Microphone);
    let pick_speaker = pick(DeviceKind::Speaker);
    let pick_camera = pick(DeviceKind::Camera);
    let headings: HashMap<&'static str, gtk::Label> = HashMap::from([
        ("Microphone", microphone_heading),
        ("Speakers", speakers_heading),
        ("Camera", camera_heading),
        ("Language", language_heading),
    ]);
    let refresh = move |model: &AppModel| {
        let Some(media) = super::media_of(model) else {
            return;
        };
        let room = model.kept::<RoomState>();
        for (phrase, heading) in &headings {
            let said = t(phrase);
            if heading.label() != said {
                heading.set_label(&said);
            }
        }
        // What each list ticks: the device picked where it is still there,
        // the system's default otherwise; for the camera, the one the call
        // is sending where it is on.
        let ticked = |kind: DeviceKind, found: &[Device]| -> Option<String> {
            let chosen = super::chosen_device(model, kind)
                .filter(|id| found.iter().any(|device| &device.id == id));
            let sending = (kind == DeviceKind::Camera)
                .then(|| {
                    room.local
                        .camera
                        .borrow()
                        .as_ref()
                        .and_then(|camera| camera.device())
                })
                .flatten()
                .filter(|id| found.iter().any(|device| &device.id == id));
            sending.or(chosen).or_else(|| {
                found
                    .iter()
                    .find(|device| device.is_default)
                    .map(|device| device.id.clone())
            })
        };
        for (kind, list, pressed) in [
            (DeviceKind::Microphone, &microphones, &pick_microphone),
            (DeviceKind::Speaker, &speakers, &pick_speaker),
            (DeviceKind::Camera, &cameras, &pick_camera),
        ] {
            let found = media.devices(kind);
            let tick = ticked(kind, &found);
            list.show(&found, tick.as_deref(), pressed);
        }
        meter_slide.set_reveal_child(room.local.microphone_on.get());
        quiet.set(true);
        let original = super::original_sound(model);
        if original_switch.is_active() != original {
            original_switch.set_active(original);
        }
        let blur_offered = remote::allows(model, remote::feature::BACKGROUND_BLUR);
        blur_slide.set_reveal_child(blur_offered);
        if blur_switch.is_active() != model.blur_chosen() {
            blur_switch.set_active(model.blur_chosen());
        }
        quiet.set(false);
        original_title.set_label(&t("Original sound"));
        original_detail.set_label(&t(
            "No noise suppression or automatic volume. For headphones and music.",
        ));
        blur_title.set_label(&t("Blur my background"));
        // The language on screen, as the Mac's panel ticks it.
        let current = i18n::language();
        for (language, button, tick) in &language_rows {
            set_ticked(button, tick, current == *language);
        }
    };
    PanelView {
        widget: outer.upcast(),
        refresh: Box::new(refresh),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_meter_is_drawn_in_decibels_as_the_mac_draws_it() {
        assert_eq!(shown_level(0.0), 0.0);
        assert!(shown_level(0.001) < 1e-4, "-60 dBFS is empty");
        assert!(
            (shown_level(0.316_227_8) - 1.0).abs() < 1e-4,
            "-10 dBFS is full"
        );
        // A voice at -26 dBFS is in the upper half.
        assert!(shown_level(0.05) > 0.6);
        // The hiss of a quiet room (-62 dBFS) is under the line, a whisper
        // (-50) over it.
        assert!(shown_level(0.000_8) < HEARD);
        assert!(shown_level(0.003_2) > HEARD);
    }
}
