//! The relays a deployment offers, and which one is nearest.
//!
//! Follows web/src/live/relays.ts, and reads the list the way the server writes
//! it in `GET /api/relays` (internal/app/app.go, `relayList` and `relayEntry`).
//! The choice is made by timing each candidate exactly as the web client does,
//! so a native participant lands where a browser on the same network would:
//!
//! - nothing is measured unless the deployment says it will act on a
//!   measurement (`measure`), and then never a relay kept for administrators or
//!   one out of service;
//! - a list of fewer than two is answered without measuring;
//! - where every candidate advertises a STUN `probe`, one round trip over UDP is
//!   timed to each; if any of them does not answer, every relay is timed the
//!   other way instead, so the readings stay comparable;
//! - otherwise the signalling socket, `<url>/rtc`, is opened with no token, and
//!   whatever comes back first (the upgrade, the refusal, or an error) is the
//!   answer;
//! - 1.5 s per probe, all at once, the fastest wins and ties go to the earlier
//!   relay in the list; the winner is trusted for five minutes, and [`forget`]
//!   drops it when a call is lost.
//!
//! Two things differ from the browser, both because a browser cannot do them.
//! The UDP round trip is a bare STUN binding request (RFC 5389) sent from a
//! socket of our own, where the web has to build an RTCPeerConnection to make
//! the browser send the same request. And the signalling probe is written by
//! hand as the WebSocket opening handshake, because the only other request a
//! relay may ever see from a client is that one: a relay in mainland China is
//! taken off the air when it is seen answering ordinary web requests, which is
//! why the web stopped measuring with a fetch, and why nothing here may follow a
//! redirect, retry over another method, or touch any other path.

use std::{
    collections::HashMap,
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};

use base64::Engine as _;
use futures_util::future::join_all;
use serde_json::Value;
use tokio::{io::AsyncWriteExt as _, net::UdpSocket};

use crate::ProtocolError;

/// How long one relay is given to answer (relays.ts, `PROBE_TIMEOUT_MS`).
///
/// This sits between somebody pressing join and their call starting. A relay
/// that has not answered in a second and a half is not the one to hold a
/// conversation on, and one that is down must cost no more than this.
pub const PROBE_TIMEOUT: Duration = Duration::from_millis(1500);

/// How long a choice is trusted before it is measured again (relays.ts,
/// `CACHE_MS`): long enough that three joins in a row measure once, short
/// enough that a route which changed during the day is noticed. Held in memory
/// only, because a measurement belongs to the network the device is on now.
pub const CACHE_FOR: Duration = Duration::from_secs(5 * 60);

/// The port a STUN URL means when it names none (RFC 5389, section 9). The web
/// hands `stun:<probe>` to the browser, which fills this in for a probe written
/// without a port, so the same probe has to reach the same place here.
const STUN_PORT: u16 = 3478;

/// The first retransmission interval for a binding request over UDP, doubled
/// after every send as RFC 5389 section 7.2.1 asks. 250 ms is the initial
/// interval WebRTC's own STUN client uses, so a request lost on the way is sent
/// again on the schedule a browser would send it, inside the same 1.5 s.
const STUN_FIRST_RETRY: Duration = Duration::from_millis(250);

/// One relay as `GET /api/relays` describes it.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct Relay {
    /// The key a join asks for. Not a word to show anybody; `label` is.
    pub name: String,
    /// The signalling address. The server withholds it from anybody but an
    /// administrator whenever the relay has a `probe`, so it is often absent.
    pub url: Option<String>,
    pub region: Option<String>,
    /// What a person is shown instead of the name.
    pub label: Option<String>,
    /// Where the machine is, for the globe. Absent where nobody has said, and a
    /// relay without a place is left off rather than guessed at.
    pub lat: Option<f64>,
    pub lon: Option<f64>,
    /// Where it answers STUN binding requests, as host:port.
    pub probe: Option<String>,
    /// Here, but not taking calls: shown greyed, never measured or chosen.
    pub maintenance: bool,
    /// Held in reserve; works, but every byte goes the long way.
    pub fallback: bool,
    /// Only administrators are offered it, and only they may ask for it.
    pub admin_only: bool,
}

