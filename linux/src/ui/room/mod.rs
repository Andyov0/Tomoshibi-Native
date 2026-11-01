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
//! the core's threads; [`observer`] sends each `CallEvent` to the main
//! thread before anything reads it.
//!
//! What is here is the skeleton's first version, for the room module to
//! replace: it joins the call it is handed with no devices, keeps the
//! joined call in [`RoomState`], counts who is there, leaves, and says how
//! the call ended. It is what proves the core's media side (WebRTC) links
//! and connects on Linux: a debug run given `TOMOSHIBI_AUTOJOIN` reaches it.

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
};

use adw::prelude::*;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};
use tomoshibi_core::call::{join_call, Call, CallError, CallEvent, CallObserver, CallState};

use super::{kit, Context};
use crate::{
    app::{log, AppModel, CallEntry, Change, Ending, Screen},
    i18n::{t, t_with},
    media::Media,
};

/// What the room keeps in the model while a call is open
/// (`AppModel::kept::<RoomState>()`), so that a screen built again finds
/// the call rather than joining a second time.
#[derive(Default)]
pub struct RoomState {
    /// The joined call; `None` while joining and after it ends.
    pub call: RefCell<Option<Arc<Call>>>,
    /// A join is out for the call entered.
    pub joining: Cell<bool>,
    /// The connection's state, as the core last said it.
    pub state: Cell<Option<CallState>>,
    /// Everybody in the call, this app included.
    pub people: Cell<usize>,
}

/// The room's own change, for [`AppModel::announce`].
pub const CHANGED: Change = Change::Module("room");

/// The screen for `screen`: the call or its ending.
pub fn page(context: &Context, screen: Screen) -> gtk::Widget {
    match screen {
        Screen::Call => call(context),
        _ => ended(context),
    }
}

/// Opens the call `entry` describes and starts telling `observer` about it.
pub async fn join(
    entry: &CallEntry,
    observer: Arc<dyn CallObserver>,
) -> Result<Arc<Call>, CallError> {
    join_call(entry.options(), observer).await
}

/// An observer for the core's threads, and the main thread's end of it:
/// every event is sent, in order, to the receiver, which the room reads
/// from GLib's main context (`glib::spawn_future_local`). Nothing GTK, and
/// nothing of the model, is touched on the core's threads.
pub fn observer() -> (Arc<dyn CallObserver>, UnboundedReceiver<CallEvent>) {
    let (sender, receiver) = unbounded_channel();
    (Arc::new(ToMainThread(sender)), receiver)
}

struct ToMainThread(UnboundedSender<CallEvent>);

impl CallObserver for ToMainThread {
    fn on_event(&self, event: CallEvent) {
        // The receiver is gone once the room has stopped listening, after
        // the call ended; what arrives after that has nobody to tell.
        let _ = self.0.send(event);
    }
}

/// How LiveKit's word for a disconnection reads to the person (the Mac's
/// AppModel, CallModel.swift `gone`): the host closing the room and being
/// taken out want their own sentences, and anything else that was not this
/// app leaving is a lost connection.
pub fn ending_of(reason: &str) -> Ending {
    if reason.contains("RoomDeleted") || reason.contains("RoomClosed") {
        Ending::Closed
    } else if reason.contains("ParticipantRemoved") {
        Ending::Removed
    } else if reason.contains("ClientInitiated") {
        Ending::Left
    } else {
        Ending::Lost(reason.to_owned())
    }
}

/// Joins the call entered, once, and follows it until it ends.
fn start(model: &AppModel, media: Rc<dyn Media>) {
    let room = model.kept::<RoomState>();
    let Some(entry) = model.call_entry() else {
        return;
    };
    if room.joining.get() || room.call.borrow().is_some() {
        return;
    }
    room.joining.set(true);
    let model = model.downgrade();
    glib::spawn_future_local(async move {
        let (observer, mut events) = observer();
        let joined = join(&entry, observer).await;
        let Some(model) = model.upgrade() else {
            return;
        };
        let room = model.kept::<RoomState>();
        room.joining.set(false);
        let call = match joined {
            Ok(call) => call,
            Err(fault) => {
                log(&format!("the call could not be joined: {fault}"));
                model.end_call(Ending::Lost(fault.to_string()));
                return;
            }
        };
        media.attach_call(&call);
        room.people.set(call.participants().len() + 1);
        room.state.set(Some(CallState::Connected));
        room.call.replace(Some(call.clone()));
        log(&format!(
            "joined the call as {}: {} in the call",
            call.local_identity(),
            room.people.get()
        ));
        model.announce(CHANGED);
        #[cfg(debug_assertions)]
        if model.launch().make_invite {
            make_invite(&entry);
        }

        let weak = model.downgrade();
        drop(model);
        while let Some(event) = events.recv().await {
            let Some(model) = weak.upgrade() else {
                return;
            };
            if !follow(&model, media.as_ref(), &call, event) {
                return;
            }
        }
    });
}

