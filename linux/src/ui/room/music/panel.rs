//! The music panel (MusicPanel.tsx; the Mac's MusicPanelView): the song desk
//! everybody has, and for whoever can reach the library a search of it.
//! Choosing a song adds it to the desk and starts the desk here if nobody
//! runs one; somebody who cannot reach the library asks the desk instead, by
//! a song's name or a link as it was shared.
//!
//! The volume beside what is playing is the listener's own: for somebody
//! listening, their level of that person's music, muted at the server; for
//! whoever plays it, their own speakers alone. Built once when the panel
//! opens and brought up to date in place; the queue's rows slide in and out
//! by their keys rather than the list being made again.

use std::{cell::RefCell, rc::Rc, time::Duration};

use adw::prelude::*;
use gtk::glib;
use tomoshibi_protocol::{api::music_clock, jukebox::DeskEntry};

use super::{desk_of, monitor_level, song, SongDesk, Tab};
use crate::{
    app::AppModel,
    i18n::{t, t_with},
    ui::room::{
        panels::{frame, muted, section, PanelView},
        Panel, RoomState,
    },
};

fn slide(child: &impl IsA<gtk::Widget>) -> gtk::Revealer {
    gtk::Revealer::builder()
        .transition_type(gtk::RevealerTransitionType::SlideDown)
        .transition_duration(crate::theme::motion::SETTLE_MS)
        .child(child)
        .build()
}

fn set_text(label: &gtk::Label, text: &str) {
    if label.label() != text {
        label.set_label(text);
    }
}

fn icon(name: &str, said: &str) -> gtk::Button {
    let button = gtk::Button::from_icon_name(name);
    button.add_css_class("tmsb-close");
    button.set_tooltip_text(Some(said));
    button.update_property(&[gtk::accessible::Property::Label(said)]);
    button
}

fn label_said(button: &gtk::Button, said: &str) {
    if button.tooltip_text().as_deref() != Some(said) {
        button.set_tooltip_text(Some(said));
        button.update_property(&[gtk::accessible::Property::Label(said)]);
    }
}

/// Rows kept by key in a column, sliding in and out.
struct Rows<T> {
    column: gtk::Box,
    rows: RefCell<Vec<(String, gtk::Revealer, T)>>,
}

impl<T> Rows<T> {
    fn new() -> Rows<T> {
        Rows {
            column: gtk::Box::new(gtk::Orientation::Vertical, 0),
            rows: RefCell::new(Vec::new()),
        }
    }

    /// Brings the rows to `keys` in that order: made with `make`, updated
    /// with `update`, and the ones no longer wanted slid away.
    fn show(
        &self,
        keys: &[String],
        make: impl Fn(&str) -> (gtk::Widget, T),
        update: impl Fn(usize, &str, &T),
    ) {
        let mut rows = self.rows.borrow_mut();
        rows.retain(|(key, revealer, _)| {
            if keys.contains(key) {
                return true;
            }
            let (column, gone) = (self.column.downgrade(), revealer.downgrade());
            revealer.connect_child_revealed_notify(move |revealer| {
                if !revealer.is_child_revealed() {
                    if let (Some(column), Some(gone)) = (column.upgrade(), gone.upgrade()) {
                        if gone.parent().as_ref() == Some(column.upcast_ref()) {
                            column.remove(&gone);
                        }
                    }
                }
            });
            revealer.set_reveal_child(false);
            if !adw::is_animations_enabled(revealer) {
                self.column.remove(revealer);
            }
            false
        });
        let mut previous: Option<gtk::Widget> = None;
        for (index, key) in keys.iter().enumerate() {
            let at = rows.iter().position(|(kept, _, _)| kept == key);
            let revealer = match at {
                Some(at) => rows[at].1.clone(),
                None => {
                    let (widget, parts) = make(key);
                    let revealer = slide(&widget);
                    self.column.insert_child_after(&revealer, previous.as_ref());
                    revealer.set_reveal_child(true);
                    rows.push((key.clone(), revealer.clone(), parts));
                    revealer
                }
            };
            // In the order wanted: a song moved to the top moves there.
            if revealer.prev_sibling() != previous {
                self.column
                    .reorder_child_after(&revealer, previous.as_ref());
            }
            previous = Some(revealer.upcast());
            if let Some((_, _, parts)) = rows.iter().find(|(kept, _, _)| kept == key) {
                update(index, key, parts);
            }
        }
    }
}

