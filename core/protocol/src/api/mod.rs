//! The deployment's HTTP API, spoken the way the web client speaks it.
//!
//! One `Deployment` per server the app talks to: a base URL and one HTTP
//! client with a cookie jar, and a method for every request the web client
//! makes. Each submodule names the web file it follows (web/src/live/api.ts,
//! account.ts, host.ts, knock.ts, meeting.ts, watch-api.ts, music.ts, and the
//! account page's own requests in web/src/account/Account.tsx), and the server
//! handlers that answer them are in internal/app/ of the server's repository.
//!
//! What the deployment sees from here is what it sees from a browser: the same
//! paths escaped the same way, the same JSON in the same key order, the same
//! headers, and no `Origin` or `Sec-Fetch-Site` (internal/admin/api.go
//! `FromHere` lets a request without them through, which is how the watchdog
//! and the enrolment script are let through too). The one header that differs
//! is the user agent, which says which app this is (agent.rs): the deployment
//! keeps it for its management pages, and nobody in a call ever sees it.
//!
//! Errors. The server sends `{"error": code}` with a refusal and the app owns
//! the sentence, so a refusal comes back as `Refused { status, code }` with the
//! code untouched. Where the answer carries no code: a gated feature that is not
//! configured (music, watching together, avatars) answers Go's plain-text 404,
//! which is `not_found`; the host controls fall back to the codes host.ts falls
//! back to (`mute_failed` and the rest); anything else is an empty code, which
//! is the web's `Refused(undefined)` and reads as the general failure. Nothing
//! answering at all is `Network`, and an answer of the wrong shape `Unexpected`.
//!
//! Where the web client swallows a failure into a neutral answer (who is signed
//! in, whether a room is live, what an invite says), the same neutral answer
//! comes back here for anything the deployment said, and `Network` for when it
//! said nothing: a page was loaded from the server and so knows it is there,
//! and an app was not, and has to be able to say "cannot reach the server"
//! rather than "nobody is signed in".
//!
//! Nothing here retries. A join carrying a passphrase is charged to the
//! deployment's guessing budget (internal/guess: ten a minute from one address,
//! thirty for everybody), and so is a sign-in; repeating either on a flaky
//! network would spend somebody's guesses on their own correct passphrase.

mod account;
mod agent;
mod meetings;
mod music;
mod remote;
mod rooms;
pub(crate) mod text;
mod updates;
mod watch;

use std::{sync::Arc, time::Duration};

use reqwest::{
    cookie::{CookieStore, Jar},
    header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE},
    Method, RequestBuilder,
};
use serde_json::Value;
use url::Url;

use crate::ProtocolError;

pub use account::Account;
pub use meetings::{host_not_coming_after_ms, meeting_ask_every_ms, Arrangement};
pub use music::{
    describe_music_audio, music_clock, MusicAudio, MusicLibrary, MusicLink, MusicLinkKind,
    MusicQuality, MusicTrack,
};
pub use remote::{
    parse_remote_config, remote_notice_text, remote_phrases, RemoteConfig, RemoteNotice,
};
pub use rooms::{
    knock_ask_every_ms, knock_give_up_after_ms, DeploymentInfo, DoorAnswer, DoorState,
    HostStanding, InviteInfo, JoinForward, JoinGrant, Joining, KnockTicket, Knocker, MadeInvite,
    Opening,
};
pub use updates::{
    update_notes, version_is_newer, AppBuild, UpdateCheck, UpdateDownload, UpdateError,
    UpdatePlatform, UpdateProgress,
};
pub use watch::{
    read_resolved, watch_media_link, MediaItem, MediaServer, ResolvedLink, ResolvedPlay,
    ResolvedPlayKind, ResolvedProxy, ResolvedProxyKind, ResolvedVideo, TicketVideo,
    UnresolvedReason,
};

use text::encode_uri_component;

