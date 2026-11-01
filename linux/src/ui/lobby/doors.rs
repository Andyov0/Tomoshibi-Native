//! The screens between the lobby and a call (Screens.swift, Screens.cs): the
//! pre-join for somebody who came through a link or chose a room, the knock
//! on a door that turned them away, and the wait for an arranged meeting's
//! host. Each is one card over the page, as on the Mac.

use std::rc::Rc;

use adw::prelude::*;
use gtk::glib;

use super::{
    pieces::{self, icon},
    stage,
};
use crate::{
    app::{self, words, AppModel, Change, Gone},
    i18n::{t, t_with},
    theme::scale,
    ui::{kit, Context},
};

/// A screen that is a single card over the page, with the language in the
/// corner (Screens.swift `CardScreen`).
fn card_screen(context: &Context, width: i32, content: &gtk::Box) -> gtk::Widget {
    let card = gtk::Box::new(gtk::Orientation::Vertical, 0);
    card.add_css_class("tmsb-pane");
    card.add_css_class("tmsb-door");
    card.append(content);
    let clamp = adw::Clamp::builder()
        .maximum_size(width)
        .tightening_threshold(width)
        .child(&kit::Arrival::new(&card).rises(0))
        // As tall as what is in it: given the page's height, the knock's
        // short card ran down to the window's foot.
        .valign(gtk::Align::Start)
        .margin_top(24)
        .margin_bottom(48)
        .margin_start(16)
        .margin_end(16)
        .build();
    let scroller = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vexpand(true)
        .child(&clamp)
        .build();
    let bar = kit::bar();
    bar.pack_end(&pieces::language_button(&context.model));
    let page = adw::ToolbarView::new();
    page.add_top_bar(&bar);
    page.set_content(Some(&pieces::with_toast(&context.model, &scroller)));
    page.upcast()
}

/// The room, in the machine's monospace, with the way to copy its address
/// where a plain address is a way in, and what its door asks.
fn room_heading(model: &AppModel) -> gtk::Box {
    let title = pieces::heading(&model.room(), true);
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    row.set_halign(gtk::Align::Center);
    row.append(&title);
    if let Some(link) = model.room_link() {
        row.append(&pieces::copy_button(&link, 30));
    }
    let column = gtk::Box::new(gtk::Orientation::Vertical, 6);
    column.append(&row);
    if let Some(door) = model.door() {
        column.append(&pieces::note(&door));
    }
    column
}

/// The name, and a passphrase where one is asked for, in one box with a
/// hairline between them, as the web holds them: they are one answer.
fn identity_box(model: &AppModel, name_only: bool, submit: Rc<dyn Fn()>) -> gtk::Box {
    let fields = gtk::Box::new(gtk::Orientation::Vertical, 0);
    fields.add_css_class("tmsb-fields");
    let name = gtk::Entry::builder()
        .placeholder_text(t("Your name"))
        .primary_icon_name("avatar-default-symbolic")
        .input_purpose(gtk::InputPurpose::Name)
        .build();
    name.add_css_class("tmsb-field");
    name.set_widget_name("door.name");
    name.update_property(&[gtk::accessible::Property::Label(&t("Your name"))]);
    // A name the link was made for is shown and cannot be changed: the
    // server signs that name in whatever is typed.
    match model.invited_name() {
        Some(invited) => {
            name.set_text(&invited);
            name.set_editable(false);
            name.set_can_focus(false);
        }
        None => name.set_text(&model.name()),
    }
    fields.append(&name);
    let weak = model.downgrade();
    let fixed = model.invited_name().is_some();
    name.connect_changed(move |entry| {
        if fixed {
            return;
        }
        if let Some(model) = weak.upgrade() {
            model.set_name(&entry.text());
        }
    });
    if !name_only {
        fields.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
        let passphrase = gtk::PasswordEntry::builder()
            .placeholder_text(t("Passphrase"))
            .show_peek_icon(true)
            .build();
        passphrase.add_css_class("tmsb-field");
        passphrase.set_text(&model.passphrase());
        passphrase.update_property(&[gtk::accessible::Property::Label(&t("Passphrase"))]);
        let weak = model.downgrade();
        passphrase.connect_changed(move |entry| {
            if let Some(model) = weak.upgrade() {
                model.set_passphrase(&entry.text());
            }
        });
        let submit_passphrase = submit.clone();
        passphrase.connect_activate(move |_| submit_passphrase());
        fields.append(&passphrase);
    }
    name.connect_activate(move |_| submit());
    fields
}

