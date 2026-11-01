//! The small console for whoever is running the meeting (HostPanel.tsx, the
//! Mac's HostPanelView): a link somebody can be sent, who is at the door,
//! where the meeting is held, the people in it, each of whom can be
//! quietened, made host or removed, and ending it for everybody, asked
//! twice, in the panel itself rather than in a dialog over the call.
//! Deliberately less than the management pages can do: this reaches nobody
//! outside this room.
//!
//! The room can change hands while the panel is open, and then the panel
//! says somebody else runs it rather than keeping controls that would all
//! answer "not yours".
//!
//! Brought up to date piece by piece: a section comes and goes by sliding,
//! a person or a knock by their own row arriving or leaving, and a label
//! changes where it stands. It was cleared and made again whole whenever
//! anything in it changed, so a press could land on a row made again under
//! the pointer, and every refresh was everything appearing at once.

use std::cell::RefCell;

use adw::prelude::*;

use super::{
    glyph::{self, Glyph},
    motion::{Away, Motion},
    panels::{frame, muted, pill, section, PanelView},
    Panel, RoomState,
};
use crate::{
    app::AppModel,
    i18n::{say, t},
    theme::motion,
};

const HOST_CSS: &str = r#"
.tmsb-host-card {
  background-color: alpha(@tmsb_call_wash, 0.05);
  border-radius: 14px; padding: 4px;
}
.tmsb-host-card.door { background-color: alpha(@tmsb_call_tally, 0.10); }
.tmsb-host-card.danger { background-color: alpha(@tmsb_call_danger, 0.10); padding: 12px; }
.tmsb-host-link {
  font-family: monospace; font-size: 10.5px; color: alpha(@tmsb_call_fg, 0.8);
  background-color: alpha(black, 0.25); border-radius: 10px; padding: 8px 10px;
  box-shadow: inset 0 0 0 1px alpha(@tmsb_call_wash, 0.07);
}
.tmsb-host-note { font-size: 11px; color: @tmsb_call_muted; }
.tmsb-host-foot { font-size: 11px; color: alpha(@tmsb_call_muted, 0.85); }
.tmsb-host-elsewhere { font-size: 11px; color: @tmsb_call_muted; }
.tmsb-host-person { padding: 6px 6px 6px 8px; border-radius: 12px; }
.tmsb-host-person:hover { background-color: alpha(@tmsb_call_wash, 0.06); }
.tmsb-host-person label.name { font-size: 13px; font-weight: 500; }
button.tmsb-panel-button.faint { opacity: 0.45; }
button.tmsb-quiet {
  min-height: 26px; padding: 0 10px; border: none; border-radius: 999px; box-shadow: none;
  background-color: alpha(@tmsb_call_wash, 0.05); color: @tmsb_call_muted; font-size: 11.5px; font-weight: 500;
  transition: background-color 120ms ease-out, color 120ms ease-out;
}
button.tmsb-quiet:hover { background-color: alpha(@tmsb_call_danger, 0.14); color: @tmsb_call_fg; }
button.tmsb-quiet:disabled { opacity: 0.4; }
.tmsb-host-elsewhere-chevron { color: @tmsb_call_muted; -gtk-icon-size: 10px; transition: -gtk-icon-transform 320ms cubic-bezier(0.2, 0.9, 0.3, 1); }
.tmsb-host-elsewhere-chevron.open { -gtk-icon-transform: rotate(180deg); }
.tmsb-host-other { padding: 18px 0; }
.tmsb-host-other image { color: @tmsb_call_muted; }
"#;

