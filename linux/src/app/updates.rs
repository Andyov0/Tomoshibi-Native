//! The app's own updates as the lobby offers them (Updates.cs, Updates.kt):
//! when to look, what to offer, and the download's progress. The download
//! and the replacing of the AppImage are the update module's (update/);
//! this is the offer.
//!
//! Nothing about an update interrupts a call: an offer is shown only before
//! one (the lobby's sidebar foot, the sign-in's head), one found during a
//! call waits for the lobby, and while a build is being put in place no
//! call is begun, since the install ends the app.
//!
//! Asked once at start and every six hours after, or as often as the
//! deployment says (update_check_hours). A check that fails is kept quiet and
//! asked again in ten minutes: a notice for each would teach somebody to
//! ignore the one that matters. "Later" puts that version away until the app
//! is next started; a newer one is offered all the same. A check somebody
//! asks for in the settings always answers, "You have the latest version."
//! included, and brings back a version put away. The deployment can switch
//! the whole of it off (app-updates): then nothing is looked for or offered,
//! but a download already under way is seen through.

use std::{
    cell::{Cell, RefCell},
    collections::HashSet,
    rc::Rc,
    sync::Arc,
    time::Duration,
};

use gio::prelude::*;
use tomoshibi_protocol::api::{AppBuild, UpdateCheck, UpdateError, UpdateProgress};

use super::{log, remote, AppModel, Change, Screen};
use crate::{i18n::t, update};

pub const EVERY: Duration = Duration::from_secs(6 * 3600);
pub const RETRY: Duration = Duration::from_secs(10 * 60);

/// Where an update has got to.
#[derive(Debug, Clone, PartialEq)]
pub enum Step {
    None,
    /// A newer build, and what went wrong with the last try at it.
    Offered {
        build: AppBuild,
        problem: Option<String>,
    },
    Downloading {
        build: AppBuild,
        received: u64,
        total: u64,
    },
    /// Downloaded and being put in place; the app starts again after.
    Installing { build: AppBuild },
}

/// What a check somebody asked for found, for the settings to say.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Said {
    Checking,
    Current,
    Failed,
}

/// What the model keeps (`AppModel::kept`).
pub struct UpdateState {
    pub step: RefCell<Step>,
    pub said: Cell<Option<Said>>,
    later: RefCell<HashSet<String>>,
    started: Cell<bool>,
    /// Moved on by every new schedule of looks, so that a wait in the one
    /// before ends in nothing.
    schedule: Cell<u64>,
    installer: RefCell<Option<Rc<update::Installer>>>,
}

impl Default for UpdateState {
    fn default() -> Self {
        UpdateState {
            step: RefCell::new(Step::None),
            said: Cell::new(None),
            later: RefCell::new(HashSet::new()),
            started: Cell::new(false),
            schedule: Cell::new(0),
            installer: RefCell::new(None),
        }
    }
}

/// The model's own change for the offer.
pub const CHANGED: Change = Change::Update;

fn allowed(model: &AppModel) -> bool {
    remote::allows(model, remote::feature::APP_UPDATES)
}

/// The build being offered, downloaded or installed.
pub fn build(model: &AppModel) -> Option<AppBuild> {
    match &*model.kept::<UpdateState>().step.borrow() {
        Step::None => None,
        Step::Offered { build, .. }
        | Step::Downloading { build, .. }
        | Step::Installing { build } => Some(build.clone()),
    }
}

/// Whether the offer is up: something to say, before a call, and not put
/// away; with updates switched off, only a download already under way.
pub fn shown(model: &AppModel) -> bool {
    if !matches!(model.screen(), Screen::Lobby | Screen::SignIn) {
        return false;
    }
    let state = model.kept::<UpdateState>();
    let step = state.step.borrow();
    match &*step {
        Step::None => false,
        Step::Offered { build, .. } => {
            allowed(model) && !state.later.borrow().contains(&build.version)
        }
        _ => true,
    }
}

/// Whether the settings say anything about updates.
pub fn speaks(model: &AppModel) -> bool {
    allowed(model)
        || matches!(
            &*model.kept::<UpdateState>().step.borrow(),
            Step::Downloading { .. } | Step::Installing { .. }
        )
}

/// Whether a call may not be begun now.
pub fn holds_calls(model: &AppModel) -> bool {
    matches!(
        &*model.kept::<UpdateState>().step.borrow(),
        Step::Installing { .. }
    )
}

fn set(model: &AppModel, step: Step) {
    model.kept::<UpdateState>().step.replace(step);
    model.announce(CHANGED);
}

/// Starts looking: now, and on the schedule after. Once.
pub fn start(model: &AppModel) {
    let state = model.kept::<UpdateState>();
    if state.started.replace(true) {
        return;
    }
    schedule(model, Duration::ZERO);
    // The deployment's interval, or the switch, changing: a new schedule.
    let weak = model.downgrade();
    model.subscribe(move |_, change| {
        if change == Change::Remote {
            if let Some(model) = weak.upgrade() {
                schedule(&model, interval(&model));
            }
        }
    });
}

fn interval(model: &AppModel) -> Duration {
    model
        .kept::<remote::RemoteState>()
        .answer
        .borrow()
        .update_interval()
        .unwrap_or(EVERY)
}

