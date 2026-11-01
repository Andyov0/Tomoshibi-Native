//! A newer build, offered where it covers nothing: the sidebar's foot above
//! the account, and the sign-in's head under the deployment's notice
//! (UpdateBanner.swift, UpdateBanner.cs). It says what is on offer, then how
//! far the download has got, then that the app will open again; and it goes
//! once it has nothing to say.

use adw::prelude::*;

use super::pieces::set_class;
use crate::{
    app::{
        updates::{self, Step},
        words, AppModel, Change,
    },
    i18n::{t, t_with},
    theme::motion,
    ui::kit,
};

/// The offer, in a revealer that opens and closes it.
pub fn offer(model: &AppModel) -> gtk::Revealer {
    let title = gtk::Label::builder().wrap(true).xalign(0.0).build();
    title.add_css_class("tmsb-strong");
    let detail = gtk::Label::builder().wrap(true).xalign(0.0).build();
    detail.add_css_class("tmsb-muted");
    detail.add_css_class("tmsb-small");
    let bar = gtk::ProgressBar::new();
    bar.add_css_class("tmsb-progress");
    let later = kit::plain_button(&t("Later"));
    later.set_height_request(30);
    later.add_css_class("tmsb-compact");
    let update = kit::primary_button(&t("Update"));
    update.set_height_request(30);
    update.add_css_class("tmsb-compact");
    let cancel = kit::plain_button(&t("Cancel"));
    cancel.set_height_request(30);
    cancel.add_css_class("tmsb-compact");
    let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    buttons.set_halign(gtk::Align::End);
    buttons.append(&later);
    buttons.append(&cancel);
    buttons.append(&update);

    let card = gtk::Box::new(gtk::Orientation::Vertical, 8);
    card.add_css_class("tmsb-offer");
    card.set_widget_name("lobby.update");
    card.append(&title);
    card.append(&detail);
    card.append(&bar);
    card.append(&buttons);
    let revealer = gtk::Revealer::builder()
        .transition_type(gtk::RevealerTransitionType::SlideUp)
        .transition_duration(motion::SETTLE_MS)
        .child(&card)
        .build();

    let fill = {
        let (revealer, title, detail, bar, later, update, cancel) = (
            revealer.downgrade(),
            title.downgrade(),
            detail.downgrade(),
            bar.downgrade(),
            later.downgrade(),
            update.downgrade(),
            cancel.downgrade(),
        );
        move |model: &AppModel| {
            let (
                Some(revealer),
                Some(title),
                Some(detail),
                Some(bar),
                Some(later),
                Some(update),
                Some(cancel),
            ) = (
                revealer.upgrade(),
                title.upgrade(),
                detail.upgrade(),
                bar.upgrade(),
                later.upgrade(),
                update.upgrade(),
                cancel.upgrade(),
            )
            else {
                return;
            };
            let shown = updates::shown(model);
            revealer.set_reveal_child(shown);
            if !shown {
                return;
            }
            let step = model.kept::<updates::UpdateState>().step.borrow().clone();
            let label = |build: &tomoshibi_protocol::api::AppBuild| {
                tomoshibi_protocol::api::version_label(build.version.clone())
            };
            match step {
                Step::None => {}
                Step::Offered { build, problem } => {
                    title.set_label(&t_with(
                        "Tomoshibi {version} is available",
                        &[("version", &label(&build))],
                    ));
                    let notes = tomoshibi_protocol::api::update_notes(
                        build.clone(),
                        crate::i18n::language().code().into(),
                    );
                    let said = problem.clone().or(notes);
                    detail.set_visible(said.is_some());
                    detail.set_label(said.as_deref().unwrap_or_default());
                    set_class(&detail, "tmsb-danger-text", problem.is_some());
                    bar.set_visible(false);
                    later.set_visible(true);
                    update.set_visible(true);
                    cancel.set_visible(false);
                }
                Step::Downloading {
                    build,
                    received,
                    total,
                } => {
                    title.set_label(&t_with(
                        "Downloading Tomoshibi {version}",
                        &[("version", &label(&build))],
                    ));
                    detail.set_visible(true);
                    detail.set_label(&t_with(
                        "{received} of {total}",
                        &[
                            ("received", &words::megabytes(received)),
                            ("total", &words::megabytes(total)),
                        ],
                    ));
                    set_class(&detail, "tmsb-danger-text", false);
                    bar.set_visible(true);
                    bar.set_fraction(if total > 0 {
                        received as f64 / total as f64
                    } else {
                        0.0
                    });
                    later.set_visible(false);
                    update.set_visible(false);
                    cancel.set_visible(true);
                }
                Step::Installing { build } => {
                    title.set_label(&t_with(
                        "Installing Tomoshibi {version}",
                        &[("version", &label(&build))],
                    ));
                    detail.set_visible(true);
                    detail.set_label(&t("Tomoshibi will open again when it is done."));
                    set_class(&detail, "tmsb-danger-text", false);
                    bar.set_visible(false);
                    later.set_visible(false);
                    update.set_visible(false);
                    cancel.set_visible(false);
                }
            }
        }
    };
    fill(model);
    let weak = model.downgrade();
    later.connect_clicked(move |_| {
        if let Some(model) = weak.upgrade() {
            updates::later(&model);
        }
    });
    let weak = model.downgrade();
    update.connect_clicked(move |_| {
        if let Some(model) = weak.upgrade() {
            updates::accept(&model);
        }
    });
    let weak = model.downgrade();
    cancel.connect_clicked(move |_| {
        if let Some(model) = weak.upgrade() {
            updates::cancel(&model);
        }
    });
    model.subscribe_while(&revealer, move |model, change| {
        if matches!(change, Change::Update | Change::Screen | Change::Remote) {
            fill(model);
        }
    });
    revealer
}