/// The cookie an account's session travels in (internal/app/account.go).
const ACCOUNT_COOKIE: &str = "meet-live.account";

/// The update key's field of [`BUILD_MARK`]: `env!("TOMOSHIBI_UPDATE_KEY")`,
/// the release key build.rs bakes into the core for update signing
/// (api/updates.rs `update_key`), once that change is in this tree; until
/// then empty, which every packaging script refuses.
macro_rules! build_mark_update_key { () => { "" }; }

/// The one deployment these apps are for.
///
/// Fixed here rather than in each app so that every client built on the core,
/// the Mac and the iPhone now and Windows and Android later, is held to it
/// alike: the apps are this deployment's, and a copy of one pointed at a server
/// of somebody else's own is exactly what is not wanted. Until now the address
/// was a field anybody could type into, and a link to another server took the
/// app there.
///
/// Its host comes from the build (build.rs: deployment.local at the
/// repository's root, or the environment's TOMOSHIBI_HOST), so that the
/// published source names no deployment; a build given neither is for
/// meet.example.com and reaches nothing.
pub const HOME: &str = concat!("https://", env!("TOMOSHIBI_HOST"), "/");

/// [`HOME`]'s host name alone.
pub const HOST: &str = env!("TOMOSHIBI_HOST");

/// [`HOME`], for an app to open on.
#[uniffi::export]
pub fn home_deployment() -> String {
    std::hint::black_box(BUILD_MARK);
    HOME.to_owned()
}

/// [`HOST`], the deployment this core was built for, for an app's release
/// build to check against its own before it opens anything (a build given no
/// host is for meet.example.com and reaches nothing).
#[uniffi::export]
pub fn home_host() -> String {
    HOST.to_owned()
}

/// Whether this core was built without debug assertions, which is what
/// [`admitted`] keys on: only such a core holds the app to [`HOME`]. A debug
/// or `local` core in a release app accepts any deployment a link names and
/// writes its relay timings to standard error, and nothing else about the app
/// shows it, so a release build checks this once rather than trusting the
/// profile the core happened to be built with.
#[uniffi::export]
pub fn core_is_release() -> bool {
    std::hint::black_box(BUILD_MARK);
    !cfg!(debug_assertions)
}

/// What a packaging script needs to know about the core, as text in the
/// built library, for the scripts (apple/dmg/check-core.sh,
/// windows/check-core.ps1, the Android release's `requireReleaseCore`), which
/// have a file to read and no core to call: the profile, `release` or
/// `debug`, then [`HOST`], then the update key the core holds every update to
/// (hexadecimal, empty where it checks none), between `tomoshibi-core-build:`
/// and `;`, separated by colons. A release app with a debug core, one for
/// meet.example.com or one checking no update's signature packaged as readily
/// as a good one before they read this.
/// Kept in the binary by the calls the apps make at launch
/// ([`home_deployment`], [`Deployment::new`]), which name it, so that the
/// linker's dead stripping cannot take it out of an app.
#[cfg(not(debug_assertions))]
pub const BUILD_MARK: &str =
    concat!("tomoshibi-core-build:release:", env!("TOMOSHIBI_HOST"), ":", build_mark_update_key!(), ";");
#[cfg(debug_assertions)]
pub const BUILD_MARK: &str =
    concat!("tomoshibi-core-build:debug:", env!("TOMOSHIBI_HOST"), ":", build_mark_update_key!(), ";");

/// Whether a build may talk to the deployment at `origin`: a release build
/// only to [`HOME`]. A debug build talks to the one it is given, because the
/// tests run deployments of their own on loopback and would otherwise have
/// nothing to drive.
fn admitted(origin: &Url, release: bool) -> bool {
    !release || origin.as_str() == HOME
}

/// One deployment, and the conversation with it.
#[derive(uniffi::Object)]
pub struct Deployment {
    pub(crate) api: Api,
}

