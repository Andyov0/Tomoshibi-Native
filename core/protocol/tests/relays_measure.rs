//! Measuring the relays (web/src/live/relays.test.ts, `fastest` and
//! `preferred`), against relays started inside each test.
//!
//! What these guard: measuring is an optimisation, and an optimisation that can
//! stop a call from happening is worse than none. Every failure has to end in a
//! join without a preference. And the measurement itself must not be the thing
//! that gets a relay blocked: a relay in mainland China is taken off the air
//! when it is seen answering web requests, so all a relay may ever see from
//! this is a STUN binding request a round, or a WebSocket opening to /rtc a
//! round, and never more than five rounds.
//!
//! The choice is cached for the whole process, so the tests take turns.

use std::{
    net::{IpAddr, SocketAddr},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, UdpSocket},
};
use tomoshibi_protocol::relays::{self, Relay, RelayList};

static TURN: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn ms(millis: u64) -> Duration {
    Duration::from_millis(millis)
}

fn relay(name: &str) -> Relay {
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

fn at(name: &str, socket: &Socket) -> Relay {
    Relay { url: Some(socket.url()), ..relay(name) }
}

/// How a pretend relay's signalling socket answers.
#[derive(Clone, Copy)]
enum Answer {
    /// Refuses the missing token after this long, as a relay does.
    Refuse(Duration),
    /// Accepts the upgrade after this long.
    Accept(Duration),
    /// Redirects somewhere else after this long.
    Redirect(Duration),
    /// Takes the connection and never says anything.
    Never,
    /// Refuses the first this many connections after the delay, as a relay
    /// does, and says nothing to any after them.
    RefuseFirst(usize, Duration),
}

/// A relay's signalling port: every byte each connection sent, in order.
struct Socket {
    address: SocketAddr,
    connections: Arc<Mutex<Vec<Vec<u8>>>>,
}

impl Socket {
    async fn start(answer: Answer) -> Socket {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let connections = Arc::new(Mutex::new(Vec::<Vec<u8>>::new()));

        let log = connections.clone();
        tokio::spawn(async move {
            while let Ok((mut stream, _)) = listener.accept().await {
                let at = {
                    let mut log = log.lock().unwrap();
                    log.push(Vec::new());
                    log.len() - 1
                };

                let log = log.clone();
                tokio::spawn(async move {
                    let mut buffer = [0u8; 4096];
                    let mut answered = false;

                    loop {
                        let read = match stream.read(&mut buffer).await {
                            Ok(0) | Err(_) => break,
                            Ok(read) => read,
                        };

                        let head_complete = {
                            let mut log = log.lock().unwrap();
                            log[at].extend_from_slice(&buffer[..read]);
                            log[at].windows(4).any(|w| w == b"\r\n\r\n")
                        };

                        if answered || !head_complete {
                            continue;
                        }
                        answered = true;

                        let (delay, response): (Duration, &[u8]) = match answer {
                            Answer::Never => continue,
                            Answer::RefuseFirst(first, _) if at >= first => continue,
                            Answer::Refuse(delay) | Answer::RefuseFirst(_, delay) => (
                                delay,
                                b"HTTP/1.1 401 Unauthorized\r\ncontent-type: text/plain\r\ncontent-length: 13\r\nconnection: close\r\n\r\nmissing token",
                            ),
                            Answer::Accept(delay) => (
                                delay,
                                b"HTTP/1.1 101 Switching Protocols\r\nupgrade: websocket\r\nconnection: Upgrade\r\nsec-websocket-accept: s3pPLMBiTxaQ9kYGzzhZRbK+xOo=\r\n\r\n",
                            ),
                            Answer::Redirect(delay) => (
                                delay,
                                b"HTTP/1.1 302 Found\r\nlocation: /elsewhere\r\ncontent-length: 0\r\n\r\n",
                            ),
                        };

                        tokio::time::sleep(delay).await;
                        if stream.write_all(response).await.is_err() {
                            break;
                        }
                    }
                });
            }
        });

        Socket { address, connections }
    }

    fn url(&self) -> String {
        format!("ws://{}", self.address)
    }

    fn connections(&self) -> Vec<Vec<u8>> {
        self.connections.lock().unwrap().clone()
    }

    fn count(&self) -> usize {
        self.connections.lock().unwrap().len()
    }
}

/// A port nothing listens on, which refuses at once.
async fn closed() -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    listener.local_addr().unwrap()
}

/// A relay's STUN probe, built as internal/rtc/stun.go builds it: every
/// datagram it was sent and when, and an answer to binding requests after
/// `delay`, skipping the first `ignore` datagrams. `None` never answers.
struct Stun {
    address: SocketAddr,
    datagrams: Arc<Mutex<Vec<(Instant, Vec<u8>)>>>,
}

impl Stun {
    async fn start(delay: Option<Duration>, ignore: usize) -> Stun {
        Stun::answering(move |count, _| if count <= ignore { None } else { delay }).await
    }

