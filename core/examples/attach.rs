// Calls the exported functions that are not async the way an app does: from
// a thread of its own, outside any runtime, once a remote sound has arrived.
// Each of them used to panic there ("no reactor running"), which in the app
// was a crash the moment somebody with a microphone was heard.
//
//   cargo run --example attach -- <base-url> <room>

use std::{
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc, Arc, OnceLock,
    },
    time::Duration,
};

use serde::Deserialize;
use tomoshibi_core::call::{join_call, AudioSink, Call, CallEvent, CallObserver, CallOptions};
use tomoshibi_protocol::share::SoundKind;

#[derive(Deserialize)]
struct Grant {
    url: String,
    token: String,
    room: String,
}

struct Arrivals(mpsc::Sender<String>);

impl CallObserver for Arrivals {
    fn on_event(&self, event: CallEvent) {
        if let CallEvent::TrackSubscribed { track, .. } = event {
            let _ = self.0.send(track.sid);
        }
    }
}

struct Silent;

impl CallObserver for Silent {
    fn on_event(&self, _event: CallEvent) {}
}

#[derive(Default)]
struct Counting(AtomicU64);

impl AudioSink for Counting {
    fn on_audio(&self, samples: Vec<i16>, _rate: u32, _channels: u32) {
        self.0.fetch_add(samples.len() as u64, Ordering::Relaxed);
    }
}

async fn grant(base: &str, room: &str, name: &str) -> Grant {
    reqwest::Client::new()
        .post(format!("{base}/api/rooms/{room}/join"))
        .json(&serde_json::json!({"name": name, "passphrase": "", "relay": ""}))
        .send()
        .await
        .expect("join")
        .json()
        .await
        .expect("join answer")
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let base = args.get(1).cloned().unwrap_or("http://127.0.0.1:8080".into());
    let room = args.get(2).cloned().unwrap_or("attach".into());
    let options = |g: Grant| CallOptions { url: g.url, token: g.token, room: g.room, sealing_word: None, forward: None };

    // The joins are async and run on a runtime of the example's own; the
    // calls under test are made from this thread, which is not in it.
    let scaffold = tokio::runtime::Runtime::new().unwrap();
    let (arrived, arrivals) = mpsc::channel();
    static LISTENER: OnceLock<Arc<Call>> = OnceLock::new();
    let (speaker, input) = scaffold.block_on(async {
        let speaker = join_call(options(grant(&base, &room, "Speaker").await), Arc::new(Silent)).await.expect("speaker");
        let input = speaker.clone().publish_audio(SoundKind::Voice { original: false }).await.expect("publish");
        let listener = join_call(options(grant(&base, &room, "Listener").await), Arc::new(Arrivals(arrived))).await.expect("listener");
        let _ = LISTENER.set(listener);
        (speaker, input)
    });
    let feeding = input.clone();
    scaffold.spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_millis(10));
        loop {
            tick.tick().await;
            feeding.push(vec![1000i16; 480]);
        }
    });

    let sid = arrivals.recv_timeout(Duration::from_secs(10)).expect("a sound arrived");
    let listener = LISTENER.get().unwrap().clone();
    let counting = Arc::new(Counting::default());
    println!("attaching {sid} from a thread outside any runtime");
    listener.attach_audio(sid.clone(), counting.clone());
    std::thread::sleep(Duration::from_secs(2));
    println!("samples after attach: {}", counting.0.load(Ordering::Relaxed));
    listener.set_remote_enabled(sid.clone(), false).expect("disable");
    listener.set_remote_enabled(sid.clone(), true).expect("enable");
    input.set_muted(true);
    input.set_muted(false);
    listener.detach(sid);
    println!("all calls returned");
    scaffold.block_on(async {
        listener.leave().await;
        speaker.leave().await;
    });
}
