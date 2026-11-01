//! The entry: one GtkApplication per session.
//!
//! The application is registered on the session bus under `APP_ID`, so a
//! second launch (somebody opening a tomoshibi:// or https invitation link
//! while the app is open, which the desktop file's x-scheme-handler does)
//! does not open a second app: GApplication hands its links to the first
//! instance's `open` and exits. The first opens them in its one window, as
//! the Mac and Windows apps do with theirs.

use std::{cell::RefCell, rc::Rc};

use adw::prelude::*;
use gtk::{gio, glib};
use tomoshibi_linux::{
    app::{self, client, log, AppModel, Launch},
    i18n, media, theme, ui, update, APP_ID,
};

/// What the first instance holds once it has started.
struct Running {
    model: AppModel,
    window: adw::ApplicationWindow,
}

fn main() -> glib::ExitCode {
    // Said before the first Deployment is made, which reads it then
    // (core/protocol/src/api/agent.rs).
    client::tell();
    let launch = Launch::from_environment();

    let application = adw::Application::builder()
        .application_id(APP_ID)
        .flags(gio::ApplicationFlags::HANDLES_OPEN)
        .build();
    let running: Rc<RefCell<Option<Running>>> = Rc::new(RefCell::new(None));

    // The preferences' last changes, which wait a moment before they are
    // written (app/prefs.rs), are written as the app quits.
    application.connect_shutdown({
        let running = running.clone();
        move |_| {
            if let Some(running) = running.borrow().as_ref() {
                running.model.preferences().flush();
            }
        }
    });

    application.connect_startup(|_| {
        // Day and night follow the system; nothing in the app overrides it,
        // and the call draws itself dark in either (theme.rs).
        adw::StyleManager::default().set_color_scheme(adw::ColorScheme::Default);
        gtk::Window::set_default_icon_name(APP_ID);
    });

    application.connect_activate({
        let (running, launch) = (running.clone(), launch.clone());
        move |application| {
            start(application, &running, &launch);
            if let Some(running) = running.borrow().as_ref() {
                running.window.present();
            }
        }
    });

    // Links from a second launch, or from this one's own command line.
    application.connect_open({
        let (running, launch) = (running.clone(), launch.clone());
        move |application, files, _| {
            start(application, &running, &launch);
            let Some(running) = running
                .borrow()
                .as_ref()
                .map(|running| (running.model.clone(), running.window.clone()))
            else {
                return;
            };
            let (model, window) = running;
            for file in files {
                let uri = file.uri();
                match app::pasted_link(&uri) {
                    Some(link) => {
                        // The link itself only in a debug build: an
                        // invitation's token is as good as a key to the room.
                        if cfg!(debug_assertions) {
                            log(&format!("a link was handed over: {link}"));
                        } else {
                            log("a link was handed over");
                        }
                        model.open_link(&link);
                    }
                    None => log("a launch handed over something that is not a link"),
                }
            }
            window.present();
        }
    });

    application.run()
}

/// Builds the model and the window the first time the application is asked
/// to show anything, and asks the deployment what it is.
fn start(application: &adw::Application, running: &Rc<RefCell<Option<Running>>>, launch: &Launch) {
    if running.borrow().is_some() {
        return;
    }
    let model = AppModel::new(launch.clone());

    // A language given for this run wins over the one kept, and is not kept.
    let kept = launch
        .language
        .clone()
        .or_else(|| model.preferences().string(i18n::STORAGE_KEY));
    i18n::start(kept.as_deref());
    let accent = model
        .preferences()
        .string(theme::ACCENT_KEY)
        .and_then(|key| theme::Accent::of(&key))
        .unwrap_or(theme::Accent::Amber);
    theme::install(accent);

    let media = media::standard();
    let window = ui::window::open(application, &model, media.clone());
    debug_switches(application, &window, &model, media);
    // The image this one replaced, the next build, the desktop entry.
    update::at_start(&model, &window);
    log(&format!(
        "Tomoshibi {} for Linux, core {}, {}",
        client::version_label(),
        tomoshibi_core::core_version(),
        if tomoshibi_protocol::api::core_is_release() {
            "release"
        } else {
            "debug"
        },
    ));
    model.connect();
    running.replace(Some(Running { model, window }));
}

/// The test switches (launch.rs), in a debug build only.
#[cfg(debug_assertions)]
fn debug_switches(
    application: &adw::Application,
    window: &adw::ApplicationWindow,
    model: &AppModel,
    media: std::rc::Rc<dyn media::Media>,
) {
    if let Some(path) = model.launch().snapshot.clone() {
        ui::window::snapshot_when_settled(window, model, path);
    }
    if let Some(seconds) = model.launch().quit_after_seconds {
        let (application, model) = (application.downgrade(), model.downgrade());
        glib::timeout_add_seconds_local_once(seconds, move || {
            glib::spawn_future_local(async move {
                // Out of the call first, so that the others see this one
                // leave rather than time out.
                if let Some(model) = model.upgrade() {
                    ui::room::leave(&model, media.as_ref()).await;
                }
                if let Some(application) = application.upgrade() {
                    log("quitting, as TOMOSHIBI_DEBUG_QUIT_AFTER asked");
                    application.quit();
                }
            });
        });
    }
}

#[cfg(not(debug_assertions))]
fn debug_switches(
    _: &adw::Application,
    _: &adw::ApplicationWindow,
    _: &AppModel,
    _: std::rc::Rc<dyn media::Media>,
) {
}
