//! The lobby, laid out as the Mac's (Lobby.swift `SidebarLobby`): a sidebar
//! with the two things to do at its head (Start, Join), the places beside
//! the home (the scheduled meetings, the server where there is a choice),
//! the meetings coming up and the rooms joined lately, and at its foot a
//! newer build, the fleet's light and the account; beside it the stage, with
//! the picture of yourself or the animals, a greeting, and the microphone
//! and camera as they will be on joining.
//!
//! The section somebody is in is kept in the model, so that a lobby built
//! again (a change of language) opens where it was.

use std::cell::Cell;

use adw::prelude::*;
use gtk::glib;

use super::{
    animals::Portraits,
    pieces::{self, icon, set_class},
    schedule, servers, sheet, stage, update_offer,
};
use crate::{
    app::{self, lobby, log, words, AppModel, Change},
    i18n::{t, t_with},
    theme::motion,
    ui::{kit, settings, Context},
};

/// Where in the lobby somebody is, beside its home.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Place {
    #[default]
    Home,
    Scheduled,
    Servers,
}

impl Place {
    fn name(self) -> &'static str {
        match self {
            Place::Home => "home",
            Place::Scheduled => "scheduled",
            Place::Servers => "servers",
        }
    }
}

/// What the lobby keeps across being built again.
#[derive(Default)]
pub struct LobbyPlace {
    pub place: Cell<Place>,
}

const CHANGED: app::Change = app::Change::Module("lobby/place");

fn go(model: &AppModel, place: Place) {
    let kept = model.kept::<LobbyPlace>();
    if kept.place.replace(place) != place {
        if place == Place::Scheduled {
            schedule::opened(model);
        }
        model.announce(CHANGED);
    }
}

