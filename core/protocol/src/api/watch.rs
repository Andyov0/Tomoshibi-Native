//! The watch gateway, as a client sees it: whether there is one, what a pasted
//! link plays, what a ticket plays, and the media servers a deployment has an
//! account on.
//!
//! Follows web/src/live/watch-api.ts; the server is internal/app/watch.go and
//! watchledger.go. Every answer comes from a gateway the client never sees,
//! through the server, and what it names is fetched and played here, so each is
//! read as defensively as the web reads it: an address that is not http, a
//! relay that is not this server's own relay path, a YouTube id that is not one,
//! are refused rather than used. The relay paths the web keeps relative to its
//! own origin are made absolute on the deployment's.
//!
//! Two response headers matter and both are read off each answer rather than
//! once: `X-Watch-Relay: off` says the deployment will not relay this video,
//! and `X-Watch-Grantor` names whose grant a media server's film is relayed
//! under, which the holder of a show checks a handed-over ticket against.

use std::sync::Arc;

use reqwest::Method;
use serde_json::Value;

use super::{
    read::{positive, text, whole, yes},
    text::{js_trim, utf16_prefix, word_dash},
    unexpected, Answer, Api, Deployment,
};
use crate::{runtime::on_core, ProtocolError};

/// How a video plays from where it is (watch-api.ts `Playable.kind`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum ResolvedPlayKind {
    /// A file a video element plays.
    File,
    /// An HLS playlist.
    Hls,
    /// A YouTube video, by its id.
    Youtube,
    /// Only through the relay: a media server's film, whose every address
    /// carries the deployment's account.
    Relay,
    /// Only through the relay: a Bilibili bangumi episode, whose whole is
    /// handed only to a request carrying Bilibili's own Referer.
    Bangumi,
}

impl ResolvedPlayKind {
    fn read(said: &str) -> Option<Self> {
        Some(match said {
            "file" => Self::File,
            "hls" => Self::Hls,
            "youtube" => Self::Youtube,
            "relay" => Self::Relay,
            "bangumi" => Self::Bangumi,
            _ => return None,
        })
    }

    /// watch-api.ts `relayOnly`.
    fn relay_only(self) -> bool {
        matches!(self, Self::Relay | Self::Bangumi)
    }
}

/// How a video plays from where it is: an address for a file or a playlist,
/// an id for YouTube, nothing for what plays only through the relay.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ResolvedPlay {
    pub kind: ResolvedPlayKind,
    pub url: Option<String>,
    pub id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum ResolvedProxyKind {
    File,
    Hls,
}

/// The same video relayed by the deployment, for whoever cannot reach it.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ResolvedProxy {
    pub kind: ResolvedProxyKind,
    /// `{base}/api/watch/media?t=<ticket>`, and nothing else.
    pub url: String,
}

/// What a link plays, as the gateway read it (watch-api.ts `Resolved`).
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct ResolvedVideo {
    /// The name the gateway filed it under, and the only thing about it a call
    /// passes around: everybody else turns it back into the rest by asking the
    /// server (`video_info`).
    pub ticket: String,
    pub title: String,
    /// Seconds; nought where nobody could tell, or for something live.
    pub duration: f64,
    /// A poster from the relay or from the open web over https, or empty.
    pub cover: String,
    /// The link as it was understood.
    pub link: String,
    pub live: bool,
    /// Played through the relay from the start, and from where it is only if
    /// the relay fails.
    pub relay: bool,
    pub play: ResolvedPlay,
    /// None on an answer the deployment will not relay.
    pub proxy: Option<ResolvedProxy>,
    /// Whose grant a media server's film is relayed under, as the server said.
    pub grantor: Option<String>,
}

