//! The share button's popover (ShareButton.tsx; the Mac's ScreenShareControl
//! and SoundShareControl, here in one menu as the web has them): the size
//! and the frame rate first, because they are what somebody came to change,
//! then the press that starts or stops the share; and sound shared on its
//! own, with the lossless switch beneath it.
//!
//! The size and rate are remembered under the web's names (local.rs
//! `choose_share`) and applied at once to a share that is running; while one
//! runs the menu shows what it is sending, which the uplink card can change
//! without the menu. Built once with the popover and brought up to date by
//! the returned refresh; what comes and goes slides.

use std::{cell::Cell, rc::Rc};

use adw::prelude::*;
use tomoshibi_protocol::share::{share_rates_for, ShareQuality, SHARE_FRAME_RATES};

use super::{page::menu_row, RoomState};
use crate::{app::AppModel, i18n::t};

/// What a size promises (ShareButton.tsx `QUALITY_LABELS`): automatic is the
/// only setting that will quietly send less.
fn describes(quality: ShareQuality) -> String {
    match quality {
        ShareQuality::Auto => t("Adjusts to the connection"),
        ShareQuality::P1080 => t("Up to 240 frames a second"),
        ShareQuality::P1440 => t("Up to 120 frames a second"),
        ShareQuality::P4k => t("Up to 60 frames a second"),
    }
}

fn quality_label(quality: ShareQuality) -> String {
    match quality {
        ShareQuality::Auto => t("Automatic"),
        ShareQuality::P1080 => "1080p".into(),
        ShareQuality::P1440 => "1440p".into(),
        ShareQuality::P4k => "4K".into(),
    }
}

fn slide(child: &impl IsA<gtk::Widget>, kind: gtk::RevealerTransitionType) -> gtk::Revealer {
    gtk::Revealer::builder()
        .transition_type(kind)
        .transition_duration(crate::theme::motion::SETTLE_MS)
        .child(child)
        .build()
}

fn class(widget: &impl IsA<gtk::Widget>, name: &str, on: bool) {
    if on {
        widget.as_ref().add_css_class(name);
    } else {
        widget.as_ref().remove_css_class(name);
    }
}

fn set_text(label: &gtk::Label, text: &str) {
    if label.label() != text {
        label.set_label(text);
    }
}

