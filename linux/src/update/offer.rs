//! The app's own updates as the screens see them: looking for a newer build,
//! offering it, and with the person's agreement downloading it, putting it
//! in the AppImage's place and starting it (Windows' Tomoshibi.Basics
//! Updates.cs, Android's Updates.kt, the Mac's Updates.swift, rule for
//! rule where Linux has the same thing to decide).
//!
//! Asked once the deployment has answered and every six hours after (the
//! deployment may say otherwise, `check_every`); a check that fails is kept
//! quiet and asked again in ten minutes, since a notice for each would teach
//! somebody to ignore the one that matters. "Later" puts the version away
//! until the app is next started; a newer one is offered all the same. A
//! check asked for in the settings always answers, "You have the latest
//! version." included (`said`), and brings back a version put away.
//!
//! The owner's rule above the rest: nothing about an update interrupts a
//! call. The offer is the lobby's to show, and only there (`shown`); a
//! download is held when the person goes towards a call and carried on,
//! from where it stopped, when they are back; and from the moment the new
//! build is being put in place no call is begun (`holds_calls`), since what
//! follows ends the app and starts the new one.
//!
//! An app not running from an AppImage (a build tree, a package somebody
//! made of it) cannot replace itself: its offer is the download page
//! instead (`installable`, `open_download_page`).
//!
//! Everything here is on the main thread. The core's download says how far
//! it has got on its own threads, through a channel to here, as the room's
//! call events do.
//!
//! The screens read the state with the functions below and are told of
//! every change by `Change::Update`. The deployment's remote switch
//! (`app-updates`) and number (`update_check_hours`) arrive through
//! `allow` and `check_every`, from whoever reads the remote configuration.

use std::{cell::RefCell, collections::HashSet, rc::Rc, sync::Arc, time::Duration};

use tomoshibi_protocol::api::{update_notes, AppBuild, UpdateCheck, UpdateError, UpdateProgress};

use super::{place::Beside, Installer};
use crate::{
    app::{log, AppModel, Change, Screen},
    i18n::{self, t},
};

/// How often to look, unless the deployment says (`check_every`).
pub const EVERY: Duration = Duration::from_secs(6 * 60 * 60);
/// How soon to look again after a look that failed.
pub const RETRY: Duration = Duration::from_secs(10 * 60);
/// How long the bar shows full before the app closes to start the new
/// build: a build that arrived faster than the bar went across was put in
/// place with the bar a third of the way, and the last thing somebody saw
/// of the download was a leap to the app closing (Updates.cs FillAtMost).
const FULL_FOR: Duration = Duration::from_millis(900);

/// Where an update has got to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    None,
    /// A newer build, and what went wrong with the last try at it, if
    /// anything did.
    Offered {
        build: AppBuild,
        problem: Option<String>,
    },
    Downloading {
        build: AppBuild,
        received: u64,
        total: u64,
    },
    /// A download the person left the lobby during: stopped, kept, and
    /// carried on when they are back.
    Held {
        build: AppBuild,
        received: u64,
        total: u64,
    },
    /// The build has arrived and is being put in place; the app closes and
    /// the new one opens next.
    Installing {
        build: AppBuild,
    },
}

impl Step {
    /// The build offered, downloaded or installed, whatever the step.
    pub fn build(&self) -> Option<&AppBuild> {
        match self {
            Step::None => None,
            Step::Offered { build, .. }
            | Step::Downloading { build, .. }
            | Step::Held { build, .. }
            | Step::Installing { build } => Some(build),
        }
    }
}

/// What a check somebody asked for found, for the settings to say.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Said {
    Checking,
    Current,
    Failed,
}

/// What the model keeps for the updates (`AppModel::kept`).
pub struct Updates {
    state: RefCell<State>,
}

impl Default for Updates {
    fn default() -> Self {
        Updates {
            state: RefCell::new(State {
                step: Step::None,
                said: None,
                rolled_back: false,
                later: HashSet::new(),
                allowed: true,
                every: EVERY,
                schedule: 0,
                started: false,
                installer: None,
                transfer: 0,
            }),
        }
    }
}

struct State {
    step: Step,
    said: Option<Said>,
    /// This run is the version an update was rolled back to (relaunch.rs):
    /// the update it offers again did not start on this machine.
    rolled_back: bool,
    /// Versions put away with "Later" until the next start.
    later: HashSet<String>,
    allowed: bool,
    every: Duration,
    /// Moved on by every new schedule of looks, so that a wait in the one
    /// before ends in nothing.
    schedule: u64,
    started: bool,
    /// The download under way, for its Cancel.
    installer: Option<Rc<Installer>>,
    /// Moved on by every download, so that the progress and the end of one
    /// that was called off are not taken for the next one's.
    transfer: u64,
}

