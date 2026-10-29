//! The apps' own updates: whether the deployment has a newer build of the app
//! that is running, and that build, downloaded whole and checked.
//!
//! The server's side is internal/app/updates.go: `GET
//! /api/apps/latest/{platform}` says the newest build's version, size, hash
//! and notes, and `/api/apps/latest/{platform}/file` hands the build over in
//! ranges, with its hash as the ETag. Neither asks for an account, because an
//! app somebody uses as a guest on an invitation has no session and still
//! needs its updates; what stands in for one is that the request says it is
//! one of the apps, which every `Deployment` does in its user agent
//! (agent.rs). A request through anything else is refused `not_the_app`.
//!
//! Here rather than in each app, so that the Mac, Android and Windows apps
//! ask the same address with the same user agent, compare versions by the
//! same rule and refuse a file by the same checks. What each app does with a
//! build that arrived (mounting a disk image and replacing itself, handing an
//! APK to the package installer) is its own.
//!
//! The iOS app asks the first address too, by the same rule, and is told a
//! version and its notes and nothing else: TestFlight installs it, so all an
//! app can do with an iOS build is say that it is waiting there. Such a
//! build has no size or hash, and a download of one is refused here before
//! anything is asked of the deployment, whose `/file` for iOS is `no_build`
//! whatever its manifest says.
//!
//! Every build that is a file is signed by the deployment's release key, an
//! Ed25519 key kept on no server. The size and the hash come from the same
//! server as the file, so on their own they prove only that the file is the
//! one that server meant: a server, or a shelf of builds, that somebody else
//! had written to would describe their own file just as faithfully, and on
//! Windows nothing else stands between that file and a silent install, the
//! setup being signed by nobody. So each build's line of the manifest
//! carries a signature (`sig`) made where the build was made, which
//! updates.go passes on untouched, and the key's public half is built into
//! the core (build.rs, TOMOSHIBI_UPDATE_KEY). A check offers no build the
//! key did not sign, and a download moves nothing to its destination that
//! is not, byte for byte, a build the key signed. The Mac's Developer ID
//! and Android's same-signer rule stand as they were, with this beneath
//! them; on Windows it is the whole of the trust. A core built with no key
//! (the tests' and a local deployment's) checks no signatures and offers
//! what it is told, as every core before this one did, and an app from
//! before it reads past the signature.
//!
//! A download is resumable. It is written beside its destination as
//! `<destination>.<first sixteen hex digits of the hash>.part`, named by the
//! build so that what is left of one build is never continued as another, and
//! asked for again from where it stopped with `If-Range` holding the hash: if
//! the deployment has had a newer build copied in meanwhile it answers with
//! the whole of that one instead of the rest of the old, which this sees by
//! its ETag and reports as `Superseded` rather than splicing two builds into
//! a file that would only fail its hash. Nothing is moved to the destination
//! until its size and its SHA-256 are the ones the check said; a file that is
//! not is deleted, so that nobody can install it by accident afterwards.

use std::{
    collections::HashMap,
    io::Read,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use reqwest::{
    header::{CONTENT_LENGTH, CONTENT_RANGE, ETAG, IF_RANGE, RANGE},
    Method, Response,
};
use ring::signature::{UnparsedPublicKey, ED25519};
use serde_json::Value;
use tokio::{fs, io::AsyncWriteExt, sync::watch};

use super::{network, Api, Deployment};
use crate::{runtime::on_core, ProtocolError};

/// The platforms the deployment says the newest build of, by the section of
/// its manifest each is in. The Mac, Android and Windows builds are files it
/// hands over; the iOS app is installed by TestFlight, and the deployment
/// only says which version is newest there and what changed in it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum UpdatePlatform {
    Mac,
    Android,
    Windows,
    /// Last, and to stay last. A variant crosses into the apps as its
    /// position counted from one, so a binding generated before this one
    /// was added still reads the other three rightly; put first, it would
    /// have such a binding read the Mac as Android.
    Ios,
}

impl UpdatePlatform {
    /// The word in the path, as updates.go matches it.
    fn section(self) -> &'static str {
        match self {
            Self::Mac => "mac",
            Self::Android => "android",
            Self::Windows => "windows",
            Self::Ios => "ios",
        }
    }

    /// Whether the app is installed from somewhere other than the
    /// deployment, so that its build is a version and notes and there is no
    /// file of it to download. updates.go's `elsewhere`, by the same name.
    fn elsewhere(self) -> bool {
        matches!(self, Self::Ios)
    }
}

/// The newest build the deployment has for a platform.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct AppBuild {
    pub platform: UpdatePlatform,
    /// Dotted numbers, as the apps write their own: 0.1.0 build 8 is 0.1.0.8.
    pub version: String,
    /// Bytes. Nought for an iOS build, which has no file here.
    pub size: u64,
    /// Lower-case hexadecimal. Empty for an iOS build.
    pub sha256: String,
    /// What changed, by language tag (zh-Hans, zh-Hant, ja, en); any of them
    /// may be missing, and so may all of them. See `update_notes`.
    pub notes: HashMap<String, String>,
    /// The name the deployment keeps the build under, as the release key
    /// signed it; not where an app saves it. Empty for an iOS build, and
    /// for one from a deployment that does not say it.
    ///
    /// Last, with a default, as `signature` is: added after the apps were
    /// written, and an app that makes a build of its own (Android's tests
    /// do) goes on making one without either.
    #[uniffi(default = "")]
    pub file_name: String,
    /// The release key's signature of the build (`Signed`), in base64, as
    /// the manifest has it. Empty where there is none, which a core built
    /// with a key offers and downloads nothing of.
    #[uniffi(default = "")]
    pub signature: String,
}

/// What a check found.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum UpdateCheck {
    /// Nothing newer than what is running, including a deployment with no
    /// build to offer this platform at all.
    UpToDate,
    Available {
        build: AppBuild,
    },
}

/// Why a download did not finish.
///
/// Its own error rather than `ProtocolError`, whose four cases every app
/// already matches exhaustively: a download fails in ways a request does not
/// (cancelled, a file that is not the build, a disk that will not take it),
/// and each wants its own sentence.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, uniffi::Error)]
pub enum UpdateError {
    /// `cancel` was called. What had arrived is kept, for the next download
    /// of the same build to carry on from.
    #[error("cancelled")]
    Cancelled,
    /// The deployment answered and said no; `code` is its `{"error": ...}`,
    /// `not_found` for Go's plain 404, and empty where it gave none.
    #[error("refused with {code} ({status})")]
    Refused { status: u16, code: String },
    /// The connection failed or ended early. What had arrived is kept.
    #[error("network: {detail}")]
    Network { detail: String },
    /// What arrived is not the build that was offered: another size, or
    /// another hash. It has been deleted.
    #[error("the download is not the build offered: {detail}")]
    Mismatch { detail: String },
    /// The deployment is now handing out a different build from the one
    /// this download was for; check again, and offer that one.
    #[error("the deployment has a newer build than the one being downloaded")]
    Superseded,
    /// The file could not be written, read or moved where it was asked to go.
    #[error("storage: {detail}")]
    Storage { detail: String },
    /// Something came back that does not have the shape it should; or the
    /// build was one with nothing to download, an iOS build among them, and
    /// nothing was asked of the deployment.
    ///
    /// The second is not a case of its own because the apps match these
    /// cases exhaustively (the Mac's CoreTransfer has no default), and a
    /// new one would stop every app building until each had a sentence for
    /// something it is never asked to do: no app downloads an iOS build.
    #[error("unexpected answer: {detail}")]
    Unexpected { detail: String },
}

