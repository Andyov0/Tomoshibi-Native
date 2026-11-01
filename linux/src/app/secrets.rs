//! Where the app keeps what must not sit in a plain file: the account's
//! session cookie (`account:<base>`, as Windows keeps it with DPAPI in
//! Model/Kept.cs), a guest's passphrase (`passphrase:<base>`, AppModel.cs
//! `PassphraseKey`) and the kept identity's proof
//! (`tomoshibi.identity-proof.<base>`, KeptIdentity.cs; kept.rs).
//!
//! On Linux this is the Secret Service, through libsecret: GNOME Keyring or
//! KWallet behind it, whatever the desktop runs. The preferences file is
//! readable by anybody the user's umask lets read it (0644 on most systems),
//! and a session cookie in it would be a session anybody on the machine
//! could take.
//!
//! A machine with no Secret Service at all (a bare compositor, a container,
//! a desktop whose keyring is not running) still runs the app: what cannot
//! be stored is held for this run only, and the next start asks somebody to
//! sign in again. That is said once in the log rather than to the person,
//! who can do nothing about it from here.

use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    ffi::{c_char, c_int, c_void, CStr, CString},
    ptr,
};

use super::log;

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
/// rehearsal, whose secrets are the test's and must not land in the owner's
/// keyring (launch.rs `rehearsal`).
pub fn standard(rehearsal: bool) -> Box<dyn Secrets> {
    if rehearsal {
        Box::<InMemory>::default()
    } else {
        Box::new(SecretService::default())
    }
}

/// The key a session cookie is kept under for a deployment's base address.
pub fn session_key(base: &str) -> String {
    format!("account:{base}")
}

/// The key a guest's passphrase is kept under (AppModel.cs `PassphraseKey`).
pub fn passphrase_key(base: &str) -> String {
    format!("passphrase:{base}")
}

/// The Secret Service, with this run's memory under it for when it cannot
/// be reached: everything written is held here as well, so that a store
/// that failed still reads back what was written in this run.
#[derive(Default)]
pub struct SecretService {
    memory: InMemory,
    /// Set once the service has failed, so that the log says it once and
    /// the rest of the run does not wait on a bus that is not there.
    failed: Cell<bool>,
}

impl SecretService {
    fn failed(&self, what: &str, fault: String) {
        if !self.failed.replace(true) {
            log(&format!(
                "the Secret Service could not {what} ({fault}); secrets are kept for this run only"
            ));
        }
    }
}

impl Secrets for SecretService {
    fn read(&self, key: &str) -> Option<String> {
        if let Some(held) = self.memory.read(key) {
            return Some(held);
        }
        if self.failed.get() {
            return None;
        }
        match ffi::lookup(key) {
            Ok(found) => {
                if let Some(found) = &found {
                    self.memory.write(key, found);
                }
                found
            }
            Err(fault) => {
                self.failed("be read", fault);
                None
            }
        }
    }

    fn write(&self, key: &str, value: &str) {
        self.memory.write(key, value);
        if self.failed.get() {
            return;
        }
        if let Err(fault) = ffi::store(key, value) {
            self.failed("keep a secret", fault);
        }
    }

    fn remove(&self, key: &str) {
        self.memory.remove(key);
        if self.failed.get() {
            return;
        }
        if let Err(fault) = ffi::clear(key) {
            self.failed("forget a secret", fault);
        }
    }
}

/// libsecret's "password" calls, with one schema of our own and one
/// attribute, the key. The synchronous calls: each is a D-Bus round trip of
/// a millisecond or two, made at start and when somebody signs in or out,
/// and the trait they serve is synchronous. A locked keyring shows the
/// desktop's own unlock prompt, which is a separate process.
mod ffi {
    use super::*;
    use glib::ffi::{
        g_error_free, g_hash_table_insert, g_hash_table_new, g_hash_table_unref, g_str_equal,
        g_str_hash, GError, GHashTable,
    };

    const SCHEMA_NAME: &CStr = c"app.tomoshibi.Linux";
    const ATTRIBUTE: &CStr = c"key";
    /// SECRET_SCHEMA_ATTRIBUTE_STRING.
    const STRING: c_int = 0;
    /// SECRET_SCHEMA_NONE.
    const NO_FLAGS: c_int = 0;

    #[repr(C)]
    struct SchemaAttribute {
        name: *const c_char,
        kind: c_int,
    }

    /// SecretSchema exactly as secret-schema.h lays it out, its reserved
    /// fields included: libsecret reads the whole structure.
    #[repr(C)]
    struct Schema {
        name: *const c_char,
        flags: c_int,
        attributes: [SchemaAttribute; 32],
        reserved: c_int,
        reserved1: *mut c_void,
        reserved2: *mut c_void,
        reserved3: *mut c_void,
        reserved4: *mut c_void,
        reserved5: *mut c_void,
        reserved6: *mut c_void,
        reserved7: *mut c_void,
    }

