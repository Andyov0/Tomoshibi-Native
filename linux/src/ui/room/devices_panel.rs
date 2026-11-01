//! The devices panel (DeviceMenu.tsx; the Mac's DevicesPanel): the
//! microphones, speakers and cameras to pick from, and how the voice is
//! sent.

use std::cell::RefCell;

use adw::prelude::*;

use super::{
    panels::{clear, frame, muted, section, PanelView},
    Panel, RoomState,
};
use crate::{app::AppModel, i18n::t, media::DeviceKind};

// ---- devices -------------------------------------------------------------

pub(super) fn build(model: &AppModel) -> PanelView {
    let (outer, body) = frame(model, Panel::Devices, &t("Devices"), &t("Close"));
    let lists = gtk::Box::new(gtk::Orientation::Vertical, 4);
    body.append(&lists);
    let refresh = {
        let lists = lists.downgrade();
        let shown: RefCell<Option<String>> = RefCell::new(None);
        move |model: &AppModel| {
            let Some(lists) = lists.upgrade() else { return };
            let Some(media) = super::media_of(model) else {
                return;
            };
            let room = model.kept::<RoomState>();
            let original = super::original_sound(model);
            // Drawn again only when what it would show changed, so that a
            // press is not lost to a row made again under the pointer.
            let mut said = String::new();
            for kind in [
                DeviceKind::Microphone,
                DeviceKind::Camera,
                DeviceKind::Speaker,
            ] {
                for device in media.devices(kind) {
                    said.push_str(&format!(
                        "{kind:?}{}{}{};",
                        device.id, device.label, device.is_default
                    ));
                }
                said.push_str(&format!("{:?}|", super::chosen_device(model, kind)));
            }
            said.push_str(&format!("{original}{}", crate::i18n::language().code()));
            if shown.borrow().as_deref() == Some(said.as_str()) {
                return;
            }
            shown.replace(Some(said));
            clear(&lists);
            for (kind, heading) in [
                (DeviceKind::Microphone, t("Microphone")),
                (DeviceKind::Camera, t("Camera")),
                (DeviceKind::Speaker, t("Speakers")),
            ] {
                lists.append(&section(&heading));
                let found = media.devices(kind);
                if found.is_empty() {
                    lists.append(&muted(&t("No devices found")));
                    continue;
                }
                let chosen = super::chosen_device(model, kind);
                let chosen_present = chosen
                    .as_ref()
                    .is_some_and(|id| found.iter().any(|device| &device.id == id));
                for device in found {
                    let ticked = if chosen_present {
                        chosen.as_deref() == Some(device.id.as_str())
                    } else {
                        device.is_default
                    };
                    let row = choice_row(&device.label, ticked);
                    row.set_widget_name(&format!("device:{}", device.id));
                    let (weak, id) = (model.downgrade(), device.id.clone());
                    row.connect_clicked(move |_| {
                        if let Some(model) = weak.upgrade() {
                            super::choose_device(&model, kind, Some(id.clone()));
                        }
                    });
                    lists.append(&row);
                }
                if kind == DeviceKind::Microphone {
                    // The web's "original sound" (sound.ts): the voice sent
                    // as it is, without noise suppression or levelling.
                    let row = choice_row(&t("Original sound"), original);
                    row.set_tooltip_text(Some(&t(
                        "No noise suppression or automatic volume. For headphones and music.",
                    )));
                    let weak = model.downgrade();
                    row.connect_clicked(move |_| {
                        if let Some(model) = weak.upgrade() {
                            super::set_original_sound(&model, !super::original_sound(&model));
                        }
                    });
                    lists.append(&row);
                }
            }
            let _ = room;
        }
    };
    PanelView {
        widget: outer.upcast(),
        refresh: Box::new(refresh),
    }
}

/// One of a list to pick from: the chosen one ticked in the signal colour.
fn choice_row(title: &str, ticked: bool) -> gtk::Button {
    let line = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let label = gtk::Label::builder()
        .label(title)
        .xalign(0.0)
        .hexpand(true)
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .build();
    let tick = gtk::Image::from_icon_name("object-select-symbolic");
    tick.add_css_class("tick");
    tick.set_opacity(if ticked { 1.0 } else { 0.0 });
    line.append(&label);
    line.append(&tick);
    let button = gtk::Button::builder().child(&line).build();
    button.add_css_class("tmsb-choice");
    if ticked {
        button.update_state(&[gtk::accessible::State::Checked(
            gtk::AccessibleTristate::True,
        )]);
    }
    button
}
