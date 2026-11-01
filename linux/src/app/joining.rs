//! Getting into a call: the room and the name, a link opened, the join
//! itself, the knock on a door that turned somebody away, and the wait for an
//! arranged meeting's host. The web's App.tsx, PreJoin.tsx, Knocking.tsx and
//! Waiting.tsx, as AppModel.cs and Android's AppModel.kt follow them decision
//! for decision.
//!
//! The deployment's grant is asked for here; the call itself is the room's
//! (`AppModel::enter_call` hands it the grant).

use tomoshibi_protocol::{
    api::{
        host_not_coming_after_ms, knock_ask_every_ms, knock_give_up_after_ms,
        meeting_ask_every_ms, Arrangement, DoorState, InviteInfo, Joining, Opening,
    },
    identity::{looks_generated, normalise_room_name},
    relays::RelayList,
    ProtocolError,
};

use super::{
    kept::KeptIdentity, links::OpenedLink, log, secrets, updates, words, AppModel, CallEntry,
    Screen, CHANGED,
};
use crate::i18n::{t, t_with};

/// The web's own storage name for the name typed, so the habit carries over.
pub const NAME_KEY: &str = "meet-live.name";
/// A relay chosen by hand (api.ts `rememberRelay`).
pub const RELAY_KEY: &str = "meet-live.relay";

/// How an arranged meeting's wait stands (Waiting.tsx).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Waiting {
    /// Asking at the web's pace whether the host has begun.
    pub asking: bool,
    pub gone: Option<Gone>,
    /// The last read did not get through; reading again.
    pub unreachable: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gone {
    Cancelled,
    Ended,
    NotComing,
}

impl AppModel {
    // ---- what the screens before a call hold -------------------------------

    pub fn room(&self) -> String {
        self.inner.state.borrow().room.clone()
    }

    /// Only ever what the person typed: an invitation's name is
    /// [`invited_name`](Self::invited_name) and is never written here
    /// (PreJoin.tsx: "Not a name somebody else chose for them").
    pub fn name(&self) -> String {
        self.inner.state.borrow().name.clone()
    }

    pub fn set_name(&self, name: &str) {
        if self.inner.state.borrow().name == name {
            return;
        }
        self.inner.state.borrow_mut().name = name.to_owned();
        self.preferences().set(NAME_KEY, name);
        self.changed(CHANGED);
    }

    pub fn passphrase(&self) -> String {
        self.inner.state.borrow().passphrase.clone()
    }

    /// A guest's passphrase, kept in the secret store for this deployment.
    pub fn set_passphrase(&self, passphrase: &str) {
        if self.inner.state.borrow().passphrase == passphrase {
            return;
        }
        self.inner.state.borrow_mut().passphrase = passphrase.to_owned();
        let key = secrets::passphrase_key(&self.base_url());
        if passphrase.is_empty() {
            self.secrets().remove(&key);
        } else {
            self.secrets().write(&key, passphrase);
        }
        self.changed(CHANGED);
    }

    /// Never stored, as the web never stores it (secrecy.ts).
    pub fn sealing_word(&self) -> String {
        self.inner.state.borrow().sealing_word.clone()
    }

    pub fn set_sealing_word(&self, word: &str) {
        self.inner.state.borrow_mut().sealing_word = word.to_owned();
        self.changed(CHANGED);
    }

    pub fn invite(&self) -> Option<String> {
        self.inner.state.borrow().invite.clone()
    }

    pub fn invitation(&self) -> Option<InviteInfo> {
        self.inner.state.borrow().invitation.clone()
    }

    /// The name an invitation was made for, which the guest will wear
    /// whatever is typed (PreJoin.tsx `locked`).
    pub fn invited_name(&self) -> Option<String> {
        self.invitation()
            .and_then(|invitation| invitation.name)
            .filter(|name| !name.is_empty())
    }

    /// Somebody who arrived on an invitation link, who has a name to choose
    /// and nothing else to answer.
    pub fn arrived_invited(&self) -> bool {
        self.inner.state.borrow().invitation.is_some()
    }

    /// The screen before a call was reached by a link rather than the
    /// lobby's own join: kept until the person goes back to the lobby, so a
    /// join that is refused comes back to the same screen with the same
    /// picker on it.
    pub fn arrived_by_link(&self) -> bool {
        self.inner.state.borrow().arrived_by_link
    }