/// Why a link plays nothing here (watch-api.ts `Unresolved`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum UnresolvedReason {
    NotAVideo,
    /// Read, and not playable here; or playable only through a relay this
    /// deployment does not run.
    Unavailable,
    /// A members' Bilibili episode.
    MembersOnly,
    /// Under DRM.
    CopyProtected,
    /// Not offered where the deployment reads it from.
    RegionLocked,
    /// Read as a show's, and answered as not there.
    NotFound,
    /// This account was not given watching together.
    NotAllowed,
    /// A media server's film, which is relayed only for somebody given that,
    /// and only for what they chose themselves.
    EmbyNotAllowed,
    Failed,
}

/// What resolving a pasted link came to.
// Unboxed although one side is much the larger, because a box is not
// something UniFFI can carry across to Swift.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, uniffi::Enum)]
pub enum ResolvedLink {
    Playable { video: ResolvedVideo },
    NotPlayable { reason: UnresolvedReason },
}

/// What a ticket plays, as the server says now (watch-api.ts `readInfo`).
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, uniffi::Enum)]
pub enum TicketVideo {
    Known {
        video: ResolvedVideo,
    },
    /// Read, and playable only through a relay the server will not run for it:
    /// a different thing to say from "could not be read, try again", because no
    /// retry plays it.
    Unavailable,
    /// Expired, refused, not a ticket, or not readable.
    Failed,
}

/// A media server the deployment has an account on.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct MediaServer {
    pub key: String,
    pub name: String,
}

/// A library, a folder, a series, a season, or something to watch.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct MediaItem {
    pub id: String,
    pub name: String,
    pub r#type: String,
    /// Something to open.
    pub folder: bool,
    /// Something to watch.
    pub playable: bool,
    pub year: u32,
    pub duration: f64,
    /// The poster, from the deployment, or empty. Cookie-gated.
    pub image: String,
}

/// What a library item is resolved as to play it (watch-api.ts `mediaLink`).
#[uniffi::export]
pub fn watch_media_link(server: String, item: String) -> String {
    format!("library:{server}/{item}")
}

/// watch-api.ts `TICKET`: what the gateway issues, and nothing longer.
pub(crate) fn is_ticket(ticket: &str) -> bool {
    word_dash(ticket, 16, 64)
}

/// watch-api.ts `RELAYED`: a ticket's media, relative to /api/watch/.
fn is_relayed(path: &str) -> bool {
    path.strip_prefix("media?t=").is_some_and(is_ticket)
}

/// watch-api.ts `IMAGE`: a poster the deployment fetches from a media server.
fn is_image(path: &str) -> bool {
    let Some(rest) = path.strip_prefix("library?op=image&server=") else {
        return false;
    };
    let Some((server, rest)) = rest.split_once("&id=") else {
        return false;
    };
    let Some((id, tag)) = rest.split_once("&tag=") else {
        return false;
    };

    word_dash(server, 1, usize::MAX)
        && !id.is_empty()
        && id.bytes().all(|b| b.is_ascii_hexdigit())
        && word_dash(tag, 0, usize::MAX)
}

/// watch-api.ts `relaysNothing`.
fn relays_nothing(answer: &Answer) -> bool {
    answer.header("X-Watch-Relay") == Some("off")
}

/// watch-api.ts `withGrantor`: kept as said, within reason, because it is
/// compared and never shown or fetched, and one dropped for its shape would
/// leave a relayed film with no grantor to check it against.
fn with_grantor(mut video: ResolvedVideo, answer: &Answer) -> ResolvedVideo {
    let grantor = answer
        .header("X-Watch-Grantor")
        .map(|said| utf16_prefix(js_trim(said), 64))
        .unwrap_or_default();
    if !grantor.is_empty() {
        video.grantor = Some(grantor);
    }

    video
}

