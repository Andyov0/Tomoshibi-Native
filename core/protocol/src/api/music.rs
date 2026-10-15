//! The music library: which libraries there are, searching them, what a
//! track's audio is, reading a shared link, and where the audio is played from.
//!
//! Follows web/src/live/music.ts; the server is internal/app/music.go, which
//! passes a signed-in person's request on to a gateway the client never sees.
//! The answers are read defensively and the shapes that matter are pinned:
//! "this account may not play it" is told apart from "the library broke", and
//! the quality said is what the file is.

use std::sync::Arc;

use reqwest::Method;
use serde_json::Value;

use super::{
    read::{number, text, yes},
    text::utf16_prefix,
    Answer, Api, Deployment,
};
use crate::{runtime::on_core, ProtocolError};

/// One library the deployment has a gateway to.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct MusicLibrary {
    pub id: String,
    pub name: String,
    /// Signed out, a library plays at most 320 kbit/s.
    pub signed_in: bool,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct MusicTrack {
    pub id: String,
    pub title: String,
    pub artists: Vec<String>,
    pub album: String,
    pub cover: String,
    /// Seconds.
    pub duration: f64,
}

/// What a track's audio is, read off the file by the library.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct MusicAudio {
    pub format: String,
    /// Samples a second; nought where the library could not tell.
    pub rate: u32,
    pub channels: u32,
    pub bits: u32,
    /// `master`, `hires`, `lossless`, `320k`, `128k`.
    pub tier: String,
}

/// Which file to ask for: the best the library's account may have, or the
/// best of one kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum MusicQuality {
    Best,
    Lossless,
    Lossy,
}

impl MusicQuality {
    pub(crate) fn said(self) -> &'static str {
        match self {
            Self::Best => "best",
            Self::Lossless => "lossless",
            Self::Lossy => "lossy",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum MusicLinkKind {
    Playlist,
    Song,
}

/// What a pasted link names: a playlist or a song, and its tracks.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct MusicLink {
    pub source: String,
    pub kind: MusicLinkKind,
    pub title: String,
    pub tracks: Vec<MusicTrack>,
}

/// `"FLAC · 24-bit · 44.1 kHz"`, or `"MP3 · 320 kbps"`: what is actually being
/// played (music.ts `describeAudio`).
#[uniffi::export]
pub fn describe_music_audio(audio: MusicAudio) -> String {
    let format = audio.format.to_uppercase();

    if audio.format != "flac" {
        return if audio.tier.ends_with('k') {
            format!("{format} · {}", audio.tier.replacen('k', " kbps", 1))
        } else {
            format
        };
    }

    let rate = if audio.rate == 0 {
        String::new()
    } else {
        let khz = f64::from(audio.rate) / 1000.0;
        if audio.rate.is_multiple_of(1000) {
            format!("{khz:.0} kHz")
        } else {
            // `toFixed(1)` rounds an exact half up where Rust's formatting
            // rounds it to even; 44.25 is the kind of rate where they part.
            let tenths = khz * 10.0;
            if tenths.fract() == 0.5 {
                format!("{:.1} kHz", (tenths.floor() + 1.0) / 10.0)
            } else {
                format!("{khz:.1} kHz")
            }
        }
    };
    let bits = if audio.bits == 0 {
        String::new()
    } else {
        format!("{}-bit", audio.bits)
    };

    [format, bits, rate]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" · ")
}

/// Minutes and seconds (music.ts `clock`).
#[uniffi::export]
pub fn music_clock(seconds: f64) -> String {
    // `Math.round` rounds a half up, toward positive infinity; and a time that
    // is not a number is nought rather than the web's "NaN:NaN".
    let whole = if seconds.is_finite() {
        let floor = seconds.floor();
        (if seconds - floor >= 0.5 {
            floor + 1.0
        } else {
            floor
        })
        .max(0.0)
    } else {
        0.0
    } as u64;

    format!("{}:{:02}", whole / 60, whole % 60)
}

fn read_track(value: &Value) -> MusicTrack {
    MusicTrack {
        id: text(value.get("id")),
        title: text(value.get("title")),
        artists: value
            .get("artists")
            .and_then(Value::as_array)
            .map(|all| {
                all.iter()
                    .map(|one| text(Some(one)))
                    .filter(|one| !one.is_empty())
                    .collect()
            })
            .unwrap_or_default(),
        album: text(value.get("album")),
        cover: text(value.get("cover")),
        duration: number(value.get("duration")),
    }
}