    /// The name this person will be called in the room: the account's, the
    /// invitation's, or what they typed.
    pub fn joining_name(&self) -> String {
        self.account()
            .map(|account| account.name)
            .or_else(|| self.invited_name())
            .unwrap_or_else(|| self.name().trim().to_owned())
    }

    /// A join is out and has not yet landed in its call.
    pub fn joining(&self) -> bool {
        self.inner.state.borrow().joining
    }

    // ---- the relays --------------------------------------------------------

    pub fn relays(&self) -> Option<RelayList> {
        self.inner.state.borrow().relays.clone()
    }

    /// Asks again how much of the fleet is answering, for the light in the
    /// sidebar's foot (Lobby.tsx `Fleet`), and which relays there are.
    pub fn refresh_relays(&self) {
        let Some(deployment) = self.deployment() else {
            return;
        };
        let weak = self.downgrade();
        glib::spawn_future_local(async move {
            let Ok(relays) = deployment.relays().await else {
                return;
            };
            let Some(model) = weak.upgrade() else {
                return;
            };
            let changed = model.inner.state.borrow().relays.as_ref() != Some(&relays);
            model.inner.state.borrow_mut().relays = Some(relays);
            model.drop_relay_not_offered();
            if changed {
                model.changed(CHANGED);
                model.changed(super::lobby::CHANGED);
            }
        });
    }

    /// None picks whichever relay answers fastest.
    pub fn relay_choice(&self) -> Option<String> {
        self.inner.state.borrow().relay_choice.clone()
    }

    pub fn set_relay_choice(&self, choice: Option<String>) {
        self.inner.state.borrow_mut().relay_choice = choice.clone();
        if !self.launch().rehearsal() {
            match &choice {
                Some(name) => self.preferences().set(RELAY_KEY, name),
                None => self.preferences().remove(RELAY_KEY),
            }
        }
        self.changed(CHANGED);
    }

    /// A relay remembered that this deployment does not offer, or no longer
    /// takes calls on, would be sent with every join and refused.
    fn drop_relay_not_offered(&self) {
        let (Some(choice), Some(relays)) = (self.relay_choice(), self.relays()) else {
            return;
        };
        if !relays
            .relays
            .iter()
            .any(|relay| relay.name == choice && !relay.maintenance)
        {
            self.set_relay_choice(None);
        }
    }

    /// Whether there is a machine to choose at all (PreJoin.tsx
    /// `offerChoice`): more than one relay, on a deployment that acts on
    /// the choice.
    pub fn offers_relay_choice(&self) -> bool {
        self.relays()
            .is_some_and(|relays| relays.measure && relays.relays.len() > 1)
    }

    /// Whether the join screens offer the server picker: where there is a
    /// choice, to somebody who did not come to them through the lobby,
    /// whose start and join sheet has its own (the owner, 2026-10-19).
    pub fn offers_relay_choice_at_the_door(&self) -> bool {
        self.offers_relay_choice() && (self.account().is_none() || self.arrived_by_link())
    }

    // ---- the door's words ------------------------------------------------

    /// Whether the person could ask to be let in instead (PreJoin.tsx ~521).
    pub fn can_knock(&self) -> bool {
        words::can_knock(self.problem().as_deref(), &self.room())
    }

    /// RoomTitle.tsx: what this name is worth at this deployment's door,
    /// said before anybody presses Join. Not to somebody holding an
    /// invitation, nor to somebody signed in, whose door the session
    /// answers.
    pub fn door(&self) -> Option<String> {
        if self.account().is_some() || self.arrived_invited() {
            return None;
        }
        let info = self.info()?;
        let room = self.room();
        let scope = words::scope_of(&room);
        if !scope.is_empty() {
            return Some(t_with(
                "Only members of {scope} can join this room.",
                &[("scope", &scope)],
            ));
        }
        match info.joined_by {
            Joining::Accounts => return Some(t("Sign in to join a room here.")),
            Joining::Invited => {
                return Some(t(
                    "Rooms here are by invitation. Open the link you were sent, or sign in.",
                ))
            }
            Joining::Anyone => {}
        }
        match info.opened_by {
            Opening::Admins => Some(t(
                "Only administrators can start new rooms. Enter the name you were given.",
            )),
            Opening::Signed => Some(t(
                "Set a passphrase to start a new room. Anybody can join one that already exists.",
            )),
            _ => (!looks_generated(room)).then(|| t("Anyone who guesses this name can join.")),
        }
    }

