//! About: the app's version, its updates, the media core, the server's build
//! and where the code running there can be read (Settings.swift
//! `AboutSettings`, About.kt). The source is an obligation under the AGPL's
//! thirteenth section, which only a page somebody uses can discharge; its
//! words are the licence's name, not translated, as on the web.

use adw::prelude::*;

use super::new_page;
use crate::{
    app::{
        client,
        updates::{self, Said, Step},
        words, AppModel, Change,
    },
    i18n::{t, t_with},
    ui::{kit, Context},
};

pub fn page(context: &Context) -> adw::PreferencesPage {
    let model = &context.model;
    let page = new_page(
        "about",
        &t("About"),
        "help-about-symbolic",
        &t("The app, and where its code can be read."),
    );

    let head = gtk::Box::new(gtk::Orientation::Horizontal, 18);
    head.set_margin_bottom(6);
    head.append(&kit::mark(76));
    let names = gtk::Box::new(gtk::Orientation::Vertical, 4);
    names.set_valign(gtk::Align::Center);
    let name = gtk::Label::new(Some("Tomoshibi"));
    name.set_xalign(0.0);
    name.add_css_class("tmsb-about-name");
    let version = gtk::Label::new(Some(&client::version_label()));
    version.set_xalign(0.0);
    version.add_css_class("tmsb-mono");
    version.add_css_class("dim-label");
    names.append(&name);
    names.append(&version);
    head.append(&names);
    let top = adw::PreferencesGroup::new();
    top.add(&head);
    page.add(&top);

    let about = adw::PreferencesGroup::builder().title(t("About")).build();
    about.add(&fact(&t("Version"), &client::version_label()));
    let update_row = update_row(model);
    about.add(&update_row);
    about.add(&fact(&t("Media core"), &tomoshibi_core::core_version()));
    if let Some(info) = model.info() {
        // The server's build and not its address: which server this is is
        // not something the app says anywhere.
        let build = if info.build.is_empty() {
            "—".to_owned()
        } else {
            info.build.chars().take(12).collect()
        };
        about.add(&fact(&t("Server"), &build));
        if !info.source.is_empty() {
            let source = adw::ActionRow::builder()
                .title("AGPL-3.0 · source")
                .subtitle(&info.source)
                .activatable(true)
                .build();
            source.add_suffix(&gtk::Image::from_icon_name("adw-external-link-symbolic"));
            let (weak, uri) = (model.downgrade(), info.source.clone());
            source.connect_activated(move |_| {
                if let Some(model) = weak.upgrade() {
                    model.open_in_browser(&uri);
                }
            });
            about.add(&source);
        }
    }
    page.add(&about);
    page
}

fn fact(title: &str, value: &str) -> adw::ActionRow {
    let row = adw::ActionRow::builder().title(title).build();
    let value = gtk::Label::new(Some(value));
    value.add_css_class("tmsb-mono");
    value.add_css_class("dim-label");
    value.set_selectable(true);
    row.add_suffix(&value);
    row
}

/// Where the app's updates stand, and the check somebody can ask for: the
/// version on offer with Update, a download's progress with Cancel, and the
/// answer to a check, which says so even when there is nothing newer. Only
/// where the app looks for updates at all (`updates::speaks`).
fn update_row(model: &AppModel) -> adw::ActionRow {
    let row = adw::ActionRow::builder().title(t("Updates")).build();
    row.set_widget_name("settings.updates");
    let button = gtk::Button::new();
    button.set_valign(gtk::Align::Center);
    row.add_suffix(&button);
    let fill = {
        let (row, button) = (row.downgrade(), button.downgrade());
        move |model: &AppModel| {
            let (Some(row), Some(button)) = (row.upgrade(), button.upgrade()) else {
                return;
            };
            row.set_visible(updates::speaks(model));
            let state = model.kept::<updates::UpdateState>();
            let step = state.step.borrow().clone();
            let label = |build: &tomoshibi_protocol::api::AppBuild| {
                tomoshibi_protocol::api::version_label(build.version.clone())
            };
            let (said, action) = match step {
                Step::Downloading { received, total, .. } => (
                    t_with(
                        "{received} of {total}",
                        &[
                            ("received", &words::megabytes(received)),
                            ("total", &words::megabytes(total)),
                        ],
                    ),
                    Some(t("Cancel")),
                ),
                Step::Installing { build } => (
                    t_with("Installing Tomoshibi {version}", &[("version", &label(&build))]),
                    None,
                ),
                Step::Offered { build, problem } => (
                    problem.unwrap_or_else(|| {
                        t_with("Version {version} is available.", &[("version", &label(&build))])
                    }),
                    Some(t("Update")),
                ),
                Step::None => (
                    match state.said.get() {
                        Some(Said::Checking) => t("Checking for updates…"),
                        Some(Said::Current) => t("You have the latest version."),
                        Some(Said::Failed) => t("Could not check for updates. Try again later."),
                        None => String::new(),
                    },
                    Some(t("Check for updates")),
                ),
            };
            row.set_subtitle(&said);
            match action {
                Some(words) => {
                    button.set_label(&words);
                    button.set_visible(true);
                    button.set_sensitive(state.said.get() != Some(Said::Checking));
                }
                None => button.set_visible(false),
            }
        }
    };
    fill(model);
    let weak = model.downgrade();
    button.connect_clicked(move |_| {
        let Some(model) = weak.upgrade() else {
            return;
        };
        let step = model.kept::<updates::UpdateState>().step.borrow().clone();
        match step {
            Step::Downloading { .. } => updates::cancel(&model),
            Step::Offered { .. } => updates::accept(&model),
            _ => updates::check_now(&model),
        }
    });
    model.subscribe_while(&row, move |model, change| {
        if matches!(change, Change::Update | Change::Remote) {
            fill(model);
        }
    });
    row
}
