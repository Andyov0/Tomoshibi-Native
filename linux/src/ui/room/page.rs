//! The call's screen: the pictures, and over them the row at the top (the
//! room's name and the signal), the bar at the foot, the bar's popovers,
//! the panel beside the call, the notices and the pager, all laid out as
//! the Mac's RoomView lays them out over its pictures.
//!
//! Nothing here holds the call's state. [`Page::refresh`] reads it all from
//! the room's state in the model (mod.rs `RoomState`) and brings every
//! widget up to date, whenever the room says it changed and whenever the
//! language does; each widget is touched only where its value changed, and
//! everything that comes or goes does so through a [`Motion`].

use std::{
    cell::{Cell, RefCell},
    collections::{HashMap, HashSet},
    rc::Rc,
};

use adw::prelude::*;
use gtk::{gdk, glib, graphene};
use tomoshibi_core::call::CallState;
use tomoshibi_protocol::{
    hands,
    identity::{impersonating, signature_of, NamedIdentity},
    quality::ConnectionGrade,
};

use super::{
    glyph::{self, Glyph},
    motion::{Away, Motion},
    panels,
    plan::{self, Size},
    stage::Stage,
    talk::{PictureKind, Toast, ToastKind, ME, MY_SHARE},
    tile::{Marks, Tile},
    watch, Panel, Pop, RoomState, CHANGED,
};
use crate::{
    app::{AppModel, Screen},
    i18n::{self, t, t_with},
    media::Media,
    theme,
    ui::{kit, Context},
};

/// Where the bar ends above the foot of the room: 20 below it (Plan.island
/// is the bar's 58, these 20 and the gap).
const BAR_FOOT: i32 = 20;
/// Narrower than this the bar keeps its panels behind "More", as the
/// Mac's compact bar does where the whole one does not fit.
const COMPACT_BELOW: i32 = 720;
/// How long a room is left alone before controls that may step aside do
/// (hooks/useIdle.ts `AFTER`).
const IDLE_AFTER_MS: u64 = 3000;

/// How a popover is brought up to date.
type Refresh = Box<dyn Fn(&AppModel)>;

/// The bar's buttons.
struct Bar {
    island: gtk::Box,
    microphone: gtk::Button,
    camera: gtk::Button,
    share: gtk::Button,
    hand: gtk::Button,
    reactions: gtk::Button,
    messages: gtk::Button,
    messages_dot: Motion,
    sound: gtk::Button,
    music: gtk::Button,
    music_dot: Motion,
    watch: gtk::Button,
    watch_dot: Motion,
    devices: gtk::Button,
    host: gtk::Button,
    host_count: gtk::Label,
    host_count_motion: Motion,
    more: gtk::Button,
    rule: gtk::Box,
    leave: gtk::Button,
}

struct Popover {
    motion: Motion,
    /// What it hangs from.
    anchor: gtk::Widget,
    /// Right-aligned to its button rather than centred on it (the leave
    /// confirmation, UI sync).
    end: bool,
}

pub struct Page {
    model: AppModel,
    window: glib::WeakRef<adw::ApplicationWindow>,
    root: gtk::Overlay,
    stage: Stage,
    tiles: RefCell<HashMap<String, Rc<Tile>>>,
    self_preview: RefCell<Option<(Rc<dyn crate::media::Camera>, u64)>>,
    title_motion: Motion,
    title: gtk::Label,
    share_link: gtk::Button,
    share_link_motion: Motion,
    signal: gtk::Button,
    signal_motion: Motion,
    signal_bars: gtk::DrawingArea,
    signal_figure: gtk::Label,
    signal_grade: Cell<Option<ConnectionGrade>>,
    notice_motion: Motion,
    notice: gtk::Label,
    problem_motion: Motion,
    watchers_motion: Motion,
    watchers: gtk::Label,
    problem: gtk::Label,
    alone_motion: Motion,
    alone_button: gtk::Button,
    alone_icon: gtk::Image,
    alone_label: gtk::Label,
    alone_sentence: gtk::Label,
    previous_motion: Motion,
    next_motion: Motion,
    toasts: gtk::Box,
    toast_widgets: RefCell<Vec<(u64, Motion)>>,
    said: gtk::Box,
    said_widgets: RefCell<Vec<(u64, Motion)>>,
    panel_motion: Motion,
    panel_holder: gtk::Box,
    panel_shown: Cell<Option<Panel>>,
    panel_view: RefCell<Option<panels::PanelView>>,
    bar_motion: Motion,
    bar: Bar,
    pops: HashMap<&'static str, Popover>,
    pop_content: RefCell<HashMap<&'static str, Refresh>>,
    last_press: RefCell<Option<String>>,
    idle_timer: RefCell<Option<glib::SourceId>>,
    /// Remote screens this page stopped at the server because they are not
    /// on the stage (ShareCard.tsx: a share in the grid is not subscribed
    /// to), so that each is let flow again when it goes there.
    held_back: RefCell<HashSet<String>>,
    /// The menu of one picture (PictureMenu.tsx, picture_menu.rs).
    picture_menu: super::picture_menu::PictureMenu,
    /// The bar stands down the side of the room rather than across its foot
    /// (the setting's "At the side").
    side: Cell<bool>,
    /// The bar is on its way from one edge to the other: nothing else shows
    /// it until it has arrived.
    moving_bar: Cell<bool>,
    /// The video watched together, while there is a show.
    watch_view: RefCell<Option<Rc<watch::StageView>>>,
}

/// The lane the controls take down the side of the room (Plan.swift
/// `sideLane`, the web's `mr-[5.75rem]`), and how tall a room has to be
/// before they fit there at all (`sideNeeds`, useRoomFor.ts).
const SIDE_LANE: i32 = 92;
const SIDE_NEEDS: i32 = 480;

fn icon_button(icon: &str, label: &str) -> gtk::Button {
    let button = gtk::Button::from_icon_name(icon);
    button.add_css_class("tmsb-bar-button");
    set_label(&button, label);
    button
}

fn glyph_button(shape: glyph::Shape, size: i32, label: &str) -> gtk::Button {
    let button = gtk::Button::new();
    button.set_child(Some(&Glyph::new(shape, size)));
    button.add_css_class("tmsb-bar-button");
    set_label(&button, label);
    button
}

/// A button of the bar that comes and goes with what it offers (sharing
/// where it can be done, running the room once the deployment says so, the
/// panels behind "More" in a narrow window): wrapped so that it slides in
/// and out and the capsule grows and shrinks with it, rather than either
/// jumping (the Mac animates the bar's own width for the same reason).
fn coming(widget: &impl IsA<gtk::Widget>) -> gtk::Revealer {
    let revealer = gtk::Revealer::builder()
        .transition_type(gtk::RevealerTransitionType::SlideRight)
        .transition_duration(crate::theme::motion::SETTLE_MS)
        .reveal_child(true)
        .child(widget)
        .build();
    // Gone once it has slid away, so that the bar's spacing does not keep
    // a gap for it.
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
        .ancestor(gtk::Revealer::static_type())
        .and_then(|found| found.downcast::<gtk::Revealer>().ok())
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
    // Without animations the notify above may not come.
    if !on && !adw::is_animations_enabled(&revealer) {
        revealer.set_visible(false);
    }
}

fn set_label(widget: &impl IsA<gtk::Widget>, label: &str) {
    let widget = widget.as_ref();
    if widget.tooltip_text().as_deref() != Some(label) {
        widget.set_tooltip_text(Some(label));
        widget.update_property(&[gtk::accessible::Property::Label(label)]);
    }
}

fn class(widget: &impl IsA<gtk::Widget>, name: &str, on: bool) {
    if on {
        widget.as_ref().add_css_class(name);
    } else {
        widget.as_ref().remove_css_class(name);
    }
}

/// A row of a menu: a title and, beneath it, what it does.
pub(super) fn menu_row(
    title: &str,
    detail: Option<&str>,
    icon: Option<&str>,
) -> (gtk::Button, gtk::Label) {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    if let Some(icon) = icon {
        let image = gtk::Image::from_icon_name(icon);
        image.set_valign(gtk::Align::Center);
        row.append(&image);
    }
    let words = gtk::Box::new(gtk::Orientation::Vertical, 1);
    words.set_valign(gtk::Align::Center);
    let heading = gtk::Label::builder().label(title).xalign(0.0).build();
    heading.add_css_class("title");
    words.append(&heading);
    if let Some(detail) = detail {
        let detail = gtk::Label::builder()
            .label(detail)
            .xalign(0.0)
            .wrap(true)
            .max_width_chars(34)
            .build();
        detail.add_css_class("detail");
        words.append(&detail);
    }
    row.append(&words);
    let button = gtk::Button::builder().child(&row).build();
    button.add_css_class("tmsb-menu-row");
    if detail.is_some() {
        button.add_css_class("tall");
    }
    (button, heading)
}

