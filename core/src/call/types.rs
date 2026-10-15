//! What crosses into the apps: the shapes of participants, tracks and events.
//!
//! LiveKit's own types do not cross the binding, and should not: the apps see
//! a call the way the web client's UI does, as people with pictures and
//! sounds, and every field here is one some screen reads.

use std::collections::HashMap;

use livekit::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum MediaKind {
    Audio,
    Video,
}

/// Where a track comes from, as the web client names the sources. A shared
/// screen and its sound are two sources, and music is `Unknown` with the name
/// "music" or "music-lossless" (web/src/live/lossless.ts). The protocol crate
/// owns the type, since its sharing rules speak of sources too, and two copies
/// of one enum collide in the apps' one Swift module.
pub use tomoshibi_protocol::share::MediaSource;

pub(crate) fn media_source(source: TrackSource) -> MediaSource {
    match source {
        TrackSource::Camera => MediaSource::Camera,
        TrackSource::Microphone => MediaSource::Microphone,
        TrackSource::Screenshare => MediaSource::ScreenShare,
        TrackSource::ScreenshareAudio => MediaSource::ScreenShareAudio,
        _ => MediaSource::Unknown,
    }
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct TrackInfo {
    pub sid: String,
    pub kind: MediaKind,
    pub source: MediaSource,
    /// Load-bearing for some sources: "listening", "listening-lossless",
    /// "music" and "music-lossless" each mean something to every client.
    pub name: String,
    pub muted: bool,
    pub width: u32,
    pub height: u32,
    pub simulcast: bool,
    pub subscribed: bool,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct ParticipantInfo {
    /// `<t|g|a><trip>-<random>`; what it proves is in the identity module.
    pub identity: String,
    pub name: String,
    pub attributes: HashMap<String, String>,
    pub tracks: Vec<TrackInfo>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum LinkQuality {
    Excellent,
    Good,
    Poor,
    Lost,
}

impl From<ConnectionQuality> for LinkQuality {
    fn from(quality: ConnectionQuality) -> Self {
        match quality {
            ConnectionQuality::Excellent => Self::Excellent,
            ConnectionQuality::Good => Self::Good,
            ConnectionQuality::Poor => Self::Poor,
            ConnectionQuality::Lost => Self::Lost,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum CallState {
    Connected,
    Reconnecting,
    Disconnected,
}

/// Everything a call tells the app, in the order it happens.
#[derive(Debug, Clone, PartialEq, uniffi::Enum)]
pub enum CallEvent {
    StateChanged { state: CallState },
    /// The call is over. `reason` is LiveKit's own word for why: the web
    /// client rejoins by itself when it is `RoomClosed` or `Removed` within
    /// 15 s of a `moving` or `placing` message (App.tsx ~560-627), and so
    /// should the apps.
    Ended { reason: String },
    ParticipantJoined { participant: ParticipantInfo },
    ParticipantLeft { identity: String },
    ParticipantChanged { participant: ParticipantInfo },
    TrackPublished { identity: String, track: TrackInfo },
    TrackUnpublished { identity: String, sid: String },
    /// Media for this track can now be attached to a sink.
    TrackSubscribed { identity: String, track: TrackInfo },
    TrackUnsubscribed { identity: String, sid: String },
    TrackMuted { identity: String, sid: String, muted: bool },
    LocalTrackPublished { track: TrackInfo },
    LocalTrackUnpublished { sid: String },
    Speaking { identities: Vec<String> },
    Quality { identity: String, quality: LinkQuality },
    Chat { id: String, identity: String, name: String, text: String, timestamp: i64 },
    /// A data packet. Server messages (`host`, `moving`, `placing`) arrive
    /// with no `identity`, and the web only believes those that do.
    Data { topic: String, identity: Option<String>, payload: Vec<u8> },
    AttributesChanged { identity: String, attributes: HashMap<String, String> },
    /// Whether frames from this participant can be read: in a sealed call a
    /// wrong word still subscribes, and only this says it.
    Sealed { identity: String, readable: bool },
    /// This participant's own share is more than its line carries (the card
    /// strain.ts shows), or no longer, when `hint` is absent.
    Strain { hint: Option<tomoshibi_protocol::strain::StrainHint> },
    /// The lossless stream beside this participant's sound was given up on:
    /// the upload could not keep up. Everyone still hears the Opus track.
    LosslessGaveUp,
    /// How the call is going, every two seconds (connection.ts): the grade,
    /// the round trip, loss, rates, jitter, the share and this device's
    /// address as the relay sees it.
    Connection { reading: tomoshibi_protocol::quality::ConnectionReading },
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct IceServer {
    pub url: String,
    pub username: String,
    pub credential: String,
}

/// How to reach a call: the join answer's `url` and `token`, the room's
/// normalised name (the sealing key is made from it), the word if the call is
/// sealed, and the join answer's `forward` when there is one.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct CallOptions {
    pub url: String,
    pub token: String,
    pub room: String,
    pub sealing_word: Option<String>,
    pub forward: Option<IceServer>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, uniffi::Error)]
pub enum CallError {
    #[error("could not connect: {message}")]
    Connect { message: String },
    #[error("could not publish: {message}")]
    Publish { message: String },
    #[error("no such track: {sid}")]
    NoSuchTrack { sid: String },
    #[error("{message}")]
    Failed { message: String },
}
