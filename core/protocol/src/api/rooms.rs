//! A deployment's rooms: what the deployment says about itself, joining, the
//! host's controls, invitations and the waiting room.
//!
//! Follows web/src/live/api.ts (`deployment`, `join`), host.ts, knock.ts, the
//! invitation half of account.ts (`invited`, `meeting`), and the guest links of
//! web/src/account/Account.tsx. The server's side is internal/app/app.go (the
//! join, `deployment`, the error codes), host.go, invite.go and knock.go.

use std::{fmt, sync::Arc};

use reqwest::Method;
use serde::Serialize;
use serde_json::{json, Value};

use super::{
    read::{some_text, text, yes},
    text::{encode_uri_component, iso_from_ms, js_trim},
    unexpected, Api, Deployment,
};
use crate::{relays::RelayList, runtime::on_core, ProtocolError};

/// Who may use a name nobody has used before (api.ts `Opening`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum Opening {
    Anyone,
    Signed,
    Accounts,
    Admins,
}

/// Who may enter a room that already exists (api.ts `Joining`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum Joining {
    Anyone,
    Invited,
    Accounts,
}

/// What the deployment says about itself (`GET /api/deployment`).
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct DeploymentInfo {
    pub opened_by: Opening,
    pub joined_by: Joining,
    /// Where the code running there can be read; shown because the AGPL's
    /// thirteenth section obliges the deployment to offer it to its users.
    pub source: String,
    /// The commit the server was built from, empty where it does not say; see
    /// web/src/live/freshness.ts for what a client does with it.
    pub build: String,
}

impl DeploymentInfo {
    /// api.ts `PLAIN`: what every deployment was before either setting existed,
    /// and what a deployment that will not say is taken to be.
    fn plain() -> Self {
        Self {
            opened_by: Opening::Anyone,
            joined_by: Joining::Anyone,
            source: String::new(),
            build: String::new(),
        }
    }

    /// api.ts `deployment`, narrowing exactly as it does: the three opening
    /// values that exist are let through and anything else is `anyone`, and an
    /// unknown joining value is the closed one, because a page that cannot tell
    /// what the door is must not promise anybody it is open.
    fn read(body: &Value) -> Self {
        let opened_by = match body.get("openedBy").and_then(Value::as_str) {
            Some("admins") => Opening::Admins,
            Some("signed") => Opening::Signed,
            Some("accounts") => Opening::Accounts,
            _ => Opening::Anyone,
        };
        let joined_by = match body.get("joinedBy").and_then(Value::as_str) {
            Some("anyone") => Joining::Anyone,
            Some("accounts") => Joining::Accounts,
            _ => Joining::Invited,
        };

        Self {
            opened_by,
            joined_by,
            source: text(body.get("source")),
            build: text(body.get("build")),
        }
    }
}

/// Where to send media when that is not the machine holding the room.
#[derive(Clone, PartialEq, Eq, uniffi::Record)]
pub struct JoinForward {
    pub url: String,
    pub username: String,
    pub credential: String,
}

impl fmt::Debug for JoinForward {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("JoinForward")
            .field("url", &self.url)
            .field("username", &self.username)
            .finish_non_exhaustive()
    }
}

/// What the server hands back for one room (api.ts `Join`).
#[derive(Clone, PartialEq, Eq, uniffi::Record)]
pub struct JoinGrant {
    /// Where to open the signalling socket.
    pub url: String,
    /// Authorises this one room under this one identity, for a few hours; also
    /// what the host controls and the waiting room are authorised with.
    pub token: String,
    /// What this participant is known as. The app keeps it and sends it back on
    /// the next join, so a reconnect is the same person rather than a second
    /// one arriving while the first is cleaned up.
    pub identity: String,
    /// The room actually authorised, after the server lower-cased it.
    pub room: String,
    /// What to call the machine this call was sent to.
    pub relay: Option<String>,
    /// The deployment's region for the relay this client chose, where it was
    /// given that one; absent means unknown, not abroad.
    pub region: Option<String>,
    /// What to call the machine the meeting is actually on, where that is not
    /// the one dialled.
    pub holding: Option<String>,
    /// The relay to send media through, where that is not the holding machine.
    pub forward: Option<JoinForward>,
}

