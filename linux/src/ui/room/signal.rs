//! The signal's figures (Signal.tsx `Detail`, the Mac's Signal.swift), in
//! the popover under the bars: how the connection is graded, which machine
//! the call is on, then the readings, each a label and a value.
//!
//! The rows are made once and changed where they stand as readings arrive,
//! every two seconds; a row with nothing to say (no share, no second
//! machine) slides shut rather than leaving a dash for good. Made again
//! whole at every reading, the popover flickered under the eye reading it.

use std::rc::Rc;

use adw::prelude::*;
use tomoshibi_protocol::quality::{ConnectionGrade, ShareLimitation};

use super::RoomState;
use crate::{
    app::AppModel,
    i18n::{say, t},
    theme::motion,
};

const SIGNAL_CSS: &str = r#"
.tmsb-signal-figures label.figure { font-feature-settings: "tnum"; }
.tmsb-signal-figures label.figure.warn { color: @tmsb_call_tally; }
.tmsb-signal-figures label.grade { font-weight: 600; }
"#;

/// The figures' rows, in the order they are listed.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Row {
    Grade,
    Relay,
    Original,
    Server,
    Address,
    RoundTrip,
    Lost,
    Jitter,
    Share,
    HeldBack,
    Sending,
    Receiving,
    Measuring,
}

const ROWS: [Row; 13] = [
    Row::Grade,
    Row::Relay,
    Row::Original,
    Row::Server,
    Row::Address,
    Row::RoundTrip,
    Row::Lost,
    Row::Jitter,
    Row::Share,
    Row::HeldBack,
    Row::Sending,
    Row::Receiving,
    Row::Measuring,
];

struct Line {
    row: Row,
    holder: gtk::Revealer,
    label: gtk::Label,
    value: gtk::Label,
}

/// Fills `card` with the figures and returns how they are brought up to
/// date (page.rs's popover `Refresh`).
pub fn figures(model: &AppModel, card: &gtk::Box) -> Box<dyn Fn(&AppModel)> {
    let _ = model;
    crate::theme::add_module_css("room-signal", SIGNAL_CSS);
    let list = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .margin_start(10)
        .margin_end(10)
        .margin_top(8)
        .margin_bottom(8)
        .build();
    list.add_css_class("tmsb-signal-figures");
    list.set_widget_name("signal.figures");
    card.append(&list);
    card.set_width_request(260);
    let lines: Vec<Line> = ROWS
        .iter()
        .map(|row| {
            let label = gtk::Label::builder().xalign(0.0).build();
            let value = gtk::Label::builder()
                .xalign(1.0)
                .hexpand(true)
                .ellipsize(gtk::pango::EllipsizeMode::Start)
                .build();
            value.add_css_class("figure");
            if *row == Row::Grade || *row == Row::Measuring {
                label.add_css_class(if *row == Row::Grade {
                    "grade"
                } else {
                    "tmsb-muted"
                });
                value.set_visible(false);
            } else {
                label.add_css_class("tmsb-muted");
            }
            let line = gtk::Box::new(gtk::Orientation::Horizontal, 16);
            line.set_margin_top(3);
            line.set_margin_bottom(3);
            line.append(&label);
            line.append(&value);
            let holder = gtk::Revealer::builder()
                .transition_type(gtk::RevealerTransitionType::SlideDown)
                .transition_duration(motion::SETTLE_MS)
                .child(&line)
                .build();
            list.append(&holder);
            Line {
                row: *row,
                holder,
                label,
                value,
            }
        })
        .collect();
    let lines = Rc::new(lines);
    Box::new(move |model: &AppModel| {
        let said = said(model);
        for line in lines.iter() {
            match said.iter().find(|(row, ..)| *row == line.row) {
                Some((_, label, value, warn)) => {
                    if line.label.label() != *label {
                        line.label.set_label(label);
                    }
                    if line.value.label() != *value {
                        line.value.set_label(value);
                    }
                    if *warn {
                        line.value.add_css_class("warn");
                    } else {
                        line.value.remove_css_class("warn");
                    }
                    line.holder.set_reveal_child(true);
                }
                None => line.holder.set_reveal_child(false),
            }
        }
    })
}

