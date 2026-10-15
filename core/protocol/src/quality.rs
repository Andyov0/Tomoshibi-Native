//! The connection meter: how the call is actually going, from the numbers.
//!
//! Follows web/src/live/connection.ts. Three numbers carry most of it, and they
//! say different things. Round trip is distance: it barely moves with load, and
//! it is the one that says "you are on the wrong relay". Loss is the network
//! refusing what was sent; it breaks a voice and gets worse the harder the call
//! pushes. Rate is what is being pushed, which means little alone and separates
//! a quiet call from a starved one beside the other two. The media server's own
//! verdict about this participant is a floor and never the answer.
//!
//! The media crate hands in what the web reads from `getRTCStatsReport()`, one
//! [`TrackReport`] per publication that has stats, every two seconds
//! ([`SAMPLE_EVERY`]); [`ConnectionMeter`] folds them exactly as the web's hook
//! does and says what the panel shows.
//!
//! Left out because only a browser needs them: the hook's `live` flag (a
//! React effect outliving its room), the walk over the SDK's publications (the
//! media crate does it and hands over the reports), the server region read off
//! `room.serverInfo` (passed in as `holding`), and the RTCPeerConnection that
//! asks a relay for this device's address, which is a single STUN binding
//! request sent directly here.

use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
    time::Duration,
};

use tokio::sync::OnceCell;

/// How often the web takes a reading (connection.ts, `EVERY_MS`). Rates are
/// means across this gap, because an instantaneous bitrate is a flicker.
pub const SAMPLE_EVERY: Duration = Duration::from_millis(2000);

/// How many samples in a row with nothing to say about why a share is held
/// back keep the last reason (connection.ts, `MISSING_FOR`): four seconds,
/// enough to ride out a report taken while the encoder is reconfiguring, not
/// so long that a browser which stopped reporting goes on being quoted.
pub const MISSING_FOR: u32 = 2;

/// Round trips, in milliseconds, past which the light turns amber and then red
/// (connection.ts, `GOOD_RTT_MS` and `FAIR_RTT_MS`): a hundred and fifty is
/// where people start talking over each other, four hundred where they give up
/// and take turns.
pub const GOOD_RTT_MS: f64 = 150.0;
pub const FAIR_RTT_MS: f64 = 400.0;

/// Loss, in per cent of what was sent, past which the light turns amber and
/// then red (connection.ts, `GOOD_LOSS` and `FAIR_LOSS`): two per cent is
/// audible on a voice, eight is a call somebody is going to leave.
pub const GOOD_LOSS_PERCENT: f64 = 2.0;
pub const FAIR_LOSS_PERCENT: f64 = 8.0;

/// How long a relay is given to say what address it sees before the row stays
/// empty (connection.ts, `ASK_FOR`).
pub const ASK_FOR: Duration = Duration::from_millis(5000);

/// What the light says.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum ConnectionGrade {
    Good,
    Fair,
    Poor,
    Lost,
}

/// The media server's verdict about this participant, as LiveKit publishes it
/// in `ConnectionQualityChanged`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum ServerQuality {
    Excellent,
    Good,
    Poor,
    Lost,
    Unknown,
}

/// Why an encoder is sending less than it was asked for, where it says.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum ShareLimitation {
    Cpu,
    Bandwidth,
    /// The browser knows and will not say. Kept, because it is worth seeing
    /// once when everything else looks fine.
    Other,
}

/// What a screen share is actually managing.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct ShareReading {
    pub width: Option<f64>,
    pub height: Option<f64>,
    pub fps: Option<f64>,
    /// Whether this device is the one sending it; otherwise it is somebody
    /// else's share as it arrives here.
    pub sending: bool,
    /// Only ever said for a share this device is sending.
    pub limited: Option<ShareLimitation>,
}

