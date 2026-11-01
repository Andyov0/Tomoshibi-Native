//! Switches for driving watching together from a test, in a debug build and
//! nowhere else (the Mac's WatchDebug, whose names these are). None changes
//! what crosses the call: what the stand-in reads is carried by the show as
//! a gateway's ticket would be.
//!
//! - `TOMOSHIBI_DEBUG_WATCH_LINKS=1` stands in for a deployment's watch
//!   gateway, for one that has none: this person may start a show, and a
//!   link to a file or a playlist on this machine (127.0.0.1 or localhost,
//!   or a host named in `TOMOSHIBI_DEBUG_WATCH_HOSTS`, comma separated, for
//!   a test whose files are served from a neighbouring container) reads as
//!   a video that plays from there; anything else is "not a video". Nothing
//!   anywhere else is ever fetched. `?duration=<seconds>` on the link gives
//!   the video its length, as a gateway's answer says it.
//! - `TOMOSHIBI_DEBUG_WATCH_LOG=<file>` gets a line at every keep step: the
//!   wall clock in milliseconds, where the player is, whether it plays, and
//!   where the show is.

use std::{cell::RefCell, collections::HashMap, io::Write};

use tomoshibi_protocol::{
    api::{ResolvedLink, ResolvedPlay, ResolvedPlayKind, ResolvedVideo, UnresolvedReason},
    watch::WatchPlayerReading,
};

use super::Info;

thread_local! {
    /// The stand-in's tickets, by the ticket, with the link each was made for.
    static TICKETS: RefCell<HashMap<String, String>> = RefCell::new(HashMap::new());
}

pub fn links() -> bool {
    std::env::var_os("TOMOSHIBI_DEBUG_WATCH_LINKS").is_some()
}

fn hosts() -> Vec<String> {
    let mut hosts = vec!["127.0.0.1".to_owned(), "localhost".to_owned()];
    if let Ok(more) = std::env::var("TOMOSHIBI_DEBUG_WATCH_HOSTS") {
        hosts.extend(
            more.split(',')
                .map(|host| host.trim().to_lowercase())
                .filter(|host| !host.is_empty()),
        );
    }
    hosts
}

/// What the stand-in makes of a pasted link.
pub fn resolve(text: &str) -> ResolvedLink {
    let link = text.trim();
    let not = ResolvedLink::NotPlayable {
        reason: UnresolvedReason::NotAVideo,
    };
    let Some(rest) = link
        .strip_prefix("http://")
        .or_else(|| link.strip_prefix("https://"))
    else {
        return not;
    };
    let (authority, path) = rest.split_once('/').unwrap_or((rest, ""));
    let host = authority
        .rsplit('@')
        .next()
        .unwrap_or_default()
        .split(':')
        .next()
        .unwrap_or_default()
        .to_lowercase();
    if !hosts().contains(&host) {
        return not;
    }
    let (path, query) = path.split_once('?').unwrap_or((path, ""));
    let extension = path
        .rsplit_once('.')
        .map(|(_, extension)| extension.to_lowercase())
        .unwrap_or_default();
    if !["mp4", "m4v", "mov", "m3u8", "webm", "mkv", "ogv", "ogg"].contains(&extension.as_str()) {
        return not;
    }
    // FNV-1a, so the same link is the same ticket for the whole run.
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in link.bytes() {
        hash = (hash ^ byte as u64).wrapping_mul(0x0000_0100_0000_01b3);
    }
    let ticket = format!("debugwatch-{hash:016x}");
    TICKETS.with(|tickets| tickets.borrow_mut().insert(ticket.clone(), link.to_owned()));
    let duration = query
        .split('&')
        .find_map(|pair| pair.strip_prefix("duration="))
        .and_then(|value| value.parse::<f64>().ok())
        .unwrap_or(0.0);
    let title = path
        .rsplit('/')
        .next()
        .and_then(|file| file.rsplit_once('.').map(|(name, _)| name.to_owned()))
        .unwrap_or_default();
    let hls = extension == "m3u8";
    ResolvedLink::Playable {
        video: ResolvedVideo {
            ticket,
            title,
            duration,
            cover: String::new(),
            link: link.to_owned(),
            live: false,
            relay: false,
            play: ResolvedPlay {
                kind: if hls {
                    ResolvedPlayKind::Hls
                } else {
                    ResolvedPlayKind::File
                },
                url: Some(link.to_owned()),
                id: None,
            },
            proxy: None,
            grantor: None,
        },
    }
}

/// What a stand-in ticket plays, or nothing for a ticket that is not one.
pub fn info(ticket: &str) -> Option<Info> {
    if !links() {
        return None;
    }
    let link = TICKETS.with(|tickets| tickets.borrow().get(ticket).cloned())?;
    match resolve(&link) {
        ResolvedLink::Playable { video } => Some(Info::Known(video)),
        ResolvedLink::NotPlayable { .. } => Some(Info::Failed),
    }
}

/// A line of the keep log, where a test asked for one.
pub fn log_step(reading: &WatchPlayerReading, playing: bool, expected: f64) {
    let Ok(path) = std::env::var("TOMOSHIBI_DEBUG_WATCH_LOG") else {
        return;
    };
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.as_millis())
        .unwrap_or_default();
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let _ = writeln!(
            file,
            "{stamp} ready {} at {:.3} playing {} show {} expected {:.3}",
            reading.ready, reading.time, reading.playing, playing, expected
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_local_video_is_one() {
        assert!(matches!(
            resolve("http://127.0.0.1:8000/clip.mp4?duration=6"),
            ResolvedLink::Playable { video } if video.duration == 6.0 && video.title == "clip"
                && video.ticket.starts_with("debugwatch-")
        ));
        assert!(matches!(
            resolve("http://localhost/live/index.m3u8"),
            ResolvedLink::Playable { video } if video.play.kind == ResolvedPlayKind::Hls
        ));
        for elsewhere in [
            "https://www.youtube.com/watch?v=x",
            "http://example.org/clip.mp4",
            "http://127.0.0.1/page.html",
            "file:///clip.mp4",
        ] {
            assert!(
                matches!(resolve(elsewhere), ResolvedLink::NotPlayable { .. }),
                "{elsewhere}"
            );
        }
    }

    #[test]
    fn a_stand_in_ticket_is_one_the_protocol_takes() {
        let ResolvedLink::Playable { video } = resolve("http://127.0.0.1/a.mp4") else {
            panic!("not a video");
        };
        assert!(tomoshibi_protocol::watch::is_ticket(&video.ticket));
    }
}
