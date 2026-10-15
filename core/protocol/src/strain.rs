//! Telling somebody sharing their screen that their line cannot carry it.
//!
//! Follows web/src/live/strain.ts, rule for rule; the recorded readings its
//! tests replay are web/src/live/strain.recorded.ts, carried here as
//! tests/fixtures/strain/recorded.json.
//!
//! A hint and nothing more. A share asks for what its setting is worth and the
//! sender's own congestion control decides what goes out; when the line is
//! smaller than the ask, `MaintainResolution` keeps every frame sharp and drops
//! frames instead, and the share stutters for everybody watching while the
//! person sharing sees a perfectly smooth screen. Measured on a real call:
//! 2560x1440 at 120 asking 48.7 Mb/s on a home uplink carrying about 37, and
//! nobody at either end able to say why it juddered. So this reads the sender,
//! and when the evidence is unambiguous it shows the person sharing what the
//! line carries, what the share wants, and one setting that would fit.
//!
//! ## What this must never be
//!
//! It never applies anything. The owner banned automatic downgrade on
//! 2026-09-07 ("不允许自动降低码率") after an automatic follower halved a
//! share's ceiling whenever it judged the share to be struggling, took a still
//! screen for a struggling one, and remembered one bad afternoon as the ceiling
//! for every later share. Nothing here holds a sender, a track or a room, and
//! nothing here writes anything down: the setting changes only when the person
//! presses for it, through the same retune the menu uses (`accept` hands the
//! caller the setting to apply and does not apply it), and "Not now" lasts
//! exactly as long as the share it was said about.
//!
//! ## What the sender actually reports
//!
//! Measured in Chrome 145 with a share-shaped sender: the limitation reason
//! reads "none" throughout while the top layer sends 36 of 57 captured frames,
//! so waiting for it to say "bandwidth" would never fire for the case this is
//! for; it is used only to rule out ("cpu" or "other" means the line is not the
//! explanation). It reads "none" during the estimate's climb at the start of a
//! share too, which reads reading by reading exactly like a full line. And the
//! encoder overshoots its targets, so a target says the allotment is short,
//! not what fits.
//!
//! ## The rule
//!
//! Read every two seconds. A reading is strained when the top layer is being
//! sent at all; the reason is "none" or "bandwidth"; the capture is moving (ten
//! frames a second or more); the top layer sends at most three quarters of the
//! frames offered; it is allotted at most three quarters of its ceiling; and it
//! spends at least four fifths of what it is allotted. Nothing is read for the
//! first thirty seconds of a share or fifteen after any change, and what was
//! read before a change is thrown away. The share is said to be struggling when,
//! over the last twenty seconds, four readings in five were strained, the latest
//! among them; the line's estimate was flat rather than climbing; the line was
//! full; the encoder was carrying the setting the menu shows; and what the line
//! carries is less than the share asks. It is withdrawn the same way, by four
//! in five clear. A still screen neither raises it nor takes it down. Every
//! missing figure makes a reading say nothing, so a sender that reports less is
//! told less, never something else.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::share::{
    share_ask_of, share_ladder_at, share_rung_small, share_size_of, share_steps_down, ShareCapture, ShareRunning,
    ShareSetting,
};

/// How often the sender is read.
pub const STRAIN_EVERY_MS: f64 = 2000.0;

/// How long a new share is left alone: the line's estimate starts low and climbs.
pub const STRAIN_QUIET_AFTER_START_MS: f64 = 30_000.0;

/// How long a changed share is left alone, for the same reason on a smaller scale.
pub const STRAIN_QUIET_AFTER_CHANGE_MS: f64 = 15_000.0;

/// How much evidence it takes, in either direction.
pub const STRAIN_WINDOW_MS: f64 = 20_000.0;

/// The share of readings in the window that have to agree.
const MOST: f64 = 0.8;

/// A window with fewer readings than this has gaps in it (reads that failed, an
/// app the system stopped running), and four in five of not very many is not
/// twenty seconds of anything.
const AT_LEAST: usize = ((STRAIN_WINDOW_MS / STRAIN_EVERY_MS) * 0.75) as usize;

/// Frames a second entering the encoder below which the picture is still.
const MOVING: f64 = 10.0;

/// The top layer sends at most this share of the frames it was offered.
const DROPPING: f64 = 0.75;

/// The sender allots the top layer at most this share of its ceiling. A layer
/// allotted its whole ceiling that still drops frames has a ceiling too low for
/// the content, which a smaller setting would make worse.
const SHORT: f64 = 0.75;

/// The top layer sends at least this share of what it was allotted. A short
/// target not being spent is an estimate that never had a reason to climb.
const SPENDING: f64 = 0.8;

/// How much the estimate may rise across the window and still be a ceiling.
const CLIMBING: f64 = 1.15;

