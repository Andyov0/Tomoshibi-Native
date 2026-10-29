//! The deployment API against a small HTTP server started in each test, which
//! records every request and answers what the test scripts.
//!
//! Two kinds of case. Those the web client has, ported from
//! web/src/live/{api,account,watch-api,music}.test.ts with the same inputs and
//! the same expectations. And those that guard what a page gets for free and an
//! app does not: where the passphrase goes, what headers go with it, what the
//! cookie jar keeps, and how each kind of failure arrives.

use std::{
    io,
    sync::{Arc, Mutex},
};

use serde_json::{json, Value};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};
use tomoshibi_protocol::{
    api::{
        read_resolved, Deployment, DoorState, Joining, MusicQuality, Opening, ResolvedLink,
        ResolvedPlay, ResolvedPlayKind, ResolvedProxy, ResolvedProxyKind, ResolvedVideo,
        TicketVideo, UnresolvedReason,
    },
    ProtocolError,
};
use url::Url;

// ----------------------------------------------------------------------------
// The server.

#[derive(Debug, Clone)]
struct Seen {
    method: String,
    target: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl Seen {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }

    fn body(&self) -> &str {
        std::str::from_utf8(&self.body).unwrap_or("<not utf-8>")
    }

    fn path(&self) -> &str {
        self.target.split('?').next().unwrap_or_default()
    }

    fn query(&self) -> Vec<(String, String)> {
        Url::parse(&format!("http://x{}", self.target))
            .unwrap()
            .query_pairs()
            .into_owned()
            .collect()
    }
}

