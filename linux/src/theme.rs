//! The apps' palette, scale and motion as GTK CSS: the same numbers as
//! windows/Tomoshibi.Basics/Theme.cs, apple's Theme.swift and Android's
//! basics/Theme.kt, so that the same app looks the same on every machine.
//!
//! Two palettes, dark and light, with day and night following the system
//! through libadwaita's style manager (the owner, 2026-10-26: keep this
//! interface, add a day mode, follow the system), and the call drawn dark in
//! either, as FaceTime and Meet are: a call is looked at for an hour at a
//! stretch and the pictures are the only thing in it that should be bright.
//!
//! GTK 4.14, the oldest the app supports, has no CSS custom properties
//! (`var()` arrived in 4.16), so the palette is written as named colours
//! (`@define-color`), which are global. That is why there are two sets of
//! names: `tmsb_*` follows the system and is rewritten when it changes, and
//! `tmsb_call_*` is the dark palette always, for everything inside a call.
//! A module's own CSS uses the names, never numbers, and is installed with
//! [`add_module_css`] so that no module has to edit this file.
//!
//! No gradients anywhere (the owner's rule for every platform): every fill
//! here is one solid colour.

use std::cell::RefCell;

/// A colour as the other apps write theirs: red, green, blue and alpha, each
/// from nought to one.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Shade {
    pub red: f32,
    pub green: f32,
    pub blue: f32,
    pub alpha: f32,
}

impl Shade {
    pub const fn rgb(red: f32, green: f32, blue: f32) -> Self {
        Self {
            red,
            green,
            blue,
            alpha: 1.0,
        }
    }

    /// `#RRGGBB`, as the palette writes the light one.
    pub const fn hex(rgb: u32) -> Self {
        Self::hex_alpha(rgb, 1.0)
    }

    pub const fn hex_alpha(rgb: u32, alpha: f32) -> Self {
        Self {
            red: ((rgb >> 16) & 0xFF) as f32 / 255.0,
            green: ((rgb >> 8) & 0xFF) as f32 / 255.0,
            blue: (rgb & 0xFF) as f32 / 255.0,
            alpha,
        }
    }

    pub const fn with_alpha(self, alpha: f32) -> Self {
        Self { alpha, ..self }
    }

    /// The colour as CSS: eight bits a channel, rounded as Compose and
    /// Windows round a float colour (Shade.Bytes in Theme.cs).
    pub fn css(self) -> String {
        let byte = |channel: f32| (channel * 255.0).round().clamp(0.0, 255.0) as u8;
        if self.alpha >= 1.0 {
            format!(
                "#{:02x}{:02x}{:02x}",
                byte(self.red),
                byte(self.green),
                byte(self.blue)
            )
        } else {
            format!(
                "rgba({},{},{},{})",
                byte(self.red),
                byte(self.green),
                byte(self.blue),
                (self.alpha * 1000.0).round() / 1000.0
            )
        }
    }

    pub fn rgba(self) -> gtk::gdk::RGBA {
        gtk::gdk::RGBA::new(self.red, self.green, self.blue, self.alpha)
    }
}

pub const WHITE: Shade = Shade::rgb(1.0, 1.0, 1.0);
pub const BLACK: Shade = Shade::rgb(0.0, 0.0, 0.0);

/// Which of the two palettes something is drawn in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scheme {
    Dark,
    Light,
}

/// The neutrals and the two signal colours of one scheme (Theme.cs
/// `Palette`). The wash is what the chrome's fills, hairlines and edges are
/// drawn in: white over the dark ground, black over the light one.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Palette {
    pub background: Shade,
    pub surface: Shade,
    pub surface_high: Shade,
    pub border: Shade,
    pub foreground: Shade,
    pub muted: Shade,
    pub good: Shade,
    pub danger: Shade,
    pub danger_foreground: Shade,
    pub wash: Shade,
    pub good_text: Shade,
    pub danger_text: Shade,
}

/// The dark palette, unchanged since the apps began.
pub const DARK: Palette = Palette {
    background: Shade::rgb(0.043, 0.043, 0.051),
    surface: Shade::rgb(0.110, 0.110, 0.118),
    surface_high: Shade::rgb(0.173, 0.173, 0.180),
    border: WHITE.with_alpha(0.10),
    foreground: Shade::rgb(0.961, 0.961, 0.969),
    muted: Shade {
        red: 0.922,
        green: 0.922,
        blue: 0.961,
        alpha: 0.6,
    },
    good: Shade::rgb(0.188, 0.820, 0.345),
    danger: Shade::rgb(1.0, 0.271, 0.227),
    danger_foreground: WHITE,
    wash: WHITE,
    good_text: Shade::rgb(0.188, 0.820, 0.345),
    danger_text: Shade::rgb(1.0, 0.271, 0.227),
};