/// What the app implements to show how far a download has got. Called from
/// the core's own threads, at most ten times a second and once more at the
/// end, with `received` never going down within one run.
#[uniffi::export(with_foreign)]
pub trait UpdateProgress: Send + Sync {
    fn progressed(&self, received: u64, total: u64);
}

/// Whether `offered` is a later version than `running`: dotted numbers
/// compared one part at a time as numbers, a missing part counting as
/// nought, so 0.1.0.10 is after 0.1.0.9 and 0.1.0 is 0.1.0.0. Either one
/// not being dotted numbers at all is never newer, so that a version
/// somebody mistyped into the manifest is not offered to everybody for ever.
#[uniffi::export]
pub fn version_is_newer(offered: String, running: String) -> bool {
    match (segments(&offered), segments(&running)) {
        (Some(offered), Some(running)) => later(&offered, &running),
        _ => false,
    }
}

/// The version an app calls itself and the deployment offers it, from the
/// version people are told and the build: the version alone for its first
/// build, and the build as a fourth part for every later one, so that 0.1.4
/// build 2 is 0.1.4.2 (owner, 2026-10-22: a small fix keeps the version and
/// takes the next build; a new version is for new features or a large
/// update). The first build has no fourth part because 0.1.4 is what every
/// app out there already compares and installs as 0.1.4, and 0.1.4.2 is after
/// it by `version_is_newer`'s own rule; a 0.1.4.1 would be after 0.1.4 too,
/// and offered to the app that is already it. A build that is not a number is
/// taken for the first.
#[uniffi::export]
pub fn version_with_build(version: String, build: String) -> String {
    let version = version.trim();
    match build.trim().parse::<u64>() {
        Ok(build) if build > 1 => format!("{version}.{build}"),
        _ => version.to_owned(),
    }
}

/// How a version is written for people: a later build of a version as the
/// version with the build in brackets after it, 0.1.4.2 as "0.1.4 (2)", and
/// the first build as the version alone, 0.1.4 and 0.1.4.1 both as "0.1.4"
/// (the deployment joins the build on as 0.1.4.1 for the Macs from before
/// 0.1.2). Anything else is written as it is.
#[uniffi::export]
pub fn version_label(version: String) -> String {
    match segments(&version).as_deref() {
        Some([a, b, c, build]) if *build > 1 => format!("{a}.{b}.{c} ({build})"),
        Some([a, b, c, _]) => format!("{a}.{b}.{c}"),
        _ => version.trim().to_owned(),
    }
}

/// The notes to show beside a build for somebody reading `language`: in
/// that language, else in the language without its region or script, else
/// in English, else none at all, which is a banner with no notes rather than
/// notes in a language the reader may not read.
#[uniffi::export]
pub fn update_notes(build: AppBuild, language: String) -> Option<String> {
    notes_in(&build.notes, &language)
}

pub(super) fn notes_in(notes: &HashMap<String, String>, language: &str) -> Option<String> {
    let find = |tag: &str| {
        notes
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(tag))
            .map(|(_, note)| note.clone())
    };

    let mut tag = language.trim();
    loop {
        if tag.is_empty() {
            break;
        }
        if let Some(note) = find(tag) {
            return Some(note);
        }
        match tag.rfind('-') {
            Some(cut) => tag = &tag[..cut],
            None => break,
        }
    }

    find("en")
}

/// A version's parts, or `None` where it is not dotted numbers.
fn segments(version: &str) -> Option<Vec<u64>> {
    let parts = version
        .trim()
        .split('.')
        .map(|part| {
            if part.is_empty() || !part.bytes().all(|b| b.is_ascii_digit()) {
                None
            } else {
                part.parse::<u64>().ok()
            }
        })
        .collect::<Option<Vec<_>>>()?;

    (!parts.is_empty()).then_some(parts)
}

fn later(offered: &[u64], running: &[u64]) -> bool {
    for at in 0..offered.len().max(running.len()) {
        let (a, b) = (
            offered.get(at).copied().unwrap_or(0),
            running.get(at).copied().unwrap_or(0),
        );
        if a != b {
            return a > b;
        }
    }

    false
}

/// The public half of the release key the deployment's builds are signed
/// with, as build.rs found it: 64 hexadecimal digits, or nothing for a core
/// built to check no signatures.
const RELEASE_KEY: &str = env!("TOMOSHIBI_UPDATE_KEY");

/// The release key, where this core was built with one.
fn release_key() -> Option<[u8; 32]> {
    key_from_hex(RELEASE_KEY)
}

fn key_from_hex(hex: &str) -> Option<[u8; 32]> {
    let hex = hex.as_bytes();
    if hex.len() != 64 || !hex.iter().all(u8::is_ascii_hexdigit) {
        return None;
    }

    let digit = |b: u8| (b as char).to_digit(16).unwrap_or(0) as u8;
    let mut key = [0u8; 32];
    for (at, pair) in hex.chunks(2).enumerate() {
        key[at] = digit(pair[0]) << 4 | digit(pair[1]);
    }

    Some(key)
}

/// The public half of the release key this core holds the deployment's
/// builds to, in hexadecimal; none where it was built to check no
/// signatures, as the tests' cores and a local deployment's are.
///
/// For a release build's own check of itself. build.rs already refuses to
/// make a core that is given out (debug assertions off) for a real
/// deployment without a key; an app or a packaging step that wants to see
/// the key from the finished build, beside whether its core is a release
/// one, asks here.
#[uniffi::export]
pub fn update_key() -> Option<String> {
    release_key().map(|_| RELEASE_KEY.to_ascii_lowercase())
}

/// The first line of what the release key signs: what the signature is for,
/// so that one made for a build can never be read as made for anything else
/// the key might sign, and which form of the lines below follows it, so that
/// a later form can be told apart from this one.
const SIGNED_AS: &str = "tomoshibi-update-v1";

/// What the release key signs of a build, as these lines, each ended by a
/// newline, in this order and with nothing else:
///
/// ```text
/// tomoshibi-update-v1
/// platform=windows
/// version=0.1.4.2
/// file=Tomoshibi-Setup-0.1.4.2.exe
/// size=145012345
/// sha256=<64 lower-case hexadecimal digits>
/// ```
///
/// The version is the one the app is told and installs by, with the build
/// joined on (updates.go `versionFor`, `version_with_build` here): the app is
/// never told the manifest's version and build apart, and the joined one is
/// what it compares and what each installer holds the file it runs to, so a
/// signature over it covers both. The platform keeps a Mac's signed disk
/// image from being offered to Windows under its own signature. The notes
/// are not signed: they are words beside a build, edited after it is
/// shelved, and no part of what is installed.
///
/// Nothing here is signed unless every value is held to characters that
/// cannot carry a newline or an equals sign (`vouched_for_by`), so no two
/// builds are ever the same lines. The release tooling that signs a
/// manifest writes these lines by the same rule, and a test holds the two
/// to one example.
struct Signed<'a> {
    platform: UpdatePlatform,
    version: &'a str,
    file_name: &'a str,
    size: u64,
    sha256: &'a str,
}

impl<'a> Signed<'a> {
    fn of(build: &'a AppBuild) -> Self {
        Self {
            platform: build.platform,
            version: &build.version,
            file_name: &build.file_name,
            size: build.size,
            sha256: &build.sha256,
        }
    }

    fn message(&self) -> Vec<u8> {
        format!(
            "{SIGNED_AS}\nplatform={}\nversion={}\nfile={}\nsize={}\nsha256={}\n",
            self.platform.section(),
            self.version,
            self.file_name,
            self.size,
            self.sha256
        )
        .into_bytes()
    }

