//! The call's own rules, installed through theme::add_module_css("room").
//!
//! Every colour is one of the call's named colours (`tmsb_call_*`), which
//! are the dark palette in every appearance (UI-SYNC decision 6), so the
//! call reads the same on a light desktop. GTK 4.14 has no var(), so the
//! scale's numbers are written out; each says which value of
//! theme::scale or theme::motion it is.
//!
//! GTK's min-height and min-width are the content box's: padding and a
//! border add to them. The bar's buttons have neither (their edge is an
//! inset shadow, which takes no room), so a 44 here is 44 on screen; the
//! widget tree of a headless run (`.gtk.tree.txt`) is where that is checked.

pub const ROOM_CSS: &str = r#"
.tmsb-room { background-color: @tmsb_call_bg; color: @tmsb_call_fg; }

/* The window's own buttons over the call: the call's ink, whatever the
   system's appearance, which libadwaita's style otherwise follows, on a
   dark plate with a hairline, as the stage's buttons are: on a tenth of
   white over a bright shared screen the minimise button could not be seen
   at all (KDE and Xfce show it; GNOME shows only close). */
window.tmsb-call headerbar.tmsb-bar windowcontrols > button {
  color: white;
  background: none;
  box-shadow: none;
}
/* libadwaita draws the button as the circle round its symbol. */
window.tmsb-call headerbar.tmsb-bar windowcontrols > button > image {
  background-color: rgba(0, 0, 0, 0.55);
  box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.14);
  transition: background-color 120ms ease-out;
}
window.tmsb-call headerbar.tmsb-bar windowcontrols > button:hover > image {
  background-color: rgba(0, 0, 0, 0.7);
}
window.tmsb-call headerbar.tmsb-bar windowcontrols > button:active > image {
  background-color: rgba(0, 0, 0, 0.8);
}

/* A picture: the tile's grey around a picture of another shape, the tile
   radius (scale::TILE_RADIUS). */
.tmsb-tile {
  background-color: @tmsb_call_surface;
  border-radius: 8px;
}
.tmsb-tile.filled { background-color: black; border-radius: 0; }
.tmsb-tile-edge {
  border-radius: 8px;
  border: 1px solid transparent;
  box-shadow: none;
  transition: border-color 320ms cubic-bezier(0.2, 0.9, 0.3, 1), box-shadow 320ms cubic-bezier(0.2, 0.9, 0.3, 1);
}
/* Laid over another picture, as the card in a call of two is: lifted off
   it by a hairline and a shadow, since both are the same grey. */
.tmsb-tile-edge.floating {
  border-color: alpha(white, 0.14);
}
/* The tally round a speaker, faded rather than switched, so a pause
   between two words does not flash it. */
.tmsb-ring {
  border-radius: 8px;
  border: 2px solid @tmsb_call_tally;
  opacity: 0;
  transition: opacity 200ms cubic-bezier(0.2, 0.9, 0.3, 1);
}
.tmsb-ring.on { opacity: 1; }
.tmsb-tile.filled .tmsb-ring, .tmsb-tile.filled .tmsb-tile-edge { border-radius: 0; }

/* The name plate: a capsule about 24 high, 8 from the edge, black at
   seven tenths, the name always white (UI-SYNC decision 5). */
.tmsb-plate {
  background-color: rgba(0, 0, 0, 0.7);
  border-radius: 999px;
  min-height: 24px;
  padding: 0 10px;
  color: white;
}
.tmsb-plate label { font-size: 12.5px; font-weight: 500; color: white; }
.tmsb-plate image { color: rgba(255, 255, 255, 0.85); -gtk-icon-size: 11px; }
.tmsb-plate image.silenced { color: @tmsb_call_danger; }
.tmsb-plate.compact { min-height: 18px; padding: 0 6px; }
.tmsb-plate.compact label { font-size: 10.5px; }
.tmsb-proven { background-color: rgba(255, 255, 255, 0.7); border-radius: 999px; min-width: 4px; min-height: 4px; }
.tmsb-unverified {
  background-color: @tmsb_call_danger; color: @tmsb_call_on_danger;
  border-radius: 999px; padding: 1px 5px; font-size: 9.5px; font-weight: 600;
}