    /// The room's address where a plain address is a way in: only where
    /// anybody may come in by the name, and never for a room held under a
    /// scope (Screens.kt `RoomHeading`).
    pub fn room_link(&self) -> Option<String> {
        let room = self.room();
        let info = self.info()?;
        if info.joined_by != Joining::Anyone || room.is_empty() || !words::scope_of(&room).is_empty() {
            return None;
        }
        let base = self.deployment()?.base_url();
        let base = if base.ends_with('/') { base } else { format!("{base}/") };
        Some(format!("{base}#/{room}"))
    }

    /// PreJoin.tsx's one line under the identity, which changes rather than
    /// doubling: what a passphrase is for until there is one, and what it
    /// does once there is.
    pub fn identity_line(&self) -> String {
        if let Some(invited) = self.invited_name() {
            return t_with("You were invited to this room as {name}.", &[("name", &invited)]);
        }
        if self.arrived_invited() {
            return t("You were invited to this room. Choose a name and go in.");
        }
        if !self.passphrase().is_empty() {
            let name = self.joining_name();
            let name = if name.is_empty() { "?".to_owned() } else { name };
            return t_with("Only you can join as {name}", &[("name", &name)]);
        }
        t("Add a passphrase to keep your name, and to run the room you open.")
    }

    /// When the link stops working, in the reader's own clock.
    pub fn link_window(&self) -> Option<String> {
        if !self.arrived_invited() {
            return None;
        }
        let invitation = self.invitation()?;
        let until = words::instant(invitation.until.as_deref()?)?;
        Some(match invitation.from.as_deref().and_then(words::instant) {
            Some(from) => t_with(
                "This link works from {from} until {until}.",
                &[("from", &words::said(from)), ("until", &words::said(until))],
            ),
            None => t_with("This link works until {until}.", &[("until", &words::said(until))]),
        })
    }

    // ---- joining -----------------------------------------------------------

    /// Refuses to begin a call while a build is being put in place, which
    /// ends the app, and says why.
    fn refused_for_update(&self) -> bool {
        if !updates::holds_calls(self) {
            return false;
        }
        log("refused a call: an update is being installed");
        self.mention(t(
            "Tomoshibi is installing an update and will open again in a moment. Start or join the meeting then.",
        ));
        true
    }

    /// The pre-join for a room by its name. Only somebody signed in, or
    /// holding an invitation, is taken to a room by its name: the app is a
    /// way in for the people who belong to the deployment and for the people
    /// they send links to (the owner's rule, 2026-10-15).
    pub fn prepare(&self, room: &str) {
        if self.screen() == Screen::Call || self.refused_for_update() {
            return;
        }
        if self.account().is_none() && self.invite().is_none() && !self.launch().rehearsal() {
            self.tell(t(
                "Rooms here are by invitation. Open the link you were sent, or sign in.",
            ));
            return;
        }
        self.inner.state.borrow_mut().room = normalise_room_name(room.trim().to_owned());
        self.clear_problem();
        self.changed(CHANGED);
        self.show(Screen::Prejoin);
    }

    /// Straight into a room by name from the start and join sheet, which has
    /// already taken the choices the pre-join would.
    pub async fn enter(&self, room: &str) {
        if self.account().is_none() && self.invite().is_none() && !self.launch().rehearsal() {
            self.tell(t(
                "Rooms here are by invitation. Open the link you were sent, or sign in.",
            ));
            return;
        }
        self.inner.state.borrow_mut().room = normalise_room_name(room.trim().to_owned());
        self.clear_problem();
        self.join_now(None).await;
    }

    /// Joins the room held, from the pre-join or the wait.
    pub fn join(&self) {
        let model = self.clone();
        glib::spawn_future_local(async move {
            model.join_now(None).await;
        });
    }

    /// Calls off a join that has not yet landed: Back on the pre-join,
    /// Cancel on the start and join sheet, leaving the door or the wait.
    pub fn cancel_join(&self) {
        let mut state = self.inner.state.borrow_mut();
        if state.joining {
            state.join_attempt += 1;
        }
    }