    /// Whether `signature`, in base64, is `key`'s signature of these fields.
    /// False for anything that is not: no signature, one that is not 64
    /// bytes, one of other fields or by another key; and false for an iOS
    /// build, which has no file and is never signed.
    fn vouched_for_by(&self, key: &[u8; 32], signature: &str) -> bool {
        let plain = !self.platform.elsewhere()
            && self
                .version
                .bytes()
                .all(|b| b.is_ascii_digit() || b == b'.')
            && segments(self.version).is_some()
            && signable_name(self.file_name)
            && self.size > 0
            && self.sha256.len() == 64
            && self
                .sha256
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
        if !plain {
            return false;
        }

        let Ok(signature) = BASE64.decode(signature) else {
            return false;
        };

        signature.len() == 64
            && UnparsedPublicKey::new(&ED25519, key)
                .verify(&self.message(), &signature)
                .is_ok()
    }
}

/// Whether a file name can be put into what is signed: what updates.go
/// holds a manifest's file to (a name in the shelf itself, not hidden), and
/// further to letters, digits and `.`, `_`, `+`, `-`, so that no name can
/// carry a newline into the lines signed. The names builds are shelved
/// under, Tomoshibi-0.1.6.2.apk and Tomoshibi-Setup-0.1.4.2.exe, are all of
/// this kind.
fn signable_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && !name.starts_with('.')
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'+' | b'-'))
}

#[uniffi::export]
impl Deployment {
    /// Asks whether there is a build of this app for `platform` newer than
    /// `running`, the version the app reports in its user agent.
    ///
    /// A deployment with nothing for the platform (`no_build`) is up to date.
    /// Anything else that is not an answer is an error, which an app checking
    /// on its own keeps to itself and tries again later, and an app asked to
    /// check says.
    ///
    /// For `Ios` a build that is offered is one to announce, with its
    /// version and notes, and not to download: its size is nought and its
    /// hash empty, and `download_update` refuses it.
    ///
    /// In a core built with a release key, a newest build the key did not
    /// sign is an error too (`Unexpected`), whether or not it is newer: it is
    /// not "up to date", which would be the app saying something it does not
    /// know, and it is never offered.
    pub async fn check_for_update(
        self: Arc<Self>,
        platform: UpdatePlatform,
        running: String,
    ) -> Result<UpdateCheck, ProtocolError> {
        on_core(async move { self.api.check_for_update(platform, &running).await }).await
    }

    /// A download of `build` to `path`, which starts when it is run. The
    /// directory is made if it is not there.
    ///
    /// An iOS build is not downloaded: running it is `Unexpected` at once,
    /// with nothing asked of the deployment and nothing written to disk. So,
    /// in a core built with a release key, is a build the key did not sign.
    pub fn download_update(&self, build: AppBuild, path: String) -> Arc<UpdateDownload> {
        UpdateDownload::new(self.api.clone(), build, PathBuf::from(path), release_key())
    }
}

impl Api {
    async fn check_for_update(
        &self,
        platform: UpdatePlatform,
        running: &str,
    ) -> Result<UpdateCheck, ProtocolError> {
        if segments(running).is_none() {
            return Err(ProtocolError::Invalid {
                detail: format!("{running} is not a version"),
            });
        }

        let path = format!("/api/apps/latest/{}", platform.section());
        let answer = self.send(self.request(Method::GET, &path)?).await?;

        if answer.status == 404 && answer.code().as_deref() == Some("no_build") {
            return Ok(UpdateCheck::UpToDate);
        }

        let answer = answer.accepted("")?;
        let body = answer
            .value()
            .ok_or_else(|| super::unexpected("the newest build is not JSON"))?;
        let build = read_build(platform, &body, release_key().as_ref())?;

        Ok(
            if version_is_newer(build.version.clone(), running.to_owned()) {
                UpdateCheck::Available { build }
            } else {
                UpdateCheck::UpToDate
            },
        )
    }
}

/// updates.go `updateView`, held to what a download needs: a version that
/// can be compared, a size, and a hash that can be checked. A build the
/// deployment describes wrongly is not offered, since nothing that arrived
/// could ever match it.
///
/// An iOS build is held to its version alone, and is given no size or hash
/// whatever the answer says: updates.go leaves both out for it, and a pair
/// that came anyway would describe a file nobody can install on a phone.
///
/// Where there is a release `key`, any other build must carry the key's
/// signature of what it says (`Signed`), or it is not offered. Checked here,
/// before the version is compared, so that a deployment handing out a build
/// nobody signed is a failed check every time it is asked, which an app
/// asked to check says, rather than an update that never comes.
fn read_build(
    platform: UpdatePlatform,
    body: &Value,
    key: Option<&[u8; 32]>,
) -> Result<AppBuild, ProtocolError> {
    let wrong = |what: &str| super::unexpected(format!("the newest build {what}"));

    if body.get("platform").and_then(Value::as_str) != Some(platform.section()) {
        return Err(wrong("is for another platform"));
    }

    let version = body
        .get("version")
        .and_then(Value::as_str)
        .filter(|version| segments(version).is_some())
        .ok_or_else(|| wrong("has no version that can be compared"))?
        .trim()
        .to_owned();

    let (size, sha256, file_name, signature) = if platform.elsewhere() {
        (0, String::new(), String::new(), String::new())
    } else {
        let size = body
            .get("size")
            .and_then(Value::as_u64)
            .filter(|size| *size > 0)
            .ok_or_else(|| wrong("has no size"))?;

        let sha256 = body
            .get("sha256")
            .and_then(Value::as_str)
            .filter(|hash| hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit()))
            .ok_or_else(|| wrong("has no SHA-256"))?
            .to_ascii_lowercase();

        // Taken as they come: what they have to be is the signature's to
        // say, and a deployment from before signatures says neither.
        let text = |field: &str| {
            body.get(field)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned()
        };

        (size, sha256, text("file"), text("sig"))
    };

    // updates.go keeps only notes that are strings and trims them; the same
    // again here, because a note is shown as it comes and the server is not
    // the only thing that could have written it.
    let notes = body
        .get("notes")
        .and_then(Value::as_object)
        .map(|notes| {
            notes
                .iter()
                .filter_map(|(language, note)| {
                    let note = note.as_str()?.trim();
                    (!note.is_empty()).then(|| (language.clone(), note.to_owned()))
                })
                .collect()
        })
        .unwrap_or_default();

    let build = AppBuild {
        platform,
        version,
        size,
        sha256,
        notes,
        file_name,
        signature,
    };

    if let Some(key) = key {
        if !platform.elsewhere() && !Signed::of(&build).vouched_for_by(key, &build.signature) {
            return Err(wrong(if build.signature.is_empty() {
                "is not signed"
            } else {
                "is not signed by the release key"
            }));
        }
    }

    Ok(build)
}

/// One download of one build to one place.
#[derive(uniffi::Object)]
pub struct UpdateDownload {
    api: Api,
    build: AppBuild,
    path: PathBuf,
    /// Set once by `cancel`, and never cleared: a download called off stays
    /// called off, and carrying on is a new download of the same build,
    /// which picks up what this one left.
    stop: watch::Sender<bool>,
    /// The release key the build is held to, where the core was built with
    /// one: `release_key()`, and a key of the tests' own in theirs.
    key: Option<[u8; 32]>,
}

/// How often progress is said at most. A phone's JNA callback and a
/// SwiftUI redraw per network read of a fifty-megabyte file are thousands
/// of each for a bar that moves a pixel at a time.
const PROGRESS_EVERY: Duration = Duration::from_millis(100);

#[uniffi::export]
impl UpdateDownload {
    /// Downloads the build, resuming whatever an earlier download of it left,
    /// and returns once the file at the destination is the build offered.
    pub async fn run(
        self: Arc<Self>,
        progress: Arc<dyn UpdateProgress>,
    ) -> Result<(), UpdateError> {
        on_core(async move { self.fetch(progress.as_ref()).await }).await
    }