    /// One that answers the `count`th datagram it is sent (from 1), which
    /// belongs to the `round`th transaction it has seen (from 1; a resent
    /// request is the same round), after what `answer` says, or not at all.
    async fn answering(answer: impl Fn(usize, usize) -> Option<Duration> + Send + 'static) -> Stun {
        let socket = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
        let address = socket.local_addr().unwrap();
        let datagrams = Arc::new(Mutex::new(Vec::<(Instant, Vec<u8>)>::new()));

        let log = datagrams.clone();
        tokio::spawn(async move {
            let mut buffer = [0u8; 1500];
            let mut rounds: Vec<Vec<u8>> = Vec::new();
            while let Ok((read, from)) = socket.recv_from(&mut buffer).await {
                let packet = buffer[..read].to_vec();
                let count = {
                    let mut log = log.lock().unwrap();
                    log.push((Instant::now(), packet.clone()));
                    log.len()
                };
                let id = packet.get(8..20).map(<[u8]>::to_vec).unwrap_or_default();
                let round = match rounds.iter().position(|seen| *seen == id) {
                    Some(at) => at + 1,
                    None => {
                        rounds.push(id);
                        rounds.len()
                    }
                };

                let Some(delay) = answer(count, round) else { continue };
                let Some(reply) = binding(&packet, from) else { continue };

                let socket = socket.clone();
                tokio::spawn(async move {
                    tokio::time::sleep(delay).await;
                    let _ = socket.send_to(&reply, from).await;
                });
            }
        });

        Stun { address, datagrams }
    }

    fn probe(&self) -> String {
        self.address.to_string()
    }

    fn datagrams(&self) -> Vec<Vec<u8>> {
        self.datagrams.lock().unwrap().iter().map(|(_, datagram)| datagram.clone()).collect()
    }

    fn arrivals(&self) -> Vec<Instant> {
        self.datagrams.lock().unwrap().iter().map(|(at, _)| *at).collect()
    }

    /// The transactions it was asked, each once, in the order they first
    /// arrived: one per round, however often a round's request was resent.
    fn transactions(&self) -> Vec<[u8; 12]> {
        let mut seen: Vec<[u8; 12]> = Vec::new();
        for datagram in self.datagrams() {
            let id: [u8; 12] = datagram[8..20].try_into().unwrap();
            if !seen.contains(&id) {
                seen.push(id);
            }
        }
        seen
    }
}

/// The relay's answer to a binding request (internal/rtc/stun.go, `binding`).
fn binding(packet: &[u8], from: SocketAddr) -> Option<Vec<u8>> {
    const COOKIE: [u8; 4] = [0x21, 0x12, 0xA4, 0x42];

    if packet.len() < 20
        || packet[0..2] != [0x00, 0x01]
        || packet[4..8] != COOKIE
        || usize::from(u16::from_be_bytes([packet[2], packet[3]])) != packet.len() - 20
    {
        return None;
    }

    let (family, address) = match from.ip() {
        IpAddr::V4(ip) => (0x01u8, ip.octets().to_vec()),
        IpAddr::V6(ip) => (0x02u8, ip.octets().to_vec()),
    };

    let mut value = vec![0, family];
    value.extend_from_slice(&(from.port() ^ 0x2112).to_be_bytes());
    for (i, byte) in address.iter().enumerate() {
        value.push(byte ^ if i < 4 { COOKIE[i] } else { packet[8 + (i - 4)] });
    }

    let mut reply = Vec::new();
    reply.extend_from_slice(&[0x01, 0x01]);
    reply.extend_from_slice(&((4 + value.len()) as u16).to_be_bytes());
    reply.extend_from_slice(&COOKIE);
    reply.extend_from_slice(&packet[8..20]);
    reply.extend_from_slice(&[0x00, 0x20]);
    reply.extend_from_slice(&(value.len() as u16).to_be_bytes());
    reply.extend_from_slice(&value);
    Some(reply)
}

fn list(relays: Vec<Relay>, measure: bool) -> RelayList {
    RelayList { relays, measure, online: 0, total: 0 }
}

/// Lets a relay's side of each connection finish reading what was sent.
async fn settle() {
    tokio::time::sleep(ms(150)).await;
}

/// The request line and headers of one connection, and whatever followed.
fn request(bytes: &[u8]) -> (String, Vec<(String, String)>, Vec<u8>) {
    let end = bytes.windows(4).position(|w| w == b"\r\n\r\n").expect("a complete request head") + 4;
    let head = std::str::from_utf8(&bytes[..end]).unwrap();
    let mut lines = head.split("\r\n").filter(|line| !line.is_empty());

    let line = lines.next().unwrap().to_owned();
    let headers = lines
        .map(|line| {
            let (name, value) = line.split_once(':').unwrap();
            (name.trim().to_ascii_lowercase(), value.trim().to_owned())
        })
        .collect();

    (line, headers, bytes[end..].to_vec())
}

fn header<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    headers.iter().find(|(key, _)| key == name).map(|(_, value)| value.as_str())
}

#[tokio::test]
async fn names_the_relay_that_answered_soonest() {
    let _turn = TURN.lock().await;
    relays::forget();

    let sh = Socket::start(Answer::Refuse(ms(150))).await;
    let hk = Socket::start(Answer::Refuse(ms(5))).await;
    let jp = Socket::start(Answer::Refuse(ms(250))).await;

    let offered = [at("shanghai", &sh), at("hongkong", &hk), at("tokyo", &jp)];
    assert_eq!(relays::fastest(&offered).await.as_deref(), Some("hongkong"));
}