struct Reply {
    status: u16,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl Reply {
    fn json(status: u16, body: Value) -> Self {
        Self {
            status,
            headers: vec![("Content-Type".into(), "application/json".into())],
            body: body.to_string().into_bytes(),
        }
    }

    fn text(status: u16, body: &str) -> Self {
        Self {
            status,
            headers: vec![("Content-Type".into(), "text/html".into())],
            body: body.as_bytes().to_vec(),
        }
    }

    /// Go's `http.NotFound`, which is what a feature the deployment has not
    /// configured answers.
    fn not_found() -> Self {
        Self {
            status: 404,
            headers: vec![("Content-Type".into(), "text/plain; charset=utf-8".into())],
            body: b"404 page not found\n".to_vec(),
        }
    }

    fn empty(status: u16) -> Self {
        Self {
            status,
            headers: vec![],
            body: vec![],
        }
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

    fn last(&self) -> Seen {
        self.seen().pop().expect("a request was made")
    }

    fn to(&self, path: &str) -> Vec<Seen> {
        self.seen()
            .into_iter()
            .filter(|seen| seen.path() == path)
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
    let mut request = lines.next().unwrap_or_default().split(' ');
    let (method, target) = (
        request.next().unwrap_or_default().to_owned(),
        request.next().unwrap_or_default().to_owned(),
    );
    let headers: Vec<(String, String)> = lines
        .filter_map(|line| line.split_once(':'))
        .map(|(key, value)| (key.trim().to_ascii_lowercase(), value.trim().to_owned()))
        .collect();
    let length: usize = headers
        .iter()
        .find(|(key, _)| key == "content-length")
        .and_then(|(_, value)| value.parse().ok())
        .unwrap_or(0);

    let mut body = buffer[end + 4..].to_vec();
    while body.len() < length {
        let mut chunk = vec![0u8; length - body.len()];
        let read = socket.read(&mut chunk).await?;
        if read == 0 {
            break;
        }
        body.extend_from_slice(&chunk[..read]);
    }

    let seen = Seen {
        method,
        target,
        headers,
        body,
    };
    recorded.lock().unwrap().push(seen.clone());
    let reply = handler(&seen);

    let mut out = format!(
        "HTTP/1.1 {} Stub\r\nContent-Length: {}\r\nConnection: close\r\n",
        reply.status,
        reply.body.len()
    );
    for (name, value) in &reply.headers {
        out.push_str(&format!("{name}: {value}\r\n"));
    }
    out.push_str("\r\n");
    socket.write_all(out.as_bytes()).await?;
    socket.write_all(&reply.body).await?;
    socket.shutdown().await
}

/// What a deployment with no relays answers, so a join that measures has
/// something to read.
fn no_relays(seen: &Seen) -> Option<Reply> {
    (seen.path() == "/api/relays").then(|| {
        Reply::json(
            200,
            json!({ "relays": [], "measure": false, "fleet": { "online": 0, "total": 0 } }),
        )
    })
}

fn grant() -> Value {
    json!({ "url": "wss://x.invalid", "token": "t", "identity": "gaaaaaaaaaa-0123456789abcdef0123456789abcdef", "room": "standup" })
}

fn refused(status: u16, code: &str) -> ProtocolError {
    ProtocolError::Refused {
        status,
        code: code.into(),
    }
}

fn s(value: &str) -> String {
    value.to_owned()
}

// ----------------------------------------------------------------------------
// How each kind of failure arrives.

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_refusal_carries_the_servers_code_untouched() {
    let stub = Stub::start(|seen| {
        no_relays(seen).unwrap_or_else(|| Reply::json(403, json!({ "error": "not_invited" })))
    })
    .await;

    let said = stub
        .deployment()
        .join(s("standup"), s("Somebody"), s(""), None, None, s("x"), None)
        .await;

    assert_eq!(said.unwrap_err(), refused(403, "not_invited"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_feature_that_is_not_configured_is_not_found() {
    let stub = Stub::start(|_| Reply::not_found()).await;
    let deployment = stub.deployment();

    assert_eq!(
        deployment.clone().music_sources().await.unwrap_err(),
        refused(404, "not_found")
    );
    assert_eq!(
        deployment
            .clone()
            .music_search(s("a"), s("x"), 1)
            .await
            .unwrap_err(),
        refused(404, "not_found")
    );
    assert_eq!(
        deployment
            .clone()
            .music_track(s("a"), s("1"), MusicQuality::Best)
            .await
            .unwrap_err(),
        refused(404, "not_found")
    );
    assert_eq!(
        deployment.clone().music_link(s("x")).await.unwrap_err(),
        refused(404, "not_found")
    );
    assert_eq!(
        deployment
            .clone()
            .resolve_video(s("https://x"), false)
            .await
            .unwrap_err(),
        refused(404, "not_found")
    );
    assert_eq!(
        deployment
            .clone()
            .video_info(s("Abc_def-1234567890xyz"))
            .await
            .unwrap_err(),
        refused(404, "not_found")
    );
    assert_eq!(
        deployment.clone().media_servers().await.unwrap_err(),
        refused(404, "not_found")
    );
    // The one question the web answers with a plain no, and so does this.
    assert!(!deployment.watch_ready().await.unwrap());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_answer_without_a_code_is_the_general_failure_or_the_controls_own() {
    let stub = Stub::start(|seen| {
        if seen.path().ends_with("/mute") && seen.header("authorization") == Some("Bearer coded") {
            Reply::json(403, json!({ "error": "not_yours" }))
        } else {
            Reply::text(502, "<html>bad gateway</html>")
        }
    })
    .await;
    let d = stub.deployment();
    let (room, bearer) = (s("standup"), s("tok"));

    assert_eq!(
        d.clone()
            .knock(room.clone(), s("Somebody"))
            .await
            .unwrap_err(),
        refused(502, "")
    );
    assert_eq!(
        d.clone().arrangements().await.unwrap_err(),
        refused(502, "")
    );

    // host.ts throws a code of its own for each control where the server's
    // answer has none, so the panel can say which thing could not be done.
    assert_eq!(
        d.clone()
            .silence(room.clone(), bearer.clone(), s("g"), s("TR_1"))
            .await
            .unwrap_err(),
        refused(502, "mute_failed")
    );
    assert_eq!(
        d.clone()
            .turn_out(room.clone(), bearer.clone(), s("g"))
            .await
            .unwrap_err(),
        refused(502, "remove_failed")
    );
    assert_eq!(
        d.clone()
            .dissolve(room.clone(), bearer.clone())
            .await
            .unwrap_err(),
        refused(502, "close_failed")
    );
    assert_eq!(
        d.clone()
            .move_room(room.clone(), bearer.clone(), s("sh"))
            .await
            .unwrap_err(),
        refused(502, "move_failed")
    );
    assert_eq!(
        d.clone()
            .hand_over(room.clone(), bearer.clone(), s("t"))
            .await
            .unwrap_err(),
        refused(502, "handover_failed")
    );
    assert_eq!(
        d.clone()
            .create_invite(room.clone(), bearer.clone())
            .await
            .unwrap_err(),
        refused(502, "invite_failed")
    );
    assert_eq!(
        d.clone()
            .revoke_invites(room.clone(), Some(bearer))
            .await
            .unwrap_err(),
        refused(502, "revoke_failed")
    );

    // And the server's code wins wherever there is one.
    assert_eq!(
        d.silence(room, s("coded"), s("g"), s("TR_1"))
            .await
            .unwrap_err(),
        refused(403, "not_yours")
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn nothing_answering_is_a_network_failure() {
    // Bound and then dropped, so nothing listens there.
    let address = TcpListener::bind("127.0.0.1:0")
        .await
        .unwrap()
        .local_addr()
        .unwrap();
    let deployment = Deployment::new(format!("http://{address}")).unwrap();

    assert!(matches!(
        deployment.clone().deployment_info().await,
        Err(ProtocolError::Network { .. })
    ));
    assert!(matches!(
        deployment.clone().me().await,
        Err(ProtocolError::Network { .. })
    ));
    let joined = deployment
        .join(
            s("standup"),
            s("A"),
            s("hunter2"),
            None,
            None,
            s(""),
            Some(s("a-token")),
        )
        .await;
    let Err(ProtocolError::Network { detail: message }) = joined else {
        panic!("{joined:?}")
    };
    // Said without the address, which carries the invite.
    assert!(!message.contains("a-token"), "{message}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_answer_of_the_wrong_shape_is_unexpected() {
    let stub = Stub::start(|seen| match seen.target.as_str() {
        "/api/rooms/one/join" => Reply::text(200, "not json"),
        _ => Reply::json(200, json!({ "url": 1, "token": "t" })),
    })
    .await;
    let d = stub.deployment();

    assert!(matches!(
        d.clone()
            .join(s("one"), s("A"), s(""), None, None, s("x"), None)
            .await,
        Err(ProtocolError::Unexpected { .. })
    ));
    assert!(matches!(
        d.join(s("two"), s("A"), s(""), None, None, s("x"), None).await,
        Err(ProtocolError::Unexpected { .. })
    ));
}

#[test]
fn a_deployment_is_an_http_address() {
    for bad in [
        "",
        "live.example.org",
        "ftp://live.example.org",
        "file:///etc/passwd",
    ] {
        assert!(
            matches!(Deployment::new(s(bad)), Err(ProtocolError::Invalid { .. })),
            "{bad}"
        );
    }
    // The API is at /api on the origin, as the web fetches it, whatever path
    // was typed after it.
    assert_eq!(
        Deployment::new(s("https://live.example.org/some/page?x=1#y"))
            .unwrap()
            .base_url(),
        "https://live.example.org/"
    );
}

// ----------------------------------------------------------------------------
// The join.

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_passphrase_never_goes_in_an_address() {
    let stub = Stub::start(|seen| {
        no_relays(seen).unwrap_or_else(|| match seen.path() {
            "/api/account/session" | "/api/account/passphrase" => {
                Reply::json(200, json!({ "name": "a", "trip": "b" }))
            }
            _ => Reply::json(200, grant()),
        })
    })
    .await;
    let d = stub.deployment();

    d.clone()
        .join(
            s("standup@acme"),
            s("Alice"),
            s("hunter2 #sec"),
            None,
            None,
            s(""),
            Some(s("a-token")),
        )
        .await
        .unwrap();
    d.clone()
        .sign_in(s("alice"), s("hunter2 #sec"))
        .await
        .unwrap();
    d.change_passphrase(s("hunter2 #sec"), s("hunter3 #sec"))
        .await
        .unwrap();

    let join = stub
        .to("/api/rooms/standup%40acme/join")
        .pop()
        .expect("the join, its room escaped as a browser escapes it");
    // The invite in the query, as api.ts puts it; nothing else.
    assert_eq!(join.target, "/api/rooms/standup%40acme/join?invite=a-token");
    assert!(join.body().contains("hunter2 #sec"));

    for seen in stub.seen() {
        assert!(
            !seen.target.contains("hunter"),
            "{} carried a passphrase in its address",
            seen.target
        );
        assert!(
            seen.headers
                .iter()
                .all(|(_, value)| !value.contains("hunter")),
            "{seen:?}"
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_join_body_is_byte_for_byte_what_a_browser_sends() {
    let stub =
        Stub::start(|seen| no_relays(seen).unwrap_or_else(|| Reply::json(200, grant()))).await;
    let d = stub.deployment();

    // With no identity the key is left out, as JSON.stringify leaves out
    // undefined; with one it comes first, as api.ts writes the object.
    d.clone()
        .join(
            s("standup"),
            s("Somebody"),
            s(""),
            None,
            None,
            s("SG Misaka"),
            None,
        )
        .await
        .unwrap();
    assert_eq!(
        stub.last().body(),
        r#"{"name":"Somebody","passphrase":"","relay":"SG Misaka"}"#
    );

    let identity = "gaaaaaaaaaa-0123456789abcdef0123456789abcdef";
    d.clone()
        .join(
            s("standup"),
            s("Some \"body\""),
            s("p"),
            Some(s(identity)),
            None,
            s("SG Misaka"),
            None,
        )
        .await
        .unwrap();
    let join = stub.last();
    assert_eq!(
        join.body(),
        format!(
            r#"{{"identity":"{identity}","name":"Some \"body\"","passphrase":"p","relay":"SG Misaka"}}"#
        )
    );
    assert_eq!(join.header("content-type"), Some("application/json"));
    assert_eq!(join.method, "POST");

    // A relay somebody chose is not measured against.
    assert!(stub.to("/api/relays").is_empty());

    // Nobody chose, so the relays are asked about, and a deployment that does
    // not want a measurement is sent none.
    d.clone()
        .join(s("standup"), s("Somebody"), s(""), Some(s("")), None, s(""), None)
        .await
        .unwrap();
    assert_eq!(stub.to("/api/relays").len(), 1);
    assert_eq!(
        stub.last().body(),
        r#"{"name":"Somebody","passphrase":"","relay":""}"#
    );

    // The proof an issued identity came with goes right after it, as api.ts
    // writes `{ identity, proof, name, passphrase, relay }`.
    d.clone()
        .join(
            s("standup"),
            s("Somebody"),
            s(""),
            Some(s(identity)),
            Some(s("pr00f_-x")),
            s("x"),
            None,
        )
        .await
        .unwrap();
    assert_eq!(
        stub.last().body(),
        format!(
            r#"{{"identity":"{identity}","proof":"pr00f_-x","name":"Somebody","passphrase":"","relay":"x"}}"#
        )
    );

    // A proof is about the identity it came with: with none, or an empty one,
    // it is not sent; and an empty proof is no proof.
    for (kept, proof) in [(None, s("pr00f")), (Some(s("")), s("pr00f")), (Some(s(identity)), s(""))] {
        d.clone()
            .join(s("standup"), s("Somebody"), s(""), kept.clone(), Some(proof), s("x"), None)
            .await
            .unwrap();
        let sent = stub.last();
        let body = sent.body();
        assert!(!body.contains("proof"), "{kept:?}: {body}");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_join_reads_every_field_and_ignores_the_ones_it_does_not_know() {
    let stub = Stub::start(|_| {
        Reply::json(
            200,
            json!({
                "url": "wss://hk.example/", "token": "tok", "identity": "tid", "room": "standup",
                "proof": "pr00f-of-tid",
                "relay": "Hong Kong", "region": "HK", "holding": "Shanghai", "added": { "later": true },
                "forward": { "url": "turns:hk.example:443", "username": "u", "credential": "c" }
            }),
        )
    })
    .await;

    let grant = stub
        .deployment()
        .join(s("Standup"), s("A"), s(""), None, None, s("hk"), None)
        .await
        .unwrap();

    assert_eq!(grant.proof.as_deref(), Some("pr00f-of-tid"));
    assert_eq!(grant.relay.as_deref(), Some("Hong Kong"));
    assert_eq!(grant.region.as_deref(), Some("HK"));
    assert_eq!(grant.holding.as_deref(), Some("Shanghai"));
    assert_eq!(
        grant.forward.as_ref().map(|f| f.credential.as_str()),
        Some("c")
    );
    // A grant is a credential for a few hours, and does not print as one; nor
    // does the proof, which is what keeps the identity this guest's.
    let printed = format!("{grant:?}");
    assert!(
        !printed.contains("tok") && !printed.contains("\"c\"") && !printed.contains("pr00f"),
        "{printed}"
    );
}

/// How long a join token is good for is the server's own arithmetic, so a
/// device whose clock is minutes out still keeps the join's token for as long
/// as the server will take it (rooms.rs `token_lifetime`).
#[test]
fn a_join_tokens_lifetime_is_read_off_the_servers_clock() {
    use base64::Engine;
    use std::time::Duration;
    use tomoshibi_protocol::api::token_lifetime;

    let token = |claims: Value| {
        format!(
            "e30.{}.c2ln",
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(claims.to_string())
        )
    };

    // As the control node signs one (iat and nbf both now) and as the media
    // server's own library does.
    assert_eq!(
        token_lifetime(&token(json!({ "exp": 1_800_000_300, "iat": 1_800_000_000, "nbf": 1_800_000_000, "jti": "p" }))),
        Some(Duration::from_secs(300))
    );
    assert_eq!(
        token_lifetime(&token(json!({ "exp": 1_800_000_600, "nbf": 1_800_000_000 }))),
        Some(Duration::from_secs(600))
    );
    for nonsense in [
        token(json!({ "iat": 1_800_000_000 })),
        token(json!({ "exp": 1_800_000_000 })),
        token(json!({ "exp": 1_800_000_000, "iat": 1_800_000_000 })),
        token(json!({ "exp": 1_700_000_000, "iat": 1_800_000_000 })),
        token(json!({ "exp": "soon", "iat": 1_800_000_000 })),
        s("not a token"),
        s("a.%%%.b"),
        s(""),
    ] {
        assert_eq!(token_lifetime(&nonsense), None, "{nonsense}");
    }
}

/// An identity a passphrase or an account keeps comes with no proof, and nor
/// does any answer from a server older than proofs; an empty one is none.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_join_without_a_proof_says_none() {
    let stub = Stub::start(|seen| match seen.path() {
        "/api/rooms/one/join" => Reply::json(200, grant()),
        _ => Reply::json(200, json!({ "url": "wss://x.invalid", "token": "t", "identity": "i", "room": "two", "proof": "" })),
    })
    .await;
    let d = stub.deployment();

    for room in ["one", "two"] {
        let grant = d
            .clone()
            .join(s(room), s("A"), s(""), None, None, s("x"), None)
            .await
            .unwrap();
        assert_eq!(grant.proof, None, "{room}");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn requests_carry_no_origin_and_no_fetch_metadata() {
    let stub =
        Stub::start(|seen| no_relays(seen).unwrap_or_else(|| Reply::json(200, grant()))).await;
    let d = stub.deployment();

    d.clone()
        .join(s("standup"), s("A"), s("p"), None, None, s(""), None)
        .await
        .unwrap();
    let _ = d.clone().sign_in(s("a"), s("p")).await;
    let _ = d
        .clone()
        .silence(s("standup"), s("tok"), s("g"), s("TR"))
        .await;
    let _ = d
        .arrange_meeting(s("standup"), 1_788_338_640_000, s(""))
        .await;

    // internal/admin/api.go FromHere lets a request without Sec-Fetch-Site
    // through; one claiming another site is refused the session.
    for seen in stub.seen() {
        assert_eq!(seen.header("origin"), None, "{seen:?}");
        assert_eq!(seen.header("sec-fetch-site"), None, "{seen:?}");
    }
}

// ----------------------------------------------------------------------------
// The host's controls.

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_host_controls_carry_the_join_token() {
    let stub = Stub::start(|seen| match (seen.method.as_str(), seen.path()) {
        ("GET", "/api/rooms/standup/host") => {
            Reply::json(200, json!({ "yours": true, "admin": false, "host": "" }))
        }
        ("POST", "/api/rooms/standup/host") => Reply::json(200, json!({ "host": "k7m2q6x4rt" })),
        ("PUT", "/api/rooms/standup/relay") => Reply::json(200, json!({ "relay": "Hong Kong" })),
        ("POST", "/api/rooms/standup/invites") => Reply::json(
            200,
            json!({ "room": "standup", "expires": "2026-09-03T08:44:00Z", "token": "a-token" }),
        ),
        ("DELETE", "/api/rooms/standup/invites") => Reply::json(200, json!({ "revoked": 3 })),
        _ => Reply::empty(204),
    })
    .await;
    let d = stub.deployment();
    let (room, bearer) = (s("standup"), s("the.join.token"));

    let standing = d
        .clone()
        .host_standing(room.clone(), bearer.clone())
        .await
        .unwrap();
    assert!(standing.yours && !standing.admin);
    let asked = stub.last();
    assert_eq!(asked.header("authorization"), Some("Bearer the.join.token"));
    assert_eq!(asked.header("content-type"), Some("application/json"));

    assert_eq!(
        d.clone()
            .hand_over(room.clone(), bearer.clone(), s("tk7m2q6x4rt-x"))
            .await
            .unwrap(),
        "k7m2q6x4rt"
    );
    assert_eq!(stub.last().body(), r#"{"to":"tk7m2q6x4rt-x"}"#);

    d.clone()
        .silence(room.clone(), bearer.clone(), s("gabc"), s("TR_mic"))
        .await
        .unwrap();
    assert_eq!(
        stub.last().body(),
        r#"{"identity":"gabc","track":"TR_mic"}"#
    );

    d.clone()
        .turn_out(room.clone(), bearer.clone(), s("g x/y"))
        .await
        .unwrap();
    assert_eq!(stub.last().target, "/api/rooms/standup/people/g%20x%2Fy");
    assert_eq!(stub.last().method, "DELETE");

    d.clone()
        .dissolve(room.clone(), bearer.clone())
        .await
        .unwrap();
    assert_eq!(stub.last().target, "/api/rooms/standup/close");
    assert!(stub.last().body.is_empty());

    assert_eq!(
        d.clone()
            .move_room(room.clone(), bearer.clone(), s("hk"))
            .await
            .unwrap(),
        "Hong Kong"
    );
    assert_eq!(stub.last().body(), r#"{"relay":"hk"}"#);

    let made = d
        .clone()
        .create_invite(room.clone(), bearer.clone())
        .await
        .unwrap();
    assert_eq!(made.link, format!("{}/?invite=a-token#/standup", stub.base));
    assert_eq!(made.until.as_deref(), Some("2026-09-03T08:44:00Z"));
    assert!(stub.last().body.is_empty());

    assert_eq!(
        d.clone()
            .revoke_invites(room.clone(), Some(bearer.clone()))
            .await
            .unwrap(),
        3
    );

    for seen in stub.seen() {
        assert_eq!(
            seen.header("authorization"),
            Some("Bearer the.join.token"),
            "{} {}",
            seen.method,
            seen.target
        );
    }

    // The account page revokes a scoped room's links with its session alone.
    assert_eq!(d.revoke_invites(room, None).await.unwrap(), 3);
    assert_eq!(stub.last().header("authorization"), None);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_standing_invite_is_asked_for_by_name_and_with_instants() {
    let stub = Stub::start(|_| {
        Reply::json(
            200,
            json!({ "room": "standup@acme", "name": "Client Co", "token": "tk" }),
        )
    })
    .await;

    let made = stub
        .deployment()
        .create_standing_invite(
            s("standup@acme"),
            s("  Client Co "),
            Some(1_788_338_640_000),
            None,
        )
        .await
        .unwrap();

    let asked = stub.last();
    assert_eq!(asked.target, "/api/rooms/standup%40acme/invites");
    assert_eq!(
        asked.body(),
        r#"{"standing":true,"name":"Client Co","from":"2026-09-02T08:44:00.000Z","until":""}"#
    );
    assert_eq!(made.link, format!("{}/?invite=tk#/standup@acme", stub.base));
    assert_eq!(made.name.as_deref(), Some("Client Co"));
}

// ----------------------------------------------------------------------------
// The cookie jar.

fn signed_in(seen: &Seen) -> Reply {
    match (seen.method.as_str(), seen.path()) {
        ("POST", "/api/account/session") => Reply::json(200, json!({ "name": "alice", "trip": "k7m2q6x4rt", "can": [] }))
            .with("Set-Cookie", "meet-live.account=Zm9v_YmFy-c2Vzc2lvbg; Path=/; Max-Age=2592000; HttpOnly; SameSite=Strict"),
        ("DELETE", "/api/account/session") => Reply::empty(500),
        ("GET", "/api/account/me") if seen.header("cookie").is_some_and(|c| c.contains("meet-live.account=")) => {
            Reply::json(200, json!({ "name": "alice", "trip": "k7m2q6x4rt", "avatar": "/api/avatar/k7m2q6x4rt", "can": ["watch"] }))
        }
        _ => Reply::json(401, json!({ "error": "not_yours" })),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_account_session_can_be_kept_and_put_back() {
    let stub = Stub::start(signed_in).await;
    let first = stub.deployment();

    assert_eq!(first.account_cookie(), None);
    first
        .clone()
        .sign_in(s("alice"), s("secret"))
        .await
        .unwrap();
    let kept = first.account_cookie().expect("the session is in the jar");
    assert_eq!(kept, "Zm9v_YmFy-c2Vzc2lvbg");
    assert_eq!(
        first.cookie_header().as_deref(),
        Some("meet-live.account=Zm9v_YmFy-c2Vzc2lvbg")
    );

    // The next launch: a new jar, and the session put back from the Keychain.
    let next = stub.deployment();
    assert_eq!(next.clone().me().await.unwrap(), None);
    next.restore_account_cookie(kept.clone()).unwrap();
    let me = next.clone().me().await.unwrap().expect("signed in again");
    assert_eq!(
        stub.last().header("cookie"),
        Some("meet-live.account=Zm9v_YmFy-c2Vzc2lvbg")
    );
    assert_eq!(
        me.avatar,
        Some(format!("{}/api/avatar/k7m2q6x4rt", stub.base))
    );
    assert_eq!(me.can, Some(vec![s("watch")]));

    // Forgotten when told to, and refused when what is given could not be one.
    next.restore_account_cookie(s("")).unwrap();
    assert_eq!(next.account_cookie(), None);
    assert_eq!(next.clone().me().await.unwrap(), None);
    assert_eq!(stub.last().header("cookie"), None);
    for bad in ["two words", "semi;colon", "quote\"d", "comma,ed"] {
        assert!(
            matches!(
                next.restore_account_cookie(s(bad)),
                Err(ProtocolError::Invalid { .. })
            ),
            "{bad}"
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn signing_out_forgets_the_session_whatever_the_server_says() {
    let stub = Stub::start(signed_in).await;
    let d = stub.deployment();

    d.clone().sign_in(s("alice"), s("secret")).await.unwrap();
    assert!(d.account_cookie().is_some());

    // The server answers 500 and clears nothing; the jar forgets it anyway.
    d.clone().sign_out().await.unwrap();
    assert_eq!(stub.last().method, "DELETE");
    assert_eq!(d.account_cookie(), None);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_invite_cookie_lives_in_the_jar_for_the_next_join() {
    let stub = Stub::start(|seen| {
        no_relays(seen).unwrap_or_else(|| {
            let reply = Reply::json(200, grant());
            if seen.target.contains("?invite=") {
                reply.with(
                    "Set-Cookie",
                    "meet-live.invite=a-token; Path=/; Max-Age=86400; HttpOnly; SameSite=Lax",
                )
            } else {
                reply
            }
        })
    })
    .await;
    let d = stub.deployment();

    d.clone()
        .join(
            s("standup"),
            s("A"),
            s(""),
            None,
            None,
            s("x"),
            Some(s("a-token")),
        )
        .await
        .unwrap();
    d.clone()
        .join(s("standup"), s("A"), s(""), None, None, s("x"), None)
        .await
        .unwrap();

    let rejoin = stub.last();
    assert_eq!(rejoin.target, "/api/rooms/standup/join");
    assert_eq!(rejoin.header("cookie"), Some("meet-live.invite=a-token"));
    // Not the account's, which nobody signed in to.
    assert_eq!(d.account_cookie(), None);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_redirect_is_followed_at_home_and_nowhere_else() {
    let elsewhere = Stub::start(|_| Reply::json(200, grant())).await;
    let target = format!("{}/api/rooms/standup/join", elsewhere.base);
    let stub = Stub::start(move |seen| match seen.path() {
        "/api/rooms/away/join" => Reply::empty(307).with("Location", &target),
        "/api/rooms/home/join" => Reply::empty(307).with("Location", "/api/rooms/moved/join"),
        _ => no_relays(seen).unwrap_or_else(|| Reply::json(200, grant())),
    })
    .await;
    let d = stub.deployment();

    // A 307 carries its body, and this one is a passphrase.
    let away = d
        .clone()
        .join(s("away"), s("A"), s("hunter2"), None, None, s("x"), None)
        .await;
    assert!(
        matches!(away, Err(ProtocolError::Unexpected { .. })),
        "{away:?}"
    );
    assert!(
        elsewhere.seen().is_empty(),
        "the passphrase went to another host"
    );

    d.join(s("home"), s("A"), s("hunter2"), None, None, s("x"), None)
        .await
        .unwrap();
    assert_eq!(stub.last().target, "/api/rooms/moved/join");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_picture_the_server_would_refuse_for_its_size_is_not_sent() {
    let stub = Stub::start(|_| Reply::json(200, json!({ "name": "a", "trip": "b" }))).await;
    let d = stub.deployment();

    let big = format!("data:image/jpeg;base64,{}", "A".repeat(100 << 10));
    assert_eq!(
        d.clone().set_avatar(big).await.unwrap_err(),
        refused(413, "avatar_too_large")
    );
    assert!(stub.seen().is_empty());

    d.set_avatar(s("data:image/jpeg;base64,AAAA"))
        .await
        .unwrap();
    assert_eq!(
        stub.last().body(),
        r#"{"image":"data:image/jpeg;base64,AAAA"}"#
    );
    assert_eq!(stub.last().method, "PUT");
}

// ----------------------------------------------------------------------------
// What the deployment says about itself (api.ts `deployment`).

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_deployment_is_narrowed_the_way_the_web_narrows_it() {
    let answers = Arc::new(Mutex::new(Vec::<Reply>::new()));
    let queue = answers.clone();
    let stub = Stub::start(move |_| queue.lock().unwrap().remove(0)).await;
    let d = stub.deployment();
    let ask = |reply: Reply| {
        answers.lock().unwrap().push(reply);
        d.clone().deployment_info()
    };

    let said = ask(Reply::json(200, json!({ "openedBy": "signed", "joinedBy": "accounts", "source": "https://src", "build": "abc" })))
        .await
        .unwrap();
    assert_eq!(
        (
            said.opened_by,
            said.joined_by,
            said.source.as_str(),
            said.build.as_str()
        ),
        (Opening::Signed, Joining::Accounts, "https://src", "abc")
    );

    // An opening value nobody knows is "anyone"; a joining one is the closed
    // door, because a page that cannot tell must not promise it is open.
    let said = ask(Reply::json(
        200,
        json!({ "openedBy": "everybody", "joinedBy": "whoever", "build": 7 }),
    ))
    .await
    .unwrap();
    assert_eq!(
        (said.opened_by, said.joined_by, said.build.as_str()),
        (Opening::Anyone, Joining::Invited, "")
    );

    // A deployment that will not say is what every deployment was.
    for reply in [
        Reply::empty(500),
        Reply::text(200, "<html>"),
        Reply::json(200, Value::Null),
    ] {
        let said = ask(reply).await.unwrap();
        assert_eq!(
            (said.opened_by, said.joined_by),
            (Opening::Anyone, Joining::Anyone)
        );
    }
}

// ----------------------------------------------------------------------------
// account.test.ts, "invited".

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_invite_says_its_name_and_its_window() {
    let stub = Stub::start(|seen| match seen.path() {
        "/api/invites/token" => Reply::json(
            200,
            json!({ "room": "standup@acme", "name": "Client Co", "from": "2026-03-03T02:00:00Z", "expires": "2026-03-03T04:00:00Z" }),
        ),
        "/api/invites/early" => Reply::json(403, json!({ "error": "invite_not_yet", "from": "2026-03-03T02:00:00Z" })),
        _ => Reply::text(502, "not json"),
    })
    .await;
    let d = stub.deployment();

    let said = d.clone().invite_info(s("token")).await.unwrap();
    assert_eq!(said.room.as_deref(), Some("standup@acme"));
    assert_eq!(said.name.as_deref(), Some("Client Co"));
    assert_eq!(said.from.as_deref(), Some("2026-03-03T02:00:00Z"));
    // `expires` on the wire, `until` here, and the kind of translation that
    // breaks silently: a link with an end would read as one without.
    assert_eq!(said.until.as_deref(), Some("2026-03-03T04:00:00Z"));
    assert_eq!(said.error, None);

    let early = d.clone().invite_info(s("early")).await.unwrap();
    assert_eq!(
        (early.error.as_deref(), early.from.as_deref()),
        (Some("invite_not_yet"), Some("2026-03-03T02:00:00Z"))
    );

    assert_eq!(
        d.invite_info(s("garbled")).await.unwrap().error.as_deref(),
        Some("no_such_invite")
    );
}

// ----------------------------------------------------------------------------
// The waiting room and arranged meetings.

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_knock_is_made_asked_about_and_answered() {
    let stub = Stub::start(|seen| match (seen.method.as_str(), seen.path()) {
        ("POST", "/api/rooms/standup/knock") => Reply::json(200, json!({ "token": "kt", "state": "knocking" })),
        ("GET", "/api/rooms/standup/knock/kt") => Reply::json(200, json!({ "state": "admitted", "invite": "inv" })),
        ("GET", "/api/rooms/standup/knock/odd") => Reply::json(200, json!({ "state": "pondering" })),
        ("GET", "/api/rooms/standup/knocks") => Reply::json(
            200,
            json!({ "knocking": [{ "id": "k1", "name": "Bo", "address": "203.0.113.9", "at": "2026-09-02T08:44:00Z" }] }),
        ),
        ("POST", "/api/rooms/standup/knocks/k1") => Reply::json(200, json!({ "state": "admitted" })),
        _ => Reply::empty(404),
    })
    .await;
    let d = stub.deployment();

    let ticket = d.clone().knock(s("standup"), s("Bo")).await.unwrap();
    assert_eq!(
        (ticket.token.as_str(), ticket.state),
        ("kt", DoorState::Knocking)
    );
    assert_eq!(stub.last().body(), r#"{"name":"Bo"}"#);
    assert_eq!(stub.last().header("content-type"), Some("application/json"));

    let answer = d.clone().at_the_door(s("standup"), s("kt")).await.unwrap();
    assert_eq!(
        (answer.state, answer.invite.as_deref()),
        (DoorState::Admitted, Some("inv"))
    );
    // A state nobody knows is still waiting, as Knocking.tsx reads it.
    assert_eq!(
        d.clone()
            .at_the_door(s("standup"), s("odd"))
            .await
            .unwrap()
            .state,
        DoorState::Knocking
    );

    let waiting = d.clone().knocking(s("standup"), s("tok")).await.unwrap();
    assert_eq!(waiting.len(), 1);
    assert_eq!(
        (waiting[0].id.as_str(), waiting[0].name.as_str()),
        ("k1", "Bo")
    );
    assert_eq!(stub.last().header("authorization"), Some("Bearer tok"));

    assert_eq!(
        d.answer_knock(s("standup"), s("k1"), s("tok"), true)
            .await
            .unwrap(),
        DoorState::Admitted
    );
    assert_eq!(stub.last().body(), r#"{"admit":true}"#);
    assert_eq!(stub.last().header("authorization"), Some("Bearer tok"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_meeting_is_arranged_with_an_instant_and_read_back() {
    let stub = Stub::start(|seen| match (seen.method.as_str(), seen.path()) {
        ("POST", "/api/meetings") => Reply::json(
            200,
            json!({ "id": "m1", "room": "standup", "at": "2026-09-02T08:44:00Z", "started": false, "ended": false,
                    "token": "mt", "mine": true, "from": "2026-09-02T08:29:00Z" }),
        ),
        ("GET", "/api/meetings") => Reply::json(401, json!({ "error": "not_signed_in" })),
        ("GET", "/api/meetings/dead") => Reply::json(404, json!({ "error": "no_such_meeting" })),
        ("DELETE", "/api/meetings/m1") => Reply::empty(204),
        _ => Reply::empty(500),
    })
    .await;
    let d = stub.deployment();

    let made = d
        .clone()
        .arrange_meeting(s("standup"), 1_788_338_640_000, s(""))
        .await
        .unwrap();
    // toISOString's shape, in arrange's key order.
    assert_eq!(
        stub.last().body(),
        r#"{"room":"standup","at":"2026-09-02T08:44:00.000Z","relay":""}"#
    );
    assert_eq!(
        (made.token.as_deref(), made.mine, made.relay),
        (Some("mt"), true, None)
    );
    assert_eq!(
        d.meeting_link(made.token.unwrap(), made.room),
        format!("{}/?meeting=mt#/standup", stub.base)
    );

    assert_eq!(
        d.clone().arrangements().await.unwrap_err(),
        refused(401, "not_signed_in")
    );
    assert_eq!(
        d.clone().arranged(s("dead")).await.unwrap_err(),
        refused(404, "no_such_meeting")
    );
    d.cancel_meeting(s("m1")).await.unwrap();
}

// ----------------------------------------------------------------------------
// Links and addresses.

#[test]
fn links_are_built_the_way_the_web_builds_them() {
    let d = Deployment::new(s("https://live.example.org")).unwrap();

    assert_eq!(
        d.invite_link(s("a-token"), s("standup@acme")),
        "https://live.example.org/?invite=a-token#/standup@acme"
    );
    assert_eq!(
        d.meeting_link(s("m t"), s("standup")),
        "https://live.example.org/?meeting=m%20t#/standup"
    );
    assert_eq!(
        d.avatar_url(s("k7m2q6x4rt")),
        "https://live.example.org/api/avatar/k7m2q6x4rt"
    );

    // music.test.ts: "plays from the same origin".
    assert_eq!(
        d.music_audio_url(s("a"), s("1"), MusicQuality::Best),
        "https://live.example.org/api/music/audio?source=a&id=1&quality=best"
    );

    let ticket = s("Abc_def-1234567890xyz");
    assert_eq!(
        d.watch_media_url(ticket).unwrap(),
        "https://live.example.org/api/watch/media?t=Abc_def-1234567890xyz"
    );
    for bad in [
        "short",
        "../admin?t=Abc_def-1234567890xyz",
        "Abc_def-1234567890xyz&url=https://evil",
    ] {
        assert!(
            matches!(
                d.watch_media_url(s(bad)),
                Err(ProtocolError::Invalid { .. })
            ),
            "{bad}"
        );
    }

    // Avatar.tsx: a picture only for somebody who arrived signed in.
    let hex = "0123456789abcdef0123456789abcdef";
    assert_eq!(
        d.avatar_for(format!("ak7m2q6x4rt-{hex}")),
        Some(s("https://live.example.org/api/avatar/k7m2q6x4rt"))
    );
    assert_eq!(d.avatar_for(format!("tk7m2q6x4rt-{hex}")), None);
    assert_eq!(d.avatar_for(format!("gk7m2q6x4rt-{hex}")), None);
    assert_eq!(d.avatar_for(s("someone")), None);
}

// ----------------------------------------------------------------------------
// watch-api.test.ts.

const TICKET: &str = "Abc_def-1234567890xyz";

fn good() -> Value {
    json!({
        "ticket": TICKET,
        "title": "A film",
        "duration": 213,
        "cover": "https://i.example.invalid/c.jpg",
        "link": "https://example.invalid/v",
        "live": false,
        "relay": false,
        "play": { "kind": "file", "url": "https://cdn.example.invalid/v.mp4" },
        "proxy": { "kind": "file", "url": format!("media?t={TICKET}") },
    })
}

/// `{ ...base, ...changes }`.
fn with(mut base: Value, changes: Value) -> Value {
    for (key, value) in changes.as_object().unwrap() {
        base[key] = value.clone();
    }
    base
}

fn without(mut base: Value, key: &str) -> Value {
    base.as_object_mut().unwrap().remove(key);
    base
}

fn origin() -> Url {
    Url::parse("https://meet.invalid/").unwrap()
}

fn read(body: Value) -> Option<ResolvedVideo> {
    read_resolved(&origin(), &body, true)
}

#[test]
fn a_resolved_video_is_read_with_its_relay_made_a_path_on_this_server() {
    assert_eq!(
        read(good()),
        Some(ResolvedVideo {
            ticket: s(TICKET),
            title: s("A film"),
            duration: 213.0,
            cover: s("https://i.example.invalid/c.jpg"),
            link: s("https://example.invalid/v"),
            live: false,
            relay: false,
            play: ResolvedPlay {
                kind: ResolvedPlayKind::File,
                url: Some(s("https://cdn.example.invalid/v.mp4")),
                id: None
            },
            proxy: Some(ResolvedProxy {
                kind: ResolvedProxyKind::File,
                url: format!("https://meet.invalid/api/watch/media?t={TICKET}"),
            }),
            grantor: None,
        })
    );
}

#[test]
fn a_video_starts_on_the_relay_only_when_the_gateway_says_so() {
    assert!(read(with(good(), json!({ "relay": true }))).unwrap().relay);
    assert!(!read(without(good(), "relay")).unwrap().relay);
    assert!(!read(with(good(), json!({ "relay": "yes" }))).unwrap().relay);
}

#[test]
fn youtube_plays_by_id_alone() {
    let found = read(with(
        good(),
        json!({ "play": { "kind": "youtube", "id": "dQw4w9WgXcQ", "url": "https://x" } }),
    ))
    .unwrap();
    assert_eq!(
        found.play,
        ResolvedPlay {
            kind: ResolvedPlayKind::Youtube,
            url: None,
            id: Some(s("dQw4w9WgXcQ"))
        }
    );
}

#[test]
fn a_video_naming_something_this_app_should_not_fetch_or_run_is_refused() {
    for bad in [
        with(
            good(),
            json!({ "play": { "kind": "file", "url": "javascript:alert(1)" } }),
        ),
        with(
            good(),
            json!({ "play": { "kind": "file", "url": "//evil.example.invalid/v.mp4" } }),
        ),
        with(
            good(),
            json!({ "play": { "kind": "youtube", "id": "../../x" } }),
        ),
        with(
            good(),
            json!({ "play": { "kind": "flash", "url": "https://x" } }),
        ),
        with(
            good(),
            json!({ "proxy": { "kind": "file", "url": "https://evil.example.invalid/media?t=x" } }),
        ),
        with(
            good(),
            json!({ "proxy": { "kind": "file", "url": "../admin?t=x" } }),
        ),
        with(
            good(),
            json!({ "proxy": { "kind": "file", "url": "media?t=x&url=https://evil" } }),
        ),
        with(
            good(),
            json!({ "proxy": { "kind": "file", "url": "media?t=Another_ticket_1234567" } }),
        ),
        with(good(), json!({ "ticket": "short" })),
        with(good(), json!({ "play": "file" })),
    ] {
        assert_eq!(read(bad.clone()), None, "{bad}");
    }
}

#[test]
fn a_cover_over_plain_http_is_dropped() {
    assert_eq!(
        read(with(
            good(),
            json!({ "cover": "http://i.example.invalid/c.jpg" })
        ))
        .unwrap()
        .cover,
        ""
    );
}

#[test]
fn a_video_only_the_relay_can_play_has_no_address_and_its_poster_from_the_relay() {
    let found = read(with(
        good(),
        json!({ "relay": true, "play": { "kind": "relay", "url": "https://media.example.invalid/x?api_key=secret" },
                "cover": "media?t=Poster_ticket_12345678" }),
    ))
    .unwrap();
    assert_eq!(
        found.play,
        ResolvedPlay {
            kind: ResolvedPlayKind::Relay,
            url: None,
            id: None
        }
    );
    assert_eq!(
        found.cover,
        "https://meet.invalid/api/watch/media?t=Poster_ticket_12345678"
    );

    for cover in [
        "library?op=image&server=a&id=1",
        "../media?t=x",
        "javascript:x",
        "//evil.invalid/c.jpg",
        "media?t=x&y=z",
    ] {
        assert_eq!(
            read(with(good(), json!({ "cover": cover }))).unwrap().cover,
            "",
            "{cover}"
        );
    }
}

#[test]
fn a_bangumi_episode_is_read_with_the_relay_as_where_it_plays() {
    let episode = with(
        good(),
        json!({ "link": "https://www.bilibili.com/bangumi/play/ep508405", "cover": "https://i0.hdslb.com/bfs/archive/ep508405.jpg",
                "play": { "kind": "bangumi", "url": "https://upos-sz.bilivideo.com/x.m4s" },
                "proxy": { "kind": "hls", "url": format!("media?t={TICKET}") } }),
    );
    let found = read(episode).unwrap();

    assert_eq!(
        found.play,
        ResolvedPlay {
            kind: ResolvedPlayKind::Bangumi,
            url: None,
            id: None
        }
    );
    assert!(!found.relay);
    assert_eq!(
        found.proxy.map(|proxy| proxy.kind),
        Some(ResolvedProxyKind::Hls)
    );
    assert_eq!(found.cover, "https://i0.hdslb.com/bfs/archive/ep508405.jpg");
}

/// A stub answering every request with one status, body and set of headers.
async fn answering(status: u16, body: Value, headers: &[(&str, &str)]) -> Stub {
    let headers: Vec<(String, String)> = headers.iter().map(|(k, v)| (s(k), s(v))).collect();
    Stub::start(move |_| {
        let mut reply = Reply::json(status, body.clone());
        reply.headers.extend(headers.clone());
        reply
    })
    .await
}

fn not(reason: UnresolvedReason) -> ResolvedLink {
    ResolvedLink::NotPlayable { reason }
}

async fn resolve(status: u16, body: Value, headers: &[(&str, &str)]) -> ResolvedLink {
    answering(status, body, headers)
        .await
        .deployment()
        .resolve_video(s("https://x"), false)
        .await
        .unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_link_that_is_not_a_video_is_told_from_one_that_cannot_play_and_one_that_failed() {
    assert_eq!(
        resolve(404, json!({ "error": "not_a_video" }), &[]).await,
        not(UnresolvedReason::NotAVideo)
    );
    assert_eq!(
        resolve(422, json!({ "error": "unavailable" }), &[]).await,
        not(UnresolvedReason::Unavailable)
    );
    assert_eq!(
        resolve(502, json!({}), &[]).await,
        not(UnresolvedReason::Failed)
    );
    let ResolvedLink::Playable { video } = resolve(200, good(), &[]).await else {
        panic!()
    };
    assert_eq!(video.title, "A film");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn why_a_video_cannot_play_is_read_where_the_gateway_says() {
    for (code, reason) in [
        ("members_only", UnresolvedReason::MembersOnly),
        ("copy_protected", UnresolvedReason::CopyProtected),
        ("region_locked", UnresolvedReason::RegionLocked),
        ("not_found", UnresolvedReason::NotFound),
    ] {
        assert_eq!(
            resolve(422, json!({ "error": code }), &[]).await,
            not(reason)
        );
    }
    for body in [
        json!({ "error": "unavailable" }),
        json!({ "error": "<b>account_banned</b>" }),
        json!({}),
    ] {
        assert_eq!(
            resolve(422, body, &[]).await,
            not(UnresolvedReason::Unavailable)
        );
    }
    let html = Stub::start(|_| Reply::text(422, "<html>")).await;
    assert_eq!(
        html.deployment()
            .resolve_video(s("https://x"), false)
            .await
            .unwrap(),
        not(UnresolvedReason::Unavailable)
    );
    // The same codes on any other status are not read as these.
    for code in ["members_only", "not_found"] {
        assert_eq!(
            resolve(404, json!({ "error": code }), &[]).await,
            not(UnresolvedReason::NotAVideo)
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_link_read_for_somebody_else_says_so_and_only_then() {
    let stub = answering(200, good(), &[]).await;
    let d = stub.deployment();

    d.clone()
        .resolve_video(s("library:home/1a2b"), true)
        .await
        .unwrap();
    d.resolve_video(s("library:home/1a2b"), false)
        .await
        .unwrap();

    let asked: Vec<Vec<(String, String)>> = stub.seen().iter().map(Seen::query).collect();
    assert_eq!(
        asked[0],
        vec![(s("url"), s("library:home/1a2b")), (s("for"), s("other"))]
    );
    assert_eq!(asked[1], vec![(s("url"), s("library:home/1a2b"))]);
    // URLSearchParams' spelling, which escapes the colon and the slash.
    assert_eq!(
        stub.seen()[1].target,
        "/api/watch/resolve?url=library%3Ahome%2F1a2b"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_two_refusals_of_a_person_are_told_apart() {
    assert_eq!(
        resolve(403, json!({ "error": "emby_not_allowed" }), &[]).await,
        not(UnresolvedReason::EmbyNotAllowed)
    );
    assert_eq!(
        resolve(403, json!({ "error": "not_allowed" }), &[]).await,
        not(UnresolvedReason::NotAllowed)
    );
    assert_eq!(
        resolve(403, json!({ "error": "something_else" }), &[]).await,
        not(UnresolvedReason::Failed)
    );
    assert_eq!(
        resolve(401, json!({ "error": "not_signed_in" }), &[]).await,
        not(UnresolvedReason::Failed)
    );
    let html = Stub::start(|_| Reply::text(403, "<html>")).await;
    assert_eq!(
        html.deployment()
            .resolve_video(s("https://x"), false)
            .await
            .unwrap(),
        not(UnresolvedReason::Failed)
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn no_gateway_or_nobody_signed_in_is_not_ready() {
    assert!(!answering(404, json!({}), &[])
        .await
        .deployment()
        .watch_ready()
        .await
        .unwrap());
    assert!(!answering(401, json!({}), &[])
        .await
        .deployment()
        .watch_ready()
        .await
        .unwrap());
    assert!(answering(200, json!({ "ready": true }), &[])
        .await
        .deployment()
        .watch_ready()
        .await
        .unwrap());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_bangumi_episode_cannot_play_where_the_server_relays_none() {
    let episode = with(
        good(),
        json!({ "play": { "kind": "bangumi" }, "proxy": { "kind": "hls", "url": format!("media?t={TICKET}") } }),
    );

    let ResolvedLink::Playable { video } = resolve(200, episode.clone(), &[]).await else {
        panic!()
    };
    assert_eq!(
        video.proxy.map(|proxy| proxy.kind),
        Some(ResolvedProxyKind::Hls)
    );

    assert_eq!(
        resolve(200, episode.clone(), &[("X-Watch-Relay", "off")]).await,
        not(UnresolvedReason::Unavailable)
    );

    let stub = answering(200, episode, &[("X-Watch-Relay", "off")]).await;
    assert_eq!(
        stub.deployment().video_info(s(TICKET)).await.unwrap(),
        TicketVideo::Unavailable
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_deployment_that_relays_nothing_gives_no_relayed_copy() {
    let ResolvedLink::Playable { video } = resolve(
        200,
        with(
            good(),
            json!({ "relay": true, "cover": "media?t=Poster_ticket_12345678" }),
        ),
        &[("X-Watch-Relay", "off")],
    )
    .await
    else {
        panic!()
    };
    assert_eq!(
        (
            video.title.as_str(),
            video.relay,
            video.cover.as_str(),
            video.proxy
        ),
        ("A film", false, "", None)
    );
    assert_eq!(
        video.play.url.as_deref(),
        Some("https://cdn.example.invalid/v.mp4")
    );

    let relayed_only = with(
        good(),
        json!({ "relay": true, "play": { "kind": "relay" } }),
    );
    assert_eq!(
        resolve(200, relayed_only, &[("X-Watch-Relay", "off")]).await,
        not(UnresolvedReason::Unavailable)
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_one_film_a_deployment_agreed_to_relay_names_its_grantor() {
    let film = with(
        good(),
        json!({ "relay": true, "play": { "kind": "relay" }, "cover": "media?t=Poster_ticket_12345678" }),
    );
    let ResolvedLink::Playable { video } =
        resolve(200, film, &[("X-Watch-Grantor", " eeeeefffff ")]).await
    else {
        panic!()
    };

    assert_eq!(video.play.kind, ResolvedPlayKind::Relay);
    assert!(video.relay);
    assert!(video
        .cover
        .ends_with("/api/watch/media?t=Poster_ticket_12345678"));
    assert_eq!(video.grantor.as_deref(), Some("eeeeefffff"));
}

async fn info(ticket: &str, body: Value, headers: &[(&str, &str)]) -> TicketVideo {
    let stub = answering(200, body, headers).await;
    let found = stub.deployment().video_info(s(ticket)).await.unwrap();
    if let Some(asked) = stub.seen().first() {
        assert_eq!(asked.query(), vec![(s("t"), s(ticket))]);
    }
    found
}

fn film(ticket: &str) -> Value {
    with(
        good(),
        json!({ "ticket": ticket, "proxy": { "kind": "hls", "url": format!("media?t={ticket}") }, "relay": true, "play": { "kind": "relay" } }),
    )
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_ticket_reads_as_what_the_server_says_it_plays() {
    let ticket = "Granted_film_ticket_001";
    let TicketVideo::Known { video } =
        info(ticket, film(ticket), &[("X-Watch-Grantor", "eeeeefffff")]).await
    else {
        panic!()
    };
    assert_eq!(video.play.kind, ResolvedPlayKind::Relay);
    assert!(video
        .proxy
        .unwrap()
        .url
        .ends_with(&format!("/api/watch/media?t={ticket}")));
    assert_eq!(video.grantor.as_deref(), Some("eeeeefffff"));

    // Not "failed": a retry does not play a film whose grant was taken away.
    let ticket = "Ungranted_film_ticket_02";
    assert_eq!(
        info(ticket, film(ticket), &[("X-Watch-Relay", "off")]).await,
        TicketVideo::Unavailable
    );

    let ticket = "Nobodys_film_ticket_006";
    let plain = with(
        good(),
        json!({ "ticket": ticket, "proxy": { "kind": "file", "url": format!("media?t={ticket}") } }),
    );
    let TicketVideo::Known { video } = info(ticket, plain, &[]).await else {
        panic!()
    };
    assert_eq!(video.grantor, None);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_ticket_answered_about_another_or_unreadably_is_failed_not_unavailable() {
    let ticket = "Other_answer_ticket_0007";
    let other = with(
        good(),
        json!({ "ticket": "Someone_elses_ticket_08", "proxy": { "kind": "hls", "url": "media?t=Someone_elses_ticket_08" }, "play": { "kind": "bangumi" } }),
    );
    assert_eq!(
        info(ticket, other, &[("X-Watch-Relay", "off")]).await,
        TicketVideo::Failed
    );

    let garbled = "Garbled_answer_ticket_09";
    let body = with(
        good(),
        json!({ "ticket": garbled, "play": { "kind": "bangumi" } }),
    );
    assert_eq!(
        info(garbled, body, &[("X-Watch-Relay", "off")]).await,
        TicketVideo::Failed
    );

    let expired = answering(410, json!({}), &[]).await;
    assert_eq!(
        expired
            .deployment()
            .video_info(s("Expired_film_ticket_0003"))
            .await
            .unwrap(),
        TicketVideo::Failed
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_ticket_is_never_asked_about_where_it_is_not_one() {
    let stub = answering(200, good(), &[]).await;
    assert_eq!(
        stub.deployment()
            .video_info(s("https://evil.example.invalid/v.mp4"))
            .await
            .unwrap(),
        TicketVideo::Failed
    );
    assert!(stub.seen().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_media_servers_and_their_items_are_what_the_gateway_makes() {
    let stub = Stub::start(|seen| match seen.query().first().map(|(_, op)| op.as_str()) {
        Some("servers") => Reply::json(200, json!({ "servers": [{ "key": "home", "name": "Home" }, { "key": "../x", "name": "Bad" }] })),
        Some("views") => Reply::json(
            200,
            json!({ "items": [
                { "id": "1a2b", "name": "Film", "type": "Movie", "folder": false, "playable": true, "year": 2020, "duration": 5400,
                  "image": "library?op=image&server=home&id=1a2b&tag=abc" },
                { "id": "3c", "name": "Odd poster", "image": "https://evil.invalid/p.jpg" },
                { "id": "not-hex!", "name": "Bad id" },
            ] }),
        ),
        _ => Reply::json(502, json!({})),
    })
    .await;
    let d = stub.deployment();

    let servers = d.clone().media_servers().await.unwrap();
    assert_eq!(
        servers
            .iter()
            .map(|one| one.key.as_str())
            .collect::<Vec<_>>(),
        vec!["home"]
    );
    assert_eq!(stub.last().target, "/api/watch/library?op=servers");

    let found = d.clone().browse_media(s("home"), None).await.unwrap();
    assert_eq!(
        found.iter().map(|one| one.id.as_str()).collect::<Vec<_>>(),
        vec!["1a2b", "3c"]
    );
    assert_eq!(
        found[0].image,
        format!(
            "{}/api/watch/library?op=image&server=home&id=1a2b&tag=abc",
            stub.base
        )
    );
    assert_eq!(
        (found[0].year, found[0].duration, found[0].playable),
        (2020, 5400.0, true)
    );
    assert_eq!(found[1].image, "");
    assert_eq!(
        stub.last().target,
        "/api/watch/library?op=views&server=home"
    );

    // Not empty where the server did not answer: that is a failure, not a
    // library with nothing in it.
    assert_eq!(
        d.clone().search_media(s("home"), s("x")).await.unwrap_err(),
        refused(502, "")
    );
    assert_eq!(
        stub.last().target,
        "/api/watch/library?op=search&server=home&q=x"
    );

    let _ = d.browse_media(s("home"), Some(s("ff"))).await;
    assert_eq!(
        stub.last().target,
        "/api/watch/library?op=items&server=home&parent=ff"
    );
}

// ----------------------------------------------------------------------------
// music.test.ts.

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_libraries_are_read_signed_in_or_not() {
    let stub = answering(
        200,
        json!({ "sources": [{ "id": "a", "name": "A", "signedIn": true }, { "id": "b", "name": "B", "signedIn": false }, { "id": "", "name": "C" }] }),
        &[],
    )
    .await;
    let found = stub.deployment().music_sources().await.unwrap();

    assert_eq!(
        found
            .iter()
            .map(|one| (one.id.as_str(), one.signed_in))
            .collect::<Vec<_>>(),
        vec![("a", true), ("b", false)]
    );
    assert_eq!(
        answering(401, json!({ "error": "not_signed_in" }), &[])
            .await
            .deployment()
            .music_sources()
            .await
            .unwrap_err(),
        refused(401, "not_signed_in")
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn searching_asks_as_the_web_asks_and_keeps_what_a_track_needs() {
    let stub = answering(
        200,
        json!({ "tracks": [
            { "id": "1", "title": "Sunny Day", "artists": ["Jay"], "album": "Ye Hui Mei", "cover": "c", "duration": 269 },
            { "id": "", "title": "no id" },
            { "title": "no id either" },
        ] }),
        &[],
    )
    .await;

    let found = stub
        .deployment()
        .music_search(s("a"), s("sunny"), 1)
        .await
        .unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(
        (
            found[0].title.as_str(),
            found[0].artists.clone(),
            found[0].duration
        ),
        ("Sunny Day", vec![s("Jay")], 269.0)
    );
    assert_eq!(
        stub.last().target,
        "/api/music/search?source=a&q=sunny&page=1"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn what_a_track_is_and_when_it_cannot_be_played() {
    let lossless = answering(
        200,
        json!({ "format": "flac", "rate": 44100, "channels": 2, "bits": 24, "tier": "lossless" }),
        &[],
    )
    .await;
    let audio = lossless
        .deployment()
        .music_track(s("a"), s("1"), MusicQuality::Best)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        (
            audio.format.as_str(),
            audio.rate,
            audio.channels,
            audio.bits,
            audio.tier.as_str()
        ),
        ("flac", 44100, 2, 24, "lossless")
    );
    assert_eq!(
        lossless.last().target,
        "/api/music/track?source=a&id=1&quality=best"
    );

    // Channels nobody could read are two, as music.ts has it.
    let mono = answering(200, json!({ "format": "mp3", "channels": 0 }), &[]).await;
    assert_eq!(
        mono.deployment()
            .music_track(s("a"), s("1"), MusicQuality::Lossy)
            .await
            .unwrap()
            .unwrap()
            .channels,
        2
    );

    // Unavailable, not a failure, where the account may not play it.
    let refused_track = answering(404, json!({ "error": "unavailable" }), &[]).await;
    assert_eq!(
        refused_track
            .deployment()
            .music_track(s("a"), s("1"), MusicQuality::Best)
            .await
            .unwrap(),
        None
    );

    let broken = answering(502, json!({ "error": "upstream" }), &[]).await;
    assert_eq!(
        broken
            .deployment()
            .music_track(s("a"), s("1"), MusicQuality::Best)
            .await
            .unwrap_err(),
        refused(502, "upstream")
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_shared_music_link_is_read_or_is_nothing() {
    let song = answering(200, json!({ "source": "a", "kind": "song", "title": "T", "tracks": [{ "id": "1", "title": "x" }] }), &[]).await;
    let linked = song
        .deployment()
        .music_link(s("listen: https://music.example/s/1"))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        (linked.kind, linked.tracks.len()),
        (tomoshibi_protocol::api::MusicLinkKind::Song, 1)
    );
    assert_eq!(
        song.last().query(),
        vec![(s("text"), s("listen: https://music.example/s/1"))]
    );

    let nothing = answering(404, json!({ "error": "not_a_link" }), &[]).await;
    assert_eq!(
        nothing.deployment().music_link(s("hello")).await.unwrap(),
        None
    );
}
