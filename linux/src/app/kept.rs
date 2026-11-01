//! What a join leaves behind for the next one, and the choices about one's
//! own devices that a call starts from, under the names every other app and
//! the web keep them by.

use serde_json::{Map, Value};

use super::{Preferences, Secrets};

/// The identity a deployment gave this app on its last join, and the proof it
/// gave with it, kept together and sent back together (api.ts keeps
/// `meet-live.identity` and `meet-live.identity-proof` side by side;
/// KeptIdentity.cs, KeptIdentity.kt and KeptIdentity.swift are the same).
///
/// A guest's identity is issued by the server and is on every roster in the
/// room, so anybody could send it back as their own; the server keeps an
/// issued identity only for whoever shows the proof it handed out with it.
/// Kept apart, or not at all, an app guest was a new participant at every
/// reconnect and "Join again", and read to everybody else as somebody
/// leaving and a stranger arriving. So the proof lives exactly as long as
/// the identity, and an answer that comes without one clears it, so that a
/// proof is never sent beside an identity it is not for.
///
/// The identity goes in the preferences, under the other apps' name; the
/// proof goes in the secret store under theirs, since the preferences file
/// is readable by anybody the umask lets read it and the proof is what
/// seats somebody in the guest's place.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct KeptIdentity {
    pub identity: Option<String>,
    pub proof: Option<String>,
}

impl KeptIdentity {
    /// Where the two are kept for the deployment at `base`.
    pub fn keys(base: &str) -> (String, String) {
        (
            format!("tomoshibi.identity.{base}"),
            format!("tomoshibi.identity-proof.{base}"),
        )
    }

    pub fn read(preferences: &Preferences, secrets: &dyn Secrets, base: &str) -> KeptIdentity {
        let (identity_key, proof_key) = Self::keys(base);
        let identity = preferences.string(&identity_key).filter(|v| !v.is_empty());
        // A proof outliving its identity is about nobody.
        let proof = identity
            .as_ref()
            .and_then(|_| secrets.read(&proof_key))
            .filter(|v| !v.is_empty());
        KeptIdentity { identity, proof }
    }

    /// What a join answered, kept whole: the identity, and its proof or none.
    pub fn keep(&self, preferences: &Preferences, secrets: &dyn Secrets, base: &str) {
        let (identity_key, proof_key) = Self::keys(base);
        match &self.identity {
            Some(identity) if !identity.is_empty() => preferences.set(&identity_key, identity),
            _ => preferences.remove(&identity_key),
        }
        match (&self.identity, &self.proof) {
            (Some(_), Some(proof)) if !proof.is_empty() => secrets.write(&proof_key, proof),
            _ => secrets.remove(&proof_key),
        }
    }
}

/// The kinds of device somebody picks from a list (preferences.ts
/// `CHOSEN_KINDS`), by the browser's names for them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DeviceSlot {
    Microphone,
    Camera,
    Speakers,
}

impl DeviceSlot {
    pub const ALL: [DeviceSlot; 3] = [
        DeviceSlot::Microphone,
        DeviceSlot::Camera,
        DeviceSlot::Speakers,
    ];

    pub fn key(self) -> &'static str {
        match self {
            DeviceSlot::Microphone => "audioinput",
            DeviceSlot::Camera => "videoinput",
            DeviceSlot::Speakers => "audiooutput",
        }
    }
}

/// Whether the camera and the microphone start on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Joining {
    pub camera: bool,
    pub microphone: bool,
}

impl Joining {
    /// The microphone on and the camera off: how most people walk into a
    /// call, and the cheaper of the two mistakes.
    pub const FIRST: Joining = Joining {
        camera: false,
        microphone: true,
    };
}

/// The choices about one's own devices that a call starts from, kept under
/// the web's own names (live/preferences.ts, live/sound.ts) and read the way
/// the web reads them (DeviceMemory.cs, DeviceMemory.kt).
///
/// `meet-live.devices` is whether the microphone and the camera start on;
/// `meet-live.device-choice` is which microphone, camera and speakers were
/// last picked, by kind; `meet-live.original-sound` whether the voice goes
/// as it is; `meet-live.blur` whether the background is blurred. Each is a
/// string, as a browser's storage holds it, and a value a reader does not
/// recognise is read as never having been told anything: a value half
/// written, or written by a later build, may not become a call that starts
/// with a silent microphone.
pub struct DeviceMemory;