#[tokio::test]
async fn skips_a_relay_that_does_not_answer() {
    let _turn = TURN.lock().await;
    relays::forget();

    // Shanghai is down; it would have been nearest had it been up.
    let sh = Socket::start(Answer::Never).await;
    let hk = Socket::start(Answer::Refuse(ms(30))).await;
    let jp = Socket::start(Answer::Refuse(ms(200))).await;

    let offered = [at("shanghai", &sh), at("hongkong", &hk), at("tokyo", &jp)];
    assert_eq!(relays::fastest(&offered).await.as_deref(), Some("hongkong"));
}

#[tokio::test]
async fn names_nothing_when_no_relay_answers_and_gives_up_at_the_timeout() {
    let _turn = TURN.lock().await;
    relays::forget();

    let sh = Socket::start(Answer::Never).await;
    let hk = Socket::start(Answer::Never).await;

    let started = Instant::now();
    assert_eq!(relays::fastest(&[at("shanghai", &sh), at("hongkong", &hk)]).await, None);
    let took = started.elapsed();

    // All at once and each bounded: three rounds of a second and a half for
    // both together, not twice that, and not five rounds: at the third miss
    // three answers are out of reach (relays.ts, `inRounds`).
    assert!(took >= ms(4450), "gave up after {took:?}");
    assert!(took < ms(5200), "waited {took:?}");

    // Both were tried, three times each.
    assert_eq!((sh.count(), hk.count()), (3, 3));
}

#[tokio::test]
async fn does_not_measure_when_there_is_no_choice() {
    let _turn = TURN.lock().await;
    relays::forget();

    let sh = Socket::start(Answer::Refuse(ms(5))).await;

    assert_eq!(relays::fastest(&[at("shanghai", &sh)]).await.as_deref(), Some("shanghai"));
    relays::forget();
    assert_eq!(relays::fastest(&[]).await, None);

    // Not one socket opened. Measuring a list of one costs a round trip and
    // cannot change the answer.
    settle().await;
    assert_eq!(sh.count(), 0);
}

#[tokio::test]
async fn reuses_a_recent_measurement_rather_than_taking_it_again() {
    let _turn = TURN.lock().await;
    relays::forget();

    let sh = Socket::start(Answer::Refuse(ms(150))).await;
    let hk = Socket::start(Answer::Refuse(ms(5))).await;
    let offered = [at("shanghai", &sh), at("hongkong", &hk)];

    assert_eq!(relays::fastest(&offered).await.as_deref(), Some("hongkong"));
    settle().await;
    let before = (sh.count(), hk.count());

    assert_eq!(relays::fastest(&offered).await.as_deref(), Some("hongkong"));
    settle().await;
    assert_eq!((sh.count(), hk.count()), before);
}

// A cached name that is no longer offered must not be sent: the deployment has
// changed its relays, and the server would ignore the old name, silently
// costing this client the measurement it thought it had.
#[tokio::test]
async fn measures_again_when_the_cached_relay_is_gone_from_the_list() {
    let _turn = TURN.lock().await;
    relays::forget();

    let sh = Socket::start(Answer::Refuse(ms(150))).await;
    let hk = Socket::start(Answer::Refuse(ms(5))).await;
    assert_eq!(relays::fastest(&[at("shanghai", &sh), at("hongkong", &hk)]).await.as_deref(), Some("hongkong"));

    let jp = Socket::start(Answer::Refuse(ms(5))).await;
    assert_eq!(relays::fastest(&[at("shanghai", &sh), at("tokyo", &jp)]).await.as_deref(), Some("tokyo"));
}

// Called when a call drops: the held reading names as fastest the machine that
// just stopped carrying a conversation.
#[tokio::test]
async fn forgetting_measures_again() {
    let _turn = TURN.lock().await;
    relays::forget();

    let sh = Socket::start(Answer::Refuse(ms(150))).await;
    let hk = Socket::start(Answer::Refuse(ms(5))).await;
    let offered = [at("shanghai", &sh), at("hongkong", &hk)];

    relays::fastest(&offered).await;
    relays::forget_relay_measurement();
    relays::fastest(&offered).await;

    settle().await;
    assert_eq!((sh.count(), hk.count()), (2 * relays::ROUNDS, 2 * relays::ROUNDS));
}

