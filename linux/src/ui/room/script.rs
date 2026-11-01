//! What a test asks the room to do, and picture, in a debug run:
//! TOMOSHIBI_DEBUG_ROOM, a list of `<milliseconds>:<action>` separated by
//! semicolons, each counted from the moment the call is joined.
//!
//! The actions are the presses a person would make, made through the same
//! functions the buttons call: `panel=<messages|sound|devices|host|none>`,
//! `popover=<reactions|leave|share|more|signal|none>`, `say=<text>`,
//! `hand=<up|down>`, `react=<0-5>`, `mic=<on|off>`, `camera=<on|off>`,
//! `share=<on|off>`, `pin=<me|first|none>`, `fill=<on|off>`, `page=<+1|-1>`,
//! `volume=<0-100>` (the first person's), and two that only look:
//! `shot=<name>`, GTK's own picture of the window and its widget tree beside
//! the run's main snapshot, and `report`, every tile and what it shows,
//! written to the log. A headless run (linux/build/run-headless.sh) keeps
//! the app until its main snapshot, so a script ends before
//! TOMOSHIBI_DEBUG_SNAPSHOT_AFTER_MS.
//!
//! Debug builds only, like every switch (app::Launch).

use std::time::Duration;

use adw::prelude::*;

use super::{Hearing, Panel, Pop, RoomState};
use crate::app::{log, AppModel};

pub fn run(model: &AppModel) {
    let Ok(script) = std::env::var("TOMOSHIBI_DEBUG_ROOM") else {
        return;
    };
    for step in script
        .split(';')
        .map(str::trim)
        .filter(|step| !step.is_empty())
    {
        let Some((at, action)) = step.split_once(':') else {
            log(&format!("room script: no time in {step:?}"));
            continue;
        };
        let Ok(at) = at.trim().parse::<u64>() else {
            log(&format!("room script: no time in {step:?}"));
            continue;
        };
        let (weak, action) = (model.downgrade(), action.trim().to_owned());
        glib::timeout_add_local_once(Duration::from_millis(at), move || {
            if let Some(model) = weak.upgrade() {
                act(&model, &action);
            }
        });
    }
}

fn act(model: &AppModel, action: &str) {
    let (verb, value) = action.split_once('=').unwrap_or((action, ""));
    log(&format!("room script: {verb} {value}"));
    let room = model.kept::<RoomState>();
    match verb {
        "panel" => {
            let wanted = Panel::of(value);
            let open = room.view.borrow().panel;
            match (open, wanted) {
                (Some(open), None) => super::toggle_panel(model, open),
                (_, Some(wanted)) if open != Some(wanted) => super::toggle_panel(model, wanted),
                _ => {}
            }
        }
        "popover" => {
            room.view.borrow_mut().pop = Pop::of(value);
            super::changed(model);
        }
        "say" => {
            room.view.borrow_mut().draft = value.to_owned();
            super::send_draft(model);
        }
        "hand" => super::set_hand(model, value == "up"),
        "react" => {
            let choices = tomoshibi_protocol::hands::reaction_choices();
            if let Some(what) = value.parse::<usize>().ok().and_then(|at| choices.get(at)) {
                super::react(model, what);
            }
        }
        "mic" => super::set_microphone(model, value == "on"),
        "camera" => super::set_camera(model, value == "on"),
        "share" => {
            if value == "on" {
                super::start_share(model);
            } else {
                super::stop_share(model);
            }
        }
        "pin" => {
            let first = room.talk.borrow().as_ref().and_then(|talk| {
                talk.people
                    .first()
                    .map(|person| format!("{}/camera", person.id))
            });
            let id = match value {
                "me" => Some(super::talk::ME.to_owned()),
                "first" => first,
                _ => None,
            };
            room.view.borrow_mut().pin.pin(id);
            super::changed(model);
        }
        "fill" => {
            if value == "on" {
                super::fill(model);
            } else {
                super::unfill(model);
            }
        }
        "page" => super::turn_page(model, value.parse().unwrap_or(1)),
        "volume" => {
            let first = room
                .talk
                .borrow()
                .as_ref()
                .and_then(|talk| talk.people.first().map(|person| person.id.clone()));
            if let (Some(first), Ok(level)) = (first, value.parse::<f32>()) {
                super::set_hearing(
                    model,
                    &first,
                    Hearing {
                        level: level / 100.0,
                        blocked: false,
                    },
                );
            }
        }
        "report" => report(model),
        "shot" => shot(model, value),
        other => log(&format!("room script: no such action {other:?}")),
    }
}

