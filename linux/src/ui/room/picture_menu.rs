//! What can be done to one picture, on the gesture that has always meant
//! that: the secondary press, or a long press (PictureMenu.tsx, the Mac's
//! `pictureMenu`). Everything here is reachable another way, and that is the
//! point: a double press nobody finds is a feature nobody has, and a menu
//! says the names out loud. Each item names the picture, since a menu opened
//! on a small tile in a busy grid has to say which one it caught.
//!
//! Drawn in the window, beside the bar's popovers and in their style, one
//! open at a time across the whole screen: opening it closes the bar's
//! popover, a bar popover opening closes it, and a press outside it or
//! Escape closes it. It arrives and leaves as they do.

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use adw::prelude::*;
use gtk::{gdk, glib, graphene};
use tomoshibi_protocol::identity::signature_of;

use super::{
    motion::{Away, Motion},
    stage::Stage,
    talk::{ME, MY_SHARE},
    Hearing, Panel, RoomState, Sound, CHANGED,
};
use crate::{
    app::AppModel,
    i18n::{t, t_with},
};

/// One item of the menu.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum Item {
    Larger(String),
    Everybody,
    Fill,
    LeaveFullscreen,
    /// The person's sound for this picture: muted (`true`) or back on.
    Mute(String, Sound, bool),
    SoundSettings,
    CopySignature(String),
    CopyLink,
    Rule,
}

/// What the menu offers on the picture `id`, from the room as it is.
pub(super) fn items(model: &AppModel, id: &str) -> Vec<Item> {
    let room = model.kept::<RoomState>();
    let talk = room.talk.borrow();
    let Some(talk) = talk.as_ref() else {
        return Vec::new();
    };
    let view = room.view.borrow();
    let (owner, kind) = id.rsplit_once('/').unwrap_or((id, "camera"));
    let mine = id == ME || id == MY_SHARE;
    let identity = if mine {
        talk.local_identity.clone()
    } else {
        owner.to_owned()
    };
    let person = (!mine).then(|| talk.person(&identity)).flatten();
    let name = if mine {
        talk.local_name.clone()
    } else {
        match person {
            Some(person) => person.label(),
            None => return Vec::new(),
        }
    };
    let screen = kind == "screen";
    let label = if screen {
        t_with("{name} (screen)", &[("name", &name)])
    } else {
        name
    };
    let on_stage = view.pin.pinned() == Some(id);
    let mut out = vec![if on_stage {
        Item::Everybody
    } else {
        Item::Larger(label.clone())
    }];
    // Only for the picture already on the stage, which is what fills the
    // screen: anywhere else it would have to be put there first, and the
    // press would do two things under one name.
    if on_stage {
        out.push(if view.filled {
            Item::LeaveFullscreen
        } else {
            Item::Fill
        });
    }
    // Nobody hears themselves, so nothing here for one's own pictures.
    if person.is_some() {
        let sound = if screen { Sound::Screen } else { Sound::Voice };
        let blocked = view.hearing_of(&identity, sound).blocked;
        out.push(Item::Rule);
        out.push(Item::Mute(label, sound, !blocked));
        out.push(Item::SoundSettings);
    }
    // Only a signature somebody earned: an issued one is fresh each visit,
    // and a copy of it is true for this call and misleading afterwards.
    if let Some(signature) = signature_of(identity).filter(|mark| mark.proven) {
        out.push(Item::Rule);
        out.push(Item::CopySignature(signature.trip));
    }
    // The room's own, at the foot of every picture's: in a call of two the
    // pictures are all there is, and the room's menu has nowhere else.
    if super::link_works(model) {
        out.push(Item::Rule);
        out.push(Item::CopyLink);
    }
    out
}