/// A song in a list: its title, who sang it, and a note beside.
fn song_row(title: &str, artists: &str) -> (gtk::Box, gtk::Label, gtk::Label) {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    row.add_css_class("tmsb-music-row");
    row.set_margin_start(6);
    let words = gtk::Box::new(gtk::Orientation::Vertical, 0);
    words.set_hexpand(true);
    words.set_valign(gtk::Align::Center);
    let heading = gtk::Label::builder()
        .label(title)
        .xalign(0.0)
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .build();
    heading.add_css_class("title");
    let below = gtk::Label::builder()
        .label(artists)
        .xalign(0.0)
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .build();
    below.add_css_class("artists");
    words.append(&heading);
    words.append(&below);
    row.append(&words);
    (row, heading, below)
}

struct QueueParts {
    title: gtk::Label,
    artists: gtk::Label,
    up: gtk::Button,
    off: gtk::Button,
}

struct ResultParts {
    add: gtk::Button,
}

pub(in crate::ui::room) fn panel(model: &AppModel) -> PanelView {
    crate::theme::add_module_css("room-media", crate::ui::room::devices_panel::MEDIA_CSS);
    let (outer, body) = frame(model, Panel::Music, &t("Music"), &t("Close music"));
    let desk = desk_of(model);

    // The tabs, for whoever can reach the library.
    let tabs = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    tabs.add_css_class("tmsb-music-tabs");
    tabs.set_homogeneous(true);
    let desk_tab = gtk::Button::with_label(&t("Song desk"));
    desk_tab.add_css_class("tmsb-choice");
    let search_tab = gtk::Button::with_label(&t("Search"));
    search_tab.add_css_class("tmsb-choice");
    tabs.append(&desk_tab);
    tabs.append(&search_tab);
    let tabs_slide = slide(&tabs);
    body.append(&tabs_slide);
    let stack = gtk::Stack::builder()
        .transition_type(gtk::StackTransitionType::Crossfade)
        .transition_duration(crate::theme::motion::SETTLE_MS)
        .vhomogeneous(false)
        .build();
    body.append(&stack);
    for (button, tab) in [(&desk_tab, Tab::Desk), (&search_tab, Tab::Search)] {
        let desk = desk.clone();
        button.connect_clicked(move |_| {
            if let Some(desk) = desk.as_ref() {
                desk.tab.set(tab);
                desk.changed();
            }
        });
    }

    // ---- the desk ----
    let page = gtk::Box::new(gtk::Orientation::Vertical, 10);
    let now = gtk::Box::new(gtk::Orientation::Vertical, 6);
    now.add_css_class("tmsb-music-now");
    let now_title = gtk::Label::builder().xalign(0.0).wrap(true).build();
    now_title.add_css_class("title");
    let now_artists = gtk::Label::builder().xalign(0.0).wrap(true).build();
    now_artists.add_css_class("artists");
    let now_by = muted("");
    let now_quality = gtk::Label::builder().xalign(0.0).build();
    now_quality.add_css_class("tmsb-music-quality");
    let progress = gtk::ProgressBar::new();
    progress.add_css_class("tmsb-music-progress");
    let clock = gtk::Label::builder().xalign(1.0).build();
    clock.add_css_class("tmsb-music-quality");
    let progress_box = gtk::Box::new(gtk::Orientation::Vertical, 2);
    progress_box.append(&progress);
    progress_box.append(&clock);
    let progress_slide = slide(&progress_box);
    let controls = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    let pause = icon("media-playback-pause-symbolic", &t("Pause"));
    pause.set_widget_name("music.pause");
    let next = icon("media-skip-forward-symbolic", &t("Next song"));
    next.set_widget_name("music.next");
    let stop = icon("media-playback-stop-symbolic", &t("Stop the desk"));
    stop.set_widget_name("music.stop");
    let vote = gtk::Button::with_label("");
    vote.add_css_class("tmsb-panel-button");
    vote.set_widget_name("music.vote");
    let holder_controls = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    holder_controls.append(&pause);
    holder_controls.append(&next);
    holder_controls.append(&stop);
    let holder_slide = gtk::Revealer::builder()
        .transition_type(gtk::RevealerTransitionType::Crossfade)
        .transition_duration(crate::theme::motion::SETTLE_MS)
        .child(&holder_controls)
        .build();
    let vote_slide = gtk::Revealer::builder()
        .transition_type(gtk::RevealerTransitionType::Crossfade)
        .transition_duration(crate::theme::motion::SETTLE_MS)
        .child(&vote)
        .build();
    controls.append(&holder_slide);
    controls.append(&vote_slide);
    // The listener's own volume of it, or the holder's own speakers.
    let volume_label = muted("");
    let volume = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 100.0, 1.0);
    volume.set_draw_value(false);
    volume.set_hexpand(true);
    volume.set_widget_name("music.volume");
    let mute = icon("audio-volume-high-symbolic", "");
    let volume_line = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    volume_line.append(&volume);
    volume_line.append(&mute);
    now.append(&now_title);
    now.append(&now_artists);
    now.append(&now_by);
    now.append(&now_quality);
    now.append(&progress_slide);
    now.append(&controls);
    now.append(&volume_label);
    now.append(&volume_line);
    let now_slide = slide(&now);
    page.append(&now_slide);
    let idle = muted("");
    let idle_slide = slide(&idle);
    page.append(&idle_slide);
    let loading_line = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let spinner = gtk::Spinner::new();
    let loading = muted("");
    loading.set_hexpand(true);
    loading.set_ellipsize(gtk::pango::EllipsizeMode::End);
    let loading_off = icon("window-close-symbolic", "");
    loading_line.append(&spinner);
    loading_line.append(&loading);
    loading_line.append(&loading_off);
    let loading_slide = slide(&loading_line);
    page.append(&loading_slide);
    let queue_heading = section(&t("Queue"));
    let queue_heading_slide = slide(&queue_heading);
    page.append(&queue_heading_slide);
    let queue: Rc<Rows<QueueParts>> = Rc::new(Rows::new());
    page.append(&queue.column);
    // Turned down under speech, the listener's own choice.
    let duck_row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    duck_row.add_css_class("tmsb-setting");
    let duck_words = gtk::Label::builder()
        .xalign(0.0)
        .wrap(true)
        .hexpand(true)
        .build();
    duck_words.add_css_class("title");
    let duck = gtk::Switch::builder().valign(gtk::Align::Center).build();
    duck.set_widget_name("music.duck");
    duck_row.append(&duck_words);
    duck_row.append(&duck);
    page.append(&duck_row);
    // The box: a song's name, or a link as it was shared.
    let ask_line = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    let ask_box = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    ask_box.add_css_class("tmsb-music-entry");
    ask_box.set_hexpand(true);
    let ask = gtk::Entry::builder().hexpand(true).has_frame(false).build();
    ask.set_widget_name("music.ask");
    ask_box.append(&ask);
    let ask_go = gtk::Button::with_label("");
    ask_go.add_css_class("tmsb-panel-button");
    ask_go.add_css_class("primary");
    ask_go.set_widget_name("music.ask.go");
    ask_line.append(&ask_box);
    ask_line.append(&ask_go);
    page.append(&ask_line);
    stack.add_named(&page, Some("desk"));

    // ---- the library ----
    let search_page = gtk::Box::new(gtk::Orientation::Vertical, 8);
    let search_line = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    let search_box = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    search_box.add_css_class("tmsb-music-entry");
    search_box.set_hexpand(true);
    let search = gtk::Entry::builder().hexpand(true).has_frame(false).build();
    search.set_widget_name("music.search");
    search_box.append(&search);
    let search_go = gtk::Button::with_label("");
    search_go.add_css_class("tmsb-panel-button");
    search_line.append(&search_box);
    search_line.append(&search_go);
    search_page.append(&search_line);
    let searching = gtk::Spinner::new();
    let searching_slide = slide(&searching);
    search_page.append(&searching_slide);
    let search_problem = muted("");
    let search_problem_slide = slide(&search_problem);
    search_page.append(&search_problem_slide);
    let results: Rc<Rows<ResultParts>> = Rc::new(Rows::new());
    search_page.append(&results.column);
    stack.add_named(&search_page, Some("search"));

    // ---- presses ----
    if let Some(desk) = desk.clone() {
        let weak = Rc::downgrade(&desk);
        pause.connect_clicked(move |_| {
            if let Some(desk) = weak.upgrade() {
                let paused = desk
                    .snapshot
                    .borrow()
                    .state
                    .as_ref()
                    .is_some_and(|state| state.paused);
                desk.pause(!paused);
            }
        });
        let weak = Rc::downgrade(&desk);
        next.connect_clicked(move |_| {
            if let Some(desk) = weak.upgrade() {
                desk.skip();
            }
        });
        let weak = Rc::downgrade(&desk);
        stop.connect_clicked(move |_| {
            if let Some(desk) = weak.upgrade() {
                desk.close();
            }
        });
        let weak = Rc::downgrade(&desk);
        vote.connect_clicked(move |_| {
            if let Some(desk) = weak.upgrade() {
                desk.vote();
            }
        });
        let weak = Rc::downgrade(&desk);
        loading_off.connect_clicked(move |_| {
            if let Some(desk) = weak.upgrade() {
                let key = desk
                    .snapshot
                    .borrow()
                    .state
                    .as_ref()
                    .and_then(|state| state.next.as_ref().map(|next| next.key.clone()));
                if let Some(key) = key {
                    desk.remove(&key);
                }
            }
        });
        let asking = {
            let (weak, entry) = (Rc::downgrade(&desk), ask.downgrade());
            move || {
                if let (Some(desk), Some(entry)) = (weak.upgrade(), entry.upgrade()) {
                    let typed = entry.text().to_string();
                    if !typed.trim().is_empty() {
                        desk.ask(&typed);
                        entry.set_text("");
                    }
                }
            }
        };
        let again = asking.clone();
        ask.connect_activate(move |_| again());
        ask_go.connect_clicked(move |_| asking());
        let searching = {
            let (weak, entry) = (Rc::downgrade(&desk), search.downgrade());
            move || {
                if let (Some(desk), Some(entry)) = (weak.upgrade(), entry.upgrade()) {
                    desk.search_library(&entry.text());
                }
            }
        };
        let again = searching.clone();
        search.connect_activate(move |_| again());
        search_go.connect_clicked(move |_| searching());
        let (weak, model_weak) = (Rc::downgrade(&desk), model.downgrade());
        duck.connect_active_notify(move |switch| {
            if let (Some(desk), Some(model)) = (weak.upgrade(), model_weak.upgrade()) {
                if desk.ducks() != switch.is_active() {
                    desk.set_ducks(&model, switch.is_active());
                }
            }
        });
        let (weak, model_weak) = (Rc::downgrade(&desk), model.downgrade());
        volume.connect_value_changed(move |scale| {
            let (Some(desk), Some(model)) = (weak.upgrade(), model_weak.upgrade()) else {
                return;
            };
            let level = (scale.value() / 100.0) as f32;
            if desk.playing_here() {
                if (monitor_level(&model) - level).abs() > 0.005 {
                    desk.set_monitor_volume(&model, level);
                }
            } else if let Some(who) = playing_person(&model, &desk) {
                let (was, blocked) = desk.hearing_of(&who);
                if (was - level).abs() > 0.005 {
                    desk.set_hearing(&model, &who, level, blocked);
                }
            }
        });
        let (weak, model_weak) = (Rc::downgrade(&desk), model.downgrade());
        mute.connect_clicked(move |_| {
            let (Some(desk), Some(model)) = (weak.upgrade(), model_weak.upgrade()) else {
                return;
            };
            if let Some(who) = playing_person(&model, &desk) {
                let (level, blocked) = desk.hearing_of(&who);
                desk.set_hearing(&model, &who, level, !blocked);
            }
        });
    }

    // The song's place, twice a second while the panel is there.
    {
        let (desk, progress, clock) = (desk.clone(), progress.downgrade(), clock.downgrade());
        glib::timeout_add_local(Duration::from_millis(500), move || {
            let (Some(progress), Some(clock)) = (progress.upgrade(), clock.upgrade()) else {
                return glib::ControlFlow::Break;
            };
            if let Some((at, of)) = desk.as_ref().and_then(|desk| desk.progress()) {
                progress.set_fraction(if of > 0.0 {
                    (at / of).clamp(0.0, 1.0)
                } else {
                    0.0
                });
                set_text(
                    &clock,
                    &format!("{} / {}", music_clock(at), music_clock(of)),
                );
            }
            glib::ControlFlow::Continue
        });
    }

    let refresh = move |model: &AppModel| {
        let Some(desk) = desk.clone() else { return };
        let snapshot = desk.snapshot.borrow().clone();
        let state = snapshot.state.as_ref();
        let libraries = desk.libraries.borrow().is_some();
        tabs_slide.set_reveal_child(libraries);
        let tab = if libraries { desk.tab.get() } else { Tab::Desk };
        stack.set_visible_child_name(if tab == Tab::Search { "search" } else { "desk" });
        if tab == Tab::Search {
            desk_tab.remove_css_class("held");
            search_tab.add_css_class("held");
        } else {
            desk_tab.add_css_class("held");
            search_tab.remove_css_class("held");
        }
        desk_tab.set_label(&t("Song desk"));
        search_tab.set_label(&t("Search"));

        // What is playing: the desk's, or music from somebody whose desk is
        // not heard here (a sealed call), which still has a volume.
        let now_entry = state.and_then(|state| state.now.clone());
        let player = playing_person(model, &desk);
        let mine = snapshot.mine;
        if let Some(entry) = now_entry.as_ref() {
            set_text(&now_title, &entry.title);
            set_text(&now_artists, &entry.artists.join(", "));
            set_text(
                &now_by,
                &t_with("Asked for by {name}", &[("name", &entry.by)]),
            );
            let paused = state.is_some_and(|state| state.paused);
            set_text(
                &now_quality,
                &match (paused, state.and_then(|state| state.quality.clone())) {
                    (true, _) => t("Paused"),
                    (false, Some(quality)) => quality,
                    (false, None) => String::new(),
                },
            );
            pause.set_icon_name(if paused {
                "media-playback-start-symbolic"
            } else {
                "media-playback-pause-symbolic"
            });
            label_said(&pause, &if paused { t("Play") } else { t("Pause") });
        } else if let Some(who) = player.as_ref() {
            let name = desk.name_of(model, who);
            set_text(
                &now_title,
                &t_with(
                    "{name} (music)",
                    &[("name", &if name.is_empty() { who.clone() } else { name })],
                ),
            );
            set_text(&now_artists, "");
            set_text(&now_by, "");
            set_text(&now_quality, "");
        }
        now_by.set_visible(now_entry.is_some());
        now_slide.set_reveal_child(now_entry.is_some() || player.is_some());
        progress_slide.set_reveal_child(mine && desk.playing_here() && now_entry.is_some());
        holder_slide.set_reveal_child(mine && now_entry.is_some());
        vote_slide.set_reveal_child(!mine && now_entry.is_some());
        if let Some(state) = state {
            vote.set_label(&t_with(
                "Vote to skip ({votes}/{needed})",
                &[("votes", &state.votes.len()), ("needed", &state.needed)],
            ));
        }
        label_said(&next, &t("Next song"));
        label_said(&stop, &t("Stop the desk"));
        // The volume: the holder's own speakers, or the listener's level of
        // the person playing.
        let (level, blocked) = if desk.playing_here() {
            (monitor_level(model), false)
        } else {
            player
                .as_ref()
                .map(|who| desk.hearing_of(who))
                .unwrap_or((1.0, false))
        };
        set_text(
            &volume_label,
            &if desk.playing_here() {
                t("In your speakers")
            } else {
                t("Music volume in your speakers")
            },
        );
        let shown = f64::from(level) * 100.0;
        if (volume.value() - shown).abs() > 0.5 {
            volume.set_value(shown);
        }
        mute.set_visible(!desk.playing_here());
        mute.set_icon_name(if blocked {
            "audio-volume-muted-symbolic"
        } else {
            "audio-volume-high-symbolic"
        });
        volume.update_property(&[gtk::accessible::Property::Label(&t(
            "Music volume in your speakers",
        ))]);

        set_text(
            &idle,
            &if libraries {
                t("Search a song or paste a playlist link to start the song desk.")
            } else if state.is_some() {
                t("Nothing is playing. Ask for a song below.")
            } else {
                t("When somebody plays music in this call, it shows up here.")
            },
        );
        idle_slide.set_reveal_child(now_entry.is_none() && player.is_none());
        let next_entry = state.and_then(|state| state.next.clone());
        if let Some(next_entry) = next_entry.as_ref() {
            set_text(
                &loading,
                &t_with("Loading {title}", &[("title", &next_entry.title)]),
            );
            label_said(
                &loading_off,
                &t_with("Take {title} off", &[("title", &next_entry.title)]),
            );
        }
        spinner.set_spinning(next_entry.is_some());
        loading_off.set_visible(mine);
        loading_slide.set_reveal_child(next_entry.is_some());

        let entries: Vec<DeskEntry> = state.map(|state| state.queue.clone()).unwrap_or_default();
        set_text(&queue_heading, &t("Queue"));
        queue_heading_slide.set_reveal_child(!entries.is_empty());
        let keys: Vec<String> = entries.iter().map(|entry| entry.key.clone()).collect();
        let weak = Rc::downgrade(&desk);
        queue.show(
            &keys,
            |key| {
                let (row, title, artists) = song_row("", "");
                let up = icon("go-up-symbolic", "");
                let off = icon("window-close-symbolic", "");
                let (weak_up, weak_off, key_up, key_off) =
                    (weak.clone(), weak.clone(), key.to_owned(), key.to_owned());
                up.connect_clicked(move |_| {
                    if let Some(desk) = weak_up.upgrade() {
                        desk.to_top(&key_up);
                    }
                });
                off.connect_clicked(move |_| {
                    if let Some(desk) = weak_off.upgrade() {
                        desk.remove(&key_off);
                    }
                });
                row.append(&up);
                row.append(&off);
                (
                    row.upcast(),
                    QueueParts {
                        title,
                        artists,
                        up,
                        off,
                    },
                )
            },
            |index, key, parts| {
                if let Some(entry) = entries.iter().find(|entry| entry.key == key) {
                    set_text(&parts.title, &format!("{}  {}", index + 1, entry.title));
                    set_text(&parts.artists, &entry.artists.join(", "));
                    label_said(
                        &parts.up,
                        &t_with("Play {title} next", &[("title", &entry.title)]),
                    );
                    label_said(
                        &parts.off,
                        &t_with("Take {title} off", &[("title", &entry.title)]),
                    );
                }
                parts.up.set_visible(mine && index > 0);
                parts.off.set_visible(mine);
            },
        );

        set_text(&duck_words, &t("Lower the music while anybody talks"));
        if duck.is_active() != desk.ducks() {
            duck.set_active(desk.ducks());
        }
        ask.set_placeholder_text(Some(&t("A song's name, or a playlist link")));
        ask.update_property(&[gtk::accessible::Property::Label(&t("Ask for a song"))]);
        ask_go.set_label(&t("Ask for a song"));
        ask_go.set_sensitive(!desk.asking.get());

        // The library.
        search.set_placeholder_text(Some(&t("Search songs")));
        search_go.set_label(&t("Search"));
        search_go.set_sensitive(!desk.searching.get());
        searching.set_spinning(desk.searching.get());
        searching_slide.set_reveal_child(desk.searching.get());
        let problem = desk.search_problem.borrow().clone();
        if let Some(problem) = problem.as_ref() {
            set_text(&search_problem, problem);
        }
        search_problem_slide.set_reveal_child(problem.is_some());
        let found = desk.search_results.borrow().clone().unwrap_or_default();
        let library = desk.search_library_id().unwrap_or_default();
        let keys: Vec<String> = found.iter().map(|track| track.id.clone()).collect();
        let weak = Rc::downgrade(&desk);
        let added = desk.added.borrow().clone();
        results.show(
            &keys,
            |key| {
                let track = found.iter().find(|track| track.id == key).cloned();
                let (row, _, _) = song_row(
                    track
                        .as_ref()
                        .map(|track| track.title.as_str())
                        .unwrap_or_default(),
                    &track
                        .as_ref()
                        .map(|track| track.artists.join(", "))
                        .unwrap_or_default(),
                );
                let add = icon("list-add-symbolic", "");
                let (weak, library) = (weak.clone(), library.clone());
                add.connect_clicked(move |_| {
                    if let (Some(desk), Some(track)) = (weak.upgrade(), track.clone()) {
                        desk.added.borrow_mut().insert(track.id.clone());
                        desk.add(vec![song(track, &library)]);
                    }
                });
                row.append(&add);
                (row.upcast(), ResultParts { add })
            },
            |_, key, parts| {
                let done = added.contains(key);
                parts.add.set_icon_name(if done {
                    "object-select-symbolic"
                } else {
                    "list-add-symbolic"
                });
                parts.add.set_sensitive(!done);
                let title = found
                    .iter()
                    .find(|track| track.id == key)
                    .map(|track| track.title.clone())
                    .unwrap_or_default();
                label_said(
                    &parts.add,
                    &if done {
                        t("Added")
                    } else {
                        t_with("Add {title}", &[("title", &title)])
                    },
                );
            },
        );
        let _ = model.kept::<RoomState>();
    };
    PanelView {
        widget: outer.upcast(),
        refresh: Box::new(refresh),
    }
}

/// Whoever plays music in the call that this panel turns up or down: the
/// desk's holder where it is heard, or anybody with a music track.
fn playing_person(model: &AppModel, desk: &SongDesk) -> Option<String> {
    let room = model.kept::<RoomState>();
    let talk = room.talk.borrow();
    let talk = talk.as_ref()?;
    let holder = desk
        .snapshot
        .borrow()
        .state
        .as_ref()
        .map(|state| state.holder.clone());
    if let Some(holder) = holder {
        if talk
            .person(&holder)
            .is_some_and(|person| person.music_track().is_some())
        {
            return Some(holder);
        }
    }
    talk.people
        .iter()
        .find(|person| person.music_track().is_some())
        .map(|person| person.id.clone())
}