/// The light palette. The muted words are at 75 per cent rather than
/// Apple's 60, which read at 3.1 to 3.4:1 on the light ground, under the
/// 4.5:1 small print needs (the web's ui-polish, 2026-10-29).
pub const LIGHT: Palette = Palette {
    background: Shade::hex(0xF2F2F7),
    surface: Shade::hex(0xFFFFFF),
    surface_high: Shade::hex(0xE5E5EA),
    border: BLACK.with_alpha(0.10),
    foreground: Shade::hex(0x1C1C1E),
    muted: Shade::hex_alpha(0x3C3C43, 0.75),
    good: Shade::hex(0x34C759),
    danger: Shade::hex(0xFF3B30),
    danger_foreground: WHITE,
    wash: BLACK,
    good_text: Shade::hex(0x217E38),
    danger_text: Shade::hex(0xD70015),
};

pub fn palette(scheme: Scheme) -> Palette {
    match scheme {
        Scheme::Dark => DARK,
        Scheme::Light => LIGHT,
    }
}

/// The one switch for the meeting's appearance (Theme.cs `Daylight`): true
/// keeps the call dark whatever the system says.
pub const MEETING_STAYS_DARK: bool = true;

/// The colours on offer for the tally, the one signal colour the interface
/// spends, chosen by the person; the first is the product's. Stored under
/// [`ACCENT_KEY`] by these lower-case names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Accent {
    Amber,
    Orange,
    Pink,
    Purple,
    Indigo,
    Blue,
    Teal,
    Green,
    Graphite,
}

pub const ACCENT_KEY: &str = "tomoshibi.accent";
pub const BADGE_KEY: &str = "tomoshibi.connection-badge";
pub const CONTROLS_KEY: &str = "meet-live.controls";

impl Accent {
    pub const ALL: [Accent; 9] = [
        Accent::Amber,
        Accent::Orange,
        Accent::Pink,
        Accent::Purple,
        Accent::Indigo,
        Accent::Blue,
        Accent::Teal,
        Accent::Green,
        Accent::Graphite,
    ];

    pub fn key(self) -> &'static str {
        match self {
            Accent::Amber => "amber",
            Accent::Orange => "orange",
            Accent::Pink => "pink",
            Accent::Purple => "purple",
            Accent::Indigo => "indigo",
            Accent::Blue => "blue",
            Accent::Teal => "teal",
            Accent::Green => "green",
            Accent::Graphite => "graphite",
        }
    }

    pub fn of(key: &str) -> Option<Accent> {
        Self::ALL.into_iter().find(|accent| accent.key() == key)
    }

    /// The fill in a scheme: Apple's system colours, the light ones a step
    /// deeper so that they still read on a white ground.
    pub fn color(self, scheme: Scheme) -> Shade {
        match scheme {
            Scheme::Dark => match self {
                Accent::Amber => Shade::rgb(1.0, 0.635, 0.231),
                Accent::Orange => Shade::rgb(1.0, 0.624, 0.039),
                Accent::Pink => Shade::rgb(1.0, 0.216, 0.373),
                Accent::Purple => Shade::rgb(0.749, 0.353, 0.949),
                Accent::Indigo => Shade::rgb(0.369, 0.361, 0.902),
                Accent::Blue => Shade::rgb(0.039, 0.518, 1.0),
                Accent::Teal => Shade::rgb(0.251, 0.784, 0.878),
                Accent::Green => Shade::rgb(0.188, 0.820, 0.345),
                Accent::Graphite => Shade::rgb(0.596, 0.596, 0.616),
            },
            Scheme::Light => match self {
                Accent::Amber => Shade::hex(0xF2901E),
                Accent::Orange => Shade::hex(0xFF9500),
                Accent::Pink => Shade::hex(0xFF2D55),
                Accent::Purple => Shade::hex(0xAF52DE),
                Accent::Indigo => Shade::hex(0x5856D6),
                Accent::Blue => Shade::hex(0x007AFF),
                Accent::Teal => Shade::hex(0x30B0C7),
                Accent::Green => Shade::hex(0x34C759),
                Accent::Graphite => Shade::hex(0x8E8E93),
            },
        }
    }

    /// The tally as words or a thin symbol on the ground: in the light scheme
    /// a deeper ink of the same hue that reads at 4.5:1.
    pub fn text_color(self, scheme: Scheme) -> Shade {
        match scheme {
            Scheme::Dark => self.color(Scheme::Dark),
            Scheme::Light => match self {
                Accent::Amber => Shade::hex(0xA35C09),
                Accent::Orange => Shade::hex(0xA15E00),
                Accent::Pink => Shade::hex(0xDF002B),
                Accent::Purple => Shade::hex(0xA338D9),
                Accent::Indigo => Shade::hex(0x5856D6),
                Accent::Blue => Shade::hex(0x006BE0),
                Accent::Teal => Shade::hex(0x217989),
                Accent::Green => Shade::hex(0x217E38),
                Accent::Graphite => Shade::hex(0x6E6E73),
            },
        }
    }

    /// What is drawn on the tally, in either scheme: the primary button's
    /// dark ink on amber (UI-SYNC decision 4).
    pub fn on_color(self) -> Shade {
        match self {
            Accent::Amber | Accent::Orange => Shade::rgb(0.122, 0.071, 0.0),
            Accent::Teal => Shade::rgb(0.0, 0.15, 0.18),
            Accent::Green => Shade::rgb(0.0, 0.17, 0.055),
            Accent::Graphite => Shade::rgb(0.07, 0.07, 0.08),
            _ => WHITE,
        }
    }
}

