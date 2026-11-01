//! The picture of yourself before a call (Lobby.swift `CameraStage`): the
//! camera, live, where somebody has it on, and the animals on a plain ground
//! where they do not. The drawing fades out as the picture fades in, over
//! the settling time, and back the other way (art/README.md).
//!
//! The camera is the model's (`AppModel::preview_camera`), opened once for
//! whichever screen shows it first and handed to the call on joining, so
//! that the camera somebody just saw is the one the call sends and nothing
//! opens it twice. The camera last picked is the one previewed, where it is
//! still plugged in (PreJoin.tsx ~373: "a preview of another camera would be
//! a rehearsal of the wrong picture").

use std::{cell::Cell, rc::Rc};

use adw::prelude::*;
use gtk::glib;

use super::animals::{Placement, Portraits};
use crate::{
    app::{self, log, AppModel, DeviceSlot},
    i18n::t,
    media::DeviceKind,
    render::{Fit, VideoView},
    theme::motion,
    ui::Context,
};

/// The stage, and its animals for the screen to hold still while it is
/// covered.
pub struct Stage {
    pub widget: gtk::Stack,
    pub portraits: Portraits,
}

/// A stage for `placement`: the lobby's ground, or a card's.
pub fn stage(context: &Context, placement: Placement) -> Stage {
    let model = &context.model;
    let launch = model.launch();
    let portraits = Portraits::new(placement, launch.portrait.as_deref(), launch.portrait_at);
    let view = VideoView::new();
    view.set_fit(Fit::Cover);
    view.set_mirrored(true);
    view.set_hexpand(true);
    view.set_vexpand(true);
    let stack = gtk::Stack::builder()
        .transition_type(gtk::StackTransitionType::Crossfade)
        .transition_duration(motion::SETTLE_MS)
        .hexpand(true)
        .vexpand(true)
        .build();
    stack.add_css_class("tmsb-stage-ground");
    stack.add_named(&portraits, Some("drawing"));
    stack.add_named(&view, Some("camera"));
    stack.set_visible_child_name(if model.start_with_camera() {
        "camera"
    } else {
        "drawing"
    });

    // The preview this stage draws, to take its sink off again.
    let attached: Rc<Cell<Option<u64>>> = Rc::new(Cell::new(None));
    let follow = {
        let (stack, view, media, attached) = (
            stack.downgrade(),
            view.downgrade(),
            context.media.clone(),
            attached.clone(),
        );
        move |model: &AppModel| {
            let (Some(stack), Some(view)) = (stack.upgrade(), view.upgrade()) else {
                return;
            };
            let on = model.start_with_camera();
            stack.set_visible_child_name(if on { "camera" } else { "drawing" });
            if !on {
                if let (Some(id), Some(camera)) = (attached.take(), model.preview_camera()) {
                    camera.remove_preview(id);
                }
                view.clear();
                // Nothing before a call shows it now: it is let go of.
                model.set_preview_camera(None);
                return;
            }
            if attached.get().is_some() {
                return;
            }
            if let Some(camera) = model.preview_camera() {
                attached.set(Some(camera.add_preview(view.sink())));
                return;
            }
            let (weak, view, attached, media) = (
                model.downgrade(),
                view.downgrade(),
                attached.clone(),
                media.clone(),
            );
            glib::spawn_future_local(async move {
                let Some(model) = weak.upgrade() else {
                    return;
                };
                // The one last picked where it is plugged in, the system's
                // first otherwise.
                let present: Vec<String> = media
                    .devices(DeviceKind::Camera)
                    .into_iter()
                    .map(|device| device.id)
                    .collect();
                let choice = model
                    .device_choice(DeviceSlot::Camera)
                    .filter(|id| present.contains(id));
                let opened = media.open_camera(choice).await;
                // Switched off, or gone into a call, meanwhile: this one is
                // not wanted, and is let go of. The first open waits up to
                // five seconds for the portal, and a join pressed in that
                // time (or an autojoin) went into the call without it;
                // kept, it became the preview of a screen nobody could see
                // again, its device held and its light on for the whole
                // call, beside the camera the call opened. Each open is a
                // camera of its own, so closing this one touches no other.
                if !model.start_with_camera() || !model.screen().before_call() {
                    if let Ok(camera) = opened {
                        camera.close();
                    }
                    return;
                }
                // Opened by another stage meanwhile: the lobby's, whose open
                // was still waiting when the pre-join replaced it, finishes
                // first and keeps its camera as the preview. That one is the
                // preview, and this stage draws it; before, this stage drew
                // nothing and the pre-join showed an empty card with the
                // camera on.
                if let Some(preview) = model.preview_camera() {
                    if let Ok(camera) = opened {
                        camera.close();
                    }
                    if let Some(view) = view.upgrade() {
                        if attached.get().is_none() {
                            attached.set(Some(preview.add_preview(view.sink())));
                        }
                    }
                    return;
                }
                match opened {
                    Ok(camera) => {
                        camera.set_blur(model.blur());
                        model.set_preview_camera(Some(camera.clone()));
                        if let Some(view) = view.upgrade() {
                            attached.set(Some(camera.add_preview(view.sink())));
                        }
                    }
                    Err(fault) => {
                        log(&format!(
                            "the camera's preview could not be opened: {fault}"
                        ));
                        model.mention(t("Can't use your camera"));
                    }
                }
            });
        }
    };
    follow(model);
    model.subscribe_while(&stack, move |model, change| {
        if change == app::CHANGED {
            follow(model);
        }
    });
    let weak = model.downgrade();
    stack.connect_destroy(move |_| {
        if let (Some(model), Some(id)) = (weak.upgrade(), attached.take()) {
            if let Some(camera) = model.preview_camera() {
                camera.remove_preview(id);
            }
        }
    });
    Stage {
        widget: stack,
        portraits,
    }
}

/// The stage as a card (the sheet's and the pre-join's): 16 by 9 at the
/// card's width (`height`, worked out by the screen from its fixed width,
/// since GTK does not lay a box out height for width), rounded, with the
/// switches on it.
pub fn card(context: &Context, height: i32) -> (gtk::Overlay, Portraits) {
    let Stage { widget, portraits } = stage(context, Placement::Card);
    let frame = gtk::Box::new(gtk::Orientation::Vertical, 0);
    frame.append(&widget);
    frame.add_css_class("tmsb-stage-card");
    frame.set_overflow(gtk::Overflow::Hidden);
    frame.set_size_request(-1, height);
    let overlay = gtk::Overlay::new();
    overlay.set_child(Some(&frame));
    let switches = super::pieces::device_switches(&context.model, 44);
    switches.set_valign(gtk::Align::End);
    switches.set_margin_bottom(12);
    overlay.add_overlay(&switches);
    (overlay, portraits)
}
