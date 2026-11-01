//! Where every picture sits, as coordinates rather than as a tree: the
//! web's plan.ts and layout.ts, through the Mac's port (TomoshibiBasics
//! Plan.swift), rule for rule.
//!
//! The web learned this the hard way: an arrangement expressed by which
//! container a tile is in destroys and rebuilds the tile whenever the
//! arrangement changes, and a picture that is rebuilt resubscribes and waits
//! for a keyframe, the black screen. A picture placed by coordinates is
//! built once for as long as its owner is in the call, and moving it to the
//! stage is an animation rather than an interruption (stage.rs places the
//! tiles from what this says).
//!
//! Nothing here touches GTK, so that the rules are tested as the Mac's are
//! (the web's own cases, plan.test.ts and usePin.test.tsx).

use std::collections::{HashMap, HashSet};

/// A place on screen, in the room's own coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Rect {
    pub const fn new(x: f64, y: f64, width: f64, height: f64) -> Rect {
        Rect {
            x,
            y,
            width,
            height,
        }
    }

    pub fn max_x(&self) -> f64 {
        self.x + self.width
    }

    pub fn max_y(&self) -> f64 {
        self.y + self.height
    }

    pub fn area(&self) -> f64 {
        self.width * self.height
    }

    /// The place `t` of the way from `self` to `to`.
    pub fn toward(&self, to: &Rect, t: f64) -> Rect {
        let at = |a: f64, b: f64| a + (b - a) * t;
        Rect::new(
            at(self.x, to.x),
            at(self.y, to.y),
            at(self.width, to.width),
            at(self.height, to.height),
        )
    }
}

/// A container's size.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Size {
    pub width: f64,
    pub height: f64,
}

impl Size {
    pub const fn new(width: f64, height: f64) -> Size {
        Size { width, height }
    }
}

/// Space between pictures, and between a picture and the edge.
pub const GAP: f64 = 8.0;
/// Nine keeps a grid readable on a laptop and caps how many streams the call
/// asks for at once.
pub const TILES_PER_PAGE: usize = 9;
/// The controls' reserve at the foot: the bar's 58 and the 20 below it.
pub const ISLAND: f64 = 20.0 + 58.0 + GAP;
/// Two people are one person being looked at and one being checked on.
pub const ALONE_TOGETHER: usize = 2;
const STRIP: f64 = 112.0;
const HEADROOM: f64 = 56.0;
const LANDSCAPE: f64 = 16.0 / 9.0;
const UPRIGHT_ASPECT: f64 = 3.0 / 4.0;
const FILL: f64 = 0.92;
/// What a room that pages keeps clear either side for its arrows, so that
/// they sit beside the pictures they turn rather than over the first and the
/// last of them.
pub const PAGER_RESERVE: f64 = 52.0;

/// A screen held upright, rather than a window that happens to be tall: a
/// fifth taller than wide, so a window dragged through square does not
/// reshape the call under the pointer.
pub fn upright(container: Size) -> bool {
    container.height > container.width * 1.2
}

