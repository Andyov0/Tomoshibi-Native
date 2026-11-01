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
    rc::Rc,
    time::Duration,
};

/// How long after the last change the file is written: the changes in
/// between are written together. The name field changed the preferences
/// at every key pressed, and the whole file was written each time.
const WRITE_AFTER: Duration = Duration::from_millis(400);

/// The preferences file, read once and written whole a moment after
/// changes, as one write for all the changes made in that moment; and at
/// once when the app quits ([`flush`](Self::flush), and when it is dropped).
pub struct Preferences {
    inner: Rc<Inner>,
}

struct Inner {
    path: Option<PathBuf>,
    values: RefCell<BTreeMap<String, String>>,
    /// Whether changes are written; a rehearsal's are not, since what they
    /// would write over is the owner's (launch.rs).
    keeps: bool,
    /// Whether a write waits for [`WRITE_AFTER`]; the tests' preferences
    /// are written at once unless they ask.
    waits: bool,
    /// The write that is waiting.
    pending: RefCell<Option<glib::JoinHandle<()>>>,
}

impl Preferences {
    /// The file in the user's config directory.
    pub fn standard(keeps: bool) -> Preferences {
        let path = glib::user_config_dir()
            .join("tomoshibi")
            .join("preferences.json");
        Self::at(Some(path), keeps).waiting()
    }

    /// Preferences held in memory alone, for tests.
    pub fn in_memory() -> Preferences {
        Self::at(None, false)
    }

    pub fn at(path: Option<PathBuf>, keeps: bool) -> Preferences {
        let values = path.as_deref().map(read).unwrap_or_default();
        Preferences {
            inner: Rc::new(Inner {
                path,
                values: RefCell::new(values),
                keeps,
                waits: false,
                pending: RefCell::new(None),
            }),
        }
    }

    /// The same preferences, written a moment after changes rather than at
    /// once: the wait runs on the main context of the thread that made the
    /// change (the app's main loop).
    pub fn waiting(mut self) -> Preferences {
        if let Some(inner) = Rc::get_mut(&mut self.inner) {
            inner.waits = true;
        }
        self
    }

    pub fn string(&self, key: &str) -> Option<String> {
        self.inner.values.borrow().get(key).cloned()
    }

    pub fn bool(&self, key: &str) -> Option<bool> {
        match self.string(key)?.as_str() {
            "true" | "1" => Some(true),
            "false" | "0" => Some(false),
            _ => None,
        }
    }

    pub fn set(&self, key: &str, value: impl Into<String>) {
        let value = value.into();
        let changed = self.inner.values.borrow().get(key) != Some(&value);
        if changed {
            self.inner.values.borrow_mut().insert(key.to_owned(), value);
            self.changed();
        }
    }

    pub fn set_bool(&self, key: &str, value: bool) {
        self.set(key, if value { "true" } else { "false" });
    }

    pub fn remove(&self, key: &str) {
        if self.inner.values.borrow_mut().remove(key).is_some() {
            self.changed();
        }
    }

    /// Writes what is waiting to be written, now.
    pub fn flush(&self) {
        if let Some(waiting) = self.inner.pending.take() {
            waiting.abort();
            self.inner.write();
        }
    }

    fn changed(&self) {
        let inner = &self.inner;
        if inner.path.is_none() || !inner.keeps {
            return;
        }
        if !inner.waits {
            inner.write();
            return;
        }
        // One write waits at a time, and writes whatever the values are
        // when it runs.
        if inner.pending.borrow().is_some() {
            return;
        }
        let weak = Rc::downgrade(&self.inner);
        let waiting = glib::MainContext::ref_thread_default().spawn_local(async move {
            glib::timeout_future(WRITE_AFTER).await;
            if let Some(inner) = weak.upgrade() {
                inner.pending.take();
                inner.write();
            }
        });
        inner.pending.replace(Some(waiting));
    }
}

impl Inner {
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

impl Drop for Inner {
    fn drop(&mut self) {
        if let Some(waiting) = self.pending.take() {
            waiting.abort();
            self.write();
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

    #[test]
    fn changes_made_together_are_written_once_a_moment_later() {
        let dir =
            std::env::temp_dir().join(format!("tomoshibi-prefs-wait-{}", std::process::id()));
        let path = dir.join("preferences.json");
        let _ = std::fs::remove_dir_all(&dir);
        let context = glib::MainContext::new();
        context
            .with_thread_default(|| {
                let kept = Preferences::at(Some(path.clone()), true).waiting();
                for typed in ["A", "An", "Ann"] {
                    kept.set("meet-live.name", typed);
                }
                assert!(!path.exists(), "written at a key press");
                let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
                while !path.exists() && std::time::Instant::now() < until {
                    context.iteration(true);
                }
                assert_eq!(
                    Preferences::at(Some(path.clone()), true)
                        .string("meet-live.name")
                        .as_deref(),
                    Some("Ann")
                );
                // A change waiting when the preferences go is written then.
                kept.set("meet-live.name", "Bea");
                drop(kept);
                assert_eq!(
                    Preferences::at(Some(path.clone()), true)
                        .string("meet-live.name")
                        .as_deref(),
                    Some("Bea")
                );
            })
            .expect("the test's own context");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
