//! The relays a deployment offers, and which one is nearest.
//!
//! Follows web/src/live/relays.ts. The list comes from `GET /api/relays`; the
//! choice is made by timing each candidate the way the web client does, so a
//! native participant lands where a browser on the same network would.
//!
//! (Being written: the shapes and signatures below are fixed so that the API
//! module can call them; the bodies are filled in separately.)

use crate::ProtocolError;

/// One relay as `GET /api/relays` describes it.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct Relay {
    pub name: String,
    pub url: Option<String>,
    pub region: Option<String>,
    pub label: Option<String>,
    pub lat: Option<f64>,
    pub lon: Option<f64>,
    pub probe: Option<String>,
    pub maintenance: bool,
    pub fallback: bool,
    pub admin_only: bool,
}

/// The whole answer: the relays, whether the client is expected to measure,
/// and how much of the fleet is up.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct RelayList {
    pub relays: Vec<Relay>,
    pub measure: bool,
    pub online: u32,
    pub total: u32,
}

/// Reads `GET /api/relays` from the deployment at `base`.
pub async fn fetch(client: &reqwest::Client, base: &url::Url) -> Result<RelayList, ProtocolError> {
    let _ = (client, base);
    Err(ProtocolError::Invalid { message: "relays::fetch is being written".into() })
}

/// The relay to ask for when joining, or `None` to let the server choose.
pub async fn preferred(list: &RelayList) -> Option<String> {
    let _ = list;
    None
}
