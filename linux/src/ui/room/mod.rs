//! The call: its plan and layout, the tiles and the stage, the control bar,
//! the panels (messages, sound, devices, host), the reactions, the name
//! plates, leaving, and the screen after it ends. Drawn in the call's
//! palette (`tmsb_call_*`) in every appearance. The Mac's RoomView and
//! CallModel are the reference for the desktop layout and behaviour, the
//! web client (Room.tsx and its neighbours) for every rule.
//!
//! The window asks [`page`] for the call when the model moves to
//! `Screen::Call` (after `AppModel::enter_call`) and for the ending at
//! `Screen::Ended`. The call is opened with [`join`]; every remote picture
//! is a `render::VideoView` given to `Call::attach_video`, the call is
//! handed to `Media::attach_call` once joined and every remote sound then
//! goes to `Media::play`, and the devices are the `media::Media` in the
//! context. When the call is over the room says why with
//! `AppModel::end_call` (after `Media::detach_call`).
//!
//! The call outlives its widgets: the window throws a screen away and
//! builds it again, so the joined `Call` and everything the room knows about
//! it live in the model (`AppModel::kept::<RoomState>()`), and the call's
//! screen re-words itself on `i18n::on_change` rather than being built again
//! in the middle of a call (ui/window.rs). The observer is called on the
//! core's threads; [`observer`] sends each `CallEvent` to the main thread
//! before anything reads it.
//!
//! The parts: plan.rs (where pictures go, the web's rules), talk.rs (the
//! call as the room knows it, from its events), stage.rs and tile.rs (the
//! pictures), page.rs (the screen and everything on it), panels.rs (the
//! panels beside the call), motion.rs and glyph.rs (how pieces come and go,
//! and the symbols the icon theme lacks), and, in a debug build, fake.rs
//! (devices that are not there) and script.rs (what a test asks the room to
//! do and picture).

mod css;
#[cfg(debug_assertions)]
mod fake;
mod glyph;
mod motion;
mod page;
mod panels;
pub mod plan;
#[cfg(debug_assertions)]
mod script;
mod stage;
pub mod talk;
mod tile;

use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    rc::Rc,
    sync::Arc,
    time::Duration,
};

use adw::prelude::*;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};
use tomoshibi_core::call::{join_call, Call, CallError, CallEvent, CallObserver, CallState};
use tomoshibi_protocol::{
    api::{HostStanding, Joining, Knocker},
    hands,
    identity::scope_of,
    share::ShareQuality,
};

use super::Context;
use crate::{
    app::{log, AppModel, CallEntry, Change, Ending, Screen},
    i18n::{t, t_with},
    media::{Camera, Media, MediaError, Microphone, ScreenShare, ShareChoice},
};
use talk::{Effect, Line, Talk, Toast, ToastKind};

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
    /// The call as the room knows it, from its events.
    pub talk: RefCell<Option<Talk>>,
    /// What the person has done to the screen: the stage, the page, the
    /// open panel and popover, the volumes, the notices.
    pub view: RefCell<View>,
    /// This participant's own devices in the call.
    pub local: Devices,
    /// What the deployment says about who runs the room, and the door.
    pub host: RefCell<Host>,
    /// The devices the call was joined with: the context's, or in a debug
    /// run the test ones (fake.rs).
    pub media: RefCell<Option<Rc<dyn Media>>>,
    /// The call's screen while it is shown. Held here, and only here: every
    /// closure on its widgets holds it weakly, so that the widgets do not
    /// keep it alive, and this lets it go when the window moves to another
    /// screen. It is also how what is drawn once rather than kept (a
    /// reaction rising off a picture) reaches it.
    page: RefCell<Option<Rc<page::Page>>>,
    next_notice: Cell<u64>,
}

/// The panels beside the call (the Mac's `RoomView.Panel`). Watch together
/// and music are not here: the core's protocols for them are, but playing
/// a shared video or the song desk needs a player on this platform, which
/// does not exist yet; their buttons are not offered rather than offered
/// and empty.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Panel {
    Messages,
    Sound,
    Devices,
    Host,
}

impl Panel {
    pub fn of(name: &str) -> Option<Panel> {
        match name {
            "messages" => Some(Panel::Messages),
            "sound" => Some(Panel::Sound),
            "devices" => Some(Panel::Devices),
            "host" => Some(Panel::Host),
            _ => None,
        }
    }
}

/// The bar's popovers, one open at a time across the whole screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pop {
    Reactions,
    Leave,
    Share,
    More,
    Signal,
}

impl Pop {
    pub fn of(name: &str) -> Option<Pop> {
        match name {
            "reactions" => Some(Pop::Reactions),
            "leave" => Some(Pop::Leave),
            "share" | "share.screen" => Some(Pop::Share),
            "more" => Some(Pop::More),
            "signal" => Some(Pop::Signal),
            _ => None,
        }
    }
}

/// One person's sound as the listener set it (hearing.ts): a level from
/// nought to one, and whether it is stopped altogether.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Hearing {
    pub level: f32,
    pub blocked: bool,
}

impl Default for Hearing {
    fn default() -> Self {
        Hearing {
            level: 1.0,
            blocked: false,
        }
    }
}

#[derive(Default)]
pub struct View {
    pub pin: plan::StagePin,
    pub page: usize,
    /// The stage given the whole screen (the web's "Fill the screen").
    pub filled: bool,
    /// The other person of a call of two, put on the stage only to fill the
    /// screen with them: leaving the full screen takes them off it again.
    pub pinned_to_fill: Option<String>,
    pub panel: Option<Panel>,
    pub pop: Option<Pop>,
    /// The alone-in-the-room capsule, once closed, stays closed.
    pub alone_dismissed: bool,
    /// The `watching` attribute as this participant last wrote it.
    pub watching_written: String,
    pub copied: bool,
    /// What is in the messages box, kept with the call so that closing the
    /// panel half way through a sentence does not lose it.
    pub draft: String,
    pub sending: bool,
    /// The last message did not leave; its words are back in the box.
    pub chat_failed: bool,
    pub hearing: HashMap<String, Hearing>,
    pub toasts: Vec<(u64, Toast)>,
    pub said: Vec<(u64, Line)>,
    /// The share's size setting in the share popover.
    pub share_quality: Option<ShareQuality>,
    /// A press that failed, said over the stage until pressed.
    pub problem: Option<String>,
    /// The controls are stepped aside (idle while the stage fills the
    /// screen, or the setting says to).
    pub idle: bool,
}

