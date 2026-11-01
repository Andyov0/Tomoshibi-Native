//! Where calls are held, chosen by hand (ServerPicker.tsx; Lobby.swift
//! `ServersView`): whichever answers fastest, or a relay picked from the
//! list under its region, each with its round trip as measured from here.
//! Measured when the section opens, and again on asking; the order the first
//! measurement settled on is kept, so that a row does not move out from
//! under a pointer already reaching for it.

use adw::prelude::*;

use super::pieces::{icon, set_class};
use crate::{
    app::{self, lobby, AppModel},
    i18n::t,
    ui::{kit, Context},
};

pub fn section(context: &Context) -> gtk::Widget {
    let model = &context.model;
    model.measure_relays();
    let title = gtk::Label::new(Some(&t("Server")));
    title.set_xalign(0.0);
    title.add_css_class("tmsb-section-heading");
    let again = kit::plain_button(&t("Measure again"));
    again.set_height_request(30);
    again.add_css_class("tmsb-compact");
    again.set_valign(gtk::Align::Center);
    let head = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    title.set_hexpand(true);
    head.append(&title);
    head.append(&again);

    let list = gtk::Box::new(gtk::Orientation::Vertical, 8);
    list.set_widget_name("servers.list");
    let page = gtk::Box::new(gtk::Orientation::Vertical, 18);
    page.set_margin_top(16);
    page.set_margin_bottom(28);
    page.set_margin_start(28);
    page.set_margin_end(28);
    page.append(&kit::Arrival::new(&head).rises(0));
    page.append(&kit::Arrival::new(&list).rises(40));
    let clamp = adw::Clamp::builder().maximum_size(640).child(&page).build();
    let scroller = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .child(&clamp)
        .vexpand(true)
        .build();

    let weak = model.downgrade();
    again.connect_clicked(move |_| {
        if let Some(model) = weak.upgrade() {
            model.measure_relays();
        }
    });
    let fill = {
        let (list, again) = (list.downgrade(), again.downgrade());
        move |model: &AppModel| {
            let (Some(list), Some(again)) = (list.upgrade(), again.upgrade()) else {
                return;
            };
            let measuring = model.lobby().measuring.get();
            again.set_sensitive(!measuring);
            again.set_label(&if measuring { t("Measuring…") } else { t("Measure again") });
            while let Some(child) = list.first_child() {
                list.remove(&child);
            }
            let fastest = model.fastest_relay();
            list.append(&row(
                model,
                None,
                &t("Automatic"),
                &match &fastest {
                    Some((name, millis)) => format!("{} · {}", model.relay_label(name), ms(*millis)),
                    None => t("Whichever answers fastest"),
                },
                model.relay_choice().is_none(),
                true,
            ));
            let measured = model.lobby().measured.borrow().clone();
            for (region, relays) in model.relays_by_region() {
                if !region.is_empty() {
                    let label = gtk::Label::new(Some(&region));
                    label.set_xalign(0.0);
                    label.add_css_class("tmsb-section-title");
                    list.append(&label);
                }
                for relay in relays {
                    let detail = if relay.maintenance {
                        t("Not taking calls")
                    } else {
                        match measured.as_ref().map(|measured| measured.get(&relay.name).copied().flatten()) {
                            Some(Some(millis)) => ms(millis),
                            Some(None) => t("Timeout"),
                            None if measuring => t("Measuring…"),
                            None => String::new(),
                        }
                    };
                    list.append(&row(
                        model,
                        Some(&relay.name),
                        &model.relay_label(&relay.name),
                        &detail,
                        model.relay_choice().as_deref() == Some(relay.name.as_str()),
                        !relay.maintenance,
                    ));
                }
            }
        }
    };
    fill(model);
    model.subscribe_while(&scroller, move |model, change| {
        if change == app::CHANGED || change == lobby::CHANGED {
            fill(model);
        }
    });
    scroller.upcast()
}

fn ms(millis: f64) -> String {
    format!("{} ms", millis.round() as i64)
}

fn row(
    model: &AppModel,
    name: Option<&str>,
    title: &str,
    detail: &str,
    chosen: bool,
    usable: bool,
) -> gtk::Button {
    let line = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    let symbol = icon(
        if name.is_none() {
            "emblem-synchronizing-symbolic"
        } else {
            "network-server-symbolic"
        },
        15,
    );
    symbol.add_css_class("tmsb-muted");
    line.append(&symbol);
    let words = gtk::Box::new(gtk::Orientation::Vertical, 2);
    words.set_hexpand(true);
    let title = gtk::Label::new(Some(title));
    title.set_xalign(0.0);
    title.add_css_class("tmsb-strong");
    words.append(&title);
    if !detail.is_empty() {
        let detail = gtk::Label::new(Some(detail));
        detail.set_xalign(0.0);
        detail.add_css_class("tmsb-muted");
        detail.add_css_class("tmsb-small");
        detail.add_css_class("tmsb-numeric");
        words.append(&detail);
    }
    line.append(&words);
    let check = icon("object-select-symbolic", 15);
    check.add_css_class("tmsb-tally-text");
    check.set_opacity(if chosen { 1.0 } else { 0.0 });
    line.append(&check);
    let button = gtk::Button::builder().child(&line).sensitive(usable).build();
    button.add_css_class("tmsb-choice-row");
    set_class(&button, "chosen", chosen);
    let (weak, name) = (model.downgrade(), name.map(str::to_owned));
    button.connect_clicked(move |_| {
        if let Some(model) = weak.upgrade() {
            model.set_relay_choice(name.clone());
        }
    });
    button
}