// Automatic is for somebody who did not choose, and choosing a relay they will
// be refused at the door is worse than choosing nothing.
#[tokio::test]
async fn never_measures_or_chooses_a_reserved_or_resting_relay() {
    let _turn = TURN.lock().await;
    relays::forget();

    let reserved = Socket::start(Answer::Refuse(ms(1))).await;
    let resting = Socket::start(Answer::Refuse(ms(1))).await;
    let hk = Socket::start(Answer::Refuse(ms(150))).await;
    let jp = Socket::start(Answer::Refuse(ms(250))).await;

    let offered = [
        Relay { admin_only: true, ..at("reserved", &reserved) },
        Relay { maintenance: true, ..at("resting", &resting) },
        at("hongkong", &hk),
        at("tokyo", &jp),
    ];
    assert_eq!(relays::fastest(&offered).await.as_deref(), Some("hongkong"));

    // And one usable relay is no choice at all, so nothing is measured.
    relays::forget();
    let lone = [
        Relay { admin_only: true, ..at("reserved", &reserved) },
        Relay { maintenance: true, ..at("resting", &resting) },
        at("tokyo", &jp),
    ];
    assert_eq!(relays::fastest(&lone).await.as_deref(), Some("tokyo"));

    settle().await;
    assert_eq!((reserved.count(), resting.count(), jp.count()), (0, 0, relays::ROUNDS));
}

// The one request a relay may see: the WebSocket opening a call begins with,
// with no token, closed as soon as it is answered.
#[tokio::test]
async fn the_websocket_opening_is_the_only_request_a_relay_sees() {
    let _turn = TURN.lock().await;
    relays::forget();

    let sh = Socket::start(Answer::Refuse(ms(5))).await;
    let hk = Socket::start(Answer::Refuse(ms(150))).await;

    // Trailing slashes are dropped and a path is kept, as the web joins them.
    let offered = [
        Relay { url: Some(format!("ws://{}/relay///", sh.address)), ..relay("shanghai") },
        Relay { url: Some(format!("http://{}", hk.address)), ..relay("hongkong") },
    ];
    assert_eq!(relays::fastest(&offered).await.as_deref(), Some("shanghai"));
    settle().await;

    for (socket, path) in [(&sh, "/relay/rtc"), (&hk, "/rtc")] {
        // A connection of its own for every round, as every round of the
        // web's is a new WebSocket, and no more than the rounds.
        let connections = socket.connections();
        assert_eq!(connections.len(), relays::ROUNDS, "one connection per round");

        let mut keys = Vec::new();
        for connection in &connections {
            let (line, headers, after) = request(connection);
            assert_eq!(line, format!("GET {path} HTTP/1.1"));
            assert_eq!(header(&headers, "upgrade").map(str::to_ascii_lowercase).as_deref(), Some("websocket"));
            assert_eq!(header(&headers, "connection").map(str::to_ascii_lowercase).as_deref(), Some("upgrade"));
            assert_eq!(header(&headers, "sec-websocket-version"), Some("13"));
            assert_eq!(header(&headers, "sec-websocket-key").map(str::len), Some(24));
            keys.push(header(&headers, "sec-websocket-key").unwrap().to_owned());

            // No token, no credentials, nothing of the deployment's.
            for absent in ["authorization", "cookie", "proxy-authorization"] {
                assert_eq!(header(&headers, absent), None, "{absent} sent to a relay");
            }

            // And nothing after it: not a second request on the same connection.
            assert!(after.is_empty(), "sent after the upgrade request: {after:?}");
        }

        keys.sort();
        keys.dedup();
        assert_eq!(keys.len(), relays::ROUNDS, "a handshake key reused");
    }
}

// A relay that answers with a redirect has answered. Following it would be a
// second, ordinary web request to the relay, which is what gets one blocked.
#[tokio::test]
async fn a_redirect_is_an_answer_and_is_not_followed() {
    let _turn = TURN.lock().await;
    relays::forget();

    let sh = Socket::start(Answer::Redirect(ms(5))).await;
    let hk = Socket::start(Answer::Refuse(ms(150))).await;

    assert_eq!(relays::fastest(&[at("shanghai", &sh), at("hongkong", &hk)]).await.as_deref(), Some("shanghai"));
    settle().await;

    // One connection a round, each the upgrade and nothing after it: a
    // followed redirect would be a sixth, or a second request on one.
    let connections = sh.connections();
    assert_eq!(connections.len(), relays::ROUNDS, "the redirect was followed");
    for connection in &connections {
        let (line, _, after) = request(connection);
        assert_eq!(line, "GET /rtc HTTP/1.1");
        assert!(after.is_empty(), "sent after the upgrade request: {after:?}");
    }
}

// Should a relay ever accept the upgrade, the socket is closed the way a
// browser's `socket.close()` closes it, and not left holding a connection on a
// relay this client may not use.
#[tokio::test]
async fn an_accepted_socket_is_closed_with_a_close_frame() {
    let _turn = TURN.lock().await;
    relays::forget();

    let sh = Socket::start(Answer::Accept(ms(5))).await;
    let hk = Socket::start(Answer::Refuse(ms(150))).await;

    assert_eq!(relays::fastest(&[at("shanghai", &sh), at("hongkong", &hk)]).await.as_deref(), Some("shanghai"));
    settle().await;

    let connections = sh.connections();
    assert_eq!(connections.len(), relays::ROUNDS);
    for connection in &connections {
        let (_, _, after) = request(connection);

        // A masked close frame with no payload, and nothing else.
        assert_eq!(after.len(), 6, "after the upgrade: {after:?}");
        assert_eq!(&after[..2], &[0x88, 0x80]);
    }
}

