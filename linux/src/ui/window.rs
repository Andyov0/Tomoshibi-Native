//! The main window: one screen at a time, crossfading from one to the next,
//! as the Mac app's single window changes screens.
//!
//! The window asks the module that owns a screen to build it (lobby/ for
//! the screens before a call, room/ for the call and its ending) when the
//! model moves to it, and throws the one it left away once the crossfade is
//! over, so that coming back to a screen builds it fresh from the model. A
//! change of language builds the screen showing again, in the new language,
//! and crossfades to it.
//!
//! The two screens with nothing to fill in, starting and unreachable, are
//! drawn here. Owned by the skeleton; a module changes what its screens look
//! like in its own files, never here.

use std::{cell::RefCell, rc::Rc};

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
    model.subscribe_while(&window, move |_, change| {
        if change == Change::Screen {
            on_change(false);
        }
    });
    let on_language = present.clone();
    crate::i18n::on_change(move |_| on_language(true));

    present(false);
    window
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
            // The rises take 420 ms after delays of up to about 100 ms, and
            // the crossfade 320; a second and a half is past all of them.
            glib::timeout_add_local_once(std::time::Duration::from_millis(1500), move || {
                if let Some(window) = window.upgrade() {
                    // The tree first, so that it is there by the time the
                    // log says the picture is.
                    let tree = format!("{}.tree.txt", path.trim_end_matches(".png"));
                    let mut lines = String::new();
                    describe(window.upcast_ref(), 0, &mut lines);
                    if let Err(fault) = std::fs::write(&tree, lines) {
                        log(&format!("the widget tree could not be written: {fault}"));
                    }
                    match render_png(&window, &path) {
                        Ok(()) => log(&format!("snapshot written to {path}")),
                        Err(fault) => log(&format!("snapshot failed: {fault}")),
                    }
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
