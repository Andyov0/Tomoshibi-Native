//! The main window: one screen at a time, crossfading from one to the next,
//! as the Mac app's single window changes screens.
//!
//! The window asks the module that owns a screen to build it (lobby/ for
//! the screens before a call, room/ for the call and its ending) when the
//! model moves to it, and throws the one it left away once the crossfade is
//! over, so that coming back to a screen builds it fresh from the model. A
//! change of language builds the screen showing again, in the new language,
//! and crossfades to it; except the call and its ending, which the room
//! re-words in place (`i18n::on_change`), since building the call again
//! would build its every picture and panel again in the middle of the call.
//!
//! The two screens with nothing to fill in, starting and unreachable, are
//! drawn here. Owned by the skeleton; a module changes what its screens look
//! like in its own files, never here.
//!
//! The window is also where a call meets the desktop: closing it, or the
//! session ending the app (SIGTERM at logout, SIGINT, SIGHUP), leaves the
//! call before the app goes, as a closed browser tab is seen to go; the
//! machine is kept from idling and suspending while a call is on screen; and
//! the call's two keys, the microphone and the camera, are the window's.

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::Duration,
};

use adw::prelude::*;
use gtk::glib;

use super::{kit, lobby, room, Context};
use crate::{
    app::{log, AppModel, Change, Screen},
    i18n::t,
    media::Media,
    theme::motion,
};

/// The window's first size: the Mac app's default window, and as narrow as
/// any screen is laid out for (the sign-in's narrow layout, the lobby's
/// sidebar folded away).
pub const DEFAULT_SIZE: (i32, i32) = (1100, 720);
pub const SMALLEST_SIZE: (i32, i32) = (360, 560);

struct Shown {
    screen: Screen,
    /// The stack child's name; a screen built again (a change of language)
    /// gets a new one so that the crossfade has two children to fade.
    name: String,
}

/// Opens the main window on `app` and keeps it showing the model's screen.
pub fn open(
    app: &adw::Application,
    model: &AppModel,
    media: Rc<dyn Media>,
) -> adw::ApplicationWindow {
    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("Tomoshibi")
        .default_width(DEFAULT_SIZE.0)
        .default_height(DEFAULT_SIZE.1)
        .width_request(SMALLEST_SIZE.0)
        .height_request(SMALLEST_SIZE.1)
        .build();
    window.add_css_class("tmsb-window");

    let stack = gtk::Stack::builder()
        .transition_type(gtk::StackTransitionType::Crossfade)
        .transition_duration(motion::SETTLE_MS)
        .hhomogeneous(false)
        .vhomogeneous(false)
        .build();
    window.set_content(Some(&stack));

    let context = Context::new(model.clone(), media, &window);
    let shown: Rc<RefCell<Option<Shown>>> = Rc::new(RefCell::new(None));
    let generation = Rc::new(std::cell::Cell::new(0u64));

    let present = {
        let (context, stack, shown, generation, window) = (
            context.clone(),
            stack.clone(),
            shown.clone(),
            generation.clone(),
            window.clone(),
        );
        move |rebuild: bool| {
            let screen = context.model.screen();
            if !rebuild && shown.borrow().as_ref().map(|now| now.screen) == Some(screen) {
                return;
            }
            generation.set(generation.get() + 1);
            let name = format!("{}-{}", screen.name(), generation.get());
            let page = page_for(&context, screen);
            stack.add_named(&page, Some(&name));
            stack.set_visible_child_name(&name);
            // Calls stay dark in every appearance (UI-SYNC decision 6): the
            // window's own ground follows the screen.
            if screen.is_call() {
                window.add_css_class("tmsb-call");
            } else {
                window.remove_css_class("tmsb-call");
            }
            shown.replace(Some(Shown { screen, name }));
            log(&format!("showing {}", screen.name()));
        }
    };

    // The screens left behind are removed once the crossfade has finished
    // with them; removed sooner, the fade would cut.
    {
        let shown = shown.clone();
        stack.connect_transition_running_notify(move |stack| {
            if stack.is_transition_running() {
                return;
            }
            let keep = shown.borrow().as_ref().map(|now| now.name.clone());
            let mut child = stack.first_child();
            while let Some(page) = child {
                child = page.next_sibling();
                if stack.page(&page).name().map(|name| name.to_string()) != keep {
                    stack.remove(&page);
                }
            }
        });
    }

    let on_change = present.clone();
    let awake = StayAwake::new(&window);
    model.subscribe_while(&window, move |model, change| {
        if change == Change::Screen {
            on_change(false);
            awake.follow(model.screen() == Screen::Call);
            // Moving on from a screen is being done with what was typed on
            // it.
            model.keep_passphrase();
        }
    });
    keep_passphrase_when_left(&window, model);
    leave_before_closing(&window, &context);
    call_keys(&window, model);
    let on_language = present.clone();
    let model = context.model.downgrade();
    crate::i18n::on_change_while(&window, move |_| {
        let in_call = model
            .upgrade()
            .is_some_and(|model| model.screen().is_call());
        if !in_call {
            on_language(true);
        }
    });

    present(false);
    window
}

