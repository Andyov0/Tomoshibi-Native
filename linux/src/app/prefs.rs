//! The app's preferences, in `~/.config/tomoshibi/preferences.json` (the
//! XDG config directory), under the web's own key names: the same keys the
//! web keeps in local storage and Windows keeps in
//! `%LOCALAPPDATA%\Tomoshibi\preferences.json` (Defaults.cs), so that one
//! name means one thing on every platform.
//!
//! Flat string values only, as local storage has. A secret never goes here:
//! the session cookie and the kept identity's proof are in the secret store
//! (secrets.rs).

use std::{
    cell::RefCell,
    collections::BTreeMap,
    path::{Path, PathBuf},
};

/// The preferences file, read once and written whole on every change.
pub struct Preferences {
    path: Option<PathBuf>,
    values: RefCell<BTreeMap<String, String>>,
    /// Whether changes are written; a rehearsal's are not, since what they
    /// would write over is the owner's (launch.rs).
    keeps: bool,
}

impl Preferences {
    /// The file in the user's config directory.
    pub fn standard(keeps: bool) -> Preferences {
        let path = glib::user_config_dir()
            .join("tomoshibi")
            .join("preferences.json");
        Self::at(Some(path), keeps)
    }

    /// Preferences held in memory alone, for tests.
    pub fn in_memory() -> Preferences {
        Self::at(None, false)
    }

    pub fn at(path: Option<PathBuf>, keeps: bool) -> Preferences {
        let values = path.as_deref().map(read).unwrap_or_default();
        Preferences {
            path,
            values: RefCell::new(values),
            keeps,
        }
    }

    pub fn string(&self, key: &str) -> Option<String> {
        self.values.borrow().get(key).cloned()
    }

    pub fn bool(&self, key: &str) -> Option<bool> {
        match self.string(key)?.as_str() {
            "true" | "1" => Some(true),
            "false" | "0" => Some(false),
            _ => None,
        }
    }

    pub fn set(&self, key: &str, value: impl Into<String>) {
        self.values
            .borrow_mut()
            .insert(key.to_owned(), value.into());
        self.write();
    }

    pub fn set_bool(&self, key: &str, value: bool) {
        self.set(key, if value { "true" } else { "false" });
    }

    pub fn remove(&self, key: &str) {
        if self.values.borrow_mut().remove(key).is_some() {
            self.write();
        }
    }

    fn write(&self) {
        let (Some(path), true) = (&self.path, self.keeps) else {
            return;
        };
        let text = match serde_json::to_string_pretty(&*self.values.borrow()) {
            Ok(text) => text,
            Err(_) => return,
        };
        // Written beside and renamed over, so that a crash mid-write leaves
        // the old file rather than half of the new one.
        let result = (|| -> std::io::Result<()> {
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir)?;
            }
            let partial = path.with_extension("json.part");
            std::fs::write(&partial, text)?;
            std::fs::rename(&partial, path)
        })();
        if let Err(fault) = result {
            eprintln!("tomoshibi: the preferences could not be written: {fault}");
        }
    }
}

fn read(path: &Path) -> BTreeMap<String, String> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return BTreeMap::new();
    };
    // A value that is not a string (an older file, a hand edit) is read as
    // its JSON text rather than dropping the whole file.
    match serde_json::from_str::<BTreeMap<String, serde_json::Value>>(&text) {
        Ok(values) => values
            .into_iter()
            .map(|(key, value)| match value {
                serde_json::Value::String(text) => (key, text),
                other => (key, other.to_string()),
            })
            .collect(),
        Err(_) => BTreeMap::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_survive_a_restart_and_a_rehearsal_writes_nothing() {
        let dir = std::env::temp_dir().join(format!("tomoshibi-prefs-{}", std::process::id()));
        let path = dir.join("preferences.json");
        let _ = std::fs::remove_dir_all(&dir);

        let kept = Preferences::at(Some(path.clone()), true);
        kept.set("meet-live.name", "Ann");
        kept.set_bool("tomoshibi.blur", true);
        let again = Preferences::at(Some(path.clone()), true);
        assert_eq!(again.string("meet-live.name").as_deref(), Some("Ann"));
        assert_eq!(again.bool("tomoshibi.blur"), Some(true));

        let rehearsal = Preferences::at(Some(path.clone()), false);
        rehearsal.set("meet-live.name", "Test");
        assert_eq!(rehearsal.string("meet-live.name").as_deref(), Some("Test"));
        assert_eq!(
            Preferences::at(Some(path), true)
                .string("meet-live.name")
                .as_deref(),
            Some("Ann")
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
