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
//! - where every candidate advertises a STUN `probe`, round trips over UDP are
//!   timed to each; if any of them does not answer, every relay is timed the
//!   other way instead, so the readings stay comparable;
//! - otherwise the signalling socket, `<url>/rtc`, is opened with no token, and
//!   whatever comes back first (the upgrade, the refusal, or an error) is the
//!   answer;
//! - each relay is timed in [`ROUNDS`] rounds, one after another, every relay
//!   at once, each round given 1.5 s; a reading is the median of the rounds
//!   that answered, and a relay that answered fewer than [`QUORUM`] has not
//!   answered, whichever way it was timed, so its rounds stop at its third
//!   miss; over UDP the first relay out stops everybody's UDP rounds;
//! - the fastest wins and ties go to the earlier relay in the list; the winner
//!   is trusted for five minutes, and [`forget`] drops it when a call is lost.
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
    future::Future,
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};

use base64::Engine as _;
use futures_util::{future::join_all, stream::FuturesUnordered, StreamExt as _};
use serde_json::Value;
use tokio::{io::AsyncWriteExt as _, net::UdpSocket};

use crate::ProtocolError;

/// How long one round with one relay is given to answer (relays.ts,
/// `PROBE_TIMEOUT_MS`).
///
/// This sits between somebody pressing join and their call starting. A relay
/// that has not answered in a second and a half is not the one to hold a
/// conversation on. A round that runs out is a round the relay missed, and a
/// relay that is down is given up on at its third ([`QUORUM`]), so it costs
/// three of these over each kind of reading rather than five.
pub const PROBE_TIMEOUT: Duration = Duration::from_millis(1500);

/// How many times each relay is timed, one round after another (the owner's
/// rule of 2026-10-19, which the web follows in relays.ts).
///
/// One reading was not a measurement: the first round trip to a relay comes
/// back slow far more often than the rest (a name to resolve, a route and a
/// NAT mapping to be made, a radio to wake), so a single reading ranked
/// relays by which one happened to pay for that, and the picker showed a
/// number for the nearest machine that the call on it never had. Five, and
/// the median of them, lets one bad round fall out without letting one good
/// one decide. In turn rather than at once, because rounds sent together
/// share the slow start instead of leaving it behind; every relay is timed
/// at the same time as the others, as before, so the wait is the slowest
/// relay's five round trips and not the sum of everybody's.
pub const ROUNDS: usize = 5;

/// How many of the [`ROUNDS`] a relay has to answer to have a reading at all,
/// over UDP and over the socket alike (relays.ts, `ENOUGH`).
///
/// The median of one or two answers is the single reading this replaced, and
/// a relay that drops more than half of what it is sent is not one to put a
/// call on however quick the rounds that came back. Over UDP a relay short of
/// this is a broken probe port, which the all-or-nothing rule in [`measure`]
/// exists to make visible: it sends the whole list to the signalling socket.
/// It is also what bounds the wait: once a relay has missed three it cannot
/// reach three answers, and is not asked again.
pub const QUORUM: usize = 3;

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

    let list = parse(&body);
    remember_mainland(&list.relays);
    Ok(list)
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
/// one probe short of its [`QUORUM`] sends everybody the slower way, which
/// keeps every number comparable and makes a broken probe port visible.
async fn measure(list: &[Relay]) -> Vec<(String, Option<Duration>)> {
    let probed = |relay: &Relay| relay.probe.as_deref().is_some_and(|probe| !probe.is_empty());

    if !list.is_empty() && list.iter().all(probed) {
        if let Some(took) = over_udp(list).await {
            return answered(list, took.into_iter().map(Some).collect());
        }
    }

    answered(list, join_all(list.iter().map(over_signalling)).await)
}

/// Up to [`ROUNDS`] readings taken one after another (relays.ts, `inRounds`),
/// the rounds that answered, in order.
///
/// A round that brings nothing (refused outright, or the whole
/// [`PROBE_TIMEOUT`] with its retransmissions and no answer) is a miss and the
/// next round is taken all the same, since one lost round is not a relay
/// that is down. At the third miss the relay can no longer reach [`QUORUM`]
/// answers, and is not asked again: a dead relay costs three timeouts, not
/// five.
async fn rounds<F, R>(mut round: F) -> Vec<Duration>
where
    F: FnMut() -> R,
    R: Future<Output = Option<Duration>>,
{
    let mut took = Vec::with_capacity(ROUNDS);
    let mut missed = 0;

    for _ in 0..ROUNDS {
        match round().await {
            Some(one) => took.push(one),
            None => {
                missed += 1;
                if missed > ROUNDS - QUORUM {
                    break;
                }
            }
        }
    }

    took
}

