//! How loud everybody else is (SoundPanel.tsx, the Mac's SoundPanelView).
//!
//! A list of what can be heard rather than of what can be seen: the person
//! who is too loud may be on no picture at all. One row per thing there is
//! to hear, because a shared screen's sound, the song desk's music and the
//! voice describing them arrive on separate tracks and are turned down
//! separately; nobody hears their own microphone, so there is no row for
//! it. Each person's rows share a card, so that whose sound a slider is
//! never has to be read twice.
//!
//! Brought up to date row by row: a person arriving or leaving is a card
//! arriving or leaving, a share's sound starting or stopping a row sliding
//! in or out, and everything else is changed where it stands. Made again
//! whole on every event of the call, a slider being dragged was taken out
//! from under the pointer.

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use adw::prelude::*;
use tomoshibi_core::call::MediaSource;

use super::{
    motion::{Away, Motion},
    panels::{frame, muted, PanelView},
    Hearing, Panel, RoomState, Sound,
};
use crate::{
    app::AppModel,
    i18n::{t, t_with},
    media::audio::mixer::MAX_VOLUME,
    theme::motion,
};

/// The rows' own look. Volume is drawn in the foreground's white, never the
/// signal colour, which is kept for what is on (the Mac's GlassSlider,
/// Windows' SoundPanel: a volume is not something that is happening).
const SOUND_CSS: &str = r#"
.tmsb-sound-card {
  background-color: alpha(@tmsb_call_wash, 0.05);
  border-radius: 14px; padding: 10px 12px 8px 12px;
}
.tmsb-sound-name { font-size: 13px; font-weight: 600; }
.tmsb-sound-note {
  font-size: 10.5px; font-weight: 500; color: @tmsb_call_muted;
  background-color: alpha(@tmsb_call_wash, 0.07); border-radius: 999px; padding: 1px 7px;
}
.tmsb-sound-title { font-size: 11.5px; font-weight: 500; color: @tmsb_call_muted; }
.tmsb-sound-title.silenced { color: alpha(@tmsb_call_muted, 0.7); }
.tmsb-sound-title image { -gtk-icon-size: 11px; }
button.tmsb-sound-mute {
  min-width: 28px; min-height: 28px; padding: 0; border: none; border-radius: 999px;
  background-color: alpha(@tmsb_call_wash, 0.06); color: @tmsb_call_muted;
  box-shadow: inset 0 0 0 1px alpha(@tmsb_call_wash, 0.06); -gtk-icon-size: 12px;
  transition: background-color 120ms ease-out, color 320ms cubic-bezier(0.2, 0.9, 0.3, 1),
              transform 140ms cubic-bezier(0.2, 0.9, 0.3, 1);
}
button.tmsb-sound-mute:hover { background-color: alpha(@tmsb_call_wash, 0.16); color: @tmsb_call_fg; }
button.tmsb-sound-mute:active { background-color: alpha(@tmsb_call_wash, 0.22); transform: scale(0.95); }
button.tmsb-sound-mute.blocked {
  background-color: alpha(@tmsb_call_wash, 0.16); color: @tmsb_call_danger_text;
  box-shadow: inset 0 0 0 1px alpha(@tmsb_call_wash, 0.16);
}
.tmsb-sound-percent { font-size: 11px; font-weight: 500; color: @tmsb_call_muted; font-feature-settings: "tnum"; }
.tmsb-sound-percent.louder { color: @tmsb_tally_text; }
.tmsb-panel scale:disabled { opacity: 0.4; }
"#;