fn report(model: &AppModel) {
    let room = model.kept::<RoomState>();
    if let Some(page) = room.page.borrow().clone() {
        for line in page.report().lines() {
            log(&format!("room report: {line}"));
        }
    }
    let talk = room.talk.borrow();
    if let Some(talk) = talk.as_ref() {
        log(&format!(
            "room report: {} in the call, {} messages, hands {:?}, unread {}",
            talk.count(),
            talk.messages.len(),
            talk.raised_hands(),
            talk.unread
        ));
        for line in &talk.messages {
            log(&format!(
                "room report: message from {}: {}",
                line.name, line.text
            ));
        }
    }
}

/// GTK's own picture of the window, beside the run's main snapshot:
/// `<label>.<name>.gtk.png` and its widget tree.
fn shot(model: &AppModel, name: &str) {
    let Some(main) = model.launch().snapshot.clone() else {
        log("room script: no TOMOSHIBI_DEBUG_SNAPSHOT to put pictures beside");
        return;
    };
    let base = main
        .trim_end_matches(".png")
        .trim_end_matches(".gtk")
        .to_owned();
    let path = format!("{base}.{name}.gtk.png");
    let room = model.kept::<RoomState>();
    let Some(window) = room.page.borrow().clone().and_then(|page| page.window()) else {
        return;
    };
    let mut tree = String::new();
    describe(window.upcast_ref(), 0, &mut tree);
    let _ = std::fs::write(format!("{base}.{name}.gtk.tree.txt"), tree);
    render_when_laid_out(window, path, 20);
}

/// GTK's picture of the window once it is laid out: a widget waiting for
/// its allocation draws nothing into a WidgetPaintable, and a call whose
/// tiles are moving or whose reactions are rising is often waiting for one,
/// so the picture is asked for again a frame later until it comes.
fn render_when_laid_out(window: adw::ApplicationWindow, path: String, tries: u32) {
    let (width, height) = (window.width(), window.height());
    let paintable = gtk::WidgetPaintable::new(Some(&window));
    let snapshot = gtk::Snapshot::new();
    paintable.snapshot(&snapshot, width as f64, height as f64);
    let drawn = snapshot
        .to_node()
        .and_then(|node| Some((node, window.renderer()?)));
    let Some((node, renderer)) = drawn else {
        if tries == 0 {
            log(&format!("room script: nothing was drawn for {path}"));
            return;
        }
        glib::timeout_add_local_once(Duration::from_millis(50), move || {
            render_when_laid_out(window, path, tries - 1);
        });
        return;
    };
    match renderer.render_texture(&node, None).save_to_png(&path) {
        Ok(()) => log(&format!("room script: picture written to {path}")),
        Err(fault) => log(&format!("room script: picture failed: {fault}")),
    }
}

/// Every widget showing with its place, classes and words, as the window's
/// own tree is written (ui/window.rs `describe`).
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
    let classes: String = widget
        .css_classes()
        .iter()
        .map(|class| format!(".{class}"))
        .collect();
    let said = widget
        .downcast_ref::<gtk::Label>()
        .map(|label| format!(" {:?}", label.label().as_str()))
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
        if widget.opacity() < 1.0 {
            format!(" (opacity {:.2})", widget.opacity())
        } else {
            String::new()
        },
        indent = depth * 2,
    );
    let mut child = widget.first_child();
    while let Some(next) = child {
        describe(&next, depth + 1, lines);
        child = next.next_sibling();
    }
}