impl fmt::Debug for JoinGrant {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("JoinGrant")
            .field("url", &self.url)
            .field("identity", &self.identity)
            .field("room", &self.room)
            .field("relay", &self.relay)
            .field("region", &self.region)
            .field("holding", &self.holding)
            .field("forward", &self.forward)
            .finish_non_exhaustive()
    }
}

impl JoinGrant {
    fn read(body: &Value) -> Result<Self, ProtocolError> {
        let required = |key: &str| {
            body.get(key)
                .and_then(Value::as_str)
                .map(str::to_owned)
                .ok_or_else(|| unexpected(format!("a join answer without its {key}")))
        };
        let forward = body
            .get("forward")
            .filter(|value| value.is_object())
            .map(|forward| JoinForward {
                url: text(forward.get("url")),
                username: text(forward.get("username")),
                credential: text(forward.get("credential")),
            });

        Ok(Self {
            url: required("url")?,
            token: required("token")?,
            identity: required("identity")?,
            room: required("room")?,
            relay: some_text(body.get("relay")),
            region: some_text(body.get("region")),
            holding: some_text(body.get("holding")),
            forward: forward.filter(|forward| !forward.url.is_empty()),
        })
    }
}

/// What this participant may do in the room they are in (host.ts `Standing`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct HostStanding {
    /// Whether they may act, one way or another.
    pub yours: bool,
    /// And whether by being an administrator, which is worth saying differently.
    pub admin: bool,
}

/// What an invite says about itself, or why it is no good (account.ts
/// `Invitation`). Times are instants as the server wrote them.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct InviteInfo {
    pub room: Option<String>,
    /// The name the guest will be called, where whoever made the link chose one.
    pub name: Option<String>,
    /// When it begins to work. Also sent with `invite_not_yet`.
    pub from: Option<String>,
    /// When it stops; absent for a link that lasts until it is revoked. The
    /// server calls this `expires`.
    pub until: Option<String>,
    /// `no_such_invite`, `invite_expired`, `invite_not_yet`, `rate_limited`.
    pub error: Option<String>,
}

/// An invitation just made, with the link to hand out.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct MadeInvite {
    pub token: String,
    pub link: String,
    pub room: String,
    pub name: Option<String>,
    pub from: Option<String>,
    pub until: Option<String>,
}

/// What the door said about somebody standing at it (knock.ts `AtTheDoor`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum DoorState {
    Knocking,
    Admitted,
    /// Turned away, or never answered: the same thing from outside the door,
    /// and told apart nowhere, or knocking would say which rooms exist.
    Refused,
}

impl DoorState {
    /// Anything the door says that is not one of the two endings is still
    /// waiting, which is how components/room/Knocking.tsx reads it: the poll
    /// stops only on an admission carrying an invite, or on a refusal.
    fn read(value: Option<&Value>) -> Self {
        match value.and_then(Value::as_str) {
            Some("admitted") => Self::Admitted,
            Some("refused") => Self::Refused,
            _ => Self::Knocking,
        }
    }
}

/// A knock, and the token to ask about it with.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct KnockTicket {
    pub token: String,
    pub state: DoorState,
}

/// What the door said when asked.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct DoorAnswer {
    pub state: DoorState,
    /// The invite to join with, once admitted.
    pub invite: Option<String>,
}

/// One person waiting, as the room sees them (knock.ts `Knocking`).
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct Knocker {
    pub id: String,
    pub name: String,
    pub address: String,
    pub at: String,
}

/// knock.ts `ASK_EVERY`: how often the person at the door asks.
#[uniffi::export]
pub fn knock_ask_every_ms() -> u64 {
    3000
}

/// knock.ts `GIVE_UP_AFTER`: twice the longest the server holds a knock, so it
/// only ever ends the waiting for a server that never answers.
#[uniffi::export]
pub fn knock_give_up_after_ms() -> u64 {
    600_000
}

/// The join's body. Field order is the order api.ts writes the object in, and
/// a missing identity is left out rather than sent as null, as
/// `JSON.stringify` leaves out `undefined`: the body is then byte for byte the
/// one a browser sends.
#[derive(Serialize)]
struct JoinBody<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    identity: Option<&'a str>,
    name: &'a str,
    passphrase: &'a str,
    relay: &'a str,
}

