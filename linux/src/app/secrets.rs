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
//! Never on the main thread. Every call into libsecret is a D-Bus round
//! trip, and with the keyring locked it shows the desktop's unlock prompt
//! and waits for somebody to answer it: made from the main thread, at start
//! and at every key pressed in the guest's passphrase field, that froze the
//! window (no redraw, "not responding") for as long as the prompt stood. So
//! one worker thread of its own makes every call; writes return at once and
//! a burst of writes to one key is one write of the last value; a read the
//! app can wait for is [`Secrets::read_soon`], awaited on the main loop.
//!
//! A machine with no Secret Service at all (a bare compositor, a container,
//! a desktop whose keyring is not running) still runs the app: what cannot
//! be stored is held for this run only, and the next start asks somebody to
//! sign in again. That is said once in the log rather than to the person,
//! who can do nothing about it from here.

use std::{
    cell::RefCell,
    collections::HashMap,
    ffi::{c_char, c_int, c_void, CStr, CString},
    future::Future,
    pin::Pin,
    ptr,
    rc::Rc,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc, Mutex, OnceLock,
    },
};

use super::log;

/// What a read hands back, to await on the main loop.
pub type Soon = Pin<Box<dyn Future<Output = Option<String>>>>;

/// A store of small secrets by key. Called on the main thread.
pub trait Secrets {
    /// What is kept under `key`. May wait on the store: for a read made
    /// while somebody waits for an answer anyway (a join being prepared).
    fn read(&self, key: &str) -> Option<String>;
    /// Keeps `value` under `key`; returns at once.
    fn write(&self, key: &str, value: &str);
    /// Forgets `key`; returns at once.
    fn remove(&self, key: &str);
    /// [`read`](Self::read) without holding up the caller's thread: the
    /// answer arrives when the store gives it.
    fn read_soon(&self, key: &str) -> Soon {
        let found = self.read(key);
        Box::pin(std::future::ready(found))
    }
    /// Waits, for at most `within`, until every write already made has
    /// reached the store; false when they have not. For the app's last
    /// moment: a write still queued when the process ends is lost.
    fn settle(&self, within: std::time::Duration) -> bool {
        let _ = within;
        true
    }
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

/// What the worker does with libsecret: the three calls, behind a trait so
/// that a test can stand a slow or locked keyring in for it.
pub trait Backend: Send + Sync + 'static {
    fn lookup(&self, key: &str) -> Result<Option<String>, String>;
    fn store(&self, key: &str, value: &str) -> Result<(), String>;
    fn clear(&self, key: &str) -> Result<(), String>;
}

/// libsecret itself.
struct Libsecret;

impl Backend for Libsecret {
    fn lookup(&self, key: &str) -> Result<Option<String>, String> {
        ffi::lookup(key)
    }

    fn store(&self, key: &str, value: &str) -> Result<(), String> {
        ffi::store(key, value)
    }

    fn clear(&self, key: &str) -> Result<(), String> {
        ffi::clear(key)
    }
}

/// A write still to be made, the latest for its key.
enum Pending {
    Store(String),
    Clear,
}

enum Job {
    /// Make the pending write for this key, if it is still there.
    Flush(String),
    /// Say so: every job queued before this one has been done.
    Done(mpsc::Sender<()>),
    Lookup(
        String,
        Box<dyn FnOnce(Result<Option<String>, String>) + Send>,
    ),
}

/// The worker's side: the writes waiting, the backend, and whether it has
/// failed, all shared with the thread.
struct Shared {
    pending: Mutex<HashMap<String, Pending>>,
    backend: Box<dyn Backend>,
    /// Set once the service has failed, so that the log says it once and
    /// the rest of the run does not wait on a bus that is not there.
    failed: AtomicBool,
}

impl Shared {
    fn failed(&self, what: &str, fault: String) {
        if !self.failed.swap(true, Ordering::SeqCst) {
            log(&format!(
                "the Secret Service could not {what} ({fault}); secrets are kept for this run only"
            ));
        }
    }