    /// Asks the deployment for the room held and hands the grant to the
    /// room. `relay` is the relay to ask for on this join alone, in place of
    /// the one chosen.
    pub async fn join_now(&self, relay: Option<String>) {
        let Some(deployment) = self.deployment() else {
            return;
        };
        if self.joining() || self.refused_for_update() {
            return;
        }
        let attempt = {
            let mut state = self.inner.state.borrow_mut();
            state.joining = true;
            state.join_attempt
        };
        self.changed(CHANGED);
        self.ask(1);
        let room = self.room();
        let name = self.joining_name();
        // Signed in, the web sends the account's name and no passphrase: the
        // session is the proof (api.ts). A guest of a room held under a scope
        // is minted a mark of their own and a passphrase is ignored, so none
        // is sent (PreJoin.tsx `scopedGuest`).
        let signed = self.account().is_some();
        let scoped_guest = self.arrived_invited() && !words::scope_of(&room).is_empty();
        let passphrase = if signed || scoped_guest {
            String::new()
        } else {
            self.passphrase()
        };
        let base = deployment.base_url();
        let kept = KeptIdentity::read(self.preferences(), self.secrets(), &base);
        log(&format!("joining {room}"));
        let granted = deployment
            .clone()
            .join(
                room.clone(),
                name.clone(),
                passphrase,
                kept.identity,
                kept.proof,
                relay.or_else(|| self.relay_choice()).unwrap_or_default(),
                self.invite(),
            )
            .await;
        self.ask(-1);
        let called_off = self.inner.state.borrow().join_attempt != attempt;
        let grant = match granted {
            Ok(grant) => grant,
            Err(fault) => {
                self.inner.state.borrow_mut().joining = false;
                self.changed(CHANGED);
                // Why they are not in the meeting, which stays until read; a
                // join that was called off has nothing to say.
                if !called_off {
                    log(&format!(
                        "the join was refused: {}",
                        words::code(&fault).unwrap_or("not answered")
                    ));
                    self.tell(words::explain_join(&fault, &room));
                }
                return;
            }
        };
        // Every join's identity is kept, with its proof or with none:
        // signing in or out makes the server issue a new one, and an issued
        // one sent back without its proof is replaced by a fresh one.
        KeptIdentity {
            identity: Some(grant.identity.clone()).filter(|identity| !identity.is_empty()),
            proof: grant.proof.clone(),
        }
        .keep(self.preferences(), self.secrets(), &base);
        if called_off {
            self.inner.state.borrow_mut().joining = false;
            self.changed(CHANGED);
            return;
        }
        log(&format!("granted the room {}", grant.room));
        let camera_on = self.start_with_camera();
        let camera = if camera_on {
            self.take_preview_camera()
        } else {
            None
        };
        let sealing_word = Some(self.sealing_word()).filter(|word| !word.is_empty());
        self.enter_call(CallEntry {
            deployment,
            grant,
            sealing_word,
            name,
            microphone_on: self.start_with_microphone(),
            camera_on,
            microphone: None,
            camera,
        });
    }

    /// `TOMOSHIBI_AUTOJOIN`, in a debug run: joins `room` as soon as the
    /// deployment has answered, under `TOMOSHIBI_NAME` (or "Linux"), straight
    /// into the call, with the devices `TOMOSHIBI_MICROPHONE` and
    /// `TOMOSHIBI_CAMERA` say (off where they say nothing). What the interop
    /// scripts and the headless runs use to put the app in a room.
    pub fn autojoin(&self, room: String) {
        let name = self
            .launch()
            .name
            .clone()
            .unwrap_or_else(|| "Linux".to_owned());
        let passphrase = self
            .launch()
            .passphrase_file
            .as_ref()
            .and_then(|file| std::fs::read_to_string(file).ok())
            .map(|text| text.trim().to_owned())
            .unwrap_or_default();
        {
            let mut state = self.inner.state.borrow_mut();
            state.name = name;
            state.passphrase = passphrase;
            state.invite = self.launch().invite.clone();
            state.room = normalise_room_name(room.trim().to_owned());
            state.start_microphone = self.launch().microphone.unwrap_or(false);
            state.start_camera = self.launch().camera.unwrap_or(false);
        }
        self.join();
    }

