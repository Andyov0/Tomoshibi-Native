//! The account somebody signs in to, and what they change about it.
//!
//! Follows web/src/live/account.ts (`me`, `signIn`, `signOut`) and the account
//! page's own requests in web/src/account/Account.tsx (the passphrase and the
//! picture); the server is internal/app/account.go. The session is the
//! HttpOnly cookie `meet-live.account`, which lives in the deployment's jar and
//! which the app keeps across launches through `account_cookie` and
//! `restore_account_cookie`.

use std::sync::Arc;

use reqwest::Method;
use serde::Serialize;
use serde_json::Value;

use super::{
    read::{some_text, yes},
    unexpected, Api, Deployment,
};
use crate::{runtime::on_core, ProtocolError};

/// Who is signed in (account.ts `Me`, the server's `accountView`).
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct Account {
    pub name: String,
    /// The account's signature: the public half of it, and what proves the
    /// name in a room.
    pub trip: String,
    /// The account's picture, as an address on the deployment, where it has one.
    pub avatar: Option<String>,
    /// Whether this person also runs the deployment. Told so the app can offer
    /// the way to the management pages, and authorising nothing.
    pub admin: bool,
    /// When the account was made, as an instant.
    pub created: Option<String>,
    /// The scopes the account carries, for offering where a room name is typed.
    pub scopes: Vec<String>,
    /// What the account may use (`watch`, `emby`, `music`, `arrange`, `invite`,
    /// `open`). `None` only from a server older than permissions, which the
    /// account page reads as the defaults rather than as nothing.
    pub can: Option<Vec<String>>,
}

/// The picture the server allows, at most: the whole body is read through a
/// 96 KiB limit (`accountAvatar`).
const AVATAR_LIMIT: usize = 96 << 10;

fn strings(value: Option<&Value>) -> Option<Vec<String>> {
    value.and_then(Value::as_array).map(|all| {
        all.iter()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect()
    })
}

impl Api {
    fn account(&self, body: &Value) -> Result<Account, ProtocolError> {
        let name = body.get("name").and_then(Value::as_str);
        let trip = body.get("trip").and_then(Value::as_str);
        let (Some(name), Some(trip)) = (name, trip) else {
            return Err(unexpected("an account without its name or signature"));
        };

        Ok(Account {
            name: name.to_owned(),
            trip: trip.to_owned(),
            // The server says `/api/avatar/<trip>`, a path on its own origin,
            // which a page can use as it is and an app cannot.
            avatar: some_text(body.get("avatar")).map(|path| self.absolute(&path)),
            admin: yes(body.get("admin")),
            created: some_text(body.get("created")),
            scopes: strings(body.get("scopes")).unwrap_or_default(),
            can: strings(body.get("can")),
        })
    }

    async fn me(&self) -> Result<Option<Account>, ProtocolError> {
        let answer = self
            .send(self.request(Method::GET, "/api/account/me")?)
            .await?;

        // Not signed in, a deployment with no accounts, and an answer that
        // cannot be read all mean the same here, as in account.ts: nobody is
        // signed in, carry on as before accounts existed.
        if !answer.ok() {
            return Ok(None);
        }

        Ok(answer.value().and_then(|body| self.account(&body).ok()))
    }

    /// A request whose answer is the account as it now is.
    async fn changed(
        &self,
        method: Method,
        path: &str,
        body: Option<String>,
        fallback: &str,
    ) -> Result<Account, ProtocolError> {
        let mut request = self.json_request(method, path)?;
        if let Some(body) = body {
            request = request.body(body);
        }

        let answer = self.send(request).await?.accepted(fallback)?;

        self.account(&answer.json::<Value>("the account")?)
    }
}

#[derive(Serialize)]
struct SignIn<'a> {
    name: &'a str,
    passphrase: &'a str,
}

#[derive(Serialize)]
struct NewPassphrase<'a> {
    current: &'a str,
    next: &'a str,
}

#[derive(Serialize)]
struct Picture<'a> {
    image: &'a str,
}

#[uniffi::export]
impl Deployment {
    /// The account this app is signed in to, if any.
    pub async fn me(self: Arc<Self>) -> Result<Option<Account>, ProtocolError> {
        on_core(async move { self.api.me().await }).await
    }