/* A raised hand, top left of the picture. */
.tmsb-hand {
  background-color: @tmsb_call_tally;
  color: @tmsb_call_on_tally;
  border-radius: 999px;
  min-width: 28px;
  min-height: 28px;
  box-shadow: 0 2px 6px rgba(0, 0, 0, 0.3);
}
.tmsb-hand.compact { min-width: 22px; min-height: 22px; }
/* Beside the name: a circle as tall as the name's line, not a pill. */
.tmsb-hand.small { min-width: 18px; min-height: 18px; padding: 0; box-shadow: none; }

/* The buttons drawn on a picture: 36 (scale::STAGE_BUTTON). */
button.tmsb-stage-button {
  min-width: 36px; min-height: 36px; padding: 0;
  border-radius: 999px; border: none;
  background-color: rgba(0, 0, 0, 0.55);
  color: white;
  box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.12);
  -gtk-icon-size: 14px;
  transition: background-color 120ms ease-out, transform 140ms cubic-bezier(0.2, 0.9, 0.3, 1);
}
button.tmsb-stage-button:hover { background-color: rgba(0, 0, 0, 0.7); }
button.tmsb-stage-button:active { background-color: rgba(0, 0, 0, 0.8); transform: scale(0.95); }
/* "Their screen", "Their camera": the same height, with its words. */
button.tmsb-stage-button.words { padding: 0 14px 0 12px; }
button.tmsb-stage-button.words label { font-size: 13px; font-weight: 500; }

/* The keyboard's focus on a picture: a ring inside its edge, faded in. */
.tmsb-focus-ring {
  border-radius: 8px;
  border: 2px solid transparent;
  transition: border-color 200ms cubic-bezier(0.2, 0.9, 0.3, 1);
}
.tmsb-tile:focus-visible > .tmsb-focus-ring { border-color: alpha(white, 0.85); }
.tmsb-tile.filled > .tmsb-focus-ring { border-radius: 0; }

/* Somebody else's screen while it is not on the stage (ShareCard.tsx, the
   Mac's ShareCardFace): flat, a step lighter than a person's tile; the
   colour is on Watch alone. */
.tmsb-tile.share-card { background-color: @tmsb_call_surface_high; }
.tmsb-share-card-mark {
  background-color: alpha(white, 0.08); color: alpha(@tmsb_call_fg, 0.9);
  border-radius: 999px; min-width: 48px; min-height: 48px; -gtk-icon-size: 22px;
}
.compact .tmsb-share-card-mark { min-width: 30px; min-height: 30px; -gtk-icon-size: 15px; }
.tmsb-share-card .tmsb-share-said { font-size: 13.5px; }
.compact .tmsb-share-card .tmsb-share-said, .tmsb-share-card.compact .tmsb-share-said { font-size: 11px; }
.tmsb-watch-pill {
  background-color: @tmsb_call_tally; color: @tmsb_call_on_tally;
  border-radius: 999px; min-height: 26px; padding: 0 12px;
  font-size: 12px; font-weight: 600; -gtk-icon-size: 10px;
}

/* A sealed call whose word this participant does not share with them. */
.tmsb-unreadable {
  background-color: rgba(0, 0, 0, 0.7); color: white;
  border-radius: 999px; min-height: 24px; padding: 0 10px;
  font-size: 12px; font-weight: 500; -gtk-icon-size: 12px;
}

/* One's own share, which has no picture here (the Mac's MyShareFace). */
.tmsb-tile.mine-shared { background-color: @tmsb_call_surface_high; }
.tmsb-share-mark {
  background-color: alpha(@tmsb_call_tally, 0.18); color: @tmsb_call_tally;
  border-radius: 999px; min-width: 52px; min-height: 52px; -gtk-icon-size: 22px;
}
.tmsb-share-said { font-size: 14px; font-weight: 600; }
.compact .tmsb-share-mark { min-width: 34px; min-height: 34px; -gtk-icon-size: 14px; }
.compact .tmsb-share-said { font-size: 11px; }

/* A reaction rising off a picture (Reacting.tsx). */
.tmsb-float { font-size: 30px; }
.tmsb-float.compact { font-size: 22px; }

