//! The meetings this account has arranged, and arranging one (Schedule.tsx;
//! ScheduleView.swift and ScheduleView.cs): a name, a day, a time in quarter
//! hours on a 24-hour clock, the machine where there is a choice; and the
//! list, each with its link to copy, a calendar file to keep, and a way to
//! call it off. A section beside the sidebar, as on the Mac.
//!
//! The form's day and time are the lobby's (app::schedule::ScheduleForm),
//! started at the next quarter hour and started again at every opening for
//! whatever nobody chose, with the other apps' rules for a change of the
//! clocks.

use std::{cell::RefCell, rc::Rc};

use adw::prelude::*;
use gtk::glib;
use tomoshibi_protocol::{
    api::Arrangement,
    identity::{generate_room_name, normalise_room_name, valid_room_name},
};

use super::pieces::{self, icon, set_class};
use crate::{
    app::{
        lobby::{self, Status},
        schedule::{Day, LocalZone, ScheduleForm, TooSoonOrLate, AHEAD_DAYS},
        words, AppModel, Change,
    },
    i18n::{t, t_with},
    ui::{kit, Context},
};

/// The form, kept by the lobby so that a section built again (a change of
/// language) keeps what was typed and chosen.
pub struct Draft {
    pub planning: RefCell<String>,
    pub form: RefCell<ScheduleForm>,
    /// The meeting whose link was just put on the clipboard.
    pub copied: RefCell<Option<String>>,
}

impl Default for Draft {
    fn default() -> Self {
        Draft {
            planning: RefCell::new(String::new()),
            form: RefCell::new(ScheduleForm::new(words::now(), &LocalZone)),
            copied: RefCell::new(None),
        }
    }
}

/// The schedule opened: what nobody chose starts again from now.
pub fn opened(model: &AppModel) {
    model
        .kept::<Draft>()
        .form
        .borrow_mut()
        .open(words::now(), &LocalZone);
    model.refresh_arrangements();
}

pub fn section(context: &Context) -> gtk::Widget {
    let model = &context.model;
    let draft = model.kept::<Draft>();

    let title = gtk::Label::new(Some(&t("Scheduled meetings")));
    title.set_xalign(0.0);
    title.add_css_class("tmsb-section-heading");
    let subtitle = gtk::Label::builder()
        .label(t("Pick a time and hand out the link before it starts."))
        .xalign(0.0)
        .wrap(true)
        .build();
    subtitle.add_css_class("tmsb-muted");
    let heading = gtk::Box::new(gtk::Orientation::Vertical, 4);
    heading.append(&title);
    heading.append(&subtitle);

    let form = form(context, &draft);
    let list = gtk::Box::new(gtk::Orientation::Vertical, 10);
    list.set_hexpand(true);
    list.set_widget_name("schedule.list");

    // Side by side once both fit, one above the other below that; one
    // above the other at first, which fits any width: side by side at
    // first, the section asked for 782 at its first layout in a window
    // 1000 wide, before it was told its width, and the split view was 22
    // too wide for the window (seen in Japanese).
    let together = gtk::Box::new(gtk::Orientation::Vertical, 20);
    let form_arrival = kit::Arrival::new(&form).rises(40);
    form_arrival.set_size_request(360, -1);
    form_arrival.set_valign(gtk::Align::Start);
    together.append(&form_arrival);
    let list_arrival = kit::Arrival::new(&list).rises(80);
    list_arrival.set_valign(gtk::Align::Start);
    list_arrival.set_hexpand(true);
    together.append(&list_arrival);

    let page = gtk::Box::new(gtk::Orientation::Vertical, 20);
    page.set_margin_top(16);
    page.set_margin_bottom(28);
    page.set_margin_start(28);
    page.set_margin_end(28);
    page.append(&kit::Arrival::new(&heading).rises(0));
    page.append(&together);
    let scroller = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .child(&page)
        .vexpand(true)
        .build();

    // Narrower than both side by side (360 + 20 + 420 and the margins):
    // one above the other.
    let lay = {
        let together = together.downgrade();
        move |width: i32| {
            if let Some(together) = together.upgrade() {
                together.set_orientation(if width >= 856 {
                    gtk::Orientation::Horizontal
                } else {
                    gtk::Orientation::Vertical
                });
            }
        }
    };
    let watcher = gtk::DrawingArea::new();
    watcher.set_can_target(false);
    watcher.set_hexpand(true);
    watcher.set_size_request(-1, 0);
    watcher.connect_resize(move |_, width, _| lay(width));
    page.prepend(&watcher);

    let fill = {
        let (list, ctx) = (list.downgrade(), context.clone());
        move |model: &AppModel| {
            if let Some(list) = list.upgrade() {
                fill_list(&ctx, model, &list);
            }
        }
    };
    fill(model);
    model.subscribe_while(&scroller, move |model, change| {
        if change == lobby::CHANGED || matches!(change, Change::Account) {
            fill(model);
        }
    });
    scroller.upcast()
}

