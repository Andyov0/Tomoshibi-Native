//! A month to choose a day from (the Mac's `MonthChoice` in Kit+Settings,
//! which the schedule and the guest links use), drawn here rather than with
//! GtkCalendar for three reasons the pictures showed: GtkCalendar's days are
//! 38 high where a day is a finger's 44 (UI sync), it names the months and
//! the weekdays in the system's language rather than the app's (a Japanese
//! window had "November" over "Sun Mon"), and it lets the past be chosen.
//!
//! The heading is the month and a step either way; under it the weekdays
//! from the language's own first day of the week, and the month's days in
//! six rows, so that the card is the same height whatever the month. The
//! chosen day is the tally's disc, today is a ring of it, and the days that
//! cannot be chosen are faint. A new month slides in from the side it is
//! on, and the chosen disc moves with a crossfade of its colour.

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use adw::prelude::*;

use super::pieces::{self, icon, set_class};
use crate::{
    app::{schedule::Day, words},
    i18n::{self, t},
    theme::motion,
};

/// The rows of days: the most any month needs, so that every month is as
/// tall as every other.
const ROWS: usize = 6;

/// The day of the week of `day`, 0 for Sunday (1970-01-01 was a Thursday).
pub fn weekday(day: Day) -> u32 {
    (day.days() + 4).rem_euclid(7) as u32
}

fn days_in_month(year: i32, month: u32) -> u32 {
    let (next_year, next_month) = if month == 12 {
        (year + 1, 1)
    } else {
        (year, month + 1)
    };
    (Day {
        year: next_year,
        month: next_month,
        day: 1,
    }
    .days()
        - Day {
            year,
            month,
            day: 1,
        }
        .days()) as u32
}

/// The month's cells in rows of seven from `first` (0 for Sunday): the days
/// under their weekdays, with blanks before the first and after the last.
pub fn cells(year: i32, month: u32, first: u32) -> Vec<Option<u32>> {
    let lead = (weekday(Day {
        year,
        month,
        day: 1,
    }) + 7
        - first)
        % 7;
    let mut out: Vec<Option<u32>> = vec![None; lead as usize];
    out.extend((1..=days_in_month(year, month)).map(Some));
    out.resize(ROWS * 7, None);
    out
}

fn step_month(year: i32, month: u32, by: i32) -> (i32, u32) {
    let index = year * 12 + month as i32 - 1 + by;
    (index.div_euclid(12), (index.rem_euclid(12) + 1) as u32)
}

struct State {
    shown: Cell<(i32, u32)>,
    chosen: Cell<Day>,
    earliest: Day,
    latest: Day,
    today: Day,
    /// The days of the month showing, for the chosen one to move without
    /// building the month again.
    buttons: RefCell<Vec<(Day, gtk::Button)>>,
    picked: Box<dyn Fn(Day)>,
}

/// A month showing `chosen`, whose days from `earliest` to `latest` can be
/// chosen; `picked` is told each day chosen.
pub fn month(
    chosen: Day,
    today: Day,
    earliest: Day,
    latest: Day,
    picked: impl Fn(Day) + 'static,
) -> gtk::Box {
    let language = i18n::language();
    let root = gtk::Box::new(gtk::Orientation::Vertical, 6);
    root.add_css_class("tmsb-month");
    root.set_widget_name("schedule.day");

    let title = gtk::Label::new(None);
    title.set_xalign(0.0);
    title.set_hexpand(true);
    title.add_css_class("tmsb-month-title");
    let back = pieces::round_button(&icon("go-previous-symbolic", 12), 32, &t("Previous month"));
    let on = pieces::round_button(&icon("go-next-symbolic", 12), 32, &t("Next month"));
    let head = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    head.set_margin_start(6);
    head.append(&title);
    head.append(&back);
    head.append(&on);
    root.append(&head);

    let weekdays = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    weekdays.set_homogeneous(true);
    for letter in words::weekday_letters(language) {
        let label = gtk::Label::new(Some(letter));
        label.add_css_class("tmsb-weekday");
        weekdays.append(&label);
    }
    root.append(&weekdays);

    let pages = gtk::Stack::builder()
        .transition_duration(motion::SETTLE_MS)
        .hhomogeneous(true)
        .vhomogeneous(true)
        .build();
    root.append(&pages);

    let state = Rc::new(State {
        shown: Cell::new((chosen.year, chosen.month)),
        chosen: Cell::new(chosen),
        earliest,
        latest,
        today,
        buttons: RefCell::new(Vec::new()),
        picked: Box::new(picked),
    });

    let show: Rc<dyn Fn(i32)> = {
        let (state, pages, title, back, on) = (
            state.clone(),
            pages.downgrade(),
            title.downgrade(),
            back.downgrade(),
            on.downgrade(),
        );
        Rc::new(move |by: i32| {
            let (Some(pages), Some(title), Some(back), Some(on)) = (
                pages.upgrade(),
                title.upgrade(),
                back.upgrade(),
                on.upgrade(),
            ) else {
                return;
            };
            let (year, month) = state.shown.get();
            let (year, month) = step_month(year, month, by);
            state.shown.set((year, month));
            title.set_label(&words::month_title(year, month, language));
            back.set_sensitive((year, month) > (state.earliest.year, state.earliest.month));
            on.set_sensitive((year, month) < (state.latest.year, state.latest.month));
            let grid = days(&state, year, month, words::first_weekday(language));
            let name = format!("{year}-{month}");
            if let Some(old) = pages.child_by_name(&name) {
                pages.remove(&old);
            }
            pages.add_named(&grid, Some(&name));
            pages.set_transition_type(if by > 0 {
                gtk::StackTransitionType::SlideLeft
            } else {
                gtk::StackTransitionType::SlideRight
            });
            pages.set_visible_child(&grid);
        })
    };
    show(0);
    // The month left behind goes once the slide is over.
    pages.connect_transition_running_notify(|pages| {
        if pages.is_transition_running() {
            return;
        }
        let keep = pages.visible_child();
        let mut child = pages.first_child();
        while let Some(page) = child {
            child = page.next_sibling();
            if Some(&page) != keep.as_ref() {
                pages.remove(&page);
            }
        }
    });
    {
        let show = show.clone();
        back.connect_clicked(move |_| show(-1));
    }
    on.connect_clicked(move |_| show(1));
    root
}

