//! The account (Account.tsx; Settings.swift `AccountSettings`): who you are
//! here and your picture, what you can use, the passphrase, and guest links
//! into your scopes' rooms. Somebody not signed in is told where to do it.

use std::rc::Rc;

use adw::prelude::*;
use gtk::glib;

use super::new_page;
use crate::{
    app::{words, AppModel},
    i18n::{t, t_with},
    ui::{kit, lobby::pieces, Context},
};

const PERMISSIONS: [&str; 6] = ["watch", "emby", "music", "arrange", "invite", "open"];

/// manage/grants.ts `PERMISSION_NAMES`: the words the management pages
/// tick, so that "an administrator gave me Music" and this say one word.
fn permission_name(one: &str) -> String {
    match one {
        "watch" => t("Watch together"),
        "emby" => t("Emby library"),
        "music" => t("Music"),
        "arrange" => t("Arrange meetings"),
        "invite" => t("Guest links"),
        _ => t("Open new rooms"),
    }
}

pub fn page(context: &Context) -> adw::PreferencesPage {
    let model = &context.model;
    let page = new_page(
        "account",
        &t("Account"),
        "avatar-default-symbolic",
        &t("Who you are here, and what you can use."),
    );
    let Some(account) = model.account() else {
        let group = adw::PreferencesGroup::builder()
            .title(t("Account"))
            .description(t("Sign in to change your passphrase or picture."))
            .build();
        page.add(&group);
        return page;
    };

    // ---- who ----------------------------------------------------------------

    let who = adw::PreferencesGroup::new();
    let head = gtk::Box::new(gtk::Orientation::Horizontal, 16);
    head.set_margin_bottom(6);
    head.append(&pieces::avatar(model, &account.name, 64));
    let names = gtk::Box::new(gtk::Orientation::Vertical, 4);
    names.set_valign(gtk::Align::Center);
    let name = gtk::Label::new(Some(&account.name));
    name.set_xalign(0.0);
    name.add_css_class("tmsb-profile-name");
    names.append(&name);
    let said = gtk::Label::builder()
        .label(t(
            "Shown beside your name in a call. Scaled down before it is sent.",
        ))
        .wrap(true)
        .xalign(0.0)
        .build();
    said.add_css_class("dim-label");
    names.append(&said);
    head.append(&names);
    who.add(&head);
    let choose = gtk::Button::with_label(&t("Choose a picture"));
    let remove = gtk::Button::with_label(&t("Remove"));
    let sign_out = gtk::Button::with_label(&t("Sign out"));
    sign_out.add_css_class("destructive-action");
    remove.set_sensitive(
        account
            .avatar
            .as_deref()
            .is_some_and(|avatar| !avatar.is_empty()),
    );
    let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    buttons.append(&choose);
    buttons.append(&remove);
    let space = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    space.set_hexpand(true);
    buttons.append(&space);
    buttons.append(&sign_out);
    who.add(&buttons);
    page.add(&who);
    {
        let ctx = context.clone();
        choose.connect_clicked(move |_| choose_picture(&ctx));
        let weak = model.downgrade();
        remove.connect_clicked(move |button| {
            button.set_sensitive(false);
            let weak = weak.clone();
            glib::spawn_future_local(async move {
                if let Some(model) = weak.upgrade() {
                    if let Err(said) = model.remove_picture().await {
                        model.mention(said);
                    }
                }
            });
        });
        let ctx = context.clone();
        sign_out.connect_clicked(move |button| {
            if let Some(dialog) = button.ancestor(adw::Dialog::static_type()) {
                if let Ok(dialog) = dialog.downcast::<adw::Dialog>() {
                    dialog.close();
                }
            }
            crate::ui::lobby::home::ask_to_sign_out(&ctx);
        });
    }

    // ---- what they may use --------------------------------------------------

    let abilities = adw::PreferencesGroup::builder()
        .title(t("What you can use"))
        .description(t("Set by whoever runs this deployment."))
        .build();
    let held: Vec<&str> = PERMISSIONS
        .iter()
        .copied()
        .filter(|one| model.holds(one))
        .collect();
    // Emby is left off what was withheld: it is given one at a time and
    // spends the deployment's money, and a list of it to everybody without
    // it would be an advertisement for asking (Account.tsx `Abilities`).
    let withheld: Vec<&str> = PERMISSIONS
        .iter()
        .copied()
        .filter(|one| *one != "emby" && !model.holds(one))
        .collect();
    if !held.is_empty() {
        abilities.add(&chips(&held, false));
    }
    if !withheld.is_empty() {
        let label = gtk::Label::new(Some(&t("Not opened to this account")));
        label.set_xalign(0.0);
        label.add_css_class("dim-label");
        label.add_css_class("caption");
        label.set_margin_top(10);
        abilities.add(&label);
        abilities.add(&chips(&withheld, true));
    }
    let facts = gtk::ListBox::new();
    facts.add_css_class("boxed-list");
    facts.set_selection_mode(gtk::SelectionMode::None);
    facts.set_margin_top(12);
    if !account.scopes.is_empty() {
        let scopes = account
            .scopes
            .iter()
            .map(|scope| format!("@{scope}"))
            .collect::<Vec<_>>()
            .join("  ");
        let row = adw::ActionRow::builder()
            .title(t("Your scopes"))
            .subtitle(scopes)
            .build();
        row.add_css_class("property");
        facts.append(&row);
    }
    if let Some(created) = account.created.as_deref().and_then(words::instant) {
        let row = adw::ActionRow::builder()
            .title(t("Account created"))
            .subtitle(words::said(created))
            .build();
        row.add_css_class("property");
        facts.append(&row);
    }
    if facts.first_child().is_some() {
        abilities.add(&facts);
    }
    page.add(&abilities);

    // ---- the passphrase -----------------------------------------------------

    page.add(&passphrase_group(model));
    if model.holds("invite") && !account.scopes.is_empty() {
        page.add(&guest_links(model, &account.scopes));
    }
    page
}

