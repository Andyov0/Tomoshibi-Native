//! The Tomoshibi native core: media, protocol and the conversation with a
//! deployment, shared by every app. The apps reach it through bindings that
//! UniFFI generates from what this crate exports.

uniffi::setup_scaffolding!();

pub use tomoshibi_protocol::runtime;

pub mod call;
pub mod spike;

/// The core's own version, which the apps show beside theirs so that a report
/// says which core it came from.
#[uniffi::export]
pub fn core_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}