pub fn build(model: &AppModel) -> PanelView {
    crate::theme::add_module_css("room-sound", SOUND_CSS);
    let (outer, body) = frame(model, Panel::Sound, &t("Sound"), &t("Close sound"));
    let nobody = Motion::hidden(&muted(&t("Nobody else is here.")), Away::FADE);
    let cards = gtk::Box::new(gtk::Orientation::Vertical, 8);
    cards.set_widget_name("sound.cards");
    body.append(&nobody);
    body.append(&cards);
    let shown: Rc<RefCell<Vec<PersonSounds>>> = Rc::default();
    let refresh = {
        let (cards, nobody) = (cards.downgrade(), nobody.downgrade());
        move |model: &AppModel| {
            let (Some(cards), Some(nobody)) = (cards.upgrade(), nobody.upgrade()) else {
                return;
            };
            let room = model.kept::<RoomState>();
            let people: Vec<Heard> = room
                .talk
                .borrow()
                .as_ref()
                .map(|talk| {
                    talk.people
                        .iter()
                        .map(|person| Heard {
                            identity: person.id.clone(),
                            name: person.label(),
                            microphone_off: person.microphone_off(),
                            screen: person.track(MediaSource::ScreenShareAudio).is_some(),
                            music: person.music_track().is_some(),
                        })
                        .collect()
                })
                .unwrap_or_default();
            nobody.show(people.is_empty());
            let mut shown = shown.borrow_mut();
            // Gone: their card leaves where it is.
            shown.retain(|card| {
                if people.iter().any(|person| person.identity == card.identity) {
                    return true;
                }
                let (cards, gone) = (cards.downgrade(), card.motion.downgrade());
                card.motion.leave(move || {
                    if let (Some(cards), Some(gone)) = (cards.upgrade(), gone.upgrade()) {
                        if gone.parent().as_ref() == Some(cards.upcast_ref()) {
                            cards.remove(&gone);
                        }
                    }
                });
                false
            });
            // Arrived: a card of their own at the foot of the list.
            for person in &people {
                if shown.iter().any(|card| card.identity == person.identity) {
                    continue;
                }
                let card = PersonSounds::new(model, &person.identity);
                cards.append(&card.motion);
                card.motion.show(true);
                shown.push(card);
            }
            let view = room.view.borrow();
            for card in shown.iter() {
                if let Some(person) = people
                    .iter()
                    .find(|person| person.identity == card.identity)
                {
                    card.update(person, &view);
                }
            }
        }
    };
    PanelView {
        widget: outer.upcast(),
        refresh: Box::new(refresh),
    }
}

/// One person, as the panel reads them.
struct Heard {
    identity: String,
    name: String,
    microphone_off: bool,
    screen: bool,
    music: bool,
}

/// One person's card: their name, the note when their microphone is off,
/// their voice's row, and their screen's and music's while they send them.
struct PersonSounds {
    identity: String,
    motion: Motion,
    name: gtk::Label,
    note: gtk::Revealer,
    voice: SoundRow,
    screen: (gtk::Revealer, SoundRow),
    music: (gtk::Revealer, SoundRow),
}

impl PersonSounds {
    fn new(model: &AppModel, identity: &str) -> PersonSounds {
        let card = gtk::Box::new(gtk::Orientation::Vertical, 8);
        card.add_css_class("tmsb-sound-card");
        card.set_widget_name(&format!("sound:{identity}"));
        let head = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let name = gtk::Label::builder()
            .xalign(0.0)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .build();
        name.add_css_class("tmsb-sound-name");
        // Said rather than drawn as a state of the slider, which is about
        // what the listener chose; this is what the other person is doing.
        let note_label = gtk::Label::new(None);
        note_label.add_css_class("tmsb-sound-note");
        let note = gtk::Revealer::builder()
            .transition_type(gtk::RevealerTransitionType::Crossfade)
            .transition_duration(motion::SETTLE_MS)
            .child(&note_label)
            .valign(gtk::Align::Center)
            .build();
        head.append(&name);
        head.append(&note);
        card.append(&head);
        let voice = SoundRow::new(model, identity, Sound::Voice, None);
        card.append(&voice.root);
        let other = |sound: Sound, icon: &str| {
            let row = SoundRow::new(model, identity, sound, Some(icon));
            let holder = gtk::Revealer::builder()
                .transition_type(gtk::RevealerTransitionType::SlideDown)
                .transition_duration(motion::SETTLE_MS)
                .child(&row.root)
                .build();
            card.append(&holder);
            (holder, row)
        };
        let screen = other(Sound::Screen, "video-display-symbolic");
        let music = other(Sound::Music, "folder-music-symbolic");
        PersonSounds {
            identity: identity.to_owned(),
            motion: Motion::hidden(&card, Away::GROW),
            name,
            note,
            voice,
            screen,
            music,
        }
    }

