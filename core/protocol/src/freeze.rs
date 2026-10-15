//! A shared screen that has stopped moving for one viewer, asked for a rung
//! lower until it moves again.
//!
//! Follows web/src/live/freeze.ts, rule for rule; its header carries the
//! measurements behind every number, and what follows is the part a reader of
//! this file needs.
//!
//! ## The fault
//!
//! The server chooses which rung of a share each viewer is sent, from its own
//! estimate of that viewer's line, and it can get stuck: measured with the
//! relays' settings, it judged a line congested three seconds into a share and
//! then let the original grow to 48 Mb/s on a 30 Mb/s line without choosing
//! again. The viewer received 20 Mb/s for a minute and decoded none of it.
//! Nothing says so; the picture stops, the sound goes on, and a frozen share
//! looks exactly like a still one.
//!
//! ## What this does about it
//!
//! Fed this viewer's receiver once a second, it asks for the next rung down
//! when the share keeps arriving but nothing new is decoded for two seconds, or
//! when a tenth of what was sent is lost while the receiver counts the picture
//! freezing; holds that for twenty seconds; and then asks for everything again.
//! A freeze soon after asking for the original again doubles the next hold, up
//! to five minutes, because each return restarts a layer dynacast had stopped
//! and on a line that cannot carry it the asking itself made a freeze every
//! forty seconds to a minute. Two minutes on the original without one, and the
//! hold is twenty seconds again.
//!
//! It never touches what anybody sends, and never what anybody else receives:
//! the only thing it produces is the size this viewer's own subscription asks
//! for. The owner chose that on 2026-10-13 knowing it is an automatic step down,
//! because it is one viewer's and nobody else's.
//!
//! ## How the media crate drives it
//!
//! One `ShareFreezeWatch` per remote screen-share publication object. A full
//! reconnection replaces every publication with a new object under the same
//! track sid, and a watch kept by sid went on reading the new receiver while
//! capping the dead publication (found in review on the web, 2026-10-14), so a
//! publication that is not the one the watch was made for gets a new watch.
//! Once a second, `tick` with a reading of the receiver, or with none while the
//! share is not subscribed, has no receiver, or the app is in the background
//! (readings there are not evidence of anything, and the clock still lifts a
//! cap whose time is up); and `subscribed` the moment its track arrives again.
//! Whatever either returns is sent as the subscription's requested size.

use std::sync::{Arc, Mutex};

/// How often the receiver is read.
pub const FREEZE_EVERY_MS: f64 = 1000.0;

/// How long frames have to stand still while bytes arrive before it is a freeze.
pub const FREEZE_STALL_MS: f64 = 2000.0;

/// How long the lower rung is held before the original is asked for again, the
/// first time.
pub const FREEZE_HOLD_MS: f64 = 20_000.0;

/// The longest a lower rung is ever held, however often the original failed.
pub const FREEZE_LONGEST_HOLD_MS: f64 = 300_000.0;

/// How long after the original is asked for again that a freeze still says it
/// could not be carried, and how long without one before the holds start again
/// from twenty seconds. One figure for both, so that there is no third state
/// between failing and forgiven to reason about.
pub const FREEZE_TRIED_MS: f64 = 120_000.0;

/// How long after this changes what is asked for before anything is judged
/// again: the server switches at the next keyframe of the rung asked for, and
/// until then the picture may stand still for reasons this caused.
pub const FREEZE_SETTLE_MS: f64 = 3000.0;

/// The share of packets lost over the stall window that counts as a freeze,
/// while the receiver also counts the picture freezing in the same window.
///
/// Not on its own: the receiver's `packetsLost` goes on counting packets a
/// retransmission brought back. Measured on a line dropping 3% at random it read
/// 2.2 to 3.7% in every window while the picture froze for a third of a second
/// in a minute, and a crowded 2.4 GHz Wi-Fi losing twelve per cent and
/// recovering all of it would have spent most of its time on the 720p rung, with
/// a keyframe at every switch. Nor with retransmissions subtracted: the server
/// probes for room by sending earlier packets again on the retransmission
/// stream, and subtracted they would hide real loss exactly while it decides.
/// So the evidence of harm is the receiver's own `freezeCount`; one that does
/// not report it is judged on the stall alone.
pub const FREEZE_LOSSY: f64 = 0.1;