pub(super) fn build(model: &AppModel, card: &gtk::Box) -> Box<dyn Fn(&AppModel)> {
    crate::theme::add_module_css("room-media", super::devices_panel::MEDIA_CSS);
    card.set_widget_name("pop.share");
    card.set_width_request(320);

    let picture = gtk::Label::builder().xalign(0.0).build();
    picture.add_css_class("tmsb-menu-heading");
    card.append(&picture);
    let sizes = gtk::Box::new(gtk::Orientation::Horizontal, 2);
    sizes.set_homogeneous(true);
    let mut size_buttons = Vec::new();
    for quality in [
        ShareQuality::Auto,
        ShareQuality::P1080,
        ShareQuality::P1440,
        ShareQuality::P4k,
    ] {
        let button = gtk::Button::with_label(&quality_label(quality));
        button.add_css_class("tmsb-choice");
        button.set_widget_name(&format!(
            "share.quality:{}",
            tomoshibi_protocol::share::share_quality_key(quality)
        ));
        let weak = model.downgrade();
        button.connect_clicked(move |_| {
            if let Some(model) = weak.upgrade() {
                super::choose_share(&model, quality, None);
            }
        });
        sizes.append(&button);
        size_buttons.push((quality, button));
    }
    card.append(&sizes);
    let describe = gtk::Label::builder().xalign(0.0).wrap(true).build();
    describe.add_css_class("tmsb-menu-describe");
    card.append(&describe);

    // The frame rate, absent for automatic, which chooses the rate as well:
    // a control that did nothing would be one somebody set and believed.
    let rates_box = gtk::Box::new(gtk::Orientation::Vertical, 2);
    let rates_heading = gtk::Label::builder().xalign(0.0).build();
    rates_heading.add_css_class("tmsb-menu-heading");
    rates_box.append(&rates_heading);
    let chips = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    chips.set_margin_start(10);
    chips.set_margin_end(10);
    chips.set_margin_bottom(4);
    let mut rate_chips = Vec::new();
    for rate in SHARE_FRAME_RATES {
        let chip = gtk::Button::with_label(&rate.to_string());
        chip.add_css_class("tmsb-chip");
        chip.set_widget_name(&format!("share.rate:{rate}"));
        let weak = model.downgrade();
        chip.connect_clicked(move |_| {
            if let Some(model) = weak.upgrade() {
                let (quality, _) = super::share_shown(&model);
                super::choose_share(&model, quality, Some(rate));
            }
        });
        let holder = slide(&chip, gtk::RevealerTransitionType::SlideRight);
        chips.append(&holder);
        rate_chips.push((rate, chip, holder));
    }
    rates_box.append(&chips);
    let rates = slide(&rates_box, gtk::RevealerTransitionType::SlideDown);
    card.append(&rates);

    let rule = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    rule.add_css_class("tmsb-menu-rule");
    card.append(&rule);

    // The press that starts or stops the share, last, nearest the button.
    let (go, go_title) = menu_row(
        &t("Share your screen"),
        Some(""),
        Some("screen-shared-symbolic"),
    );
    go.set_widget_name("share.go");
    let go_detail = detail_of(&go);
    let weak = model.downgrade();
    go.connect_clicked(move |_| {
        if let Some(model) = weak.upgrade() {
            super::close_pop(&model);
            let sharing = model.kept::<RoomState>().local.share.borrow().is_some();
            if sharing {
                super::stop_share(&model);
            } else {
                super::start_share(&model);
            }
        }
    });
    card.append(&go);

    // Sound on its own: stopping it while it runs, or starting it and
    // whether it goes losslessly, decided before it starts.
    let (stop_sound, stop_sound_title) = menu_row(
        &t("Stop sharing sound"),
        Some(""),
        Some("media-playback-stop-symbolic"),
    );
    stop_sound.set_widget_name("share.sound.stop");
    let stop_sound_detail = detail_of(&stop_sound);
    let weak = model.downgrade();
    stop_sound.connect_clicked(move |_| {
        if let Some(model) = weak.upgrade() {
            super::close_pop(&model);
            super::stop_listening(&model);
        }
    });
    let stop_sound_slide = slide(&stop_sound, gtk::RevealerTransitionType::SlideDown);
    card.append(&stop_sound_slide);

    let start_box = gtk::Box::new(gtk::Orientation::Vertical, 2);
    let (listen, listen_title) = menu_row(
        &t("Share only sound"),
        Some(""),
        Some("audio-x-generic-symbolic"),
    );
    listen.set_widget_name("share.sound");
    let listen_detail = detail_of(&listen);
    let weak = model.downgrade();
    listen.connect_clicked(move |_| {
        if let Some(model) = weak.upgrade() {
            super::close_pop(&model);
            super::start_listening(&model);
        }
    });
    start_box.append(&listen);
    let lossless_row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    lossless_row.add_css_class("tmsb-setting");
    lossless_row.set_margin_start(10);
    lossless_row.set_margin_end(6);
    let lossless_words = gtk::Box::new(gtk::Orientation::Vertical, 1);
    lossless_words.set_hexpand(true);
    lossless_words.set_valign(gtk::Align::Center);
    let lossless_title = gtk::Label::builder().xalign(0.0).build();
    lossless_title.add_css_class("title");
    let lossless_detail = gtk::Label::builder()
        .xalign(0.0)
        .wrap(true)
        .max_width_chars(34)
        .build();
    lossless_detail.add_css_class("detail");
    lossless_words.append(&lossless_title);
    lossless_words.append(&lossless_detail);
    let lossless = gtk::Switch::builder().valign(gtk::Align::Center).build();
    lossless.set_widget_name("share.lossless");
    lossless_row.append(&lossless_words);
    lossless_row.append(&lossless);
    start_box.append(&lossless_row);
    let start_slide = slide(&start_box, gtk::RevealerTransitionType::SlideDown);
    card.append(&start_slide);
    let quiet = Rc::new(Cell::new(false));
    {
        let (weak, quiet) = (model.downgrade(), quiet.clone());
        lossless.connect_active_notify(move |switch| {
            if quiet.get() {
                return;
            }
            if let Some(model) = weak.upgrade() {
                super::set_lossless(&model, switch.is_active());
            }
        });
    }

    let (go, listen, stop_sound) = (go.downgrade(), listen.downgrade(), stop_sound.downgrade());
    Box::new(move |model: &AppModel| {
        let room = model.kept::<RoomState>();
        let shares = model.kept::<super::Shares>();
        let sharing = room.local.share.borrow().is_some();
        let listening = shares.listening.borrow().clone();
        let (quality, fps) = super::share_shown(model);

        set_text(&picture, &t("Picture"));
        for (each, button) in &size_buttons {
            class(button, "held", *each == quality);
            let label = quality_label(*each);
            if button.label().as_deref() != Some(label.as_str()) {
                button.set_label(&label);
            }
            button.update_state(&[gtk::accessible::State::Checked(if *each == quality {
                gtk::AccessibleTristate::True
            } else {
                gtk::AccessibleTristate::False
            })]);
        }
        set_text(&describe, &describes(quality));
        set_text(&rates_heading, &t("Frames a second"));
        let allowed = share_rates_for(quality);
        for (rate, chip, holder) in &rate_chips {
            holder.set_reveal_child(allowed.contains(rate));
            class(chip, "held", *rate == fps);
        }
        rates.set_reveal_child(quality != ShareQuality::Auto);

        set_text(
            &go_title,
            &if sharing {
                t("Stop sharing")
            } else {
                t("Share your screen")
            },
        );
        // The screen without its own sound while sound is shared on its own.
        let quiet_screen = listening.is_some() && !sharing;
        set_text(
            &go_detail,
            &if quiet_screen {
                t("Without its sound, while sound is shared on its own")
            } else {
                String::new()
            },
        );
        go_detail.set_visible(quiet_screen);
        if let Some(go) = go.upgrade() {
            go.set_sensitive(!room.local.sharing_busy.get());
        }

        stop_sound_slide.set_reveal_child(listening.is_some());
        if let Some(listening) = listening.as_ref() {
            set_text(&stop_sound_title, &t("Stop sharing sound"));
            set_text(
                &stop_sound_detail,
                &if listening.sends_lossless() {
                    t("Lossless")
                } else {
                    t("Real time")
                },
            );
        }
        stop_sound_detail.set_visible(true);
        // Not offered while the screen is shared, as the web has it: a share
        // already sends its sound.
        start_slide.set_reveal_child(listening.is_none() && !sharing);
        set_text(&listen_title, &t("Share only sound"));
        set_text(
            &listen_detail,
            &t("Everything this computer plays, except the call"),
        );
        if let Some(listen) = listen.upgrade() {
            listen.set_sensitive(!shares.listening_busy.get());
        }
        if let Some(stop_sound) = stop_sound.upgrade() {
            stop_sound.set_sensitive(!shares.listening_busy.get());
        }
        // Off where it cannot work, with the reason in place of what it
        // does, so nobody turns it on and gets real time without being told.
        let sealed = super::sealed(model);
        set_text(&lossless_title, &t("Lossless"));
        set_text(
            &lossless_detail,
            &if sealed {
                t("Not available in an encrypted call")
            } else {
                t("Every sample as it is played, about a second behind")
            },
        );
        quiet.set(true);
        let on = super::lossless_wanted(model) && !sealed;
        if lossless.is_active() != on {
            lossless.set_active(on);
        }
        quiet.set(false);
        lossless.set_sensitive(!sealed);
        lossless_row.set_opacity(if sealed { 0.7 } else { 1.0 });
    })
}

/// The detail label `menu_row` made under a row's title.
fn detail_of(row: &gtk::Button) -> gtk::Label {
    let mut found = None;
    let mut stack = vec![row.clone().upcast::<gtk::Widget>()];
    while let Some(widget) = stack.pop() {
        if widget.has_css_class("detail") {
            if let Ok(label) = widget.clone().downcast::<gtk::Label>() {
                found = Some(label);
                break;
            }
        }
        let mut child = widget.first_child();
        while let Some(next) = child {
            stack.push(next.clone());
            child = next.next_sibling();
        }
    }
    found.unwrap_or_default()
}