/* The bar: the call's raised surface in a capsule, 44 buttons. */
.tmsb-island {
  background-color: @tmsb_call_surface;
  border-radius: 999px;
  padding: 7px;
  box-shadow: inset 0 0 0 1px alpha(@tmsb_call_wash, 0.10), 0 8px 24px rgba(0, 0, 0, 0.35);
}
.tmsb-island.compact { padding: 5px; }
.tmsb-island-rule { background-color: @tmsb_call_border; min-width: 1px; min-height: 26px; margin: 0 2px; }
/* Down the side of the room: the rule lies across the column. */
.tmsb-island.side .tmsb-island-rule { min-width: 26px; min-height: 1px; margin: 2px 0; }
.tmsb-island.side button.tmsb-bar-button.narrow { min-width: 44px; min-height: 28px; }

button.tmsb-bar-button {
  min-width: 44px; min-height: 44px; padding: 0;
  border: none; border-radius: 999px;
  background-color: alpha(@tmsb_call_wash, 0.12);
  color: @tmsb_call_fg;
  box-shadow: inset 0 0 0 1px alpha(@tmsb_call_wash, 0.08);
  -gtk-icon-size: 18px;
  transition: background-color 120ms ease-out, color 320ms cubic-bezier(0.2, 0.9, 0.3, 1),
              box-shadow 320ms cubic-bezier(0.2, 0.9, 0.3, 1), transform 140ms cubic-bezier(0.2, 0.9, 0.3, 1);
}
.compact button.tmsb-bar-button { min-width: 40px; min-height: 40px; -gtk-icon-size: 16px; }
/* Hover a tenth of the way toward the foreground, pressed sixteen
   hundredths and 0.95 (Theme.Press: 0.12, 0.21, 0.26). */
button.tmsb-bar-button:hover { background-color: alpha(@tmsb_call_wash, 0.21); }
button.tmsb-bar-button:active { background-color: alpha(@tmsb_call_wash, 0.26); transform: scale(0.95); }
/* Its panel or popover open: held down, neutral grey, never amber
   (UI-SYNC decision 2). */
button.tmsb-bar-button.held {
  background-color: alpha(@tmsb_call_wash, 0.26);
  box-shadow: inset 0 0 0 1px alpha(@tmsb_call_wash, 0.2);
}
/* Something it stands for is happening: a share going out, a hand up. */
button.tmsb-bar-button.lit {
  background-color: @tmsb_call_tally; color: @tmsb_call_on_tally;
  box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.3);
}
button.tmsb-bar-button.lit:hover { background-color: alpha(@tmsb_call_tally, 0.9); }
button.tmsb-bar-button.lit:active { background-color: alpha(@tmsb_call_tally, 0.8); }
/* The microphone or camera off: a solid foreground plate with the symbol
   in the page's colour (UI-SYNC decision 1), not red. */
button.tmsb-bar-button.off { background-color: @tmsb_call_fg; color: @tmsb_call_bg; box-shadow: none; }
button.tmsb-bar-button.off:hover { background-color: alpha(@tmsb_call_fg, 0.9); }
button.tmsb-bar-button.off:active { background-color: alpha(@tmsb_call_fg, 0.8); }
button.tmsb-bar-button:disabled { opacity: 0.4; }
/* The narrow half beside the hand that opens the reactions. */
button.tmsb-bar-button.narrow { min-width: 32px; -gtk-icon-size: 12px; }
/* Leave: the red capsule, 56 by 44 (UI-SYNC decision 3). */
button.tmsb-bar-button.leave {
  min-width: 56px; background-color: @tmsb_call_danger; color: @tmsb_call_on_danger;
  box-shadow: none; -gtk-icon-size: 18px;
}
.compact button.tmsb-bar-button.leave { min-width: 52px; }
button.tmsb-bar-button.leave:hover { background-color: alpha(@tmsb_call_danger, 0.9); }
button.tmsb-bar-button.leave:active, button.tmsb-bar-button.leave.held { background-color: alpha(@tmsb_call_danger, 0.8); }

/* The dot that says something waits behind a closed panel, ringed in the
   bar's grey; the count of people at the door. */
