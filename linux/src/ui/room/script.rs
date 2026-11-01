//! What a test asks the room to do, and picture, in a debug run:
//! TOMOSHIBI_DEBUG_ROOM, a list of `<milliseconds>:<action>` separated by
//! semicolons, each counted from the moment the call is joined.
//!
//! The actions are the presses a person would make, made through the same
//! functions the buttons call:
//! `panel=<messages|sound|devices|host|music|watch|none>`,
//! `popover=<reactions|leave|share|more|signal|none>`, `say=<text>`,
//! `hand=<up|down>`, `react=<0-5>`, `mic=<on|off>`, `camera=<on|off>`,
//! `share=<on|off>`, `pin=<me|first|none>`, `fill=<on|off>`, `page=<+1|-1>`,
//! `volume=<0-100>` (the first person's), `leave`, the host's `invite`,
//! `silence`, `remove` and `make-host` (the first person); three that stand
//! in for the network rather than a person: `blink=<ms>`, the first person
//! said to have left and, that many milliseconds later (60 when not said),
//! to have joined again, as the core says it of everybody when its own
//! connection is made again; `network=<offline|online>`, the path the system
//! would report (`Call::path_seen`, nothing on it, or the path standing
//! now); and `woke`, the machine back from sleep (`Call::woke`); a few that
//! reach what a person would reach with the pointer or the keyboard:
//! `hear=<voice|screen|music>:<0-100|off|on>` (the first person's sound),
//! `menu=<me|first|first-screen|none>` (a picture's menu, opened at its
//! middle), `key=<microphone|camera|leave>` (the window's actions the
//! keys are bound to), `end=<ask|cancel>` (the host's question),
//! `relays=<name,name,...>` (relays to move the meeting to, as the
//! deployment would list them), `move=<relay>` and `place=<relay>` (the
//! server's word that the meeting, or this participant, is moving, and the
//! call closing under it 400 ms later, through `follow` as the core says
//! both), `again` (the ending's "Join again") and `close` (the host's
//! "End for everybody"); the share menu's `share-quality=<setting>` and
//! `share-fps=<rate>`; the song desk's `music=<step>` (music/mod.rs
//! `script`) and watching together's `watch=<step>` (watch/mod.rs
//! `script`); `stage=<screen|camera>`, the stage's own buttons; and
//! `controls=<side|always|idle|detailed|signal|hidden|hover|menu|focus|tab>`,
//! where the bar stands, what the signal says, and the pointer and the
//! keyboard on the pictures; and two that only look:
//! `shot=<name>`, GTK's own picture of the window and its widget tree beside
//! the run's main snapshot, and `report`, every tile and what it shows,
//! written to the log. A headless run (linux/build/run-headless.sh) keeps
//! the app until its main snapshot, so a script ends before
//! TOMOSHIBI_DEBUG_SNAPSHOT_AFTER_MS.
//!
//! The script runs once a run: a call joined again (a move, "Join again")
//! does not play it a second time.
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
    thread_local! {
        static RAN: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    }
    if RAN.with(|ran| ran.replace(true)) {
        return;
    }
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
                    super::Sound::Voice,
                    Hearing {
                        level: level / 100.0,
                        blocked: false,
                    },
                );
            }
        }
        "invite" => super::invite(model),
        "silence" | "remove" | "make-host" => {
            let first = room
                .talk
                .borrow()
                .as_ref()
                .and_then(|talk| talk.people.first().map(|person| person.id.clone()));
            if let Some(first) = first {
                match verb {
                    "silence" => super::silence(model, &first),
                    "remove" => super::turn_out(model, &first),
                    _ => super::hand_over(model, &first),
                }
            }
        }
        "blink" => blink(model, value.parse().unwrap_or(60)),
        "network" => {
            if let Some(call) = super::call_of(model) {
                let path = if value == "offline" {
                    tomoshibi_protocol::network::NetworkPath {
                        online: false,
                        route: String::new(),
                    }
                } else {
                    crate::app::network::path_of(&gio::NetworkMonitor::default())
                };
                call.path_seen(path);
            }
        }
        "woke" => {
            if let Some(call) = super::call_of(model) {
                call.woke();
            }
        }
        "hear" => {
            let (which, level) = value.split_once(':').unwrap_or((value, ""));
            let sound = match which {
                "screen" => super::Sound::Screen,
                "music" => super::Sound::Music,
                _ => super::Sound::Voice,
            };
            let first = room
                .talk
                .borrow()
                .as_ref()
                .and_then(|talk| talk.people.first().map(|person| person.id.clone()));
            if let Some(first) = first {
                let now = room.view.borrow().hearing_of(&first, sound);
                let hearing = match level {
                    "off" => Hearing {
                        blocked: true,
                        ..now
                    },
                    "on" => Hearing {
                        blocked: false,
                        ..now
                    },
                    level => Hearing {
                        level: level.parse::<f32>().unwrap_or(100.0) / 100.0,
                        ..now
                    },
                };
                super::set_hearing(model, &first, sound, hearing);
            }
        }
        "menu" => {
            let first = room
                .talk
                .borrow()
                .as_ref()
                .and_then(|talk| talk.people.first().map(|person| person.id.clone()));
            let id = match value {
                "me" => Some(super::talk::ME.to_owned()),
                "first" => first.map(|first| format!("{first}/camera")),
                "first-screen" => first.map(|first| format!("{first}/screen")),
                _ => None,
            };
            super::picture_menu::open_for_test(id);
        }
        "key" => {
            let name = match value {
                "microphone" => "win.toggle-microphone",
                "camera" => "win.toggle-camera",
                _ => "win.leave-call",
            };
            let window = gtk::Window::toplevels()
                .iter::<glib::Object>()
                .flatten()
                .find_map(|window| window.downcast::<adw::ApplicationWindow>().ok());
            if let Some(window) = window {
                if let Err(fault) = WidgetExt::activate_action(&window, name, None) {
                    log(&format!("room script: {name} did not run: {fault}"));
                }
            }
        }
        "end" => super::ask_to_end(model, value != "cancel"),
        "relays" => {
            let relays = value
                .split(',')
                .filter(|name| !name.is_empty())
                .map(|name| tomoshibi_protocol::relays::Relay {
                    name: name.to_owned(),
                    url: None,
                    region: None,
                    label: None,
                    lat: None,
                    lon: None,
                    probe: None,
                    maintenance: false,
                    fallback: false,
                    admin_only: false,
                })
                .collect();
            room.host.borrow_mut().relays = Some(relays);
            super::choose_relay(model, true);
        }
        "move" | "place" => {
            let Some(call) = super::call_of(model) else {
                return;
            };
            super::follow(
                model,
                &call,
                tomoshibi_core::call::CallEvent::Data {
                    topic: if verb == "move" { "moving" } else { "placing" }.to_owned(),
                    identity: None,
                    payload: value.as_bytes().to_vec(),
                },
            );
            let (weak, reason) = (
                model.downgrade(),
                if verb == "move" {
                    "RoomDeleted"
                } else {
                    "ParticipantRemoved"
                },
            );
            glib::timeout_add_local_once(Duration::from_millis(400), move || {
                if let Some(model) = weak.upgrade() {
                    super::follow(
                        &model,
                        &call,
                        tomoshibi_core::call::CallEvent::Ended {
                            reason: reason.to_owned(),
                        },
                    );
                }
            });
        }
        "again" => model.join_again(),
        "close" => super::end_for_everybody(model),
        "report" => report(model),
        "leave" => {
            let weak = model.downgrade();
            glib::spawn_future_local(async move {
                if let Some(model) = weak.upgrade() {
                    super::leave(&model, &crate::media::Quiet).await;
                }
            });
        }
        "share-quality" => {
            let quality =
                tomoshibi_protocol::share::share_remembered_quality(Some(value.to_owned()));
            super::choose_share(model, quality, None);
        }
        "share-fps" => {
            if let Ok(fps) = value.parse::<u32>() {
                let (quality, _) = super::share_shown(model);
                super::choose_share(model, quality, Some(fps));
            }
        }
        "music" => super::music::script(model, value),
        // Watching together, as its panel and its stage would drive it
        // (watch/mod.rs `script`): `watch=add <link>`, `play`, `pause`,
        // `seek <seconds>`, `skip`, `stop`, `panel`, `media`.
        "watch" => super::watch::script(model, value),
        // The screen or the camera of the first person sharing (or of the
        // first person, where nobody is) on the stage, as "Their screen"
        // and "Their camera" put it there.
        "stage" => {
            let first = room.talk.borrow().as_ref().and_then(|talk| {
                talk.people
                    .iter()
                    .find(|person| person.screen_sid().is_some())
                    .or_else(|| talk.people.first())
                    .map(|person| person.id.clone())
            });
            if let Some(first) = first {
                room.view
                    .borrow_mut()
                    .pin
                    .pin(Some(format!("{first}/{value}")));
                super::changed(model);
            }
        }
        // Where the controls stand and what the signal says (the settings'
        // values: side, always, idle; detailed, signal, hidden); `hover`,
        // the stage's buttons shown as the pointer shows them, and `menu`,
        // the first person's picture's menu, for a run that has no pointer.
        "controls" => {
            use crate::theme::{BADGE_KEY, CONTROLS_KEY};
            match value {
                "side" | "idle" => model.preferences().set(CONTROLS_KEY, value),
                "always" => model.preferences().remove(CONTROLS_KEY),
                "detailed" | "hidden" => model.preferences().set(BADGE_KEY, value),
                "signal" => model.preferences().remove(BADGE_KEY),
                "hover" => {
                    if let Some(page) = room.page.borrow().clone() {
                        page.hover_for_test();
                    }
                }
                // The keyboard: the first picture focused as Tab would
                // focus it, with its ring, and `tab`, the focus moved on
                // as Tab moves it, saying where it went.
                "focus" | "tab" => {
                    if let Some(page) = room.page.borrow().clone() {
                        page.focus_for_test(value == "tab");
                    }
                }
                // The first person's picture's menu, as a right press opens it.
                "menu" => {
                    if let Some(page) = room.page.borrow().clone() {
                        page.menu_for_test();
                    }
                }
                _ => {}
            }
            super::changed(model);
        }
        "shot" => shot(model, value),
        other => log(&format!("room script: no such action {other:?}")),
    }
}

