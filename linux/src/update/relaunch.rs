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
    changes
}

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
/// process's id, the image, the image it replaced.
///
/// Waits at most thirty seconds for this process to exit, which takes the
/// time a call takes to be left and the window to close; and watches the
/// new one for fifteen, more than a first start takes to show its window
/// (and to unpack itself, where it runs unpacked) on the slowest machine
/// it was tried on.
const SHELL: &str = r#"
pid=$1 image=$2 old=$3
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
      mv -f "$old" "$image" && exec "$image"
    fi
    exit "$status"
  fi
  [ -e "$old" ] || exit 0
  sleep 0.1
  watched=$((watched + 1))
done
exit 0
"#;

/// Starts the image at `image` once this process has exited (the caller
/// quits the app next), with the system's environment and no arguments:
/// any link this run was opened with has been opened.
pub fn relaunch(image: &Path) -> io::Result<()> {
    relaunch_after(std::process::id(), image)
}

fn relaunch_after(pid: u32, image: &Path) -> io::Result<()> {
    let old = Beside::new(image).old();
    let mut command = Command::new("/bin/sh");
    command
        .arg("-c")
        .arg(SHELL)
        .arg("sh")
        .arg(pid.to_string())
        .arg(image)
        .arg(old)
        .stdin(Stdio::null())
        // Its own process group, so that whatever ends the app's group (a
        // terminal's job control, a desktop session ending the app) does
        // not end the shell that is to start the next one.
        .process_group(0);
    as_the_host(&mut command);
    command.spawn().map(|_| ())
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
        relaunch_after(app.id(), &image).unwrap();
        std::thread::sleep(Duration::from_millis(500));
        assert!(!started.exists(), "started while the app was still running");

        fs::write(dir.join("stop"), b"").unwrap();
        app.wait().unwrap();
        assert!(wait_for(|| started.exists()), "never started");
        assert!(wait_for(|| !old.exists()));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_new_image_that_fails_at_once_is_replaced_by_the_old_again() {
        let dir = scratch("rollback");
        let image = dir.join("Tomoshibi.AppImage");
        let old = Beside::new(&image).old();
        let again = dir.join("old-started");
        script(&image, "exit 3");
        script(&old, &format!("touch '{}'", again.display()));

        let mut app = stand_in(&dir);
        relaunch_after(app.id(), &image).unwrap();
        fs::write(dir.join("stop"), b"").unwrap();
        app.wait().unwrap();

        assert!(
            wait_for(|| again.exists()),
            "the old build was not started again"
        );
        assert!(!old.exists(), "put back, not copied");
        assert!(fs::read_to_string(&image).unwrap().contains("old-started"));
        let _ = fs::remove_dir_all(&dir);
    }
}