/// One reading of the meter (connection.ts, `Reading`).
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct ConnectionReading {
    pub grade: ConnectionGrade,
    /// Round trip to the relay, whole milliseconds.
    pub rtt_ms: Option<f64>,
    /// Of what was sent over the last interval, the share the far end never
    /// received.
    pub loss_percent: Option<f64>,
    pub up_kbps: Option<f64>,
    pub down_kbps: Option<f64>,
    /// How unevenly packets arrive, whole milliseconds.
    pub jitter_ms: Option<f64>,
    /// Absent when nothing is shared, so the row is not a permanent dash.
    pub share: Option<ShareReading>,
    /// This device's own public address as the relay sees it, and never the
    /// relay's.
    pub own_address: Option<String>,
    /// What the machine holding the call calls itself, where it says.
    pub holding: Option<String>,
    /// Whether anything has been measured yet. Before then the light is green
    /// by default, not by evidence.
    pub measured: bool,
}

impl ConnectionReading {
    /// The reading before a room, or before its first sample: nothing
    /// measured, rather than a healthy zero, which is how a disconnected call
    /// comes to show three green bars.
    pub fn unmeasured() -> Self {
        ConnectionReading {
            grade: ConnectionGrade::Good,
            rtt_ms: None,
            loss_percent: None,
            up_kbps: None,
            down_kbps: None,
            jitter_ms: None,
            share: None,
            own_address: None,
            holding: None,
            measured: false,
        }
    }
}

/// One entry of a WebRTC stats report, holding only the fields the web reads,
/// named as the W3C statistics name them. A field the report does not carry is
/// `None`; `id` is "" where the entry has none.
#[derive(Debug, Clone, PartialEq, uniffi::Enum)]
pub enum StatsEntry {
    LocalCandidate {
        id: String,
        address: Option<String>,
        candidate_type: Option<String>,
        related_address: Option<String>,
        url: Option<String>,
        relay_protocol: Option<String>,
    },
    CandidatePair {
        id: String,
        state: Option<String>,
        nominated: Option<bool>,
        /// Seconds.
        current_round_trip_time: Option<f64>,
        local_candidate_id: Option<String>,
    },
    OutboundRtp {
        id: String,
        ssrc: Option<u32>,
        track_identifier: Option<String>,
        bytes_sent: Option<f64>,
        packets_sent: Option<f64>,
        frame_width: Option<f64>,
        frame_height: Option<f64>,
        frames_per_second: Option<f64>,
        quality_limitation_reason: Option<String>,
    },
    RemoteInboundRtp {
        id: String,
        packets_lost: Option<f64>,
        /// Seconds.
        jitter: Option<f64>,
        /// Seconds.
        round_trip_time: Option<f64>,
    },
    InboundRtp {
        id: String,
        ssrc: Option<u32>,
        track_identifier: Option<String>,
        bytes_received: Option<f64>,
        frame_width: Option<f64>,
        frame_height: Option<f64>,
        frames_per_second: Option<f64>,
    },
    /// Anything else. Still counted for the de-duplication by id.
    Other { id: String },
}

/// One publication's stats report.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct TrackReport {
    /// Published by this device, rather than received from somebody else.
    pub local: bool,
    /// The publication's source is the screen share (not its audio).
    pub screen_share: bool,
    pub entries: Vec<StatsEntry>,
}

/// The four states a share's reason can be in on the way in. `Nothing` is the
/// browser saying "none", which clears a warning, and is kept apart from a
/// report that said nothing: the two once collapsed into one, and a cleared
/// CPU warning went on being carried forward for the rest of the call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Limit {
    Cpu,
    Bandwidth,
    Other,
    Nothing,
}

/// What one pass over the reports found (connection.ts, `Gathered`).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Gathered {
    pub rtt_ms: Option<f64>,
    pub jitter_ms: Option<f64>,
    pub own_address: Option<String>,
    /// The relay to ask for this device's address, where the candidates did
    /// not say, as a `stun:` URL.
    pub ask_at: Option<String>,
    pub share: Option<GatheredShare>,
    pub bytes_sent: f64,
    pub bytes_received: f64,
    pub packets_sent: f64,
    pub packets_lost: f64,
}

