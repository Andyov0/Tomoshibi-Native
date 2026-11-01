//! The call: its plan and layout, the tiles and the stage, the control bar,
//! the panels (messages, sound, devices, host, reactions, watch together,
//! music), the name plates, leaving, and the screen after it ends. Drawn in
//! the call's palette (`tmsb_call_*`) in every appearance.
//!
//! Owned by the room module. The window asks [`page`] for the call when the
//! model moves to `Screen::Call` (after `AppModel::enter_call`) and for the
//! ending at `Screen::Ended`. The call is opened with [`join`]; every remote
//! picture is a `render::VideoView` given to `Call::attach_video`, the call
//! is handed to `Media::attach_call` once joined and every remote sound
//! then goes to `Media::play`, and the devices are the `media::Media` in the
//! context. When the call is over the room says why with
//! `AppModel::end_call` (after `Media::detach_call`).
//!
//! The call outlives its widgets: the window throws a screen away and
//! builds it again, so the joined `Call` and what the room knows about it
//! live in the model (`AppModel::kept::<YourRoomState>()`), and the call's
//! screens re-word themselves on `i18n::on_change` rather than being built
//! again in the middle of a call (ui/window.rs). The observer is called on
//! the core's threads; it sends each `CallEvent` to the main thread
//! (`glib::MainContext::default().invoke`) before anything is drawn.

use std::sync::Arc;

use adw::prelude::*;
use tomoshibi_core::call::{join_call, Call, CallError, CallObserver};

use super::{kit, Context};
use crate::{
    app::{CallEntry, Screen},
    i18n::t,
};

/// The screen for `screen`: the call or its ending.
pub fn page(context: &Context, screen: Screen) -> gtk::Widget {
    match screen {
        Screen::Call => call(context),
        _ => ended(context),
    }
}

/// Opens the call `entry` describes and starts telling `observer` about it.
/// The observer is called on the core's threads; it sends what it is told
/// to the main thread before anything is drawn.
pub async fn join(
    entry: &CallEntry,
    observer: Arc<dyn CallObserver>,
) -> Result<Arc<Call>, CallError> {
    join_call(entry.options(), observer).await
}

/// Not written yet: the call's ground, and the way back.
fn call(context: &Context) -> gtk::Widget {
    let page = gtk::Box::new(gtk::Orientation::Vertical, 0);
    page.add_css_class("tmsb-call-ground");
    page.append(&kit::bar());
    let column = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(14)
        .valign(gtk::Align::Center)
        .vexpand(true)
        .build();
    let room = context
        .model
        .call_entry()
        .map(|entry| entry.grant.room)
        .unwrap_or_default();
    column.append(&kit::muted(&room));
    page.append(&kit::Arrival::new(&column).rises(0));
    page.upcast()
}

/// Not written yet: the sentence for how the call ended, and the lobby.
fn ended(context: &Context) -> gtk::Widget {
    let page = gtk::Box::new(gtk::Orientation::Vertical, 0);
    page.add_css_class("tmsb-call-ground");
    page.append(&kit::bar());
    let column = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(14)
        .valign(gtk::Align::Center)
        .vexpand(true)
        .build();
    column.append(&kit::muted(&t("You left the call.")));
    let back = kit::plain_button(&t("Back to meetings"));
    back.set_halign(gtk::Align::Center);
    let model = context.model.downgrade();
    back.connect_clicked(move |_| {
        if let Some(model) = model.upgrade() {
            model.show(Screen::Lobby);
        }
    });
    column.append(&back);
    page.append(&kit::Arrival::new(&column).rises(0));
    page.upcast()
}
