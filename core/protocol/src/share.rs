//! What this participant publishes, and how long it holds a share it receives.
//!
//! Follows three files of the web client, and nothing here is new:
//!
//!   - web/src/live/room.ts for pictures: the camera's capture and the SDK
//!     ladder the web keeps for it; a shared screen's two questions (how large,
//!     how many frames), what each pairing is worth in bits, the rungs beneath
//!     it, `settle` (the top layer's ceiling brought down to the capture that
//!     arrived), `retune` (a setting changed mid-share), and the record of what
//!     a share is running at.
//!   - web/src/live/sound.ts for sound: a voice, a voice left alone, a screen's
//!     sound, sound shared on its own, and the song desk's music, each with its
//!     own bitrate, channels, DTX, RED, source and track name.
//!   - web/src/live/delay.ts for the cushion: 200 ms held on every remote
//!     screen share and screen-share sound, never on a camera or a microphone.
//!
//! A browser in the same room sees whatever these numbers produce, so each one
//! is the web's, and where the Rust SDK would turn the same options into
//! different encodings than the browser SDK does, the records here are shaped
//! so that it cannot: see `PublishLayer` and `SharePublish::simulcast`.
//!
//! Everything here is arithmetic. Capturing, publishing and writing sender
//! parameters belong to the media crate, which reads these records and hands
//! them to LiveKit's Rust SDK as they are.

use std::sync::{Arc, Mutex};

/* -------------------------------------------------------------------------- */
/* Shapes shared by the camera and the share                                  */
/* -------------------------------------------------------------------------- */

/// The only video codec this client publishes, camera and share alike.
///
/// H.264 because it is the one codec every machine encodes and decodes in
/// hardware (room.ts, publishDefaults). VP9 was tried for its layering and cost
/// every participant in a call of six a software decode of five streams; VP8 was
/// kept for still text for a while and put the cost of one person's sharpness
/// on everybody else's processor.
pub const VIDEO_CODEC: &str = "h264";

/// Whether a second encode in another codec is published beside the first.
///
/// Never. The browser SDK's default starts a whole second encode of the same
/// picture the moment one subscriber cannot decode H.264, at the same ceiling,
/// in software, and nothing anywhere says so: one person joining roughly doubles
/// what the person sharing uploads (room.ts, `backupCodec: false`).
pub const BACKUP_CODEC: bool = false;

/// What an encoder gives up first when the line cannot carry everything.
///
/// Maps one to one onto the SDK's `DegradationPreference`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum VideoDegradation {
    MaintainResolution,
    MaintainFramerate,
    Balanced,
}

/// A layer as it is encoded: the shape of the SDK's `VideoPreset`.
///
/// For the Rust SDK this is passed as it is (`VideoPreset::new(width, height,
/// max_bitrate, max_framerate)`), and it is already what the browser SDK would
/// encode. That matters for the frame rate. The browser SDK holds every rung to
/// the top layer's rate as it builds the encodings (encodingsFromPresets takes
/// the smaller of the two), so a share at fifteen sends fifteen frames in every
/// layer; the Rust SDK copies a preset's rate verbatim, and the same preset would
/// send a rung at thirty under a top at fifteen. So the rate here is the one the
/// browser would write, not the preset's own.
#[derive(Debug, Clone, Copy, PartialEq, uniffi::Record)]
pub struct PublishLayer {
    pub width: u32,
    pub height: u32,
    pub max_bitrate: u64,
    pub max_framerate: f64,
}

/// The top layer's encoding: the shape of the SDK's `VideoEncoding`. Its size is
/// the capture's own, which the SDK reads from the track.
#[derive(Debug, Clone, Copy, PartialEq, uniffi::Record)]
pub struct PublishEncoding {
    pub max_bitrate: u64,
    pub max_framerate: f64,
}

/// A preset as room.ts and the SDK name one: a size and what it may send.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Preset {
    width: u32,
    height: u32,
    max_bitrate: u64,
    max_framerate: u32,
}

impl Preset {
    const fn new(width: u32, height: u32, max_bitrate: u64, max_framerate: u32) -> Self {
        Self { width, height, max_bitrate, max_framerate }
    }

    fn shorter(&self) -> u32 {
        self.width.min(self.height)
    }

    /// As the browser SDK encodes it beneath a top layer sent at `top_rate`.
    fn beneath(&self, top_rate: f64) -> PublishLayer {
        PublishLayer {
            width: self.width,
            height: self.height,
            max_bitrate: self.max_bitrate,
            max_framerate: (self.max_framerate as f64).min(top_rate),
        }
    }

    /// The preset itself, unclamped, as room.ts holds it.
    fn as_layer(&self) -> PublishLayer {
        PublishLayer {
            width: self.width,
            height: self.height,
            max_bitrate: self.max_bitrate,
            max_framerate: self.max_framerate as f64,
        }
    }
}

/* -------------------------------------------------------------------------- */
/* The camera                                                                 */
/* -------------------------------------------------------------------------- */

/// What kind of device is capturing, for the one rule that depends on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum CameraDevice {
    Phone,
    Tablet,
    Computer,
}

/// What the camera is asked to capture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct CameraCapture {
    pub width: u32,
    pub height: u32,
    pub frame_rate: u32,
}

/// The SDK's 16:9 presets (livekit-client 2.21, `VideoPresets`), smallest first.
const PRESETS_169: [Preset; 9] = [
    Preset::new(160, 90, 90_000, 20),
    Preset::new(320, 180, 160_000, 20),
    Preset::new(384, 216, 180_000, 20),
    Preset::new(640, 360, 450_000, 20),
    Preset::new(960, 540, 800_000, 25),
    Preset::new(1280, 720, 1_700_000, 30),
    Preset::new(1920, 1080, 3_000_000, 30),
    Preset::new(2560, 1440, 5_000_000, 30),
    Preset::new(3840, 2160, 8_000_000, 30),
];

/// The SDK's 4:3 presets (`VideoPresets43`), smallest first.
const PRESETS_43: [Preset; 9] = [
    Preset::new(160, 120, 70_000, 20),
    Preset::new(240, 180, 125_000, 20),
    Preset::new(320, 240, 140_000, 20),
    Preset::new(480, 360, 330_000, 20),
    Preset::new(640, 480, 500_000, 20),
    Preset::new(720, 540, 600_000, 25),
    Preset::new(960, 720, 1_300_000, 30),
    Preset::new(1440, 1080, 2_300_000, 30),
    Preset::new(1920, 1440, 3_800_000, 30),
];

