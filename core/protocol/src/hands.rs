//! Raised hands, reactions, and who is watching a screen share.
//!
//! Follows web/src/live/hands.ts and web/src/live/watching.ts, and the one
//! place watching.ts is fed from (web/src/routes/Room.tsx, `watched`).
//!
//! Two kinds of thing travel here, and the web is careful to send each the way
//! its kind needs, so a native participant has to send them the same way or a
//! browser in the room sees the difference:
//!
//! - A raised hand is state: true until lowered, visible to somebody who joins
//!   ten minutes later, gone when the participant goes. It is the participant
//!   attribute `hand`, "up" or "".
//! - A reaction is an event: seen, then over; nobody arriving later should see
//!   it. It is a reliable data message on the topic `reaction` whose payload is
//!   the glyph itself, UTF-8, from a fixed list of six.
//! - Who is looking at a share is state again, for the same reasons as a hand:
//!   the attribute `watching` holds the identity of the sharer whose share is
//!   on this participant's stage, or "". The media server tells a publisher
//!   nothing about who subscribed, so the people looking say so, and the sharer
//!   reads it off the roster.
//!
//! Attributes are broadcast to the whole room, so both are written only when
//! they change: writing the same value again is a message to everybody that
//! says nothing, and doing it per render is a call that slows down the longer
//! it runs.
//!
//! Left to the apps, as the web leaves it to React: holding each reaction on
//! screen for [`REACTION_SHOWN_FOR`] and showing the sender their own at once
//! (data does not come back to whoever sent it), and recomputing the lists on
//! roster events as well as attribute ones, since somebody who leaves stops
//! watching without any attribute saying so.

use std::{collections::HashMap, time::Duration};

use crate::ProtocolError;

/// The participant attribute a raised hand is kept in (hands.ts, `RAISED`).
pub const HAND: &str = "hand";

/// The value of [`HAND`] while the hand is up; lowered is "".
pub const HAND_UP: &str = "up";

/// The data topic reactions travel on (hands.ts, `REACTION`).
pub const REACTION_TOPIC: &str = "reaction";

/// Reactions are published reliably, as the web publishes them.
pub const REACTION_RELIABLE: bool = true;

/// The reactions a deployment offers, in the web's order (hands.ts,
/// `REACTIONS`).
///
/// Written as escapes because the bytes are the protocol: the heart is U+2764
/// followed by the emoji variation selector U+FE0F, and a heart without it is a
/// different string that every browser drops as not on the list.
pub const REACTIONS: [&str; 6] = [
    "\u{1F44D}",
    "\u{1F44F}",
    "\u{1F602}",
    "\u{1F389}",
    "\u{2764}\u{FE0F}",
    "\u{1F62E}",
];

/// How long a reaction stays on screen (hands.ts, `REACTION_FOR`).
pub const REACTION_SHOWN_FOR: Duration = Duration::from_millis(4000);

/// The participant attribute that says whose share is on this participant's
/// stage (watching.ts, `key`).
pub const WATCHING: &str = "watching";

/// One participant as far as these rules need: who, and their attributes.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct RosterMember {
    pub identity: String,
    pub attributes: HashMap<String, String>,
}

/// What is on this participant's stage, as much of it as `watching` needs.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct StagePicture {
    /// The identity of whoever the picture belongs to.
    pub owner: String,
    /// Whether it is a screen share, rather than a camera or the video watched
    /// together.
    pub screen: bool,
    /// Whether it is this participant's own.
    pub local: bool,
}

/// Whether a participant's attributes say their hand is up.
///
/// Exactly "up"; anything else, absent included, is down.
#[uniffi::export]
pub fn hand_is_up(attributes: HashMap<String, String>) -> bool {
    attributes.get(HAND).is_some_and(|value| value == HAND_UP)
}

/// Whose hands are up, in roster order (hands.ts, `useHands`).
///
/// Read off the roster rather than collected from messages, so it is right for
/// somebody who has just arrived and right again after a reconnect. The roster
/// is the web's: this participant first, then everybody else.
#[uniffi::export]
pub fn raised_hands(roster: Vec<RosterMember>) -> Vec<String> {
    roster
        .into_iter()
        .filter(|member| member.attributes.get(HAND).is_some_and(|value| value == HAND_UP))
        .map(|member| member.identity)
        .collect()
}