fn form(context: &Context, draft: &Rc<Draft>) -> gtk::Box {
    let model = &context.model;
    let card = gtk::Box::new(gtk::Orientation::Vertical, 14);
    card.add_css_class("tmsb-card");
    if !model.holds("arrange") {
        let said = gtk::Label::builder()
            .label(t("Arranging meetings has not been opened to you."))
            .wrap(true)
            .xalign(0.0)
            .build();
        said.add_css_class("tmsb-muted");
        card.append(&said);
        return card;
    }

    let room = gtk::Entry::builder()
        .placeholder_text(t("Room name"))
        .text(draft.planning.borrow().as_str())
        .build();
    room.add_css_class("tmsb-input");
    super::sheet::mono_once_typed(&room);
    room.set_widget_name("schedule.room");
    room.update_property(&[gtk::accessible::Property::Label(&t("Room name"))]);
    let hint = gtk::Label::builder()
        .label(t(
            "Name it yourself, or leave it blank for one nobody has used.",
        ))
        .wrap(true)
        .xalign(0.0)
        .build();
    hint.add_css_class("tmsb-muted");
    hint.add_css_class("tmsb-small");

    let form = draft.form.borrow().clone();
    // From today to the furthest day a meeting can be arranged for, as the
    // Mac's month offers (the verdict below says why any other is refused).
    let today = {
        let wall = words::Wall::local(words::now());
        Day {
            year: wall.year,
            month: wall.month,
            day: wall.day,
        }
    };
    let say_later: Rc<RefCell<Option<Rc<dyn Fn()>>>> = Rc::new(RefCell::new(None));
    let calendar = {
        let (draft, say_later) = (draft.clone(), say_later.clone());
        super::calendar::month(
            form.day,
            today,
            today,
            today.add_days(AHEAD_DAYS),
            move |day| {
                draft.form.borrow_mut().choose_day(day);
                if let Some(say) = say_later.borrow().as_ref() {
                    say();
                }
            },
        )
    };
    let calendar_well = gtk::Box::new(gtk::Orientation::Vertical, 0);
    calendar_well.add_css_class("tmsb-well");
    calendar_well.add_css_class("tmsb-month-well");
    calendar_well.append(&calendar);

    let hours: Vec<String> = (0..24).map(|hour| format!("{hour:02}")).collect();
    let hour = gtk::DropDown::from_strings(&hours.iter().map(String::as_str).collect::<Vec<_>>());
    hour.set_selected(form.hour);
    hour.update_property(&[gtk::accessible::Property::Label(&t("Time"))]);
    let minutes = ["00", "15", "30", "45"];
    let minute = gtk::DropDown::from_strings(&minutes);
    minute.set_selected(form.minute / 15);
    let colon = gtk::Label::new(Some(":"));
    let clock = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    clock.append(&hour);
    clock.append(&colon);
    clock.append(&minute);
    let time_row = labelled_row(&t("Time"), "alarm-symbolic", &clock);

    let server_row = labelled_row(
        &t("Server"),
        "network-server-symbolic",
        &pieces::server_menu(model),
    );
    let server_slot = pieces::unfolding(&server_row, model.offers_relay_choice());

    let verdict_symbol = icon("alarm-symbolic", 12);
    let verdict = gtk::Label::builder()
        .wrap(true)
        .xalign(0.0)
        .hexpand(true)
        .build();
    verdict.add_css_class("tmsb-small");
    let verdict_row = gtk::Box::new(gtk::Orientation::Horizontal, 7);
    verdict_row.append(&verdict_symbol);
    verdict_row.append(&verdict);
    verdict_row.set_widget_name("schedule.verdict");

    let arrange = kit::primary_button(&t("Arrange"));
    arrange.set_height_request(40);
    arrange.add_css_class("tmsb-compact");
    arrange.set_widget_name("schedule.arrange");

    card.append(&room);
    card.append(&hint);
    card.append(&calendar_well);
    card.append(&time_row);
    card.append(&server_slot);
    card.append(&kit::divider());
    card.append(&verdict_row);
    card.append(&arrange);

    let arranging = Rc::new(std::cell::Cell::new(false));
    let say = {
        let (verdict, verdict_symbol, arrange, draft, arranging) = (
            verdict.downgrade(),
            verdict_symbol.downgrade(),
            arrange.downgrade(),
            draft.clone(),
            arranging.clone(),
        );
        move || {
            let (Some(verdict), Some(symbol), Some(arrange)) = (
                verdict.upgrade(),
                verdict_symbol.upgrade(),
                arrange.upgrade(),
            ) else {
                return;
            };
            let now = words::now();
            let form = draft.form.borrow().clone();
            let problem = form.problem(&LocalZone, now);
            let said = match problem {
                Some(TooSoonOrLate::Gone) => t("That time has already gone."),
                Some(TooSoonOrLate::TooFar) => t("That is more than sixty days away."),
                None => t_with(
                    "Arranged for {when}",
                    &[("when", &words::said(form.instant(&LocalZone, now)))],
                ),
            };
            verdict.set_label(&said);
            set_class(&verdict, "tmsb-danger-text", problem.is_some());
            set_class(&verdict, "tmsb-muted", problem.is_none());
            symbol.set_icon_name(Some(if problem.is_some() {
                "dialog-warning-symbolic"
            } else {
                "alarm-symbolic"
            }));
            set_class(&symbol, "tmsb-danger-text", problem.is_some());
            set_class(&symbol, "tmsb-muted", problem.is_none());
            arrange.set_sensitive(problem.is_none() && !arranging.get());
        }
    };
    say();
    {
        let (draft, say) = (draft.clone(), say.clone());
        room.connect_changed(move |room| {
            draft.planning.replace(room.text().to_string());
            say();
        });
    }
    {
        let say = say.clone();
        say_later.replace(Some(Rc::new(move || say())));
    }
    {
        let (draft, say, minute) = (draft.clone(), say.clone(), minute.downgrade());
        hour.connect_selected_notify(move |hour| {
            let minute = minute.upgrade().map(|m| m.selected()).unwrap_or(0) * 15;
            draft.form.borrow_mut().choose_time(hour.selected(), minute);
            say();
        });
    }
    {
        let (draft, say, hour) = (draft.clone(), say.clone(), hour.downgrade());
        minute.connect_selected_notify(move |minute| {
            let hour = hour.upgrade().map(|h| h.selected()).unwrap_or(0);
            draft
                .form
                .borrow_mut()
                .choose_time(hour, minute.selected() * 15);
            say();
        });
    }
    // The verdict says the time again as the clock moves on.
    {
        let say = say.clone();
        let source = glib::timeout_add_seconds_local(30, move || {
            say();
            glib::ControlFlow::Continue
        });
        let source = RefCell::new(Some(source));
        card.connect_destroy(move |_| {
            if let Some(source) = source.take() {
                source.remove();
            }
        });
    }
    {
        let (weak, draft, arranging, say, room) = (
            model.downgrade(),
            draft.clone(),
            arranging.clone(),
            say.clone(),
            room.downgrade(),
        );
        arrange.connect_clicked(move |_| {
            let Some(model) = weak.upgrade() else {
                return;
            };
            if arranging.get() {
                return;
            }
            let now = words::now();
            let form = draft.form.borrow().clone();
            if form.problem(&LocalZone, now).is_some() {
                say();
                return;
            }
            let typed = draft.planning.borrow().clone();
            let normal = normalise_room_name(typed);
            let target = if valid_room_name(normal.clone()) {
                normal
            } else {
                generate_room_name()
            };
            arranging.set(true);
            say();
            let (draft, arranging, say, room) =
                (draft.clone(), arranging.clone(), say.clone(), room.clone());
            glib::spawn_future_local(async move {
                let made = model
                    .arrange(target, form.instant(&LocalZone, now), model.relay_choice())
                    .await;
                arranging.set(false);
                match made {
                    Ok(made) => {
                        draft.planning.replace(String::new());
                        draft.form.borrow_mut().arranged();
                        if let Some(room) = room.upgrade() {
                            room.set_text("");
                        }
                        // The link is what is handed out, so it is on the
                        // clipboard at once.
                        if let Some(link) = model.meeting_link(&made) {
                            pieces::copy(&link);
                            draft.copied.replace(Some(made.id.clone()));
                            let (weak, draft, id) =
                                (model.downgrade(), draft.clone(), made.id.clone());
                            glib::timeout_add_seconds_local_once(2, move || {
                                if draft.copied.borrow().as_deref() == Some(id.as_str()) {
                                    draft.copied.replace(None);
                                    if let Some(model) = weak.upgrade() {
                                        model.announce(lobby::CHANGED);
                                    }
                                }
                            });
                            model.announce(lobby::CHANGED);
                        }
                    }
                    Err(refusal) => model.mention(refusal),
                }
                say();
            });
        });
    }
    let slot = server_slot.downgrade();
    model.subscribe_while(&card, move |model, change| {
        if change == crate::app::CHANGED {
            if let Some(slot) = slot.upgrade() {
                slot.set_reveal_child(model.offers_relay_choice());
            }
        }
    });
    card
}