/// The SDK's camera rungs (`defaultSimulcastPresets169`, `...43`): h180 and h360.
const RUNGS_169: [Preset; 2] = [PRESETS_169[1], PRESETS_169[3]];
const RUNGS_43: [Preset; 2] = [PRESETS_43[1], PRESETS_43[3]];

/// What the camera captures.
///
/// The web asks for 1280x720 at thirty and, on a device pointed at with a
/// finger, 640x360 at twenty (room.ts, `videoCaptureDefaults`, from the SDK's
/// h720 and h360 presets): a phone in a nine-up grid is drawn a couple of
/// hundred points wide however large it was captured, so the lines above that
/// are encoded, sent and thrown away on the one device paying for all three out
/// of a battery.
///
/// The native rule is phones 360 and tablets and computers 720. The web's test
/// is `(pointer: coarse)`, which a tablet's browser also answers yes to, so an
/// iPad in a browser captures 360 lines where the native app captures 720; a
/// tablet draws a call large enough to use them, and its encoder and battery are
/// a computer's rather than a phone's.
#[uniffi::export]
pub fn camera_capture(device: CameraDevice) -> CameraCapture {
    let preset = match device {
        CameraDevice::Phone => PRESETS_169[3],
        CameraDevice::Tablet | CameraDevice::Computer => PRESETS_169[5],
    };
    CameraCapture { width: preset.width, height: preset.height, frame_rate: preset.max_framerate }
}

/// How a camera of this capture size is published.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct CameraPublish {
    pub encoding: PublishEncoding,
    /// Smallest first: exactly the layers the browser SDK puts beneath the top.
    pub rungs: Vec<PublishLayer>,
    /// False only when `rungs` is empty; see `SharePublish::simulcast`.
    pub simulcast: bool,
    pub degradation: VideoDegradation,
}

/// The camera's encodings, as the browser SDK works them out from the size the
/// capture came back at (computeVideoEncodings, determineAppropriateEncoding,
/// defaultSimulcastLayers; livekit-client.esm.mjs around 13111 and 21146).
///
/// The web sets no encoding of its own for the camera and keeps the SDK's: the
/// top layer takes the first preset at least as wide as the capture's longer
/// side (1280x720 is 1.7 Mb/s at thirty, 640x360 is 450 kb/s at twenty), and
/// beneath it go 320x180 at 160 kb/s and 640x360 at 450 kb/s, both at twenty
/// (the 4:3 tables where the capture is nearer 4:3). Three layers from a longer
/// side of 960, two from 480, one below that.
///
/// Worked out here rather than left to the Rust SDK, because the Rust SDK would
/// not arrive at the same numbers. It picks the first preset strictly wider than
/// the capture, so a 1280x720 camera would ask 3 Mb/s where every browser asks
/// 1.7; and given two rungs for a capture between 480 and 960 wide it keeps the
/// larger where the browser keeps the smaller. Given these rungs and this
/// encoding it encodes what the browser does.
#[uniffi::export]
pub fn camera_publish(width: u32, height: u32) -> Result<CameraPublish, crate::ProtocolError> {
    if width == 0 || height == 0 {
        return Err(crate::ProtocolError::Invalid {
            detail: "a camera is published at the size it was captured, and this one has none".into(),
        });
    }

    let (presets, rungs) =
        if sixteen_nine(width, height) { (&PRESETS_169, &RUNGS_169) } else { (&PRESETS_43, &RUNGS_43) };

    let size = width.max(height);
    let mut top = presets[0];
    for preset in presets {
        top = *preset;
        if preset.width >= size {
            break;
        }
    }

    let rate = top.max_framerate as f64;
    let beneath: Vec<PublishLayer> = if size >= 960 {
        rungs.iter().map(|rung| rung.beneath(rate)).collect()
    } else if size >= 480 {
        vec![rungs[0].beneath(rate)]
    } else {
        Vec::new()
    };

    Ok(CameraPublish {
        encoding: PublishEncoding { max_bitrate: top.max_bitrate, max_framerate: rate },
        simulcast: !beneath.is_empty(),
        rungs: beneath,
        // The SDK's default for a camera, which the web keeps: a face that
        // moves smoothly at a lower resolution is a better call than a sharp
        // one that stutters.
        degradation: VideoDegradation::MaintainFramerate,
    })
}

/// Whether a picture is nearer 16:9 than 4:3, by the SDK's own test.
fn sixteen_nine(width: u32, height: u32) -> bool {
    let (width, height) = (width as f64, height as f64);
    let aspect = if width > height { width / height } else { height / width };
    (aspect - 16.0 / 9.0).abs() < (aspect - 4.0 / 3.0).abs()
}

/* -------------------------------------------------------------------------- */
/* A shared screen: the two questions                                         */
/* -------------------------------------------------------------------------- */

/// How large a picture a share asks for. Automatic is a ceiling that suits any
/// display (2560x1440 at sixty, which is what most people get because most
/// people never open the menu); the named sizes are for somebody who decided.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, uniffi::Enum)]
pub enum ShareQuality {
    Auto,
    P1080,
    P1440,
    P4k,
}

pub const SHARE_QUALITIES: [ShareQuality; 4] =
    [ShareQuality::Auto, ShareQuality::P1080, ShareQuality::P1440, ShareQuality::P4k];

/// How many frames a share may be asked for.
pub const SHARE_FRAME_RATES: [u32; 5] = [15, 30, 60, 120, 240];

/// Both choices together, the way a share is started and changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, uniffi::Record)]
pub struct ShareSetting {
    pub quality: ShareQuality,
    pub frame_rate: u32,
}

/// A width and a height.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct ShareSize {
    pub width: u32,
    pub height: u32,
}

/// What a capture turned out to be, as far as anything has said.
#[derive(Debug, Clone, Copy, PartialEq, Default, uniffi::Record)]
pub struct ShareCapture {
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub frame_rate: Option<f64>,
}

/// What a share's pixels are, which the encoder uses to choose between holding
/// a picture still and holding motion smooth (`MediaStreamTrack.contentHint`,
/// libwebrtc's `kDetailed`, `kText` and `kFluid`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum ShareContentHint {
    Detail,
    Text,
    Motion,
}

/// Every frame rate each named size offers. Ceilings from what an encoder can
/// actually do, not preferences: 4K at 120 is a billion pixels a second, and an
/// encoder asked for it does not fail loudly, it falls behind, and a share that
/// drifts further behind the longer it runs is worse than one never offered.
/// Every slower rate stays: a page of code at 4K wants fifteen frames and the
/// sharpest picture.
fn rates_of(quality: ShareQuality) -> &'static [u32] {
    match quality {
        ShareQuality::Auto | ShareQuality::P1080 => &SHARE_FRAME_RATES,
        ShareQuality::P1440 => &[15, 30, 60, 120],
        ShareQuality::P4k => &[15, 30, 60],
    }
}

