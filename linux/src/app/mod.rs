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

pub mod client;
pub mod launch;
pub mod prefs;
pub mod secrets;

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
};

use adw::prelude::*;
use tomoshibi_core::call::{CallOptions, IceServer};
use tomoshibi_protocol::{
    api::{Account, Deployment, DeploymentInfo, JoinGrant},
    ProtocolError,
};

use crate::media::{Camera, Microphone};

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
/// and the devices the pre-join already opened, so that the microphone the
/// person just heard on the meter is the one the call sends.
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
}

/// A subscription, to give back with [`AppModel::unsubscribe`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Subscription(u64);

type Listener = Rc<dyn Fn(&AppModel, Change)>;

#[derive(Default)]
struct State {
    screen: Option<Screen>,
    asking: u32,
    problem: Option<String>,
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
}

struct Inner {
    launch: Launch,
    preferences: Preferences,
    secrets: Box<dyn Secrets>,
    state: RefCell<State>,
    listeners: RefCell<Vec<(u64, Listener)>>,
    next: Cell<u64>,
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
        Self::with_parts(launch, preferences, secrets)
    }

    /// A model with the parts given, for tests.
    pub fn with_parts(
        launch: Launch,
        preferences: Preferences,
        secrets: Box<dyn Secrets>,
    ) -> AppModel {
        AppModel {
            inner: Rc::new(Inner {
                launch,
                preferences,
                secrets,
                state: RefCell::new(State::default()),
                listeners: RefCell::new(Vec::new()),
                next: Cell::new(1),
            }),
        }
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

    /// Moves to `screen`, and says so if it is another.
    pub fn show(&self, screen: Screen) {
        let before = self.inner.state.borrow_mut().screen.replace(screen);
        if before != Some(screen) {
            self.changed(Change::Screen);
        }
    }

    /// Shows a sentence for something that went wrong.
    pub fn tell(&self, sentence: impl Into<String>) {
        self.inner.state.borrow_mut().problem = Some(sentence.into());
        self.changed(Change::Problem);
    }

    pub fn clear_problem(&self) {
        if self.inner.state.borrow_mut().problem.take().is_some() {
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
                self.show(if self.account().is_some() {
                    Screen::Lobby
                } else {
                    Screen::SignIn
                });
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
        {
            let mut state = self.inner.state.borrow_mut();
            state.deployment = Some(deployment);
            state.info = Some(info);
            state.account = account;
        }
        self.changed(Change::Info);
        self.changed(Change::Account);
        Ok(())
    }

    /// Signs in with an account's name and passphrase, keeps the session, and
    /// moves to the lobby; a refusal is shown as its sentence.
    pub fn sign_in(&self, name: String, passphrase: String) {
        let Some(deployment) = self.deployment() else {
            return;
        };
        if self.busy() || name.trim().is_empty() || passphrase.is_empty() {
            return;
        }
        let model = self.clone();
        glib::spawn_future_local(async move {
            model.ask(1);
            let result = deployment
                .clone()
                .sign_in(name.trim().to_owned(), passphrase)
                .await;
            model.ask(-1);
            match result {
                Ok(account) => {
                    if let Some(cookie) = deployment.account_cookie() {
                        model
                            .secrets()
                            .write(&secrets::session_key(&deployment.base_url()), &cookie);
                    }
                    model.inner.state.borrow_mut().account = Some(account);
                    model.changed(Change::Account);
                    model.clear_problem();
                    model.show(Screen::Lobby);
                }
                Err(fault) => model.tell(explain_sign_in(&fault)),
            }
        });
    }

    /// Hands a call to the room and shows it: what the pre-join (or the
    /// knock, or the wait) calls once the deployment has granted the join.
    pub fn enter_call(&self, entry: CallEntry) {
        {
            let mut state = self.inner.state.borrow_mut();
            state.call = Some(entry);
            state.ending = None;
        }
        self.show(Screen::Call);
    }

    /// The room says the call is over, and why; the ended screen follows.
    pub fn end_call(&self, ending: Ending) {
        log(&format!("the call ended: {ending:?}"));
        {
            let mut state = self.inner.state.borrow_mut();
            state.call = None;
            state.ending = Some(ending);
        }
        self.show(Screen::Ended);
    }

    /// Opens a link somebody was sent: an invitation, a meeting, or a room
    /// on this deployment. Before the deployment has answered it is kept and
    /// opened after. The lobby module fills in what each kind of link does.
    pub fn open_link(&self, link: &str) {
        if self.deployment().is_none() {
            self.inner.state.borrow_mut().opened_with = Some(link.to_owned());
            return;
        }
        log("a link was handed to the app; opening links is not written yet");
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
