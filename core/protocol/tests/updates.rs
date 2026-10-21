//! The apps' updates against a small server started in each test, which
//! answers the two addresses of internal/app/updates.go the way they answer
//! and can be told to misbehave: to break a download off half way, to hold it
//! open with nothing more coming, to ignore a range, to hand over another
//! build than the one it described.
//!
//! What is guarded is what would go wrong without anybody seeing it. A
//! version compared as text offers 0.1.0.9 to somebody on 0.1.0.10. A hash
//! not checked installs whatever arrived. A resumed download that does not
//! ask for the rest of the same build splices two builds together. A part
//! not kept after a broken connection downloads fifty megabytes again on a
//! phone's data. A cancel that does not stop the transfer goes on spending
//! it. An iOS build, which TestFlight installs, read as if it had a file
//! offers a download nobody can install, and downloaded at all spends a
//! request on a phone for a file the deployment does not have. Each test
//! below fails if its guard is taken out; that was tried.

use std::{
    collections::HashMap,
    io,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

use serde_json::{json, Value};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};
use tomoshibi_protocol::{
    api::{
        update_notes, version_is_newer, AppBuild, Deployment, UpdateCheck, UpdateError,
        UpdatePlatform, UpdateProgress,
    },
    ProtocolError,
};

// ----------------------------------------------------------------------------
// The server.

#[derive(Debug, Clone)]
struct Seen {
    target: String,
    headers: Vec<(String, String)>,
}

impl Seen {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }
}

/// What the server does with one request.
struct Reply {
    status: u16,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
    /// Sends this many bytes of the body and closes the connection, having
    /// promised all of it.
    cut_after: Option<usize>,
    /// Sends this many bytes and then nothing more, holding the connection.
    stall_after: Option<usize>,
    /// Says no length, so the body runs until the connection closes, as an
    /// HTTP/1.0 proxy may hand it on.
    unmeasured: bool,
}

impl Reply {
    fn json(status: u16, body: Value) -> Self {
        Self::bytes(status, body.to_string().into_bytes()).with("Content-Type", "application/json")
    }

    fn bytes(status: u16, body: Vec<u8>) -> Self {
        Self {
            status,
            headers: vec![],
            body,
            cut_after: None,
            stall_after: None,
            unmeasured: false,
        }
    }

    /// Go's `http.NotFound`.
    fn not_found() -> Self {
        Self::bytes(404, b"404 page not found\n".to_vec())
            .with("Content-Type", "text/plain; charset=utf-8")
    }

    fn with(mut self, name: &str, value: &str) -> Self {
        self.headers.push((name.into(), value.into()));
        self
    }
}

type Handler = Arc<dyn Fn(&Seen) -> Reply + Send + Sync>;

struct Stub {
    base: String,
    seen: Arc<Mutex<Vec<Seen>>>,
}

impl Stub {
    async fn start(handler: impl Fn(&Seen) -> Reply + Send + Sync + 'static) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let seen = Arc::new(Mutex::new(Vec::new()));
        let handler: Handler = Arc::new(handler);

        let recorded = seen.clone();
        tokio::spawn(async move {
            while let Ok((socket, _)) = listener.accept().await {
                let (handler, recorded) = (handler.clone(), recorded.clone());
                tokio::spawn(async move {
                    let _ = serve(socket, handler, recorded).await;
                });
            }
        });

        Self { base, seen }
    }

    fn deployment(&self) -> Arc<Deployment> {
        Deployment::new(self.base.clone()).unwrap()
    }

    fn seen(&self) -> Vec<Seen> {
        self.seen.lock().unwrap().clone()
    }

    fn files(&self) -> Vec<Seen> {
        self.seen()
            .into_iter()
            .filter(|seen| seen.target.ends_with("/file"))
            .collect()
    }
}