/// Bytes a reading has to bring for the share to count as still arriving.
/// Small: a paused or still share brings nothing at all and a frozen one brings
/// megabits. The server's probing padding is not in `bytesReceived`.
const ARRIVING: f64 = 4000.0;

/// Packets a loss figure has to be taken over before it means anything.
const COUNTED: f64 = 100.0;

/// How far short of a request a rung may be and still serve it: the server's
/// own `layerSelectionTolerance` (livekit-server mediatrackreceiver.go).
const TOLERANCE: f64 = 0.9;

/// What is asked for to lift a cap: larger than any picture covers, so that the
/// smaller of it and the picture's own measurement is always the measurement
/// (freeze.ts `UNCAPPED`).
pub const FREEZE_UNCAPPED: FreezeRung = FreezeRung { width: 16384, height: 16384 };

/// A rung of a share, as its publication describes it; also a size asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct FreezeRung {
    pub width: u32,
    pub height: u32,
}

/// One reading of this viewer's receiver, reduced to what the rule needs.
/// Counters are totals so far; a figure the receiver did not give is absent.
#[derive(Debug, Clone, Copy, PartialEq, Default, uniffi::Record)]
pub struct FreezeReading {
    /// When, in milliseconds on the caller's monotonic clock.
    pub at: f64,
    /// Payload bytes received (`bytesReceived`).
    pub bytes: Option<f64>,
    /// Frames decoded (`framesDecoded`).
    pub frames: Option<f64>,
    /// Packets received (`packetsReceived`).
    pub received: Option<f64>,
    /// Packets lost, counting those a retransmission brought back (`packetsLost`).
    pub lost: Option<f64>,
    /// Freezes the receiver has counted in the picture (`freezeCount`).
    pub freezes: Option<f64>,
}

/// One inbound RTP entry of a receiver's stats, as the media crate reads it.
#[derive(Debug, Clone, PartialEq, Default, uniffi::Record)]
pub struct FreezeInbound {
    /// "video" or "audio".
    pub kind: String,
    pub bytes_received: Option<f64>,
    pub frames_decoded: Option<f64>,
    pub packets_received: Option<f64>,
    pub packets_lost: Option<f64>,
    pub freeze_count: Option<f64>,
}

/// Reduce a receiver's inbound entries to a reading (freeze.ts `readInbound`).
///
/// One inbound video stream per receiver; the largest by bytes should there
/// ever be more, because it is the one being drawn. A figure that is not a
/// finite number is no figure.
#[uniffi::export]
pub fn freeze_read_inbound(entries: Vec<FreezeInbound>, at: f64) -> FreezeReading {
    let mut inbound: Option<&FreezeInbound> = None;
    for entry in entries.iter().filter(|entry| entry.kind == "video") {
        let bytes = numeric(entry.bytes_received).unwrap_or(0.0);
        if inbound.is_none_or(|was| bytes > numeric(was.bytes_received).unwrap_or(0.0)) {
            inbound = Some(entry);
        }
    }

    FreezeReading {
        at,
        bytes: inbound.and_then(|one| numeric(one.bytes_received)),
        frames: inbound.and_then(|one| numeric(one.frames_decoded)),
        received: inbound.and_then(|one| numeric(one.packets_received)),
        lost: inbound.and_then(|one| numeric(one.packets_lost)),
        freezes: inbound.and_then(|one| numeric(one.freeze_count)),
    }
}

/// Packets and freezes over one reading.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FreezeRecent {
    pub at: f64,
    pub received: f64,
    pub lost: f64,
    pub froze: f64,
}

/// The rung asked for instead of the measurement, and until when.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FreezeCap {
    pub rung: FreezeRung,
    pub until: f64,
}

/// What is known about one share between readings (freeze.ts `Watch`).
#[derive(Debug, Clone, PartialEq)]
pub struct FreezeWatch {
    pub last: Option<FreezeReading>,
    /// Since when frames have stood still while bytes kept arriving.
    pub stalled: Option<f64>,
    /// Packets and freezes over the last stall window, reading by reading.
    pub recent: Vec<FreezeRecent>,
    pub cap: Option<FreezeCap>,
    /// Nothing is judged before this.
    pub quiet: f64,
    /// How long the next cap is held: twenty seconds, doubled for every failed
    /// return.
    pub hold: f64,
    /// When the original was last asked for again, while that is still being
    /// tried. Never set under a cap: taking one clears it, so a second step under
    /// the same cap is not taken for a second failed return.
    pub returned: Option<f64>,
}

