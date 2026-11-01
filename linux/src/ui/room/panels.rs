//! The panels beside the call: messages (ChatPanel.tsx), sound
//! (SoundPanel.tsx, each person's volume; sound_panel.rs), devices
//! (DeviceMenu.tsx) and running the room (HostPanel.tsx, the door among it;
//! host_panel.rs). Opaque, with their close button 32 drawn at the top right
//! (UI sync); page.rs slides them in from the side, or up from the foot
//! where they cover the call.
//!
//! Each is built once when it opens and brought up to date by its
//! `refresh`, which reads the room's state like everything else on the
//! call's screen and changes only what changed: a row made again under the
//! pointer loses the press, and one made again on every event of the call
//! appears from nowhere each time.

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use adw::prelude::*;
use gtk::glib;
use tomoshibi_core::call::CallState;

use super::{
    glyph::{self, Glyph},
    motion::{Away, Motion},
    talk::{named, Line},
    Panel, RoomState,
};
use crate::{app::AppModel, i18n::t, media::DeviceKind};

/// A panel and how it is brought up to date.
pub struct PanelView {
    pub widget: gtk::Widget,
    pub refresh: Box<dyn Fn(&AppModel)>,
}

pub fn build(model: &AppModel, panel: Panel) -> PanelView {
    match panel {
        Panel::Messages => messages(model),
        Panel::Sound => super::sound_panel::build(model),
        Panel::Devices => devices(model),
        Panel::Host => super::host_panel::build(model),
    }
}

/// The heading, the close button, and the body beneath, scrolled.
pub(super) fn frame(
    model: &AppModel,
    panel: Panel,
    title: &str,
    close_label: &str,
) -> (gtk::Box, gtk::Box) {
    let outer = gtk::Box::new(gtk::Orientation::Vertical, 0);
    outer.set_vexpand(true);
    let head = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    head.set_margin_start(16);
    head.set_margin_end(8);
    head.set_margin_top(8);
    head.set_margin_bottom(4);
    let heading = gtk::Label::builder()
        .label(title)
        .xalign(0.0)
        .hexpand(true)
        .build();
    heading.add_css_class("tmsb-panel-title");
    let close = gtk::Button::from_icon_name("window-close-symbolic");
    close.add_css_class("tmsb-close");
    close.set_tooltip_text(Some(close_label));
    close.update_property(&[gtk::accessible::Property::Label(close_label)]);
    close.set_widget_name("panel.close");
    let weak = model.downgrade();
    close.connect_clicked(move |_| {
        if let Some(model) = weak.upgrade() {
            super::toggle_panel(&model, panel);
        }
    });
    head.append(&heading);
    head.append(&close);
    outer.append(&head);
    let body = gtk::Box::new(gtk::Orientation::Vertical, 8);
    body.set_margin_start(16);
    body.set_margin_end(16);
    body.set_margin_bottom(16);
    let scroller = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vexpand(true)
        .propagate_natural_height(true)
        .child(&body)
        .build();
    outer.append(&scroller);
    outer.set_widget_name(match panel {
        Panel::Messages => "panel.messages",
        Panel::Sound => "panel.sound",
        Panel::Devices => "panel.devices",
        Panel::Host => "panel.host",
    });
    (outer, body)
}

pub(super) fn muted(text: &str) -> gtk::Label {
    let label = gtk::Label::builder()
        .label(text)
        .xalign(0.0)
        .wrap(true)
        .build();
    label.add_css_class("tmsb-muted");
    label
}

pub(super) fn section(text: &str) -> gtk::Label {
    let label = gtk::Label::builder()
        .label(text)
        .xalign(0.0)
        .margin_top(6)
        .build();
    label.add_css_class("tmsb-panel-section");
    label
}

pub(super) fn clear(container: &gtk::Box) {
    while let Some(child) = container.first_child() {
        container.remove(&child);
    }
}

pub(super) fn pill(label: &str) -> gtk::Button {
    let button = gtk::Button::with_label(label);
    button.add_css_class("tmsb-panel-button");
    button
}