/// How much of the line's estimate has to be going out on it for the line to be
/// the limit: a full line carried 88 to 97% of its estimate, and an estimate held
/// down by a starved encoder sits near two thirds, where libwebrtc's rate
/// control stops it.
const FULL: f64 = 0.85;

/// How far above the menu's setting the encoder's ceiling may be and still be
/// that setting: rounding, nothing more.
const CARRYING: f64 = 1.02;

/// The share of the line a suggestion may use, leaving room for the camera and
/// the voice, which are not counted, and for the estimate being an estimate.
const HEADROOM: f64 = 0.8;

/// A suggestion has to cost at least this much less than now, as actually sent.
const CHEAPER: f64 = 0.95;

/// The share of its ceiling the sender allots the 1080p rung while the top
/// layer is on: 8.5 of 11.3 Mb/s at sixty and 6.0 of 8 at thirty (Chrome 145,
/// 2026-10-14).
const ALLOTTED: f64 = 0.75;

/* -------------------------------------------------------------------------- */
/* The sender's report                                                        */
/* -------------------------------------------------------------------------- */

/// One outbound RTP entry of the share's sender, as the stats report has it.
/// Field names are the W3C stats names; a figure not given is absent.
#[derive(Debug, Clone, PartialEq, Default, uniffi::Record)]
pub struct StrainOutbound {
    pub id: Option<String>,
    pub rid: Option<String>,
    /// "video" or "audio"; audio entries are passed over.
    pub kind: Option<String>,
    pub active: Option<bool>,
    /// Milliseconds, as the report stamps it.
    pub timestamp: Option<f64>,
    pub bytes_sent: Option<f64>,
    pub frames_sent: Option<f64>,
    pub frames_encoded: Option<f64>,
    pub frames_per_second: Option<f64>,
    pub frame_width: Option<f64>,
    pub frame_height: Option<f64>,
    pub target_bitrate: Option<f64>,
    pub quality_limitation_reason: Option<String>,
    pub media_source_id: Option<String>,
}

/// One media-source entry: the picture entering the encoder.
#[derive(Debug, Clone, PartialEq, Default, uniffi::Record)]
pub struct StrainMediaSource {
    pub id: Option<String>,
    pub kind: Option<String>,
    pub frames: Option<f64>,
    pub frames_per_second: Option<f64>,
    pub width: Option<f64>,
    pub height: Option<f64>,
}

/// One candidate-pair entry.
#[derive(Debug, Clone, PartialEq, Default, uniffi::Record)]
pub struct StrainCandidatePair {
    pub id: Option<String>,
    pub state: Option<String>,
    pub nominated: Option<bool>,
    pub available_outgoing_bitrate: Option<f64>,
    pub bytes_sent: Option<f64>,
}

/// One transport entry.
#[derive(Debug, Clone, PartialEq, Default, uniffi::Record)]
pub struct StrainTransport {
    pub id: Option<String>,
    pub selected_candidate_pair_id: Option<String>,
}

/// The entries of one stats report that the rule reads, each list in the
/// report's own order.
#[derive(Debug, Clone, PartialEq, Default, uniffi::Record)]
pub struct StrainReport {
    pub outbound: Vec<StrainOutbound>,
    pub media_sources: Vec<StrainMediaSource>,
    pub candidate_pairs: Vec<StrainCandidatePair>,
    pub transports: Vec<StrainTransport>,
}

/// One encoding of the share's sender, from its parameters.
///
/// Bottom first, as the browser's sender lists them and room.ts relies on. The
/// Rust SDK lists them top first; they are put in order here by rid (q, h, f)
/// where every one has one of those, so either order reads the same.
#[derive(Debug, Clone, PartialEq, Default, uniffi::Record)]
pub struct StrainEncoding {
    pub rid: Option<String>,
    pub active: Option<bool>,
    pub max_bitrate: Option<f64>,
}

/// A rung beneath the top layer: its ceiling and the bits a second it sent.
#[derive(Debug, Clone, Copy, PartialEq, uniffi::Record)]
pub struct StrainRung {
    pub ceiling: Option<f64>,
    pub spent: Option<f64>,
}

