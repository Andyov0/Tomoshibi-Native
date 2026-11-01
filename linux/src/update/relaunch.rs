//! Starting the image that was just put in place, and handing the system's
//! own environment to whatever the app starts.
//!
//! The app is one instance per session (main.rs): a second start hands its
//! links to the first over the session bus and exits. So the new version
//! cannot be started while this one runs, or it would hand itself to this
//! one and vanish, and this one cannot start it after it has quit. A small
//! shell, started on its own and outliving the app, waits for this process
//! to be gone and then starts the image at its path, which is the new build
//! by then.
//!
//! The shell also watches the new one for its first seconds. A new build
//! that exits with a failure before it has removed the image it replaced
//! (place.rs `remove_replaced`, done once its window is up) did not start
//! on this machine, and the shell puts the replaced image back and starts
//! that, so that an update which cannot run here leaves the app as it was
//! rather than leaving nothing that opens.

use std::{
    ffi::OsString,
    io,
    os::unix::process::CommandExt,
    path::Path,
    process::{Command, Stdio},
};

use super::place::Beside;

/// What AppRun names the variables it set by (linux/build/AppRun).
const SET_BY_APPRUN: &str = "TOMOSHIBI_APPRUN_SET";
const ORIGINAL: &str = "TOMOSHIBI_ORIGINAL_";

/// What the AppImage runtime sets for the program it starts, which the next
/// start sets again for itself, and what belongs to this start alone (the
/// desktop's startup notification, spent by this window).
const THIS_START_ONLY: [&str; 6] = [
    "APPDIR",
    "APPIMAGE",
    "ARGV0",
    "OWD",
    "DESKTOP_STARTUP_ID",
    "XDG_ACTIVATION_TOKEN",
];

/// The changes that turn this process's environment back into the one the
/// system gave the image: for each variable, its value, or `None` to remove
/// it. `get` reads this process's environment.
///
/// What AppRun set is put back as it was before (or removed where it was
/// not set), so that a browser opened for a link is not handed this image's
/// pixbuf loaders or an empty GIO module directory, and the new version's
/// AppRun sees the system's values rather than this mount's, which is gone
/// once this process has exited.
pub fn host_environment(
    get: impl Fn(&str) -> Option<OsString>,
    names: impl IntoIterator<Item = OsString>,
) -> Vec<(String, Option<OsString>)> {
    let mut changes: Vec<(String, Option<OsString>)> = Vec::new();
    let set = get(SET_BY_APPRUN)
        .map(|value| value.to_string_lossy().into_owned())
        .unwrap_or_default();
    for name in set.split_whitespace() {
        changes.push((name.to_owned(), get(&format!("{ORIGINAL}{name}"))));
    }
    changes.push((SET_BY_APPRUN.to_owned(), None));
    for name in names {
        let name = name.to_string_lossy();
        if name.starts_with(ORIGINAL) {
            changes.push((name.into_owned(), None));
        }
    }
    for name in THIS_START_ONLY {
        changes.push((name.to_owned(), None));
    }
    // The renderer main.rs chose for a machine without a GPU, which is this
    // app's choice and not the system's.
    if get(RENDERER_CHOSEN).is_some() {
        changes.push(("GSK_RENDERER".to_owned(), None));
        changes.push((RENDERER_CHOSEN.to_owned(), None));
    }
    changes
}

/// Set by the relaunch's shell for the version it put back after the new
/// one failed to start, so that the version put back can say why the
/// update it offers again did not take.
pub const DID_NOT_START: &str = "TOMOSHIBI_UPDATE_DID_NOT_START";

/// Set beside GSK_RENDERER where main.rs chose the renderer.
pub const RENDERER_CHOSEN: &str = "TOMOSHIBI_CHOSE_RENDERER";

/// [`host_environment`] for this process.
pub fn host_environment_now() -> Vec<(String, Option<OsString>)> {
    host_environment(
        |name| std::env::var_os(name),
        std::env::vars_os().map(|(name, _)| name),
    )
}

/// Gives `command` the system's environment rather than this image's.
pub fn as_the_host(command: &mut Command) -> &mut Command {
    for (name, value) in host_environment_now() {
        match value {
            Some(value) => command.env(name, value),
            None => command.env_remove(name),
        };
    }
    command
}