pub fn page(context: &Context) -> gtk::Widget {
    let model = &context.model;
    let Some(account) = model.account() else {
        return gtk::Box::new(gtk::Orientation::Vertical, 0).upcast();
    };

    // ---- the sidebar ----------------------------------------------------

    let start = kit::primary_button("");
    start.set_child(Some(&pieces::labelled(
        Some(icon("camera-video-symbolic", 15).upcast_ref()),
        &t("Start"),
    )));
    start.set_tooltip_text(Some(&t("Start a meeting")));
    start.set_height_request(crate::theme::scale::CONTROL_LARGE);
    start.add_css_class("tmsb-compact");
    start.set_hexpand(true);
    start.set_widget_name("lobby.start");
    let join = kit::plain_button("");
    join.set_child(Some(&pieces::labelled(
        Some(pieces::glyph(pieces::Glyph::DoorIn, 15).upcast_ref()),
        &t("Join"),
    )));
    join.set_tooltip_text(Some(&t("Join a meeting")));
    join.set_hexpand(true);
    join.add_css_class("tmsb-compact");
    join.set_widget_name("lobby.join");
    let actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    actions.set_homogeneous(true);
    actions.set_margin_start(12);
    actions.set_margin_end(12);
    actions.set_margin_top(4);
    actions.set_margin_bottom(8);
    actions.append(&start);
    actions.append(&join);

    let places = gtk::ListBox::new();
    places.add_css_class("navigation-sidebar");
    places.add_css_class("tmsb-places");
    let home_row = place_row(&t("Home"), "go-home-symbolic", None);
    let scheduled_badge = gtk::Label::new(None);
    scheduled_badge.add_css_class("tmsb-badge");
    let scheduled_row = place_row(
        &t("Scheduled meetings"),
        "x-office-calendar-symbolic",
        Some(&scheduled_badge),
    );
    scheduled_row.set_widget_name("lobby.scheduled");
    let servers_row = place_row(&t("Server"), "network-server-symbolic", None);
    servers_row.set_widget_name("lobby.servers");
    places.append(&home_row);
    places.append(&scheduled_row);
    places.append(&servers_row);

    let upcoming_title = section_title(&t("Upcoming"));
    let upcoming = gtk::ListBox::new();
    upcoming.add_css_class("navigation-sidebar");
    upcoming.set_selection_mode(gtk::SelectionMode::None);
    let upcoming_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
    upcoming_box.append(&upcoming_title);
    upcoming_box.append(&upcoming);
    let upcoming_revealer = pieces::unfolding(&upcoming_box, false);

    let recent_title = section_title(&t("Recent"));
    let recent = gtk::ListBox::new();
    recent.add_css_class("navigation-sidebar");
    recent.set_selection_mode(gtk::SelectionMode::None);
    let recent_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
    recent_box.append(&recent_title);
    recent_box.append(&recent);
    let recent_revealer = pieces::unfolding(&recent_box, false);

    let lists = gtk::Box::new(gtk::Orientation::Vertical, 6);
    lists.append(&places);
    lists.append(&upcoming_revealer);
    lists.append(&recent_revealer);
    let scroller = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vexpand(true)
        .child(&lists)
        .build();

    let foot = gtk::Box::new(gtk::Orientation::Vertical, 8);
    foot.set_margin_start(10);
    foot.set_margin_end(10);
    foot.set_margin_bottom(10);
    foot.append(&update_offer::offer(model));
    foot.append(&fleet_line(model));
    foot.append(&account_button(context, &account.name));

    let sidebar_column = gtk::Box::new(gtk::Orientation::Vertical, 0);
    sidebar_column.append(&kit::Arrival::new(&actions).rises(0));
    sidebar_column.append(&scroller);
    sidebar_column.append(&kit::Arrival::new(&foot).rises(80));
    let sidebar = adw::ToolbarView::new();
    sidebar.add_top_bar(&kit::bar());
    sidebar.set_content(Some(&sidebar_column));
    sidebar.add_css_class("tmsb-sidebar");

    // ---- the main area --------------------------------------------------

    let home = home_stage(context, &account.name);
    let sections = gtk::Stack::builder()
        .transition_type(gtk::StackTransitionType::Crossfade)
        .transition_duration(motion::SETTLE_MS)
        .hhomogeneous(false)
        .vhomogeneous(false)
        .build();
    sections.add_named(&home.0, Some(Place::Home.name()));
    let content_bar = kit::bar();
    let server_menu = pieces::server_menu(model);
    let server_slot = pieces::fading(&server_menu, model.offers_relay_choice());
    content_bar.pack_end(&server_slot);
    let toggle = gtk::ToggleButton::builder()
        .icon_name("sidebar-show-symbolic")
        .tooltip_text(t("Show the sidebar"))
        .visible(false)
        .build();
    content_bar.pack_start(&toggle);
    let main = adw::ToolbarView::new();
    main.add_top_bar(&content_bar);
    main.add_top_bar(&pieces::notice_card(model));
    main.set_content(Some(&pieces::with_toast(model, &sections)));
    main.add_css_class("tmsb-main");
    // The stage reaches the top of the window under the bar, whose ground
    // is clear, as the Mac's stage runs under its toolbar: below the bar,
    // a camera picture had a strip of the page's ground above it, 46 high
    // and light by day. The greeting and the other sections start below the
    // bars, however tall the notice makes them.
    main.set_extend_content_to_top_edge(true);
    {
        let (words, sections) = (home.2.downgrade(), sections.downgrade());
        let follow = move |main: &adw::ToolbarView| {
            let height = main.top_bar_height();
            if let Some(words) = words.upgrade() {
                words.set_margin_top(height + 10);
            }
            if let Some(sections) = sections.upgrade() {
                let mut child = sections.first_child();
                while let Some(section) = child {
                    child = section.next_sibling();
                    if sections.page(&section).name().as_deref() != Some(Place::Home.name()) {
                        section.set_margin_top(height);
                    }
                }
            }
        };
        follow(&main);
        main.connect_top_bar_height_notify(follow);
    }

    let split = adw::OverlaySplitView::builder()
        .sidebar(&sidebar)
        .content(&main)
        .min_sidebar_width(240.0)
        .max_sidebar_width(300.0)
        .sidebar_width_fraction(0.25)
        .build();
    split
        .bind_property("collapsed", &toggle, "visible")
        .sync_create()
        .build();
    split
        .bind_property("show-sidebar", &toggle, "active")
        .sync_create()
        .bidirectional()
        .build();
    let narrow = adw::Breakpoint::new(
        adw::BreakpointCondition::parse("max-width: 640sp").expect("a condition"),
    );
    narrow.add_setter(&split, "collapsed", Some(&true.to_value()));
    let bin = adw::BreakpointBin::builder()
        .child(&split)
        .width_request(360)
        .height_request(400)
        .build();
    bin.add_breakpoint(narrow);
    bin.add_css_class("tmsb-lobby");

    // ---- what it does -----------------------------------------------------

    let ctx = context.clone();
    start.connect_clicked(move |_| sheet::present(&ctx, sheet::Mode::Start));
    let ctx = context.clone();
    join.connect_clicked(move |_| sheet::present(&ctx, sheet::Mode::Join(String::new())));

    {
        let weak = model.downgrade();
        let (home_row, scheduled_row, servers_row) =
            (home_row.clone(), scheduled_row.clone(), servers_row.clone());
        places.connect_row_activated(move |_, row| {
            let Some(model) = weak.upgrade() else {
                return;
            };
            let place = if row == &home_row {
                Place::Home
            } else if row == &scheduled_row {
                Place::Scheduled
            } else if row == &servers_row {
                Place::Servers
            } else {
                return;
            };
            go(&model, place);
        });
    }

    let show_place = {
        let (sections, places, home_row, scheduled_row, servers_row, ctx) = (
            sections.downgrade(),
            places.downgrade(),
            home_row.downgrade(),
            scheduled_row.downgrade(),
            servers_row.downgrade(),
            context.clone(),
        );
        move |model: &AppModel| {
            let (Some(sections), Some(places)) = (sections.upgrade(), places.upgrade()) else {
                return;
            };
            let mut place = model.kept::<LobbyPlace>().place.get();
            if place == Place::Servers && !model.offers_relay_choice() {
                place = Place::Home;
                model.kept::<LobbyPlace>().place.set(place);
            }
            // Each section is built when it is first gone to, and built
            // again on every visit after so that it arrives as it does the
            // first time.
            if place != Place::Home {
                if let Some(old) = sections.child_by_name(place.name()) {
                    sections.remove(&old);
                }
                let section = match place {
                    Place::Scheduled => schedule::section(&ctx),
                    Place::Servers => servers::section(&ctx),
                    Place::Home => unreachable!(),
                };
                // Below the bars the stage runs under (above).
                if let Some(main) = sections.ancestor(adw::ToolbarView::static_type()) {
                    if let Ok(main) = main.downcast::<adw::ToolbarView>() {
                        section.set_margin_top(main.top_bar_height());
                    }
                }
                sections.add_named(&section, Some(place.name()));
            }
            sections.set_visible_child_name(place.name());
            let row = match place {
                Place::Home => home_row.upgrade(),
                Place::Scheduled => scheduled_row.upgrade(),
                Place::Servers => servers_row.upgrade(),
            };
            places.select_row(row.as_ref());
        }
    };
    show_place(model);

    let fill = {
        let (
            servers_row,
            server_slot,
            scheduled_badge,
            upcoming,
            upcoming_revealer,
            recent,
            recent_revealer,
            ctx,
        ) = (
            servers_row.downgrade(),
            server_slot.downgrade(),
            scheduled_badge.downgrade(),
            upcoming.downgrade(),
            upcoming_revealer.downgrade(),
            recent.downgrade(),
            recent_revealer.downgrade(),
            context.clone(),
        );
        move |model: &AppModel| {
            if let Some(row) = servers_row.upgrade() {
                row.set_visible(model.offers_relay_choice());
            }
            if let Some(slot) = server_slot.upgrade() {
                slot.set_reveal_child(model.offers_relay_choice());
            }
            let coming = model.upcoming();
            if let Some(badge) = scheduled_badge.upgrade() {
                badge.set_label(&coming.len().to_string());
                badge.set_visible(!coming.is_empty());
            }
            if let (Some(list), Some(revealer)) = (upcoming.upgrade(), upcoming_revealer.upgrade())
            {
                list.remove_all();
                for one in coming.iter().take(5) {
                    list.append(&meeting_row(model, one));
                }
                revealer.set_reveal_child(!coming.is_empty());
            }
            if let (Some(list), Some(revealer)) = (recent.upgrade(), recent_revealer.upgrade()) {
                list.remove_all();
                let recents = model.recent_here();
                for one in recents.iter().take(6) {
                    list.append(&recent_row(&ctx, one));
                }
                revealer.set_reveal_child(!recents.is_empty());
            }
        }
    };
    fill(model);
    model.subscribe_while(&bin, move |model, change| {
        if change == CHANGED {
            show_place(model);
        }
        if matches!(change, Change::Account) || change == lobby::CHANGED || change == app::CHANGED {
            fill(model);
        }
    });

    // The animals hold still while a sheet or the settings cover them.
    let portraits = home.1;
    let hold = {
        let portraits = portraits.downgrade();
        move |window: &adw::ApplicationWindow| {
            if let Some(portraits) = portraits.upgrade() {
                portraits.set_held(window.visible_dialog().is_some());
            }
        }
    };
    if let Some(window) = context.window() {
        hold(&window);
        let handler = std::cell::RefCell::new(Some(window.connect_visible_dialog_notify(hold)));
        let weak = window.downgrade();
        bin.connect_destroy(move |_| {
            if let (Some(window), Some(handler)) = (weak.upgrade(), handler.take()) {
                window.disconnect(handler);
            }
        });
    }

    open_from_debug(context, &bin, false);
    kit::menu_layer(&bin).upcast()
}