#[derive(Default)]
pub struct Devices {
    pub microphone: RefCell<Option<Rc<dyn Microphone>>>,
    pub camera: RefCell<Option<Rc<dyn Camera>>>,
    pub share: RefCell<Option<Rc<dyn ScreenShare>>>,
    pub microphone_on: Cell<bool>,
    pub camera_on: Cell<bool>,
    pub microphone_published: Cell<bool>,
    /// A microphone or camera press still being carried out: another of the
    /// same meanwhile is ignored (ControlBar.tsx `guard`), and only ignored.
    /// One flag each, so that the join turning both on starts both.
    pub microphone_busy: Cell<bool>,
    pub camera_busy: Cell<bool>,
    pub sharing_busy: Cell<bool>,
    /// The hand's own press, so that a pending microphone does not hold up a
    /// raised hand, nor the hand a microphone.
    pub raising: Cell<bool>,
}

#[derive(Default)]
pub struct Host {
    /// What the server last said; nothing until it has.
    pub standing: Option<HostStanding>,
    pub waiting: Vec<Knocker>,
    /// The one host action under way, named by what it acts on.
    pub busy: Option<String>,
    /// The link the last "Invite somebody" made.
    pub link: Option<String>,
    /// "Close the room" asked once, waiting for the second press.
    pub ending: bool,
}

impl Host {
    pub fn yours(&self) -> bool {
        self.standing
            .as_ref()
            .is_some_and(|standing| standing.yours)
    }
}

/// The room's own change, for [`AppModel::announce`].
pub const CHANGED: Change = Change::Module("room");

/// The screen for `screen`: the call or its ending.
pub fn page(context: &Context, screen: Screen) -> gtk::Widget {
    crate::theme::add_module_css("room", css::ROOM_CSS);
    match screen {
        Screen::Call => {
            let media = media_for(context);
            let built = page::Page::build(context, media.clone());
            let widget = built.widget();
            let room = context.model.kept::<RoomState>();
            room.page.replace(Some(built));
            // Let go of once the window shows another screen: the ending,
            // or the next call's own screen.
            let weak = Rc::downgrade(&room);
            let shown = widget.downgrade();
            context
                .model
                .subscribe_while(&widget, move |model, change| {
                    if change == Change::Screen && model.screen() != Screen::Call {
                        if let Some(room) = weak.upgrade() {
                            let ours = room
                                .page
                                .borrow()
                                .as_ref()
                                .is_some_and(|page| Some(page.widget()) == shown.upgrade());
                            if ours {
                                room.page.replace(None);
                            }
                        }
                    }
                });
            start(&context.model, media);
            widget
        }
        _ => page::ended(context),
    }
}

/// The devices this call uses: the context's, or the test ones in a debug
/// run that asks for them.
fn media_for(context: &Context) -> Rc<dyn Media> {
    let room = context.model.kept::<RoomState>();
    if let Some(media) = room.media.borrow().clone() {
        return media;
    }
    #[cfg(debug_assertions)]
    if fake::wanted().is_some() {
        log("using the test devices (TOMOSHIBI_DEBUG_FAKE_MEDIA)");
        let media: Rc<dyn Media> = fake::FakeMedia::new();
        room.media.replace(Some(media.clone()));
        return media;
    }
    room.media.replace(Some(context.media.clone()));
    context.media.clone()
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

/// Seconds since the epoch, for the speaking record.
fn now() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.as_secs_f64())
        .unwrap_or_default()
}

fn changed(model: &AppModel) {
    model.announce(CHANGED);
}

/// The joined call, while there is one.
pub(crate) fn call_of(model: &AppModel) -> Option<Arc<Call>> {
    model.kept::<RoomState>().call.borrow().clone()
}