    fn run(&self, job: Job) {
        match job {
            Job::Flush(key) => {
                let Some(write) = self
                    .pending
                    .lock()
                    .ok()
                    .and_then(|mut held| held.remove(&key))
                else {
                    return;
                };
                if self.failed.load(Ordering::SeqCst) {
                    return;
                }
                let done = match &write {
                    Pending::Store(value) => self.backend.store(&key, value),
                    Pending::Clear => self.backend.clear(&key),
                };
                if let Err(fault) = done {
                    match write {
                        Pending::Store(_) => self.failed("keep a secret", fault),
                        Pending::Clear => self.failed("forget a secret", fault),
                    }
                }
            }
            Job::Done(said) => {
                let _ = said.send(());
            }
            Job::Lookup(key, answer) => {
                if self.failed.load(Ordering::SeqCst) {
                    answer(Err("the Secret Service failed earlier".into()));
                    return;
                }
                let found = self.backend.lookup(&key);
                if let Err(fault) = &found {
                    self.failed("be read", fault.clone());
                }
                answer(found);
            }
        }
    }
}

/// The Secret Service, with what this run knows in front of it: everything
/// written is known at once, so that a store that is slow, locked or gone
/// still reads back what was written in this run, and a key once read is
/// not asked for again.
pub struct SecretService {
    /// A value, or `None` for a key known to hold nothing; a key not here
    /// has not been asked about yet.
    known: Rc<RefCell<HashMap<String, Option<String>>>>,
    shared: Arc<Shared>,
    /// The worker, started at the first call that needs it.
    jobs: OnceLock<mpsc::Sender<Job>>,
}

impl Default for SecretService {
    fn default() -> Self {
        SecretService::with_backend(Box::new(Libsecret))
    }
}

impl SecretService {
    pub fn with_backend(backend: Box<dyn Backend>) -> SecretService {
        SecretService {
            known: Rc::default(),
            shared: Arc::new(Shared {
                pending: Mutex::new(HashMap::new()),
                backend,
                failed: AtomicBool::new(false),
            }),
            jobs: OnceLock::new(),
        }
    }

    fn failed(&self) -> bool {
        self.shared.failed.load(Ordering::SeqCst)
    }

    /// Hands a job to the worker; false when there is no worker to hand it
    /// to (the thread could not be started), which is a store that failed.
    fn send(&self, job: Job) -> bool {
        let jobs = self.jobs.get_or_init(|| {
            let (sender, receiver) = mpsc::channel::<Job>();
            let shared = self.shared.clone();
            let started = std::thread::Builder::new()
                .name("tomoshibi-secrets".into())
                .spawn(move || {
                    while let Ok(job) = receiver.recv() {
                        shared.run(job);
                    }
                });
            if let Err(fault) = started {
                self.shared.failed("be reached", fault.to_string());
            }
            sender
        });
        jobs.send(job).is_ok()
    }

    /// Queues the latest write for `key`; one already waiting is replaced,
    /// so that typing a passphrase is one write of what was typed last.
    fn queue(&self, key: &str, write: Pending) {
        if self.failed() {
            return;
        }
        let waiting = match self.shared.pending.lock() {
            Ok(mut held) => held.insert(key.to_owned(), write).is_some(),
            Err(_) => return,
        };
        if !waiting {
            self.send(Job::Flush(key.to_owned()));
        }
    }
}

impl Secrets for SecretService {
    fn read(&self, key: &str) -> Option<String> {
        if let Some(known) = self.known.borrow().get(key) {
            return known.clone();
        }
        if self.failed() {
            return None;
        }
        let (sender, receiver) = mpsc::channel();
        let asked = self.send(Job::Lookup(
            key.to_owned(),
            Box::new(move |found| {
                let _ = sender.send(found);
            }),
        ));
        if !asked {
            return None;
        }
        match receiver.recv() {
            Ok(Ok(found)) => {
                self.known
                    .borrow_mut()
                    .insert(key.to_owned(), found.clone());
                found
            }
            _ => None,
        }
    }

