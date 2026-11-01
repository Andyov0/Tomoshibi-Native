//! Putting the app among the desktop's apps, and making it the one that
//! opens tomoshibi:// links.
//!
//! An AppImage is a file somebody downloaded: until something says so, no
//! menu lists it and no link opens it. On the first start from an image the
//! app asks once whether to add itself; yes writes, under the user's own
//! data directory and nowhere else, a desktop entry naming the image
//! (`~/.local/share/applications/app.tomoshibi.Linux.desktop`, with
//! `x-scheme-handler/tomoshibi` among its types), the icons beside it in the
//! hicolor theme, and the entry as the default for the scheme in
//! `~/.config/mimeapps.list`, which is where GIO, xdg-open and KDE all look
//! first. Asked and declined, it is not asked again (the preference
//! [`KEPT`]); the entry is the settings' to offer again.
//!
//! An entry this app wrote names the image it was written for
//! (`X-Tomoshibi-Image`). A later start from an image somewhere else (the
//! file moved, or a newer download kept beside the old one) writes the entry
//! again for itself, without asking, since the person said yes to the app
//! and not to a path; an update replaces the image at its own path, so it
//! changes nothing here. An entry the person removed is left removed.
//!
//! Nothing is written in a rehearsal (a debug build given its deployment,
//! launch.rs) unless `TOMOSHIBI_DEBUG_DESKTOP` asks: `add` adds without
//! asking, `ask` asks as a first start would, for the tests.

use std::{
    cell::Cell,
    fs, io,
    path::{Path, PathBuf},
    rc::Rc,
};

use adw::prelude::*;

use crate::{
    app::{log, AppModel, Change, Screen},
    i18n::{self, Language},
    APP_ID,
};

/// What the person said when asked: `added` or `declined`. A Linux-only
/// preference, so a name of its own rather than a web key.
pub const KEPT: &str = "tomoshibi.linux.desktop";

/// The entry inside the image (linux/data), which this one is made from,
/// so that the two say the same.
const TEMPLATE: &str = include_str!("../../data/app.tomoshibi.Linux.desktop");
/// The drawing, which is enough for any desktop with an SVG loader; the
/// fixed sizes are copied from the image where it has them.
const ICON: &str = include_str!("../../data/icons/app.tomoshibi.Linux.svg");

/// The key an entry of this app's names its image by.
const IMAGE_KEY: &str = "X-Tomoshibi-Image";

/// The desktop entry's locales for each of the app's languages; the
/// AppImage's own entry is given the same ones (appimage-inside.sh).
const LOCALES: [(Language, &[&str]); 3] = [
    (Language::ZhHans, &["zh_CN", "zh_SG"]),
    (Language::ZhHant, &["zh_TW", "zh_HK"]),
    (Language::Ja, &["ja"]),
];

/// Where the entry, the icons and the default handler go.
#[derive(Debug, Clone)]
pub struct Places {
    /// `$XDG_DATA_HOME`, `~/.local/share`.
    pub data_home: PathBuf,
    /// `$XDG_CONFIG_HOME`, `~/.config`.
    pub config_home: PathBuf,
    /// The mounted (or unpacked) image (`$APPDIR`), for its icons.
    pub appdir: Option<PathBuf>,
}

impl Places {
    pub fn standard() -> Places {
        Places {
            data_home: glib::user_data_dir(),
            config_home: glib::user_config_dir(),
            appdir: std::env::var_os("APPDIR").map(PathBuf::from),
        }
    }

    fn entry(&self) -> PathBuf {
        self.data_home
            .join("applications")
            .join(format!("{APP_ID}.desktop"))
    }

    fn hicolor(&self) -> PathBuf {
        self.data_home.join("icons").join("hicolor")
    }
}

/// [`integrate`] for the running image, off the main thread; an error
/// where the app is not running from one, since an entry must name a file
/// that stays where it is.
pub async fn integrate_desktop() -> io::Result<()> {
    let image = super::running_image().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "the app is not running from an AppImage",
        )
    })?;
    integrate_in_background(&image, &Places::standard()).await
}

