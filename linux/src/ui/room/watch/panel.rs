//! The watch together panel (WatchPanel.tsx, WatchTab.tsx, MediaBrowser.tsx;
//! the Mac's WatchPanelView and MediaBrowserView): a link to paste, what is
//! playing and coming up, and the media servers where this person may
//! browse for something to add. Drawn as the room's other panels are
//! (panels.rs): opaque, the heading and the close button at the top.
//!
//! Every row comes and goes with a transition, and a row is kept for as long
//! as its video is in the show, rather than the list being emptied and
//! filled again on every word from the holder.

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use adw::prelude::*;
use gtk::pango;
use tomoshibi_protocol::api::{watch_media_link, MediaItem, MediaServer};

use super::{clock_text, engine, Engine, Info};
use crate::{
    app::{AppModel, WeakModel},
    i18n::{t, t_with},
    ui::room::{
        motion::{Away, Motion},
        panels::PanelView,
        Panel,
    },
};

/// One video's row, kept while the video is in the show.
struct Row {
    key: String,
    motion: Motion,
    title: gtk::Label,
    detail: gtk::Label,
    mark: gtk::Image,
    remove: gtk::Button,
}

/// What the media browser is showing: a server's libraries, a folder in
/// them, or what a search found.
#[derive(Default)]
struct Browse {
    server: Option<MediaServer>,
    /// The folders opened, from the libraries down, by id and name.
    trail: Vec<(String, String)>,
    items: Vec<MediaItem>,
    loading: bool,
    /// Which answer is wanted, so that a slow one for a folder already left
    /// does not fill the one opened since.
    asked: u64,
}

struct WatchPanel {
    model: WeakModel,
    outer: gtk::Box,
    tabs: gtk::Revealer,
    queue_tab: gtk::Button,
    media_tab: gtk::Button,
    pages: gtk::Stack,
    entry: gtk::Entry,
    add: gtk::Button,
    adding: gtk::Stack,
    busy: Cell<bool>,
    list: gtk::Box,
    rows: RefCell<Vec<Row>>,
    heading: Motion,
    empty: Motion,
    empty_words: gtk::Label,
    stop: gtk::Revealer,
    browse: RefCell<Browse>,
    browser_head: gtk::Box,
    browser_back: gtk::Button,
    browser_title: gtk::Label,
    search: gtk::SearchEntry,
    items: gtk::Box,
}

/// The panel, and how it is brought up to date.
pub fn panel(model: &AppModel) -> PanelView {
    let built = WatchPanel::build(model);
    let widget = built.outer.clone().upcast();
    if let Some(engine) = engine(model) {
        engine.load_servers();
        if std::mem::take(&mut engine.shared.borrow_mut().tab_media) {
            built.pages.set_visible_child_name("media");
        }
    }
    built.refresh(model);
    let kept = built.clone();
    PanelView {
        widget,
        refresh: Box::new(move |model| kept.refresh(model)),
    }
}

fn muted(text: &str) -> gtk::Label {
    let label = gtk::Label::builder()
        .label(text)
        .xalign(0.0)
        .wrap(true)
        .build();
    label.add_css_class("tmsb-muted");
    label
}