/// The shared scale (native-audit section 5 in the UI sync). Spacing,
/// radii and the control heights a pointer gets; icons are 0.40 of their
/// button. Use these rather than numbers of one's own.
pub mod scale {
    pub const SPACING: [i32; 11] = [2, 4, 6, 8, 10, 12, 14, 16, 20, 24, 32];
    pub const RADII: [i32; 6] = [8, 10, 14, 18, 22, 30];
    /// Pointer control heights: small, regular, large, and the primary's.
    pub const CONTROL_SMALL: i32 = 28;
    pub const CONTROL: i32 = 32;
    pub const CONTROL_LARGE: i32 = 36;
    pub const CONTROL_PRIMARY: i32 = 44;
    pub const ICON_RATIO: f32 = 0.40;
    /// The tile's corner, 0.5rem.
    pub const TILE_RADIUS: i32 = 8;
    /// A lobby card's corner.
    pub const CARD_RADIUS: i32 = 18;
    /// Menus: radius 14, row radius 10, row height 32 on a desktop.
    pub const MENU_RADIUS: i32 = 14;
    pub const MENU_ROW_RADIUS: i32 = 10;
    pub const MENU_ROW: i32 = 32;
    /// The leave capsule (UI-SYNC decision 3).
    pub const LEAVE_WIDTH: i32 = 56;
    pub const LEAVE_HEIGHT: i32 = 44;
    /// Stage buttons on a picture, drawn at this size and shown on hover.
    pub const STAGE_BUTTON: i32 = 36;
    /// The name plate on a picture: about 24 high, 8 from the edge.
    pub const NAME_PLATE: i32 = 24;
    pub const NAME_PLATE_INSET: i32 = 8;
}

/// Motion, as Theme.cs has it. Every element enters and leaves with a
/// transition; GTK skips them when `gtk-enable-animations` is off, which is
/// how reduced motion is respected, and libadwaita's animations do the same.
pub mod motion {
    /// Arrivals decelerate into place over this long (--ease-settle).
    pub const SETTLE_MS: u32 = 320;
    /// Departures need no such courtesy (--ease-leave).
    pub const LEAVE_MS: u32 = 200;
    pub const SETTLE_CURVE: (f32, f32, f32, f32) = (0.2, 0.9, 0.3, 1.0);
    pub const LEAVE_CURVE: (f32, f32, f32, f32) = (0.4, 0.0, 1.0, 1.0);
    /// What an arrival grows from.
    pub const ARRIVE_SCALE: f32 = 0.97;
    /// The screens before a call bring their pieces in from fourteen points
    /// below, over 420 ms on the arriving curve.
    pub const RISE_MS: u32 = 420;
    pub const RISE_LIFT: f32 = 14.0;
    /// Hover is 10 per cent toward the foreground, pressed 16 and a scale of
    /// 0.95, one value everywhere (UI sync, "also mirror from the web").
    pub const HOVER_MIX: f32 = 0.10;
    pub const PRESSED_MIX: f32 = 0.16;
    pub const PRESSED_SCALE: f32 = 0.95;
    /// Disabled controls are drawn at this opacity, except the primary,
    /// which turns into a neutral grey plate instead.
    pub const DISABLED_OPACITY: f32 = 0.4;
}