impl Page {
    pub fn build(context: &Context, _media: Rc<dyn Media>) -> Rc<Page> {
        let model = context.model.clone();
        let root = gtk::Overlay::builder().name("room").build();
        root.add_css_class("tmsb-room");
        let stage = Stage::new();
        stage.set_widget_name("room.stage");
        root.set_child(Some(&stage));
        super::veil::attach(&root, &model);
        let picture_menu = super::picture_menu::attach(&root, &stage, &model);

        // The window's own buttons, and where it is dragged from, over the
        // pictures: the call reaches the top of the window as the Mac's
        // hidden title bar lets it.
        let header = kit::bar();
        header.set_valign(gtk::Align::Start);
        root.add_overlay(&header);

        // The row at the top: the room's name and the signal.
        let title = gtk::Label::builder()
            .ellipsize(gtk::pango::EllipsizeMode::Middle)
            .max_width_chars(28)
            .build();
        title.add_css_class("tmsb-room-title");
        title.set_widget_name("room.title");
        let signal_bars = gtk::DrawingArea::builder()
            .content_width(14)
            .content_height(12)
            .can_target(false)
            .build();
        // The signal's bars and, with the setting's "With details", the
        // round trip beside them, which then opens the figures (the Mac's
        // SignalBadge); with "Signal only" the bars alone, pressing nothing.
        let signal_figure = gtk::Label::builder().visible(false).build();
        signal_figure.add_css_class("tmsb-signal-figure");
        let signal_inner = gtk::Box::new(gtk::Orientation::Horizontal, 7);
        signal_inner.set_halign(gtk::Align::Center);
        signal_inner.append(&signal_bars);
        signal_inner.append(&signal_figure);
        let signal = gtk::Button::builder().child(&signal_inner).build();
        signal.add_css_class("tmsb-signal");
        signal.set_widget_name("room.signal");
        // Only once connected, and only when the setting does not hide it:
        // it comes and goes rather than appearing between two frames.
        let signal_motion = Motion::hidden(&signal, Away::GROW);
        // The room's link from the row at the top, for as long as the call
        // lasts, rather than only from the capsule while alone in the room:
        // the link, or for whoever runs a room the link does not let people
        // into, an invitation (EmptyRoom.tsx `share`).
        let share_link = gtk::Button::builder()
            .child(&Glyph::new(glyph::LINK, 14))
            .build();
        share_link.add_css_class("tmsb-signal");
        share_link.add_css_class("round");
        share_link.set_widget_name("room.share-link");
        let share_link_motion = Motion::hidden(&share_link, Away::GROW);
        let title_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        title_row.append(&title);
        title_row.append(&share_link_motion);
        title_row.append(&signal_motion);
        let title_motion = Motion::new(&title_row, Away::TOP);
        title_motion.set_halign(gtk::Align::Start);
        title_motion.set_valign(gtk::Align::Start);
        title_motion.set_margin_start(14);
        title_motion.set_margin_top(12);
        root.add_overlay(&title_motion);

        // Over the stage: the connection while it is not there, and a press
        // that failed.
        let notice = gtk::Label::new(None);
        notice.add_css_class("tmsb-notice");
        let notice_motion = Motion::hidden(&notice, Away::TOP);
        notice_motion.set_halign(gtk::Align::Center);
        notice_motion.set_valign(gtk::Align::Start);
        notice_motion.set_margin_top(14);
        notice_motion.set_can_target(false);
        root.add_overlay(&notice_motion);
        let problem = gtk::Label::builder().wrap(true).build();
        problem.add_css_class("tmsb-notice");
        problem.add_css_class("danger");
        // Who has one's own share open, at the top right below the window's
        // buttons (Room.tsx: top-16 right-3; the Mac's WatchersCard).
        let watchers = gtk::Label::builder()
            .xalign(0.0)
            .wrap(true)
            .max_width_chars(28)
            .build();
        let watchers_card = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        watchers_card.add_css_class("tmsb-watchers");
        let eye = gtk::Image::from_icon_name("view-reveal-symbolic");
        eye.set_valign(gtk::Align::Start);
        watchers_card.append(&eye);
        watchers_card.append(&watchers);
        let watchers_motion = Motion::hidden(&watchers_card, Away::TOP);
        watchers_motion.set_halign(gtk::Align::End);
        watchers_motion.set_valign(gtk::Align::Start);
        watchers_motion.set_margin_top(56);
        watchers_motion.set_margin_end(14);
        watchers_motion.set_widget_name("room.watchers");
        root.add_overlay(&watchers_motion);
        let problem_motion = Motion::hidden(&problem, Away::TOP);
        problem_motion.set_halign(gtk::Align::Center);
        problem_motion.set_valign(gtk::Align::Start);
        problem_motion.set_margin_top(52);
        root.add_overlay(&problem_motion);

        // The pager's arrows, beside the pictures they turn.
        let previous = gtk::Button::from_icon_name("go-previous-symbolic");
        previous.add_css_class("tmsb-pager");
        set_label(&previous, &t("Previous page"));
        let previous_motion = Motion::hidden(&previous, Away::FADE);
        previous_motion.set_halign(gtk::Align::Start);
        previous_motion.set_valign(gtk::Align::Center);
        previous_motion.set_margin_start(8);
        root.add_overlay(&previous_motion);
        let next = gtk::Button::from_icon_name("go-next-symbolic");
        next.add_css_class("tmsb-pager");
        set_label(&next, &t("Next page"));
        let next_motion = Motion::hidden(&next, Away::FADE);
        next_motion.set_halign(gtk::Align::End);
        next_motion.set_valign(gtk::Align::Center);
        next_motion.set_margin_end(8);
        root.add_overlay(&next_motion);

        // Alone in the room: the link, as a small capsule above the bar.
        let alone_icon = gtk::Image::from_icon_name("edit-copy-symbolic");
        let alone_label = gtk::Label::new(None);
        let alone_inner = gtk::Box::new(gtk::Orientation::Horizontal, 7);
        alone_inner.append(&alone_icon);
        alone_inner.append(&alone_label);
        let alone_button = gtk::Button::builder().child(&alone_inner).build();
        alone_button.add_css_class("tmsb-alone-button");
        alone_button.set_widget_name("room.alone.link");
        let alone_sentence = gtk::Label::builder()
            .wrap(true)
            .max_width_chars(40)
            .margin_start(16)
            .margin_end(6)
            .build();
        alone_sentence.add_css_class("tmsb-muted");
        let alone_rule = gtk::Box::builder().valign(gtk::Align::Center).build();
        alone_rule.add_css_class("tmsb-alone-rule");
        let alone_close = gtk::Button::from_icon_name("window-close-symbolic");
        alone_close.add_css_class("tmsb-close");
        set_label(&alone_close, &t("Close"));
        let alone = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        alone.add_css_class("tmsb-alone");
        alone.append(&alone_button);
        alone.append(&alone_sentence);
        alone.append(&alone_rule);
        alone.append(&alone_close);
        let alone_motion = Motion::hidden(&alone, Away::FOOT);
        alone_motion.set_halign(gtk::Align::Center);
        alone_motion.set_valign(gtk::Align::End);
        alone_motion.set_margin_bottom(BAR_FOOT + 58 + 14);
        root.add_overlay(&alone_motion);

        // What was said while the messages were closed, bottom left, and the
        // notices, bottom right.
        let said = gtk::Box::new(gtk::Orientation::Vertical, 8);
        said.set_halign(gtk::Align::Start);
        said.set_valign(gtk::Align::End);
        said.set_margin_start(12);
        said.set_margin_bottom(plan::ISLAND as i32 + 8);
        said.set_can_target(false);
        root.add_overlay(&said);
        let toasts = gtk::Box::new(gtk::Orientation::Vertical, 10);
        toasts.set_halign(gtk::Align::End);
        toasts.set_valign(gtk::Align::End);
        toasts.set_margin_end(16);
        toasts.set_margin_bottom(plan::ISLAND as i32 + 8);
        toasts.set_widget_name("room.toasts");
        root.add_overlay(&toasts);

        // The panel beside the call.
        let panel_holder = gtk::Box::new(gtk::Orientation::Vertical, 0);
        panel_holder.add_css_class("tmsb-panel");
        panel_holder.set_overflow(gtk::Overflow::Hidden);
        let panel_motion = Motion::hidden(&panel_holder, Away::SIDE);
        panel_motion.set_halign(gtk::Align::End);
        panel_motion.set_valign(gtk::Align::Fill);
        // Below the window's own buttons, which are at the top right here
        // where the Mac's are at the top left.
        panel_motion.set_margin_top(52);
        panel_motion.set_margin_end(12);
        panel_motion.set_margin_bottom(plan::ISLAND as i32);
        panel_motion.set_widget_name("room.panel");
        root.add_overlay(&panel_motion);

        // The bar.
        let bar = Self::build_bar();
        let bar_motion = Motion::new(&bar.island, Away::FOOT);
        bar_motion.set_halign(gtk::Align::Center);
        bar_motion.set_valign(gtk::Align::End);
        bar_motion.set_margin_bottom(BAR_FOOT);
        bar_motion.set_widget_name("room.bar");
        root.add_overlay(&bar_motion);
        super::strain::attach(&root, &model);

        let window = glib::WeakRef::new();
        window.set(context.window().as_ref());

        // The popovers off the bar.
        let mut pops = HashMap::new();
        let pop_content: RefCell<HashMap<&'static str, Refresh>> = RefCell::new(HashMap::new());
        for (name, anchor, end) in [
            (
                "reactions",
                bar.reactions.clone().upcast::<gtk::Widget>(),
                false,
            ),
            ("leave", bar.leave.clone().upcast(), true),
            ("share", bar.share.clone().upcast(), false),
            ("more", bar.more.clone().upcast(), false),
            ("signal", signal.clone().upcast(), false),
        ] {
            let (card, refresh) = Self::build_pop(&model, name);
            card.add_css_class("tmsb-menu");
            let motion = Motion::hidden(
                &card,
                if name == "signal" {
                    Away::TOP
                } else {
                    Away::RISE
                },
            );
            motion.set_widget_name(&format!("pop:{name}"));
            root.add_overlay(&motion);
            pops.insert(
                name,
                Popover {
                    motion,
                    anchor,
                    end,
                },
            );
            pop_content.borrow_mut().insert(name, refresh);
        }

        let page = Rc::new(Page {
            model: model.clone(),
            window,
            root,
            stage,
            tiles: RefCell::new(HashMap::new()),
            self_preview: RefCell::new(None),
            title_motion,
            title,
            share_link,
            share_link_motion,
            signal,
            signal_motion,
            signal_bars,
            signal_figure,
            signal_grade: Cell::new(None),
            notice_motion,
            notice,
            problem_motion,
            watchers_motion,
            watchers,
            problem,
            alone_motion,
            alone_button,
            alone_icon,
            alone_label,
            alone_sentence,
            previous_motion,
            next_motion,
            toasts,
            toast_widgets: RefCell::new(Vec::new()),
            said,
            said_widgets: RefCell::new(Vec::new()),
            panel_motion,
            panel_holder,
            panel_shown: Cell::new(None),
            panel_view: RefCell::new(None),
            bar_motion,
            bar,
            pops,
            pop_content,
            last_press: RefCell::new(None),
            idle_timer: RefCell::new(None),
            held_back: RefCell::new(HashSet::new()),
            picture_menu,
            side: Cell::new(false),
            moving_bar: Cell::new(false),
            watch_view: RefCell::new(None),
        });
        page.connect(&previous, &next, &alone_close);
        page.refresh();
        page
    }

    pub fn widget(&self) -> gtk::Widget {
        self.root.clone().upcast()
    }

    fn build_bar() -> Bar {
        let island = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        island.add_css_class("tmsb-island");
        island.set_widget_name("room.island");
        let microphone = icon_button("audio-input-microphone-symbolic", "");
        microphone.set_widget_name("bar.microphone");
        let camera = icon_button("camera-video-symbolic", "");
        camera.set_widget_name("bar.camera");
        let share = icon_button("screen-shared-symbolic", "");
        share.set_widget_name("bar.share");
        let hand = glyph_button(glyph::HAND, 17, "");
        hand.set_widget_name("bar.hand");
        let reactions = icon_button("pan-up-symbolic", "");
        reactions.add_css_class("narrow");
        reactions.set_widget_name("bar.reactions");
        let hand_pair = gtk::Box::new(gtk::Orientation::Horizontal, 2);
        hand_pair.append(&hand);
        hand_pair.append(&reactions);
        let messages = glyph_button(glyph::MESSAGE, 17, "");
        messages.set_widget_name("bar.messages");
        let dot = gtk::Box::builder()
            .halign(gtk::Align::Center)
            .valign(gtk::Align::Center)
            .build();
        dot.add_css_class("tmsb-dot");
        let messages_dot = Motion::hidden(&dot, Away::GROW);
        messages_dot.set_halign(gtk::Align::End);
        messages_dot.set_valign(gtk::Align::Start);
        messages_dot.set_can_target(false);
        let messages_holder = gtk::Overlay::builder().child(&messages).build();
        messages_holder.add_overlay(&messages_dot);
        let sound = icon_button("audio-volume-high-symbolic", "");
        sound.set_widget_name("bar.sound");
        let music = icon_button("emblem-music-symbolic", "");
        music.set_widget_name("bar.music");
        let music_dot_shape = gtk::Box::builder()
            .halign(gtk::Align::Center)
            .valign(gtk::Align::Center)
            .build();
        music_dot_shape.add_css_class("tmsb-dot");
        let music_dot = Motion::hidden(&music_dot_shape, Away::GROW);
        music_dot.set_halign(gtk::Align::End);
        music_dot.set_valign(gtk::Align::Start);
        music_dot.set_can_target(false);
        let music_holder = gtk::Overlay::builder().child(&music).build();
        music_holder.add_overlay(&music_dot);
        // Watching together, beside the music and apart from it as the web
        // puts it (ControlBar.tsx): the one puts a picture on the stage, the
        // other plays into the sound. A dot while a video plays and its panel
        // is closed.
        let watch = glyph_button(glyph::CLAPPERBOARD, 17, "");
        watch.set_widget_name("bar.watch");
        let watch_spot = gtk::Box::builder()
            .halign(gtk::Align::Center)
            .valign(gtk::Align::Center)
            .build();
        watch_spot.add_css_class("tmsb-dot");
        let watch_dot = Motion::hidden(&watch_spot, Away::GROW);
        watch_dot.set_halign(gtk::Align::End);
        watch_dot.set_valign(gtk::Align::Start);
        watch_dot.set_can_target(false);
        let watch_holder = gtk::Overlay::builder().child(&watch).build();
        watch_holder.add_overlay(&watch_dot);
        let devices = icon_button("emblem-system-symbolic", "");
        devices.set_widget_name("bar.devices");
        let host = glyph_button(glyph::CROWN, 17, "");
        host.set_widget_name("bar.host");
        let host_count = gtk::Label::builder()
            .halign(gtk::Align::Center)
            .valign(gtk::Align::Center)
            .build();
        host_count.add_css_class("tmsb-count");
        let host_count_motion = Motion::hidden(&host_count, Away::GROW);
        host_count_motion.set_halign(gtk::Align::End);
        host_count_motion.set_valign(gtk::Align::Start);
        host_count_motion.set_can_target(false);
        let host_holder = gtk::Overlay::builder().child(&host).build();
        host_holder.add_overlay(&host_count_motion);
        let more = icon_button("view-more-horizontal-symbolic", "");
        more.set_widget_name("bar.more");
        let rule = gtk::Box::builder().valign(gtk::Align::Center).build();
        rule.add_css_class("tmsb-island-rule");
        let leave = icon_button("call-stop-symbolic", "");
        leave.add_css_class("leave");
        leave.set_widget_name("bar.leave");
        // The order is the web's and every app's (UI-SYNC decision 6).
        island.append(&microphone);
        island.append(&camera);
        island.append(&coming(&share));
        island.append(&hand_pair);
        island.append(&messages_holder);
        island.append(&coming(&sound));
        // Music beside the sound it is part of (ControlBar.tsx), shown once
        // there is something to show.
        island.append(&coming(&music_holder));
        let watch_coming = coming(&watch_holder);
        // Not offered until the deployment says this person may start a
        // show or one is running: shown at once, it was a button for nothing.
        watch_coming.set_reveal_child(false);
        watch_coming.set_visible(false);
        island.append(&watch_coming);
        island.append(&coming(&host_holder));
        island.append(&coming(&devices));
        island.append(&coming(&more));
        island.append(&coming(&rule));
        island.append(&leave);
        Bar {
            island,
            microphone,
            camera,
            share,
            hand,
            reactions,
            messages,
            messages_dot,
            sound,
            music,
            music_dot,
            watch,
            watch_dot,
            devices,
            host,
            host_count,
            host_count_motion,
            more,
            rule,
            leave,
        }
    }