/// The frame rates this size may be asked for (room.ts `ratesFor`).
#[uniffi::export]
pub fn share_rates_for(quality: ShareQuality) -> Vec<u32> {
    rates_of(quality).to_vec()
}

/// Whether a pairing is one the menu offers at all (room.ts `offers`).
#[uniffi::export]
pub fn share_offers(quality: ShareQuality, frame_rate: u32) -> bool {
    rates_of(quality).contains(&frame_rate)
}

const AUTOMATIC: Preset = Preset::new(2560, 1440, 0, 60);

/// The picture a setting asks the capture for; automatic asks for what it sends.
#[uniffi::export]
pub fn share_size_of(quality: ShareQuality) -> ShareSize {
    match quality {
        ShareQuality::Auto | ShareQuality::P1440 => ShareSize { width: 2560, height: 1440 },
        ShareQuality::P1080 => ShareSize { width: 1920, height: 1080 },
        ShareQuality::P4k => ShareSize { width: 3840, height: 2160 },
    }
}

/// How a quality is written down, as the web writes it under "meet.share.quality".
#[uniffi::export]
pub fn share_quality_key(quality: ShareQuality) -> String {
    match quality {
        ShareQuality::Auto => "auto",
        ShareQuality::P1080 => "1080p",
        ShareQuality::P1440 => "1440p",
        ShareQuality::P4k => "4k",
    }
    .to_string()
}

/// The quality to use, from what was written down: automatic for anything that
/// is not one of the four names, including nothing at all.
#[uniffi::export]
pub fn share_remembered_quality(stored: Option<String>) -> ShareQuality {
    SHARE_QUALITIES
        .into_iter()
        .find(|quality| stored.as_deref() == Some(share_quality_key(*quality).as_str()))
        .unwrap_or(ShareQuality::Auto)
}

/// The frame rate to use, from what was written down (the web's
/// "meet-live.share-frame-rate", read through `Number`).
///
/// Sixty for anything that is not one of the five rates: thirty is visibly less
/// smooth on anything that scrolls or plays, and this is what somebody gets who
/// picks a size and never touches the rate. A rate stored while a larger size
/// allowed it comes back as it is and is clamped by `share_profile`, so it can
/// never reach an encoder that cannot do it.
#[uniffi::export]
pub fn share_remembered_frame_rate(stored: Option<String>) -> u32 {
    let read = stored.as_deref().map(js_number).unwrap_or(0.0);
    SHARE_FRAME_RATES.into_iter().find(|rate| *rate as f64 == read).unwrap_or(60)
}

/* -------------------------------------------------------------------------- */
/* What a setting is worth                                                    */
/* -------------------------------------------------------------------------- */

/// About a fifth of a bit per pixel per frame: twice what H.264 needs for screen
/// content, on purpose, since 2026-09-07 when the person sharing asked that the
/// picture never be trimmed. A ceiling, not a target; a still page spends a
/// fraction of it.
const BITS_PER_PIXEL_PER_FRAME: f64 = 0.22;

/// Past this the limit is the network, not the number. 1080p at 240 and 4K at
/// sixty both land near it.
const BITRATE_CAP: f64 = 80_000_000.0;

/// The rate the per-pixel figure was chosen to be right at.
const BASE_RATE: f64 = 30.0;

/// Bits a second for a picture of this size at this rate (room.ts `bitrateFor`).
///
/// The rate enters under a square root: on a screen, one frame differs from the
/// last by a moved cursor and a line of text, and linear in the rate the high
/// settings asked for forty-four and fifty megabits, which no ordinary path
/// carries; the estimator clamps, the encoder drops frames, and the share
/// stutters while the throughput reading looks healthy.
///
/// There is no floor. An eight-megabit floor once sat here, raised nothing
/// anybody could choose (the smallest pairing is 9.68 Mb/s), and truncated the
/// refund `settle` gives a small window.
#[uniffi::export]
pub fn share_bitrate_for(width: u32, height: u32, frame_rate: f64) -> u64 {
    let per_frame = width as f64 * height as f64 * BITS_PER_PIXEL_PER_FRAME;
    let wanted = for_rate(per_frame * BASE_RATE, frame_rate);
    round(BITRATE_CAP.min(wanted))
}

/// What a picture worth `at_base` at thirty frames is worth at `frame_rate`: the
/// one curve, shared by the top layer and the middle rung's sixty, so that a
/// change to it cannot part them.
fn for_rate(at_base: f64, frame_rate: f64) -> f64 {
    at_base * (frame_rate / BASE_RATE).sqrt()
}

/// JavaScript's `Math.round` for the non-negative figures it is used on here:
/// a half goes up, which for a positive number is also what Rust's own does.
fn round(value: f64) -> u64 {
    value.max(0.0).round() as u64
}

/// Everything one setting sends at most (room.ts `settingsFor`).
#[derive(Debug, Clone, Copy, PartialEq, uniffi::Record)]
pub struct ShareProfile {
    pub width: u32,
    pub height: u32,
    /// The rate asked of the capture and the top layer, clamped to the size.
    pub frame_rate: u32,
    pub max_bitrate: u64,
    pub content_hint: ShareContentHint,
    pub degradation: VideoDegradation,
}

/// What one setting sends at most.
///
/// A rate the size does not offer is clamped to the size's fastest rather than
/// refused: a rate remembered from a larger allowance (240 chosen at 1080p and
/// then 4K picked) would otherwise reach an encoder that cannot do it, and that
/// failure is silent.
///
/// Every setting holds the picture and lets the frames go
/// (`MaintainResolution`). What is on a shared screen is text, code and
/// diagrams: a late frame is late, and type scaled to two thirds of its size is
/// unreadable with nothing the person reading can do about it. And every setting
/// is one encode at its ceiling; automatic no longer gives ground either.
#[uniffi::export]
pub fn share_profile(frame_rate: u32, quality: ShareQuality) -> ShareProfile {
    if quality == ShareQuality::Auto {
        return ShareProfile {
            width: AUTOMATIC.width,
            height: AUTOMATIC.height,
            frame_rate: AUTOMATIC.max_framerate,
            max_bitrate: share_bitrate_for(AUTOMATIC.width, AUTOMATIC.height, AUTOMATIC.max_framerate as f64),
            content_hint: ShareContentHint::Detail,
            degradation: VideoDegradation::MaintainResolution,
        };
    }

    let size = share_size_of(quality);
    let rates = rates_of(quality);
    let capped = if rates.contains(&frame_rate) { frame_rate } else { rates.last().copied().unwrap_or(30) };

    ShareProfile {
        width: size.width,
        height: size.height,
        frame_rate: capped,
        max_bitrate: share_bitrate_for(size.width, size.height, capped as f64),
        content_hint: if capped <= 30 { ShareContentHint::Text } else { ShareContentHint::Motion },
        degradation: VideoDegradation::MaintainResolution,
    }
}

