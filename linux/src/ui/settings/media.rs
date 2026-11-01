//! Audio and video: what starts on when somebody joins, the voice as it is,
//! and which camera, microphone and speakers (Settings.swift
//! `MediaSettings`, MediaSettings.kt). A device picked here is remembered at
//! once under the web's own name, as the page's choice is a preference
//! rather than a switch, and "Forget" goes back to the system's.

use adw::prelude::*;

use super::new_page;
use crate::{
    app::{self, remote, AppModel, DeviceSlot},
    i18n::t,
    media::{Device, DeviceKind},
    ui::Context,
};

pub fn page(context: &Context) -> adw::PreferencesPage {
    let model = &context.model;
    let page = new_page(
        "media",
        &t("Audio & video"),
        "camera-web-symbolic",
        &t("What starts on when you join, and which devices."),
    );

    let joining = adw::PreferencesGroup::builder()
        .title(t("When you join"))
        .build();
    let microphone = adw::SwitchRow::builder()
        .title(t("Start with the microphone on"))
        .active(model.start_with_microphone())
        .build();
    microphone.set_widget_name("settings.start-microphone");
    let weak = model.downgrade();
    microphone.connect_active_notify(move |row| {
        if let Some(model) = weak.upgrade() {
            model.set_start_with_microphone(row.is_active());
        }
    });
    let camera = adw::SwitchRow::builder()
        .title(t("Start with the camera on"))
        .active(model.start_with_camera())
        .build();
    let weak = model.downgrade();
    camera.connect_active_notify(move |row| {
        if let Some(model) = weak.upgrade() {
            model.set_start_with_camera(row.is_active());
        }
    });
    let original = adw::SwitchRow::builder()
        .title(t("Original sound"))
        .subtitle(t(
            "No noise suppression or automatic volume. For headphones and music.",
        ))
        .active(model.original_sound())
        .build();
    let weak = model.downgrade();
    original.connect_active_notify(move |row| {
        if let Some(model) = weak.upgrade() {
            model.set_original_sound(row.is_active());
        }
    });
    joining.add(&microphone);
    joining.add(&camera);
    joining.add(&original);
    page.add(&joining);

    let devices = adw::PreferencesGroup::builder().title(t("Devices")).build();
    devices.add(&device_row(
        context,
        DeviceSlot::Camera,
        DeviceKind::Camera,
        &t("Camera"),
    ));
    if remote::allows(model, remote::feature::BACKGROUND_BLUR) {
        let blur = adw::SwitchRow::builder()
            .title(t("Blur my background"))
            .active(model.blur_chosen())
            .build();
        let weak = model.downgrade();
        blur.connect_active_notify(move |row| {
            if let Some(model) = weak.upgrade() {
                model.set_blur(row.is_active());
            }
        });
        devices.add(&blur);
    }
    devices.add(&device_row(
        context,
        DeviceSlot::Microphone,
        DeviceKind::Microphone,
        &t("Microphone"),
    ));
    devices.add(&device_row(
        context,
        DeviceSlot::Speakers,
        DeviceKind::Speaker,
        &t("Speakers"),
    ));
    let forget = gtk::Button::with_label(&t("Forget"));
    forget.add_css_class("flat");
    let forget_row = adw::ActionRow::builder()
        .title(t("Calls start on the microphone, camera and speakers you last picked in one, while they are plugged in."))
        .build();
    forget_row.add_css_class("property");
    forget.set_valign(gtk::Align::Center);
    forget_row.add_suffix(&forget);
    let any_chosen = |model: &AppModel| {
        DeviceSlot::ALL
            .iter()
            .any(|slot| model.device_choice(*slot).is_some())
    };
    forget_row.set_visible(any_chosen(model));
    let weak = model.downgrade();
    forget.connect_clicked(move |_| {
        if let Some(model) = weak.upgrade() {
            model.forget_devices();
        }
    });
    devices.add(&forget_row);
    let row_weak = forget_row.downgrade();
    model.subscribe_while(&forget_row, move |model, change| {
        if change == app::CHANGED {
            if let Some(row) = row_weak.upgrade() {
                row.set_visible(any_chosen(model));
            }
        }
    });
    page.add(&devices);
    page
}

/// A device of one kind, "Same as the system" first, the one picked chosen
/// where it is still plugged in.
fn device_row(context: &Context, slot: DeviceSlot, kind: DeviceKind, title: &str) -> adw::ComboRow {
    let model = &context.model;
    let combo = adw::ComboRow::builder().title(title).build();
    combo.set_widget_name(match slot {
        DeviceSlot::Camera => "settings.camera",
        DeviceSlot::Microphone => "settings.microphone",
        DeviceSlot::Speakers => "settings.speakers",
    });
    let fill = {
        let (combo, media) = (combo.downgrade(), context.media.clone());
        move |model: &AppModel| {
            let Some(combo) = combo.upgrade() else {
                return;
            };
            let devices: Vec<Device> = media.devices(kind);
            let mut names = vec![t("Same as the system")];
            names.extend(devices.iter().map(|device| device.label.clone()));
            let list = gtk::StringList::new(&names.iter().map(String::as_str).collect::<Vec<_>>());
            let chosen = model
                .device_choice(slot)
                .and_then(|id| devices.iter().position(|device| device.id == id))
                .map(|index| index as u32 + 1)
                .unwrap_or(0);
            // SAFETY: the key is only ever set here, with this type.
            unsafe {
                combo.set_data("tmsb-devices", devices);
            }
            combo.set_model(Some(&list));
            combo.set_selected(chosen);
        }
    };
    fill(model);
    let weak = model.downgrade();
    combo.connect_selected_notify(move |combo| {
        let Some(model) = weak.upgrade() else {
            return;
        };
        // SAFETY: set above with this type.
        let id = unsafe {
            combo
                .data::<Vec<Device>>("tmsb-devices")
                .and_then(|devices| {
                    let devices = devices.as_ref();
                    (combo.selected() as usize)
                        .checked_sub(1)
                        .and_then(|index| devices.get(index))
                        .map(|device| device.id.clone())
                })
        };
        if model.device_choice(slot) != id {
            model.choose_device(slot, id.as_deref());
        }
    });
    let fill_again = fill.clone();
    context.media.on_devices_changed(Box::new({
        let weak = model.downgrade();
        move || {
            if let Some(model) = weak.upgrade() {
                fill_again(&model);
            }
        }
    }));
    combo
}
