//! Sound carried without loss.
//!
//! Opus is a model of what a listener will notice, and what it sends is its own
//! reconstruction. Somebody who asked for the sound they share to arrive as it
//! left cannot be given that on a media track at all, so beside the Opus track,
//! which always goes out, the sharer streams the exact samples as data over the
//! call's reliable data channel, to the listeners who ask for them and to
//! nobody else. A listener playing that stream holds the Opus track back at the
//! media server, so the same song is neither downloaded nor heard twice.
//!
//! A port of the web client, file for file, because a browser in the same room
//! is on the other end of every packet:
//!
//! - This file follows the top of web/src/live/lossless.ts: the topics, the
//!   track names and the two channels they make up.
//! - [`format`](mod@format) follows the rest of lossless.ts: the packet, the
//!   FLAC-like coding inside it, and the float conversions.
//! - [`playout`] follows web/src/live/playout.ts and playout.worklet.ts: the
//!   buffer a listener plays from, which waits and drops but never invents.
//! - [`sender`] follows web/src/live/lossless-sender.ts: who has asked, who is
//!   sent to, and when an upload that cannot keep up is given up on.
//! - [`receiver`] follows web/src/live/lossless-receiver.ts: asking, playing,
//!   and every way a listener decides the stream has failed and Opus is back.
//!
//! Nothing here touches media or the network. The sender and the receiver are
//! state machines with the clock passed in, in milliseconds: the media crate
//! hands them what arrived (a chunk of capture, a data packet, an ask, a tick)
//! and publishes what they return. Every data packet either side sends is
//! reliable and addressed, never broadcast; see [`Outgoing`].

pub mod format;
pub mod playout;
pub mod receiver;
pub mod sender;

pub use format::{
    decoded_to_int24, from_int24, pack, to_int24, unpack, Block, BOTTOM, FRAMES, TOP,
};
pub use playout::{ceiling_frames, target_frames, Playout, PlayoutCounts, Worklet};
pub use receiver::{Hearing, LosslessReceiver, MakePlayer, Outcome, Player, Publication, Remote};
pub use sender::{Captured, LosslessSender};

/// Data-packet topics of the shared sound: one for the sound, one for asking.
pub const TOPIC: &str = "lossless";
pub const ASK_TOPIC: &str = "lossless-ask";

/// The name the Opus track of a lossless share is published under.
///
/// The track goes out either way: it is what everybody on an older page hears,
/// and what anybody hears if the lossless stream fails. Its name is how a
/// listener learns there is a lossless stream to ask for.
pub const LOSSLESS: &str = "listening-lossless";

/// How often a listener repeats its ask, in milliseconds; an ask lasts three of these.
pub const ASK_EVERY_MS: u64 = 5000;

/// Which of the two kinds of sound shared by itself a stream is.
///
/// The web client's settings call them "screen" and "music" (hearing.ts); a
/// listener can hear both from one person at once, losslessly, and turn either
/// down without the other. Exported to the apps as `LosslessSound`, since a
/// bare `Sound` there would claim a name the voice has as good a right to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, uniffi::Enum)]
#[uniffi(name = "LosslessSound")]
pub enum Sound {
    Screen,
    Music,
}

/// The source a track is published with, as LiveKit names them.
///
/// Only two matter here, but the media crate maps all of them, so a camera or
/// a microphone is told apart from the Unknown source the music uses rather
/// than folded into it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TrackSource {
    Camera,
    Microphone,
    ScreenShare,
    ScreenShareAudio,
    Unknown,
}

/// One way of sharing sound by itself, and everything that tells it apart.
///
/// Sound captured from an application and the song desk's music are kept apart
/// from top to bottom. They used to be one in the web client, and the SDK keeps
/// one screen-share-audio track per person: starting a screen share while a
/// song played sent the screen without its sound, and stopping it took the
/// song with it. So the music has a source of its own, Unknown, and its own
/// track names and topics (lossless.ts, `Channel`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Channel {
    pub sound: Sound,
    pub source: TrackSource,
    /// The Opus track's name when no lossless stream is offered beside it.
    pub opus: &'static str,
    /// Its name when one is: how a listener learns there is one to ask for.
    pub lossless: &'static str,
    /// The topic the packets travel on.
    pub topic: &'static str,
    /// The topic a listener asks on.
    pub ask: &'static str,
}

/// Sound shared from an application or a tab. The names it has always had.
pub const SHARED: Channel = Channel {
    sound: Sound::Screen,
    source: TrackSource::ScreenShareAudio,
    opus: "listening",
    lossless: LOSSLESS,
    topic: TOPIC,
    ask: ASK_TOPIC,
};

/// The song desk's music.
pub const MUSIC: Channel = Channel {
    sound: Sound::Music,
    source: TrackSource::Unknown,
    opus: "music",
    lossless: "music-lossless",
    topic: "lossless-music",
    ask: "lossless-music-ask",
};

/// Both channels, in the web client's order, which is the order a refresh
/// visits them in and so the order its asks go out in. A static rather than a
/// constant, so a channel found in it can be held for as long as a stream is.
pub static CHANNELS: [Channel; 2] = [SHARED, MUSIC];

impl Channel {
    /// The channel a kind of sound travels on.
    pub fn of(sound: Sound) -> &'static Channel {
        match sound {
            Sound::Screen => &CHANNELS[0],
            Sound::Music => &CHANNELS[1],
        }
    }

    /// The channel whose packets travel on `topic`, if any does.
    pub fn by_topic(topic: &str) -> Option<&'static Channel> {
        CHANNELS.iter().find(|channel| channel.topic == topic)
    }

    /// The channel whose asks travel on `topic`, if any does.
    pub fn by_ask(topic: &str) -> Option<&'static Channel> {
        CHANNELS.iter().find(|channel| channel.ask == topic)
    }
}

/// Whether a publication is sound shared on this channel, in either form.
pub fn on_channel(channel: &Channel, source: TrackSource, track_name: &str) -> bool {
    source == channel.source && (track_name == channel.opus || track_name == channel.lossless)
}

/// A data packet to publish: reliable, on `topic`, to exactly the identities in `to`.
///
/// Never broadcast. Sending only to whoever asked is what keeps the stream
/// from costing anything where it is not used, and an ask is meant for one
/// sharer only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outgoing {
    pub topic: &'static str,
    pub to: Vec<String>,
    pub payload: Vec<u8>,
}