    /// One of the bar's popovers, and how it is brought up to date.
    fn build_pop(model: &AppModel, name: &'static str) -> (gtk::Box, Refresh) {
        let card = gtk::Box::new(gtk::Orientation::Vertical, 2);
        let weak = model.downgrade();
        match name {
            "reactions" => {
                // The six a browser shows, in its order, from the protocol
                // crate (hands.rs `REACTIONS`).
                let row = gtk::Box::new(gtk::Orientation::Horizontal, 2);
                for what in hands::reaction_choices() {
                    let button = gtk::Button::with_label(&what);
                    button.add_css_class("tmsb-reaction");
                    button.set_widget_name(&format!("reaction:{what}"));
                    let weak = weak.clone();
                    button.connect_clicked(move |_| {
                        if let Some(model) = weak.upgrade() {
                            super::react(&model, &what);
                            super::close_pop(&model);
                        }
                    });
                    row.append(&button);
                }
                card.append(&row);
                card.set_widget_name("pop.reactions");
                (card, Box::new(|_| {}))
            }
            "leave" => {
                let (leave, _) = menu_row(
                    &t("Leave the call"),
                    Some(&t("The meeting carries on without you.")),
                    Some("application-exit-symbolic"),
                );
                leave.set_widget_name("leave.leave");
                let (end, _) = menu_row(
                    &t("End for everybody"),
                    Some(&t("Everybody is disconnected and links stop working.")),
                    Some("system-lock-screen-symbolic"),
                );
                end.add_css_class("danger");
                end.set_widget_name("leave.end");
                let w = weak.clone();
                leave.connect_clicked(move |button| {
                    button.set_sensitive(false);
                    let Some(model) = w.upgrade() else { return };
                    super::close_pop(&model);
                    let weak = model.downgrade();
                    glib::spawn_future_local(async move {
                        if let Some(model) = weak.upgrade() {
                            // A show this participant runs is closed, and
                            // the word that it closed sent, before the call
                            // goes: sent after, it never arrived.
                            watch::leaving(&model).await;
                            let media = super::media_of(&model);
                            let quiet = crate::media::Quiet;
                            let media: &dyn Media = match media.as_deref() {
                                Some(media) => media,
                                None => &quiet,
                            };
                            super::leave(&model, media).await;
                        }
                    });
                });
                let w = weak.clone();
                end.connect_clicked(move |_| {
                    if let Some(model) = w.upgrade() {
                        super::end_for_everybody(&model);
                    }
                });
                card.append(&leave);
                let end_motion = Motion::hidden(&end, Away::GROW);
                card.append(&end_motion);
                card.set_width_request(272);
                let (leave_row, end_row) = (leave.downgrade(), end.downgrade());
                (
                    card,
                    Box::new(move |model: &AppModel| {
                        let room = model.kept::<RoomState>();
                        let host = room.host.borrow();
                        end_motion.show(host.yours());
                        if let Some(end) = end_row.upgrade() {
                            end.set_sensitive(host.busy.as_deref() != Some("close"));
                        }
                        if let Some(leave) = leave_row.upgrade() {
                            leave.set_sensitive(room.call.borrow().is_some());
                        }
                    }),
                )
            }
            "share" => {
                let refresh = super::share_menu::build(model, &card);
                (card, refresh)
            }
            "more" => {
                let (sound, _) = menu_row(&t("Sound"), None, Some("audio-volume-high-symbolic"));
                let (music, _) = menu_row(&t("Music"), None, Some("emblem-music-symbolic"));
                let (devices, _) = menu_row(&t("Devices"), None, Some("emblem-system-symbolic"));
                let (host, _) =
                    menu_row(&t("Running this room"), None, Some("system-users-symbolic"));
                let (watch_row, _) =
                    menu_row(&t("Watch together"), None, Some("video-x-generic-symbolic"));
                watch_row.set_widget_name("more.watch");
                for (row, panel) in [
                    (&sound, Panel::Sound),
                    (&music, Panel::Music),
                    (&watch_row, Panel::Watch),
                    (&devices, Panel::Devices),
                    (&host, Panel::Host),
                ] {
                    let w = weak.clone();
                    row.connect_clicked(move |_| {
                        if let Some(model) = w.upgrade() {
                            super::toggle_panel(&model, panel);
                        }
                    });
                    card.append(row);
                }
                card.set_width_request(240);
                let host_row = host.downgrade();
                let music_row = music.downgrade();
                let watch_row = watch_row.downgrade();
                (
                    card,
                    Box::new(move |model: &AppModel| {
                        if let Some(host) = host_row.upgrade() {
                            host.set_visible(model.kept::<RoomState>().host.borrow().yours());
                        }
                        if let Some(music) = music_row.upgrade() {
                            let open =
                                model.kept::<RoomState>().view.borrow().panel == Some(Panel::Music);
                            music.set_visible(open || super::music::available(model));
                        }
                        if let Some(watch_row) = watch_row.upgrade() {
                            watch_row.set_visible(watch::offered(model));
                        }
                    }),
                )
            }
            _ => {
                // The signal's figures (signal.rs).
                let refresh = super::signal::figures(model, &card);
                (card, refresh)
            }
        }
    }