/// The first person leaves and joins again `after` milliseconds later, told
/// to the room the way the core tells it of a reconnect: the same events,
/// through the same `follow`.
fn blink(model: &AppModel, after: u64) {
    let room = model.kept::<RoomState>();
    let Some(call) = super::call_of(model) else {
        return;
    };
    let first = room.talk.borrow().as_ref().and_then(|talk| {
        talk.people
            .first()
            .map(|person| tomoshibi_core::call::ParticipantInfo {
                identity: person.id.clone(),
                name: person.name.clone(),
                attributes: person.attributes.clone(),
                tracks: person.tracks.values().cloned().collect(),
            })
    });
    let Some(first) = first else {
        return;
    };
    super::follow(
        model,
        &call,
        tomoshibi_core::call::CallEvent::ParticipantLeft {
            identity: first.identity.clone(),
        },
    );
    let weak = model.downgrade();
    glib::timeout_add_local_once(Duration::from_millis(after), move || {
        if let Some(model) = weak.upgrade() {
            super::follow(
                &model,
                &call,
                tomoshibi_core::call::CallEvent::ParticipantJoined { participant: first },
            );
        }
    });
}

fn report(model: &AppModel) {
    let room = model.kept::<RoomState>();
    if let Some(page) = room.page.borrow().clone() {
        for line in page.report().lines() {
            log(&format!("room report: {line}"));
        }
    }
    let toasts: Vec<String> = room
        .view
        .borrow()
        .toasts
        .iter()
        .map(|(_, toast)| toast.title.clone())
        .collect();
    log(&format!(
        "room report: screen {}, notices {toasts:?}, moving {:?}, sharing {}",
        model.screen().name(),
        room.moving.borrow().as_ref().map(|(to, _)| to.clone()),
        room.local.share.borrow().is_some()
    ));
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
    // The app's window, whichever screen it shows: a picture of the ending
    // is taken after the call's screen has gone.
    let Some(window) = gtk::Window::toplevels()
        .iter::<glib::Object>()
        .flatten()
        .find_map(|window| window.downcast::<adw::ApplicationWindow>().ok())
    else {
        return;
    };
    let mut tree = String::new();
    describe(window.upcast_ref(), 0, &mut tree);
    let _ = std::fs::write(format!("{base}.{name}.gtk.tree.txt"), tree);
    render_when_laid_out(window, path, 20);
}

