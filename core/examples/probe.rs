// A throwaway probe for the native core's riskiest assumption: that one
// participant can publish its voice, the sound of its screen and its music as
// three independent audio tracks, the way the web client does, so that a
// browser in the same room can turn each one up, down or off on its own.
//
//   cargo run --example probe -- <base-url> <room> <seconds>
//
// It joins through the deployment's own API, publishes synthetic tones and a
// synthetic camera, says something in chat, raises a hand, sends a reaction,
// mints an invitation for a browser to follow it in with, and prints every
// event it sees until the time is up.

use std::{borrow::Cow, collections::HashMap, f32::consts::TAU, time::Duration};

use livekit::{
    options::{AudioEncoding, TrackPublishOptions, VideoCodec, VideoPreset},
    prelude::*,
    webrtc::{
        audio_frame::AudioFrame,
        audio_source::{native::NativeAudioSource, AudioSourceOptions, RtcAudioSource},
        video_frame::{I420Buffer, VideoFrame, VideoRotation},
        video_source::{native::NativeVideoSource, RtcVideoSource, VideoResolution},
    },
};
use futures_util::StreamExt;
use livekit::e2ee::{
    key_provider::{KeyProvider, KeyProviderOptions},
    E2eeOptions, EncryptionType,
};
use livekit::webrtc::{
    audio_stream::native::NativeAudioStream, native::frame_cryptor::KeyDerivationAlgorithm,
    video_stream::native::NativeVideoStream,
};
use serde::Deserialize;

#[derive(Deserialize, Debug)]
struct Grant {
    url: String,
    token: String,
    identity: String,
    room: String,
}

#[derive(Deserialize, Debug)]
struct Invite {
    token: String,
}

const RATE: u32 = 48_000;

fn tone(source: NativeAudioSource, channels: u32, hz: [f32; 2]) {
    tokio::spawn(async move {
        let per = (RATE / 100) as usize;
        let mut phase = [0f32; 2];
        let mut tick = tokio::time::interval(Duration::from_millis(10));
        loop {
            tick.tick().await;
            let mut data = Vec::with_capacity(per * channels as usize);
            for _ in 0..per {
                for c in 0..channels as usize {
                    phase[c] = (phase[c] + TAU * hz[c] / RATE as f32) % TAU;
                    data.push((phase[c].sin() * 6000.0) as i16);
                }
            }
            let frame = AudioFrame {
                data: Cow::Owned(data),
                sample_rate: RATE,
                num_channels: channels,
                samples_per_channel: per as u32,
            };
            if source.capture_frame(&frame).await.is_err() {
                return;
            }
        }
    });
}

fn picture(source: NativeVideoSource, width: u32, height: u32) {
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_millis(33));
        let mut n: u32 = 0;
        loop {
            tick.tick().await;
            n = n.wrapping_add(1);
            let mut buffer = I420Buffer::new(width, height);
            let (sy, _, _) = buffer.strides();
            let (y, u, v) = buffer.data_mut();
            for row in 0..height {
                for col in 0..width {
                    y[(row * sy + col) as usize] = ((col + row + n * 4) % 256) as u8;
                }
            }
            u.fill(90);
            v.fill(200);
            let frame = VideoFrame {
                rotation: VideoRotation::VideoRotation0,
                timestamp_us: 0,
                frame_metadata: None,
                buffer,
            };
            source.capture_frame(&frame);
        }
    });
}