/// What every request needs, cheap to clone into the core's runtime.
#[derive(Clone)]
pub(crate) struct Api {
    pub(crate) client: reqwest::Client,
    pub(crate) jar: Arc<Jar>,
    /// The deployment's origin with a `/` path. Every endpoint is at `/api` on
    /// the origin, because the web client fetches absolute paths, so a path in
    /// what somebody typed is dropped rather than prefixed to everything.
    pub(crate) base: Url,
}

#[uniffi::export]
impl Deployment {
    /// A deployment at `base`, such as `https://live.example.org`.
    #[uniffi::constructor]
    pub fn new(base: String) -> Result<Arc<Self>, ProtocolError> {
        let parsed = Url::parse(base.trim())
            .map_err(|fault| invalid(format!("{base} is not an address: {fault}")))?;

        if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
            return Err(invalid(format!("{base} is not an http or https address")));
        }

        let origin = Url::parse(&format!("{}/", parsed.origin().ascii_serialization()))
            .map_err(|fault| invalid(format!("{base} has no usable origin: {fault}")))?;

        std::hint::black_box(BUILD_MARK);
        if !admitted(&origin, !cfg!(debug_assertions)) {
            return Err(invalid(format!("{base} is not the server this app is for")));
        }

        let jar = Arc::new(Jar::default());

        // Redirects are followed within the deployment and nowhere else. A 307
        // carries its body with it, and a join's body is a passphrase: a proxy
        // in front of the server that sent it to another host would be handing
        // that host a secret the person typed for this one. A browser's page
        // is same-origin with its API and never meets the question.
        let home = origin.origin();
        let redirect = reqwest::redirect::Policy::custom(move |attempt| {
            if attempt.previous().len() >= 5 {
                attempt.error("redirected too many times")
            } else if attempt.url().origin() == home {
                attempt.follow()
            } else {
                attempt.stop()
            }
        });

        let client = reqwest::Client::builder()
            .cookie_provider(jar.clone())
            .user_agent(agent::user_agent())
            .redirect(redirect)
            // A connection that will not open is a server that is not there,
            // and saying so after ten seconds is better than a spinner.
            .connect_timeout(Duration::from_secs(10))
            // Longer than the hundred seconds the server gives a gateway
            // (internal/app/gateway.go), so a slow resolve is answered by the
            // server's own 502 rather than cut off here first.
            .read_timeout(Duration::from_secs(110))
            .build()
            .map_err(|fault| ProtocolError::Unexpected {
                detail: format!("the HTTP client would not start: {fault}"),
            })?;