.tmsb-dot {
  background-color: @tmsb_call_tally; border-radius: 999px;
  min-width: 5px; min-height: 5px; border: 2px solid @tmsb_call_surface_high;
}
.tmsb-count {
  background-color: @tmsb_call_tally; color: @tmsb_call_on_tally; border-radius: 999px;
  min-width: 12px; min-height: 12px; padding: 0 2px; font-size: 10px; font-weight: 700;
  border: 2px solid @tmsb_call_surface_high;
}

/* Menus and popovers off the bar: opaque, radius 14, rows radius 10 and
   at least 44 for the leave confirmation (UI sync), 32 elsewhere. */
.tmsb-menu {
  background-color: @tmsb_call_surface;
  border-radius: 14px;
  padding: 6px;
  box-shadow: inset 0 0 0 1px alpha(@tmsb_call_wash, 0.10), 0 12px 32px rgba(0, 0, 0, 0.45);
}
/* A row is 32 on screen (scale::MENU_ROW): min-height is the content's, so
   any padding above and below makes it taller, and 4 each way made 40. */
button.tmsb-menu-row {
  border: none; border-radius: 10px; box-shadow: none;
  background-color: transparent; color: @tmsb_call_fg;
  min-height: 32px; padding: 0 10px;
  transition: background-color 120ms ease-out;
}
/* A row with a line beneath its title (the leave confirmation): 44. */
button.tmsb-menu-row.tall { min-height: 36px; padding: 4px 10px; }
button.tmsb-menu-row:hover { background-color: alpha(@tmsb_call_wash, 0.10); }
button.tmsb-menu-row:active { background-color: alpha(@tmsb_call_wash, 0.16); }
button.tmsb-menu-row.danger { color: @tmsb_call_danger_text; }
.tmsb-menu-row .detail { font-size: 11.5px; color: @tmsb_call_muted; }
.tmsb-menu-row .title { font-size: 13px; font-weight: 500; }
.tmsb-menu-heading { font-size: 11px; font-weight: 600; color: @tmsb_call_muted; padding: 6px 10px 2px 10px; }
.tmsb-menu-rule { background-color: @tmsb_call_border; min-height: 1px; margin: 4px 6px; }

/* The reactions: cells 40 drawn (UI sync). */
button.tmsb-reaction {
  min-width: 40px; min-height: 40px; padding: 0;
  border: none; border-radius: 10px; box-shadow: none;
  background-color: transparent; font-size: 22px;
  transition: background-color 120ms ease-out, transform 140ms cubic-bezier(0.2, 0.9, 0.3, 1);
}
button.tmsb-reaction:hover { background-color: alpha(@tmsb_call_wash, 0.10); }
button.tmsb-reaction:active { background-color: alpha(@tmsb_call_wash, 0.16); transform: scale(0.95); }

/* The panels beside the call: opaque, the close button 32. */
.tmsb-panel {
  background-color: @tmsb_call_surface;
  border-radius: 18px;
  box-shadow: inset 0 0 0 1px alpha(@tmsb_call_wash, 0.10), 0 12px 32px rgba(0, 0, 0, 0.4);
  color: @tmsb_call_fg;
}
.tmsb-panel-title { font-size: 15px; font-weight: 600; }
button.tmsb-close {
  min-width: 32px; min-height: 32px; padding: 0; border: none; border-radius: 999px;
  background-color: transparent; color: @tmsb_call_muted; box-shadow: none; -gtk-icon-size: 14px;
  transition: background-color 120ms ease-out;
}
button.tmsb-close:hover { background-color: alpha(@tmsb_call_wash, 0.10); color: @tmsb_call_fg; }
button.tmsb-close:active { background-color: alpha(@tmsb_call_wash, 0.16); }
.tmsb-panel .tmsb-muted, .tmsb-room .tmsb-muted { color: @tmsb_call_muted; }
.tmsb-panel-section { font-size: 11px; font-weight: 600; color: @tmsb_call_muted; }
.tmsb-panel scale trough { background-color: alpha(@tmsb_call_wash, 0.14); min-height: 4px; border-radius: 999px; }
.tmsb-panel scale highlight { background-color: @tmsb_call_fg; border-radius: 999px; }
/* The knob 14 across over a trough 4 high: libadwaita's own knob is 20 with
   a margin of -8 (4 left over), and the margin kept with a 14 knob left it
   -2 wide and high, which GTK warned of twice for every slider drawn (38
   lines in a call of six). -5 keeps the 4 over and the knob centred. */