/// The median of the rounds that answered, where at least [`QUORUM`] did
/// (relays.ts, `reading`).
///
/// Of the rounds that answered rather than of all five with the misses
/// counted as slow: a miss is no reading, and a relay that answered four
/// times in thirty milliseconds and once not at all is a thirty-millisecond
/// relay that dropped a packet. The middle one of an odd number; of four, the
/// mean of the middle two, which is what a median of four is. The web takes it
/// the same way, so a browser and an app on one network rank alike.
fn reading(took: &[Duration]) -> Option<Duration> {
    if took.len() < QUORUM {
        return None;
    }

    let mut sorted = took.to_vec();
    sorted.sort_unstable();

    let middle = sorted.len() / 2;
    if sorted.len() % 2 == 1 {
        Some(sorted[middle])
    } else {
        Some((sorted[middle - 1] + sorted[middle]) / 2)
    }
}

/// What a debug build says about each relay it timed, on standard error: the
/// rounds and what was made of them, which is the only way to see why a call
/// went where it did. Nothing in a release build.
fn report(name: &str, over: &str, took: &[Duration], made: Option<Duration>) {
    if !cfg!(debug_assertions) {
        return;
    }

    let ms = |one: &Duration| format!("{:.1}", one.as_secs_f64() * 1000.0);
    let rounds = took.iter().map(ms).collect::<Vec<_>>().join(", ");
    let made = match made {
        Some(one) => format!("median {} ms", ms(&one)),
        None => "no reading".to_owned(),
    };

    eprintln!("tomoshibi relays: {name} over {over}: {} answered [{rounds}] ms, {made}", took.len());
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

/// Every relay's reading over UDP, in the list's order, or nothing at all
/// when any one of them has none.
///
/// The probes' names are resolved first, all at once and before any clock
/// starts. The single reading used to start its clock before resolving, so
/// the first number for every relay included a DNS lookup, which is the
/// device's network and not the relay's, and was much of why the first round
/// was always the slow one. A name that will not resolve within the timeout
/// is no reading, as before.
///
/// Every relay is then timed at once, and the moment one of them cannot have
/// a reading the rest are dropped, rounds in flight and all (relays.ts,
/// `inRounds` with `together`): the list is going to the signalling socket
/// whatever they say, and their remaining rounds would only delay that.
async fn over_udp(list: &[Relay]) -> Option<Vec<Duration>> {
    let targets = join_all(list.iter().map(|relay| async move {
        let probe = relay.probe.as_deref()?;
        tokio::time::timeout(PROBE_TIMEOUT, resolve(probe)).await.ok().flatten()
    }))
    .await;
    let targets: Vec<SocketAddr> = targets.into_iter().collect::<Option<_>>()?;

    let mut pending: FuturesUnordered<_> = list
        .iter()
        .zip(targets)
        .enumerate()
        .map(|(at, (relay, target))| async move { (at, over_udp_to(&relay.name, target).await) })
        .collect();

    let mut took = vec![Duration::ZERO; list.len()];
    while let Some((at, one)) = pending.next().await {
        took[at] = one?;
    }

    Some(took)
}

/// One relay's reading over UDP: its rounds, each a new binding request from
/// a new socket, as each of the web's rounds is a new peer connection, and
/// their [`reading`].
///
/// Each round is timed from just before its request is sent to its answer.
async fn over_udp_to(name: &str, target: SocketAddr) -> Option<Duration> {
    let took = rounds(|| async move {
        let started = Instant::now();
        tokio::time::timeout(PROBE_TIMEOUT, binding_at(target)).await.ok().flatten()?;
        Some(started.elapsed())
    })
    .await;

    let made = reading(&took);
    report(name, "udp", &took, made);
    made
}

/// The relay's signalling socket, opened and timed in rounds, or nothing.
///
/// Any answer counts: the upgrade accepted, the missing token refused, or the
/// connection failing outright. That is what relays.ts does, taking `open`,
/// `close` and `error` alike, because a refusal is a complete round trip. A
/// relay without an address, or with one a browser would not open a socket to,
/// has no reading, as the browser's constructor throwing gives none. Every
/// round is a connection of its own (the prober keeps none), as each of the
/// web's is a new WebSocket, and the rounds come to their [`reading`] as they
/// do over UDP; one relay out here stops nobody else, there being no other
/// way left to time them.
///
/// Never a relay in mainland China ([`mainland`]): it is left without a
/// reading and is not contacted. A mainland relay signals through another
/// (the server's SignalVia) because the providers in front of those machines
/// intercept TLS to a server without an ICP filing and take the machine off
/// the air for answering it. The server withholds such a relay's address
/// only where it has a probe, so one configured without a probe, or one
/// whose probe is down while another relay's is missing, had every client
/// open `wss://` to it each time it measured. Without a reading it is never
/// the fastest by this way of measuring, which is the price: an automatic
/// choice over the socket lands abroad. The web's relays.ts times the same
/// relays the same way and needs the same guard; that change is the
/// coordinator's to make on the web.
async fn over_signalling(relay: &Relay) -> Option<Duration> {
    if mainland(relay.region.clone()) {
        return None;
    }
    let target = signalling(relay.url.as_deref()?)?;
    // Built before the clock starts: the first build loads the system's
    // certificate roots, which is the device's time and not the relay's.
    let client = prober()?;

    let took = rounds(|| {
        let (client, target) = (&client, target.clone());
        async move {
            let started = Instant::now();
            tokio::time::timeout(PROBE_TIMEOUT, open(client, target)).await.ok()?;
            Some(started.elapsed())
        }
    })
    .await;

    let made = reading(&took);
    report(&relay.name, "signalling", &took, made);
    made
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

/// The STUN answer to one binding request to the server `probe` names: the
/// address the far end saw. See [`binding_at`].
pub(crate) async fn binding(probe: &str) -> Option<SocketAddr> {
    binding_at(resolve(probe).await?).await
}

/// The STUN answer to one binding request to `target`.
///
/// Sent from a fresh socket, and retransmitted unchanged, the same
/// transaction, until an answer or the caller's deadline. Nothing but the
/// 20-byte binding request ever leaves the socket: no attributes, no
/// credentials, no second message.
async fn binding_at(target: SocketAddr) -> Option<SocketAddr> {
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

/// The hosts of every relay a list read from the deployment ([`fetch`]) put
/// in mainland China, for [`tls_into_mainland`] to refuse a socket to.
static MAINLAND_HOSTS: Mutex<Vec<String>> = Mutex::new(Vec::new());

fn mainland_hosts() -> std::sync::MutexGuard<'static, Vec<String>> {
    // Only ever pushed to whole, so a poisoned lock is still a usable one.
    MAINLAND_HOSTS.lock().unwrap_or_else(|poison| poison.into_inner())
}

/// Notes the host of each relay in `list` that is in mainland China and whose
/// address the deployment gave. Kept for the life of the process: a relay
/// that was once in the mainland does not move, and forgetting one would be
/// the TLS handshake this exists to prevent.
pub fn remember_mainland(list: &[Relay]) {
    let mut hosts = mainland_hosts();
    for relay in list.iter().filter(|relay| mainland(relay.region.clone())) {
        let host = relay.url.as_deref().and_then(|url| url::Url::parse(url).ok());
        let Some(host) = host.as_ref().and_then(url::Url::host_str).map(str::to_ascii_lowercase) else {
            continue;
        };
        if !hosts.contains(&host) {
            hosts.push(host);
        }
    }
}

/// Whether opening `url` would start a TLS handshake (`wss:` or `https:`)
/// with a relay this process has been told is in mainland China.
///
/// The join answer's `url` is where the call's signalling goes. For a
/// mainland relay the server gives another relay's address (SignalVia), and
/// only where no relay can carry the signalling does it give the mainland
/// relay's own, on the reasoning that a call that may be intercepted beats
/// one that certainly fails. But the interception is not all: the TLS
/// handshake is what gets the relay taken off the air for everybody, so the
/// core's `Call::open` refuses it.
/// Only relays whose address some list gave can be recognised: the server
/// withholds the address of a relay with a probe from anybody but an
/// administrator, and those are not in the join's way unless the server's own
/// fallback puts them there. The web needs the same refusal (relays.ts and
/// room.ts); that change is the coordinator's to make on the web.
pub fn tls_into_mainland(url: &str) -> bool {
    let Ok(parsed) = url::Url::parse(url.trim()) else { return false };
    if !matches!(parsed.scheme(), "wss" | "https") {
        return false;
    }
    let Some(host) = parsed.host_str().map(str::to_ascii_lowercase) else { return false };
    mainland_hosts().contains(&host)
}

/// Whether a join's `forward` address is one the client may hand to WebRTC:
/// a `turn:` or `stun:` server, which is UDP or plain TCP. A `turns:` address
/// is TURN over TLS, and anything else is not a relay's at all; the media
/// relays the server forwards through include ones in mainland China, where
/// a TLS handshake is what takes the relay off the air, so neither is used.
/// The web passes the address through (room.ts) and needs the same check;
/// that change is the coordinator's to make on the web.
pub fn forwardable(url: &str) -> bool {
    let url = url.trim().to_ascii_lowercase();
    url.starts_with("turn:") || url.starts_with("stun:")
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

    // The rule the web shares, on readings no socket can be made to produce
    // exactly: the slow first round falls out, an even count takes the mean
    // of the middle two, and the order the rounds came in is not the order
    // they are ranked in.
    #[test]
    fn a_reading_is_the_median_of_the_rounds_that_answered() {
        let ms = Duration::from_millis;

        assert_eq!(reading(&[ms(180), ms(31), ms(30), ms(33), ms(29)]), Some(ms(31)));
        assert_eq!(reading(&[ms(40), ms(10), ms(30), ms(20)]), Some(ms(25)));
        assert_eq!(reading(&[ms(90), ms(12), ms(11)]), Some(ms(12)));
    }

    #[test]
    fn fewer_than_three_answers_are_no_reading() {
        let ms = Duration::from_millis;

        assert_eq!(QUORUM, 3);
        assert_eq!(reading(&[ms(10), ms(11)]), None);
        assert_eq!(reading(&[ms(10)]), None);
        assert_eq!(reading(&[]), None);
        assert!(reading(&[ms(10), ms(11), ms(12)]).is_some());
    }

    /// Runs `rounds` over answers given in order (`None` is a round that
    /// brought nothing), and says how many rounds were asked for.
    fn ran(answers: &[Option<u64>]) -> (Vec<Duration>, usize) {
        let asked = std::cell::Cell::new(0);
        let took = futures_util::FutureExt::now_or_never(rounds(|| {
            let at = asked.get();
            asked.set(at + 1);
            let answer = answers.get(at).copied().flatten().map(Duration::from_millis);
            async move { answer }
        }))
        .expect("rounds that never wait finish at once");

        (took, asked.get())
    }

    #[test]
    fn five_rounds_and_none_after_the_third_miss() {
        let ms = Duration::from_millis;

        // Five, and not a sixth however many more would answer.
        let every = [Some(5), Some(6), Some(7), Some(8), Some(9), Some(10), Some(11)];
        assert_eq!(ran(&every), (vec![ms(5), ms(6), ms(7), ms(8), ms(9)], ROUNDS));

        // A missed round is not the end: the next is taken, the first round
        // included, which is the one most likely to be lost.
        assert_eq!(ran(&[None, Some(6), Some(7), Some(8), Some(9)]), (vec![ms(6), ms(7), ms(8), ms(9)], ROUNDS));
        assert_eq!(ran(&[Some(5), None, Some(7), None, Some(9)]), (vec![ms(5), ms(7), ms(9)], ROUNDS));

        // At the third miss three answers are out of reach, and the relay is
        // not asked again.
        assert_eq!(ran(&[None, None, Some(7), None, Some(9)]), (vec![ms(7)], 4));
        assert_eq!(ran(&[None, None, None, Some(8), Some(9)]), (vec![], 3));
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
    fn a_forward_is_plain_turn_or_stun_and_nothing_else() {
        assert!(forwardable("turn:relay.example.com:3478?transport=udp"));
        assert!(forwardable("TURN:relay.example.com:3478"));
        assert!(forwardable(" stun:relay.example.com:3478"));
        assert!(!forwardable("turns:relay.example.com:5349?transport=tcp"));
        assert!(!forwardable("wss://relay.example.com"));
        assert!(!forwardable("https://relay.example.com"));
        assert!(!forwardable(""));
    }

    #[test]
    fn only_a_tls_address_of_a_known_mainland_relay_is_refused() {
        let at = |name: &str, url: &str, region: Option<&str>| Relay {
            url: Some(url.into()),
            region: region.map(str::to_owned),
            ..named(name)
        };
        remember_mainland(&[
            at("shanghai", "wss://sh.relays.example.net", Some("CN-East")),
            at("beijing", "ws://bj.relays.example.net:7880", Some("cn north")),
            at("hongkong", "wss://hk.relays.example.net", Some("HK")),
            Relay { region: Some("CN".into()), ..named("guangzhou") },
        ]);

        assert!(tls_into_mainland("wss://sh.relays.example.net"));
        assert!(tls_into_mainland("https://SH.relays.example.net/"));
        assert!(tls_into_mainland("wss://bj.relays.example.net:443"));
        // Plain WebSocket to the same machine is not TLS.
        assert!(!tls_into_mainland("ws://sh.relays.example.net"));
        // The relay a mainland relay signals through.
        assert!(!tls_into_mainland("wss://hk.relays.example.net"));
        // Nobody said where this one is.
        assert!(!tls_into_mainland("wss://elsewhere.example.net"));
        assert!(!tls_into_mainland("not a url"));
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