    fn update(&self, person: &Heard, view: &super::View) {
        if self.name.label() != person.name {
            self.name.set_label(&person.name);
        }
        if let Some(note) = self.note.child().and_downcast::<gtk::Label>() {
            note.set_label(&t("microphone off"));
        }
        self.note.set_reveal_child(person.microphone_off);
        self.voice.update(&person.name, view);
        let (holder, row) = &self.screen;
        holder.set_reveal_child(person.screen);
        row.update(&t_with("{name} (screen)", &[("name", &person.name)]), view);
        let (holder, row) = &self.music;
        holder.set_reveal_child(person.music);
        row.update(&t_with("{name} (music)", &[("name", &person.name)]), view);
    }
}

/// One sound's mute, volume and reading (SoundPanel.tsx `SoundRow`): the
/// speaker on the left, as the web has it, then the slider, then what it is
/// at in figures, because a slider near one end tells nobody whether it is
/// at nought.
struct SoundRow {
    root: gtk::Box,
    identity: String,
    sound: Sound,
    title: Option<gtk::Label>,
    mute: gtk::Button,
    scale: gtk::Scale,
    percent: gtk::Label,
    /// Set while the slider is moved from here, so that it is not taken
    /// for a press.
    quiet: Rc<Cell<bool>>,
}

impl SoundRow {
    fn new(model: &AppModel, identity: &str, sound: Sound, icon: Option<&str>) -> SoundRow {
        let root = gtk::Box::new(gtk::Orientation::Vertical, 4);
        let title = icon.map(|icon| {
            let line = gtk::Box::new(gtk::Orientation::Horizontal, 6);
            line.add_css_class("tmsb-sound-title");
            line.append(&gtk::Image::from_icon_name(icon));
            let words = gtk::Label::builder()
                .xalign(0.0)
                .ellipsize(gtk::pango::EllipsizeMode::End)
                .build();
            line.append(&words);
            line.set_margin_start(2);
            root.append(&line);
            words
        });
        let line = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        let mute = gtk::Button::from_icon_name("audio-volume-high-symbolic");
        mute.add_css_class("tmsb-sound-mute");
        mute.set_valign(gtk::Align::Center);
        // To three times as sent, marked at as sent (the web's VolumeRange
        // lists it, and its browser draws a tick there): past it is where
        // hearing can be harmed.
        let scale = gtk::Scale::with_range(
            gtk::Orientation::Horizontal,
            0.0,
            f64::from(MAX_VOLUME) * 100.0,
            5.0,
        );
        scale.set_hexpand(true);
        scale.set_draw_value(false);
        scale.set_value(100.0);
        scale.add_mark(100.0, gtk::PositionType::Bottom, None);
        scale.set_widget_name(&format!(
            "volume:{identity}{}",
            match sound {
                Sound::Voice => "",
                Sound::Screen => ":screen",
                Sound::Music => ":music",
            }
        ));
        let percent = gtk::Label::builder().xalign(1.0).width_chars(5).build();
        percent.add_css_class("tmsb-sound-percent");
        line.append(&mute);
        line.append(&scale);
        line.append(&percent);
        root.append(&line);

        let quiet = Rc::new(Cell::new(false));
        let (weak, who, hush) = (model.downgrade(), identity.to_owned(), quiet.clone());
        scale.connect_value_changed(move |scale| {
            if hush.get() {
                return;
            }
            if let Some(model) = weak.upgrade() {
                // A drag that comes within six of as sent settles on it rather
                // than a hair either side.
                if scale.value() != 100.0 && (scale.value() - 100.0).abs() < 6.0 {
                    hush.set(true);
                    scale.set_value(100.0);
                    hush.set(false);
                }
                let was = model
                    .kept::<RoomState>()
                    .view
                    .borrow()
                    .hearing_of(&who, sound);
                let level = (scale.value() / 100.0) as f32;
                if level > 1.0 && was.level <= 1.0 {
                    super::louder_than_sent(&model);
                }
                super::set_hearing(
                    &model,
                    &who,
                    sound,
                    Hearing {
                        level,
                        blocked: was.blocked,
                    },
                );
            }
        });
        let (weak, who) = (model.downgrade(), identity.to_owned());
        mute.connect_clicked(move |_| {
            if let Some(model) = weak.upgrade() {
                let now = model
                    .kept::<RoomState>()
                    .view
                    .borrow()
                    .hearing_of(&who, sound);
                super::set_hearing(
                    &model,
                    &who,
                    sound,
                    Hearing {
                        blocked: !now.blocked,
                        ..now
                    },
                );
            }
        });
        SoundRow {
            root,
            identity: identity.to_owned(),
            sound,
            title,
            mute,
            scale,
            percent,
            quiet,
        }
    }