.tmsb-panel scale slider { background-color: @tmsb_call_fg; min-width: 14px; min-height: 14px; margin: -5px; border-radius: 999px; box-shadow: none; border: none; }
.tmsb-panel-row { min-height: 36px; }
button.tmsb-panel-button {
  min-height: 32px; padding: 0 12px; border: none; border-radius: 999px; box-shadow: none;
  background-color: alpha(@tmsb_call_wash, 0.10); color: @tmsb_call_fg;
  transition: background-color 120ms ease-out;
}
button.tmsb-panel-button:hover { background-color: alpha(@tmsb_call_wash, 0.18); }
button.tmsb-panel-button:active { background-color: alpha(@tmsb_call_wash, 0.24); }
button.tmsb-panel-button.danger { color: @tmsb_call_danger_text; }
button.tmsb-panel-button.primary { background-color: @tmsb_call_tally; color: @tmsb_call_on_tally; }
button.tmsb-panel-button:disabled { opacity: 0.4; }
button.tmsb-choice {
  min-height: 32px; padding: 0 10px; border: none; border-radius: 10px; box-shadow: none;
  background-color: transparent; color: @tmsb_call_fg;
}
button.tmsb-choice:hover { background-color: alpha(@tmsb_call_wash, 0.10); }
button.tmsb-choice.held { background-color: alpha(@tmsb_call_wash, 0.16); }
button.tmsb-choice label { font-weight: normal; }
button.tmsb-choice image.tick { color: @tmsb_call_fg; }

/* Messages. */
.tmsb-line-name { font-size: 12px; font-weight: 600; color: @tmsb_call_muted; }
.tmsb-line-text { font-size: 13.5px; }
.tmsb-line-mine .tmsb-line-bubble { background-color: alpha(@tmsb_call_tally, 0.18); }
.tmsb-line-bubble { background-color: alpha(@tmsb_call_wash, 0.08); border-radius: 14px; padding: 6px 10px; }
.tmsb-say {
  background-color: alpha(@tmsb_call_wash, 0.06); border-radius: 18px;
  border: 1px solid @tmsb_call_border; padding: 0 4px 0 12px; min-height: 34px;
}
.tmsb-say entry, .tmsb-say entry:focus-within {
  background: none; border: none; box-shadow: none; outline: none; min-height: 32px; padding: 0;
}
.tmsb-say text { background: none; color: @tmsb_call_fg; }
.tmsb-say.failed { border-color: @tmsb_call_danger; }
button.tmsb-send {
  min-width: 28px; min-height: 28px; padding: 0; border: none; border-radius: 999px; box-shadow: none;
  background-color: @tmsb_call_tally; color: @tmsb_call_on_tally;
}
button.tmsb-send:disabled { background-color: @tmsb_call_surface_high; color: @tmsb_call_muted; }

/* The row at the top: the room's name and the signal, 32 high. */
.tmsb-room-title {
  background-color: alpha(@tmsb_call_surface, 0.92);
  border-radius: 999px; min-height: 32px; padding: 0 12px;
  box-shadow: inset 0 0 0 1px alpha(@tmsb_call_wash, 0.10);
  font-size: 13px; font-weight: 600;
}
button.tmsb-signal {
  background-color: alpha(@tmsb_call_surface, 0.92);
  border: none; border-radius: 999px; min-height: 32px; min-width: 32px; padding: 0 10px;
  box-shadow: inset 0 0 0 1px alpha(@tmsb_call_wash, 0.10); color: @tmsb_call_fg;
}
button.tmsb-signal:hover { background-color: mix(@tmsb_call_surface, @tmsb_call_fg, 0.10); }
/* "Signal only": the bars, with nothing to press. */
button.tmsb-signal.quiet:hover, button.tmsb-signal.quiet:active { background-color: alpha(@tmsb_call_surface, 0.92); }
button.tmsb-signal.detailed { padding: 0 11px; }
button.tmsb-signal.round { padding: 0; min-width: 32px; }
.tmsb-signal-figure { font-size: 11.5px; font-weight: 500; font-feature-settings: "tnum"; color: @tmsb_call_muted; }
.tmsb-signal-figures label { font-size: 12px; }
.tmsb-signal-figures label.figure { font-family: monospace; }

