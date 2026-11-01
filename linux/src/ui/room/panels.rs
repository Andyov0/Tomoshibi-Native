//! The panels beside the call: messages (ChatPanel.tsx), sound
//! (SoundPanel.tsx, each person's volume), devices (DeviceMenu.tsx) and
//! running the room (HostPanel.tsx, the door among it). Opaque, with their
//! close button 32 drawn at the top right (UI sync); page.rs slides them in
//! from the side, or up from the foot where they cover the call.
//!
//! Each is built once when it opens and brought up to date by its
//! `refresh`, which reads the room's state like everything else on the
//! call's screen.

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use adw::prelude::*;
use gtk::glib;
use tomoshibi_core::call::CallState;

use super::{
    glyph::{self, Glyph},
    page::menu_row,
    talk::{named, Line},
    Hearing, Panel, RoomState,
};
use crate::{
    app::AppModel,
    i18n::{t, t_with},
    media::DeviceKind,
};

/// A panel and how it is brought up to date.
pub struct PanelView {
    pub widget: gtk::Widget,
    pub refresh: Box<dyn Fn(&AppModel)>,
}

pub fn build(model: &AppModel, panel: Panel) -> PanelView {
    match panel {
        Panel::Messages => messages(model),
        Panel::Sound => sound(model),
        Panel::Devices => devices(model),
        Panel::Host => host(model),
    }
}

/// The heading, the close button, and the body beneath, scrolled.
fn frame(model: &AppModel, panel: Panel, title: &str, close_label: &str) -> (gtk::Box, gtk::Box) {
    let outer = gtk::Box::new(gtk::Orientation::Vertical, 0);
    outer.set_vexpand(true);
    let head = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    head.set_margin_start(16);
    head.set_margin_end(8);
    head.set_margin_top(8);
    head.set_margin_bottom(4);
    let heading = gtk::Label::builder()
        .label(title)
        .xalign(0.0)
        .hexpand(true)
        .build();
    heading.add_css_class("tmsb-panel-title");
    let close = gtk::Button::from_icon_name("window-close-symbolic");
    close.add_css_class("tmsb-close");
    close.set_tooltip_text(Some(close_label));
    close.update_property(&[gtk::accessible::Property::Label(close_label)]);
    close.set_widget_name("panel.close");
    let weak = model.downgrade();
    close.connect_clicked(move |_| {
        if let Some(model) = weak.upgrade() {
            super::toggle_panel(&model, panel);
        }
    });
    head.append(&heading);
    head.append(&close);
    outer.append(&head);
    let body = gtk::Box::new(gtk::Orientation::Vertical, 8);
    body.set_margin_start(16);
    body.set_margin_end(16);
    body.set_margin_bottom(16);
    let scroller = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vexpand(true)
        .propagate_natural_height(true)
        .child(&body)
        .build();
    outer.append(&scroller);
    outer.set_widget_name(match panel {
        Panel::Messages => "panel.messages",
        Panel::Sound => "panel.sound",
        Panel::Devices => "panel.devices",
        Panel::Host => "panel.host",
    });
    (outer, body)
}

fn muted(text: &str) -> gtk::Label {
    let label = gtk::Label::builder()
        .label(text)
        .xalign(0.0)
        .wrap(true)
        .build();
    label.add_css_class("tmsb-muted");
    label
}

fn section(text: &str) -> gtk::Label {
    let label = gtk::Label::builder()
        .label(text)
        .xalign(0.0)
        .margin_top(6)
        .build();
    label.add_css_class("tmsb-panel-section");
    label
}

fn clear(container: &gtk::Box) {
    while let Some(child) = container.first_child() {
        container.remove(&child);
    }
}

fn pill(label: &str) -> gtk::Button {
    let button = gtk::Button::with_label(label);
    button.add_css_class("tmsb-panel-button");
    button
}

// ---- messages ------------------------------------------------------------