/// TOMOSHIBI_DEBUG_MAKE_INVITE: an invite into the call this app opened,
/// its token written to the log for the test that asked for it. Only a
/// debug build, against the local deployment a test gives it, ever does
/// this; an invite is a key to the room.
#[cfg(debug_assertions)]
fn make_invite(entry: &CallEntry) {
    let (deployment, room, bearer) = (
        entry.deployment.clone(),
        entry.grant.room.clone(),
        entry.grant.token.clone(),
    );
    glib::spawn_future_local(async move {
        match deployment.create_invite(room, bearer).await {
            Ok(made) => log(&format!("made an invite: {}", made.token)),
            Err(fault) => log(&format!("no invite was made: {fault}")),
        }
    });
}

/// One event of the call; false once the call is over.
fn follow(model: &AppModel, media: &dyn Media, call: &Arc<Call>, event: CallEvent) -> bool {
    let room = model.kept::<RoomState>();
    match event {
        CallEvent::StateChanged { state } => room.state.set(Some(state)),
        CallEvent::ParticipantJoined { participant } => {
            room.people.set(call.participants().len() + 1);
            log(&format!(
                "{} joined: {} in the call",
                participant.identity,
                room.people.get()
            ));
        }
        CallEvent::ParticipantLeft { identity } => {
            room.people.set(call.participants().len() + 1);
            log(&format!(
                "{identity} left: {} in the call",
                room.people.get()
            ));
        }
        CallEvent::Ended { reason } => {
            // A leave from here has already said how it ended.
            if room.call.borrow_mut().take().is_some() {
                media.detach_call();
                model.end_call(ending_of(&reason));
            }
            return false;
        }
        _ => return true,
    }
    model.announce(CHANGED);
    true
}

/// Leaves the call, if there is one, and says so: the leave button, and a
/// debug run's TOMOSHIBI_DEBUG_QUIT_AFTER before it quits.
pub async fn leave(model: &AppModel, media: &dyn Media) {
    let call = model.kept::<RoomState>().call.borrow_mut().take();
    if let Some(call) = call {
        call.leave().await;
        media.detach_call();
        model.end_call(Ending::Left);
    }
}

/// The skeleton's call: the room's name, how the connection is, how many
/// are there, and leaving.
fn call(context: &Context) -> gtk::Widget {
    let model = &context.model;
    let page = gtk::Box::new(gtk::Orientation::Vertical, 0);
    page.add_css_class("tmsb-call-ground");
    page.append(&kit::bar());
    let column = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(14)
        .valign(gtk::Align::Center)
        .vexpand(true)
        .build();
    let room_name = model
        .call_entry()
        .map(|entry| entry.grant.room)
        .unwrap_or_default();
    column.append(&kit::title(&room_name));
    let status = kit::muted("");
    status.set_widget_name("room.status");
    column.append(&status);
    let leave_button = kit::plain_button(&t("Leave the call"));
    leave_button.set_halign(gtk::Align::Center);
    column.append(&leave_button);
    page.append(&kit::Arrival::new(&column).rises(0));

    let fill = {
        let status = status.downgrade();
        move |model: &AppModel| {
            let Some(status) = status.upgrade() else {
                return;
            };
            let room = model.kept::<RoomState>();
            let said = match room.state.get() {
                None => t("Connecting…"),
                Some(CallState::Reconnecting) => t("Reconnecting…"),
                Some(CallState::Disconnected) => t("Connection lost"),
                Some(CallState::Connected) => {
                    t_with("{count} people", &[("count", &room.people.get())])
                }
            };
            status.set_label(&said);
        }
    };
    fill(model);
    model.subscribe_while(&page, move |model, change| {
        if change == CHANGED {
            fill(model);
        }
    });

    let (weak, media) = (model.downgrade(), context.media.clone());
    leave_button.connect_clicked(move |button| {
        button.set_sensitive(false);
        let (weak, media) = (weak.clone(), media.clone());
        glib::spawn_future_local(async move {
            if let Some(model) = weak.upgrade() {
                leave(&model, media.as_ref()).await;
            }
        });
    });

    start(model, context.media.clone());
    page.upcast()
}

/// The sentence for how the call ended, and the way back.
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
    let sentence = match context.model.ending() {
        Some(Ending::Closed) => t("The host closed this room."),
        Some(Ending::Removed) => t("You were removed from this room."),
        Some(Ending::Lost(_)) => t("Connection lost"),
        Some(Ending::Left) | None => t("You left the call."),
    };
    column.append(&kit::muted(&sentence));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endings_read_as_the_mac_reads_them() {
        assert_eq!(ending_of("RoomDeleted"), Ending::Closed);
        assert_eq!(ending_of("ParticipantRemoved"), Ending::Removed);
        assert_eq!(ending_of("ClientInitiated"), Ending::Left);
        assert_eq!(ending_of("SignalClose"), Ending::Lost("SignalClose".into()));
    }

    #[test]
    fn the_observer_hands_events_over_in_order() {
        let (observer, mut events) = observer();
        observer.on_event(CallEvent::LosslessGaveUp);
        observer.on_event(CallEvent::ParticipantLeft {
            identity: "a".into(),
        });
        assert_eq!(events.try_recv().unwrap(), CallEvent::LosslessGaveUp);
        assert!(matches!(
            events.try_recv().unwrap(),
            CallEvent::ParticipantLeft { .. }
        ));
        assert!(events.try_recv().is_err());
    }
}
