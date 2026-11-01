//! The video everybody is watching, drawn as one more picture of the room
//! (WatchScreen.tsx, the Mac's WatchStageView): placed by the room's plan
//! like any tile, on the stage when a show starts, in the strip when
//! somebody puts another picture there.
//!
//! The controls drive the show, not this player: pressing pause asks the
//! show to pause, and everybody pauses; this player also does it at once,
//! so that the press is felt without waiting for the show's answer. They are
//! shown under the pointer and while the video is still, and leave and
//! arrive as everything in the call does. In the strip the picture goes on
//! playing, labelled, with no controls.

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::Duration,
};

use adw::prelude::*;
use gtk::pango;

use super::{clock_text, engine, player::Lost, site, Info};
use crate::{
    app::{AppModel, WeakModel},
    i18n::{t, t_with},
    ui::room::{
        motion::{Away, Motion},
        Panel, RoomState,
    },
};

/// Where the room put the video, and what reaches across it there.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Place {
    pub on_stage: bool,
    pub filled: bool,
    /// Small: in the strip.
    pub compact: bool,
    /// The stage's own buttons: how far below the top and in from the right.
    pub controls_top: i32,
    pub controls_end: i32,
    /// How far the controls are lifted off the foot, clear of the bar.
    pub lift: i32,
}

/// Which face the stage shows.
#[derive(Debug, Clone, PartialEq)]
enum Face {
    Nothing,
    Waiting,
    Cannot { unavailable: bool },
    Playing,
}

pub struct StageView {
    model: WeakModel,
    root: gtk::Overlay,
    holder: gtk::Box,
    shown_key: RefCell<Option<String>>,
    face: RefCell<Option<Face>>,
    spinner: Motion,
    empty: Motion,
    empty_words: gtk::Label,
    empty_add: gtk::Button,
    cannot: Motion,
    cannot_words: gtk::Label,
    cannot_next: gtk::Button,
    lost: Motion,
    lost_title: gtk::Label,
    lost_words: gtk::Label,
    controls: Motion,
    title: gtk::Label,
    by: gtk::Label,
    scale: gtk::Scale,
    play: gtk::Button,
    next: gtk::Button,
    time: gtk::Label,
    mute: gtk::Button,
    volume: gtk::Scale,
    videos: gtk::Button,
    stage_buttons: Motion,
    fill: gtk::Button,
    tag: Motion,
    tag_words: gtk::Label,
    hovering: Cell<bool>,
    /// A position being dragged to, and when it is to be sent.
    scrubbing: Cell<Option<f64>>,
    scrub_timer: RefCell<Option<glib::SourceId>>,
    place: Cell<Place>,
    timer: RefCell<Option<glib::SourceId>>,
}

fn stage_button(icon: &str, name: &str) -> gtk::Button {
    let button = gtk::Button::from_icon_name(icon);
    button.add_css_class("tmsb-watch-control");
    button.set_widget_name(name);
    button
}

fn label_button(name: &str, prominent: bool) -> gtk::Button {
    let button = gtk::Button::new();
    button.add_css_class("tmsb-watch-pill-button");
    if prominent {
        button.add_css_class("prominent");
    }
    button.set_widget_name(name);
    button.set_halign(gtk::Align::Center);
    button
}

fn centred_column() -> gtk::Box {
    gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(12)
        .halign(gtk::Align::Center)
        .valign(gtk::Align::Center)
        .margin_start(16)
        .margin_end(16)
        .build()
}

fn set_label(widget: &impl IsA<gtk::Widget>, label: &str) {
    let widget = widget.as_ref();
    if widget.tooltip_text().as_deref() != Some(label) {
        widget.set_tooltip_text(Some(label));
        widget.update_property(&[gtk::accessible::Property::Label(label)]);
    }
}