fn updates(model: &AppModel) -> Rc<Updates> {
    model.kept::<Updates>()
}

fn set(model: &AppModel, step: Step) {
    updates(model).state.borrow_mut().step = step;
    model.announce(Change::Update);
}

// ---- what the screens read ---------------------------------------------

/// Where the update has got to.
pub fn step(model: &AppModel) -> Step {
    updates(model).state.borrow().step.clone()
}

/// The answer to the last check asked for, while the settings show it.
pub fn said(model: &AppModel) -> Option<Said> {
    updates(model).state.borrow().said
}

/// Whether the lobby's offer is up: something to say, on a screen before
/// any call, and not put away; with updates switched off, only a download
/// or an install that was already under way.
pub fn shown(model: &AppModel) -> bool {
    if !in_the_lobby(model.screen()) {
        return false;
    }
    let updates = updates(model);
    let state = updates.state.borrow();
    match &state.step {
        Step::None | Step::Held { .. } => false,
        Step::Offered { build, .. } => state.allowed && !state.later.contains(&build.version),
        Step::Downloading { .. } | Step::Installing { .. } => true,
    }
}

/// Whether a call may not be begun now: while the new build is being put
/// in place, after which the app closes. The lobby refuses to begin one
/// and says why ("Tomoshibi is installing an update and will open again in
/// a moment. Start or join the meeting then.").
pub fn holds_calls(model: &AppModel) -> bool {
    matches!(step(model), Step::Installing { .. })
}

/// Whether the settings say anything about updates: unless they are
/// switched off, and then still while one is under way, for its progress
/// and its Cancel.
pub fn speaks(model: &AppModel) -> bool {
    updates(model).state.borrow().allowed
        || matches!(
            step(model),
            Step::Downloading { .. } | Step::Held { .. } | Step::Installing { .. }
        )
}

/// Whether this app can put a build in its own place: it runs from an
/// AppImage. Otherwise the offer's button is the download page.
pub fn installable() -> bool {
    super::running_image().is_some()
}

/// Why the offer's button is the download page rather than "Update", for
/// the lobby to say beside it; `None` where the app can replace itself.
pub fn why_not_installable() -> Option<String> {
    (!installable()).then(|| {
        t("Tomoshibi is not running from an AppImage, so it cannot update itself. Download the new version instead.")
    })
}

/// What changed in the build offered, in the reader's language, else in
/// English, else nothing (the core's update_notes).
pub fn notes(model: &AppModel) -> Option<String> {
    let build = step(model).build()?.clone();
    update_notes(build, i18n::language().code().to_owned())
}

/// The version offered as people are told it, "0.1.7 (2)".
pub fn version_label(build: &AppBuild) -> String {
    tomoshibi_protocol::api::version_label(build.version.clone())
}

/// The deployment's download page, for an app that cannot replace itself.
pub fn download_page(model: &AppModel) -> Option<String> {
    let deployment = model.deployment()?;
    Some(format!("{}apps", deployment.base_url()))
}

/// Opens the download page in the browser, with the system's environment
/// rather than this image's (relaunch.rs `launch_context`).
pub fn open_download_page(model: &AppModel) {
    let Some(page) = download_page(model) else {
        return;
    };
    let context = super::relaunch::launch_context();
    if let Err(fault) = gio::AppInfo::launch_default_for_uri(&page, Some(&context)) {
        log(&format!(
            "update: the download page could not be opened: {fault}"
        ));
    }
}

// ---- what the screens do -------------------------------------------------

/// Starts looking, once for the life of the model: as soon as the
/// deployment has answered, then every six hours. And holds a download when
/// the person heads for a call, and carries it on when they are back.
pub fn start(model: &AppModel) {
    {
        let updates = updates(model);
        let mut state = updates.state.borrow_mut();
        if state.started {
            return;
        }
        state.started = true;
    }
    // Whatever an earlier run downloaded was put in place, failed, or was
    // left behind; none of it is trusted to be the build offered now.
    if let Some(image) = super::running_image() {
        Beside::new(&image).clear_downloads();
    }
    let waiting_for_deployment = std::cell::Cell::new(true);
    model.subscribe(move |model, change| match change {
        Change::Info if waiting_for_deployment.get() && model.deployment().is_some() => {
            waiting_for_deployment.set(false);
            if updates(model).state.borrow().allowed {
                arrange(model, true);
            }
        }
        Change::Screen => screen_changed(model),
        _ => {}
    });
}