// relays.ts counts `error` as an answer alongside `open` and `close`, so a
// relay whose port refuses the connection outright has answered, and quickly.
// Pinned here because it is the web's rule, and a native client that ranked
// differently would land somewhere a browser on the same network would not.
#[tokio::test]
async fn a_refused_connection_counts_as_an_answer_as_it_does_in_a_browser() {
    let _turn = TURN.lock().await;
    relays::forget();

    let refusing = closed().await;
    let hk = Socket::start(Answer::Refuse(ms(300))).await;

    let offered = [Relay { url: Some(format!("ws://{refusing}")), ..relay("shanghai") }, at("hongkong", &hk)];
    assert_eq!(relays::fastest(&offered).await.as_deref(), Some("shanghai"));
}

#[tokio::test]
async fn a_relay_with_no_address_has_no_reading() {
    let _turn = TURN.lock().await;
    relays::forget();

    let hk = Socket::start(Answer::Refuse(ms(150))).await;
    let jp = Socket::start(Answer::Refuse(ms(250))).await;

    // The server withholds the address of a relay with a probe from anybody
    // but an administrator; without a probe answering, it cannot be timed.
    let offered = [relay("shanghai"), at("hongkong", &hk), at("tokyo", &jp)];
    assert_eq!(relays::fastest(&offered).await.as_deref(), Some("hongkong"));
}

#[tokio::test]
async fn times_round_trips_over_udp_when_every_relay_has_a_probe() {
    let _turn = TURN.lock().await;
    relays::forget();

    // Over the socket Shanghai would win; over UDP Hong Kong does, and UDP is
    // what is asked for when every relay offers it.
    let sh_stun = Stun::start(Some(ms(150)), 0).await;
    let hk_stun = Stun::start(Some(ms(5)), 0).await;
    let sh = Socket::start(Answer::Refuse(ms(5))).await;
    let hk = Socket::start(Answer::Refuse(ms(150))).await;

    let offered = [
        Relay { probe: Some(sh_stun.probe()), ..at("shanghai", &sh) },
        Relay { probe: Some(hk_stun.probe()), ..at("hongkong", &hk) },
    ];
    assert_eq!(relays::fastest(&offered).await.as_deref(), Some("hongkong"));
    settle().await;

    // No socket was opened to either.
    assert_eq!((sh.count(), hk.count()), (0, 0));

    // And each probe was sent one binding request a round, with nothing
    // attached to it, every one a transaction of its own.
    for stun in [&sh_stun, &hk_stun] {
        let datagrams = stun.datagrams();
        assert_eq!(datagrams.len(), relays::ROUNDS, "datagrams: {datagrams:?}");

        for request in &datagrams {
            assert_eq!(request.len(), 20, "a binding request with attributes: {request:?}");
            assert_eq!(&request[0..2], &[0x00, 0x01], "not a binding request");
            assert_eq!(&request[2..4], &[0x00, 0x00]);
            assert_eq!(&request[4..8], &[0x21, 0x12, 0xA4, 0x42]);
        }
    }

    let mut ids = [sh_stun.transactions(), hk_stun.transactions()].concat();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), 2 * relays::ROUNDS, "a transaction id reused");
}

// The owner's complaint (2026-10-19): the first UDP reading is always the
// slow one, and a single reading ranked relays by which one happened to pay
// for it. The relay here is the nearer one on four rounds of five.
#[tokio::test]
async fn a_slow_first_round_does_not_decide_the_reading() {
    let _turn = TURN.lock().await;
    relays::forget();

    let sh_stun = Stun::answering(|count, _| Some(if count == 1 { ms(200) } else { ms(5) })).await;
    let hk_stun = Stun::start(Some(ms(40)), 0).await;

    let offered = [
        Relay { probe: Some(sh_stun.probe()), ..relay("shanghai") },
        Relay { probe: Some(hk_stun.probe()), ..relay("hongkong") },
    ];
    let read = relays::timings(&offered).await;
    let (sh, hk) = (read[0].millis.unwrap(), read[1].millis.unwrap());
    assert!(sh < 30.0, "shanghai read {sh} ms from rounds of 200, 5, 5, 5, 5");
    assert!((40.0..100.0).contains(&hk), "hongkong read {hk} ms");

    assert_eq!(relays::fastest(&offered).await.as_deref(), Some("shanghai"));
}

// Nor does one lucky round: a relay that answered quickly once and slowly
// four times is the slow one.
#[tokio::test]
async fn a_quick_round_does_not_decide_the_reading_either() {
    let _turn = TURN.lock().await;
    relays::forget();

    let sh_stun = Stun::answering(|count, _| Some(if count == 1 { ms(5) } else { ms(90) })).await;
    let hk_stun = Stun::start(Some(ms(40)), 0).await;

    let offered = [
        Relay { probe: Some(sh_stun.probe()), ..relay("shanghai") },
        Relay { probe: Some(hk_stun.probe()), ..relay("hongkong") },
    ];
    assert_eq!(relays::fastest(&offered).await.as_deref(), Some("hongkong"));
}