        Ok(Arc::new(Self {
            api: Api {
                client,
                jar,
                base: origin,
            },
        }))
    }

    /// The origin every request goes to, ending in `/`.
    pub fn base_url(&self) -> String {
        self.api.base.to_string()
    }

    /// The account session's cookie value, for the app to keep in the Keychain
    /// across launches. `None` while nobody is signed in.
    pub fn account_cookie(&self) -> Option<String> {
        let header = self.api.jar.cookies(&self.api.base)?;

        header
            .to_str()
            .ok()?
            .split(';')
            .find_map(|pair| pair.trim().strip_prefix("meet-live.account="))
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
    }

    /// Puts back a session `account_cookie` gave out earlier, or forgets the
    /// one held when `value` is empty.
    ///
    /// Written with the attributes the server gives it (HttpOnly,
    /// SameSite=Strict, Secure over https) and no expiry: the server keeps the
    /// session's thirty sliding days itself, and a session it has ended is
    /// refused by it whatever the jar holds.
    pub fn restore_account_cookie(&self, value: String) -> Result<(), ProtocolError> {
        let secure = if self.api.base.scheme() == "https" {
            "; Secure"
        } else {
            ""
        };

        if value.is_empty() {
            self.api.jar.add_cookie_str(
                &format!("{ACCOUNT_COOKIE}=; Path=/; Max-Age=0{secure}"),
                &self.api.base,
            );
            return Ok(());
        }

        // RFC 6265's cookie-octet. A value outside it would be split or cut by
        // the jar into a different cookie from the one that was kept, and the
        // session would quietly be somebody else's or nobody's.
        if !value
            .bytes()
            .all(|b| matches!(b, 0x21 | 0x23..=0x2B | 0x2D..=0x3A | 0x3C..=0x5B | 0x5D..=0x7E))
        {
            return Err(invalid("that is not a cookie value".into()));
        }

        self.api.jar.add_cookie_str(
            &format!("{ACCOUNT_COOKIE}={value}; Path=/; HttpOnly; SameSite=Strict{secure}"),
            &self.api.base,
        );

        Ok(())
    }

    /// The `Cookie` header this deployment is sent, for a media player that
    /// fetches a cookie-gated address itself (a media server's posters, the
    /// music library's audio).
    pub fn cookie_header(&self) -> Option<String> {
        self.api
            .jar
            .cookies(&self.api.base)
            .and_then(|header| header.to_str().ok().map(str::to_owned))
    }

    /// The link an invite is handed out as: `{base}/?invite=T#/room`, built the
    /// way host.ts `invite` and the account page build it.
    pub fn invite_link(&self, token: String, room: String) -> String {
        self.api.invite_link(&token, &room)
    }

    /// The link an arranged meeting is handed out as: `{base}/?meeting=T#/room`
    /// (meeting.ts `linkFor`).
    pub fn meeting_link(&self, token: String, room: String) -> String {
        format!(
            "{}?meeting={}#/{room}",
            self.api.base,
            encode_uri_component(&token)
        )
    }

    /// Where the picture of the account with this signature is served. Absent
    /// pictures answer 404.
    pub fn avatar_url(&self, trip: String) -> String {
        format!(
            "{}api/avatar/{}",
            self.api.base,
            encode_uri_component(&trip)
        )
    }

    /// The picture to show for a participant, or `None` where only a drawn
    /// figure may stand in for them.
    ///
    /// Only for an identity minted for somebody signed in (`a…`), as
    /// components/room/Avatar.tsx decides: the same mark reached by typing a
    /// passphrase into a join form does not wear the account's picture, or the
    /// picture would be a second credential, and a weaker one.
    pub fn avatar_for(&self, identity: String) -> Option<String> {
        let signature = crate::identity::signature_of(identity)?;

        signature.account.then(|| self.avatar_url(signature.trip))
    }

    /// The relayed copy of a ticket's video, `{base}/api/watch/media?t=T`, for
    /// a player to open; refused for anything that is not a ticket
    /// (watch-api.ts `TICKET`), so that nobody in a call can hand this a path.
    pub fn watch_media_url(&self, ticket: String) -> Result<String, ProtocolError> {
        if !watch::is_ticket(&ticket) {
            return Err(invalid("that is not a watch ticket".into()));
        }

        Ok(self.api.absolute(&format!("/api/watch/media?t={ticket}")))
    }

    /// Where a track's audio is played from (music.ts `audioUrl`). Cookie-gated
    /// like everything under /api/music; see `cookie_header`.
    pub fn music_audio_url(&self, source: String, id: String, quality: MusicQuality) -> String {
        let query = url::form_urlencoded::Serializer::new(String::new())
            .append_pair("source", &source)
            .append_pair("id", &id)
            .append_pair("quality", quality.said())
            .finish();

        self.api.absolute(&format!("/api/music/audio?{query}"))
    }
}

impl Deployment {
    /// The HTTP client, for the media crate's own requests to the same server.
    pub fn http(&self) -> &reqwest::Client {
        &self.api.client
    }

    /// The deployment's origin, ending in `/`.
    pub fn base(&self) -> &Url {
        &self.api.base
    }
}

fn invalid(message: String) -> ProtocolError {
    ProtocolError::Invalid { detail: message }
}

