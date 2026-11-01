//! Putting a downloaded build in the running AppImage's place.
//!
//! The build is downloaded beside the image, in the same folder, so that it
//! is on the same file system and a rename moves it into place: the path of
//! the image is the old build or the new one at every instant, never a
//! half-written file and never nothing, whatever happens to the machine in
//! between. The image being replaced is the one running, which is safe: the
//! runtime holds it open (mounted, or unpacked from it), and a rename over
//! its name leaves what it holds alone.
//!
//! The image replaced is kept beside the new one as `<image>.old` until the
//! new one has started (`remove_replaced`, from `at_start`), so that a build
//! that does not start on this machine is not the only one left on it: the
//! relaunch puts the old one back (relaunch.rs). It is kept by a hard link
//! made before the rename, so that keeping it does not open a moment with
//! no image at the path; a file system with no hard links (a FAT stick)
//! moves the image aside first and puts it back if the second rename fails.

use std::{
    fs::{self, File},
    io,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
};

/// Where the parts of one image's update go: all beside it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Beside {
    pub image: PathBuf,
}

impl Beside {
    pub fn new(image: &Path) -> Beside {
        Beside {
            image: image.to_owned(),
        }
    }

    fn folder(&self) -> &Path {
        self.image.parent().unwrap_or_else(|| Path::new("/"))
    }

    fn name(&self) -> String {
        self.image
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Tomoshibi.AppImage".to_owned())
    }

    /// Where the core downloads the build to: hidden, in the image's own
    /// folder. The core writes `<this>.<hash>.part` while it downloads and
    /// renames it here once it is the build offered, byte for byte.
    pub fn download(&self) -> PathBuf {
        self.folder().join(format!(".{}.update", self.name()))
    }

    /// The image this one replaces, kept until the new one has started.
    pub fn old(&self) -> PathBuf {
        let mut name = self.image.as_os_str().to_owned();
        name.push(".old");
        PathBuf::from(name)
    }

    /// Whether a new file can be made in the image's folder, which is
    /// asked before downloading anything: a build that arrives whole and
    /// cannot be put in place is fifty megabytes for nothing.
    pub fn writable(&self) -> io::Result<()> {
        let probe = self
            .folder()
            .join(format!(".{}.{}.probe", self.name(), std::process::id()));
        File::create(&probe)?;
        fs::remove_file(&probe)
    }