// In turn for one relay, so a round does not share the slow start of the
// one before it, and every relay at once, so the wait is one relay's rounds
// and not everybody's added up.
#[tokio::test]
async fn rounds_go_one_after_another_and_relays_all_at_once() {
    let _turn = TURN.lock().await;
    relays::forget();

    let sh_stun = Stun::start(Some(ms(80)), 0).await;
    let hk_stun = Stun::start(Some(ms(80)), 0).await;
    let offered = [
        Relay { probe: Some(sh_stun.probe()), ..relay("shanghai") },
        Relay { probe: Some(hk_stun.probe()), ..relay("hongkong") },
    ];

    let started = Instant::now();
    relays::timings(&offered).await;
    let took = started.elapsed();

    for stun in [&sh_stun, &hk_stun] {
        let arrivals = stun.arrivals();
        assert_eq!(arrivals.len(), relays::ROUNDS);
        // Each request only once the one before it was answered.
        for pair in arrivals.windows(2) {
            let apart = pair[1] - pair[0];
            assert!(apart >= ms(75), "a round asked {apart:?} after the one before");
        }
    }

    // Five rounds of 80 ms, for both relays together.
    assert!(took >= ms(390), "five rounds in {took:?}");
    assert!(took < ms(700), "the relays waited for each other: {took:?}");
}

// Over UDP a relay has to answer three of its rounds to have a reading, and
// one that does not sends the whole list to the socket. Two answers here
// would have won over UDP; over the socket the other relay does.
#[tokio::test]
async fn a_relay_answering_fewer_than_three_rounds_over_udp_sends_everybody_over_the_socket() {
    let _turn = TURN.lock().await;
    relays::forget();

    let sh_stun = Stun::answering(|_, round| (round <= 2).then_some(ms(5))).await;
    let hk_stun = Stun::start(Some(ms(50)), 0).await;
    let sh = Socket::start(Answer::Refuse(ms(150))).await;
    let hk = Socket::start(Answer::Refuse(ms(5))).await;

    let offered = [
        Relay { probe: Some(sh_stun.probe()), ..at("shanghai", &sh) },
        Relay { probe: Some(hk_stun.probe()), ..at("hongkong", &hk) },
    ];
    assert_eq!(relays::fastest(&offered).await.as_deref(), Some("hongkong"));
    settle().await;
    assert_eq!((sh.count(), hk.count()), (relays::ROUNDS, relays::ROUNDS));

    // Two rounds answered and three that were not, the third of them its last.
    assert_eq!(sh_stun.transactions().len(), 5);
}

#[tokio::test]
async fn three_rounds_answered_over_udp_are_a_reading() {
    let _turn = TURN.lock().await;
    relays::forget();

    let sh_stun = Stun::answering(|_, round| (round <= 3).then_some(ms(5))).await;
    let hk_stun = Stun::start(Some(ms(50)), 0).await;
    let sh = Socket::start(Answer::Refuse(ms(150))).await;
    let hk = Socket::start(Answer::Refuse(ms(5))).await;

    let offered = [
        Relay { probe: Some(sh_stun.probe()), ..at("shanghai", &sh) },
        Relay { probe: Some(hk_stun.probe()), ..at("hongkong", &hk) },
    ];
    assert_eq!(relays::fastest(&offered).await.as_deref(), Some("shanghai"));
    settle().await;

    // Still over UDP: no socket to either, and all five rounds asked, two
    // misses being one short of giving up.
    assert_eq!((sh.count(), hk.count()), (0, 0));
    assert_eq!(sh_stun.transactions().len(), relays::ROUNDS);
    assert_eq!(hk_stun.transactions().len(), relays::ROUNDS);
}

// The round most likely to go missing is the first, and missing it is not
// being down: the relay that answers the other four keeps its reading, and
// with it the list keeps to UDP.
#[tokio::test]
async fn a_missed_first_round_is_not_the_end_of_a_relay() {
    let _turn = TURN.lock().await;
    relays::forget();

    let sh_stun = Stun::answering(|_, round| (round > 1).then_some(ms(5))).await;
    let hk_stun = Stun::start(Some(ms(40)), 0).await;
    let sh = Socket::start(Answer::Refuse(ms(150))).await;
    let hk = Socket::start(Answer::Refuse(ms(5))).await;

    let offered = [
        Relay { probe: Some(sh_stun.probe()), ..at("shanghai", &sh) },
        Relay { probe: Some(hk_stun.probe()), ..at("hongkong", &hk) },
    ];
    assert_eq!(relays::fastest(&offered).await.as_deref(), Some("shanghai"));
    settle().await;
    assert_eq!((sh.count(), hk.count()), (0, 0));
    assert_eq!(sh_stun.transactions().len(), relays::ROUNDS);
}

// Over the socket as over UDP, a relay must answer three of its five: one
// that answered twice, however quickly, has no reading and is not chosen.
#[tokio::test]
async fn over_the_socket_two_answers_are_no_reading_either() {
    let _turn = TURN.lock().await;
    relays::forget();

    let sh = Socket::start(Answer::RefuseFirst(2, ms(5))).await;
    let hk = Socket::start(Answer::Refuse(ms(150))).await;

    let read = relays::timings(&[at("shanghai", &sh), at("hongkong", &hk)]).await;
    assert_eq!(read[0].millis, None, "two answers of five were a reading");
    assert!(read[1].millis.is_some());

    // Asked five times: two answers, and the third miss its last round.
    settle().await;
    assert_eq!(sh.count(), relays::ROUNDS);
}

