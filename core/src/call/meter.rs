//! The connection meter, fed (web/src/live/connection.ts,
//! `useConnectionQuality`): every two seconds, every publication's stats
//! report, this participant's first, folded by the protocol crate's
//! `ConnectionMeter` into what the corner of the call shows.
//!
//! The light only ever reflected the media server's own verdict before this,
//! which grades on its own scale and does not see a round trip or a loss the
//! way the person in the call feels them; the web reads the numbers, and so
//! does this.

use std::{sync::Weak, time::Duration};

use livekit::{
    prelude::*,
    webrtc::stats::{IceCandidatePairState, IceCandidateType, IceServerTransportProtocol, QualityLimitationReason, RtcStats},
};
use tomoshibi_protocol::quality::{connection_sample_every_ms, ServerQuality, StatsEntry, TrackReport};

use super::{freeze::now_ms, Call, CallEvent};

fn text(value: &str) -> Option<String> {
    (!value.is_empty()).then(|| value.to_string())
}

/// One report in the shape the web reads, holding only what it reads.
fn entries(stats: Vec<RtcStats>) -> Vec<StatsEntry> {
    stats
        .into_iter()
        .map(|entry| match entry {
            RtcStats::LocalCandidate(local) => StatsEntry::LocalCandidate {
                id: local.rtc.id,
                address: text(&local.local_candidate.address),
                candidate_type: local.local_candidate.candidate_type.map(|kind| {
                    match kind {
                        IceCandidateType::Host => "host",
                        IceCandidateType::Srflx => "srflx",
                        IceCandidateType::Prflx => "prflx",
                        IceCandidateType::Relay => "relay",
                    }
                    .to_string()
                }),
                related_address: text(&local.local_candidate.related_address),
                url: text(&local.local_candidate.url),
                relay_protocol: local.local_candidate.relay_protocol.map(|protocol| {
                    match protocol {
                        IceServerTransportProtocol::Udp => "udp",
                        IceServerTransportProtocol::Tcp => "tcp",
                        IceServerTransportProtocol::Tls => "tls",
                    }
                    .to_string()
                }),
            },
            RtcStats::CandidatePair(pair) => StatsEntry::CandidatePair {
                id: pair.rtc.id,
                state: pair.candidate_pair.state.map(|state| {
                    match state {
                        IceCandidatePairState::Frozen => "frozen",
                        IceCandidatePairState::Waiting => "waiting",
                        IceCandidatePairState::InProgress => "in-progress",
                        IceCandidatePairState::Failed => "failed",
                        IceCandidatePairState::Succeeded => "succeeded",
                    }
                    .to_string()
                }),
                nominated: Some(pair.candidate_pair.nominated),
                current_round_trip_time: Some(pair.candidate_pair.current_round_trip_time),
                local_candidate_id: text(&pair.candidate_pair.local_candidate_id),
            },
            RtcStats::OutboundRtp(out) => StatsEntry::OutboundRtp {
                id: out.rtc.id,
                ssrc: Some(out.stream.ssrc),
                // The sending side has no track identifier in the standard's
                // outbound entry; its media source is what tells one share
                // from the next.
                track_identifier: text(&out.outbound.media_source_id),
                bytes_sent: Some(out.sent.bytes_sent as f64),
                packets_sent: Some(out.sent.packets_sent as f64),
                frame_width: (out.outbound.frame_width > 0).then_some(out.outbound.frame_width as f64),
                frame_height: (out.outbound.frame_height > 0).then_some(out.outbound.frame_height as f64),
                frames_per_second: (out.outbound.frames_per_second > 0.0).then_some(out.outbound.frames_per_second),
                quality_limitation_reason: Some(
                    match out.outbound.quality_limitation_reason {
                        QualityLimitationReason::None => "none",
                        QualityLimitationReason::Cpu => "cpu",
                        QualityLimitationReason::Bandwidth => "bandwidth",
                        QualityLimitationReason::Other => "other",
                    }
                    .to_string(),
                ),
            },
            RtcStats::RemoteInboundRtp(remote) => StatsEntry::RemoteInboundRtp {
                id: remote.rtc.id,
                packets_lost: Some(remote.received.packets_lost as f64),
                jitter: Some(remote.received.jitter),
                round_trip_time: Some(remote.remote_inbound.round_trip_time),
            },
            RtcStats::InboundRtp(inbound) => StatsEntry::InboundRtp {
                id: inbound.rtc.id,
                ssrc: Some(inbound.stream.ssrc),
                track_identifier: text(&inbound.inbound.track_identifier),
                bytes_received: Some(inbound.inbound.bytes_received as f64),
                frame_width: (inbound.inbound.frame_width > 0).then_some(inbound.inbound.frame_width as f64),
                frame_height: (inbound.inbound.frame_height > 0).then_some(inbound.inbound.frame_height as f64),
                frames_per_second: (inbound.inbound.frames_per_second > 0.0).then_some(inbound.inbound.frames_per_second),
            },
            other => StatsEntry::Other { id: id_of(&other) },
        })
        .collect()
}

fn id_of(entry: &RtcStats) -> String {
    match entry {
        RtcStats::Codec(s) => s.rtc.id.clone(),
        RtcStats::MediaSource(s) => s.rtc.id.clone(),
        RtcStats::MediaPlayout(s) => s.rtc.id.clone(),
        RtcStats::PeerConnection(s) => s.rtc.id.clone(),
        RtcStats::DataChannel(s) => s.rtc.id.clone(),
        RtcStats::Transport(s) => s.rtc.id.clone(),
        RtcStats::RemoteCandidate(s) => s.rtc.id.clone(),
        RtcStats::Certificate(s) => s.rtc.id.clone(),
        RtcStats::RemoteOutboundRtp(s) => s.rtc.id.clone(),
        RtcStats::Stream(s) => s.rtc.id.clone(),
        _ => String::new(),
    }
}

/// The server's verdict about this participant, as the meter takes it.
pub(super) fn server_quality(quality: ConnectionQuality) -> ServerQuality {
    match quality {
        ConnectionQuality::Excellent => ServerQuality::Excellent,
        ConnectionQuality::Good => ServerQuality::Good,
        ConnectionQuality::Poor => ServerQuality::Poor,
        ConnectionQuality::Lost => ServerQuality::Lost,
    }
}

/// Samples until the call goes away.
pub(super) async fn keep(call: Weak<Call>) {
    let mut tick = tokio::time::interval(Duration::from_millis(connection_sample_every_ms()));
    loop {
        tick.tick().await;
        let Some(call) = call.upgrade() else { return };
        let mut reports = Vec::new();
        for publication in call.room.local_participant().track_publications().into_values() {
            if let Some(track) = publication.track() {
                if let Ok(stats) = track.get_stats().await {
                    reports.push(TrackReport {
                        local: true,
                        screen_share: publication.source() == TrackSource::Screenshare,
                        entries: entries(stats),
                    });
                }
            }
        }
        for participant in call.room.remote_participants().into_values() {
            for publication in participant.track_publications().into_values() {
                if let Some(track) = publication.track() {
                    if let Ok(stats) = track.get_stats().await {
                        reports.push(TrackReport {
                            local: false,
                            screen_share: publication.source() == TrackSource::Screenshare,
                            entries: entries(stats),
                        });
                    }
                }
            }
        }
        let reading = call.meter.clone().sample(now_ms() as u64, reports, None).await;
        call.observer.on_event(CallEvent::Connection { reading });
    }
}