/// A share as one pass found it.
#[derive(Debug, Clone, PartialEq)]
pub struct GatheredShare {
    pub width: Option<f64>,
    pub height: Option<f64>,
    pub fps: Option<f64>,
    pub sending: bool,
    pub limited: Option<Limit>,
    /// Which track it is, so nothing is carried from one share to the next.
    pub id: String,
}

/// A local candidate, as much of it as the address on the panel needs
/// (connection.ts, `Candidate`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub address: String,
    pub kind: String,
    pub related: Option<String>,
    pub relayed: bool,
    pub url: Option<String>,
}

/// A stats field that is a number and not something else.
fn numeric(value: Option<f64>) -> Option<f64> {
    value.filter(|value| value.is_finite())
}

/// JavaScript's `Math.round`: halves go up, towards positive infinity.
fn js_round(value: f64) -> f64 {
    let floor = value.floor();
    if value - floor >= 0.5 {
        floor + 1.0
    } else {
        floor
    }
}

/// The grade, with the server's verdict as a floor (connection.ts, `grade`).
///
/// The worse of distance and loss, not their average: a perfect round trip
/// losing a tenth of everything is not a fair connection. The server can make
/// the reading worse and never better, because a reading that says all is well
/// while the call visibly is not sends somebody looking in the wrong place.
#[uniffi::export]
pub fn connection_grade(published: ServerQuality, rtt_ms: Option<f64>, loss_percent: Option<f64>) -> ConnectionGrade {
    if published == ServerQuality::Lost {
        return ConnectionGrade::Lost;
    }

    let mut own = ConnectionGrade::Good;

    if let Some(rtt) = rtt_ms {
        if rtt > FAIR_RTT_MS {
            own = ConnectionGrade::Poor;
        } else if rtt > GOOD_RTT_MS {
            own = ConnectionGrade::Fair;
        }
    }

    if let Some(loss) = loss_percent {
        if loss > FAIR_LOSS_PERCENT {
            own = ConnectionGrade::Poor;
        } else if loss > GOOD_LOSS_PERCENT && own == ConnectionGrade::Good {
            own = ConnectionGrade::Fair;
        }
    }

    if published == ServerQuality::Poor && own == ConnectionGrade::Good {
        return ConnectionGrade::Fair;
    }

    own
}

/// How often the panel should be refreshed, in milliseconds.
#[uniffi::export]
pub fn connection_sample_every_ms() -> u64 {
    SAMPLE_EVERY.as_millis() as u64
}