fn chips(ones: &[&str], withheld: bool) -> gtk::FlowBox {
    let flow = gtk::FlowBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .column_spacing(6)
        .row_spacing(6)
        .homogeneous(false)
        .max_children_per_line(6)
        .build();
    for one in ones {
        let chip = gtk::Label::new(Some(&permission_name(one)));
        chip.add_css_class("tmsb-chip");
        if withheld {
            chip.add_css_class("withheld");
        }
        if *one == "emby" {
            chip.add_css_class("lit");
        }
        flow.append(&chip);
    }
    flow
}

fn passphrase_group(model: &AppModel) -> adw::PreferencesGroup {
    let group = adw::PreferencesGroup::builder()
        .title(t("Passphrase"))
        .description(t(
            "Your passphrase is not stored here and cannot be sent to you. If you lose it, an administrator sets a new one.",
        ))
        .build();
    let current = adw::PasswordEntryRow::builder()
        .title(t("Current passphrase"))
        .build();
    let next = adw::PasswordEntryRow::builder()
        .title(t("New passphrase"))
        .build();
    let change = kit::primary_button(&t("Change it"));
    change.set_height_request(36);
    change.add_css_class("tmsb-compact");
    change.set_halign(gtk::Align::End);
    change.set_margin_top(10);
    change.set_sensitive(false);
    let said = gtk::Label::builder().wrap(true).xalign(0.0).build();
    said.add_css_class("caption");
    said.set_margin_top(10);
    group.add(&current);
    group.add(&next);
    let foot = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    // What came of a change arrives with a fade rather than at once.
    let said_slot = pieces::fading(&said, false);
    said_slot.set_hexpand(true);
    foot.append(&said_slot);
    foot.append(&change);
    group.add(&foot);
    let ready = {
        let (current, next, change) = (current.downgrade(), next.downgrade(), change.downgrade());
        move || {
            if let (Some(current), Some(next), Some(change)) =
                (current.upgrade(), next.upgrade(), change.upgrade())
            {
                change
                    .set_sensitive(!current.text().is_empty() && next.text().chars().count() >= 8);
            }
        }
    };
    {
        let ready = ready.clone();
        current.connect_changed(move |_| ready());
    }
    next.connect_changed(move |_| ready());
    let (weak, current_weak, next_weak, said_weak, slot_weak) = (
        model.downgrade(),
        current.downgrade(),
        next.downgrade(),
        said.downgrade(),
        said_slot.downgrade(),
    );
    change.connect_clicked(move |button| {
        let (Some(model), Some(current), Some(next)) =
            (weak.upgrade(), current_weak.upgrade(), next_weak.upgrade())
        else {
            return;
        };
        button.set_sensitive(false);
        let (current_text, next_text) = (current.text().to_string(), next.text().to_string());
        let (said, slot, button, current, next) = (
            said_weak.clone(),
            slot_weak.clone(),
            button.downgrade(),
            current.downgrade(),
            next.downgrade(),
        );
        glib::spawn_future_local(async move {
            let result = model.change_passphrase(current_text, next_text).await;
            if let Some(slot) = slot.upgrade() {
                slot.set_reveal_child(true);
            }
            if let Some(said) = said.upgrade() {
                match &result {
                    Ok(()) => {
                        said.set_label(&t("Changed."));
                        said.remove_css_class("error");
                        said.add_css_class("success");
                    }
                    Err(refusal) => {
                        said.set_label(refusal);
                        said.remove_css_class("success");
                        said.add_css_class("error");
                    }
                }
            }
            if result.is_ok() {
                for entry in [current.upgrade(), next.upgrade()].into_iter().flatten() {
                    entry.set_text("");
                }
            }
            if let Some(button) = button.upgrade() {
                button.set_sensitive(false);
            }
        });
    });
    group
}

