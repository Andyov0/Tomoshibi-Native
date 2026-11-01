//! The account's own business, as the settings' account page does it
//! (Account.tsx; AppModel.cs's account section): signing out, the
//! passphrase, the picture, and guest links into a scope's rooms.

use tomoshibi_protocol::api::MadeInvite;

use super::{log, secrets, words, AppModel, Change, Screen};
use crate::i18n::t;

impl AppModel {
    /// Signs out here and at the deployment, forgets the session, and goes
    /// back to signing in.
    pub fn sign_out(&self) {
        let Some(deployment) = self.deployment() else {
            return;
        };
        let model = self.clone();
        glib::spawn_future_local(async move {
            // The core empties its own jar whether or not the deployment
            // heard, so that a session is never left authorising anything.
            if let Err(fault) = deployment.clone().sign_out().await {
                log(&format!("the deployment did not hear the sign-out: {fault}"));
            }
            model
                .secrets()
                .remove(&secrets::session_key(&deployment.base_url()));
            model.inner.state.borrow_mut().account = None;
            log("signed out");
            model.changed(Change::Account);
            if !model.screen().is_call() {
                model.show(Screen::SignIn);
            }
        });
    }

    /// Whether the account holds a permission, by what the server said; a
    /// server older than permissions said nothing, which reads as the
    /// defaults (manage/grants.ts `DEFAULTS`: everything but Emby).
    pub fn holds(&self, permission: &str) -> bool {
        let Some(account) = self.account() else {
            return false;
        };
        match &account.can {
            Some(can) => can.iter().any(|held| held == permission),
            None => permission != "emby",
        }
    }

    pub fn picture_version(&self) -> u32 {
        self.inner.state.borrow().picture_version
    }

    /// Where the account's picture is, or none: an absolute address as it
    /// is, a path on the deployment otherwise, with the version that keeps a
    /// cache honest.
    pub fn avatar_address(&self) -> Option<String> {
        let avatar = self.account()?.avatar.filter(|avatar| !avatar.is_empty())?;
        if avatar.starts_with("http://") || avatar.starts_with("https://") {
            return Some(avatar);
        }
        let base = self.base_url();
        let path = avatar.trim_start_matches('/');
        let version = self.picture_version();
        Some(format!(
            "{}/{path}{}",
            base.trim_end_matches('/'),
            if version > 0 { format!("?{version}") } else { String::new() }
        ))
    }

    fn account_changed(&self, account: tomoshibi_protocol::api::Account, picture: bool) {
        {
            let mut state = self.inner.state.borrow_mut();
            state.account = Some(account);
            if picture {
                state.picture_version += 1;
            }
        }
        self.changed(Change::Account);
    }

    /// Account.tsx `Picture`: the picture as a data URL, already scaled;
    /// the sentence to say where it did not take.
    pub async fn set_picture(&self, data_url: String) -> Result<(), String> {
        let deployment = self.deployment().ok_or_else(|| t("That picture could not be used."))?;
        match deployment.set_avatar(data_url).await {
            Ok(account) => {
                self.account_changed(account, true);
                Ok(())
            }
            Err(fault) => {
                log(&format!("the picture was refused: {fault}"));
                Err(t("That picture could not be used."))
            }
        }
    }

    pub async fn remove_picture(&self) -> Result<(), String> {
        let deployment = self.deployment().ok_or_else(|| t("That picture could not be used."))?;
        match deployment.remove_avatar().await {
            Ok(account) => {
                self.account_changed(account, true);
                Ok(())
            }
            Err(_) => Err(t("That picture could not be used.")),
        }
    }

    /// Account.tsx `Passphrase`.
    pub async fn change_passphrase(&self, current: String, next: String) -> Result<(), String> {
        let deployment = self
            .deployment()
            .ok_or_else(|| t("That could not be changed. Try again."))?;
        match deployment.change_passphrase(current, next).await {
            Ok(account) => {
                self.account_changed(account, false);
                Ok(())
            }
            Err(fault) => Err(words::explain_passphrase(&fault)),
        }
    }

    /// Account.tsx `GuestLinks`: a standing link into a scope's room, open
    /// from and until the instants given (seconds since 1970).
    pub async fn make_guest_link(
        &self,
        room: String,
        name: String,
        from: Option<i64>,
        until: Option<i64>,
    ) -> Result<MadeInvite, String> {
        let deployment = self
            .deployment()
            .ok_or_else(|| t("That link could not be made. Try again."))?;
        deployment
            .create_standing_invite(
                room,
                name.trim().to_owned(),
                from.map(|at| at * 1000),
                until.map(|at| at * 1000),
            )
            .await
            .map_err(|fault| words::explain_link(&fault))
    }

    /// Stops every link into a room, returning how many stopped.
    pub async fn stop_guest_links(&self, room: String) -> Result<u32, String> {
        let deployment = self
            .deployment()
            .ok_or_else(|| t("That link could not be made. Try again."))?;
        deployment
            .revoke_invites(room, None)
            .await
            .map_err(|fault| words::explain_link(&fault))
    }
}