/// The whole answer: the relays, whether the client is expected to measure,
/// and how much of the fleet is up.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct RelayList {
    pub relays: Vec<Relay>,
    /// Whether this deployment will act on a measurement. When it is false a
    /// measurement would be a delay before every join that changed nothing.
    pub measure: bool,
    /// How many relays are answering, out of `total`. Both are zero where the
    /// deployment did not say (an older one, or a failed request), which is
    /// where the web's lobby shows no fleet light at all.
    pub online: u32,
    pub total: u32,
}

impl RelayList {
    /// What the web's `relays()` returns for every failure: nothing to choose,
    /// and nothing to measure, so the join proceeds without a preference.
    pub fn empty() -> Self {
        RelayList { relays: Vec::new(), measure: false, online: 0, total: 0 }
    }
}

/// One relay's reading for somebody choosing by hand.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct RelayTiming {
    pub name: String,
    /// The round trip in milliseconds, or nothing where it did not answer or
    /// was not measured.
    pub millis: Option<f64>,
}

/// Reads `GET /api/relays` from the deployment at `base`.
///
/// Never fails for anything the web client shrugs off. A status other than
/// success, a body that is not the expected JSON, and a request that did not
/// get through all come back as [`RelayList::empty`], as relays.ts answers
/// them: measuring is an optimisation, and an optimisation that can stop a
/// call from happening is worse than none. Only a `base` that cannot carry a
/// path is an error, because that is a mistake in the caller and not in the
/// deployment.
///
/// Asked at the origin's `/api/relays`, as the web's relative fetch is, with
/// the client's cookies: an administrator's session is what decides whether
/// the reserved relays and the addresses are in the answer.
pub async fn fetch(client: &reqwest::Client, base: &url::Url) -> Result<RelayList, ProtocolError> {
    if base.cannot_be_a_base() {
        return Err(ProtocolError::Invalid { detail: format!("{base} cannot hold a path") });
    }

    let endpoint = base
        .join("/api/relays")
        .map_err(|fault| ProtocolError::Invalid { detail: fault.to_string() })?;

    let response = match client
        .get(endpoint)
        .header(reqwest::header::ACCEPT, "application/json")
        .send()
        .await
    {
        Ok(response) => response,
        Err(_) => return Ok(RelayList::empty()),
    };

    if !response.status().is_success() {
        return Ok(RelayList::empty());
    }

    let Ok(body) = response.bytes().await else {
        return Ok(RelayList::empty());
    };

    Ok(parse(&body))
}

/// Reads a `GET /api/relays` body, leniently.
///
/// The web takes the body as it comes: `relays` when it is an array and
/// nothing otherwise, `measure` only when it is literally `true`, `fleet`
/// passed through. Here each relay has to be a JSON object with a string
/// `name` to be kept, because a relay without one cannot be asked for; any
/// field of the wrong type is read as absent, and fields nobody knows are
/// ignored, so a newer server does not break an older app.
pub fn parse(body: &[u8]) -> RelayList {
    let Ok(Value::Object(body)) = serde_json::from_slice::<Value>(body) else {
        return RelayList::empty();
    };

    let relays = match body.get("relays") {
        Some(Value::Array(relays)) => relays.iter().filter_map(relay).collect(),
        _ => Vec::new(),
    };

    let measure = body.get("measure") == Some(&Value::Bool(true));

    let count = |key: &str| -> u32 {
        body.get("fleet")
            .and_then(|fleet| fleet.get(key))
            .and_then(Value::as_u64)
            .map(|n| n.min(u64::from(u32::MAX)) as u32)
            .unwrap_or(0)
    };

    RelayList { relays, measure, online: count("online"), total: count("total") }
}

