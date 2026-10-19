// Joins a room where others already show their cameras, publishes a camera
// of its own straight after, as every app does at a join with the camera on,
// and counts the frames each remote camera hands over; again and again.
//
// Publishing the first camera rebuilt every remote picture's receive stream
// in libwebrtc (the start-bitrate hint was written on the receiving
// sections of the offer too), and a packet in the gap left a picture with
// no sink: decoded, and handed to nobody for the rest of the call. On
// Windows one launch in four lost a camera that way. This is the same race
// without an app, for any platform the core runs on.
//
//   cargo run --example blackout -- <base-url> <room> <rounds> <seconds> [publish after ms] [busy threads]

use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

use serde::Deserialize;
use tomoshibi_protocol::share::SoundKind;
use tomoshibi_core::call::{
    join_call, Call, CallEvent, CallObserver, CallOptions, MediaKind, MediaSource, RemoteFrame, VideoKind, VideoSink,
};

#[derive(Deserialize)]
struct Grant {
    url: String,
    token: String,
    room: String,
}

#[cfg(target_vendor = "apple")]
#[link(name = "CoreVideo", kind = "framework")]
extern "C" {
    fn CVPixelBufferRelease(buffer: *mut std::ffi::c_void);
}

#[derive(Default)]
struct Counting(AtomicU64);

impl VideoSink for Counting {
    fn on_frame(&self, frame: RemoteFrame) {
        // The frame's pixel buffer is retained once for whoever takes it.
        #[cfg(target_vendor = "apple")]
        if frame.pixel_buffer != 0 {
            unsafe { CVPixelBufferRelease(frame.pixel_buffer as *mut std::ffi::c_void) };
        }
        let _ = frame;
        self.0.fetch_add(1, Ordering::Relaxed);
    }
}

/// Attaches a counter to every remote camera the moment it is subscribed,
/// as the apps attach their pictures.
struct Watching {
    call: Mutex<Option<Arc<Call>>>,
    early: Mutex<Vec<String>>,
    counts: Mutex<HashMap<String, Arc<Counting>>>,
}

impl Watching {
    fn attach(&self, call: &Arc<Call>, sid: String) {
        let counter = Arc::new(Counting::default());
        self.counts.lock().unwrap().insert(sid.clone(), counter.clone());
        call.attach_video(sid.clone(), counter);
        // Asked for large, as a tile on a desktop asks: the top layer is
        // many more packets a second than the lowest, and one of them has
        // to land in the gap.
        let _ = call.request_video_size(sid, 1280, 720);
    }
}

impl CallObserver for Watching {
    fn on_event(&self, event: CallEvent) {
        let CallEvent::TrackSubscribed { track, .. } = event else { return };
        if track.kind != MediaKind::Video || track.source != MediaSource::Camera {
            return;
        }
        match self.call.lock().unwrap().clone() {
            Some(call) => self.attach(&call, track.sid),
            None => self.early.lock().unwrap().push(track.sid),
        }
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
    let room = args.get(2).cloned().unwrap_or("blackout".into());
    let rounds: usize = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(10);
    let seconds: u64 = args.get(4).and_then(|s| s.parse().ok()).unwrap_or(6);
    let after_ms: u64 = args.get(5).and_then(|s| s.parse().ok()).unwrap_or(150);
    // Threads kept busy throughout, standing in for an app's drawing: the
    // gap is wider on a machine with no time to spare.
    let busy: usize = args.get(6).and_then(|s| s.parse().ok()).unwrap_or(0);
    for _ in 0..busy {
        std::thread::spawn(|| {
            let mut x = 0u64;
            loop {
                x = std::hint::black_box(x.wrapping_mul(6364136223846793005).wrapping_add(1));
            }
        });
    }
    let mut bad = 0;
    for round in 1..=rounds {
        let g = grant(&base, &room, &format!("Blackout {round}")).await;
        let watching = Arc::new(Watching { call: Mutex::new(None), early: Mutex::new(Vec::new()), counts: Mutex::new(HashMap::new()) });
        let options = CallOptions { url: g.url, token: g.token, room: g.room, sealing_word: None, forward: None };
        let call = join_call(options, watching.clone()).await.expect("joined");
        *watching.call.lock().unwrap() = Some(call.clone());
        for sid in std::mem::take(&mut *watching.early.lock().unwrap()) {
            watching.attach(&call, sid);
        }
        // The microphone and then the camera, as the apps publish them at a
        // join: the camera a moment after the room's pictures have started
        // arriving.
        let voice = call.clone().publish_audio(SoundKind::Voice { original: false }).await.expect("microphone");
        tokio::time::sleep(Duration::from_millis(after_ms)).await;
        let (width, height) = (1280u32, 720u32);
        let camera = call.clone().publish_video(VideoKind::Camera, width, height).await.expect("camera");
        let feeding = camera.clone();
        let feeder = tokio::spawn(async move {
            let (y, uv) = (vec![120u8; (width * height) as usize], vec![128u8; (width * height / 4) as usize]);
            let mut tick = tokio::time::interval(Duration::from_millis(33));
            let mut at = 0i64;
            loop {
                tick.tick().await;
                at += 33_000;
                feeding.push_planes(y.clone(), uv.clone(), uv.clone(), width, width / 2, 1, width, height, at, 0);
                for _ in 0..3 {
                    voice.push(vec![0i16; 480]);
                }
            }
        });
        tokio::time::sleep(Duration::from_secs(seconds)).await;
        let counts: Vec<(String, u64)> =
            watching.counts.lock().unwrap().iter().map(|(sid, c)| (sid.clone(), c.0.load(Ordering::Relaxed))).collect();
        let zero = counts.iter().filter(|(_, n)| *n == 0).count();
        if zero > 0 || counts.is_empty() {
            bad += 1;
        }
        println!(
            "round {round}: {} cameras={} zero={zero} {}",
            if zero == 0 && !counts.is_empty() { "ok" } else { "BAD" },
            counts.len(),
            counts.iter().map(|(sid, n)| format!("{sid}={n}")).collect::<Vec<_>>().join(" ")
        );
        feeder.abort();
        call.leave().await;
    }
    println!("bad {bad} of {rounds}");
}
