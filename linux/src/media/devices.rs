//! Which [`Media`] the app runs with: the real devices of this machine,
//! put together from audio.rs, camera.rs and screen.rs.
//!
//! Owned by the media module, like those three. Until they are written it
//! is [`Quiet`], which lists and opens nothing, so that every screen can be
//! built and pictured without devices. A debug run given
//! `TOMOSHIBI_SOURCE` (app::Launch) is meant to send that file in place of
//! the camera or the microphone, for tests on machines with neither.

use std::rc::Rc;

use super::{Media, Quiet};

/// The media this machine has.
pub fn standard() -> Rc<dyn Media> {
    Rc::new(Quiet)
}