/// How long a call is given to be left before the app goes anyway: a
/// leave that takes longer is a server that is not answering, and the app
/// is not held open for it (Windows' MainWindow, two seconds too).
const LEAVE_WITHIN: Duration = Duration::from_secs(2);

/// Leaves the call, if there is one, giving it at most [`LEAVE_WITHIN`].
async fn leave_within(model: &AppModel, media: &dyn Media) {
    let leaving = Box::pin(room::leave(model, media));
    let late = Box::pin(glib::timeout_future(LEAVE_WITHIN));
    if let futures_util::future::Either::Right(_) =
        futures_util::future::select(leaving, late).await
    {
        log("the call did not finish leaving in time; closing anyway");
    }
}

/// Closing the window in a call leaves the call first, and the window goes
/// once it has (or two seconds have passed); so does the session ending the
/// app. Without it GtkApplication quit with the call still open, the core
/// never said goodbye, and everybody else kept a frozen picture of this
/// person until the server gave up on them.
fn leave_before_closing(window: &adw::ApplicationWindow, context: &Context) {
    let leaving = Rc::new(Cell::new(false));
    {
        let (model, media, leaving) = (
            context.model.downgrade(),
            context.media.clone(),
            leaving.clone(),
        );
        window.connect_close_request(move |window| {
            let Some(model) = model.upgrade() else {
                return glib::Propagation::Proceed;
            };
            model.keep_passphrase();
            if leaving.get() || room::call_of(&model).is_none() {
                return glib::Propagation::Proceed;
            }
            leaving.set(true);
            log("the window was closed in a call: leaving it first");
            let (window, media) = (window.downgrade(), media.clone());
            glib::spawn_future_local(async move {
                leave_within(&model, media.as_ref()).await;
                if let Some(window) = window.upgrade() {
                    window.close();
                }
            });
            glib::Propagation::Stop
        });
    }

    // The session's ending, and a terminal's interrupt, the same way: out of
    // the call, then out. A second signal while that is under way quits at
    // once, as a second Ctrl-C is meant to.
    let Some(application) = window.application() else {
        return;
    };
    for signal in [libc::SIGTERM, libc::SIGINT, libc::SIGHUP] {
        let (model, media, application, leaving) = (
            context.model.downgrade(),
            context.media.clone(),
            application.downgrade(),
            leaving.clone(),
        );
        on_signal(signal, move || {
            let (Some(model), Some(application)) = (model.upgrade(), application.upgrade()) else {
                return glib::ControlFlow::Break;
            };
            if leaving.replace(true) {
                log("asked again to quit: quitting now");
                application.quit();
                return glib::ControlFlow::Continue;
            }
            log(&format!("signal {signal}: leaving any call, then quitting"));
            model.keep_passphrase();
            let media = media.clone();
            glib::spawn_future_local(async move {
                leave_within(&model, media.as_ref()).await;
                application.quit();
            });
            glib::ControlFlow::Continue
        });
    }
}