/// Account.tsx `GuestLinks`: a standing link into one of the account's
/// scopes' rooms, for one person or for whoever has it, working from and
/// until the times given (as a duration from now, the end left empty for a
/// link that works until it is stopped), and stopping every link to a room.
fn guest_links(model: &AppModel, scopes: &[String]) -> adw::PreferencesGroup {
    let group = adw::PreferencesGroup::builder()
        .title(t("Guest links"))
        .description(t(
            "A link lets somebody outside your scope into one of its rooms as a guest. Leave the end empty for a link that works until it is stopped.",
        ))
        .build();
    let room = adw::EntryRow::builder().title(t("Room name")).build();
    room.add_css_class("tmsb-mono");
    let name = adw::EntryRow::builder()
        .title(t("Their name, if the link is for one person"))
        .build();
    let scope_list = gtk::StringList::new(&scopes.iter().map(String::as_str).collect::<Vec<_>>());
    let scope = adw::ComboRow::builder()
        .title(t("Scope"))
        .model(&scope_list)
        .build();
    let hours = ["", "1", "24", "168"];
    // The first, no end, is the empty end the description speaks of.
    let until_words = ["—".to_owned(), t("1 hour"), t("24 hours"), t("1 week")];
    let until_list =
        gtk::StringList::new(&until_words.iter().map(String::as_str).collect::<Vec<_>>());
    let until = adw::ComboRow::builder()
        .title(t("Works until"))
        .model(&until_list)
        .build();
    // When it starts working: now, or a day and a quarter hour chosen on
    // the month (Account.tsx's "Works from", which the Mac offers as a
    // calendar and a clock). The end above is counted from when it starts.
    let (from, from_at) = works_from();
    group.add(&room);
    group.add(&scope);
    group.add(&name);
    group.add(&from);
    group.add(&until);
    let make = kit::primary_button(&t("Make a link"));
    make.set_height_request(36);
    make.add_css_class("tmsb-compact");
    let stop = gtk::Button::with_label(&t("Stop every link to this room working"));
    stop.add_css_class("flat");
    let said = gtk::Label::builder()
        .wrap(true)
        .xalign(0.0)
        .selectable(true)
        .build();
    said.add_css_class("caption");
    let said_slot = pieces::unfolding(&said, false);
    let foot = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    foot.set_margin_top(10);
    foot.append(&stop);
    let space = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    space.set_hexpand(true);
    foot.append(&space);
    foot.append(&make);
    group.add(&foot);
    said.set_margin_top(8);
    group.add(&said_slot);

    let scopes = scopes.to_vec();
    let target = {
        let (room, scope, scopes) = (room.downgrade(), scope.downgrade(), scopes.clone());
        move || -> Option<String> {
            let room = room.upgrade()?.text().to_string();
            let scope = scopes.get(scope.upgrade()?.selected() as usize)?.clone();
            let local = tomoshibi_protocol::identity::normalise_room_name(room);
            let local = local.split('@').next().unwrap_or_default().to_owned();
            let full = format!("{local}@{scope}");
            tomoshibi_protocol::identity::valid_room_name(full.clone()).then_some(full)
        }
    };
    {
        let (weak, target, name, until, said, from_at) = (
            model.downgrade(),
            target.clone(),
            name.downgrade(),
            until.downgrade(),
            said.downgrade(),
            from_at.clone(),
        );
        make.connect_clicked(move |button| {
            let (Some(model), Some(room)) = (weak.upgrade(), target()) else {
                return;
            };
            let who = name
                .upgrade()
                .map(|name| name.text().to_string())
                .unwrap_or_default();
            let starts = from_at();
            let until_at = until
                .upgrade()
                .and_then(|until| hours.get(until.selected() as usize).copied())
                .and_then(|hours| hours.parse::<i64>().ok())
                .map(|hours| starts.unwrap_or_else(words::now) + hours * 3600);
            button.set_sensitive(false);
            let (button, said) = (button.downgrade(), said.clone());
            glib::spawn_future_local(async move {
                let made = model.make_guest_link(room, who, starts, until_at).await;
                if let Some(button) = button.upgrade() {
                    button.set_sensitive(true);
                }
                let Some(said) = said.upgrade() else {
                    return;
                };
                reveal_parent(&said);
                match made {
                    Ok(made) => {
                        pieces::copy(&made.link);
                        let mut line = match made.until.as_deref().and_then(words::instant) {
                            Some(until) => t_with(
                                "This link works until {until}.",
                                &[("until", &words::said(until))],
                            ),
                            None => t("This link works until somebody stops it."),
                        };
                        if let Some(name) = made.name.filter(|name| !name.is_empty()) {
                            line.push(' ');
                            line.push_str(&t_with(
                                "Whoever uses it joins as {name}.",
                                &[("name", &name)],
                            ));
                        }
                        said.set_label(&format!("{line}\n{}", made.link));
                        said.remove_css_class("error");
                    }
                    Err(refusal) => {
                        said.set_label(&refusal);
                        said.add_css_class("error");
                    }
                }
            });
        });
    }
    {
        let (weak, said) = (model.downgrade(), said.downgrade());
        stop.connect_clicked(move |button| {
            let (Some(model), Some(room)) = (weak.upgrade(), target()) else {
                return;
            };
            button.set_sensitive(false);
            let (button, said) = (button.downgrade(), said.clone());
            glib::spawn_future_local(async move {
                let stopped = model.stop_guest_links(room).await;
                if let Some(button) = button.upgrade() {
                    button.set_sensitive(true);
                }
                if let Some(said) = said.upgrade() {
                    reveal_parent(&said);
                    match stopped {
                        Ok(1) => said.set_label(&t("One link stopped working.")),
                        Ok(count) => said.set_label(&t_with(
                            "{count} links stopped working.",
                            &[("count", &count)],
                        )),
                        Err(refusal) => said.set_label(&refusal),
                    }
                }
            });
        });
    }
    group
}

