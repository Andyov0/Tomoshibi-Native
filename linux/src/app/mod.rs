//! The app's model: which deployment it talks to and what it has said, who is
//! signed in, which screen is showing and why, and the preferences, as
//! windows/Tomoshibi/Model/AppModel.cs and Android's AppModel hold them.
//!
//! Everything here lives on GTK's main thread. The core's async calls run on
//! the core's own tokio runtime (`tomoshibi_protocol::runtime::on_core`), and
//! a future awaiting one is polled from GLib's main context with
//! `glib::spawn_future_local`, so a result comes back on the main thread
//! without any locking here. Nothing the core calls back on its own threads
//! (call events, frames, sound) may touch the model or a widget directly: it
//! is sent to the main thread first (`glib::MainContext::default()
//! .invoke(...)` or a channel), as render/ and ui/room do.
//!
//! The screens never poll. They [`AppModel::subscribe`] and are told what
//! changed ([`Change`]), and read the model's state again.
//!
//! Owned by the app module; the screens use only what is `pub` here, and a
//! screen that needs more asks for it here rather than reaching into the
//! core itself, so that one place decides what is kept and what is shown.
//!
//! A module's own state that outlives its screens (the room's call, the
//! update offer, the settings' open page) need not be added here: it keeps
//! it in [`AppModel::kept`], one value per type of its own, and says it
//! changed with [`AppModel::announce`] and a [`Change::Module`] named for
//! it. The screens are thrown away and built again (a change of language
//! does it), so nothing a screen must not lose may live in a widget.

pub mod account;
pub mod client;
pub mod joining;
pub mod kept;
pub mod launch;
pub mod links;
pub mod lobby;
pub mod network;
pub mod prefs;
pub mod remote;
pub mod schedule;
pub mod secrets;
pub mod updates;
pub mod words;

use std::{
    any::{Any, TypeId},
    cell::{Cell, RefCell},
    collections::HashMap,
    rc::Rc,
    sync::Arc,
};

use adw::prelude::*;
use tomoshibi_core::call::{CallOptions, IceServer};
use tomoshibi_protocol::{
    api::{Account, Arrangement, Deployment, DeploymentInfo, InviteInfo, JoinGrant},
    relays::RelayList,
    ProtocolError,
};

use crate::media::{Camera, Microphone};

pub use joining::{Gone, Waiting};
pub use kept::{DeviceMemory, DeviceSlot};
pub use launch::Launch;
pub use prefs::Preferences;
pub use secrets::Secrets;

use crate::i18n::t;

/// The screen the main window shows (AppModel.cs `Screen`, with signing in
/// named for itself rather than read off the lobby with nobody signed in).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Screen {
    /// The deployment the app opens on is being asked about; nothing to fill
    /// in yet.
    Starting,
    /// It did not answer; the one way on is to ask again.
    Unreachable,
    /// Nobody is signed in: an account's name and passphrase, or a link.
    SignIn,
    /// Signed in: the sidebar and stage, start, join and arrange.
    Lobby,
    /// Before a call: the preview, the name, the devices.
    Prejoin,
    /// Asking somebody inside to let you in.
    Knocking,
    /// Waiting for an arranged meeting's host.
    Waiting,
    Call,
    Ended,
}

impl Screen {
    /// A name for the screen, as the window's stack and the log call it and
    /// as `TOMOSHIBI_SHOW` names it.
    pub fn name(self) -> &'static str {
        match self {
            Screen::Starting => "starting",
            Screen::Unreachable => "unreachable",
            Screen::SignIn => "signin",
            Screen::Lobby => "lobby",
            Screen::Prejoin => "prejoin",
            Screen::Knocking => "knocking",
            Screen::Waiting => "waiting",
            Screen::Call => "call",
            Screen::Ended => "ended",
        }
    }

    /// The call and its ending are drawn dark in every appearance.
    pub fn is_call(self) -> bool {
        matches!(self, Screen::Call | Screen::Ended)
    }

    /// The screens before a call that show the picture of yourself.
    pub fn before_call(self) -> bool {
        matches!(
            self,
            Screen::Lobby | Screen::Prejoin | Screen::Knocking | Screen::Waiting
        )
    }
}