fn room_path(room: &str, rest: &str) -> String {
    format!("/api/rooms/{}{rest}", encode_uri_component(room))
}

fn invite_from(api: &Api, said: &Value, room: &str) -> Result<MadeInvite, ProtocolError> {
    let token = some_text(said.get("token"))
        .ok_or_else(|| unexpected("an invite answer without its token"))?;

    Ok(MadeInvite {
        link: api.invite_link(&token, room),
        token,
        room: some_text(said.get("room")).unwrap_or_else(|| room.to_owned()),
        name: some_text(said.get("name")),
        from: some_text(said.get("from")),
        until: some_text(said.get("expires")),
    })
}

impl Api {
    async fn deployment_info(&self) -> Result<DeploymentInfo, ProtocolError> {
        let answer = self
            .send(self.request(Method::GET, "/api/deployment")?)
            .await?;

        if !answer.ok() {
            return Ok(DeploymentInfo::plain());
        }

        // A body that is not JSON, or is `null`, is the web's exception and so
        // its PLAIN; anything else is narrowed field by field.
        Ok(match answer.value() {
            None | Some(Value::Null) => DeploymentInfo::plain(),
            Some(body) => DeploymentInfo::read(&body),
        })
    }

    /// relays.ts `preferred`: the relay that measured fastest, or empty where
    /// the deployment does not want a measurement or measuring failed. Empty
    /// is a preference the server reads as "keep the room together", so a call
    /// still happens either way.
    async fn preferred_relay(&self) -> String {
        match crate::relays::fetch(&self.client, &self.base).await {
            Ok(list) => crate::relays::preferred(&list).await.unwrap_or_default(),
            Err(_) => String::new(),
        }
    }

    async fn join(
        &self,
        room: &str,
        name: &str,
        passphrase: &str,
        identity: Option<&str>,
        relay: &str,
        invite: Option<&str>,
    ) -> Result<JoinGrant, ProtocolError> {
        // Measured only where nobody chose: a relay picked by hand overrules
        // the measurement, and taking one anyway would be five rounds to every
        // relay of waiting for an answer already overruled.
        let measured;
        let relay = if relay.is_empty() {
            measured = self.preferred_relay().await;
            measured.as_str()
        } else {
            relay
        };

        // The invite in the query and the passphrase in the body, as api.ts
        // has it. An invite is a link and already in every log a link reaches;
        // a passphrase in a query would reach the access log, and nothing
        // written there can be taken back.
        let mut path = room_path(room, "/join");
        if let Some(invite) = invite.filter(|invite| !invite.is_empty()) {
            path.push_str("?invite=");
            path.push_str(&encode_uri_component(invite));
        }

        let body = serde_json::to_vec(&JoinBody {
            identity: identity.filter(|identity| !identity.is_empty()),
            name,
            passphrase,
            relay,
        })
        .map_err(|fault| unexpected(format!("the join could not be written: {fault}")))?;

        let answer = self
            .send(self.json_request(Method::POST, &path)?.body(body))
            .await?
            .accepted("")?;

        JoinGrant::read(&answer.json::<Value>("the join")?)
    }

    async fn host_standing(&self, room: &str, bearer: &str) -> Result<HostStanding, ProtocolError> {
        let answer = self
            .send(self.bearer_request(Method::GET, &room_path(room, "/host"), bearer)?)
            .await?
            .accepted("")?;
        let said = answer.json::<Value>("the host's standing")?;

        Ok(HostStanding {
            yours: yes(said.get("yours")),
            admin: yes(said.get("admin")),
        })
    }

    /// One of host.ts's controls: `fallback` is the code it throws where the
    /// server's answer carries none.
    async fn host_act(
        &self,
        method: Method,
        room: &str,
        rest: &str,
        bearer: &str,
        body: Option<String>,
        fallback: &str,
    ) -> Result<super::Answer, ProtocolError> {
        let mut request = self.bearer_request(method, &room_path(room, rest), bearer)?;
        if let Some(body) = body {
            request = request.body(body);
        }

        self.send(request).await?.accepted(fallback)
    }