// ---- messages ------------------------------------------------------------

/// A link in a message, in the colour the web draws one (Linked.tsx:
/// text-tally-ink, underlined).
const MESSAGES_CSS: &str = r#"
.tmsb-line-text link { color: @tmsb_call_tally; text-decoration-line: underline; }
.tmsb-line-text link:hover { color: mix(@tmsb_call_tally, @tmsb_call_fg, 0.2); }
"#;

fn messages(model: &AppModel) -> PanelView {
    crate::theme::add_module_css("room-messages", MESSAGES_CSS);
    let (outer, body) = frame(model, Panel::Messages, &t("Messages"), &t("Close messages"));
    let lines = gtk::Box::new(gtk::Orientation::Vertical, 10);
    lines.set_widget_name("messages.lines");
    let empty = muted(&t("Messages disappear when the call ends."));
    empty.set_margin_top(6);
    body.append(&empty);
    body.append(&lines);

    // The box: the draft kept with the call, Enter to send.
    let entry = gtk::Entry::builder()
        .placeholder_text(t("Say something"))
        .hexpand(true)
        .has_frame(false)
        .build();
    entry.set_widget_name("messages.say");
    entry.update_property(&[gtk::accessible::Property::Label(&t("Say something"))]);
    let send = gtk::Button::new();
    send.set_child(Some(&Glyph::new(glyph::ARROW_UP, 14)));
    send.add_css_class("tmsb-send");
    send.set_valign(gtk::Align::Center);
    send.set_tooltip_text(Some(&t("Send")));
    send.update_property(&[gtk::accessible::Property::Label(&t("Send"))]);
    send.set_widget_name("messages.send");
    let say = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    say.add_css_class("tmsb-say");
    say.set_margin_start(12);
    say.set_margin_end(12);
    say.set_margin_bottom(12);
    say.append(&entry);
    say.append(&send);
    outer.append(&say);

    let quiet = Rc::new(Cell::new(false));
    let weak = model.downgrade();
    let hush = quiet.clone();
    entry.connect_changed(move |entry| {
        if hush.get() {
            return;
        }
        if let Some(model) = weak.upgrade() {
            let room = model.kept::<RoomState>();
            let mut view = room.view.borrow_mut();
            view.draft = entry.text().to_string();
            if !view.draft.is_empty() {
                view.chat_failed = false;
            }
            drop(view);
            super::changed(&model);
        }
    });
    let weak = model.downgrade();
    entry.connect_activate(move |_| {
        if let Some(model) = weak.upgrade() {
            super::send_draft(&model);
        }
    });
    let weak = model.downgrade();
    send.connect_clicked(move |_| {
        if let Some(model) = weak.upgrade() {
            super::send_draft(&model);
        }
    });
    // The box takes the keys as the panel opens.
    let focus = entry.downgrade();
    glib::idle_add_local_once(move || {
        if let Some(entry) = focus.upgrade() {
            entry.grab_focus();
        }
    });

    let shown: Rc<RefCell<Vec<String>>> = Rc::default();
    // What was there when the panel opened arrives with the panel; only
    // what is said while it is open arrives on its own.
    let opened = Rc::new(Cell::new(false));
    let scroller = body.parent().and_then(|viewport| viewport.parent());
    let refresh = {
        let (lines, empty, entry, send, say) = (
            lines.downgrade(),
            empty.downgrade(),
            entry.downgrade(),
            send.downgrade(),
            say.downgrade(),
        );
        let scroller = scroller.map(|scroller| scroller.downgrade());
        move |model: &AppModel| {
            let (Some(lines), Some(empty), Some(entry), Some(send), Some(say)) = (
                lines.upgrade(),
                empty.upgrade(),
                entry.upgrade(),
                send.upgrade(),
                say.upgrade(),
            ) else {
                return;
            };
            let room = model.kept::<RoomState>();
            let talk = room.talk.borrow();
            let messages: Vec<Line> = talk
                .as_ref()
                .map(|talk| talk.messages.clone())
                .unwrap_or_default();
            let ids: Vec<String> = messages.iter().map(|line| line.id.clone()).collect();
            let mut was = shown.borrow_mut();
            // Only ever added to, but for the oldest going past the five
            // hundred kept: then it is drawn again.
            let appended = was.len() <= ids.len() && ids[..was.len()] == was[..];
            if !appended {
                clear(&lines);
                was.clear();
            }
            let added = ids.len() > was.len();
            let arriving = opened.replace(true);
            for line in &messages[was.len()..] {
                let widget = line_widget(model, line);
                if arriving {
                    // A new message rises into the list rather than
                    // appearing in it (Theme.arrive).
                    let motion = Motion::hidden(&widget, Away::FOOT);
                    lines.append(&motion);
                    motion.show(true);
                } else {
                    lines.append(&widget);
                }
            }
            *was = ids;
            empty.set_visible(messages.is_empty());
            if added {
                if let Some(scroller) = scroller.as_ref().and_then(|scroller| scroller.upgrade()) {
                    let scroller = scroller.downcast::<gtk::ScrolledWindow>().ok();
                    glib::idle_add_local_once(move || {
                        if let Some(scroller) = scroller {
                            let adjustment = scroller.vadjustment();
                            adjustment.set_value(adjustment.upper());
                        }
                    });
                }
            }
            let view = room.view.borrow();
            if entry.text() != view.draft {
                quiet.set(true);
                entry.set_text(&view.draft);
                entry.set_position(-1);
                quiet.set(false);
            }
            let connected = room.state.get() == Some(CallState::Connected);
            send.set_sensitive(!view.draft.trim().is_empty() && !view.sending && connected);
            if view.chat_failed {
                say.add_css_class("failed");
            } else {
                say.remove_css_class("failed");
            }
        }
    };
    PanelView {
        widget: outer.upcast(),
        refresh: Box::new(refresh),
    }
}