/// The server, where the person may choose one here, and the seal.
fn options_card(model: &AppModel, sealing_on: bool) -> (gtk::Box, Rc<std::cell::Cell<bool>>) {
    let options = gtk::Box::new(gtk::Orientation::Vertical, 0);
    options.add_css_class("tmsb-well");
    if model.offers_relay_choice_at_the_door() {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        row.add_css_class("tmsb-well-row");
        let symbol = icon("network-server-symbolic", 14);
        symbol.add_css_class("tmsb-muted");
        row.append(&symbol);
        let words = gtk::Label::new(Some(&t("Server")));
        words.set_xalign(0.0);
        words.set_hexpand(true);
        row.append(&words);
        row.append(&pieces::server_menu(model));
        options.append(&row);
        let rule = gtk::Separator::new(gtk::Orientation::Horizontal);
        rule.add_css_class("tmsb-well-rule");
        options.append(&rule);
    }
    let (seal, sealing) = pieces::seal_control(model, sealing_on);
    seal.add_css_class("tmsb-well-row");
    options.append(&seal);
    (options, sealing)
}

/// The line saying why the last press did not take, with its close where it
/// stays.
fn problem_line(model: &AppModel) -> gtk::Revealer {
    let words = gtk::Label::builder()
        .wrap(true)
        .justify(gtk::Justification::Center)
        .hexpand(true)
        .build();
    words.add_css_class("tmsb-danger-text");
    words.add_css_class("tmsb-small");
    words.set_widget_name("door.problem");
    let revealer = pieces::unfolding(&words, false);
    let fill = {
        let (words, revealer) = (words.downgrade(), revealer.downgrade());
        move |model: &AppModel| {
            let (Some(words), Some(revealer)) = (words.upgrade(), revealer.upgrade()) else {
                return;
            };
            match model.problem() {
                Some(problem) => {
                    words.set_label(&problem);
                    revealer.set_reveal_child(true);
                }
                None => revealer.set_reveal_child(false),
            }
        }
    };
    fill(model);
    model.subscribe_while(&revealer, move |model, change| {
        if change == Change::Problem {
            fill(model);
        }
    });
    revealer
}

/// A quiet way back.
fn back_button(go_back: impl Fn() + 'static) -> gtk::Button {
    let button = kit::plain_button("");
    button.set_child(Some(&pieces::labelled(
        Some(icon("go-previous-symbolic", 13).upcast_ref()),
        &t("Back"),
    )));
    button.set_height_request(scale::CONTROL);
    button.set_halign(gtk::Align::Center);
    button.set_widget_name("door.back");
    button.connect_clicked(move |_| go_back());
    button
}

// ---- the pre-join ----------------------------------------------------------

