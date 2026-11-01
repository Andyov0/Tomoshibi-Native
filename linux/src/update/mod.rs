//! The app's own updates: the AppImage replacing itself.
//!
//! The core does the part every app shares (core/protocol/src/api/
//! updates.rs): it asks the deployment for the newest Linux build of this
//! architecture (`UpdatePlatform::Linux`, the manifest's `linux-x86_64` or
//! `linux-aarch64`), compares versions, downloads in resumable ranges, and
//! moves nothing into place that is not, byte for byte, a build the release
//! key signed. What is this app's own (as Windows' AppUpdates.cs and the
//! Mac's disk image swap are theirs): downloading beside the running
//! AppImage (`$APPIMAGE`), replacing it atomically with a rename on the same
//! file system and keeping it executable (place.rs), and starting the new
//! one once this one has gone, with the old one put back if the new one
//! fails at once (relaunch.rs); the offer the lobby shows and its schedule
//! (offer.rs); and the desktop integration a first run asks about
//! (desktop.rs).
//!
//! Owned by the update module, with linux/build/appimage.sh, which makes
//! the image.
//!
//! What the other modules call: `at_start` once the window is open (main.rs);
//! the offer's state and actions in `offer` (`offer::step`, `offer::shown`,
//! `offer::install`, `offer::later`, `offer::cancel`, `offer::check_now`,
//! `offer::holds_calls`, `offer::notes`), told by `Change::Update`; the
//! desktop's in `desktop` (`desktop::integrated`, `desktop::integrate_desktop`);
//! and `relaunch::launch_context` for opening a web page with the system's
//! own environment.

pub mod desktop;
pub mod offer;
pub mod place;
pub mod relaunch;

use std::{
    io,
    path::{Path, PathBuf},
    sync::Arc,
};

use tomoshibi_protocol::{
    api::{
        AppBuild, Deployment, UpdateCheck, UpdateDownload, UpdateError, UpdatePlatform,
        UpdateProgress,
    },
    ProtocolError,
};

use crate::app::{client, log, AppModel};

pub use desktop::integrate_desktop;
pub use relaunch::relaunch;

/// The platform this app asks for.
pub const PLATFORM: UpdatePlatform = UpdatePlatform::Linux;

/// Whether the deployment has a newer build of this app than the one
/// running.
pub async fn check(deployment: Arc<Deployment>) -> Result<UpdateCheck, ProtocolError> {
    deployment
        .check_for_update(PLATFORM, client::dotted_version())
        .await
}

/// The AppImage this process runs from, which is what an update replaces.
/// `None` when the app runs from anywhere else (a build tree, a package
/// manager's install), which is never replaced: the offer then says where
/// to get the new build instead.
pub fn running_image() -> Option<PathBuf> {
    let path = PathBuf::from(std::env::var_os("APPIMAGE")?);
    path.is_absolute().then_some(path)
}

/// What a Storage error says when the image's folder takes no new file,
/// for `offer::explain` to say so in a sentence of its own.
const NOT_ALLOWED: &str = "not allowed to write in the image's folder";

/// An error from the file system as the core's kind of error.
fn storage(fault: &io::Error) -> UpdateError {
    UpdateError::Storage {
        detail: if fault.kind() == io::ErrorKind::PermissionDenied
            || fault.raw_os_error() == Some(30)
        {
            // EACCES, EPERM, and EROFS (30): a folder of somebody else's, or
            // a read-only mount.
            format!("{NOT_ALLOWED}: {fault}")
        } else {
            fault.to_string()
        },
    }
}

/// One install of one build over the running AppImage: the download (the
/// core's, resumable, refused unless it is byte for byte what the release
/// key signed), then the replacement. What the offer holds while it shows
/// progress.
pub struct Installer {
    build: AppBuild,
    beside: place::Beside,
    download: Arc<UpdateDownload>,
}