pub fn build(model: &AppModel) -> PanelView {
    crate::theme::add_module_css("room-host", HOST_CSS);
    // The relays to move to, asked once, when the panel is first opened.
    super::load_relays(model);
    let (outer, body) = frame(model, Panel::Host, &t("Running this room"), &t("Close"));
    let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    body.append(&content);

    let measuring = slider(&muted(&t("Measuring…")));
    content.append(&measuring);

    // Somebody else runs it.
    let other = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    other.add_css_class("tmsb-host-other");
    other.set_halign(gtk::Align::Center);
    other.append(&Glyph::new(glyph::CROWN, 13));
    let other_words = muted(&t("Somebody else runs this room."));
    other.append(&other_words);
    let other_holder = slider(&other);
    content.append(&other_holder);

    // Everything for whoever runs it. The space between the sections is
    // each section's own, so that one slid shut leaves none behind.
    let yours = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let yours_holder = slider(&yours);
    content.append(&yours_holder);

    let inviting = Inviting::new(model);
    yours.append(&inviting.card);
    let door = Door::new();
    yours.append(&door.holder);
    let elsewhere = Elsewhere::new(model);
    yours.append(&elsewhere.holder);
    let people = People::new();
    yours.append(&people.card);
    let ending = Ending::new(model);
    yours.append(&ending.stack);
    for section in [
        inviting.card.upcast_ref::<gtk::Widget>(),
        people.card.upcast_ref(),
        ending.stack.upcast_ref(),
    ] {
        section.set_margin_bottom(12);
    }
    let foot = gtk::Label::builder()
        .xalign(0.0)
        .wrap(true)
        .max_width_chars(48)
        .margin_start(6)
        .margin_end(6)
        .build();
    foot.add_css_class("tmsb-host-foot");
    yours.append(&foot);

    let refresh = {
        let (measuring, other_holder, yours_holder, foot) = (
            measuring.downgrade(),
            other_holder.downgrade(),
            yours_holder.downgrade(),
            foot.downgrade(),
        );
        move |model: &AppModel| {
            let (Some(measuring), Some(other_holder), Some(yours_holder), Some(foot)) = (
                measuring.upgrade(),
                other_holder.upgrade(),
                yours_holder.upgrade(),
                foot.upgrade(),
            ) else {
                return;
            };
            let room = model.kept::<RoomState>();
            let host = room.host.borrow();
            let standing = host.standing;
            measuring.set_reveal_child(standing.is_none());
            let yours = standing.as_ref().is_some_and(|standing| standing.yours);
            other_holder.set_reveal_child(standing.is_some() && !yours);
            yours_holder.set_reveal_child(yours);
            if !yours {
                return;
            }
            let admin = standing.as_ref().is_some_and(|standing| standing.admin);
            let scoped = super::scoped(model);
            let talk = room.talk.borrow();
            let others = talk
                .as_ref()
                .map(|talk| talk.people.clone())
                .unwrap_or_default();
            inviting.update(&host, room.view.borrow().copied, scoped);
            door.update(model, &host);
            elsewhere.update(&host);
            people.update(model, &host, &others, scoped);
            ending.update(&host, scoped);
            let said = if admin {
                t("You can do this because you administer this server. Removing somebody ends their call; it does not stop them rejoining.")
            } else {
                t("Removing somebody ends their call. It does not stop them rejoining with the same name.")
            };
            if foot.label() != said {
                foot.set_label(&said);
            }
        }
    };
    PanelView {
        widget: outer.upcast(),
        refresh: Box::new(refresh),
    }
}

/// A section that slides open and shut rather than appearing.
fn slider(child: &impl IsA<gtk::Widget>) -> gtk::Revealer {
    gtk::Revealer::builder()
        .transition_type(gtk::RevealerTransitionType::SlideDown)
        .transition_duration(motion::SETTLE_MS)
        .child(child)
        .build()
}