/* Notices over the stage, and the corner's notices. */
.tmsb-notice {
  background-color: @tmsb_call_surface_high; border-radius: 999px; padding: 6px 14px;
  font-size: 13px; box-shadow: 0 6px 18px rgba(0, 0, 0, 0.35);
}
.tmsb-notice.danger { background-color: @tmsb_call_danger; color: @tmsb_call_on_danger; }
.tmsb-toast {
  background-color: @tmsb_call_surface_high; border-radius: 14px; padding: 10px 12px;
  box-shadow: inset 0 0 0 1px alpha(@tmsb_call_wash, 0.10), 0 8px 24px rgba(0, 0, 0, 0.35);
  border-left: 3px solid transparent;
}
.tmsb-toast.signal { border-left-color: @tmsb_call_tally; }
.tmsb-toast.failure { border-left-color: @tmsb_call_danger; }
.tmsb-toast .title { font-size: 13px; font-weight: 500; }
.tmsb-toast .detail { font-size: 12px; color: @tmsb_call_muted; }
.tmsb-said {
  background-color: @tmsb_call_surface_high; border-radius: 14px; padding: 8px 12px;
  box-shadow: inset 0 0 0 1px alpha(@tmsb_call_wash, 0.10);
}

/* Alone in the room: the link, as a small capsule above the controls. */
.tmsb-alone {
  background-color: @tmsb_call_surface; border-radius: 999px; padding: 0;
  box-shadow: inset 0 0 0 1px alpha(@tmsb_call_wash, 0.10), 0 8px 24px rgba(0, 0, 0, 0.35);
}
button.tmsb-alone-button {
  border: none; box-shadow: none; background-color: transparent; color: @tmsb_call_fg;
  border-radius: 999px; min-height: 34px; padding: 0 10px 0 14px; font-size: 13px; font-weight: 500;
}
button.tmsb-alone-button:hover { background-color: alpha(@tmsb_call_wash, 0.10); }
.tmsb-alone-rule { background-color: alpha(@tmsb_call_wash, 0.12); min-width: 1px; min-height: 16px; }

/* Who has one's share open. */
.tmsb-watchers {
  background-color: @tmsb_call_surface; border-radius: 14px; padding: 8px 12px; font-size: 12.5px;
  box-shadow: inset 0 0 0 1px alpha(@tmsb_call_wash, 0.10);
}
.tmsb-watchers image { color: @tmsb_call_muted; }

/* The pager's arrows beside the pictures. */
button.tmsb-pager {
  min-width: 36px; min-height: 36px; padding: 0; border: none; border-radius: 999px;
  background-color: @tmsb_call_surface; color: @tmsb_call_fg;
  box-shadow: inset 0 0 0 1px alpha(@tmsb_call_wash, 0.10);
}
button.tmsb-pager:hover { background-color: mix(@tmsb_call_surface, @tmsb_call_fg, 0.10); }
.tmsb-pages { font-size: 11px; color: @tmsb_call_muted; }

