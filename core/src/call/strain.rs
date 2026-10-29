//! The uplink hint for this participant's own share (web/src/live/strain.ts):
//! every two seconds while sharing, the share's sender is read and handed to
//! the protocol crate's `ShareStrainWatch`, and whatever it says the card
//! should do goes to the app as a `Strain` event.
//!
//! It only ever suggests. The owner banned lowering anybody's share on its own
//! on 2026-09-07; the card offers a lower setting and the person decides.

use std::{
    sync::{Arc, Mutex, Weak},
    time::Duration,
};

use livekit::{
    prelude::*,
    webrtc::stats::{IceCandidatePairState, QualityLimitationReason, RtcStats},
};
use tomoshibi_protocol::share::{ShareQuality, ShareRun, ShareRunning};
use tomoshibi_protocol::strain::{
    ShareStrainWatch, StrainCandidatePair, StrainEncoding, StrainMediaSource, StrainOutbound, StrainReport,
    StrainTransport, StrainUpdate,
};

use super::{freeze::now_ms, Call, CallEvent};

pub(super) struct Strain {
    pub(super) watch: Arc<ShareStrainWatch>,
    pub(super) run: Arc<ShareRun>,
    /// The share being watched: its publication and track.
    pub(super) share: Mutex<Option<(String, LocalVideoTrack)>>,
}

impl Default for Strain {
    fn default() -> Self {
        Self { watch: ShareStrainWatch::new(), run: ShareRun::new(), share: Mutex::new(None) }
    }
}

impl Strain {
    pub(super) fn started(&self, sid: String, track: LocalVideoTrack, quality: ShareQuality, fps: u32) {
        self.run.begin(quality, fps, now_ms());
        *super::held(&self.share) = Some((sid, track));
        self.watch.running_changed(self.running());
    }

    pub(super) fn stopped(&self, sid: &str) {
        let mut share = super::held(&self.share);
        if share.as_ref().is_some_and(|(s, _)| s == sid) {
            *share = None;
            self.run.end();
            self.watch.sharing_stopped();
        }
    }

    pub(super) fn running(&self) -> Option<ShareRunning> {
        self.run.current()
    }
}

fn reason(of: &QualityLimitationReason) -> String {
    match of {
        QualityLimitationReason::None => "none",
        QualityLimitationReason::Cpu => "cpu",
        QualityLimitationReason::Bandwidth => "bandwidth",
        QualityLimitationReason::Other => "other",
    }
    .into()
}

fn state(of: &Option<IceCandidatePairState>) -> Option<String> {
    of.as_ref().map(|state| {
        match state {
            IceCandidatePairState::Frozen => "frozen",
            IceCandidatePairState::Waiting => "waiting",
            IceCandidatePairState::InProgress => "in-progress",
            IceCandidatePairState::Failed => "failed",
            IceCandidatePairState::Succeeded => "succeeded",
        }
        .into()
    })
}

fn report(stats: Vec<RtcStats>) -> StrainReport {
    let mut out = StrainReport::default();
    for entry in stats {
        match entry {
            RtcStats::OutboundRtp(o) => out.outbound.push(StrainOutbound {
                id: Some(o.rtc.id.clone()),
                rid: Some(o.outbound.rid.clone()).filter(|r| !r.is_empty()),
                kind: Some(o.stream.kind.clone()),
                active: Some(o.outbound.active),
                timestamp: Some(o.rtc.timestamp as f64 / 1000.0),
                bytes_sent: Some(o.sent.bytes_sent as f64),
                frames_sent: Some(o.outbound.frames_sent as f64),
                frames_encoded: Some(o.outbound.frames_encoded as f64),
                frames_per_second: Some(o.outbound.frames_per_second),
                frame_width: Some(o.outbound.frame_width as f64),
                frame_height: Some(o.outbound.frame_height as f64),
                target_bitrate: Some(o.outbound.target_bitrate),
                quality_limitation_reason: Some(reason(&o.outbound.quality_limitation_reason)),
                media_source_id: Some(o.outbound.media_source_id.clone()),
            }),
            RtcStats::MediaSource(m) => out.media_sources.push(StrainMediaSource {
                id: Some(m.rtc.id.clone()),
                kind: Some(m.source.kind.clone()),
                frames: Some(m.video.frames as f64),
                frames_per_second: Some(m.video.frames_per_second),
                width: Some(m.video.width as f64),
                height: Some(m.video.height as f64),
            }),
            RtcStats::CandidatePair(c) => out.candidate_pairs.push(StrainCandidatePair {
                id: Some(c.rtc.id.clone()),
                state: state(&c.candidate_pair.state),
                nominated: Some(c.candidate_pair.nominated),
                available_outgoing_bitrate: Some(c.candidate_pair.available_outgoing_bitrate).filter(|b| *b > 0.0),
                bytes_sent: Some(c.candidate_pair.bytes_sent as f64),
            }),
            RtcStats::Transport(t) => out.transports.push(StrainTransport {
                id: Some(t.rtc.id.clone()),
                selected_candidate_pair_id: Some(t.transport.selected_candidate_pair_id.clone()),
            }),
            _ => {}
        }
    }
    out
}

/// Reads this participant's share every two seconds until the call goes.
pub(super) async fn keep(call: Weak<Call>) {
    let mut tick = tokio::time::interval(Duration::from_secs(2));
    loop {
        tick.tick().await;
        let Some(call) = call.upgrade() else { return };
        let share = super::held(&call.strain.share).clone();
        let begin = call.strain.watch.begin(call.strain.running(), share.as_ref().map(|(sid, _)| sid.clone()), now_ms());
        if begin.hide {
            call.observer.on_event(CallEvent::Strain { hint: None });
        }
        let (true, Some((_, track))) = (begin.read, share) else { continue };
        let Ok(stats) = track.get_stats().await else { continue };
        let encodings = track
            .sender_encodings()
            .into_iter()
            .map(|e| StrainEncoding {
                rid: Some(e.rid.clone()).filter(|r| !r.is_empty()),
                active: Some(e.active),
                max_bitrate: e.max_bitrate.map(|b| b as f64),
            })
            .collect();
        match call.strain.watch.reading(report(stats), encodings, now_ms()) {
            StrainUpdate::Show { hint } => call.observer.on_event(CallEvent::Strain { hint: Some(hint) }),
            StrainUpdate::Hide => call.observer.on_event(CallEvent::Strain { hint: None }),
            StrainUpdate::Keep => {}
        }
    }
}