    fn write(&self, key: &str, value: &str) {
        self.known
            .borrow_mut()
            .insert(key.to_owned(), Some(value.to_owned()));
        self.queue(key, Pending::Store(value.to_owned()));
    }

    fn remove(&self, key: &str) {
        self.known.borrow_mut().insert(key.to_owned(), None);
        self.queue(key, Pending::Clear);
    }

    fn read_soon(&self, key: &str) -> Soon {
        if let Some(known) = self.known.borrow().get(key) {
            return Box::pin(std::future::ready(known.clone()));
        }
        if self.failed() {
            return Box::pin(std::future::ready(None));
        }
        let (sender, receiver) = tokio::sync::oneshot::channel();
        let asked = self.send(Job::Lookup(
            key.to_owned(),
            Box::new(move |found| {
                let _ = sender.send(found);
            }),
        ));
        let (known, key) = (self.known.clone(), key.to_owned());
        Box::pin(async move {
            if !asked {
                return None;
            }
            let found = receiver.await.ok()?.ok()?;
            // Written meanwhile: what was written is newer than what was read.
            let mut known = known.borrow_mut();
            known.entry(key).or_insert(found).clone()
        })
    }

    /// The writes are handed to the worker and the call returns, so the
    /// app could end with a passphrase, a session or a proof still queued:
    /// closed straight after signing in, or ended by the session's SIGTERM,
    /// the next start asked somebody to sign in again, and a keyring that
    /// was locked, its unlock prompt standing, lost the write every time.
    /// So the last moment waits for the queue, but not for ever: a prompt
    /// nobody answers must not keep the process alive.
    fn settle(&self, within: std::time::Duration) -> bool {
        // No worker: nothing was ever queued.
        if self.jobs.get().is_none() || self.failed() {
            return true;
        }
        let (said, heard) = mpsc::channel();
        if !self.send(Job::Done(said)) {
            return true;
        }
        heard.recv_timeout(within).is_ok()
    }
}