/// [`integrate`], with the files written and the menus' caches made on a
/// thread of its own: the two cache tools are programs that are waited
/// for, and waiting for them on the main thread held the window still,
/// at a press of Add and at every start from an image the entry did not
/// name, which happens before the window is first shown. The entry's words
/// are worked out here, on the main thread, where the phrases are.
pub async fn integrate_in_background(image: &Path, places: &Places) -> io::Result<()> {
    let job = Integration::prepare(image, places)?;
    gio::spawn_blocking(move || job.write())
        .await
        .unwrap_or_else(|_| Err(io::Error::other("the desktop entry's writer stopped")))
}

/// Everything [`integrate`] writes, once its words are known; written on
/// any thread.
pub struct Integration {
    places: Places,
    entry: String,
}

impl Integration {
    pub fn prepare(image: &Path, places: &Places) -> io::Result<Integration> {
        let entry = entry_for(image).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "the image's path cannot be written into a desktop entry",
            )
        })?;
        Ok(Integration {
            places: places.clone(),
            entry,
        })
    }

    pub fn write(self) -> io::Result<()> {
        write_integration(&self.entry, &self.places)
    }
}

/// The image the installed entry names, where there is an entry of this
/// app's.
pub fn integrated_image(places: &Places) -> Option<PathBuf> {
    let text = fs::read_to_string(places.entry()).ok()?;
    text.lines().find_map(|line| {
        let value = line.strip_prefix(IMAGE_KEY)?.strip_prefix('=')?;
        Some(PathBuf::from(unescape(value)))
    })
}

/// Whether the desktop lists this image now.
pub fn integrated() -> bool {
    match super::running_image() {
        Some(image) => integrated_image(&Places::standard()).as_deref() == Some(image.as_path()),
        None => false,
    }
}

/// Writes the entry for `image`, the icons, and the default handler, here
/// and now (the tests'; the app uses [`integrate_in_background`]).
pub fn integrate(image: &Path, places: &Places) -> io::Result<()> {
    Integration::prepare(image, places)?.write()
}

fn write_integration(text: &str, places: &Places) -> io::Result<()> {

    // The icons first, so that a menu that notices the entry at once finds
    // its icon.
    let hicolor = places.hicolor();
    write_atomically(
        &hicolor
            .join("scalable")
            .join("apps")
            .join(format!("{APP_ID}.svg")),
        ICON.as_bytes(),
    )?;
    if let Some(appdir) = &places.appdir {
        let from = appdir.join("usr/share/icons/hicolor");
        if let Ok(sizes) = fs::read_dir(&from) {
            for size in sizes.flatten() {
                let png = size.path().join("apps").join(format!("{APP_ID}.png"));
                if let Ok(bytes) = fs::read(&png) {
                    write_atomically(
                        &hicolor
                            .join(size.file_name())
                            .join("apps")
                            .join(format!("{APP_ID}.png")),
                        &bytes,
                    )?;
                }
            }
        }
    }
    // GTK reads the theme's icon-theme.cache only while it is newer than
    // the theme's folder; touching the folder makes one some other program
    // left there stale, so that the icons just written are looked for.
    let touch = hicolor.join(format!(".{APP_ID}.touch"));
    if fs::write(&touch, b"").is_ok() {
        let _ = fs::remove_file(&touch);
    }

    write_atomically(&places.entry(), text.as_bytes())?;
    set_default_handler(&places.config_home, &format!("{APP_ID}.desktop"))?;

    // The menus' caches, where the tools are there to make them; the entry
    // and mimeapps.list work without them.
    for (tool, folder) in [
        (
            "update-desktop-database",
            places.data_home.join("applications"),
        ),
        ("gtk-update-icon-cache", hicolor.clone()),
    ] {
        let mut command = std::process::Command::new(tool);
        if tool == "gtk-update-icon-cache" {
            command.args(["-q", "-t", "-f"]);
        } else {
            command.arg("-q");
        }
        command
            .arg(&folder)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        super::relaunch::as_the_host(&mut command);
        let _ = command.status();
    }
    Ok(())
}