/// The rate a share's top layer goes out at: the setting's, or fewer where the
/// capture says it gives fewer, never more. A capture that reports no rate, or
/// nought, has said nothing.
fn sent_at(capture: &ShareCapture, frame_rate: f64) -> f64 {
    let given = capture.frame_rate.filter(|rate| *rate != 0.0 && !rate.is_nan()).unwrap_or(frame_rate);
    given.min(frame_rate)
}

/// The ceiling for a profile against what the capture says it is. Downward only.
fn worth_of(profile: &ShareProfile, settled: &ShareCapture) -> u64 {
    let rate = sent_at(settled, profile.frame_rate as f64);
    profile.max_bitrate.min(share_bitrate_for(
        settled.width.unwrap_or(profile.width),
        settled.height.unwrap_or(profile.height),
        rate,
    ))
}

/// The ceiling a setting comes to against a capture, by the arithmetic
/// `share_retune_encodings` applies (room.ts `askOf`).
///
/// One function for both, because the uplink hint in strain.rs checks whether a
/// setting would fit a line before offering it, and is only right while it gets
/// the answer the retune will get. The capture is clamped to the setting's size
/// first, which is what narrowing the capture does to it on the way in.
#[uniffi::export]
pub fn share_ask_of(setting: ShareSetting, capture: ShareCapture) -> u64 {
    let profile = share_profile(setting.frame_rate, setting.quality);
    worth_of(
        &profile,
        &ShareCapture {
            width: Some(capture.width.unwrap_or(profile.width).min(profile.width)),
            height: Some(capture.height.unwrap_or(profile.height).min(profile.height)),
            frame_rate: capture.frame_rate,
        },
    )
}

/// Every setting the menu offers that asks for less than this one in both
/// directions: never a larger picture and never more frames (room.ts
/// `stepsDown`).
///
/// Largest first, and within a size the most frames first, so a caller taking
/// the first of several equal answers takes the one nearest to what was chosen.
/// Automatic is never among them, and counts as what it sends, 1440p at sixty.
#[uniffi::export]
pub fn share_steps_down(from: ShareSetting) -> Vec<ShareSetting> {
    let top = match from.quality {
        ShareQuality::Auto => (AUTOMATIC.width, AUTOMATIC.height, AUTOMATIC.max_framerate),
        quality => {
            let size = share_size_of(quality);
            (size.width, size.height, from.frame_rate)
        }
    };

    let mut steps = Vec::new();
    for quality in [ShareQuality::P4k, ShareQuality::P1440, ShareQuality::P1080] {
        let size = share_size_of(quality);
        if size.width > top.0 || size.height > top.1 {
            continue;
        }

        for frame_rate in rates_of(quality).iter().rev().copied() {
            if frame_rate > top.2 || (quality == from.quality && frame_rate == from.frame_rate) {
                continue;
            }
            steps.push(ShareSetting { quality, frame_rate });
        }
    }
    steps
}

/* -------------------------------------------------------------------------- */
/* The rungs beneath a share                                                  */
/* -------------------------------------------------------------------------- */

/// 1280x720 at 2.5 Mb/s and thirty frames: a small picture and a short line.
///
/// For a day it was 1.5 and fifteen, and under moving content that came out at
/// six to eleven frames a second, which was all two sessions sharing a lossy
/// 20 Mb/s line received for the minute measured. The owner settled these on
/// 2026-10-14; they are a decision, not a tuning.
const SMALL: Preset = Preset::new(1280, 720, 2_500_000, 30);

/// 1920x1080 at 8 Mb/s and thirty: a viewer whose line cannot carry the
/// original. Before it the gap above 720p was fourteen to nineteen times, and a
/// viewer on a 20 to 30 Mb/s line swung between the two at every keyframe (the
/// share that "jumps to 360p every so often", 2026-10-13).
const MIDDLE: Preset = Preset::new(1920, 1080, 8_000_000, 30);

/// The share's rate at and above which its middle rung is sent at sixty.
const FAST: u32 = 60;

/// The middle rung of a share sent at sixty frames or more: 8 Mb/s priced up for
/// sixty by the top layer's own curve, 11,313,708 b/s. At thirty for every
/// share, a sixty-frame share reached anybody watching in an ordinary window
/// (the most common picture) at thirty. Sixty rather than the share's own 120
/// or 240, because the rung is for a line that cannot carry the original.
fn middle_fast() -> Preset {
    Preset::new(1920, 1080, round(for_rate(MIDDLE.max_bitrate as f64, FAST as f64)), FAST)
}

/// The small rung, as room.ts names it (`SMALL`).
#[uniffi::export]
pub fn share_rung_small() -> PublishLayer {
    SMALL.as_layer()
}

/// The middle rung of a share under sixty frames (`MIDDLE`).
#[uniffi::export]
pub fn share_rung_middle() -> PublishLayer {
    MIDDLE.as_layer()
}

/// The middle rung of a share at sixty frames or more (`MIDDLE_FAST`).
#[uniffi::export]
pub fn share_rung_middle_fast() -> PublishLayer {
    middle_fast().as_layer()
}

fn ladder(frame_rate: f64) -> [Preset; 2] {
    [SMALL, if frame_rate >= FAST as f64 { middle_fast() } else { MIDDLE }]
}

/// What can go beneath the top layer of a share sent at `frame_rate`, smallest
/// first (room.ts `ladderAt`): the order the SDK sorts presets into by bitrate,
/// and so the order the encodings come back in.
#[uniffi::export]
pub fn share_ladder_at(frame_rate: f64) -> Vec<PublishLayer> {
    ladder(frame_rate).iter().map(Preset::as_layer).collect()
}

fn rungs_beneath(capture: &ShareCapture, frame_rate: f64) -> Vec<Preset> {
    let ladder = ladder(sent_at(capture, frame_rate));
    let shorter = capture.width.unwrap_or(0).min(capture.height.unwrap_or(0));
    if shorter == 0 {
        return ladder.to_vec();
    }
    ladder.into_iter().filter(|rung| rung.shorter() < shorter).collect()
}