/// TOMOSHIBI_DEBUG_LOBBY: a part of the lobby opened at start, once, for a
/// picture (the Mac's LobbyDebug).
pub fn open_from_debug(context: &Context, anchor: &impl IsA<gtk::Widget>, signing_in: bool) {
    #[derive(Default)]
    struct Taken(Cell<bool>);
    let model = &context.model;
    let Some(part) = model.launch().lobby.clone() else {
        return;
    };
    // The sign-in has the language and nothing else of these.
    if signing_in != (part == "language") {
        return;
    }
    if model.kept::<Taken>().0.replace(true) {
        return;
    }
    let ctx = context.clone();
    let anchor = anchor.as_ref().downgrade();
    // A menu once the screen has arrived, so that a picture of its own
    // arrival is not taken over the screen's; the rest as before.
    // `<menu>-close` opens it and closes it a second later, for a picture
    // of it leaving.
    let (part, closes) = match part.strip_suffix("-close") {
        Some(menu) => (menu.to_owned(), true),
        None => (part, false),
    };
    let after = if matches!(part.as_str(), "account" | "language" | "server") {
        1500
    } else {
        400
    };
    glib::timeout_add_local_once(std::time::Duration::from_millis(after), move || {
        let model = &ctx.model;
        match part.as_str() {
            "start" => sheet::present(&ctx, sheet::Mode::Start),
            // Start pressed and the sheet closed at once, as Escape or a
            // press outside closes it: nothing must come of the press.
            "start-escape" => {
                sheet::present(&ctx, sheet::Mode::Start);
                let ctx = ctx.clone();
                glib::timeout_add_local_once(std::time::Duration::from_millis(600), move || {
                    let Some(window) = ctx.window() else {
                        return;
                    };
                    let Some(dialog) = window.visible_dialog() else {
                        return;
                    };
                    let root = dialog.clone().upcast::<gtk::Widget>();
                    if let Some(room) = find(&root, "sheet.room").and_downcast::<gtk::Entry>() {
                        room.set_text("escape-test");
                    }
                    if let Some(go) = find(&root, "sheet.go").and_downcast::<gtk::Button>() {
                        log("pressing Start, then closing the sheet as Escape does");
                        go.emit_clicked();
                    }
                    dialog.close();
                    let model = ctx.model.downgrade();
                    glib::timeout_add_local_once(std::time::Duration::from_millis(4000), move || {
                        if let Some(model) = model.upgrade() {
                            log(&format!("after the sheet closed: {}", model.screen().name()));
                        }
                    });
                });
            }
            "join" => sheet::present(&ctx, sheet::Mode::Join(String::new())),
            "again" => {
                let room = model
                    .recent_here()
                    .first()
                    .map(|recent| recent.room.clone())
                    .unwrap_or_else(|| "team-sync".into());
                sheet::present(&ctx, sheet::Mode::Again(room));
            }
            "scheduled" => go(model, Place::Scheduled),
            #[cfg(debug_assertions)]
            "update" => crate::app::updates::pose_offer(model),
            "servers" => go(model, Place::Servers),
            "account" | "language" | "server" => {
                if let Some(anchor) = anchor.upgrade() {
                    let name = format!("lobby.{part}");
                    let name = name.as_str();
                    if let Some(menu) = find(&anchor, name).and_then(|w| kit::Menu::of(&w)) {
                        menu.popup();
                        if closes {
                            glib::timeout_add_local_once(
                                std::time::Duration::from_millis(1000),
                                move || {
                                    log("closing the menu");
                                    menu.popdown();
                                },
                            );
                        }
                    }
                }
            }
            "settings" => {
                if let Some(window) = ctx.window() {
                    settings::present(&ctx, &window, None);
                }
            }
            other if other.starts_with("settings/") => {
                if let Some(window) = ctx.window() {
                    settings::present(&ctx, &window, Some(&other["settings/".len()..]));
                }
            }
            _ => {}
        }
    });
}