/// A check asked for (the settings' "Check for updates"): always answers,
/// and brings back a version put away.
pub fn check_now(model: &AppModel) {
    {
        let updates = updates(model);
        let mut state = updates.state.borrow_mut();
        state.later.clear();
        state.said = Some(Said::Checking);
    }
    model.announce(Change::Update);
    let model = model.clone();
    glib::spawn_future_local(async move {
        check(&model, true).await;
    });
}

/// "Later": the version offered is not offered again until the next start.
pub fn later(model: &AppModel) {
    if let Some(build) = step(model).build() {
        updates(model)
            .state
            .borrow_mut()
            .later
            .insert(build.version.clone());
        model.announce(Change::Update);
    }
}

/// "Update": downloads the build offered and puts it in place, then the app
/// closes and the new one opens. Outside an AppImage, opens the download
/// page instead.
pub fn install(model: &AppModel) {
    let Step::Offered { build, .. } = step(model) else {
        return;
    };
    if !installable() {
        log("update: not running from an AppImage; opening the download page instead");
        open_download_page(model);
        return;
    }
    download(model, build, 0, 0);
}

/// Calls the download off; what had arrived is kept for the next try.
pub fn cancel(model: &AppModel) {
    let installer = {
        let updates = updates(model);
        let mut state = updates.state.borrow_mut();
        state.transfer += 1;
        state.installer.take()
    };
    if let Some(installer) = installer {
        installer.cancel();
    }
    if let Some(build) = step(model).build().cloned() {
        if !matches!(step(model), Step::Installing { .. }) {
            set(
                model,
                Step::Offered {
                    build,
                    problem: None,
                },
            );
        }
    }
}

/// Whether to look for and offer updates (the deployment's app-updates).
/// Switched off, the looking stops and an offer is taken off the screen; a
/// download or an install under way goes on to its end. Switched on again,
/// the app looks at once, since the offer it missed may be days old.
pub fn allow(model: &AppModel, on: bool) {
    let (was, started) = {
        let updates = updates(model);
        let mut state = updates.state.borrow_mut();
        let was = state.allowed;
        state.allowed = on;
        if !on {
            state.schedule += 1;
        }
        (was, state.started)
    };
    if was != on {
        model.announce(Change::Update);
        if on && started && model.deployment().is_some() {
            arrange(model, true);
        }
    }
}

/// How often to look (the deployment's update_check_hours); nought or less
/// keeps the six hours.
pub fn check_every(model: &AppModel, hours: f64) {
    let every = if hours.is_finite() && hours > 0.0 {
        Duration::from_secs_f64(hours * 3600.0)
    } else {
        EVERY
    };
    let (changed, allowed, started) = {
        let updates = updates(model);
        let mut state = updates.state.borrow_mut();
        let changed = state.every != every;
        state.every = every;
        (changed, state.allowed, state.started)
    };
    if changed && allowed && started && model.deployment().is_some() {
        arrange(model, false);
    }
}

/// The sentence for a download that did not finish (Updates.cs `Sentence`,
/// with the Linux cases of its own).
pub fn explain(fault: &UpdateError) -> String {
    match fault {
        UpdateError::Network { .. } => t("The download stopped. Check your connection and try again."),
        UpdateError::Mismatch { .. } => t("The download was damaged and has been deleted. Try again."),
        UpdateError::Storage { detail } if detail.contains(super::NOT_ALLOWED) => t(
            "You are not allowed to change Tomoshibi where it is. Move the AppImage into a folder of your own, or ask whoever manages this computer to update it.",
        ),
        UpdateError::Storage { detail } if detail.contains("No space") => {
            t("There is not enough room on this device for the update.")
        }
        UpdateError::Storage { .. } => t("The update could not be saved on this device."),
        UpdateError::Unexpected { detail } if detail.contains("signed") => {
            t("The update is not signed by Tomoshibi's developer, so it was not installed.")
        }
        _ => t("The update could not be downloaded. Try again later."),
    }
}

// ---- the inside ----------------------------------------------------------

/// A screen before any call, where an offer may show and a download runs.
fn in_the_lobby(screen: Screen) -> bool {
    matches!(screen, Screen::SignIn | Screen::Lobby | Screen::Unreachable)
}