fn messages(model: &AppModel) -> PanelView {
    let (outer, body) = frame(model, Panel::Messages, &t("Messages"), &t("Close messages"));
    let lines = gtk::Box::new(gtk::Orientation::Vertical, 10);
    lines.set_widget_name("messages.lines");
    let empty = muted(&t("Messages disappear when the call ends."));
    empty.set_margin_top(6);
    body.append(&empty);
    body.append(&lines);

    // The box: the draft kept with the call, Enter to send.
    let entry = gtk::Entry::builder()
        .placeholder_text(t("Say something"))
        .hexpand(true)
        .has_frame(false)
        .build();
    entry.set_widget_name("messages.say");
    entry.update_property(&[gtk::accessible::Property::Label(&t("Say something"))]);
    let send = gtk::Button::new();
    send.set_child(Some(&Glyph::new(glyph::ARROW_UP, 14)));
    send.add_css_class("tmsb-send");
    send.set_valign(gtk::Align::Center);
    send.set_tooltip_text(Some(&t("Send")));
    send.update_property(&[gtk::accessible::Property::Label(&t("Send"))]);
    send.set_widget_name("messages.send");
    let say = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    say.add_css_class("tmsb-say");
    say.set_margin_start(12);
    say.set_margin_end(12);
    say.set_margin_bottom(12);
    say.append(&entry);
    say.append(&send);
    outer.append(&say);

    let quiet = Rc::new(Cell::new(false));
    let weak = model.downgrade();
    let hush = quiet.clone();
    entry.connect_changed(move |entry| {
        if hush.get() {
            return;
        }
        if let Some(model) = weak.upgrade() {
            let room = model.kept::<RoomState>();
            let mut view = room.view.borrow_mut();
            view.draft = entry.text().to_string();
            if !view.draft.is_empty() {
                view.chat_failed = false;
            }
            drop(view);
            super::changed(&model);
        }
    });
    let weak = model.downgrade();
    entry.connect_activate(move |_| {
        if let Some(model) = weak.upgrade() {
            super::send_draft(&model);
        }
    });
    let weak = model.downgrade();
    send.connect_clicked(move |_| {
        if let Some(model) = weak.upgrade() {
            super::send_draft(&model);
        }
    });
    // The box takes the keys as the panel opens.
    let focus = entry.downgrade();
    glib::idle_add_local_once(move || {
        if let Some(entry) = focus.upgrade() {
            entry.grab_focus();
        }
    });

    let shown: Rc<RefCell<Vec<String>>> = Rc::default();
    let scroller = body.parent().and_then(|viewport| viewport.parent());
    let refresh = {
        let (lines, empty, entry, send, say) = (
            lines.downgrade(),
            empty.downgrade(),
            entry.downgrade(),
            send.downgrade(),
            say.downgrade(),
        );
        let scroller = scroller.map(|scroller| scroller.downgrade());
        move |model: &AppModel| {
            let (Some(lines), Some(empty), Some(entry), Some(send), Some(say)) = (
                lines.upgrade(),
                empty.upgrade(),
                entry.upgrade(),
                send.upgrade(),
                say.upgrade(),
            ) else {
                return;
            };
            let room = model.kept::<RoomState>();
            let talk = room.talk.borrow();
            let messages: Vec<Line> = talk
                .as_ref()
                .map(|talk| talk.messages.clone())
                .unwrap_or_default();
            let ids: Vec<String> = messages.iter().map(|line| line.id.clone()).collect();
            let mut was = shown.borrow_mut();
            // Only ever added to, but for the oldest going past the five
            // hundred kept: then it is drawn again.
            let appended = was.len() <= ids.len() && ids[..was.len()] == was[..];
            if !appended {
                clear(&lines);
                was.clear();
            }
            let added = ids.len() > was.len();
            for line in &messages[was.len()..] {
                lines.append(&line_widget(line));
            }
            *was = ids;
            empty.set_visible(messages.is_empty());
            if added {
                if let Some(scroller) = scroller.as_ref().and_then(|scroller| scroller.upgrade()) {
                    let scroller = scroller.downcast::<gtk::ScrolledWindow>().ok();
                    glib::idle_add_local_once(move || {
                        if let Some(scroller) = scroller {
                            let adjustment = scroller.vadjustment();
                            adjustment.set_value(adjustment.upper());
                        }
                    });
                }
            }
            let view = room.view.borrow();
            if entry.text() != view.draft {
                quiet.set(true);
                entry.set_text(&view.draft);
                entry.set_position(-1);
                quiet.set(false);
            }
            let connected = room.state.get() == Some(CallState::Connected);
            send.set_sensitive(!view.draft.trim().is_empty() && !view.sending && connected);
            if view.chat_failed {
                say.add_css_class("failed");
            } else {
                say.remove_css_class("failed");
            }
        }
    };
    PanelView {
        widget: outer.upcast(),
        refresh: Box::new(refresh),
    }
}

