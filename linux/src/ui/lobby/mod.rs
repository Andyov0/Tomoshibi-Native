//! The screens before a call: signing in, the lobby (the sidebar and the
//! stage, as the Mac lays it out), the pre-join, knocking and waiting, with
//! the lobby's notices, the update offer, the animals, and the start, join
//! and arrange sheets.
//!
//! Owned by the lobby module. The window asks [`page`] for a screen
//! whenever the model moves to one of these; everything else in this
//! directory is the module's own. A call is entered by handing the
//! deployment's grant to `AppModel::enter_call`, which shows the room.

pub mod sign_in;

use adw::prelude::*;

use super::{kit, Context};
use crate::app::Screen;

/// The screen for `screen`: one of signing in, the lobby, the pre-join,
/// knocking and waiting.
pub fn page(context: &Context, screen: Screen) -> gtk::Widget {
    match screen {
        Screen::SignIn => sign_in::page(context),
        other => not_yet(other),
    }
}

/// A screen the module has not written yet: its name, so that a run that
/// reaches it says where it is.
fn not_yet(screen: Screen) -> gtk::Widget {
    let page = gtk::Box::new(gtk::Orientation::Vertical, 0);
    page.append(&kit::bar());
    let column = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(14)
        .valign(gtk::Align::Center)
        .vexpand(true)
        .build();
    column.append(&kit::mark(56));
    column.append(&kit::muted(screen.name()));
    page.append(&kit::Arrival::new(&column).rises(0));
    page.upcast()
}