/// One reading of the share's sender, reduced to what the rule needs.
#[derive(Debug, Clone, PartialEq, Default, uniffi::Record)]
pub struct StrainSample {
    /// When, in milliseconds on the caller's clock.
    pub at: f64,
    /// Whether the top layer is being sent at all. dynacast pauses it when
    /// nobody watches at full size, and a paused layer sends nothing for
    /// reasons that have nothing to do with the line.
    pub active: bool,
    /// Frames a second entering the encoder.
    pub source: Option<f64>,
    /// Frames a second the top layer sent.
    pub sent: Option<f64>,
    /// Bits a second the top layer sent.
    pub spent: Option<f64>,
    /// Bits a second every other layer of the share sent.
    pub rest: Option<f64>,
    /// The layers beneath the top one by one, bottom first.
    pub rungs: Option<Vec<StrainRung>>,
    /// Bits a second that went out on the line, everything on it.
    pub carried: Option<f64>,
    /// What the sender allotted the top layer.
    pub target: Option<f64>,
    /// What the top layer may send at most.
    pub ceiling: Option<f64>,
    /// What every active layer may send, together: what the share asks.
    pub asked: Option<f64>,
    /// The sender's estimate of what the line carries.
    pub available: Option<f64>,
    /// Why the encoder says it is held back, where it says.
    pub reason: Option<String>,
    /// The picture entering the encoder.
    pub width: Option<u32>,
    pub height: Option<u32>,
}

/// What one reading leaves for the next, so rates are changes rather than totals.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct StrainCounters {
    pub at: f64,
    pub frames: Option<f64>,
    pub layers: HashMap<String, (Option<f64>, Option<f64>)>,
    /// The line's own count of bytes, by the pair it was counted on.
    pub line: Option<(String, Option<f64>)>,
}

/// The rid order the SDKs give encodings, bottom first.
fn rid_rank(rid: Option<&str>) -> Option<usize> {
    ["q", "h", "f"].iter().position(|one| Some(*one) == rid)
}