/// The rungs a capture of this size is published with: those smaller than it,
/// as presets (room.ts `rungsBeneath`).
///
/// A rung that is not smaller is a copy of the original. The SDK scales a rung
/// by the capture's shorter side over the rung's and stops at one, so a 1080p
/// display would carry the middle rung at 1920x1080, the top's own picture at
/// a lower ceiling, and dynacast would turn it on with the original whenever
/// anybody watched at full size: upload paid for nothing. A 1080p capture is
/// published with the small rung alone, a window of 720 lines or less with none.
///
/// Every rung when the capture will not say how large it is; the SDK scales
/// none of them above the capture either way. Decided once, at publish: the
/// number of encodings cannot change without publishing again, and publishing
/// again drops every viewer's picture for a keyframe at the moment somebody
/// adjusted a setting.
#[uniffi::export]
pub fn share_rungs_beneath(capture: ShareCapture, frame_rate: f64) -> Vec<PublishLayer> {
    rungs_beneath(&capture, frame_rate).iter().map(Preset::as_layer).collect()
}

/* -------------------------------------------------------------------------- */
/* Starting a share                                                           */
/* -------------------------------------------------------------------------- */

/// What the capture is asked for when a share starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct ShareCaptureRequest {
    pub width: u32,
    pub height: u32,
    pub frame_rate: u32,
    pub content_hint: ShareContentHint,
}

/// What the capture is asked for when a share starts (room.ts `share`).
///
/// The size chosen and the rate clamped to it. The web asks for the size as an
/// ideal rather than a ceiling, because the SDK's capture options have no way to
/// say ceiling, and the browser scales a smaller display up to it; a clamp to the
/// screen was tried and handed somebody sharing a 4K monitor from a laptop 1080p
/// of it. A native capture that can only ask for at most this is no worse:
/// `share_publish` and `settle` price what arrives, not what was asked.
#[uniffi::export]
pub fn share_capture_request(frame_rate: u32, quality: ShareQuality) -> ShareCaptureRequest {
    let profile = share_profile(frame_rate, quality);
    ShareCaptureRequest {
        width: profile.width,
        height: profile.height,
        frame_rate: profile.frame_rate,
        content_hint: profile.content_hint,
    }
}

/// How a share is published, given the capture that came back.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct SharePublish {
    /// The top layer as the web publishes it: the setting's whole ceiling and
    /// rate. `settled_bitrate`, where there is one, is written over it after.
    pub encoding: PublishEncoding,
    /// Smallest first, each held to the top layer's rate; see `PublishLayer`.
    /// Empty on purpose for a capture of 720 lines or less.
    pub rungs: Vec<PublishLayer>,
    /// Whether to publish with simulcast: false exactly when `rungs` is empty.
    ///
    /// The web publishes `simulcast: true` with an empty list, because the
    /// browser SDK reads a missing list as "use mine" and adds a rung of its own
    /// (a quarter of the top layer at half its size), and reads an empty one as
    /// the original alone. The Rust SDK given an empty list unwraps a rung that
    /// is not there and panics for any capture 480 or more wide
    /// (options.rs, compute_video_encodings); with simulcast off it encodes the
    /// original alone, which is the same single layer the browser sends.
    pub simulcast: bool,
    /// What `settle` lowers the top layer's ceiling to once the share is up,
    /// where the capture is worth less than the setting.
    pub settled_bitrate: Option<u64>,
    pub degradation: VideoDegradation,
    pub content_hint: ShareContentHint,
    /// What the share is recorded as running at; see `ShareRun`.
    pub recorded: ShareSetting,
}

/// How a share is published (room.ts `share`, its `publishing` options and
/// `settle`).
///
/// The whole allowance in the top layer and the rungs beneath it. Nothing is
/// taken off the top to pay for them: the SDK hands the top layer the encoding
/// it was given and each rung the bitrate its own preset names. A share once
/// went out as one encode, and on a live call a viewer whose leg carried two
/// megabits a second saw the picture disappear while the sound went on, because
/// a media server holding one layer can only forward it or pause it.
#[uniffi::export]
pub fn share_publish(frame_rate: u32, quality: ShareQuality, captured: ShareCapture) -> SharePublish {
    let profile = share_profile(frame_rate, quality);
    let top_rate = profile.frame_rate as f64;
    let rungs: Vec<PublishLayer> =
        rungs_beneath(&captured, top_rate).iter().map(|rung| rung.beneath(top_rate)).collect();

    SharePublish {
        encoding: PublishEncoding { max_bitrate: profile.max_bitrate, max_framerate: top_rate },
        simulcast: !rungs.is_empty(),
        rungs,
        settled_bitrate: share_settle(frame_rate, quality, captured),
        degradation: profile.degradation,
        content_hint: profile.content_hint,
        recorded: share_recorded(quality, frame_rate),
    }
}

/// The ceiling the top layer comes down to for the picture that actually
/// arrived, or nothing where it stays (room.ts `settle`).
///
/// What a share is worth is worked out from the size chosen, and that size is a
/// request: a window is smaller than a screen, and a display is whatever it is.
/// So the number reaching the encoder was computed for a picture nobody was
/// necessarily sending, and upload is the scarcest thing in these calls.
///
/// The rate as well as the size: somebody who picks 240 on a display that gives
/// sixty was handed a ceiling worked out for 240, and the size check alone never
/// saw it. Only downward, never above what was chosen, and only the top layer,
/// which is the one the setting decides. Nothing anybody sees changes; a
/// ceiling is a ceiling. A capture that will not say its size settles nothing.
#[uniffi::export]
pub fn share_settle(frame_rate: u32, quality: ShareQuality, settled: ShareCapture) -> Option<u64> {
    let profile = share_profile(frame_rate, quality);
    let width = settled.width.unwrap_or(0);
    let height = settled.height.unwrap_or(0);
    if width == 0 || height == 0 {
        return None;
    }

    let rate = sent_at(&settled, profile.frame_rate as f64);
    let worth = share_bitrate_for(width, height, rate);
    (worth < profile.max_bitrate).then_some(worth)
}

/// The setting a share is recorded as running at.
///
/// Automatic keeps the rate it was handed rather than the sixty it sends,
/// because that is the rate the menu goes back to if somebody picks a size
/// mid-share; the price of automatic does not depend on it. A named size records
/// its clamped rate, which is what is being sent.
#[uniffi::export]
pub fn share_recorded(quality: ShareQuality, frame_rate: u32) -> ShareSetting {
    match quality {
        ShareQuality::Auto => ShareSetting { quality, frame_rate },
        _ => ShareSetting { quality, frame_rate: share_profile(frame_rate, quality).frame_rate },
    }
}