/// The widget named `name` under `root`.
pub fn find(root: &gtk::Widget, name: &str) -> Option<gtk::Widget> {
    if root.widget_name() == name {
        return Some(root.clone());
    }
    let mut child = root.first_child();
    while let Some(next) = child {
        if let Some(found) = find(&next, name) {
            return Some(found);
        }
        child = next.next_sibling();
    }
    None
}

fn place_row(title: &str, symbol: &str, badge: Option<&gtk::Label>) -> gtk::ListBoxRow {
    let row_box = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    let image = icon(symbol, 16);
    image.add_css_class("tmsb-tally-text");
    row_box.append(&image);
    let label = gtk::Label::new(Some(title));
    label.set_xalign(0.0);
    label.set_hexpand(true);
    label.set_ellipsize(gtk::pango::EllipsizeMode::End);
    row_box.append(&label);
    if let Some(badge) = badge {
        row_box.append(badge);
    }
    let row = gtk::ListBoxRow::builder().child(&row_box).build();
    row.add_css_class("tmsb-place");
    row
}

fn section_title(text: &str) -> gtk::Label {
    let label = gtk::Label::new(Some(text));
    label.set_xalign(0.0);
    label.add_css_class("tmsb-section-title");
    label
}

/// An arranged meeting in the sidebar: its room and when; it opens as its
/// link does.
fn meeting_row(model: &AppModel, one: &tomoshibi_protocol::api::Arrangement) -> gtk::ListBoxRow {
    let column = gtk::Box::new(gtk::Orientation::Vertical, 1);
    let room = gtk::Label::new(Some(&one.room));
    room.set_xalign(0.0);
    room.set_ellipsize(gtk::pango::EllipsizeMode::End);
    room.add_css_class("tmsb-row-title");
    room.add_css_class("tmsb-mono");
    let when = gtk::Label::new(Some(&if one.started {
        t("Under way")
    } else {
        words::instant(&one.at)
            .map(words::said)
            .unwrap_or_else(|| one.at.clone())
    }));
    when.set_xalign(0.0);
    when.set_ellipsize(gtk::pango::EllipsizeMode::End);
    when.add_css_class("tmsb-row-detail");
    set_class(&when, "tmsb-tally-text", one.started);
    column.append(&room);
    column.append(&when);
    let button = gtk::Button::builder().child(&column).build();
    button.add_css_class("tmsb-row-button");
    let (weak, one) = (model.downgrade(), one.clone());
    button.connect_clicked(move |_| {
        if let Some(model) = weak.upgrade() {
            model.open_meeting(&one);
        }
    });
    gtk::ListBoxRow::builder()
        .child(&button)
        .activatable(false)
        .build()
}