/// Whether the system's appearance is dark now, as libadwaita follows it.
pub fn system_scheme() -> Scheme {
    if adw::StyleManager::default().is_dark() {
        Scheme::Dark
    } else {
        Scheme::Light
    }
}

/// The named colours for one palette and accent, under a prefix.
fn named_colors(prefix: &str, palette: &Palette, accent: Accent, scheme: Scheme) -> String {
    let mut css = String::new();
    let mut define = |name: &str, shade: Shade| {
        css.push_str(&format!("@define-color {prefix}{name} {};\n", shade.css()));
    };
    define("bg", palette.background);
    define("surface", palette.surface);
    define("surface_high", palette.surface_high);
    define("border", palette.border);
    define("fg", palette.foreground);
    define("muted", palette.muted);
    define("good", palette.good);
    define("danger", palette.danger);
    define("on_danger", palette.danger_foreground);
    define("wash", palette.wash);
    define("good_text", palette.good_text);
    define("danger_text", palette.danger_text);
    define("tally", accent.color(scheme));
    define("tally_text", accent.text_color(scheme));
    define("on_tally", accent.on_color());
    css
}

/// The rules every screen shares, written once against the named colours.
/// libadwaita's own accent (a switch that is on, a selected day, the focus
/// ring) is the person's tally rather than the system's blue, as the other
/// apps draw their switches in it.
/// `.tmsb-primary` is the amber button with dark ink, a neutral grey plate
/// with grey words when disabled, never faded amber (UI-SYNC decision 4).
///
/// GTK's min-height is the content's, and libadwaita gives a button five
/// pixels above and below and a field a border: a primary given 44 was 54
/// high on screen and the link field 38. So the buttons here have no
/// vertical padding and no border, and a bordered field's min-height is its
/// height less the two borders, so that what is drawn is the scale's size.
const SHARED_RULES: &str = r#"
@define-color accent_bg_color @tmsb_tally;
@define-color accent_fg_color @tmsb_on_tally;
@define-color accent_color @tmsb_tally_text;
window.tmsb-window, window.tmsb-window > contents {
  background-color: @tmsb_bg;
  color: @tmsb_fg;
}
.tmsb-muted { color: @tmsb_muted; }
.tmsb-title { font-size: 28px; font-weight: 700; }
.tmsb-pane {
  background-color: @tmsb_surface;
  border: 1px solid @tmsb_border;
  border-radius: 30px;
  padding: 18px;
}
.tmsb-fields {
  background-color: mix(@tmsb_surface, @tmsb_wash, 0.04);
  border: 1px solid @tmsb_border;
  border-radius: 14px;
}
.tmsb-fields separator { background-color: @tmsb_border; min-height: 1px; }
.tmsb-field {
  background: none;
  border: none;
  box-shadow: none;
  outline: none;
  min-height: 44px;
  padding: 0 12px;
  color: @tmsb_fg;
}
.tmsb-field image { color: @tmsb_muted; }
.tmsb-link-field {
  background-color: mix(@tmsb_surface, @tmsb_wash, 0.04);
  border: 1px solid @tmsb_border;
  border-radius: 999px;
  min-height: 34px;
  padding: 0 14px;
}
button.tmsb-primary {
  background-color: @tmsb_tally;
  color: @tmsb_on_tally;
  border: none;
  border-radius: 999px;
  min-height: 44px;
  padding: 0 18px;
  font-weight: 600;
  box-shadow: none;
  transition: background-color 200ms cubic-bezier(0.2, 0.9, 0.3, 1),
              color 200ms cubic-bezier(0.2, 0.9, 0.3, 1),
              transform 120ms cubic-bezier(0.2, 0.9, 0.3, 1);
}
button.tmsb-primary:hover { background-color: mix(@tmsb_tally, @tmsb_fg, 0.10); }
button.tmsb-primary:active { background-color: mix(@tmsb_tally, @tmsb_fg, 0.16); transform: scale(0.95); }
button.tmsb-primary:disabled { background-color: @tmsb_surface_high; color: @tmsb_muted; }
button.tmsb-plain {
  background-color: mix(@tmsb_surface, @tmsb_wash, 0.06);
  color: @tmsb_fg;
  border: none;
  border-radius: 999px;
  min-height: 36px;
  padding: 0 14px;
  box-shadow: none;
  transition: background-color 200ms cubic-bezier(0.2, 0.9, 0.3, 1), transform 120ms cubic-bezier(0.2, 0.9, 0.3, 1);
}
button.tmsb-plain:hover { background-color: mix(@tmsb_surface, @tmsb_fg, 0.10); }
button.tmsb-plain:active { background-color: mix(@tmsb_surface, @tmsb_fg, 0.16); transform: scale(0.95); }
button.tmsb-plain:disabled { opacity: 0.4; }
.tmsb-divider { background-color: @tmsb_border; min-height: 1px; }
.tmsb-small { font-size: 11px; }
.tmsb-footnote { font-size: 11px; color: @tmsb_muted; font-family: monospace; }
headerbar.tmsb-bar { background: none; box-shadow: none; border: none; }
window.tmsb-window.tmsb-call, window.tmsb-window.tmsb-call > contents, .tmsb-call-ground {
  background-color: @tmsb_call_bg;
  color: @tmsb_call_fg;
}
window.tmsb-window.tmsb-call .tmsb-muted { color: @tmsb_call_muted; }
window.tmsb-window.tmsb-call button.tmsb-plain {
  background-color: mix(@tmsb_call_surface, @tmsb_call_wash, 0.06);
  color: @tmsb_call_fg;
}
window.tmsb-window.tmsb-call button.tmsb-plain:hover { background-color: mix(@tmsb_call_surface, @tmsb_call_fg, 0.10); }
window.tmsb-window.tmsb-call button.tmsb-plain:active { background-color: mix(@tmsb_call_surface, @tmsb_call_fg, 0.16); }
window.tmsb-window.tmsb-call button.tmsb-primary { background-color: @tmsb_call_tally; color: @tmsb_call_on_tally; }
window.tmsb-window.tmsb-call button.tmsb-primary:hover { background-color: mix(@tmsb_call_tally, @tmsb_call_fg, 0.10); }
window.tmsb-window.tmsb-call button.tmsb-primary:active { background-color: mix(@tmsb_call_tally, @tmsb_call_fg, 0.16); }
window.tmsb-window.tmsb-call button.tmsb-primary:disabled { background-color: @tmsb_call_surface_high; color: @tmsb_call_muted; }
"#;