/// A launch context that gives what it starts (a browser for a link, the
/// download page) the system's environment rather than this image's; for
/// `gio::AppInfo::launch_default_for_uri`, which the app uses to open web
/// pages. Without it, a GTK 3 browser started from here was handed the
/// image's pixbuf loaders and an empty GIO module directory.
pub fn launch_context() -> gio::AppLaunchContext {
    use gtk::prelude::*;
    let context: gio::AppLaunchContext = match gtk::gdk::Display::default() {
        Some(display) => display.app_launch_context().upcast(),
        None => gio::AppLaunchContext::new(),
    };
    for (name, value) in host_environment_now() {
        match value {
            Some(value) => context.setenv(name, value),
            None => context.unsetenv(name),
        }
    }
    context
}

/// The shell that starts the new image once this process is gone, and puts
/// the old one back if the new one fails at once. Its arguments: this
/// process's id, the image, the image it replaced, and the tree this
/// version was unpacked into where it ran unpacked (or nothing).
///
/// That tree is removed once the new version has started. The AppImage
/// runtime unpacks an image into /tmp/appimage_extracted_<digest of the
/// image> when it is not mounted (APPIMAGE_EXTRACT_AND_RUN, or no FUSE),
/// and the 2025 runtime leaves it there when the program exits: 183 MB for
/// every version an update replaced. Not while anything still runs from
/// it, which only the same build started again could.
///
/// Waits at most thirty seconds for this process to exit, which takes the
/// time a call takes to be left and the window to close; and watches the
/// new one for fifteen, more than a first start takes to show its window
/// (and to unpack itself, where it runs unpacked) on the slowest machine
/// it was tried on.
const SHELL: &str = r#"
pid=$1 image=$2 old=$3 unpacked=$4
forget_unpacked() {
  case $unpacked in */appimage_extracted_*) ;; *) return 0 ;; esac
  [ -d "$unpacked" ] || return 0
  for exe in /proc/[0-9]*/exe; do
    case $(readlink "$exe" 2>/dev/null) in "$unpacked"/*) return 0 ;; esac
  done
  chmod -R u+w "$unpacked" 2>/dev/null
  rm -rf "$unpacked"
}
waited=0
while kill -0 "$pid" 2>/dev/null; do
  waited=$((waited + 1))
  if [ "$waited" -gt 300 ]; then
    echo "tomoshibi: the app did not exit; the new version was not started" >&2
    exit 1
  fi
  sleep 0.1
done
"$image" &
child=$!
watched=0
while [ "$watched" -lt 150 ]; do
  if ! kill -0 "$child" 2>/dev/null; then
    wait "$child"
    status=$?
    if [ "$status" -ne 0 ] && [ -e "$old" ]; then
      echo "tomoshibi: the new version did not start (exit $status); putting back the one it replaced" >&2
      mv -f "$old" "$image" && TOMOSHIBI_UPDATE_DID_NOT_START=1 exec "$image"
    fi
    [ -e "$old" ] || forget_unpacked
    exit "$status"
  fi
  if [ ! -e "$old" ]; then
    forget_unpacked
    exit 0
  fi
  sleep 0.1
  watched=$((watched + 1))
done
exit 0
"#;

/// Starts the image at `image` once this process has exited (the caller
/// quits the app next), with the system's environment and no arguments:
/// any link this run was opened with has been opened.
pub fn relaunch(image: &Path) -> io::Result<()> {
    relaunch_after(std::process::id(), image, unpacked_tree().as_deref())
}

/// The tree this process runs from where the runtime unpacked its image
/// rather than mounting it.
fn unpacked_tree() -> Option<std::path::PathBuf> {
    super::running_image()?;
    let appdir = std::path::PathBuf::from(std::env::var_os("APPDIR")?);
    let named = appdir
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with("appimage_extracted_"));
    (named && appdir.is_absolute()).then_some(appdir)
}

fn relaunch_after(pid: u32, image: &Path, unpacked: Option<&Path>) -> io::Result<()> {
    let old = Beside::new(image).old();
    let mut command = Command::new("/bin/sh");
    command
        .arg("-c")
        .arg(SHELL)
        .arg("sh")
        .arg(pid.to_string())
        .arg(image)
        .arg(old)
        .arg(unpacked.map(|tree| tree.as_os_str()).unwrap_or_default())
        .stdin(Stdio::null())
        // Its own process group, so that whatever ends the app's group (a
        // terminal's job control, a desktop session ending the app) does
        // not end the shell that is to start the next one.
        .process_group(0);
    as_the_host(&mut command);
    close_inherited(&mut command);
    command.spawn().map(|_| ())
}

/// Starts the image at `image` again, with no arguments, handing it the
/// links this start was given in its environment rather than on its
/// command line, and returns at once; the caller exits next.
///
/// A link that opened the app (tomoshibi://host/?invite=TOKEN, from the
/// desktop entry's %U) was on the command line of the AppImage runtime
/// and of the app for as long as they ran, and /proc/<pid>/cmdline can be
/// read by every user of the machine, where an invitation's token is as
/// good as a key to the room. AppRun moves the links into the environment
/// (readable by this user alone), and the start that has them on its line
/// hands them on here and ends, so that its runtime, whose line it cannot
/// change, ends too a moment later.
pub fn hand_links_over(image: &Path, links: &str) -> io::Result<()> {
    let mut command = Command::new(image);
    command.stdin(Stdio::null()).process_group(0);
    as_the_host(&mut command);
    command.env(OPEN_LINKS, links).env(LINKS_HANDED, "1");
    // This start's activation (the desktop's leave to raise the window it
    // opens, which as_the_host drops as this start's alone) is handed on
    // with the links: the start that shows the window is the next one, and
    // without it a window opened by clicking an invitation was not raised.
    for name in ["XDG_ACTIVATION_TOKEN", "DESKTOP_STARTUP_ID"] {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
    close_inherited(&mut command);
    command.spawn().map(|_| ())
}

/// Where AppRun puts the links a start was given, and the mark of a start
/// that was handed them by [`hand_links_over`].
pub const OPEN_LINKS: &str = "TOMOSHIBI_OPEN_LINKS";
pub const LINKS_HANDED: &str = "TOMOSHIBI_LINKS_HANDED";

/// Closes, in what `command` starts, every descriptor this process was
/// given beyond the three standard ones. Rust marks the descriptors it
/// opens itself close-on-exec, but not the ones the process inherited: the
/// AppImage runtime's keep-alive pipe went from the old version through
/// the relaunch's shell into the new version, and the old runtime, which
/// waits for that pipe to close, never exited. It kept its mount and the
/// deleted old image (60 MB) until the new version quit, and every update
/// in one session added another. They are marked close-on-exec in the
/// child just before it runs the program (close_range, Linux 5.11; one by
/// one on an older kernel), which leaves the child's own exec-error pipe
/// as it was.
fn close_inherited(command: &mut Command) {
    const CLOSE_RANGE_CLOEXEC: libc::c_uint = 1 << 2;
    // SAFETY: sysconf is called here, in the parent.
    let limit = unsafe { libc::sysconf(libc::_SC_OPEN_MAX) };
    let limit = if limit <= 0 { 1024 } else { limit.min(65_536) } as libc::c_int;
    // SAFETY: between fork and exec only async-signal-safe calls are made:
    // the close_range system call and fcntl.
    unsafe {
        command.pre_exec(move || {
            let marked = libc::syscall(
                libc::SYS_close_range,
                3 as libc::c_uint,
                libc::c_uint::MAX,
                CLOSE_RANGE_CLOEXEC,
            ) == 0;
            if !marked {
                for descriptor in 3..limit {
                    let flags = libc::fcntl(descriptor, libc::F_GETFD);
                    if flags >= 0 {
                        libc::fcntl(descriptor, libc::F_SETFD, flags | libc::FD_CLOEXEC);
                    }
                }
            }
            Ok(())
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        collections::HashMap,
        fs,
        os::unix::fs::PermissionsExt,
        path::PathBuf,
        time::{Duration, Instant},
    };

    fn environment(pairs: &[(&str, &str)]) -> Vec<(String, Option<OsString>)> {
        let map: HashMap<String, OsString> = pairs
            .iter()
            .map(|(name, value)| (name.to_string(), OsString::from(value)))
            .collect();
        let names: Vec<OsString> = map.keys().map(OsString::from).collect();
        host_environment(|name| map.get(name).cloned(), names)
    }

    fn change<'a>(
        changes: &'a [(String, Option<OsString>)],
        name: &str,
    ) -> Option<&'a Option<OsString>> {
        changes
            .iter()
            .rev()
            .find(|(changed, _)| changed == name)
            .map(|(_, value)| value)
    }

    #[test]
    fn what_apprun_set_goes_back_to_the_system_s_values() {
        let changes = environment(&[
            (
                "TOMOSHIBI_APPRUN_SET",
                "XDG_DATA_DIRS GDK_PIXBUF_MODULE_FILE GIO_MODULE_DIR",
            ),
            ("XDG_DATA_DIRS", "/tmp/.mount_x/usr/share:/usr/share"),
            ("TOMOSHIBI_ORIGINAL_XDG_DATA_DIRS", "/usr/share"),
            (
                "GDK_PIXBUF_MODULE_FILE",
                "/tmp/.mount_x/usr/lib/loaders.cache",
            ),
            ("GIO_MODULE_DIR", "/tmp/.mount_x/usr/lib/gio/modules"),
            ("APPIMAGE", "/home/a/Tomoshibi.AppImage"),
            ("HOME", "/home/a"),
        ]);
        assert_eq!(
            change(&changes, "XDG_DATA_DIRS"),
            Some(&Some(OsString::from("/usr/share"))),
            "the system's own value"
        );
        assert_eq!(
            change(&changes, "GDK_PIXBUF_MODULE_FILE"),
            Some(&None),
            "not set before, so not set after"
        );
        assert_eq!(change(&changes, "GIO_MODULE_DIR"), Some(&None));
        assert_eq!(
            change(&changes, "TOMOSHIBI_ORIGINAL_XDG_DATA_DIRS"),
            Some(&None)
        );
        assert_eq!(change(&changes, "TOMOSHIBI_APPRUN_SET"), Some(&None));
        assert_eq!(change(&changes, "APPIMAGE"), Some(&None));
        assert_eq!(change(&changes, "HOME"), None, "the rest is left alone");
    }

    #[test]
    fn the_renderer_the_app_chose_is_not_handed_on() {
        let chosen = environment(&[("GSK_RENDERER", "cairo"), ("TOMOSHIBI_CHOSE_RENDERER", "1")]);
        assert_eq!(change(&chosen, "GSK_RENDERER"), Some(&None));
        // One the person set is theirs, and stays.
        let theirs = environment(&[("GSK_RENDERER", "vulkan")]);
        assert_eq!(change(&theirs, "GSK_RENDERER"), None);
    }

    #[test]
    fn outside_an_appimage_nothing_of_the_system_s_is_changed() {
        let changes = environment(&[("XDG_DATA_DIRS", "/usr/share"), ("HOME", "/home/a")]);
        assert_eq!(change(&changes, "XDG_DATA_DIRS"), None);
        assert!(
            changes.iter().all(|(_, value)| value.is_none()),
            "only removals of what is not there"
        );
    }

    // The relaunch's shell, with images that are shell scripts.

    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("tomoshibi-relaunch-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn script(path: &Path, body: &str) {
        fs::write(path, format!("#!/bin/sh\n{body}\n")).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }

    fn wait_for(what: impl Fn() -> bool) -> bool {
        let until = Instant::now() + Duration::from_secs(20);
        while Instant::now() < until {
            if what() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        false
    }

    /// A process that stands for the app being replaced, exiting when told.
    fn stand_in(dir: &Path) -> std::process::Child {
        let stop = dir.join("stop");
        Command::new("/bin/sh")
            .arg("-c")
            .arg(format!(
                "while [ ! -e '{}' ]; do sleep 0.05; done",
                stop.display()
            ))
            .spawn()
            .unwrap()
    }

    #[test]
    fn the_new_image_starts_only_after_the_app_is_gone_and_clears_the_old() {
        let dir = scratch("starts");
        let image = dir.join("Tomoshibi.AppImage");
        let old = Beside::new(&image).old();
        let started = dir.join("started");
        // The new build: says it started, and removes the old one as the
        // app does once its window is up.
        script(
            &image,
            &format!("touch '{}'; rm -f '{}'", started.display(), old.display()),
        );
        fs::write(&old, b"the old build").unwrap();

        let mut app = stand_in(&dir);
        relaunch_after(app.id(), &image, None).unwrap();
        std::thread::sleep(Duration::from_millis(500));
        assert!(!started.exists(), "started while the app was still running");

        fs::write(dir.join("stop"), b"").unwrap();
        app.wait().unwrap();
        assert!(wait_for(|| started.exists()), "never started");
        assert!(wait_for(|| !old.exists()));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_new_image_is_not_handed_what_this_process_inherited() {
        let dir = scratch("inherited");
        let image = Dir(&dir).image();
        let listed = dir.join("descriptors");
        script(&image, &format!("ls /proc/$$/fd > '{}'", listed.display()));
        // A descriptor as the AppImage runtime hands one over: not marked
        // close-on-exec.
        let mut ends = [0 as libc::c_int; 2];
        assert_eq!(unsafe { libc::pipe(ends.as_mut_ptr()) }, 0);
        let mut app = stand_in(&dir);
        relaunch_after(app.id(), &image, None).unwrap();
        fs::write(dir.join("stop"), b"").unwrap();
        app.wait().unwrap();
        assert!(wait_for(|| listed.exists()), "never started");
        std::thread::sleep(Duration::from_millis(200));
        let held: Vec<String> = fs::read_to_string(&listed)
            .unwrap()
            .lines()
            .map(str::to_owned)
            .collect();
        unsafe {
            libc::close(ends[0]);
            libc::close(ends[1]);
        }
        for end in ends {
            assert!(
                !held.contains(&end.to_string()),
                "descriptor {end} reached the new image: {held:?}"
            );
        }
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_tree_the_replaced_version_ran_from_goes_once_the_new_one_has_started() {
        let dir = scratch("unpacked");
        let image = Dir(&dir).image();
        let old = Beside::new(&image).old();
        let tree = dir.join("appimage_extracted_0123456789abcdef0123456789abcdef");
        fs::create_dir_all(tree.join("usr/bin")).unwrap();
        fs::write(tree.join("usr/bin/tomoshibi"), b"the old build").unwrap();
        // As unpacked from a squashfs: folders the owner cannot write in.
        fs::set_permissions(tree.join("usr/bin"), fs::Permissions::from_mode(0o555)).unwrap();
        script(&image, &format!("rm -f '{}'", old.display()));
        fs::write(&old, b"the old build").unwrap();
        let mut app = stand_in(&dir);
        relaunch_after(app.id(), &image, Some(&tree)).unwrap();
        fs::write(dir.join("stop"), b"").unwrap();
        app.wait().unwrap();
        assert!(wait_for(|| !tree.exists()), "the unpacked tree was left");
        let _ = fs::remove_dir_all(&dir);
    }

    struct Dir<'a>(&'a Path);

    impl Dir<'_> {
        fn image(&self) -> PathBuf {
            self.0.join("Tomoshibi.AppImage")
        }
    }

    #[test]
    fn a_new_image_that_fails_at_once_is_replaced_by_the_old_again() {
        let dir = scratch("rollback");
        let image = dir.join("Tomoshibi.AppImage");
        let old = Beside::new(&image).old();
        let again = dir.join("old-started");
        script(&image, "exit 3");
        script(
            &old,
            &format!(
                "echo \"$TOMOSHIBI_UPDATE_DID_NOT_START\" > '{}'",
                again.display()
            ),
        );

        let mut app = stand_in(&dir);
        relaunch_after(app.id(), &image, None).unwrap();
        fs::write(dir.join("stop"), b"").unwrap();
        app.wait().unwrap();

        assert!(
            wait_for(|| again.exists()),
            "the old build was not started again"
        );
        assert!(!old.exists(), "put back, not copied");
        assert_eq!(
            fs::read_to_string(&again).unwrap().trim(),
            "1",
            "the version put back is not told the update did not start"
        );
        assert!(fs::read_to_string(&image).unwrap().contains("old-started"));
        let _ = fs::remove_dir_all(&dir);
    }
}