    /// The presses, the pointer and the keys.
    fn connect(
        self: &Rc<Self>,
        previous: &gtk::Button,
        next: &gtk::Button,
        alone_close: &gtk::Button,
    ) {
        let model = &self.model;
        let weak_model = model.downgrade();
        let on = |button: &gtk::Button, act: fn(&AppModel)| {
            let weak = weak_model.clone();
            button.connect_clicked(move |_| {
                if let Some(model) = weak.upgrade() {
                    act(&model);
                }
            });
        };
        on(&self.bar.microphone, |model| {
            let on = model.kept::<RoomState>().local.microphone_on.get();
            super::set_microphone(model, !on);
        });
        on(&self.bar.camera, |model| {
            let on = model.kept::<RoomState>().local.camera_on.get();
            super::set_camera(model, !on);
        });
        on(&self.bar.share, |model| {
            super::toggle_pop(model, Pop::Share)
        });
        on(&self.bar.hand, |model| {
            let up = model
                .kept::<RoomState>()
                .talk
                .borrow()
                .as_ref()
                .is_some_and(|talk| talk.hand_up);
            super::set_hand(model, !up);
        });
        on(&self.bar.reactions, |model| {
            super::toggle_pop(model, Pop::Reactions)
        });
        on(&self.bar.messages, |model| {
            super::toggle_panel(model, Panel::Messages)
        });
        on(&self.bar.sound, |model| {
            super::toggle_panel(model, Panel::Sound)
        });
        on(&self.bar.watch, |model| {
            super::toggle_panel(model, Panel::Watch)
        });
        on(&self.bar.devices, |model| {
            super::toggle_panel(model, Panel::Devices)
        });
        on(&self.bar.music, |model| {
            super::toggle_panel(model, Panel::Music)
        });
        on(&self.bar.host, |model| {
            super::toggle_panel(model, Panel::Host)
        });
        on(&self.bar.more, |model| super::toggle_pop(model, Pop::More));
        on(&self.bar.leave, |model| {
            super::toggle_pop(model, Pop::Leave)
        });
        // The figures only with the setting's "With details"; the bars alone
        // say what they say and open nothing (the Mac's SignalBadge).
        on(&self.signal, |model| {
            if model.preferences().string(theme::BADGE_KEY).as_deref() == Some("detailed") {
                super::toggle_pop(model, Pop::Signal);
            }
        });
        on(&self.share_link, |model| {
            if super::link_works(model) {
                super::copy_link(model);
            } else {
                super::invite(model);
            }
        });
        on(previous, |model| super::turn_page(model, -1));
        on(next, |model| super::turn_page(model, 1));
        on(&self.alone_button, |model| {
            if super::link_works(model) {
                super::copy_link(model);
            } else {
                super::invite(model);
            }
        });
        on(alone_close, |model| {
            model.kept::<RoomState>().view.borrow_mut().alone_dismissed = true;
            super::changed(model);
        });

        // A press that failed goes when it is pressed.
        let click = gtk::GestureClick::new();
        let weak = weak_model.clone();
        click.connect_released(move |_, _, _, _| {
            if let Some(model) = weak.upgrade() {
                model.kept::<RoomState>().view.borrow_mut().problem = None;
                super::changed(&model);
            }
        });
        self.problem_motion.add_controller(click);

        // A press on a picture puts it on the stage or takes it off; a
        // second within the double-press interval fills the screen with the
        // one the first press was on, wherever it has moved since.
        let presses = gtk::GestureClick::new();
        presses.set_button(gdk::BUTTON_PRIMARY);
        let page = Rc::downgrade(self);
        presses.connect_pressed(move |_, count, x, y| {
            let Some(page) = page.upgrade() else { return };
            let model = page.model.clone();
            page.poke();
            if count == 1 {
                let id = page.stage.tile_at(x, y);
                page.last_press.replace(id.clone());
                if let Some(id) = id {
                    super::press_picture(&model, &id);
                }
            } else if count == 2 {
                let first = page.last_press.borrow().clone();
                if let Some(id) = first {
                    super::expand_picture(&model, &id);
                }
            }
        });
        self.stage.add_controller(presses);

        // A press outside an open popover closes it and goes no further, as
        // the Mac's PopoverCatcher takes it: drawn over the pictures, the
        // press that closed the reactions would otherwise also pin a tile.
        let outside = gtk::GestureClick::new();
        outside.set_propagation_phase(gtk::PropagationPhase::Capture);
        let page = Rc::downgrade(self);
        outside.connect_pressed(move |gesture, _, x, y| {
            let Some(page) = page.upgrade() else { return };
            page.poke();
            let point = graphene::Point::new(x as f32, y as f32);
            let inside = |widget: &gtk::Widget| {
                widget
                    .compute_bounds(&page.root)
                    .is_some_and(|bounds| bounds.contains_point(&point))
            };
            let open = page.model.kept::<RoomState>().view.borrow().pop;
            let Some(open) = open else { return };
            let Some(pop) = page.pops.get(pop_name(open)) else {
                return;
            };
            if inside(pop.motion.upcast_ref()) || inside(&pop.anchor) {
                return;
            }
            super::close_pop(&page.model);
            gesture.set_state(gtk::EventSequenceState::Claimed);
        });
        self.root.add_controller(outside);

        // The pointer anywhere brings the controls back.
        let motion = gtk::EventControllerMotion::new();
        let page = Rc::downgrade(self);
        motion.connect_motion(move |_, _, _| {
            if let Some(page) = page.upgrade() {
                page.poke();
            }
        });
        self.root.add_controller(motion);

        // Escape: an open popover first, then the filled stage, as a
        // browser's full screen leaves on it.
        if let Some(window) = self.window.upgrade() {
            let keys = gtk::EventControllerKey::new();
            keys.set_propagation_phase(gtk::PropagationPhase::Capture);
            let weak = weak_model.clone();
            let page = Rc::downgrade(self);
            keys.connect_key_pressed(move |_, key, _, _| {
                let Some(model) = weak.upgrade() else {
                    return glib::Propagation::Proceed;
                };
                if key != gdk::Key::Escape || model.screen() != Screen::Call {
                    return glib::Propagation::Proceed;
                }
                if page.upgrade().is_some_and(|page| page.picture_menu.close()) {
                    return glib::Propagation::Stop;
                }
                if super::close_pop(&model) {
                    return glib::Propagation::Stop;
                }
                if model.kept::<RoomState>().view.borrow().filled {
                    super::unfill(&model);
                    return glib::Propagation::Stop;
                }
                glib::Propagation::Proceed
            });
            window.add_controller(keys.clone());
            let window = window.downgrade();
            self.root.connect_destroy(move |_| {
                if let Some(window) = window.upgrade() {
                    window.remove_controller(&keys);
                }
            });
        }

        // The popovers hang from their buttons.
        let page = Rc::downgrade(self);
        self.root.connect_get_child_position(move |overlay, child| {
            let page = page.upgrade()?;
            let name = child.widget_name();
            let name = name.strip_prefix("pop:")?;
            let (_, natural) = child.preferred_size();
            let (width, height) = (natural.width(), natural.height());
            let (whole, tall) = (overlay.width(), overlay.height());
            let pop = page.pops.get(name)?;
            let anchor = pop.anchor.compute_bounds(overlay)?;
            // Beside the bar's buttons where the bar stands down the side,
            // centred on the one pressed.
            if page.side.get() && name != "signal" {
                let x = (anchor.x().round() as i32 - width - 12).max(8);
                let y = (anchor.y() + anchor.height() / 2.0 - height as f32 / 2.0).round() as i32;
                let y = y.clamp(8, (tall - height - 8).max(8));
                return Some(gdk::Rectangle::new(x, y, width, height));
            }
            let x = if pop.end {
                anchor.x() + anchor.width() - width as f32
            } else {
                anchor.x() + anchor.width() / 2.0 - width as f32 / 2.0
            };
            let x = (x.round() as i32).clamp(8, (whole - width - 8).max(8));
            // Above the bar's buttons; below the signal, which is at the top.
            let y = if name == "signal" {
                (anchor.y() + anchor.height()).round() as i32 + 8
            } else {
                (anchor.y().round() as i32 - height - 12).max(8)
            };
            Some(gdk::Rectangle::new(x, y, width, height))
        });

        // The stage asks where the tiles go at its size.
        let page = Rc::downgrade(self);
        self.stage.set_planner(move |width, height| {
            page.upgrade()
                .map(|page| page.plan(width, height).spots)
                .unwrap_or_default()
        });
        let page = Rc::downgrade(self);
        self.stage.connect_resized(move || {
            if let Some(page) = page.upgrade() {
                page.refresh();
            }
        });

        // The signal's bars, coloured by the grade.
        let page = Rc::downgrade(self);
        self.signal_bars
            .set_draw_func(move |area, cr, width, height| {
                let Some(page) = page.upgrade() else { return };
                let grade = page.signal_grade.get();
                let lit = match grade {
                    Some(ConnectionGrade::Good) | None => 3,
                    Some(ConnectionGrade::Fair) => 2,
                    Some(ConnectionGrade::Poor) => 1,
                    Some(ConnectionGrade::Lost) => 0,
                };
                let palette = theme::palette(theme::Scheme::Dark);
                let colour = match grade {
                    Some(ConnectionGrade::Good) | None => palette.good,
                    Some(ConnectionGrade::Fair) => theme::accent().color(theme::Scheme::Dark),
                    _ => palette.danger,
                };
                let unlit = palette.muted.with_alpha(0.35);
                let _ = area;
                let bar_w = width as f64 / 4.0;
                for index in 0..3 {
                    let bar_h = height as f64 * (index as f64 + 1.0) / 3.0;
                    let x = index as f64 * (bar_w + bar_w / 2.0);
                    let shade = if index < lit { colour } else { unlit };
                    cr.set_source_rgba(
                        shade.red as f64,
                        shade.green as f64,
                        shade.blue as f64,
                        shade.alpha as f64,
                    );
                    cr.rectangle(x, height as f64 - bar_h, bar_w, bar_h);
                    let _ = cr.fill();
                }
            });

        // The words follow the language without the call being built again.
        let page = Rc::downgrade(self);
        i18n::on_change_while(&self.root, move |_| {
            if let Some(page) = page.upgrade() {
                page.reword();
                page.refresh();
            }
        });

        let page = Rc::downgrade(self);
        model.subscribe_while(&self.root, move |_, change| {
            if change == CHANGED {
                if let Some(page) = page.upgrade() {
                    page.refresh();
                }
            }
        });

        // A debug run's TOMOSHIBI_DEBUG_PANEL and TOMOSHIBI_DEBUG_POPOVER
        // open one on arrival, as the Mac's do.
        #[cfg(debug_assertions)]
        {
            let room = model.kept::<RoomState>();
            if let Some(panel) = std::env::var("TOMOSHIBI_DEBUG_PANEL")
                .ok()
                .and_then(|name| Panel::of(&name))
            {
                room.view.borrow_mut().panel = Some(panel);
            }
            if let Some(pop) = std::env::var("TOMOSHIBI_DEBUG_POPOVER")
                .ok()
                .and_then(|name| Pop::of(&name))
            {
                room.view.borrow_mut().pop = Some(pop);
            }
        }
    }

    /// What says a phrase once, built: the popovers' and the panels'.
    fn reword(&self) {
        self.panel_shown.set(None);
        for (name, pop) in &self.pops {
            let (card, refresh) = Self::build_pop(&self.model, name);
            card.add_css_class("tmsb-menu");
            // The popover keeps its place in the overlay; only what is in it
            // is made again in the new language.
            if let Some(old) = pop.motion.child() {
                if let Some(old) = old.downcast_ref::<gtk::Box>() {
                    while let Some(child) = old.first_child() {
                        old.remove(&child);
                    }
                    while let Some(child) = card.first_child() {
                        card.remove(&child);
                        old.append(&child);
                    }
                    old.set_width_request(card.width_request());
                }
            }
            self.pop_content.borrow_mut().insert(name, refresh);
        }
        for tile in self.tiles.borrow().values() {
            tile.reword();
        }
        if let Some(view) = self.watch_view.borrow().as_ref() {
            view.reword();
        }
        self.picture_menu.close();
    }

    /// The controls come back on any sign of somebody, and step aside again
    /// after a while where they may (ControlBar.tsx `hiding`).
    fn poke(self: &Rc<Self>) {
        let room = self.model.kept::<RoomState>();
        let was_idle = std::mem::replace(&mut room.view.borrow_mut().idle, false);
        if let Some(timer) = self.idle_timer.take() {
            timer.remove();
        }
        self.arm_idle();
        if was_idle {
            self.refresh();
        }
    }

    /// Counts the room's stillness from now.
    fn arm_idle(self: &Rc<Self>) {
        let page = Rc::downgrade(self);
        let timer = glib::timeout_add_local_once(
            std::time::Duration::from_millis(IDLE_AFTER_MS),
            move || {
                if let Some(page) = page.upgrade() {
                    page.idle_timer.replace(None);
                    page.model.kept::<RoomState>().view.borrow_mut().idle = true;
                    page.refresh();
                }
            },
        );
        self.idle_timer.replace(Some(timer));
    }

    /// Every tile and what it shows, one a line, for a test to read: the
    /// proof that a remote picture is on screen is a tile with frames drawn.
    pub fn report(&self) -> String {
        let mut ids: Vec<String> = self.tiles.borrow().keys().cloned().collect();
        ids.sort();
        let mut out = String::new();
        for id in ids {
            let tiles = self.tiles.borrow();
            let tile = &tiles[&id];
            let marks = tile.marks();
            let spot = self.stage.spot(&id).unwrap_or_default();
            out.push_str(&format!(
                "tile {id} \"{}\" at {:.0},{:.0} {:.0}x{:.0}: track {:?}, picture {}, frames drawn {}, frames arrived {}, hand {}, speaking {}, muted {}\n",
                marks.label,
                spot.x,
                spot.y,
                spot.width,
                spot.height,
                tile.attached.borrow(),
                tile.video().has_picture(),
                tile.video().frames_drawn(),
                tile.video().slot().arrived(),
                tile.hand_shown(),
                tile.speaking(),
                marks.muted,
            ));
            let (buttons, switch) = tile.controls_shown();
            out.push_str(&format!(
                "tile {id} stage buttons {buttons}, their other picture {switch}, share card {}\n",
                marks.share_card
            ));
        }
        if let Some(view) = self.watch_view.borrow().as_ref() {
            let spot = self.stage.spot(watch::STAGE_ID).unwrap_or_default();
            out.push_str(&format!(
                "watch at {:.0},{:.0} {:.0}x{:.0}: frames drawn {}\n",
                spot.x,
                spot.y,
                spot.width,
                spot.height,
                view.frames_drawn()
            ));
        }
        out
    }

    /// Shows the stage's buttons on every picture that has them, as the
    /// pointer over it would, for a run that has no pointer.
    #[cfg(debug_assertions)]
    pub fn hover_for_test(&self) {
        for tile in self.tiles.borrow().values() {
            tile.hover_for_test(true);
        }
        if let Some(view) = self.watch_view.borrow().as_ref() {
            view.hover_for_test();
        }
    }

