//! How the app was started: which deployment it talks to, and the debug
//! switches the tests drive it with (windows/Tomoshibi/Model/Launch.cs, the
//! Apple apps' `TOMOSHIBI_*` environment).
//!
//! A release build reads nothing from the environment and talks to the one
//! deployment the core was built for (`home_deployment`; a release core
//! admits no other). A debug build reads the switches below, and a test must
//! always give it `TOMOSHIBI_BASE`: without one it opens on the home
//! deployment the core was built with, which for a test build is
//! meet.example.com and reaches nothing, and for a build made with a real
//! deployment.local would be production.

/// What the app was started with.
#[derive(Debug, Clone, Default)]
pub struct Launch {
    /// `TOMOSHIBI_BASE`: the deployment to talk to instead of the home one.
    pub base: Option<String>,
    /// `TOMOSHIBI_NAME`: the name to join with.
    pub name: Option<String>,
    /// `TOMOSHIBI_SIGNIN=name:file`: signs in at start with the passphrase
    /// read from `file`, so that it never appears on a command line.
    pub sign_in: Option<(String, String)>,
    /// `TOMOSHIBI_AUTOJOIN`: the room to join as soon as the deployment
    /// answers.
    pub autojoin: Option<String>,
    /// `TOMOSHIBI_PASSPHRASE_FILE`: a file holding the passphrase the
    /// autojoin enters with, which makes the identity one the deployment
    /// can prove and so the host of a room it opens; read from a file so
    /// that it never appears on a command line.
    pub passphrase_file: Option<String>,
    /// `TOMOSHIBI_INVITE`: an invite token the autojoin enters with, for a
    /// room that already has somebody in it (a deployment's rooms are
    /// joined by invitation unless one is signed in).
    pub invite: Option<String>,
    /// `TOMOSHIBI_DEBUG_MAKE_INVITE`: once in a call it opened, the app makes
    /// an invite to it and writes the token to its log, for a second app or
    /// browser in a test to join with.
    pub make_invite: bool,
    /// `TOMOSHIBI_LANGUAGE`: the interface's language for this run, not kept.
    pub language: Option<String>,
    /// `TOMOSHIBI_MUTE`: start with the microphone off; `speakers` silences
    /// the playback instead.
    pub mute: Option<String>,
    /// `TOMOSHIBI_SOURCE`: a file the media module sends in place of the
    /// camera (y4m) or the microphone (wav), for tests with no devices.
    pub source: Option<String>,
    /// `TOMOSHIBI_SHOW`: a screen or panel to open at start, by name
    /// (`signin`, `lobby`, `settings`, `settings/<page>`, `room/<panel>`), for
    /// screenshots.
    pub show: Option<String>,
    /// `TOMOSHIBI_DEBUG_LOG`: a file to write the app's log to as well.
    pub log_file: Option<String>,
    /// `TOMOSHIBI_DEBUG_SNAPSHOT=path.png`: draws the window into this file
    /// once it has settled, independent of any compositor's screenshot.
    pub snapshot: Option<String>,
    /// `TOMOSHIBI_DEBUG_SNAPSHOT_AFTER_MS`: how long after the screen is
    /// shown the snapshot is drawn (1500 when not given), for a picture of
    /// a state that takes longer to arrive (a second person in the call).
    pub snapshot_after_ms: Option<u64>,
    /// `TOMOSHIBI_DEBUG_QUIT_AFTER`: seconds after which the app leaves any
    /// call and quits.
    pub quit_after_seconds: Option<u32>,
    /// `TOMOSHIBI_MICROPHONE` and `TOMOSHIBI_CAMERA` (`on` or `off`): whether
    /// the microphone and the camera start on, for this run. An autojoin
    /// with neither given goes in with both off, as the skeleton's did.
    pub microphone: Option<bool>,
    pub camera: Option<bool>,
    /// `TOMOSHIBI_PREPARE`: a room to open the pre-join on once the
    /// deployment has answered (Launch.cs `Prepare`).
    pub prepare: Option<String>,
    /// `TOMOSHIBI_OPEN`: a link opened once the deployment has answered, as
    /// if the system had handed it over.
    pub open: Option<String>,
    /// `TOMOSHIBI_DEBUG_LOBBY`: a part of the screens before a call to open
    /// at start, for pictures (the Mac's LobbyDebug): `start`, `join`,
    /// `again`, `scheduled`, `servers`, `account`, `language`, `settings`,
    /// `settings/<page>`, `knock`, `notice`.
    pub lobby: Option<String>,
    /// `TOMOSHIBI_DEBUG_PORTRAIT`: the animal the lobby opens on, by name,
    /// and `TOMOSHIBI_DEBUG_PORTRAIT_AT`: the seconds into its turn it
    /// stands still at, for a picture of one moment of the drawing.
    pub portrait: Option<String>,
    pub portrait_at: Option<f64>,
}

impl Launch {
    /// The switches, read in a debug build only.
    pub fn from_environment() -> Launch {
        #[cfg(debug_assertions)]
        {
            Self::read(|name| std::env::var(name).ok())
        }
        #[cfg(not(debug_assertions))]
        {
            Launch::default()
        }
    }