fn relay(value: &Value) -> Option<Relay> {
    let entry = value.as_object()?;

    let text = |key: &str| entry.get(key).and_then(Value::as_str).map(str::to_owned);
    let number = |key: &str| entry.get(key).and_then(Value::as_f64).filter(|n| n.is_finite());
    // The server writes these with `omitempty`, so absent is false; anything
    // but a literal true is false too, which is how the web's truthiness reads
    // every value the server can send.
    let flag = |key: &str| entry.get(key) == Some(&Value::Bool(true));

    Some(Relay {
        name: text("name")?,
        url: text("url"),
        region: text("region"),
        label: text("label"),
        lat: number("lat"),
        lon: number("lon"),
        probe: text("probe"),
        maintenance: flag("maintenance"),
        fallback: flag("fallback"),
        admin_only: flag("adminOnly"),
    })
}

/// The relay to ask for when joining, or `None` to let the server choose.
///
/// `None` when the deployment does not want a measurement, when there is
/// nothing to choose between, and when nothing answered: the server then
/// keeps the room together, a worse route but a working call.
pub async fn preferred(list: &RelayList) -> Option<String> {
    if !list.measure {
        return None;
    }

    fastest(&list.relays).await
}

/// The last choice and when its measurement began.
struct Choice {
    name: String,
    at: Instant,
}

static CHOSEN: Mutex<Option<Choice>> = Mutex::new(None);

fn chosen() -> std::sync::MutexGuard<'static, Option<Choice>> {
    // A panic elsewhere while holding this cannot leave a half-written choice,
    // so a poisoned lock is still a usable one.
    CHOSEN.lock().unwrap_or_else(|poison| poison.into_inner())
}

/// Measures every usable relay at once and names the fastest (relays.ts,
/// `fastest`).
///
/// Never a relay kept for administrators and never one out of service: the
/// automatic choice is for somebody who did not choose, and landing them on a
/// relay they will be refused at is worse than not choosing. Fewer than two
/// left is answered without a single packet, because measuring a list of one
/// cannot change the answer.
pub async fn fastest(list: &[Relay]) -> Option<String> {
    let usable: Vec<Relay> =
        list.iter().filter(|relay| !relay.admin_only && !relay.maintenance).cloned().collect();

    if usable.len() < 2 {
        return usable.first().map(|relay| relay.name.clone());
    }

    let now = Instant::now();
    if let Some(name) = remembered(chosen().as_ref(), &usable, now) {
        return Some(name);
    }

    let took = measure(&usable).await;
    let best = best(&took)?;

    *chosen() = Some(Choice { name: best.clone(), at: now });
    Some(best)
}

/// The cached choice, while it is fresh and still on offer.
///
/// A cached name that has left the list is not sent: the server would ignore
/// it, silently costing this client the measurement it thought it had.
fn remembered(choice: Option<&Choice>, list: &[Relay], now: Instant) -> Option<String> {
    let choice = choice?;
    let fresh = now.saturating_duration_since(choice.at) < CACHE_FOR;

    (fresh && list.iter().any(|relay| relay.name == choice.name)).then(|| choice.name.clone())
}

/// The quickest answer. Strictly quicker displaces, so of two equal readings
/// the one earlier in the list stays, as the web's loop over its map does.
fn best(took: &[(String, Option<Duration>)]) -> Option<String> {
    let mut best: Option<(&str, Duration)> = None;

    for (name, one) in took {
        let Some(one) = *one else { continue };
        if best.is_none_or(|(_, quickest)| one < quickest) {
            best = Some((name, one));
        }
    }

    best.map(|(name, _)| name.to_owned())
}

/// Forgets the last choice (relays.ts, `forget`).
///
/// For when a call drops, which is the moment the held reading is worst: it
/// names as fastest the machine that has just stopped carrying a conversation.
/// A relay somebody chose by hand is not this module's to forget.
pub fn forget() {
    *chosen() = None;
}

/// Forgets the last automatic relay choice; the app calls this when a call is
/// lost, as the web does.
#[uniffi::export]
pub fn forget_relay_measurement() {
    forget();
}