    /// Calls the download off; `run` returns `Cancelled` soon after.
    pub fn cancel(&self) {
        self.stop.send_replace(true);
    }

    /// Where the build will be once it has arrived.
    pub fn destination(&self) -> String {
        self.path.to_string_lossy().into_owned()
    }
}

/// What one attempt came to, where it is not the download's own result.
enum Attempt {
    Done(Result<(), UpdateError>),
    /// What was on disk could not be carried on from; start again from
    /// nothing, once.
    Restart,
}

impl UpdateDownload {
    fn new(api: Api, build: AppBuild, path: PathBuf, key: Option<[u8; 32]>) -> Arc<Self> {
        let (stop, _) = watch::channel(false);

        Arc::new(Self {
            api,
            build,
            path,
            stop,
            key,
        })
    }

    fn part(&self) -> PathBuf {
        let mut name = self.path.as_os_str().to_owned();
        name.push(format!(".{}.part", &self.build.sha256[..16]));
        PathBuf::from(name)
    }

    async fn fetch(&self, progress: &dyn UpdateProgress) -> Result<(), UpdateError> {
        if *self.stop.borrow() {
            return Err(UpdateError::Cancelled);
        }
        // Asked of the platform, and not left to the missing size and hash
        // below. A build is a record an app can make for itself, and an iOS
        // one given a size and a hash would otherwise be asked for at
        // `/api/apps/latest/ios/file`, a request spent to be told no_build.
        if self.build.platform.elsewhere() {
            return Err(UpdateError::Unexpected {
                detail: "the iOS app is installed through TestFlight, and there is no file of it to download"
                    .into(),
            });
        }
        if self.build.sha256.len() != 64 || self.build.size == 0 {
            return Err(UpdateError::Unexpected {
                detail: "that build has no size or hash to check it by".into(),
            });
        }
        // Before anything is asked or written. A build an app hands back is
        // the one a check offered, which the key had already signed; one it
        // did not sign was made some other way, and nothing that arrived for
        // it could be moved into place below.
        if let Some(key) = &self.key {
            if !Signed::of(&self.build).vouched_for_by(key, &self.build.signature) {
                return Err(UpdateError::Unexpected {
                    detail: "that build is not signed by the release key".into(),
                });
            }
        }

        if let Some(parent) = self
            .path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            fs::create_dir_all(parent).await.map_err(storage)?;
        }
        // A file already at the destination is not trusted to be this build:
        // it may be another's, or one an earlier run moved there and an app
        // then failed to install. The part is what carries over.
        discard(&self.path).await?;

        let part = self.part();
        let mut restarted = false;
        loop {
            match self.attempt(&part, progress, restarted).await {
                Attempt::Done(result) => return result,
                Attempt::Restart if !restarted => {
                    discard(&part).await?;
                    restarted = true;
                }
                Attempt::Restart => {
                    discard(&part).await?;
                    return Err(UpdateError::Unexpected {
                        detail: "the deployment would not hand the build over from its start"
                            .into(),
                    });
                }
            }
        }
    }

    async fn attempt(
        &self,
        part: &Path,
        progress: &dyn UpdateProgress,
        restarted: bool,
    ) -> Attempt {
        let size = self.build.size;

        let mut have = match fs::metadata(part).await {
            Ok(meta) => meta.len(),
            Err(fault) if fault.kind() == std::io::ErrorKind::NotFound => 0,
            Err(fault) => return Attempt::Done(Err(storage(fault))),
        };
        if have > size {
            if let Err(fault) = discard(part).await {
                return Attempt::Done(Err(fault));
            }
            have = 0;
        }

        // Bytes on disk from before this attempt, which are the ones a bad
        // hash may be the fault of.
        let carried = have;

        if have < size {
            match self.receive(part, have, progress).await {
                Ok(()) => {}
                Err(Received::Restart) => return Attempt::Restart,
                Err(Received::Failed(fault)) => return Attempt::Done(Err(fault)),
            }
        }

        progress.progressed(size, size);

        let (hash, length) = {
            let part = part.to_owned();
            match tokio::task::spawn_blocking(move || sha256_of(&part)).await {
                Ok(Ok(measured)) => measured,
                Ok(Err(fault)) => return Attempt::Done(Err(storage(fault))),
                Err(fault) => {
                    return Attempt::Done(Err(UpdateError::Unexpected {
                        detail: format!("the hash could not be taken: {fault}"),
                    }))
                }
            }
        };

        if hash != self.build.sha256 {
            if carried > 0 && !restarted {
                return Attempt::Restart;
            }
            if let Err(fault) = discard(part).await {
                return Attempt::Done(Err(fault));
            }
            return Attempt::Done(Err(UpdateError::Mismatch {
                detail: format!("its SHA-256 is {hash}, not {}", self.build.sha256),
            }));
        }

        Attempt::Done(self.settle(part, &hash, length).await)
    }

    /// Moves a part that has arrived whole to the destination, once the
    /// release key is seen to have signed exactly what it holds.
    ///
    /// The signature is checked against the part as it was just measured and
    /// hashed, and not against the build as offered, which the comparisons
    /// before this have held it to already: so that a fault in any of them,
    /// or a check of the size or hash taken out by a later change, still
    /// moves nothing into place that the key did not sign. This is the last
    /// step before an installer that runs whatever it is given; on Windows
    /// that is a setup run silently.
    async fn settle(&self, part: &Path, sha256: &str, length: u64) -> Result<(), UpdateError> {
        if let Some(key) = &self.key {
            let arrived = Signed {
                size: length,
                sha256,
                ..Signed::of(&self.build)
            };
            if !arrived.vouched_for_by(key, &self.build.signature) {
                discard(part).await?;
                return Err(UpdateError::Mismatch {
                    detail: "what arrived is not a build the release key signed".into(),
                });
            }
        }

        fs::rename(part, &self.path).await.map_err(storage)
    }

    /// Fetches the bytes from `have` onwards into `part`.
    async fn receive(
        &self,
        part: &Path,
        have: u64,
        progress: &dyn UpdateProgress,
    ) -> Result<(), Received> {
        let size = self.build.size;
        let path = format!("/api/apps/latest/{}/file", self.build.platform.section());
        let mut request = self
            .api
            .request(Method::GET, &path)
            .map_err(|fault| Received::Failed(protocol(fault)))?;
        if have > 0 {
            request = request
                .header(RANGE, format!("bytes={have}-"))
                .header(IF_RANGE, format!("\"{}\"", self.build.sha256));
        }

        let mut stop = self.stop.subscribe();
        let mut response = tokio::select! {
            sent = request.send() => sent.map_err(|fault| Received::Failed(network_fault(fault)))?,
            _ = stop.wait_for(|stopped| *stopped) => return Err(Received::Failed(UpdateError::Cancelled)),
        };

        let status = response.status().as_u16();
        if !(200..300).contains(&status) && status != 416 {
            return Err(Received::Failed(refusal(response).await));
        }

        // The hash is the ETag (updates.go). Where it is there and is not
        // this build's, the deployment has another build now, whatever range
        // it is answering with.
        if let Some(tag) = header(&response, ETAG.as_str()) {
            let tag = tag.trim().trim_start_matches("W/").trim_matches('"');
            if !tag.eq_ignore_ascii_case(&self.build.sha256) {
                discard(part).await.map_err(Received::Failed)?;
                return Err(Received::Failed(UpdateError::Superseded));
            }
        }

        let (mut file, mut at) = match status {
            206 => {
                // `bytes <first>-<last>/<whole>`: the rest of this file, from
                // exactly where the part ends, or nothing can be added to it.
                let range = header(&response, CONTENT_RANGE.as_str()).unwrap_or_default();
                let Some((first, whole)) = content_range(&range) else {
                    return Err(Received::Restart);
                };
                if whole.is_some_and(|whole| whole != size) {
                    discard(part).await.map_err(Received::Failed)?;
                    return Err(Received::Failed(UpdateError::Superseded));
                }
                if first != have {
                    return Err(Received::Restart);
                }
                let file = fs::OpenOptions::new()
                    .append(true)
                    .open(part)
                    .await
                    .map_err(|fault| Received::Failed(storage(fault)))?;
                (file, have)
            }
            // The range was not satisfiable: the part says it has more of the
            // file than the file has.
            416 => return Err(Received::Restart),
            _ => {
                // The whole file, because nothing was asked for or because
                // the deployment would not give a range of it.
                if let Some(length) = header(&response, CONTENT_LENGTH.as_str())
                    .and_then(|length| length.parse::<u64>().ok())
                {
                    if length != size {
                        discard(part).await.map_err(Received::Failed)?;
                        return Err(Received::Failed(UpdateError::Superseded));
                    }
                }
                let file = fs::File::create(part)
                    .await
                    .map_err(|fault| Received::Failed(storage(fault)))?;
                (file, 0)
            }
        };

        progress.progressed(at, size);
        let mut said = Instant::now();

        let streamed: Result<(), UpdateError> = async {
            loop {
                let chunk = tokio::select! {
                    chunk = response.chunk() => chunk.map_err(network_fault)?,
                    _ = stop.wait_for(|stopped| *stopped) => return Err(UpdateError::Cancelled),
                };
                let Some(chunk) = chunk else { break };

                if at + chunk.len() as u64 > size {
                    return Err(UpdateError::Mismatch {
                        detail: format!("more than the {size} bytes offered arrived"),
                    });
                }
                file.write_all(&chunk).await.map_err(storage)?;
                at += chunk.len() as u64;

                if said.elapsed() >= PROGRESS_EVERY {
                    progress.progressed(at, size);
                    said = Instant::now();
                }
            }
            Ok(())
        }
        .await;

        // What arrived is written out before anything else is decided, on
        // every way out: a tokio file that is dropped mid-write finishes it
        // later on a thread of its own, and the next attempt measuring the
        // part could read it short.
        let flushed = async {
            file.flush().await?;
            file.sync_all().await
        }
        .await;
        drop(file);

        match streamed {
            Err(fault @ UpdateError::Mismatch { .. }) => {
                discard(part).await.map_err(Received::Failed)?;
                return Err(Received::Failed(fault));
            }
            Err(fault) => return Err(Received::Failed(fault)),
            Ok(()) => {}
        }
        flushed.map_err(|fault| Received::Failed(storage(fault)))?;

        if at < size {
            // Kept: the next download of this build carries on from here.
            return Err(Received::Failed(UpdateError::Network {
                detail: format!("the download stopped at {at} of {size} bytes"),
            }));
        }

        Ok(())
    }
}