impl WatchPanel {
    fn build(model: &AppModel) -> Rc<WatchPanel> {
        let outer = gtk::Box::new(gtk::Orientation::Vertical, 0);
        outer.set_vexpand(true);
        outer.set_widget_name("panel.watch");
        let head = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        head.set_margin_start(16);
        head.set_margin_end(8);
        head.set_margin_top(8);
        head.set_margin_bottom(4);
        let heading = gtk::Label::builder()
            .label(t("Watch together"))
            .xalign(0.0)
            .hexpand(true)
            .build();
        heading.add_css_class("tmsb-panel-title");
        let close = gtk::Button::from_icon_name("window-close-symbolic");
        close.add_css_class("tmsb-close");
        close.set_tooltip_text(Some(&t("Close watch together")));
        close.update_property(&[gtk::accessible::Property::Label(&t("Close watch together"))]);
        close.set_widget_name("panel.close");
        let weak = model.downgrade();
        close.connect_clicked(move |_| {
            if let Some(model) = weak.upgrade() {
                crate::ui::room::toggle_panel(&model, Panel::Watch);
            }
        });
        head.append(&heading);
        head.append(&close);
        outer.append(&head);

        // The two tabs, only for somebody the server lists media servers
        // for, which is somebody given them.
        let tabs_row = gtk::Box::new(gtk::Orientation::Horizontal, 2);
        tabs_row.set_margin_start(12);
        tabs_row.set_margin_end(12);
        tabs_row.set_margin_bottom(6);
        tabs_row.add_css_class("tmsb-watch-tabs");
        let queue_tab = gtk::Button::with_label(&t("Queue"));
        queue_tab.add_css_class("tmsb-choice");
        queue_tab.set_hexpand(true);
        let media_tab = gtk::Button::with_label(&t("Media servers"));
        media_tab.add_css_class("tmsb-choice");
        media_tab.set_hexpand(true);
        media_tab.set_widget_name("watch.tab.media");
        tabs_row.append(&queue_tab);
        tabs_row.append(&media_tab);
        let tabs = gtk::Revealer::builder()
            .transition_type(gtk::RevealerTransitionType::SlideDown)
            .transition_duration(crate::theme::motion::SETTLE_MS)
            .reveal_child(false)
            .child(&tabs_row)
            .build();
        outer.append(&tabs);

        let pages = gtk::Stack::builder()
            .transition_type(gtk::StackTransitionType::Crossfade)
            .transition_duration(crate::theme::motion::SETTLE_MS)
            .vexpand(true)
            .build();

        // The queue: the field, what is playing and coming up, and stopping.
        let queue = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let field_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        field_row.set_margin_start(12);
        field_row.set_margin_end(12);
        field_row.set_margin_top(6);
        field_row.set_margin_bottom(10);
        let entry = gtk::Entry::builder()
            .placeholder_text(t("A video link, or the text it was shared in"))
            .hexpand(true)
            .max_length(2000)
            .input_purpose(gtk::InputPurpose::Url)
            .build();
        entry.add_css_class("tmsb-watch-field");
        entry.set_widget_name("watch.link");
        entry.update_property(&[gtk::accessible::Property::Label(&t("Video link"))]);
        let add = gtk::Button::from_icon_name("list-add-symbolic");
        add.add_css_class("tmsb-send");
        add.add_css_class("tmsb-watch-add");
        add.set_valign(gtk::Align::Center);
        add.set_tooltip_text(Some(&t("Add a video")));
        add.update_property(&[gtk::accessible::Property::Label(&t("Add a video"))]);
        add.set_widget_name("watch.add-link");
        let spinner = gtk::Spinner::builder().spinning(true).build();
        let adding = gtk::Stack::builder()
            .transition_type(gtk::StackTransitionType::Crossfade)
            .transition_duration(crate::theme::motion::SETTLE_MS)
            .valign(gtk::Align::Center)
            .build();
        adding.add_named(&add, Some("add"));
        adding.add_named(&spinner, Some("busy"));
        adding.set_visible_child_name("add");
        field_row.append(&entry);
        field_row.append(&adding);
        queue.append(&field_row);
        let rule = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        rule.add_css_class("tmsb-menu-rule");
        queue.append(&rule);
        let list = gtk::Box::new(gtk::Orientation::Vertical, 2);
        list.set_margin_start(8);
        list.set_margin_end(8);
        list.set_margin_top(8);
        list.set_margin_bottom(8);
        list.set_widget_name("watch.list");
        let queue_heading = gtk::Label::builder().label(t("Queue")).xalign(0.0).build();
        queue_heading.add_css_class("tmsb-menu-heading");
        let heading_motion = Motion::hidden(&queue_heading, Away::GROW);
        let empty_words = muted("");
        empty_words.set_justify(gtk::Justification::Center);
        empty_words.set_xalign(0.5);
        empty_words.set_margin_top(24);
        empty_words.set_margin_start(16);
        empty_words.set_margin_end(16);
        let empty = Motion::hidden(&empty_words, Away::GROW);
        list.append(&empty);
        let scroller = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vexpand(true)
            .child(&list)
            .build();
        queue.append(&scroller);
        let stop_button = gtk::Button::builder()
            .label(t("Stop watching together"))
            .halign(gtk::Align::End)
            .build();
        stop_button.add_css_class("tmsb-panel-button");
        stop_button.set_widget_name("watch.stop");
        let stop_row = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let stop_rule = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        stop_rule.add_css_class("tmsb-menu-rule");
        stop_row.append(&stop_rule);
        stop_button.set_margin_top(6);
        stop_button.set_margin_bottom(10);
        stop_button.set_margin_end(12);
        stop_row.append(&stop_button);
        let stop = gtk::Revealer::builder()
            .transition_type(gtk::RevealerTransitionType::SlideUp)
            .transition_duration(crate::theme::motion::SETTLE_MS)
            .reveal_child(false)
            .child(&stop_row)
            .build();
        queue.append(&stop);
        pages.add_named(&queue, Some("queue"));

        // The media servers.
        let media = gtk::Box::new(gtk::Orientation::Vertical, 6);
        media.set_margin_start(12);
        media.set_margin_end(12);
        media.set_margin_top(6);
        let browser_head = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        let browser_back = gtk::Button::from_icon_name("go-previous-symbolic");
        browser_back.add_css_class("tmsb-close");
        browser_back.set_tooltip_text(Some(&t("Back")));
        browser_back.update_property(&[gtk::accessible::Property::Label(&t("Back"))]);
        let browser_title = gtk::Label::builder()
            .xalign(0.0)
            .hexpand(true)
            .ellipsize(pango::EllipsizeMode::End)
            .build();
        browser_title.add_css_class("tmsb-panel-section");
        browser_head.append(&browser_back);
        browser_head.append(&browser_title);
        media.append(&browser_head);
        let search = gtk::SearchEntry::builder()
            .placeholder_text(t("Search films and series"))
            .build();
        media.append(&search);
        let items = gtk::Box::new(gtk::Orientation::Vertical, 2);
        let items_scroller = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vexpand(true)
            .child(&items)
            .build();
        media.append(&items_scroller);
        pages.add_named(&media, Some("media"));
        outer.append(&pages);

        let panel = Rc::new(WatchPanel {
            model: model.downgrade(),
            outer,
            tabs,
            queue_tab,
            media_tab,
            pages,
            entry,
            add,
            adding,
            busy: Cell::new(false),
            list,
            rows: RefCell::new(Vec::new()),
            heading: heading_motion,
            empty,
            empty_words,
            stop,
            browse: RefCell::new(Browse::default()),
            browser_head,
            browser_back,
            browser_title,
            search,
            items,
        });
        panel.connect(&stop_button);
        panel.show_browser();
        panel
    }