/// libsecret's "password" calls, with one schema of our own and one
/// attribute, the key. The synchronous calls, made only on the worker
/// thread (above): each is a D-Bus round trip, and one into a locked keyring
/// waits on the desktop's unlock prompt, a separate process, for as long as
/// it stands.
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
            let found =
                unsafe { secret_password_lookupv_sync(schema, table, ptr::null_mut(), error) };
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
        fn a_call_with_no_secret_service_is_an_error_not_a_crash() {
            // The test runs with no session bus at all: libsecret must say
            // so through the GError, which is read and freed here, and the
            // store falls back on this run's memory. A schema laid out
            // wrongly crashes inside libsecret instead.
            if std::env::var_os("DBUS_SESSION_BUS_ADDRESS").is_some() {
                return;
            }
            match lookup("tomoshibi-test-key") {
                Ok(None) | Err(_) => {}
                Ok(Some(found)) => panic!("a secret nobody stored: {found}"),
            }
            assert!(store("tomoshibi-test-key", "value").is_err());
        }

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
    use std::sync::Condvar;

    /// A keyring that holds every call until let go, and counts them.
    #[derive(Default)]
    struct Held {
        open: Mutex<bool>,
        opened: Condvar,
        stored: Mutex<Vec<(String, String)>>,
        lookups: Mutex<usize>,
    }

    impl Held {
        fn wait(&self) {
            let mut open = self.open.lock().unwrap();
            while !*open {
                open = self.opened.wait(open).unwrap();
            }
        }

        fn let_go(&self) {
            *self.open.lock().unwrap() = true;
            self.opened.notify_all();
        }
    }

    struct Fake(Arc<Held>);

    impl Backend for Fake {
        fn lookup(&self, key: &str) -> Result<Option<String>, String> {
            self.0.wait();
            *self.0.lookups.lock().unwrap() += 1;
            Ok(Some(format!("kept {key}")))
        }

        fn store(&self, key: &str, value: &str) -> Result<(), String> {
            self.0.wait();
            self.0
                .stored
                .lock()
                .unwrap()
                .push((key.to_owned(), value.to_owned()));
            Ok(())
        }

        fn clear(&self, _: &str) -> Result<(), String> {
            self.0.wait();
            Ok(())
        }
    }

    fn settle(store: &SecretService) {
        // A lookup queued behind the writes answers once they are made.
        let (sender, receiver) = mpsc::channel();
        store.send(Job::Lookup(
            "settle".into(),
            Box::new(move |_| {
                let _ = sender.send(());
            }),
        ));
        receiver.recv().unwrap();
    }

    #[test]
    fn writes_return_at_once_and_a_burst_to_one_key_is_one_write() {
        let held = Arc::new(Held::default());
        let store = SecretService::with_backend(Box::new(Fake(held.clone())));
        // Typed a key at a time into a locked keyring: nothing waits.
        let started = std::time::Instant::now();
        for typed in ["s", "se", "sec", "secr", "secre", "secret"] {
            store.write("passphrase:http://a/", typed);
        }
        assert!(started.elapsed() < std::time::Duration::from_millis(500));
        assert_eq!(
            store.read("passphrase:http://a/").as_deref(),
            Some("secret")
        );
        held.let_go();
        settle(&store);
        let stored = held.stored.lock().unwrap().clone();
        assert!(
            stored.len() <= 2,
            "a write for every key pressed: {stored:?}"
        );
        assert_eq!(
            stored.last().map(|(_, value)| value.as_str()),
            Some("secret"),
            "the last value typed is the one kept"
        );
    }

    #[test]
    fn a_read_the_app_waits_for_holds_nothing_up() {
        let held = Arc::new(Held::default());
        let store = SecretService::with_backend(Box::new(Fake(held.clone())));
        let mut soon = store.read_soon("account:http://a/");
        let context = glib::MainContext::new();
        // Asked, and not answered while the keyring holds it: the caller
        // carries on.
        assert!(context.block_on(std::future::poll_fn(|cx| {
            std::task::Poll::Ready(soon.as_mut().poll(cx).is_pending())
        })));
        held.let_go();
        assert_eq!(
            context.block_on(soon).as_deref(),
            Some("kept account:http://a/")
        );
        // Known from then on: not asked for again.
        assert_eq!(
            store.read("account:http://a/").as_deref(),
            Some("kept account:http://a/")
        );
        assert_eq!(*held.lookups.lock().unwrap(), 1);
    }

    #[test]
    fn the_last_moment_waits_for_the_writes_but_not_for_ever() {
        let held = Arc::new(Held::default());
        let store = SecretService::with_backend(Box::new(Fake(held.clone())));
        // Nothing queued: nothing to wait for.
        assert!(store.settle(std::time::Duration::from_millis(10)));
        store.write("account:http://a/", "cookie");
        // A keyring that does not answer (locked, its prompt standing):
        // given up on after the time allowed, not waited for.
        let started = std::time::Instant::now();
        assert!(!store.settle(std::time::Duration::from_millis(200)));
        assert!(started.elapsed() < std::time::Duration::from_secs(2));
        // Answered in time: the write is in the store when settle returns.
        let letting = held.clone();
        let opener = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(100));
            letting.let_go();
        });
        assert!(store.settle(std::time::Duration::from_secs(10)));
        assert_eq!(
            held.stored
                .lock()
                .unwrap()
                .last()
                .map(|(_, value)| value.clone()),
            Some("cookie".to_owned())
        );
        opener.join().unwrap();
    }

    #[test]
    fn a_store_that_cannot_be_reached_still_holds_this_runs_secrets() {
        let store = SecretService::default();
        store.shared.failed.store(true, Ordering::SeqCst);
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