fn set_class(widget: &impl IsA<gtk::Widget>, name: &str, on: bool) {
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

/// A row of the panel's menu: a symbol and a title, pressed as one.
fn menu_item(icon: &str, title: &str) -> (gtk::Button, gtk::Image, gtk::Label) {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    let image = gtk::Image::from_icon_name(icon);
    let heading = gtk::Label::builder()
        .label(title)
        .xalign(0.0)
        .hexpand(true)
        .build();
    heading.add_css_class("title");
    row.append(&image);
    row.append(&heading);
    let button = gtk::Button::builder().child(&row).build();
    button.add_css_class("tmsb-menu-row");
    (button, image, heading)
}

// ---- a link -----------------------------------------------------------------

struct Inviting {
    card: gtk::Box,
    invite: gtk::Button,
    invite_icon: gtk::Image,
    invite_words: gtk::Label,
    link_holder: gtk::Revealer,
    link: gtk::Label,
    explained: gtk::Label,
    revoke: gtk::Button,
}

impl Inviting {
    fn new(model: &AppModel) -> Inviting {
        let card = gtk::Box::new(gtk::Orientation::Vertical, 0);
        card.add_css_class("tmsb-host-card");
        let (invite, invite_icon, invite_words) =
            menu_item("insert-link-symbolic", &t("Invite somebody"));
        invite.set_widget_name("host.invite");
        let weak = model.downgrade();
        invite.connect_clicked(move |_| {
            if let Some(model) = weak.upgrade() {
                super::invite(&model);
            }
        });
        card.append(&invite);
        let link_box = gtk::Box::new(gtk::Orientation::Vertical, 8);
        link_box.set_margin_start(8);
        link_box.set_margin_end(8);
        link_box.set_margin_top(4);
        link_box.set_margin_bottom(8);
        // Its natural width held to the panel's: a label that wraps still
        // asks for its whole line, and a link's line widened the panel to
        // twice its width.
        let link = gtk::Label::builder()
            .xalign(0.0)
            .wrap(true)
            .wrap_mode(gtk::pango::WrapMode::Char)
            .max_width_chars(36)
            .selectable(true)
            .build();
        link.add_css_class("tmsb-host-link");
        link.set_widget_name("host.link");
        let explained = gtk::Label::builder()
            .xalign(0.0)
            .wrap(true)
            .max_width_chars(48)
            .build();
        explained.add_css_class("tmsb-host-note");
        // A link goes out and cannot be taken back; this stops it without
        // ending the meeting.
        let revoke = gtk::Button::new();
        revoke.add_css_class("tmsb-quiet");
        revoke.set_halign(gtk::Align::Start);
        revoke.set_widget_name("host.revoke");
        let weak = model.downgrade();
        revoke.connect_clicked(move |_| {
            if let Some(model) = weak.upgrade() {
                super::revoke(&model);
            }
        });
        link_box.append(&link);
        link_box.append(&explained);
        link_box.append(&revoke);
        let link_holder = slider(&link_box);
        card.append(&link_holder);
        Inviting {
            card,
            invite,
            invite_icon,
            invite_words,
            link_holder,
            link,
            explained,
            revoke,
        }
    }

    /// `copied`: the link was just made and copied, said on the button for
    /// a moment (mod.rs `mark_copied`).
    fn update(&self, host: &super::Host, copied: bool, scoped: bool) {
        let copied = copied && host.link.is_some();
        self.invite
            .set_sensitive(host.busy.as_deref() != Some("invite"));
        set_text(
            &self.invite_words,
            &if copied {
                t("Link copied")
            } else {
                t("Invite somebody")
            },
        );
        self.invite_icon.set_icon_name(Some(if copied {
            "object-select-symbolic"
        } else {
            "insert-link-symbolic"
        }));
        if let Some(link) = host.link.as_ref() {
            set_text(&self.link, link);
        }
        self.link_holder.set_reveal_child(host.link.is_some());
        set_text(
            &self.explained,
            &format!(
                "{} {}",
                t("Anybody with this link can join, until you end the meeting. No passphrase needed."),
                t("It stops working after a day either way.")
            ),
        );
        // Under a scope it also stops the standing links members made, so
        // it says every link (HostPanel.tsx).
        self.revoke.set_label(&if scoped {
            t("Stop every link to this room working")
        } else {
            t("Stop this link working")
        });
        self.revoke
            .set_sensitive(host.busy.as_deref() != Some("revoke"));
    }
}

// ---- the door ---------------------------------------------------------------

/// Who is outside, under the control for letting people in: the two are
/// the same thought (HostPanel.tsx).
struct Door {
    holder: gtk::Revealer,
    rows: gtk::Box,
    shown: RefCell<Vec<(String, Motion, gtk::Button, gtk::Button)>>,
}

impl Door {
    fn new() -> Door {
        let card = gtk::Box::new(gtk::Orientation::Vertical, 4);
        card.add_css_class("tmsb-host-card");
        card.add_css_class("door");
        card.set_margin_bottom(12);
        let heading = section(&t("Waiting to be let in"));
        heading.set_margin_start(10);
        heading.set_margin_top(8);
        card.append(&heading);
        let rows = gtk::Box::new(gtk::Orientation::Vertical, 4);
        rows.set_margin_start(10);
        rows.set_margin_end(6);
        rows.set_margin_bottom(8);
        card.append(&rows);
        Door {
            holder: slider(&card),
            rows,
            shown: RefCell::default(),
        }
    }

    fn update(&self, model: &AppModel, host: &super::Host) {
        self.holder.set_reveal_child(!host.waiting.is_empty());
        let mut shown = self.shown.borrow_mut();
        shown.retain(|(id, motion, _, _)| {
            if host.waiting.iter().any(|knocker| &knocker.id == id) {
                return true;
            }
            let (rows, gone) = (self.rows.downgrade(), motion.downgrade());
            motion.leave(move || {
                if let (Some(rows), Some(gone)) = (rows.upgrade(), gone.upgrade()) {
                    if gone.parent().as_ref() == Some(rows.upcast_ref()) {
                        rows.remove(&gone);
                    }
                }
            });
            false
        });
        for knocker in &host.waiting {
            if shown.iter().any(|(id, ..)| *id == knocker.id) {
                continue;
            }
            let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
            row.add_css_class("tmsb-panel-row");
            let name = gtk::Label::builder()
                .label(&knocker.name)
                .xalign(0.0)
                .hexpand(true)
                .ellipsize(gtk::pango::EllipsizeMode::End)
                .build();
            let admit = pill(&t("Let in"));
            admit.add_css_class("primary");
            let refuse = pill(&t("Not now"));
            for (button, yes) in [(&admit, true), (&refuse, false)] {
                let (weak, id) = (model.downgrade(), knocker.id.clone());
                button.connect_clicked(move |_| {
                    if let Some(model) = weak.upgrade() {
                        super::answer_knock(&model, &id, yes);
                    }
                });
            }
            row.append(&name);
            row.append(&refuse);
            row.append(&admit);
            let motion = Motion::hidden(&row, Away::GROW);
            self.rows.append(&motion);
            motion.show(true);
            shown.push((knocker.id.clone(), motion, admit, refuse));
        }
        for (_, _, admit, refuse) in shown.iter() {
            admit.set_sensitive(host.busy.is_none());
            refuse.set_sensitive(host.busy.is_none());
        }
    }
}

// ---- where it is held -------------------------------------------------------

/// "Hold this call somewhere else": the relays the meeting can be put on,
/// where there are two or more (HostPanel.tsx `Elsewhere`). The move under
/// way takes the chevron's place with a spinner.
struct Elsewhere {
    holder: gtk::Revealer,
    open_button: gtk::Button,
    chevron: gtk::Image,
    turning: gtk::Stack,
    spinner: gtk::Spinner,
    list_holder: gtk::Revealer,
    relays: gtk::Box,
    shown: RefCell<Vec<String>>,
    model: crate::app::WeakModel,
}

impl Elsewhere {
    fn new(model: &AppModel) -> Elsewhere {
        let card = gtk::Box::new(gtk::Orientation::Vertical, 2);
        card.add_css_class("tmsb-host-card");
        card.set_margin_bottom(12);
        let (open_button, _, words) = menu_item(
            "network-server-symbolic",
            &t("Hold this call somewhere else"),
        );
        open_button.set_widget_name("host.elsewhere");
        let chevron = gtk::Image::from_icon_name("pan-down-symbolic");
        chevron.add_css_class("tmsb-host-elsewhere-chevron");
        let spinner = gtk::Spinner::new();
        let turning = gtk::Stack::builder()
            .transition_type(gtk::StackTransitionType::Crossfade)
            .transition_duration(motion::SETTLE_MS)
            .build();
        turning.add_named(&chevron, Some("chevron"));
        turning.add_named(&spinner, Some("busy"));
        if let Some(row) = words.parent().and_downcast::<gtk::Box>() {
            row.append(&turning);
        }
        let weak = model.downgrade();
        open_button.connect_clicked(move |_| {
            if let Some(model) = weak.upgrade() {
                let open = model.kept::<RoomState>().host.borrow().choosing_relay;
                super::choose_relay(&model, !open);
            }
        });
        card.append(&open_button);
        let list = gtk::Box::new(gtk::Orientation::Vertical, 2);
        let note = gtk::Label::builder()
            .label(t("Everybody is reconnected. Nobody has to do anything."))
            .xalign(0.0)
            .wrap(true)
            .max_width_chars(48)
            .margin_start(10)
            .margin_end(10)
            .margin_top(2)
            .margin_bottom(4)
            .build();
        note.add_css_class("tmsb-host-elsewhere");
        list.append(&note);
        let relays = gtk::Box::new(gtk::Orientation::Vertical, 2);
        list.append(&relays);
        let list_holder = slider(&list);
        card.append(&list_holder);
        Elsewhere {
            holder: slider(&card),
            open_button,
            chevron,
            turning,
            spinner,
            list_holder,
            relays,
            shown: RefCell::default(),
            model: model.downgrade(),
        }
    }

    fn update(&self, host: &super::Host) {
        let usable = host.usable_relays();
        self.holder.set_reveal_child(usable.len() >= 2);
        let busy = host.busy.as_deref() == Some("move");
        self.open_button.set_sensitive(!busy);
        self.spinner.set_spinning(busy);
        self.turning
            .set_visible_child_name(if busy { "busy" } else { "chevron" });
        set_class(&self.chevron, "open", host.choosing_relay);
        self.list_holder
            .set_reveal_child(host.choosing_relay && usable.len() >= 2);
        // Made again only when the list itself changed, which is once, when
        // the deployment answers.
        let names: Vec<String> = usable.iter().map(|relay| relay.name.clone()).collect();
        if *self.shown.borrow() == names {
            return;
        }
        super::panels::clear(&self.relays);
        for relay in &usable {
            let shown = relay
                .label
                .clone()
                .filter(|label| !label.is_empty())
                .unwrap_or_else(|| relay.name.clone());
            let (row, _, _) = menu_item("go-next-symbolic", &say(&shown));
            row.set_widget_name(&format!("host.move:{}", relay.name));
            let (weak, name) = (self.model.clone(), relay.name.clone());
            row.connect_clicked(move |_| {
                if let Some(model) = weak.upgrade() {
                    super::move_meeting(&model, &name);
                }
            });
            self.relays.append(&row);
        }
        self.shown.replace(names);
    }
}

// ---- the people -------------------------------------------------------------

struct PersonRow {
    identity: String,
    motion: Motion,
    name: gtk::Label,
    spinner: gtk::Revealer,
    mute: gtk::Button,
    make_host: gtk::Button,
    remove: gtk::Button,
}

struct People {
    card: gtk::Box,
    rows: gtk::Box,
    nobody: gtk::Revealer,
    shown: RefCell<Vec<PersonRow>>,
}

impl People {
    fn new() -> People {
        let card = gtk::Box::new(gtk::Orientation::Vertical, 0);
        card.add_css_class("tmsb-host-card");
        let nobody_words = muted(&t("Nobody else is here."));
        nobody_words.set_margin_top(14);
        nobody_words.set_margin_bottom(14);
        nobody_words.set_halign(gtk::Align::Center);
        let nobody = slider(&nobody_words);
        let rows = gtk::Box::new(gtk::Orientation::Vertical, 0);
        card.append(&nobody);
        card.append(&rows);
        People {
            card,
            rows,
            nobody,
            shown: RefCell::default(),
        }
    }

    fn update(
        &self,
        model: &AppModel,
        host: &super::Host,
        others: &[super::talk::Person],
        scoped: bool,
    ) {
        self.nobody.set_reveal_child(others.is_empty());
        let mut shown = self.shown.borrow_mut();
        shown.retain(|row| {
            if others.iter().any(|person| person.id == row.identity) {
                return true;
            }
            let (rows, gone) = (self.rows.downgrade(), row.motion.downgrade());
            row.motion.leave(move || {
                if let (Some(rows), Some(gone)) = (rows.upgrade(), gone.upgrade()) {
                    if gone.parent().as_ref() == Some(rows.upcast_ref()) {
                        rows.remove(&gone);
                    }
                }
            });
            false
        });
        for person in others {
            if shown.iter().any(|row| row.identity == person.id) {
                continue;
            }
            let row = person_row(model, &person.id);
            self.rows.append(&row.motion);
            row.motion.show(true);
            shown.push(row);
        }
        let busy = host.busy.is_some();
        for row in shown.iter() {
            let Some(person) = others.iter().find(|person| person.id == row.identity) else {
                continue;
            };
            set_text(&row.name, &person.label());
            row.spinner
                .set_reveal_child(host.busy.as_deref() == Some(person.id.as_str()));
            // A microphone there and not already muted (`isMicrophoneEnabled`).
            row.mute.set_sensitive(!busy && !person.microphone_off());
            // A room held under a scope has no host to hand over, and the
            // server refuses it outright: the button goes rather than
            // answering with a refusal. Faint beside a guest rather than
            // taken away, and pressable, so the reason can be said.
            row.make_host.set_visible(!scoped);
            let guest = super::is_guest(&person.id);
            set_class(&row.make_host, "faint", guest);
            row.make_host.set_tooltip_text(Some(&if guest {
                t("They joined without a passphrase, so they cannot run the room. Ask them to rejoin with one.")
            } else {
                t("Make host")
            }));
            row.make_host.set_sensitive(!busy);
            row.remove.set_sensitive(!busy);
        }
    }
}

fn person_row(model: &AppModel, identity: &str) -> PersonRow {
    let row = gtk::Box::new(gtk::Orientation::Vertical, 4);
    row.add_css_class("tmsb-host-person");
    row.set_widget_name(&format!("host.person:{identity}"));
    let head = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let name = gtk::Label::builder()
        .xalign(0.0)
        .hexpand(true)
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .build();
    name.add_css_class("name");
    let spinner_widget = gtk::Spinner::new();
    spinner_widget.set_spinning(true);
    let spinner = gtk::Revealer::builder()
        .transition_type(gtk::RevealerTransitionType::Crossfade)
        .transition_duration(motion::SETTLE_MS)
        .child(&spinner_widget)
        .build();
    head.append(&name);
    head.append(&spinner);
    row.append(&head);
    let actions = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    let mute = pill(&t("Mute"));
    let make_host = pill(&t("Make host"));
    let remove = pill(&t("Remove from room"));
    remove.add_css_class("danger");
    for (button, act) in [
        (&mute, super::silence as fn(&AppModel, &str)),
        (&make_host, super::hand_over),
        (&remove, super::turn_out),
    ] {
        let (weak, who) = (model.downgrade(), identity.to_owned());
        button.connect_clicked(move |_| {
            if let Some(model) = weak.upgrade() {
                act(&model, &who);
            }
        });
        actions.append(button);
    }
    row.append(&actions);
    PersonRow {
        identity: identity.to_owned(),
        motion: Motion::hidden(&row, Away::GROW),
        name,
        spinner,
        mute,
        make_host,
        remove,
    }
}

// ---- ending it --------------------------------------------------------------

/// Asked once rather than done at once: ending a meeting for eleven people
/// cannot be undone, and its button sits beside three that can. The
/// question names what will happen rather than asking whether they are sure
/// (HostPanel.tsx `ending`).
struct Ending {
    stack: gtk::Stack,
    sentence: gtk::Label,
    close: gtk::Button,
}

impl Ending {
    fn new(model: &AppModel) -> Ending {
        let stack = gtk::Stack::builder()
            .transition_type(gtk::StackTransitionType::Crossfade)
            .transition_duration(motion::SETTLE_MS)
            .vhomogeneous(false)
            .interpolate_size(true)
            .build();
        let ask_card = gtk::Box::new(gtk::Orientation::Vertical, 0);
        ask_card.add_css_class("tmsb-host-card");
        let (ask, _, _) = menu_item("system-lock-screen-symbolic", &t("End this meeting"));
        ask.set_widget_name("host.end");
        let weak = model.downgrade();
        ask.connect_clicked(move |_| {
            if let Some(model) = weak.upgrade() {
                super::ask_to_end(&model, true);
            }
        });
        ask_card.append(&ask);
        stack.add_named(&ask_card, Some("ask"));

        let sure = gtk::Box::new(gtk::Orientation::Vertical, 12);
        sure.add_css_class("tmsb-host-card");
        sure.add_css_class("danger");
        let sentence = gtk::Label::builder()
            .xalign(0.0)
            .wrap(true)
            .max_width_chars(36)
            .build();
        sure.append(&sentence);
        let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let cancel = pill(&t("Cancel"));
        // Red: ending it for everybody is leaving, for everybody.
        let close = pill(&t("Close the room"));
        close.add_css_class("danger");
        close.set_hexpand(true);
        close.set_widget_name("host.close");
        let weak = model.downgrade();
        cancel.connect_clicked(move |_| {
            if let Some(model) = weak.upgrade() {
                super::ask_to_end(&model, false);
            }
        });
        let weak = model.downgrade();
        close.connect_clicked(move |_| {
            if let Some(model) = weak.upgrade() {
                super::end_for_everybody(&model);
            }
        });
        buttons.append(&cancel);
        buttons.append(&close);
        sure.append(&buttons);
        stack.add_named(&sure, Some("sure"));
        stack.set_visible_child_name("ask");
        Ending {
            stack,
            sentence,
            close,
        }
    }

    fn update(&self, host: &super::Host, scoped: bool) {
        set_text(
            &self.sentence,
            &if scoped {
                t("Everybody will be disconnected and told the room has closed. Links made in the call stop working; links made on an account page do not.")
            } else {
                t("Everybody will be disconnected and told the room has closed. Links to it stop working.")
            },
        );
        self.close
            .set_sensitive(host.busy.as_deref() != Some("close"));
        let wanted = if host.ending { "sure" } else { "ask" };
        if self.stack.visible_child_name().as_deref() != Some(wanted) {
            self.stack.set_visible_child_name(wanted);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::Host;
    use tomoshibi_protocol::{api::HostStanding, relays::Relay};

    fn relay(name: &str, maintenance: bool, admin_only: bool) -> Relay {
        Relay {
            name: name.into(),
            url: None,
            region: None,
            label: None,
            lat: None,
            lon: None,
            probe: None,
            maintenance,
            fallback: false,
            admin_only,
        }
    }

    #[test]
    fn only_relays_this_host_may_use_are_offered() {
        let mut host = Host {
            standing: Some(HostStanding {
                yours: true,
                admin: false,
            }),
            relays: Some(vec![
                relay("tokyo", false, false),
                relay("resting", true, false),
                relay("reserved", false, true),
            ]),
            ..Host::default()
        };
        let names = |host: &Host| -> Vec<String> {
            host.usable_relays()
                .into_iter()
                .map(|relay| relay.name)
                .collect()
        };
        assert_eq!(names(&host), vec!["tokyo"], "one: nothing to choose");
        host.standing = Some(HostStanding {
            yours: true,
            admin: true,
        });
        assert_eq!(names(&host), vec!["tokyo", "reserved"]);
    }
}