fn labelled_row(title: &str, symbol: &str, control: &impl IsA<gtk::Widget>) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let image = icon(symbol, 12);
    image.add_css_class("tmsb-muted");
    let words = gtk::Label::new(Some(title));
    words.add_css_class("tmsb-muted");
    words.set_xalign(0.0);
    words.set_hexpand(true);
    row.append(&image);
    row.append(&words);
    row.append(control);
    row
}

fn fill_list(context: &Context, model: &AppModel, list: &gtk::Box) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }
    let state = model.lobby();
    match state.status.get() {
        Status::SignedOut => {
            list.append(&notice(
                &t("Your session has expired. Sign in again."),
                None,
            ));
            return;
        }
        Status::Failed => {
            let again = kit::plain_button(&t("Try again"));
            again.set_height_request(30);
            again.add_css_class("tmsb-compact");
            let weak = model.downgrade();
            again.connect_clicked(move |_| {
                if let Some(model) = weak.upgrade() {
                    model.refresh_arrangements();
                }
            });
            list.append(&notice(&t("Could not load your meetings."), Some(&again)));
            return;
        }
        _ => {}
    }
    let upcoming = model.upcoming();
    if upcoming.is_empty() {
        if state.status.get() == Status::Loading {
            let spinner = gtk::Spinner::builder()
                .spinning(true)
                .halign(gtk::Align::Center)
                .build();
            spinner.set_size_request(16, 16);
            spinner.set_margin_top(24);
            list.append(&spinner);
            return;
        }
        let empty = gtk::Box::new(gtk::Orientation::Vertical, 10);
        empty.add_css_class("tmsb-card");
        empty.set_widget_name("schedule.empty");
        let symbol = icon("x-office-calendar-symbolic", 26);
        symbol.add_css_class("tmsb-muted");
        empty.append(&symbol);
        let said = gtk::Label::new(Some(&t("No meetings arranged yet.")));
        said.add_css_class("tmsb-strong");
        empty.append(&said);
        list.append(&empty);
        return;
    }
    let copied = model.kept::<Draft>().copied.borrow().clone();
    for one in &upcoming {
        list.append(&meeting_card(
            context,
            model,
            one,
            copied.as_deref() == Some(&one.id),
        ));
    }
}

