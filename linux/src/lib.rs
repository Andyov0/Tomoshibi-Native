//! Tomoshibi for Linux: GTK 4 and libadwaita over the shared core, linked
//! directly (the core's exports are ordinary Rust as well as UniFFI).
//!
//! The modules, each owned by one piece of work and changed by the others
//! only through what it makes public:
//!
//! - `app`: the model (the deployment, the account, the kept identity, the
//!   preferences, the remote configuration, the network watch) and the
//!   debug switches.
//! - `i18n`, `theme`, `ui::kit`, `ui::window`: the words, the look and the
//!   pieces every screen shares, and the window that shows one screen at a
//!   time. Added to at the end, never changed under another's feet.
//! - `ui::lobby`: the screens before a call. `ui::settings`: the settings.
//!   `ui::room`: the call and its ending.
//! - `render`: remote pictures on screen (`VideoView`).
//! - `media`: the devices (the `Media` traits in media/mod.rs are agreed
//!   between the media and room modules; audio.rs, camera.rs and screen.rs
//!   implement them).
//! - `update`: the AppImage replacing itself, and the desktop integration.
//!
//! The entry, the GtkApplication, is main.rs.

pub mod app;
pub mod i18n;
pub mod media;
pub mod render;
pub mod theme;
pub mod ui;
pub mod update;

/// The application's id: its D-Bus name, which makes it one instance per
/// session (a second launch hands its links to the first), and the name of
/// its .desktop file and icons.
pub const APP_ID: &str = "app.tomoshibi.Linux";