/// Reduce one stats report to a reading (strain.ts `readSample`).
///
/// Every figure is read only if the sender gave it. The top layer is found by
/// the rid of the last encoding; the picture entering the encoder by the
/// media-source entry, which Chrome links from only one of the three layers,
/// measured, which is why the link is not insisted on.
pub fn read_sample(
    report: &StrainReport,
    encodings: &[StrainEncoding],
    previous: Option<&StrainCounters>,
    now: f64,
) -> (StrainSample, StrainCounters) {
    let mut encodings = encodings.to_vec();
    if encodings.iter().all(|encoding| rid_rank(encoding.rid.as_deref()).is_some()) {
        encodings.sort_by_key(|encoding| rid_rank(encoding.rid.as_deref()));
    }

    let outbound: Vec<&StrainOutbound> =
        report.outbound.iter().filter(|entry| entry.kind.as_deref() != Some("audio")).collect();
    let top = encodings.last();
    let top_rid = top.and_then(|top| top.rid.clone());

    // By rid where there is one. A single encoding has none, and then there is
    // only one layer to be.
    let layer_index = top_rid
        .as_ref()
        .and_then(|rid| outbound.iter().position(|entry| entry.rid.as_ref() == Some(rid)))
        .or((outbound.len() == 1).then_some(0));
    let layer = layer_index.map(|index| outbound[index]);

    let linked =
        report.media_sources.iter().find(|entry| entry.id == layer.and_then(|layer| layer.media_source_id.clone()));
    let source = linked.or_else(|| report.media_sources.iter().find(|entry| entry.kind.as_deref() == Some("video")));

    // The pair the transport says it chose, and otherwise the one nominated.
    let transport = report.transports.iter().find(|entry| entry.selected_candidate_pair_id.is_some());
    let chosen = transport.and_then(|transport| transport.selected_candidate_pair_id.clone());
    let pair = report.candidate_pairs.iter().find(|entry| entry.id == chosen).or_else(|| {
        report
            .candidate_pairs
            .iter()
            .find(|entry| entry.nominated == Some(true) && entry.state.as_deref() == Some("succeeded"))
    });

    let stamp = layer.and_then(|layer| numeric(layer.timestamp)).unwrap_or(now);
    let seconds = previous.map(|previous| (stamp - previous.at) / 1000.0).unwrap_or(0.0);

    // A change over the gap, or nothing: a counter that went backwards belongs
    // to a stream that was replaced, and a gap of nothing divides by nothing.
    let rate = |value: Option<f64>, was: Option<f64>| -> Option<f64> {
        let (value, was) = (value?, was?);
        (seconds > 0.0 && value >= was).then(|| (value - was) / seconds)
    };

    let key = |entry: &StrainOutbound| entry.rid.clone().or(entry.id.clone()).unwrap_or_default();
    let mut layers: HashMap<String, (Option<f64>, Option<f64>)> = HashMap::new();
    for entry in &outbound {
        layers.insert(
            key(entry),
            (numeric(entry.frames_sent).or(numeric(entry.frames_encoded)), numeric(entry.bytes_sent)),
        );
    }

    // What the pair has sent, which counts every sender on the connection: in
    // Chrome 145 it ran ahead of the share's own layers by a second sender's
    // worth. Only against a reading of the same pair; a connection that moved
    // pairs is a different count.
    let line = pair.and_then(|pair| pair.id.clone().map(|id| (id, numeric(pair.bytes_sent))));
    let carried_bytes = match (&line, previous.and_then(|previous| previous.line.as_ref())) {
        (Some((pair, bytes)), Some((was_pair, was_bytes))) if pair == was_pair => rate(*bytes, *was_bytes),
        _ => None,
    };

    let counters = StrainCounters {
        at: stamp,
        frames: source.and_then(|source| numeric(source.frames)),
        layers: layers.clone(),
        line,
    };

    let before = |entry: &StrainOutbound| previous.and_then(|previous| previous.layers.get(&key(entry)).copied());
    let mine = layer.and_then(|layer| layers.get(&key(layer)).copied());
    let was = layer.and_then(before);

    // Every other layer's bits, or nothing if any of them cannot be said: a
    // partial sum would understate what the rest of the share takes and offer
    // a setting that does not fit.
    let mut rest = Some(0.0);
    for (index, entry) in outbound.iter().enumerate() {
        if Some(index) == layer_index {
            continue;
        }
        let bits = rate(layers.get(&key(entry)).and_then(|one| one.1), before(entry).and_then(|one| one.1));
        rest = match (rest, bits) {
            (Some(rest), Some(bits)) => Some(rest + bits * 8.0),
            _ => None,
        };
    }

    let spent_bytes = rate(mine.and_then(|one| one.1), was.and_then(|one| one.1));

    // By rid, as the top layer is found; a rung with no entry of its own sent
    // nothing anybody can count, so no figure rather than a zero that would read
    // as a rung known to be idle.
    let rungs: Vec<StrainRung> = encodings
        .iter()
        .take(encodings.len().saturating_sub(1))
        .map(|encoding| {
            let entry = encoding.rid.as_ref().and_then(|rid| outbound.iter().find(|one| one.rid.as_ref() == Some(rid)));
            let bytes = entry.and_then(|entry| {
                rate(layers.get(&key(entry)).and_then(|one| one.1), before(entry).and_then(|one| one.1))
            });
            StrainRung { ceiling: numeric(encoding.max_bitrate), spent: bytes.map(|bytes| bytes * 8.0) }
        })
        .collect();

    let asked = encodings
        .iter()
        .all(|encoding| encoding.active == Some(false) || numeric(encoding.max_bitrate).is_some())
        .then(|| {
            encodings
                .iter()
                .map(|encoding| if encoding.active == Some(false) { 0.0 } else { encoding.max_bitrate.unwrap_or(0.0) })
                .sum()
        });

    let sample = StrainSample {
        at: now,
        active: top.is_some_and(|top| top.active != Some(false))
            && layer.is_some_and(|layer| layer.active != Some(false)),
        source: rate(counters.frames, previous.and_then(|previous| previous.frames))
            .or(source.and_then(|source| numeric(source.frames_per_second))),
        sent: rate(mine.and_then(|one| one.0), was.and_then(|one| one.0))
            .or(layer.and_then(|layer| numeric(layer.frames_per_second))),
        spent: spent_bytes.map(|bytes| bytes * 8.0),
        rest,
        rungs: Some(rungs),
        carried: carried_bytes.map(|bytes| bytes * 8.0),
        target: layer.and_then(|layer| numeric(layer.target_bitrate)),
        ceiling: top.and_then(|top| numeric(top.max_bitrate)),
        asked,
        available: pair.and_then(|pair| numeric(pair.available_outgoing_bitrate)),
        reason: layer.and_then(|layer| layer.quality_limitation_reason.clone()),
        width: source
            .and_then(|source| numeric(source.width))
            .or(layer.and_then(|layer| numeric(layer.frame_width)))
            .map(|width| width as u32),
        height: source
            .and_then(|source| numeric(source.height))
            .or(layer.and_then(|layer| numeric(layer.frame_height)))
            .map(|height| height as u32),
    };

    (sample, counters)
}

/* -------------------------------------------------------------------------- */
/* Judging                                                                    */
/* -------------------------------------------------------------------------- */

/// What one reading says about the line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum StrainVerdict {
    Strained,
    Clear,
    Unknown,
}

/// What one reading says about the line (strain.ts `classify`).
pub fn classify(sample: &StrainSample) -> StrainVerdict {
    if !sample.active {
        return StrainVerdict::Unknown;
    }
    let (Some(source), Some(sent), Some(spent), Some(target), Some(ceiling), Some(_), Some(reason)) = (
        sample.source,
        sample.sent,
        sample.spent,
        sample.target,
        sample.ceiling,
        sample.available,
        sample.reason.as_deref(),
    ) else {
        return StrainVerdict::Unknown;
    };

    // The machine, or something the sender will not name. Neither is the line,
    // and a hint about the line would send somebody to the wrong place.
    if reason != "none" && reason != "bandwidth" {
        return StrainVerdict::Unknown;
    }
    if source < MOVING {
        return StrainVerdict::Unknown;
    }

    let dropping = sent <= DROPPING * source;
    let short = target <= SHORT * ceiling;
    let spending = spent >= SPENDING * target;

    if dropping && short && spending {
        StrainVerdict::Strained
    } else {
        StrainVerdict::Clear
    }
}