/// One month's days, in a grid of six rows of seven.
fn days(state: &Rc<State>, year: i32, month: u32, first: u32) -> gtk::Grid {
    let grid = gtk::Grid::builder()
        .row_homogeneous(true)
        .column_homogeneous(true)
        .build();
    let mut buttons = Vec::new();
    let language = i18n::language();
    for (index, cell) in cells(year, month, first).into_iter().enumerate() {
        let (column, row) = ((index % 7) as i32, (index / 7) as i32);
        let Some(number) = cell else {
            let blank = gtk::Box::new(gtk::Orientation::Horizontal, 0);
            blank.add_css_class("tmsb-day-blank");
            grid.attach(&blank, column, row, 1, 1);
            continue;
        };
        let day = Day {
            year,
            month,
            day: number,
        };
        let label = gtk::Label::new(Some(&number.to_string()));
        label.set_halign(gtk::Align::Center);
        label.set_valign(gtk::Align::Center);
        let button = gtk::Button::builder().child(&label).build();
        button.add_css_class("tmsb-day");
        button.set_sensitive(day >= state.earliest && day <= state.latest);
        set_class(&button, "today", day == state.today);
        set_class(&button, "chosen", day == state.chosen.get());
        button.update_property(&[gtk::accessible::Property::Label(&words::day_said(
            year, month, number, language,
        ))]);
        let weak = Rc::downgrade(state);
        button.connect_clicked(move |_| {
            if let Some(state) = weak.upgrade() {
                choose(&state, day);
            }
        });
        grid.attach(&button, column, row, 1, 1);
        buttons.push((day, button));
    }
    state.buttons.replace(buttons);
    grid
}

fn choose(state: &State, day: Day) {
    if state.chosen.replace(day) == day {
        return;
    }
    for (one, button) in state.buttons.borrow().iter() {
        set_class(button, "chosen", *one == day);
    }
    (state.picked)(day);
}

/// The month's rules, against the palette (lobby/mod.rs installs them with
/// the rest of the lobby's): a day is 44 high to the pointer and the finger
/// alike, its disc 32 across.
pub const CSS: &str = r#"
.tmsb-month-title { font-size: 14px; font-weight: 600; }
.tmsb-weekday { font-size: 10.5px; font-weight: 600; color: @tmsb_muted; min-height: 20px; }
.tmsb-day-blank { min-height: 44px; }
button.tmsb-day {
  min-height: 44px; min-width: 0; padding: 0; border: none; box-shadow: none; background: none;
  font-weight: normal;
}
button.tmsb-day > label {
  min-width: 32px; min-height: 32px; border-radius: 999px; font-size: 12.5px;
  font-feature-settings: "tnum";
  transition: background-color 320ms cubic-bezier(0.2, 0.9, 0.3, 1), color 320ms cubic-bezier(0.2, 0.9, 0.3, 1);
}
button.tmsb-day:hover > label { background-color: alpha(@tmsb_fg, 0.08); }
button.tmsb-day:active > label { background-color: alpha(@tmsb_fg, 0.14); }
button.tmsb-day.today > label { box-shadow: inset 0 0 0 1px alpha(@tmsb_tally, 0.7); font-weight: 600; }
button.tmsb-day.chosen > label { background-color: @tmsb_tally; color: @tmsb_on_tally; font-weight: 600; box-shadow: none; }
button.tmsb-day:disabled { filter: none; }
button.tmsb-day:disabled > label { color: alpha(@tmsb_fg, 0.30); }
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn days_fall_under_their_weekdays() {
        // 1 November 2026 is a Sunday.
        assert_eq!(
            weekday(Day {
                year: 2026,
                month: 11,
                day: 1
            }),
            0
        );
        let sunday_first = cells(2026, 11, 0);
        assert_eq!(sunday_first[0], Some(1));
        assert_eq!(sunday_first.len(), 42);
        // From Monday, the Sunday is the seventh cell.
        let monday_first = cells(2026, 11, 1);
        assert_eq!(monday_first[6], Some(1));
        assert_eq!(monday_first.iter().flatten().count(), 30);
        // February in a leap year.
        assert_eq!(cells(2028, 2, 0).iter().flatten().count(), 29);
    }

    #[test]
    fn months_step_across_years() {
        assert_eq!(step_month(2026, 12, 1), (2027, 1));
        assert_eq!(step_month(2027, 1, -1), (2026, 12));
        assert_eq!(step_month(2026, 11, 0), (2026, 11));
    }
}