async fn publish_audio(
    room: &Room,
    name: &str,
    channels: u32,
    hz: [f32; 2],
    source: TrackSource,
    bitrate: u64,
    red: bool,
) {
    let native = NativeAudioSource::new(AudioSourceOptions::default(), RATE, channels, 0);
    let track = LocalAudioTrack::create_audio_track(name, RtcAudioSource::Native(native.clone()));
    let options = TrackPublishOptions {
        source,
        audio_encoding: Some(AudioEncoding { max_bitrate: bitrate }),
        dtx: false,
        red,
        ..Default::default()
    };
    match room.local_participant().publish_track(LocalTrack::Audio(track), options).await {
        Ok(p) => println!("published audio {:?} name={:?} sid={}", source, name, p.sid()),
        Err(e) => println!("audio {:?} refused: {e}", source),
    }
    tone(native, channels, hz);
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let base = args.get(1).cloned().unwrap_or("http://127.0.0.1:8080".into());
    let room_name = args.get(2).cloned().unwrap_or("native-probe".into());
    let seconds: u64 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(60);

    let http = reqwest::Client::new();
    let grant: Grant = http
        .post(format!("{base}/api/rooms/{room_name}/join"))
        .json(&serde_json::json!({"name": "Native Probe", "passphrase": "native-probe-passphrase", "relay": ""}))
        .send()
        .await
        .expect("join")
        .json()
        .await
        .expect("join answer");
    println!("joined as {} in {} via {}", grant.identity, grant.room, grant.url);

    let mut options = RoomOptions::default();
    options.adaptive_stream = true;
    options.dynacast = true;
    // A sealed call, keyed exactly as the web client keys it: the room's own
    // name, a NUL, then the word (web/src/live/secrecy.ts). The older `e2ee`
    // field rather than `encryption`, because the newer one also encrypts the
    // data channel and a browser would read none of it.
    if let Ok(word) = std::env::var("SEAL") {
        let key = format!("{}\u{0}{}", grant.room, word).into_bytes();
        let provider = KeyProvider::with_shared_key(
            KeyProviderOptions {
                ratchet_window_size: 0,
                ratchet_salt: b"LKFrameEncryptionKey".to_vec(),
                failure_tolerance: -1,
                key_ring_size: 16,
                key_derivation_algorithm: KeyDerivationAlgorithm::PBKDF2,
            },
            key,
        );
        #[allow(deprecated)]
        {
            options.e2ee = Some(E2eeOptions { encryption_type: EncryptionType::Gcm, key_provider: provider });
        }
        println!("sealed with a word of {} characters", word.chars().count());
    }
    let (room, mut events) = Room::connect(&grant.url, &grant.token, options).await.expect("connect");

    publish_audio(&room, "", 1, [440.0, 440.0], TrackSource::Microphone, 64_000, true).await;
    publish_audio(&room, "", 2, [660.0, 880.0], TrackSource::ScreenshareAudio, 160_000, true).await;
    publish_audio(&room, "music", 2, [330.0, 495.0], TrackSource::Unknown, 510_000, false).await;

    let camera = NativeVideoSource::new(VideoResolution { width: 1280, height: 720 }, false);
    let track = LocalVideoTrack::create_video_track("", RtcVideoSource::Native(camera.clone()));
    let options = TrackPublishOptions {
        source: TrackSource::Camera,
        video_codec: VideoCodec::H264,
        simulcast: true,
        simulcast_layers: Some(vec![
            VideoPreset::new(320, 180, 160_000, 20.0),
            VideoPreset::new(640, 360, 450_000, 20.0),
        ]),
        ..Default::default()
    };
    match room.local_participant().publish_track(LocalTrack::Video(track), options).await {
        Ok(p) => println!("published camera sid={}", p.sid()),
        Err(e) => println!("camera refused: {e}"),
    }
    picture(camera, 1280, 720);

    let invite: Invite = http
        .post(format!("{base}/api/rooms/{}/invites", grant.room))
        .bearer_auth(&grant.token)
        .send()
        .await
        .expect("invite")
        .json()
        .await
        .expect("invite answer");
    println!("INVITE {}", invite.token);

    let deadline = tokio::time::sleep(Duration::from_secs(seconds));
    tokio::pin!(deadline);
    let mut greeted = false;
    loop {
        tokio::select! {
            _ = &mut deadline => break,
            Some(event) = events.recv() => match event {
                RoomEvent::ParticipantConnected(p) => println!("in: {} {}", p.identity(), p.name()),
                RoomEvent::ParticipantActive(p) => {
                    println!("active: {}", p.identity());
                    if !greeted {
                        greeted = true;
                        let me = room.local_participant();
                        // Later rather than at once: a page that has only just
                        // joined mounts its chat a moment after it is reachable.
                        let later = me.clone();
                        tokio::spawn(async move {
                            tokio::time::sleep(Duration::from_secs(5)).await;
                            let said = later.send_chat_message("hello from the native core".into(), None, None).await;
                            println!("chat sent: {}", said.is_ok());
                        });
                        let mut hand = HashMap::new();
                        hand.insert("hand".to_string(), "up".to_string());
                        println!("hand: {:?}", me.set_attributes(hand).await.map(|_| ()));
                        let packet = DataPacket {
                            payload: "\u{1F44D}".as_bytes().to_vec(),
                            topic: Some("reaction".into()),
                            reliable: true,
                            ..Default::default()
                        };
                        println!("reaction: {:?}", me.publish_data(packet).await.map(|_| ()));
                    }
                }
                RoomEvent::ParticipantDisconnected(p) => println!("out: {}", p.identity()),
                RoomEvent::TrackSubscribed { track, publication, participant } => {
                    println!(
                        "subscribed: {} {:?} {:?} name={:?}",
                        participant.identity(), publication.kind(), publication.source(), publication.name()
                    );
                    // What was decoded, not what was declared: a wrong key
                    // still subscribes, and only the frames tell.
                    let label = format!("{:?}", publication.source());
                    // The receive-side cushion the web client puts on shared
                    // screens, tried here on whatever arrives.
                    if let Ok(ms) = std::env::var("PLAYOUT_MS") {
                        let ms: f64 = ms.parse().unwrap_or(200.0);
                        println!("playout delay {ms} ms on {label}: {:?}", track.set_playout_delay(Some(ms / 1000.0)));
                    }
                    let watched = track.clone();
                    let name = label.clone();
                    tokio::spawn(async move {
                        tokio::time::sleep(Duration::from_secs(12)).await;
                        if let Ok(stats) = watched.get_stats().await {
                            for s in stats {
                                if let livekit::webrtc::stats::RtcStats::InboundRtp(i) = s {
                                    let n = i.inbound.jitter_buffer_emitted_count.max(1) as f64;
                                    println!(
                                        "buffer {name}: average {:.0} ms, target {:.0} ms, minimum {:.0} ms over {} samples",
                                        i.inbound.jitter_buffer_delay / n * 1000.0,
                                        i.inbound.jitter_buffer_target_delay / n * 1000.0,
                                        i.inbound.jitter_buffer_minimum_delay / n * 1000.0,
                                        i.inbound.jitter_buffer_emitted_count
                                    );
                                }
                            }
                        }
                    });
                    match track {
                        RemoteTrack::Video(video) => {
                            // What the freeze fallback and the pixel-exact
                            // subscription both rest on: asking the server
                            // for a size, and getting that rung.
                            if std::env::var("LAYERS").is_ok() {
                                let publication = publication.clone();
                                tokio::spawn(async move {
                                    tokio::time::sleep(Duration::from_secs(3)).await;
                                    println!("asking for 1280x720");
                                    publication.update_video_dimensions(livekit::track::TrackDimension(1280, 720));
                                    tokio::time::sleep(Duration::from_secs(10)).await;
                                    println!("asking for 320x180");
                                    publication.update_video_dimensions(livekit::track::TrackDimension(320, 180));
                                });
                            }
                            let mut stream = NativeVideoStream::new(video.rtc_track());
                            tokio::spawn(async move {
                                let (mut n, mut last) = (0u64, tokio::time::Instant::now());
                                while let Some(frame) = stream.next().await {
                                    n += 1;
                                    if last.elapsed() >= Duration::from_secs(2) {
                                        println!("decoded {label}: {n} frames, {}x{}", frame.buffer.width(), frame.buffer.height());
                                        last = tokio::time::Instant::now();
                                    }
                                }
                            });
                        }
                        RemoteTrack::Audio(audio) => {
                            let mut stream = NativeAudioStream::new(audio.rtc_track(), 48_000, 1);
                            tokio::spawn(async move {
                                let (mut sum, mut count, mut last) = (0f64, 0u64, tokio::time::Instant::now());
                                while let Some(frame) = stream.next().await {
                                    for v in frame.data.iter() { sum += (*v as f64) * (*v as f64); count += 1; }
                                    if last.elapsed() >= Duration::from_secs(5) {
                                        println!("heard {label}: rms {:.0}", (sum / count.max(1) as f64).sqrt());
                                        (sum, count, last) = (0.0, 0, tokio::time::Instant::now());
                                    }
                                }
                            });
                        }
                    }
                }
                RoomEvent::E2eeStateChanged { participant, state } => println!("e2ee {}: {:?}", participant.identity(), state),
                RoomEvent::ChatMessage { message, participant } => println!(
                    "chat from {:?}: {}", participant.map(|p| p.identity()), message.message
                ),
                RoomEvent::DataReceived { payload, topic, participant, .. } => println!(
                    "data {:?} from {:?}: {}", topic, participant.map(|p| p.identity()), String::from_utf8_lossy(&payload)
                ),
                RoomEvent::ParticipantAttributesChanged { participant, changed_attributes } => println!(
                    "attributes of {}: {:?}", participant.identity(), changed_attributes
                ),
                RoomEvent::Disconnected { reason } => { println!("disconnected {:?}", reason); break }
                _ => {}
            }
        }
    }
    let _ = room.close().await;
    println!("done");
}