/// A recent room: the row opens it again (joined if a meeting is live in
/// it, started again under the same name if not); the cross that takes it
/// off the list shows under the pointer, as a sidebar's own rows show what
/// they offer.
fn recent_row(context: &Context, recent: &lobby::Recent) -> gtk::ListBoxRow {
    let row_box = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    row_box.append(&pieces::initial(&recent.room, 26));
    let column = gtk::Box::new(gtk::Orientation::Vertical, 1);
    column.set_hexpand(true);
    let room = gtk::Label::new(Some(&recent.room));
    room.set_xalign(0.0);
    room.set_ellipsize(gtk::pango::EllipsizeMode::End);
    room.add_css_class("tmsb-row-title");
    let when = gtk::Label::new(Some(&words::lately(recent.at_ms / 1000)));
    when.set_xalign(0.0);
    when.add_css_class("tmsb-row-detail");
    column.append(&room);
    column.append(&when);
    row_box.append(&column);
    let open = gtk::Button::builder().child(&row_box).hexpand(true).build();
    open.add_css_class("tmsb-row-button");
    let forget_label = t_with("Remove {room} from recent", &[("room", &recent.room)]);
    let forget = pieces::round_button(&icon("window-close-symbolic", 9), 20, &forget_label);
    forget.add_css_class("tmsb-forget");
    let forget_slot = pieces::fading(&forget, false);
    let line = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    line.append(&open);
    line.append(&forget_slot);
    let hover = gtk::EventControllerMotion::new();
    {
        let slot = forget_slot.downgrade();
        hover.connect_enter(move |_, _, _| {
            if let Some(slot) = slot.upgrade() {
                slot.set_reveal_child(true);
            }
        });
        let slot = forget_slot.downgrade();
        hover.connect_leave(move |_| {
            if let Some(slot) = slot.upgrade() {
                slot.set_reveal_child(false);
            }
        });
    }
    line.add_controller(hover);
    let (ctx, name) = (context.clone(), recent.room.clone());
    open.connect_clicked(move |_| sheet::present(&ctx, sheet::Mode::Again(name.clone())));
    let (weak, recent) = (context.model.downgrade(), recent.clone());
    forget.connect_clicked(move |_| {
        if let Some(model) = weak.upgrade() {
            model.forget_recent(&recent);
        }
    });
    gtk::ListBoxRow::builder()
        .child(&line)
        .activatable(false)
        .build()
}