fn aspect(container: Size) -> f64 {
    if upright(container) {
        UPRIGHT_ASPECT
    } else {
        LANDSCAPE
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Arrangement {
    columns: usize,
    rows: usize,
    width: f64,
    height: f64,
}

/// The column count that gives each person the most picture (layout.ts).
fn arrange(container: Size, count: usize, gap: f64, aspect: f64) -> Option<Arrangement> {
    let (width, height) = (container.width, container.height);
    if count == 0 || width <= 0.0 || height <= 0.0 {
        return None;
    }
    let mut best = None;
    let mut best_score = 0.0;
    for columns in 1..=count {
        let rows = count.div_ceil(columns);
        let cell_width = (width - (columns - 1) as f64 * gap) / columns as f64;
        let cell_height = (height - (rows - 1) as f64 * gap) / rows as f64;
        if cell_width <= 0.0 || cell_height <= 0.0 {
            continue;
        }
        let mut tile_width = cell_width.min(cell_height * aspect);
        let mut tile_height = tile_width / aspect;
        let ceiling_fill = if container.height > container.width * 1.2 {
            1.0
        } else {
            FILL
        };
        let ceiling = (height * ceiling_fill - (rows - 1) as f64 * gap) / rows as f64;
        if tile_height > ceiling {
            tile_height = ceiling;
            tile_width = tile_height * aspect;
        }
        let score = tile_width * tile_height * (count as f64 / (columns * rows) as f64);
        if score > best_score {
            best_score = score;
            best = Some(Arrangement {
                columns,
                rows,
                width: tile_width.floor(),
                height: tile_height.floor(),
            });
        }
    }
    best
}

pub type Spots = HashMap<String, Rect>;

/// A grid, centred, each row centred on its own, keeping `sides` clear at
/// the left and right and never lower than the controls allow (the Mac's
/// two departures from plan.ts: six people in a wide window were drawn edge
/// to edge, and a full grid's last row ran a third of the way under the
/// bar).
pub fn grid(container: Size, ids: &[String], island: f64, sides: f64) -> Spots {
    let mut plan = Spots::new();
    let inner = Size::new(
        (container.width - 2.0 * sides).max(0.0),
        (container.height - island).max(0.0),
    );
    let Some(layout) = arrange(inner, ids.len(), GAP, aspect(container)) else {
        return plan;
    };
    let grid_height = layout.rows as f64 * layout.height + (layout.rows - 1) as f64 * GAP;
    let top = ((container.height - grid_height) / 2.0)
        .min(container.height - island - grid_height)
        .max(0.0);
    for (index, id) in ids.iter().enumerate() {
        let row = index / layout.columns;
        let column = index % layout.columns;
        let in_row = if row == layout.rows - 1 {
            ids.len() - row * layout.columns
        } else {
            layout.columns
        };
        let row_width = in_row as f64 * layout.width + (in_row - 1) as f64 * GAP;
        plan.insert(
            id.clone(),
            Rect::new(
                (container.width - row_width) / 2.0 + column as f64 * (layout.width + GAP),
                top + row as f64 * (layout.height + GAP),
                layout.width,
                layout.height,
            ),
        );
    }
    plan
}

/// One picture on the stage, the rest in a filmstrip beneath.
pub fn focus(
    container: Size,
    stage: &str,
    strip: &[String],
    fullscreen: bool,
    island: f64,
) -> Spots {
    let mut plan = Spots::new();
    if container.width <= 0.0 || container.height <= 0.0 {
        return plan;
    }
    if fullscreen {
        plan.insert(
            stage.to_owned(),
            Rect::new(0.0, 0.0, container.width, container.height),
        );
        return plan;
    }
    let bottom = container.height - island;
    let show_strip = !strip.is_empty();
    let strip_top = bottom - STRIP;
    plan.insert(
        stage.to_owned(),
        Rect::new(
            GAP,
            GAP,
            (container.width - 2.0 * GAP).max(0.0),
            ((if show_strip { strip_top - GAP } else { bottom }) - GAP).max(0.0),
        ),
    );
    if !show_strip {
        return plan;
    }
    let width = STRIP * aspect(container);
    let total = strip.len() as f64 * width + (strip.len() - 1) as f64 * GAP;
    let left = (container.width - total) / 2.0;
    for (index, id) in strip.iter().enumerate() {
        plan.insert(
            id.clone(),
            Rect::new(left + index as f64 * (width + GAP), strip_top, width, STRIP),
        );
    }
    plan
}

/// How many the filmstrip holds at this width; never fewer than one.
pub fn strip_capacity(container: Size) -> usize {
    let width = STRIP * aspect(container);
    let usable = container.width - 2.0 * GAP + GAP;
    ((usable / (width + GAP)).floor().max(0.0) as usize).max(1)
}

/// The other person filling the screen and the rest as cards down the
/// trailing edge, the way every telephone draws a call of two.
pub fn close(container: Size, filling: &str, cards: &[String], island: f64) -> Spots {
    let mut plan = Spots::new();
    if container.width <= 0.0 || container.height <= 0.0 {
        return plan;
    }
    plan.insert(
        filling.to_owned(),
        Rect::new(0.0, 0.0, container.width, container.height),
    );
    let share = if upright(container) { 0.26 } else { 0.18 };
    let width = (container.width * share).min((container.height - island - 2.0 * GAP) / 2.0);
    let height = width / aspect(container);
    for (index, id) in cards.iter().enumerate() {
        let top = HEADROOM + index as f64 * (height + GAP);
        if top + height > container.height - island {
            continue;
        }
        plan.insert(
            id.clone(),
            Rect::new(container.width - width - GAP, top, width, height),
        );
    }
    plan
}

/// How far above the bottom something drawn over the pictures must end to
/// leave these ones uncovered (plan.ts `clearOf`).
pub fn clear_of(plan: &Spots, ids: &[String], container: Size) -> Option<f64> {
    let highest = ids
        .iter()
        .filter_map(|id| plan.get(id).map(|spot| spot.y))
        .reduce(f64::min)?;
    Some(container.height - highest + GAP)
}

/// How much further up a card that ends `ends` above the bottom has to go
/// to end `clear` above it.
pub fn lift(clear: Option<f64>, ends: f64) -> f64 {
    clear.map_or(0.0, |clear| (clear - ends).max(0.0))
}

/// Everything the room needs from one arrangement: where each picture is,
/// and which page of how many is showing.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Room {
    pub spots: Spots,
    /// The page showing, already brought back inside the pages there are.
    pub page: usize,
    pub pages: usize,
    /// The pictures drawn below or around the stage, in order: the strip,
    /// the grid, or the card in a call of two.
    pub shown: Vec<String>,
}

/// Where every picture goes, as Room.tsx's `Stage` works it out.
///
/// `pictures` is everybody, in the order they are drawn; `mine` is this
/// participant's own camera, which never pages away (losing sight of
/// yourself because the room got busy is disorienting, and it costs
/// nothing) and takes one of the page's places rather than being added
/// beyond them. `stage` is whatever is pinned.
pub fn room(
    container: Size,
    pictures: &[String],
    mine: Option<&str>,
    stage: Option<&str>,
    fullscreen: bool,
    page: usize,
    island: f64,
) -> Room {
    let stage = stage.filter(|id| pictures.iter().any(|picture| picture == id));
    let me = mine.filter(|id| pictures.iter().any(|picture| picture == id));
    let kept: Vec<String> = me
        .filter(|me| Some(*me) != stage)
        .map(|me| vec![me.to_owned()])
        .unwrap_or_default();
    let rest: Vec<String> = pictures
        .iter()
        .filter(|id| Some(id.as_str()) != me && Some(id.as_str()) != stage)
        .cloned()
        .collect();
    let places = |width: f64| {
        let capacity = if stage.is_some() {
            strip_capacity(Size::new(width, container.height))
        } else {
            TILES_PER_PAGE
        };
        capacity.saturating_sub(kept.len()).max(1)
    };
    // A room that will page gives its arrows room, and the strip holds what
    // fits beside them.
    let paged = rest.len() > places(container.width);
    let per = places(if paged {
        container.width - 2.0 * PAGER_RESERVE
    } else {
        container.width
    });
    let pages = rest.len().div_ceil(per).max(1);
    let showing = page.min(pages - 1);
    let start = (showing * per).min(rest.len());
    let end = (start + per).min(rest.len());
    let mut shown = kept.clone();
    shown.extend(rest[start..end].iter().cloned());
    let spots = if let Some(stage) = stage {
        let strip: &[String] = if fullscreen { &[] } else { &shown };
        focus(container, stage, strip, fullscreen, island)
    } else if pictures.len() <= ALONE_TOGETHER && !rest.is_empty() {
        // Two people is one filling the screen and the other a card, the
        // way a telephone has always drawn it.
        let cards: Vec<String> = me.map(|me| vec![me.to_owned()]).unwrap_or_default();
        close(container, &rest[0], &cards, island)
    } else {
        grid(
            container,
            &shown,
            island,
            if paged { PAGER_RESERVE } else { GAP },
        )
    };
    Room {
        spots,
        page: showing,
        pages,
        shown,
    }
}

/// Which picture owns the stage (hooks/usePin.ts, the Mac's `StagePin`).
///
/// A share takes the stage when it starts and gives it back when it stops,
/// and the rule the web names as the one easy to miss: only a pin made
/// automatically is taken away automatically. Somebody who put a person on
/// the stage by hand keeps them there when a share starts or stops. A share
/// claims the stage when it starts, or when the stage is emptied by
/// something other than the person looking; choosing the grid is a choice,
/// and it holds until a new share starts.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct StagePin {
    pinned: Option<String>,
    auto: Option<String>,
    seen: HashSet<String>,
}