async fn serve(
    mut socket: TcpStream,
    handler: Handler,
    recorded: Arc<Mutex<Vec<Seen>>>,
) -> io::Result<()> {
    let mut buffer = Vec::new();
    let end = loop {
        let mut chunk = [0u8; 4096];
        let read = socket.read(&mut chunk).await?;
        if read == 0 {
            return Ok(());
        }
        buffer.extend_from_slice(&chunk[..read]);
        if let Some(at) = buffer.windows(4).position(|window| window == b"\r\n\r\n") {
            break at;
        }
    };

    let head = String::from_utf8_lossy(&buffer[..end]).into_owned();
    let mut lines = head.split("\r\n");
    let target = lines
        .next()
        .unwrap_or_default()
        .split(' ')
        .nth(1)
        .unwrap_or_default()
        .to_owned();
    let headers = lines
        .filter_map(|line| line.split_once(':'))
        .map(|(key, value)| (key.trim().to_ascii_lowercase(), value.trim().to_owned()))
        .collect();

    let seen = Seen { target, headers };
    recorded.lock().unwrap().push(seen.clone());
    let reply = handler(&seen);

    let mut out = format!("HTTP/1.1 {} Stub\r\nConnection: close\r\n", reply.status);
    if !reply.unmeasured {
        out.push_str(&format!("Content-Length: {}\r\n", reply.body.len()));
    }
    for (name, value) in &reply.headers {
        out.push_str(&format!("{name}: {value}\r\n"));
    }
    out.push_str("\r\n");
    socket.write_all(out.as_bytes()).await?;

    if let Some(stall) = reply.stall_after {
        socket.write_all(&reply.body[..stall]).await?;
        socket.flush().await?;
        tokio::time::sleep(Duration::from_secs(60)).await;
        return Ok(());
    }
    let sent = reply.cut_after.unwrap_or(reply.body.len());
    socket.write_all(&reply.body[..sent]).await?;
    socket.shutdown().await
}

// ----------------------------------------------------------------------------
// Builds, and the file endpoint as updates.go answers it.