/// Why the last call ended (the Mac's AppModel `Ending`, App.tsx's `ended`
/// handler): endings that look alike from the inside and want different
/// sentences, since somebody told only "You left the call." after the host
/// closed the room reads the call as having broken.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ending {
    /// They pressed leave.
    Left,
    /// The room was closed for everybody (ROOM_DELETED).
    Closed,
    /// They alone were taken out (PARTICIPANT_REMOVED).
    Removed,
    /// Anything else: the connection failed and the client gave up; the
    /// detail is for the log.
    Lost(String),
}

/// Everything the room needs to open a call, handed from the screens before
/// it (the pre-join, the knock, the wait) to the room's screen through the
/// model: the deployment's grant, the word if the call is sealed, the name,
/// and the devices the pre-join already opened, so that the camera the
/// person just saw on the preview is the one the call sends.
#[derive(Clone)]
pub struct CallEntry {
    pub deployment: Arc<Deployment>,
    pub grant: JoinGrant,
    /// The sealing word, when the room is sealed (secrecy.ts).
    pub sealing_word: Option<String>,
    /// The name others see.
    pub name: String,
    pub microphone_on: bool,
    pub camera_on: bool,
    pub microphone: Option<Rc<dyn Microphone>>,
    pub camera: Option<Rc<dyn Camera>>,
}

impl CallEntry {
    /// The options `tomoshibi_core::call::join_call` takes, made from the
    /// grant as AppModel.cs makes them (the forward relay as an ICE server,
    /// an empty word as none).
    pub fn options(&self) -> CallOptions {
        CallOptions {
            url: self.grant.url.clone(),
            token: self.grant.token.clone(),
            room: self.grant.room.clone(),
            sealing_word: self.sealing_word.clone().filter(|word| !word.is_empty()),
            forward: self.grant.forward.as_ref().map(|forward| IceServer {
                url: forward.url.clone(),
                username: forward.username.clone(),
                credential: forward.credential.clone(),
            }),
        }
    }
}

/// What changed, for a screen to read again.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change {
    Screen,
    /// A request started or finished; buttons that send one are disabled
    /// while [`AppModel::busy`].
    Busy,
    /// The sentence to show for something that went wrong, or none.
    Problem,
    Account,
    /// The deployment answered what it is (`DeploymentInfo`).
    Info,
    /// The deployment's remote configuration: notices, laid-over phrases,
    /// feature switches.
    Remote,
    /// A newer build was found, or a download moved.
    Update,
    /// Something a module keeps in [`AppModel::kept`] changed, by the
    /// module's own name for it ("room", "settings", "update/offer"); only
    /// that module's screens read it.
    Module(&'static str),
}

/// The model's own change for what the screens before a call fill in and
/// choose (the room, the name, the devices to start with, the relay, the
/// invitation, the wait and the knock), which the skeleton's `Change` has
/// no case for.
pub const CHANGED: Change = Change::Module("app");

/// A subscription, to give back with [`AppModel::unsubscribe`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Subscription(u64);

type Listener = Rc<dyn Fn(&AppModel, Change)>;

#[derive(Default)]
struct State {
    screen: Option<Screen>,
    asking: u32,
    problem: Option<String>,
    /// The problem is about why somebody is not in a meeting, and stays
    /// until closed (notices.ts `joinFailed`); otherwise it fades.
    problem_stays: bool,
    /// Moved on by every problem said, so that a fade only takes the one it
    /// was set for.
    problem_said: u64,
    deployment: Option<Arc<Deployment>>,
    info: Option<DeploymentInfo>,
    account: Option<Account>,
    /// A link the app was opened with, or handed by a second launch, to be
    /// opened once the deployment has answered.
    opened_with: Option<String>,
    /// The call being entered or in, for the room's screen.
    call: Option<CallEntry>,
    /// How the last call ended, for the ended screen.
    ending: Option<Ending>,
    /// What the screens before a call hold (joining.rs).
    room: String,
    name: String,
    passphrase: String,
    sealing_word: String,
    invite: Option<String>,
    invitation: Option<InviteInfo>,
    arrived_by_link: bool,
    relays: Option<RelayList>,
    relay_choice: Option<String>,
    start_microphone: bool,
    start_camera: bool,
    joining: bool,
    join_attempt: u64,
    arrangement: Option<Arrangement>,
    meeting_token: Option<String>,
    waiting: Waiting,
    wait_task: u64,
    knock_task: u64,
    /// Moved on by every new picture, so the old one is not shown from a
    /// cache under the same address.
    picture_version: u32,
    started: bool,
}