fn line_widget(line: &Line) -> gtk::Widget {
    let row = gtk::Box::new(gtk::Orientation::Vertical, 2);
    if line.mine {
        row.add_css_class("tmsb-line-mine");
    }
    let name = gtk::Label::builder()
        .label(named(&line.name, &line.identity))
        .xalign(0.0)
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .build();
    name.add_css_class("tmsb-line-name");
    let bubble = gtk::Box::new(gtk::Orientation::Vertical, 0);
    bubble.add_css_class("tmsb-line-bubble");
    bubble.set_halign(gtk::Align::Start);
    let text = gtk::Label::builder()
        .label(&line.text)
        .xalign(0.0)
        .wrap(true)
        .wrap_mode(gtk::pango::WrapMode::WordChar)
        .selectable(true)
        .max_width_chars(40)
        .build();
    text.add_css_class("tmsb-line-text");
    bubble.append(&text);
    row.append(&name);
    row.append(&bubble);
    row.upcast()
}

// ---- sound -------------------------------------------------------------

fn sound(model: &AppModel) -> PanelView {
    let (outer, body) = frame(model, Panel::Sound, &t("Sound"), &t("Close sound"));
    let rows = gtk::Box::new(gtk::Orientation::Vertical, 10);
    let nobody = muted(&t("Nobody else is here."));
    body.append(&nobody);
    body.append(&rows);
    let shown: Rc<RefCell<Vec<(String, String)>>> = Rc::default();
    let sliders: Rc<RefCell<Vec<Slider>>> = Rc::default();
    let refresh = {
        let (rows, nobody) = (rows.downgrade(), nobody.downgrade());
        move |model: &AppModel| {
            let (Some(rows), Some(nobody)) = (rows.upgrade(), nobody.upgrade()) else {
                return;
            };
            let room = model.kept::<RoomState>();
            let people: Vec<(String, String)> = room
                .talk
                .borrow()
                .as_ref()
                .map(|talk| {
                    talk.people
                        .iter()
                        .map(|person| (person.id.clone(), person.label()))
                        .collect()
                })
                .unwrap_or_default();
            nobody.set_visible(people.is_empty());
            if *shown.borrow() != people {
                clear(&rows);
                sliders.borrow_mut().clear();
                for (identity, name) in &people {
                    let (row, scale, mute, quiet) = volume_row(model, identity, name);
                    rows.append(&row);
                    sliders
                        .borrow_mut()
                        .push((identity.clone(), scale, mute, quiet));
                }
                shown.replace(people);
            }
            let view = room.view.borrow();
            for (identity, scale, mute, quiet) in sliders.borrow().iter() {
                let hearing = view.hearing.get(identity).copied().unwrap_or_default();
                let value = (hearing.level * 100.0).round() as f64;
                if (scale.value() - value).abs() > 0.5 {
                    quiet.set(true);
                    scale.set_value(value);
                    quiet.set(false);
                }
                mute.set_icon_name(if hearing.blocked || hearing.level <= 0.0 {
                    "audio-volume-muted-symbolic"
                } else {
                    "audio-volume-high-symbolic"
                });
                if hearing.blocked {
                    mute.add_css_class("held");
                } else {
                    mute.remove_css_class("held");
                }
                scale.set_sensitive(!hearing.blocked);
            }
        }
    };
    PanelView {
        widget: outer.upcast(),
        refresh: Box::new(refresh),
    }
}

/// One person's slider: whose, the slider, the mute beside it, and the
/// flag that keeps a value set from here from being taken as a press.
type Slider = (String, gtk::Scale, gtk::Button, Rc<Cell<bool>>);