    /// The keyboard's focus on the first picture (`tab`: moved on as Tab
    /// moves it), with the ring a keyboard's focus shows, and where it is
    /// written to the log.
    #[cfg(debug_assertions)]
    pub fn focus_for_test(&self, tab: bool) {
        let Some(window) = self.window.upgrade() else {
            return;
        };
        window.set_focus_visible(true);
        if tab {
            window.child_focus(gtk::DirectionType::TabForward);
        } else {
            let first = self
                .stage
                .ids()
                .into_iter()
                .filter_map(|id| self.tiles.borrow().get(&id).cloned())
                .next();
            if let Some(tile) = first {
                tile.root.grab_focus();
            }
        }
        let focused = gtk::prelude::GtkWindowExt::focus(&window)
            .map(|widget| format!("{} {}", widget.type_().name(), widget.widget_name()))
            .unwrap_or_else(|| "nothing".into());
        crate::app::log(&format!("room script: focus on {focused}"));
    }

    /// Opens the first person's camera picture's menu, in its middle, as a
    /// right press would, for a run that has no pointer.
    #[cfg(debug_assertions)]
    pub fn menu_for_test(self: &Rc<Self>) {
        let id = self
            .model
            .kept::<RoomState>()
            .talk
            .borrow()
            .as_ref()
            .and_then(|talk| {
                talk.people
                    .first()
                    .map(|person| format!("{}/camera", person.id))
            });
        let Some(id) = id else { return };
        let Some(spot) = self.stage.spot(&id) else {
            return;
        };
        let at = self
            .stage
            .compute_point(
                &self.root,
                &graphene::Point::new(
                    (spot.x + spot.width / 2.0) as f32,
                    (spot.y + spot.height / 2.0) as f32,
                ),
            )
            .map(|point| (point.x() as f64, point.y() as f64))
            .unwrap_or((spot.x, spot.y));
        self.picture_menu.open(&id, at.0, at.1);
    }

    pub fn fullscreen(&self, on: bool) {
        if let Some(window) = self.window.upgrade() {
            if on {
                window.fullscreen();
            } else {
                window.unfullscreen();
            }
        }
    }

    /// Every picture, in the order they are drawn: the video watched
    /// together first while there is a show (the web lists it as a picture,
    /// surface.ts `surfaces`, and so does the Mac), then the call's own.
    fn pictures(&self) -> Vec<super::talk::Picture> {
        let room = self.model.kept::<RoomState>();
        let talk = room.talk.borrow();
        let Some(talk) = talk.as_ref() else {
            return Vec::new();
        };
        let mut pictures = Vec::new();
        if watch::on_stage(&self.model) {
            pictures.push(super::talk::Picture {
                id: watch::STAGE_ID.to_owned(),
                identity: String::new(),
                kind: PictureKind::Screen,
                sid: None,
                mine: false,
            });
        }
        pictures.extend(talk.pictures(
            room.local.camera_on.get(),
            room.local.share.borrow().is_some(),
        ));
        pictures
    }

    /// The controls' reserve at the room's foot: the bar's lane there, or
    /// only the gap where it stands down the side.
    fn island(&self) -> f64 {
        if self.side.get() {
            plan::GAP
        } else {
            plan::ISLAND
        }
    }

    /// Where every picture goes at a size, from the state as it is.
    fn plan(&self, width: f64, height: f64) -> plan::Room {
        let room = self.model.kept::<RoomState>();
        if room.talk.borrow().is_none() {
            return plan::Room::default();
        }
        let view = room.view.borrow();
        let ids: Vec<String> = self
            .pictures()
            .into_iter()
            .map(|picture| picture.id)
            .collect();
        plan::room(
            Size::new(width, height),
            &ids,
            Some(ME),
            view.pin.pinned(),
            view.filled,
            view.page,
            self.island(),
        )
    }

    /// A reaction rising off `from`'s camera picture (`talk::ME` for one's
    /// own).
    pub fn float(&self, from: &str, what: &str) {
        let me = self
            .model
            .kept::<RoomState>()
            .talk
            .borrow()
            .as_ref()
            .map(|talk| talk.local_identity.clone());
        let id = if from == ME || Some(from) == me.as_deref() {
            ME.to_owned()
        } else {
            format!("{from}/camera")
        };
        if let Some(tile) = self.tiles.borrow().get(&id) {
            tile.float(what);
        }
    }

    /// Brings everything on screen up to date with the room's state.
    pub fn refresh(self: &Rc<Self>) {
        let model = self.model.clone();
        let room = model.kept::<RoomState>();
        let width = self.root.width();
        let compact = width > 0 && width < COMPACT_BELOW;

        // The stage first: who has it, as the pictures now are.
        let joined = room.talk.borrow().is_some();
        // Where the bar stands, before anything is placed around it.
        self.place_bar(compact);
        if joined {
            let pictures = self.pictures();
            let ids: Vec<String> = pictures.iter().map(|picture| picture.id.clone()).collect();
            // The video watched together takes the stage when a show starts,
            // before anybody's screen, as on the Mac (RoomView `shares`).
            let mut shares: Vec<String> = Vec::new();
            if watch::on_stage(&model) {
                shares.push(watch::STAGE_ID.to_owned());
            }
            shares.extend(
                room.talk
                    .borrow()
                    .as_ref()
                    .map(|talk| talk.shares())
                    .unwrap_or_default(),
            );
            let unfill = {
                let mut view = room.view.borrow_mut();
                view.pin.update(&ids, &shares);
                // Taking the last picture off the stage leaves nothing to
                // fill the screen with (Room.tsx `exit`).
                view.pin.pinned().is_none() && view.filled
            };
            if unfill {
                let weak = model.downgrade();
                glib::idle_add_local_once(move || {
                    if let Some(model) = weak.upgrade() {
                        super::unfill(&model);
                    }
                });
            }
            self.place_tiles(&pictures);
            super::write_watching(&model);
        }

        self.refresh_bar(compact);
        self.refresh_chrome(compact);
        self.refresh_panel(width);
        self.refresh_pops();
        self.refresh_notices();
    }

    fn place_tiles(self: &Rc<Self>, pictures: &[super::talk::Picture]) {
        let model = &self.model;
        let room = model.kept::<RoomState>();
        let (width, height) = self.stage.size();
        if width <= 0.0 || height <= 0.0 {
            return;
        }
        let planned = self.plan(width, height);
        {
            let mut view = room.view.borrow_mut();
            view.page = planned.page;
        }
        let call = super::call_of(model);
        let talk = room.talk.borrow();
        let Some(talk) = talk.as_ref() else { return };
        let view = room.view.borrow();
        let pinned = view.pin.pinned().map(str::to_owned);
        let two = pinned.is_none() && pictures.len() <= plan::ALONE_TOGETHER && pictures.len() == 2;
        let island = self.island();

        // Tiles for the pictures drawn, and none for the rest.
        let roster: Vec<NamedIdentity> = std::iter::once(NamedIdentity {
            name: talk.local_name.clone(),
            identity: talk.local_identity.clone(),
        })
        .chain(talk.people.iter().map(|person| NamedIdentity {
            name: person.name.clone(),
            identity: person.id.clone(),
        }))
        .collect();
        let gone: Vec<String> = self
            .tiles
            .borrow()
            .keys()
            .filter(|id| !planned.spots.contains_key(*id))
            .cloned()
            .collect();
        for id in gone {
            if let Some(tile) = self.tiles.borrow_mut().remove(&id) {
                self.let_go(&tile, call.as_ref());
                self.stage.remove(&id);
            }
        }
        // A track this page held back that has gone is forgotten with it.
        let live: HashSet<String> = pictures
            .iter()
            .filter_map(|picture| picture.sid.clone())
            .collect();
        self.held_back.borrow_mut().retain(|sid| live.contains(sid));

        // What reaches across the top of the room: the row with the room's
        // name and the signal at the left, the window's own buttons at the
        // right, and an open panel down the right; the stage's buttons keep
        // below and beside them, as the web's keep below the host's console
        // (StageControls.tsx `group-data-[hosting]`).
        let furniture: Vec<graphene::Rect> = [
            self.title_motion
                .compute_bounds(&self.stage)
                .filter(|_| self.title_motion.is_shown()),
            self.window_buttons(),
        ]
        .into_iter()
        .flatten()
        .collect();
        let panel_left = self
            .panel_motion
            .compute_bounds(&self.stage)
            .filter(|_| self.panel_motion.is_shown() && view.panel.is_some())
            .map(|panel| panel.x() as f64);

        // The watch stage, while there is a show.
        let watching = pictures.iter().any(|picture| picture.id == watch::STAGE_ID);
        if !watching {
            if let Some(gone) = self.watch_view.borrow_mut().take() {
                gone.stop();
                self.stage.remove(watch::STAGE_ID);
            }
        }

        for picture in pictures {
            let Some(spot) = planned.spots.get(&picture.id) else {
                continue;
            };
            let on_stage = pinned.as_deref() == Some(picture.id.as_str());
            let filled = view.filled && on_stage;
            let filling_two = two && !picture.mine;
            // The stage's buttons in the corner of the picture, below what
            // reaches across it and clear of a panel over it.
            let controls_top = furniture
                .iter()
                .filter(|reach| {
                    let corner = graphene::Rect::new(
                        (spot.max_x() - 12.0 - 132.0) as f32,
                        spot.y as f32,
                        132.0,
                        12.0 + 36.0,
                    );
                    reach.intersection(&corner).is_some()
                })
                .map(|reach| (reach.y() + reach.height()) as f64 + 8.0 - spot.y)
                .fold(12.0, f64::max)
                .round() as i32;
            let controls_end = panel_left
                .filter(|left| *left < spot.max_x())
                .map(|left| (spot.max_x() - left + 8.0).round() as i32)
                .unwrap_or(12);
            if picture.id == watch::STAGE_ID {
                let view_widget = self.watch_view_for(*spot);
                view_widget.set_place(watch::Place {
                    on_stage,
                    filled,
                    compact: spot.height < 200.0,
                    controls_top,
                    controls_end,
                    lift: if spot.max_y() > height - island + 1.0
                        && self.bar_motion.is_shown()
                        && !self.side.get()
                    {
                        (island - plan::GAP) as i32
                    } else {
                        0
                    },
                });
                continue;
            }
            let tile = self.tile_for(&picture.id, *spot);
            let person = talk.person(&picture.identity);
            let name = if picture.mine {
                talk.local_name.clone()
            } else {
                person
                    .map(|person| person.label())
                    .unwrap_or_else(|| picture.identity.clone())
            };
            let is_camera = picture.kind == PictureKind::Camera;
            // Somebody else's screen that is not on the stage is a card in
            // its place, and nothing of it is received (ShareCard.tsx).
            let share_card = !picture.mine && !is_camera && !on_stage;
            // What the plate says: a screen says so, and one's own pictures
            // say so (surface.ts `label`, the Mac's RoomView): a shared
            // screen read as its owner's camera turned off.
            let label = match (picture.mine, is_camera) {
                (true, true) => t_with("{name} (you)", &[("name", &name)]),
                (true, false) => t_with(
                    "{name} (you)",
                    &[("name", &t_with("{name} (screen)", &[("name", &name)]))],
                ),
                (false, false) if !share_card => t_with("{name} (screen)", &[("name", &name)]),
                _ => name.clone(),
            };
            let hearing = view
                .hearing
                .get(&picture.identity)
                .copied()
                .unwrap_or_default();
            // Under the row at the top (the room's name and the signal),
            // whose bounds are where they are drawn now.
            let cornered = self
                .title_motion
                .compute_bounds(&self.root)
                .filter(|_| self.title_motion.is_shown())
                .is_some_and(|row| {
                    row.intersection(&graphene::Rect::new(
                        spot.x as f32,
                        spot.y as f32,
                        44.0,
                        44.0,
                    ))
                    .is_some()
                });
            // A picture that runs under the bar keeps its name above it,
            // unless it is wide enough that the bar, centred, misses the
            // corner the name is in (the Mac's `underBar`).
            let under_bar =
                !self.side.get() && spot.max_y() > height - island + 1.0 && spot.width < 900.0;
            let lift = if under_bar && self.bar_motion.is_shown() {
                (island - plan::GAP) as i32
            } else {
                0
            };
            let my_share = picture.mine && picture.kind == PictureKind::Screen;
            if my_share {
                let weak = model.downgrade();
                tile.on_stop_sharing(move || {
                    if let Some(model) = weak.upgrade() {
                        super::stop_share(&model);
                    }
                });
            }
            let other = counterpart(&picture.id, pictures);
            let marks = Marks {
                my_share,
                share_card,
                hand_beside_name: cornered,
                lift,
                label,
                name,
                identity: picture.identity.clone(),
                proven: is_camera
                    && signature_of(picture.identity.clone()).is_some_and(|mark| mark.proven),
                unverified: is_camera
                    && roster
                        .iter()
                        .find(|named| named.identity == picture.identity)
                        .is_some_and(|named| impersonating(named.clone(), roster.clone())),
                speaking: is_camera
                    && if picture.mine {
                        talk.me_speaking && room.local.microphone_on.get()
                    } else {
                        person.is_some_and(|person| person.speaking)
                    },
                muted: is_camera
                    && if picture.mine {
                        !room.local.microphone_on.get()
                    } else {
                        person.is_none_or(|person| person.microphone_off())
                    },
                silenced: !picture.mine && (hearing.blocked || hearing.level <= 0.0),
                hand: is_camera
                    && if picture.mine {
                        talk.hand_up
                    } else {
                        person.is_some_and(|person| person.hand_up())
                    },
                unreadable: !picture.mine
                    && person.is_some_and(|person| person.readable == Some(false)),
                compact: spot.height < 140.0,
                floating: two && picture.mine,
                filled,
                fill_button: if on_stage || filling_two {
                    Some(filled)
                } else {
                    None
                },
                other: other.as_ref().map(|(_, kind)| *kind),
                controls_at_foot: filling_two && !on_stage,
                controls_top,
                controls_end,
                kind: Some(picture.kind),
                mirrored: picture.mine && is_camera,
                unlabelled: false,
            };
            tile.set_marks(marks);
            let fill_target = picture.id.clone();
            let weak = model.downgrade();
            tile.on_fill(move || {
                let Some(model) = weak.upgrade() else { return };
                let room = model.kept::<RoomState>();
                let (filled, pinned) = {
                    let view = room.view.borrow();
                    (view.filled, view.pin.pinned().map(str::to_owned))
                };
                if filled {
                    super::unfill(&model);
                } else if pinned.as_deref() == Some(fill_target.as_str()) {
                    super::fill(&model);
                } else {
                    super::fill_alone(&model, &fill_target);
                }
            });
            // "Their screen" or "Their camera": the same person's other
            // picture on the stage in this one's place.
            if let Some((other_id, _)) = other {
                let weak = model.downgrade();
                tile.on_switch(move || {
                    if let Some(model) = weak.upgrade() {
                        model
                            .kept::<RoomState>()
                            .view
                            .borrow_mut()
                            .pin
                            .pin(Some(other_id.clone()));
                        super::changed(&model);
                    }
                });
            }

            // The picture: a remote track's frames, or one's own camera.
            if picture.mine {
                if picture.kind == PictureKind::Camera {
                    self.preview_into(&tile);
                }
            } else if let Some(call) = call.as_ref() {
                // A share in the grid is received not at all: the server is
                // told to stop sending it, as a browser's adaptive stream
                // stops a track no element shows, and it flows again when
                // it goes to the stage. Its sound is another track and
                // plays on.
                let wanted = if share_card {
                    None
                } else {
                    picture.sid.clone()
                };
                if let Some(sid) = picture.sid.as_ref().filter(|_| !is_camera) {
                    let held = self.held_back.borrow().contains(sid);
                    if share_card && !held {
                        match call.set_remote_enabled(sid.clone(), false) {
                            Ok(()) => {
                                self.held_back.borrow_mut().insert(sid.clone());
                            }
                            Err(fault) => crate::app::log(&format!(
                                "a share in the grid was not held back: {fault}"
                            )),
                        }
                    } else if !share_card && held {
                        let _ = call.set_remote_enabled(sid.clone(), true);
                        self.held_back.borrow_mut().remove(sid);
                    }
                }
                let attached = tile.attached.borrow().clone();
                if attached != wanted {
                    if let Some(old) = attached {
                        call.detach(old);
                        tile.video().clear();
                    }
                    if let Some(sid) = wanted.as_ref() {
                        call.attach_video(sid.clone(), tile.video().sink());
                        let (w, h) = tile.video().size_wanted();
                        if w > 0 && h > 0 {
                            let _ = call.request_video_size(sid.clone(), w, h);
                        }
                    }
                    tile.attached.replace(wanted);
                }
            }
        }
        drop(view);
        self.stage.set_spots(&planned.spots, true);

        // The pager, where there are pages.
        let pages = planned.pages;
        self.previous_motion.show(pages > 1 && planned.page > 0);
        self.next_motion.show(pages > 1 && planned.page + 1 < pages);
    }