fn line_widget(model: &AppModel, line: &Line) -> gtk::Widget {
    let row = gtk::Box::new(gtk::Orientation::Vertical, 2);
    if line.mine {
        row.add_css_class("tmsb-line-mine");
    }
    let name = gtk::Label::builder()
        .label(named(&line.name, &line.identity))
        .xalign(0.0)
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .build();
    name.add_css_class("tmsb-line-name");
    let bubble = gtk::Box::new(gtk::Orientation::Vertical, 0);
    bubble.add_css_class("tmsb-line-bubble");
    bubble.set_halign(gtk::Align::Start);
    let text = gtk::Label::builder()
        .xalign(0.0)
        .wrap(true)
        .wrap_mode(gtk::pango::WrapMode::WordChar)
        .selectable(true)
        .max_width_chars(40)
        .build();
    text.set_markup(&linked(&line.text));
    text.add_css_class("tmsb-line-text");
    let weak = model.downgrade();
    text.connect_activate_link(move |label, uri| {
        if let Some(model) = weak.upgrade() {
            open_link(&model, label, uri);
        }
        glib::Propagation::Stop
    });
    bubble.append(&text);
    row.append(&name);
    row.append(&bubble);
    row.upcast()
}

// ---- devices -------------------------------------------------------------

fn devices(model: &AppModel) -> PanelView {
    let (outer, body) = frame(model, Panel::Devices, &t("Devices"), &t("Close"));
    let lists = gtk::Box::new(gtk::Orientation::Vertical, 4);
    body.append(&lists);
    let refresh = {
        let lists = lists.downgrade();
        let shown: RefCell<Option<String>> = RefCell::new(None);
        move |model: &AppModel| {
            let Some(lists) = lists.upgrade() else { return };
            let Some(media) = super::media_of(model) else {
                return;
            };
            let room = model.kept::<RoomState>();
            let original = super::original_sound(model);
            // Drawn again only when what it would show changed, so that a
            // press is not lost to a row made again under the pointer.
            let mut said = String::new();
            for kind in [
                DeviceKind::Microphone,
                DeviceKind::Camera,
                DeviceKind::Speaker,
            ] {
                for device in media.devices(kind) {
                    said.push_str(&format!(
                        "{kind:?}{}{}{};",
                        device.id, device.label, device.is_default
                    ));
                }
                said.push_str(&format!("{:?}|", super::chosen_device(model, kind)));
            }
            said.push_str(&format!("{original}{}", crate::i18n::language().code()));
            if shown.borrow().as_deref() == Some(said.as_str()) {
                return;
            }
            shown.replace(Some(said));
            clear(&lists);
            for (kind, heading) in [
                (DeviceKind::Microphone, t("Microphone")),
                (DeviceKind::Camera, t("Camera")),
                (DeviceKind::Speaker, t("Speakers")),
            ] {
                lists.append(&section(&heading));
                let found = media.devices(kind);
                if found.is_empty() {
                    lists.append(&muted(&t("No devices found")));
                    continue;
                }
                let chosen = super::chosen_device(model, kind);
                let chosen_present = chosen
                    .as_ref()
                    .is_some_and(|id| found.iter().any(|device| &device.id == id));
                for device in found {
                    let ticked = if chosen_present {
                        chosen.as_deref() == Some(device.id.as_str())
                    } else {
                        device.is_default
                    };
                    let row = choice_row(&device.label, ticked);
                    row.set_widget_name(&format!("device:{}", device.id));
                    let (weak, id) = (model.downgrade(), device.id.clone());
                    row.connect_clicked(move |_| {
                        if let Some(model) = weak.upgrade() {
                            super::choose_device(&model, kind, Some(id.clone()));
                        }
                    });
                    lists.append(&row);
                }
                if kind == DeviceKind::Microphone {
                    // The web's "original sound" (sound.ts): the voice sent
                    // as it is, without noise suppression or levelling.
                    let row = choice_row(&t("Original sound"), original);
                    row.set_tooltip_text(Some(&t(
                        "No noise suppression or automatic volume. For headphones and music.",
                    )));
                    let weak = model.downgrade();
                    row.connect_clicked(move |_| {
                        if let Some(model) = weak.upgrade() {
                            super::set_original_sound(&model, !super::original_sound(&model));
                        }
                    });
                    lists.append(&row);
                }
            }
            let _ = room;
        }
    };
    PanelView {
        widget: outer.upcast(),
        refresh: Box::new(refresh),
    }
}

