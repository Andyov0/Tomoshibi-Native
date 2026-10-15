// Fills a room with people who say and show nothing, for looking at what a
// crowded call looks like (pages, the strip) without a browser per person.
//
//   cargo run --example crowd -- <base-url> <room> <people> <seconds>

use std::{sync::Arc, time::Duration};

use serde::Deserialize;
use tomoshibi_core::call::{join_call, CallEvent, CallObserver, CallOptions};

#[derive(Deserialize)]
struct Grant {
    url: String,
    token: String,
    room: String,
}

struct Silent;

impl CallObserver for Silent {
    fn on_event(&self, _event: CallEvent) {}
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let base = args.get(1).cloned().unwrap_or("http://127.0.0.1:8080".into());
    let room = args.get(2).cloned().unwrap_or("crowd".into());
    let people: usize = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(11);
    let seconds: u64 = args.get(4).and_then(|s| s.parse().ok()).unwrap_or(60);
    let http = reqwest::Client::new();
    let mut calls = Vec::new();
    for n in 1..=people {
        let grant: Grant = http
            .post(format!("{base}/api/rooms/{room}/join"))
            .json(&serde_json::json!({"name": format!("Guest {n}"), "passphrase": "", "relay": ""}))
            .send()
            .await
            .expect("join")
            .json()
            .await
            .expect("answer");
        let options = CallOptions { url: grant.url, token: grant.token, room: grant.room, sealing_word: None, forward: None };
        calls.push(join_call(options, Arc::new(Silent)).await.expect("call"));
        println!("joined Guest {n}");
    }
    tokio::time::sleep(Duration::from_secs(seconds)).await;
    for call in calls {
        call.leave().await;
    }
}
