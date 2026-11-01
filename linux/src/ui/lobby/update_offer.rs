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
    // Why the button downloads rather than updates, where the app does not
    // run from an AppImage and cannot replace itself (update::offer).
    let why = gtk::Label::builder().wrap(true).xalign(0.0).build();
    why.add_css_class("tmsb-muted");
    why.add_css_class("tmsb-small");
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
    // What can be pressed at each step, as pages that crossfade and change
    // size smoothly from one to the next (the offer's Later and Update, the
    // download's Cancel, nothing while it installs); the lines under the
    // title and the progress unfold and fold. They were shown and hidden in
    // one frame each.
    let offered = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    offered.set_halign(gtk::Align::End);
    offered.append(&later);
    offered.append(&update);
    let downloading = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    downloading.set_halign(gtk::Align::End);
    downloading.append(&cancel);
    let buttons = gtk::Stack::builder()
        .transition_type(gtk::StackTransitionType::Crossfade)
        .transition_duration(motion::SETTLE_MS)
        .interpolate_size(true)
        .hhomogeneous(false)
        .vhomogeneous(false)
        .build();
    buttons.add_named(&offered, Some("offered"));
    buttons.add_named(&downloading, Some("downloading"));
    buttons.add_named(
        &gtk::Box::new(gtk::Orientation::Horizontal, 0),
        Some("none"),
    );
    let detail_slot = super::pieces::unfolding(&detail, false);
    let why_slot = super::pieces::unfolding(&why, false);
    let bar_slot = super::pieces::unfolding(&bar, false);

    let card = gtk::Box::new(gtk::Orientation::Vertical, 0);
    card.add_css_class("tmsb-offer");
    card.set_widget_name("lobby.update");
    // The gaps are the lines' own, so that a folded line leaves none.
    title.set_margin_bottom(2);
    for line in [&detail, &why] {
        line.set_margin_top(6);
    }
    bar.set_margin_top(8);
    buttons.set_margin_top(8);
    card.append(&title);
    card.append(&detail_slot);
    card.append(&why_slot);
    card.append(&bar_slot);
    card.append(&buttons);
    let revealer = gtk::Revealer::builder()
        .transition_type(gtk::RevealerTransitionType::SlideUp)
        .transition_duration(motion::SETTLE_MS)
        .child(&card)
        .build();

    let fill = {
        let (revealer, title, detail, why, bar, update, buttons, detail_slot, why_slot, bar_slot) = (
            revealer.downgrade(),
            title.downgrade(),
            detail.downgrade(),
            why.downgrade(),
            bar.downgrade(),
            update.downgrade(),
            buttons.downgrade(),
            detail_slot.downgrade(),
            why_slot.downgrade(),
            bar_slot.downgrade(),
        );
        move |model: &AppModel| {
            let (
                Some(revealer),
                Some(title),
                Some(detail),
                Some(why),
                Some(bar),
                Some(update),
                Some(buttons),
                Some(detail_slot),
                Some(why_slot),
                Some(bar_slot),
            ) = (
                revealer.upgrade(),
                title.upgrade(),
                detail.upgrade(),
                why.upgrade(),
                bar.upgrade(),
                update.upgrade(),
                buttons.upgrade(),
                detail_slot.upgrade(),
                why_slot.upgrade(),
                bar_slot.upgrade(),
            )
            else {
                return;
            };
            let shown = updates::shown(model);
            revealer.set_reveal_child(shown);
            if !shown {
                return;
            }
            let step = updates::step(model);
            let label = updates::version_label;
            let not_installable = updates::why_not_installable();
            why_slot.set_reveal_child(
                matches!(step, Step::Offered { .. }) && not_installable.is_some(),
            );
            if let Some(said) = not_installable.as_deref() {
                why.set_label(said);
            }
            update.set_label(&if not_installable.is_some() {
                t("Download")
            } else {
                t("Update")
            });
            match step {
                // A held download is not shown (`shown`): it is held
                // because the person went towards a call.
                Step::None | Step::Held { .. } => {}
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
                    detail_slot.set_reveal_child(said.is_some());
                    if let Some(said) = said.as_deref() {
                        detail.set_label(said);
                    }
                    set_class(&detail, "tmsb-danger-text", problem.is_some());
                    bar_slot.set_reveal_child(false);
                    buttons.set_visible_child_name("offered");
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
                    detail_slot.set_reveal_child(true);
                    detail.set_label(&t_with(
                        "{received} of {total}",
                        &[
                            ("received", &words::megabytes(received)),
                            ("total", &words::megabytes(total)),
                        ],
                    ));
                    set_class(&detail, "tmsb-danger-text", false);
                    bar_slot.set_reveal_child(true);
                    bar.set_fraction(if total > 0 {
                        received as f64 / total as f64
                    } else {
                        0.0
                    });
                    buttons.set_visible_child_name("downloading");
                }
                Step::Installing { build } => {
                    title.set_label(&t_with(
                        "Installing Tomoshibi {version}",
                        &[("version", &label(&build))],
                    ));
                    detail_slot.set_reveal_child(true);
                    detail.set_label(&t("Tomoshibi will open again when it is done."));
                    set_class(&detail, "tmsb-danger-text", false);
                    bar_slot.set_reveal_child(false);
                    buttons.set_visible_child_name("none");
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
