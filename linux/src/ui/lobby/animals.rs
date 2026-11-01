//! The little animals drawn while the camera is off (art/README.md): one
//! animal after another in soft grey lines, stroke by stroke, from
//! art/portraits.json, the file every client plays as it is so that every
//! platform draws the same lines at the same moments.
//!
//! Each stroke is a GskPath, trimmed by arc length with a GskPathMeasure as
//! the pen goes (the Windows app does the same with Composition's path trims,
//! PreviewGlass.cs), and everything drawn so far is stroked once, as one
//! shape, so that the colour does not double where two lines cross.
//!
//! It plays only while it can be seen: a widget that is not mapped has no
//! frames, and the lobby holds it still (`set_held`) while a sheet or the
//! settings cover it, and it resumes from where it was rather than from
//! where the clock has got to. Nothing is redrawn while nothing moves (the
//! gap before each animal and the hold after it): the frame clock is let go
//! of and a timer brings it back. With animations off (the system's reduced
//! motion), the first animal is drawn finished and still.

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::OnceLock,
};

use adw::prelude::*;
use gtk::{glib, graphene, gsk, subclass::prelude::*};
use serde::Deserialize;

/// The file, as the art directory has it.
const PORTRAITS: &str = include_str!("../../../../art/portraits.json");

#[derive(Debug, Deserialize)]
struct File {
    format: u32,
    canvas: Canvas,
    #[serde(rename = "lineWidth")]
    line_width: f32,
    placement: String,
    gap: f64,
    portraits: Vec<PortraitFile>,
}

#[derive(Debug, Deserialize)]
struct Canvas {
    width: f32,
    height: f32,
}

#[derive(Debug, Deserialize)]
struct PortraitFile {
    name: String,
    hold: f64,
    fade: f64,
    strokes: Vec<StrokeFile>,
}

#[derive(Debug, Deserialize)]
struct StrokeFile {
    d: String,
    start: f64,
    duration: f64,
}

/// One stroke, its pieces (one for each `M`) parsed and measured.
pub struct Stroke {
    pieces: Vec<(gsk::Path, gsk::PathMeasure, f32)>,
    length: f32,
    pub start: f64,
    pub duration: f64,
}

pub struct Portrait {
    pub name: String,
    pub hold: f64,
    pub fade: f64,
    pub strokes: Vec<Stroke>,
}

impl Portrait {
    /// The seconds the strokes take, the last one ending.
    pub fn writing(&self) -> f64 {
        self.strokes
            .iter()
            .map(|stroke| stroke.start + stroke.duration)
            .fold(0.0, f64::max)
    }
}

pub struct Art {
    pub canvas: (f32, f32),
    pub line_width: f32,
    pub gap: f64,
    pub portraits: Vec<Portrait>,
}

impl Art {
    /// The seconds one animal's turn takes, from its gap to the end of its
    /// fade.
    pub fn turn(&self, index: usize) -> f64 {
        let portrait = &self.portraits[index];
        self.gap + portrait.writing() + portrait.hold + portrait.fade
    }
}

/// `u * u * (3 - 2u)` for `u` held to [0, 1].
pub fn smoothstep(u: f64) -> f64 {
    let u = u.clamp(0.0, 1.0);
    u * u * (3.0 - 2.0 * u)
}

/// The share of `stroke` that is down `t` seconds into its animal's turn.
pub fn drawn(art: &Art, stroke: &Stroke, t: f64) -> f64 {
    let tau = t - art.gap;
    if tau < 0.0 {
        return 0.0;
    }
    if stroke.duration <= 0.0 {
        return if tau >= stroke.start { 1.0 } else { 0.0 };
    }
    smoothstep((tau - stroke.start) / stroke.duration)
}

/// The drawing's opacity `t` seconds into its animal's turn.
pub fn opacity(art: &Art, portrait: &Portrait, t: f64) -> f64 {
    let tau = t - art.gap;
    if portrait.fade <= 0.0 {
        return if tau < portrait.writing() + portrait.hold {
            1.0
        } else {
            0.0
        };
    }
    1.0 - smoothstep((tau - portrait.writing() - portrait.hold) / portrait.fade)
}