fn notice(said: &str, action: Option<&gtk::Button>) -> gtk::Box {
    let card = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    card.add_css_class("tmsb-card");
    let symbol = icon("dialog-warning-symbolic", 16);
    symbol.add_css_class("tmsb-muted");
    card.append(&symbol);
    let words = gtk::Label::builder()
        .label(said)
        .wrap(true)
        .xalign(0.0)
        .hexpand(true)
        .build();
    card.append(&words);
    if let Some(action) = action {
        card.append(action);
    }
    card
}

fn meeting_card(context: &Context, model: &AppModel, one: &Arrangement, copied: bool) -> gtk::Box {
    let card = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    card.add_css_class("tmsb-card");
    card.add_css_class("tmsb-meeting");
    let column = gtk::Box::new(gtk::Orientation::Vertical, 3);
    column.set_hexpand(true);
    let room = gtk::Label::new(Some(&one.room));
    room.set_xalign(0.0);
    room.set_ellipsize(gtk::pango::EllipsizeMode::End);
    room.add_css_class("tmsb-mono");
    room.add_css_class("tmsb-strong");
    let when = gtk::Label::new(Some(
        &words::instant(&one.at)
            .map(words::said)
            .unwrap_or_else(|| one.at.clone()),
    ));
    when.set_xalign(0.0);
    when.add_css_class("tmsb-muted");
    when.add_css_class("tmsb-small");
    column.append(&room);
    column.append(&when);
    if one.started {
        let badge = gtk::Label::new(Some(&t("Under way")));
        badge.add_css_class("tmsb-under-way");
        badge.set_halign(gtk::Align::Start);
        column.append(&badge);
    }
    card.append(&column);
    if let Some(link) = model.meeting_link(one) {
        let copy = pieces::round_button(
            &icon(
                if copied {
                    "object-select-symbolic"
                } else {
                    "edit-copy-symbolic"
                },
                13,
            ),
            32,
            &if copied {
                t("Link copied")
            } else {
                t("Copy link")
            },
        );
        set_class(&copy, "tmsb-tally-text", copied);
        let link_copy = link.clone();
        copy.connect_clicked(move |button| {
            pieces::copy(&link_copy);
            button.set_child(Some(&icon("object-select-symbolic", 13)));
            button.add_css_class("tmsb-tally-text");
        });
        card.append(&copy);
        let calendar = pieces::round_button(
            &icon("x-office-calendar-symbolic", 13),
            32,
            &t("Add to calendar"),
        );
        let (weak, one_kept, ctx) = (model.downgrade(), one.clone(), context.clone());
        calendar.connect_clicked(move |_| {
            if let Some(model) = weak.upgrade() {
                save_calendar(&ctx, &model, &one_kept);
            }
        });
        card.append(&calendar);
    }
    if one.mine {
        let cancel =
            pieces::round_button(&icon("window-close-symbolic", 12), 32, &t("Cancel meeting"));
        let (weak, id) = (model.downgrade(), one.id.clone());
        cancel.connect_clicked(move |button| {
            button.set_sensitive(false);
            let (weak, id) = (weak.clone(), id.clone());
            glib::spawn_future_local(async move {
                if let Some(model) = weak.upgrade() {
                    if let Err(refusal) = model.cancel_meeting(id).await {
                        model.mention(refusal);
                    }
                }
            });
        });
        card.append(&cancel);
    }
    card
}

/// calendar.ts `icsFor`, saved where the person chooses.
fn save_calendar(context: &Context, model: &AppModel, one: &Arrangement) {
    let Some(text) = model.calendar_text(one) else {
        return;
    };
    let dialog = gtk::FileDialog::builder()
        .title(t("Add to calendar"))
        .initial_name(format!("{}.ics", one.room.replace('@', "-")))
        .build();
    let window = context.window();
    let weak = model.downgrade();
    dialog.save(
        window.as_ref(),
        gtk::gio::Cancellable::NONE,
        move |chosen| {
            let Ok(file) = chosen else {
                return;
            };
            let written = file
                .path()
                .map(|path| std::fs::write(path, text.as_bytes()));
            if !matches!(written, Some(Ok(()))) {
                if let Some(model) = weak.upgrade() {
                    model.mention(t("Something went wrong. Try again."));
                }
            }
        },
    );
}