/// How much of the fleet is answering, in a light and two numbers
/// (Lobby.tsx `Fleet`); nothing where the deployment does not say.
fn fleet_line(model: &AppModel) -> gtk::Revealer {
    let dot = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    dot.set_size_request(6, 6);
    dot.set_valign(gtk::Align::Center);
    dot.add_css_class("tmsb-dot");
    let count = gtk::Label::new(None);
    count.add_css_class("tmsb-numeric");
    let words = gtk::Label::new(Some(&t("Relays answering")));
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    row.add_css_class("tmsb-fleet");
    row.set_margin_start(6);
    row.append(&dot);
    row.append(&count);
    row.append(&words);
    let revealer = pieces::fading(&row, false);
    let fill = {
        let (revealer, dot, count) = (revealer.downgrade(), dot.downgrade(), count.downgrade());
        move |model: &AppModel| {
            let (Some(revealer), Some(dot), Some(count)) =
                (revealer.upgrade(), dot.upgrade(), count.upgrade())
            else {
                return;
            };
            let Some(relays) = model.relays().filter(|relays| relays.total > 0) else {
                revealer.set_reveal_child(false);
                return;
            };
            count.set_label(&format!("{}/{}", relays.online, relays.total));
            set_class(&dot, "none", relays.online == 0);
            set_class(&dot, "all", relays.online == relays.total);
            revealer.set_reveal_child(true);
        }
    };
    fill(model);
    model.subscribe_while(&revealer, move |model, change| {
        if change == app::CHANGED {
            fill(model);
        }
    });
    revealer
}

