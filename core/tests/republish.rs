//! What this participant sends can still be taken back after a full
//! reconnect.
//!
//! A full reconnect (a Mac waking from sleep, an outage longer than a resume
//! survives, a relay or its node restarting) publishes every local track again
//! under a new SID. The inputs kept the first one, and every unpublish after it
//! was answered "track not found" and swallowed by the apps: the camera turned
//! off froze on everybody else's screen, a stopped share stayed on the stage,
//! and original sound sent a second microphone. Here a second participant
//! watches what the server says the first one is sending.
//!
//! Against a local deployment, `TOMOSHIBI_LOCAL_DEPLOYMENT` or
//! http://127.0.0.1:8080, with rooms opened by anyone; skipped, saying so,
//! where nothing answers. Only a loopback address is ever used. The browser's
//! view of the same thing is interop/sent.js with the `republish` example.

use std::{
    sync::{mpsc, Arc},
    time::{Duration, Instant},
};

use tomoshibi_core::call::{
    join_call, Call, CallEvent, CallObserver, CallOptions, CallState, MediaKind, VideoKind,
};
use tomoshibi_protocol::{api::Deployment, share::SoundKind, ProtocolError};

async fn live() -> Option<String> {
    let base = std::env::var("TOMOSHIBI_LOCAL_DEPLOYMENT").unwrap_or_else(|_| "http://127.0.0.1:8080".into());
    let host = reqwest::Url::parse(&base).ok()?.host_str()?.to_owned();
    assert!(
        matches!(host.as_str(), "127.0.0.1" | "localhost" | "[::1]"),
        "{base} is not a local deployment, and nothing here is sent anywhere else"
    );
    match Deployment::new(base.clone()).unwrap().deployment_info().await {
        Ok(_) => Some(base),
        Err(ProtocolError::Network { detail }) => {
            eprintln!("skipped: no deployment answering at {base} ({detail})");
            None
        }
        Err(other) => panic!("the deployment at {base} answered oddly: {other:?}"),
    }
}

struct States(mpsc::Sender<CallState>);

impl CallObserver for States {
    fn on_event(&self, event: CallEvent) {
        if let CallEvent::StateChanged { state } = event {
            let _ = self.0.send(state);
        }
    }
}

struct Quiet;

impl CallObserver for Quiet {
    fn on_event(&self, _event: CallEvent) {}
}

/// Joins as `name` and says the call and an invitation to the room (empty
/// where this participant may not make one).
async fn join(
    base: &str,
    room: &str,
    name: &str,
    passphrase: &str,
    invite: Option<String>,
    observer: Arc<dyn CallObserver>,
) -> (Arc<Call>, String) {
    let deployment = Deployment::new(base.to_owned()).unwrap();
    let grant = deployment
        .clone()
        .join(room.to_owned(), name.to_owned(), passphrase.to_owned(), None, String::new(), invite)
        .await
        .expect("join");
    let call = join_call(
        CallOptions { url: grant.url, token: grant.token.clone(), room: grant.room, sealing_word: None, forward: None },
        observer,
    )
    .await
    .expect("call");
    let made = deployment.create_invite(room.to_owned(), grant.token).await.map(|made| made.token).unwrap_or_default();
    (call, made)
}

/// What the watcher is told the sender is sending: each track's kind and SID.
fn sending(watcher: &Call, sender: &str) -> Vec<(MediaKind, String)> {
    watcher
        .participants()
        .into_iter()
        .filter(|p| p.identity == sender)
        .flat_map(|p| p.tracks.into_iter().map(|t| (t.kind, t.sid)))
        .collect()
}

async fn until(what: &str, mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(20);
    while !done() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_track_published_again_by_a_full_reconnect_can_still_be_taken_back() {
    let Some(base) = live().await else { return };
    let room = format!("republish-{}", std::process::id());

    let (states, heard) = mpsc::channel();
    // A passphrase makes the sender the room's host, which may invite the
    // watcher in; it is one join against the deployment's guessing budget.
    let (sender, invite) = join(&base, &room, "Sender", "republish-test-passphrase", None, Arc::new(States(states))).await;
    let (watcher, _) = join(&base, &room, "Watcher", "", Some(invite), Arc::new(Quiet)).await;
    let who = sender.local_identity();

    let camera = sender.clone().publish_video(VideoKind::Camera, 640, 480).await.expect("camera");
    let voice = sender.clone().publish_audio(SoundKind::Voice { original: false }).await.expect("voice");
    let first = (camera.sid(), voice.sid());
    until("the watcher to see both tracks", || sending(&watcher, &who).len() == 2).await;

    while heard.try_recv().is_ok() {}
    sender.clone().simulate_full_reconnect().await.expect("simulated");
    let started = Instant::now();
    let mut reconnecting = false;
    loop {
        let left = Duration::from_secs(30).saturating_sub(started.elapsed());
        match heard.recv_timeout(left).expect("the reconnect finished") {
            CallState::Reconnecting => reconnecting = true,
            CallState::Connected if reconnecting => break,
            _ => {}
        }
    }

    // Published again, under new SIDs, and the inputs say so.
    until("the watcher to see both tracks again", || {
        let now = sending(&watcher, &who);
        now.len() == 2 && now.iter().all(|(_, sid)| *sid != first.0 && *sid != first.1)
    })
    .await;
    assert_ne!(camera.sid(), first.0, "the camera's input still names the SID it had before the reconnect");

    camera.clone().unpublish().await.expect("the camera was taken back");
    until("the camera to leave the watcher's view", || {
        sending(&watcher, &who).iter().all(|(kind, _)| *kind != MediaKind::Video)
    })
    .await;

    voice.clone().unpublish().await.expect("the voice was taken back");
    until("the voice to leave the watcher's view", || sending(&watcher, &who).is_empty()).await;

    watcher.leave().await;
    sender.leave().await;
}