    /// Where the window's own buttons are drawn over the room, in the
    /// stage's coordinates: at the top right on most desktops, over the
    /// pictures as the Mac's hidden title bar's are.
    fn window_buttons(&self) -> Option<graphene::Rect> {
        let window = self.window.upgrade()?;
        let mut found = None;
        let mut queue: Vec<gtk::Widget> = vec![window.upcast()];
        while let Some(widget) = queue.pop() {
            if widget.css_name() == "windowcontrols" && widget.is_visible() && widget.width() > 0 {
                if let Some(bounds) = widget.compute_bounds(&self.stage) {
                    found = Some(match found {
                        None => bounds,
                        Some(seen) => bounds.union(&seen),
                    });
                }
                continue;
            }
            let mut child = widget.first_child();
            while let Some(next) = child {
                child = next.next_sibling();
                queue.push(next);
            }
        }
        found
    }

    /// The watch stage's view, made the first time a show is drawn.
    fn watch_view_for(self: &Rc<Self>, spot: plan::Rect) -> Rc<watch::StageView> {
        if let Some(view) = self.watch_view.borrow().as_ref() {
            return view.clone();
        }
        let view = watch::StageView::new(&self.model);
        let motion = Motion::hidden(view.widget(), Away::GROW);
        self.stage.add(watch::STAGE_ID, &motion, spot);
        self.watch_view.replace(Some(view.clone()));
        view
    }

    /// The tile for a picture, made the first time it is drawn.
    fn tile_for(self: &Rc<Self>, id: &str, spot: plan::Rect) -> Rc<Tile> {
        if let Some(tile) = self.tiles.borrow().get(id) {
            return tile.clone();
        }
        let tile = Tile::new(id);
        // The size the picture is drawn at is the size asked of the server
        // (adaptive stream), in device pixels.
        let (weak_tile, weak_model) = (Rc::downgrade(&tile), self.model.downgrade());
        tile.video().connect_size_wanted(move |_, width, height| {
            let (Some(tile), Some(model)) = (weak_tile.upgrade(), weak_model.upgrade()) else {
                return;
            };
            let sid = tile.attached.borrow().clone();
            if let (Some(sid), Some(call)) = (sid, super::call_of(&model)) {
                let _ = call.request_video_size(sid, width, height);
            }
        });
        // Enter on the focused picture does what a press on it does, and
        // the menu key opens its menu where it is.
        let (weak_model, pressed) = (self.model.downgrade(), id.to_owned());
        tile.on_press(move || {
            if let Some(model) = weak_model.upgrade() {
                super::press_picture(&model, &pressed);
            }
        });
        let (page, asked, weak_tile) = (Rc::downgrade(self), id.to_owned(), Rc::downgrade(&tile));
        tile.on_menu(move |x, y| {
            let (Some(page), Some(tile)) = (page.upgrade(), weak_tile.upgrade()) else {
                return;
            };
            let at = tile
                .root
                .compute_point(&page.root, &graphene::Point::new(x as f32, y as f32))
                .map(|point| (point.x() as f64, point.y() as f64))
                .unwrap_or((x, y));
            page.picture_menu.open(&asked, at.0, at.1);
        });
        let motion = Motion::hidden(&tile.root, Away::GROW);
        self.stage.add(id, &motion, spot);
        self.tiles.borrow_mut().insert(id.to_owned(), tile.clone());
        tile
    }

    /// Where the bar stands: across the foot of the room, or down its side
    /// where the setting says so and the room is tall enough for the column
    /// (the Mac's `barPlace`). The room is then narrower by the bar's lane,
    /// so that the pictures, the panels and the cards on the right all keep
    /// out of it, and the bar goes from one edge to the other by leaving and
    /// coming back rather than jumping.
    fn place_bar(self: &Rc<Self>, compact: bool) {
        let room = self.model.kept::<RoomState>();
        let filled = room.view.borrow().filled;
        let height = self.root.height();
        let side = self
            .model
            .preferences()
            .string(theme::CONTROLS_KEY)
            .as_deref()
            == Some("side")
            && height >= SIDE_NEEDS
            && !filled;
        let _ = compact;
        if self.side.get() == side {
            return;
        }
        self.side.set(side);
        self.stage.set_margin_end(if side { SIDE_LANE } else { 0 });
        let page = Rc::downgrade(self);
        let arrange = move || {
            let Some(page) = page.upgrade() else { return };
            let side = page.side.get();
            let bar = &page.bar;
            let axis = if side {
                gtk::Orientation::Vertical
            } else {
                gtk::Orientation::Horizontal
            };
            bar.island.set_orientation(axis);
            if let Some(pair) = bar.hand.parent().and_downcast::<gtk::Box>() {
                pair.set_orientation(axis);
            }
            class(&bar.island, "side", side);
            let mut child = bar.island.first_child();
            while let Some(next) = child {
                if let Some(revealer) = next.downcast_ref::<gtk::Revealer>() {
                    revealer.set_transition_type(if side {
                        gtk::RevealerTransitionType::SlideDown
                    } else {
                        gtk::RevealerTransitionType::SlideRight
                    });
                }
                child = next.next_sibling();
            }
            let motion = &page.bar_motion;
            if side {
                motion.set_halign(gtk::Align::End);
                motion.set_valign(gtk::Align::Center);
                motion.set_margin_end(12);
                motion.set_margin_bottom(0);
                motion.set_from(Away::SIDE);
            } else {
                motion.set_halign(gtk::Align::Center);
                motion.set_valign(gtk::Align::End);
                motion.set_margin_end(0);
                motion.set_margin_bottom(BAR_FOOT);
                motion.set_from(Away::FOOT);
            }
            page.moving_bar.set(false);
            motion.show(true);
            page.refresh();
        };
        if self.bar_motion.is_shown() && adw::is_animations_enabled(&self.root) {
            self.moving_bar.set(true);
            self.bar_motion.leave(arrange);
        } else {
            arrange();
        }
    }

    /// Stops drawing into a tile that is going: its track detached, or its
    /// camera preview removed.
    fn let_go(&self, tile: &Tile, call: Option<&std::sync::Arc<tomoshibi_core::call::Call>>) {
        if let Some(sid) = tile.attached.take() {
            if let Some(call) = call {
                call.detach(sid);
            }
        }
        if tile.id == ME {
            if let Some((camera, id)) = self.self_preview.take() {
                camera.remove_preview(id);
            }
        }
    }