    async fn invite_info(&self, token: &str) -> Result<InviteInfo, ProtocolError> {
        let answer = self
            .send(self.request(
                Method::GET,
                &format!("/api/invites/{}", encode_uri_component(token)),
            )?)
            .await?;
        let body = answer.value().unwrap_or(Value::Null);

        if answer.ok() {
            return Ok(InviteInfo {
                room: some_text(body.get("room")),
                name: some_text(body.get("name")),
                from: some_text(body.get("from")),
                until: some_text(body.get("expires")),
                error: None,
            });
        }

        Ok(InviteInfo {
            room: None,
            name: None,
            from: some_text(body.get("from")),
            until: None,
            error: Some(some_text(body.get("error")).unwrap_or_else(|| "no_such_invite".into())),
        })
    }
}

#[uniffi::export]
impl Deployment {
    /// What the deployment says about itself. Whatever it answers that is not
    /// a usable answer reads as `anyone`/`anyone`, as the web reads it; only a
    /// deployment that cannot be reached is an error.
    pub async fn deployment_info(self: Arc<Self>) -> Result<DeploymentInfo, ProtocolError> {
        on_core(async move { self.api.deployment_info().await }).await
    }

    /// The relays the deployment offers (`GET /api/relays`).
    pub async fn relays(self: Arc<Self>) -> Result<RelayList, ProtocolError> {
        on_core(async move { crate::relays::fetch(&self.api.client, &self.api.base).await }).await
    }

    /// Asks to be authorised for a room.
    ///
    /// `identity` is the one a previous join on this device returned, sent
    /// back so a reconnect is the same participant. `relay` is a relay's name
    /// somebody chose, or empty to send the fastest by measurement, which is
    /// taken here as the web client takes it. `invite` is an invite token, sent
    /// in the query; the deployment then keeps it in a cookie of its own, which
    /// this jar holds for the next join.
    ///
    /// Never retried. With a passphrase every attempt is charged to the
    /// deployment's guessing budget.
    pub async fn join(
        self: Arc<Self>,
        room: String,
        name: String,
        passphrase: String,
        identity: Option<String>,
        relay: String,
        invite: Option<String>,
    ) -> Result<JoinGrant, ProtocolError> {
        on_core(async move {
            self.api
                .join(
                    &room,
                    &name,
                    &passphrase,
                    identity.as_deref(),
                    &relay,
                    invite.as_deref(),
                )
                .await
        })
        .await
    }

    /// Whether the bearer of this join token may run the room.
    pub async fn host_standing(
        self: Arc<Self>,
        room: String,
        bearer: String,
    ) -> Result<HostStanding, ProtocolError> {
        on_core(async move { self.api.host_standing(&room, &bearer).await }).await
    }

    /// Hands the room to the participant with this identity, who must have
    /// proven their name (`not_proven` otherwise). Returns the new host's
    /// signature.
    pub async fn hand_over(
        self: Arc<Self>,
        room: String,
        bearer: String,
        to: String,
    ) -> Result<String, ProtocolError> {
        on_core(async move {
            let answer = self
                .api
                .host_act(
                    Method::POST,
                    &room,
                    "/host",
                    &bearer,
                    Some(json!({ "to": to }).to_string()),
                    "handover_failed",
                )
                .await?;
            Ok(answer
                .value()
                .map(|said| text(said.get("host")))
                .unwrap_or_default())
        })
        .await
    }