/* -------------------------------------------------------------------------- */
/* Changing a share as it runs                                                */
/* -------------------------------------------------------------------------- */

/// What a running capture is narrowed to when its setting changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct ShareCaptureConstraints {
    /// Ceilings: a capture can be asked for less than its source, never more,
    /// so a 1080p display adjusted to 1440p is left alone rather than scaled up.
    pub max_width: u32,
    pub max_height: u32,
    pub ideal_frame_rate: u32,
    /// Set before the constraints, so a frame captured during the change is
    /// already labelled.
    pub content_hint: ShareContentHint,
}

/// The first half of a change mid-share: what the capture is narrowed to
/// (room.ts `retune`, its `applyConstraints`).
///
/// A share is changed in place rather than published again: the picker would
/// otherwise come up in front of somebody who already chose a window, in front
/// of the meeting. The capture is read again after this is applied, and that
/// reading is what `share_retune_encodings` prices.
#[uniffi::export]
pub fn share_retune_capture(frame_rate: u32, quality: ShareQuality) -> ShareCaptureConstraints {
    let profile = share_profile(frame_rate, quality);
    ShareCaptureConstraints {
        max_width: profile.width,
        max_height: profile.height,
        ideal_frame_rate: profile.frame_rate,
        content_hint: profile.content_hint,
    }
}

/// One encoding's new parameters.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct ShareEncodingUpdate {
    /// Its place from the bottom: 0 is the smallest. The SDKs name places by
    /// rid, `q`, `h` and `f` from the bottom; the browser's sender lists them
    /// bottom first and the Rust SDK's top first, so `rid` is the safer key.
    pub index: u32,
    pub rid: String,
    pub top: bool,
    pub max_bitrate: u64,
    pub max_framerate: f64,
    /// Left as it is where absent: the top layer is never rescaled.
    pub scale_resolution_down_by: Option<f64>,
}

/// The second half of a change mid-share: what goes out.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct ShareRetune {
    /// Bottom first. Empty when there are no encodings to write, which the web
    /// reports as a change that did not take.
    pub encodings: Vec<ShareEncodingUpdate>,
    pub degradation: VideoDegradation,
    /// What to record the share as running at, once the write has taken; a
    /// refused write leaves the share sending what it sent before, and the menu
    /// showing that is the menu telling the truth.
    pub recorded: ShareSetting,
}

/// The second half of a change mid-share: every encoding's ceiling, rate and
/// scale, against the capture as it is after `share_retune_capture` (room.ts
/// `retune`, the sender's parameters).
///
/// The top layer takes the new setting and the rungs keep being rungs. This once
/// gave every encoding the setting's ceiling, which was right while a share was
/// one encode and silently destructive after: a fallback handed the top layer's
/// ceiling stops being a fallback, and somebody nudging the frame rate doubled
/// their upload and took away the only thing a starved viewer could receive.
///
/// The top's ceiling is priced against the picture that is there, not the one
/// asked for: writing the setting's own figure threw away `settle`'s correction
/// for good, and an 800x600 window went from 2.2 Mb/s to eight with one nudge.
///
/// Each rung by its place, from the ladder for the rate the share goes out at
/// now: a share stepped from sixty to thirty would otherwise keep a middle rung
/// priced for sixty. Never more frames than the top, as the SDK has it at
/// publish. And rescaled against the capture as it is now, because the SDK
/// works the scale out once at publish against a picture that may no longer
/// exist.
#[uniffi::export]
pub fn share_retune_encodings(
    frame_rate: u32,
    quality: ShareQuality,
    settled: ShareCapture,
    encodings: u32,
) -> ShareRetune {
    let profile = share_profile(frame_rate, quality);
    let recorded = share_recorded(quality, frame_rate);

    if encodings == 0 {
        return ShareRetune { encodings: Vec::new(), degradation: profile.degradation, recorded };
    }

    let top = encodings - 1;
    let shortest = settled.width.unwrap_or(profile.width).min(settled.height.unwrap_or(profile.height));
    let worth = worth_of(&profile, &settled);
    let ladder = ladder(sent_at(&settled, profile.frame_rate as f64));
    let top_rate = profile.frame_rate as f64;

    let updates = (0..encodings)
        .map(|index| {
            let rid = ["q", "h", "f"].get(index as usize).copied().unwrap_or("").to_string();
            if index == top {
                return ShareEncodingUpdate {
                    index,
                    rid,
                    top: true,
                    max_bitrate: worth,
                    max_framerate: top_rate,
                    scale_resolution_down_by: None,
                };
            }

            // A share is published with the first rungs of its ladder and never
            // a later one without an earlier, so a place beneath the top names
            // its rung.
            let beneath = ladder.get(index as usize).copied().unwrap_or(SMALL);
            ShareEncodingUpdate {
                index,
                rid,
                top: false,
                max_bitrate: beneath.max_bitrate,
                max_framerate: (beneath.max_framerate as f64).min(top_rate),
                scale_resolution_down_by: Some((shortest as f64 / beneath.height as f64).max(1.0)),
            }
        })
        .collect();

    ShareRetune { encodings: updates, degradation: profile.degradation, recorded }
}

/* -------------------------------------------------------------------------- */
/* What a share is running at                                                 */
/* -------------------------------------------------------------------------- */

/// The share being sent now, as the last start or change left it (room.ts
/// `Running`).
///
/// `id` says which share this is, and nothing else does: a full reconnect
/// publishes the same capture again under a new track sid without anybody
/// pressing anything, and the hint took that for a new share and forgot "Not
/// now" in the middle of the share it was said about. `started` and `since` are
/// on the caller's monotonic clock, in milliseconds: the hint ignores the first
/// moments of a share and of any change, because the line's estimate starts low
/// and climbs.
#[derive(Debug, Clone, Copy, PartialEq, uniffi::Record)]
pub struct ShareRunning {
    pub id: u64,
    pub quality: ShareQuality,
    pub frame_rate: u32,
    pub started: f64,
    pub since: f64,
}

impl ShareRunning {
    pub fn setting(&self) -> ShareSetting {
        ShareSetting { quality: self.quality, frame_rate: self.frame_rate }
    }
}

/// Where the share's setting is written down by the two things that apply one,
/// a start and a change, and read by the menu and the hint alike (room.ts
/// `record`, `runningShare`). Nothing here is stored; it is the share in front
/// of everybody now.
#[derive(Debug, Default, uniffi::Object)]
pub struct ShareRun {
    state: Mutex<(Option<ShareRunning>, u64)>,
}