    /// The switches as `get` gives them, blank ones as missing.
    pub fn read(get: impl Fn(&str) -> Option<String>) -> Launch {
        let get = |name: &str| {
            get(name)
                .map(|value| value.trim().to_owned())
                .filter(|value| !value.is_empty())
        };
        let sign_in = get("TOMOSHIBI_SIGNIN").and_then(|value| {
            let (name, file) = value.split_once(':')?;
            (!name.is_empty() && !file.is_empty()).then(|| (name.to_owned(), file.to_owned()))
        });
        Launch {
            base: get("TOMOSHIBI_BASE"),
            name: get("TOMOSHIBI_NAME"),
            sign_in,
            autojoin: get("TOMOSHIBI_AUTOJOIN"),
            invite: get("TOMOSHIBI_INVITE"),
            passphrase_file: get("TOMOSHIBI_PASSPHRASE_FILE"),
            make_invite: get("TOMOSHIBI_DEBUG_MAKE_INVITE").is_some(),
            language: get("TOMOSHIBI_LANGUAGE"),
            mute: get("TOMOSHIBI_MUTE"),
            source: get("TOMOSHIBI_SOURCE"),
            show: get("TOMOSHIBI_SHOW"),
            log_file: get("TOMOSHIBI_DEBUG_LOG"),
            snapshot: get("TOMOSHIBI_DEBUG_SNAPSHOT"),
            snapshot_after_ms: get("TOMOSHIBI_DEBUG_SNAPSHOT_AFTER_MS")
                .and_then(|value| value.parse().ok()),
            quit_after_seconds: get("TOMOSHIBI_DEBUG_QUIT_AFTER")
                .and_then(|value| value.parse().ok()),
            microphone: get("TOMOSHIBI_MICROPHONE").and_then(|value| switch(&value)),
            camera: get("TOMOSHIBI_CAMERA").and_then(|value| switch(&value)),
            prepare: get("TOMOSHIBI_PREPARE"),
            open: get("TOMOSHIBI_OPEN"),
            lobby: get("TOMOSHIBI_DEBUG_LOBBY"),
            portrait: get("TOMOSHIBI_DEBUG_PORTRAIT"),
            portrait_at: get("TOMOSHIBI_DEBUG_PORTRAIT_AT").and_then(|value| value.parse().ok()),
        }
    }

    /// A test is driving the app (a debug build given a deployment of its
    /// own), and nothing it does is kept: not the name, the session, the
    /// identity, the language or the colour (Launch.cs `Rehearsal`). The
    /// tests run the very app somebody uses, and what they would write over
    /// is the owner's.
    pub fn rehearsal(&self) -> bool {
        self.base.is_some()
    }

    /// The deployment to open on.
    pub fn deployment_base(&self) -> String {
        self.base
            .clone()
            .unwrap_or_else(tomoshibi_protocol::api::home_deployment)
    }
}

/// `on` or `off`, as the other apps' switches are written.
fn switch(value: &str) -> Option<bool> {
    match value.to_ascii_lowercase().as_str() {
        "on" | "1" | "true" | "yes" => Some(true),
        "off" | "0" | "false" | "no" => Some(false),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn launched(pairs: &[(&str, &str)]) -> Launch {
        let map: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        Launch::read(|name| map.get(name).cloned())
    }

    #[test]
    fn a_launch_given_a_deployment_is_a_rehearsal() {
        let launch = launched(&[
            ("TOMOSHIBI_BASE", "http://127.0.0.1:20000"),
            ("TOMOSHIBI_NAME", " Ann "),
        ]);
        assert!(launch.rehearsal());
        assert_eq!(launch.deployment_base(), "http://127.0.0.1:20000");
        assert_eq!(launch.name.as_deref(), Some("Ann"));
    }

    #[test]
    fn without_one_it_opens_on_home() {
        let launch = launched(&[("TOMOSHIBI_BASE", "  ")]);
        assert!(!launch.rehearsal());
        assert_eq!(
            launch.deployment_base(),
            tomoshibi_protocol::api::home_deployment()
        );
    }

    #[test]
    fn device_switches_are_on_or_off_and_nothing_else() {
        let launch = launched(&[("TOMOSHIBI_MICROPHONE", "off"), ("TOMOSHIBI_CAMERA", "maybe")]);
        assert_eq!(launch.microphone, Some(false));
        assert_eq!(launch.camera, None);
    }

    #[test]
    fn a_sign_in_needs_a_name_and_a_file() {
        assert_eq!(
            launched(&[("TOMOSHIBI_SIGNIN", "ann:/run/p")]).sign_in,
            Some(("ann".into(), "/run/p".into()))
        );
        assert_eq!(launched(&[("TOMOSHIBI_SIGNIN", "ann")]).sign_in, None);
        assert_eq!(launched(&[("TOMOSHIBI_SIGNIN", ":/run/p")]).sign_in, None);
    }
}