impl DeviceMemory {
    pub const DEVICES_KEY: &'static str = "meet-live.devices";
    pub const CHOICE_KEY: &'static str = "meet-live.device-choice";
    pub const ORIGINAL_KEY: &'static str = "meet-live.original-sound";
    pub const BLUR_KEY: &'static str = "meet-live.blur";
    /// Longer than any device's identifier, and short enough to refuse
    /// what is not one.
    pub const MOST_ID: usize = 256;

    fn object(text: Option<String>) -> Option<Map<String, Value>> {
        match serde_json::from_str::<Value>(&text?).ok()? {
            Value::Object(map) => Some(map),
            _ => None,
        }
    }

    pub fn joining(preferences: &Preferences) -> Joining {
        let Some(parsed) = Self::object(preferences.string(Self::DEVICES_KEY)) else {
            return Joining::FIRST;
        };
        // Each on its own: a record that lost one half keeps the other.
        Joining {
            camera: parsed
                .get("camera")
                .and_then(Value::as_bool)
                .unwrap_or(Joining::FIRST.camera),
            microphone: parsed
                .get("microphone")
                .and_then(Value::as_bool)
                .unwrap_or(Joining::FIRST.microphone),
        }
    }

    /// Written as the web's `JSON.stringify` writes it, the camera first.
    pub fn remember_joining(preferences: &Preferences, joining: Joining) {
        preferences.set(
            Self::DEVICES_KEY,
            format!(
                "{{\"camera\":{},\"microphone\":{}}}",
                joining.camera, joining.microphone
            ),
        );
    }