/// One of a list to pick from: the chosen one ticked in the signal colour.
fn choice_row(title: &str, ticked: bool) -> gtk::Button {
    let line = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let label = gtk::Label::builder()
        .label(title)
        .xalign(0.0)
        .hexpand(true)
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .build();
    let tick = gtk::Image::from_icon_name("object-select-symbolic");
    tick.add_css_class("tick");
    tick.set_opacity(if ticked { 1.0 } else { 0.0 });
    line.append(&label);
    line.append(&tick);
    let button = gtk::Button::builder().child(&line).build();
    button.add_css_class("tmsb-choice");
    if ticked {
        button.update_state(&[gtk::accessible::State::Checked(
            gtk::AccessibleTristate::True,
        )]);
    }
    button
}

// ---- links in messages -----------------------------------------------------

/// The text as Pango markup with its addresses made links (Linked.tsx): only
/// a complete URL over http or https, its trailing punctuation left out,
/// since guessing at bare hosts turns "see chapter 3.e.g" into a link nobody
/// meant, and a false link is worse than a missed one.
pub(super) fn linked(text: &str) -> String {
    let mut out = String::new();
    let mut at = 0;
    for (start, end) in links_in(text) {
        out.push_str(&glib::markup_escape_text(&text[at..start]));
        let address = glib::markup_escape_text(&text[start..end]);
        out.push_str(&format!("<a href=\"{address}\">{address}</a>"));
        at = end;
    }
    out.push_str(&glib::markup_escape_text(&text[at..]));
    out
}