/// The value to write to [`HAND`] to put this participant's hand up or down,
/// or `None` when it already says so and nothing should be sent.
///
/// `current` is this participant's own `hand` attribute as it stands. The web
/// compares "is it up" with "should it be up", so a value that is neither "up"
/// nor "" counts as down and is not rewritten to lower it.
#[uniffi::export]
pub fn hand_to_write(current: Option<String>, up: bool) -> Option<String> {
    let already = current.as_deref() == Some(HAND_UP);
    if already == up {
        return None;
    }

    Some(if up { HAND_UP } else { "" }.to_owned())
}

/// The reactions on offer, in order, for the picker.
#[uniffi::export]
pub fn reaction_choices() -> Vec<String> {
    REACTIONS.iter().map(|one| (*one).to_owned()).collect()
}

/// How long a reaction stays on screen, in milliseconds.
#[uniffi::export]
pub fn reaction_shown_for_ms() -> u64 {
    REACTION_SHOWN_FOR.as_millis() as u64
}

/// The payload to publish on [`REACTION_TOPIC`] for `what`.
///
/// The web's picker can only produce one of the six; this refuses anything
/// else, because a browser would drop it and the sender would be shown a
/// reaction nobody else saw.
#[uniffi::export]
pub fn reaction_payload(what: String) -> Result<Vec<u8>, ProtocolError> {
    if REACTIONS.contains(&what.as_str()) {
        Ok(what.into_bytes())
    } else {
        Err(ProtocolError::Invalid { message: format!("{what:?} is not one of the reactions") })
    }
}

/// The reaction a received data message carries, or `None` when it is not a
/// reaction to show (hands.ts, `useReactions`).
///
/// Only on the reaction topic and only from a participant: data the server
/// sends on its own behalf has no sender, and the web ignores it. The payload
/// is decoded as the browser's `TextDecoder` decodes it, which drops a leading
/// byte order mark, and then has to be one of the six exactly. A tile is not a
/// place to render whatever text somebody else's client put on the wire.
#[uniffi::export]
pub fn received_reaction(topic: Option<String>, from: Option<String>, payload: Vec<u8>) -> Option<String> {
    if topic.as_deref() != Some(REACTION_TOPIC) || from.is_none() {
        return None;
    }

    let text = payload.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(&payload);
    // Bytes that are not UTF-8 decode to replacement characters in a browser,
    // which are not on the list either, so refusing them outright drops exactly
    // what the web drops.
    let text = std::str::from_utf8(text).ok()?;

    REACTIONS.iter().find(|one| **one == text).map(|one| (*one).to_owned())
}

/// Whose share this participant is looking at, as [`WATCHING`] should say it
/// (Room.tsx, `watched`): the owner of a screen share that is on the stage and
/// is not this participant's own, and "" otherwise.
///
/// Only the stage counts. A share sitting in the strip as a card is something
/// nobody has opened, and counting it would tell the sharer twelve people are
/// watching a picture nobody has looked at.
#[uniffi::export]
pub fn watched_sharer(stage: Option<StagePicture>) -> String {
    match stage {
        Some(picture) if picture.screen && !picture.local => picture.owner,
        _ => String::new(),
    }
}

/// The value to write to [`WATCHING`], or `None` when it already says so
/// (watching.ts, `useWatching`).
///
/// `current` is this participant's own attribute as it stands, absent read as
/// "", and `sharer` what [`watched_sharer`] says now.
#[uniffi::export]
pub fn watching_to_write(current: Option<String>, sharer: String) -> Option<String> {
    (current.unwrap_or_default() != sharer).then_some(sharer)
}

/// Everybody who says they are looking at this participant's share, in roster
/// order (watching.ts, `useWatchers`).
///
/// Nobody while this participant is not sharing a screen, and never this
/// participant: the list is the people who set [`WATCHING`] to `me`.
#[uniffi::export]
pub fn watchers(me: String, sharing: bool, roster: Vec<RosterMember>) -> Vec<String> {
    if !sharing {
        return Vec::new();
    }

    roster
        .into_iter()
        .filter(|member| member.identity != me && member.attributes.get(WATCHING) == Some(&me))
        .map(|member| member.identity)
        .collect()
}
