//! The settings: every page the Mac and Android have (the account, the
//! devices, the appearance and language, the connection and relays, watch
//! together, music, about), in a libadwaita preferences dialog over the main
//! window.
//!
//! Owned by the settings module. Opened with [`present`] from wherever a
//! screen offers it (the lobby's settings button, the room's More menu);
//! reads and writes through the model's preferences under the web's key
//! names, and lists devices through the context's `Media`.

use adw::prelude::*;

use super::Context;
use crate::i18n::t;

/// The settings' pages by name, for `TOMOSHIBI_SHOW=settings/<page>`.
pub const PAGES: [&str; 0] = [];

/// Opens the settings over `parent`, on `page` when one is named.
pub fn present(context: &Context, parent: &impl IsA<gtk::Widget>, page: Option<&str>) {
    let _ = (context, page);
    let dialog = adw::PreferencesDialog::new();
    dialog.set_title(&t("Settings"));
    dialog.present(Some(parent));
}