/// The looking, every `every`, from now or from one wait from now. Any
/// schedule before is given up: its wait ends, it sees it is not the
/// schedule any more, and looks no more.
fn arrange(model: &AppModel, looking_first: bool) {
    let mine = {
        let updates = updates(model);
        let mut state = updates.state.borrow_mut();
        state.schedule += 1;
        state.schedule
    };
    let model = model.downgrade();
    glib::spawn_future_local(async move {
        let current = |model: &AppModel| updates(model).state.borrow().schedule == mine;
        if !looking_first {
            let Some(every) = model
                .upgrade()
                .map(|model| updates(&model).state.borrow().every)
            else {
                return;
            };
            glib::timeout_future(every).await;
        }
        loop {
            let Some(strong) = model.upgrade() else {
                return;
            };
            if !current(&strong) {
                return;
            }
            let answered = check(&strong, false).await;
            if !current(&strong) {
                return;
            }
            let wait = if answered {
                updates(&strong).state.borrow().every
            } else {
                RETRY
            };
            drop(strong);
            glib::timeout_future(wait).await;
        }
    });
}

/// One look; whether the deployment answered.
async fn check(model: &AppModel, asked: bool) -> bool {
    let Some(deployment) = model.deployment() else {
        if asked {
            updates(model).state.borrow_mut().said = Some(Said::Failed);
            model.announce(Change::Update);
        }
        return false;
    };
    let answer = super::check(deployment).await;
    let busy = matches!(
        step(model),
        Step::Downloading { .. } | Step::Held { .. } | Step::Installing { .. }
    );
    match answer {
        Ok(UpdateCheck::Available { build }) => {
            log(&format!(
                "update: {} is offered ({} bytes)",
                build.version, build.size
            ));
            if asked {
                updates(model).state.borrow_mut().said = None;
            }
            // A download under way of the same build goes on; of an older
            // one, it goes on too, and the next look offers the newer.
            if !busy {
                // Offered again after it failed to start here: said, so
                // that the offer is not taken for one nobody has tried.
                let problem = updates(model)
                    .state
                    .borrow()
                    .rolled_back
                    .then(|| t("This update does not run on this device."));
                set(
                    model,
                    Step::Offered {
                        build: build.clone(),
                        problem,
                    },
                );
                debug_install(model);
            } else if asked {
                model.announce(Change::Update);
            }
            true
        }
        Ok(UpdateCheck::UpToDate) => {
            if asked {
                updates(model).state.borrow_mut().said = Some(Said::Current);
            }
            if !busy && step(model) != Step::None {
                set(model, Step::None);
            } else if asked {
                model.announce(Change::Update);
            }
            true
        }
        Err(fault) => {
            // Not answering and answering with a build nobody signed are
            // both failures here, said as what they were.
            log(&format!("update: the check failed: {fault}"));
            if asked {
                updates(model).state.borrow_mut().said = Some(Said::Failed);
                model.announce(Change::Update);
            }
            false
        }
    }
}

/// A build offered as if the deployment had one, in a debug build: for a
/// picture of the lobby's offer (`TOMOSHIBI_DEBUG_LOBBY=update`), since
/// the local deployments the tests run have no Linux build to offer.
#[cfg(debug_assertions)]
pub fn pose(model: &AppModel, build: AppBuild) {
    set(
        model,
        Step::Offered {
            build,
            problem: None,
        },
    );
}

/// `TOMOSHIBI_DEBUG_UPDATE=install`, in a debug build: takes an offered
/// build at once, as the button would, for the update's end-to-end test.
fn debug_install(model: &AppModel) {
    #[cfg(debug_assertions)]
    if std::env::var("TOMOSHIBI_DEBUG_UPDATE").as_deref() == Ok("install") {
        // Not the build that has just failed to start here: the switch is
        // inherited by the version put back, which installed it again,
        // round and round.
        if updates(model).state.borrow().rolled_back {
            log("update: not installing again what did not start here");
            return;
        }
        log("update: installing, as TOMOSHIBI_DEBUG_UPDATE asked");
        install(model);
    }
    #[cfg(not(debug_assertions))]
    let _ = model;
}

/// What the core's download says on its own threads, sent to the main one.
struct Progress(tokio::sync::mpsc::UnboundedSender<(u64, u64)>);

impl UpdateProgress for Progress {
    fn progressed(&self, received: u64, total: u64) {
        let _ = self.0.send((received, total));
    }
}

