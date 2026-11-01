//! Audio and video: what starts on when somebody joins, the voice as it is,
//! and which camera, microphone and speakers (Settings.swift
//! `MediaSettings`, MediaSettings.kt). A device picked here is remembered at
//! once under the web's own name, as the page's choice is a preference
//! rather than a switch, and "Forget" goes back to the system's.

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use adw::prelude::*;

use super::new_page;
use crate::{
    app::{self, remote, AppModel, DeviceSlot},
    i18n::t,
    media::{Camera, Device, DeviceKind, Microphone},
    render::{Fit, VideoView},
    ui::{kit, lobby::pieces, Context},
};

pub fn page(context: &Context) -> adw::PreferencesPage {
    let model = &context.model;
    let page = new_page(
        "media",
        &t("Audio & video"),
        "camera-web-symbolic",
        &t("What starts on when you join, and which devices."),
    );

    let check = adw::PreferencesGroup::new();
    check.add(&device_check(context));
    page.add(&check);

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
                kit::fade_visible(&row, any_chosen(model));
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

/// The camera as the call will see it, with how loudly the microphone hears
/// you over its foot (the Mac's MediaSettings preview and DeviceCheck): the
/// two questions the web's join screen answers about the two devices
/// (PreJoin.tsx, MicLevel.tsx), asked here where they are chosen. The
/// devices are open only while the page is on screen, since a camera's light
/// that stays on after the page has gone is a light that lies: opened when
/// the page is shown, let go of when another page is or the settings close.
///
/// Before a call the lobby's own preview is drawn where it has one, and a
/// camera is opened for the page where it has not. In a call the camera is
/// the call's and is not opened a second time (a V4L2 camera streams to
/// one reader); the microphone's level is read from a capture of its own.
fn device_check(context: &Context) -> gtk::Widget {
    let view = VideoView::new();
    view.set_fit(Fit::Cover);
    view.set_mirrored(true);
    view.set_hexpand(true);
    view.set_vexpand(true);
    let missing = pieces::icon("camera-video-symbolic", 28);
    missing.add_css_class("tmsb-muted");
    let stage = gtk::Stack::builder()
        .transition_type(gtk::StackTransitionType::Crossfade)
        .transition_duration(crate::theme::motion::SETTLE_MS)
        .build();
    stage.add_named(&missing, Some("off"));
    stage.add_named(&view, Some("camera"));
    stage.set_visible_child_name("off");

    let symbol = pieces::icon("audio-input-microphone-symbolic", 13);
    let hearing = gtk::Label::new(Some(&t("Hearing you")));
    let meter = gtk::Box::new(gtk::Orientation::Horizontal, 2);
    meter.add_css_class("tmsb-meter");
    meter.set_valign(gtk::Align::Center);
    meter.update_property(&[gtk::accessible::Property::Label(&t("Hearing you"))]);
    for _ in 0..SEGMENTS {
        let segment = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        segment.set_size_request(4, 14);
        meter.append(&segment);
    }
    let capsule = gtk::Box::new(gtk::Orientation::Horizontal, 9);
    capsule.add_css_class("tmsb-hearing");
    capsule.append(&symbol);
    capsule.append(&hearing);
    capsule.append(&meter);
    capsule.set_halign(gtk::Align::Start);
    capsule.set_valign(gtk::Align::End);
    capsule.set_margin_start(12);
    capsule.set_margin_bottom(12);

    let overlay = gtk::Overlay::new();
    overlay.set_child(Some(&stage));
    overlay.add_overlay(&capsule);
    overlay.add_css_class("tmsb-check");
    overlay.set_overflow(gtk::Overflow::Hidden);
    overlay.set_size_request(-1, 300);
    overlay.set_widget_name("settings.check");

    let check = Rc::new(Check {
        context: context.clone(),
        view,
        stage: stage.downgrade(),
        symbol: symbol.downgrade(),
        meter: meter.downgrade(),
        state: RefCell::new(None),
        level: Cell::new(0.0),
        generation: Cell::new(0),
    });
    {
        let check = check.clone();
        overlay.connect_map(move |_| check.start());
    }
    {
        let check = check.clone();
        overlay.connect_unmap(move |_| check.stop());
    }
    // Another device picked on this page is the one checked.
    let weak = Rc::downgrade(&check);
    context.model.subscribe_while(&overlay, move |model, change| {
        if change != app::CHANGED {
            return;
        }
        let Some(check) = weak.upgrade() else {
            return;
        };
        let wanted = (
            model.device_choice(DeviceSlot::Camera),
            model.device_choice(DeviceSlot::Microphone),
        );
        let running = check.state.borrow().as_ref().map(|state| state.chosen.clone());
        if running.is_some_and(|running| running != wanted) {
            check.stop();
            check.start();
        }
    });
    overlay.upcast()
}

/// The meter's segments (LevelMeter, segments: 12).
const SEGMENTS: usize = 12;

struct Check {
    context: Context,
    view: VideoView,
    stage: glib::WeakRef<gtk::Stack>,
    symbol: glib::WeakRef<gtk::Image>,
    meter: glib::WeakRef<gtk::Box>,
    state: RefCell<Option<Running>>,
    level: Cell<f64>,
    /// Moved on by every stop, so that an open finishing after the page
    /// went lets its device go.
    generation: Cell<u64>,
}

struct Running {
    chosen: (Option<String>, Option<String>),
    camera: Option<(Rc<dyn Camera>, u64, bool)>,
    microphone: Option<Rc<dyn Microphone>>,
    ticking: Option<glib::SourceId>,
}

impl Check {
    fn start(self: &Rc<Self>) {
        if self.state.borrow().is_some() {
            return;
        }
        let model = self.context.model.clone();
        let chosen = (
            model.device_choice(DeviceSlot::Camera),
            model.device_choice(DeviceSlot::Microphone),
        );
        self.state.replace(Some(Running {
            chosen: chosen.clone(),
            camera: None,
            microphone: None,
            ticking: None,
        }));
        let generation = self.generation.get();
        // The camera: the lobby's own preview where there is one, one of
        // the page's own before a call otherwise, none in a call.
        if let Some(preview) = model.preview_camera() {
            let id = preview.add_preview(self.view.sink());
            self.camera_running(Some((preview, id, false)));
        } else if model.screen().before_call() {
            let (check, media, choice) = (
                Rc::downgrade(self),
                self.context.media.clone(),
                chosen.0.clone(),
            );
            glib::spawn_future_local(async move {
                let opened = media.open_camera(choice).await;
                let Some(check) = check.upgrade() else {
                    if let Ok(camera) = opened {
                        camera.close();
                    }
                    return;
                };
                match opened {
                    Ok(camera) if check.generation.get() == generation => {
                        camera.set_blur(check.context.model.blur());
                        let id = camera.add_preview(check.view.sink());
                        check.camera_running(Some((camera, id, true)));
                    }
                    Ok(camera) => camera.close(),
                    Err(fault) => {
                        crate::app::log(&format!("settings: the camera could not be opened: {fault}"));
                        check.camera_running(None);
                    }
                }
            });
        }
        // The microphone, read on a timer as the meter's needle.
        let (check, media, choice) = (
            Rc::downgrade(self),
            self.context.media.clone(),
            chosen.1.clone(),
        );
        glib::spawn_future_local(async move {
            let opened = media.open_microphone(choice).await;
            let Some(check) = check.upgrade() else {
                if let Ok(microphone) = opened {
                    microphone.close();
                }
                return;
            };
            match opened {
                Ok(microphone) if check.generation.get() == generation => {
                    let weak = Rc::downgrade(&check);
                    let ticking = glib::timeout_add_local(
                        std::time::Duration::from_millis(50),
                        move || match weak.upgrade() {
                            Some(check) => {
                                check.tick();
                                glib::ControlFlow::Continue
                            }
                            None => glib::ControlFlow::Break,
                        },
                    );
                    if let Some(state) = check.state.borrow_mut().as_mut() {
                        state.microphone = Some(microphone);
                        state.ticking = Some(ticking);
                    }
                }
                Ok(microphone) => microphone.close(),
                Err(fault) => {
                    crate::app::log(&format!(
                        "settings: the microphone could not be opened: {fault}"
                    ));
                    if let Some(symbol) = check.symbol.upgrade() {
                        symbol.set_icon_name(Some("microphone-disabled-symbolic"));
                    }
                }
            }
        });
    }

    fn camera_running(&self, camera: Option<(Rc<dyn Camera>, u64, bool)>) {
        let on = camera.is_some();
        if let Some(state) = self.state.borrow_mut().as_mut() {
            state.camera = camera;
        }
        if let Some(stage) = self.stage.upgrade() {
            stage.set_visible_child_name(if on { "camera" } else { "off" });
        }
    }

    /// The level, held at its peak and let fall back gently (the Mac's
    /// DeviceCheck), on the segments and the symbol.
    fn tick(&self) {
        let heard = self
            .state
            .borrow()
            .as_ref()
            .and_then(|state| state.microphone.as_ref().map(|microphone| microphone.level()))
            .unwrap_or(0.0) as f64;
        let shown = heard.clamp(0.0, 1.0);
        let level = self.level.get();
        let level = if shown > level {
            shown
        } else {
            level * 0.8 + shown * 0.2
        };
        self.level.set(level);
        if let Some(meter) = self.meter.upgrade() {
            let mut child = meter.first_child();
            let mut index = 0;
            while let Some(segment) = child {
                child = segment.next_sibling();
                pieces::set_class(&segment, "lit", (index as f64) / (SEGMENTS as f64) < level);
                index += 1;
            }
        }
        if let Some(symbol) = self.symbol.upgrade() {
            pieces::set_class(&symbol, "tmsb-tally-text", level > 0.08);
        }
    }

    fn stop(&self) {
        self.generation.set(self.generation.get() + 1);
        let Some(running) = self.state.take() else {
            return;
        };
        if let Some(ticking) = running.ticking {
            ticking.remove();
        }
        if let Some(microphone) = running.microphone {
            microphone.close();
        }
        if let Some((camera, id, own)) = running.camera {
            camera.remove_preview(id);
            if own {
                camera.close();
            }
        }
        self.view.clear();
        self.level.set(0.0);
        if let Some(stage) = self.stage.upgrade() {
            stage.set_visible_child_name("off");
        }
    }
}