pub(crate) fn unexpected(message: impl Into<String>) -> ProtocolError {
    ProtocolError::Unexpected {
        detail: message.into(),
    }
}

impl Api {
    pub(crate) fn invite_link(&self, token: &str, room: &str) -> String {
        format!(
            "{}?invite={}#/{room}",
            self.base,
            encode_uri_component(token)
        )
    }

    /// A path on the deployment, such as `/api/deployment`, as a URL.
    pub(crate) fn url(&self, path: &str) -> Result<Url, ProtocolError> {
        self.base
            .join(path)
            .map_err(|fault| invalid(format!("{path} is not a path: {fault}")))
    }

    /// The same, as a string for somebody else to fetch; the paths built here
    /// are always ones `join` accepts, so the fallback is never reached.
    pub(crate) fn absolute(&self, path: &str) -> String {
        self.url(path)
            .map(String::from)
            .unwrap_or_else(|_| format!("{}{}", self.base, path.trim_start_matches('/')))
    }

    pub(crate) fn request(
        &self,
        method: Method,
        path: &str,
    ) -> Result<RequestBuilder, ProtocolError> {
        Ok(self.client.request(method, self.url(path)?))
    }

    /// A request with `content-type: application/json` and nothing else, the
    /// way knock.ts and meeting.ts send even their GETs.
    pub(crate) fn json_request(
        &self,
        method: Method,
        path: &str,
    ) -> Result<RequestBuilder, ProtocolError> {
        Ok(self
            .request(method, path)?
            .header(CONTENT_TYPE, "application/json"))
    }

    /// A request authorised by a join token, as host.ts `ask` sends it.
    pub(crate) fn bearer_request(
        &self,
        method: Method,
        path: &str,
        bearer: &str,
    ) -> Result<RequestBuilder, ProtocolError> {
        let authorization = HeaderValue::from_str(&format!("Bearer {bearer}"))
            .map_err(|_| invalid("that join token cannot travel in a header".into()))?;

        Ok(self
            .json_request(method, path)?
            .header(AUTHORIZATION, authorization))
    }

    /// Sends a request and reads the whole answer.
    pub(crate) async fn send(&self, request: RequestBuilder) -> Result<Answer, ProtocolError> {
        let response = request.send().await.map_err(network)?;
        let status = response.status().as_u16();

        // Only a redirect the policy above declined comes back as one: an
        // answer from a host that is not this deployment.
        if (300..400).contains(&status) {
            let to = response
                .headers()
                .get("location")
                .and_then(|value| value.to_str().ok())
                .unwrap_or("nowhere");
            return Err(unexpected(format!(
                "the deployment sent this request elsewhere ({status} to {to})"
            )));
        }

        let headers = response.headers().clone();
        let body = response.bytes().await.map_err(network)?.to_vec();

        Ok(Answer {
            status,
            headers,
            body,
        })
    }
}

/// A transport failure, said without its URL: a join's carries the invite in
/// its query, and an error is the kind of thing that ends up in a log.
fn network(fault: reqwest::Error) -> ProtocolError {
    let fault = fault.without_url();
    let mut message = fault.to_string();
    let mut cause = std::error::Error::source(&fault);

    while let Some(inner) = cause {
        message.push_str(": ");
        message.push_str(&inner.to_string());
        cause = inner.source();
    }

    ProtocolError::Network { detail: message }
}

/// What came back, whole.
pub(crate) struct Answer {
    pub(crate) status: u16,
    pub(crate) headers: HeaderMap,
    pub(crate) body: Vec<u8>,
}

impl Answer {
    pub(crate) fn ok(&self) -> bool {
        (200..300).contains(&self.status)
    }

    /// The body as JSON, or `None` where it is not.
    pub(crate) fn value(&self) -> Option<Value> {
        serde_json::from_slice(&self.body).ok()
    }