    /// Back to the lobby from any screen before a call, calling off a join
    /// that is out.
    pub fn back_to_lobby(&self) {
        if self.screen() == Screen::Call {
            return;
        }
        self.cancel_join();
        {
            let mut state = self.inner.state.borrow_mut();
            state.arrived_by_link = false;
            state.invite = None;
            state.invitation = None;
            state.wait_task += 1;
            state.knock_task += 1;
            state.arrangement = None;
            state.meeting_token = None;
            state.waiting = Waiting::default();
        }
        self.clear_problem();
        self.changed(CHANGED);
        self.show(self.home());
    }

    /// "Join again", after a call: the pre-join for the room just left.
    pub fn join_again(&self) {
        let room = self.room();
        if room.is_empty() {
            self.back_to_lobby();
        } else {
            self.show(Screen::Prejoin);
        }
    }

    // ---- a link ------------------------------------------------------------

    /// Opens a link somebody was sent: an invitation, an arranged meeting or
    /// a room (`https://<deployment>/?invite=T#/room` and its kin). Not over
    /// a call. Before the deployment has answered it is kept and opened
    /// after.
    pub fn open_link(&self, link: &str) {
        if self.deployment().is_none() {
            self.inner.state.borrow_mut().opened_with = Some(link.to_owned());
            return;
        }
        let model = self.clone();
        let link = link.to_owned();
        glib::spawn_future_local(async move {
            model.open_link_now(&link).await;
        });
    }

    async fn open_link_now(&self, link: &str) {
        if self.screen() == Screen::Call {
            self.mention(t("Leave this call to open that link."));
            return;
        }
        if self.refused_for_update() {
            return;
        }
        self.cancel_join();
        let Some(parts) = OpenedLink::parse(link) else {
            return;
        };
        let Some(deployment) = self.deployment() else {
            return;
        };
        // A link to another server is refused rather than followed: the app
        // is for this deployment alone.
        if !parts.is_for(&deployment.base_url()) {
            self.tell(t("That link is for another server."));
            return;
        }
        if let Some(meeting) = parts.meeting.clone() {
            {
                let mut state = self.inner.state.borrow_mut();
                if !parts.room.is_empty() {
                    state.room = parts.room.clone();
                }
                state.arrived_by_link = true;
            }
            self.wait(meeting);
            return;
        }
        if let Some(invite) = parts.invite.clone() {
            let said = deployment.clone().invite_info(invite.clone()).await.ok();
            // An invitation that is no good is still somebody sent here on
            // purpose, and they are told which of four things it is, because
            // they are different next moves (App.tsx).
            let error = match &said {
                None => Some("no_such_invite".to_owned()),
                Some(info) => info.error.clone(),
            };
            if let Some(error) = error {
                {
                    let mut state = self.inner.state.borrow_mut();
                    state.invite = None;
                    state.invitation = None;
                    state.arrived_by_link = false;
                }
                let from = said
                    .as_ref()
                    .and_then(|info| info.from.as_deref())
                    .and_then(words::instant);
                self.tell(match (error.as_str(), from) {
                    ("invite_not_yet", Some(opens)) => t_with(
                        "That invitation opens {when}. Come back then.",
                        &[("when", &words::said(opens))],
                    ),
                    ("invite_expired", _) => t("That meeting has ended."),
                    ("rate_limited", _) => t("Too many attempts. Try again in a moment."),
                    _ => t("That invitation is no longer good. Ask for another."),
                });
                self.changed(CHANGED);
                self.show(self.home());
                return;
            }
            let info = said.expect("an answer with no error");
            {
                let mut state = self.inner.state.borrow_mut();
                state.invite = Some(invite);
                state.room = info.room.clone().unwrap_or_else(|| parts.room.clone());
                state.invitation = Some(info);
            }
        } else if !parts.room.is_empty() {
            // A link with a room and nothing else is a room's name, and only
            // somebody signed in is taken anywhere by one of those.
            if self.account().is_none() {
                self.tell(t_with(
                    "{room} is by invitation. Ask the organiser for a link.",
                    &[("room", &parts.room)],
                ));
                self.inner.state.borrow_mut().arrived_by_link = false;
                self.show(self.home());
                return;
            }
            self.inner.state.borrow_mut().room = parts.room.clone();
        }
        if !self.room().is_empty() {
            self.inner.state.borrow_mut().arrived_by_link = true;
            self.clear_problem();
            self.changed(CHANGED);
            self.show(Screen::Prejoin);
        }
    }

