//! A meeting being moved to another machine, from a chair (Moving.tsx, the
//! Mac's MovingOverlay).
//!
//! A move ends the call and everybody comes straight back; what that looked
//! like was being thrown out ("The host closed this room." and the lobby,
//! on Linux, which never came back at all). So this is drawn over the whole
//! call, its controls included, for the few seconds it takes, says where
//! the meeting is going, and offers nothing to press: presses pass through
//! it, and there is nothing under it to press while the call is away. The
//! destination is what the server announced, put through `say()` as the
//! web puts it.
//!
//! Attached to the call's screen once (page.rs) and brought in and out by
//! the room's `moving`, with the call's own arrival and leaving.

use adw::prelude::*;

use super::{
    motion::{Away, Motion},
    RoomState, CHANGED,
};
use crate::{
    app::AppModel,
    i18n::{self, say, t, t_with},
};

const VEIL_CSS: &str = r#"
.tmsb-veil { background-color: alpha(@tmsb_call_bg, 0.86); }
.tmsb-veil .title { font-size: 14px; font-weight: 500; color: @tmsb_call_fg; }
.tmsb-veil .detail { font-size: 12px; color: @tmsb_call_muted; }
.tmsb-veil spinner { color: @tmsb_call_muted; }
"#;

/// Lays the veil over `root`, the call's screen, and keeps it following the
/// room's `moving`.
pub fn attach(root: &gtk::Overlay, model: &AppModel) {
    crate::theme::add_module_css("room-veil", VEIL_CSS);
    let spinner = gtk::Spinner::builder()
        .width_request(18)
        .height_request(18)
        .build();
    let title = gtk::Label::builder()
        .wrap(true)
        .justify(gtk::Justification::Center)
        .build();
    title.add_css_class("title");
    // The one thing worth saying: nobody has to do anything.
    let detail = gtk::Label::builder()
        .wrap(true)
        .justify(gtk::Justification::Center)
        .build();
    detail.add_css_class("detail");
    let words = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(12)
        .halign(gtk::Align::Center)
        .valign(gtk::Align::Center)
        .margin_start(24)
        .margin_end(24)
        .hexpand(true)
        .vexpand(true)
        .build();
    words.append(&spinner);
    words.append(&title);
    words.append(&detail);
    let veil = gtk::Box::builder()
        .accessible_role(gtk::AccessibleRole::Status)
        .hexpand(true)
        .vexpand(true)
        .build();
    veil.add_css_class("tmsb-veil");
    veil.append(&words);
    let motion = Motion::hidden(&veil, Away::GROW);
    motion.set_widget_name("room.veil");
    motion.set_can_target(false);
    root.add_overlay(&motion);
    root.set_measure_overlay(&motion, false);

    let follow = {
        let (root, motion, spinner, title, detail) = (
            root.downgrade(),
            motion.downgrade(),
            spinner.downgrade(),
            title.downgrade(),
            detail.downgrade(),
        );
        move |model: &AppModel| {
            let (Some(root), Some(motion), Some(spinner), Some(title), Some(detail)) = (
                root.upgrade(),
                motion.upgrade(),
                spinner.upgrade(),
                title.upgrade(),
                detail.upgrade(),
            ) else {
                return;
            };
            let to = model
                .kept::<RoomState>()
                .moving
                .borrow()
                .as_ref()
                .map(|(to, _)| to.clone());
            let showing = to.is_some();
            if let Some(to) = to {
                // A move with no destination is still a move, and saying
                // so without a name is better than saying nothing.
                let named = say(to.trim());
                title.set_label(&if named.is_empty() {
                    t("Moving this meeting")
                } else {
                    t_with("Moving this meeting to {name}", &[("name", &named)])
                });
                detail.set_label(&t("You will be back in a moment."));
                if !motion.is_shown() {
                    // Over everything on the call's screen, whatever was
                    // laid over it after the veil was: the controls, the
                    // panels, the popovers.
                    motion.insert_before(&root, None::<&gtk::Widget>);
                }
            }
            spinner.set_spinning(showing);
            motion.show(showing);
            // Presses go through it, as the web's pointer-events-none and the
            // Mac's allowsHitTesting(false) let them: it is not a question.
            motion.set_can_target(false);
        }
    };
    follow(model);
    let on_room = follow.clone();
    model.subscribe_while(root, move |model, change| {
        if change == CHANGED {
            on_room(model);
        }
    });
    let weak = model.downgrade();
    i18n::on_change_while(root, move |_| {
        if let Some(model) = weak.upgrade() {
            follow(&model);
        }
    });
}