fn schedule(model: &AppModel, first: Duration) {
    let state = model.kept::<UpdateState>();
    let mine = state.schedule.get() + 1;
    state.schedule.set(mine);
    let weak = model.downgrade();
    glib::spawn_future_local(async move {
        let mut wait = first;
        loop {
            glib::timeout_future(wait).await;
            let Some(model) = weak.upgrade() else {
                return;
            };
            if model.kept::<UpdateState>().schedule.get() != mine {
                return;
            }
            wait = match look(&model, false).await {
                Some(_) => interval(&model),
                None => RETRY,
            };
        }
    });
}

/// A check somebody asked for in the settings: always answers, and brings
/// back a version put away.
pub fn check_now(model: &AppModel) {
    let state = model.kept::<UpdateState>();
    state.later.borrow_mut().clear();
    state.said.set(Some(Said::Checking));
    model.announce(CHANGED);
    let model = model.clone();
    glib::spawn_future_local(async move {
        let found = look(&model, true).await;
        let state = model.kept::<UpdateState>();
        state.said.set(match found {
            None => Some(Said::Failed),
            Some(false) => Some(Said::Current),
            Some(true) => None,
        });
        model.announce(CHANGED);
    });
}

/// Asks once: whether a newer build was found, or none where the
/// deployment could not be asked.
async fn look(model: &AppModel, asked: bool) -> Option<bool> {
    if !asked && !allowed(model) {
        return Some(false);
    }
    let deployment = model.deployment()?;
    match update::check(deployment).await {
        Ok(UpdateCheck::UpToDate) => {
            if matches!(&*model.kept::<UpdateState>().step.borrow(), Step::Offered { .. }) {
                set(model, Step::None);
            }
            Some(false)
        }
        Ok(UpdateCheck::Available { build }) => {
            let busy = matches!(
                &*model.kept::<UpdateState>().step.borrow(),
                Step::Downloading { .. } | Step::Installing { .. }
            );
            if !busy {
                log(&format!("a newer build is offered: {}", build.version));
                set(model, Step::Offered {
                    build,
                    problem: None,
                });
            }
            Some(true)
        }
        Err(fault) => {
            log(&format!("the update check was not answered: {fault}"));
            None
        }
    }
}

/// "Later": this version is put away until the app is next started.
pub fn later(model: &AppModel) {
    if let Some(build) = build(model) {
        model
            .kept::<UpdateState>()
            .later
            .borrow_mut()
            .insert(build.version);
    }
    model.announce(CHANGED);
}

/// Progress on the core's threads, sent to the main thread.
struct Progress(tokio::sync::mpsc::UnboundedSender<(u64, u64)>);

impl UpdateProgress for Progress {
    fn progressed(&self, received: u64, total: u64) {
        let _ = self.0.send((received, total));
    }
}

/// "Update": downloads the build and puts it in the place of the running
/// AppImage, then starts it. An app not running from an AppImage cannot
/// replace itself, and is sent to the deployment's page to download it.
pub fn accept(model: &AppModel) {
    let Some(deployment) = model.deployment() else {
        return;
    };
    let Some(build) = build(model) else {
        return;
    };
    let Some(installer) = update::Installer::new(deployment.clone(), build.clone()) else {
        log("not running from an AppImage: the offer opens the deployment's page");
        model.open_in_browser(&deployment.base_url());
        return;
    };
    let installer = Rc::new(installer);
    model
        .kept::<UpdateState>()
        .installer
        .replace(Some(installer.clone()));
    let total = build.size;
    set(model, Step::Downloading {
        build: build.clone(),
        received: 0,
        total,
    });
    let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
    let weak = model.downgrade();
    glib::spawn_future_local(async move {
        while let Some((received, total)) = receiver.recv().await {
            let Some(model) = weak.upgrade() else {
                return;
            };
            let state = model.kept::<UpdateState>();
            let still = matches!(&*state.step.borrow(), Step::Downloading { .. });
            if still {
                state.step.replace(Step::Downloading {
                    build: build.clone(),
                    received,
                    total,
                });
                model.announce(CHANGED);
            }
        }
    });
    let weak = model.downgrade();
    glib::spawn_future_local(async move {
        let ran = installer.run(Arc::new(Progress(sender))).await;
        let Some(model) = weak.upgrade() else {
            return;
        };
        model.kept::<UpdateState>().installer.replace(None);
        let Some(build) = self::build(&model) else {
            return;
        };
        match ran {
            Ok(()) => {
                set(&model, Step::Installing { build });
                match update::relaunch(installer.image()) {
                    Ok(()) => {
                        log("the new build is in place; starting it");
                        if let Some(application) = gio::Application::default() {
                            application.quit();
                        }
                    }
                    Err(fault) => {
                        log(&format!("the new build could not be started: {fault}"));
                        set(&model, Step::Offered {
                            build: self::build(&model).unwrap_or_else(|| installer.build().clone()),
                            problem: Some(t("The update could not be installed. Try again.")),
                        });
                    }
                }
            }
            Err(UpdateError::Cancelled) => set(&model, Step::Offered {
                build,
                problem: None,
            }),
            Err(fault) => {
                log(&format!("the update did not arrive: {fault}"));
                set(&model, Step::Offered {
                    build,
                    problem: Some(t("The update could not be installed. Try again.")),
                });
            }
        }
    });
}

/// Calls off a download; what had arrived is kept for the next try.
pub fn cancel(model: &AppModel) {
    if let Some(installer) = model.kept::<UpdateState>().installer.borrow().as_ref() {
        installer.cancel();
    }
}
