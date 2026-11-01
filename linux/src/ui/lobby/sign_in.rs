//! Signing in: an account's name and passphrase, or the link somebody was
//! sent, as Windows lays it out on a wide window (UI/Screens/SignIn.cs) and
//! the Mac does: the mark and the heading, then one pane with the two
//! fields, the primary button, and the way in with a link under a divider.
//!
//! Above the mark, in the page's flow and never one over the other, the
//! deployment's notice (which is for whoever has the app, signed in or not)
//! and under it a newer build; the language is in the bar's corner, as on
//! every web page before a call.

use adw::prelude::*;

use super::{pieces, update_offer};
use crate::{
    app::{pasted_link, AppModel, Change},
    i18n::t,
    theme::scale,
    ui::{kit, Context},
};

/// The column is no wider than this on any window (SignIn.cs `MaxWidth`).
const COLUMN_WIDTH: i32 = 420;

pub fn page(context: &Context) -> gtk::Widget {
    let model = &context.model;

    let mark = kit::mark(76);
    let heading = gtk::Box::new(gtk::Orientation::Vertical, 12);
    heading.append(&mark);
    heading.append(&kit::title(&t("Sign in")));
    heading.append(&kit::muted(&t(
        "Meetings here are started by the people who belong here. If somebody sent you a link, open that instead.",
    )));

    // The two fields share one rounded box with a hairline between them, as
    // Shared.IdentityBox draws them.
    let name = gtk::Entry::builder()
        .placeholder_text(t("Name"))
        .primary_icon_name("avatar-default-symbolic")
        .input_purpose(gtk::InputPurpose::Name)
        .build();
    name.add_css_class("tmsb-field");
    name.update_property(&[gtk::accessible::Property::Label(&t("Name"))]);
    let passphrase = gtk::PasswordEntry::builder()
        .placeholder_text(t("Passphrase"))
        .show_peek_icon(true)
        .build();
    passphrase.add_css_class("tmsb-field");
    passphrase.update_property(&[gtk::accessible::Property::Label(&t("Passphrase"))]);
    let fields = gtk::Box::new(gtk::Orientation::Vertical, 0);
    fields.add_css_class("tmsb-fields");
    fields.append(&name);
    fields.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
    fields.append(&passphrase);

    let go = kit::primary_button(&t("Sign in"));
    go.set_widget_name("signin.go");

    let divider = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    divider.append(&kit::divider());
    let asked = gtk::Label::new(Some(&t("Were you sent a link?")));
    asked.add_css_class("tmsb-muted");
    asked.add_css_class("tmsb-small");
    divider.append(&asked);
    divider.append(&kit::divider());

    let link = gtk::Entry::builder()
        .placeholder_text(t("Paste the link you were sent."))
        .primary_icon_name("insert-link-symbolic")
        .hexpand(true)
        .build();
    link.add_css_class("tmsb-link-field");
    link.update_property(&[gtk::accessible::Property::Label(&t(
        "Paste the link you were sent.",
    ))]);
    let open = kit::plain_button(&t("Join"));
    let link_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    link_row.append(&link);
    link_row.append(&open);

    let form = gtk::Box::new(gtk::Orientation::Vertical, 14);
    form.append(&fields);
    form.append(&go);
    form.append(&divider);
    form.append(&link_row);
    let pane = gtk::Box::new(gtk::Orientation::Vertical, 0);
    pane.add_css_class("tmsb-pane");
    pane.append(&form);

    let column = gtk::Box::new(gtk::Orientation::Vertical, 22);
    column.append(&kit::Arrival::new(&heading).rises(0));
    column.append(&kit::Arrival::new(&pane).rises(60));
    let clamp = adw::Clamp::builder()
        .maximum_size(COLUMN_WIDTH)
        .tightening_threshold(COLUMN_WIDTH)
        .child(&column)
        .margin_top(scale::SPACING[10] + scale::SPACING[9])
        .margin_bottom(scale::SPACING[10])
        .margin_start(scale::SPACING[7])
        .margin_end(scale::SPACING[7])
        .build();
    // The notice and a newer build, as wide as each other and no wider
    // than the column's head.
    let head = gtk::Box::new(gtk::Orientation::Vertical, 10);
    head.append(&pieces::notice_card(model));
    head.append(&update_offer::offer(model));
    let head = adw::Clamp::builder()
        .maximum_size(520)
        .child(&head)
        .margin_start(scale::SPACING[7])
        .margin_end(scale::SPACING[7])
        .build();
    let flow = gtk::Box::new(gtk::Orientation::Vertical, 0);
    flow.append(&head);
    flow.append(&clamp);
    let scroller = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vexpand(true)
        .child(&flow)
        .build();

    let body = gtk::Box::new(gtk::Orientation::Vertical, 0);
    body.append(&scroller);
    #[cfg(debug_assertions)]
    body.append(&footnote(model));

    // Something that went wrong arrives at the foot and leaves by itself,
    // rather than a line that appears and vanishes.
    let toasts = pieces::with_toast(model, &body);
    let bar = kit::bar();
    bar.pack_end(&pieces::language_button(model));
    let page = adw::ToolbarView::new();
    page.add_top_bar(&bar);
    page.set_content(Some(&toasts));

    // ---- what it does ---------------------------------------------------

    let update = {
        let (go, open, name, passphrase, link) = (
            go.downgrade(),
            open.downgrade(),
            name.downgrade(),
            passphrase.downgrade(),
            link.downgrade(),
        );
        move |model: &AppModel| {
            let (Some(go), Some(open), Some(name), Some(passphrase), Some(link)) = (
                go.upgrade(),
                open.upgrade(),
                name.upgrade(),
                passphrase.upgrade(),
                link.upgrade(),
            ) else {
                return;
            };
            let filled = !name.text().trim().is_empty() && !passphrase.text().is_empty();
            go.set_sensitive(filled && !model.busy() && model.deployment().is_some());
            open.set_sensitive(pasted_link(&link.text()).is_some());
        }
    };
    update(model);
    for entry in [
        name.upcast_ref::<gtk::Editable>(),
        passphrase.upcast_ref(),
        link.upcast_ref(),
    ] {
        let (update, model) = (update.clone(), model.downgrade());
        entry.connect_changed(move |_| {
            if let Some(model) = model.upgrade() {
                update(&model);
            }
        });
    }

    let sign_in = {
        let (model, name, passphrase) =
            (model.downgrade(), name.downgrade(), passphrase.downgrade());
        move || {
            let (Some(model), Some(name), Some(passphrase)) =
                (model.upgrade(), name.upgrade(), passphrase.upgrade())
            else {
                return;
            };
            model.sign_in(name.text().to_string(), passphrase.text().to_string());
        }
    };
    go.connect_clicked({
        let sign_in = sign_in.clone();
        move |_| sign_in()
    });
    passphrase.connect_activate({
        let sign_in = sign_in.clone();
        move |_| sign_in()
    });
    name.connect_activate({
        let passphrase = passphrase.downgrade();
        move |_| {
            if let Some(passphrase) = passphrase.upgrade() {
                passphrase.grab_focus();
            }
        }
    });

    let open_link = {
        let (model, link) = (model.downgrade(), link.downgrade());
        move || {
            let (Some(model), Some(link)) = (model.upgrade(), link.upgrade()) else {
                return;
            };
            if let Some(pasted) = pasted_link(&link.text()) {
                model.open_link(&pasted);
            }
        }
    };
    open.connect_clicked({
        let open_link = open_link.clone();
        move |_| open_link()
    });
    link.connect_activate(move |_| open_link());

    let passphrase_kept = passphrase.downgrade();
    model.subscribe_while(&page, move |model, change| match change {
        Change::Busy | Change::Info => update(model),
        // A refused passphrase is cleared, as SignIn.cs clears it, so that
        // the next attempt is typed rather than resent.
        Change::Problem if model.problem().is_some() && model.account().is_none() => {
            if let Some(passphrase) = passphrase_kept.upgrade() {
                passphrase.set_text("");
            }
        }
        _ => {}
    });

    super::home::open_from_debug(context, &page, true);
    page.upcast()
}