fn media_of(model: &AppModel) -> Option<Rc<dyn Media>> {
    model.kept::<RoomState>().media.borrow().clone()
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
    // A fresh call starts from nothing the last one left.
    room.talk.replace(None);
    room.view.replace(View::default());
    room.host.replace(Host::default());
    room.local.microphone_on.set(false);
    room.local.camera_on.set(false);
    room.local.microphone_published.set(false);
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
        let mut talk = Talk::new(
            &entry.grant.room,
            &call.local_identity(),
            &call.local_name(),
        );
        talk.arrive(&call.participants());
        room.talk.replace(Some(talk));
        room.people.set(call.participants().len() + 1);
        room.state.set(Some(CallState::Connected));
        room.call.replace(Some(call.clone()));
        log(&format!(
            "joined the call as {}: {} in the call",
            call.local_identity(),
            room.people.get()
        ));
        // What the join already said (the people there, their tracks) is
        // taken before anybody arriving counts as news (CallModel
        // `settled`).
        while let Ok(event) = events.try_recv() {
            if !follow(&model, &call, event) {
                return;
            }
        }
        if let Some(talk) = room.talk.borrow_mut().as_mut() {
            talk.settled = true;
        }
        devices_at_join(&model, &entry, &call);
        refresh_standing(&model);
        watch_the_door(&model);
        changed(&model);
        #[cfg(debug_assertions)]
        {
            if model.launch().make_invite {
                make_invite(&entry);
            }
            script::run(&model);
        }

        let weak = model.downgrade();
        drop(model);
        while let Some(event) = events.recv().await {
            let Some(model) = weak.upgrade() else {
                return;
            };
            if !follow(&model, &call, event) {
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
fn follow(model: &AppModel, call: &Arc<Call>, event: CallEvent) -> bool {
    let room = model.kept::<RoomState>();
    let effects = {
        let mut talk = room.talk.borrow_mut();
        let Some(talk) = talk.as_mut() else {
            return true;
        };
        let effects = talk.apply(event, now());
        room.state.set(Some(talk.state));
        room.people.set(talk.count());
        effects
    };
    let mut going_on = true;
    for effect in effects {
        match effect {
            Effect::Play { sid, identity } => {
                if let Some(media) = media_of(model) {
                    media.play(call, &sid, &identity);
                }
                apply_hearing(model, &identity);
            }
            Effect::StopPlaying { sid } => {
                if let Some(media) = media_of(model) {
                    media.stop_playing(call, &sid);
                }
            }
            Effect::Toast(toast) => announce(model, toast),
            Effect::Said(line) => say_in_corner(model, line),
            Effect::Reaction { from, what } => {
                log(&format!("{from} reacted with {what}"));
                if let Some(page) = room.page.borrow().clone() {
                    page.float(&from, &what);
                }
            }
            Effect::Standing => refresh_standing(model),
            Effect::Hearing => {
                let everybody: Vec<String> = room
                    .talk
                    .borrow()
                    .as_ref()
                    .map(Talk::identities)
                    .unwrap_or_default();
                for identity in everybody {
                    apply_hearing(model, &identity);
                }
            }
            Effect::Ended { reason } => {
                log(&format!("the call ended: {reason}"));
                // A leave from here has already said how it ended.
                if room.call.borrow_mut().take().is_some() {
                    let_go_of_devices(model);
                    if let Some(media) = media_of(model) {
                        media.detach_call();
                    }
                    model.end_call(ending_of(&reason));
                }
                going_on = false;
            }
        }
    }
    changed(model);
    going_on
}

/// Leaves the call, if there is one, and says so: the leave button, and a
/// debug run's TOMOSHIBI_DEBUG_QUIT_AFTER before it quits.
pub async fn leave(model: &AppModel, media: &dyn Media) {
    let room = model.kept::<RoomState>();
    let call = room.call.borrow_mut().take();
    if let Some(call) = call {
        stop_sharing_now(model).await;
        let_go_of_devices(model);
        call.leave().await;
        // The devices the call was joined with, which in a debug run are not
        // the context's.
        match media_of(model) {
            Some(own) => own.detach_call(),
            None => media.detach_call(),
        }
        model.end_call(Ending::Left);
    }
}

/// Closes the microphone and the camera, which the call opened or was
/// handed by the pre-join.
fn let_go_of_devices(model: &AppModel) {
    let room = model.kept::<RoomState>();
    if let Some(microphone) = room.local.microphone.take() {
        microphone.close();
    }
    if let Some(camera) = room.local.camera.take() {
        camera.close();
    }
    room.local.microphone_on.set(false);
    room.local.camera_on.set(false);
}

// ---- notices -----------------------------------------------------------

/// Says something in the corner, for as long as it says it lasts; said
/// again under the same key it replaces the one there.
fn announce(model: &AppModel, toast: Toast) {
    let room = model.kept::<RoomState>();
    let id = room.next_notice.get() + 1;
    room.next_notice.set(id);
    let lasts = toast.lasts_ms;
    {
        let mut view = room.view.borrow_mut();
        if let Some(key) = toast.key.as_ref() {
            view.toasts
                .retain(|(_, kept)| kept.key.as_ref() != Some(key));
        }
        view.toasts.push((id, toast));
        // The newest few: a burst of arrivals is not a wall of cards.
        let over = view.toasts.len().saturating_sub(4);
        view.toasts.drain(..over);
    }
    if let Some(lasts) = lasts {
        let weak = model.downgrade();
        glib::timeout_add_local_once(Duration::from_millis(lasts), move || {
            if let Some(model) = weak.upgrade() {
                dismiss(&model, id);
            }
        });
    }
    changed(model);
}

pub(crate) fn dismiss(model: &AppModel, id: u64) {
    let room = model.kept::<RoomState>();
    room.view
        .borrow_mut()
        .toasts
        .retain(|(kept, _)| *kept != id);
    changed(model);
}

/// A failure, said briefly.
fn failed(model: &AppModel, sentence: impl Into<String>) {
    announce(model, Toast::failure(sentence));
}

fn say_in_corner(model: &AppModel, line: Line) {
    let room = model.kept::<RoomState>();
    let id = room.next_notice.get() + 1;
    room.next_notice.set(id);
    {
        let mut view = room.view.borrow_mut();
        view.said.push((id, line));
        let over = view.said.len().saturating_sub(3);
        view.said.drain(..over);
    }
    let weak = model.downgrade();
    glib::timeout_add_local_once(Duration::from_millis(talk::SAID_FOR_MS), move || {
        if let Some(model) = weak.upgrade() {
            let room = model.kept::<RoomState>();
            room.view.borrow_mut().said.retain(|(kept, _)| *kept != id);
            changed(&model);
        }
    });
}

// ---- devices -----------------------------------------------------------

/// The devices the pre-join opened, or the ones it said to turn on.
fn devices_at_join(model: &AppModel, entry: &CallEntry, call: &Arc<Call>) {
    let room = model.kept::<RoomState>();
    if let Some(microphone) = entry.microphone.clone() {
        room.local.microphone.replace(Some(microphone));
    }
    if let Some(camera) = entry.camera.clone() {
        room.local.camera.replace(Some(camera));
    }
    let mute = model.launch().mute.clone();
    if mute.as_deref() == Some("speakers") {
        if let Some(media) = media_of(model) {
            media.set_silent(true);
        }
    }
    let mut microphone = entry.microphone_on && mute.is_none();
    let mut camera = entry.camera_on;
    let mut share = false;
    #[cfg(debug_assertions)]
    {
        microphone |= fake::on_at_join("microphone") && mute.is_none();
        camera |= fake::on_at_join("camera");
        share |= fake::on_at_join("share");
    }
    let _ = call;
    if microphone {
        set_microphone(model, true);
    }
    if camera {
        set_camera(model, true);
    }
    if share {
        start_share(model);
    }
}

/// The microphone on or off: published once, then muted and unmuted at
/// the track (the Mac's `setMicrophone`), so that turning it back on is
/// immediate and a second voice is never published.
pub(crate) fn set_microphone(model: &AppModel, on: bool) {
    let room = model.kept::<RoomState>();
    let (Some(call), Some(media)) = (call_of(model), media_of(model)) else {
        return;
    };
    if room.local.microphone_busy.replace(true) {
        return;
    }
    let weak = model.downgrade();
    glib::spawn_future_local(async move {
        let Some(model) = weak.upgrade() else {
            return;
        };
        let room = model.kept::<RoomState>();
        let result = async {
            if !on {
                if let Some(microphone) = room.local.microphone.borrow().as_ref() {
                    microphone.set_muted(true);
                }
                return Ok(());
            }
            let existing = room.local.microphone.borrow().clone();
            let microphone = match existing {
                Some(microphone) => microphone,
                None => {
                    let chosen = chosen_device(&model, crate::media::DeviceKind::Microphone);
                    let opened = media.open_microphone(chosen).await?;
                    room.local.microphone.replace(Some(opened.clone()));
                    opened
                }
            };
            if !room.local.microphone_published.get() {
                microphone
                    .publish(call.clone(), original_sound(&model))
                    .await?;
                room.local.microphone_published.set(true);
            }
            microphone.set_muted(false);
            Ok::<(), MediaError>(())
        }
        .await;
        room.local.microphone_busy.set(false);
        match result {
            Ok(()) => room.local.microphone_on.set(on),
            Err(fault) => {
                log(&format!("the microphone did not start: {fault}"));
                if let Some(microphone) = room.local.microphone.borrow().as_ref() {
                    microphone.set_muted(true);
                }
                room.local.microphone_on.set(false);
                failed(&model, device_sentence(&fault, true));
            }
        }
        changed(&model);
    });
}

/// The camera on or off: published when turned on and taken down when
/// turned off, the device let go with it so that its light goes out (the
/// Mac's `setCamera`).
pub(crate) fn set_camera(model: &AppModel, on: bool) {
    let room = model.kept::<RoomState>();
    let (Some(call), Some(media)) = (call_of(model), media_of(model)) else {
        return;
    };
    if room.local.camera_busy.replace(true) {
        return;
    }
    let weak = model.downgrade();
    glib::spawn_future_local(async move {
        let Some(model) = weak.upgrade() else {
            return;
        };
        let room = model.kept::<RoomState>();
        let result = async {
            if !on {
                if let Some(camera) = room.local.camera.take() {
                    camera.unpublish().await;
                    camera.close();
                }
                return Ok(());
            }
            let existing = room.local.camera.borrow().clone();
            let camera = match existing {
                Some(camera) => camera,
                None => {
                    let chosen = chosen_device(&model, crate::media::DeviceKind::Camera);
                    let opened = media.open_camera(chosen).await?;
                    room.local.camera.replace(Some(opened.clone()));
                    opened
                }
            };
            if let Err(fault) = camera.publish(call.clone()).await {
                // A camera published and never fed is a black tile for
                // everybody else; one that did not publish is let go.
                camera.close();
                room.local.camera.replace(None);
                return Err(fault);
            }
            Ok::<(), MediaError>(())
        }
        .await;
        room.local.camera_busy.set(false);
        match result {
            Ok(()) => room.local.camera_on.set(on),
            Err(fault) => {
                log(&format!("the camera did not start: {fault}"));
                room.local.camera_on.set(false);
                failed(&model, device_sentence(&fault, false));
            }
        }
        changed(&model);
    });
}

/// The web's own record of the devices picked in a call (preferences.ts
/// `deviceChoice`): one JSON object under one key, a device id by the
/// browser's kind names. The settings write the same.
const CHOICE_KEY: &str = "meet-live.device-choice";
/// sound.ts `ORIGINAL_KEY`, "on" when the voice is sent as it is.
const ORIGINAL_SOUND_KEY: &str = "meet-live.original-sound";

fn kind_name(kind: crate::media::DeviceKind) -> &'static str {
    use crate::media::DeviceKind;
    match kind {
        DeviceKind::Microphone => "audioinput",
        DeviceKind::Camera => "videoinput",
        DeviceKind::Speaker => "audiooutput",
    }
}

/// The device last picked of a kind, read as the web reads it: anything
/// that is not a short string is nothing picked.
pub(crate) fn chosen_device(model: &AppModel, kind: crate::media::DeviceKind) -> Option<String> {
    let stored = model.preferences().string(CHOICE_KEY)?;
    let parsed: serde_json::Value = serde_json::from_str(&stored).ok()?;
    parsed
        .get(kind_name(kind))
        .and_then(serde_json::Value::as_str)
        .filter(|id| !id.is_empty() && id.len() <= 256)
        .map(str::to_owned)
}

fn remember_device(model: &AppModel, kind: crate::media::DeviceKind, id: Option<&str>) {
    let mut choice = model
        .preferences()
        .string(CHOICE_KEY)
        .and_then(|stored| serde_json::from_str::<serde_json::Value>(&stored).ok())
        .filter(serde_json::Value::is_object)
        .unwrap_or_else(|| serde_json::json!({}));
    if let Some(object) = choice.as_object_mut() {
        match id {
            Some(id) if id.len() <= 256 => {
                object.insert(kind_name(kind).to_owned(), serde_json::Value::from(id));
            }
            _ => {
                object.remove(kind_name(kind));
            }
        }
    }
    model.preferences().set(CHOICE_KEY, choice.to_string());
}

fn original_sound(model: &AppModel) -> bool {
    model.preferences().string(ORIGINAL_SOUND_KEY).as_deref() == Some("on")
}

/// The web's "original sound" (sound.ts `applyOriginalSound`): the voice
/// captured again without noise suppression or levelling, and remembered.
/// A voice already published is published again with the new capture,
/// which the Microphone contract says it does when asked again.
pub(crate) fn set_original_sound(model: &AppModel, on: bool) {
    if on {
        model.preferences().set(ORIGINAL_SOUND_KEY, "on");
    } else {
        model.preferences().remove(ORIGINAL_SOUND_KEY);
    }
    let room = model.kept::<RoomState>();
    let microphone = room.local.microphone.borrow().clone();
    if let (Some(microphone), Some(call)) = (microphone, call_of(model)) {
        if room.local.microphone_published.get() {
            let weak = model.downgrade();
            glib::spawn_future_local(async move {
                if let Err(fault) = microphone.publish(call, on).await {
                    log(&format!("the voice was not published again: {fault}"));
                    if let Some(model) = weak.upgrade() {
                        failed(&model, t("That could not be done. Try again."));
                    }
                }
            });
        }
    }
    changed(model);
}

fn device_sentence(fault: &MediaError, microphone: bool) -> String {
    match (fault, microphone) {
        (MediaError::Denied | MediaError::Unsupported | MediaError::NotFound, true) => {
            t("Can't use your microphone")
        }
        (MediaError::Denied | MediaError::Unsupported | MediaError::NotFound, false) => {
            t("Can't use your camera")
        }
        _ => t("That could not be done. Try again."),
    }
}

/// A device chosen in the devices panel: the microphone and camera opened
/// again on it, published again where they were on.
pub(crate) fn choose_device(model: &AppModel, kind: crate::media::DeviceKind, id: Option<String>) {
    use crate::media::DeviceKind;
    let room = model.kept::<RoomState>();
    // Not kept in a rehearsal (Launch::rehearsal), as nothing is.
    remember_device(model, kind, id.as_deref());
    match kind {
        DeviceKind::Speaker => {
            if let Some(media) = media_of(model) {
                media.set_speaker(id);
            }
        }
        DeviceKind::Microphone => {
            let on = room.local.microphone_on.get();
            if let Some(old) = room.local.microphone.take() {
                old.close();
            }
            room.local.microphone_published.set(false);
            room.local.microphone_on.set(false);
            if on {
                set_microphone(model, true);
            }
        }
        DeviceKind::Camera => {
            let on = room.local.camera_on.get();
            let old = room.local.camera.take();
            let weak = model.downgrade();
            glib::spawn_future_local(async move {
                if let Some(old) = old {
                    old.unpublish().await;
                    old.close();
                }
                if let Some(model) = weak.upgrade() {
                    model.kept::<RoomState>().local.camera_on.set(false);
                    if on {
                        set_camera(&model, true);
                    }
                    changed(&model);
                }
            });
        }
    }
    changed(model);
}

/// Shares the screen with the setting the share popover shows.
pub(crate) fn start_share(model: &AppModel) {
    let room = model.kept::<RoomState>();
    let (Some(call), Some(media)) = (call_of(model), media_of(model)) else {
        return;
    };
    if room.local.sharing_busy.get() || room.local.share.borrow().is_some() {
        return;
    }
    room.local.sharing_busy.set(true);
    let quality = room
        .view
        .borrow()
        .share_quality
        .unwrap_or(ShareQuality::Auto);
    let choice = ShareChoice {
        fps: if quality == ShareQuality::Auto {
            30
        } else {
            60
        },
        quality,
        with_sound: true,
    };
    changed(model);
    let weak = model.downgrade();
    glib::spawn_future_local(async move {
        let shared = media.share_screen(call, choice).await;
        let Some(model) = weak.upgrade() else {
            return;
        };
        let room = model.kept::<RoomState>();
        room.local.sharing_busy.set(false);
        match shared {
            Ok(share) => {
                let weak = model.downgrade();
                share.on_ended(Box::new(move || {
                    if let Some(model) = weak.upgrade() {
                        model.kept::<RoomState>().local.share.replace(None);
                        changed(&model);
                    }
                }));
                room.local.share.replace(Some(share));
            }
            // The person closing the system's picker is no failure.
            Err(MediaError::Denied) => {}
            Err(fault) => {
                log(&format!("the screen was not shared: {fault}"));
                failed(&model, t("That could not be done. Try again."));
            }
        }
        changed(&model);
    });
}

pub(crate) fn stop_share(model: &AppModel) {
    let weak = model.downgrade();
    glib::spawn_future_local(async move {
        if let Some(model) = weak.upgrade() {
            stop_sharing_now(&model).await;
            changed(&model);
        }
    });
}

async fn stop_sharing_now(model: &AppModel) {
    let share = model.kept::<RoomState>().local.share.take();
    if let Some(share) = share {
        share.stop().await;
    }
}

// ---- the hand, reactions, messages ------------------------------------

/// The hand is an attribute, written only when it changes (hands.ts).
pub(crate) fn set_hand(model: &AppModel, up: bool) {
    let room = model.kept::<RoomState>();
    let Some(call) = call_of(model) else {
        return;
    };
    let current = room.talk.borrow().as_ref().map(|talk| talk.hand_up);
    let Some(value) = hands::hand_to_write(
        current.map(|up| if up { "up".into() } else { String::new() }),
        up,
    ) else {
        return;
    };
    if room.local.raising.replace(true) {
        return;
    }
    let weak = model.downgrade();
    glib::spawn_future_local(async move {
        let written = call
            .set_attributes(HashMap::from([(hands::HAND.to_owned(), value)]))
            .await;
        let Some(model) = weak.upgrade() else {
            return;
        };
        let room = model.kept::<RoomState>();
        room.local.raising.set(false);
        match written {
            Ok(()) => {
                if let Some(talk) = room.talk.borrow_mut().as_mut() {
                    talk.hand_up = up;
                }
            }
            Err(fault) => {
                log(&format!("the hand was not written: {fault}"));
                failed(&model, t("That could not be done. Try again."));
            }
        }
        changed(&model);
    });
}

/// Sends a reaction to everybody and shows it on this participant's own
/// picture at once: data does not come back to whoever sent it (hands.ts
/// `react`). The topic and payload are the protocol crate's, which refuses
/// anything but the six a browser would show.
/// Says whose share is on this participant's stage (watching.ts
/// `useWatching`): the owner of somebody else's screen on the stage, or
/// nothing, written only when it changes, so that whoever is sharing can see
/// who has it open (their watchers card). A share in the strip does not
/// count: nobody has opened it.
pub(crate) fn write_watching(model: &AppModel) {
    let room = model.kept::<RoomState>();
    let Some(call) = call_of(model) else {
        return;
    };
    let stage = {
        let view = room.view.borrow();
        view.pin.pinned().map(|id| {
            let local = id == talk::ME || id == talk::MY_SHARE;
            let (owner, kind) = id.rsplit_once('/').unwrap_or((id, ""));
            hands::StagePicture {
                owner: if local {
                    call.local_identity()
                } else {
                    owner.to_owned()
                },
                screen: kind == "screen",
                local,
            }
        })
    };
    let sharer = hands::watched_sharer(stage);
    let current = room.view.borrow().watching_written.clone();
    let Some(value) = hands::watching_to_write(Some(current), sharer) else {
        return;
    };
    room.view.borrow_mut().watching_written = value.clone();
    let weak = model.downgrade();
    glib::spawn_future_local(async move {
        let written = call
            .set_attributes(HashMap::from([(hands::WATCHING.to_owned(), value)]))
            .await;
        if let Err(fault) = written {
            log(&format!("the watching attribute was not written: {fault}"));
            // Written again at the next change, from what it says now.
            if let Some(model) = weak.upgrade() {
                model.kept::<RoomState>().view.borrow_mut().watching_written = String::from("?");
            }
        }
    });
}

/// Who has this participant's shared screen open (watching.ts
/// `useWatchers`), by name.
pub(crate) fn watchers(model: &AppModel) -> Option<Vec<String>> {
    let room = model.kept::<RoomState>();
    if room.local.share.borrow().is_none() {
        return None;
    }
    let talk = room.talk.borrow();
    let talk = talk.as_ref()?;
    let roster: Vec<hands::RosterMember> = talk
        .people
        .iter()
        .map(|person| hands::RosterMember {
            identity: person.id.clone(),
            attributes: person.attributes.clone(),
        })
        .collect();
    Some(
        hands::watchers(talk.local_identity.clone(), true, roster)
            .into_iter()
            .map(|identity| {
                talk.person(&identity)
                    .map(|person| person.label())
                    .unwrap_or(identity)
            })
            .collect(),
    )
}

pub(crate) fn react(model: &AppModel, what: &str) {
    let Some(call) = call_of(model) else {
        return;
    };
    let Ok(payload) = hands::reaction_payload(what.to_owned()) else {
        return;
    };
    if let Some(page) = model.kept::<RoomState>().page.borrow().clone() {
        page.float(talk::ME, what);
    }
    glib::spawn_future_local(async move {
        if let Err(fault) = call
            .send_data(
                hands::REACTION_TOPIC.to_owned(),
                payload,
                Vec::new(),
                hands::REACTION_RELIABLE,
            )
            .await
        {
            log(&format!("the reaction was not sent: {fault}"));
        }
    });
}

/// Sends what is in the box (ChatPanel.tsx `send`): cleared at once and put
/// back if it did not leave, because losing what somebody typed is worse
/// than showing it twice. Nothing goes while a message is on its way, or
/// while the call is reconnecting.
pub(crate) fn send_draft(model: &AppModel) {
    let room = model.kept::<RoomState>();
    let Some(call) = call_of(model) else {
        return;
    };
    let connected = room.state.get() == Some(CallState::Connected);
    let body = {
        let mut view = room.view.borrow_mut();
        let body = view.draft.trim().to_owned();
        if body.is_empty() || view.sending || !connected {
            return;
        }
        view.draft.clear();
        view.chat_failed = false;
        view.sending = true;
        body
    };
    changed(model);
    let weak = model.downgrade();
    glib::spawn_future_local(async move {
        let sent = call.send_chat(body.clone()).await;
        let Some(model) = weak.upgrade() else {
            return;
        };
        let room = model.kept::<RoomState>();
        {
            let mut view = room.view.borrow_mut();
            view.sending = false;
            if sent.is_err() {
                // Whatever was typed since goes after it.
                view.draft = if view.draft.is_empty() {
                    body.clone()
                } else {
                    format!("{body}\n{}", view.draft)
                };
                view.chat_failed = true;
            }
        }
        match sent {
            Ok(()) => {
                let at = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|since| since.as_millis() as i64)
                    .unwrap_or_default();
                let id = format!("mine-{at}");
                if let Some(talk) = room.talk.borrow_mut().as_mut() {
                    talk.sent(&body, at, id);
                }
            }
            Err(fault) => log(&format!("the message was not sent: {fault}")),
        }
        changed(&model);
    });
}

// ---- the screen's own choices ------------------------------------------

pub(crate) fn toggle_panel(model: &AppModel, panel: Panel) {
    let room = model.kept::<RoomState>();
    let open = {
        let mut view = room.view.borrow_mut();
        view.pop = None;
        view.panel = if view.panel == Some(panel) {
            None
        } else {
            Some(panel)
        };
        if view.panel != Some(Panel::Host) {
            room.host.borrow_mut().ending = false;
        }
        view.panel
    };
    if let Some(talk) = room.talk.borrow_mut().as_mut() {
        talk.set_chat_open(open == Some(Panel::Messages));
    }
    if open == Some(Panel::Host) {
        refresh_standing(model);
    }
    changed(model);
}

pub(crate) fn toggle_pop(model: &AppModel, pop: Pop) {
    let room = model.kept::<RoomState>();
    {
        let mut view = room.view.borrow_mut();
        view.pop = if view.pop == Some(pop) {
            None
        } else {
            Some(pop)
        };
    }
    changed(model);
}

pub(crate) fn close_pop(model: &AppModel) -> bool {
    let room = model.kept::<RoomState>();
    let was = room.view.borrow_mut().pop.take();
    if was.is_some() {
        changed(model);
    }
    was.is_some()
}

/// A press on a picture: on to the stage or back off it. While the stage
/// fills the screen a press only brings the controls back (the Mac's
/// `select`): no video player leaves its full screen on a click.
pub(crate) fn press_picture(model: &AppModel, id: &str) {
    let room = model.kept::<RoomState>();
    {
        let mut view = room.view.borrow_mut();
        if view.filled {
            return;
        }
        view.pin.toggle(id);
    }
    changed(model);
}

/// A second press within the double-press interval: fills the screen with
/// the picture the first press was on, wherever that has moved (the Mac's
/// `expand`).
pub(crate) fn expand_picture(model: &AppModel, id: &str) {
    let filled = model.kept::<RoomState>().view.borrow().filled;
    if filled {
        unfill(model);
        return;
    }
    model
        .kept::<RoomState>()
        .view
        .borrow_mut()
        .pin
        .pin(Some(id.to_owned()));
    fill(model);
}

/// Fills the screen with the stage: the window's own full screen, the
/// nearest thing to the browser's.
pub(crate) fn fill(model: &AppModel) {
    let room = model.kept::<RoomState>();
    {
        let mut view = room.view.borrow_mut();
        if view.pin.pinned().is_none() {
            return;
        }
        view.filled = true;
    }
    if let Some(page) = room.page.borrow().clone() {
        page.fullscreen(true);
    }
    changed(model);
}

/// Fills the screen with the other person of a call of two, from the button
/// on their picture (Room.tsx `alone`).
pub(crate) fn fill_alone(model: &AppModel, id: &str) {
    {
        let room = model.kept::<RoomState>();
        let mut view = room.view.borrow_mut();
        view.pin.pin(Some(id.to_owned()));
        view.pinned_to_fill = Some(id.to_owned());
    }
    fill(model);
}

pub(crate) fn unfill(model: &AppModel) {
    let room = model.kept::<RoomState>();
    {
        let mut view = room.view.borrow_mut();
        view.filled = false;
        if let Some(id) = view.pinned_to_fill.take() {
            if view.pin.pinned() == Some(id.as_str()) {
                view.pin.pin(None);
            }
        }
    }
    if let Some(page) = room.page.borrow().clone() {
        page.fullscreen(false);
    }
    changed(model);
}

pub(crate) fn turn_page(model: &AppModel, by: i32) {
    let room = model.kept::<RoomState>();
    {
        let mut view = room.view.borrow_mut();
        view.page = (view.page as i32 + by).max(0) as usize;
    }
    changed(model);
}

// ---- hearing ------------------------------------------------------------

/// A person's volume, from the sound panel.
pub(crate) fn set_hearing(model: &AppModel, identity: &str, hearing: Hearing) {
    model
        .kept::<RoomState>()
        .view
        .borrow_mut()
        .hearing
        .insert(identity.to_owned(), hearing);
    apply_hearing(model, identity);
    changed(model);
}

/// Puts a person's chosen volume on what is playing of them: their voice
/// through the devices, and their shared sound and music at the server too
/// (`Call::set_hearing`, which also turns the lossless streams standing in
/// for them), as hearing.ts does.
fn apply_hearing(model: &AppModel, identity: &str) {
    let room = model.kept::<RoomState>();
    let hearing = room
        .view
        .borrow()
        .hearing
        .get(identity)
        .copied()
        .unwrap_or_default();
    let audible = if hearing.blocked { 0.0 } else { hearing.level };
    if let Some(media) = media_of(model) {
        media.set_volume(identity, audible);
    }
    let Some(call) = call_of(model) else {
        return;
    };
    let shares_sound = room.talk.borrow().as_ref().and_then(|talk| {
        let person = talk.person(identity)?;
        Some((
            person
                .track(tomoshibi_core::call::MediaSource::ScreenShareAudio)
                .is_some(),
            person.music_track().is_some(),
        ))
    });
    let Some((screen, music)) = shares_sound else {
        return;
    };
    if hearing == Hearing::default() && !screen && !music {
        return;
    }
    let identity = identity.to_owned();
    glib::spawn_future_local(async move {
        for (has, is_music) in [(screen, false), (music, true)] {
            if has {
                call.clone()
                    .set_hearing(identity.clone(), is_music, hearing.level, hearing.blocked)
                    .await;
            }
        }
    });
}

// ---- running the room ----------------------------------------------------

/// Asks the deployment again who runs the room (host.ts `useStanding`).
fn refresh_standing(model: &AppModel) {
    let (Some(call), Some(entry)) = (call_of(model), model.call_entry()) else {
        return;
    };
    let room_name = entry.grant.room.clone();
    let deployment = entry.deployment.clone();
    let weak = model.downgrade();
    glib::spawn_future_local(async move {
        let said = deployment.host_standing(room_name, call.token()).await;
        let Some(model) = weak.upgrade() else {
            return;
        };
        if let Ok(said) = said {
            let room = model.kept::<RoomState>();
            let mut host = room.host.borrow_mut();
            if !said.yours {
                host.waiting.clear();
                host.link = None;
            }
            host.standing = Some(said);
            drop(host);
            changed(&model);
        }
    });
}

/// Looks at the door every four seconds while this participant may open
/// it (AtTheDoor.tsx `LOOK_EVERY`).
fn watch_the_door(model: &AppModel) {
    let weak = model.downgrade();
    glib::timeout_add_local(Duration::from_secs(4), move || {
        let Some(model) = weak.upgrade() else {
            return glib::ControlFlow::Break;
        };
        let room = model.kept::<RoomState>();
        let Some(call) = call_of(&model) else {
            return glib::ControlFlow::Break;
        };
        if !room.host.borrow().yours() {
            return glib::ControlFlow::Continue;
        }
        let Some(entry) = model.call_entry() else {
            return glib::ControlFlow::Break;
        };
        let weak = model.downgrade();
        glib::spawn_future_local(async move {
            // A round that did not answer is not an empty door.
            let Ok(said) = entry
                .deployment
                .knocking(entry.grant.room.clone(), call.token())
                .await
            else {
                return;
            };
            if let Some(model) = weak.upgrade() {
                let room = model.kept::<RoomState>();
                let before = room.host.borrow().waiting.len();
                let now = said.len();
                room.host.borrow_mut().waiting = said;
                if now > before && room.view.borrow().panel != Some(Panel::Host) {
                    announce(
                        &model,
                        Toast {
                            kind: ToastKind::Signal,
                            key: Some("door".into()),
                            lasts_ms: None,
                            closable: true,
                            ..Toast::said(if now == 1 {
                                t("1 person")
                            } else {
                                t_with(
                                    "{count} people are waiting to be let in",
                                    &[("count", &now)],
                                )
                            })
                        },
                    );
                }
                changed(&model);
            }
        });
        glib::ControlFlow::Continue
    });
}

/// One host action at a time, and what went wrong said in words.
fn host_act<F>(model: &AppModel, who: &str, work: F)
where
    F: std::future::Future<Output = Result<Option<String>, tomoshibi_protocol::ProtocolError>>
        + 'static,
{
    let room = model.kept::<RoomState>();
    if room.host.borrow().busy.is_some() {
        return;
    }
    room.host.borrow_mut().busy = Some(who.to_owned());
    changed(model);
    let weak = model.downgrade();
    glib::spawn_future_local(async move {
        let done = work.await;
        let Some(model) = weak.upgrade() else {
            return;
        };
        model.kept::<RoomState>().host.borrow_mut().busy = None;
        match done {
            Ok(Some(said)) => announce(&model, Toast::said(said)),
            Ok(None) => {}
            Err(fault) => {
                log(&format!("a host action failed: {fault}"));
                failed(&model, crate::app::explain(&fault));
            }
        }
        refresh_standing(&model);
        changed(&model);
    });
}

fn host_parts(
    model: &AppModel,
) -> Option<(Arc<tomoshibi_protocol::api::Deployment>, String, String)> {
    let call = call_of(model)?;
    let entry = model.call_entry()?;
    Some((
        entry.deployment.clone(),
        entry.grant.room.clone(),
        call.token(),
    ))
}

/// Makes an invitation to this room and copies it, since whoever pressed
/// for one is about to paste it somewhere.
pub(crate) fn invite(model: &AppModel) {
    let Some((deployment, room_name, bearer)) = host_parts(model) else {
        return;
    };
    let weak = model.downgrade();
    host_act(model, "invite", async move {
        let made = deployment
            .clone()
            .create_invite(room_name.clone(), bearer)
            .await?;
        let link = deployment.invite_link(made.token, room_name);
        if let Some(model) = weak.upgrade() {
            copy(&link);
            model.kept::<RoomState>().host.borrow_mut().link = Some(link);
            mark_copied(&model);
        }
        Ok(None)
    });
}

pub(crate) fn revoke(model: &AppModel) {
    let Some((deployment, room_name, bearer)) = host_parts(model) else {
        return;
    };
    let weak = model.downgrade();
    host_act(model, "revoke", async move {
        let gone = deployment.revoke_invites(room_name, Some(bearer)).await?;
        if let Some(model) = weak.upgrade() {
            model.kept::<RoomState>().host.borrow_mut().link = None;
        }
        // Said, because "nothing happened" and "three links stopped
        // working" look the same otherwise.
        Ok(Some(if gone == 1 {
            t("One link stopped working.")
        } else {
            t_with("{count} links stopped working.", &[("count", &gone)])
        }))
    });
}

/// Quietens somebody's microphone from the server, not by asking them.
pub(crate) fn silence(model: &AppModel, identity: &str) {
    let Some((deployment, room_name, bearer)) = host_parts(model) else {
        return;
    };
    let track = model
        .kept::<RoomState>()
        .talk
        .borrow()
        .as_ref()
        .and_then(|talk| {
            talk.person(identity)?
                .track(tomoshibi_core::call::MediaSource::Microphone)
                .map(|track| track.sid.clone())
        });
    let Some(track) = track else {
        return;
    };
    let identity = identity.to_owned();
    host_act(model, &identity.clone(), async move {
        deployment
            .silence(room_name, bearer, identity, track)
            .await?;
        Ok(None)
    });
}

pub(crate) fn turn_out(model: &AppModel, identity: &str) {
    let Some((deployment, room_name, bearer)) = host_parts(model) else {
        return;
    };
    let identity = identity.to_owned();
    host_act(model, &identity.clone(), async move {
        deployment.turn_out(room_name, bearer, identity).await?;
        Ok(None)
    });
}

pub(crate) fn hand_over(model: &AppModel, identity: &str) {
    let Some((deployment, room_name, bearer)) = host_parts(model) else {
        return;
    };
    let identity = identity.to_owned();
    host_act(model, &identity.clone(), async move {
        deployment.hand_over(room_name, bearer, identity).await?;
        Ok(None)
    });
}

pub(crate) fn answer_knock(model: &AppModel, id: &str, admit: bool) {
    let Some((deployment, room_name, bearer)) = host_parts(model) else {
        return;
    };
    let (weak, id) = (model.downgrade(), id.to_owned());
    host_act(model, &id.clone(), async move {
        deployment
            .answer_knock(room_name, id.clone(), bearer, admit)
            .await?;
        // Taken off the list now rather than at the next round.
        if let Some(model) = weak.upgrade() {
            model
                .kept::<RoomState>()
                .host
                .borrow_mut()
                .waiting
                .retain(|knocker| knocker.id != id);
        }
        Ok(None)
    });
}

/// Ends the meeting for everybody: asked once rather than done at once,
/// since it cannot be undone (HostPanel.tsx `ending`).
pub(crate) fn end_for_everybody(model: &AppModel) {
    let Some((deployment, room_name, bearer)) = host_parts(model) else {
        return;
    };
    host_act(model, "close", async move {
        deployment.dissolve(room_name, bearer).await?;
        Ok(None)
    });
}

pub(crate) fn ask_to_end(model: &AppModel, asking: bool) {
    model.kept::<RoomState>().host.borrow_mut().ending = asking;
    changed(model);
}

// ---- the room's link ----------------------------------------------------

/// Whether a plain link to this room lets people in (useJoining.ts
/// `useLinkWorks`): only where anybody may use a room that exists, and
/// never for a room under a scope.
pub(crate) fn link_works(model: &AppModel) -> bool {
    let joined_by = model
        .info()
        .map(|info| info.joined_by)
        .unwrap_or(Joining::Anyone);
    let room_name = model
        .call_entry()
        .map(|entry| entry.grant.room)
        .unwrap_or_default();
    joined_by == Joining::Anyone && scope_of(room_name).is_empty()
}

/// The room's plain address, `{base}/#/{room}`, which is what the web
/// copies from its address bar.
pub(crate) fn room_link(model: &AppModel) -> Option<String> {
    let entry = model.call_entry()?;
    let base = entry.deployment.base_url();
    Some(format!(
        "{}/#/{}",
        base.trim_end_matches('/'),
        entry.grant.room
    ))
}

pub(crate) fn copy_link(model: &AppModel) {
    if let Some(link) = room_link(model) {
        copy(&link);
        mark_copied(model);
    }
}

fn copy(text: &str) {
    if let Some(display) = gtk::gdk::Display::default() {
        display.clipboard().set_text(text);
    }
}

/// "Link copied" for a moment and a half, then the button again.
fn mark_copied(model: &AppModel) {
    let room = model.kept::<RoomState>();
    room.view.borrow_mut().copied = true;
    changed(model);
    let weak = model.downgrade();
    glib::timeout_add_local_once(Duration::from_millis(1600), move || {
        if let Some(model) = weak.upgrade() {
            model.kept::<RoomState>().view.borrow_mut().copied = false;
            changed(&model);
        }
    });
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

    #[test]
    fn panels_and_popovers_are_named_as_the_mac_names_them() {
        assert_eq!(Panel::of("messages"), Some(Panel::Messages));
        assert_eq!(Panel::of("host"), Some(Panel::Host));
        assert_eq!(Panel::of("watch"), None);
        assert_eq!(Pop::of("share.screen"), Some(Pop::Share));
        assert_eq!(Pop::of("reactions"), Some(Pop::Reactions));
    }
}
