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
    choose_renderer();
    // The links AppRun took off the command line (an invitation's token is
    // as good as a key to the room, and a command line can be read by every
    // user of the machine): a start from an image that has them on its
    // runtime's line hands them to a fresh start of the image and ends, so
    // that the runtime's line goes with it (update/relaunch.rs). Taken out
    // of the environment either way, so that nothing this starts has them.
    let links = std::env::var(update::relaunch::OPEN_LINKS).unwrap_or_default();
    let handed = std::env::var_os(update::relaunch::LINKS_HANDED).is_some();
    std::env::remove_var(update::relaunch::OPEN_LINKS);
    std::env::remove_var(update::relaunch::LINKS_HANDED);
    let links: Vec<String> = links
        .lines()
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect();
    if !links.is_empty() && !handed {
        if let Some(image) = update::running_image() {
            match update::relaunch::hand_links_over(&image, &links.join("\n")) {
                Ok(()) => {
                    log("handed the links to a start of the image without them on its line");
                    return glib::ExitCode::SUCCESS;
                }
                Err(fault) => log(&format!("the links could not be handed on: {fault}")),
            }
        }
    }

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

    // The links, as the application's own arguments: opened here, or
    // handed over the session bus to the instance already running, as a
    // second launch's are.
    if links.is_empty() {
        application.run()
    } else {
        let mut arguments = vec![std::env::args().next().unwrap_or_else(|| "tomoshibi".into())];
        arguments.extend(links);
        application.run_with_args(&arguments)
    }
}

/// GTK's renderer, where nobody chose one: Cairo on a machine without a
/// GPU, GTK's own choice otherwise.
///
/// Without a GPU (a virtual machine, a thin client) GTK's GL renderers run
/// on Mesa's llvmpipe, which rasterises in software on the processor: a
/// call of four drawn by the default renderer (ngl) took 2.4 cores in a
/// debug build, 1.4 of them llvmpipe's, where Cairo drew the same call in
/// 1.0 (GL: 2.2); and a
/// window drawn in software by GL got a frame every 150 to 220 ms, so the
/// start sheet's arrival, a timed animation, was seen in two frames (about
/// 15 per cent, then all of it), where Cairo gave one every 26 ms. Mesa
/// rasterises in software when there is no DRM render node to draw with
/// (/dev/dri/renderD*), or when LIBGL_ALWAYS_SOFTWARE says so; NVIDIA's own
/// driver draws GL without a render node where its DRM module is not
/// loaded, so its control device counts as a GPU too. Set before
/// anything else runs, since GTK reads it when the window is realised;
/// marked, so that what the app starts (a browser, the next version) is
/// handed the system's own choice (update/relaunch.rs).
fn choose_renderer() {
    if std::env::var_os("GSK_RENDERER").is_some() {
        return;
    }
    let render_node = std::fs::read_dir("/dev/dri").is_ok_and(|folder| {
        folder
            .flatten()
            .any(|entry| entry.file_name().to_string_lossy().starts_with("renderD"))
    }) || std::path::Path::new("/dev/nvidiactl").exists();
    let software = std::env::var("LIBGL_ALWAYS_SOFTWARE")
        .is_ok_and(|value| !value.is_empty() && value != "0" && value != "false");
    if !render_node || software {
        std::env::set_var("GSK_RENDERER", "cairo");
        std::env::set_var(update::relaunch::RENDERER_CHOSEN, "1");
        log(if software {
            "drawing with Cairo: GL would be drawn in software here (LIBGL_ALWAYS_SOFTWARE)"
        } else {
            "drawing with Cairo: there is no GPU to draw GL with (no render node)"
        });
    }
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
    // TOMOSHIBI_DEBUG_FRAMES=<seconds>: every frame the window paints for
    // that long from the start, with how long since the one before, to see
    // a transition starved of frames (a dialog's first frames, drawn in
    // software).
    if let Some(seconds) = std::env::var("TOMOSHIBI_DEBUG_FRAMES")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
    {
        let until = glib::monotonic_time() + seconds as i64 * 1_000_000;
        window.connect_realize(move |window| {
            let Some(clock) = window.frame_clock() else {
                return;
            };
            let last = std::cell::Cell::new(0i64);
            clock.connect_after_paint(move |clock| {
                let now = glib::monotonic_time();
                if now > until {
                    return;
                }
                let since = if last.get() > 0 { (now - last.get()) / 1000 } else { 0 };
                last.set(now);
                log(&format!(
                    "frame {} painted at {} ms, {since} ms after the last",
                    clock.frame_counter(),
                    now / 1000
                ));
            });
        });
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