/// The tracks in an answer, dropping anything without an id or a title.
fn read_tracks(value: Option<&Value>) -> Vec<MusicTrack> {
    value
        .and_then(Value::as_array)
        .map(|all| {
            all.iter()
                .map(read_track)
                .filter(|one| !one.id.is_empty() && !one.title.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

/// A count from the library: any finite number, as music.ts reads it, held
/// to what a record can carry.
fn count(value: Option<&Value>) -> u32 {
    number(value).clamp(0.0, f64::from(u32::MAX)) as u32
}

impl Api {
    async fn music(&self, what: &str, pairs: &[(&str, &str)]) -> Result<Answer, ProtocolError> {
        let mut path = format!("/api/music/{what}");
        if !pairs.is_empty() {
            path.push('?');
            path.push_str(
                &url::form_urlencoded::Serializer::new(String::new())
                    .extend_pairs(pairs)
                    .finish(),
            );
        }

        self.send(self.request(Method::GET, &path)?).await
    }
}

#[uniffi::export]
impl Deployment {
    /// The libraries there are. A deployment without one answers `not_found`
    /// and somebody not signed in `not_signed_in`, which the web reads as no
    /// library and an app reads the same.
    pub async fn music_sources(self: Arc<Self>) -> Result<Vec<MusicLibrary>, ProtocolError> {
        on_core(async move {
            let answer = self.api.music("sources", &[]).await?.accepted("")?;
            let body = answer.value().unwrap_or(Value::Null);
            Ok(body
                .get("sources")
                .and_then(Value::as_array)
                .map(|all| {
                    all.iter()
                        .map(|one| MusicLibrary {
                            id: text(one.get("id")),
                            name: text(one.get("name")),
                            signed_in: yes(one.get("signedIn")),
                        })
                        .filter(|one| !one.id.is_empty() && !one.name.is_empty())
                        .collect()
                })
                .unwrap_or_default())
        })
        .await
    }

    /// Searches one library; `page` counts from one.
    pub async fn music_search(
        self: Arc<Self>,
        source: String,
        query: String,
        page: u32,
    ) -> Result<Vec<MusicTrack>, ProtocolError> {
        on_core(async move {
            let page = page.to_string();
            let answer = self
                .api
                .music(
                    "search",
                    &[("source", &source), ("q", &query), ("page", &page)],
                )
                .await?
                .accepted("")?;
            Ok(read_tracks(
                answer.value().unwrap_or(Value::Null).get("tracks"),
            ))
        })
        .await
    }

    /// What a track's audio is, or `None` where this library's account may not
    /// play it, which is a different thing from the library failing.
    pub async fn music_track(
        self: Arc<Self>,
        source: String,
        id: String,
        quality: MusicQuality,
    ) -> Result<Option<MusicAudio>, ProtocolError> {
        on_core(async move {
            let answer = self
                .api
                .music(
                    "track",
                    &[
                        ("source", &source),
                        ("id", &id),
                        ("quality", quality.said()),
                    ],
                )
                .await?;
            if answer.status == 404 && !answer.absent() {
                return Ok(None);
            }
            let body = answer.accepted("")?.json::<Value>("the track")?;
            Ok(Some(MusicAudio {
                format: text(body.get("format")),
                rate: count(body.get("rate")),
                channels: Some(count(body.get("channels")))
                    .filter(|n| *n != 0)
                    .unwrap_or(2),
                bits: count(body.get("bits")),
                tier: text(body.get("tier")),
            }))
        })
        .await
    }

    /// Reads a playlist or song link as somebody shared it, a bare address or
    /// the sentence a music app wraps around one; `None` for anything else.
    pub async fn music_link(
        self: Arc<Self>,
        pasted: String,
    ) -> Result<Option<MusicLink>, ProtocolError> {
        on_core(async move {
            let answer = self
                .api
                .music("link", &[("text", &utf16_prefix(&pasted, 2000))])
                .await?;
            if answer.status == 404 && !answer.absent() {
                return Ok(None);
            }
            let body = answer.accepted("")?.json::<Value>("the link")?;
            Ok(Some(MusicLink {
                source: text(body.get("source")),
                kind: if body.get("kind").and_then(Value::as_str) == Some("song") {
                    MusicLinkKind::Song
                } else {
                    MusicLinkKind::Playlist
                },
                title: text(body.get("title")),
                tracks: read_tracks(body.get("tracks")),
            }))
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn audio(format: &str, rate: u32, bits: u32, tier: &str) -> MusicAudio {
        MusicAudio {
            format: format.into(),
            rate,
            channels: 2,
            bits,
            tier: tier.into(),
        }
    }

    // music.test.ts, "is said as what is played".
    #[test]
    fn says_what_is_played() {
        assert_eq!(
            describe_music_audio(audio("flac", 44100, 24, "lossless")),
            "FLAC · 24-bit · 44.1 kHz"
        );
        assert_eq!(
            describe_music_audio(audio("flac", 96000, 24, "hires")),
            "FLAC · 24-bit · 96 kHz"
        );
        assert_eq!(
            describe_music_audio(audio("mp3", 44100, 16, "320k")),
            "MP3 · 320 kbps"
        );
        assert_eq!(
            describe_music_audio(audio("flac", 44250, 0, "lossless")),
            "FLAC · 44.3 kHz"
        );
        assert_eq!(describe_music_audio(audio("aac", 0, 0, "master")), "AAC");
    }

    // music.test.ts, the clock half of the last case.
    #[test]
    fn says_minutes_and_seconds() {
        assert_eq!(music_clock(269.0), "4:29");
        assert_eq!(music_clock(5.0), "0:05");
        assert_eq!(music_clock(2.5), "0:03");
        assert_eq!(music_clock(-4.0), "0:00");
    }

    #[test]
    fn keeps_what_a_track_needs_and_drops_what_is_not_one() {
        let said = serde_json::json!([
            { "id": "1", "title": "Sunny Day", "artists": ["Jay", 3, ""], "album": "Ye Hui Mei", "cover": "c", "duration": 269 },
            { "id": "", "title": "no id" },
            { "title": "no id either" },
        ]);

        assert_eq!(
            read_tracks(Some(&said)),
            vec![MusicTrack {
                id: "1".into(),
                title: "Sunny Day".into(),
                artists: vec!["Jay".into()],
                album: "Ye Hui Mei".into(),
                cover: "c".into(),
                duration: 269.0,
            }]
        );
    }
}