fn download(model: &AppModel, build: AppBuild, received: u64, total: u64) {
    let Some(deployment) = model.deployment() else {
        return;
    };
    let Some(installer) = Installer::new(deployment, build.clone()) else {
        return;
    };
    // Asked before anything is downloaded: a build that arrives whole and
    // cannot be put in place is fifty megabytes for nothing.
    if let Err(fault) = installer.can_write() {
        log(&format!(
            "update: the image's folder takes no new file: {fault}"
        ));
        set(
            model,
            Step::Offered {
                build,
                problem: Some(explain(&super::storage(&fault))),
            },
        );
        return;
    }
    let installer = Rc::new(installer);
    let this = {
        let updates = updates(model);
        let mut state = updates.state.borrow_mut();
        state.transfer += 1;
        state.installer = Some(installer.clone());
        state.transfer
    };
    let total = if total > 0 { total } else { build.size };
    set(
        model,
        Step::Downloading {
            build: build.clone(),
            received,
            total,
        },
    );
    log(&format!("update: downloading {}", build.version));

    let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
    let progress: Arc<dyn UpdateProgress> = Arc::new(Progress(sender));
    let current = {
        let model = model.downgrade();
        move || {
            model
                .upgrade()
                .filter(|model| updates(model).state.borrow().transfer == this)
        }
    };
    {
        let current = current.clone();
        let build = build.clone();
        glib::spawn_future_local(async move {
            while let Some((received, total)) = receiver.recv().await {
                let Some(model) = current() else {
                    return;
                };
                if matches!(step(&model), Step::Downloading { .. }) {
                    set(
                        &model,
                        Step::Downloading {
                            build: build.clone(),
                            received,
                            total,
                        },
                    );
                }
            }
        });
    }
    glib::spawn_future_local(async move {
        let result = installer.run(progress).await;
        let Some(model) = current() else {
            // Called off or held meanwhile; that path has said what it is.
            return;
        };
        updates(&model).state.borrow_mut().installer = None;
        match result {
            Ok(()) => {
                log(&format!(
                    "update: {} is in place of {}",
                    build.version,
                    installer.image().display()
                ));
                set(
                    &model,
                    Step::Installing {
                        build: build.clone(),
                    },
                );
                glib::timeout_future(FULL_FOR).await;
                start_the_new_one(&model, &build, installer.image());
            }
            Err(UpdateError::Cancelled) => {}
            Err(UpdateError::Superseded) => {
                log("update: a newer build was shelved meanwhile; looking again");
                set(&model, Step::None);
                glib::spawn_future_local(async move {
                    check(&model, false).await;
                });
            }
            Err(fault) => {
                log(&format!("update: the download did not finish: {fault}"));
                set(
                    &model,
                    Step::Offered {
                        build: build.clone(),
                        problem: Some(explain(&fault)),
                    },
                );
            }
        }
    });
}

/// This run is the version an update was rolled back to, because the new
/// one did not start (main.rs reads the relaunch's mark).
pub fn rolled_back(model: &AppModel) {
    log("update: the new version did not start here; this is the one it replaced, put back");
    updates(model).state.borrow_mut().rolled_back = true;
}

/// Starts the new build once this one has gone, and closes this one.
fn start_the_new_one(model: &AppModel, build: &AppBuild, image: &std::path::Path) {
    match super::relaunch(image) {
        Ok(()) => {
            log(&format!("update: closing, for {} to start", build.version));
            if let Some(application) = gio::Application::default() {
                use gio::prelude::*;
                application.quit();
            }
        }
        Err(fault) => {
            // The new build is in place and this one goes on; the next start
            // is the new one.
            log(&format!(
                "update: the new version could not be started: {fault}"
            ));
            set(
                model,
                Step::Offered {
                    build: build.clone(),
                    problem: Some(t(
                        "Tomoshibi was updated but could not open again by itself. Open it again.",
                    )),
                },
            );
        }
    }
}

/// A download is held when the person heads for a call, and carried on
/// when they are back before any call.
fn screen_changed(model: &AppModel) {
    let screen = model.screen();
    match step(model) {
        Step::Downloading {
            build,
            received,
            total,
        } if !in_the_lobby(screen) => {
            log("update: holding the download for the call");
            let installer = {
                let updates = updates(model);
                let mut state = updates.state.borrow_mut();
                state.transfer += 1;
                state.installer.take()
            };
            if let Some(installer) = installer {
                installer.cancel();
            }
            set(
                model,
                Step::Held {
                    build,
                    received,
                    total,
                },
            );
        }
        Step::Held {
            build,
            received,
            total,
        } if in_the_lobby(screen) => {
            log("update: carrying the download on");
            download(model, build, received, total);
        }
        _ => {}
    }
}
