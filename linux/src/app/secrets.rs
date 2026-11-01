//! Where the app keeps what must not sit in a plain file: the account's
//! session cookie (`account:<base>`, as Windows keeps it with DPAPI in
//! Model/Kept.cs) and the kept identity's proof
//! (`tomoshibi.identity.<base>-proof`, KeptIdentity.cs).
//!
//! On Linux this is the Secret Service (libsecret; GNOME Keyring or KWallet
//! behind it). The skeleton holds secrets in memory only, which is also what
//! a rehearsal uses; the app module replaces [`standard`] with the Secret
//! Service and keeps this trait as the boundary.

use std::{cell::RefCell, collections::HashMap};

/// A store of small secrets by key. Called on the main thread.
pub trait Secrets {
    fn read(&self, key: &str) -> Option<String>;
    fn write(&self, key: &str, value: &str);
    fn remove(&self, key: &str);
}

/// Secrets for this run alone.
#[derive(Default)]
pub struct InMemory {
    held: RefCell<HashMap<String, String>>,
}

impl Secrets for InMemory {
    fn read(&self, key: &str) -> Option<String> {
        self.held.borrow().get(key).cloned()
    }

    fn write(&self, key: &str, value: &str) {
        self.held
            .borrow_mut()
            .insert(key.to_owned(), value.to_owned());
    }

    fn remove(&self, key: &str) {
        self.held.borrow_mut().remove(key);
    }
}

/// The store the app uses: the Secret Service in a real run, memory in a
/// rehearsal. Until the app module adds libsecret, memory in both.
pub fn standard(rehearsal: bool) -> Box<dyn Secrets> {
    let _ = rehearsal;
    Box::<InMemory>::default()
}

/// The key a session cookie is kept under for a deployment's base address.
pub fn session_key(base: &str) -> String {
    format!("account:{base}")
}