    #[link(name = "secret-1")]
    extern "C" {
        fn secret_password_storev_sync(
            schema: *const Schema,
            attributes: *mut GHashTable,
            collection: *const c_char,
            label: *const c_char,
            password: *const c_char,
            cancellable: *mut c_void,
            error: *mut *mut GError,
        ) -> glib::ffi::gboolean;
        fn secret_password_lookupv_sync(
            schema: *const Schema,
            attributes: *mut GHashTable,
            cancellable: *mut c_void,
            error: *mut *mut GError,
        ) -> *mut c_char;
        fn secret_password_clearv_sync(
            schema: *const Schema,
            attributes: *mut GHashTable,
            cancellable: *mut c_void,
            error: *mut *mut GError,
        ) -> glib::ffi::gboolean;
        fn secret_password_free(password: *mut c_char);
    }

    fn schema() -> Schema {
        const NONE: SchemaAttribute = SchemaAttribute {
            name: ptr::null(),
            kind: 0,
        };
        let mut attributes = [NONE; 32];
        attributes[0] = SchemaAttribute {
            name: ATTRIBUTE.as_ptr(),
            kind: STRING,
        };
        Schema {
            name: SCHEMA_NAME.as_ptr(),
            flags: NO_FLAGS,
            attributes,
            reserved: 0,
            reserved1: ptr::null_mut(),
            reserved2: ptr::null_mut(),
            reserved3: ptr::null_mut(),
            reserved4: ptr::null_mut(),
            reserved5: ptr::null_mut(),
            reserved6: ptr::null_mut(),
            reserved7: ptr::null_mut(),
        }
    }

    /// Runs `call` with the schema and a table of the one attribute; the
    /// strings live until it returns.
    fn with_key<T>(
        key: &str,
        call: impl FnOnce(*const Schema, *mut GHashTable, *mut *mut GError) -> T,
    ) -> Result<T, String> {
        let value = CString::new(key).map_err(|_| "a key with a NUL in it".to_owned())?;
        let schema = schema();
        let mut error: *mut GError = ptr::null_mut();
        // SAFETY: the table holds borrowed pointers to `ATTRIBUTE` (static)
        // and `value`, which outlive it; it is unreferenced before `value`
        // is dropped. The schema is a stack value read only during `call`.
        let result = unsafe {
            let table = g_hash_table_new(Some(g_str_hash), Some(g_str_equal));
            g_hash_table_insert(
                table,
                ATTRIBUTE.as_ptr() as *mut c_void,
                value.as_ptr() as *mut c_void,
            );
            let result = call(&schema, table, &mut error);
            g_hash_table_unref(table);
            result
        };
        if error.is_null() {
            Ok(result)
        } else {
            // SAFETY: a GError set by libsecret, freed once here.
            let message = unsafe {
                let message = CStr::from_ptr((*error).message)
                    .to_string_lossy()
                    .into_owned();
                g_error_free(error);
                message
            };
            Err(message)
        }
    }

    pub fn lookup(key: &str) -> Result<Option<String>, String> {
        with_key(key, |schema, table, error| {
            // SAFETY: the arguments are valid for the call (with_key).
            let found = unsafe { secret_password_lookupv_sync(schema, table, ptr::null_mut(), error) };
            if found.is_null() {
                return None;
            }
            // SAFETY: a NUL-terminated string libsecret allocated, freed by
            // its own function once copied.
            unsafe {
                let text = CStr::from_ptr(found).to_string_lossy().into_owned();
                secret_password_free(found);
                Some(text)
            }
        })
    }

    pub fn store(key: &str, value: &str) -> Result<(), String> {
        let password = CString::new(value).map_err(|_| "a secret with a NUL in it".to_owned())?;
        let label = CString::new(format!("Tomoshibi ({key})"))
            .map_err(|_| "a key with a NUL in it".to_owned())?;
        with_key(key, |schema, table, error| {
            // SAFETY: the arguments are valid for the call (with_key); a
            // null collection is the default one.
            unsafe {
                secret_password_storev_sync(
                    schema,
                    table,
                    ptr::null(),
                    label.as_ptr(),
                    password.as_ptr(),
                    ptr::null_mut(),
                    error,
                )
            }
        })
        .map(drop)
    }

    pub fn clear(key: &str) -> Result<(), String> {
        with_key(key, |schema, table, error| {
            // SAFETY: the arguments are valid for the call (with_key).
            unsafe { secret_password_clearv_sync(schema, table, ptr::null_mut(), error) }
        })
        .map(drop)
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn the_schema_is_laid_out_as_libsecret_reads_it() {
            // secret-schema.h on a 64-bit system: a name, the flags, 32
            // attributes of a pointer and an int each, an int and seven
            // pointers.
            let pointer = std::mem::size_of::<*const c_void>();
            assert_eq!(std::mem::size_of::<SchemaAttribute>(), 2 * pointer);
            assert_eq!(
                std::mem::size_of::<Schema>(),
                2 * pointer + 32 * 2 * pointer + pointer + 7 * pointer
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_store_that_cannot_be_reached_still_holds_this_runs_secrets() {
        let store = SecretService::default();
        store.failed.set(true);
        store.write("account:http://127.0.0.1:1/", "cookie");
        assert_eq!(
            store.read("account:http://127.0.0.1:1/").as_deref(),
            Some("cookie")
        );
        store.remove("account:http://127.0.0.1:1/");
        assert_eq!(store.read("account:http://127.0.0.1:1/"), None);
    }

    #[test]
    fn keys_are_the_other_platforms_names() {
        assert_eq!(session_key("https://a/"), "account:https://a/");
        assert_eq!(passphrase_key("https://a/"), "passphrase:https://a/");
    }
}
