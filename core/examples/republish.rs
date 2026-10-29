// Publishes what an app in a call publishes (a camera, a shared screen and a
// voice), and takes each of them back on command, the way the apps do: the
// camera turned off, the share stopped, original sound switched on (the old
// voice unpublished and a new one published).
//
// After a full reconnect the SDK publishes every local track again under a new
// SID. The inputs kept the first SID, and every one of those commands failed
// with "track not found", which the apps swallowed: a camera turned off froze
// on everybody else's screen, a stopped share stayed on the stage, and
// original sound sent two microphones. Restart the deployment (or cut the
// network for longer than a resume survives) between `ready` and the
// commands, and read what a browser in the room is sent (interop/sent.js),
// the browser having followed on the invitation this prints.
//
//   cargo run --example republish -- <base-url> <room>
//
// Commands on standard input, one a line: `camera`, `share`, `original`,
// `quit`. Each prints what it did and what the core answered; every call
// event is printed as it arrives.

use std::{
    io::BufRead,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use serde::Deserialize;
use tomoshibi_core::call::{join_call, AudioInput, CallEvent, CallObserver, CallOptions, VideoInput, VideoKind};
use tomoshibi_protocol::share::{ShareQuality, SoundKind};

#[derive(Deserialize)]
struct Grant {
    url: String,
    token: String,
    room: String,
}

#[derive(Deserialize)]
struct Invite {
    token: String,
}

struct Printing(Instant);

impl CallObserver for Printing {
    fn on_event(&self, event: CallEvent) {
        let at = self.0.elapsed().as_secs_f64();
        match event {
            CallEvent::StateChanged { state } => println!("{at:7.2} state {state:?}"),
            CallEvent::LocalTrackPublished { track } => {
                println!("{at:7.2} local published {} {:?} {:?}", track.sid, track.kind, track.source)
            }
            CallEvent::LocalTrackUnpublished { sid } => println!("{at:7.2} local unpublished {sid}"),
            CallEvent::ParticipantJoined { participant } => println!("{at:7.2} joined {}", participant.name),
            CallEvent::ParticipantLeft { identity } => println!("{at:7.2} left {identity}"),
            CallEvent::Ended { reason } => println!("{at:7.2} ended {reason}"),
            _ => {}
        }
    }
}

/// A picture that changes every frame, so that an encoder never has nothing
/// to send.
fn feed_video(input: Arc<VideoInput>, width: u32, height: u32, stop: Arc<Mutex<bool>>) {
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_millis(66));
        let mut n: u8 = 0;
        while !*stop.lock().unwrap() {
            tick.tick().await;
            n = n.wrapping_add(3);
            let y = vec![n; (width * height) as usize];
            let chroma = vec![128u8; ((width / 2) * (height / 2)) as usize];
            input.push_planes(y, chroma.clone(), chroma, width, width / 2, 1, width, height, 0, 0);
        }
    });
}

fn feed_audio(input: Arc<AudioInput>, stop: Arc<Mutex<bool>>) {
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_millis(10));
        let mut phase = 0f32;
        while !*stop.lock().unwrap() {
            tick.tick().await;
            let samples: Vec<i16> = (0..480 * input.channels() as usize)
                .map(|_| {
                    phase = (phase + std::f32::consts::TAU * 440.0 / 48_000.0) % std::f32::consts::TAU;
                    (phase.sin() * 3000.0) as i16
                })
                .collect();
            input.push(samples);
        }
    });
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let base = args.get(1).cloned().unwrap_or("http://127.0.0.1:8080".into());
    let room = args.get(2).cloned().unwrap_or("republish".into());

    let grant: Grant = reqwest::Client::new()
        .post(format!("{base}/api/rooms/{room}/join"))
        // A passphrase makes whoever opens the room its host, and only the
        // host may invite the browser in.
        .json(&serde_json::json!({"name": "Native", "passphrase": "native-republish-passphrase", "relay": ""}))
        .send()
        .await
        .expect("join")
        .json()
        .await
        .expect("join answer");

    let call = join_call(
        CallOptions { url: grant.url, token: grant.token, room: grant.room, sealing_word: None, forward: None },
        Arc::new(Printing(Instant::now())),
    )
    .await
    .expect("call");

    let camera = call.clone().publish_video(VideoKind::Camera, 640, 480).await.expect("camera");
    let camera_stop = Arc::new(Mutex::new(false));
    feed_video(camera.clone(), 640, 480, camera_stop.clone());

    let share = call
        .clone()
        .publish_video(VideoKind::Screen { fps: 15, quality: ShareQuality::Auto }, 1280, 720)
        .await
        .expect("share");
    let share_stop = Arc::new(Mutex::new(false));
    feed_video(share.clone(), 1280, 720, share_stop.clone());

    let mut voice = call.clone().publish_audio(SoundKind::Voice { original: false }).await.expect("voice");
    let mut voice_stop = Arc::new(Mutex::new(false));
    feed_audio(voice.clone(), voice_stop.clone());

    // Rooms that exist are entered by invitation, so the browser follows on
    // one minted with the join token, as the probe's browser does.
    let invite: Invite = reqwest::Client::new()
        .post(format!("{base}/api/rooms/{room}/invites"))
        .bearer_auth(call.token())
        .send()
        .await
        .expect("invite")
        .json()
        .await
        .expect("invite answer");
    println!("invite {}", invite.token);
    println!("ready: camera {} share {} voice {}", camera.sid(), share.sid(), voice.sid());

    let (lines, mut commands) = tokio::sync::mpsc::unbounded_channel::<String>();
    std::thread::spawn(move || {
        for line in std::io::stdin().lock().lines().map_while(Result::ok) {
            if lines.send(line.trim().to_owned()).is_err() {
                return;
            }
        }
    });

    while let Some(command) = commands.recv().await {
        match command.as_str() {
            "camera" => {
                // Capture stops first, as in the apps; then the track goes.
                *camera_stop.lock().unwrap() = true;
                println!("camera off ({}): {:?}", camera.sid(), camera.clone().unpublish().await);
            }
            "share" => {
                *share_stop.lock().unwrap() = true;
                println!("share stopped ({}): {:?}", share.sid(), share.clone().unpublish().await);
            }
            "original" => {
                *voice_stop.lock().unwrap() = true;
                println!("old voice off ({}): {:?}", voice.sid(), voice.clone().unpublish().await);
                voice = call.clone().publish_audio(SoundKind::Voice { original: true }).await.expect("voice");
                voice_stop = Arc::new(Mutex::new(false));
                feed_audio(voice.clone(), voice_stop.clone());
                println!("original voice on ({})", voice.sid());
            }
            "quit" => break,
            "" => {}
            other => println!("unknown command {other}"),
        }
    }

    call.leave().await;
    println!("left");
}
