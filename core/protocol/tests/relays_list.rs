//! Reading `GET /api/relays` and the regions (web/src/live/relays.test.ts,
//! `relays` and `mainland`), against a deployment started inside each test.
//!
//! Every way the list can fail to arrive (no endpoint, a body that is not the
//! list, nobody answering) has to end as nothing to choose, so that the join
//! proceeds exactly as it would on a deployment with a single relay.

use std::sync::{Arc, Mutex};

use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};
use tomoshibi_protocol::relays::{self, Relay, RelayList};

/// A deployment answering one request with `status` and `body`, keeping what
/// it was asked.
async fn deployment(status: &'static str, kind: &'static str, body: &'static str) -> (url::Url, Arc<Mutex<String>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let asked = Arc::new(Mutex::new(String::new()));

    let log = asked.clone();
    tokio::spawn(async move {
        let Ok((mut stream, _)) = listener.accept().await else { return };

        let mut head = Vec::new();
        let mut buffer = [0u8; 4096];
        while !head.windows(4).any(|w| w == b"\r\n\r\n") {
            match stream.read(&mut buffer).await {
                Ok(0) | Err(_) => return,
                Ok(read) => head.extend_from_slice(&buffer[..read]),
            }
        }
        *log.lock().unwrap() = String::from_utf8_lossy(&head).into_owned();

        let response = format!(
            "HTTP/1.1 {status}\r\ncontent-type: {kind}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
            body.len()
        );
        let _ = stream.write_all(response.as_bytes()).await;
    });

    // A base with a path of its own: the web's fetch is relative to the origin.
    (url::Url::parse(&format!("http://{address}/rooms/standup")).unwrap(), asked)
}

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

#[tokio::test]
async fn reports_what_the_control_node_offers() {
    // The shape internal/app/app.go writes, `omitempty` and all, with a field
    // this client has never heard of.
    let (base, asked) = deployment(
        "200 OK",
        "application/json",
        r#"{"relays":[
            {"name":"shanghai","region":"CN-East","label":"Shanghai","lat":31.23,"lon":121.47,"probe":"sh.example.com:3479"},
            {"name":"hongkong","url":"wss://hk.example.com","region":"HK","label":"Hong Kong","fallback":true,"maintenance":true},
            {"name":"lab","url":"wss://lab.example.com","adminOnly":true,"colour":"teal"}
        ],"measure":true,"fleet":{"online":2,"total":3},"since":"2026-10-01"}"#,
    )
    .await;

    let offered = relays::fetch(&reqwest::Client::new(), &base).await.unwrap();

    assert_eq!(
        offered,
        RelayList {
            relays: vec![
                Relay {
                    region: Some("CN-East".into()),
                    label: Some("Shanghai".into()),
                    lat: Some(31.23),
                    lon: Some(121.47),
                    probe: Some("sh.example.com:3479".into()),
                    ..named("shanghai")
                },
                Relay {
                    url: Some("wss://hk.example.com".into()),
                    region: Some("HK".into()),
                    label: Some("Hong Kong".into()),
                    fallback: true,
                    maintenance: true,
                    ..named("hongkong")
                },
                Relay { url: Some("wss://lab.example.com".into()), admin_only: true, ..named("lab") },
            ],
            measure: true,
            online: 2,
            total: 3,
        }
    );

    let asked = asked.lock().unwrap().clone();
    assert!(asked.starts_with("GET /api/relays HTTP/1.1\r\n"), "asked: {asked}");
    assert!(asked.to_ascii_lowercase().contains("\r\naccept: application/json\r\n"), "asked: {asked}");
}

#[tokio::test]
async fn treats_a_missing_endpoint_as_nothing_to_choose() {
    let (base, _) = deployment("404 Not Found", "text/plain", "not found").await;

    assert_eq!(relays::fetch(&reqwest::Client::new(), &base).await, Ok(RelayList::empty()));
}

#[tokio::test]
async fn treats_an_unreadable_body_as_nothing_to_choose() {
    let (base, _) = deployment("200 OK", "text/html", "<html>a proxy error page</html>").await;

    assert_eq!(relays::fetch(&reqwest::Client::new(), &base).await, Ok(RelayList::empty()));
}

#[tokio::test]
async fn treats_a_deployment_that_does_not_answer_as_nothing_to_choose() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    drop(listener);

    let base = url::Url::parse(&format!("http://{address}/")).unwrap();
    assert_eq!(relays::fetch(&reqwest::Client::new(), &base).await, Ok(RelayList::empty()));
}

// The web takes `measure` only when it is literally true and `relays` only when
// it is an array; here a relay is kept only with a name, since one without
// cannot be asked for, and a field of the wrong type is read as absent.
#[test]
fn reads_a_body_of_the_wrong_shape_leniently() {
    assert_eq!(relays::parse(br#"{"relays":{"name":"x"},"measure":"yes"}"#), RelayList::empty());
    assert_eq!(relays::parse(b"null"), RelayList::empty());
    assert_eq!(relays::parse(b"[]"), RelayList::empty());

    let read = relays::parse(
        br#"{"relays":[
            "shanghai",
            {"label":"no name"},
            {"name":7},
            {"name":"tokyo","url":5,"lat":"north","adminOnly":"true","maintenance":1}
        ],"measure":true,"fleet":{"online":"two","total":-1}}"#,
    );

    assert_eq!(read, RelayList { relays: vec![named("tokyo")], measure: true, online: 0, total: 0 });
}

// Which regions are mainland China, which decides whether a viewer is sent
// straight to the relay for YouTube. Wrong one way, a viewer in Shanghai waits
// for a player that never comes; wrong the other, somebody abroad is streamed
// every video through the deployment for nothing.
#[test]
fn mainland_is_the_deployments_cn_regions_however_they_are_typed() {
    for region in ["CN-East", "cn-south", "CN-Southwest", "CN", " CN-North ", "CN/Shanghai", "cn east", "\u{FEFF}CN"] {
        assert!(relays::mainland(Some(region.into())), "{region:?}");
    }
}

#[test]
fn mainland_is_nothing_else() {
    assert!(!relays::mainland(None));

    for region in ["", "HK", "JP-Tokyo", "Oversea/Asia", "CNX", "SG-CN", "C", "cn_east", "\u{0085}CN"] {
        assert!(!relays::mainland(Some(region.into())), "{region:?}");
    }
}