    // ---- an arranged meeting -----------------------------------------------

    pub fn arrangement(&self) -> Option<Arrangement> {
        self.inner.state.borrow().arrangement.clone()
    }

    pub fn waiting(&self) -> Waiting {
        self.inner.state.borrow().waiting.clone()
    }

    /// Whether the meeting has begun and carries the way in, so that the
    /// button is the ordinary Join.
    pub fn arranged_ready(&self) -> bool {
        self.arrangement()
            .is_some_and(|one| one.started && one.invite.is_some())
    }

    /// When the host may begin it: the server begins a meeting only from
    /// `from`.
    pub fn arranged_opens(&self) -> Option<i64> {
        let one = self.arrangement()?;
        words::instant(one.from.as_deref().unwrap_or(&one.at))
    }

    fn set_waiting(&self, change: impl FnOnce(&mut Waiting)) {
        change(&mut self.inner.state.borrow_mut().waiting);
        self.changed(CHANGED);
    }

    /// Waiting.tsx, a pre-join whose button waits: read on arrival so the
    /// screen can say what it is for, and again every thirty seconds until
    /// it can. Nothing is asked of the meeting but what it is until somebody
    /// presses.
    pub fn wait(&self, token: String) {
        let task = {
            let mut state = self.inner.state.borrow_mut();
            state.wait_task += 1;
            state.meeting_token = Some(token);
            state.arrangement = None;
            state.waiting = Waiting::default();
            state.wait_task
        };
        self.clear_problem();
        self.changed(CHANGED);
        self.show(Screen::Waiting);
        let weak = self.downgrade();
        glib::spawn_future_local(async move {
            loop {
                let Some(model) = weak.upgrade() else {
                    return;
                };
                if model.inner.state.borrow().wait_task != task {
                    return;
                }
                if model.look().await.is_some() || model.waiting().gone.is_some() {
                    return;
                }
                drop(model);
                glib::timeout_future_seconds(30).await;
            }
        });
    }

    /// Reads the meeting once.
    async fn look(&self) -> Option<Arrangement> {
        let deployment = self.deployment()?;
        let token = self.inner.state.borrow().meeting_token.clone()?;
        match deployment.arranged(token).await {
            Ok(said) => {
                {
                    let mut state = self.inner.state.borrow_mut();
                    state.room = said.room.clone();
                    state.arrangement = Some(said.clone());
                    state.waiting.unreachable = false;
                    if said.ended {
                        state.waiting.gone = Some(Gone::Ended);
                    }
                }
                self.changed(CHANGED);
                Some(said)
            }
            Err(ProtocolError::Refused { code, .. }) if code == "no_such_meeting" => {
                self.set_waiting(|waiting| waiting.gone = Some(Gone::Cancelled));
                None
            }
            Err(_) => {
                self.set_waiting(|waiting| waiting.unreachable = true);
                None
            }
        }
    }

    /// The one button: join where it has begun, begin it where it is the
    /// host's, otherwise wait for the host.
    pub fn go_arranged(&self) {
        let Some(one) = self.arrangement() else {
            return;
        };
        if self.arranged_ready() {
            self.inner.state.borrow_mut().invite = one.invite.clone();
            self.join();
            return;
        }
        // The host does not wait for the host.
        if one.mine {
            self.join();
            return;
        }
        self.ask_for_arranged();
    }