/// Times every relay in `list` and says how long each took (relays.ts,
/// `timings`), for somebody choosing by hand.
///
/// Uncached on purpose: the picker is opened because the automatic answer just
/// disappointed somebody, and a reading from five minutes ago answers a
/// question they did not ask. A relay out of service is shown without a
/// reading and is not contacted, and does not decide how the others are
/// measured either. A reserved relay is measured: whoever was sent it is an
/// administrator.
pub async fn timings(list: &[Relay]) -> Vec<RelayTiming> {
    let measurable: Vec<Relay> = list.iter().filter(|relay| !relay.maintenance).cloned().collect();
    let took: HashMap<String, Option<Duration>> = measure(&measurable).await.into_iter().collect();

    list.iter()
        .map(|relay| RelayTiming {
            name: relay.name.clone(),
            millis: took.get(&relay.name).copied().flatten().map(|took| took.as_secs_f64() * 1000.0),
        })
        .collect()
}

/// [`timings`], for the relay picker.
#[uniffi::export]
pub async fn relay_timings(relays: Vec<Relay>) -> Vec<RelayTiming> {
    crate::runtime::on_core(async move { timings(&relays).await }).await
}

/// Times a list, over UDP where every relay allows it and over the signalling
/// socket otherwise (relays.ts, `measure`).
///
/// One kind of reading for the whole list, never a mixture: a STUN answer is
/// one round trip and the socket is about three, and ranking the two together
/// hands the call to whichever relays happen to have a probe configured. So
/// one probe that fails sends everybody the slower way, which keeps every
/// number comparable and makes a broken probe port visible.
async fn measure(list: &[Relay]) -> Vec<(String, Option<Duration>)> {
    let probed = |relay: &Relay| relay.probe.as_deref().is_some_and(|probe| !probe.is_empty());

    if !list.is_empty() && list.iter().all(probed) {
        let took = join_all(list.iter().map(over_udp)).await;
        if took.iter().all(Option::is_some) {
            return answered(list, took);
        }
    }

    answered(list, join_all(list.iter().map(over_signalling)).await)
}

/// Pairs readings with names the way the web's `Map` does: one entry per name,
/// in the order each name first appears, holding the last reading for it.
fn answered(list: &[Relay], took: Vec<Option<Duration>>) -> Vec<(String, Option<Duration>)> {
    let mut out: Vec<(String, Option<Duration>)> = Vec::with_capacity(list.len());

    for (relay, one) in list.iter().zip(took) {
        match out.iter_mut().find(|(name, _)| *name == relay.name) {
            Some(slot) => slot.1 = one,
            None => out.push((relay.name.clone(), one)),
        }
    }

    out
}

/// One round trip over UDP to the relay's probe, or nothing.
///
/// Timed from before the name is resolved, as the browser's clock starts
/// before it resolves the STUN server's, to the first valid answer.
async fn over_udp(relay: &Relay) -> Option<Duration> {
    let probe = relay.probe.as_deref()?;
    let started = Instant::now();

    tokio::time::timeout(PROBE_TIMEOUT, binding(probe)).await.ok().flatten()?;

    Some(started.elapsed())
}

/// The relay's signalling socket, opened and timed, or nothing.
///
/// Any answer counts: the upgrade accepted, the missing token refused, or the
/// connection failing outright. That is what relays.ts does, taking `open`,
/// `close` and `error` alike, because a refusal is a complete round trip. A
/// relay without an address, or with one a browser would not open a socket to,
/// has no reading, as the browser's constructor throwing gives none.
async fn over_signalling(relay: &Relay) -> Option<Duration> {
    let target = signalling(relay.url.as_deref()?)?;
    // Built before the clock starts: the first build loads the system's
    // certificate roots, which is the device's time and not the relay's.
    let client = prober()?;

    let started = Instant::now();
    tokio::time::timeout(PROBE_TIMEOUT, open(&client, target)).await.ok()?;

    Some(started.elapsed())
}