fn words(item: &Item) -> (String, &'static str) {
    match item {
        Item::Larger(name) => (
            t_with("Show {name} larger", &[("name", name)]),
            "zoom-in-symbolic",
        ),
        Item::Everybody => (t("Show everybody"), "view-grid-symbolic"),
        Item::Fill => (t("Fill the screen"), "view-fullscreen-symbolic"),
        Item::LeaveFullscreen => (t("Leave fullscreen"), "view-restore-symbolic"),
        Item::Mute(name, _, true) => (
            t_with("Mute {name}", &[("name", name)]),
            "audio-volume-muted-symbolic",
        ),
        Item::Mute(name, _, false) => (
            t_with("Unmute {name}", &[("name", name)]),
            "audio-volume-high-symbolic",
        ),
        Item::SoundSettings => (t("Sound settings"), "multimedia-volume-control-symbolic"),
        Item::CopySignature(_) => (t("Copy signature"), "edit-copy-symbolic"),
        Item::CopyLink => (t("Copy link"), "insert-link-symbolic"),
        Item::Rule => (String::new(), ""),
    }
}

fn act(model: &AppModel, id: &str, item: &Item) {
    match item {
        Item::Larger(_) | Item::Everybody => super::toggle_stage(model, id),
        Item::Fill => super::fill(model),
        Item::LeaveFullscreen => super::unfill(model),
        Item::Mute(_, sound, mute) => {
            let identity = model
                .kept::<RoomState>()
                .talk
                .borrow()
                .as_ref()
                .and_then(|talk| {
                    let owner = id.rsplit_once('/').map_or(id, |(owner, _)| owner);
                    talk.person(owner).map(|person| person.id.clone())
                });
            if let Some(identity) = identity {
                let now = model
                    .kept::<RoomState>()
                    .view
                    .borrow()
                    .hearing_of(&identity, *sound);
                super::set_hearing(
                    model,
                    &identity,
                    *sound,
                    Hearing {
                        blocked: *mute,
                        ..now
                    },
                );
            }
        }
        Item::SoundSettings => {
            if model.kept::<RoomState>().view.borrow().panel != Some(Panel::Sound) {
                super::toggle_panel(model, Panel::Sound);
            }
        }
        Item::CopySignature(trip) => super::copy(trip),
        Item::CopyLink => super::copy_link(model),
        Item::Rule => {}
    }
}

const MENU_NAME: &str = "picture-menu";