fn volume_row(
    model: &AppModel,
    identity: &str,
    name: &str,
) -> (gtk::Box, gtk::Scale, gtk::Button, Rc<Cell<bool>>) {
    let row = gtk::Box::new(gtk::Orientation::Vertical, 2);
    row.add_css_class("tmsb-panel-row");
    let label = gtk::Label::builder()
        .label(name)
        .xalign(0.0)
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .build();
    let line = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let scale = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 100.0, 1.0);
    scale.set_hexpand(true);
    scale.set_draw_value(false);
    scale.set_value(100.0);
    let said = t_with("{name}'s volume", &[("name", &name)]);
    scale.update_property(&[gtk::accessible::Property::Label(&said)]);
    scale.set_widget_name(&format!("volume:{identity}"));
    let mute = gtk::Button::from_icon_name("audio-volume-high-symbolic");
    mute.add_css_class("tmsb-close");
    let mute_said = t_with("Mute {name}", &[("name", &name)]);
    mute.set_tooltip_text(Some(&mute_said));
    mute.update_property(&[gtk::accessible::Property::Label(&mute_said)]);
    line.append(&scale);
    line.append(&mute);
    row.append(&label);
    row.append(&line);
    let quiet = Rc::new(Cell::new(false));
    let (weak, who, hush) = (model.downgrade(), identity.to_owned(), quiet.clone());
    scale.connect_value_changed(move |scale| {
        if hush.get() {
            return;
        }
        if let Some(model) = weak.upgrade() {
            let blocked = model
                .kept::<RoomState>()
                .view
                .borrow()
                .hearing
                .get(&who)
                .is_some_and(|hearing| hearing.blocked);
            super::set_hearing(
                &model,
                &who,
                Hearing {
                    level: (scale.value() / 100.0) as f32,
                    blocked,
                },
            );
        }
    });
    let (weak, who) = (model.downgrade(), identity.to_owned());
    mute.connect_clicked(move |_| {
        if let Some(model) = weak.upgrade() {
            let now = model
                .kept::<RoomState>()
                .view
                .borrow()
                .hearing
                .get(&who)
                .copied()
                .unwrap_or_default();
            super::set_hearing(
                &model,
                &who,
                Hearing {
                    blocked: !now.blocked,
                    ..now
                },
            );
        }
    });
    (row, scale, mute, quiet)
}

// ---- devices -------------------------------------------------------------