/// Whether anything moves `t` seconds into an animal's turn: not in the gap
/// (nothing drawn) and not in the hold (the drawing finished and still).
/// Returns how long the stillness lasts from `t` when it is still.
pub fn still_for(art: &Art, index: usize, t: f64) -> Option<f64> {
    let portrait = &art.portraits[index];
    let writing = portrait.writing();
    if t < art.gap {
        return Some(art.gap - t);
    }
    let held_from = art.gap + writing;
    let held_until = held_from + portrait.hold;
    (t >= held_from && t < held_until).then_some(held_until - t)
}

fn parse(text: &str) -> Option<Art> {
    let file: File = serde_json::from_str(text).ok()?;
    // A reader that does not know the format draws nothing.
    if file.format != 1 || file.placement != "bottom-center" {
        return None;
    }
    let portraits = file
        .portraits
        .into_iter()
        .map(|portrait| Portrait {
            name: portrait.name,
            hold: portrait.hold,
            fade: portrait.fade,
            strokes: portrait
                .strokes
                .into_iter()
                .filter_map(|stroke| {
                    let pieces: Vec<(gsk::Path, gsk::PathMeasure, f32)> = pieces_of(&stroke.d)
                        .into_iter()
                        .filter_map(|piece| {
                            let path = gsk::Path::parse(&piece).ok()?;
                            let measure = gsk::PathMeasure::new(&path);
                            let length = measure.length();
                            Some((path, measure, length))
                        })
                        .collect();
                    let length = pieces.iter().map(|(_, _, length)| length).sum();
                    (!pieces.is_empty()).then_some(Stroke {
                        pieces,
                        length,
                        start: stroke.start,
                        duration: stroke.duration,
                    })
                })
                .collect(),
        })
        .filter(|portrait: &Portrait| !portrait.strokes.is_empty())
        .collect::<Vec<_>>();
    (!portraits.is_empty()).then_some(Art {
        canvas: (file.canvas.width, file.canvas.height),
        line_width: file.line_width,
        gap: file.gap,
        portraits,
    })
}

/// A stroke's path data cut at each `M`: the pen goes through the pieces in
/// order, and the stroke's length is the sum of theirs.
fn pieces_of(d: &str) -> Vec<String> {
    let mut pieces: Vec<String> = Vec::new();
    for token in d.split_whitespace() {
        if token == "M" || pieces.is_empty() {
            pieces.push(String::new());
        }
        let piece = pieces.last_mut().expect("a piece");
        if !piece.is_empty() {
            piece.push(' ');
        }
        piece.push_str(token);
    }
    pieces
}

thread_local! {
    static ART: OnceLock<Option<Rc<Art>>> = const { OnceLock::new() };
}

/// The art, parsed once per thread (GSK's paths are not shared across
/// threads, and only the main thread draws).
pub fn art() -> Option<Rc<Art>> {
    ART.with(|art| art.get_or_init(|| parse(PORTRAITS).map(Rc::new)).clone())
}

/// Where the canvas goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Placement {
    /// The lobby's stage beside the sidebar (README, "On a desktop lobby
    /// with no card").
    #[default]
    Stage,
    /// A card: the sheets' and the pre-join's picture of yourself
    /// (`bottom-center`, 12 in from either side).
    Card,
}

/// The stage's head and foot, less what of the Mac's 130 and 96 the window's
/// own bar holds: the header bar above the stage is 46 of the Mac's 130.
pub const STAGE_HEAD: f32 = 84.0;
pub const STAGE_FOOT: f32 = 96.0;