/// Reads every publication's report and adds them up (connection.ts,
/// `gather`).
///
/// This device's publications are walked before everybody else's whatever
/// order they arrive in, because that is the web's order and both the
/// de-duplication and which share is reported depend on it. An entry whose id
/// has been seen is skipped: a track's report describes its whole peer
/// connection, so two tracks on one connection would otherwise count every
/// byte twice.
pub fn gather(reports: &[TrackReport]) -> Gathered {
    let mut out = Gathered::default();

    let ordered = reports.iter().filter(|report| report.local).chain(reports.iter().filter(|report| !report.local));

    let mut walked: Vec<&TrackReport> = Vec::with_capacity(reports.len());
    for report in ordered {
        walked.push(report);

        // The share is read from its own publication rather than from the
        // pooled numbers, because a frame rate cannot be added up: a camera at
        // thirty beside a share at fifteen would read as forty-five.
        if report.screen_share {
            if report.local {
                read_share(report, true, &mut out);
            } else if !out.share.as_ref().is_some_and(|share| share.sending) {
                // Somebody else's share, unless this device is sending one:
                // then the question is about its own encoder.
                read_share(report, false, &mut out);
            }
        }
    }

    let mut seen: HashSet<&str> = HashSet::new();
    // Collected as met and resolved afterwards, because the pair naming a
    // candidate and the candidate itself come in whatever order the report has.
    // Kept in the order met, as the web's Map keeps them.
    let mut locals: Vec<(String, Candidate)> = Vec::new();
    let mut nominated: Option<String> = None;

    for report in walked {
        for entry in &report.entries {
            let id = entry_id(entry);
            if !id.is_empty() && !seen.insert(id) {
                continue;
            }

            match entry {
                StatsEntry::LocalCandidate { id, address, candidate_type, related_address, url, relay_protocol } => {
                    let Some(address) = address else { continue };
                    if id.is_empty() {
                        continue;
                    }

                    let candidate = Candidate {
                            address: address.clone(),
                            kind: candidate_type.clone().unwrap_or_default(),
                            related: related_address.clone(),
                            url: url.clone(),
                            relayed: candidate_type.as_deref() == Some("relay")
                                || relay_protocol.is_some()
                                || url.as_deref().is_some_and(|url| url.starts_with("turn")),
                    };

                    match locals.iter_mut().find(|(known, _)| known == id) {
                        Some(slot) => slot.1 = candidate,
                        None => locals.push((id.clone(), candidate)),
                    }
                }

                StatsEntry::CandidatePair { state, nominated: chosen, current_round_trip_time, local_candidate_id, .. } => {
                    // Only the pair in use; the ones that lost the race have
                    // stale or absent timings.
                    if state.as_deref() != Some("succeeded") || *chosen != Some(true) {
                        continue;
                    }

                    if let Some(rtt) = numeric(*current_round_trip_time) {
                        out.rtt_ms = Some(js_round(rtt * 1000.0));
                    }

                    if let Some(local) = local_candidate_id {
                        nominated = Some(local.clone());
                    }
                }

                StatsEntry::OutboundRtp { bytes_sent, packets_sent, .. } => {
                    out.bytes_sent += numeric(*bytes_sent).unwrap_or(0.0);
                    out.packets_sent += numeric(*packets_sent).unwrap_or(0.0);
                }

                StatsEntry::RemoteInboundRtp { packets_lost, jitter, round_trip_time, .. } => {
                    // What the far end says it did not get. A packet that never
                    // arrived leaves no trace on this side, so loss can only be
                    // read from the report that came back.
                    out.packets_lost += numeric(*packets_lost).unwrap_or(0.0).max(0.0);

                    if let Some(jitter) = numeric(*jitter) {
                        out.jitter_ms = Some(js_round(jitter * 1000.0));
                    }

                    // Some stacks report a round trip here and nowhere else.
                    if out.rtt_ms.is_none() {
                        if let Some(rtt) = numeric(*round_trip_time) {
                            out.rtt_ms = Some(js_round(rtt * 1000.0));
                        }
                    }
                }

                StatsEntry::InboundRtp { bytes_received, .. } => {
                    out.bytes_received += numeric(*bytes_received).unwrap_or(0.0);
                }

                StatsEntry::Other { .. } => {}
            }
        }
    }

    if let Some(nominated) = nominated {
        out.own_address = own_address(&locals, &nominated);
        if out.own_address.is_none() {
            out.ask_at = relay_to_ask(candidate(&locals, &nominated));
        }
    }

    out
}

fn entry_id(entry: &StatsEntry) -> &str {
    match entry {
        StatsEntry::LocalCandidate { id, .. }
        | StatsEntry::CandidatePair { id, .. }
        | StatsEntry::OutboundRtp { id, .. }
        | StatsEntry::RemoteInboundRtp { id, .. }
        | StatsEntry::InboundRtp { id, .. }
        | StatsEntry::Other { id } => id,
    }
}