    /// Signs in, keeping the session in the jar.
    ///
    /// A refusal carries the server's code: `not_yours` where the pair does not
    /// go together (all account.ts ever says), `blocked`, `rate_limited`. Every
    /// attempt is charged to the deployment's guessing budget, so this is
    /// never retried.
    pub async fn sign_in(
        self: Arc<Self>,
        name: String,
        passphrase: String,
    ) -> Result<Account, ProtocolError> {
        on_core(async move {
            let body = serde_json::to_string(&SignIn {
                name: &name,
                passphrase: &passphrase,
            })
            .map_err(|fault| unexpected(format!("the sign-in could not be written: {fault}")))?;
            self.api
                .changed(
                    Method::POST,
                    "/api/account/session",
                    Some(body),
                    "not_yours",
                )
                .await
        })
        .await
    }

    /// Signs out: the server ends the session, and the jar forgets it whatever
    /// the server said.
    ///
    /// Forgotten here as well because an app, unlike a page, can: a session
    /// left in the jar after somebody pressed "sign out" on a network that
    /// dropped the request would go on authorising everything they do.
    pub async fn sign_out(self: Arc<Self>) -> Result<(), ProtocolError> {
        on_core(async move {
            let request = self.api.request(Method::DELETE, "/api/account/session")?;
            let sent = self.api.send(request).await;
            self.restore_account_cookie(String::new())?;
            sent.map(drop)
        })
        .await
    }

    /// Changes the account's passphrase. Refusals: `not_yours` (the current one
    /// is wrong), `passphrase_too_short` (under eight characters),
    /// `passphrase_unchanged`, `passphrase_in_use`.
    pub async fn change_passphrase(
        self: Arc<Self>,
        current: String,
        next: String,
    ) -> Result<Account, ProtocolError> {
        on_core(async move {
            let body = serde_json::to_string(&NewPassphrase {
                current: &current,
                next: &next,
            })
            .map_err(|fault| unexpected(format!("the passphrase could not be written: {fault}")))?;
            self.api
                .changed(Method::POST, "/api/account/passphrase", Some(body), "")
                .await
        })
        .await
    }

    /// Sets the account's picture from a data URL (`data:image/...;base64,...`).
    ///
    /// The account page scales a picture to 256 pixels square and encodes it
    /// as JPEG at 0.82 before sending it; the app does the same. One the server
    /// would refuse for its size is refused here with the server's own status
    /// and code before it is sent: sent, it would be cut off at the limit and
    /// arrive as a broken connection rather than as the refusal.
    pub async fn set_avatar(self: Arc<Self>, image: String) -> Result<Account, ProtocolError> {
        on_core(async move {
            let body = serde_json::to_string(&Picture { image: &image }).map_err(|fault| {
                unexpected(format!("the picture could not be written: {fault}"))
            })?;
            if body.len() > AVATAR_LIMIT {
                return Err(ProtocolError::Refused {
                    status: 413,
                    code: "avatar_too_large".into(),
                });
            }
            self.api
                .changed(Method::PUT, "/api/account/avatar", Some(body), "")
                .await
        })
        .await
    }

    /// Removes the account's picture.
    pub async fn remove_avatar(self: Arc<Self>) -> Result<Account, ProtocolError> {
        on_core(async move {
            self.api
                .changed(Method::DELETE, "/api/account/avatar", None, "")
                .await
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::read::text;

    #[test]
    fn reads_an_account_and_makes_its_picture_an_address() {
        let deployment = Deployment::new("https://live.example.org".into()).unwrap();
        let account = deployment
            .api
            .account(&serde_json::json!({
                "name": "alice", "trip": "k7m2q6x4rt", "avatar": "/api/avatar/k7m2q6x4rt",
                "admin": true, "scopes": ["acme"], "can": ["watch", 3], "later": "ignored"
            }))
            .unwrap();

        assert_eq!(
            account.avatar.as_deref(),
            Some("https://live.example.org/api/avatar/k7m2q6x4rt")
        );
        assert!(account.admin);
        assert_eq!(account.scopes, vec!["acme".to_owned()]);
        assert_eq!(account.can, Some(vec!["watch".to_owned()]));
        assert_eq!(text(Some(&Value::Null)), "");
    }

    #[test]
    fn tells_a_server_without_permissions_from_an_account_without_any() {
        let deployment = Deployment::new("https://live.example.org".into()).unwrap();
        let old = deployment
            .api
            .account(&serde_json::json!({ "name": "a", "trip": "b" }))
            .unwrap();
        let none = deployment
            .api
            .account(&serde_json::json!({ "name": "a", "trip": "b", "can": [] }))
            .unwrap();

        assert_eq!(old.can, None);
        assert_eq!(none.can, Some(vec![]));
    }
}