/// Calls `handler` on the main loop whenever `signal` arrives
/// (`g_unix_signal_add_full`, which glib-rs moved out of `glib` into a crate
/// of its own; the function is libglib's, linked already). Only from the
/// main thread: the handler is not `Send`, and the source is attached to
/// the default main context, which runs it there.
fn on_signal(signal: i32, handler: impl FnMut() -> glib::ControlFlow + 'static) {
    use std::ffi::{c_int, c_uint, c_void};

    type Handler = Box<dyn FnMut() -> glib::ControlFlow>;

    #[link(name = "glib-2.0")]
    extern "C" {
        fn g_unix_signal_add_full(
            priority: c_int,
            signum: c_int,
            handler: Option<unsafe extern "C" fn(*mut c_void) -> c_int>,
            user_data: *mut c_void,
            notify: Option<unsafe extern "C" fn(*mut c_void)>,
        ) -> c_uint;
    }

    unsafe extern "C" fn call(data: *mut c_void) -> c_int {
        // SAFETY: `data` is the Box made below, alive until `release`.
        let handler = unsafe { &mut *(data as *mut Handler) };
        match handler() {
            glib::ControlFlow::Continue => 1,
            glib::ControlFlow::Break => 0,
        }
    }

    unsafe extern "C" fn release(data: *mut c_void) {
        // SAFETY: given back once by GLib when the source goes.
        drop(unsafe { Box::from_raw(data as *mut Handler) });
    }

    let handler: Box<Handler> = Box::new(Box::new(handler));
    // SAFETY: GLib owns the pointer from here and frees it through
    // `release`; G_PRIORITY_DEFAULT is 0.
    unsafe {
        g_unix_signal_add_full(
            0,
            signal,
            Some(call),
            Box::into_raw(handler) as *mut c_void,
            Some(release),
        );
    }
}

/// The passphrase typed into a field is kept once the field is left (the
/// model's `keep_passphrase`): focus moving to anything that is not a
/// passphrase field. Typed into, it is only held, not written a key at a
/// time into the keyring.
fn keep_passphrase_when_left(window: &adw::ApplicationWindow, model: &AppModel) {
    let model = model.downgrade();
    window.connect_focus_widget_notify(move |window| {
        let in_passphrase = gtk::prelude::GtkWindowExt::focus(window).is_some_and(|focus| {
            focus.is::<gtk::PasswordEntry>()
                || focus.ancestor(gtk::PasswordEntry::static_type()).is_some()
        });
        if !in_passphrase {
            if let Some(model) = model.upgrade() {
                model.keep_passphrase();
            }
        }
    });
}

/// The two switches people reach for without looking, on the keys other
/// call apps use for them (the Mac's ⌘D and ⌘E, its Call menu): Ctrl+D the
/// microphone, Ctrl+E the camera, and Leave, as window actions. Leave has no
/// key, as it has none on the Mac; it is there for the desktop to offer.
fn call_keys(window: &adw::ApplicationWindow, model: &AppModel) {
    let actions: [(&str, fn(&AppModel)); 3] = [
        ("toggle-microphone", room::toggle_microphone),
        ("toggle-camera", room::toggle_camera),
        ("leave-call", |model: &AppModel| {
            let weak = model.downgrade();
            glib::spawn_future_local(async move {
                if let Some(model) = weak.upgrade() {
                    let media = room::media_or_quiet(&model);
                    room::leave(&model, media.as_ref()).await;
                }
            });
        }),
    ];
    for (name, act) in actions {
        let action = gtk::gio::SimpleAction::new(name, None);
        let weak = model.downgrade();
        action.connect_activate(move |_, _| {
            if let Some(model) = weak.upgrade() {
                if model.screen() == Screen::Call {
                    act(&model);
                }
            }
        });
        window.add_action(&action);
    }
    if let Some(application) = window.application() {
        application.set_accels_for_action("win.toggle-microphone", &["<Control>d"]);
        application.set_accels_for_action("win.toggle-camera", &["<Control>e"]);
    }
}

/// Keeps the session from idling (the screen dimming, locking) and from
/// suspending while a call is on screen, as the Mac's StayAwake does and as a
/// browser playing a call's video does; let go when the call is.
struct StayAwake {
    window: glib::WeakRef<adw::ApplicationWindow>,
    cookie: Cell<Option<u32>>,
}

impl StayAwake {
    fn new(window: &adw::ApplicationWindow) -> Rc<StayAwake> {
        let weak = glib::WeakRef::new();
        weak.set(Some(window));
        Rc::new(StayAwake {
            window: weak,
            cookie: Cell::new(None),
        })
    }