/// The entry for an image at `image`: the image's own entry, with the image
/// to run, the image the entry is for, and the two sentences in the app's
/// other languages. `None` for a path a desktop entry cannot hold (one with
/// a line break in it).
pub fn entry_for(image: &Path) -> Option<String> {
    let path = image.to_str()?;
    if path.contains(['\n', '\r']) {
        return None;
    }
    let mut lines: Vec<String> = Vec::new();
    for line in TEMPLATE.lines() {
        if line.starts_with("Exec=") {
            lines.push(format!("TryExec={}", escape(path)));
            lines.push(format!("Exec={} %U", exec_argument(path)));
        } else {
            lines.push(line.to_owned());
        }
    }
    for key in ["GenericName", "Comment"] {
        let Some(english) = TEMPLATE
            .lines()
            .find_map(|line| line.strip_prefix(key)?.strip_prefix('='))
        else {
            continue;
        };
        for (language, locales) in LOCALES {
            let said = i18n::template_in(english, language);
            if said == english {
                continue;
            }
            for locale in locales {
                lines.push(format!("{key}[{locale}]={}", escape(&said)));
            }
        }
    }
    lines.push(format!("{IMAGE_KEY}={}", escape(path)));
    Some(lines.join("\n") + "\n")
}

/// A desktop entry's string value: a backslash written as two, and the
/// characters that cannot stand for themselves as their escapes.
fn escape(value: &str) -> String {
    let mut written = String::new();
    for c in value.chars() {
        match c {
            '\\' => written.push_str("\\\\"),
            '\t' => written.push_str("\\t"),
            c => written.push(c),
        }
    }
    if written.starts_with(' ') {
        written.replace_range(0..1, "\\s");
    }
    written
}

fn unescape(value: &str) -> String {
    let mut read = String::new();
    let mut chars = value.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            read.push(c);
            continue;
        }
        match chars.next() {
            Some('s') => read.push(' '),
            Some('t') => read.push('\t'),
            Some('n') => read.push('\n'),
            Some('r') => read.push('\r'),
            Some(other) => read.push(other),
            None => read.push('\\'),
        }
    }
    read
}

/// The image as the program of an `Exec` line: in double quotes, with the
/// four characters the quoting rule reserves escaped and a percent sign
/// doubled (it opens a field code otherwise), and then written as a string
/// value, which doubles every backslash again. A path with a space or a
/// quote in it (an image in "~/My Apps") runs as itself rather than as the
/// first word of it.
fn exec_argument(path: &str) -> String {
    let mut quoted = String::from("\"");
    for c in path.chars() {
        match c {
            '"' | '`' | '$' | '\\' => {
                quoted.push('\\');
                quoted.push(c);
            }
            '%' => quoted.push_str("%%"),
            c => quoted.push(c),
        }
    }
    quoted.push('"');
    quoted.replace('\\', "\\\\")
}

/// Makes `desktop_id` the default for tomoshibi:// in `mimeapps.list`,
/// keeping everything else in the file (other types' defaults, comments).
fn set_default_handler(config_home: &Path, desktop_id: &str) -> io::Result<()> {
    const SCHEME: &str = "x-scheme-handler/tomoshibi";
    let path = config_home.join("mimeapps.list");
    let file = glib::KeyFile::new();
    if path.exists() {
        file.load_from_file(
            &path,
            glib::KeyFileFlags::KEEP_COMMENTS | glib::KeyFileFlags::KEEP_TRANSLATIONS,
        )
        .map_err(|fault| io::Error::other(fault.to_string()))?;
    }
    file.set_string("Default Applications", SCHEME, desktop_id);
    // Added too, as GIO's own set_as_default_for_type does, so that a
    // desktop that reads only the associations offers it for the scheme.
    let added: Vec<String> = file
        .string_list("Added Associations", SCHEME)
        .map(|list| list.iter().map(|one| one.to_string()).collect())
        .unwrap_or_default();
    let mut added: Vec<String> = added.into_iter().filter(|one| one != desktop_id).collect();
    added.insert(0, desktop_id.to_owned());
    // A list is its items each ended by a semicolon; a desktop id has none
    // of the characters a string value would escape.
    let added: String = added.iter().map(|one| format!("{one};")).collect();
    file.set_string("Added Associations", SCHEME, &added);
    fs::create_dir_all(config_home)?;
    write_atomically(&path, file.to_data().as_bytes())
}

/// Written beside and renamed over, so that a desktop reading the file at
/// that moment reads the old one or the new one.
fn write_atomically(path: &Path, bytes: &[u8]) -> io::Result<()> {
    if let Some(folder) = path.parent() {
        fs::create_dir_all(folder)?;
    }
    let mut partial = path.as_os_str().to_owned();
    partial.push(format!(".{}.part", std::process::id()));
    let partial = PathBuf::from(partial);
    fs::write(&partial, bytes)?;
    fs::rename(&partial, path)
}

