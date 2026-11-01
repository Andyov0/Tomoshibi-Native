//! Start a meeting or join one (Lobby.swift `MeetingSheet`): the picture of
//! yourself, the room, where it is held and whether it is sealed, and then
//! straight into the call. A dialog over the lobby, as the Mac's is a sheet
//! over its window.
//!
//! A meeting is asked about first, as the web does: one that is happening
//! cannot be started and one that is not cannot be joined. An answer that
//! cannot be had goes through; the join is the authority. A room from the
//! recent list is joined if a meeting is live in it and started again under
//! the same name if not: opened as a join, it was refused whenever the
//! meeting had ended, which for a room somebody has been in is most of the
//! time.

use std::{cell::RefCell, rc::Rc};

use adw::prelude::*;
use gtk::glib;
use tomoshibi_protocol::identity::{generate_room_name, normalise_room_name, valid_room_name};

use super::{pieces, stage};
use crate::{
    app::{log, pasted_link, AppModel, Change, Screen},
    i18n::{t, t_with},
    ui::{kit, Context},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mode {
    Start,
    Join(String),
    /// A room from the recent list, settled into Start or Join once the
    /// deployment says whether a meeting is live in it.
    Again(String),
}

/// The sheet's width, as the Mac's.
const WIDTH: i32 = 560;

pub fn present(context: &Context, mode: Mode) {
    let Some(window) = context.window() else {
        return;
    };
    let model = &context.model;
    model.clear_problem();
    let mode = Rc::new(RefCell::new(mode));
    let dialog = adw::Dialog::builder().content_width(WIDTH).build();
    dialog.add_css_class("tmsb-sheet");
    dialog.set_widget_name("lobby.sheet");

    // ---- the head ---------------------------------------------------------

    let close = pieces::round_button(&pieces::icon("window-close-symbolic", 13), 32, &t("Cancel"));
    let starting_now = matches!(*mode.borrow(), Mode::Start);
    let room_typed = match &*mode.borrow() {
        Mode::Join(room) | Mode::Again(room) => room.clone(),
        Mode::Start => String::new(),
    };
    let choice_slot = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    choice_slot.set_hexpand(true);
    choice_slot.set_halign(gtk::Align::Center);
    let head = gtk::CenterBox::new();
    head.set_start_widget(Some(&close));
    head.set_center_widget(Some(&choice_slot));
    let balance = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    balance.set_size_request(32, 32);
    head.set_end_widget(Some(&balance));

    // ---- the picture ------------------------------------------------------

    let (card, portraits) = stage::card(context, ((WIDTH - 40) as f64 * 9.0 / 16.0) as i32);
    portraits.set_held(false);

    // ---- the form ---------------------------------------------------------

    // As narrow as the sheet: a field's natural width is its placeholder's,
    // and the long one in monospace made the sheet 780 wide.
    let room = gtk::Entry::builder()
        .text(&room_typed)
        .hexpand(true)
        .width_chars(8)
        .build();
    room.add_css_class("tmsb-input");
    room.add_css_class("tmsb-mono");
    room.set_widget_name("sheet.room");
    let scope_warning = gtk::Label::builder().wrap(true).xalign(0.0).build();
    scope_warning.add_css_class("tmsb-small");
    scope_warning.add_css_class("tmsb-danger-text");
    let scope_warning_slot = pieces::unfolding(&scope_warning, false);
    scoped_room_field(model, &room, &scope_warning, &scope_warning_slot);

    let options = gtk::Box::new(gtk::Orientation::Vertical, 0);
    options.add_css_class("tmsb-well");
    let server_row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    server_row.add_css_class("tmsb-well-row");
    let server_symbol = pieces::icon("network-server-symbolic", 14);
    server_symbol.add_css_class("tmsb-muted");
    server_row.append(&server_symbol);
    let server_words = gtk::Label::new(Some(&t("Server")));
    server_words.set_xalign(0.0);
    server_words.set_hexpand(true);
    server_row.append(&server_words);
    server_row.append(&pieces::server_menu(model));
    let server_rule = gtk::Separator::new(gtk::Orientation::Horizontal);
    server_rule.add_css_class("tmsb-well-rule");
    let server_part = gtk::Box::new(gtk::Orientation::Vertical, 0);
    server_part.append(&server_row);
    server_part.append(&server_rule);
    let server_slot = pieces::unfolding(&server_part, model.offers_relay_choice());
    let (seal, sealing) = pieces::seal_control(model, !model.sealing_word().is_empty());
    seal.add_css_class("tmsb-well-row");
    options.append(&server_slot);
    options.append(&seal);

    let form = gtk::Box::new(gtk::Orientation::Vertical, 10);
    form.append(&room);
    form.append(&scope_warning_slot);
    form.append(&options);

    // ---- the foot ---------------------------------------------------------

    let problem = gtk::Label::builder()
        .wrap(true)
        .justify(gtk::Justification::Center)
        .build();
    problem.add_css_class("tmsb-danger-text");
    problem.add_css_class("tmsb-small");
    let problem_slot = pieces::unfolding(&problem, false);
    let knock = kit::plain_button(&t("Ask to be let in"));
    let knock_slot = pieces::fading(&knock, false);
    let cancel = kit::plain_button(&t("Cancel"));
    let go = kit::primary_button(&if starting_now { t("Start") } else { t("Join") });
    go.set_height_request(crate::theme::scale::CONTROL_LARGE);
    go.add_css_class("tmsb-compact");
    go.set_widget_name("sheet.go");
    let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    buttons.append(&knock_slot);
    let space = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    space.set_hexpand(true);
    buttons.append(&space);
    buttons.append(&cancel);
    buttons.append(&go);
    let foot = gtk::Box::new(gtk::Orientation::Vertical, 10);
    foot.append(&problem_slot);
    foot.append(&buttons);

    let column = gtk::Box::new(gtk::Orientation::Vertical, 16);
    column.set_margin_top(20);
    column.set_margin_bottom(20);
    column.set_margin_start(20);
    column.set_margin_end(20);
    column.append(&head);
    column.append(&card);
    column.append(&form);
    column.append(&foot);
    // A layer of its own for the menus in it: the dialog is drawn over the
    // lobby, and so over the lobby's layer.
    dialog.set_child(Some(&kit::menu_layer(&column)));

    // ---- what it does -----------------------------------------------------

    let working = Rc::new(std::cell::Cell::new(false));
    let going: Rc<RefCell<Option<glib::JoinHandle<()>>>> = Rc::new(RefCell::new(None));

    // The words that change with the choice: the field's prompt, the button.
    let follow_mode = {
        let (room, go) = (room.downgrade(), go.downgrade());
        let mode = mode.clone();
        move || {
            let (Some(room), Some(go)) = (room.upgrade(), go.upgrade()) else {
                return;
            };
            let starting = matches!(*mode.borrow(), Mode::Start);
            room.set_placeholder_text(Some(&if starting {
                t("Name it yourself, or leave it blank for one nobody has used.")
            } else {
                t("Room name or invite link")
            }));
            room.update_property(&[gtk::accessible::Property::Label(&t("Room name"))]);
            go.set_label(&if starting { t("Start") } else { t("Join") });
        }
    };
    let make_choice = {
        let (choice_slot, mode, follow_mode, room) = (
            choice_slot.downgrade(),
            mode.clone(),
            follow_mode.clone(),
            room.downgrade(),
        );
        move || {
            let Some(slot) = choice_slot.upgrade() else {
                return;
            };
            while let Some(child) = slot.first_child() {
                slot.remove(&child);
            }
            let starting = matches!(*mode.borrow(), Mode::Start);
            let (mode, follow_mode, room) = (mode.clone(), follow_mode.clone(), room.clone());
            let choice = pieces::segmented(
                &[t("Start new meeting"), t("Join a meeting")],
                if starting { 0 } else { 1 },
                move |chosen| {
                    let typed = room
                        .upgrade()
                        .map(|room| room.text().to_string())
                        .unwrap_or_default();
                    mode.replace(if chosen == 0 {
                        Mode::Start
                    } else {
                        Mode::Join(typed)
                    });
                    follow_mode();
                },
            );
            choice.set_size_request(300, -1);
            choice.set_widget_name("sheet.choice");
            slot.append(&choice);
        }
    };
    make_choice();
    follow_mode();

    let update = {
        let (go, knock_slot, knock, problem, problem_slot, room, server_slot, working, mode) = (
            go.downgrade(),
            knock_slot.downgrade(),
            knock.downgrade(),
            problem.downgrade(),
            problem_slot.downgrade(),
            room.downgrade(),
            server_slot.downgrade(),
            working.clone(),
            mode.clone(),
        );
        move |model: &AppModel| {
            let (
                Some(go),
                Some(knock_slot),
                Some(knock),
                Some(problem),
                Some(problem_slot),
                Some(room),
                Some(server_slot),
            ) = (
                go.upgrade(),
                knock_slot.upgrade(),
                knock.upgrade(),
                problem.upgrade(),
                problem_slot.upgrade(),
                room.upgrade(),
                server_slot.upgrade(),
            )
            else {
                return;
            };
            let starting = matches!(*mode.borrow(), Mode::Start);
            let typed = room.text().to_string();
            let named = valid_room_name(normalise_room_name(typed.clone()));
            // And while any join is out, a called-off one included: a second
            // beside it would go in as the same participant.
            go.set_sensitive(
                !working.get()
                    && !model.busy()
                    && (starting || named || pasted_link(&typed).is_some()),
            );
            match model.problem() {
                Some(said) => {
                    problem.set_label(&said);
                    problem_slot.set_reveal_child(true);
                }
                None => problem_slot.set_reveal_child(false),
            }
            knock_slot.set_reveal_child(model.can_knock());
            knock.set_sensitive(!model.busy());
            server_slot.set_reveal_child(model.offers_relay_choice());
        }
    };
    update(model);
    {
        let (update, weak) = (update.clone(), model.downgrade());
        room.connect_changed(move |_| {
            if let Some(model) = weak.upgrade() {
                update(&model);
            }
        });
    }

    let run = {
        let (model, room, mode, sealing, working, going, dialog_weak, update) = (
            model.downgrade(),
            room.downgrade(),
            mode.clone(),
            sealing.clone(),
            working.clone(),
            going.clone(),
            dialog.downgrade(),
            update.clone(),
        );
        move || {
            let (Some(model), Some(room)) = (model.upgrade(), room.upgrade()) else {
                return;
            };
            if working.get() || model.busy() {
                return;
            }
            let Some(deployment) = model.deployment() else {
                return;
            };
            let starting = matches!(*mode.borrow(), Mode::Start);
            let again = matches!(*mode.borrow(), Mode::Again(_));
            let typed = room.text().to_string();
            if !sealing.get() {
                model.set_sealing_word("");
            }
            // An invitation pasted where a name goes is opened as the link
            // it is: a room's name taken out of it would leave the
            // invitation behind.
            if !starting {
                if let Some(link) = pasted_link(&typed) {
                    if let Some(dialog) = dialog_weak.upgrade() {
                        dialog.close();
                    }
                    model.open_link(&link);
                    return;
                }
            }
            let normal = normalise_room_name(typed);
            let target = if starting && !valid_room_name(normal.clone()) {
                generate_room_name()
            } else {
                normal
            };
            if !valid_room_name(target.clone()) {
                return;
            }
            working.set(true);
            model.clear_problem();
            update(&model);
            let (weak, working, update) = (model.downgrade(), working.clone(), update.clone());
            let handle = glib::spawn_future_local(async move {
                let live = deployment.room_live(target.clone()).await.ok().flatten();
                let Some(model) = weak.upgrade() else {
                    return;
                };
                if starting && live == Some(true) {
                    model.mention(t("A meeting is already happening under that name."));
                } else if !starting && !again && live == Some(false) {
                    model.mention(t("No meeting is happening under that name."));
                } else {
                    model.enter(&target).await;
                }
                working.set(false);
                update(&model);
            });
            going.replace(Some(handle));
        }
    };
    {
        let run = run.clone();
        go.connect_clicked(move |_| run());
    }
    {
        let run = run.clone();
        room.connect_activate(move |_| run());
    }
    let weak = model.downgrade();
    knock.connect_clicked(move |_| {
        if let Some(model) = weak.upgrade() {
            // The pre-join is where somebody waits at the door.
            model.prepare(&model.room());
            model.knock();
        }
    });
    // Cancel and the cross call off whatever the press set going: they stayed
    // live while a join was out and did nothing to it, so somebody who
    // pressed Cancel was put in the call a moment later.
    let cancel_all = {
        let (weak, going, dialog) = (model.downgrade(), going.clone(), dialog.downgrade());
        move || {
            if let Some(handle) = going.take() {
                handle.abort();
            }
            if let Some(model) = weak.upgrade() {
                model.cancel_join();
            }
            if let Some(dialog) = dialog.upgrade() {
                dialog.close();
            }
        }
    };
    {
        let cancel_all = cancel_all.clone();
        cancel.connect_clicked(move |_| cancel_all());
    }
    close.connect_clicked(move |_| cancel_all());

    // The call came, or the screen went elsewhere: the sheet goes, and what
    // brought it there is not called off.
    let arrived = Rc::new(std::cell::Cell::new(false));
    let dialog_weak = dialog.downgrade();
    let arrived_now = arrived.clone();
    model.subscribe_while(&dialog, move |model, change| match change {
        Change::Screen if model.screen() != Screen::Lobby => {
            if let Some(dialog) = dialog_weak.upgrade() {
                arrived_now.set(true);
                dialog.force_close();
            }
        }
        Change::Busy | Change::Problem => update(model),
        other if other == crate::app::CHANGED || other == crate::app::lobby::CHANGED => {
            update(model)
        }
        _ => {}
    });
    // Escape and a press outside close the sheet as Cancel does, and call
    // off what it set going the same way. They closed it and nothing else:
    // the dropped handle did not stop the task, and somebody who pressed
    // Escape to take back a Join was put in the pre-join or the call a
    // moment later.
    let (weak, going_now) = (model.downgrade(), going.clone());
    dialog.connect_closed(move |_| {
        let Some(model) = weak.upgrade() else {
            return;
        };
        if !arrived.get() {
            if let Some(handle) = going_now.take() {
                handle.abort();
            }
            model.cancel_join();
        }
        if !model.problem_stays() {
            model.clear_problem();
        }
    });

    // A room from the recent list, settled into the choice it is.
    if let Mode::Again(name) = mode.borrow().clone() {
        let (mode, make_choice, follow_mode) =
            (mode.clone(), make_choice.clone(), follow_mode.clone());
        if let Some(deployment) = model.deployment() {
            glib::spawn_future_local(async move {
                let live = deployment.room_live(name.clone()).await.ok().flatten();
                // Not over a choice made meanwhile.
                if !matches!(*mode.borrow(), Mode::Again(_)) {
                    return;
                }
                if let Some(live) = live {
                    log(&format!(
                        "{name} is {}",
                        if live { "live" } else { "not live" }
                    ));
                    mode.replace(if live { Mode::Join(name) } else { Mode::Start });
                    make_choice();
                    follow_mode();
                }
            });
        }
    }

    dialog.present(Some(&window));
    room.grab_focus();
}

/// A room's name with the part after `@` completed from the account's own
/// scopes (ScopedRoomInput.tsx): one scope is written in at the `@`, several
/// are offered, and a scope the account is not in is said under the field
/// before anything is pressed. Only completed, never rewritten; an
/// administrator belongs to every scope and is never warned.
pub fn scoped_room_field(
    model: &AppModel,
    entry: &gtk::Entry,
    warning: &gtk::Label,
    warning_slot: &gtk::Revealer,
) {
    let Some(account) = model.account() else {
        return;
    };
    let scopes = account.scopes.clone();
    let admin = account.admin;
    // Under the field and as wide as it, and not modal: the keyboard stays
    // in the field, and a press elsewhere puts the suggestions away and
    // goes on to what it pressed.
    let menu = kit::Menu::new(entry, 0, kit::MenuSide::Below, kit::MenuAlign::Fill);
    menu.set_modal(false);
    let previous = Rc::new(RefCell::new(entry.text().to_string()));
    let (warning, warning_slot, menu_weak) = (
        warning.downgrade(),
        warning_slot.downgrade(),
        menu.downgrade(),
    );
    entry.connect_changed(move |entry| {
        let text = entry.text().to_string();
        let old = previous.replace(text.clone());
        // The one scope there is, written in at the `@`.
        if text == format!("{old}@") && scopes.len() == 1 && !old.contains('@') {
            let completed = format!("{text}{}", scopes[0]);
            entry.set_text(&completed);
            entry.set_position(-1);
            return;
        }
        let typed_scope = text.split_once('@').map(|(_, scope)| scope.to_lowercase());
        let matches: Vec<String> = match &typed_scope {
            Some(typed) => scopes
                .iter()
                .filter(|scope| scope.starts_with(typed.as_str()) && *scope != typed)
                .cloned()
                .collect(),
            None => Vec::new(),
        };
        if let Some(menu) = menu_weak.upgrade() {
            let column = menu.column();
            // The rows change only when the scopes offered do, so that the
            // open menu does not flicker under each letter typed.
            let shown: Vec<String> = {
                let mut shown = Vec::new();
                let mut child = column.first_child();
                while let Some(row) = child {
                    child = row.next_sibling();
                    shown.push(row.widget_name().to_string());
                }
                shown
            };
            if shown != matches {
                while let Some(child) = column.first_child() {
                    column.remove(&child);
                }
                for scope in &matches {
                    let row = pieces::menu_row(scope, None, false);
                    row.set_widget_name(scope);
                    row.set_can_focus(false);
                    let (entry_weak, scope, pop) =
                        (entry.downgrade(), scope.clone(), menu.downgrade());
                    row.connect_clicked(move |_| {
                        if let Some(entry) = entry_weak.upgrade() {
                            let text = entry.text().to_string();
                            if let Some((room, _)) = text.split_once('@') {
                                entry.set_text(&format!("{room}@{scope}"));
                                entry.set_position(-1);
                            }
                            entry.grab_focus_without_selecting();
                        }
                        if let Some(pop) = pop.upgrade() {
                            pop.popdown();
                        }
                    });
                    column.append(&row);
                }
            }
            if matches.is_empty() {
                menu.popdown();
            } else {
                menu.popup();
            }
        }
        let outside = typed_scope.as_ref().filter(|typed| {
            !typed.is_empty() && !admin && !scopes.contains(typed) && matches.is_empty()
        });
        if let (Some(warning), Some(slot)) = (warning.upgrade(), warning_slot.upgrade()) {
            match outside {
                Some(scope) => {
                    warning.set_label(&t_with(
                        "You are not in {scope}, so this room will not let you in.",
                        &[("scope", scope)],
                    ));
                    slot.set_reveal_child(true);
                }
                None => slot.set_reveal_child(false),
            }
        }
    });
}