/// The account, the settings and signing out, as the app's own menu over
/// the sidebar's foot.
fn account_button(context: &Context, name: &str) -> gtk::Button {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    row.append(&pieces::avatar(&context.model, name, 30));
    let label = gtk::Label::new(Some(name));
    label.set_xalign(0.0);
    label.set_hexpand(true);
    label.set_ellipsize(gtk::pango::EllipsizeMode::End);
    label.add_css_class("tmsb-strong");
    row.append(&label);
    row.append(&icon("pan-down-symbolic", 12));
    let button = kit::menu_button(&row);
    button.set_tooltip_text(Some(&t("Settings")));
    button.add_css_class("tmsb-account");
    button.set_widget_name("lobby.account");
    let (menu, column) = pieces::menu_for(&button, 262, kit::MenuSide::Above, kit::MenuAlign::Start);
    let settings_row = menu_item(&t("Settings"), "emblem-system-symbolic");
    let sign_out_row = menu_item(&t("Sign out"), "system-log-out-symbolic");
    sign_out_row.add_css_class("tmsb-destructive");
    column.append(&settings_row);
    column.append(&pieces::menu_rule());
    column.append(&sign_out_row);
    let (ctx, pop) = (context.clone(), menu.downgrade());
    settings_row.connect_clicked(move |_| {
        if let Some(pop) = pop.upgrade() {
            pop.popdown();
        }
        if let Some(window) = ctx.window() {
            settings::present(&ctx, &window, None);
        }
    });
    let (ctx, pop) = (context.clone(), menu.downgrade());
    sign_out_row.connect_clicked(move |_| {
        if let Some(pop) = pop.upgrade() {
            pop.popdown();
        }
        ask_to_sign_out(&ctx);
    });
    button
}

fn menu_item(title: &str, symbol: &str) -> gtk::Button {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    row.append(&icon(symbol, 15));
    let label = gtk::Label::new(Some(title));
    label.set_xalign(0.0);
    label.set_hexpand(true);
    row.append(&label);
    let button = gtk::Button::builder().child(&row).build();
    button.add_css_class("tmsb-menu-row");
    button
}

/// Signing out asks first (SignOutPrompt.cs): coming back is a name and a
/// passphrase away.
pub fn ask_to_sign_out(context: &Context) {
    let dialog = adw::AlertDialog::new(
        Some(&t("Sign out of Tomoshibi?")),
        Some(&t(
            "To come back, you will sign in again with your name and passphrase.",
        )),
    );
    dialog.add_response("cancel", &t("Cancel"));
    dialog.add_response("sign-out", &t("Sign out"));
    dialog.set_response_appearance("sign-out", adw::ResponseAppearance::Destructive);
    dialog.set_default_response(Some("cancel"));
    dialog.set_close_response("cancel");
    let weak = context.model.downgrade();
    dialog.connect_response(None, move |_, response| {
        if response == "sign-out" {
            if let Some(model) = weak.upgrade() {
                model.sign_out();
            }
        }
    });
    if let Some(window) = context.window() {
        dialog.present(Some(&window));
    }
}

/// Home: the picture of yourself, a greeting, and the switches for how you
/// will arrive.
fn home_stage(context: &Context, name: &str) -> (gtk::Overlay, Portraits, kit::Arrival) {
    let stage = stage::stage(context, super::animals::Placement::Stage);
    let overlay = gtk::Overlay::new();
    overlay.set_child(Some(&stage.widget));
    let hello = gtk::Label::builder()
        .label(pieces::greeting(name))
        .xalign(0.0)
        .wrap(true)
        .build();
    hello.add_css_class("tmsb-greeting");
    let sub = gtk::Label::builder()
        .label(t("Start one from the sidebar when you are ready."))
        .xalign(0.0)
        .wrap(true)
        .build();
    sub.add_css_class("tmsb-greeting-sub");
    let words = gtk::Box::new(gtk::Orientation::Vertical, 6);
    words.append(&hello);
    words.append(&sub);
    let words = kit::Arrival::new(&words).rises(0);
    words.set_halign(gtk::Align::Start);
    words.set_valign(gtk::Align::Start);
    words.set_margin_start(36);
    words.set_margin_top(16);
    words.set_margin_end(36);
    overlay.add_overlay(&words);
    let switches = pieces::device_switches(&context.model, 44);
    let switches = kit::Arrival::new(&switches).rises(80);
    switches.set_halign(gtk::Align::Center);
    switches.set_valign(gtk::Align::End);
    switches.set_margin_bottom(28);
    overlay.add_overlay(&switches);
    (overlay, stage.portraits, words)
}