    fn connect(self: &Rc<Self>, stop_button: &gtk::Button) {
        let weak = Rc::downgrade(self);
        let add = move || {
            if let Some(panel) = weak.upgrade() {
                panel.add_link();
            }
        };
        let also = add.clone();
        self.add.connect_clicked(move |_| also());
        self.entry.connect_activate(move |_| add());
        let weak = self.model.clone();
        stop_button.connect_clicked(move |_| {
            if let Some(engine) = weak.upgrade().and_then(|model| engine(&model)) {
                engine.stop();
            }
        });
        let weak = Rc::downgrade(self);
        self.queue_tab.connect_clicked(move |_| {
            if let Some(panel) = weak.upgrade() {
                panel.pages.set_visible_child_name("queue");
                panel.mark_tabs();
            }
        });
        let weak = Rc::downgrade(self);
        self.media_tab.connect_clicked(move |_| {
            if let Some(panel) = weak.upgrade() {
                panel.pages.set_visible_child_name("media");
                panel.mark_tabs();
            }
        });
        let weak = Rc::downgrade(self);
        self.browser_back.connect_clicked(move |_| {
            if let Some(panel) = weak.upgrade() {
                panel.back();
            }
        });
        let weak = Rc::downgrade(self);
        self.search.connect_activate(move |search| {
            if let Some(panel) = weak.upgrade() {
                panel.search_for(search.text().trim());
            }
        });
    }

    fn mark_tabs(&self) {
        let media = self.pages.visible_child_name().as_deref() == Some("media");
        for (tab, on) in [(&self.queue_tab, !media), (&self.media_tab, media)] {
            if on {
                tab.add_css_class("held");
            } else {
                tab.remove_css_class("held");
            }
        }
    }

    fn engine(&self) -> Option<Rc<Engine>> {
        self.model.upgrade().and_then(|model| engine(&model))
    }