struct Inner {
    launch: Launch,
    preferences: Preferences,
    secrets: Box<dyn Secrets>,
    state: RefCell<State>,
    listeners: RefCell<Vec<(u64, Listener)>>,
    next: Cell<u64>,
    /// What the modules keep, one value per type (`kept`).
    kept: RefCell<HashMap<TypeId, Rc<dyn Any>>>,
    /// The camera opened for the picture of yourself before a call: the
    /// lobby's stage, the sheets and the pre-join show it, and the call
    /// takes it over (`CallEntry::camera`) rather than opening it again.
    preview: RefCell<Option<Rc<dyn Camera>>>,
}

/// The model, cheap to clone: every clone is the same model.
#[derive(Clone)]
pub struct AppModel {
    inner: Rc<Inner>,
}

impl AppModel {
    /// The model for this run, with the standard preferences and secret
    /// store (neither kept in a rehearsal). Made after `client::tell`: the
    /// first deployment it makes reads the client's description then.
    pub fn new(launch: Launch) -> AppModel {
        let keeps = !launch.rehearsal();
        let preferences = Preferences::standard(keeps);
        let secrets = secrets::standard(launch.rehearsal());
        let model = Self::with_parts(launch, preferences, secrets);
        // What the deployment said last time applies before the first
        // screen is drawn, as the language does.
        remote::restore(&model);
        model
    }

    /// A model with the parts given, for tests.
    pub fn with_parts(
        launch: Launch,
        preferences: Preferences,
        secrets: Box<dyn Secrets>,
    ) -> AppModel {
        let joining = DeviceMemory::joining(&preferences);
        let state = State {
            name: preferences.string(joining::NAME_KEY).unwrap_or_default(),
            // A relay the person picked on their own deployment means
            // nothing on a test's.
            relay_choice: if launch.rehearsal() {
                None
            } else {
                preferences.string(joining::RELAY_KEY)
            },
            start_microphone: launch.microphone.unwrap_or(joining.microphone),
            start_camera: launch.camera.unwrap_or(joining.camera),
            ..State::default()
        };
        let model = AppModel {
            inner: Rc::new(Inner {
                launch,
                preferences,
                secrets,
                state: RefCell::new(state),
                listeners: RefCell::new(Vec::new()),
                next: Cell::new(1),
                kept: RefCell::new(HashMap::new()),
                preview: RefCell::new(None),
            }),
        };
        // TOMOSHIBI_MUTE: the microphone starts off for this run.
        if model.launch().mute.as_deref().is_some_and(|mute| mute != "speakers") {
            model.inner.state.borrow_mut().start_microphone = false;
        }
        // The camera seen before a call is let go of once nothing before a
        // call shows it, unless the call has taken it.
        model.subscribe(|model, change| {
            if change == Change::Screen && !model.screen().before_call() {
                model.set_preview_camera(None);
            }
        });
        model
    }

    pub fn launch(&self) -> &Launch {
        &self.inner.launch
    }

    pub fn preferences(&self) -> &Preferences {
        &self.inner.preferences
    }

    pub fn secrets(&self) -> &dyn Secrets {
        self.inner.secrets.as_ref()
    }

    // ---- what the screens read --------------------------------------------

    pub fn screen(&self) -> Screen {
        self.inner.state.borrow().screen.unwrap_or(Screen::Starting)
    }

    /// Whether a request is out; what sends one is disabled meanwhile.
    pub fn busy(&self) -> bool {
        self.inner.state.borrow().asking > 0
    }

    pub fn problem(&self) -> Option<String> {
        self.inner.state.borrow().problem.clone()
    }

    /// Whether the problem shown stays until somebody closes it, rather than
    /// fading by itself.
    pub fn problem_stays(&self) -> bool {
        self.inner.state.borrow().problem_stays
    }

    pub fn deployment(&self) -> Option<Arc<Deployment>> {
        self.inner.state.borrow().deployment.clone()
    }

    pub fn info(&self) -> Option<DeploymentInfo> {
        self.inner.state.borrow().info.clone()
    }

