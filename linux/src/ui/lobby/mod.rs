//! The screens before a call: signing in, the lobby (the sidebar and the
//! stage, as the Mac lays it out), the pre-join, knocking and waiting, with
//! the lobby's notices, the update offer, the animals, and the start, join
//! and arrange sheets.
//!
//! Owned by the lobby module. The window asks [`page`] for a screen
//! whenever the model moves to one of these; everything else in this
//! directory is the module's own. A call is entered by handing the
//! deployment's grant to `AppModel::enter_call`, which shows the room.

pub mod animals;
pub mod calendar;
pub mod doors;
pub mod home;
pub mod pieces;
pub mod schedule;
pub mod servers;
pub mod sheet;
pub mod sign_in;
pub mod stage;
pub mod update_offer;

use super::Context;
use crate::{app::Screen, theme};

/// The screen for `screen`: one of signing in, the lobby, the pre-join,
/// knocking and waiting.
pub fn page(context: &Context, screen: Screen) -> gtk::Widget {
    theme::add_module_css("lobby", CSS);
    theme::add_module_css("lobby-calendar", calendar::CSS);
    match screen {
        Screen::SignIn => sign_in::page(context),
        Screen::Lobby => home::page(context),
        Screen::Prejoin => doors::prejoin(context),
        Screen::Knocking => doors::knocking(context),
        _ => doors::waiting(context),
    }
}

/// The lobby's own rules, against the palette's names (theme.rs). Hover is
/// ten per cent toward the foreground and pressed sixteen with a scale of
/// 0.95, one value everywhere; no gradients.
const CSS: &str = r#"
.tmsb-lobby .tmsb-sidebar { background-color: @tmsb_surface; border-right: 1px solid @tmsb_border; }
.tmsb-lobby .tmsb-main, .tmsb-stage-ground { background-color: @tmsb_bg; }
.tmsb-stage-card { border-radius: 20px; border: 1px solid @tmsb_border; }
.tmsb-stage-card .tmsb-stage-ground { background-color: @tmsb_surface; }
/* The Mac's shadow under the greeting (Lobby.swift: 0.45, radius 16, two
   down), which keeps it legible over a busy or dark camera picture. */
.tmsb-greeting { font-size: 34px; font-weight: 700; text-shadow: 0 2px 16px alpha(black, 0.45); }
.tmsb-greeting-sub { font-size: 15px; color: alpha(@tmsb_fg, 0.78); text-shadow: 0 2px 16px alpha(black, 0.45); }
.tmsb-section-heading { font-size: 26px; font-weight: 700; }
.tmsb-heading { font-size: 24px; font-weight: 700; }
.tmsb-room-heading { font-size: 20px; font-family: monospace; }
.tmsb-note { font-size: 12.5px; color: @tmsb_muted; }
.tmsb-strong { font-weight: 600; }
.tmsb-mono { font-family: monospace; }
.tmsb-numeric { font-feature-settings: "tnum"; }
.tmsb-tally-text { color: @tmsb_tally_text; }
.tmsb-danger-text { color: @tmsb_danger_text; }
.tmsb-section-title { font-size: 11px; font-weight: 600; color: @tmsb_muted; margin: 12px 12px 4px 12px; }
.tmsb-row-title { font-size: 13px; font-weight: 500; }
.tmsb-row-detail { font-size: 11px; color: @tmsb_muted; }
.tmsb-places row, .tmsb-lobby .navigation-sidebar row { min-height: 32px; border-radius: 10px; }
.tmsb-lobby .navigation-sidebar { background: none; }
.tmsb-lobby .navigation-sidebar > row:selected { background-color: mix(@tmsb_surface, @tmsb_fg, 0.10); }
.tmsb-badge { font-size: 11px; font-weight: 600; color: @tmsb_muted; min-width: 18px; }
button.tmsb-row-button { background: none; border: none; box-shadow: none; padding: 4px 6px; border-radius: 10px; min-height: 0; }
button.tmsb-row-button:hover { background-color: alpha(@tmsb_fg, 0.06); }
button.tmsb-row-button:active { background-color: alpha(@tmsb_fg, 0.10); }