/// How much of the line's estimate went out on it: the line's own count where
/// the sender keeps one, otherwise the share's layers, which leave out the
/// camera and the voice and so can only make this quieter, never louder.
fn fullness(sample: &StrainSample) -> Option<f64> {
    let available = sample.available.filter(|available| *available > 0.0)?;
    let out = sample.carried.or(sample.spent.map(|spent| spent + sample.rest.unwrap_or(0.0)))?;
    Some(out / available)
}

/// The middle value of what is there, or nothing.
fn median(values: impl IntoIterator<Item = Option<f64>>) -> Option<f64> {
    let mut known: Vec<f64> = values.into_iter().flatten().collect();
    if known.is_empty() {
        return None;
    }
    known.sort_by(f64::total_cmp);
    let middle = known.len() / 2;
    Some(if known.len() % 2 == 1 { known[middle] } else { (known[middle - 1] + known[middle]) / 2.0 })
}

/// What is said to the person sharing.
#[derive(Debug, Clone, Copy, PartialEq, uniffi::Record)]
pub struct StrainHint {
    /// What the line carries, in bits a second.
    pub have: f64,
    /// What the share asks for, in bits a second.
    pub need: f64,
    /// The setting that would fit.
    pub to: ShareSetting,
}

/// What the readings so far say should be showing now.
#[derive(Debug, Clone, Copy, PartialEq, uniffi::Enum)]
pub enum StrainSaid {
    Show { hint: StrainHint },
    TakeDown,
    LeaveAsIs,
}

/// Of two settings that come to the same, the smaller picture by name: two sizes
/// only price the same when the capture is no larger than the smaller, so only
/// the label differs, and "Switch to 1440p" offered to somebody sharing a 1080p
/// display reads as asking for more.
fn area(setting: &ShareSetting) -> u64 {
    let size = share_size_of(setting.quality);
    size.width as u64 * size.height as u64
}

/// The setting to offer, or nothing (strain.ts `suggest`).
///
/// The one worth most whose ceiling fits in `budget` plus whatever `back` says
/// the rungs beneath would give back under it. Never larger and never faster
/// than what was chosen. Priced twice: at its ceiling against the capture's
/// size, which is what the retune will set and what has to fit; and at the
/// frames the capture actually offers, which is what it would change, because
/// sixty offered to a display that gives sixty under a setting of 120 changes
/// the label and nothing that is sent.
///
/// Nothing, rather than the smallest, when nothing fits: a card whose only
/// button is a dismissal is a warning nobody can act on, and its figures are
/// ceilings, so "even that will not fit" could easily be false.
pub fn suggest(
    from: ShareSetting,
    budget: f64,
    capture: ShareCapture,
    back: impl Fn(&ShareSetting) -> f64,
) -> Option<ShareSetting> {
    let now = share_ask_of(from, capture) as f64;
    let sized = ShareCapture { width: capture.width, height: capture.height, frame_rate: None };
    let mut best: Option<(ShareSetting, f64)> = None;

    for step in share_steps_down(from) {
        if share_ask_of(step, sized) as f64 > budget + back(&step) {
            continue;
        }
        let worth = share_ask_of(step, capture) as f64;
        if worth >= now * CHEAPER {
            continue;
        }
        let better = match best {
            None => true,
            Some((chosen, chosen_worth)) => {
                worth > chosen_worth || (worth == chosen_worth && area(&step) < area(&chosen))
            }
        };
        if better {
            best = Some((step, worth));
        }
    }

    best.map(|(step, _)| step)
}

/// What the rungs beneath the top would give back to it under `step`, by one
/// reading, or nothing where the reading cannot say (strain.ts `givenBack`).
///
/// A rung gives back only where `step` lowers its ceiling (the 1080p rung is
/// 11.3 Mb/s at sixty frames and 8 below), and then only what it was sending
/// above what the sender would allot it under the lower one. Measured rather
/// than priced: a rung on calm content sending less than that allotment goes on
/// sending the same. Without this, a 1440p share at sixty on a 30 Mb/s line was
/// offered 1080p at fifteen when 1080p at thirty fitted (2026-10-14).
fn given_back(sample: &StrainSample, step: &ShareSetting) -> Option<f64> {
    let rungs = sample.rungs.as_ref()?;
    let ladder = share_ladder_at(step.frame_rate as f64);
    let small = share_rung_small();

    let mut back = 0.0;
    for (index, rung) in rungs.iter().enumerate() {
        let then = ladder.get(index).copied().unwrap_or(small).max_bitrate as f64;
        let (Some(ceiling), Some(spent)) = (rung.ceiling, rung.spent) else { continue };
        if then >= ceiling {
            continue;
        }
        back += (spent - ALLOTTED * then).max(0.0);
    }
    Some(back)
}