/// Where the links are, as byte ranges: Linked.tsx's
/// `\bhttps?:\/\/[^\s<>()[\]{}]+[^\s<>()[\]{}.,;:!?'"]`, written out.
fn links_in(text: &str) -> Vec<(usize, usize)> {
    let stops = |c: char| c.is_whitespace() || "<>()[]{}".contains(c);
    let trailing = |c: char| ".,;:!?'\"".contains(c);
    let lower = text.to_ascii_lowercase();
    let mut found = Vec::new();
    let mut from = 0;
    while from < text.len() {
        let next = ["http://", "https://"]
            .iter()
            .filter_map(|scheme| {
                lower[from..]
                    .find(scheme)
                    .map(|at| (from + at, scheme.len()))
            })
            .min();
        let Some((start, scheme)) = next else { break };
        // A word's start, as the expression's \b, which is ASCII's: not
        // the middle of one.
        let joined = text[..start]
            .chars()
            .next_back()
            .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_');
        let rest = &text[start + scheme..];
        let run = rest.find(stops).unwrap_or(rest.len());
        let mut end = start + scheme + run;
        while end > start + scheme && text[..end].chars().next_back().is_some_and(trailing) {
            end -= text[..end].chars().next_back().map_or(1, char::len_utf8);
        }
        // At least two characters after the scheme, as the expression
        // wants one and then one that is not punctuation.
        if !joined && text[start + scheme..end].chars().count() >= 2 {
            found.push((start, end));
            from = end;
        } else {
            from = start + scheme;
        }
    }
    found
}

/// Opens a link pressed in a message, in the person's browser, without
/// leaving the call: through the desktop's own way of opening a link
/// (gtk::UriLauncher, the portal where there is one, over the window), or,
/// from an AppImage, through the model's, which starts the browser with the
/// system's environment rather than the image's (AppModel::open_in_browser
/// says what the image's did to a browser).
fn open_link(model: &AppModel, label: &gtk::Label, uri: &str) {
    if std::env::var_os("APPIMAGE").is_some() {
        model.open_in_browser(uri);
        return;
    }
    let window = label
        .root()
        .and_then(|root| root.downcast::<gtk::Window>().ok());
    let (weak, address) = (model.downgrade(), uri.to_owned());
    gtk::UriLauncher::new(uri).launch(window.as_ref(), gtk::gio::Cancellable::NONE, move |done| {
        if let Err(fault) = done {
            crate::app::log(&format!("a link could not be opened: {fault}"));
            if let Some(model) = weak.upgrade() {
                model.open_in_browser(&address);
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn links(text: &str) -> Vec<&str> {
        links_in(text)
            .into_iter()
            .map(|(start, end)| &text[start..end])
            .collect()
    }

    #[test]
    fn only_whole_addresses_are_links_and_their_full_stop_is_not() {
        assert_eq!(
            links("see https://example.com/a?b=1. and http://x.org,"),
            vec!["https://example.com/a?b=1", "http://x.org"]
        );
        assert_eq!(
            links("(https://example.com/page)"),
            vec!["https://example.com/page"]
        );
        assert!(links("example.com, e.g. chapter 3").is_empty());
        assert!(links("xhttps://example.com").is_empty(), "inside a word");
        assert_eq!(
            links("éhttps://e.com"),
            vec!["https://e.com"],
            "\\b is ASCII's"
        );
        assert!(links("https://").is_empty());
        assert_eq!(links("HTTPS://Example.com"), vec!["HTTPS://Example.com"]);
        assert_eq!(
            links("см. https://пример.рф/путь»"),
            vec!["https://пример.рф/путь»"],
            "only ASCII punctuation is left off the end, as the expression's class"
        );
    }

    #[test]
    fn the_markup_escapes_everything_but_the_link() {
        assert_eq!(
            linked("a<b> https://e.com/?a=1&b=2"),
            "a&lt;b&gt; <a href=\"https://e.com/?a=1&amp;b=2\">https://e.com/?a=1&amp;b=2</a>"
        );
        assert_eq!(linked("no link & <tag>"), "no link &amp; &lt;tag&gt;");
    }
}