/// Where the probe's socket goes: `<url>/rtc`, with trailing slashes on the
/// relay's address dropped as the web drops them, and the scheme a browser's
/// WebSocket would use (http and https are read as ws and wss, as the
/// WebSocket constructor reads them; anything else, or a fragment, is refused).
///
/// Returned as the http(s) URL the handshake is written to.
fn signalling(url: &str) -> Option<url::Url> {
    let mut target = url::Url::parse(&format!("{}/rtc", url.trim_end_matches('/'))).ok()?;

    if target.fragment().is_some() {
        return None;
    }

    let scheme = match target.scheme() {
        "ws" | "http" => "http",
        "wss" | "https" => "https",
        _ => return None,
    };
    target.set_scheme(scheme).ok()?;

    Some(target)
}

/// The one HTTP client the signalling probe uses.
///
/// Its own rather than the caller's, for what it must not do: follow a
/// redirect (a second request), go through a proxy (which would time the
/// proxy), keep the connection afterwards (a socket held open on a relay this
/// client may not use), or send the deployment's cookies to a relay. HTTP/1.1
/// only, because a WebSocket opening is an HTTP/1.1 upgrade.
fn prober() -> Option<reqwest::Client> {
    static CLIENT: OnceLock<Option<reqwest::Client>> = OnceLock::new();

    CLIENT
        .get_or_init(|| {
            reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .no_proxy()
                .pool_max_idle_per_host(0)
                .http1_only()
                .build()
                .ok()
        })
        .clone()
}

/// Sends the WebSocket opening handshake (RFC 6455, section 4.1) with no token
/// and waits for whatever comes back.
///
/// The request is the upgrade a call begins with and nothing more. Should a
/// relay ever accept it, the socket is closed with a close frame, as the
/// browser's `socket.close()` does, and not left open on a relay this client
/// may not use.
async fn open(client: &reqwest::Client, target: url::Url) {
    use reqwest::header::{CONNECTION, SEC_WEBSOCKET_KEY, SEC_WEBSOCKET_VERSION, UPGRADE};

    let key = base64::engine::general_purpose::STANDARD.encode(rand::random::<[u8; 16]>());

    let sent = client
        .get(target)
        .header(CONNECTION, "Upgrade")
        .header(UPGRADE, "websocket")
        .header(SEC_WEBSOCKET_VERSION, "13")
        .header(SEC_WEBSOCKET_KEY, key)
        .send()
        .await;

    let Ok(response) = sent else { return };
    if response.status() != reqwest::StatusCode::SWITCHING_PROTOCOLS {
        return;
    }

    tokio::spawn(tokio::time::timeout(PROBE_TIMEOUT, async move {
        let Ok(mut socket) = response.upgrade().await else { return };

        // A close frame with no status, masked as every frame from a client
        // must be.
        let mask = rand::random::<[u8; 4]>();
        let frame = [0x88, 0x80, mask[0], mask[1], mask[2], mask[3]];
        let _ = socket.write_all(&frame).await;
        let _ = socket.shutdown().await;
    }));
}

/// The STUN answer to one binding request: the address the far end saw.
///
/// Sent from a fresh socket to the address `probe` names, and retransmitted
/// unchanged, the same transaction, until an answer or the caller's deadline.
/// Nothing but the 20-byte binding request ever leaves the socket: no
/// attributes, no credentials, no second message.
pub(crate) async fn binding(probe: &str) -> Option<SocketAddr> {
    let target = resolve(probe).await?;

    let local = match target {
        SocketAddr::V4(_) => SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0),
        SocketAddr::V6(_) => SocketAddr::new(IpAddr::V6(Ipv6Addr::UNSPECIFIED), 0),
    };

    let socket = UdpSocket::bind(local).await.ok()?;
    // Connected, so only datagrams from the probe are read at all.
    socket.connect(target).await.ok()?;

    let id: [u8; 12] = rand::random();
    let request = binding_request(&id);

    let mut buffer = [0u8; 1500];
    let mut wait = STUN_FIRST_RETRY;

    loop {
        socket.send(&request).await.ok()?;

        let until = tokio::time::Instant::now() + wait;
        loop {
            match tokio::time::timeout_at(until, socket.recv(&mut buffer)).await {
                Err(_) => break,
                // Refused by the far side's host, or any other failure of the
                // socket itself: no answer is coming on it.
                Ok(Err(_)) => return None,
                Ok(Ok(read)) => match binding_answer(&buffer[..read], &id) {
                    Some(Ok(seen)) => return Some(seen),
                    Some(Err(())) => return None,
                    None => continue,
                },
            }
        }

        wait = wait.saturating_mul(2);
    }
}