    pub fn account(&self) -> Option<Account> {
        self.inner.state.borrow().account.clone()
    }

    /// The call being entered or in.
    pub fn call_entry(&self) -> Option<CallEntry> {
        self.inner.state.borrow().call.clone()
    }

    /// How the last call ended.
    pub fn ending(&self) -> Option<Ending> {
        self.inner.state.borrow().ending.clone()
    }

    /// The deployment's address, as the core writes it, or the one the app
    /// opens on before it has answered.
    pub fn base_url(&self) -> String {
        self.deployment()
            .map(|deployment| deployment.base_url())
            .unwrap_or_else(|| self.launch().deployment_base())
    }

    /// A module's own state, one value per type, made with `Default` the
    /// first time it is asked for and the same value every time after, for
    /// as long as the model lives. The type is the key, so a module uses a
    /// type of its own (`struct RoomState { .. }` in ui/room), with `Cell`s
    /// and `RefCell`s inside for what changes.
    pub fn kept<T: Default + 'static>(&self) -> Rc<T> {
        let made = self
            .inner
            .kept
            .borrow_mut()
            .entry(TypeId::of::<T>())
            .or_insert_with(|| Rc::new(T::default()) as Rc<dyn Any>)
            .clone();
        // The entry for a type is only ever made from that type above.
        made.downcast::<T>().expect("kept under its own type")
    }

    // ---- being told -------------------------------------------------------

    /// Calls `listener` on the main thread after every change.
    pub fn subscribe(&self, listener: impl Fn(&AppModel, Change) + 'static) -> Subscription {
        let id = self.inner.next.get();
        self.inner.next.set(id + 1);
        self.inner
            .listeners
            .borrow_mut()
            .push((id, Rc::new(listener)));
        Subscription(id)
    }

    pub fn unsubscribe(&self, subscription: Subscription) {
        self.inner
            .listeners
            .borrow_mut()
            .retain(|(id, _)| *id != subscription.0);
    }

    /// [`subscribe`](Self::subscribe) for as long as `widget` lives: the
    /// subscription ends when it is destroyed, so a screen that is thrown
    /// away is not kept alive, or told anything, by the model.
    pub fn subscribe_while(
        &self,
        widget: &impl IsA<gtk::Widget>,
        listener: impl Fn(&AppModel, Change) + 'static,
    ) {
        let subscription = self.subscribe(listener);
        let model = self.downgrade();
        widget.as_ref().connect_destroy(move |_| {
            if let Some(model) = model.upgrade() {
                model.unsubscribe(subscription);
            }
        });
    }

    pub fn downgrade(&self) -> WeakModel {
        WeakModel(Rc::downgrade(&self.inner))
    }

    /// Tells every listener of a change; what a module calls after changing
    /// what it keeps ([`Change::Module`]). The model's own changes are said
    /// by the model.
    pub fn announce(&self, change: Change) {
        self.changed(change);
    }

    fn changed(&self, change: Change) {
        // Copied out first, so that a listener may subscribe, unsubscribe or
        // change the model again without the list being borrowed twice.
        let listeners: Vec<Listener> = self
            .inner
            .listeners
            .borrow()
            .iter()
            .map(|(_, listener)| listener.clone())
            .collect();
        for listener in listeners {
            listener(self, change);
        }
    }

    // ---- what the screens do ----------------------------------------------

    /// Moves to `screen`, and says so if it is another. The lobby is
    /// signing in for somebody who is not signed in: the room's "Back to
    /// meetings" asks for the lobby whoever pressed it.
    pub fn show(&self, screen: Screen) {
        let screen = if screen == Screen::Lobby && self.account().is_none() {
            Screen::SignIn
        } else {
            screen
        };
        let before = self.inner.state.borrow_mut().screen.replace(screen);
        if before != Some(screen) {
            self.changed(Change::Screen);
        }
    }

    /// Shows a sentence about why somebody is not where they meant to be,
    /// which stays until they close it (notices.ts `joinFailed`): an
    /// invitation that opens tomorrow said when, and was gone in five
    /// seconds while the person was still reading it.
    pub fn tell(&self, sentence: impl Into<String>) {
        {
            let mut state = self.inner.state.borrow_mut();
            state.problem = Some(sentence.into());
            state.problem_stays = true;
            state.problem_said += 1;
        }
        self.changed(Change::Problem);
    }

    /// Shows a sentence about something that happened and is over, which
    /// fades by itself after ten seconds (notices.ts `actionFailed`).
    pub fn mention(&self, sentence: impl Into<String>) {
        let said = {
            let mut state = self.inner.state.borrow_mut();
            state.problem = Some(sentence.into());
            state.problem_stays = false;
            state.problem_said += 1;
            state.problem_said
        };
        self.changed(Change::Problem);
        let weak = self.downgrade();
        glib::timeout_add_seconds_local_once(10, move || {
            let Some(model) = weak.upgrade() else {
                return;
            };
            let still = model.inner.state.borrow().problem_said == said;
            if still {
                model.clear_problem();
            }
        });
    }

    pub fn clear_problem(&self) {
        let had = {
            let mut state = self.inner.state.borrow_mut();
            state.problem_stays = false;
            state.problem.take().is_some()
        };
        if had {
            self.changed(Change::Problem);
        }
    }

    fn ask(&self, by: i32) {
        {
            let mut state = self.inner.state.borrow_mut();
            state.asking = state.asking.saturating_add_signed(by);
        }
        self.changed(Change::Busy);
    }

    /// Asks the deployment what it is and who is signed in, and shows the
    /// screen that follows: the lobby, signing in, or, for a deployment that
    /// does not answer, the screen that asks again. Returns at once; the
    /// answer arrives on the main thread.
    pub fn connect(&self) {
        let model = self.clone();
        glib::spawn_future_local(async move {
            let _ = model.connect_now().await;
        });
    }

    /// [`connect`](Self::connect), awaited.
    pub async fn connect_now(&self) -> Result<(), ProtocolError> {
        // Nothing here may take the screen off a call that is live; the only
        // way from a call to another deployment is to leave it first.
        if self.screen() == Screen::Call {
            return Ok(());
        }
        self.ask(1);
        let reached = self.reach().await;
        self.ask(-1);
        match &reached {
            Ok(()) => {
                let info = self.info();
                log(&format!(
                    "reached the deployment ({}): opened by {:?}, joined by {:?}, build {}",
                    if self.account().is_some() {
                        "signed in"
                    } else {
                        "nobody signed in"
                    },
                    info.as_ref().map(|info| info.opened_by),
                    info.as_ref().map(|info| info.joined_by),
                    info.as_ref()
                        .map(|info| info.build.as_str())
                        .filter(|build| !build.is_empty())
                        .unwrap_or("unknown"),
                ));
                self.clear_problem();
                let preparing = !self.room().is_empty() && self.screen() == Screen::Prejoin;
                if !preparing {
                    self.show(self.home());
                }
                self.refresh_relays();
                self.refresh_arrangements();
                let first = !std::mem::replace(&mut self.inner.state.borrow_mut().started, true);
                if first {
                    self.started().await;
                }
                if let Some(link) = self.inner.state.borrow_mut().opened_with.take() {
                    self.open_link(&link);
                }
            }
            Err(fault) => {
                log(&format!("the deployment did not answer: {fault}"));
                self.tell(explain(fault));
                // Opening on a deployment that will not answer leaves
                // somebody where they can try again, rather than on the mark.
                if self.screen() == Screen::Starting {
                    self.show(Screen::Unreachable);
                }
            }
        }
        reached
    }

    /// The screen somebody goes back to before a call: the lobby, or for
    /// somebody not signed in, signing in.
    pub fn home(&self) -> Screen {
        if self.account().is_some() {
            Screen::Lobby
        } else {
            Screen::SignIn
        }
    }

    /// Once, the first time the deployment answers: what runs for the life
    /// of the app, and what a test's launch asks for.
    async fn started(&self) {
        remote::start(self);
        updates::start(self);
        network::start(self);
        // The relays' light, asked again every minute while no call shows.
        let weak = self.downgrade();
        glib::timeout_add_seconds_local(60, move || match weak.upgrade() {
            Some(model) => {
                if !model.screen().is_call() {
                    model.refresh_relays();
                }
                glib::ControlFlow::Continue
            }
            None => glib::ControlFlow::Break,
        });

        let launch = self.launch().clone();
        if let Some(name) = &launch.name {
            self.inner.state.borrow_mut().name = name.clone();
        }
        if let Some((name, file)) = &launch.sign_in {
            // Read into a variable and never written anywhere: the file is
            // the test's, and the log is read over a shared folder.
            match std::fs::read_to_string(file) {
                Ok(passphrase) => {
                    self.sign_in_now(name.clone(), passphrase.trim().to_owned())
                        .await
                }
                Err(fault) => log(&format!("TOMOSHIBI_SIGNIN's file could not be read: {fault}")),
            }
        }
        if let Some(room) = launch.autojoin.clone() {
            self.autojoin(room);
        } else if let Some(room) = launch.prepare.clone() {
            self.prepare(&room);
        } else if let Some(link) = launch.open.clone() {
            self.open_link(&link);
        }
    }

    async fn reach(&self) -> Result<(), ProtocolError> {
        let base = self.launch().deployment_base();
        let deployment = Deployment::new(base)?;
        let key = secrets::session_key(&deployment.base_url());
        if let Some(cookie) = self.secrets().read(&key) {
            if deployment.restore_account_cookie(cookie).is_err() {
                log("the kept session could not be restored");
            }
        }
        let info = deployment.clone().deployment_info().await?;
        // Who is signed in is asked after the deployment has said what it is;
        // a deployment that answers the first and not the second is reached,
        // with nobody signed in, as the web reads it.
        let account = deployment.clone().me().await.unwrap_or(None);
        let passphrase = self
            .secrets()
            .read(&secrets::passphrase_key(&deployment.base_url()))
            .unwrap_or_default();
        {
            let mut state = self.inner.state.borrow_mut();
            state.deployment = Some(deployment);
            state.info = Some(info);
            state.account = account;
            if state.passphrase.is_empty() {
                state.passphrase = passphrase;
            }
        }
        self.changed(Change::Info);
        self.changed(Change::Account);
        Ok(())
    }

    /// Signs in with an account's name and passphrase, keeps the session, and
    /// moves to the lobby; a refusal is shown as its sentence.
    pub fn sign_in(&self, name: String, passphrase: String) {
        let model = self.clone();
        glib::spawn_future_local(async move {
            model.sign_in_now(name, passphrase).await;
        });
    }

    /// [`sign_in`](Self::sign_in), awaited.
    pub async fn sign_in_now(&self, name: String, passphrase: String) {
        let Some(deployment) = self.deployment() else {
            return;
        };
        if self.busy() || name.trim().is_empty() || passphrase.is_empty() {
            return;
        }
        self.ask(1);
        let result = deployment
            .clone()
            .sign_in(name.trim().to_owned(), passphrase)
            .await;
        self.ask(-1);
        match result {
            Ok(account) => {
                if let Some(cookie) = deployment.account_cookie() {
                    self.secrets()
                        .write(&secrets::session_key(&deployment.base_url()), &cookie);
                }
                log("signed in");
                self.inner.state.borrow_mut().account = Some(account);
                self.changed(Change::Account);
                self.clear_problem();
                self.refresh_arrangements();
                self.refresh_relays();
                if self.screen() == Screen::SignIn {
                    self.show(Screen::Lobby);
                }
            }
            Err(fault) => {
                log(&format!("sign-in refused: {fault}"));
                self.mention(explain_sign_in(&fault));
            }
        }
    }

    /// Hands a call to the room and shows it: what the pre-join (or the
    /// knock, or the wait) calls once the deployment has granted the join.
    pub fn enter_call(&self, entry: CallEntry) {
        let room = entry.grant.room.clone();
        {
            let mut state = self.inner.state.borrow_mut();
            state.call = Some(entry);
            state.ending = None;
            state.joining = false;
            state.room = room.clone();
        }
        self.remember_room(&room);
        self.clear_problem();
        self.show(Screen::Call);
    }

    /// The room says the call is over, and why; the ended screen follows.
    pub fn end_call(&self, ending: Ending) {
        log(&format!("the call ended: {ending:?}"));
        let lost = matches!(ending, Ending::Lost(_));
        {
            let mut state = self.inner.state.borrow_mut();
            state.call = None;
            state.ending = Some(ending);
        }
        if lost {
            self.forget_relay_measurement();
        }
        self.show(Screen::Ended);
    }

    // ---- the devices a call starts with ------------------------------------

    pub fn start_with_microphone(&self) -> bool {
        self.inner.state.borrow().start_microphone
    }

    pub fn start_with_camera(&self) -> bool {
        self.inner.state.borrow().start_camera
    }

    /// What starts on when somebody joins (preferences.ts `meet-live.devices`),
    /// written as it is switched, so that the settings and the join screens
    /// are one choice and not two that disagree.
    pub fn set_start_with_microphone(&self, on: bool) {
        self.inner.state.borrow_mut().start_microphone = on;
        self.remember_devices();
    }

    pub fn set_start_with_camera(&self, on: bool) {
        self.inner.state.borrow_mut().start_camera = on;
        self.remember_devices();
    }

    fn remember_devices(&self) {
        let joining = kept::Joining {
            camera: self.start_with_camera(),
            microphone: self.start_with_microphone(),
        };
        DeviceMemory::remember_joining(self.preferences(), joining);
        self.changed(CHANGED);
    }

    /// The device last picked of a kind, or none for the system's.
    pub fn device_choice(&self, slot: DeviceSlot) -> Option<String> {
        DeviceMemory::choice(self.preferences(), slot)
    }

    pub fn choose_device(&self, slot: DeviceSlot, id: Option<&str>) {
        DeviceMemory::remember_choice(self.preferences(), slot, id);
        self.changed(CHANGED);
    }

    /// The devices picked are forgotten: from now on, the system's.
    pub fn forget_devices(&self) {
        DeviceMemory::forget_choices(self.preferences());
        self.changed(CHANGED);
    }

    pub fn original_sound(&self) -> bool {
        DeviceMemory::original_sound(self.preferences())
    }

    pub fn set_original_sound(&self, on: bool) {
        DeviceMemory::set_original_sound(self.preferences(), on);
        self.changed(CHANGED);
    }

    /// Whether the background is blurred: what was chosen, where the
    /// deployment offers it (Remote.cs `Blur`).
    pub fn blur(&self) -> bool {
        DeviceMemory::blur(self.preferences())
            && remote::allows(self, remote::feature::BACKGROUND_BLUR)
    }

    pub fn blur_chosen(&self) -> bool {
        DeviceMemory::blur(self.preferences())
    }

    pub fn set_blur(&self, on: bool) {
        DeviceMemory::set_blur(self.preferences(), on);
        if let Some(camera) = self.preview_camera() {
            camera.set_blur(self.blur());
        }
        self.changed(CHANGED);
    }

    /// The camera showing the picture of yourself before a call.
    pub fn preview_camera(&self) -> Option<Rc<dyn Camera>> {
        self.inner.preview.borrow().clone()
    }

    /// Holds `camera` as the preview, letting go of the one before.
    pub fn set_preview_camera(&self, camera: Option<Rc<dyn Camera>>) {
        let old = self.inner.preview.replace(camera.clone());
        if let Some(old) = old {
            let same = camera.as_ref().is_some_and(|new| Rc::ptr_eq(new, &old));
            if !same {
                old.close();
            }
        }
    }

    /// The preview handed over to a call, which keeps it from here.
    pub fn take_preview_camera(&self) -> Option<Rc<dyn Camera>> {
        self.inner.preview.borrow_mut().take()
    }

    /// Opens a page in the person's browser: the deployment's own page for
    /// a download, a meeting's link.
    pub fn open_in_browser(&self, uri: &str) {
        let launcher = gtk::UriLauncher::new(uri);
        let window = gtk::Window::list_toplevels()
            .into_iter()
            .find_map(|widget| widget.downcast::<gtk::Window>().ok());
        launcher.launch(window.as_ref(), gio::Cancellable::NONE, |result| {
            if let Err(fault) = result {
                log(&format!("the browser could not be opened: {fault}"));
            }
        });
    }
}