/// watch-api.ts `readResolved`, with the relay's paths made absolute on
/// `base`. Without `relays`, what the deployment would have relayed is left
/// out: the relayed copy, a poster that comes only through it, and a source
/// that plays only through it, which is then nothing at all.
pub fn read_resolved(base: &url::Url, body: &Value, relays: bool) -> Option<ResolvedVideo> {
    let on_base = |path: &str| base.join(path).map(String::from).unwrap_or_default();
    let play = body.get("play");
    let proxy = body.get("proxy");

    let kind = ResolvedPlayKind::read(&text(play.and_then(|play| play.get("kind"))))?;
    let proxy_kind = match text(proxy.and_then(|proxy| proxy.get("kind"))).as_str() {
        "file" => ResolvedProxyKind::File,
        "hls" => ResolvedProxyKind::Hls,
        _ => return None,
    };
    let ticket = text(body.get("ticket"));
    if !is_ticket(&ticket) {
        return None;
    }

    // The ticket's own relay on this server and nothing else: an address the
    // gateway named anywhere would be one this app fetched for it.
    if text(proxy.and_then(|proxy| proxy.get("url"))) != format!("media?t={ticket}") {
        return None;
    }

    let play_url = text(play.and_then(|play| play.get("url")));
    let play_id = text(play.and_then(|play| play.get("id")));
    if kind == ResolvedPlayKind::Youtube && !word_dash(&play_id, 11, 11) {
        return None;
    }
    if matches!(kind, ResolvedPlayKind::File | ResolvedPlayKind::Hls)
        && !(play_url.starts_with("http://") || play_url.starts_with("https://"))
    {
        return None;
    }
    if kind.relay_only() && !relays {
        return None;
    }

    // A picture from the relay, which is a media server's poster fetched with
    // the deployment's account, or one on the open web; nothing else.
    let cover = text(body.get("cover"));
    let cover = if relays && is_relayed(&cover) {
        on_base(&format!("/api/watch/{cover}"))
    } else if cover.starts_with("https://") {
        cover
    } else {
        String::new()
    };

    let play = match kind {
        ResolvedPlayKind::Youtube => ResolvedPlay {
            kind,
            url: None,
            id: Some(play_id),
        },
        _ if kind.relay_only() => ResolvedPlay {
            kind,
            url: None,
            id: None,
        },
        _ => ResolvedPlay {
            kind,
            url: Some(play_url),
            id: None,
        },
    };

    Some(ResolvedVideo {
        title: utf16_prefix(&text(body.get("title")), 300),
        duration: positive(body.get("duration")),
        cover,
        link: text(body.get("link")),
        live: yes(body.get("live")),
        relay: relays && yes(body.get("relay")),
        play,
        proxy: relays.then(|| ResolvedProxy {
            kind: proxy_kind,
            url: on_base(&format!("/api/watch/media?t={ticket}")),
        }),
        grantor: None,
        ticket,
    })
}

/// watch-api.ts `withheldBy`: a 422 read for its reason.
fn withheld_by(answer: &Answer) -> UnresolvedReason {
    match answer.code().as_deref() {
        Some("members_only") => UnresolvedReason::MembersOnly,
        Some("copy_protected") => UnresolvedReason::CopyProtected,
        Some("region_locked") => UnresolvedReason::RegionLocked,
        Some("not_found") => UnresolvedReason::NotFound,
        _ => UnresolvedReason::Unavailable,
    }
}

/// watch-api.ts `refusedBy`: the two refusals of a person rather than a link.
fn refused_by(answer: &Answer) -> UnresolvedReason {
    match answer.code().as_deref() {
        Some("not_allowed") => UnresolvedReason::NotAllowed,
        Some("emby_not_allowed") => UnresolvedReason::EmbyNotAllowed,
        _ => UnresolvedReason::Failed,
    }
}

fn read_item(api: &Api, value: &Value) -> Option<MediaItem> {
    let id = text(value.get("id"));
    if id.is_empty() || id.len() > 64 || !id.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let image = text(value.get("image"));

    Some(MediaItem {
        id,
        name: utf16_prefix(&text(value.get("name")), 300),
        r#type: text(value.get("type")),
        folder: yes(value.get("folder")),
        playable: yes(value.get("playable")),
        year: whole(value.get("year")),
        duration: positive(value.get("duration")),
        image: if is_image(&image) {
            api.absolute(&format!("/api/watch/{image}"))
        } else {
            String::new()
        },
    })
}