#[uniffi::export]
impl ShareRun {
    #[uniffi::constructor]
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// A share has begun: a new id, its quiet time from now.
    ///
    /// Always new, even with a share still on record: a share stopped from the
    /// system's own control never passes through a stop here, and the next
    /// share would otherwise inherit the old one's start and skip the quiet
    /// time it is owed.
    pub fn begin(&self, quality: ShareQuality, frame_rate: u32, now: f64) -> ShareRunning {
        self.record(share_recorded(quality, frame_rate), true, now)
    }

    /// The running share's setting changed and the change took: the same share,
    /// changed now. Records a new share if none is on record.
    pub fn change(&self, quality: ShareQuality, frame_rate: u32, now: f64) -> ShareRunning {
        self.record(share_recorded(quality, frame_rate), false, now)
    }

    /// The share stopped.
    pub fn end(&self) {
        self.state.lock().unwrap().0 = None;
    }

    pub fn current(&self) -> Option<ShareRunning> {
        self.state.lock().unwrap().0
    }
}

impl ShareRun {
    fn record(&self, setting: ShareSetting, begun: bool, now: f64) -> ShareRunning {
        let mut state = self.state.lock().unwrap();
        let was = state.0;
        let running = match was {
            Some(was) if !begun => ShareRunning {
                id: was.id,
                quality: setting.quality,
                frame_rate: setting.frame_rate,
                started: was.started,
                since: now,
            },
            _ => {
                state.1 += 1;
                ShareRunning {
                    id: state.1,
                    quality: setting.quality,
                    frame_rate: setting.frame_rate,
                    started: now,
                    since: now,
                }
            }
        };
        state.0 = Some(running);
        running
    }
}

/* -------------------------------------------------------------------------- */
/* Sound                                                                      */
/* -------------------------------------------------------------------------- */

/// Where a track says it comes from: the SDK's track sources.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum MediaSource {
    Camera,
    Microphone,
    ScreenShare,
    ScreenShareAudio,
    Unknown,
}

/// A voice: 64 kb/s, where Opus stops being the thing anybody notices.
pub const VOICE_BITRATE: u64 = 64_000;
/// A voice left alone: more to carry once nothing is taken out of it.
pub const ORIGINAL_BITRATE: u64 = 96_000;
/// Music and a share's sound: transparent for stereo Opus, and with RED still
/// less on the wire than the smallest picture a share sends.
pub const MUSIC_BITRATE: u64 = 160_000;
/// Sound shared on its own: the most a 20 ms Opus frame holds.
pub const LISTENING_BITRATE: u64 = 510_000;

/// Track names a listener tells sound shared on its own apart by (lossless.ts):
/// the Opus track is named for whether a lossless stream is offered beside it.
pub const LISTENING: &str = "listening";
pub const LISTENING_LOSSLESS: &str = "listening-lossless";
pub const MUSIC: &str = "music";
pub const MUSIC_LOSSLESS: &str = "music-lossless";

/// What is sent as sound.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum SoundKind {
    /// The microphone; `original` is the "original sound" switch.
    Voice { original: bool },
    /// The sound that goes with a shared screen's picture.
    ScreenSound,
    /// Sound shared on its own, captured from an application.
    Listening { lossless_offered: bool },
    /// A track the song desk plays from the library.
    LibraryMusic { lossless_offered: bool },
}

/// What the capture is asked for: `MediaTrackConstraints` as the web sets them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct SoundCapture {
    pub echo_cancellation: bool,
    pub noise_suppression: bool,
    pub auto_gain_control: bool,
    pub voice_isolation: bool,
    pub channels: u32,
    /// Where the web names one: 48 kHz for anything captured as music.
    pub sample_rate: Option<u32>,
    /// Keep this app's own output out of the capture (the web's
    /// `restrictOwnAudio`; ScreenCaptureKit's `excludesCurrentProcessAudio`):
    /// sharing the whole system's sound would otherwise send everybody the
    /// call back a moment later.
    pub exclude_own_audio: bool,
}

/// How one kind of sound is captured and published.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct SoundProfile {
    pub source: MediaSource,
    /// The track's name; empty where the web gives none.
    pub name: String,
    pub max_bitrate: u64,
    /// Announced and sent as two channels (the web's `forceStereo`).
    pub stereo: bool,
    pub dtx: bool,
    /// Asked for; the SDKs turn it off in an encrypted call regardless.
    pub red: bool,
    pub capture: SoundCapture,
}

/// A voice is processed by default and left alone in original sound, except
/// for echo cancellation, which stays on even then: without it, anybody on
/// speakers sends the room back to itself, and the person who switched this on
/// is not the one who would hear it. Voice isolation is asked to be off rather
/// than left out, because the SDK's default asks for it, and where it is
/// honoured the voice is rebuilt by a model that is bad at leaving a voice
/// sounding like itself.
fn voice_capture(original: bool) -> SoundCapture {
    SoundCapture {
        echo_cancellation: true,
        noise_suppression: !original,
        auto_gain_control: !original,
        voice_isolation: false,
        channels: 1,
        sample_rate: None,
        exclude_own_audio: false,
    }
}

/// Everything that improves a voice damages music, so all of it is off.
const MUSIC_CAPTURE: SoundCapture = SoundCapture {
    echo_cancellation: false,
    noise_suppression: false,
    auto_gain_control: false,
    voice_isolation: false,
    channels: 2,
    sample_rate: Some(48_000),
    exclude_own_audio: true,
};

/// How a kind of sound is captured and published (sound.ts).
///
/// A voice at 64 kb/s with DTX off and RED on: DTX stopped sending in a pause
/// and clipped the first syllable after it, and RED repairs the cross-border
/// losses this deployment exists to survive. A share's sound and music in
/// stereo at 160 kb/s, DTX and RED named rather than left to the SDK, which
/// turns both off for a stereo track it is not told about.
///
/// Sound shared on its own at 510 kb/s with RED off, not by preference: at that
/// rate each packet carries two 1275-byte frames with RED, the media server's
/// 1500-byte packet buffer drops every one, and the listener receives nothing
/// at all; from about 250 kb/s the browser leaves the redundant copy out anyway.
/// The song desk's music is published the same way on a channel of its own
/// (source Unknown, names "music" and "music-lossless"), because a share of its
/// own source would be unpublished by stopping a screen share, taking the song
/// with it.
#[uniffi::export]
pub fn sound_profile(kind: SoundKind) -> SoundProfile {
    match kind {
        SoundKind::Voice { original } => SoundProfile {
            source: MediaSource::Microphone,
            name: String::new(),
            max_bitrate: if original { ORIGINAL_BITRATE } else { VOICE_BITRATE },
            stereo: false,
            dtx: false,
            red: true,
            capture: voice_capture(original),
        },
        SoundKind::ScreenSound => SoundProfile {
            source: MediaSource::ScreenShareAudio,
            name: String::new(),
            max_bitrate: MUSIC_BITRATE,
            stereo: true,
            dtx: false,
            red: true,
            capture: MUSIC_CAPTURE,
        },
        SoundKind::Listening { lossless_offered } => SoundProfile {
            source: MediaSource::ScreenShareAudio,
            name: if lossless_offered { LISTENING_LOSSLESS } else { LISTENING }.to_string(),
            max_bitrate: LISTENING_BITRATE,
            stereo: true,
            dtx: false,
            red: false,
            capture: MUSIC_CAPTURE,
        },
        SoundKind::LibraryMusic { lossless_offered } => SoundProfile {
            source: MediaSource::Unknown,
            name: if lossless_offered { MUSIC_LOSSLESS } else { MUSIC }.to_string(),
            max_bitrate: LISTENING_BITRATE,
            stereo: true,
            dtx: false,
            red: false,
            capture: MUSIC_CAPTURE,
        },
    }
}