/// What a reading led to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum FreezeAct {
    Cap { rung: FreezeRung },
    Release,
}

/// A watch with no evidence in it, keeping from `from` what is not evidence
/// about the picture but about the line: the cap, and how the original fared
/// when it was asked for again (freeze.ts `fresh`).
///
/// Every path that throws readings away goes through here, so none of them
/// forgets the back-off; one that did would put a viewer whose share went to
/// its card and back on twenty seconds again.
pub fn fresh(now: f64, from: Option<&FreezeWatch>) -> FreezeWatch {
    FreezeWatch {
        last: None,
        stalled: None,
        recent: Vec::new(),
        cap: from.and_then(|watch| watch.cap),
        quiet: now,
        hold: from.map(|watch| watch.hold).unwrap_or(FREEZE_HOLD_MS),
        returned: from.and_then(|watch| watch.returned),
    }
}

/// The rungs of a share, smallest first, one per height, from the layers its
/// publication lists (freeze.ts `rungsOf`).
///
/// The sizes the share was published at, not the height of the frames arriving:
/// a share published at 1440p and set to 1080p afterwards sends its top at 1080
/// lines, as tall as its 1080p rung, and read by its frames a viewer frozen on
/// that top was stepped past the very rung built for them.
#[uniffi::export]
pub fn freeze_rungs_of(layers: Vec<FreezeRung>) -> Vec<FreezeRung> {
    let mut rungs: Vec<FreezeRung> = Vec::new();
    for layer in layers {
        if layer.width == 0 || layer.height == 0 || rungs.iter().any(|rung| rung.height == layer.height) {
            continue;
        }
        rungs.push(layer);
    }
    rungs.sort_by_key(|rung| rung.height);
    rungs
}

/// Which rung a request for `asking` is answered with, by the server's rule:
/// the smallest rung at least nine tenths as tall as asked, or the top
/// (freeze.ts `rungAsked`, after mediatrackreceiver.go). None where nothing is
/// known, which callers read as the top.
///
/// `portrait` compares the request's width instead, as freeze.ts does.
#[uniffi::export]
pub fn freeze_rung_asked(rungs: Vec<FreezeRung>, asking: Option<FreezeRung>, portrait: bool) -> Option<u32> {
    rung_asked(&rungs, asking, portrait).map(|index| index as u32)
}

pub fn rung_asked(rungs: &[FreezeRung], asking: Option<FreezeRung>, portrait: bool) -> Option<usize> {
    let asking = asking?;
    if rungs.is_empty() {
        return None;
    }

    let size = (if portrait { asking.width } else { asking.height }) as f64 * TOLERANCE;
    Some(rungs.iter().position(|rung| rung.height as f64 >= size).unwrap_or(rungs.len() - 1))
}

/// The size a subscription asks the server for, given what this file asked for
/// and what the picture measures (the browser SDK's
/// `RemoteTrackPublication.emitTrackUpdate`): the smaller of the two by area,
/// or whichever there is.
///
/// This is how a cap lowers a request and a lift hands it back to the
/// measurement. Where the app measures nothing, the request is what this file
/// asked for, and a lift is the largest size there is: the original.
#[uniffi::export]
pub fn freeze_subscription_size(requested: Option<FreezeRung>, measured: Option<FreezeRung>) -> Option<FreezeRung> {
    let area = |size: &FreezeRung| size.width as u64 * size.height as u64;
    match (requested, measured) {
        (Some(requested), Some(measured)) => {
            Some(if area(&measured) < area(&requested) { measured } else { requested })
        }
        (requested, measured) => measured.or(requested),
    }
}