/// The canvas's origin and scale in a widget of `width` by `height`.
pub fn place(placement: Placement, canvas: (f32, f32), width: f32, height: f32) -> (f32, f32, f32) {
    match placement {
        Placement::Card => {
            let inset = 12.0;
            let scale = (height / canvas.1)
                .min((width - 2.0 * inset) / canvas.0)
                .max(0.0);
            let x = (width - canvas.0 * scale) / 2.0;
            let y = height - canvas.1 * scale;
            (x, y, scale)
        }
        Placement::Stage => {
            let band = (height - STAGE_HEAD - STAGE_FOOT).max(0.0);
            let side = (band / 0.75).min(width * 0.5).min(480.0);
            let centre = (width / 2.0, STAGE_HEAD + band / 2.0 + side * 0.08);
            let scale = side / canvas.0;
            (centre.0 - side / 2.0, centre.1 - side / 2.0, scale)
        }
    }
}

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct Portraits {
        pub placement: Cell<Placement>,
        pub index: Cell<usize>,
        /// Seconds into the animal's turn.
        pub t: Cell<f64>,
        /// The frame time last seen, in microseconds.
        pub last: Cell<Option<i64>>,
        pub tick: RefCell<Option<gtk::TickCallbackId>>,
        pub wake: RefCell<Option<glib::SourceId>>,
        pub held: Cell<bool>,
        /// A debug run's moment, drawn still.
        pub fixed: Cell<bool>,
        pub dark: RefCell<Option<glib::SignalHandlerId>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Portraits {
        const NAME: &'static str = "TomoshibiPortraits";
        type Type = super::Portraits;
        type ParentType = gtk::Widget;
    }

    impl ObjectImpl for Portraits {
        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();
            obj.set_can_target(false);
            obj.set_hexpand(true);
            obj.set_vexpand(true);
            obj.update_property(&[gtk::accessible::Property::Label("")]);
            let weak = obj.downgrade();
            let handler = adw::StyleManager::default().connect_dark_notify(move |_| {
                if let Some(obj) = weak.upgrade() {
                    obj.queue_draw();
                }
            });
            self.dark.replace(Some(handler));
        }

        fn dispose(&self) {
            if let Some(handler) = self.dark.take() {
                adw::StyleManager::default().disconnect(handler);
            }
            if let Some(source) = self.wake.take() {
                source.remove();
            }
        }
    }

    impl WidgetImpl for Portraits {
        fn map(&self) {
            self.parent_map();
            self.obj().play();
        }

        fn unmap(&self) {
            self.obj().pause();
            self.parent_unmap();
        }

        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            self.obj().draw(snapshot);
        }
    }
}