.tmsb-initial { border-radius: 999px; color: #ffffff; font-weight: 600; }
.tmsb-avatar-picture { border-radius: 999px; box-shadow: inset 0 0 0 1px alpha(@tmsb_wash, 0.12); }
.tmsb-initial-0 { background-color: #5e5ce6; }
.tmsb-initial-1 { background-color: #2e8b9a; }
.tmsb-initial-2 { background-color: #9a5cc8; }
.tmsb-initial-3 { background-color: #3478f6; }
.tmsb-initial-4 { background-color: #c66b3f; }
.tmsb-initial-5 { background-color: #3f9863; }

.tmsb-capsule {
  background-color: @tmsb_surface;
  border: 1px solid @tmsb_border;
  border-radius: 999px;
  padding: 8px;
}
button.tmsb-switch {
  min-width: 44px; min-height: 44px; padding: 0;
  border-radius: 999px; border: none; box-shadow: none;
  background-color: mix(@tmsb_surface, @tmsb_wash, 0.08);
  color: @tmsb_fg;
  transition: background-color 320ms cubic-bezier(0.2, 0.9, 0.3, 1), color 320ms cubic-bezier(0.2, 0.9, 0.3, 1), transform 120ms cubic-bezier(0.2, 0.9, 0.3, 1);
}
button.tmsb-switch:hover { background-color: mix(@tmsb_surface, @tmsb_fg, 0.10); }
button.tmsb-switch:active { background-color: mix(@tmsb_surface, @tmsb_fg, 0.16); transform: scale(0.95); }
button.tmsb-switch.off { background-color: @tmsb_fg; color: @tmsb_bg; }
button.tmsb-switch.off:hover { background-color: mix(@tmsb_fg, @tmsb_bg, 0.10); }
button.tmsb-switch.off:active { background-color: mix(@tmsb_fg, @tmsb_bg, 0.16); }

button.tmsb-round {
  min-width: 0; min-height: 0; padding: 0;
  border-radius: 999px; border: none; box-shadow: none;
  background-color: mix(@tmsb_surface, @tmsb_wash, 0.06);
  color: @tmsb_fg;
  transition: background-color 200ms cubic-bezier(0.2, 0.9, 0.3, 1), transform 120ms cubic-bezier(0.2, 0.9, 0.3, 1);
}
button.tmsb-round:hover { background-color: mix(@tmsb_surface, @tmsb_fg, 0.10); }
button.tmsb-round:active, button.tmsb-round.open { background-color: mix(@tmsb_surface, @tmsb_fg, 0.16); }
button.tmsb-round:active { transform: scale(0.95); }
button.tmsb-forget { background-color: alpha(@tmsb_fg, 0.07); color: @tmsb_muted; }

button.tmsb-primary.tmsb-compact, button.tmsb-plain.tmsb-compact { font-size: 13px; padding: 0 14px; min-height: 0; }
button.tmsb-pill {
  min-height: 28px; padding: 0 12px; border-radius: 999px; border: none; box-shadow: none;
  background-color: mix(@tmsb_surface, @tmsb_wash, 0.06); font-size: 12.5px; font-weight: normal;
  transition: background-color 200ms cubic-bezier(0.2, 0.9, 0.3, 1);
}
button.tmsb-pill:hover { background-color: mix(@tmsb_surface, @tmsb_fg, 0.10); }
button.tmsb-pill:active, button.tmsb-pill.open { background-color: mix(@tmsb_surface, @tmsb_fg, 0.16); }
button.tmsb-account {
  padding: 8px 10px; border-radius: 16px; border: 1px solid @tmsb_border; box-shadow: none;
  background-color: mix(@tmsb_surface, @tmsb_wash, 0.03); font-weight: normal;
  transition: background-color 200ms cubic-bezier(0.2, 0.9, 0.3, 1);
}
button.tmsb-account:hover { background-color: mix(@tmsb_surface, @tmsb_fg, 0.10); }
button.tmsb-account:active, button.tmsb-account.open { background-color: mix(@tmsb_surface, @tmsb_fg, 0.16); }

/* A menu drawn in the window (kit::Menu): opaque, on a colour of its own
   that stands off the sidebar it opens over by night as well as by day,
   with a hairline and a shadow; radius 14, rows 32 with radius 10. */
menu-card > box.tmsb-menu {
  border-radius: 14px; padding: 6px; background-color: @tmsb_menu; color: @tmsb_fg;
  border: 1px solid @tmsb_menu_edge;
  box-shadow: 0 10px 30px alpha(black, 0.22), 0 1px 4px alpha(black, 0.12);
}
menu-card > box.tmsb-menu > scrolledwindow { background: none; }
button.tmsb-menu-row {
  min-height: 32px; padding: 0 10px; border-radius: 10px; border: none; box-shadow: none; background: none;
  font-weight: normal;
}
button.tmsb-menu-row:hover { background-color: alpha(@tmsb_fg, 0.08); }
button.tmsb-menu-row:active { background-color: alpha(@tmsb_fg, 0.14); }
button.tmsb-menu-row.tmsb-destructive { color: @tmsb_danger_text; }
.tmsb-menu-heading { font-size: 11px; font-weight: 600; color: @tmsb_muted; margin: 6px 10px 2px 10px; }
separator.tmsb-menu-rule { margin: 4px 6px; background-color: @tmsb_border; }

.tmsb-field-row { padding-left: 14px; }
.tmsb-field-row > entry.tmsb-field, .tmsb-field-row > passwordentry.tmsb-field { padding-left: 0; }

.tmsb-toast {
  background-color: @tmsb_surface; border: 1px solid @tmsb_border; border-radius: 999px;
  padding: 9px 16px 9px 14px; color: @tmsb_fg;
}
.tmsb-toast-words { font-size: 13px; }
button.tmsb-toast-close { margin-right: -9px; }
.tmsb-notice {
  background-color: @tmsb_surface; border: 1px solid @tmsb_border; border-radius: 14px;
  padding: 12px 12px 12px 16px; margin: 6px 16px 6px 16px;
}
.tmsb-notice.warning { border-color: @tmsb_tally; }
.tmsb-notice.warning image { color: @tmsb_tally_text; }
.tmsb-offer {
  background-color: mix(@tmsb_surface, @tmsb_wash, 0.04); border: 1px solid @tmsb_border;
  border-radius: 14px; padding: 12px;
}
progressbar.tmsb-progress > trough { min-height: 4px; border-radius: 2px; background-color: alpha(@tmsb_fg, 0.10); }
progressbar.tmsb-progress > trough > progress { min-height: 4px; border-radius: 2px; background-color: @tmsb_tally; }
.tmsb-fleet { font-size: 11px; color: @tmsb_muted; }
.tmsb-dot { border-radius: 3px; background-color: @tmsb_tally; min-width: 6px; min-height: 6px; }
.tmsb-dot.all { background-color: @tmsb_good; }
.tmsb-dot.none { background-color: @tmsb_danger; }

.tmsb-card {
  background-color: @tmsb_surface; border: 1px solid @tmsb_border; border-radius: 18px; padding: 18px;
}
.tmsb-well {
  background-color: mix(@tmsb_surface, @tmsb_wash, 0.04); border: 1px solid @tmsb_border; border-radius: 18px;
}
.tmsb-well-row { padding: 12px 14px; min-height: 24px; }
separator.tmsb-well-rule { background-color: @tmsb_border; min-height: 1px; }
entry.tmsb-input, passwordentry.tmsb-input {
  background-color: mix(@tmsb_surface, @tmsb_wash, 0.04); border: 1px solid @tmsb_border;
  border-radius: 12px; min-height: 38px; padding: 0 12px; box-shadow: none; outline: none;
}
/* The chosen segment is a plate of its own colour (a raised grey by night,
   white with a shadow by day) over the track, and its words are the
   foreground's while the others' are muted: by night the chosen plate was
   the track's own grey, and the two choices looked the same. */
.tmsb-segmented {
  background-color: @tmsb_segment_track; border-radius: 999px; padding: 2px;
}
.tmsb-segmented > button {
  min-height: 28px; padding: 0 12px; border-radius: 999px; border: none; box-shadow: none; background: none;
  font-size: 13px; font-weight: 500; color: @tmsb_muted;
  transition: background-color 200ms cubic-bezier(0.2, 0.9, 0.3, 1), color 200ms cubic-bezier(0.2, 0.9, 0.3, 1);
}
.tmsb-segmented > button:hover { background-color: alpha(@tmsb_fg, 0.06); color: @tmsb_fg; }
.tmsb-segmented > button:checked {
  background-color: @tmsb_segment; color: @tmsb_fg; font-weight: 600;
  box-shadow: 0 1px 3px alpha(black, 0.16), 0 0 0 1px alpha(@tmsb_wash, 0.06);
}
dialog.tmsb-sheet .dialog-contents, dialog.tmsb-sheet > widget > floating-sheet > sheet { background-color: @tmsb_bg; }
.tmsb-under-way {
  font-size: 11px; font-weight: 600; color: @tmsb_on_tally; background-color: @tmsb_tally;
  border-radius: 999px; padding: 1px 8px;
}
.tmsb-month-well { padding: 10px 8px 6px 8px; }
button.tmsb-choice-row {
  padding: 12px 14px; border-radius: 14px; border: 1px solid @tmsb_border; box-shadow: none;
  background-color: @tmsb_surface;
}
button.tmsb-choice-row:hover { background-color: mix(@tmsb_surface, @tmsb_fg, 0.06); }
button.tmsb-choice-row.chosen { border-color: @tmsb_tally; }
.tmsb-door { padding: 22px; }
"#;