/// A share's size and rate from one report (connection.ts, `readShare`).
///
/// The largest entry by area, because a share is published as a ladder and a
/// sender's report holds a row per rung in no promised order: taking whichever
/// came last once showed somebody sharing at 1440p as "360p". Ties keep the
/// first, so an entry with no size yet cannot displace one that has one; and a
/// report with no entry of the kind leaves what was found before alone.
fn read_share(report: &TrackReport, sending: bool, out: &mut Gathered) {
    let mut best: Option<GatheredShare> = None;
    let mut widest = -1.0;

    for entry in &report.entries {
        let (id, ssrc, track_identifier, width, height, fps, reason) = match entry {
            StatsEntry::OutboundRtp {
                id,
                ssrc,
                track_identifier,
                frame_width,
                frame_height,
                frames_per_second,
                quality_limitation_reason,
                ..
            } if sending => (
                id,
                ssrc,
                track_identifier,
                frame_width,
                frame_height,
                frames_per_second,
                quality_limitation_reason.as_deref(),
            ),
            StatsEntry::InboundRtp { id, ssrc, track_identifier, frame_width, frame_height, frames_per_second, .. }
                if !sending =>
            {
                (id, ssrc, track_identifier, frame_width, frame_height, frames_per_second, None)
            }
            _ => continue,
        };

        let width = numeric(*width);
        let height = numeric(*height);
        let area = width.unwrap_or(0.0) * height.unwrap_or(0.0);

        if area > widest {
            widest = area;
            best = Some(GatheredShare {
                width,
                height,
                fps: numeric(*fps),
                sending,
                // Only the sending side knows why its encoder holds back; a
                // receiver is being told what somebody else's machine decided.
                limited: if sending { limitation(reason) } else { None },
                // The web's `String(ssrc ?? trackIdentifier ?? id ?? "")`.
                id: match (ssrc, track_identifier) {
                    (Some(ssrc), _) => ssrc.to_string(),
                    (None, Some(track)) => track.clone(),
                    (None, None) => id.clone(),
                },
            });
        }
    }

    if let Some(best) = best {
        out.share = Some(best);
    }
}

fn limitation(reason: Option<&str>) -> Option<Limit> {
    match reason? {
        "cpu" => Some(Limit::Cpu),
        "bandwidth" => Some(Limit::Bandwidth),
        "other" => Some(Limit::Other),
        "none" => Some(Limit::Nothing),
        _ => None,
    }
}

/// This device's own public address, from the candidate in use (connection.ts,
/// `ownAddress`).
///
/// A reflexive candidate is the address the far end sees, which on a direct
/// path is this device's. A host candidate is a private address and answers
/// no question anybody had. Through a TURN relay the candidate in use is the
/// relay's own address, or a peer-reflexive one from the far end of the
/// relay's path; that was once shown as somebody's own address and read,
/// correctly, as a stranger's. What the relay sees is the related address of
/// the relay candidate, the address the allocation was made from.
///
/// `locals` is every local candidate by id, in the order the reports held them.
pub fn own_address(locals: &[(String, Candidate)], nominated: &str) -> Option<String> {
    let local = candidate(locals, nominated)?;
    if local.kind == "host" {
        return None;
    }
    if !local.relayed {
        return Some(local.address.clone());
    }

    let allocation = if local.kind == "relay" {
        Some(local)
    } else {
        // A peer-reflexive candidate on a relayed path names the relay
        // candidate it went out through as its related address.
        locals
            .iter()
            .map(|(_, one)| one)
            .find(|one| one.kind == "relay" && Some(&one.address) == local.related.as_ref())
    };

    disclosed(allocation?.related.as_deref())
}

fn candidate<'a>(locals: &'a [(String, Candidate)], id: &str) -> Option<&'a Candidate> {
    locals.iter().find(|(known, _)| known == id).map(|(_, one)| one)
}

/// An address the stack actually disclosed, rather than its placeholder.
fn disclosed(address: Option<&str>) -> Option<String> {
    match address {
        None | Some("") | Some("0.0.0.0") | Some("::") => None,
        Some(address) => Some(address.to_owned()),
    }
}

/// The STUN address of the TURN relay a relayed candidate went through, as a
/// `stun:` URL, or nothing (connection.ts, `relayToAsk`).
///
/// Only a plain `turn:` server: a relay answers STUN on its TURN port. One
/// reached over TLS (`turns:`) is left alone, because STUN to it in the clear
/// goes somewhere the call was told not to send anything in the clear.
pub fn relay_to_ask(candidate: Option<&Candidate>) -> Option<String> {
    let candidate = candidate?;
    if !candidate.relayed {
        return None;
    }

    let rest = candidate.url.as_deref()?.strip_prefix("turn:")?;
    let host = rest.split('?').next().unwrap_or_default();

    (!host.is_empty()).then(|| format!("stun:{host}"))
}

