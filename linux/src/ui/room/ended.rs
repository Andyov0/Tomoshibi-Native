//! The screen after a call (the Mac's EndedScreen, App.tsx's endings): how
//! it ended, in the room's name, and the ways on. Drawn dark, as the call
//! was.
//!
//! Back in where that is something to do: after leaving, and after a
//! connection that dropped, "Join again" is one press back into the same
//! room with the devices already chosen. Not into a room the host closed, or
//! one somebody was put out of. A join from here that is refused says why
//! here, under the button, which stays to try again.

use adw::prelude::*;

use crate::{
    app::{AppModel, Change, Ending, Screen},
    i18n::{self, t},
    theme::motion,
    ui::{kit, Context},
};

const ENDED_CSS: &str = r#"
.tmsb-ended-mark {
  background-color: @tmsb_call_surface; border-radius: 999px;
  min-width: 64px; min-height: 64px;
  box-shadow: inset 0 0 0 1px alpha(@tmsb_call_wash, 0.10);
}
.tmsb-ended-mark image { -gtk-icon-size: 26px; color: @tmsb_call_fg; }
.tmsb-ended-mark.quiet image { color: @tmsb_call_muted; }
.tmsb-ended-room { font-size: 13px; color: @tmsb_call_muted; }
.tmsb-ended-problem {
  background-color: alpha(@tmsb_call_danger, 0.14); color: @tmsb_call_fg;
  border-radius: 14px; padding: 10px 14px; font-size: 13px;
}
"#;

/// The ending, and the ways on.
pub fn page(context: &Context) -> gtk::Widget {
    crate::theme::add_module_css("room-ended", ENDED_CSS);
    let page = gtk::Box::new(gtk::Orientation::Vertical, 0);
    page.add_css_class("tmsb-call-ground");
    page.add_css_class("tmsb-room");
    page.append(&kit::bar());
    let column = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(14)
        .valign(gtk::Align::Center)
        .halign(gtk::Align::Center)
        .vexpand(true)
        .width_request(320)
        .build();

    let symbol = gtk::Image::new();
    let mark = gtk::Box::builder()
        .halign(gtk::Align::Center)
        .valign(gtk::Align::Center)
        .build();
    symbol.set_hexpand(true);
    symbol.set_halign(gtk::Align::Center);
    mark.append(&symbol);
    mark.add_css_class("tmsb-ended-mark");
    mark.set_widget_name("ended.mark");

    let sentence = gtk::Label::builder()
        .wrap(true)
        .justify(gtk::Justification::Center)
        .build();
    sentence.add_css_class("tmsb-ended-title");
    sentence.set_widget_name("ended.sentence");
    // Which room it was: the one thing the sentence does not say, and what
    // "Join again" goes back into.
    let room_name = gtk::Label::builder()
        .wrap(true)
        .wrap_mode(gtk::pango::WrapMode::WordChar)
        .justify(gtk::Justification::Center)
        .build();
    room_name.add_css_class("tmsb-ended-room");
    room_name.set_widget_name("ended.room");

    // "Join again", with the app's spinner beside its words while the join
    // is out.
    let again_words = gtk::Label::new(None);
    let spinner = gtk::Spinner::new();
    let spinner_holder = gtk::Revealer::builder()
        .transition_type(gtk::RevealerTransitionType::SlideRight)
        .transition_duration(motion::SETTLE_MS)
        .child(&spinner)
        .build();
    let again_row = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    again_row.set_halign(gtk::Align::Center);
    spinner.set_margin_end(8);
    again_row.append(&spinner_holder);
    again_row.append(&again_words);
    let again = kit::primary_button("");
    again.set_child(Some(&again_row));
    again.set_widget_name("ended.again");
    let again_holder = coming(&again);

    let back = gtk::Button::new();
    back.set_widget_name("ended.back");

    // Why a join from here did not go in, until it is tried again.
    let problem = gtk::Label::builder()
        .wrap(true)
        .justify(gtk::Justification::Center)
        .max_width_chars(40)
        .build();
    problem.add_css_class("tmsb-ended-problem");
    problem.set_widget_name("ended.problem");
    let problem_holder = coming(&problem);

    let buttons = gtk::Box::new(gtk::Orientation::Vertical, 10);
    for button in [&again, &back] {
        button.set_width_request(220);
        button.set_halign(gtk::Align::Center);
    }
    buttons.append(&again_holder);
    buttons.append(&back);

    column.append(&mark);
    column.append(&sentence);
    column.append(&room_name);
    column.append(&buttons);
    column.append(&problem_holder);
    page.append(&kit::Arrival::new(&column).rises(0));

    let model = context.model.downgrade();
    again.connect_clicked(move |_| {
        if let Some(model) = model.upgrade() {
            model.join_again();
        }
    });
    let model = context.model.downgrade();
    back.connect_clicked(move |_| {
        if let Some(model) = model.upgrade() {
            model.back_to_lobby();
        }
    });

    // Worded here and again on a change of language: the ended screen is a
    // call screen, which the window does not build again (ui/window.rs).
    let word = {
        let (mark, symbol, sentence, room_name, again, again_words, spinner, spinner_holder) = (
            mark.downgrade(),
            symbol.downgrade(),
            sentence.downgrade(),
            room_name.downgrade(),
            again.downgrade(),
            again_words.downgrade(),
            spinner.downgrade(),
            spinner_holder.downgrade(),
        );
        let (back, problem, model) = (
            back.downgrade(),
            problem.downgrade(),
            context.model.downgrade(),
        );
        move || {
            let (Some(mark), Some(symbol), Some(sentence), Some(room_name), Some(again)) = (
                mark.upgrade(),
                symbol.upgrade(),
                sentence.upgrade(),
                room_name.upgrade(),
                again.upgrade(),
            ) else {
                return;
            };
            let (
                Some(again_words),
                Some(spinner),
                Some(spinner_holder),
                Some(back),
                Some(problem),
                Some(model),
            ) = (
                again_words.upgrade(),
                spinner.upgrade(),
                spinner_holder.upgrade(),
                back.upgrade(),
                problem.upgrade(),
                model.upgrade(),
            )
            else {
                return;
            };
            let ending = model.ending();
            let (said, icon) = words(ending.as_ref());
            sentence.set_label(&said);
            symbol.set_icon_name(Some(icon));
            set_class(&mark, "quiet", matches!(ending, Some(Ending::Left) | None));
            let room = model.room();
            room_name.set_label(&room);
            room_name.set_visible(!room.is_empty());

            // Back in only where that is something to do.
            let offered = offers_again(ending.as_ref()) && !room.is_empty();
            come_or_go(&again, offered);
            let joining = model.joining() || model.busy();
            again.set_sensitive(!joining);
            again_words.set_label(&t("Join again"));
            spinner.set_spinning(joining);
            spinner_holder.set_reveal_child(joining);
            set_label(&again, &t("Join again"));

            back.set_label(&t("Back to meetings"));
            // The way on is the primary where there is no way back in.
            set_class(&back, "tmsb-primary", !offered);
            set_class(&back, "tmsb-plain", offered);
            back.set_height_request(if offered {
                crate::theme::scale::CONTROL_LARGE
            } else {
                crate::theme::scale::CONTROL_PRIMARY
            });

            let said_problem = model.problem();
            if let Some(said) = said_problem.as_ref() {
                problem.set_label(said);
            }
            come_or_go(&problem, said_problem.is_some());
        }
    };
    word();
    let reword = word.clone();
    i18n::on_change_while(&page, move |_| reword());
    context
        .model
        .subscribe_while(&page, move |model: &AppModel, change| {
            let ours =
                matches!(change, Change::Busy | Change::Problem) || change == crate::app::CHANGED;
            if ours && model.screen() == Screen::Ended {
                word();
            }
        });
    page.upcast()
}