/// In a debug build, what the deployment said about itself, under the
/// pane: the proof that the core reached it, and which one it reached.
#[cfg(debug_assertions)]
fn footnote(model: &AppModel) -> gtk::Label {
    // Wrapped: one line of it was wider than a narrow window, and a window
    // whose content is wider than the compositor allows is a protocol error
    // that ends the app (seen in a 400-wide kiosk output).
    let label = gtk::Label::builder()
        .margin_bottom(scale::SPACING[5])
        .margin_start(scale::SPACING[7])
        .margin_end(scale::SPACING[7])
        .wrap(true)
        .wrap_mode(gtk::pango::WrapMode::WordChar)
        .justify(gtk::Justification::Center)
        .selectable(true)
        .build();
    label.add_css_class("tmsb-footnote");
    let fill = {
        let label = label.downgrade();
        move |model: &AppModel| {
            let Some(label) = label.upgrade() else {
                return;
            };
            let base = model
                .deployment()
                .map(|deployment| deployment.base_url())
                .unwrap_or_default();
            let said = match model.info() {
                Some(info) => format!(
                    "{base}  server {}  opened by {:?}  joined by {:?}",
                    if info.build.is_empty() {
                        "unknown"
                    } else {
                        info.build.as_str()
                    },
                    info.opened_by,
                    info.joined_by
                ),
                None => format!("{base}  not answered"),
            };
            label.set_label(&said);
        }
    };
    fill(model);
    model.subscribe_while(&label, move |model, change| {
        if change == Change::Info {
            fill(model);
        }
    });
    label
}