/// Decide, from the readings so far, what should be showing now (strain.ts
/// `judge`).
///
/// `settled` is the moment readings start to count: thirty seconds after the
/// share began or fifteen after it last changed, whichever is later.
pub fn judge(samples: &[StrainSample], settled: f64, setting: Option<ShareSetting>, showing: bool) -> StrainSaid {
    let counted: Vec<&StrainSample> = samples.iter().filter(|sample| sample.at >= settled).collect();
    let (Some(first), Some(latest)) = (counted.first(), counted.last()) else { return StrainSaid::LeaveAsIs };

    // Not until the readings reach back a whole window, all of it after the
    // quiet time.
    if latest.at - first.at < STRAIN_WINDOW_MS - STRAIN_EVERY_MS / 2.0 {
        return StrainSaid::LeaveAsIs;
    }

    let window: Vec<&StrainSample> =
        counted.iter().copied().filter(|sample| sample.at > latest.at - STRAIN_WINDOW_MS).collect();
    if window.len() < AT_LEAST {
        return StrainSaid::LeaveAsIs;
    }

    let said: Vec<StrainVerdict> = window.iter().map(|sample| classify(sample)).collect();
    let strained = said.iter().filter(|one| **one == StrainVerdict::Strained).count() as f64;
    let clear = said.iter().filter(|one| **one == StrainVerdict::Clear).count() as f64;
    let now = said[said.len() - 1];
    let length = window.len() as f64;

    if showing {
        return if now == StrainVerdict::Clear && clear >= MOST * length {
            StrainSaid::TakeDown
        } else {
            StrainSaid::LeaveAsIs
        };
    }

    if now != StrainVerdict::Strained || strained < MOST * length {
        return StrainSaid::LeaveAsIs;
    }

    // Flat rather than climbing. The estimate's climb at the start of a share
    // looks exactly like a full line reading by reading; the difference is only
    // visible across time.
    let third = (window.len() / 3).max(1);
    let early = median(window[..third].iter().map(|sample| sample.available));
    let late = median(window[window.len() - third..].iter().map(|sample| sample.available));
    let (Some(early), Some(late)) = (early, late) else { return StrainSaid::LeaveAsIs };
    if late > early * CLIMBING {
        return StrainSaid::LeaveAsIs;
    }

    // And full rather than merely flat: an estimate also stops climbing when
    // nothing is testing it. libwebrtc holds it to about one and a half times
    // what gets through, so a starved encoder holds it down with it, and a stall
    // of that kind replayed on a 60 Mb/s line passed every other test here and
    // offered 1080p at fifteen.
    match median(window.iter().map(|sample| fullness(sample))) {
        Some(full) if full >= FULL => {}
        _ => return StrainSaid::LeaveAsIs,
    }

    let Some(setting) = setting else { return StrainSaid::LeaveAsIs };

    // The encoder carrying something other than the setting it is supposed to
    // (a publication that came back with its first options, a retune that did
    // not take) makes every figure here about one setting and every word on the
    // card about another. Only above it counts: what settle and retune write is
    // the setting's price or less, never more.
    let ask =
        share_ask_of(setting, ShareCapture { width: latest.width, height: latest.height, frame_rate: None }) as f64;
    match latest.ceiling {
        Some(ceiling) if ceiling <= CARRYING * ask => {}
        _ => return StrainSaid::LeaveAsIs,
    }

    let have = median(window.iter().map(|sample| sample.available));
    let need = latest.asked;
    // What the rungs take now, under the setting being sent.
    let rest = median(window.iter().map(|sample| sample.rest));

    // What would make the sentence false: a line that carries what the share
    // asks is not the reason it stutters.
    let (Some(have), Some(need), Some(rest)) = (have, need, rest) else { return StrainSaid::LeaveAsIs };
    if have >= need {
        return StrainSaid::LeaveAsIs;
    }

    let back = |step: &ShareSetting| median(window.iter().map(|sample| given_back(sample, step))).unwrap_or(0.0);
    let capture = ShareCapture {
        width: latest.width,
        height: latest.height,
        frame_rate: median(window.iter().map(|sample| sample.source)),
    };

    match suggest(setting, HEADROOM * have - rest, capture, back) {
        Some(to) => StrainSaid::Show { hint: StrainHint { have, need, to } },
        None => StrainSaid::LeaveAsIs,
    }
}