/// "Works from": a row whose switch says whether the link waits for a
/// moment, opening onto the month and a quarter-hour clock; and what it
/// says (`None` while it works from now).
fn works_from() -> (adw::ExpanderRow, Rc<dyn Fn() -> Option<i64>>) {
    use crate::app::schedule::{self, Day, LocalZone};
    let (day, hour, minute) = schedule::start(words::now(), &LocalZone);
    let chosen = Rc::new(std::cell::Cell::new((day, hour, minute)));
    let row = adw::ExpanderRow::builder()
        .title(t("Works from"))
        .subtitle("—")
        .show_enable_switch(true)
        .enable_expansion(false)
        .build();
    row.set_widget_name("settings.guest-from");
    let at = {
        let (chosen, row) = (chosen.clone(), row.downgrade());
        Rc::new(move || -> Option<i64> {
            let row = row.upgrade()?;
            if !row.enables_expansion() {
                return None;
            }
            let (day, hour, minute) = chosen.get();
            Some(schedule::instant(
                day,
                hour,
                minute,
                &LocalZone,
                words::now(),
            ))
        }) as Rc<dyn Fn() -> Option<i64>>
    };
    let say = {
        let (row, at) = (row.downgrade(), at.clone());
        Rc::new(move || {
            if let Some(row) = row.upgrade() {
                row.set_subtitle(&at().map(words::said).unwrap_or_else(|| "—".to_owned()));
            }
        })
    };
    let today = {
        let wall = words::Wall::local(words::now());
        Day {
            year: wall.year,
            month: wall.month,
            day: wall.day,
        }
    };
    let month = {
        let (chosen, say) = (chosen.clone(), say.clone());
        crate::ui::lobby::calendar::month(day, today, today, today.add_days(365), move |day| {
            let (_, hour, minute) = chosen.get();
            chosen.set((day, hour, minute));
            say();
        })
    };
    month.set_margin_top(8);
    month.set_margin_bottom(4);
    month.set_margin_start(8);
    month.set_margin_end(8);
    let hours: Vec<String> = (0..24).map(|hour| format!("{hour:02}")).collect();
    let hour_choice =
        gtk::DropDown::from_strings(&hours.iter().map(String::as_str).collect::<Vec<_>>());
    hour_choice.set_selected(hour);
    hour_choice.update_property(&[gtk::accessible::Property::Label(&t("Hour"))]);
    let minute_choice = gtk::DropDown::from_strings(&["00", "15", "30", "45"]);
    minute_choice.set_selected(minute / 15);
    minute_choice.update_property(&[gtk::accessible::Property::Label(&t("Minute"))]);
    for choice in [&hour_choice, &minute_choice] {
        let (chosen, say, hour_choice, minute_choice) = (
            chosen.clone(),
            say.clone(),
            hour_choice.downgrade(),
            minute_choice.downgrade(),
        );
        choice.connect_selected_notify(move |_| {
            let (Some(hour), Some(minute)) = (hour_choice.upgrade(), minute_choice.upgrade())
            else {
                return;
            };
            let (day, _, _) = chosen.get();
            chosen.set((day, hour.selected(), minute.selected() * 15));
            say();
        });
    }
    let clock = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    clock.append(&hour_choice);
    clock.append(&gtk::Label::new(Some(":")));
    clock.append(&minute_choice);
    let time = adw::ActionRow::builder().title(t("Time")).build();
    clock.set_valign(gtk::Align::Center);
    time.add_suffix(&clock);
    let holder = gtk::ListBoxRow::builder()
        .child(&month)
        .activatable(false)
        .selectable(false)
        .build();
    row.add_row(&holder);
    row.add_row(&time);
    row.connect_enable_expansion_notify(move |_| say());
    // TOMOSHIBI_DEBUG_GUEST_FROM=1 opens it, for a picture.
    #[cfg(debug_assertions)]
    if std::env::var_os("TOMOSHIBI_DEBUG_GUEST_FROM").is_some() {
        row.set_enable_expansion(true);
        row.set_expanded(true);
    }
    (row, at)
}