/// Every row with something to say: its label, its value and whether the
/// value is worth a second look.
fn said(model: &AppModel) -> Vec<(Row, String, String, bool)> {
    let room = model.kept::<RoomState>();
    let talk = room.talk.borrow();
    let reading = talk.as_ref().and_then(|talk| talk.connection.clone());
    let grade = reading.as_ref().map(|reading| reading.grade);
    let mut rows = vec![(
        Row::Grade,
        match grade {
            Some(ConnectionGrade::Lost) => t("Connection lost"),
            Some(ConnectionGrade::Poor) => t("Poor connection"),
            Some(ConnectionGrade::Fair) => t("Fair connection"),
            _ => t("Good connection"),
        },
        String::new(),
        false,
    )];
    // Said by the server, which picked both machines, and by nothing else
    // (Signal.tsx `relayed`): one line where the call is on the machine that
    // was dialled; two where somebody's signalling is forwarded to the one
    // holding the meeting, which is not a fault and is said nowhere else.
    let entry = model.call_entry();
    let relay = entry
        .as_ref()
        .and_then(|entry| entry.grant.relay.clone())
        .filter(|relay| !relay.is_empty());
    let holding = entry
        .as_ref()
        .and_then(|entry| entry.grant.holding.clone())
        .or_else(|| reading.as_ref().and_then(|reading| reading.holding.clone()))
        .filter(|holding| !holding.is_empty());
    match (relay, holding) {
        (Some(relay), Some(holding)) if relay != holding => {
            rows.push((Row::Relay, t("Relay server"), say(&relay), false));
            rows.push((Row::Original, t("Original server"), say(&holding), false));
        }
        (Some(relay), _) => rows.push((Row::Server, t("Server"), say(&relay), false)),
        _ => {}
    }
    let ms = |value: Option<f64>| {
        value
            .map(|v| format!("{} ms", v.round() as i64))
            .unwrap_or_else(|| "—".into())
    };
    let rate = |value: Option<f64>| {
        value
            .map(|kbps| {
                if kbps >= 1000.0 {
                    format!("{:.1} Mbps", kbps / 1000.0)
                } else {
                    format!("{} kbps", kbps.round() as i64)
                }
            })
            .unwrap_or_else(|| "—".into())
    };
    if let Some(reading) = reading.as_ref() {
        if let Some(address) = reading.own_address.as_ref() {
            rows.push((Row::Address, t("Your address"), address.clone(), false));
        }
    }
    let reading_ref = reading.as_ref();
    rows.push((
        Row::RoundTrip,
        t("Round trip"),
        ms(reading_ref.and_then(|reading| reading.rtt_ms)),
        false,
    ));
    let loss = reading_ref.and_then(|reading| reading.loss_percent);
    rows.push((
        Row::Lost,
        t("Lost"),
        loss.map(|loss| format!("{loss:.1}%"))
            .unwrap_or_else(|| "—".into()),
        loss.unwrap_or(0.0) > 2.0,
    ));
    rows.push((
        Row::Jitter,
        t("Jitter"),
        ms(reading_ref.and_then(|reading| reading.jitter_ms)),
        false,
    ));
    if let Some(share) = reading_ref.and_then(|reading| reading.share.as_ref()) {
        let size = match (share.height, share.fps) {
            (Some(height), Some(fps)) => format!("{}p · {} fps", height as i64, fps as i64),
            _ => t("Measuring…"),
        };
        rows.push((
            Row::Share,
            if share.sending {
                t("Sharing")
            } else {
                t("Watching")
            },
            size,
            share.fps.unwrap_or(30.0) < 10.0,
        ));
        if let Some(limited) = share.limited {
            rows.push((
                Row::HeldBack,
                t("Held back by"),
                match limited {
                    ShareLimitation::Cpu => t("this machine"),
                    ShareLimitation::Bandwidth => t("the connection"),
                    ShareLimitation::Other => t("something the browser will not name"),
                },
                true,
            ));
        }
    }
    rows.push((
        Row::Sending,
        t("Sending"),
        rate(reading_ref.and_then(|reading| reading.up_kbps)),
        false,
    ));
    rows.push((
        Row::Receiving,
        t("Receiving"),
        rate(reading_ref.and_then(|reading| reading.down_kbps)),
        false,
    ));
    if !reading_ref.is_some_and(|reading| reading.measured) {
        rows.push((Row::Measuring, t("Measuring…"), String::new(), false));
    }
    rows
}