/// A figure in megabits the way the card says it: whole above ten, one decimal
/// below (strain.ts `megabits`, after JavaScript's `Math.round` and `toFixed`,
/// both of which take a half upward).
#[uniffi::export]
pub fn strain_megabits(bits: f64) -> String {
    let mega = bits / 1_000_000.0;
    if mega >= 10.0 {
        return format!("{}", mega.round());
    }

    // An exact half is the one place Rust's formatting (to even) and toFixed
    // (upward) part, and it is exact only for quarters: 2.25 is "2.3".
    let tenths = mega * 10.0;
    if tenths.fract() == 0.5 && (mega * 4.0).fract() == 0.0 {
        let up = tenths.ceil() as i64;
        return format!("{}{}.{}", if up < 0 { "-" } else { "" }, up.abs() / 10, up.abs() % 10);
    }
    format!("{mega:.1}")
}

/* -------------------------------------------------------------------------- */
/* The watch over one share                                                   */
/* -------------------------------------------------------------------------- */

/// What the card should do now.
#[derive(Debug, Clone, Copy, PartialEq, uniffi::Enum)]
pub enum StrainUpdate {
    Show { hint: StrainHint },
    Hide,
    Keep,
}

/// What the start of a reading found.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct StrainBegin {
    /// Take the card down now: the share, its connection or its setting is not
    /// the one the card was about.
    pub hide: bool,
    /// Read the sender and pass the report to `reading`. False once the
    /// question has been put away for this share.
    pub read: bool,
}

/// What pressing the card's button comes to.
#[derive(Debug, Clone, Copy, PartialEq, uniffi::Enum)]
pub enum StrainAccept {
    /// Apply this through the same retune the menu uses, then say how it went
    /// with `accepted`.
    Apply { setting: ShareSetting },
    /// The share moved since the card was worked out (the menu, in the moment
    /// before the card went down): the press is dropped rather than applied to
    /// a share it is not about, and the card goes.
    Dropped,
    /// Nothing to press: no card, or a press already under way.
    Ignored,
}

/// This client's own share, watched for a line that cannot carry it (strain.ts
/// `useShareStrain`).
///
/// Only ever about the share this client is sending, so only the person
/// sharing is ever told. Kept per share by `ShareRunning::id`, not by track sid:
/// a full reconnect publishes the same capture again under a new sid in the
/// middle of one share, and a watch keyed on the sid took that for a new share
/// and forgot "Not now" (replayed on the web, the card was back twenty-two
/// seconds after it). The new connection is owed the quiet time a new share is,
/// because its estimate climbs from nothing again.
///
/// Holds no sender and no room, so it cannot change the share; see the module's
/// header for why that is the point.
#[derive(Debug, Default, uniffi::Object)]
pub struct ShareStrainWatch {
    state: Mutex<WatchState>,
}

#[derive(Debug, Default)]
struct WatchState {
    current: Option<Watch>,
    /// What the card shows, if anything.
    hint: Option<StrainHint>,
    busy: bool,
    /// The share as `begin` found it, for the reading that follows.
    reading_for: Option<ShareRunning>,
}

#[derive(Debug)]
struct Watch {
    /// Which share: `ShareRunning::id`. Everything below belongs to it alone.
    share: u64,
    /// Which publication of it. A full reconnect publishes one share again.
    sid: String,
    /// Which setting of it: `ShareRunning::since`.
    since: f64,
    /// When the connection carrying it began estimating the line from nothing.
    fresh: f64,
    samples: Vec<StrainSample>,
    counters: Option<StrainCounters>,
    /// "Not now", for the rest of this share.
    dismissed: bool,
    showing: bool,
}

impl Watch {
    /// What was measured was measured against something no longer being sent,
    /// and whatever was said about it no longer applies.
    fn reset(&mut self) -> bool {
        self.samples.clear();
        self.counters = None;
        std::mem::replace(&mut self.showing, false)
    }
}

#[uniffi::export]
impl ShareStrainWatch {
    #[uniffi::constructor]
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// The first half of a reading, every two seconds while sharing: which share
    /// this is, and whether to read its sender at all. `sid` is the share's
    /// publication as published now; `running` is what `ShareRun` has on record.
    pub fn begin(&self, running: Option<ShareRunning>, sid: Option<String>, now: f64) -> StrainBegin {
        let mut state = self.state.lock().unwrap();
        state.reading_for = None;

        let (Some(running), Some(sid)) = (running, sid) else { return StrainBegin { hide: false, read: false } };

        let mut hide = false;

        // A different share from the last reading's: nothing said about that
        // one is about this one.
        if state.current.as_ref().map(|current| current.share) != Some(running.id) {
            state.current = Some(Watch {
                share: running.id,
                sid: sid.clone(),
                since: running.since,
                fresh: running.started,
                samples: Vec::new(),
                counters: None,
                dismissed: false,
                showing: false,
            });
            hide = state.hint.take().is_some();
        }

        let current = state.current.as_mut().expect("made above");

        // The same share published again, on a new connection: owed the quiet
        // time a new share is owed, and still the same share.
        if current.sid != sid {
            current.sid = sid;
            current.fresh = now;
            hide |= current.reset();
        }

        // A change of setting, from the menu or from the card itself.
        if current.since != running.since {
            current.since = running.since;
            hide |= current.reset();
        }

        let read = !current.dismissed;
        if hide {
            state.hint = None;
        }
        if read {
            state.reading_for = Some(running);
        }
        StrainBegin { hide, read }
    }