/// Lays the menu over `root`, the call's screen, opened on the pictures of
/// `stage`.
pub fn attach(root: &gtk::Overlay, stage: &Stage, model: &AppModel) {
    let card = gtk::Box::new(gtk::Orientation::Vertical, 2);
    card.add_css_class("tmsb-menu");
    card.set_width_request(240);
    let motion = Motion::hidden(&card, Away::GROW);
    motion.set_widget_name(MENU_NAME);
    root.add_overlay(&motion);
    // Where it was opened, in the call's screen's coordinates, and on what.
    let at: Rc<Cell<(f64, f64)>> = Rc::default();
    let on: Rc<RefCell<Option<String>>> = Rc::default();

    let close = {
        let (motion, on) = (motion.downgrade(), on.clone());
        move || {
            if on.borrow_mut().take().is_some() {
                if let Some(motion) = motion.upgrade() {
                    motion.show(false);
                }
            }
        }
    };
    let close: Rc<dyn Fn()> = Rc::new(close);

    let open = {
        let (root, motion, card, at, on, close, model) = (
            root.downgrade(),
            motion.downgrade(),
            card.downgrade(),
            at.clone(),
            on.clone(),
            close.clone(),
            model.downgrade(),
        );
        move |id: String, x: f64, y: f64| {
            let (Some(root), Some(motion), Some(card), Some(model)) = (
                root.upgrade(),
                motion.upgrade(),
                card.upgrade(),
                model.upgrade(),
            ) else {
                return;
            };
            if super::call_of(&model).is_none() {
                return;
            }
            let offered = items(&model, &id);
            if offered.is_empty() {
                return;
            }
            // One open at a time: the bar's popover goes.
            super::close_pop(&model);
            while let Some(child) = card.first_child() {
                card.remove(&child);
            }
            let mut first: Option<gtk::Button> = None;
            for item in offered {
                if item == Item::Rule {
                    let rule = gtk::Box::new(gtk::Orientation::Horizontal, 0);
                    rule.add_css_class("tmsb-menu-rule");
                    card.append(&rule);
                    continue;
                }
                let (title, icon) = words(&item);
                let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
                row.append(&gtk::Image::from_icon_name(icon));
                let heading = gtk::Label::builder()
                    .label(&title)
                    .xalign(0.0)
                    .ellipsize(gtk::pango::EllipsizeMode::End)
                    .max_width_chars(30)
                    .build();
                heading.add_css_class("title");
                row.append(&heading);
                let button = gtk::Button::builder().child(&row).build();
                button.add_css_class("tmsb-menu-row");
                let (weak, id, close) = (model.downgrade(), id.clone(), close.clone());
                button.connect_clicked(move |_| {
                    close();
                    if let Some(model) = weak.upgrade() {
                        act(&model, &id, &item);
                    }
                });
                if first.is_none() {
                    first = Some(button.clone());
                }
                card.append(&button);
            }
            at.set((x, y));
            on.replace(Some(id));
            // Over everything laid on the call's screen after it.
            motion.insert_before(&root, None::<&gtk::Widget>);
            motion.show(true);
            root.queue_allocate();
            // The keys go to it, so that Escape closes it and the arrows
            // walk it.
            if let Some(first) = first {
                first.grab_focus();
            }
        }
    };
    let open: Rc<dyn Fn(String, f64, f64)> = Rc::new(open);
    #[cfg(debug_assertions)]
    TEST_HANDLES.with(|held| {
        let (stage_ref, root_ref) = (glib::WeakRef::new(), glib::WeakRef::new());
        stage_ref.set(Some(stage));
        root_ref.set(Some(root));
        held.replace(Some((
            stage_ref,
            root_ref,
            Rc::downgrade(&open),
            Rc::downgrade(&close),
        )));
    });

    // It is placed where it was opened, kept inside the screen.
    {
        let at = at.clone();
        root.connect_get_child_position(move |overlay, child| {
            if child.widget_name() != MENU_NAME {
                return None;
            }
            let (_, natural) = child.preferred_size();
            let (width, height) = (natural.width(), natural.height());
            let (x, y) = at.get();
            let x = (x.round() as i32).clamp(8, (overlay.width() - width - 8).max(8));
            let y = (y.round() as i32).clamp(8, (overlay.height() - height - 8).max(8));
            Some(gdk::Rectangle::new(x, y, width, height))
        });
    }

    // The secondary press on a picture, and a long press for a finger.
    let secondary = gtk::GestureClick::new();
    secondary.set_button(gdk::BUTTON_SECONDARY);
    {
        let (stage_weak, root_weak, open) = (stage.downgrade(), root.downgrade(), open.clone());
        secondary.connect_pressed(move |gesture, _, x, y| {
            let (Some(stage), Some(root)) = (stage_weak.upgrade(), root_weak.upgrade()) else {
                return;
            };
            let Some(id) = stage.tile_at(x, y) else {
                return;
            };
            gesture.set_state(gtk::EventSequenceState::Claimed);
            let point = stage
                .compute_point(&root, &graphene::Point::new(x as f32, y as f32))
                .unwrap_or_else(|| graphene::Point::new(x as f32, y as f32));
            open(id, point.x() as f64, point.y() as f64);
        });
    }
    stage.add_controller(secondary);
    let long = gtk::GestureLongPress::new();
    long.set_touch_only(true);
    {
        let (stage_weak, root_weak, open) = (stage.downgrade(), root.downgrade(), open.clone());
        long.connect_pressed(move |gesture, x, y| {
            let (Some(stage), Some(root)) = (stage_weak.upgrade(), root_weak.upgrade()) else {
                return;
            };
            let Some(id) = stage.tile_at(x, y) else {
                return;
            };
            gesture.set_state(gtk::EventSequenceState::Claimed);
            let point = stage
                .compute_point(&root, &graphene::Point::new(x as f32, y as f32))
                .unwrap_or_else(|| graphene::Point::new(x as f32, y as f32));
            open(id, point.x() as f64, point.y() as f64);
        });
    }
    stage.add_controller(long);

    // A press outside it closes it and goes no further, as the bar's
    // popovers are closed (page.rs).
    let outside = gtk::GestureClick::new();
    outside.set_button(0);
    outside.set_propagation_phase(gtk::PropagationPhase::Capture);
    {
        let (motion_weak, root_weak, on, close) = (
            motion.downgrade(),
            root.downgrade(),
            on.clone(),
            close.clone(),
        );
        outside.connect_pressed(move |gesture, _, x, y| {
            let (Some(motion), Some(root)) = (motion_weak.upgrade(), root_weak.upgrade()) else {
                return;
            };
            if on.borrow().is_none() {
                return;
            }
            let inside = motion.compute_bounds(&root).is_some_and(|bounds| {
                bounds.contains_point(&graphene::Point::new(x as f32, y as f32))
            });
            if inside {
                return;
            }
            close();
            gesture.set_state(gtk::EventSequenceState::Claimed);
        });
    }
    root.add_controller(outside);

    // Escape, from anywhere in it.
    let keys = gtk::EventControllerKey::new();
    {
        let close = close.clone();
        keys.connect_key_pressed(move |_, key, _, _| {
            if key == gdk::Key::Escape {
                close();
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
    }
    motion.add_controller(keys);

    // A bar popover opening, the picture going, or the call ending: it goes.
    let (on_room, weak_close) = (on.clone(), close.clone());
    model.subscribe_while(root, move |model, change| {
        if change != CHANGED {
            return;
        }
        let Some(id) = on_room.borrow().clone() else {
            return;
        };
        let room = model.kept::<RoomState>();
        let popover = room.view.borrow().pop.is_some();
        let gone = super::call_of(model).is_none() || items(model, &id).is_empty();
        if popover || gone {
            weak_close();
        }
    });
}

/// The menu as the script opens it (script.rs `menu=`): on the picture
/// `id`, at its middle, as a press there would; `None` closes it.
#[cfg(debug_assertions)]
type TestHandles = (
    glib::WeakRef<Stage>,
    glib::WeakRef<gtk::Overlay>,
    std::rc::Weak<dyn Fn(String, f64, f64)>,
    std::rc::Weak<dyn Fn()>,
);

#[cfg(debug_assertions)]
thread_local! {
    static TEST_HANDLES: RefCell<Option<TestHandles>> = const { RefCell::new(None) };
}

#[cfg(debug_assertions)]
pub(super) fn open_for_test(id: Option<String>) {
    TEST_HANDLES.with(|held| {
        let held = held.borrow();
        let Some((stage, root, open, close)) = held.as_ref() else {
            return;
        };
        let (Some(stage), Some(root), Some(open), Some(close)) = (
            stage.upgrade(),
            root.upgrade(),
            open.upgrade(),
            close.upgrade(),
        ) else {
            return;
        };
        let Some(id) = id else {
            close();
            return;
        };
        let Some(spot) = stage.spot(&id) else {
            crate::app::log(&format!("room script: no picture {id} for a menu"));
            return;
        };
        let middle = graphene::Point::new(
            (spot.x + spot.width / 2.0) as f32,
            (spot.y + spot.height / 2.0) as f32,
        );
        let point = stage.compute_point(&root, &middle).unwrap_or(middle);
        open(id, point.x() as f64, point.y() as f64);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_mute_item_says_what_pressing_it_does() {
        let mute = words(&Item::Mute("Ann".into(), Sound::Voice, true)).0;
        let unmute = words(&Item::Mute("Ann".into(), Sound::Voice, false)).0;
        assert_eq!(mute, "Mute Ann");
        assert_eq!(unmute, "Unmute Ann");
    }
}
