// Two participants through the core's own call API: one shares sound alone,
// offered without loss, and the other listens. Prints what the listener's
// lossless sink receives each second, so a stream that never starts, starts
// at the wrong rate, or falls back to Opus shows at once. A browser that joins
// the same room asks for the stream too, which interop scripts can check.
//
//   cargo run --example lossless -- <base-url> <room> <seconds>

use std::{
    f32::consts::TAU,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::Duration,
};

use serde::Deserialize;
use tomoshibi_core::call::{join_call, CallEvent, CallObserver, CallOptions, LosslessSink};
use tomoshibi_protocol::share::SoundKind;

#[derive(Deserialize)]
struct Grant {
    url: String,
    token: String,
    room: String,
}

struct Quiet(&'static str);

impl CallObserver for Quiet {
    fn on_event(&self, event: CallEvent) {
        match event {
            CallEvent::TrackSubscribed { identity, track } => {
                println!("{}: subscribed {:?} {:?} from {}", self.0, track.source, track.name, &identity[..12])
            }
            CallEvent::LosslessGaveUp => println!("{}: lossless given up", self.0),
            CallEvent::Ended { reason } => println!("{}: ended {reason}", self.0),
            _ => {}
        }
    }
}

#[derive(Default)]
struct Counting {
    frames: AtomicU64,
    rate: AtomicU64,
    channels: AtomicU64,
    ends: AtomicU64,
}

impl LosslessSink for Counting {
    fn on_lossless(&self, _identity: String, music: bool, samples: Vec<f32>, rate: u32, channels: u32) {
        assert!(!music);
        self.frames.fetch_add(samples.len() as u64 / channels as u64, Ordering::Relaxed);
        self.rate.store(rate as u64, Ordering::Relaxed);
        self.channels.store(channels as u64, Ordering::Relaxed);
    }

    fn on_lossless_end(&self, _identity: String, _music: bool) {
        self.ends.fetch_add(1, Ordering::Relaxed);
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

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let base = args.get(1).cloned().unwrap_or("http://127.0.0.1:8080".into());
    let room = args.get(2).cloned().unwrap_or("lossless".into());
    let seconds: u64 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(30);

    let options = |g: Grant| CallOptions { url: g.url, token: g.token, room: g.room, sealing_word: None, forward: None };
    let sharer = join_call(options(grant(&base, &room, "Lossless Sharer").await), Arc::new(Quiet("sharer"))).await.expect("sharer");
    let input = sharer.clone().publish_audio(SoundKind::Listening { lossless_offered: true }).await.expect("publish");
    println!("sharer published {}", input.sid());

    let listener = join_call(options(grant(&base, &room, "Lossless Listener").await), Arc::new(Quiet("listener"))).await.expect("listener");
    let counting = Arc::new(Counting::default());
    listener.set_lossless_sink(counting.clone());

    // 440 Hz on the left and 660 Hz on the right, ten milliseconds at a time,
    // to the Opus track and, as floats, to the lossless stream.
    let feeding = input.clone();
    tokio::spawn(async move {
        let mut phase = [0f32; 2];
        let mut tick = tokio::time::interval(Duration::from_millis(10));
        let mut at: i64 = 0;
        loop {
            tick.tick().await;
            let mut ints = Vec::with_capacity(960);
            let mut floats = Vec::with_capacity(960);
            for _ in 0..480 {
                for (c, hz) in [440f32, 660f32].iter().enumerate() {
                    phase[c] = (phase[c] + TAU * hz / 48_000.0) % TAU;
                    let value = phase[c].sin() * 0.3;
                    floats.push(value);
                    ints.push((value * 32767.0) as i16);
                }
            }
            feeding.push(ints);
            feeding.clone().push_lossless(floats, 2, 48_000, at);
            at += 10_000;
        }
    });

    let mut last = 0;
    for second in 1..=seconds {
        tokio::time::sleep(Duration::from_secs(1)).await;
        let now = counting.frames.load(Ordering::Relaxed);
        println!(
            "t={second:>2}s listener lossless: {:>6} frames this second at {} Hz x{}, ends {}",
            now - last,
            counting.rate.load(Ordering::Relaxed),
            counting.channels.load(Ordering::Relaxed),
            counting.ends.load(Ordering::Relaxed)
        );
        last = now;
    }
    listener.leave().await;
    sharer.leave().await;
}