impl StageView {
    pub fn new(model: &AppModel) -> Rc<StageView> {
        let root = gtk::Overlay::builder()
            .name("watch.stage")
            .overflow(gtk::Overflow::Hidden)
            .hexpand(true)
            .vexpand(true)
            .build();
        root.add_css_class("tmsb-tile");
        root.add_css_class("tmsb-watch-stage");
        let holder = gtk::Box::builder().hexpand(true).vexpand(true).build();
        root.set_child(Some(&holder));

        let spinner = gtk::Spinner::builder()
            .spinning(true)
            .width_request(26)
            .height_request(26)
            .halign(gtk::Align::Center)
            .valign(gtk::Align::Center)
            .build();
        let spinner = Motion::hidden(&spinner, Away::GROW);
        spinner.set_can_target(false);
        root.add_overlay(&spinner);

        // Nothing playing yet: what to do about it.
        let empty_box = centred_column();
        empty_box.add_css_class("tmsb-watch-empty");
        let film = gtk::Image::from_icon_name("video-x-generic-symbolic");
        film.add_css_class("tmsb-watch-film");
        let empty_words = gtk::Label::builder()
            .wrap(true)
            .justify(gtk::Justification::Center)
            .max_width_chars(40)
            .build();
        let empty_add = label_button("watch.add", true);
        empty_box.append(&film);
        empty_box.append(&empty_words);
        empty_box.append(&empty_add);
        let empty = Motion::hidden(&empty_box, Away::GROW);
        root.add_overlay(&empty);

        // A video this app cannot read.
        let cannot_box = centred_column();
        let cannot_words = gtk::Label::builder()
            .wrap(true)
            .justify(gtk::Justification::Center)
            .max_width_chars(40)
            .build();
        cannot_words.add_css_class("tmsb-watch-quiet");
        let cannot_next = label_button("watch.cannot.next", false);
        cannot_box.append(&cannot_words);
        cannot_box.append(&cannot_next);
        let cannot = Motion::hidden(&cannot_box, Away::GROW);
        root.add_overlay(&cannot);

        // A video that does not reach this viewer.
        let lost_box = centred_column();
        lost_box.set_spacing(8);
        lost_box.add_css_class("tmsb-watch-lost");
        let lost_mark = gtk::Image::from_icon_name("network-offline-symbolic");
        lost_mark.add_css_class("tmsb-watch-lost-mark");
        lost_mark.set_halign(gtk::Align::Center);
        let lost_title = gtk::Label::builder()
            .wrap(true)
            .justify(gtk::Justification::Center)
            .build();
        lost_title.add_css_class("tmsb-watch-lost-title");
        let lost_words = gtk::Label::builder()
            .wrap(true)
            .justify(gtk::Justification::Center)
            .max_width_chars(44)
            .build();
        lost_words.add_css_class("tmsb-watch-quiet");
        lost_box.append(&lost_mark);
        lost_box.append(&lost_title);
        lost_box.append(&lost_words);
        let lost = Motion::hidden(&lost_box, Away::GROW);
        root.add_overlay(&lost);

        // The controls under the video: its title, where it is, and the
        // presses that drive the show.
        let title = gtk::Label::builder()
            .xalign(0.0)
            .ellipsize(pango::EllipsizeMode::End)
            .single_line_mode(true)
            .build();
        title.add_css_class("tmsb-watch-title");
        let by = gtk::Label::builder()
            .xalign(0.0)
            .ellipsize(pango::EllipsizeMode::End)
            .single_line_mode(true)
            .hexpand(true)
            .build();
        by.add_css_class("tmsb-watch-quiet");
        let heading = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        heading.append(&title);
        heading.append(&by);
        let scale = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 1.0, 0.1);
        scale.set_draw_value(false);
        scale.set_hexpand(true);
        scale.add_css_class("tmsb-watch-position");
        scale.set_widget_name("watch.position");
        let play = stage_button("media-playback-start-symbolic", "watch.play");
        let next = stage_button("media-skip-forward-symbolic", "watch.next");
        let time = gtk::Label::new(None);
        time.add_css_class("tmsb-watch-time");
        time.set_widget_name("watch.time");
        let spacer = gtk::Box::builder().hexpand(true).build();
        let mute = stage_button("audio-volume-high-symbolic", "watch.mute");
        let volume = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 1.0, 0.05);
        volume.set_draw_value(false);
        volume.set_width_request(84);
        volume.set_value(1.0);
        volume.add_css_class("tmsb-watch-volume");
        let videos = stage_button("view-list-symbolic", "watch.videos");
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        row.append(&play);
        row.append(&next);
        row.append(&time);
        row.append(&spacer);
        row.append(&mute);
        row.append(&volume);
        row.append(&videos);
        let controls_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(6)
            .valign(gtk::Align::End)
            .build();
        controls_box.add_css_class("tmsb-watch-controls");
        controls_box.append(&heading);
        controls_box.append(&scale);
        controls_box.append(&row);
        let controls = Motion::hidden(&controls_box, Away::FOOT);
        controls.set_valign(gtk::Align::End);
        controls.set_widget_name("watch.controls");
        root.add_overlay(&controls);

        // The stage's own button, filling the screen with the video.
        let fill = gtk::Button::from_icon_name("view-fullscreen-symbolic");
        fill.add_css_class("tmsb-stage-button");
        fill.set_widget_name("watch.fill");
        let stage_buttons = Motion::hidden(&fill, Away::FADE);
        stage_buttons.set_halign(gtk::Align::End);
        stage_buttons.set_valign(gtk::Align::Start);
        stage_buttons.set_margin_end(12);
        stage_buttons.set_margin_top(12);
        root.add_overlay(&stage_buttons);

        // In the strip: what it is, since it has no controls there.
        let tag_words = gtk::Label::new(None);
        tag_words.add_css_class("tmsb-watch-tag");
        let tag = Motion::hidden(&tag_words, Away::FADE);
        tag.set_halign(gtk::Align::Start);
        tag.set_valign(gtk::Align::End);
        tag.set_margin_start(4);
        tag.set_margin_bottom(4);
        tag.set_can_target(false);
        root.add_overlay(&tag);

        let view = Rc::new(StageView {
            model: model.downgrade(),
            root,
            holder,
            shown_key: RefCell::new(None),
            face: RefCell::new(None),
            spinner,
            empty,
            empty_words,
            empty_add,
            cannot,
            cannot_words,
            cannot_next,
            lost,
            lost_title,
            lost_words,
            controls,
            title,
            by,
            scale,
            play,
            next,
            time,
            mute,
            volume,
            videos,
            stage_buttons,
            fill,
            tag,
            tag_words,
            hovering: Cell::new(false),
            scrubbing: Cell::new(None),
            scrub_timer: RefCell::new(None),
            place: Cell::new(Place::default()),
            timer: RefCell::new(None),
        });
        view.connect();
        view.reword();
        view.refresh();
        let weak = Rc::downgrade(&view);
        let timer = glib::timeout_add_local(
            Duration::from_millis(tomoshibi_protocol::watch::player::KEEP_MS),
            move || {
                let Some(view) = weak.upgrade() else {
                    return glib::ControlFlow::Break;
                };
                view.refresh();
                glib::ControlFlow::Continue
            },
        );
        view.timer.replace(Some(timer));
        view
    }

    pub fn widget(&self) -> &gtk::Overlay {
        &self.root
    }

    fn connect(self: &Rc<Self>) {
        let hover = gtk::EventControllerMotion::new();
        let weak = Rc::downgrade(self);
        hover.connect_enter(move |_, _, _| {
            if let Some(view) = weak.upgrade() {
                view.hovering.set(true);
                view.refresh();
            }
        });
        let weak = Rc::downgrade(self);
        hover.connect_leave(move |_| {
            if let Some(view) = weak.upgrade() {
                view.hovering.set(false);
                view.refresh();
            }
        });
        self.root.add_controller(hover);

        let on = |button: &gtk::Button, act: fn(&StageView, &AppModel)| {
            let weak = Rc::downgrade(self);
            button.connect_clicked(move |_| {
                if let Some(view) = weak.upgrade() {
                    if let Some(model) = view.model.upgrade() {
                        act(&view, &model);
                    }
                }
            });
        };
        let open_panel = |_: &StageView, model: &AppModel| {
            if model.kept::<RoomState>().view.borrow().panel != Some(Panel::Watch) {
                crate::ui::room::toggle_panel(model, Panel::Watch);
            }
        };
        on(&self.empty_add, open_panel);
        on(&self.videos, open_panel);
        on(&self.play, |_, model| {
            if let Some(engine) = engine(model) {
                engine.toggle();
            }
        });
        let skip = |_: &StageView, model: &AppModel| {
            let Some(engine) = engine(model) else { return };
            let key = engine
                .shared
                .borrow()
                .show
                .as_ref()
                .and_then(|show| show.now.as_ref().map(|video| video.key.clone()));
            if let Some(key) = key {
                engine.skip(&key);
            }
        };
        on(&self.next, skip);
        on(&self.cannot_next, skip);
        on(&self.mute, |view, model| {
            let player = engine(model).and_then(|engine| engine.player.borrow().clone());
            if let Some(player) = player {
                player.set_muted(!player.muted());
            }
            view.refresh();
        });
        on(&self.fill, |view, model| {
            let filled = model.kept::<RoomState>().view.borrow().filled;
            if filled {
                crate::ui::room::unfill(model);
            } else if view.place.get().on_stage {
                crate::ui::room::fill(model);
            }
        });

        // A drag of the position, sent once it has stopped moving for a
        // moment: every step of a drag sent to the show moved everybody's
        // player as many times.
        let weak = Rc::downgrade(self);
        self.scale.connect_change_value(move |_, _, value| {
            let Some(view) = weak.upgrade() else {
                return glib::Propagation::Proceed;
            };
            view.scrubbing.set(Some(value));
            if let Some(timer) = view.scrub_timer.take() {
                timer.remove();
            }
            let later = Rc::downgrade(&view);
            let timer = glib::timeout_add_local_once(Duration::from_millis(250), move || {
                let Some(view) = later.upgrade() else { return };
                view.scrub_timer.replace(None);
                let Some(to) = view.scrubbing.take() else {
                    return;
                };
                if let Some(engine) = view.model.upgrade().and_then(|model| engine(&model)) {
                    engine.seek(to);
                }
            });
            view.scrub_timer.replace(Some(timer));
            glib::Propagation::Proceed
        });
        let weak = Rc::downgrade(self);
        self.volume.connect_change_value(move |_, _, value| {
            if let Some(view) = weak.upgrade() {
                let player = view
                    .model
                    .upgrade()
                    .and_then(|model| engine(&model))
                    .and_then(|engine| engine.player.borrow().clone());
                if let Some(player) = player {
                    player.set_volume(value);
                }
            }
            glib::Propagation::Proceed
        });
    }

    /// Says again what is said once, in the language now.
    pub fn reword(&self) {
        self.empty_words.set_label(&t(
            "Nothing is playing. Paste a video link to watch together.",
        ));
        self.empty_add.set_label(&t("Add a video"));
        self.cannot_next.set_label(&t("Next video"));
        self.lost_title
            .set_label(&t("This video does not reach you"));
        self.tag_words.set_label(&t("Watching together"));
        set_label(&self.next, &t("Next video"));
        set_label(&self.videos, &t("Videos"));
        self.scale
            .update_property(&[gtk::accessible::Property::Label(&t(
                "Position in the video",
            ))]);
        self.volume
            .update_property(&[gtk::accessible::Property::Label(&t("Video volume"))]);
        self.face.replace(None);
        self.refresh();
    }

    /// Where the room put it now.
    pub fn set_place(&self, place: Place) {
        if self.place.replace(place) != place {
            self.refresh();
        }
    }

    /// The show is over: the picture let go of.
    pub fn stop(&self) {
        if let Some(timer) = self.timer.take() {
            timer.remove();
        }
        if let Some(timer) = self.scrub_timer.take() {
            timer.remove();
        }
        while let Some(child) = self.holder.first_child() {
            self.holder.remove(&child);
        }
    }

    /// Whether a frame of the video has been drawn, and how many, for a
    /// test to read.
    pub fn frames_drawn(&self) -> u64 {
        self.holder
            .first_child()
            .and_downcast::<crate::render::VideoView>()
            .map_or(0, |view| view.frames_drawn())
    }

    /// Shows the stage's buttons and the controls as the pointer would, for
    /// a test that has no pointer.
    #[cfg(debug_assertions)]
    pub fn hover_for_test(&self) {
        self.hovering.set(true);
        self.refresh();
    }

    /// Brings everything on it up to date with the show and the player.
    pub fn refresh(&self) {
        let Some(model) = self.model.upgrade() else {
            return;
        };
        let Some(engine) = engine(&model) else {
            return;
        };
        let place = self.place.get();
        let (show, info) = {
            let shared = engine.shared.borrow();
            let show = shared.show.clone();
            let info = show
                .as_ref()
                .and_then(|show| show.now.as_ref())
                .and_then(|video| shared.infos.get(&video.ticket).cloned());
            (show, info)
        };
        let video = show.as_ref().and_then(|show| show.now.clone());
        if let Some(video) = video.as_ref() {
            if info.is_none() {
                engine.info(&video.ticket);
            }
        }
        let player = engine.player.borrow().clone();

        // The player's picture, in place while it is this video's.
        let key = player.as_ref().map(|player| player.key.clone());
        if *self.shown_key.borrow() != key {
            while let Some(child) = self.holder.first_child() {
                self.holder.remove(&child);
            }
            if let Some(player) = player.as_ref() {
                let picture = player.view();
                picture.set_hexpand(true);
                picture.set_vexpand(true);
                self.holder.append(picture);
            }
            self.shown_key.replace(key);
        }

        let face = match (&video, &info) {
            (None, _) => Face::Nothing,
            (Some(_), Some(Info::Known(_))) if player.is_some() => Face::Playing,
            (Some(_), Some(Info::Unavailable)) => Face::Cannot { unavailable: true },
            (Some(_), Some(Info::Failed)) => Face::Cannot { unavailable: false },
            _ => Face::Waiting,
        };
        let compact = place.compact;
        self.empty.show(face == Face::Nothing);
        self.empty_words.set_visible(!compact);
        self.empty_add.set_visible(!compact);
        if let Face::Cannot { unavailable } = face {
            self.cannot_words.set_label(&if unavailable {
                t("That video cannot be played here.")
            } else {
                t("The video could not be read. Try again.")
            });
        }
        self.cannot.show(matches!(face, Face::Cannot { .. }));
        self.cannot_next.set_visible(!compact);
        let lost = player.as_ref().and_then(|player| player.lost());
        let waiting = face == Face::Waiting
            || (face == Face::Playing && player.as_ref().is_some_and(|player| player.waiting()));
        self.spinner.show(waiting && lost.is_none());
        if let (Some(lost), Some(player)) = (lost, player.as_ref()) {
            let site = site(&player.info.link);
            self.lost_words.set_label(&match lost {
                // A kind of video this player cannot open: the browsers in
                // the call can, so the show goes on for them.
                Lost::Format => {
                    t("This app cannot play this kind of video. Everybody else goes on watching.")
                }
                Lost::Refused if !site.is_empty() => {
                    t_with("It plays only on {site} itself.", &[("site", &site)])
                }
                _ if site.is_empty() => t(
                    "Your connection cannot reach where it is. Everybody else goes on watching.",
                ),
                _ => t_with(
                    "Your connection cannot reach {site}. Everybody else goes on watching.",
                    &[("site", &site)],
                ),
            });
        }
        self.lost.show(lost.is_some() && !compact);
        self.face.replace(Some(face.clone()));

        // The controls: under the pointer, and whenever the video is still
        // or being moved, never in the strip.
        let playing = show.as_ref().is_some_and(|show| show.playing);
        let scrubbing = self.scrubbing.get().is_some();
        let controls = !compact
            && face == Face::Playing
            && lost.is_none()
            && (self.hovering.get() || !playing || scrubbing);
        self.controls.show(controls);
        self.controls.set_margin_bottom(place.lift);
        if let (Some(player), Some(video)) = (player.as_ref(), video.as_ref()) {
            if self.title.label() != player.info.title {
                self.title.set_label(&player.info.title);
            }
            let mut by = vec![t_with("Added by {name}", &[("name", &video.by)])];
            if player.relayed() {
                by.push(t("Relayed"));
            }
            let by = by.join(" \u{00B7} ");
            if self.by.label() != by {
                self.by.set_label(&by);
            }
            let length = player.duration();
            let shown = self.scrubbing.get().unwrap_or_else(|| player.position());
            let live = video.live;
            self.scale.set_visible(length > 0.0 && !live);
            if length > 0.0 {
                let adjustment = self.scale.adjustment();
                if (adjustment.upper() - length).abs() > 0.01 {
                    adjustment.set_upper(length);
                }
                if self.scrubbing.get().is_none() {
                    self.scale.set_value(shown.min(length));
                }
            }
            let time = if live {
                t("Live")
            } else if length > 0.0 {
                format!("{} / {}", clock_text(shown), clock_text(length))
            } else {
                clock_text(shown)
            };
            if self.time.label() != time {
                self.time.set_label(&time);
            }
            self.play.set_icon_name(if playing {
                "media-playback-pause-symbolic"
            } else {
                "media-playback-start-symbolic"
            });
            set_label(
                &self.play,
                &if playing {
                    t("Pause for everybody")
                } else {
                    t("Play for everybody")
                },
            );
            let muted = player.muted() || player.volume() <= 0.0;
            self.mute.set_icon_name(if muted {
                "audio-volume-muted-symbolic"
            } else {
                "audio-volume-high-symbolic"
            });
            set_label(
                &self.mute,
                &if player.muted() {
                    t("Unmute the video")
                } else {
                    t("Mute the video")
                },
            );
            if !player.muted() {
                self.volume.set_value(player.volume());
            } else {
                self.volume.set_value(0.0);
            }
        }

        // The stage's button, filling the screen, under the pointer.
        self.stage_buttons
            .show(place.on_stage && !compact && self.hovering.get());
        self.stage_buttons
            .set_margin_top(place.controls_top.max(12));
        self.stage_buttons
            .set_margin_end(place.controls_end.max(10));
        let fill_label = if place.filled {
            t("Leave fullscreen")
        } else {
            t("Fill the screen")
        };
        self.fill.set_icon_name(if place.filled {
            "view-restore-symbolic"
        } else {
            "view-fullscreen-symbolic"
        });
        set_label(&self.fill, &fill_label);
        if place.filled {
            self.root.add_css_class("filled");
        } else {
            self.root.remove_css_class("filled");
        }
        self.tag.show(compact);
    }
}