/// The address a STUN server is reached at, from the `host:port` a probe is
/// written as, the port defaulting as a `stun:` URL's does.
async fn resolve(probe: &str) -> Option<SocketAddr> {
    if let Ok(address) = probe.parse::<SocketAddr>() {
        return Some(address);
    }

    let (host, port) = if let Some(rest) = probe.strip_prefix('[') {
        let (host, after) = rest.split_once(']')?;
        let port = match after {
            "" => STUN_PORT,
            after => after.strip_prefix(':')?.parse().ok()?,
        };
        (host, port)
    } else {
        match probe.split_once(':') {
            None => (probe, STUN_PORT),
            Some((_, port)) if port.contains(':') => return None,
            Some((host, port)) => (host, port.parse().ok()?),
        }
    };

    if host.is_empty() {
        return None;
    }

    tokio::net::lookup_host((host, port)).await.ok()?.next()
}

/// A STUN binding request with no attributes (RFC 5389, section 6).
fn binding_request(id: &[u8; 12]) -> [u8; 20] {
    let mut request = [0u8; 20];
    request[0..2].copy_from_slice(&BINDING_REQUEST.to_be_bytes());
    // Length zero: no attributes follow.
    request[4..8].copy_from_slice(&MAGIC_COOKIE.to_be_bytes());
    request[8..20].copy_from_slice(id);
    request
}

const BINDING_REQUEST: u16 = 0x0001;
const BINDING_SUCCESS: u16 = 0x0101;
const BINDING_ERROR: u16 = 0x0111;
const MAGIC_COOKIE: u32 = 0x2112_A442;
const MAPPED_ADDRESS: u16 = 0x0001;
const XOR_MAPPED_ADDRESS: u16 = 0x0020;

/// Reads a datagram as the answer to transaction `id`.
///
/// `None` for anything that is not an answer to it (somebody else's, or not
/// STUN at all), which is ignored and waited past. `Some(Err)` for an error
/// response to it. `Some(Ok)` for a success that says where it saw us: a
/// success without a mapped address gives the browser no reflexive candidate,
/// so it is no answer there and none here either.
fn binding_answer(packet: &[u8], id: &[u8; 12]) -> Option<Result<SocketAddr, ()>> {
    if packet.len() < 20 {
        return None;
    }

    let kind = u16::from_be_bytes([packet[0], packet[1]]);
    let length = usize::from(u16::from_be_bytes([packet[2], packet[3]]));
    let cookie = u32::from_be_bytes([packet[4], packet[5], packet[6], packet[7]]);

    if cookie != MAGIC_COOKIE || packet[8..20] != id[..] || length != packet.len() - 20 {
        return None;
    }

    match kind {
        BINDING_ERROR => return Some(Err(())),
        BINDING_SUCCESS => {}
        _ => return None,
    }

    let mut plain = None;
    let mut xored = None;

    let mut at = 20;
    while at + 4 <= packet.len() {
        let kind = u16::from_be_bytes([packet[at], packet[at + 1]]);
        let size = usize::from(u16::from_be_bytes([packet[at + 2], packet[at + 3]]));
        let value = packet.get(at + 4..at + 4 + size)?;

        match kind {
            XOR_MAPPED_ADDRESS => xored = xored.or_else(|| mapped(value, Some(&packet[4..20]))),
            MAPPED_ADDRESS => plain = plain.or_else(|| mapped(value, None)),
            _ => {}
        }

        // Attributes are padded to four bytes.
        at += 4 + size.div_ceil(4) * 4;
    }

    xored.or(plain).map(Ok)
}