/// Whether a lossless stream is offered beside sound shared on its own
/// (sound.ts `publishSound`, `losslessUnavailable`): where it was wanted, never
/// in an encrypted call, whose data the SDK does not encrypt, so the samples
/// would cross the relay in the clear, and only where the samples can be read.
#[uniffi::export]
pub fn sound_lossless_offered(wanted: bool, encrypted: bool, able: bool) -> bool {
    wanted && !encrypted && able
}

/// Whether a share's picture takes its sound with it: unless sound is already
/// being shared on its own, which carries on and is not doubled (room.ts
/// `share`, `alongside`).
#[uniffi::export]
pub fn share_sound_alongside(listening_now: bool) -> bool {
    !listening_now
}

/* -------------------------------------------------------------------------- */
/* The cushion under a share received                                         */
/* -------------------------------------------------------------------------- */

/// What every share received is held by, unless the switch says otherwise.
///
/// A fifth of a second gives a packet lost on the way time to be asked for
/// again and arrive before its frame is due, which took most of the freezing out
/// of the shares measured on bad lines (delay.ts, 2026-10-13 and 14). It needs
/// the relays' larger video packet memory (4000 packets, dev/relay.yaml); the
/// two are one change or neither.
pub const SHARE_PLAYOUT_DEFAULT_MS: u32 = 200;

/// More than this is not a cushion but a broadcast.
pub const SHARE_PLAYOUT_MOST_MS: u32 = 1000;

/// The cushion, from the switch as it was written down (delay.ts `shareDelay`):
/// nought turns it off, a whole number from 1 to 1000 replaces the default, and
/// anything else (a word, a fraction, more than a second, nothing) says
/// nothing, so the default stands. A stray value is not somebody asking for it
/// off.
#[uniffi::export]
pub fn share_playout_ms(kept: Option<String>) -> u32 {
    let Some(kept) = kept else { return SHARE_PLAYOUT_DEFAULT_MS };
    if kept.trim_matches(js_space).is_empty() {
        return SHARE_PLAYOUT_DEFAULT_MS;
    }

    let ms = js_number(&kept);
    if ms == 0.0 {
        return 0;
    }
    if ms.fract() == 0.0 && ms > 0.0 && ms <= SHARE_PLAYOUT_MOST_MS as f64 {
        ms as u32
    } else {
        SHARE_PLAYOUT_DEFAULT_MS
    }
}

/// What a remote track's receiver is held by, in milliseconds, or nothing where
/// it is left alone (delay.ts `cushionShares`).
///
/// A share's picture and its sound, together, so the two stay in step; sound
/// shared on its own is a share's sound by source, and is held too. Never a
/// camera or a microphone: the conversation itself is not made later for the
/// sake of the screen, which is the cost the owner did not accept. The voice
/// explaining a share therefore runs that much ahead of its picture. With the
/// switch off nothing is touched at all, rather than set to nought.
///
/// The Rust SDK takes it in seconds (`RemoteTrack::set_playout_delay`); the
/// browser's `jitterBufferTarget` takes milliseconds.
#[uniffi::export]
pub fn share_cushion_ms(source: MediaSource, ms: u32) -> Option<u32> {
    let held = matches!(source, MediaSource::ScreenShare | MediaSource::ScreenShareAudio);
    (held && ms > 0).then_some(ms)
}

/* -------------------------------------------------------------------------- */
/* Reading what was written down                                              */
/* -------------------------------------------------------------------------- */

fn js_space(c: char) -> bool {
    c.is_whitespace() || c == '\u{feff}'
}

/// JavaScript's `Number(text)`, for the switches the web reads that way.
///
/// The cases that differ from Rust's own parse are the ones handled: blank is
/// nought, the 0x, 0o and 0b forms are integers, "Infinity" is the only word,
/// and Rust's "inf" and "nan" are not numbers.
fn js_number(text: &str) -> f64 {
    let text = text.trim_matches(js_space);
    if text.is_empty() {
        return 0.0;
    }

    for (prefix, radix) in [("0x", 16), ("0X", 16), ("0o", 8), ("0O", 8), ("0b", 2), ("0B", 2)] {
        if let Some(digits) = text.strip_prefix(prefix) {
            if digits.is_empty() {
                return f64::NAN;
            }
            let mut value = 0.0;
            for c in digits.chars() {
                match c.to_digit(radix) {
                    Some(digit) => value = value * radix as f64 + digit as f64,
                    None => return f64::NAN,
                }
            }
            return value;
        }
    }

    match text {
        "Infinity" | "+Infinity" => return f64::INFINITY,
        "-Infinity" => return f64::NEG_INFINITY,
        _ => {}
    }

    if !text.chars().all(|c| c.is_ascii_digit() || matches!(c, '+' | '-' | '.' | 'e' | 'E')) {
        return f64::NAN;
    }
    text.parse().unwrap_or(f64::NAN)
}

#[cfg(test)]
mod tests {
    use super::js_number;

    #[test]
    fn reads_numbers_the_way_javascript_does() {
        assert_eq!(js_number(" 60 "), 60.0);
        assert_eq!(js_number(""), 0.0);
        assert_eq!(js_number("0x64"), 100.0);
        assert_eq!(js_number("1e2"), 100.0);
        assert!(js_number("inf").is_nan());
        assert!(js_number("nan").is_nan());
        assert!(js_number("on").is_nan());
        assert!(js_number("0x+5").is_nan());
        assert_eq!(js_number("Infinity"), f64::INFINITY);
    }
}