    /// One's own camera drawn into its tile, from the camera itself (the
    /// capture's preview), while it is on.
    fn preview_into(&self, tile: &Tile) {
        let room = self.model.kept::<RoomState>();
        let camera = room
            .local
            .camera
            .borrow()
            .clone()
            .filter(|_| room.local.camera_on.get());
        let mut preview = self.self_preview.borrow_mut();
        let same = match (&camera, preview.as_ref()) {
            (Some(camera), Some((held, _))) => Rc::ptr_eq(camera, held),
            (None, None) => true,
            _ => false,
        };
        if same {
            return;
        }
        if let Some((held, id)) = preview.take() {
            held.remove_preview(id);
            tile.video().clear();
        }
        if let Some(camera) = camera {
            let id = camera.add_preview(tile.video().sink());
            *preview = Some((camera, id));
        }
    }

    fn refresh_bar(self: &Rc<Self>, compact: bool) {
        let model = &self.model;
        let room = model.kept::<RoomState>();
        let view = room.view.borrow();
        let bar = &self.bar;
        let microphone = room.local.microphone_on.get();
        let camera = room.local.camera_on.get();
        let sharing = room.local.share.borrow().is_some();
        let hand = room.talk.borrow().as_ref().is_some_and(|talk| talk.hand_up);
        let unread = room.talk.borrow().as_ref().is_some_and(|talk| talk.unread);
        let host = room.host.borrow();
        let can_share =
            super::media_of(model).is_some_and(|media| media.can_share_screen()) || sharing;

        class(&bar.island, "compact", compact);
        bar.microphone.set_icon_name(if microphone {
            "audio-input-microphone-symbolic"
        } else {
            "microphone-disabled-symbolic"
        });
        set_label(
            &bar.microphone,
            &if microphone {
                t("Mute microphone")
            } else {
                t("Unmute microphone")
            },
        );
        class(&bar.microphone, "off", !microphone);
        bar.camera.set_icon_name(if camera {
            "camera-video-symbolic"
        } else {
            "camera-disabled-symbolic"
        });
        set_label(
            &bar.camera,
            &if camera {
                t("Turn camera off")
            } else {
                t("Turn camera on")
            },
        );
        class(&bar.camera, "off", !camera);
        come_or_go(&bar.share, can_share);
        class(&bar.share, "lit", sharing);
        class(&bar.share, "held", view.pop == Some(Pop::Share));
        set_label(
            &bar.share,
            &if sharing {
                t("Stop sharing")
            } else {
                t("Share your screen")
            },
        );
        class(&bar.hand, "lit", hand);
        set_label(
            &bar.hand,
            &if hand {
                t("Lower your hand")
            } else {
                t("Raise your hand")
            },
        );
        class(&bar.reactions, "held", view.pop == Some(Pop::Reactions));
        set_label(&bar.reactions, &t("Reactions"));
        let messages = view.panel == Some(Panel::Messages);
        class(&bar.messages, "held", messages);
        set_label(
            &bar.messages,
            &if messages {
                t("Hide messages")
            } else {
                t("Show messages")
            },
        );
        bar.messages_dot.show(unread && !messages);
        let sound = view.panel == Some(Panel::Sound);
        class(&bar.sound, "held", sound);
        set_label(
            &bar.sound,
            &if sound {
                t("Hide sound")
            } else {
                t("Show sound")
            },
        );
        come_or_go(&bar.sound, !compact);
        let music = view.panel == Some(Panel::Music);
        class(&bar.music, "held", music);
        set_label(
            &bar.music,
            &if music {
                t("Hide music")
            } else {
                t("Show music")
            },
        );
        come_or_go(
            &bar.music,
            (music || super::music::available(model)) && !compact,
        );
        bar.music_dot.show(!music && super::music::playing(model));
        let watch_open = view.panel == Some(Panel::Watch);
        come_or_go(
            &bar.watch,
            (watch::offered(model) || watch_open) && !compact,
        );
        class(&bar.watch, "held", watch_open);
        set_label(
            &bar.watch,
            &if watch_open {
                t("Hide watch together")
            } else {
                t("Show watch together")
            },
        );
        bar.watch_dot.show(watch::playing(model) && !watch_open);
        class(&bar.devices, "held", view.panel == Some(Panel::Devices));
        set_label(&bar.devices, &t("Devices"));
        come_or_go(&bar.devices, !compact);
        let hosting = host.yours() || view.panel == Some(Panel::Host);
        come_or_go(&bar.host, hosting && !compact);
        class(&bar.host, "held", view.panel == Some(Panel::Host));
        set_label(&bar.host, &t("Running this room"));
        let knocking = host.waiting.len();
        if knocking > 0 {
            bar.host_count.set_label(&knocking.to_string());
        }
        bar.host_count_motion
            .show(knocking > 0 && view.panel != Some(Panel::Host));
        come_or_go(&bar.more, compact);
        class(&bar.more, "held", view.pop == Some(Pop::More));
        set_label(&bar.more, &t("More controls"));
        come_or_go(&bar.rule, !compact);
        class(&bar.leave, "held", view.pop == Some(Pop::Leave));
        set_label(&bar.leave, &t("Leave"));
        bar.island.set_sensitive(room.call.borrow().is_some());

        // The controls step aside while the stage fills the screen, or where
        // the setting says they may, and never while a popover is open.
        let hides = view.filled
            || model.preferences().string(theme::CONTROLS_KEY).as_deref() == Some("idle");
        let shown = !hides || !view.idle || view.pop.is_some();
        if !self.moving_bar.get() {
            self.bar_motion.show(shown);
        }
        // Stillness is counted from the moment the controls may step aside
        // (the stage filled), not only from the pointer's last move: filled
        // from a key or a script, with the pointer still, they stayed.
        if hides && !view.idle && self.idle_timer.borrow().is_none() {
            drop(view);
            self.arm_idle();
        }
    }

    fn refresh_chrome(&self, compact: bool) {
        let model = &self.model;
        let room = model.kept::<RoomState>();
        let view = room.view.borrow();
        let talk = room.talk.borrow();
        let name = model
            .call_entry()
            .map(|entry| entry.grant.room)
            .unwrap_or_default();
        if self.title.label() != name {
            self.title.set_label(&name);
        }
        let hides = view.filled;
        self.title_motion
            .show(!hides || !view.idle || view.pop == Some(Pop::Signal));

        // The signal, only once connected: a reading of a connection that is
        // not there yet is a reading of nothing. It arrives and leaves.
        let badge = model.preferences().string(theme::BADGE_KEY);
        let connected = room.state.get() == Some(CallState::Connected) && talk.is_some();
        self.signal_motion
            .show(connected && badge.as_deref() != Some("hidden"));
        // "With details": the round trip beside the bars, and the bars
        // press open the figures; "Signal only" is the bars and nothing to
        // press.
        let detailed = badge.as_deref() == Some("detailed");
        let round_trip = talk
            .as_ref()
            .and_then(|talk| talk.connection.as_ref())
            .filter(|reading| reading.measured)
            .and_then(|reading| reading.rtt_ms)
            .map(|ms| format!("{} ms", ms.round() as i64))
            .unwrap_or_else(|| "\u{2014}".to_owned());
        if self.signal_figure.label() != round_trip {
            self.signal_figure.set_label(&round_trip);
        }
        self.signal_figure.set_visible(detailed);
        class(&self.signal, "detailed", detailed);
        class(&self.signal, "quiet", !detailed);
        self.signal.set_can_focus(detailed);
        if !detailed && view.pop == Some(Pop::Signal) {
            let weak = model.downgrade();
            glib::idle_add_local_once(move || {
                if let Some(model) = weak.upgrade() {
                    super::close_pop(&model);
                }
            });
        }
        let grade = talk.as_ref().and_then(|talk| {
            talk.connection
                .as_ref()
                .map(|reading| reading.grade)
                .or(Some(match talk.local_quality {
                    tomoshibi_core::call::LinkQuality::Poor => ConnectionGrade::Poor,
                    tomoshibi_core::call::LinkQuality::Lost => ConnectionGrade::Lost,
                    _ => ConnectionGrade::Good,
                }))
        });
        if self.signal_grade.replace(grade) != grade {
            self.signal_bars.queue_draw();
        }
        set_label(
            &self.signal,
            &match grade {
                Some(ConnectionGrade::Lost) => t("Connection lost"),
                Some(ConnectionGrade::Poor) => t("Poor connection"),
                Some(ConnectionGrade::Fair) => t("Fair connection"),
                _ => t("Good connection"),
            },
        );
        class(&self.signal, "held", view.pop == Some(Pop::Signal));

        // Alone in the room: the link that brings somebody else in, or, for
        // somebody who does not run the room, who can (EmptyRoom.tsx).
        let alone = talk.as_ref().is_some_and(|talk| talk.people.is_empty())
            && view.pin.pinned().is_none()
            && !view.alone_dismissed
            && !view.filled
            && connected;
        let link_works = super::link_works(model);
        let host = room.host.borrow();
        let not_host = !link_works && !host.yours();
        self.alone_button.set_visible(!not_host);
        self.alone_sentence.set_visible(not_host);
        self.alone_sentence
            .set_label(&t("Only whoever runs this room can let somebody else in."));
        self.alone_label.set_label(&if view.copied {
            t("Link copied")
        } else if link_works {
            t("Copy link")
        } else {
            t("Invite somebody")
        });
        self.alone_icon.set_icon_name(Some(if view.copied {
            "object-select-symbolic"
        } else if link_works {
            "edit-copy-symbolic"
        } else {
            "insert-link-symbolic"
        }));
        self.alone_button
            .set_sensitive(host.busy.as_deref() != Some("invite"));
        // Out of the way of a popover off the bar, which rises over where it
        // sits: the share menu was drawn over it with half of it showing
        // beside the menu.
        let bar_pop = matches!(
            view.pop,
            Some(Pop::Share | Pop::Reactions | Pop::More | Pop::Leave)
        );
        self.alone_motion.show(alone && !bar_pop);
        self.alone_motion.set_margin_bottom(if self.side.get() {
            20
        } else {
            BAR_FOOT + 58 + 14
        });

        // The room's link from the row at the top: the link where it lets
        // people in, an invitation for whoever runs a room it does not, and
        // nothing for anybody else, who could only send people to a door
        // that turns them away.
        let shares_link = connected && (link_works || host.yours());
        self.share_link_motion.show(shares_link);
        // The link, and a tick for a moment once it is copied.
        let ticked = self
            .share_link
            .child()
            .is_some_and(|child| child.is::<gtk::Image>());
        if view.copied != ticked {
            if view.copied {
                self.share_link
                    .set_child(Some(&gtk::Image::from_icon_name("object-select-symbolic")));
            } else {
                self.share_link
                    .set_child(Some(&Glyph::new(glyph::LINK, 14)));
            }
        }
        set_label(
            &self.share_link,
            &if view.copied {
                t("Link copied")
            } else {
                t("Share link")
            },
        );
        self.share_link
            .set_sensitive(host.busy.as_deref() != Some("invite"));
        let _ = compact;
    }