pub fn prejoin(context: &Context) -> gtk::Widget {
    let model = &context.model;
    let content = gtk::Box::new(gtk::Orientation::Vertical, 16);
    content.append(&room_heading(model));
    let (card, _) = stage::card(context, ((560 - 44) as f64 * 9.0 / 16.0) as i32);
    content.append(&card);

    let (options, sealing) = options_card(model, !model.sealing_word().is_empty());
    let join = kit::primary_button(&t("Join"));
    join.set_widget_name("prejoin.join");
    let join_now: Rc<dyn Fn()> = {
        let (weak, sealing) = (model.downgrade(), sealing.clone());
        Rc::new(move || {
            let Some(model) = weak.upgrade() else {
                return;
            };
            if model.busy() || model.joining_name().is_empty() {
                return;
            }
            if !sealing.get() {
                model.set_sealing_word("");
            }
            model.join();
        })
    };
    if model.account().is_none() {
        let identity = gtk::Box::new(gtk::Orientation::Vertical, 8);
        // Somebody on an invitation has a name to choose and nothing else
        // to answer: the link is what lets them in, and a passphrase field
        // beside it reads as something they forgot.
        identity.append(&identity_box(model, model.arrived_invited(), join_now.clone()));
        let line = pieces::note(&model.identity_line());
        identity.append(&line);
        if let Some(window) = model.link_window() {
            identity.append(&pieces::note(&window));
        }
        content.append(&identity);
        let line = line.downgrade();
        model.subscribe_while(&identity, move |model, change| {
            if change == app::CHANGED {
                if let Some(line) = line.upgrade() {
                    line.set_label(&model.identity_line());
                }
            }
        });
    }
    content.append(&options);
    content.append(&problem_line(model));
    content.append(&join);
    let knock = kit::plain_button(&t("Ask to be let in"));
    knock.set_height_request(scale::CONTROL_PRIMARY);
    knock.set_widget_name("prejoin.knock");
    let knock_slot = pieces::unfolding(&knock, model.can_knock());
    content.append(&knock_slot);
    let weak = model.downgrade();
    content.append(&back_button(move || {
        if let Some(model) = weak.upgrade() {
            model.back_to_lobby();
        }
    }));

    {
        let join_now = join_now.clone();
        join.connect_clicked(move |_| join_now());
    }
    let weak = model.downgrade();
    knock.connect_clicked(move |_| {
        if let Some(model) = weak.upgrade() {
            model.knock();
        }
    });
    let update = {
        let (join, knock, knock_slot) = (join.downgrade(), knock.downgrade(), knock_slot.downgrade());
        move |model: &AppModel| {
            let (Some(join), Some(knock), Some(knock_slot)) =
                (join.upgrade(), knock.upgrade(), knock_slot.upgrade())
            else {
                return;
            };
            let ready = !model.busy() && !model.joining_name().is_empty();
            join.set_sensitive(ready);
            knock.set_sensitive(ready);
            knock_slot.set_reveal_child(model.can_knock());
        }
    };
    update(model);
    let page = card_screen(context, 560, &content);
    model.subscribe_while(&page, move |model, change| {
        if matches!(change, Change::Busy | Change::Problem | Change::Account) || change == app::CHANGED {
            update(model);
        }
    });
    if model.launch().lobby.as_deref() == Some("knock") {
        let weak = model.downgrade();
        glib::timeout_add_local_once(std::time::Duration::from_millis(600), move || {
            if let Some(model) = weak.upgrade() {
                model.knock();
            }
        });
    }
    page
}

// ---- at the door -------------------------------------------------------------

pub fn knocking(context: &Context) -> gtk::Widget {
    let model = &context.model;
    let content = gtk::Box::new(gtk::Orientation::Vertical, 18);
    content.append(&pieces::heading(&model.room(), true));
    let spinner = gtk::Spinner::builder().spinning(true).build();
    spinner.set_size_request(16, 16);
    let waiting = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    waiting.set_halign(gtk::Align::Center);
    waiting.set_margin_top(8);
    waiting.set_margin_bottom(8);
    waiting.append(&spinner);
    let words = gtk::Label::new(Some(&t("Waiting for somebody to let you in.")));
    words.set_wrap(true);
    waiting.append(&words);
    content.append(&waiting);
    let weak = model.downgrade();
    content.append(&back_button(move || {
        if let Some(model) = weak.upgrade() {
            model.stop_knocking();
        }
    }));
    card_screen(context, 420, &content)
}

// ---- an arranged meeting -------------------------------------------------------