enum Received {
    Failed(UpdateError),
    Restart,
}

/// `bytes 100-199/200` as its first byte and, where it is said, the whole.
fn content_range(value: &str) -> Option<(u64, Option<u64>)> {
    let rest = value.trim().strip_prefix("bytes ")?;
    let (span, whole) = rest.split_once('/')?;
    let (first, _) = span.split_once('-')?;
    let first = first.trim().parse().ok()?;
    let whole = match whole.trim() {
        "*" => None,
        whole => Some(whole.parse().ok()?),
    };

    Some((first, whole))
}

fn header(response: &Response, name: &str) -> Option<String> {
    response
        .headers()
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned)
}

/// A refusal of the file, read as `Answer::refused` reads one: the server's
/// code, `not_found` for Go's plain 404, and nothing otherwise.
async fn refusal(response: Response) -> UpdateError {
    let status = response.status().as_u16();
    if (300..400).contains(&status) {
        return UpdateError::Unexpected {
            detail: format!("the deployment sent the download elsewhere ({status})"),
        };
    }

    let body = response.bytes().await.unwrap_or_default();
    let value: Option<Value> = serde_json::from_slice(&body).ok();
    let code = match &value {
        Some(value) => value
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        None if status == 404 => "not_found".to_owned(),
        None => String::new(),
    };

    UpdateError::Refused { status, code }
}

fn network_fault(fault: reqwest::Error) -> UpdateError {
    match network(fault) {
        ProtocolError::Network { detail } => UpdateError::Network { detail },
        other => protocol(other),
    }
}

fn protocol(fault: ProtocolError) -> UpdateError {
    match fault {
        ProtocolError::Refused { status, code } => UpdateError::Refused { status, code },
        ProtocolError::Network { detail } => UpdateError::Network { detail },
        ProtocolError::Unexpected { detail } | ProtocolError::Invalid { detail } => {
            UpdateError::Unexpected { detail }
        }
    }
}

fn storage(fault: std::io::Error) -> UpdateError {
    UpdateError::Storage {
        detail: fault.to_string(),
    }
}

/// Removes a file that may not be there.
async fn discard(path: &Path) -> Result<(), UpdateError> {
    match fs::remove_file(path).await {
        Ok(()) => Ok(()),
        Err(fault) if fault.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(fault) => Err(storage(fault)),
    }
}