/// Decide, from one more reading, whether to ask for something else
/// (freeze.ts `judge`).
///
/// `reading` is None when there is nothing to read; then only the clock is
/// looked at, so a cap that has run its time is lifted whether or not anybody is
/// watching and coming back to the share finds the original asked for. `asked`
/// is the index of the rung this viewer's subscription asks for where that is
/// known (see `rung_asked`); unknown is the top.
pub fn judge(
    given: &FreezeWatch,
    reading: Option<&FreezeReading>,
    rungs: &[FreezeRung],
    now: f64,
    asked: Option<usize>,
) -> (FreezeWatch, Option<FreezeAct>) {
    // Long enough on the original without a freeze, and what went wrong before
    // is forgiven.
    let mut watch = given.clone();
    if watch.returned.is_some_and(|returned| now - returned >= FREEZE_TRIED_MS) {
        watch.hold = FREEZE_HOLD_MS;
        watch.returned = None;
    }

    if watch.cap.is_some_and(|cap| now >= cap.until) {
        let lifted =
            FreezeWatch { cap: None, hold: watch.hold, returned: Some(now), ..fresh(now + FREEZE_SETTLE_MS, None) };
        return (FreezeWatch { last: reading.copied(), ..lifted }, Some(FreezeAct::Release));
    }

    let Some(reading) = reading else {
        return (fresh(watch.quiet, Some(&watch)), None);
    };

    let last = watch.last;

    // A counter that went backwards is a stream that was replaced: a
    // resubscription, a new receiver. What was known about the old one is not
    // about this one.
    if let Some(last) = last {
        if backwards(reading.frames, last.frames) || backwards(reading.bytes, last.bytes) {
            return (FreezeWatch { last: Some(*reading), ..fresh(watch.quiet, Some(&watch)) }, None);
        }
    }

    // Nothing gathered while it is quiet either: what happens then may be the
    // switch this asked for, and a stall that began inside it would otherwise be
    // judged the moment the quiet ended.
    if now < watch.quiet {
        return (FreezeWatch { last: Some(*reading), stalled: None, recent: Vec::new(), ..watch }, None);
    }

    let moved = gain(reading.frames, last.and_then(|last| last.frames)) > 0.0;
    let arriving = gain(reading.bytes, last.and_then(|last| last.bytes)) >= ARRIVING;

    // From the last reading, or from the end of the quiet if that is later: a
    // stall that began inside it is counted from where it ended.
    let stalled = match last {
        Some(last) if !moved && arriving && reading.frames.is_some() => {
            Some(watch.stalled.unwrap_or(last.at.max(watch.quiet)))
        }
        _ => None,
    };

    let mut recent = watch.recent.clone();
    if let Some(last) = last {
        if reading.received.is_some() && last.received.is_some() && reading.lost.is_some() && last.lost.is_some() {
            recent.push(FreezeRecent {
                at: now,
                received: gain(reading.received, last.received),
                lost: gain(reading.lost, last.lost),
                froze: gain(reading.freezes, last.freezes),
            });
        }
    }
    recent.retain(|one| one.at > now - FREEZE_STALL_MS);

    let next = FreezeWatch { last: Some(*reading), stalled, recent: recent.clone(), ..watch.clone() };

    let frozen = stalled.is_some_and(|since| now - since >= FREEZE_STALL_MS);

    // Loss counts only where the picture shows it; see FREEZE_LOSSY.
    let lost: f64 = recent.iter().map(|one| one.lost).sum();
    let counted = lost + recent.iter().map(|one| one.received).sum::<f64>();
    let harmed = recent.iter().any(|one| one.froze > 0.0);
    let lossy = harmed && counted >= COUNTED && lost / counted > FREEZE_LOSSY;

    if !frozen && !lossy {
        return (next, None);
    }

    // The step is from the lower of the cap already asked for (a cap that has
    // not taken yet is still a step that was taken) and the rung this viewer's
    // subscription asks for. Not from the frames arriving; see `freeze_rungs_of`.
    let top = rungs.len() as isize - 1;
    let capped = watch
        .cap
        .and_then(|cap| rungs.iter().position(|rung| rung.height == cap.rung.height))
        .map(|index| index as isize)
        .unwrap_or(-1);
    let from = (if capped < 0 { top } else { capped }).min(asked.map(|index| index as isize).unwrap_or(top));
    let lower = (from > 0).then(|| rungs.get(from as usize - 1).copied()).flatten();

    // Already at the bottom, or a share with no rungs beneath it: there is
    // nothing smaller to ask for, and asking every second would only be noise.
    // The evidence starts again.
    let Some(lower) = lower else {
        return (FreezeWatch { stalled: None, recent: Vec::new(), quiet: now + FREEZE_STALL_MS, ..next }, None);
    };

    // Frozen again while the original is still being tried: it could not be
    // carried, and it is asked for again twice as late. A further step under the
    // cap this takes is not a return (the cap clears `returned`) and leaves the
    // hold as it is.
    let hold = if watch.returned.is_some() { (watch.hold * 2.0).min(FREEZE_LONGEST_HOLD_MS) } else { watch.hold };

    (
        FreezeWatch {
            stalled: None,
            recent: Vec::new(),
            cap: Some(FreezeCap { rung: lower, until: now + hold }),
            hold,
            returned: None,
            quiet: now + FREEZE_SETTLE_MS,
            ..next
        },
        Some(FreezeAct::Cap { rung: lower }),
    )
}