/// This device's public address as the STUN server `stun` sees it
/// (connection.ts, `addressSeenBy`).
///
/// For a call relayed by TURN under a relay-only policy, where WebRTC withholds
/// every related address so the candidates cannot say. The web builds a peer
/// connection to make the browser send one binding request to the relay; this
/// sends that binding request itself. Asked once per relay for the life of
/// the process, failure included, so a relay that will not answer is not asked
/// on every reading; callers asking at the same time share the one request.
pub async fn address_seen_by(stun: &str) -> Option<String> {
    type Answers = HashMap<String, Arc<OnceCell<Option<String>>>>;
    static ASKED: Mutex<Option<Answers>> = Mutex::new(None);

    let cell = {
        let mut asked = ASKED.lock().unwrap_or_else(|poison| poison.into_inner());
        asked.get_or_insert_with(HashMap::new).entry(stun.to_owned()).or_default().clone()
    };

    cell.get_or_init(|| async {
        let target = stun.strip_prefix("stun:")?;
        let seen = tokio::time::timeout(ASK_FOR, crate::relays::binding(target)).await.ok().flatten()?;
        Some(seen.ip().to_string())
    })
    .await
    .clone()
}

/// What the last sample left behind, so rates and loss can be a change over
/// the interval rather than a total since the call began.
#[derive(Debug, Clone, Copy)]
struct Previous {
    at_ms: f64,
    bytes_sent: f64,
    bytes_received: f64,
    packets_sent: f64,
    packets_lost: f64,
}

/// The last share figures, so a sample that lacks them does not empty the row.
#[derive(Debug, Clone)]
struct Held {
    id: String,
    sending: bool,
    width: Option<f64>,
    height: Option<f64>,
    fps: Option<f64>,
    limited: Option<Limit>,
    /// Samples in a row with nothing to say about the reason.
    missing: u32,
}

#[derive(Debug)]
struct State {
    previous: Option<Previous>,
    held: Option<Held>,
    published: ServerQuality,
}

/// The connection meter for one participant's calls (connection.ts,
/// `useConnectionQuality`).
#[derive(Debug, uniffi::Object)]
pub struct ConnectionMeter {
    state: Mutex<State>,
}

impl Default for ConnectionMeter {
    fn default() -> Self {
        ConnectionMeter {
            state: Mutex::new(State { previous: None, held: None, published: ServerQuality::Excellent }),
        }
    }
}

#[uniffi::export]
impl ConnectionMeter {
    #[uniffi::constructor]
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// The media server's verdict about this participant. Only this
    /// participant's own: somebody else's bad connection is their news, and
    /// putting it here would tell the wrong person to find a better network.
    pub fn heard(&self, quality: ServerQuality) {
        self.lock().published = quality;
    }

    /// A different room. Nothing about the last one carries over: a call
    /// joined straight from another once kept the other's warning until its
    /// own first sample said otherwise.
    ///
    /// The server's verdict is kept, as the web's hook keeps it across rooms;
    /// the new room's first verdict replaces it.
    pub fn reset(&self) {
        let mut state = self.lock();
        state.previous = None;
        state.held = None;
    }

    /// Takes one sample and says what the panel shows.
    ///
    /// `at_ms` is when the sample was taken, on any clock that only goes
    /// forward between samples; `holding` is the name the node that answered
    /// the join reported for itself, "" or `None` where it gave none.
    pub async fn sample(self: Arc<Self>, at_ms: u64, reports: Vec<TrackReport>, holding: Option<String>) -> ConnectionReading {
        crate::runtime::on_core(async move {
            let gathered = gather(&reports);

            let address = match (&gathered.own_address, &gathered.ask_at) {
                (Some(address), _) => Some(address.clone()),
                (None, Some(stun)) => address_seen_by(stun).await,
                (None, None) => None,
            };

            self.fold(at_ms, gathered, address, holding)
        })
        .await
    }
}

