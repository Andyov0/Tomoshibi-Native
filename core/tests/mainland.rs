//! A call is never opened over TLS to a relay in mainland China, whose
//! providers take a machine off the air for answering a TLS handshake: not
//! the signalling, when the join's address is such a relay's own, and not a
//! TURN forward over TLS. Each is refused before a byte is sent; a listener
//! standing in for the relay sees no connection at all.

use std::{
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};

use tokio::net::TcpListener;
use tomoshibi_core::call::{join_call, CallError, CallEvent, CallObserver, CallOptions, IceServer};
use tomoshibi_protocol::relays::{remember_mainland, Relay};

struct Quiet;

impl CallObserver for Quiet {
    fn on_event(&self, _event: CallEvent) {}
}

/// A port that counts the connections made to it.
async fn listening() -> (u16, Arc<AtomicUsize>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let count = Arc::new(AtomicUsize::new(0));
    let counted = count.clone();
    tokio::spawn(async move {
        while let Ok((_stream, _)) = listener.accept().await {
            counted.fetch_add(1, Ordering::SeqCst);
        }
    });
    (port, count)
}

fn relay(name: &str, url: &str, region: &str) -> Relay {
    Relay {
        name: name.into(),
        url: Some(url.into()),
        region: Some(region.into()),
        label: None,
        lat: None,
        lon: None,
        probe: None,
        maintenance: false,
        fallback: false,
        admin_only: false,
    }
}

fn options(url: String, forward: Option<IceServer>) -> CallOptions {
    CallOptions { url, token: "not-a-token".into(), room: "mainland".into(), sealing_word: None, forward }
}

#[tokio::test(flavor = "multi_thread")]
async fn the_signalling_of_a_mainland_relay_is_never_opened_over_tls() {
    let (port, connections) = listening().await;
    // As a relay list read from the deployment would say it: the server gives
    // a mainland relay's address where it has no probe, and to an
    // administrator always.
    remember_mainland(&[relay("shanghai", &format!("wss://localhost:{port}"), "CN-East")]);

    let refused = join_call(options(format!("wss://localhost:{port}"), None), Arc::new(Quiet)).await;
    match refused {
        Err(CallError::Connect { detail }) => assert!(detail.contains("mainland"), "{detail}"),
        Err(other) => panic!("refused for the wrong reason: {other:?}"),
        Ok(_) => panic!("a call was opened to a mainland relay over TLS"),
    }
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(connections.load(Ordering::SeqCst), 0, "the mainland relay was connected to");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_forward_over_tls_is_never_used() {
    let (port, connections) = listening().await;
    let forward = IceServer {
        url: format!("turns:localhost:{port}?transport=tcp"),
        username: "user".into(),
        credential: "secret".into(),
    };

    let refused = join_call(options(format!("ws://127.0.0.1:{port}"), Some(forward)), Arc::new(Quiet)).await;
    match refused {
        Err(CallError::Connect { detail }) => {
            assert!(detail.contains("turns:"), "{detail}");
            assert!(!detail.contains("secret"), "the TURN credential is in the error: {detail}");
        }
        Err(other) => panic!("refused for the wrong reason: {other:?}"),
        Ok(_) => panic!("a call was opened through a TURN server over TLS"),
    }
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(connections.load(Ordering::SeqCst), 0, "something was connected to");
}