impl StagePin {
    pub fn pinned(&self) -> Option<&str> {
        self.pinned.as_deref()
    }

    /// A choice by hand: a picture, or none for everybody.
    pub fn pin(&mut self, id: Option<String>) {
        self.auto = None;
        self.pinned = id;
    }

    /// What a press on a picture does: on to the stage, or back off it.
    pub fn toggle(&mut self, id: &str) {
        self.auto = None;
        self.pinned = if self.pinned.as_deref() == Some(id) {
            None
        } else {
            Some(id.to_owned())
        };
    }

    /// Brings the pin up to date with the room: `pictures` is every picture
    /// there is, `shares` those that take the stage by themselves, first
    /// first.
    pub fn update(&mut self, pictures: &[String], shares: &[String]) {
        let mut emptied = false;
        if let Some(auto) = self.auto.clone() {
            if !shares.contains(&auto) {
                self.auto = None;
                if self.pinned.as_deref() == Some(auto.as_str()) {
                    self.pinned = None;
                }
                emptied = true;
            }
        }
        // A pinned participant who leaves must not take the stage with them.
        if let Some(pinned) = self.pinned.clone() {
            if !pictures.contains(&pinned) {
                self.pinned = None;
                self.auto = None;
                emptied = true;
            }
        }
        if self.pinned.is_none() {
            let claim = if emptied {
                shares.first()
            } else {
                shares.iter().find(|share| !self.seen.contains(*share))
            };
            if let Some(claim) = claim {
                self.pinned = Some(claim.clone());
                self.auto = Some(claim.clone());
            }
        }
        self.seen = shares.iter().cloned().collect();
    }
}