/// A (XOR-)MAPPED-ADDRESS value; `mask` is the cookie and transaction id for
/// the XOR form (RFC 5389, section 15.2).
fn mapped(value: &[u8], mask: Option<&[u8]>) -> Option<SocketAddr> {
    let family = *value.get(1)?;
    let mut port = u16::from_be_bytes([*value.get(2)?, *value.get(3)?]);
    let mut address = value.get(4..)?.to_vec();

    if let Some(mask) = mask {
        port ^= (MAGIC_COOKIE >> 16) as u16;
        for (byte, with) in address.iter_mut().zip(mask) {
            *byte ^= with;
        }
    }

    let ip = match (family, address.len()) {
        (0x01, 4) => IpAddr::from(<[u8; 4]>::try_from(address).ok()?),
        (0x02, 16) => IpAddr::from(<[u8; 16]>::try_from(address).ok()?),
        _ => return None,
    };

    Some(SocketAddr::new(ip, port))
}

/// Whether a relay's region puts it in mainland China (relays.ts,
/// `mainland`).
///
/// By the deployment's own labels, read as the web reads them,
/// `/^cn(?:$|[-/\s])/i` on the trimmed region: "CN" alone, or followed by a
/// dash, a slash or a space, case aside. Nothing else counts, because a wrong
/// guess sends somebody abroad to the relay for every YouTube video.
#[uniffi::export]
pub fn mainland(region: Option<String>) -> bool {
    let region = region.unwrap_or_default();
    let region = region.trim_matches(js_space);

    let mut chars = region.chars();
    let starts = matches!(chars.next(), Some('c' | 'C')) && matches!(chars.next(), Some('n' | 'N'));

    starts && chars.next().is_none_or(|next| next == '-' || next == '/' || js_space(next))
}

