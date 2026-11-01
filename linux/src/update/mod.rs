//! The app's own updates: the AppImage replacing itself.
//!
//! The core does the part every app shares (core/protocol/src/api/
//! updates.rs): it asks the deployment for the newest Linux build
//! (`UpdatePlatform::Linux`, the manifest's `linux` section), compares
//! versions, downloads in resumable ranges, and moves nothing into place that
//! is not, byte for byte, a build the release key signed. What is this app's
//! own (as Windows' AppUpdates.cs and the Mac's disk image swap are theirs):
//! downloading beside the running AppImage (`$APPIMAGE`), replacing it
//! atomically with a rename on the same file system, keeping it executable,
//! and starting the new one; and the desktop integration a first run
//! installs (a .desktop file with `x-scheme-handler/tomoshibi`, the icons).
//!
//! Owned by the update module (with linux/build/appimage.sh, which makes
//! the image). The skeleton gives the check, which the lobby's update offer
//! can already use, and the shape of the rest ([`Installer`], [`relaunch`],
//! [`integrate_desktop`]); the update module writes their insides.

use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use tomoshibi_protocol::{
    api::{AppBuild, Deployment, UpdateCheck, UpdateError, UpdatePlatform, UpdateProgress},
    ProtocolError,
};

use crate::app::client;

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

/// One install of one build over the running AppImage: the download (the
/// core's, resumable, refused unless it is byte for byte what the release
/// key signed), then the replacement. What the lobby's update offer holds
/// while it shows progress; kept in the model (`AppModel::kept`) by
/// whoever starts it, so that a screen built again finds it.
pub struct Installer {
    build: AppBuild,
    image: PathBuf,
    deployment: Arc<Deployment>,
}

impl Installer {
    /// An install of `build`, or `None` when the app is not running from an
    /// AppImage and there is nothing to replace.
    pub fn new(deployment: Arc<Deployment>, build: AppBuild) -> Option<Installer> {
        let image = running_image()?;
        Some(Installer {
            build,
            image,
            deployment,
        })
    }

    /// The build being installed.
    pub fn build(&self) -> &AppBuild {
        &self.build
    }

    /// The AppImage that will be replaced.
    pub fn image(&self) -> &Path {
        &self.image
    }

    /// Downloads the build beside the image and puts it in the image's
    /// place, keeping it executable; returns once the new image is there,
    /// for [`relaunch`]. `progress` is called on the core's threads.
    ///
    /// Not written yet: says so without downloading anything.
    pub async fn run(&self, progress: Arc<dyn UpdateProgress>) -> Result<(), UpdateError> {
        let _ = (&self.deployment, progress);
        Err(UpdateError::Unexpected {
            detail: "installing an AppImage is not written yet".into(),
        })
    }

    /// Calls the download off; `run` returns `UpdateError::Cancelled` soon
    /// after, and what had arrived is kept for the next attempt.
    pub fn cancel(&self) {}
}

/// Starts the AppImage at `image` (the one just put in place) with the
/// links this run was opened with, and lets this one quit.
///
/// Not written yet.
pub fn relaunch(image: &Path) -> std::io::Result<()> {
    let _ = image;
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "relaunching is not written yet",
    ))
}

/// Installs what the desktop needs to start the app and to hand it links:
/// a .desktop file in the user's applications directory naming the running
/// AppImage, with `x-scheme-handler/tomoshibi` among its MIME types, and
/// the icons under the hicolor theme. Done once at a start from an
/// AppImage, and again when the image has moved.
///
/// Not written yet.
pub fn integrate_desktop() -> std::io::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_linux_app_asks_for_linux_builds() {
        assert_eq!(PLATFORM, UpdatePlatform::Linux);
    }
}