    /// `name` is what the row is called: the person's name for their voice,
    /// "{name} (screen)" and "{name} (music)" for the others.
    fn update(&self, name: &str, view: &super::View) {
        let hearing = view.hearing_of(&self.identity, self.sound);
        let silenced = hearing.blocked || hearing.level <= 0.0;
        if let Some(title) = self.title.as_ref() {
            if title.label() != name {
                title.set_label(name);
            }
            if let Some(line) = title.parent() {
                set_class(&line, "silenced", silenced);
            }
        }
        let value = (hearing.level * 100.0).round() as f64;
        if (self.scale.value() - value).abs() > 0.5 {
            self.quiet.set(true);
            self.scale.set_value(value);
            self.quiet.set(false);
        }
        // Disabled rather than hidden while blocked: nothing is arriving to
        // be made louder, and a slider that moves while the sound stays off
        // is a control that lies.
        self.scale.set_sensitive(!hearing.blocked);
        let said = t_with("{name}'s volume", &[("name", &name)]);
        self.scale
            .update_property(&[gtk::accessible::Property::Label(&said)]);
        // Named for what pressing it does, not for the state it is in: the
        // two read as opposites, and only one is being decided.
        let press = if hearing.blocked {
            t_with("Unmute {name}", &[("name", &name)])
        } else {
            t_with("Mute {name}", &[("name", &name)])
        };
        if self.mute.tooltip_text().as_deref() != Some(press.as_str()) {
            self.mute.set_tooltip_text(Some(&press));
            self.mute
                .update_property(&[gtk::accessible::Property::Label(&press)]);
        }
        self.mute
            .update_state(&[gtk::accessible::State::Pressed(if hearing.blocked {
                gtk::AccessibleTristate::True
            } else {
                gtk::AccessibleTristate::False
            })]);
        self.mute.set_icon_name(if hearing.blocked {
            "audio-volume-muted-symbolic"
        } else {
            "audio-volume-high-symbolic"
        });
        set_class(&self.mute, "blocked", hearing.blocked);
        let reading = if hearing.blocked {
            t("Muted")
        } else {
            format!("{}%", value as i64)
        };
        if self.percent.label() != reading {
            self.percent.set_label(&reading);
        }
        // Past as sent in the colour that marks something live.
        set_class(
            &self.percent,
            "louder",
            !hearing.blocked && hearing.level > 1.0,
        );
    }
}

fn set_class(widget: &impl IsA<gtk::Widget>, name: &str, on: bool) {
    if on {
        widget.as_ref().add_css_class(name);
    } else {
        widget.as_ref().remove_css_class(name);
    }
}