impl ConnectionMeter {
    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|poison| poison.into_inner())
    }

    /// Folds one gathered sample into the meter (the body of connection.ts,
    /// `look`, after the reports are read and the address is known).
    pub fn fold(&self, at_ms: u64, stats: Gathered, address: Option<String>, holding: Option<String>) -> ConnectionReading {
        let mut state = self.lock();
        let now = at_ms as f64;

        let was = state.previous.replace(Previous {
            at_ms: now,
            bytes_sent: stats.bytes_sent,
            bytes_received: stats.bytes_received,
            packets_sent: stats.packets_sent,
            packets_lost: stats.packets_lost,
        });

        // The first sample has nothing to compare against: it is the baseline,
        // and says only what it can say without one.
        let seconds = was.map_or(0.0, |was| (now - was.at_ms) / 1000.0);

        let rate = |now: f64, then: f64| (((now - then) * 8.0) / seconds / 1000.0).max(0.0);
        let up_kbps = was.filter(|_| seconds > 0.0).map(|was| rate(stats.bytes_sent, was.bytes_sent));
        let down_kbps = was.filter(|_| seconds > 0.0).map(|was| rate(stats.bytes_received, was.bytes_received));

        // Over the interval, not since the call began: a call that lost badly
        // for ten seconds an hour ago is fine now.
        let loss_percent = was.and_then(|was| {
            let sent = stats.packets_sent - was.packets_sent;
            let lost = stats.packets_lost - was.packets_lost;

            // Where the counters ran backwards by exactly what was sent the web
            // divides zero by zero and shows "NaN"; that is no reading at all.
            (sent > 0.0).then(|| (lost / (sent + lost) * 100.0).clamp(0.0, 100.0)).filter(|loss| !loss.is_nan())
        });

        // Carried forward only for the same track in the same direction: a new
        // share starts with a clean row, and a share being watched does not
        // inherit what this device's encoder said about the one it sent.
        let held = state.held.take();
        let same = match (&stats.share, &held) {
            (Some(share), Some(held)) => held.id == share.id && held.sending == share.sending,
            _ => false,
        };

        // Said: taken as said, "none" included, which clears it. Not said: the
        // last answer is kept for a couple of samples, because a report taken
        // mid-reconfiguration has none and a warning that blinks reads as a
        // fault. Not said for longer: gone, because a browser that stopped
        // reporting is not still reporting "cpu".
        let mut limited = stats.share.as_ref().and_then(|share| share.limited);
        let mut missing = 0;

        if stats.share.is_some() && limited.is_none() && same {
            let held = held.as_ref().expect("same implies held");
            missing = held.missing + 1;
            limited = if missing <= MISSING_FOR { held.limited } else { None };
        }

        let carried = |now: Option<f64>, then: fn(&Held) -> Option<f64>| {
            now.or_else(|| if same { held.as_ref().and_then(then) } else { None })
        };

        state.held = stats.share.as_ref().map(|share| Held {
            id: share.id.clone(),
            sending: share.sending,
            fps: carried(share.fps, |held| held.fps),
            width: carried(share.width, |held| held.width),
            height: carried(share.height, |held| held.height),
            limited,
            missing,
        });

        ConnectionReading {
            grade: connection_grade(state.published, stats.rtt_ms, loss_percent),
            rtt_ms: stats.rtt_ms,
            loss_percent,
            up_kbps,
            down_kbps,
            jitter_ms: stats.jitter_ms,
            share: state.held.as_ref().map(|held| ShareReading {
                width: held.width,
                height: held.height,
                fps: held.fps,
                sending: held.sending,
                // "none" is a fact for the merge and nothing for the screen.
                limited: match held.limited {
                    Some(Limit::Cpu) => Some(ShareLimitation::Cpu),
                    Some(Limit::Bandwidth) => Some(ShareLimitation::Bandwidth),
                    Some(Limit::Other) => Some(ShareLimitation::Other),
                    Some(Limit::Nothing) | None => None,
                },
            }),
            own_address: address,
            holding: holding.filter(|holding| !holding.is_empty()),
            measured: was.is_some(),
        }
    }
}

/// The reading to show before a room or its first sample.
#[uniffi::export]
pub fn connection_unmeasured() -> ConnectionReading {
    ConnectionReading::unmeasured()
}