/// Unfolds the revealer a line was put in.
fn reveal_parent(line: &gtk::Label) {
    if let Some(slot) = line.parent().and_downcast::<gtk::Revealer>() {
        slot.set_reveal_child(true);
    }
}

/// Account.tsx `Picture`: a picture chosen from the disk, scaled down to at
/// most 256 square and sent as a JPEG data URL, under the server's 96 KiB.
fn choose_picture(context: &Context) {
    let filter = gtk::FileFilter::new();
    filter.add_pixbuf_formats();
    let filters = gtk::gio::ListStore::new::<gtk::FileFilter>();
    filters.append(&filter);
    let dialog = gtk::FileDialog::builder()
        .title(t("Choose a picture"))
        .filters(&filters)
        .build();
    let window = context.window();
    let weak = context.model.downgrade();
    dialog.open(
        window.as_ref(),
        gtk::gio::Cancellable::NONE,
        move |chosen| {
            let (Ok(file), Some(model)) = (chosen, weak.upgrade()) else {
                return;
            };
            let Some(path) = file.path() else {
                return;
            };
            glib::spawn_future_local(async move {
                // Decoded, scaled and encoded on a thread of its own: a
                // camera's photograph is tens of megapixels, and decoding it
                // on the main thread held the window still for as long.
                let data_url = gtk::gio::spawn_blocking(move || {
                    gtk::gdk_pixbuf::Pixbuf::from_file_at_scale(&path, 256, 256, true)
                        .ok()
                        .and_then(|pixbuf| {
                            pixbuf.save_to_bufferv("jpeg", &[("quality", "82")]).ok()
                        })
                        .map(|bytes| {
                            format!("data:image/jpeg;base64,{}", glib::base64_encode(&bytes))
                        })
                })
                .await
                .ok()
                .flatten();
                let Some(data_url) = data_url else {
                    model.mention(t("That picture could not be used."));
                    return;
                };
                if let Err(said) = model.set_picture(data_url).await {
                    model.mention(said);
                }
            });
        },
    );
}
