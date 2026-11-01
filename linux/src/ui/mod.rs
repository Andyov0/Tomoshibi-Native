//! Everything on screen. One window (window.rs) shows one screen at a time,
//! as the Mac app's single window does (TomoshibiKit's Screens.swift): the
//! screens before a call are the lobby module's (lobby/), the call and its
//! ending the room module's (room/), and the settings open over either
//! (settings/).
//!
//! A module builds its screens from a [`Context`] and nothing else, and
//! reads and changes the app through its [`AppModel`]: a screen never keeps
//! state the model should hold, so that the window can throw a screen away
//! and build it again (a change of language does) without losing anything.

pub mod kit;
pub mod lobby;
pub mod room;
pub mod settings;
pub mod window;

use std::rc::Rc;

use crate::{app::AppModel, media::Media};

/// What every screen is built with: the model, the devices, and the window
/// it is shown in (for dialogs and the settings). Cheap to clone.
#[derive(Clone)]
pub struct Context {
    pub model: AppModel,
    pub media: Rc<dyn Media>,
    window: gtk::glib::WeakRef<adw::ApplicationWindow>,
}

impl Context {
    pub fn new(model: AppModel, media: Rc<dyn Media>, window: &adw::ApplicationWindow) -> Context {
        let weak = gtk::glib::WeakRef::new();
        weak.set(Some(window));
        Context {
            model,
            media,
            window: weak,
        }
    }

    /// The main window, while it is open.
    pub fn window(&self) -> Option<adw::ApplicationWindow> {
        self.window.upgrade()
    }
}