    /// The second half: the sender's report and its encodings as they are now
    /// (bottom first, or with rids), read after `begin` said to. A read that
    /// failed is a gap, which the window allows for: skip this call.
    pub fn reading(&self, report: StrainReport, encodings: Vec<StrainEncoding>, at: f64) -> StrainUpdate {
        let mut state = self.state.lock().unwrap();
        let Some(running) = state.reading_for.take() else { return StrainUpdate::Keep };
        let Some(current) = state.current.as_mut() else { return StrainUpdate::Keep };
        if current.share != running.id || current.dismissed {
            return StrainUpdate::Keep;
        }

        let (sample, counters) = read_sample(&report, &encodings, current.counters.as_ref(), at);
        current.counters = Some(counters);
        current.samples.retain(|one| one.at > at - 2.0 * STRAIN_WINDOW_MS);
        current.samples.push(sample);

        let settled = (current.fresh + STRAIN_QUIET_AFTER_START_MS).max(running.since + STRAIN_QUIET_AFTER_CHANGE_MS);
        match judge(&current.samples, settled, Some(running.setting()), current.showing) {
            StrainSaid::TakeDown => {
                current.showing = false;
                state.hint = None;
                StrainUpdate::Hide
            }
            StrainSaid::Show { hint } => {
                current.showing = true;
                state.hint = Some(hint);
                StrainUpdate::Show { hint }
            }
            StrainSaid::LeaveAsIs => StrainUpdate::Keep,
        }
    }

    /// The share's record changed (a start, a change from the menu or from the
    /// card, or an end). The card goes the moment the share changes rather than
    /// at the next reading: two seconds is long enough to press a button in, and
    /// a press then applied what had been worked out for the old setting, 1440p
    /// at thirty pressed just after somebody chose 1080p at fifteen, a change
    /// upward nobody asked for. Returns whether to take the card down.
    pub fn running_changed(&self, running: Option<ShareRunning>) -> bool {
        let mut state = self.state.lock().unwrap();
        let Some(current) = state.current.as_mut() else { return false };
        if running.is_some_and(|running| running.id == current.share && running.since == current.since) {
            return false;
        }

        let hide = current.reset();
        if hide {
            state.hint = None;
        }
        hide
    }

    /// The share's picture is gone from the room: stopped, or a reconnect half
    /// way through. The card goes; the watch stays, because what was said about
    /// the share ("Not now") still stands when a reconnect brings it back.
    pub fn sharing_stopped(&self) {
        let mut state = self.state.lock().unwrap();
        if let Some(current) = state.current.as_mut() {
            current.reset();
        }
        state.hint = None;
        state.reading_for = None;
    }

    /// The button: the setting to apply, if the press still applies.
    pub fn accept(&self, running: Option<ShareRunning>) -> StrainAccept {
        let mut state = self.state.lock().unwrap();
        let Some(hint) = state.hint else { return StrainAccept::Ignored };
        if state.busy || !state.current.as_ref().is_some_and(|current| current.showing) {
            return StrainAccept::Ignored;
        }

        let current = state.current.as_mut().expect("checked above");
        if running.is_none_or(|running| running.id != current.share || running.since != current.since) {
            current.showing = false;
            state.hint = None;
            return StrainAccept::Dropped;
        }

        state.busy = true;
        StrainAccept::Apply { setting: hint.to }
    }

    /// How the change `accept` asked for went. Taken, the card goes at once
    /// rather than at the next reading, because a button that is pressed and
    /// then sits there for two seconds reads as one that did not work. Refused,
    /// it stays beside the notice that says so, so the same press is there to
    /// try again. Returns whether to take the card down.
    pub fn accepted(&self, took: bool) -> bool {
        let mut state = self.state.lock().unwrap();
        state.busy = false;
        if !took {
            return false;
        }
        if let Some(current) = state.current.as_mut() {
            current.showing = false;
        }
        state.hint = None;
        true
    }

    /// "Not now": the question put away until this share ends.
    pub fn decline(&self) {
        let mut state = self.state.lock().unwrap();
        if let Some(current) = state.current.as_mut() {
            current.dismissed = true;
            current.showing = false;
        }
        state.hint = None;
    }

    /// What the card shows, if anything.
    pub fn hint(&self) -> Option<StrainHint> {
        self.state.lock().unwrap().hint
    }
}

fn numeric(value: Option<f64>) -> Option<f64> {
    value.filter(|value| value.is_finite())
}