    /// Asks at the web's pace until the meeting begins (then joins with the
    /// invitation it carries), ends, or the host is two hours late.
    fn ask_for_arranged(&self) {
        let task = {
            let mut state = self.inner.state.borrow_mut();
            state.wait_task += 1;
            state.waiting.asking = true;
            state.waiting.gone = None;
            state.wait_task
        };
        self.changed(CHANGED);
        let weak = self.downgrade();
        glib::spawn_future_local(async move {
            loop {
                let Some(model) = weak.upgrade() else {
                    return;
                };
                let said = model.look().await;
                if model.inner.state.borrow().wait_task != task {
                    return;
                }
                if said.as_ref().is_some_and(|one| one.ended) || model.waiting().gone.is_some() {
                    model.set_waiting(|waiting| waiting.asking = false);
                    return;
                }
                if let Some(one) = said.as_ref().filter(|one| one.started && one.invite.is_some()) {
                    model.inner.state.borrow_mut().invite = one.invite.clone();
                    model.set_waiting(|waiting| waiting.asking = false);
                    model.join_now(None).await;
                    return;
                }
                let now_ms = words::now() * 1000;
                if let Some(at) = said.as_ref().and_then(|one| words::instant(&one.at)) {
                    if now_ms - at * 1000 > host_not_coming_after_ms() as i64 {
                        model.set_waiting(|waiting| {
                            waiting.asking = false;
                            waiting.gone = Some(Gone::NotComing);
                        });
                        return;
                    }
                }
                // A read that failed is not a reason to ask faster.
                let pause = said
                    .map(|one| meeting_ask_every_ms(one.at, now_ms))
                    .unwrap_or(30_000);
                drop(model);
                glib::timeout_future(std::time::Duration::from_millis(pause)).await;
            }
        });
    }

    /// "Ask again", after the host was said not to be coming.
    pub fn ask_again(&self) {
        self.ask_for_arranged();
    }

    pub fn stop_waiting(&self) {
        self.back_to_lobby();
    }

    // ---- the door ----------------------------------------------------------

    /// Asks the room's host to let this person in, then waits, under the
    /// name the room will know them by.
    pub fn knock(&self) {
        let Some(deployment) = self.deployment() else {
            return;
        };
        let from = self.screen();
        let model = self.clone();
        glib::spawn_future_local(async move {
            let room = model.room();
            let ticket = match deployment.clone().knock(room.clone(), model.joining_name()).await {
                Ok(ticket) => ticket,
                Err(fault) => {
                    log(&format!("the knock was not sent: {fault}"));
                    if model.screen() == from {
                        model.mention(t("That could not be sent."));
                    }
                    return;
                }
            };
            // Somebody who went back while the knock was being sent is not
            // put at the door afterwards.
            if model.screen() != from {
                return;
            }
            log(&format!("knocking on {room}"));
            let task = {
                let mut state = model.inner.state.borrow_mut();
                state.knock_task += 1;
                state.knock_task
            };
            model.clear_problem();
            model.show(Screen::Knocking);
            let started = std::time::Instant::now();
            let weak = model.downgrade();
            drop(model);
            loop {
                glib::timeout_future(std::time::Duration::from_millis(knock_ask_every_ms())).await;
                let Some(model) = weak.upgrade() else {
                    return;
                };
                if model.inner.state.borrow().knock_task != task {
                    return;
                }
                // A round that did not answer is not the end of the waiting:
                // the person inside has not gone anywhere.
                let answer = deployment
                    .clone()
                    .at_the_door(room.clone(), ticket.token.clone())
                    .await
                    .ok();
                if model.inner.state.borrow().knock_task != task {
                    return;
                }
                match answer.map(|answer| (answer.state, answer.invite)) {
                    Some((DoorState::Admitted, invite)) => {
                        model.inner.state.borrow_mut().invite = invite;
                        model.join_now(None).await;
                        // Let in and then refused at the join: the reason is
                        // on the pre-join, with its button.
                        if model.screen() == Screen::Knocking {
                            model.show(Screen::Prejoin);
                        }
                        return;
                    }
                    Some((DoorState::Refused, _)) => {
                        model.tell(t("Nobody let you in. Ask for a link."));
                        model.show(Screen::Prejoin);
                        return;
                    }
                    _ => {}
                }
                // knock.ts `GIVE_UP_AFTER`, counted whatever the rounds said.
                if started.elapsed().as_millis() as u64 > knock_give_up_after_ms() {
                    model.tell(t("Nobody let you in. Ask for a link."));
                    model.show(Screen::Prejoin);
                    return;
                }
            }
        });
    }

    pub fn stop_knocking(&self) {
        self.inner.state.borrow_mut().knock_task += 1;
        // Let in a moment ago, and already joining.
        self.cancel_join();
        self.show(Screen::Prejoin);
    }

    /// The arranged meeting the lobby's row points at, opened as its link is.
    pub fn open_meeting(&self, one: &Arrangement) {
        if let Some(link) = self.meeting_link(one) {
            self.open_link(&link);
        }
    }
}
