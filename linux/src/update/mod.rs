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
//! Owned by the update module. The skeleton gives the check, which the
//! lobby's update offer can already use; downloading and replacing are to
//! be written here.

use std::{path::PathBuf, sync::Arc};

use tomoshibi_protocol::{
    api::{AppBuild, Deployment, UpdateCheck, UpdatePlatform},
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

/// What became of an install.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Installed {
    /// The new AppImage is in place; [`relaunch`] starts it.
    Ready,
    /// Not running from an AppImage; nothing was replaced.
    NotAnImage,
}

/// Downloads `build` and puts it in place of the running AppImage.
/// Not written yet: says it was not an image, which the offer reads as "get
/// the new build from the download page".
pub async fn install(_deployment: Arc<Deployment>, _build: AppBuild) -> Result<Installed, String> {
    Ok(Installed::NotAnImage)
}

/// Starts the AppImage now in place and quits this one.
pub fn relaunch() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_linux_app_asks_for_linux_builds() {
        assert_eq!(PLATFORM, UpdatePlatform::Linux);
    }
}