// The two kinds of reading are not the same number: one round trip over UDP,
// about three over the socket. Ranking them together hands the call to
// whichever relays happen to have a probe configured.
#[tokio::test]
async fn does_not_rank_a_udp_reading_against_a_socket_one() {
    let _turn = TURN.lock().await;
    relays::forget();

    let sh = Socket::start(Answer::Refuse(ms(120))).await;
    let hk_stun = Stun::start(Some(ms(5)), 0).await;
    let hk = Socket::start(Answer::Refuse(ms(300))).await;

    let offered = [at("shanghai", &sh), Relay { probe: Some(hk_stun.probe()), ..at("hongkong", &hk) }];
    assert_eq!(relays::fastest(&offered).await.as_deref(), Some("shanghai"));

    // Nothing was sent over UDP for a list that cannot all be measured so.
    settle().await;
    assert!(hk_stun.datagrams().is_empty());
}

// One probe that does not answer sends everybody the slower way, so the
// numbers stay comparable; a relay advertising a broken probe port is worth
// finding out about, and this is how it shows.
#[tokio::test]
async fn measures_everybody_over_the_socket_when_one_probe_is_silent() {
    let _turn = TURN.lock().await;
    relays::forget();

    let sh_stun = Stun::start(None, 0).await;
    let hk_stun = Stun::start(Some(ms(5)), 0).await;
    let sh = Socket::start(Answer::Refuse(ms(5))).await;
    let hk = Socket::start(Answer::Refuse(ms(150))).await;

    let offered = [
        Relay { probe: Some(sh_stun.probe()), ..at("shanghai", &sh) },
        Relay { probe: Some(hk_stun.probe()), ..at("hongkong", &hk) },
    ];
    let started = Instant::now();
    assert_eq!(relays::fastest(&offered).await.as_deref(), Some("shanghai"));
    let took = started.elapsed();
    settle().await;
    assert_eq!((sh.count(), hk.count()), (relays::ROUNDS, relays::ROUNDS));

    // The silent probe was asked three times, each round's request sent
    // again while it was given time, and only ever unchanged.
    let asked = sh_stun.datagrams();
    assert_eq!(sh_stun.transactions().len(), 3, "rounds asked: {asked:?}");
    assert!(asked.len() >= 6, "never retransmitted: {asked:?}");
    for id in sh_stun.transactions() {
        let sent: Vec<&Vec<u8>> = asked.iter().filter(|datagram| datagram[8..20] == id).collect();
        assert!(sent.len() >= 2 && sent.iter().all(|datagram| *datagram == sent[0]), "a round's request changed: {sent:?}");
    }

    // Three timeouts for the silent probe, which stopped the other's UDP
    // rounds too, and then five quick rounds over the socket: not five
    // timeouts, and not the UDP rounds of everybody else waited out.
    assert!(took >= ms(4450), "fell back after {took:?}");
    assert!(took < ms(6000), "waited {took:?}");
}

// Once one relay is out over UDP the list is going to the socket whatever the
// rest answer, so their remaining UDP rounds are not waited for (relays.ts,
// `inRounds` with `together`): here a slow relay's fifth round would have
// ended at six seconds.
#[tokio::test]
async fn the_first_relay_out_over_udp_stops_the_others_rounds() {
    let _turn = TURN.lock().await;
    relays::forget();

    let sh_stun = Stun::start(None, 0).await;
    let hk_stun = Stun::start(Some(ms(1200)), 0).await;
    let sh = Socket::start(Answer::Refuse(ms(5))).await;
    let hk = Socket::start(Answer::Refuse(ms(5))).await;

    let offered = [
        Relay { probe: Some(sh_stun.probe()), ..at("shanghai", &sh) },
        Relay { probe: Some(hk_stun.probe()), ..at("hongkong", &hk) },
    ];
    let started = Instant::now();
    relays::timings(&offered).await;
    let took = started.elapsed();

    // Hong Kong was on its fourth round when Shanghai missed its third.
    assert_eq!(hk_stun.transactions().len(), 4);
    assert!(took < ms(5500), "waited {took:?}");
    settle().await;
    assert_eq!((sh.count(), hk.count()), (relays::ROUNDS, relays::ROUNDS));
}

// UDP loses packets. The browser's STUN client sends the same request again,
// and a native one has to, or one lost datagram ends a relay's rounds and,
// early enough, sends every relay the slow way. The round the loss slowed is
// then one of five, and the median leaves it out.
#[tokio::test]
async fn a_lost_binding_request_is_sent_again_unchanged() {
    let _turn = TURN.lock().await;
    relays::forget();

    let stun = Stun::start(Some(ms(1)), 1).await;
    let read = relays::timings(&[Relay { probe: Some(stun.probe()), ..relay("shanghai") }]).await;

    let millis = read[0].millis.expect("answered every round");
    assert!(millis < 100.0, "the round slowed by the loss decided the reading: {millis}");

    let datagrams = stun.datagrams();
    assert_eq!(datagrams.len(), relays::ROUNDS + 1, "datagrams: {datagrams:?}");
    assert_eq!(datagrams[0], datagrams[1], "a retransmission is the same request");
    assert_eq!(stun.transactions().len(), relays::ROUNDS);

    // Resent a retransmission interval after the first was sent.
    let arrivals = stun.arrivals();
    assert!(arrivals[1] - arrivals[0] >= ms(240), "resent after {:?}", arrivals[1] - arrivals[0]);
}