/// What JavaScript's `trim()` and `\s` count as space (ECMAScript WhiteSpace
/// and LineTerminator), which is not quite Rust's: it includes the byte order
/// mark and leaves out U+0085.
fn js_space(c: char) -> bool {
    matches!(
        c,
        '\u{0009}' | '\u{000A}' | '\u{000B}' | '\u{000C}' | '\u{000D}' | '\u{0020}' | '\u{00A0}'
            | '\u{1680}' | '\u{2000}'..='\u{200A}' | '\u{2028}' | '\u{2029}' | '\u{202F}'
            | '\u{205F}' | '\u{3000}' | '\u{FEFF}'
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn named(name: &str) -> Relay {
        Relay {
            name: name.into(),
            url: None,
            region: None,
            label: None,
            lat: None,
            lon: None,
            probe: None,
            maintenance: false,
            fallback: false,
            admin_only: false,
        }
    }

    // The five minutes, which an integration test cannot wait out.
    #[test]
    fn a_choice_is_trusted_for_five_minutes_and_not_after() {
        let list = [named("hongkong"), named("tokyo")];
        let at = Instant::now();
        let choice = Choice { name: "hongkong".into(), at };

        assert_eq!(remembered(Some(&choice), &list, at), Some("hongkong".into()));
        assert_eq!(
            remembered(Some(&choice), &list, at + CACHE_FOR - Duration::from_millis(1)),
            Some("hongkong".into())
        );
        assert_eq!(remembered(Some(&choice), &list, at + CACHE_FOR), None);
        assert_eq!(remembered(None, &list, at), None);
    }

    #[test]
    fn a_remembered_choice_that_left_the_list_is_not_sent() {
        let at = Instant::now();
        let choice = Choice { name: "shanghai".into(), at };

        assert_eq!(remembered(Some(&choice), &[named("hongkong"), named("tokyo")], at), None);
    }

    // Equal readings cannot be produced on purpose over a real socket.
    #[test]
    fn a_tie_goes_to_the_earlier_relay() {
        let ms = Duration::from_millis;
        let took = vec![
            ("shanghai".to_owned(), None),
            ("hongkong".to_owned(), Some(ms(30))),
            ("tokyo".to_owned(), Some(ms(30))),
            ("seoul".to_owned(), Some(ms(31))),
        ];

        assert_eq!(best(&took), Some("hongkong".into()));
        assert_eq!(best(&[("shanghai".to_owned(), None)]), None);
    }

    #[test]
    fn a_name_offered_twice_keeps_its_first_place_and_last_reading() {
        let ms = Duration::from_millis;
        let list = [named("a"), named("b"), named("a")];

        assert_eq!(
            answered(&list, vec![Some(ms(5)), Some(ms(9)), None]),
            vec![("a".to_owned(), None), ("b".to_owned(), Some(ms(9)))]
        );
    }

    #[test]
    fn the_signalling_address_is_the_one_a_browser_would_open() {
        let at = |url: &str| signalling(url).map(|url| url.to_string());

        assert_eq!(at("wss://sh.example.com"), Some("https://sh.example.com/rtc".into()));
        assert_eq!(at("wss://sh.example.com///"), Some("https://sh.example.com/rtc".into()));
        assert_eq!(at("ws://127.0.0.1:7880/relay/"), Some("http://127.0.0.1:7880/relay/rtc".into()));
        assert_eq!(at("https://sh.example.com"), Some("https://sh.example.com/rtc".into()));
        assert_eq!(at("ftp://sh.example.com"), None);
        assert_eq!(at("not a url"), None);
    }

    #[test]
    fn a_binding_answer_is_matched_by_transaction_and_unmasked() {
        let id = [7u8; 12];
        let seen: SocketAddr = "203.0.113.7:51234".parse().unwrap();

        // Built the way the relay's responder builds it (internal/rtc/stun.go).
        let mut reply = Vec::new();
        reply.extend_from_slice(&BINDING_SUCCESS.to_be_bytes());
        reply.extend_from_slice(&12u16.to_be_bytes());
        reply.extend_from_slice(&MAGIC_COOKIE.to_be_bytes());
        reply.extend_from_slice(&id);
        reply.extend_from_slice(&XOR_MAPPED_ADDRESS.to_be_bytes());
        reply.extend_from_slice(&8u16.to_be_bytes());
        reply.extend_from_slice(&[0, 0x01]);
        reply.extend_from_slice(&(51234u16 ^ 0x2112).to_be_bytes());
        for (byte, cookie) in [203u8, 0, 113, 7].iter().zip(MAGIC_COOKIE.to_be_bytes()) {
            reply.push(byte ^ cookie);
        }

        assert_eq!(binding_answer(&reply, &id), Some(Ok(seen)));

        // Somebody else's transaction is not ours.
        assert_eq!(binding_answer(&reply, &[8u8; 12]), None);

        // A length that disagrees with what arrived is not an answer.
        let mut short = reply.clone();
        short.pop();
        assert_eq!(binding_answer(&short, &id), None);

        // An error response to ours ends the wait.
        let mut refused = reply[..20].to_vec();
        refused[0..2].copy_from_slice(&BINDING_ERROR.to_be_bytes());
        refused[2..4].copy_from_slice(&0u16.to_be_bytes());
        assert_eq!(binding_answer(&refused, &id), Some(Err(())));

        // A success that says nothing about where it saw us gives a browser no
        // reflexive candidate, so it is not an answer.
        let mut bare = reply[..20].to_vec();
        bare[2..4].copy_from_slice(&0u16.to_be_bytes());
        assert_eq!(binding_answer(&bare, &id), None);
    }

    #[test]
    fn the_request_is_twenty_bytes_and_nothing_else() {
        let request = binding_request(&[9u8; 12]);

        assert_eq!(&request[0..2], &[0x00, 0x01]);
        assert_eq!(&request[2..4], &[0x00, 0x00]);
        assert_eq!(&request[4..8], &[0x21, 0x12, 0xA4, 0x42]);
        assert_eq!(&request[8..20], &[9u8; 12]);
    }
}
