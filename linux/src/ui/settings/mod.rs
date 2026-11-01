//! The settings: every page the Mac and Android have (general, audio and
//! video, watch together, music, the account, about), in a libadwaita
//! preferences dialog over the main window, the platform's own idiom for
//! them.
//!
//! Owned by the settings module. Opened with [`present`] from wherever a
//! screen offers it (the lobby's account menu, the room's More menu); reads
//! and writes through the model's preferences under the web's key names,
//! and lists devices through the context's `Media`. A change of language
//! words the open dialog again in place, on the page it was on.

mod about;
mod account;
mod calls;
mod general;
mod media;

use std::cell::RefCell;

use adw::prelude::*;

use super::Context;
use crate::{i18n::t, theme};

/// The settings' pages by name, for `TOMOSHIBI_SHOW=settings/<page>` and
/// `TOMOSHIBI_DEBUG_LOBBY=settings/<page>`.
pub const PAGES: [&str; 6] = ["general", "media", "watching", "music", "account", "about"];

thread_local! {
    /// The dialog open now, worded again when the language changes.
    static OPEN: RefCell<Option<(glib::WeakRef<adw::PreferencesDialog>, Context)>> = const { RefCell::new(None) };
    static LISTENING: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Opens the settings over `parent`, on `page` when one is named.
pub fn present(context: &Context, parent: &impl IsA<gtk::Widget>, page: Option<&str>) {
    theme::add_module_css("settings", CSS);
    let dialog = adw::PreferencesDialog::new();
    dialog.add_css_class("tmsb-settings");
    dialog.set_search_enabled(false);
    dialog.set_content_width(720);
    dialog.set_content_height(640);
    fill(&dialog, context);
    if let Some(page) = page.filter(|page| PAGES.contains(page)) {
        dialog.set_visible_page_name(page);
    }
    OPEN.with(|open| {
        let weak = glib::WeakRef::new();
        weak.set(Some(&dialog));
        open.replace(Some((weak, context.clone())));
    });
    if !LISTENING.with(|listening| listening.replace(true)) {
        crate::i18n::on_change(|_| {
            OPEN.with(|open| {
                let Some((weak, context)) = open.borrow().clone() else {
                    return;
                };
                if let Some(dialog) = weak.upgrade() {
                    let page = dialog.visible_page_name();
                    fill(&dialog, &context);
                    if let Some(page) = page {
                        dialog.set_visible_page_name(&page);
                    }
                }
            });
        });
    }
    dialog.present(Some(parent));
}

/// The pages, in the language on screen.
fn fill(dialog: &adw::PreferencesDialog, context: &Context) {
    dialog.set_title(&t("Settings"));
    // A dialog has no way to list its pages, so the ones added are kept on
    // it and taken off before it is filled again.
    // SAFETY: the key is only ever set below, with this type.
    let kept: Vec<adw::PreferencesPage> = unsafe {
        dialog
            .steal_data::<Vec<adw::PreferencesPage>>("tmsb-pages")
            .unwrap_or_default()
    };
    for page in &kept {
        dialog.remove(page);
    }
    let pages = vec![
        general::page(context),
        media::page(context),
        calls::watching(context),
        calls::music(context),
        account::page(context),
        about::page(context),
    ];
    for page in &pages {
        dialog.add(page);
    }
    // SAFETY: read back above with the same type.
    unsafe {
        dialog.set_data("tmsb-pages", pages);
    }
}

/// A page with its name, title and symbol.
pub fn new_page(name: &str, title: &str, symbol: &str, description: &str) -> adw::PreferencesPage {
    let page = adw::PreferencesPage::builder()
        .name(name)
        .title(title)
        .icon_name(symbol)
        .build();
    page.set_description(description);
    page
}

/// A row with words, a detail under them and a control at its end.
pub fn row(title: &str, subtitle: Option<&str>, control: &impl IsA<gtk::Widget>) -> adw::ActionRow {
    let row = adw::ActionRow::builder().title(title).build();
    if let Some(subtitle) = subtitle {
        row.set_subtitle(subtitle);
    }
    control.as_ref().set_valign(gtk::Align::Center);
    row.add_suffix(control);
    row
}

/// The settings' own rules.
const CSS: &str = r#"
.tmsb-settings .tmsb-swatch { min-width: 28px; min-height: 28px; padding: 0; border-radius: 999px; border: none; box-shadow: none; background: none; }
.tmsb-settings .tmsb-chip {
  font-size: 12px; padding: 3px 10px; border-radius: 999px;
  background-color: alpha(@tmsb_fg, 0.07); color: @tmsb_fg;
}
.tmsb-settings .tmsb-chip.withheld { color: @tmsb_muted; background-color: alpha(@tmsb_fg, 0.04); }
.tmsb-settings .tmsb-chip.lit { background-color: @tmsb_tally; color: @tmsb_on_tally; }
.tmsb-settings .tmsb-profile-name { font-size: 19px; font-weight: 600; }
.tmsb-settings .tmsb-about-name { font-size: 26px; font-weight: 700; }
.tmsb-settings .tmsb-mono { font-family: monospace; }
.tmsb-settings .tmsb-check {
  background-color: @tmsb_surface; border-radius: 20px; border: 1px solid alpha(@tmsb_wash, 0.08);
}
.tmsb-settings .tmsb-hearing {
  min-height: 32px; padding: 0 12px; border-radius: 999px;
  background-color: alpha(@tmsb_bg, 0.86); color: @tmsb_fg; font-size: 12px; font-weight: 500;
}
.tmsb-settings .tmsb-meter > box {
  border-radius: 2px; background-color: alpha(@tmsb_fg, 0.12);
  transition: background-color 80ms ease-out;
}
.tmsb-settings .tmsb-meter > box.lit { background-color: @tmsb_tally; }
"#;