/* Watching together: the video on the stage, black around it. */
.tmsb-tile.tmsb-watch-stage { background-color: black; }
.tmsb-watch-stage .tmsb-watch-film { -gtk-icon-size: 28px; color: @tmsb_call_muted; }
.tmsb-watch-empty label { color: @tmsb_call_muted; font-size: 13px; }
.tmsb-watch-quiet { color: alpha(white, 0.65); font-size: 12px; }
.tmsb-watch-lost {
  background-color: rgba(0, 0, 0, 0.85); border-radius: 14px; padding: 16px 20px;
}
.tmsb-watch-lost-mark {
  background-color: alpha(white, 0.08); color: alpha(white, 0.75);
  border-radius: 999px; min-width: 48px; min-height: 48px; -gtk-icon-size: 20px;
}
.tmsb-watch-lost-title { color: white; font-size: 14px; font-weight: 600; }
/* The controls under the video: one dark band, not a fade into it. */
.tmsb-watch-controls { background-color: rgba(0, 0, 0, 0.72); padding: 10px 12px 8px 12px; color: white; }
.tmsb-watch-title { font-size: 13px; font-weight: 500; color: white; }
.tmsb-watch-time { font-size: 11px; color: alpha(white, 0.8); font-feature-settings: "tnum"; margin: 0 4px; }
button.tmsb-watch-control {
  min-width: 34px; min-height: 34px; padding: 0; border: none; border-radius: 999px; box-shadow: none;
  background-color: transparent; color: white; -gtk-icon-size: 14px;
  transition: background-color 120ms ease-out, transform 140ms cubic-bezier(0.2, 0.9, 0.3, 1);
}
button.tmsb-watch-control:hover { background-color: alpha(white, 0.16); }
button.tmsb-watch-control:active { background-color: alpha(white, 0.24); transform: scale(0.95); }
.tmsb-watch-position trough, .tmsb-watch-volume trough { background-color: alpha(white, 0.2); min-height: 4px; border-radius: 999px; }
.tmsb-watch-position highlight { background-color: @tmsb_call_tally; border-radius: 999px; }
.tmsb-watch-volume highlight { background-color: white; border-radius: 999px; }
.tmsb-watch-position slider, .tmsb-watch-volume slider {
  background-color: white; min-width: 14px; min-height: 14px; margin: -5px; border-radius: 999px; box-shadow: none; border: none;
}
button.tmsb-watch-pill-button {
  min-height: 32px; padding: 0 14px; border: none; border-radius: 999px; box-shadow: none;
  background-color: alpha(white, 0.14); color: white; font-size: 12.5px; font-weight: 600;
  transition: background-color 120ms ease-out;
}
button.tmsb-watch-pill-button:hover { background-color: alpha(white, 0.22); }
button.tmsb-watch-pill-button.prominent { background-color: @tmsb_call_tally; color: @tmsb_call_on_tally; }
button.tmsb-watch-pill-button.prominent:hover { background-color: mix(@tmsb_call_tally, @tmsb_call_fg, 0.10); }
.tmsb-watch-tag {
  background-color: rgba(0, 0, 0, 0.55); color: @tmsb_call_fg; border-radius: 999px;
  padding: 3px 6px; font-size: 10.5px; font-weight: 500;
}

/* The watch together panel. */
.tmsb-watch-field {
  background-color: alpha(@tmsb_call_wash, 0.06); color: @tmsb_call_fg;
  border-radius: 999px; border: 1px solid @tmsb_call_border; min-height: 32px; padding: 0 12px; box-shadow: none;
}
.tmsb-watch-field:focus-within { border-color: alpha(@tmsb_call_tally, 0.7); }
button.tmsb-watch-add { min-width: 32px; min-height: 32px; background-color: @tmsb_call_surface_high; color: @tmsb_call_muted; }
button.tmsb-watch-add.ready { background-color: @tmsb_call_tally; color: @tmsb_call_on_tally; }
button.tmsb-watch-add:disabled { opacity: 0.4; }
.tmsb-watch-row { border-radius: 10px; padding: 6px; }
.playing > .tmsb-watch-row { background-color: alpha(@tmsb_call_tally, 0.10); }
.tmsb-watch-poster { background-color: @tmsb_call_surface_high; border-radius: 6px; color: @tmsb_call_muted; }
.tmsb-watch-playing-mark {
  background-color: @tmsb_call_tally; color: @tmsb_call_on_tally; border-radius: 999px;
  min-width: 16px; min-height: 16px; -gtk-icon-size: 8px; margin: 3px;
}
.tmsb-watch-row-title { font-size: 12.5px; font-weight: 500; }
.tmsb-watch-row-detail { font-size: 10.5px; color: @tmsb_call_muted; }
.tmsb-watch-row-detail.playing { color: @tmsb_call_tally; }
.tmsb-watch-tabs { background-color: alpha(@tmsb_call_wash, 0.06); border-radius: 10px; padding: 2px; }

/* The ended screen. */
.tmsb-ended-title { font-size: 22px; font-weight: 600; }
"#;