    fn follow(&self, in_call: bool) {
        let Some(window) = self.window.upgrade() else {
            return;
        };
        let Some(application) = window.application() else {
            return;
        };
        match (in_call, self.cookie.get()) {
            (true, None) => {
                let cookie = application.inhibit(
                    Some(&window),
                    gtk::ApplicationInhibitFlags::IDLE | gtk::ApplicationInhibitFlags::SUSPEND,
                    Some(&t("In a call")),
                );
                // Nought is the desktop declining, which is nothing to
                // undo later.
                if cookie != 0 {
                    log("keeping the machine awake for the call");
                    self.cookie.set(Some(cookie));
                }
            }
            (false, Some(cookie)) => {
                application.uninhibit(cookie);
                self.cookie.set(None);
                log("the machine may sleep again");
            }
            _ => {}
        }
    }
}

/// The screen for `screen`, from the module that owns it.
fn page_for(context: &Context, screen: Screen) -> gtk::Widget {
    match screen {
        Screen::Starting => starting(),
        Screen::Unreachable => unreachable(context),
        Screen::SignIn | Screen::Lobby | Screen::Prejoin | Screen::Knocking | Screen::Waiting => {
            lobby::page(context, screen)
        }
        Screen::Call | Screen::Ended => room::page(context, screen),
    }
}

/// While the deployment is asked what it is: the mark alone, as every app
/// opens, so that nothing is offered that the answer might take away.
fn starting() -> gtk::Widget {
    let page = gtk::Box::new(gtk::Orientation::Vertical, 0);
    page.append(&kit::bar());
    let mark = kit::mark(76);
    mark.set_vexpand(true);
    mark.set_valign(gtk::Align::Center);
    page.append(&kit::Arrival::new(&mark).rises(0));
    page.upcast()
}

/// The deployment did not answer: the sentence, and the one way on.
fn unreachable(context: &Context) -> gtk::Widget {
    let page = gtk::Box::new(gtk::Orientation::Vertical, 0);
    page.append(&kit::bar());

    let column = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(14)
        .valign(gtk::Align::Center)
        .halign(gtk::Align::Center)
        .vexpand(true)
        .width_request(320)
        .build();
    column.append(&kit::mark(76));
    let sentence = context
        .model
        .problem()
        .unwrap_or_else(|| t("Could not reach the server."));
    column.append(&kit::muted(&sentence));
    let again = kit::primary_button(&t("Try again"));
    again.set_halign(gtk::Align::Center);
    again.set_width_request(200);
    column.append(&again);
    page.append(&kit::Arrival::new(&column).rises(0));

    let model = context.model.clone();
    again.connect_clicked(move |_| model.connect());
    let button = again.downgrade();
    context.model.subscribe_while(&page, move |model, change| {
        if change == Change::Busy {
            if let Some(button) = button.upgrade() {
                button.set_sensitive(!model.busy());
            }
        }
    });
    page.upcast()
}

/// Renders the window into a PNG at `path` once the screen shown is
/// `wanted` (any but starting when `None`) and its arrivals have settled: a
/// picture of what GTK drew, whatever compositor there is, for the headless
/// runs (run-headless.sh) and for comparing with the compositor's own. The
/// widget tree with every widget's bounds goes beside it (`.tree.txt`).
#[cfg(debug_assertions)]
pub fn snapshot_when_settled(window: &adw::ApplicationWindow, model: &AppModel, path: String) {
    let wanted = model.launch().show.clone();
    // The rises take 420 ms after delays of up to about 100 ms, and the
    // crossfade 320; a second and a half is past all of them.
    let after = model.launch().snapshot_after_ms.unwrap_or(1500);
    let taken = Rc::new(std::cell::Cell::new(false));
    let take = {
        let (window, taken) = (window.downgrade(), taken.clone());
        move |model: &AppModel| {
            let screen = model.screen();
            let ready = match wanted.as_deref() {
                Some(name) => screen.name() == name,
                None => screen != Screen::Starting,
            };
            if !ready || taken.replace(true) {
                return;
            }
            let (window, path) = (window.clone(), path.clone());
            glib::timeout_add_local_once(std::time::Duration::from_millis(after), move || {
                if let Some(window) = window.upgrade() {
                    // The tree first, so that it is there by the time the
                    // log says the picture is.
                    let tree = format!("{}.tree.txt", path.trim_end_matches(".png"));
                    let mut lines = String::new();
                    describe(window.upcast_ref(), 0, &mut lines);
                    if let Err(fault) = std::fs::write(&tree, lines) {
                        log(&format!("the widget tree could not be written: {fault}"));
                    }
                    render_after_paint(&window, path);
                }
            });
        }
    };
    take(model);
    model.subscribe_while(window, move |model, change| {
        if change == Change::Screen {
            take(model);
        }
    });
}

