//! Asking the system for a screen or a window: the xdg-desktop-portal's
//! ScreenCast, through ashpd.
//!
//! The portal's own picker is the system's, as a browser's
//! `getDisplayMedia` is on Linux (Chrome and Firefox use this same portal
//! under Wayland): the person picks a monitor or a window in the desktop's
//! dialog, and the app is handed a PipeWire node and a connection that can
//! see only that node. Nothing is captured without that dialog.
//!
//! Choices made here:
//!
//! - Monitors and windows, as many as the portal offers of the two
//!   (`AvailableSourceTypes`): xdg-desktop-portal-wlr offers monitors only,
//!   GNOME and KDE both. One source, as a browser shares one.
//! - The pointer drawn into the picture (`Embedded`), as a browser's share
//!   shows it, where the portal offers that; otherwise the portal's default.
//! - Nothing persisted (`PersistMode::DoNot`), on purpose. A restore token
//!   handed to GNOME's portal skips its dialog altogether
//!   (xdg-desktop-portal-gnome's `handle_start` restores the saved streams
//!   and never shows the picker), so a kept token would share whatever was
//!   picked last without asking: somebody who shared a slide deck an hour
//!   ago and presses Share for their editor would send the deck. A
//!   browser asks every time; so does this. A share changed to another size
//!   or rate keeps its session and asks nothing (`ScreenShare::change`).
//! - The app's window as the dialog's parent where there is one, so that
//!   the picker opens over the call rather than wherever the desktop puts
//!   it; a share asked for without a window (a test) passes none.

use std::os::fd::OwnedFd;

use ashpd::{
    desktop::{
        screencast::{CursorMode, Screencast, SelectSourcesOptions, SourceType},
        PersistMode, ResponseError, Session,
    },
    enumflags2::BitFlags,
    WindowIdentifier,
};
use gtk::prelude::*;

use crate::{app::log, media::MediaError};

/// What the portal handed over for one share.
pub struct Picked {
    /// The portal, kept for closing the session.
    pub portal: Screencast,
    /// The session: closing it ends the share at the compositor, and the
    /// compositor closing it (its own Stop button) is the share ended from
    /// the system.
    pub session: Session<Screencast>,
    /// A PipeWire connection that can see the session's node.
    pub remote: OwnedFd,
    pub node: u32,
    /// The size the portal says the stream is, where it says.
    pub size: Option<(u32, u32)>,
}

/// Whether the portal offers screen casting at all here: the share
/// button's question, asked once.
pub async fn available() -> bool {
    match Screencast::new().await {
        Ok(portal) => match portal.available_source_types().await {
            Ok(types) => {
                log(&format!(
                    "the screen-cast portal is there (version {}), offering {types:?}",
                    portal.version()
                ));
                types.intersects(SourceType::Monitor | SourceType::Window)
            }
            Err(fault) => {
                log(&format!(
                    "the screen-cast portal would not say what it offers: {fault}"
                ));
                false
            }
        },
        Err(fault) => {
            log(&format!("no screen-cast portal: {fault}"));
            false
        }
    }
}

/// Asks the person for a screen or a window. `Denied` when they cancel the
/// picker, `Unsupported` where there is no portal to ask.
pub async fn pick() -> Result<Picked, MediaError> {
    let portal = Screencast::new().await.map_err(fault)?;
    let offered = portal
        .available_source_types()
        .await
        .unwrap_or_else(|_| BitFlags::from(SourceType::Monitor));
    let wanted = offered & (SourceType::Monitor | SourceType::Window);
    if wanted.is_empty() {
        log(&format!(
            "the screen-cast portal offers neither monitors nor windows: {offered:?}"
        ));
        return Err(MediaError::Unsupported);
    }
    let cursors = portal.available_cursor_modes().await.unwrap_or_default();
    let session = portal
        .create_session(Default::default())
        .await
        .map_err(fault)?;
    match started(&portal, &session, wanted, cursors).await {
        Ok((remote, node, size, kind)) => {
            log(&format!(
                "the portal handed over node {node} ({kind:?}, {size:?}), offering {wanted:?}"
            ));
            Ok(Picked {
                portal,
                session,
                remote,
                node,
                size,
            })
        }
        Err(fault) => {
            // A session that never started is closed rather than left for
            // the portal to clean up when the connection goes.
            let _ = session.close().await;
            Err(fault)
        }
    }
}

type Started = (OwnedFd, u32, Option<(u32, u32)>, Option<SourceType>);

async fn started(
    portal: &Screencast,
    session: &Session<Screencast>,
    wanted: BitFlags<SourceType>,
    cursors: BitFlags<CursorMode>,
) -> Result<Started, MediaError> {
    let mut options = SelectSourcesOptions::default()
        .set_sources(wanted)
        .set_multiple(false);
    if cursors.contains(CursorMode::Embedded) {
        options = options.set_cursor_mode(CursorMode::Embedded);
    }
    // The persist mode is version 4's; an older portal is asked nothing
    // about it and persists nothing anyway.
    if portal.version() >= 4 {
        options = options.set_persist_mode(PersistMode::DoNot);
    }
    portal
        .select_sources(session, options)
        .await
        .map_err(fault)?
        .response()
        .map_err(fault)?;

    // Kept until the answer: on Wayland the parent is an exported handle,
    // withdrawn when this is dropped.
    let parent = parent().await;
    let streams = portal
        .start(session, parent.as_ref(), Default::default())
        .await
        .map_err(fault)?
        .response()
        .map_err(fault)?;
    drop(parent);
    let stream = streams.streams().first().ok_or(MediaError::NotFound)?;
    let size = stream
        .size()
        .and_then(|(w, h)| Some((u32::try_from(w).ok()?, u32::try_from(h).ok()?)));
    let remote = portal
        .open_pipe_wire_remote(session, Default::default())
        .await
        .map_err(fault)?;
    Ok((
        remote,
        stream.pipe_wire_node_id(),
        size,
        stream.source_type(),
    ))
}

/// The window the picker should open over: the app's active one, where
/// GTK is running.
async fn parent() -> Option<WindowIdentifier> {
    if !gtk::is_initialized_main_thread() {
        return None;
    }
    let window = gtk::gio::Application::default()
        .and_downcast::<gtk::Application>()
        .and_then(|application| application.active_window())?;
    WindowIdentifier::from_native(&window).await
}

/// What a portal failure means to the room.
fn fault(error: ashpd::Error) -> MediaError {
    match error {
        ashpd::Error::Response(ResponseError::Cancelled) => {
            log("the screen picker was cancelled");
            MediaError::Denied
        }
        ashpd::Error::PortalNotFound(_) => MediaError::Unsupported,
        ashpd::Error::Zbus(zbus) if is_missing(&zbus) => {
            log(&format!("no screen-cast portal: {zbus}"));
            MediaError::Unsupported
        }
        other => MediaError::Failed(format!("the screen-cast portal: {other}")),
    }
}

/// A session bus with no portal on it, or a portal without ScreenCast.
fn is_missing(error: &ashpd::zbus::Error) -> bool {
    let text = error.to_string();
    text.contains("ServiceUnknown")
        || text.contains("UnknownInterface")
        || text.contains("UnknownMethod")
}