fn sha256(bytes: &[u8]) -> String {
    ring::digest::digest(&ring::digest::SHA256, bytes)
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// A build's bytes: not repeating, so that a splice or an offset shows in
/// the hash.
fn contents(seed: u8, length: usize) -> Vec<u8> {
    (0..length)
        .map(|at| ((at * 31 + at / 251) as u8) ^ seed)
        .collect()
}

fn build_of(bytes: &[u8], version: &str) -> AppBuild {
    AppBuild {
        platform: UpdatePlatform::Android,
        version: version.into(),
        size: bytes.len() as u64,
        sha256: sha256(bytes),
        notes: HashMap::new(),
    }
}

/// `http.ServeContent` with the hash as the ETag: a range is honoured only
/// where `If-Range` names the file as it is now.
fn serve_file(seen: &Seen, bytes: &[u8]) -> Reply {
    let tag = format!("\"{}\"", sha256(bytes));
    let range = seen
        .header("range")
        .and_then(|range| range.strip_prefix("bytes="));
    let honoured = seen
        .header("if-range")
        .map_or(true, |condition| condition == tag);

    match range.filter(|_| honoured) {
        Some(range) => {
            let first: usize = range.trim_end_matches('-').parse().unwrap();
            if first >= bytes.len() {
                return Reply::bytes(416, vec![])
                    .with("Content-Range", &format!("bytes */{}", bytes.len()));
            }
            Reply::bytes(206, bytes[first..].to_vec())
                .with("ETag", &tag)
                .with(
                    "Content-Range",
                    &format!("bytes {first}-{}/{}", bytes.len() - 1, bytes.len()),
                )
        }
        None => Reply::bytes(200, bytes.to_vec()).with("ETag", &tag),
    }
}

/// Records every progress report.
#[derive(Default)]
struct Watch {
    said: Mutex<Vec<(u64, u64)>>,
    highest: AtomicU64,
}

impl UpdateProgress for Watch {
    fn progressed(&self, received: u64, total: u64) {
        self.said.lock().unwrap().push((received, total));
        self.highest.fetch_max(received, Ordering::SeqCst);
    }
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join("tomoshibi-updates")
        .join(format!("{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn parts_in(dir: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .filter_map(|entry| entry.ok().map(|entry| entry.path()))
                .filter(|path| path.to_string_lossy().ends_with(".part"))
                .collect()
        })
        .unwrap_or_default()
}

fn part_of(destination: &Path, build: &AppBuild) -> PathBuf {
    PathBuf::from(format!(
        "{}.{}.part",
        destination.display(),
        &build.sha256[..16]
    ))
}

fn s(value: &str) -> String {
    value.to_owned()
}

// ----------------------------------------------------------------------------
// Versions and notes.

#[test]
fn a_version_is_newer_by_its_numbers_and_never_by_its_text() {
    assert!(version_is_newer(s("0.1.0.10"), s("0.1.0.9")));
    assert!(!version_is_newer(s("0.1.0.9"), s("0.1.0.10")));
    assert!(version_is_newer(s("0.1.0.1"), s("0.1.0")));
    assert!(!version_is_newer(s("0.1.0.0"), s("0.1.0")));
    assert!(!version_is_newer(s("0.1.0.8"), s("0.1.0.8")));
    assert!(!version_is_newer(s("0.1.0.8b"), s("0.1.0.7")));
}

#[test]
fn notes_are_chosen_for_the_reader() {
    let mut build = build_of(b"x", "0.1.0.8");
    build.notes = [("zh-Hant", "繁體"), ("en", "English")]
        .into_iter()
        .map(|(k, v)| (s(k), s(v)))
        .collect();

    assert_eq!(
        update_notes(build.clone(), s("zh-Hant")).as_deref(),
        Some("繁體")
    );
    assert_eq!(
        update_notes(build.clone(), s("zh-Hans")).as_deref(),
        Some("English")
    );
    build.notes.remove("en");
    assert_eq!(update_notes(build, s("ja")), None);
}

// ----------------------------------------------------------------------------
// The check.

fn latest(platform: &str, version: &str) -> Value {
    json!({
        "platform": platform,
        "version": version,
        "size": 56093610,
        "sha256": "1C66C521".to_owned() + &"0".repeat(56),
        "notes": { "zh-Hans": "  修好了  ", "en": "Fixed.", "ja": 7, "zh-Hant": "" },
    })
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_newer_build_is_offered_and_the_one_running_is_not() {
    let stub = Stub::start(|_| Reply::json(200, latest("android", "0.1.0.10"))).await;
    let deployment = stub.deployment();

    let offered = deployment
        .clone()
        .check_for_update(UpdatePlatform::Android, s("0.1.0.9"))
        .await
        .unwrap();
    let UpdateCheck::Available { build } = offered else {
        panic!("0.1.0.10 was not offered to 0.1.0.9: {offered:?}");
    };
    assert_eq!(build.version, "0.1.0.10");
    assert_eq!(build.size, 56093610);
    assert_eq!(
        build.sha256,
        "1c66c521".to_owned() + &"0".repeat(56),
        "said in lower case"
    );
    assert_eq!(
        build.notes,
        HashMap::from([(s("zh-Hans"), s("修好了")), (s("en"), s("Fixed."))]),
        "trimmed, and only notes that are text"
    );

    for running in ["0.1.0.10", "0.1.0.11", "0.2"] {
        assert_eq!(
            deployment
                .clone()
                .check_for_update(UpdatePlatform::Android, s(running))
                .await
                .unwrap(),
            UpdateCheck::UpToDate,
            "offered to {running}"
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_check_asks_for_its_own_platform_as_the_app() {
    let stub = Stub::start(|seen| {
        let platform = seen
            .target
            .rsplit('/')
            .next()
            .unwrap_or_default()
            .to_owned();
        Reply::json(200, latest(&platform, "0.1.0.8"))
    })
    .await;
    let deployment = stub.deployment();

    for (platform, path) in [
        (UpdatePlatform::Mac, "/api/apps/latest/mac"),
        (UpdatePlatform::Android, "/api/apps/latest/android"),
        (UpdatePlatform::Windows, "/api/apps/latest/windows"),
        (UpdatePlatform::Ios, "/api/apps/latest/ios"),
    ] {
        deployment
            .clone()
            .check_for_update(platform, s("0.1.0.1"))
            .await
            .unwrap();
        let seen = stub.seen().pop().unwrap();
        assert_eq!(seen.target, path);
        // What updates.go lets through, and the only thing it asks.
        let agent = seen.header("user-agent").unwrap_or_default();
        assert!(agent.starts_with("Tomoshibi/"), "{agent}");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn no_build_is_up_to_date_and_every_other_refusal_is_an_error() {
    let stub = Stub::start(|seen| match seen.target.rsplit('/').next() {
        Some("android") => Reply::json(404, json!({ "error": "no_build" })),
        Some("mac") => Reply::json(403, json!({ "error": "not_the_app" })),
        _ => Reply::not_found(),
    })
    .await;
    let deployment = stub.deployment();

    assert_eq!(
        deployment
            .clone()
            .check_for_update(UpdatePlatform::Android, s("0.1.0.1"))
            .await,
        Ok(UpdateCheck::UpToDate)
    );
    assert_eq!(
        deployment
            .clone()
            .check_for_update(UpdatePlatform::Mac, s("0.1.0.1"))
            .await,
        Err(ProtocolError::Refused {
            status: 403,
            code: s("not_the_app")
        })
    );
    // A deployment from before updates existed answers Go's plain 404, which
    // is not a deployment saying there is nothing newer: an app asked to
    // check says it could not, rather than that it is up to date.
    assert_eq!(
        deployment
            .clone()
            .check_for_update(UpdatePlatform::Windows, s("0.1.0.1"))
            .await,
        Err(ProtocolError::Refused {
            status: 404,
            code: s("not_found")
        })
    );

    let down = Stub::start(|_| Reply::bytes(502, b"bad gateway".to_vec())).await;
    assert_eq!(
        down.deployment()
            .check_for_update(UpdatePlatform::Android, s("0.1.0.1"))
            .await,
        Err(ProtocolError::Refused {
            status: 502,
            code: s("")
        })
    );

    let nobody = Deployment::new(s("http://127.0.0.1:9")).unwrap();
    assert!(matches!(
        nobody
            .check_for_update(UpdatePlatform::Android, s("0.1.0.1"))
            .await,
        Err(ProtocolError::Network { .. })
    ));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_build_described_wrongly_is_not_offered() {
    let wrongs = vec![
        ("another platform", latest("mac", "0.1.0.8")),
        (
            "no version",
            json!({ "platform": "android", "size": 1, "sha256": "a".repeat(64) }),
        ),
        (
            "a version that is not one",
            json!({ "platform": "android", "version": "0.1.0.8-beta", "size": 1, "sha256": "a".repeat(64) }),
        ),
        (
            "no size",
            json!({ "platform": "android", "version": "0.1.0.8", "size": 0, "sha256": "a".repeat(64) }),
        ),
        (
            "a short hash",
            json!({ "platform": "android", "version": "0.1.0.8", "size": 1, "sha256": "a".repeat(63) }),
        ),
        (
            "a hash that is not hex",
            json!({ "platform": "android", "version": "0.1.0.8", "size": 1, "sha256": "g".repeat(64) }),
        ),
    ];

    for (what, body) in wrongs {
        let stub = Stub::start(move |_| Reply::json(200, body.clone())).await;
        let said = stub
            .deployment()
            .check_for_update(UpdatePlatform::Android, s("0.1.0.1"))
            .await;
        assert!(
            matches!(said, Err(ProtocolError::Unexpected { .. })),
            "{what}: {said:?}"
        );
    }

    let stub = Stub::start(|_| Reply::json(200, latest("android", "0.1.0.8"))).await;
    assert!(matches!(
        stub.deployment()
            .check_for_update(UpdatePlatform::Android, s("0.1.0 (1)"))
            .await,
        Err(ProtocolError::Invalid { .. })
    ));
}

/// What updates.go answers for the iOS app: a version and notes, and no
/// size or hash, because TestFlight delivers the build.
fn latest_ios(version: &str) -> Value {
    json!({
        "platform": "ios",
        "version": version,
        "notes": { "zh-Hans": "  修好了  ", "en": "Fixed.", "ja": 7 },
    })
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_ios_build_is_a_version_and_notes_with_nothing_to_download() {
    let stub = Stub::start(|_| Reply::json(200, latest_ios("0.1.0.10"))).await;
    let deployment = stub.deployment();

    let notes = HashMap::from([(s("zh-Hans"), s("修好了")), (s("en"), s("Fixed."))]);
    assert_eq!(
        deployment
            .clone()
            .check_for_update(UpdatePlatform::Ios, s("0.1.0.9"))
            .await,
        Ok(UpdateCheck::Available {
            build: AppBuild {
                platform: UpdatePlatform::Ios,
                version: s("0.1.0.10"),
                size: 0,
                sha256: s(""),
                notes: notes.clone(),
            }
        }),
        "0.1.0.10 is after 0.1.0.9 by number, on iOS as anywhere"
    );

    for running in ["0.1.0.10", "0.1.0.11", "0.2"] {
        assert_eq!(
            deployment
                .clone()
                .check_for_update(UpdatePlatform::Ios, s(running))
                .await,
            Ok(UpdateCheck::UpToDate),
            "offered to {running}"
        );
    }

    // Given a size and a hash all the same, as a line copied into the iOS
    // list from the Mac's would carry, or as nought and nothing: neither is
    // taken for a file.
    let mut copied = latest("ios", "0.1.0.10");
    copied["notes"] = latest_ios("0.1.0.10")["notes"].clone();
    let mut nothing = latest_ios("0.1.0.10");
    nothing["size"] = json!(0);
    nothing["sha256"] = json!("");
    for body in [copied, nothing] {
        let stub = Stub::start(move |_| Reply::json(200, body.clone())).await;
        let said = stub
            .deployment()
            .check_for_update(UpdatePlatform::Ios, s("0.1.0.9"))
            .await;
        let Ok(UpdateCheck::Available { build }) = said else {
            panic!("an iOS build with a size and a hash beside it was not offered: {said:?}");
        };
        assert_eq!((build.size, build.sha256.as_str()), (0, ""), "{build:?}");
    }

    let none = Stub::start(|_| Reply::json(404, json!({ "error": "no_build" }))).await;
    assert_eq!(
        none.deployment()
            .check_for_update(UpdatePlatform::Ios, s("0.1.0.9"))
            .await,
        Ok(UpdateCheck::UpToDate)
    );

    for (what, body) in [
        ("another platform", latest("mac", "0.1.0.10")),
        ("no version", json!({ "platform": "ios" })),
        (
            "a version that is not one",
            json!({ "platform": "ios", "version": "0.1.0 (10)" }),
        ),
    ] {
        let stub = Stub::start(move |_| Reply::json(200, body.clone())).await;
        let said = stub
            .deployment()
            .check_for_update(UpdatePlatform::Ios, s("0.1.0.9"))
            .await;
        assert!(
            matches!(said, Err(ProtocolError::Unexpected { .. })),
            "{what}: {said:?}"
        );
    }
}

// ----------------------------------------------------------------------------
// The download.

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_build_arrives_whole_and_checked_where_it_was_asked_to_go() {
    let bytes = Arc::new(contents(1, 300_000));
    let build = build_of(&bytes, "0.1.0.8");
    let served = bytes.clone();
    let stub = Stub::start(move |seen| serve_file(seen, &served)).await;
    let dir = scratch("whole");
    let destination = dir.join("nested").join("Tomoshibi-0.1.0.8.apk");
    let watch = Arc::new(Watch::default());

    let download = stub
        .deployment()
        .download_update(build.clone(), destination.to_string_lossy().into());
    assert_eq!(download.destination(), destination.to_string_lossy());
    download.run(watch.clone()).await.unwrap();

    assert_eq!(std::fs::read(&destination).unwrap(), *bytes);
    assert!(
        parts_in(destination.parent().unwrap()).is_empty(),
        "the part was left behind"
    );
    let file = &stub.files()[0];
    assert_eq!(file.target, "/api/apps/latest/android/file");
    assert!(
        file.header("range").is_none(),
        "a fresh download asked for a range"
    );

    let said = watch.said.lock().unwrap().clone();
    assert_eq!(said.last(), Some(&(300_000, 300_000)));
    assert!(
        said.windows(2).all(|pair| pair[0].0 <= pair[1].0),
        "progress went backwards: {said:?}"
    );
    assert!(said.iter().all(|(_, total)| *total == 300_000));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_file_that_is_not_the_build_is_deleted_and_refused() {
    let bytes = contents(2, 50_000);
    let mut tampered = bytes.clone();
    tampered[25_000] ^= 0xff;
    let build = build_of(&bytes, "0.1.0.8");
    // The ETag is the build's, as a cache or a proxy that changed the body
    // would leave it; only the hash of what arrived can tell.
    let tag = format!("\"{}\"", build.sha256);
    let stub = Stub::start(move |_| Reply::bytes(200, tampered.clone()).with("ETag", &tag)).await;
    let dir = scratch("tampered");
    let destination = dir.join("Tomoshibi-0.1.0.8.apk");

    let said = stub
        .deployment()
        .download_update(build, destination.to_string_lossy().into())
        .run(Arc::new(Watch::default()))
        .await;

    assert!(
        matches!(said, Err(UpdateError::Mismatch { .. })),
        "{said:?}"
    );
    assert!(
        !destination.exists(),
        "a file that failed its hash was left to be installed"
    );
    assert!(
        parts_in(&dir).is_empty(),
        "a file that failed its hash was kept to be resumed"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn more_than_was_offered_is_deleted_and_refused() {
    let bytes = contents(3, 40_000);
    let mut build = build_of(&bytes, "0.1.0.8");
    build.size = 30_000;
    // No ETag and no length the client could read before the body: what is
    // left to stop it is counting what arrives.
    let stub = Stub::start(move |_| {
        let mut reply = Reply::bytes(200, bytes.clone());
        reply.unmeasured = true;
        reply
    })
    .await;
    let dir = scratch("longer");
    let destination = dir.join("Tomoshibi.apk");

    let said = stub
        .deployment()
        .download_update(build, destination.to_string_lossy().into())
        .run(Arc::new(Watch::default()))
        .await;

    assert!(
        matches!(said, Err(UpdateError::Mismatch { .. })),
        "{said:?}"
    );
    assert!(!destination.exists());
    assert!(parts_in(&dir).is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_file_of_another_length_is_another_build() {
    let bytes = contents(12, 40_000);
    let mut build = build_of(&bytes, "0.1.0.8");
    build.size = 30_000;
    let stub = Stub::start(move |_| Reply::bytes(200, bytes.clone())).await;
    let dir = scratch("otherlength");
    let destination = dir.join("Tomoshibi.apk");

    let said = stub
        .deployment()
        .download_update(build, destination.to_string_lossy().into())
        .run(Arc::new(Watch::default()))
        .await;

    assert_eq!(said, Err(UpdateError::Superseded));
    assert!(parts_in(&dir).is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_broken_download_carries_on_from_where_it_stopped() {
    let bytes = Arc::new(contents(4, 200_000));
    let build = build_of(&bytes, "0.1.0.8");
    let served = bytes.clone();
    let first = Arc::new(Mutex::new(true));
    let stub = Stub::start(move |seen| {
        let mut reply = serve_file(seen, &served);
        let mut first = first.lock().unwrap();
        if *first {
            *first = false;
            reply.cut_after = Some(80_000);
        }
        reply
    })
    .await;
    let dir = scratch("broken");
    let destination = dir.join("Tomoshibi-0.1.0.8.apk");
    let deployment = stub.deployment();

    let said = deployment
        .download_update(build.clone(), destination.to_string_lossy().into())
        .run(Arc::new(Watch::default()))
        .await;
    assert!(matches!(said, Err(UpdateError::Network { .. })), "{said:?}");
    let part = part_of(&destination, &build);
    assert_eq!(
        std::fs::metadata(&part).map(|meta| meta.len()).ok(),
        Some(80_000),
        "what arrived before the break was not kept"
    );
    assert!(!destination.exists());

    let watch = Arc::new(Watch::default());
    deployment
        .download_update(build.clone(), destination.to_string_lossy().into())
        .run(watch.clone())
        .await
        .unwrap();

    assert_eq!(std::fs::read(&destination).unwrap(), *bytes);
    assert!(!part.exists());
    let again = stub.files().pop().unwrap();
    assert_eq!(again.header("range"), Some("bytes=80000-"));
    assert_eq!(
        again.header("if-range"),
        Some(format!("\"{}\"", build.sha256).as_str())
    );
    assert_eq!(
        watch.said.lock().unwrap().first(),
        Some(&(80_000, 200_000)),
        "it started from nothing"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_body_that_ends_short_without_saying_so_is_kept_to_carry_on() {
    // No length, so the end of the connection is the end of the body as far
    // as the HTTP client knows: nothing fails until the bytes are counted.
    let bytes = Arc::new(contents(13, 90_000));
    let build = build_of(&bytes, "0.1.0.8");
    let served = bytes.clone();
    let stub = Stub::start(move |_| {
        let mut reply = Reply::bytes(200, served[..30_000].to_vec());
        reply.unmeasured = true;
        reply
    })
    .await;
    let dir = scratch("short");
    let destination = dir.join("Tomoshibi.apk");

    let said = stub
        .deployment()
        .download_update(build.clone(), destination.to_string_lossy().into())
        .run(Arc::new(Watch::default()))
        .await;

    assert!(matches!(said, Err(UpdateError::Network { .. })), "{said:?}");
    assert!(!destination.exists());
    assert_eq!(
        std::fs::metadata(part_of(&destination, &build))
            .map(|meta| meta.len())
            .ok(),
        Some(30_000)
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_server_that_will_not_give_a_range_is_downloaded_from_the_start() {
    let bytes = Arc::new(contents(5, 120_000));
    let build = build_of(&bytes, "0.1.0.8");
    let served = bytes.clone();
    let stub = Stub::start(move |_| {
        let tag = format!("\"{}\"", sha256(&served));
        Reply::bytes(200, served.to_vec()).with("ETag", &tag)
    })
    .await;
    let dir = scratch("norange");
    let destination = dir.join("Tomoshibi.apk");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(part_of(&destination, &build), &bytes[..50_000]).unwrap();

    stub.deployment()
        .download_update(build, destination.to_string_lossy().into())
        .run(Arc::new(Watch::default()))
        .await
        .unwrap();

    // Appended to the part it would be 170000 bytes and the wrong hash.
    assert_eq!(std::fs::read(&destination).unwrap(), *bytes);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_build_replaced_on_the_server_is_superseded_and_never_spliced() {
    let old = contents(6, 100_000);
    let new = Arc::new(contents(7, 100_000));
    let build = build_of(&old, "0.1.0.8");
    let served = new.clone();
    let stub = Stub::start(move |seen| serve_file(seen, &served)).await;
    let dir = scratch("replaced");
    let destination = dir.join("Tomoshibi.apk");
    std::fs::create_dir_all(&dir).unwrap();
    let part = part_of(&destination, &build);
    std::fs::write(&part, &old[..40_000]).unwrap();

    let said = stub
        .deployment()
        .download_update(build, destination.to_string_lossy().into())
        .run(Arc::new(Watch::default()))
        .await;

    assert_eq!(said, Err(UpdateError::Superseded));
    assert!(!destination.exists());
    assert!(
        !part.exists(),
        "the part of a build nobody is offered any more was kept"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_whole_part_that_is_wrong_is_fetched_again_from_nothing() {
    let bytes = Arc::new(contents(8, 60_000));
    let build = build_of(&bytes, "0.1.0.8");
    let served = bytes.clone();
    let stub = Stub::start(move |seen| serve_file(seen, &served)).await;
    let dir = scratch("wholewrong");
    let destination = dir.join("Tomoshibi.apk");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(part_of(&destination, &build), contents(9, 60_000)).unwrap();

    stub.deployment()
        .download_update(build, destination.to_string_lossy().into())
        .run(Arc::new(Watch::default()))
        .await
        .unwrap();

    assert_eq!(std::fs::read(&destination).unwrap(), *bytes);
    assert!(stub
        .files()
        .iter()
        .all(|seen| seen.header("range").is_none()));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_cancelled_download_stops_and_keeps_what_arrived() {
    let bytes = Arc::new(contents(10, 500_000));
    let build = build_of(&bytes, "0.1.0.8");
    let served = bytes.clone();
    let stub = Stub::start(move |seen| {
        let mut reply = serve_file(seen, &served);
        reply.stall_after = Some(100_000);
        reply
    })
    .await;
    let dir = scratch("cancelled");
    let destination = dir.join("Tomoshibi.apk");
    let watch = Arc::new(Watch::default());

    let download = stub
        .deployment()
        .download_update(build.clone(), destination.to_string_lossy().into());
    let running = tokio::spawn(download.clone().run(watch.clone()));

    // Until some of it has arrived, and the stub holds the rest back for a
    // minute: a cancel that only took effect between reads would wait it out.
    for _ in 0..200 {
        if watch.highest.load(Ordering::SeqCst) > 0 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    tokio::time::sleep(Duration::from_millis(200)).await;
    download.cancel();

    let said = tokio::time::timeout(Duration::from_secs(5), running)
        .await
        .expect("the download went on after it was cancelled")
        .unwrap();
    assert_eq!(said, Err(UpdateError::Cancelled));
    assert!(!destination.exists());
    assert_eq!(
        std::fs::metadata(part_of(&destination, &build))
            .map(|meta| meta.len())
            .ok(),
        Some(100_000)
    );

    // Called off stays called off.
    assert_eq!(
        download.run(Arc::new(Watch::default())).await,
        Err(UpdateError::Cancelled)
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_refused_download_says_why() {
    let stub = Stub::start(|seen| {
        if seen
            .header("user-agent")
            .is_some_and(|agent| agent.starts_with("Tomoshibi/"))
        {
            Reply::json(404, json!({ "error": "no_build" }))
        } else {
            Reply::json(403, json!({ "error": "not_the_app" }))
        }
    })
    .await;
    let dir = scratch("refused");
    let destination = dir.join("Tomoshibi.apk");

    let said = stub
        .deployment()
        .download_update(
            build_of(b"x", "0.1.0.8"),
            destination.to_string_lossy().into(),
        )
        .run(Arc::new(Watch::default()))
        .await;

    assert_eq!(
        said,
        Err(UpdateError::Refused {
            status: 404,
            code: s("no_build")
        })
    );
    assert!(!destination.exists());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_old_file_at_the_destination_is_never_taken_for_the_build() {
    let bytes = Arc::new(contents(11, 30_000));
    let build = build_of(&bytes, "0.1.0.8");
    let stub = Stub::start(|_| Reply::bytes(500, vec![])).await;
    let dir = scratch("stale");
    let destination = dir.join("Tomoshibi.apk");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(&destination, b"an older build").unwrap();

    let said = stub
        .deployment()
        .download_update(build, destination.to_string_lossy().into())
        .run(Arc::new(Watch::default()))
        .await;

    assert_eq!(
        said,
        Err(UpdateError::Refused {
            status: 500,
            code: s("")
        })
    );
    assert!(
        !destination.exists(),
        "a file that was there before was left as though it were the build"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_ios_build_is_never_downloaded_or_asked_for() {
    // A deployment that would hand a file to anything that asked, so that
    // the only thing between an iOS build and a download is the refusal.
    let bytes = Arc::new(contents(14, 20_000));
    let served = bytes.clone();
    let stub = Stub::start(move |seen| serve_file(seen, &served)).await;
    let dir = scratch("ios");
    let destination = dir.join("Tomoshibi-0.1.0.8.ipa");

    // As a check hands it over, and as an app could make one for itself,
    // with a size and a hash that the file above would match.
    let checked = AppBuild {
        platform: UpdatePlatform::Ios,
        version: s("0.1.0.8"),
        size: 0,
        sha256: s(""),
        notes: HashMap::new(),
    };
    let mut made = build_of(&bytes, "0.1.0.8");
    made.platform = UpdatePlatform::Ios;

    for build in [checked, made] {
        let said = stub
            .deployment()
            .download_update(build.clone(), destination.to_string_lossy().into())
            .run(Arc::new(Watch::default()))
            .await;
        assert!(
            matches!(&said, Err(UpdateError::Unexpected { detail }) if detail.contains("TestFlight")),
            "{build:?} came to {said:?}"
        );
    }

    assert!(
        stub.seen().is_empty(),
        "the deployment was asked for an iOS build: {:?}",
        stub.seen()
    );
    assert!(!dir.exists(), "a download of an iOS build touched the disk");
}