/// A change in a counter, or nothing where either end is unknown or it went
/// backwards.
fn gain(value: Option<f64>, was: Option<f64>) -> f64 {
    match (value, was) {
        (Some(value), Some(was)) if value >= was => value - was,
        _ => 0.0,
    }
}

fn backwards(value: Option<f64>, was: Option<f64>) -> bool {
    matches!((value, was), (Some(value), Some(was)) if value < was)
}

fn numeric(value: Option<f64>) -> Option<f64> {
    value.filter(|value| value.is_finite())
}

/// One remote share, watched for freezing (freeze.ts `watchShareFreezes`, one
/// entry of it).
#[derive(Debug, uniffi::Object)]
pub struct ShareFreezeWatch {
    entry: Mutex<Entry>,
}

#[derive(Debug)]
struct Entry {
    watch: FreezeWatch,
    /// What was last told to the subscription at a moment it could take it.
    /// Never a telling that was dropped: see `tell`.
    told: Option<FreezeRung>,
    /// Whether anything was ever asked of this subscription. One never capped is
    /// never touched, so a share that never freezes is subscribed exactly as it
    /// would be without this.
    touched: bool,
}

#[uniffi::export]
impl ShareFreezeWatch {
    /// A watch for a publication seen for the first time, at `now` milliseconds.
    #[uniffi::constructor]
    pub fn new(now: f64) -> Arc<Self> {
        Arc::new(Self { entry: Mutex::new(Entry { watch: fresh(now, None), told: None, touched: false }) })
    }

    /// One reading a second, and the size to ask for if it should change now.
    ///
    /// `layers` are the publication's published layers, `measured` the size the
    /// picture covers on screen in device pixels where the app measures one,
    /// and `portrait` whether the publication is taller than it is wide.
    /// `can_tell` is whether the share is subscribed and its track is here:
    /// a request for a track that is not is dropped by the SDK while saying
    /// nothing, and one dropped and recorded as told was never told again, so
    /// a viewer stayed a rung down, full screen included, until the share ended
    /// (found in review on the web, 2026-10-14).
    pub fn tick(
        &self,
        reading: Option<FreezeReading>,
        layers: Vec<FreezeRung>,
        measured: Option<FreezeRung>,
        portrait: bool,
        now: f64,
        can_tell: bool,
    ) -> Option<FreezeRung> {
        let mut entry = self.entry.lock().unwrap();
        let rungs = freeze_rungs_of(layers);
        let asked = rung_asked(&rungs, measured, portrait);
        let (watch, _) = judge(&entry.watch, reading.as_ref(), &rungs, now, asked);
        entry.watch = watch;
        tell(&mut entry, can_tell)
    }

    /// The share's track has arrived again (put back on the stage, or
    /// resubscribed): the readings start again, the back-off does not, and the
    /// right size is asked for at once so the first keyframe is of the rung
    /// that is wanted.
    pub fn subscribed(&self, can_tell: bool) -> Option<FreezeRung> {
        let mut entry = self.entry.lock().unwrap();
        entry.watch = fresh(entry.watch.quiet, Some(&entry.watch));
        tell(&mut entry, can_tell)
    }

    /// The cap being asked for now, if any.
    pub fn capped(&self) -> Option<FreezeRung> {
        self.entry.lock().unwrap().watch.cap.map(|cap| cap.rung)
    }
}

/// What the subscription should be told, if it is not what it was last told.
/// The tick tells as well as the arrival, because during a reconnection the
/// track is here before anything says it arrived.
fn tell(entry: &mut Entry, can_tell: bool) -> Option<FreezeRung> {
    let wanted = entry.watch.cap.map(|cap| cap.rung).or(entry.touched.then_some(FREEZE_UNCAPPED))?;
    if !can_tell || entry.told == Some(wanted) {
        return None;
    }

    entry.told = Some(wanted);
    entry.touched = true;
    Some(wanted)
}
