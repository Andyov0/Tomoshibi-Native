//! Measuring the relays (web/src/live/relays.test.ts, `fastest` and
//! `preferred`), against relays started inside each test.
//!
//! What these guard: measuring is an optimisation, and an optimisation that can
//! stop a call from happening is worse than none. Every failure has to end in a
//! join without a preference. And the measurement itself must not be the thing
//! that gets a relay blocked: a relay in mainland China is taken off the air
//! when it is seen answering web requests, so all a relay may ever see from
//! this is one STUN binding request, or one WebSocket opening to /rtc.
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
                            Answer::Refuse(delay) => (
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
/// datagram it was sent, and an answer to binding requests after `delay`,
/// skipping the first `ignore` of them. `None` never answers.
struct Stun {
    address: SocketAddr,
    datagrams: Arc<Mutex<Vec<Vec<u8>>>>,
}

impl Stun {
    async fn start(delay: Option<Duration>, ignore: usize) -> Stun {
        let socket = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
        let address = socket.local_addr().unwrap();
        let datagrams = Arc::new(Mutex::new(Vec::<Vec<u8>>::new()));

        let log = datagrams.clone();
        tokio::spawn(async move {
            let mut buffer = [0u8; 1500];
            while let Ok((read, from)) = socket.recv_from(&mut buffer).await {
                let packet = buffer[..read].to_vec();
                let count = {
                    let mut log = log.lock().unwrap();
                    log.push(packet.clone());
                    log.len()
                };

                let Some(delay) = delay else { continue };
                if count <= ignore {
                    continue;
                }
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
        self.datagrams.lock().unwrap().clone()
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

    // All at once and each bounded: one and a half seconds, not three, and
    // not however long a dead relay would keep a person waiting.
    assert!(took >= ms(1450), "gave up after {took:?}");
    assert!(took < ms(2200), "waited {took:?}");

    // Both were tried, once each.
    assert_eq!((sh.count(), hk.count()), (1, 1));
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
    assert_eq!((sh.count(), hk.count()), (2, 2));
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
    assert_eq!((reserved.count(), resting.count(), jp.count()), (0, 0, 1));
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
        let connections = socket.connections();
        assert_eq!(connections.len(), 1, "one connection per relay");

        let (line, headers, after) = request(&connections[0]);
        assert_eq!(line, format!("GET {path} HTTP/1.1"));
        assert_eq!(header(&headers, "upgrade").map(str::to_ascii_lowercase).as_deref(), Some("websocket"));
        assert_eq!(header(&headers, "connection").map(str::to_ascii_lowercase).as_deref(), Some("upgrade"));
        assert_eq!(header(&headers, "sec-websocket-version"), Some("13"));
        assert_eq!(header(&headers, "sec-websocket-key").map(str::len), Some(24));

        // No token, no credentials, nothing of the deployment's.
        for absent in ["authorization", "cookie", "proxy-authorization"] {
            assert_eq!(header(&headers, absent), None, "{absent} sent to a relay");
        }

        // And nothing after it: not a second request on the same connection.
        assert!(after.is_empty(), "sent after the upgrade request: {after:?}");
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

    let connections = sh.connections();
    assert_eq!(connections.len(), 1, "the redirect was followed");
    let (line, _, after) = request(&connections[0]);
    assert_eq!(line, "GET /rtc HTTP/1.1");
    assert!(after.is_empty(), "sent after the upgrade request: {after:?}");
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
    assert_eq!(connections.len(), 1);
    let (_, _, after) = request(&connections[0]);

    // A masked close frame with no payload, and nothing else.
    assert_eq!(after.len(), 6, "after the upgrade: {after:?}");
    assert_eq!(&after[..2], &[0x88, 0x80]);
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
async fn times_one_udp_round_trip_when_every_relay_has_a_probe() {
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

    // And each probe was sent one binding request, with nothing attached to it.
    for stun in [&sh_stun, &hk_stun] {
        let datagrams = stun.datagrams();
        assert_eq!(datagrams.len(), 1, "datagrams: {datagrams:?}");

        let request = &datagrams[0];
        assert_eq!(request.len(), 20, "a binding request with attributes: {request:?}");
        assert_eq!(&request[0..2], &[0x00, 0x01], "not a binding request");
        assert_eq!(&request[2..4], &[0x00, 0x00]);
        assert_eq!(&request[4..8], &[0x21, 0x12, 0xA4, 0x42]);
    }

    assert_ne!(sh_stun.datagrams()[0][8..20], hk_stun.datagrams()[0][8..20], "a transaction id reused");
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
    assert_eq!(relays::fastest(&offered).await.as_deref(), Some("shanghai"));
    settle().await;
    assert_eq!((sh.count(), hk.count()), (1, 1));

    // The silent probe was asked again while it was given time, and only ever
    // with the same request.
    let asked = sh_stun.datagrams();
    assert!(asked.len() >= 2, "never retransmitted: {asked:?}");
    assert!(asked.iter().all(|datagram| *datagram == asked[0]), "the request changed: {asked:?}");
}

// UDP loses packets. The browser's STUN client sends the same request again,
// and a native one has to, or one lost datagram sends every relay the slow way.
#[tokio::test]
async fn a_lost_binding_request_is_sent_again_unchanged() {
    let _turn = TURN.lock().await;
    relays::forget();

    let stun = Stun::start(Some(ms(1)), 1).await;
    let read = relays::timings(&[Relay { probe: Some(stun.probe()), ..relay("shanghai") }]).await;

    let millis = read[0].millis.expect("answered on the second request");
    assert!(millis >= 240.0, "answered before it was asked again: {millis}");

    let datagrams = stun.datagrams();
    assert_eq!(datagrams.len(), 2, "datagrams: {datagrams:?}");
    assert_eq!(datagrams[0], datagrams[1], "a retransmission is the same request");
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
    assert_eq!((resting.count(), reserved.count(), jp.count()), (0, 2, 2));
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
