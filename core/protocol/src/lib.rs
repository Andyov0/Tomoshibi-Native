//! Everything the native clients share that is not media: the conversation
//! with a deployment's HTTP API, the protocols spoken over a call's data
//! channel and participant attributes, the lossless audio codec, and the rules
//! the web client applies to sharing, freezing and strain.
//!
//! The web client in the server's repository (web/src/live/) is the reference
//! for every rule here. A native participant has to be indistinguishable from
//! a browser to everybody else in the room, so each module says which file it
//! follows, and its tests are the web client's own cases wherever they exist.
//!
//! Nothing here links WebRTC. The media side lives in the crate above, which
//! feeds this one what arrives and sends what this one decides.

uniffi::setup_scaffolding!();

pub mod error;
pub mod runtime;

pub mod api;
pub mod identity;
pub mod network;
pub mod relays;

pub mod hands;
pub mod watch;
pub mod jukebox;
pub mod lossless;

pub mod share;
pub mod freeze;
pub mod strain;
pub mod quality;

pub use error::ProtocolError;