/// App.tsx's three endings, and the one somebody chose, with the symbol for
/// each (the Mac's).
fn words(ending: Option<&Ending>) -> (String, &'static str) {
    match ending {
        Some(Ending::Closed) => (
            t("The host closed this room."),
            "system-lock-screen-symbolic",
        ),
        Some(Ending::Removed) => (
            t("You were removed from this room."),
            "action-unavailable-symbolic",
        ),
        Some(Ending::Lost(_)) => (
            t("The connection to this room was lost."),
            "network-offline-symbolic",
        ),
        Some(Ending::Left) | None => (t("You left the call."), "call-stop-symbolic"),
    }
}

/// Whether "Join again" is offered: after leaving and after a lost
/// connection, not into a room that was closed or that one was put out of.
fn offers_again(ending: Option<&Ending>) -> bool {
    matches!(ending, Some(Ending::Left) | Some(Ending::Lost(_)))
}

fn set_class(widget: &impl IsA<gtk::Widget>, name: &str, on: bool) {
    if on {
        widget.as_ref().add_css_class(name);
    } else {
        widget.as_ref().remove_css_class(name);
    }
}

/// What a reader says for a button whose words are inside a box of its own.
fn set_label(widget: &impl IsA<gtk::Widget>, label: &str) {
    widget
        .as_ref()
        .update_property(&[gtk::accessible::Property::Label(label)]);
}

/// Wrapped so that it slides in and out rather than appearing.
fn coming(widget: &impl IsA<gtk::Widget>) -> gtk::Revealer {
    let revealer = gtk::Revealer::builder()
        .transition_type(gtk::RevealerTransitionType::SlideDown)
        .transition_duration(motion::SETTLE_MS)
        .reveal_child(false)
        .visible(false)
        .child(widget)
        .build();
    revealer.connect_child_revealed_notify(|revealer| {
        if !revealer.reveals_child() && !revealer.is_child_revealed() {
            revealer.set_visible(false);
        }
    });
    revealer
}

/// Brings in, or takes away, a widget [`coming`] wrapped.
fn come_or_go(widget: &impl IsA<gtk::Widget>, on: bool) {
    let Some(revealer) = widget
        .as_ref()
        .parent()
        .and_then(|parent| parent.downcast::<gtk::Revealer>().ok())
    else {
        widget.as_ref().set_visible(on);
        return;
    };
    if revealer.reveals_child() == on && revealer.is_visible() == on {
        return;
    }
    if on {
        revealer.set_visible(true);
    }
    revealer.set_reveal_child(on);
    if !on && !adw::is_animations_enabled(&revealer) {
        revealer.set_visible(false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn back_in_only_after_leaving_or_losing_the_connection() {
        assert!(offers_again(Some(&Ending::Left)));
        assert!(offers_again(Some(&Ending::Lost("SignalClose".into()))));
        assert!(!offers_again(Some(&Ending::Closed)));
        assert!(!offers_again(Some(&Ending::Removed)));
        assert!(!offers_again(None));
    }

    #[test]
    fn each_ending_has_its_own_sentence() {
        let said: Vec<String> = [
            Some(Ending::Left),
            Some(Ending::Closed),
            Some(Ending::Removed),
            Some(Ending::Lost(String::new())),
        ]
        .iter()
        .map(|ending| words(ending.as_ref()).0)
        .collect();
        let mut distinct = said.clone();
        distinct.sort();
        distinct.dedup();
        assert_eq!(distinct.len(), 4, "{said:?}");
    }
}