impl Api {
    /// A request to the gateway's library, which the web sends with nothing
    /// but the session cookie.
    async fn library(&self, pairs: &[(&str, &str)]) -> Result<Value, ProtocolError> {
        let query = url::form_urlencoded::Serializer::new(String::new())
            .extend_pairs(pairs)
            .finish();
        let answer = self
            .send(self.request(Method::GET, &format!("/api/watch/library?{query}"))?)
            .await?
            .accepted("")?;

        answer.json::<Value>("the media library")
    }

    async fn items(&self, pairs: &[(&str, &str)]) -> Result<Vec<MediaItem>, ProtocolError> {
        let found = self.library(pairs).await?;
        let items = found
            .get("items")
            .and_then(Value::as_array)
            .ok_or_else(|| unexpected("a library answer without items"))?;

        Ok(items
            .iter()
            .filter_map(|one| read_item(self, one))
            .collect())
    }

    async fn resolve_video(
        &self,
        pasted: &str,
        for_other: bool,
    ) -> Result<ResolvedLink, ProtocolError> {
        let query = {
            let mut query = url::form_urlencoded::Serializer::new(String::new());
            query.append_pair("url", &utf16_prefix(pasted, 2000));
            // Said only when reading a link somebody else added to a show this
            // person holds: the server relays a media server's film only for
            // whoever chose it, and unsaid, a guest pasting one would spend the
            // holder's grant.
            if for_other {
                query.append_pair("for", "other");
            }
            query.finish()
        };

        let answer = self
            .send(self.request(Method::GET, &format!("/api/watch/resolve?{query}"))?)
            .await?;
        let not = |reason| Ok(ResolvedLink::NotPlayable { reason });

        if answer.absent() {
            // A deployment without a gateway, which the web never asks because
            // `watch_ready` said no; read as the link not being a video, it
            // would tell somebody something untrue about their link.
            return Err(answer.refused(""));
        }

        match answer.status {
            404 => return not(UnresolvedReason::NotAVideo),
            422 => return not(withheld_by(&answer)),
            403 => return not(refused_by(&answer)),
            _ if !answer.ok() => return not(UnresolvedReason::Failed),
            _ => {}
        }

        let Some(body) = answer.value().filter(Value::is_object) else {
            return not(UnresolvedReason::Failed);
        };
        let read = read_resolved(&self.base, &body, !relays_nothing(&answer));

        // Read, and playable only through a relay this deployment does not run.
        if read.is_none()
            && relays_nothing(&answer)
            && read_resolved(&self.base, &body, true).is_some()
        {
            return not(UnresolvedReason::Unavailable);
        }

        match read {
            Some(video) => Ok(ResolvedLink::Playable {
                video: with_grantor(video, &answer),
            }),
            None => not(UnresolvedReason::Failed),
        }
    }

    async fn video_info(&self, ticket: &str) -> Result<TicketVideo, ProtocolError> {
        // Never asked about where it is not a ticket: what arrives here came
        // from somebody else in the call.
        if !is_ticket(ticket) {
            return Ok(TicketVideo::Failed);
        }

        let query = url::form_urlencoded::Serializer::new(String::new())
            .append_pair("t", ticket)
            .finish();
        let answer = self
            .send(self.request(Method::GET, &format!("/api/watch/info?{query}"))?)
            .await?;

        if answer.absent() {
            return Err(answer.refused(""));
        }

        let body = answer
            .ok()
            .then(|| answer.value())
            .flatten()
            .filter(Value::is_object);
        let Some(body) = body else {
            return Ok(TicketVideo::Failed);
        };

        let read = read_resolved(&self.base, &body, !relays_nothing(&answer));
        let withheld = if read.is_none() && relays_nothing(&answer) {
            read_resolved(&self.base, &body, true)
        } else {
            None
        };

        // Only the ticket asked about: an answer naming another is not this one.
        Ok(match (read, withheld) {
            (Some(video), _) if video.ticket == ticket => TicketVideo::Known {
                video: with_grantor(video, &answer),
            },
            (None, Some(withheld)) if withheld.ticket == ticket => TicketVideo::Unavailable,
            _ => TicketVideo::Failed,
        })
    }
}