thread_local! {
    static PALETTE_PROVIDER: RefCell<Option<gtk::CssProvider>> = const { RefCell::new(None) };
    static MODULE_PROVIDERS: RefCell<Vec<(String, gtk::CssProvider)>> = const { RefCell::new(Vec::new()) };
    static ACCENT: RefCell<Accent> = const { RefCell::new(Accent::Amber) };
}

/// The whole stylesheet for the system's scheme now and an accent: the
/// palette that follows the system, the call's palette that does not, and
/// the shared rules.
pub fn stylesheet(system: Scheme, accent: Accent) -> String {
    let mut css = named_colors("tmsb_", &palette(system), accent, system);
    let call = if MEETING_STAYS_DARK {
        Scheme::Dark
    } else {
        system
    };
    css.push_str(&named_colors("tmsb_call_", &palette(call), accent, call));
    css.push_str(SHARED_RULES);
    css
}

/// Installs the stylesheet on the default display and keeps it following the
/// system's appearance. Called once at startup, after libadwaita is set up.
pub fn install(accent: Accent) {
    ACCENT.with(|held| *held.borrow_mut() = accent);
    let Some(display) = gtk::gdk::Display::default() else {
        return;
    };
    // libadwaita turns its lengths in sp (a clamp's width, a breakpoint's
    // condition) into pixels by the text DPI. Where nothing gives one (a
    // bare compositor such as weston or sway, a container, a desktop
    // without a settings daemon or portal) GTK leaves it unset, at -1, and
    // every such length comes out as nothing: every AdwClamp collapsed to
    // its child's narrowest, and the sign-in's column was 168 wide in a
    // window of 1100. Unset means 96, which is what GTK draws text at then;
    // it is said here, before the first window, so that libadwaita reads
    // the same.
    let settings = gtk::Settings::for_display(&display);
    if settings.gtk_xft_dpi() <= 0 {
        settings.set_gtk_xft_dpi(96 * 1024);
    }
    let provider = gtk::CssProvider::new();
    provider.load_from_string(&stylesheet(system_scheme(), accent));
    gtk::style_context_add_provider_for_display(
        &display,
        &provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
    PALETTE_PROVIDER.with(|held| *held.borrow_mut() = Some(provider));

    adw::StyleManager::default().connect_dark_notify(|_| reload());
}

/// Chooses another accent, as the settings' colour row does.
pub fn set_accent(accent: Accent) {
    ACCENT.with(|held| *held.borrow_mut() = accent);
    reload();
}

pub fn accent() -> Accent {
    ACCENT.with(|held| *held.borrow())
}

fn reload() {
    let accent = accent();
    PALETTE_PROVIDER.with(|held| {
        if let Some(provider) = held.borrow().as_ref() {
            provider.load_from_string(&stylesheet(system_scheme(), accent));
        }
    });
}

/// A module's own rules, under a name of its own (`"room"`, `"settings"`),
/// written against the `tmsb_*` and `tmsb_call_*` names. Installing a name
/// again replaces what it had. Kept at a priority just above the shared
/// rules, so a module can refine them for its own widgets.
pub fn add_module_css(name: &str, css: &str) {
    let Some(display) = gtk::gdk::Display::default() else {
        return;
    };
    MODULE_PROVIDERS.with(|held| {
        let mut held = held.borrow_mut();
        if let Some((_, provider)) = held.iter().find(|(kept, _)| kept == name) {
            provider.load_from_string(css);
            return;
        }
        let provider = gtk::CssProvider::new();
        provider.load_from_string(css);
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION + 1,
        );
        held.push((name.to_owned(), provider));
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colours_are_written_as_the_other_platforms_round_them() {
        assert_eq!(LIGHT.background.css(), "#f2f2f7");
        assert_eq!(DARK.background.css(), "#0b0b0d");
        assert_eq!(Accent::Amber.color(Scheme::Dark).css(), "#ffa23b");
        assert_eq!(BLACK.with_alpha(0.10).css(), "rgba(0,0,0,0.1)");
    }

    #[test]
    fn the_light_words_are_at_three_quarters() {
        // 60 per cent read under 4.5:1 on the light ground (ui-polish).
        assert_eq!(LIGHT.muted.alpha, 0.75);
        assert_eq!(DARK.muted.alpha, 0.6);
    }

    #[test]
    fn accents_are_stored_by_the_names_every_platform_uses() {
        for accent in Accent::ALL {
            assert_eq!(Accent::of(accent.key()), Some(accent));
        }
        assert_eq!(Accent::of("lamplight"), None);
    }

    #[test]
    fn the_call_is_dark_in_a_light_system() {
        let css = stylesheet(Scheme::Light, Accent::Amber);
        assert!(css.contains("@define-color tmsb_bg #f2f2f7;"));
        assert!(css.contains("@define-color tmsb_call_bg #0b0b0d;"));
    }

    #[test]
    fn the_shared_buttons_take_the_call_palette_inside_a_call() {
        // Seen on a light system: the call's ground was dark and its leave
        // button the day's white plate.
        let css = stylesheet(Scheme::Light, Accent::Amber);
        for rule in [
            "window.tmsb-window.tmsb-call button.tmsb-plain {",
            "window.tmsb-window.tmsb-call button.tmsb-primary {",
        ] {
            let at = css.find(rule).unwrap_or_else(|| panic!("no rule {rule}"));
            let body = &css[at..at + css[at..].find('}').unwrap()];
            assert!(
                body.contains("@tmsb_call_"),
                "{rule} uses the system's palette"
            );
            assert!(
                !body.contains("@tmsb_surface"),
                "{rule} uses the system's palette"
            );
        }
    }

    #[test]
    fn libadwaitas_accent_is_the_tally() {
        let css = stylesheet(Scheme::Dark, Accent::Teal);
        assert!(css.contains("@define-color accent_bg_color @tmsb_tally;"));
        assert!(css.contains("@define-color tmsb_tally #40c8e0;"));
    }

    #[test]
    fn nothing_is_a_gradient() {
        let css = stylesheet(Scheme::Dark, Accent::Amber);
        assert!(
            !css.contains("gradient"),
            "the owner's rule: no gradients anywhere"
        );
    }
}