/// Waiting.tsx inside PreJoin.tsx: the ordinary pre-join, because the point
/// of arriving early is to have the camera, the microphone and the name
/// settled before the meeting, with a button that waits. Nothing is asked of
/// the door until it is pressed.
pub fn waiting(context: &Context) -> gtk::Widget {
    let model = &context.model;
    let content = gtk::Box::new(gtk::Orientation::Vertical, 16);
    let heading = pieces::heading(&model.room(), true);
    content.append(&heading);
    let (card, _) = stage::card(context, ((560 - 44) as f64 * 9.0 / 16.0) as i32);
    content.append(&card);
    let (options, sealing) = options_card(model, !model.sealing_word().is_empty());
    let go: Rc<dyn Fn()> = {
        let (weak, sealing) = (model.downgrade(), sealing.clone());
        Rc::new(move || {
            if let Some(model) = weak.upgrade() {
                if !sealing.get() {
                    model.set_sealing_word("");
                }
                model.go_arranged();
            }
        })
    };
    if model.account().is_none() {
        // A guest with no saved name gives one here; the button waits for
        // it (Waiting.tsx: "Nothing is asked until there is one").
        content.append(&identity_box(model, true, go.clone()));
    }
    content.append(&options);
    let state = gtk::Box::new(gtk::Orientation::Vertical, 12);
    state.set_widget_name("waiting.state");
    content.append(&state);
    content.append(&problem_line(model));
    let weak = model.downgrade();
    content.append(&back_button(move || {
        if let Some(model) = weak.upgrade() {
            model.stop_waiting();
        }
    }));

    let fill = {
        let (state, heading, go) = (state.downgrade(), heading.downgrade(), go.clone());
        move |model: &AppModel| {
            let (Some(state), Some(heading)) = (state.upgrade(), heading.upgrade()) else {
                return;
            };
            heading.set_label(&model.room());
            while let Some(child) = state.first_child() {
                state.remove(&child);
            }
            let waiting = model.waiting();
            match waiting.gone {
                Some(Gone::Cancelled) => {
                    state.append(&pieces::note(&t("That invitation is no longer good. Ask for another.")));
                    return;
                }
                Some(Gone::Ended) => {
                    state.append(&pieces::note(&t("That meeting has ended.")));
                    return;
                }
                Some(Gone::NotComing) => {
                    state.append(&pieces::note(&t("The host has not started this meeting.")));
                    let again = kit::plain_button(&t("Ask again"));
                    again.set_height_request(scale::CONTROL_PRIMARY);
                    let weak = model.downgrade();
                    again.connect_clicked(move |_| {
                        if let Some(model) = weak.upgrade() {
                            model.ask_again();
                        }
                    });
                    state.append(&again);
                    return;
                }
                None => {}
            }
            let arrangement = model.arrangement();
            let ready = model.arranged_ready();
            let opens = model.arranged_opens();
            let now = words::now();
            let too_early = arrangement.as_ref().is_some_and(|one| one.mine)
                && !ready
                && opens.is_some_and(|opens| now < opens);
            match &arrangement {
                Some(one) if !ready => {
                    let when = words::instant(&one.at)
                        .map(words::said)
                        .unwrap_or_else(|| one.at.clone());
                    state.append(&pieces::note(&if one.mine {
                        if too_early {
                            t_with(
                                "Your meeting can start from {when}.",
                                &[("when", &words::said(opens.unwrap_or(now)))],
                            )
                        } else {
                            t_with(
                                "Your meeting is arranged for {when}. Joining starts it.",
                                &[("when", &when)],
                            )
                        }
                    } else {
                        t_with("This meeting is arranged for {when}.", &[("when", &when)])
                    }));
                }
                None if waiting.unreachable => {
                    state.append(&pieces::note(&t("Could not reach the meeting. Trying again.")));
                }
                None => {
                    let spinner = gtk::Spinner::builder().spinning(true).halign(gtk::Align::Center).build();
                    spinner.set_size_request(16, 16);
                    state.append(&spinner);
                }
                _ => {}
            }
            let words = if waiting.asking {
                t("Waiting for the host to start the meeting.")
            } else if ready {
                t("Join")
            } else if arrangement.as_ref().is_some_and(|one| one.mine) {
                t("Start the meeting")
            } else {
                t("Wait for the host")
            };
            let button = kit::primary_button(&words);
            button.set_widget_name("waiting.go");
            button.set_sensitive(
                !model.joining_name().is_empty()
                    && !waiting.asking
                    && !model.busy()
                    && arrangement.is_some()
                    && !too_early,
            );
            let go = go.clone();
            button.connect_clicked(move |_| go());
            state.append(&button);
        }
    };
    fill(model);
    let page = card_screen(context, 560, &content);
    {
        let fill = fill.clone();
        model.subscribe_while(&page, move |model, change| {
            if matches!(change, Change::Busy | Change::Account) || change == app::CHANGED {
                fill(model);
            }
        });
    }
    // The host's button comes alive when the window opens, without anybody
    // asking anything again: a clock, every thirty seconds.
    let weak = model.downgrade();
    let source = glib::timeout_add_seconds_local(30, move || match weak.upgrade() {
        Some(model) => {
            fill(&model);
            glib::ControlFlow::Continue
        }
        None => glib::ControlFlow::Break,
    });
    let source = std::cell::RefCell::new(Some(source));
    page.connect_destroy(move |_| {
        if let Some(source) = source.take() {
            source.remove();
        }
    });
    page
}