    /// The body as a `T`, or `Unexpected` naming what was being read.
    pub(crate) fn json<T: serde::de::DeserializeOwned>(
        &self,
        what: &str,
    ) -> Result<T, ProtocolError> {
        serde_json::from_slice(&self.body).map_err(|fault| unexpected(format!("{what}: {fault}")))
    }

    /// The server's code, where the body is `{"error": "<code>"}`.
    pub(crate) fn code(&self) -> Option<String> {
        self.value()?
            .get("error")?
            .as_str()
            .filter(|code| !code.is_empty())
            .map(str::to_owned)
    }

    /// Go's `http.NotFound`: the plain-text 404 a feature the deployment has
    /// not configured answers with, which is not the 404 a gateway behind it
    /// gives with a reason in JSON.
    pub(crate) fn absent(&self) -> bool {
        self.status == 404 && self.value().is_none()
    }

    pub(crate) fn header(&self, name: &str) -> Option<&str> {
        self.headers.get(name).and_then(|value| value.to_str().ok())
    }

    /// The refusal this answer is, with `fallback` where it carries no code.
    pub(crate) fn refused(&self, fallback: &str) -> ProtocolError {
        let code = self.code().unwrap_or_else(|| {
            if self.absent() {
                "not_found".into()
            } else {
                fallback.into()
            }
        });

        ProtocolError::Refused {
            status: self.status,
            code,
        }
    }

    /// The answer itself where it is a 2xx, and its refusal otherwise.
    pub(crate) fn accepted(self, fallback: &str) -> Result<Self, ProtocolError> {
        if self.ok() {
            Ok(self)
        } else {
            Err(self.refused(fallback))
        }
    }
}

/// Reading JSON the way the web client reads it: a field of the wrong type is
/// as good as absent, and absent is the empty value.
pub(crate) mod read {
    use serde_json::Value;

    /// watch-api.ts and music.ts `text`.
    pub(crate) fn text(value: Option<&Value>) -> String {
        value.and_then(Value::as_str).unwrap_or_default().to_owned()
    }

    /// A string that is there and not empty.
    pub(crate) fn some_text(value: Option<&Value>) -> Option<String> {
        value
            .and_then(Value::as_str)
            .filter(|said| !said.is_empty())
            .map(str::to_owned)
    }

    /// `=== true`.
    pub(crate) fn yes(value: Option<&Value>) -> bool {
        value.and_then(Value::as_bool) == Some(true)
    }

    /// A finite number, or nought.
    pub(crate) fn number(value: Option<&Value>) -> f64 {
        value
            .and_then(Value::as_f64)
            .filter(|n| n.is_finite())
            .unwrap_or(0.0)
    }

    /// watch-api.ts `count`: a finite number above nought, or nought.
    pub(crate) fn positive(value: Option<&Value>) -> f64 {
        Some(number(value)).filter(|n| *n > 0.0).unwrap_or(0.0)
    }

    /// A count as a whole number for a record, where the web keeps a JS
    /// number; negative and fractional parts are things no gateway sends.
    pub(crate) fn whole(value: Option<&Value>) -> u32 {
        positive(value).min(f64::from(u32::MAX)) as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn origin(of: &str) -> Url {
        let parsed = Url::parse(of).unwrap();
        Url::parse(&format!("{}/", parsed.origin().ascii_serialization())).unwrap()
    }

    #[test]
    fn a_release_build_talks_to_home_alone() {
        let at = |url: String| admitted(&origin(&url), true);
        assert!(at(format!("https://{HOST}")));
        assert!(at(format!("https://{}:443/#/room", HOST.to_ascii_uppercase())));
        assert!(!at("https://another.example.net".to_owned()));
        assert!(!at(format!("http://{HOST}")));
        assert!(!at(format!("https://{HOST}:8443")));
        assert!(!at(format!("https://{HOST}.example.org")));
    }

    #[test]
    fn a_debug_build_talks_to_the_one_it_is_given() {
        assert!(admitted(&origin("http://127.0.0.1:8080"), false));
    }
}