/// Every widget on screen with its place and size, its style classes and
/// what it says, one a line and indented by depth: what a test reads to
/// check a layout without reading pixels (a column clamped to its width, a
/// button the size the scale says).
#[cfg(debug_assertions)]
fn describe(widget: &gtk::Widget, depth: usize, lines: &mut String) {
    use std::fmt::Write as _;
    if !widget.is_visible() {
        return;
    }
    let bounds = widget
        .root()
        .and_then(|root| widget.compute_bounds(&root))
        .map(|rect| {
            format!(
                "{:.0},{:.0} {:.0}x{:.0}",
                rect.x(),
                rect.y(),
                rect.width(),
                rect.height()
            )
        })
        .unwrap_or_default();
    let classes = widget
        .css_classes()
        .iter()
        .map(|class| format!(".{class}"))
        .collect::<String>();
    let said = widget
        .downcast_ref::<gtk::Label>()
        .map(|label| format!(" {:?}", label.label().as_str()))
        .or_else(|| {
            widget
                .downcast_ref::<gtk::Button>()
                .and_then(|button| button.label())
                .map(|text| format!(" {:?}", text.as_str()))
        })
        .unwrap_or_default();
    let name = widget.widget_name();
    let _ = writeln!(
        lines,
        "{:indent$}{} {bounds}{classes}{}{said}{}",
        "",
        widget.type_().name(),
        if name.as_str() != widget.type_().name() {
            format!(" #{name}")
        } else {
            String::new()
        },
        if widget.is_sensitive() {
            ""
        } else {
            " (insensitive)"
        },
        indent = depth * 2,
    );
    let mut child = widget.first_child();
    while let Some(next) = child {
        describe(&next, depth + 1, lines);
        child = next.next_sibling();
    }
}

/// Renders the window's picture right after its next paint, when every
/// widget's render node is fresh.
///
/// A widget's render node is thrown away the moment anything under it asks
/// to be drawn again, and a paintable of the window made in between has
/// nothing to draw: with a camera's preview or a call's pictures redrawing
/// thirty times a second that is most of the time, and a pre-join with the
/// camera on failed its picture with "nothing was drawn" one run in three.
/// Just after a paint, and before the main loop hands over the next frame,
/// the nodes are all there.
#[cfg(debug_assertions)]
fn render_after_paint(window: &adw::ApplicationWindow, path: String) {
    let write = |window: &adw::ApplicationWindow, path: &str| match render_png(window, path) {
        Ok(()) => log(&format!("snapshot written to {path}")),
        Err(fault) => log(&format!("snapshot failed: {fault}")),
    };
    let Some(clock) = window.frame_clock() else {
        write(window, &path);
        return;
    };
    let handler: Rc<std::cell::Cell<Option<glib::SignalHandlerId>>> = Rc::default();
    let id = clock.connect_after_paint({
        let (window, handler) = (window.downgrade(), handler.clone());
        move |clock| {
            if let Some(id) = handler.take() {
                clock.disconnect(id);
            }
            if let Some(window) = window.upgrade() {
                write(&window, &path);
            }
        }
    });
    handler.set(Some(id));
    window.queue_draw();
}

#[cfg(debug_assertions)]
fn render_png(window: &adw::ApplicationWindow, path: &str) -> Result<(), String> {
    let (width, height) = (window.width(), window.height());
    if width <= 0 || height <= 0 {
        return Err("the window has no size yet".into());
    }
    let paintable = gtk::WidgetPaintable::new(Some(window));
    let snapshot = gtk::Snapshot::new();
    paintable.snapshot(&snapshot, width as f64, height as f64);
    let node = snapshot.to_node().ok_or("nothing was drawn")?;
    let renderer = window.renderer().ok_or("the window has no renderer")?;
    let texture = renderer.render_texture(&node, None);
    texture.save_to_png(path).map_err(|fault| fault.to_string())
}