    /// Removes what an earlier run downloaded and did not put in place: a
    /// whole build and any part of one. A part could be carried on from,
    /// but the core names parts by the build's hash, so one left by a build
    /// no longer offered would sit there for ever; and nothing on disk from
    /// before this run is trusted to be the build offered now (Windows'
    /// Updates.Start clears its folder for the same reason).
    pub fn clear_downloads(&self) {
        let download = self.download();
        let stem = download
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let _ = fs::remove_file(&download);
        if let Ok(entries) = fs::read_dir(self.folder()) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().into_owned();
                if name.starts_with(&format!("{stem}.")) && name.ends_with(".part") {
                    let _ = fs::remove_file(entry.path());
                }
            }
        }
    }

    /// Puts the downloaded build in the image's place, executable, and
    /// keeps the image it replaces as [`old`](Self::old).
    pub fn put_in_place(&self) -> io::Result<()> {
        let new = self.download();
        let image = &self.image;
        let old = self.old();

        // The image's own mode, so that an image somebody made executable
        // for themselves alone stays so, with the owner's execute bit made
        // sure of: an image that cannot be run is no update.
        let mode = fs::metadata(image)
            .map(|meta| meta.permissions().mode() & 0o7777)
            .unwrap_or(0o755)
            | 0o700;
        fs::set_permissions(&new, fs::Permissions::from_mode(mode))?;
        // On the disk before its name is: a crash after the rename must not
        // leave the image's name on a file whose contents never got there.
        File::open(&new)?.sync_all()?;

        match fs::remove_file(&old) {
            Ok(()) => {}
            Err(fault) if fault.kind() == io::ErrorKind::NotFound => {}
            Err(fault) => return Err(fault),
        }
        if fs::hard_link(image, &old).is_ok() {
            fs::rename(&new, image)?;
        } else {
            fs::rename(image, &old)?;
            if let Err(fault) = fs::rename(&new, image) {
                let _ = fs::rename(&old, image);
                return Err(fault);
            }
        }
        if let Ok(folder) = File::open(self.folder()) {
            let _ = folder.sync_all();
        }
        Ok(())
    }

    /// Removes the image the running one replaced, now that the running one
    /// has started; whether there was one.
    pub fn remove_replaced(&self) -> io::Result<bool> {
        match fs::remove_file(self.old()) {
            Ok(()) => Ok(true),
            Err(fault) if fault.kind() == io::ErrorKind::NotFound => Ok(false),
            Err(fault) => Err(fault),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("tomoshibi-place-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn the_new_build_takes_the_name_and_the_old_is_kept_beside_it() {
        let dir = scratch("swap");
        let image = dir.join("Tomoshibi-0.1.7-x86_64.AppImage");
        fs::write(&image, b"the old build").unwrap();
        fs::set_permissions(&image, fs::Permissions::from_mode(0o750)).unwrap();
        let beside = Beside::new(&image);
        assert_eq!(
            beside.download(),
            dir.join(".Tomoshibi-0.1.7-x86_64.AppImage.update")
        );
        fs::write(beside.download(), b"the new build").unwrap();

        beside.put_in_place().unwrap();

        assert_eq!(fs::read(&image).unwrap(), b"the new build");
        assert_eq!(fs::read(beside.old()).unwrap(), b"the old build");
        assert!(!beside.download().exists(), "moved, not copied");
        let mode = fs::metadata(&image).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o750, "the image's own mode, executable by its owner");

        assert!(beside.remove_replaced().unwrap());
        assert!(!beside.old().exists());
        assert!(!beside.remove_replaced().unwrap(), "nothing left to remove");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_build_that_cannot_be_moved_leaves_the_image_as_it_was() {
        let dir = scratch("missing");
        let image = dir.join("Tomoshibi.AppImage");
        fs::write(&image, b"the old build").unwrap();
        let beside = Beside::new(&image);

        // Nothing was downloaded: the move fails and the image is untouched.
        assert!(beside.put_in_place().is_err());
        assert_eq!(fs::read(&image).unwrap(), b"the old build");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn what_an_earlier_run_downloaded_is_cleared_and_nothing_else() {
        let dir = scratch("clear");
        let image = dir.join("Tomoshibi.AppImage");
        fs::write(&image, b"the build").unwrap();
        let beside = Beside::new(&image);
        let part = dir.join(".Tomoshibi.AppImage.update.0123456789abcdef.part");
        fs::write(beside.download(), b"whole").unwrap();
        fs::write(&part, b"half").unwrap();
        fs::write(dir.join("notes.txt"), b"somebody's").unwrap();

        beside.clear_downloads();

        assert!(!beside.download().exists());
        assert!(!part.exists());
        assert!(image.exists() && dir.join("notes.txt").exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_folder_that_takes_no_new_file_says_so() {
        let dir = scratch("readonly");
        let image = dir.join("Tomoshibi.AppImage");
        fs::write(&image, b"the build").unwrap();
        let beside = Beside::new(&image);
        assert!(beside.writable().is_ok());

        fs::set_permissions(&dir, fs::Permissions::from_mode(0o555)).unwrap();
        // Root writes anywhere, and the tests may run as root in a container.
        let root = fs::write(dir.join("probe"), b"").is_ok();
        if !root {
            assert!(beside.writable().is_err());
        }
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o755)).unwrap();
        let _ = fs::remove_dir_all(&dir);
    }
}