/// When each person was last heard, for bringing speakers forward in a room
/// that runs to more than one page (hooks/useSpeakingOrder.ts). By when they
/// were last heard rather than whether they are speaking now: speech is full
/// of gaps, and sorting on the moment would drop somebody down the grid
/// between two words of a sentence.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SpeakingRecord {
    heard: HashMap<String, f64>,
}

impl SpeakingRecord {
    /// Notes who is speaking at `now`, and forgets whoever is not `present`.
    pub fn hear(&mut self, speaking: &[String], present: &[String], now: f64) {
        for identity in speaking {
            self.heard.insert(identity.clone(), now);
        }
        self.heard
            .retain(|identity, _| present.iter().any(|here| here == identity));
    }

    /// `ids` with the most recently heard first; ties keep their order.
    pub fn order(&self, ids: &[String]) -> Vec<String> {
        let mut indexed: Vec<(usize, &String)> = ids.iter().enumerate().collect();
        indexed.sort_by(|(a_at, a), (b_at, b)| {
            let (left, right) = (
                self.heard.get(*a).copied().unwrap_or(0.0),
                self.heard.get(*b).copied().unwrap_or(0.0),
            );
            right
                .partial_cmp(&left)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a_at.cmp(b_at))
        });
        indexed.into_iter().map(|(_, id)| id.clone()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WINDOW: Size = Size::new(1600.0, 900.0);
    const PHONE: Size = Size::new(393.0, 852.0);

    fn ids(count: usize) -> Vec<String> {
        (0..count).map(|n| format!("p{n}")).collect()
    }

    fn with(first: &[&str], more: usize) -> Vec<String> {
        first
            .iter()
            .map(|id| id.to_string())
            .chain(ids(more))
            .collect()
    }

    fn overlaps(a: &Rect, b: &Rect) -> bool {
        let tolerance = 0.5;
        a.max_x() - tolerance > b.x
            && b.max_x() - tolerance > a.x
            && a.max_y() - tolerance > b.y
            && b.max_y() - tolerance > a.y
    }

    #[test]
    fn the_grid_places_everybody_once_without_overlap() {
        for count in 1..=9 {
            let plan = grid(WINDOW, &ids(count), ISLAND, GAP);
            assert_eq!(plan.len(), count);
            let placed: Vec<&Rect> = plan.values().collect();
            for i in 0..placed.len() {
                for j in i + 1..placed.len() {
                    assert!(!overlaps(placed[i], placed[j]), "{count} people");
                }
            }
        }
    }

    #[test]
    fn the_grid_keeps_clear_of_the_controls_and_inside_the_window() {
        for size in [WINDOW, Size::new(1280.0, 772.0), PHONE] {
            for count in 1..=9 {
                for spot in grid(size, &ids(count), ISLAND, GAP).values() {
                    assert!(
                        spot.max_y() <= size.height - ISLAND + 0.5,
                        "{count} in {size:?}"
                    );
                    assert!(spot.x >= GAP - 0.5, "{count} in {size:?}");
                    assert!(spot.y >= 0.0);
                    assert!(
                        spot.max_x() <= size.width - GAP + 0.5,
                        "{count} in {size:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn a_room_that_pages_leaves_its_arrows_room() {
        let everybody = with(&["me"], 20);
        let grid = room(WINDOW, &everybody, Some("me"), None, false, 0, ISLAND);
        for spot in grid.spots.values() {
            assert!(spot.x >= PAGER_RESERVE - 0.5);
            assert!(spot.max_x() <= WINDOW.width - PAGER_RESERVE + 0.5);
        }
        let everybody = with(&["me", "stage"], 20);
        let strip = room(
            WINDOW,
            &everybody,
            Some("me"),
            Some("stage"),
            false,
            0,
            ISLAND,
        );
        for id in &strip.shown {
            assert!(strip.spots[id].x >= PAGER_RESERVE - 0.5);
            assert!(strip.spots[id].max_x() <= WINDOW.width - PAGER_RESERVE + 0.5);
        }
    }

    #[test]
    fn the_grid_centres_a_short_last_row() {
        let plan = grid(Size::new(1200.0, 900.0), &ids(5), ISLAND, GAP);
        let lowest = plan.values().map(|spot| spot.y).fold(f64::MIN, f64::max);
        let last: Vec<&Rect> = plan.values().filter(|spot| spot.y == lowest).collect();
        let left = last.iter().map(|spot| spot.x).fold(f64::MAX, f64::min);
        let right = last
            .iter()
            .map(|spot| spot.max_x())
            .fold(f64::MIN, f64::max);
        assert!((left - (1200.0 - right)).abs() < 0.001);
    }

    #[test]
    fn nothing_is_planned_unmeasured() {
        assert!(grid(Size::default(), &ids(4), ISLAND, GAP).is_empty());
        assert!(grid(WINDOW, &[], ISLAND, GAP).is_empty());
    }

    #[test]
    fn the_stage_is_above_the_strip_and_both_clear_of_the_controls() {
        let plan = focus(WINDOW, "stage", &ids(4), false, ISLAND);
        let stage = plan["stage"];
        for id in ids(4) {
            let spot = plan[&id];
            assert!(spot.y >= stage.max_y());
            assert!(spot.max_y() <= WINDOW.height - ISLAND + 0.5);
        }
    }

    #[test]
    fn the_stage_fills_the_screen_when_asked() {
        let plan = focus(WINDOW, "stage", &ids(3), true, ISLAND);
        assert_eq!(plan.len(), 1);
        assert_eq!(
            plan["stage"],
            Rect::new(0.0, 0.0, WINDOW.width, WINDOW.height)
        );
    }

    #[test]
    fn the_strip_always_holds_somebody() {
        assert_eq!(strip_capacity(Size::new(10.0, 900.0)), 1);
        assert!(strip_capacity(Size::new(1600.0, 900.0)) > strip_capacity(Size::new(800.0, 900.0)));
    }

    #[test]
    fn a_phone_is_filled_rather_than_banded() {
        // The web measured 0.28 before it learned to turn tiles upright.
        for count in [3, 4, 6] {
            let used: f64 = grid(PHONE, &ids(count), ISLAND, GAP)
                .values()
                .map(Rect::area)
                .sum();
            assert!(
                used / (PHONE.width * (PHONE.height - ISLAND)) > 0.45,
                "{count} people"
            );
        }
    }

    #[test]
    fn upright_pictures_are_upright() {
        let plan = grid(PHONE, &ids(4), ISLAND, GAP);
        let spot = plan.values().next().unwrap();
        assert!(spot.height > spot.width);
        let plan = grid(WINDOW, &ids(4), ISLAND, GAP);
        let wide = plan.values().next().unwrap();
        assert!(wide.width > wide.height);
    }

    #[test]
    fn two_people_are_a_conversation() {
        let plan = close(PHONE, "them", &["me".to_owned()], ISLAND);
        assert_eq!(plan["them"], Rect::new(0.0, 0.0, PHONE.width, PHONE.height));
        let me = plan["me"];
        assert!(me.max_x() <= PHONE.width);
        assert!(me.max_y() <= PHONE.height - ISLAND);
        assert!(me.width < PHONE.width / 2.0);
    }

    #[test]
    fn a_card_that_would_not_fit_is_dropped() {
        let short = Size::new(393.0, 300.0);
        let plan = close(short, "them", &ids(6), ISLAND);
        for (id, spot) in &plan {
            if id != "them" {
                assert!(spot.max_y() <= short.height - ISLAND);
            }
        }
    }

    #[test]
    fn your_own_picture_never_pages_away() {
        let everybody = with(&["me"], 20);
        let mut seen = HashSet::new();
        let first = room(WINDOW, &everybody, Some("me"), None, false, 0, ISLAND);
        assert_eq!(first.pages, 3);
        for page in 0..first.pages {
            let at = room(WINDOW, &everybody, Some("me"), None, false, page, ISLAND);
            assert!(at.spots.contains_key("me"), "page {page}");
            assert_eq!(at.shown.first().map(String::as_str), Some("me"));
            assert!(at.spots.len() <= TILES_PER_PAGE);
            seen.extend(at.shown);
        }
        assert_eq!(
            seen,
            everybody.into_iter().collect::<HashSet<_>>(),
            "every picture is on some page"
        );
    }

    #[test]
    fn your_own_picture_stays_in_the_strip_too() {
        let everybody = with(&["me", "stage"], 30);
        let at = room(
            WINDOW,
            &everybody,
            Some("me"),
            Some("stage"),
            false,
            2,
            ISLAND,
        );
        assert!(at.spots.contains_key("me"));
        assert!(at.spots.contains_key("stage"));
        let places = strip_capacity(Size::new(WINDOW.width - 2.0 * PAGER_RESERVE, WINDOW.height));
        assert_eq!(at.spots.len(), places + 1);
    }

    #[test]
    fn a_page_that_no_longer_exists_shows_the_last_one() {
        let at = room(
            WINDOW,
            &with(&["me"], 10),
            Some("me"),
            None,
            false,
            7,
            ISLAND,
        );
        assert_eq!(at.page, at.pages - 1);
        assert!(!at.shown.is_empty());
    }

    #[test]
    fn two_people_are_drawn_as_a_call_of_two() {
        let at = room(
            PHONE,
            &with(&["me", "them"], 0),
            Some("me"),
            None,
            false,
            0,
            ISLAND,
        );
        assert_eq!(
            at.spots["them"],
            Rect::new(0.0, 0.0, PHONE.width, PHONE.height)
        );
        assert!(at.spots["me"].width < PHONE.width / 2.0);
    }

    #[test]
    fn filling_the_screen_draws_the_stage_alone() {
        let at = room(
            WINDOW,
            &with(&["me", "stage"], 4),
            Some("me"),
            Some("stage"),
            true,
            0,
            ISLAND,
        );
        assert_eq!(at.spots.len(), 1);
        assert_eq!(
            at.spots["stage"],
            Rect::new(0.0, 0.0, WINDOW.width, WINDOW.height)
        );
    }

    #[test]
    fn a_stage_that_is_not_in_the_room_is_the_grid() {
        let at = room(
            WINDOW,
            &with(&["me"], 4),
            Some("me"),
            Some("gone"),
            false,
            0,
            ISLAND,
        );
        assert_eq!(at.spots.len(), 5);
        assert!(!at.spots.contains_key("gone"));
    }

    #[test]
    fn a_card_is_lifted_clear_of_the_filmstrip() {
        let at = room(
            WINDOW,
            &with(&["me", "stage"], 3),
            Some("me"),
            Some("stage"),
            false,
            0,
            ISLAND,
        );
        let clear = clear_of(&at.spots, &at.shown, WINDOW);
        let strip_top = at
            .shown
            .iter()
            .map(|id| at.spots[id].y)
            .fold(f64::MAX, f64::min);
        assert_eq!(clear, Some(WINDOW.height - strip_top + GAP));
        let ends = ISLAND + 8.0 + 10.0;
        let lifted = lift(clear, ends);
        assert!(lifted > 0.0);
        assert!(WINDOW.height - (ends + lifted) <= strip_top - GAP);
        assert_eq!(lift(clear, clear.unwrap() + 40.0), 0.0);
        let grid = room(
            WINDOW,
            &with(&["me"], 3),
            Some("me"),
            None,
            false,
            0,
            ISLAND,
        );
        assert_eq!(clear_of(&grid.spots, &[], WINDOW), None);
        assert_eq!(lift(None, ends), 0.0);
    }

    fn named(all: &[&str]) -> Vec<String> {
        all.iter().map(|id| id.to_string()).collect()
    }

    #[test]
    fn a_share_takes_the_stage_when_it_starts_and_gives_it_back() {
        let mut pin = StagePin::default();
        pin.update(&named(&["a", "b"]), &[]);
        assert_eq!(pin.pinned(), None);
        pin.update(&named(&["a", "b", "a/screen"]), &named(&["a/screen"]));
        assert_eq!(pin.pinned(), Some("a/screen"));
        pin.update(&named(&["a", "b"]), &[]);
        assert_eq!(pin.pinned(), None);
    }

    #[test]
    fn a_share_already_running_on_arrival_takes_the_stage() {
        let mut pin = StagePin::default();
        pin.update(&named(&["a", "a/screen"]), &named(&["a/screen"]));
        assert_eq!(pin.pinned(), Some("a/screen"));
    }

    #[test]
    fn the_grid_can_be_chosen_while_somebody_shares() {
        let mut pin = StagePin::default();
        pin.update(&named(&["a", "b", "a/screen"]), &named(&["a/screen"]));
        pin.toggle("a/screen");
        pin.update(&named(&["a", "b", "a/screen"]), &named(&["a/screen"]));
        assert_eq!(pin.pinned(), None);
        pin.update(
            &named(&["a", "b", "a/screen", "b/screen"]),
            &named(&["a/screen", "b/screen"]),
        );
        assert_eq!(pin.pinned(), Some("b/screen"));
    }

    #[test]
    fn a_share_does_not_take_a_stage_pinned_by_hand() {
        let mut pin = StagePin::default();
        pin.update(&named(&["a", "b"]), &[]);
        pin.pin(Some("b".into()));
        pin.update(&named(&["a", "b", "a/screen"]), &named(&["a/screen"]));
        assert_eq!(pin.pinned(), Some("b"));
        pin.update(&named(&["a", "b"]), &[]);
        assert_eq!(
            pin.pinned(),
            Some("b"),
            "an unrelated share ending keeps a pin made by hand"
        );
    }

    #[test]
    fn a_pinned_person_leaving_empties_the_stage() {
        let mut pin = StagePin::default();
        pin.update(&named(&["a", "b"]), &[]);
        pin.pin(Some("b".into()));
        pin.update(&named(&["a"]), &[]);
        assert_eq!(pin.pinned(), None);
    }

    #[test]
    fn the_same_tile_twice_toggles_off() {
        let mut pin = StagePin::default();
        pin.toggle("a");
        assert_eq!(pin.pinned(), Some("a"));
        pin.toggle("a");
        assert_eq!(pin.pinned(), None);
    }

    #[test]
    fn the_first_share_keeps_the_stage() {
        let mut pin = StagePin::default();
        pin.update(&named(&["a", "b", "a/screen"]), &named(&["a/screen"]));
        pin.update(
            &named(&["a", "b", "a/screen", "b/screen"]),
            &named(&["a/screen", "b/screen"]),
        );
        assert_eq!(
            pin.pinned(),
            Some("a/screen"),
            "a second share does not fight the first"
        );
    }

    #[test]
    fn the_last_heard_come_first_and_ties_keep_their_order() {
        let mut record = SpeakingRecord::default();
        let everybody = named(&["a", "b", "c", "d"]);
        assert_eq!(record.order(&everybody), everybody, "nobody has spoken");
        record.hear(&named(&["c"]), &everybody, 1.0);
        record.hear(&named(&["b"]), &everybody, 2.0);
        assert_eq!(record.order(&everybody), named(&["b", "c", "a", "d"]));
        // Somebody who left forgets their place.
        record.hear(&[], &named(&["a", "c", "d"]), 3.0);
        assert_eq!(record.order(&everybody), named(&["c", "a", "b", "d"]));
    }

    #[test]
    fn a_rect_moves_part_of_the_way() {
        let from = Rect::new(0.0, 0.0, 100.0, 50.0);
        let to = Rect::new(100.0, 50.0, 200.0, 150.0);
        assert_eq!(from.toward(&to, 0.5), Rect::new(50.0, 25.0, 150.0, 100.0));
        assert_eq!(from.toward(&to, 1.0), to);
    }
}