/// What a debug run was told to do about the desktop.
fn debug_switch() -> Option<String> {
    #[cfg(debug_assertions)]
    {
        std::env::var("TOMOSHIBI_DEBUG_DESKTOP")
            .ok()
            .map(|value| value.trim().to_owned())
    }
    #[cfg(not(debug_assertions))]
    {
        None
    }
}

/// At a start from an image: writes the entry again where it names another
/// image, or asks once where there is none and nobody has answered.
pub fn at_start(model: &AppModel, window: &adw::ApplicationWindow) {
    let Some(image) = super::running_image() else {
        return;
    };
    let switch = debug_switch();
    if model.launch().rehearsal() && switch.is_none() {
        return;
    }
    let places = Places::standard();
    if switch.as_deref() == Some("add") {
        integrate_and_say(image, places);
        return;
    }
    match integrated_image(&places) {
        Some(named) if named == image => {}
        Some(_) => {
            log("update: the desktop entry named another image; writing it for this one");
            integrate_and_say(image, places);
        }
        None => {
            let answered = model.preferences().string(KEPT).is_some();
            if !answered || switch.as_deref() == Some("ask") {
                ask_when_settled(model, window, image);
            }
        }
    }
}

/// Writes the entry for `image` without holding the window, and says how
/// it went.
fn integrate_and_say(image: PathBuf, places: Places) {
    glib::spawn_future_local(async move {
        let result = integrate_in_background(&image, &places).await;
        say_integrated(result, &image);
    });
}

fn say_integrated(result: io::Result<()>, image: &Path) {
    match result {
        Ok(()) => log(&format!(
            "update: added to the desktop's apps, as the opener of tomoshibi:// links ({})",
            if cfg!(debug_assertions) {
                image.display().to_string()
            } else {
                "the running image".to_owned()
            }
        )),
        Err(fault) => log(&format!(
            "update: the desktop entry could not be written: {fault}"
        )),
    }
}

/// How long a screen has shown before the question comes: long enough for
/// the screen's own arrival to have finished, so that the two do not move
/// at once.
const SETTLED_MS: u32 = 900;

/// Asks once a screen before a call has settled, never over a call or over
/// the mark the app opens on.
fn ask_when_settled(model: &AppModel, window: &adw::ApplicationWindow, image: PathBuf) {
    let asked = Rc::new(Cell::new(false));
    let try_now = {
        let (model, window, asked) = (model.downgrade(), window.downgrade(), asked.clone());
        move || {
            let (Some(model), Some(window)) = (model.upgrade(), window.upgrade()) else {
                return;
            };
            if asked.get() || !calm(model.screen()) {
                return;
            }
            let screen = model.screen();
            let (later_model, later_window, later_asked, image) = (
                model.downgrade(),
                window.downgrade(),
                asked.clone(),
                image.clone(),
            );
            glib::timeout_add_local_once(
                std::time::Duration::from_millis(SETTLED_MS.into()),
                move || {
                    let (Some(model), Some(window)) =
                        (later_model.upgrade(), later_window.upgrade())
                    else {
                        return;
                    };
                    // Still on that screen, and not asked meanwhile.
                    if later_asked.get() || model.screen() != screen {
                        return;
                    }
                    later_asked.set(true);
                    ask(&model, &window, image);
                },
            );
        }
    };
    try_now();
    model.subscribe_while(window, move |_, change| {
        if change == Change::Screen {
            try_now();
        }
    });
}

fn calm(screen: Screen) -> bool {
    matches!(screen, Screen::SignIn | Screen::Lobby | Screen::Unreachable)
}

