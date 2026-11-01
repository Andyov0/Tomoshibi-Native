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
    collections::HashMap,
    rc::Rc,
};

use adw::prelude::*;
use gtk::{gdk, glib, graphene};
use tomoshibi_core::call::CallState;
use tomoshibi_protocol::{
    hands,
    identity::{impersonating, signature_of, NamedIdentity},
    quality::ConnectionGrade,
    share::ShareQuality,
};

use super::{
    glyph::{self, Glyph},
    motion::{Away, Motion},
    panels,
    plan::{self, Size},
    stage::Stage,
    talk::{PictureKind, Toast, ToastKind, ME},
    tile::{Marks, Tile},
    Panel, Pop, RoomState, CHANGED,
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
    signal: gtk::Button,
    signal_bars: gtk::DrawingArea,
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
}

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
        super::picture_menu::attach(&root, &stage, &model);

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
        let signal = gtk::Button::builder().child(&signal_bars).build();
        signal.add_css_class("tmsb-signal");
        signal.set_widget_name("room.signal");
        let title_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        title_row.append(&title);
        title_row.append(&signal);
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
            signal,
            signal_bars,
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
                let heading = gtk::Label::builder()
                    .label(t("Picture quality"))
                    .xalign(0.0)
                    .build();
                heading.add_css_class("tmsb-menu-heading");
                card.append(&heading);
                let choices = gtk::Box::new(gtk::Orientation::Horizontal, 2);
                let mut buttons = Vec::new();
                for (quality, label) in [
                    (ShareQuality::Auto, t("Automatic")),
                    (ShareQuality::P1080, "1080p".to_owned()),
                    (ShareQuality::P1440, "1440p".to_owned()),
                    (ShareQuality::P4k, "4K".to_owned()),
                ] {
                    let button = gtk::Button::with_label(&label);
                    button.add_css_class("tmsb-choice");
                    let w = weak.clone();
                    button.connect_clicked(move |_| {
                        if let Some(model) = w.upgrade() {
                            model.kept::<RoomState>().view.borrow_mut().share_quality =
                                Some(quality);
                            super::changed(&model);
                        }
                    });
                    choices.append(&button);
                    buttons.push((quality, button));
                }
                card.append(&choices);
                let rule = gtk::Box::new(gtk::Orientation::Horizontal, 0);
                rule.add_css_class("tmsb-menu-rule");
                card.append(&rule);
                let (go, go_title) = menu_row(
                    &t("Share your screen"),
                    None,
                    Some("screen-shared-symbolic"),
                );
                go.set_widget_name("share.go");
                let w = weak.clone();
                go.connect_clicked(move |_| {
                    if let Some(model) = w.upgrade() {
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
                card.set_width_request(300);
                let go = go.downgrade();
                (
                    card,
                    Box::new(move |model: &AppModel| {
                        let room = model.kept::<RoomState>();
                        let chosen = room
                            .view
                            .borrow()
                            .share_quality
                            .unwrap_or(ShareQuality::Auto);
                        for (quality, button) in &buttons {
                            class(button, "held", *quality == chosen);
                            if *quality == ShareQuality::Auto {
                                button.set_label(&t("Automatic"));
                            }
                        }
                        let sharing = room.local.share.borrow().is_some();
                        go_title.set_label(&if sharing {
                            t("Stop sharing")
                        } else {
                            t("Share your screen")
                        });
                        if let Some(go) = go.upgrade() {
                            go.set_sensitive(!room.local.sharing_busy.get());
                        }
                    }),
                )
            }
            "more" => {
                let (sound, _) = menu_row(&t("Sound"), None, Some("audio-volume-high-symbolic"));
                let (devices, _) = menu_row(&t("Devices"), None, Some("emblem-system-symbolic"));
                let (host, _) =
                    menu_row(&t("Running this room"), None, Some("system-users-symbolic"));
                for (row, panel) in [
                    (&sound, Panel::Sound),
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
                (
                    card,
                    Box::new(move |model: &AppModel| {
                        if let Some(host) = host_row.upgrade() {
                            host.set_visible(model.kept::<RoomState>().host.borrow().yours());
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
        on(&self.bar.devices, |model| {
            super::toggle_panel(model, Panel::Devices)
        });
        on(&self.bar.host, |model| {
            super::toggle_panel(model, Panel::Host)
        });
        on(&self.bar.more, |model| super::toggle_pop(model, Pop::More));
        on(&self.bar.leave, |model| {
            super::toggle_pop(model, Pop::Leave)
        });
        on(&self.signal, |model| super::toggle_pop(model, Pop::Signal));
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
            let open = page.model.kept::<RoomState>().view.borrow().pop;
            let Some(open) = open else { return };
            let Some(pop) = page.pops.get(pop_name(open)) else {
                return;
            };
            let point = graphene::Point::new(x as f32, y as f32);
            let inside = |widget: &gtk::Widget| {
                widget
                    .compute_bounds(&page.root)
                    .is_some_and(|bounds| bounds.contains_point(&point))
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
            keys.connect_key_pressed(move |_, key, _, _| {
                let Some(model) = weak.upgrade() else {
                    return glib::Propagation::Proceed;
                };
                if key != gdk::Key::Escape || model.screen() != Screen::Call {
                    return glib::Propagation::Proceed;
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
            let pop = page.pops.get(name)?;
            let anchor = pop.anchor.compute_bounds(overlay)?;
            let (_, natural) = child.preferred_size();
            let (width, height) = (natural.width(), natural.height());
            let whole = overlay.width();
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
        }
        out
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

    /// Where every picture goes at a size, from the state as it is.
    fn plan(&self, width: f64, height: f64) -> plan::Room {
        let room = self.model.kept::<RoomState>();
        let talk = room.talk.borrow();
        let Some(talk) = talk.as_ref() else {
            return plan::Room::default();
        };
        let view = room.view.borrow();
        let pictures = talk.pictures(
            room.local.camera_on.get(),
            room.local.share.borrow().is_some(),
        );
        let ids: Vec<String> = pictures.into_iter().map(|picture| picture.id).collect();
        plan::room(
            Size::new(width, height),
            &ids,
            Some(ME),
            view.pin.pinned(),
            view.filled,
            view.page,
            plan::ISLAND,
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
        if joined {
            let pictures = room
                .talk
                .borrow()
                .as_ref()
                .map(|talk| {
                    talk.pictures(
                        room.local.camera_on.get(),
                        room.local.share.borrow().is_some(),
                    )
                })
                .unwrap_or_default();
            let ids: Vec<String> = pictures.iter().map(|picture| picture.id.clone()).collect();
            let shares = room
                .talk
                .borrow()
                .as_ref()
                .map(|talk| talk.shares())
                .unwrap_or_default();
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
        for picture in pictures {
            let Some(spot) = planned.spots.get(&picture.id) else {
                continue;
            };
            let tile = self.tile_for(&picture.id, *spot);
            let person = talk.person(&picture.identity);
            let label = if picture.mine {
                t_with("{name} (you)", &[("name", &talk.local_name)])
            } else {
                person
                    .map(|person| person.label())
                    .unwrap_or_else(|| picture.identity.clone())
            };
            let hearing = view
                .hearing
                .get(&picture.identity)
                .copied()
                .unwrap_or_default();
            let is_camera = picture.kind == PictureKind::Camera;
            let filled = view.filled && pinned.as_deref() == Some(picture.id.as_str());
            let on_stage = pinned.as_deref() == Some(picture.id.as_str());
            let filling_two = two && !picture.mine;
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
            let under_bar = spot.max_y() > height - plan::ISLAND + 1.0 && spot.width < 900.0;
            let lift = if under_bar && self.bar_motion.is_shown() {
                (plan::ISLAND - plan::GAP) as i32
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
            let marks = Marks {
                my_share,
                hand_beside_name: cornered,
                lift,
                label,
                identity: picture.identity.clone(),
                proven: signature_of(picture.identity.clone()).is_some_and(|mark| mark.proven),
                unverified: roster
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
                compact: spot.height < 140.0,
                floating: two && picture.mine,
                filled,
                fill_button: if on_stage || filling_two {
                    Some(filled)
                } else {
                    None
                },
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

            // The picture: a remote track's frames, or one's own camera.
            if picture.mine {
                if picture.kind == PictureKind::Camera {
                    self.preview_into(&tile);
                }
            } else if let Some(call) = call.as_ref() {
                let wanted = picture.sid.clone();
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
        let motion = Motion::hidden(&tile.root, Away::GROW);
        self.stage.add(id, &motion, spot);
        self.tiles.borrow_mut().insert(id.to_owned(), tile.clone());
        tile
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
        self.bar_motion.show(shown);
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
        // not there yet is a reading of nothing.
        let badge = model.preferences().string(theme::BADGE_KEY);
        let connected = room.state.get() == Some(CallState::Connected) && talk.is_some();
        self.signal
            .set_visible(connected && badge.as_deref() != Some("hidden"));
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
        self.alone_motion.show(alone);
        let _ = compact;
    }

    fn refresh_panel(self: &Rc<Self>, width: i32) {
        let model = &self.model;
        let open = model.kept::<RoomState>().view.borrow().panel;
        // Beside the call where the window has room for it, the width of the
        // window less twelve either side where it has not (the Mac's
        // `panelWidth`); from the side beside it, from the foot over it.
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
        self.toasts.set_margin_end(if beside_room {
            panel_width + 24 + 8
        } else {
            16
        });
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
        self.watchers_motion.set_margin_end(if panel_open {
            self.panel_motion.width_request() + 12 + 8
        } else {
            14
        });
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
