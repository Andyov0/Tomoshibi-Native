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

use reqwest::{
    header::{CONTENT_LENGTH, CONTENT_RANGE, ETAG, IF_RANGE, RANGE},
    Method, Response,
};
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

fn notes_in(notes: &HashMap<String, String>, language: &str) -> Option<String> {
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
    /// with nothing asked of the deployment and nothing written to disk.
    pub fn download_update(&self, build: AppBuild, path: String) -> Arc<UpdateDownload> {
        let (stop, _) = watch::channel(false);

        Arc::new(UpdateDownload {
            api: self.api.clone(),
            build,
            path: PathBuf::from(path),
            stop,
        })
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
        let build = read_build(platform, &body)?;

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
fn read_build(platform: UpdatePlatform, body: &Value) -> Result<AppBuild, ProtocolError> {
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

    let (size, sha256) = if platform.elsewhere() {
        (0, String::new())
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

        (size, sha256)
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

    Ok(AppBuild {
        platform,
        version,
        size,
        sha256,
        notes,
    })
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

        let hash = {
            let part = part.to_owned();
            match tokio::task::spawn_blocking(move || sha256_of(&part)).await {
                Ok(Ok(hash)) => hash,
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

        Attempt::Done(fs::rename(part, &self.path).await.map_err(storage))
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

fn sha256_of(path: &Path) -> std::io::Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut context = ring::digest::Context::new(&ring::digest::SHA256);
    let mut buffer = vec![0u8; 1 << 20];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        context.update(&buffer[..read]);
    }

    Ok(context
        .finish()
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
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
        assert_eq!(joined("0.1.4", "1"), "0.1.4", "the first build is the version alone");
        assert_eq!(joined("0.1.4", "2"), "0.1.4.2");
        assert_eq!(joined("0.1.4", "12"), "0.1.4.12");
        assert_eq!(joined("0.1.4", ""), "0.1.4", "no build is the first");
        assert_eq!(joined("0.1.4", "2a"), "0.1.4", "nor is one that is not a number");
        assert!(newer(&joined("0.1.4", "2"), &joined("0.1.4", "1")), "a later build is newer");
        assert!(newer(&joined("0.1.4", "3"), &joined("0.1.4", "2")));
        assert!(!newer(&joined("0.1.4", "1"), "0.1.4"), "the first build is not offered to itself");
        assert!(newer(&joined("0.1.5", "1"), &joined("0.1.4", "9")), "a new version is after every build of the last");
        assert!(newer(&joined("0.1.4", "2"), "0.1.3"), "and an app from before builds were said takes them");
    }

    #[test]
    fn a_later_build_is_labelled_with_its_build_in_brackets() {
        let label = |v: &str| version_label(v.into());
        assert_eq!(label("0.1.4.2"), "0.1.4 (2)");
        assert_eq!(label("0.1.4.12"), "0.1.4 (12)");
        assert_eq!(label("0.1.4"), "0.1.4");
        assert_eq!(label("0.1.4.1"), "0.1.4", "the old Macs' joined first build");
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
}