#[uniffi::export]
impl Deployment {
    /// Whether this deployment can watch together, for this person: false
    /// where there is no gateway, nobody is signed in, or they were not given
    /// it, which the web does not tell apart either.
    pub async fn watch_ready(self: Arc<Self>) -> Result<bool, ProtocolError> {
        on_core(async move {
            Ok(self
                .api
                .send(self.api.request(Method::GET, "/api/watch/ready")?)
                .await?
                .ok())
        })
        .await
    }

    /// What a pasted link plays, or why it plays nothing. `for_other` is for
    /// whoever holds a show reading a link somebody else added to it.
    pub async fn resolve_video(
        self: Arc<Self>,
        pasted: String,
        for_other: bool,
    ) -> Result<ResolvedLink, ProtocolError> {
        on_core(async move { self.api.resolve_video(&pasted, for_other).await }).await
    }

    /// What a ticket plays, as the server says now. Asked once per ticket and
    /// kept by the caller, and asked again only by a player whose relay stopped
    /// partway through (watch-api.ts `askAgain`).
    pub async fn video_info(self: Arc<Self>, ticket: String) -> Result<TicketVideo, ProtocolError> {
        on_core(async move { self.api.video_info(&ticket).await }).await
    }

    /// The media servers there are for this person. The web reads every
    /// refusal here as none; they are passed on, and an app treats them so.
    pub async fn media_servers(self: Arc<Self>) -> Result<Vec<MediaServer>, ProtocolError> {
        on_core(async move {
            let found = self.api.library(&[("op", "servers")]).await?;
            Ok(found
                .get("servers")
                .and_then(Value::as_array)
                .map(|all| {
                    all.iter()
                        .map(|one| MediaServer {
                            key: text(one.get("key")),
                            name: text(one.get("name")),
                        })
                        .filter(|one| word_dash(&one.key, 1, usize::MAX))
                        .collect()
                })
                .unwrap_or_default())
        })
        .await
    }

    /// A server's libraries, or what is inside one of them when `parent` is given.
    pub async fn browse_media(
        self: Arc<Self>,
        server: String,
        parent: Option<String>,
    ) -> Result<Vec<MediaItem>, ProtocolError> {
        on_core(async move {
            match parent.as_deref() {
                Some(parent) => {
                    self.api
                        .items(&[("op", "items"), ("server", &server), ("parent", parent)])
                        .await
                }
                None => {
                    self.api
                        .items(&[("op", "views"), ("server", &server)])
                        .await
                }
            }
        })
        .await
    }

    /// Searches one media server.
    pub async fn search_media(
        self: Arc<Self>,
        server: String,
        words: String,
    ) -> Result<Vec<MediaItem>, ProtocolError> {
        on_core(async move {
            self.api
                .items(&[
                    ("op", "search"),
                    ("server", &server),
                    ("q", &utf16_prefix(&words, 100)),
                ])
                .await
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn knows_a_ticket_and_the_relay_paths() {
        assert!(is_ticket("Abc_def-1234567890xyz"));
        assert!(!is_ticket("short"));
        assert!(!is_ticket(&"a".repeat(65)));
        assert!(is_relayed("media?t=Poster_ticket_12345678"));
        assert!(!is_relayed("media?t=x&y=z"));
        assert!(is_image("library?op=image&server=home&id=1a2b&tag=abc"));
        assert!(is_image("library?op=image&server=home&id=1a2b&tag="));
        assert!(!is_image("library?op=image&server=a&id=1"));
        assert!(!is_image("library?op=image&server=../x&id=1a&tag=a"));
        assert!(!is_image("library?op=image&server=a&id=zz&tag=a"));
        assert_eq!(
            watch_media_link("home".into(), "1a2b".into()),
            "library:home/1a2b"
        );
    }
}
