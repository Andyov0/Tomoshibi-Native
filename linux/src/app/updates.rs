//! The app's own updates as the lobby and the settings speak of them.
//!
//! The offer itself, its schedule, the download and the replacing of the
//! AppImage are the update module's (update/offer.rs, update/mod.rs): one
//! state for the whole app, so that the lobby's offer, the settings' row and
//! the refusal to begin a call while a build is put in place all read the
//! same thing. What is the app model's is the deployment's word on it: the
//! remote configuration's app-updates switch and update_check_hours, handed
//! to the offer whenever the configuration is heard (`start`).
//!
//! Nothing about an update interrupts a call (update/offer.rs says how): an
//! offer is shown only before one, a download is held while one is under
//! way, and no call is begun while the new build is being put in place.

use tomoshibi_protocol::api::AppBuild;

use super::{remote, AppModel, Change};
use crate::update::offer;

pub use crate::update::offer::{
    cancel, check_now, holds_calls, installable, later, notes, said, shown, speaks, step,
    version_label, why_not_installable, Said, Step,
};

/// Hands the deployment's word on updates to the offer, now and whenever
/// the remote configuration is heard again. Once, when the deployment
/// first answers (`AppModel::started`); the offer's own schedule is
/// started with the window (update::at_start).
pub fn start(model: &AppModel) {
    apply(model);
    let weak = model.downgrade();
    model.subscribe(move |_, change| {
        if change == Change::Remote {
            if let Some(model) = weak.upgrade() {
                apply(&model);
            }
        }
    });
}

fn apply(model: &AppModel) {
    offer::allow(model, remote::allows(model, remote::feature::APP_UPDATES));
    // The remote configuration's interval, already kept from going under
    // the ten minutes after a failed look; none is the offer's six hours.
    let hours = model
        .kept::<remote::RemoteState>()
        .answer
        .borrow()
        .update_interval()
        .map_or(0.0, |every| every.as_secs_f64() / 3600.0);
    offer::check_every(model, hours);
}

/// The build being offered, downloaded or installed.
pub fn build(model: &AppModel) -> Option<AppBuild> {
    step(model).build().cloned()
}

/// "Update": downloads the build and puts it in the place of the running
/// AppImage, then starts it; an app not running from an AppImage opens the
/// deployment's download page instead.
pub fn accept(model: &AppModel) {
    offer::install(model);
}

/// `TOMOSHIBI_DEBUG_LOBBY=update`, in a debug run: a build offered as if
/// the deployment had one, for a picture of the offer. The local deployments
/// the tests run have no Linux build to offer; nothing is downloaded unless
/// somebody presses Update, and then the install is refused (no AppImage).
#[cfg(debug_assertions)]
pub fn pose_offer(model: &AppModel) {
    let build = AppBuild {
        platform: crate::update::PLATFORM,
        version: "0.1.8".into(),
        size: 98_600_000,
        sha256: String::new(),
        notes: [(
            "en".to_owned(),
            "Calls start faster on a slow network.".to_owned(),
        )]
        .into_iter()
        .collect(),
        file_name: String::new(),
        signature: String::new(),
    };
    offer::pose(model, build);
}