impl Installer {
    /// An install of `build`, or `None` when the app is not running from an
    /// AppImage and there is nothing to replace.
    pub fn new(deployment: Arc<Deployment>, build: AppBuild) -> Option<Installer> {
        let image = running_image()?;
        let beside = place::Beside::new(&image);
        let download = deployment.download_update(
            build.clone(),
            beside.download().to_string_lossy().into_owned(),
        );
        Some(Installer {
            build,
            beside,
            download,
        })
    }

    /// The build being installed.
    pub fn build(&self) -> &AppBuild {
        &self.build
    }

    /// The AppImage that will be replaced.
    pub fn image(&self) -> &Path {
        &self.beside.image
    }

    /// Whether the image's folder takes a new file, which a replacement
    /// needs; asked before downloading.
    pub fn can_write(&self) -> io::Result<()> {
        self.beside.writable()
    }

    /// Downloads the build beside the image and puts it in the image's
    /// place, keeping it executable and keeping the image it replaces as
    /// `<image>.old` until the new one starts; returns once the new image is
    /// there, for [`relaunch`]. `progress` is called on the core's threads.
    pub async fn run(&self, progress: Arc<dyn UpdateProgress>) -> Result<(), UpdateError> {
        self.can_write().map_err(|fault| storage(&fault))?;
        self.download.clone().run(progress).await?;
        // On a thread of its own: putting it in place waits for some fifty
        // megabytes to reach the disk (place.rs syncs the file before its
        // name), which held the window still on the main thread.
        let beside = self.beside.clone();
        gio::spawn_blocking(move || beside.put_in_place())
            .await
            .unwrap_or_else(|_| Err(io::Error::other("the update's writer stopped")))
            .map_err(|fault| storage(&fault))
    }

    /// Calls the download off; `run` returns `UpdateError::Cancelled` soon
    /// after, and what had arrived is kept for the next attempt.
    pub fn cancel(&self) {
        self.download.cancel();
    }
}

/// How long the new version's window shows before the image it replaced is
/// removed: long enough to say it started on this machine, which is what
/// keeping the old one was for (relaunch.rs watches for the removal).
const STARTED_AFTER_MS: u64 = 2000;

/// What a start does about updates, once the main window is open: removes
/// the image this one replaced, now that this one has started; starts
/// looking for the next build; and asks about the desktop once.
pub fn at_start(model: &AppModel, window: &adw::ApplicationWindow) {
    if let Some(image) = running_image() {
        let beside = place::Beside::new(&image);
        if beside.old().exists() {
            glib::timeout_add_local_once(
                std::time::Duration::from_millis(STARTED_AFTER_MS),
                move || match beside.remove_replaced() {
                    Ok(true) => log(&format!(
                        "update: removed the image this one replaced; {} has started",
                        client::version_label()
                    )),
                    Ok(false) => {}
                    Err(fault) => log(&format!(
                        "update: the image this one replaced could not be removed: {fault}"
                    )),
                },
            );
        }
    }
    offer::start(model);
    desktop::at_start(model, window);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_linux_app_asks_for_linux_builds() {
        assert_eq!(PLATFORM, UpdatePlatform::Linux);
    }

    #[test]
    fn a_folder_of_somebody_else_s_is_said_as_such() {
        let denied = io::Error::from(io::ErrorKind::PermissionDenied);
        assert_eq!(
            offer::explain(&storage(&denied)),
            "You are not allowed to change Tomoshibi where it is. Move the AppImage into a folder of your own, or ask whoever manages this computer to update it."
        );
        let read_only = io::Error::from_raw_os_error(30);
        assert!(offer::explain(&storage(&read_only)).starts_with("You are not allowed"));
        let other = io::Error::other("disk on fire");
        assert_eq!(
            offer::explain(&storage(&other)),
            "The update could not be saved on this device."
        );
    }

    #[test]
    fn a_build_nobody_signed_is_said_as_such() {
        let unsigned = UpdateError::Unexpected {
            detail: "the newest build is not signed by the release key".into(),
        };
        assert_eq!(
            offer::explain(&unsigned),
            "The update is not signed by Tomoshibi's developer, so it was not installed."
        );
    }
}