/// A model that does not keep it alive.
#[derive(Clone)]
pub struct WeakModel(std::rc::Weak<Inner>);

impl WeakModel {
    pub fn upgrade(&self) -> Option<AppModel> {
        self.0.upgrade().map(|inner| AppModel { inner })
    }
}

/// A link pasted or typed, if it is one: http, https or the app's own scheme,
/// with a host (Pieces.kt `pastedLink`, SignIn.cs `PastedLink`).
pub fn pasted_link(text: &str) -> Option<String> {
    let trimmed = text.trim();
    let (scheme, rest) = trimmed.split_once("://")?;
    let scheme = scheme.to_ascii_lowercase();
    if !matches!(scheme.as_str(), "https" | "http" | "tomoshibi") {
        return None;
    }
    let host = rest.split(['/', '?', '#']).next().unwrap_or_default();
    (!host.is_empty() && !host.contains(char::is_whitespace)).then(|| trimmed.to_owned())
}

/// What a failure to reach the deployment reads as (api.ts `explain`).
pub fn explain(fault: &ProtocolError) -> String {
    match fault {
        ProtocolError::Network { .. } => t("Could not reach the server."),
        ProtocolError::Refused { code, .. } if code == "rate_limited" => {
            t("Too many attempts. Try again in a moment.")
        }
        _ => t("Something went wrong. Try again."),
    }
}