// A probe the browser could not make a `stun:` URL of is no reading, rather
// than a failure of the whole measurement.
#[tokio::test]
async fn a_probe_that_does_not_parse_is_no_reading() {
    let _turn = TURN.lock().await;

    let read = relays::timings(&[
        Relay { probe: Some("not a host:port:at all".into()), ..relay("a") },
        Relay { probe: Some("127.0.0.1:notaport".into()), ..relay("b") },
    ])
    .await;

    assert_eq!(read.iter().map(|one| one.millis).collect::<Vec<_>>(), vec![None, None]);
}

// A relay in mainland China signals through another because a TLS handshake
// to it gets it taken off the air; one listed without a probe (or whose
// probe is down) must still never see a socket from a client measuring the
// list. It goes without a reading, here and in the picker, and the rest are
// measured as before.
#[tokio::test]
async fn a_mainland_relay_is_never_opened_a_socket_to() {
    let _turn = TURN.lock().await;
    relays::forget();

    let sh = Socket::start(Answer::Refuse(ms(1))).await;
    let bj = Socket::start(Answer::Refuse(ms(1))).await;
    let hk = Socket::start(Answer::Refuse(ms(150))).await;

    let offered = [
        Relay { region: Some("CN-East".into()), ..at("shanghai", &sh) },
        Relay { region: Some(" cn/north".into()), ..at("beijing", &bj) },
        Relay { region: Some("HK".into()), ..at("hongkong", &hk) },
    ];
    assert_eq!(relays::fastest(&offered).await.as_deref(), Some("hongkong"));

    let read = relays::timings(&offered).await;
    assert_eq!(read.iter().map(|one| one.millis.is_some()).collect::<Vec<_>>(), [false, false, true]);

    settle().await;
    assert_eq!((sh.count(), bj.count()), (0, 0), "a mainland relay was opened a socket to");
    assert_eq!(hk.count(), 2 * relays::ROUNDS);
}

// Somebody choosing by hand is shown the numbers the automatic choice is made
// from, freshly: the picker is opened when the automatic answer disappointed.
#[tokio::test]
async fn timings_measure_every_relay_but_a_resting_one_and_are_never_cached() {
    let _turn = TURN.lock().await;
    relays::forget();

    let resting = Socket::start(Answer::Refuse(ms(1))).await;
    let reserved = Socket::start(Answer::Refuse(ms(5))).await;
    let jp = Socket::start(Answer::Refuse(ms(150))).await;

    let offered = vec![
        Relay { maintenance: true, ..at("resting", &resting) },
        Relay { admin_only: true, ..at("reserved", &reserved) },
        at("tokyo", &jp),
    ];

    let read = relays::relay_timings(offered.clone()).await;
    assert_eq!(read.iter().map(|one| one.name.as_str()).collect::<Vec<_>>(), ["resting", "reserved", "tokyo"]);
    assert_eq!(read[0].millis, None);
    assert!(read[1].millis.unwrap() < read[2].millis.unwrap());

    relays::timings(&offered).await;
    settle().await;
    assert_eq!((resting.count(), reserved.count(), jp.count()), (0, 2 * relays::ROUNDS, 2 * relays::ROUNDS));
}

#[tokio::test]
async fn preferred_is_nothing_when_the_deployment_does_not_want_a_measurement() {
    let _turn = TURN.lock().await;
    relays::forget();

    let sh = Socket::start(Answer::Refuse(ms(5))).await;
    let hk = Socket::start(Answer::Refuse(ms(5))).await;

    assert_eq!(relays::preferred(&list(vec![at("shanghai", &sh), at("hongkong", &hk)], false)).await, None);

    // Nothing measured after it said no.
    settle().await;
    assert_eq!((sh.count(), hk.count()), (0, 0));
}

#[tokio::test]
async fn preferred_is_nothing_when_nothing_answered() {
    let _turn = TURN.lock().await;
    relays::forget();

    let sh = Socket::start(Answer::Never).await;
    let hk = Socket::start(Answer::Never).await;

    assert_eq!(relays::preferred(&list(vec![at("shanghai", &sh), at("hongkong", &hk)], true)).await, None);
}

#[tokio::test]
async fn preferred_names_the_fastest_when_there_is_one() {
    let _turn = TURN.lock().await;
    relays::forget();

    let sh = Socket::start(Answer::Refuse(ms(150))).await;
    let hk = Socket::start(Answer::Refuse(ms(5))).await;

    assert_eq!(
        relays::preferred(&list(vec![at("shanghai", &sh), at("hongkong", &hk)], true)).await.as_deref(),
        Some("hongkong")
    );
}
