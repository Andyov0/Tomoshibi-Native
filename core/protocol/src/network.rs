//! What the platforms' network monitors say, turned into the one thing the
//! call needs to hear: the device's default path has changed.
//!
//! A call's signalling socket and its media are on whatever interface carried
//! the default route when they were opened. When that route moves (Wi-Fi to
//! mobile data, a cable plugged in, the device back online after none), nothing
//! in the call notices until the server's ping goes unanswered, and a call that
//! was already reconnecting sits out its next wait of up to eight seconds
//! before it tries the network that has just come back. The web hears the
//! browser's `online` event and resumes or retries at once; the media crate's
//! `Call::network_changed` is the same thing for the apps.
//!
//! But a monitor is not an `online` event. Apple's path monitor, Android's
//! default network callback and Windows' network status event each speak far
//! more often than the route changes: on every change of a path's properties,
//! once per step of an interface coming up, several times for one Wi-Fi
//! hand-over, and once on being started. Each `network_changed` in a connected
//! call is a resume, the media stalling while it is done, so passing every
//! update on would interrupt a call that had nothing wrong with it, several
//! times over. So the platforms only report what they see, every time, and
//! this decides: a sighting that has stood for [`SETTLE`] without another is
//! compared with the last one that stood, and the call is told only when the
//! device has come back online or its default route is another one. The first
//! sighting is the baseline and says nothing.
//!
//! A Mac waking from sleep is the one change a monitor may not report at all:
//! the route is the one it went to sleep on, and the call's sockets died with
//! the sleep. [`NetworkWatch::woke`] makes the next settled sighting count as a
//! change, or the one already standing if the device is online.

use std::{
    sync::{Arc, Mutex, MutexGuard, Weak},
    time::Duration,
};

/// What a platform's monitor says of the device's default path, each time it
/// says anything.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct NetworkPath {
    /// Whether there is a default path that can carry the call.
    pub online: bool,
    /// What the default path goes through, in the platform's own terms: an
    /// interface and its gateways on Apple, the network's handle on Android,
    /// the adapter on Windows. Two sightings with the same route are the same
    /// path; its content means nothing here. Ignored while offline.
    pub route: String,
}

/// How long a sighting has to stand, without another one arriving, before it
/// is acted on. Long enough to take a burst of updates for one change as one
/// (a Wi-Fi hand-over is a few within a hundred milliseconds or so), and short
/// against the waits it cuts short, the shortest of which after the first two
/// is over a second.
pub const SETTLE: Duration = Duration::from_millis(500);

/// Whether the call is to be told the network changed: going from `before`,
/// the last sighting that stood (none before the first), to `now`, with
/// `woke` saying whether the device has woken from sleep since a sighting last
/// counted.
pub fn path_changed(before: Option<&NetworkPath>, now: &NetworkPath, woke: bool) -> bool {
    if !now.online {
        return false;
    }
    match before {
        // The first sighting is what the network already was when the call
        // began; there is nothing to come back from.
        None => woke,
        Some(before) => woke || !before.online || before.route != now.route,
    }
}

#[derive(Default)]
struct State {
    /// The last sighting that stood for [`SETTLE`].
    settled: Option<NetworkPath>,
    /// The latest sighting, standing or not.
    latest: Option<NetworkPath>,
    /// Woken from sleep since a sighting last counted.
    woke: bool,
    /// Moved on by every sighting, so a settling sighting knows it was
    /// overtaken.
    generation: u64,
}

/// One call's view of the network, fed every sighting by the platform.
#[derive(Default)]
pub struct NetworkWatch {
    state: Mutex<State>,
}

impl NetworkWatch {
    /// A sighting. `tell` is run once, on the core's runtime, if this sighting
    /// stands for [`SETTLE`] and is a change ([`path_changed`]); otherwise it
    /// is dropped.
    pub fn seen(self: &Arc<Self>, path: NetworkPath, tell: impl FnOnce() + Send + 'static) {
        let generation = self.note(path);
        self.settle_after(generation, tell);
    }

    /// The device woke from sleep: the sighting standing now counts as a
    /// change if it is online, or else the next one that is.
    pub fn woke(self: &Arc<Self>, tell: impl FnOnce() + Send + 'static) {
        if let Some(generation) = self.note_wake() {
            self.settle_after(generation, tell);
        }
    }

    /// Notes a sighting and returns its number, for [`Self::settle`]. `seen`
    /// is this and the wait; the tests settle by hand.
    pub fn note(&self, path: NetworkPath) -> u64 {
        let mut state = self.lock();
        state.latest = Some(path);
        state.generation += 1;
        state.generation
    }

    /// Notes a wake, and returns the number to settle, if anything has been
    /// seen to settle.
    pub fn note_wake(&self) -> Option<u64> {
        let mut state = self.lock();
        state.woke = true;
        state.latest.as_ref()?;
        state.generation += 1;
        Some(state.generation)
    }

    /// Settles sighting `generation` if nothing came after it, and says
    /// whether the call is to be told.
    pub fn settle(&self, generation: u64) -> bool {
        let mut state = self.lock();
        if state.generation != generation {
            return false;
        }
        let Some(now) = state.latest.clone() else { return false };
        let changed = path_changed(state.settled.as_ref(), &now, state.woke);
        // A wake counts once, against the first sighting that can carry the
        // call; one taken offline waits for the network to be back.
        if now.online {
            state.woke = false;
        }
        state.settled = Some(now);
        changed
    }

    fn settle_after(self: &Arc<Self>, generation: u64, tell: impl FnOnce() + Send + 'static) {
        let watch: Weak<Self> = Arc::downgrade(self);
        crate::runtime::spawn(async move {
            tokio::time::sleep(SETTLE).await;
            if watch.upgrade().is_some_and(|watch| watch.settle(generation)) {
                tell();
            }
        });
    }

    /// Everything held here is replaced whole, so a lock a panic poisoned is
    /// still a usable one.
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|poison| poison.into_inner())
    }
}