/// What a failed sign-in reads as. The web reads every refusal of a sign-in,
/// whatever its code, as the pair not going together (live/account.ts
/// `signIn` throws `not_yours` for any answer that is not ok), so a refusal
/// here says the same; only a deployment that did not answer says so.
pub fn explain_sign_in(fault: &ProtocolError) -> String {
    match fault {
        ProtocolError::Refused { .. } => t("That name and passphrase do not go together."),
        other => explain(other),
    }
}

/// One line of the app's log, on standard error and, in a debug build given
/// `TOMOSHIBI_DEBUG_LOG`, in that file too.
pub fn log(line: &str) {
    eprintln!("tomoshibi: {line}");
    #[cfg(debug_assertions)]
    if let Ok(path) = std::env::var("TOMOSHIBI_DEBUG_LOG") {
        use std::io::Write as _;
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
        {
            let _ = writeln!(file, "{line}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model() -> AppModel {
        AppModel::with_parts(
            Launch::default(),
            Preferences::in_memory(),
            Box::<secrets::InMemory>::default(),
        )
    }

    #[test]
    fn listeners_are_told_once_per_change_and_can_leave() {
        let model = model();
        let told = Rc::new(RefCell::new(Vec::new()));
        let kept = told.clone();
        let subscription = model.subscribe(move |_, change| kept.borrow_mut().push(change));
        model.show(Screen::SignIn);
        model.show(Screen::SignIn);
        model.tell("no");
        model.unsubscribe(subscription);
        model.show(Screen::Lobby);
        assert_eq!(*told.borrow(), vec![Change::Screen, Change::Problem]);
    }

    #[test]
    fn a_module_keeps_one_value_of_its_own_type() {
        #[derive(Default)]
        struct Counted(Cell<u32>);
        #[derive(Default)]
        struct Other(Cell<u32>);

        let model = model();
        model.kept::<Counted>().0.set(3);
        assert_eq!(model.kept::<Counted>().0.get(), 3, "the same value again");
        assert_eq!(
            model.kept::<Other>().0.get(),
            0,
            "another type, another value"
        );
        let clone = model.clone();
        assert_eq!(
            clone.kept::<Counted>().0.get(),
            3,
            "every clone is the model"
        );

        let told = Rc::new(RefCell::new(Vec::new()));
        let kept = told.clone();
        model.subscribe(move |_, change| kept.borrow_mut().push(change));
        model.announce(Change::Module("room"));
        assert_eq!(*told.borrow(), vec![Change::Module("room")]);
    }

    #[test]
    fn a_link_is_one_with_a_scheme_we_open_and_a_host() {
        assert!(pasted_link(" https://meet.example.com/?invite=abc#/room ").is_some());
        assert!(pasted_link("tomoshibi://meet.example.com/?invite=abc").is_some());
        assert!(pasted_link("ftp://meet.example.com/").is_none());
        assert!(pasted_link("https:///nothing").is_none());
        assert!(pasted_link("meet.example.com").is_none());
    }

    #[test]
    fn a_wrong_passphrase_reads_as_a_sentence_not_a_code() {
        let refused = ProtocolError::Refused {
            status: 403,
            code: "not_yours".into(),
        };
        assert_eq!(
            explain_sign_in(&refused),
            "That name and passphrase do not go together."
        );
        let gone = ProtocolError::Network {
            detail: "refused".into(),
        };
        assert_eq!(explain(&gone), "Could not reach the server.");
    }
}