fn ask(model: &AppModel, window: &adw::ApplicationWindow, image: PathBuf) {
    let dialog = adw::AlertDialog::new(
        Some(&i18n::t("Add Tomoshibi to your apps?")),
        Some(&i18n::t(
            "Tomoshibi will appear with your other apps, and meeting links will open in it.",
        )),
    );
    dialog.add_response("later", &i18n::t("Not now"));
    dialog.add_response("add", &i18n::t("Add"));
    dialog.set_response_appearance("add", adw::ResponseAppearance::Suggested);
    // libadwaita draws the suggested response in its own blue, which is no
    // colour of the app's; the app's accent as text instead (theme.rs
    // tmsb_tally_text, the shade of it that reads on the dialog's ground).
    crate::theme::add_module_css(
        "update",
        "dialog.alert.tmsb-update .response-area button.suggested { color: @tmsb_tally_text; }",
    );
    dialog.add_css_class("tmsb-update");
    dialog.set_default_response(Some("add"));
    dialog.set_close_response("later");
    let model = model.downgrade();
    dialog.connect_response(None, move |_, response| {
        let Some(model) = model.upgrade() else {
            return;
        };
        if response == "add" {
            let image = image.clone();
            glib::spawn_future_local(async move {
                let result = integrate_in_background(&image, &Places::standard()).await;
                let added = result.is_ok();
                say_integrated(result, &image);
                if added {
                    model.preferences().set(KEPT, "added");
                }
            });
        } else {
            log("update: not added to the desktop's apps; not asked again");
            model.preferences().set(KEPT, "declined");
        }
    });
    log("update: asking whether to add the app to the desktop's apps");
    dialog.present(Some(window));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_entry_runs_the_image_by_its_whole_path_and_opens_the_scheme() {
        let text = entry_for(Path::new(
            "/home/ann/My Apps/Tomoshibi \"new\" $1%.AppImage",
        ))
        .unwrap();
        assert!(
            text.contains(
                "\nExec=\"/home/ann/My Apps/Tomoshibi \\\\\"new\\\\\" \\\\$1%%.AppImage\" %U\n"
            ),
            "{text}"
        );
        assert!(text.contains("\nTryExec=/home/ann/My Apps/Tomoshibi \"new\" $1%.AppImage\n"));
        assert!(text.contains("\nMimeType=x-scheme-handler/tomoshibi;\n"));
        assert!(text.contains(&format!("\nIcon={APP_ID}\n")));
        assert!(
            !text.contains("Exec=tomoshibi"),
            "the image's own Exec is replaced"
        );
        assert!(entry_for(Path::new("/tmp/two\nlines.AppImage")).is_none());
    }

    #[test]
    fn the_entry_says_its_sentences_in_the_app_s_other_languages() {
        let text = entry_for(Path::new("/opt/Tomoshibi.AppImage")).unwrap();
        for line in ["GenericName[zh_CN]=", "GenericName[zh_TW]=", "Comment[ja]="] {
            assert!(text.contains(line), "{line} missing from {text}");
        }
    }

    fn places(name: &str) -> Places {
        let root =
            std::env::temp_dir().join(format!("tomoshibi-desktop-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        Places {
            data_home: root.join("share"),
            config_home: root.join("config"),
            appdir: None,
        }
    }

    #[test]
    fn integrating_names_the_image_and_keeps_other_defaults() {
        let places = places("integrate");
        fs::create_dir_all(&places.config_home).unwrap();
        fs::write(
            places.config_home.join("mimeapps.list"),
            "# somebody's\n[Default Applications]\ntext/html=firefox.desktop\n",
        )
        .unwrap();
        let image = Path::new("/home/ann/Apps/Tomoshibi.AppImage");

        integrate(image, &places).unwrap();

        assert_eq!(integrated_image(&places).as_deref(), Some(image));
        let list = fs::read_to_string(places.config_home.join("mimeapps.list")).unwrap();
        assert!(list.contains("text/html=firefox.desktop"), "{list}");
        assert!(list.contains("# somebody's"));
        assert!(list.contains(&format!("x-scheme-handler/tomoshibi={APP_ID}.desktop")));
        assert!(places
            .data_home
            .join(format!("icons/hicolor/scalable/apps/{APP_ID}.svg"))
            .exists());

        // Moved: written again for where it is now.
        let moved = Path::new("/home/ann/Tomoshibi.AppImage");
        integrate(moved, &places).unwrap();
        assert_eq!(integrated_image(&places).as_deref(), Some(moved));
        let list = fs::read_to_string(places.config_home.join("mimeapps.list")).unwrap();
        assert_eq!(
            list.matches(&format!("{APP_ID}.desktop")).count(),
            2,
            "{list}"
        );
        let _ = fs::remove_dir_all(places.data_home.parent().unwrap());
    }

    #[test]
    fn escaping_reads_back_as_it_was_written() {
        for value in ["/plain/path", "/a\\b/c", " leading space", "tab\there"] {
            assert_eq!(unescape(&escape(value)), value);
        }
    }
}
