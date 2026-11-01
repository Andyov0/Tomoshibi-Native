//! Said to the person sharing when their line cannot carry the share
//! (ShareStrain.tsx; the Mac's StrainCard): what the line sends, what the
//! share needs, and a setting that fits, to switch to or not. It changes
//! nothing by itself: the owner banned lowering a share on its own.
//!
//! The core watches the share and says what the card shows (`talk.strain`,
//! from `CallEvent::Strain`); "Switch" asks the core whether the card is
//! still about the share running (`strain_accept`), applies the setting
//! through the same change the share menu uses, and says how it went
//! (`strain_accepted`); "Not now" puts it away for this share
//! (`strain_decline`). Above the corner's notices, in their column, as the
//! Mac stacks them; it rises in and sinks out.

use adw::prelude::*;
use gtk::glib;
use tomoshibi_protocol::{share::ShareQuality, strain::strain_megabits};

use super::{
    motion::{Away, Motion},
    RoomState, CHANGED,
};
use crate::{
    app::AppModel,
    i18n::{t, t_with},
};

fn size_label(quality: ShareQuality) -> String {
    match quality {
        ShareQuality::Auto => t("Automatic"),
        ShareQuality::P1080 => "1080p".into(),
        ShareQuality::P1440 => "1440p".into(),
        ShareQuality::P4k => "4K".into(),
    }
}

/// Puts the card on the call's screen, above the notices, and keeps it up to
/// date for as long as `root` is there.
pub(super) fn attach(root: &gtk::Overlay, model: &AppModel) {
    crate::theme::add_module_css("room-media", super::devices_panel::MEDIA_CSS);
    let card = gtk::Box::new(gtk::Orientation::Vertical, 12);
    card.add_css_class("tmsb-strain");
    card.set_widget_name("room.strain");
    card.set_width_request(320);
    let top = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    let gauge = gtk::Image::from_icon_name("network-cellular-signal-weak-symbolic");
    gauge.add_css_class("gauge");
    gauge.set_valign(gtk::Align::Start);
    let words = gtk::Box::new(gtk::Orientation::Vertical, 4);
    let said = gtk::Label::builder()
        .xalign(0.0)
        .wrap(true)
        .max_width_chars(36)
        .build();
    said.add_css_class("said");
    let ask = gtk::Label::builder()
        .xalign(0.0)
        .wrap(true)
        .max_width_chars(36)
        .build();
    ask.add_css_class("ask");
    words.append(&said);
    words.append(&ask);
    top.append(&gauge);
    top.append(&words);
    let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    buttons.set_halign(gtk::Align::End);
    let not_now = gtk::Button::with_label(&t("Not now"));
    not_now.add_css_class("tmsb-panel-button");
    not_now.set_widget_name("strain.not-now");
    let switch_words = gtk::Label::new(Some(&t("Switch")));
    let spinner = gtk::Spinner::new();
    let spinning = gtk::Revealer::builder()
        .transition_type(gtk::RevealerTransitionType::SlideRight)
        .transition_duration(crate::theme::motion::SETTLE_MS)
        .child(&spinner)
        .build();
    let switch_inner = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    switch_inner.append(&spinning);
    switch_inner.append(&switch_words);
    let switch = gtk::Button::builder().child(&switch_inner).build();
    switch.add_css_class("tmsb-panel-button");
    switch.add_css_class("primary");
    switch.set_widget_name("strain.switch");
    buttons.append(&not_now);
    buttons.append(&switch);
    card.append(&top);
    card.append(&buttons);
    let motion = Motion::hidden(&card, Away::FOOT);
    motion.set_halign(gtk::Align::End);

    let weak = model.downgrade();
    not_now.connect_clicked(move |_| {
        if let Some(model) = weak.upgrade() {
            super::strain_not_now(&model);
        }
    });
    let weak = model.downgrade();
    switch.connect_clicked(move |_| {
        if let Some(model) = weak.upgrade() {
            super::strain_take(&model);
        }
    });

    // In the column of notices, first, so that it sits above them; the
    // column is the page's, found by its name once it is in the room.
    place(root, &motion);

    // The card itself only weakly: the model's subscription and the
    // language's listener below hold this, and both are let go when the
    // card is destroyed. Held strongly, the card could never be destroyed,
    // so neither was ever let go, and every call left a card, a
    // subscription and a listener behind for good, each run again on every
    // change of the model and of the language for the rest of the run.
    let refresh = {
        let (motion, said, ask) = (motion.downgrade(), said.clone(), ask.clone());
        let (switch, not_now, switch_words, spinner, spinning) = (
            switch.clone(),
            not_now.clone(),
            switch_words.clone(),
            spinner.clone(),
            spinning.clone(),
        );
        move |model: &AppModel| {
            let Some(motion) = motion.upgrade() else {
                return;
            };
            let room = model.kept::<RoomState>();
            let sharing = room.local.share.borrow().is_some();
            let hint = room.talk.borrow().as_ref().and_then(|talk| talk.strain);
            let busy = model.kept::<super::Shares>().strain_busy.get();
            if let Some(hint) = hint.filter(|_| sharing) {
                said.set_label(&t_with(
                    "Your connection is sending about {have} Mbps, and this share needs about {need} Mbps, so it stutters.",
                    &[
                        ("have", &strain_megabits(hint.have)),
                        ("need", &strain_megabits(hint.need)),
                    ],
                ));
                ask.set_label(&t_with(
                    "Switch to {size} at {rate} frames a second?",
                    &[
                        ("size", &size_label(hint.to.quality)),
                        ("rate", &hint.to.frame_rate),
                    ],
                ));
            }
            not_now.set_label(&t("Not now"));
            switch_words.set_label(&t("Switch"));
            switch.set_sensitive(!busy);
            not_now.set_sensitive(!busy);
            spinning.set_reveal_child(busy);
            spinner.set_spinning(busy);
            motion.show(hint.is_some() && sharing);
        }
    };
    let refresh = std::rc::Rc::new(refresh);
    refresh(model);
    {
        let refresh = refresh.clone();
        model.subscribe_while(&motion, move |model, change| {
            if change == CHANGED {
                refresh(model);
            }
        });
    }
    // The words again in another language, for as long as the card lives.
    let weak = model.downgrade();
    crate::i18n::on_change_while(&motion, move |_| {
        if let Some(model) = weak.upgrade() {
            refresh(&model);
        }
    });
}

/// Puts the card first in the notices' column, looking again once the page
/// has added it if it is not there yet.
fn place(root: &gtk::Overlay, motion: &Motion) {
    if let Some(column) = notices(root) {
        column.prepend(motion);
        return;
    }
    let (root, motion) = (root.downgrade(), motion.clone());
    glib::idle_add_local_once(move || {
        if let Some(root) = root.upgrade() {
            match notices(&root) {
                Some(column) => column.prepend(&motion),
                // No column: on its own at the foot's right, above the bar.
                None => {
                    motion.set_valign(gtk::Align::End);
                    motion.set_margin_end(16);
                    motion.set_margin_bottom(super::plan::ISLAND as i32 + 8);
                    root.add_overlay(&motion);
                }
            }
        }
    });
}

fn notices(root: &gtk::Overlay) -> Option<gtk::Box> {
    let mut child = root.first_child();
    while let Some(widget) = child {
        if widget.widget_name() == "room.toasts" {
            return widget.downcast::<gtk::Box>().ok();
        }
        child = widget.next_sibling();
    }
    None
}