    fn refresh_panel(self: &Rc<Self>, width: i32) {
        let model = &self.model;
        let open = model.kept::<RoomState>().view.borrow().panel;
        // Beside the call where the window has room for it, the width of the
        // window less twelve either side where it has not (the Mac's
        // `panelWidth`); from the side beside it, from the foot over it.
        // Beside the bar's lane where it stands down the side.
        let lane = if self.side.get() { SIDE_LANE } else { 0 };
        let width = width - lane;
        self.panel_motion.set_margin_end(12 + lane);
        self.panel_motion.set_margin_bottom(self.island() as i32);
        let beside = 360.min(width - 32);
        let narrow = width > 0 && width < beside + 24 + 172 + 22;
        let panel_width = if narrow {
            (width - 24).max(0)
        } else {
            beside.max(0)
        };
        if panel_width > 0 {
            self.panel_motion.set_width_request(panel_width);
        }
        self.panel_motion
            .set_from(if narrow { Away::FOOT } else { Away::SIDE });
        let was = self.panel_shown.get();
        if open != was {
            self.panel_shown.set(open);
            match (was, open) {
                // One panel for another: the first goes, then the second
                // comes, as the Mac's panels change (`.id(panel)`), rather
                // than its contents being swapped under the pointer.
                (Some(_), Some(panel)) if self.panel_motion.is_shown() => {
                    let page = Rc::downgrade(self);
                    self.panel_motion.leave(move || {
                        let Some(page) = page.upgrade() else { return };
                        // Only if it is still the one wanted when the first
                        // has gone.
                        if page.panel_shown.get() == Some(panel) {
                            page.fill_panel(panel);
                            page.panel_motion.show(true);
                            page.refresh();
                        }
                    });
                }
                (_, Some(panel)) => {
                    self.fill_panel(panel);
                    self.panel_motion.show(true);
                }
                (_, None) => self.panel_motion.show(false),
            }
        }
        if let Some(view) = self.panel_view.borrow().as_ref() {
            if open.is_some() {
                (view.refresh)(model);
            }
        }
        // The notices sit beside an open panel where there is room for both.
        let beside_room = open.is_some() && width - panel_width - 48 >= 280;
        self.toasts.set_margin_end(
            lane + if beside_room {
                panel_width + 24 + 8
            } else {
                16
            },
        );
        self.toasts.set_margin_bottom(self.island() as i32 + 8);
    }

    fn fill_panel(&self, panel: Panel) {
        if let Some(old) = self.panel_holder.first_child() {
            self.panel_holder.remove(&old);
        }
        let view = panels::build(&self.model, panel);
        self.panel_holder.append(&view.widget);
        self.panel_view.replace(Some(view));
    }

    fn refresh_pops(&self) {
        let model = &self.model;
        let open = model.kept::<RoomState>().view.borrow().pop;
        for (name, pop) in &self.pops {
            let showing = open.map(pop_name) == Some(*name);
            if showing {
                if let Some(refresh) = self.pop_content.borrow().get(name) {
                    refresh(model);
                }
                self.root.queue_allocate();
            }
            pop.motion.show(showing);
        }
    }

    fn refresh_notices(&self) {
        let model = &self.model;
        // Who has one's share open; beside an open panel rather than under
        // it.
        let watching = super::watchers(model);
        if let Some(names) = watching.as_ref() {
            self.watchers.set_label(&if names.is_empty() {
                t("Nobody has this open yet.")
            } else {
                format!("{}\n{}", t("Watching"), names.join(", "))
            });
        }
        let panel_open = model.kept::<RoomState>().view.borrow().panel.is_some();
        let lane = if self.side.get() { SIDE_LANE } else { 0 };
        let mut end = lane
            + if panel_open {
                self.panel_motion.width_request() + 12 + 8
            } else {
                14
            };
        let mut top = 56;
        // Clear of one's own picture where it is the card in the top corner
        // (a call of two, or alone with one's own share): the list was drawn
        // over its top edge. Beside it where there is room, below it where
        // there is not.
        let card = self
            .stage
            .spot(ME)
            .filter(|spot| spot.width < self.stage.size().0 / 2.0);
        if let Some(card) = card {
            let whole = self.root.width();
            let (_, natural) = self.watchers_motion.preferred_size();
            let list = graphene::Rect::new(
                (whole - end - natural.width()) as f32,
                top as f32,
                natural.width() as f32,
                natural.height().max(36) as f32,
            );
            let card_rect = graphene::Rect::new(
                card.x as f32,
                card.y as f32,
                card.width as f32,
                card.height as f32,
            );
            if list.intersection(&card_rect).is_some() {
                let beside = (whole as f64 - card.x + 8.0).round() as i32;
                if (card.x as i32) - natural.width() - 8 >= 8 {
                    end = beside;
                } else {
                    top = (card.max_y() + 8.0).round() as i32;
                }
            }
        }
        self.watchers_motion.set_margin_end(end);
        self.watchers_motion.set_margin_top(top);
        self.watchers_motion.show(watching.is_some());
        let room = model.kept::<RoomState>();
        let view = room.view.borrow();

        // The connection, while it is not there.
        let state = room.state.get();
        let joining = room.talk.borrow().is_none();
        let notice = if joining || state.is_none() {
            Some(t("Connecting…"))
        } else {
            match state {
                Some(CallState::Reconnecting) => Some(t("Reconnecting…")),
                Some(CallState::Disconnected) => Some(t("Connection lost")),
                _ => None,
            }
        };
        if let Some(said) = notice.as_ref() {
            self.notice.set_label(said);
        }
        self.notice_motion.show(notice.is_some());
        if let Some(problem) = view.problem.as_ref() {
            self.problem.set_label(problem);
        }
        self.problem_motion.show(view.problem.is_some());

        // The corner's notices, each arriving and leaving on its own.
        let wanted: Vec<(u64, Toast)> = view.toasts.clone();
        reconcile(&self.toasts, &self.toast_widgets, &wanted, |(id, toast)| {
            (*id, toast_widget(model, *id, toast))
        });
        // What was said, bottom left, above the controls and clear of the
        // name plates under it: in a grid of six at 1000 by 700 the card lay
        // over the name of the picture in that corner.
        let island = self.island();
        let mut foot = island as i32 + 8;
        let (said_width, said_height) = (self.said.width().max(240), self.said.height());
        let stage_height = self.stage.size().1;
        if said_height > 0 && stage_height > 0.0 {
            for _ in 0..3 {
                let bottom = stage_height - foot as f64;
                let top = bottom - said_height as f64;
                let covered = self
                    .tiles
                    .borrow()
                    .iter()
                    .filter_map(|(id, tile)| {
                        let spot = self.stage.spot(id)?;
                        let marks = tile.marks();
                        let inset = if marks.compact { 4.0 } else { 8.0 };
                        let plate_bottom = spot.max_y() - inset - marks.lift as f64;
                        let plate_top = plate_bottom - 24.0;
                        let plate_left = spot.x + inset;
                        let overlaps = plate_left < 12.0 + said_width as f64
                            && plate_left + 120.0 > 12.0
                            && plate_bottom > top
                            && plate_top < bottom;
                        overlaps.then_some(plate_top)
                    })
                    .fold(f64::INFINITY, f64::min);
                if !covered.is_finite() {
                    break;
                }
                foot = (stage_height - covered + 8.0).round() as i32;
            }
        }
        self.said.set_margin_bottom(foot);
        let said: Vec<(u64, super::talk::Line)> = view.said.clone();
        reconcile(&self.said, &self.said_widgets, &said, |(id, line)| {
            let card = gtk::Box::new(gtk::Orientation::Vertical, 2);
            card.add_css_class("tmsb-said");
            let name = gtk::Label::builder().label(&line.name).xalign(0.0).build();
            name.add_css_class("tmsb-line-name");
            let text = gtk::Label::builder()
                .label(&line.text)
                .xalign(0.0)
                .wrap(true)
                .max_width_chars(36)
                .lines(3)
                .ellipsize(gtk::pango::EllipsizeMode::End)
                .build();
            text.add_css_class("tmsb-line-text");
            card.append(&name);
            card.append(&text);
            (*id, card.upcast())
        });
    }
}

/// The same person's other picture, which the stage's switch offers
/// (Room.tsx `counterpart`, the Mac's): their screen for their camera and
/// back, one's own share for one's own camera; never the video watched
/// together, which belongs to whoever runs it but is no picture of them.
fn counterpart(id: &str, pictures: &[super::talk::Picture]) -> Option<(String, PictureKind)> {
    let wanted = match id {
        ME => (MY_SHARE.to_owned(), PictureKind::Screen),
        MY_SHARE => (ME.to_owned(), PictureKind::Camera),
        _ if id == watch::STAGE_ID => return None,
        _ => {
            let (owner, kind) = id.rsplit_once('/')?;
            match kind {
                "camera" => (format!("{owner}/screen"), PictureKind::Screen),
                "screen" => (format!("{owner}/camera"), PictureKind::Camera),
                _ => return None,
            }
        }
    };
    pictures
        .iter()
        .any(|picture| picture.id == wanted.0)
        .then_some(wanted)
}

fn pop_name(pop: Pop) -> &'static str {
    match pop {
        Pop::Reactions => "reactions",
        Pop::Leave => "leave",
        Pop::Share => "share",
        Pop::More => "more",
        Pop::Signal => "signal",
    }
}

/// Brings a column of cards in line with what is wanted: new ones arrive,
/// gone ones leave, the rest stay where they are.
fn reconcile<T>(
    column: &gtk::Box,
    shown: &RefCell<Vec<(u64, Motion)>>,
    wanted: &[T],
    make: impl Fn(&T) -> (u64, gtk::Widget),
) where
    T: HasId,
{
    let wanted_ids: Vec<u64> = wanted.iter().map(HasId::id).collect();
    let mut shown = shown.borrow_mut();
    shown.retain(|(id, motion)| {
        if wanted_ids.contains(id) {
            return true;
        }
        let (column, gone) = (column.downgrade(), motion.downgrade());
        motion.leave(move || {
            if let (Some(column), Some(gone)) = (column.upgrade(), gone.upgrade()) {
                column.remove(&gone);
            }
        });
        false
    });
    for item in wanted {
        if shown.iter().any(|(id, _)| *id == item.id()) {
            continue;
        }
        let (id, widget) = make(item);
        let motion = Motion::hidden(&widget, Away::FOOT);
        column.append(&motion);
        motion.show(true);
        shown.push((id, motion));
    }
}

trait HasId {
    fn id(&self) -> u64;
}

impl<T> HasId for (u64, T) {
    fn id(&self) -> u64 {
        self.0
    }
}

fn toast_widget(model: &AppModel, id: u64, toast: &Toast) -> gtk::Widget {
    let card = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    card.add_css_class("tmsb-toast");
    match toast.kind {
        ToastKind::Signal => card.add_css_class("signal"),
        ToastKind::Failure => card.add_css_class("failure"),
        ToastKind::Info => {}
    }
    card.set_width_request(260);
    let words = gtk::Box::new(gtk::Orientation::Vertical, 2);
    words.set_hexpand(true);
    let title = gtk::Label::builder()
        .label(&toast.title)
        .xalign(0.0)
        .wrap(true)
        .max_width_chars(34)
        .build();
    title.add_css_class("title");
    words.append(&title);
    if let Some(detail) = toast.detail.as_ref() {
        let detail = gtk::Label::builder()
            .label(detail)
            .xalign(0.0)
            .wrap(true)
            .max_width_chars(34)
            .build();
        detail.add_css_class("detail");
        words.append(&detail);
    }
    card.append(&words);
    if toast.closable {
        let close = gtk::Button::from_icon_name("window-close-symbolic");
        close.add_css_class("tmsb-close");
        close.set_valign(gtk::Align::Start);
        set_label(&close, &t("Close"));
        let weak = model.downgrade();
        close.connect_clicked(move |_| {
            if let Some(model) = weak.upgrade() {
                super::dismiss(&model, id);
            }
        });
        card.append(&close);
    }
    card.upcast()
}
