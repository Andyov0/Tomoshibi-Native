//! Sitting out a frozen share: once a second, every shared screen this
//! participant receives is read and handed to the protocol crate's
//! `ShareFreezeWatch` (web/src/live/freeze.ts), and whatever size it asks for
//! is what this subscription requests until it says otherwise.
//!
//! It only ever changes what this viewer asks to receive, never what anybody
//! sends; the owner chose that on 2026-10-13 knowing it is an automatic step
//! down, because it is one viewer's and nobody else's.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex, Weak},
    time::Duration,
};

use livekit::{prelude::*, track::TrackDimension, webrtc::stats::RtcStats};
use tomoshibi_protocol::freeze::{
    freeze_read_inbound, freeze_subscription_size, FreezeInbound, FreezeRung, ShareFreezeWatch, FREEZE_EVERY_MS,
};

use super::Call;

/// One watch per shared screen, by track sid. A full reconnection brings a
/// new publication under the same sid; the subscription event that follows
/// tells the watch so (`subscribed`), which is what the web's per-object
/// watches achieve.
#[derive(Default)]
pub(super) struct Freezes {
    watches: Mutex<HashMap<String, Arc<ShareFreezeWatch>>>,
    /// Whether readings mean anything: the web skips a hidden tab, and an app
    /// in the background is the same.
    pub(super) background: Mutex<bool>,
}

fn rung(dimension: TrackDimension) -> FreezeRung {
    FreezeRung { width: dimension.0, height: dimension.1 }
}

pub(super) fn now_ms() -> f64 {
    use std::sync::OnceLock;
    static START: OnceLock<std::time::Instant> = OnceLock::new();
    START.get_or_init(std::time::Instant::now).elapsed().as_secs_f64() * 1000.0
}

impl Freezes {
    fn watch(&self, sid: &str) -> Arc<ShareFreezeWatch> {
        self.watches.lock().unwrap().entry(sid.to_string()).or_insert_with(|| ShareFreezeWatch::new(now_ms())).clone()
    }

    pub(super) fn forget(&self, sid: &str) {
        self.watches.lock().unwrap().remove(sid);
    }

    pub(super) fn capped(&self, sid: &str) -> Option<FreezeRung> {
        self.watches.lock().unwrap().get(sid).and_then(|watch| watch.capped())
    }

    /// A share's track has arrived (again).
    pub(super) fn subscribed(&self, sid: &str) -> Option<FreezeRung> {
        let can_tell = !*self.background.lock().unwrap();
        self.watch(sid).subscribed(can_tell)
    }
}

/// The size a share's subscription asks for: what its view wants, under any
/// cap a freeze has put on it (the SDK's rule for two wishes, the smaller by
/// area).
pub(super) fn combined(cap: Option<FreezeRung>, wanted: Option<(u32, u32)>) -> Option<(u32, u32)> {
    let measured = wanted.map(|(width, height)| FreezeRung { width, height });
    freeze_subscription_size(cap, measured).map(|size| (size.width, size.height))
}

/// Reads every received share once a second until the call goes away.
pub(super) async fn keep(call: Weak<Call>) {
    let mut tick = tokio::time::interval(Duration::from_millis(FREEZE_EVERY_MS as u64));
    loop {
        tick.tick().await;
        let Some(call) = call.upgrade() else { return };
        let can_tell = !*call.freezes.background.lock().unwrap();
        let shares: Vec<RemoteTrackPublication> = call
            .room
            .remote_participants()
            .values()
            .flat_map(|p| p.track_publications().into_values())
            .filter(|p| p.source() == TrackSource::Screenshare)
            .collect();
        for publication in shares {
            let sid = publication.sid().to_string();
            let watch = call.freezes.watch(&sid);
            let reading = match (publication.track(), can_tell) {
                (Some(track), true) => track.get_stats().await.ok().map(|stats| {
                    let entries = stats
                        .into_iter()
                        .filter_map(|entry| match entry {
                            RtcStats::InboundRtp(inbound) => Some(FreezeInbound {
                                kind: inbound.stream.kind.clone(),
                                bytes_received: Some(inbound.inbound.bytes_received as f64),
                                frames_decoded: Some(inbound.inbound.frames_decoded as f64),
                                packets_received: Some(inbound.received.packets_received as f64),
                                packets_lost: Some(inbound.received.packets_lost as f64),
                                freeze_count: Some(inbound.inbound.freeze_count as f64),
                            }),
                            _ => None,
                        })
                        .collect();
                    freeze_read_inbound(entries, now_ms())
                }),
                _ => None,
            };
            let layers = publication.video_layers().into_iter().map(rung).collect();
            let wanted = call.wanted_sizes.lock().unwrap().get(&sid).copied();
            let measured = wanted.map(|(width, height)| FreezeRung { width, height });
            let TrackDimension(width, height) = publication.dimension();
            let portrait = height > width;
            if let Some(cap) = watch.tick(reading, layers, measured, portrait, now_ms(), can_tell) {
                if let Some((width, height)) = combined(Some(cap), wanted) {
                    publication.update_video_dimensions(TrackDimension(width, height));
                }
            }
        }
    }
}