    /// Quietens one microphone track from the server, without asking its owner.
    pub async fn silence(
        self: Arc<Self>,
        room: String,
        bearer: String,
        identity: String,
        track: String,
    ) -> Result<(), ProtocolError> {
        on_core(async move {
            #[derive(Serialize)]
            struct Mute<'a> {
                identity: &'a str,
                track: &'a str,
            }
            let body = serde_json::to_string(&Mute {
                identity: &identity,
                track: &track,
            })
            .map_err(|fault| unexpected(format!("the mute could not be written: {fault}")))?;
            self.api
                .host_act(
                    Method::POST,
                    &room,
                    "/mute",
                    &bearer,
                    Some(body),
                    "mute_failed",
                )
                .await
                .map(drop)
        })
        .await
    }

    /// Removes somebody from the room.
    pub async fn turn_out(
        self: Arc<Self>,
        room: String,
        bearer: String,
        identity: String,
    ) -> Result<(), ProtocolError> {
        on_core(async move {
            let rest = format!("/people/{}", encode_uri_component(&identity));
            self.api
                .host_act(Method::DELETE, &room, &rest, &bearer, None, "remove_failed")
                .await
                .map(drop)
        })
        .await
    }

    /// Ends the meeting for everybody, and every link to it with it.
    pub async fn dissolve(
        self: Arc<Self>,
        room: String,
        bearer: String,
    ) -> Result<(), ProtocolError> {
        on_core(async move {
            self.api
                .host_act(Method::POST, &room, "/close", &bearer, None, "close_failed")
                .await
                .map(drop)
        })
        .await
    }

    /// Moves the meeting to another relay without ending it. Returns what to
    /// call the relay it went to.
    pub async fn move_room(
        self: Arc<Self>,
        room: String,
        bearer: String,
        relay: String,
    ) -> Result<String, ProtocolError> {
        on_core(async move {
            let answer = self
                .api
                .host_act(
                    Method::PUT,
                    &room,
                    "/relay",
                    &bearer,
                    Some(json!({ "relay": relay }).to_string()),
                    "move_failed",
                )
                .await?;
            Ok(answer
                .value()
                .map(|said| text(said.get("relay")))
                .unwrap_or_default())
        })
        .await
    }

    /// Makes a link that lets people into this meeting for the rest of it, or
    /// a day (host.ts `invite`).
    pub async fn create_invite(
        self: Arc<Self>,
        room: String,
        bearer: String,
    ) -> Result<MadeInvite, ProtocolError> {
        on_core(async move {
            let answer = self
                .api
                .host_act(
                    Method::POST,
                    &room,
                    "/invites",
                    &bearer,
                    None,
                    "invite_failed",
                )
                .await?;
            invite_from(&self.api, &answer.json::<Value>("the invite")?, &room)
        })
        .await
    }

    /// Makes a standing link into a room held under a scope, from the account
    /// page rather than a call: authorised by the session, for the guest
    /// `name` (40 characters at most, or `name_too_long`), between two instants
    /// in milliseconds since the epoch, either of which may be left open.
    pub async fn create_standing_invite(
        self: Arc<Self>,
        room: String,
        name: String,
        from_ms: Option<i64>,
        until_ms: Option<i64>,
    ) -> Result<MadeInvite, ProtocolError> {
        on_core(async move {
            let instant = |ms: Option<i64>| match ms {
                None => Ok(String::new()),
                Some(ms) => iso_from_ms(ms).ok_or_else(|| ProtocolError::Invalid {
                    detail: "that time is out of range".into(),
                }),
            };
            // In the order the account page writes them; `json!` would sort them.
            #[derive(Serialize)]
            struct Standing<'a> {
                standing: bool,
                name: &'a str,
                from: String,
                until: String,
            }
            let body = serde_json::to_string(&Standing {
                standing: true,
                name: js_trim(&name),
                from: instant(from_ms)?,
                until: instant(until_ms)?,
            })
            .map_err(|fault| unexpected(format!("the invite could not be written: {fault}")))?;
            let request = self
                .api
                .json_request(Method::POST, &room_path(&room, "/invites"))?
                .body(body);
            let answer = self.api.send(request).await?.accepted("")?;
            invite_from(&self.api, &answer.json::<Value>("the invite")?, &room)
        })
        .await
    }

    /// Stops every link into the room working, without ending the meeting.
    /// With a join token as host.ts does it, or with none as the account page
    /// does for a scoped room. Returns how many stopped.
    pub async fn revoke_invites(
        self: Arc<Self>,
        room: String,
        bearer: Option<String>,
    ) -> Result<u32, ProtocolError> {
        on_core(async move {
            let path = room_path(&room, "/invites");
            let request = match bearer.as_deref() {
                Some(bearer) => self.api.bearer_request(Method::DELETE, &path, bearer)?,
                None => self.api.request(Method::DELETE, &path)?,
            };
            let answer = self.api.send(request).await?.accepted("revoke_failed")?;
            let revoked = answer
                .value()
                .and_then(|said| said.get("revoked").and_then(Value::as_f64))
                .unwrap_or(0.0);
            Ok(revoked.clamp(0.0, f64::from(u32::MAX)) as u32)
        })
        .await
    }

    /// What room an invite is for, or why it is no good. A refusal is an
    /// answer here, as account.ts `invited` has it, because `invite_not_yet`
    /// comes with the time the link opens.
    pub async fn invite_info(self: Arc<Self>, token: String) -> Result<InviteInfo, ProtocolError> {
        on_core(async move { self.api.invite_info(&token).await }).await
    }

    /// Whether a meeting is happening under this name, for somebody signed in;
    /// `None` where that cannot be said (account.ts `meeting`).
    pub async fn room_live(self: Arc<Self>, room: String) -> Result<Option<bool>, ProtocolError> {
        on_core(async move {
            let answer = self
                .api
                .send(self.api.request(Method::GET, &room_path(&room, "/live"))?)
                .await?;
            Ok(answer
                .ok()
                .then(|| answer.value())
                .flatten()
                .and_then(|said| said.get("live").and_then(Value::as_bool)))
        })
        .await
    }

    /// Knocks on a room that turned this person away.
    pub async fn knock(
        self: Arc<Self>,
        room: String,
        name: String,
    ) -> Result<KnockTicket, ProtocolError> {
        on_core(async move {
            let request = self
                .api
                .json_request(Method::POST, &room_path(&room, "/knock"))?
                .body(json!({ "name": name }).to_string());
            let said = self
                .api
                .send(request)
                .await?
                .accepted("")?
                .json::<Value>("the knock")?;
            let token = some_text(said.get("token"))
                .ok_or_else(|| unexpected("a knock answer without its token"))?;
            Ok(KnockTicket {
                token,
                state: DoorState::read(said.get("state")),
            })
        })
        .await
    }

    /// Asks whether anybody has answered a knock; every `knock_ask_every_ms`.
    pub async fn at_the_door(
        self: Arc<Self>,
        room: String,
        token: String,
    ) -> Result<DoorAnswer, ProtocolError> {
        on_core(async move {
            let path = room_path(&room, &format!("/knock/{}", encode_uri_component(&token)));
            let said = self
                .api
                .send(self.api.json_request(Method::GET, &path)?)
                .await?
                .accepted("")?
                .json::<Value>("the door")?;
            Ok(DoorAnswer {
                state: DoorState::read(said.get("state")),
                invite: some_text(said.get("invite")),
            })
        })
        .await
    }

    /// Who is at the door, for somebody inside who can answer.
    pub async fn knocking(
        self: Arc<Self>,
        room: String,
        bearer: String,
    ) -> Result<Vec<Knocker>, ProtocolError> {
        on_core(async move {
            let request =
                self.api
                    .bearer_request(Method::GET, &room_path(&room, "/knocks"), &bearer)?;
            let said = self
                .api
                .send(request)
                .await?
                .accepted("")?
                .json::<Value>("the door")?;
            Ok(said
                .get("knocking")
                .and_then(Value::as_array)
                .map(|all| {
                    all.iter()
                        .map(|one| Knocker {
                            id: text(one.get("id")),
                            name: text(one.get("name")),
                            address: text(one.get("address")),
                            at: text(one.get("at")),
                        })
                        .collect()
                })
                .unwrap_or_default())
        })
        .await
    }

    /// Lets somebody in, or does not.
    pub async fn answer_knock(
        self: Arc<Self>,
        room: String,
        id: String,
        bearer: String,
        admit: bool,
    ) -> Result<DoorState, ProtocolError> {
        on_core(async move {
            let path = room_path(&room, &format!("/knocks/{}", encode_uri_component(&id)));
            let request = self
                .api
                .bearer_request(Method::POST, &path, &bearer)?
                .body(json!({ "admit": admit }).to_string());
            let said = self
                .api
                .send(request)
                .await?
                .accepted("")?
                .json::<Value>("the door")?;
            Ok(DoorState::read(said.get("state")))
        })
        .await
    }
}