/// GTK's picture of the window once it is laid out: a widget waiting for
/// its allocation draws nothing into a WidgetPaintable, and a call whose
/// tiles are moving or whose reactions are rising is often waiting for one.
///
/// Taken just after a paint of the window's frame clock, when every
/// widget's render node is fresh, as the run's main snapshot is
/// (ui/window.rs `render_after_paint`), and asked for again at the next
/// paint when that one drew nothing: asked again on a 50 ms timer instead,
/// it landed between paints twenty times running while the pictures were
/// redrawing, and said "nothing was drawn" for a call that was on screen.
fn render_when_laid_out(window: adw::ApplicationWindow, path: String, tries: u32) {
    let Some(clock) = window.frame_clock() else {
        if !render_now(&window, &path) {
            log(&format!("room script: nothing was drawn for {path}"));
        }
        return;
    };
    let left = std::rc::Rc::new(std::cell::Cell::new(tries));
    let handler: std::rc::Rc<std::cell::Cell<Option<glib::SignalHandlerId>>> =
        std::rc::Rc::default();
    let id = clock.connect_after_paint({
        let (window, handler, left) = (window.downgrade(), handler.clone(), left.clone());
        move |clock| {
            let Some(window) = window.upgrade() else {
                if let Some(id) = handler.take() {
                    clock.disconnect(id);
                }
                return;
            };
            let done = render_now(&window, &path);
            let tries = left.get();
            if done || tries == 0 {
                if !done {
                    log(&format!("room script: nothing was drawn for {path}"));
                }
                if let Some(id) = handler.take() {
                    clock.disconnect(id);
                }
                return;
            }
            left.set(tries - 1);
            // Another paint is asked for, or a still window would never
            // give one.
            window.queue_draw();
        }
    });
    handler.set(Some(id));
    window.queue_draw();
}

/// The window drawn into a PNG at `path` now; false when nothing was drawn.
fn render_now(window: &adw::ApplicationWindow, path: &str) -> bool {
    let (width, height) = (window.width(), window.height());
    let paintable = gtk::WidgetPaintable::new(Some(window));
    let snapshot = gtk::Snapshot::new();
    paintable.snapshot(&snapshot, width as f64, height as f64);
    let drawn = snapshot
        .to_node()
        .and_then(|node| Some((node, window.renderer()?)));
    let Some((node, renderer)) = drawn else {
        return false;
    };
    match renderer.render_texture(&node, None).save_to_png(path) {
        Ok(()) => log(&format!("room script: picture written to {path}")),
        Err(fault) => log(&format!("room script: picture failed: {fault}")),
    }
    true
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
        "{:indent$}{} {bounds}{classes}{}{said}{}{}",
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
        // TOMOSHIBI_DEBUG_TREE_WIDTHS: the least and the natural width each
        // asks for, to find what holds a column wider than it was given.
        if std::env::var_os("TOMOSHIBI_DEBUG_TREE_WIDTHS").is_some() {
            let (least, natural, _, _) = widget.measure(gtk::Orientation::Horizontal, -1);
            format!(" [w {least}/{natural}]")
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
