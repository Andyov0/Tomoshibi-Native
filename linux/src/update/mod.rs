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
/// manager's install, an image unpacked by hand), which is never replaced:
/// the offer then says where to get the new build instead.
///
/// `$APPIMAGE` is trusted only when this process is that image's: its
/// program is inside `$APPDIR`, and `$APPDIR` is that image's own mount
/// (the runtime mounts it under the image's file name) or, where the image
/// was unpacked rather than mounted, the runtime that unpacked it is this
/// process's parent and is that image. The variable is inherited by
/// whatever an AppImage starts: Tomoshibi started from another AppImage
/// application's terminal, while itself running unpacked or from a
/// distribution's package, saw that application's image there, and an
/// update would have been renamed over it and the desktop entry pointed
/// at it.
pub fn running_image() -> Option<PathBuf> {
    static RUNNING: std::sync::OnceLock<Option<PathBuf>> = std::sync::OnceLock::new();
    RUNNING
        .get_or_init(|| {
            let image = PathBuf::from(std::env::var_os("APPIMAGE")?);
            let appdir = PathBuf::from(std::env::var_os("APPDIR")?);
            let program = std::env::current_exe().ok()?;
            let mounts = std::fs::read_to_string("/proc/self/mounts").unwrap_or_default();
            let parent = std::fs::read_link(format!(
                "/proc/{}/exe",
                std::os::unix::process::parent_id()
            ))
            .ok();
            let trusted = trusted_image(&image, &appdir, &program, &mounts, parent.as_deref());
            if !trusted {
                log("update: APPIMAGE names an image this process is not running from; not using it");
            }
            trusted.then_some(image)
        })
        .clone()
}

/// [`running_image`]'s judgement, on what it read.
pub fn trusted_image(
    image: &Path,
    appdir: &Path,
    program: &Path,
    mounts: &str,
    parent: Option<&Path>,
) -> bool {
    if !image.is_absolute() || !appdir.is_absolute() || !program.starts_with(appdir) {
        return false;
    }
    let Some(name) = image.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    let mounted = mounts.lines().any(|line| {
        let mut fields = line.split(' ');
        let (Some(source), Some(point), Some(kind)) = (fields.next(), fields.next(), fields.next())
        else {
            return false;
        };
        kind.starts_with("fuse")
            && Path::new(&unescape_mount(point)) == appdir
            && {
                let source = unescape_mount(source);
                source == name || Path::new(&source) == image
            }
    });
    let unpacked = appdir
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with("appimage_extracted_"))
        && parent.is_some_and(|parent| {
            parent == image
                || std::fs::canonicalize(image).is_ok_and(|image| image == parent)
        });
    mounted || unpacked
}

/// A path as /proc/mounts writes it: a space, a tab, a line break and a
/// backslash as three octal digits after a backslash.
fn unescape_mount(field: &str) -> String {
    let bytes = field.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'\\' && index + 4 <= bytes.len() {
            let digits = &bytes[index + 1..index + 4];
            if digits.iter().all(|digit| (b'0'..=b'7').contains(digit)) {
                let code = digits
                    .iter()
                    .fold(0u32, |code, digit| code * 8 + (digit - b'0') as u32);
                out.push(code as u8);
                index += 4;
                continue;
            }
        }
        out.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
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
    fn an_image_is_trusted_only_by_the_process_running_from_it() {
        let image = Path::new("/home/ann/Apps/Tomoshibi-0.1.7-x86_64.AppImage");
        let appdir = Path::new("/tmp/.mount_TomoshAbCdEf");
        let program = Path::new("/tmp/.mount_TomoshAbCdEf/usr/bin/tomoshibi");
        let mounts = "proc /proc proc rw 0 0\nTomoshibi-0.1.7-x86_64.AppImage /tmp/.mount_TomoshAbCdEf fuse.Tomoshibi-0.1.7-x86_64.AppImage ro,nosuid,nodev 0 0\n";
        assert!(trusted_image(image, appdir, program, mounts, None));
        // Another application's image, inherited: its mount is not ours.
        let other = Path::new("/home/ann/Apps/Editor.AppImage");
        assert!(!trusted_image(other, appdir, program, mounts, None));
        // Running unpacked by hand, with another image's variables.
        let unpacked = Path::new("/home/ann/squashfs-root/usr/bin/tomoshibi");
        assert!(!trusted_image(image, appdir, unpacked, mounts, None));
        // Unpacked by the runtime, which is the parent and is the image.
        let extracted = Path::new("/tmp/appimage_extracted_0123456789abcdef0123456789abcdef");
        let inside = extracted.join("usr/bin/tomoshibi");
        assert!(trusted_image(image, extracted, &inside, "", Some(image)));
        assert!(!trusted_image(image, extracted, &inside, "", Some(Path::new("/usr/bin/bash"))));
        // A path with a space, as /proc/mounts escapes it.
        let spaced = Path::new("/tmp/.mount_Tomosh X");
        let mounts = "Tomoshibi.AppImage /tmp/.mount_Tomosh\\040X fuse.Tomoshibi.AppImage ro 0 0\n";
        assert!(trusted_image(
            Path::new("/opt/Tomoshibi.AppImage"),
            spaced,
            &spaced.join("usr/bin/tomoshibi"),
            mounts,
            None
        ));
    }

    #[test]
    fn a_mount_s_path_is_read_back_whole() {
        assert_eq!(unescape_mount("/tmp/a\\040b\\134c"), "/tmp/a b\\c");
        assert_eq!(unescape_mount("/home/\u{5c71}\u{7530}/x"), "/home/\u{5c71}\u{7530}/x");
        assert_eq!(unescape_mount("/end\\04"), "/end\\04");
    }

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