/// A file's SHA-256 in lower-case hexadecimal, and how many bytes were read
/// to take it: the length of what was hashed, rather than one asked of the
/// file system before or after, which a file still being written to could
/// make another number.
fn sha256_of(path: &Path) -> std::io::Result<(String, u64)> {
    let mut file = std::fs::File::open(path)?;
    let mut context = ring::digest::Context::new(&ring::digest::SHA256);
    let mut buffer = vec![0u8; 1 << 20];
    let mut length = 0u64;
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        context.update(&buffer[..read]);
        length += read as u64;
    }

    let hash = context
        .finish()
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();

    Ok((hash, length))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn newer(offered: &str, running: &str) -> bool {
        version_is_newer(offered.into(), running.into())
    }

    #[test]
    fn versions_are_compared_part_by_part_as_numbers() {
        assert!(newer("0.1.0.10", "0.1.0.9"), "as numbers, not as text");
        assert!(newer("0.1.0.8", "0.1.0.7"));
        assert!(newer("0.2", "0.1.9.9"));
        assert!(newer("1.0.0.0", "0.99.99.99"));
        assert!(newer("0.1.0.1", "0.1.0"), "a missing part is nought");
        assert!(!newer("0.1.0.7", "0.1.0.7"));
        assert!(!newer("0.1.0.6", "0.1.0.7"));
        assert!(!newer("0.1.0", "0.1.0.0"));
        assert!(!newer("0.1.0.0", "0.1.0"));
        assert!(
            !newer("0.01.0.7", "0.1.0.7"),
            "leading noughts are the same number"
        );
        assert!(newer(" 0.1.0.8 ", "0.1.0.7"));
    }

    #[test]
    fn a_later_build_is_the_version_with_its_build_as_a_fourth_part() {
        let joined = |v: &str, b: &str| version_with_build(v.into(), b.into());
        assert_eq!(
            joined("0.1.4", "1"),
            "0.1.4",
            "the first build is the version alone"
        );
        assert_eq!(joined("0.1.4", "2"), "0.1.4.2");
        assert_eq!(joined("0.1.4", "12"), "0.1.4.12");
        assert_eq!(joined("0.1.4", ""), "0.1.4", "no build is the first");
        assert_eq!(
            joined("0.1.4", "2a"),
            "0.1.4",
            "nor is one that is not a number"
        );
        assert!(
            newer(&joined("0.1.4", "2"), &joined("0.1.4", "1")),
            "a later build is newer"
        );
        assert!(newer(&joined("0.1.4", "3"), &joined("0.1.4", "2")));
        assert!(
            !newer(&joined("0.1.4", "1"), "0.1.4"),
            "the first build is not offered to itself"
        );
        assert!(
            newer(&joined("0.1.5", "1"), &joined("0.1.4", "9")),
            "a new version is after every build of the last"
        );
        assert!(
            newer(&joined("0.1.4", "2"), "0.1.3"),
            "and an app from before builds were said takes them"
        );
    }

    #[test]
    fn a_later_build_is_labelled_with_its_build_in_brackets() {
        let label = |v: &str| version_label(v.into());
        assert_eq!(label("0.1.4.2"), "0.1.4 (2)");
        assert_eq!(label("0.1.4.12"), "0.1.4 (12)");
        assert_eq!(label("0.1.4"), "0.1.4");
        assert_eq!(
            label("0.1.4.1"),
            "0.1.4",
            "the old Macs' joined first build"
        );
        assert_eq!(label(" 0.1.4.2 "), "0.1.4 (2)");
        assert_eq!(label("0.1"), "0.1");
        assert_eq!(label("beta"), "beta");
    }

    #[test]
    fn what_is_not_a_version_is_never_newer() {
        for wrong in [
            "",
            "0.1.0.x",
            "0.1..8",
            ".1",
            "1.",
            "v0.1.0.8",
            "0.1.0 (8)",
            "0.1.-1",
            "99999999999999999999999.0",
        ] {
            assert!(!newer(wrong, "0.1.0.7"), "{wrong:?} was offered");
            assert!(
                !newer("9.9.9.9", wrong),
                "{wrong:?} was taken as a running version"
            );
        }
    }

    #[test]
    fn a_content_range_says_where_it_starts_and_how_much_there_is() {
        assert_eq!(content_range("bytes 100-199/200"), Some((100, Some(200))));
        assert_eq!(content_range("bytes 0-0/*"), Some((0, None)));
        assert_eq!(content_range("bytes */200"), None);
        assert_eq!(content_range("items 0-1/2"), None);
    }

    #[test]
    fn notes_are_in_the_readers_language_then_english_then_none() {
        let notes: HashMap<String, String> =
            [("zh-Hans", "简体"), ("ja", "日本語"), ("en", "English")]
                .into_iter()
                .map(|(k, v)| (k.to_owned(), v.to_owned()))
                .collect();

        assert_eq!(notes_in(&notes, "zh-Hans").as_deref(), Some("简体"));
        assert_eq!(notes_in(&notes, "ja").as_deref(), Some("日本語"));
        assert_eq!(notes_in(&notes, "ja-JP").as_deref(), Some("日本語"));
        assert_eq!(notes_in(&notes, "ZH-hans").as_deref(), Some("简体"));
        assert_eq!(
            notes_in(&notes, "zh-Hant").as_deref(),
            Some("English"),
            "not the other script"
        );
        assert_eq!(notes_in(&notes, "fr").as_deref(), Some("English"));
        assert_eq!(notes_in(&notes, "").as_deref(), Some("English"));

        let none_in_english: HashMap<String, String> =
            [("ja".to_owned(), "日本語".to_owned())].into();
        assert_eq!(notes_in(&none_in_english, "zh-Hans"), None);
        assert_eq!(notes_in(&HashMap::new(), "en"), None);
    }

    // ------------------------------------------------------------------------
    // The release key's signatures.
    //
    // What would go wrong here without anybody seeing it: a build offered or
    // moved into place that the key never signed, which is the whole of what
    // the key is for and which nothing on the screen would show; or a build
    // the key did sign refused, because the lines the release tooling signs
    // and the lines checked here had drifted apart, which would show only as
    // updates that stop coming. Each guard was taken out once to see its test
    // fail.

    use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};

    use ring::signature::{Ed25519KeyPair, KeyPair as _};
    use serde_json::json;
    use tokio::io::AsyncReadExt as _;

    /// The tests' own key, never any deployment's. The fixture manifest in
    /// tests/fixtures/updates was signed with it by the release tooling.
    const TEST_SEED: &[u8; 32] = b"tomoshibi update tests' own key!";
    const TEST_PUBLIC: &str = "559eabd75294200c1aa880c697863f2cff8a968adc13c279a058deff6e44ee1a";

    const SETUP: &str = "Tomoshibi-Setup-0.1.4.2.exe";

    fn test_pair(seed: &[u8; 32]) -> (Ed25519KeyPair, [u8; 32]) {
        let pair = Ed25519KeyPair::from_seed_unchecked(seed).unwrap();
        let public = pair.public_key().as_ref().try_into().unwrap();
        (pair, public)
    }

    fn hash_of(bytes: &[u8]) -> String {
        ring::digest::digest(&ring::digest::SHA256, bytes)
            .as_ref()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }

    fn signature_of(pair: &Ed25519KeyPair, signed: &Signed) -> String {
        BASE64.encode(pair.sign(&signed.message()).as_ref())
    }

    /// updates.go's answer for a Windows build of `bytes`, signed by `pair`
    /// as it says.
    fn signed_answer(pair: &Ed25519KeyPair, bytes: &[u8]) -> Value {
        let sha256 = hash_of(bytes);
        let sig = signature_of(
            pair,
            &Signed {
                platform: UpdatePlatform::Windows,
                version: "0.1.4.2",
                file_name: SETUP,
                size: bytes.len() as u64,
                sha256: &sha256,
            },
        );
        json!({
            "platform": "windows", "version": "0.1.4.2", "file": SETUP,
            "size": bytes.len(), "sha256": sha256, "sig": sig,
            "notes": { "en": "Fixed." },
        })
    }

    fn with(answer: &Value, field: &str, value: Value) -> Value {
        let mut changed = answer.clone();
        changed[field] = value;
        changed
    }

    fn without(answer: &Value, field: &str) -> Value {
        let mut changed = answer.clone();
        changed.as_object_mut().unwrap().remove(field);
        changed
    }

    fn refused(said: &Result<AppBuild, ProtocolError>) -> bool {
        matches!(said, Err(ProtocolError::Unexpected { .. }))
    }

    #[test]
    fn the_lines_signed_are_these_and_nothing_else() {
        let sha256 = "ab".repeat(32);
        let signed = Signed {
            platform: UpdatePlatform::Windows,
            version: "0.1.4.2",
            file_name: SETUP,
            size: 145012345,
            sha256: &sha256,
        };
        assert_eq!(
            String::from_utf8(signed.message()).unwrap(),
            format!(
                "tomoshibi-update-v1\nplatform=windows\nversion=0.1.4.2\n\
                 file={SETUP}\nsize=145012345\nsha256={sha256}\n"
            ),
            "the release tooling signs exactly these lines"
        );
    }

    #[test]
    fn a_signed_build_is_offered_with_its_signature() {
        let (pair, key) = test_pair(TEST_SEED);
        let answer = signed_answer(&pair, b"a setup");

        let build = read_build(UpdatePlatform::Windows, &answer, Some(&key)).unwrap();
        assert_eq!(build.file_name, SETUP);
        assert_eq!(build.signature, answer["sig"].as_str().unwrap());

        // The hash in capitals, as a manifest written by hand may have it:
        // signed in lower case, which is how it is read.
        let shouting = with(
            &answer,
            "sha256",
            json!(hash_of(b"a setup").to_ascii_uppercase()),
        );
        assert!(read_build(UpdatePlatform::Windows, &shouting, Some(&key)).is_ok());
    }

    #[test]
    fn a_build_changed_after_it_was_signed_is_not_offered() {
        let (pair, key) = test_pair(TEST_SEED);
        let answer = signed_answer(&pair, b"a setup");

        for (field, value) in [
            ("size", json!(8)),
            ("sha256", json!(hash_of(b"another setup"))),
            ("version", json!("0.1.4.3")),
            ("version", json!("0.1.5")),
            ("file", json!("Tomoshibi-Setup-0.1.4.3.exe")),
        ] {
            let said = read_build(
                UpdatePlatform::Windows,
                &with(&answer, field, value.clone()),
                Some(&key),
            );
            assert!(
                refused(&said),
                "{field} changed to {value} was offered: {said:?}"
            );
        }

        // The Mac's build, signed as the Mac's, handed to Windows: everything
        // else about it is what the key signed.
        let as_mac = signature_of(
            &pair,
            &Signed {
                platform: UpdatePlatform::Mac,
                version: "0.1.4.2",
                file_name: SETUP,
                size: 7,
                sha256: &hash_of(b"a setup"),
            },
        );
        let moved = with(&answer, "sig", json!(as_mac));
        assert!(
            refused(&read_build(UpdatePlatform::Windows, &moved, Some(&key))),
            "a signature made for the Mac passed for Windows"
        );
        assert!(
            read_build(
                UpdatePlatform::Mac,
                &with(&moved, "platform", json!("mac")),
                Some(&key)
            )
            .is_ok(),
            "and it is a good signature, for the Mac"
        );
    }

    #[test]
    fn a_build_with_no_signature_or_a_broken_one_is_not_offered_where_there_is_a_key() {
        let (pair, key) = test_pair(TEST_SEED);
        let answer = signed_answer(&pair, b"a setup");
        let raw = BASE64.decode(answer["sig"].as_str().unwrap()).unwrap();
        let mut turned = raw.clone();
        turned[10] ^= 1;

        for (what, broken) in [
            ("no signature", without(&answer, "sig")),
            ("an empty one", with(&answer, "sig", json!(""))),
            ("one that is not text", with(&answer, "sig", json!(7))),
            (
                "one that is not base64",
                with(&answer, "sig", json!("not base64 at all!")),
            ),
            (
                "one a byte short",
                with(&answer, "sig", json!(BASE64.encode(&raw[..63]))),
            ),
            (
                "one a byte long",
                with(
                    &answer,
                    "sig",
                    json!(BASE64.encode([raw.as_slice(), &[0]].concat())),
                ),
            ),
            (
                "one with a bit turned",
                with(&answer, "sig", json!(BASE64.encode(&turned))),
            ),
            ("no file name", without(&answer, "file")),
        ] {
            let said = read_build(UpdatePlatform::Windows, &broken, Some(&key));
            assert!(refused(&said), "{what} was offered: {said:?}");
        }

        // A core built with no key, the tests' or a local deployment's,
        // offers the same build unsigned, as every core before signatures did.
        let build = read_build(UpdatePlatform::Windows, &without(&answer, "sig"), None).unwrap();
        assert_eq!(build.signature, "");
    }

    #[test]
    fn a_signature_by_another_key_is_not_the_release_keys() {
        let (_, key) = test_pair(TEST_SEED);
        let (other, other_key) = test_pair(&[9; 32]);
        let answer = signed_answer(&other, b"a setup");

        assert!(refused(&read_build(
            UpdatePlatform::Windows,
            &answer,
            Some(&key)
        )));
        assert!(read_build(UpdatePlatform::Windows, &answer, Some(&other_key)).is_ok());
    }

    #[test]
    fn an_ios_build_needs_no_signature_and_keeps_none() {
        let (pair, key) = test_pair(TEST_SEED);

        let bare = json!({ "platform": "ios", "version": "0.1.6.4" });
        assert!(read_build(UpdatePlatform::Ios, &bare, Some(&key)).is_ok());

        // A Windows line copied into the iOS list, signature and all.
        let copied = with(&signed_answer(&pair, b"a setup"), "platform", json!("ios"));
        let build = read_build(UpdatePlatform::Ios, &copied, Some(&key)).unwrap();
        assert_eq!(
            (
                build.size,
                build.file_name.as_str(),
                build.signature.as_str()
            ),
            (0, "", "")
        );
    }

    #[test]
    fn nothing_that_could_carry_a_line_into_what_is_signed_is_vouched_for() {
        for name in [
            "Tomoshibi-0.1.6.dmg",
            "Tomoshibi-0.1.6.2.apk",
            SETUP,
            "a_b+c-d.exe",
        ] {
            assert!(signable_name(name), "{name:?}");
        }
        for name in [
            String::new(),
            ".Tomoshibi.exe".into(),
            "a\nsize=1.exe".into(),
            "a/b.exe".into(),
            "a=b.exe".into(),
            "Tomoshibi Setup.exe".into(),
            "x".repeat(129),
        ] {
            assert!(!signable_name(&name), "{name:?}");
        }

        // Signatures the key really made, over lines a value broke into, are
        // refused all the same: what they say is not what one build is.
        let (pair, key) = test_pair(TEST_SEED);
        let sha256 = hash_of(b"x");
        let upper = sha256.to_ascii_uppercase();
        for (version, file_name, sha256) in [
            ("0.1.4.2", "a\nsize=1.exe", sha256.as_str()),
            ("0.1.4.2\nfile=x", SETUP, sha256.as_str()),
            (" 0.1.4.2", SETUP, sha256.as_str()),
            ("0.1.4.2", SETUP, upper.as_str()),
        ] {
            let signed = Signed {
                platform: UpdatePlatform::Windows,
                version,
                file_name,
                size: 1,
                sha256,
            };
            assert!(
                !signed.vouched_for_by(&key, &signature_of(&pair, &signed)),
                "{version:?} {file_name:?} {sha256:?}"
            );
        }
    }

    #[test]
    fn the_key_is_64_hexadecimal_digits_or_none() {
        let (_, key) = test_pair(TEST_SEED);
        assert_eq!(key_from_hex(TEST_PUBLIC), Some(key));
        assert_eq!(key_from_hex(&TEST_PUBLIC.to_ascii_uppercase()), Some(key));
        for wrong in [
            String::new(),
            "none".into(),
            TEST_PUBLIC[..62].to_owned(),
            format!("{TEST_PUBLIC}00"),
            format!("+f{}", &TEST_PUBLIC[2..]),
            format!("0x{}", &TEST_PUBLIC[2..]),
            format!("g{}", &TEST_PUBLIC[1..]),
        ] {
            assert_eq!(key_from_hex(&wrong), None, "{wrong:?}");
        }
        assert_eq!(update_key().is_some(), release_key().is_some());
    }

    /// updates.go's answer for a manifest line to an app from 0.1.4 on: the
    /// version with its build joined on (withBuild, buildOf), the hash in
    /// lower case, and the file and signature as the line has them.
    fn as_offered(section: &str, line: &Value) -> Value {
        let build = match &line["build"] {
            Value::String(build) => build.clone(),
            Value::Number(build) => build.to_string(),
            _ => String::new(),
        };
        json!({
            "platform": section,
            "version": version_with_build(line["version"].as_str().unwrap().to_owned(), build),
            "size": line["size"],
            "sha256": line["sha256"].as_str().unwrap().to_ascii_lowercase(),
            "file": line["file"],
            "sig": line["sig"],
        })
    }

    /// Every file line of `manifest`, checked as offered against `key`.
    fn check_manifest(manifest: &Value, key: &[u8; 32]) -> usize {
        let mut checked = 0;
        for (section, platform) in [
            ("mac", UpdatePlatform::Mac),
            ("android", UpdatePlatform::Android),
            ("windows", UpdatePlatform::Windows),
        ] {
            for line in manifest[section].as_array().into_iter().flatten() {
                if line.get("file").is_none() {
                    continue;
                }
                let answer = as_offered(section, line);
                let build = read_build(platform, &answer, Some(key))
                    .unwrap_or_else(|fault| panic!("{section} {line} was refused: {fault:?}"));
                eprintln!("verified {section} {} {}", build.version, build.file_name);

                let mut hash = build.sha256.clone();
                let last = if hash.ends_with('0') { "1" } else { "0" };
                hash.replace_range(63.., last);
                assert!(
                    refused(&read_build(
                        platform,
                        &with(&answer, "sha256", json!(hash)),
                        Some(key)
                    )),
                    "{section} {line} was still offered with another hash"
                );
                checked += 1;
            }
        }
        checked
    }

    #[test]
    fn what_the_release_tooling_signed_is_what_the_core_accepts() {
        let manifest: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/updates/apps.signed.json"
        ))
        .unwrap();
        let key = key_from_hex(TEST_PUBLIC).unwrap();

        assert_eq!(check_manifest(&manifest, &key), 5);
    }

    /// A manifest checked by this core's own rule, against a key given with
    /// it, before it goes onto a shelf: run by hand, with
    /// TOMOSHIBI_CHECK_MANIFEST naming the apps.json and TOMOSHIBI_CHECK_KEY
    /// holding the key's 64 hexadecimal digits, as
    /// `cargo test -p tomoshibi-protocol --lib -- --ignored a_manifest_given`.
    #[test]
    #[ignore = "checks a manifest named in the environment"]
    fn a_manifest_given_to_check_is_signed_by_its_key() {
        let path = std::env::var("TOMOSHIBI_CHECK_MANIFEST").expect("TOMOSHIBI_CHECK_MANIFEST");
        let key = key_from_hex(
            std::env::var("TOMOSHIBI_CHECK_KEY")
                .expect("TOMOSHIBI_CHECK_KEY")
                .trim(),
        )
        .expect("TOMOSHIBI_CHECK_KEY is not 64 hexadecimal digits");
        let manifest: Value =
            serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();

        assert!(check_manifest(&manifest, &key) > 0, "no line with a file");
    }

    // The download, with a key.

    struct Quiet;

    impl UpdateProgress for Quiet {
        fn progressed(&self, _: u64, _: u64) {}
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir()
            .join("tomoshibi-update-signatures")
            .join(format!("{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn api_at(base: &str) -> Api {
        Deployment::new(base.to_owned()).unwrap().api.clone()
    }

    fn signed_build(pair: &Ed25519KeyPair, bytes: &[u8]) -> AppBuild {
        read_build(UpdatePlatform::Windows, &signed_answer(pair, bytes), None).unwrap()
    }

    /// updates.go's `/file`, handing over `bytes` whole to whoever asks, and
    /// counting who did.
    async fn shelf(bytes: Vec<u8>) -> (String, Arc<AtomicUsize>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let asked = Arc::new(AtomicUsize::new(0));
        let counted = asked.clone();

        tokio::spawn(async move {
            while let Ok((mut socket, _)) = listener.accept().await {
                counted.fetch_add(1, SeqCst);
                let bytes = bytes.clone();
                tokio::spawn(async move {
                    let mut head = Vec::new();
                    let mut chunk = [0u8; 1024];
                    while !head.windows(4).any(|end| end == b"\r\n\r\n") {
                        match socket.read(&mut chunk).await {
                            Ok(0) | Err(_) => return,
                            Ok(read) => head.extend_from_slice(&chunk[..read]),
                        }
                    }
                    let reply = format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nETag: \"{}\"\r\nConnection: close\r\n\r\n",
                        bytes.len(),
                        hash_of(&bytes)
                    );
                    let _ = socket.write_all(reply.as_bytes()).await;
                    let _ = socket.write_all(&bytes).await;
                    let _ = socket.shutdown().await;
                });
            }
        });

        (base, asked)
    }

    #[tokio::test]
    async fn a_signed_build_arrives_whole_with_the_key() {
        let (pair, key) = test_pair(TEST_SEED);
        let bytes = b"a setup the key signed ".repeat(4000);
        let (base, asked) = shelf(bytes.clone()).await;
        let dir = scratch("signed");
        let destination = dir.join(SETUP);

        UpdateDownload::new(
            api_at(&base),
            signed_build(&pair, &bytes),
            destination.clone(),
            Some(key),
        )
        .fetch(&Quiet)
        .await
        .unwrap();

        assert_eq!(std::fs::read(&destination).unwrap(), bytes);
        assert_eq!(asked.load(SeqCst), 1);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn a_build_the_key_did_not_sign_is_never_asked_for() {
        let (pair, key) = test_pair(TEST_SEED);
        let (other, _) = test_pair(&[9; 32]);
        let bytes = b"a setup ".repeat(4000);
        let (base, asked) = shelf(bytes.clone()).await;
        let signed = signed_build(&pair, &bytes);

        for (what, build) in [
            (
                "unsigned",
                AppBuild {
                    signature: String::new(),
                    ..signed.clone()
                },
            ),
            ("signed by another key", signed_build(&other, &bytes)),
            (
                "said to be another size",
                AppBuild {
                    size: signed.size + 1,
                    ..signed.clone()
                },
            ),
            (
                "said to be another version",
                AppBuild {
                    version: "0.1.4.3".into(),
                    ..signed.clone()
                },
            ),
        ] {
            let dir = scratch("unsigned");
            let said = UpdateDownload::new(api_at(&base), build, dir.join(SETUP), Some(key))
                .fetch(&Quiet)
                .await;
            assert!(
                matches!(&said, Err(UpdateError::Unexpected { detail }) if detail.contains("release key")),
                "{what}: {said:?}"
            );
            assert!(!dir.exists(), "{what}: the download touched the disk");
        }
        assert_eq!(
            asked.load(SeqCst),
            0,
            "the deployment was asked for a build the key did not sign"
        );

        // A core with no key downloads the same unsigned build, as before.
        let dir = scratch("unkeyed");
        let unsigned = AppBuild {
            signature: String::new(),
            ..signed
        };
        UpdateDownload::new(api_at(&base), unsigned, dir.join(SETUP), None)
            .fetch(&Quiet)
            .await
            .unwrap();
        assert_eq!(asked.load(SeqCst), 1);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn a_part_that_is_not_what_the_key_signed_is_never_moved_into_place() {
        let (pair, key) = test_pair(TEST_SEED);
        let good = b"the build the key signed".to_vec();
        let other = b"a build nobody had signed".to_vec();
        let dir = scratch("settle");
        std::fs::create_dir_all(&dir).unwrap();
        let destination = dir.join(SETUP);
        let download = UpdateDownload::new(
            api_at("http://127.0.0.1:9"),
            signed_build(&pair, &good),
            destination.clone(),
            Some(key),
        );
        let part = download.part();

        // As though every comparison before had let these through: the part
        // is measured and hashed as it is, and the key signed neither.
        for (what, bytes, length) in [
            ("another file", &other, other.len() as u64),
            ("the build, measured longer", &good, good.len() as u64 + 1),
        ] {
            std::fs::write(&part, bytes).unwrap();
            let said = download.settle(&part, &hash_of(bytes), length).await;
            assert!(
                matches!(said, Err(UpdateError::Mismatch { .. })),
                "{what}: {said:?}"
            );
            assert!(!part.exists(), "{what}: kept to be resumed");
            assert!(!destination.exists(), "{what}: moved into place");
        }

        std::fs::write(&part, &good).unwrap();
        download
            .settle(&part, &hash_of(&good), good.len() as u64)
            .await
            .unwrap();
        assert_eq!(std::fs::read(&destination).unwrap(), good);
        let _ = std::fs::remove_dir_all(dir);
    }
}