    /// Adds what is in the field; the field empties only for a video that
    /// went in.
    fn add_link(self: &Rc<Self>) {
        let text = self.entry.text().trim().to_owned();
        if text.is_empty() || self.busy.get() {
            return;
        }
        let Some(engine) = self.engine() else { return };
        self.set_busy(true);
        let weak = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            let added = engine.add(&text).await;
            if let Some(panel) = weak.upgrade() {
                if added {
                    panel.entry.set_text("");
                }
                panel.set_busy(false);
            }
        });
    }

    fn set_busy(&self, busy: bool) {
        self.busy.set(busy);
        self.adding
            .set_visible_child_name(if busy { "busy" } else { "add" });
        self.entry.set_sensitive(!busy);
    }

    fn refresh(self: &Rc<Self>, model: &AppModel) {
        let Some(engine) = engine(model) else { return };
        if engine.sealed {
            // The bar offers no button in a sealed call; should the panel be
            // reached anyway, it says why there is nothing.
            self.pages.set_visible(false);
            self.tabs.set_reveal_child(false);
            if self
                .outer
                .last_child()
                .and_downcast::<gtk::Label>()
                .is_none()
            {
                let said = muted(&t("Not available in an encrypted call"));
                said.set_margin_start(16);
                said.set_margin_top(16);
                self.outer.append(&said);
            }
            return;
        }
        let shared = engine.shared.borrow();
        let has_servers = !shared.servers.is_empty();
        self.tabs.set_reveal_child(has_servers);
        if !has_servers {
            self.pages.set_visible_child_name("queue");
        }
        self.mark_tabs();

        // Pasting starts the show here when nobody runs one and this person
        // may read links; anybody may add to a show that is running.
        let show = shared.show.clone();
        let can_add = show.is_some() || shared.ready;
        self.entry.set_sensitive(can_add && !self.busy.get());
        self.add.set_sensitive(can_add);
        let typed = !self.entry.text().trim().is_empty();
        if typed {
            self.add.add_css_class("ready");
        } else {
            self.add.remove_css_class("ready");
        }
        self.stop.set_reveal_child(shared.mine);
        self.empty_words.set_label(&if can_add {
            t("Paste a link and everybody watches it together, in step.")
        } else {
            t("When somebody signed in starts a video, it shows up here.")
        });
        let infos = shared.infos.clone();
        drop(shared);

        // What is playing, the heading, and what is coming up, in order.
        let mut wanted: Vec<(tomoshibi_protocol::watch::WatchVideo, bool)> = Vec::new();
        if let Some(show) = show.as_ref() {
            if let Some(now) = show.now.as_ref() {
                wanted.push((now.clone(), true));
            }
            wanted.extend(show.queue.iter().map(|video| (video.clone(), false)));
        }
        self.empty.show(wanted.is_empty());
        self.heading.show(
            show.as_ref()
                .is_some_and(|show| show.now.is_some() && !show.queue.is_empty()),
        );
        {
            let mut rows = self.rows.borrow_mut();
            rows.retain(|row| {
                if wanted.iter().any(|(video, _)| video.key == row.key) {
                    return true;
                }
                let (list, gone) = (self.list.downgrade(), row.motion.downgrade());
                row.motion.leave(move || {
                    if let (Some(list), Some(gone)) = (list.upgrade(), gone.upgrade()) {
                        list.remove(&gone);
                    }
                });
                false
            });
            for (video, _) in &wanted {
                if rows.iter().any(|row| row.key == video.key) {
                    continue;
                }
                let row = self.make_row(&video.key);
                self.list.append(&row.motion);
                row.motion.show(true);
                rows.push(row);
            }
        }
        // In the show's order, the heading after what is playing.
        let mut after: Option<gtk::Widget> = None;
        for (index, (video, playing)) in wanted.iter().enumerate() {
            let rows = self.rows.borrow();
            let Some(row) = rows.iter().find(|row| row.key == video.key) else {
                continue;
            };
            self.list.reorder_child_after(&row.motion, after.as_ref());
            after = Some(row.motion.clone().upcast());
            if index == 0 && *playing {
                if self.heading.parent().is_none() {
                    self.list.append(&self.heading);
                }
                self.list.reorder_child_after(&self.heading, after.as_ref());
                after = Some(self.heading.clone().upcast());
            }
            let info = infos.get(&video.ticket);
            let known = info.and_then(Info::video);
            let title = match (known, info) {
                (Some(known), _) => known.title.clone(),
                (None, Some(Info::Unavailable)) => t("That video cannot be played here."),
                (None, Some(Info::Failed)) => t("The video could not be read. Try again."),
                _ => {
                    engine.info(&video.ticket);
                    "\u{2026}".to_owned()
                }
            };
            if row.title.label() != title {
                row.title.set_label(&title);
            }
            let mut detail = Vec::new();
            if *playing {
                detail.push(t("Playing"));
            }
            detail.push(t_with("Added by {name}", &[("name", &video.by)]));
            if video.duration > 0.0 {
                detail.push(clock_text(video.duration));
            }
            let detail = detail.join(" \u{00B7} ");
            if row.detail.label() != detail {
                row.detail.set_label(&detail);
            }
            if *playing {
                row.motion.add_css_class("playing");
                row.detail.add_css_class("playing");
            } else {
                row.motion.remove_css_class("playing");
                row.detail.remove_css_class("playing");
            }
            row.mark.set_visible(*playing);
            row.remove.set_visible(!*playing);
            let label = t_with("Take {title} off", &[("title", &title)]);
            row.remove.set_tooltip_text(Some(&label));
            row.remove
                .update_property(&[gtk::accessible::Property::Label(&label)]);
        }
        if self.heading.parent().is_none() {
            self.list.append(&self.heading);
        }
        if self.browse.borrow().server.is_none() {
            self.show_browser();
        }
    }

    fn make_row(&self, key: &str) -> Row {
        let line = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        line.add_css_class("tmsb-watch-row");
        let mark = gtk::Image::from_icon_name("media-playback-start-symbolic");
        mark.add_css_class("tmsb-watch-playing-mark");
        mark.set_valign(gtk::Align::Center);
        // As wide as it is drawn: the symbol inside it asks to expand, and
        // a poster that took it grew to half the row.
        let poster = gtk::Box::builder()
            .width_request(56)
            .height_request(32)
            .hexpand(false)
            .valign(gtk::Align::Center)
            .build();
        poster.add_css_class("tmsb-watch-poster");
        let film = gtk::Image::from_icon_name("video-x-generic-symbolic");
        film.set_hexpand(true);
        film.set_halign(gtk::Align::Center);
        poster.append(&film);
        let poster_overlay = gtk::Overlay::builder().child(&poster).build();
        mark.set_halign(gtk::Align::Start);
        mark.set_valign(gtk::Align::End);
        poster_overlay.add_overlay(&mark);
        let words = gtk::Box::new(gtk::Orientation::Vertical, 2);
        words.set_hexpand(true);
        words.set_valign(gtk::Align::Center);
        let title = gtk::Label::builder()
            .xalign(0.0)
            .ellipsize(pango::EllipsizeMode::End)
            .single_line_mode(true)
            .build();
        title.add_css_class("tmsb-watch-row-title");
        let detail = gtk::Label::builder()
            .xalign(0.0)
            .ellipsize(pango::EllipsizeMode::End)
            .single_line_mode(true)
            .build();
        detail.add_css_class("tmsb-watch-row-detail");
        words.append(&title);
        words.append(&detail);
        let remove = gtk::Button::from_icon_name("window-close-symbolic");
        remove.add_css_class("tmsb-close");
        remove.set_valign(gtk::Align::Center);
        let (weak, key_owned) = (self.model.clone(), key.to_owned());
        remove.connect_clicked(move |_| {
            if let Some(engine) = weak.upgrade().and_then(|model| engine(&model)) {
                engine.remove(&key_owned);
            }
        });
        line.append(&poster_overlay);
        line.append(&words);
        line.append(&remove);
        let motion = Motion::hidden(&line, Away::GROW);
        motion.set_widget_name(&format!("watch.row:{key}"));
        Row {
            key: key.to_owned(),
            motion,
            title,
            detail,
            mark,
            remove,
        }
    }

    // ---- the media servers ------------------------------------------------

    /// What the browser shows now, drawn again.
    fn show_browser(self: &Rc<Self>) {
        let browse = self.browse.borrow();
        while let Some(child) = self.items.first_child() {
            self.items.remove(&child);
        }
        let servers = self
            .engine()
            .map(|engine| engine.shared.borrow().servers.clone())
            .unwrap_or_default();
        self.browser_back.set_visible(browse.server.is_some());
        self.search.set_visible(browse.server.is_some());
        let title = match (browse.server.as_ref(), browse.trail.last()) {
            (None, _) => t("Media servers"),
            (Some(server), None) => server.name.clone(),
            (Some(_), Some((_, name))) => name.clone(),
        };
        self.browser_title.set_label(&title);
        let _ = &self.browser_head;
        let add_row = |title: &str, detail: Option<String>, icon: &str| {
            let (row, _) = crate::ui::room::page::menu_row(title, detail.as_deref(), Some(icon));
            let motion = Motion::hidden(&row, Away::GROW);
            self.items.append(&motion);
            motion.show(true);
            row
        };
        match browse.server.clone() {
            None => {
                for server in servers {
                    let row = add_row(&server.name, None, "network-server-symbolic");
                    let weak = Rc::downgrade(self);
                    row.connect_clicked(move |_| {
                        if let Some(panel) = weak.upgrade() {
                            panel.open(server.clone(), None);
                        }
                    });
                }
            }
            Some(server) => {
                if browse.loading {
                    let spinner = gtk::Spinner::builder()
                        .spinning(true)
                        .margin_top(16)
                        .build();
                    self.items.append(&spinner);
                } else if browse.items.is_empty() {
                    self.items.append(&muted(&t("Nothing found.")));
                }
                for item in browse.items.clone() {
                    let detail = [
                        (item.year > 0).then(|| item.year.to_string()),
                        (item.duration > 0.0).then(|| clock_text(item.duration)),
                    ]
                    .into_iter()
                    .flatten()
                    .collect::<Vec<_>>()
                    .join(" \u{00B7} ");
                    let row = add_row(
                        &item.name,
                        (!detail.is_empty()).then_some(detail),
                        if item.folder {
                            "folder-symbolic"
                        } else {
                            "video-x-generic-symbolic"
                        },
                    );
                    let (weak, server) = (Rc::downgrade(self), server.clone());
                    row.connect_clicked(move |_| {
                        let Some(panel) = weak.upgrade() else { return };
                        if item.folder {
                            panel.open(server.clone(), Some((item.id.clone(), item.name.clone())));
                        } else if item.playable {
                            panel.choose(&server, &item);
                        }
                    });
                }
            }
        }
    }

    /// Opens a server's libraries, or a folder in them.
    fn open(self: &Rc<Self>, server: MediaServer, folder: Option<(String, String)>) {
        let asked = {
            let mut browse = self.browse.borrow_mut();
            if browse.server.as_ref().map(|known| &known.key) != Some(&server.key) {
                browse.trail.clear();
            }
            browse.server = Some(server.clone());
            if let Some(folder) = folder.clone() {
                browse.trail.push(folder);
            }
            browse.loading = true;
            browse.items.clear();
            browse.asked += 1;
            browse.asked
        };
        self.show_browser();
        let Some(engine) = self.engine() else { return };
        let parent = folder.map(|(id, _)| id);
        let weak = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            let found = engine
                .deployment
                .clone()
                .browse_media(server.key.clone(), parent)
                .await
                .unwrap_or_default();
            if let Some(panel) = weak.upgrade() {
                panel.fill(asked, found);
            }
        });
    }

    fn search_for(self: &Rc<Self>, words: &str) {
        let Some(server) = self.browse.borrow().server.clone() else {
            return;
        };
        if words.is_empty() {
            let last = self.browse.borrow_mut().trail.pop();
            self.open(server, last);
            return;
        }
        let asked = {
            let mut browse = self.browse.borrow_mut();
            browse.loading = true;
            browse.items.clear();
            browse.asked += 1;
            browse.asked
        };
        self.show_browser();
        let Some(engine) = self.engine() else { return };
        let (weak, words) = (Rc::downgrade(self), words.to_owned());
        glib::spawn_future_local(async move {
            let found = engine
                .deployment
                .clone()
                .search_media(server.key.clone(), words)
                .await
                .unwrap_or_default();
            if let Some(panel) = weak.upgrade() {
                panel.fill(asked, found);
            }
        });
    }

    fn fill(self: &Rc<Self>, asked: u64, found: Vec<MediaItem>) {
        {
            let mut browse = self.browse.borrow_mut();
            if browse.asked != asked {
                return;
            }
            browse.loading = false;
            browse.items = found;
        }
        self.show_browser();
    }

    fn back(self: &Rc<Self>) {
        let (server, parent) = {
            let mut browse = self.browse.borrow_mut();
            if browse.trail.pop().is_none() {
                browse.server = None;
                browse.items.clear();
                (None, None)
            } else {
                (browse.server.clone(), browse.trail.pop())
            }
        };
        match server {
            Some(server) => self.open(server, parent),
            None => self.show_browser(),
        }
    }

    /// Something to watch from a library: added as the web adds it, by its
    /// library link, and the queue shown with it.
    fn choose(self: &Rc<Self>, server: &MediaServer, item: &MediaItem) {
        let Some(engine) = self.engine() else { return };
        let link = watch_media_link(server.key.clone(), item.id.clone());
        self.pages.set_visible_child_name("queue");
        self.mark_tabs();
        self.set_busy(true);
        let weak = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            engine.add(&link).await;
            if let Some(panel) = weak.upgrade() {
                panel.set_busy(false);
            }
        });
    }
}