    pub fn choice(preferences: &Preferences, slot: DeviceSlot) -> Option<String> {
        Self::object(preferences.string(Self::CHOICE_KEY))?
            .get(slot.key())
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty() && id.len() <= Self::MOST_ID)
            .map(str::to_owned)
    }

    /// Kept once the device took (DeviceMenu.tsx `select`); `None` forgets
    /// the choice of that kind, which is "the same as the system".
    pub fn remember_choice(preferences: &Preferences, slot: DeviceSlot, id: Option<&str>) {
        if let Some(id) = id {
            if id.is_empty() || id.len() > Self::MOST_ID {
                return;
            }
        }
        let mut all: Vec<(&'static str, String)> = DeviceSlot::ALL
            .into_iter()
            .filter(|other| *other != slot)
            .filter_map(|other| Some((other.key(), Self::choice(preferences, other)?)))
            .collect();
        if let Some(id) = id {
            all.push((slot.key(), id.to_owned()));
        }
        if all.is_empty() {
            preferences.remove(Self::CHOICE_KEY);
            return;
        }
        // Sorted by key, as the other apps write it, so that the record
        // reads the same byte for byte wherever it was written.
        all.sort_by(|a, b| a.0.cmp(b.0));
        let fields: Vec<String> = all
            .into_iter()
            .map(|(key, id)| format!("{}:{}", Value::from(key), Value::from(id)))
            .collect();
        preferences.set(Self::CHOICE_KEY, format!("{{{}}}", fields.join(",")));
    }

    pub fn forget_choices(preferences: &Preferences) {
        preferences.remove(Self::CHOICE_KEY);
    }

    /// Whether the voice was last sent as it is (sound.ts
    /// `rememberedOriginal`).
    pub fn original_sound(preferences: &Preferences) -> bool {
        preferences.string(Self::ORIGINAL_KEY).as_deref() == Some("on")
    }

    pub fn set_original_sound(preferences: &Preferences, on: bool) {
        if on {
            preferences.set(Self::ORIGINAL_KEY, "on");
        } else {
            preferences.remove(Self::ORIGINAL_KEY);
        }
    }

    /// The background blurred behind somebody's camera (preferences.ts
    /// `blurWanted`).
    pub fn blur(preferences: &Preferences) -> bool {
        preferences.string(Self::BLUR_KEY).as_deref() == Some("on")
    }

    pub fn set_blur(preferences: &Preferences, on: bool) {
        if on {
            preferences.set(Self::BLUR_KEY, "on");
        } else {
            preferences.remove(Self::BLUR_KEY);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::secrets::InMemory;

    #[test]
    fn an_identity_is_kept_with_its_proof_and_a_proof_never_alone() {
        let preferences = Preferences::in_memory();
        let secrets = InMemory::default();
        let base = "http://127.0.0.1:20100/";
        KeptIdentity {
            identity: Some("guest-1".into()),
            proof: Some("p1".into()),
        }
        .keep(&preferences, &secrets, base);
        assert_eq!(
            KeptIdentity::read(&preferences, &secrets, base),
            KeptIdentity {
                identity: Some("guest-1".into()),
                proof: Some("p1".into())
            }
        );
        assert_eq!(
            preferences
                .string("tomoshibi.identity.http://127.0.0.1:20100/")
                .as_deref(),
            Some("guest-1")
        );
        assert_eq!(
            preferences.string("tomoshibi.identity-proof.http://127.0.0.1:20100/"),
            None,
            "the proof is not in the plain file"
        );

        // An answer without a proof clears the old one.
        KeptIdentity {
            identity: Some("account-1".into()),
            proof: None,
        }
        .keep(&preferences, &secrets, base);
        assert_eq!(KeptIdentity::read(&preferences, &secrets, base).proof, None);

        // A proof left behind with no identity is about nobody.
        secrets.write("tomoshibi.identity-proof.http://127.0.0.1:20100/", "stale");
        preferences.remove("tomoshibi.identity.http://127.0.0.1:20100/");
        assert_eq!(
            KeptIdentity::read(&preferences, &secrets, base),
            KeptIdentity::default()
        );
    }

    #[test]
    fn the_joining_choice_reads_each_half_on_its_own() {
        let preferences = Preferences::in_memory();
        assert_eq!(DeviceMemory::joining(&preferences), Joining::FIRST);
        DeviceMemory::remember_joining(
            &preferences,
            Joining {
                camera: true,
                microphone: false,
            },
        );
        assert_eq!(
            preferences.string("meet-live.devices").as_deref(),
            Some("{\"camera\":true,\"microphone\":false}")
        );
        preferences.set("meet-live.devices", "{\"camera\":true}");
        assert_eq!(
            DeviceMemory::joining(&preferences),
            Joining {
                camera: true,
                microphone: true
            }
        );
        preferences.set("meet-live.devices", "not json");
        assert_eq!(DeviceMemory::joining(&preferences), Joining::FIRST);
    }

    #[test]
    fn device_choices_are_written_as_the_web_writes_them() {
        let preferences = Preferences::in_memory();
        DeviceMemory::remember_choice(&preferences, DeviceSlot::Microphone, Some("mic \"1\""));
        DeviceMemory::remember_choice(&preferences, DeviceSlot::Camera, Some("/dev/video0"));
        assert_eq!(
            preferences.string("meet-live.device-choice").as_deref(),
            Some("{\"audioinput\":\"mic \\\"1\\\"\",\"videoinput\":\"/dev/video0\"}")
        );
        assert_eq!(
            DeviceMemory::choice(&preferences, DeviceSlot::Microphone).as_deref(),
            Some("mic \"1\"")
        );
        DeviceMemory::remember_choice(&preferences, DeviceSlot::Microphone, None);
        DeviceMemory::remember_choice(&preferences, DeviceSlot::Camera, None);
        assert_eq!(preferences.string("meet-live.device-choice"), None);
        DeviceMemory::remember_choice(&preferences, DeviceSlot::Speakers, Some(&"x".repeat(300)));
        assert_eq!(
            DeviceMemory::choice(&preferences, DeviceSlot::Speakers),
            None
        );
    }

    #[test]
    fn switches_are_on_or_absent() {
        let preferences = Preferences::in_memory();
        DeviceMemory::set_blur(&preferences, true);
        assert_eq!(preferences.string("meet-live.blur").as_deref(), Some("on"));
        DeviceMemory::set_blur(&preferences, false);
        assert_eq!(preferences.string("meet-live.blur"), None);
        assert!(!DeviceMemory::original_sound(&preferences));
    }
}