glib::wrapper! {
    /// The animals, drawn in the widget's whole area.
    pub struct Portraits(ObjectSubclass<imp::Portraits>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Portraits {
    /// Animals placed as `placement` says, starting on the animal and at the
    /// moment a debug run names (`TOMOSHIBI_DEBUG_PORTRAIT` and `_AT`).
    pub fn new(placement: Placement, first: Option<&str>, at: Option<f64>) -> Portraits {
        let portraits: Portraits = glib::Object::new();
        portraits.imp().placement.set(placement);
        if let Some(art) = art() {
            if let Some(index) =
                first.and_then(|name| art.portraits.iter().position(|p| p.name == name))
            {
                portraits.imp().index.set(index);
            }
            if let Some(at) = at {
                portraits
                    .imp()
                    .t
                    .set(at.clamp(0.0, art.turn(portraits.imp().index.get())));
                portraits.imp().fixed.set(true);
            } else if !animations() {
                portraits.imp().index.set(0);
                let finished = art.gap + art.portraits[0].writing();
                portraits.imp().t.set(finished);
                portraits.imp().fixed.set(true);
            }
        }
        portraits
    }

    /// Holds still while something covers it, and resumes from there.
    pub fn set_held(&self, held: bool) {
        if self.imp().held.replace(held) == held {
            return;
        }
        if held {
            self.pause();
        } else if self.is_mapped() {
            self.play();
        }
    }

    /// The animal on now, by name.
    pub fn current(&self) -> Option<String> {
        let art = art()?;
        Some(
            art.portraits[self.imp().index.get() % art.portraits.len()]
                .name
                .clone(),
        )
    }

    fn play(&self) {
        let imp = self.imp();
        if imp.fixed.get() || imp.held.get() || imp.tick.borrow().is_some() {
            return;
        }
        if let Some(source) = imp.wake.take() {
            source.remove();
        }
        imp.last.set(None);
        let id = self.add_tick_callback(|portraits, clock| {
            portraits.advance(clock.frame_time());
            glib::ControlFlow::Continue
        });
        imp.tick.replace(Some(id));
    }

    fn pause(&self) {
        let imp = self.imp();
        if let Some(id) = imp.tick.take() {
            id.remove();
        }
        if let Some(source) = imp.wake.take() {
            source.remove();
        }
        imp.last.set(None);
    }

    fn advance(&self, now: i64) {
        let Some(art) = art() else {
            return;
        };
        let imp = self.imp();
        // At most a tenth of a second a frame: a window that had no frames
        // for a while (covered, on another workspace) resumes from where it
        // was rather than from where the clock has got to.
        let delta = imp
            .last
            .get()
            .map(|last| ((now - last) as f64 / 1_000_000.0).clamp(0.0, 0.1))
            .unwrap_or(0.0);
        imp.last.set(Some(now));
        let mut index = imp.index.get() % art.portraits.len();
        let mut t = imp.t.get() + delta;
        while t >= art.turn(index) {
            t -= art.turn(index);
            index = (index + 1) % art.portraits.len();
        }
        imp.index.set(index);
        imp.t.set(t);
        self.queue_draw();
        if let Some(still) = still_for(&art, index, t) {
            // Nothing moves for this long: the frame clock is let go of,
            // and a timer brings it back at the next movement.
            if let Some(id) = imp.tick.take() {
                id.remove();
            }
            let weak = self.downgrade();
            let source = glib::timeout_add_local_once(
                std::time::Duration::from_secs_f64(still.max(0.001)),
                move || {
                    if let Some(portraits) = weak.upgrade() {
                        let imp = portraits.imp();
                        imp.wake.take();
                        imp.t.set(imp.t.get() + still);
                        if portraits.is_mapped() && !imp.held.get() {
                            portraits.play();
                        }
                    }
                },
            );
            imp.wake.replace(Some(source));
        }
    }

    fn draw(&self, snapshot: &gtk::Snapshot) {
        let Some(art) = art() else {
            return;
        };
        let imp = self.imp();
        let index = imp.index.get() % art.portraits.len();
        let portrait = &art.portraits[index];
        let t = imp.t.get();
        let fade = opacity(&art, portrait, t);
        if fade <= 0.0 {
            return;
        }
        let builder = gsk::PathBuilder::new();
        let mut any = false;
        for stroke in &portrait.strokes {
            let share = drawn(&art, stroke, t) as f32;
            if share <= 0.0 {
                continue;
            }
            let mut left = share * stroke.length;
            for (path, measure, length) in &stroke.pieces {
                if left <= 0.0 {
                    break;
                }
                if left >= *length - 0.001 {
                    builder.add_path(path);
                    any = true;
                } else if left > 0.01 {
                    // A segment whose start and end are one point would be
                    // the whole path to GSK, so a sliver is left undrawn.
                    if let (Some(start), Some(end)) = (path.start_point(), measure.point(left)) {
                        builder.add_segment(path, &start, &end);
                        any = true;
                    }
                }
                left -= length;
            }
        }
        if !any {
            return;
        }
        let path = builder.to_path();
        let (width, height) = (self.width() as f32, self.height() as f32);
        let (x, y, scale) = place(imp.placement.get(), art.canvas, width, height);
        if scale <= 0.0 {
            return;
        }
        let dark = adw::StyleManager::default().is_dark();
        // Night: #F5F5F7 at 60 per cent; day: #1C1C1E at 50 (README, "The
        // lines").
        let (colour, alpha) = if dark {
            (crate::theme::Shade::hex(0xF5F5F7), 0.60)
        } else {
            (crate::theme::Shade::hex(0x1C1C1E), 0.50)
        };
        let ink = colour.with_alpha((alpha * fade) as f32).rgba();
        let stroke = gsk::Stroke::new(art.line_width);
        stroke.set_line_cap(gsk::LineCap::Round);
        stroke.set_line_join(gsk::LineJoin::Round);
        snapshot.save();
        snapshot.translate(&graphene::Point::new(x, y));
        snapshot.scale(scale, scale);
        snapshot.append_stroke(&path, &stroke, &ink);
        snapshot.restore();
    }
}

/// Whether the system lets things move (`gtk-enable-animations`, which
/// GNOME's reduce-motion setting turns off).
pub fn animations() -> bool {
    gtk::Settings::default().is_none_or(|settings| settings.is_gtk_enable_animations())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_file_is_read_whole() {
        let art = art().expect("portraits.json reads");
        let names: Vec<&str> = art.portraits.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(
            names,
            ["cat", "rabbit", "bear", "hamster", "penguin", "chick"]
        );
        assert_eq!(art.canvas, (100.0, 100.0));
        assert!((art.line_width - 2.4).abs() < 1e-6);
        for portrait in &art.portraits {
            for stroke in &portrait.strokes {
                assert!(
                    stroke.length > 0.0,
                    "{} has a stroke of no length",
                    portrait.name
                );
            }
        }
        // The cat's eyes are one stroke of two pieces, the pen going
        // through both.
        assert_eq!(art.portraits[0].strokes[1].pieces.len(), 2);
    }

    #[test]
    fn the_timing_is_the_readmes() {
        let art = art().unwrap();
        let cat = &art.portraits[0];
        // writing = max(start + duration) = 1.817 + 0.16.
        assert!((cat.writing() - 1.977).abs() < 1e-9);
        assert!((art.turn(0) - (0.4 + 1.977 + 2.0 + 0.6)).abs() < 1e-9);
        let outline = &cat.strokes[0];
        assert_eq!(drawn(&art, outline, 0.2), 0.0, "nothing in the gap");
        assert!((drawn(&art, outline, 0.4 + outline.duration / 2.0) - 0.5).abs() < 1e-9);
        assert_eq!(drawn(&art, outline, 0.4 + outline.duration), 1.0);
        assert_eq!(opacity(&art, cat, 0.4 + 1.977 + 1.0), 1.0, "held");
        assert!((opacity(&art, cat, 0.4 + 1.977 + 2.0 + 0.3) - 0.5).abs() < 1e-9);
        assert_eq!(opacity(&art, cat, art.turn(0)), 0.0);
    }

    #[test]
    fn only_the_gap_and_the_hold_are_still() {
        let art = art().unwrap();
        assert!((still_for(&art, 0, 0.1).unwrap() - 0.3).abs() < 1e-9);
        assert_eq!(still_for(&art, 0, 1.0), None, "writing");
        assert!((still_for(&art, 0, 0.4 + 1.977 + 0.5).unwrap() - 1.5).abs() < 1e-9);
        assert_eq!(still_for(&art, 0, 0.4 + 1.977 + 2.1), None, "fading");
    }

    #[test]
    fn the_stage_puts_the_drawing_in_the_middle_of_its_band() {
        // A stage of 900 by 700: band = 700 - 84 - 96 = 520, side =
        // min(520 / 0.75, 450, 480) = 450.
        let (x, y, scale) = place(Placement::Stage, (100.0, 100.0), 900.0, 700.0);
        assert!((scale - 4.5).abs() < 1e-4);
        assert!((x - 225.0).abs() < 1e-3);
        // The drawing's middle, canvas y = 42, is the band's middle.
        assert!((y + 42.0 * scale - (84.0 + 260.0)).abs() < 1e-3);
    }

    #[test]
    fn a_card_stands_the_canvas_on_its_foot() {
        let (x, y, scale) = place(Placement::Card, (100.0, 100.0), 500.0, 280.0);
        assert!((scale - 2.8).abs() < 1e-5);
        assert!((x - 110.0).abs() < 1e-3);
        assert!(y.abs() < 1e-3);
    }

    #[test]
    fn a_stroke_is_cut_at_each_move() {
        assert_eq!(
            pieces_of("M 1 2 L 3 4 M 5 6 L 7 8"),
            ["M 1 2 L 3 4", "M 5 6 L 7 8"]
        );
    }
}