fn devices(model: &AppModel) -> PanelView {
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

// ---- running the room ----------------------------------------------------

fn host(model: &AppModel) -> PanelView {
    let (outer, body) = frame(model, Panel::Host, &t("Running this room"), &t("Close"));
    let content = gtk::Box::new(gtk::Orientation::Vertical, 8);
    body.append(&content);
    let shown: RefCell<Option<String>> = RefCell::new(None);
    let refresh = {
        let content = content.downgrade();
        move |model: &AppModel| {
            let Some(content) = content.upgrade() else {
                return;
            };
            let room = model.kept::<RoomState>();
            let host = room.host.borrow();
            let talk = room.talk.borrow();
            // Made again only when something it shows changed.
            let said = format!(
                "{:?}{:?}{:?}{:?}{:?}{}{}",
                host.standing
                    .as_ref()
                    .map(|standing| (standing.yours, standing.admin)),
                host.waiting
                    .iter()
                    .map(|knocker| &knocker.id)
                    .collect::<Vec<_>>(),
                host.busy,
                host.link,
                talk.as_ref().map(|talk| talk
                    .people
                    .iter()
                    .map(|person| (person.id.clone(), person.label(), person.microphone_off()))
                    .collect::<Vec<_>>()),
                host.ending,
                crate::i18n::language().code(),
            );
            if shown.borrow().as_deref() == Some(said.as_str()) {
                return;
            }
            shown.replace(Some(said));
            clear(&content);
            let Some(standing) = host.standing.as_ref() else {
                content.append(&muted(&t("Measuring…")));
                return;
            };
            if !standing.yours {
                content.append(&muted(&t("Somebody else runs this room.")));
                return;
            }
            let busy = host.busy.clone();

            // The door.
            if !host.waiting.is_empty() {
                content.append(&section(&t("Waiting to be let in")));
                for knocker in &host.waiting {
                    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
                    row.add_css_class("tmsb-panel-row");
                    let name = gtk::Label::builder()
                        .label(&knocker.name)
                        .xalign(0.0)
                        .hexpand(true)
                        .ellipsize(gtk::pango::EllipsizeMode::End)
                        .build();
                    let admit = pill(&t("Let in"));
                    admit.add_css_class("primary");
                    let refuse = pill(&t("Not now"));
                    for (button, yes) in [(&admit, true), (&refuse, false)] {
                        button.set_sensitive(busy.is_none());
                        let (weak, id) = (model.downgrade(), knocker.id.clone());
                        button.connect_clicked(move |_| {
                            if let Some(model) = weak.upgrade() {
                                super::answer_knock(&model, &id, yes);
                            }
                        });
                    }
                    row.append(&name);
                    row.append(&refuse);
                    row.append(&admit);
                    content.append(&row);
                }
            }

            // Invitations.
            let invite = pill(&t("Invite somebody"));
            invite.set_halign(gtk::Align::Start);
            invite.set_sensitive(busy.is_none());
            invite.set_widget_name("host.invite");
            let weak = model.downgrade();
            invite.connect_clicked(move |_| {
                if let Some(model) = weak.upgrade() {
                    super::invite(&model);
                }
            });
            content.append(&invite);
            if let Some(link) = host.link.as_ref() {
                let shown_link = gtk::Label::builder()
                    .label(link)
                    .xalign(0.0)
                    .wrap(true)
                    .wrap_mode(gtk::pango::WrapMode::Char)
                    .selectable(true)
                    .build();
                shown_link.add_css_class("tmsb-footnote");
                content.append(&shown_link);
                content.append(&muted(&t("It stops working after a day either way.")));
            }
            let revoke = pill(&t("Stop every link to this room working"));
            revoke.set_halign(gtk::Align::Start);
            revoke.set_sensitive(busy.is_none());
            let weak = model.downgrade();
            revoke.connect_clicked(move |_| {
                if let Some(model) = weak.upgrade() {
                    super::revoke(&model);
                }
            });
            content.append(&revoke);

            // The people.
            let rule = gtk::Box::new(gtk::Orientation::Horizontal, 0);
            rule.add_css_class("tmsb-menu-rule");
            content.append(&rule);
            let people = talk
                .as_ref()
                .map(|talk| talk.people.clone())
                .unwrap_or_default();
            if people.is_empty() {
                content.append(&muted(&t("Nobody else is here.")));
            }
            for person in people {
                let row = gtk::Box::new(gtk::Orientation::Vertical, 4);
                row.add_css_class("tmsb-panel-row");
                let name = gtk::Label::builder()
                    .label(person.label())
                    .xalign(0.0)
                    .ellipsize(gtk::pango::EllipsizeMode::End)
                    .build();
                row.append(&name);
                let actions = gtk::Box::new(gtk::Orientation::Horizontal, 6);
                let mute = pill(&t("Mute"));
                mute.set_sensitive(!person.microphone_off() && busy.is_none());
                let remove = pill(&t("Remove from room"));
                remove.add_css_class("danger");
                remove.set_sensitive(busy.is_none());
                let make_host = pill(&t("Make host"));
                make_host.set_sensitive(busy.is_none());
                for (button, act) in [
                    (&mute, super::silence as fn(&AppModel, &str)),
                    (&remove, super::turn_out),
                    (&make_host, super::hand_over),
                ] {
                    let (weak, who) = (model.downgrade(), person.id.clone());
                    button.connect_clicked(move |_| {
                        if let Some(model) = weak.upgrade() {
                            act(&model, &who);
                        }
                    });
                    actions.append(button);
                }
                row.append(&actions);
                content.append(&row);
            }

            // Closing it: asked once, since it cannot be undone.
            let rule = gtk::Box::new(gtk::Orientation::Horizontal, 0);
            rule.add_css_class("tmsb-menu-rule");
            content.append(&rule);
            if host.ending {
                content.append(&muted(&t(
                    "Everybody will be disconnected and told the room has closed. Links to it stop working.",
                )));
                let row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
                let cancel = pill(&t("Cancel"));
                let end = pill(&t("End this meeting"));
                end.add_css_class("danger");
                end.set_sensitive(busy.is_none());
                let weak = model.downgrade();
                cancel.connect_clicked(move |_| {
                    if let Some(model) = weak.upgrade() {
                        super::ask_to_end(&model, false);
                    }
                });
                let weak = model.downgrade();
                end.connect_clicked(move |_| {
                    if let Some(model) = weak.upgrade() {
                        super::end_for_everybody(&model);
                    }
                });
                row.append(&cancel);
                row.append(&end);
                content.append(&row);
            } else {
                let (close, _) = menu_row(
                    &t("Close the room"),
                    None,
                    Some("system-lock-screen-symbolic"),
                );
                close.add_css_class("danger");
                let weak = model.downgrade();
                close.connect_clicked(move |_| {
                    if let Some(model) = weak.upgrade() {
                        super::ask_to_end(&model, true);
                    }
                });
                content.append(&close);
            }
        }
    };
    PanelView {
        widget: outer.upcast(),
        refresh: Box::new(refresh),
    }
}
