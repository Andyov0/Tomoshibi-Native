//! A share in a real call, against a local deployment: the check that what
//! a share publishes arrives in a browser, which no unit test can make.
//!
//! Ignored unless asked for by name, and refused without a local
//! deployment. The test joins a room as its host, writes an invite to the
//! log ("made an invite: <token>") for a browser to follow it in with,
//! shares, and keeps sharing for a while before stopping and leaving:
//!
//! ```text
//! TOMOSHIBI_BASE=http://<local server> TOMOSHIBI_PASSPHRASE_FILE=<file> \
//!   cargo test -- --ignored --exact media::screen::live::shares_into_a_call --nocapture
//! ```
//!
//! With `TOMOSHIBI_SCREEN=synthetic` it shares the generated screen and
//! chord and needs no portal; without, it asks the session's portal, which
//! a test runs headless with a picker that answers by itself
//! (xdg-desktop-portal-wlr's `chooser_type=none`). The rest of the
//! environment:
//!
//! - `TOMOSHIBI_TEST_ROOM`: the room (default `screen-share`).
//! - `TOMOSHIBI_TEST_SECONDS`: how long to share (default 40).
//! - `TOMOSHIBI_TEST_FPS`: the share's rate (default 30).
//! - `TOMOSHIBI_TEST_QUALITY`: the share's size setting, `1080`, `1440` or
//!   `4k` (default automatic).
//! - `TOMOSHIBI_TEST_CHANGE_AFTER`: seconds after which the share is changed
//!   to 1080p at 15 fps without its sound, then back with its sound, to
//!   check a change keeps the share on the browser's stage.
//! - `TOMOSHIBI_TEST_OWN_TONE`: this process plays a 1 kHz tone of its own
//!   on the default output for the whole test, which the screen's sound must
//!   leave out (sound.rs): the browser should hear the other application's
//!   tone and not this one.

use std::{
    f64::consts::TAU,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};
use tomoshibi_core::call::{join_call, CallEvent, CallObserver};
use tomoshibi_protocol::{api::Deployment, share::ShareQuality};

use super::{Screens, Source};
use crate::{
    app::{log, CallEntry},
    media::ShareChoice,
};

fn env(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

struct Told(UnboundedSender<CallEvent>);

impl CallObserver for Told {
    fn on_event(&self, event: CallEvent) {
        let _ = self.0.send(event);
    }
}

#[test]
#[ignore = "needs a local deployment (TOMOSHIBI_BASE) and, for the portal, a session with one"]
fn shares_into_a_call() {
    let base = env("TOMOSHIBI_BASE").expect("TOMOSHIBI_BASE: a local deployment, never a real one");
    assert!(
        !base.contains("meet.example.com"),
        "TOMOSHIBI_BASE must be a local deployment"
    );
    crate::app::client::tell();
    let context = glib::MainContext::new();
    context
        .with_thread_default(|| context.block_on(run(base)))
        .expect("the test's main context");
}

async fn run(base: String) {
    let deployment = Deployment::new(base).expect("a deployment");
    let room = env("TOMOSHIBI_TEST_ROOM").unwrap_or_else(|| "screen-share".into());
    let passphrase = env("TOMOSHIBI_PASSPHRASE_FILE")
        .and_then(|file| std::fs::read_to_string(file).ok())
        .map(|text| text.trim().to_owned())
        .unwrap_or_default();
    let seconds: u64 = env("TOMOSHIBI_TEST_SECONDS")
        .and_then(|s| s.parse().ok())
        .unwrap_or(40);
    let fps: u32 = env("TOMOSHIBI_TEST_FPS")
        .and_then(|s| s.parse().ok())
        .unwrap_or(30);
    let change_after: Option<u64> = env("TOMOSHIBI_TEST_CHANGE_AFTER").and_then(|s| s.parse().ok());
    let quality = match env("TOMOSHIBI_TEST_QUALITY").as_deref() {
        Some("1080") => ShareQuality::P1080,
        Some("1440") => ShareQuality::P1440,
        Some("4k") => ShareQuality::P4k,
        _ => ShareQuality::Auto,
    };

    let name = "Linux share".to_owned();
    let grant = deployment
        .clone()
        .join(
            room,
            name.clone(),
            passphrase,
            None,
            None,
            String::new(),
            None,
        )
        .await
        .expect("the join was granted");
    let entry = CallEntry {
        deployment: deployment.clone(),
        grant: grant.clone(),
        sealing_word: None,
        name,
        microphone_on: false,
        camera_on: false,
        microphone: None,
        camera: None,
    };
    let (told, mut events) = unbounded_channel();
    let call = join_call(entry.options(), Arc::new(Told(told)))
        .await
        .expect("joined the call");
    log(&format!("joined the call as {}", call.local_identity()));
    let invite = deployment
        .clone()
        .create_invite(grant.room.clone(), grant.token.clone())
        .await
        .expect("an invite");
    log(&format!("made an invite: {}", invite.token));
    let follow = glib::spawn_future_local(async move {
        while let Some(event) = events.recv().await {
            match event {
                CallEvent::ParticipantJoined { participant } => {
                    log(&format!("{} joined", participant.identity));
                }
                CallEvent::ParticipantLeft { identity } => log(&format!("{identity} left")),
                CallEvent::Ended { reason } => log(&format!("the call ended: {reason}")),
                _ => {}
            }
        }
    });

    let tone = env("TOMOSHIBI_TEST_OWN_TONE").map(|_| OwnTone::start(1000.0));

    let screens = Screens::new(Source::from_environment());
    let choice = ShareChoice {
        fps,
        quality,
        with_sound: true,
    };
    let share = screens
        .share(call.clone(), choice)
        .await
        .expect("the screen was shared");
    log(&format!("shared; its sound sent: {}", share.sends_sound()));
    share.on_ended(Box::new(|| log("the share ended by itself")));

    let mut waited = 0;
    if let Some(after) = change_after.filter(|after| *after < seconds) {
        glib::timeout_future(Duration::from_secs(after)).await;
        waited = after;
        let smaller = ShareChoice {
            fps: 15,
            quality: ShareQuality::P1080,
            with_sound: false,
        };
        log(&format!(
            "changing the share: {:?}",
            share.change(smaller).await
        ));
        log(&format!("its sound sent: {}", share.sends_sound()));
        let back = (seconds - after) / 2;
        glib::timeout_future(Duration::from_secs(back)).await;
        waited += back;
        log(&format!(
            "changing it back: {:?}",
            share.change(choice).await
        ));
        log(&format!("its sound sent: {}", share.sends_sound()));
    }
    glib::timeout_future(Duration::from_secs(seconds - waited)).await;

    share.stop().await;
    if let Some(tone) = tone {
        tone.stop();
    }
    call.leave().await;
    follow.abort();
    log("left the call");
}

/// A tone this process plays through PipeWire on the default output, as
/// the call's own playback would be: the screen's sound must not carry it.
struct OwnTone {
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl OwnTone {
    fn start(hz: f64) -> OwnTone {
        let stop = Arc::new(AtomicBool::new(false));
        let thread = std::thread::spawn({
            let stop = stop.clone();
            move || play(hz, &stop)
        });
        OwnTone {
            stop,
            thread: Some(thread),
        }
    }

    fn stop(mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn play(hz: f64, stop: &AtomicBool) {
    use pipewire as pw;
    use pw::spa;

    assert!(super::init_pipewire(), "PipeWire");
    let mainloop = pw::main_loop::MainLoopRc::new(None).expect("main loop");
    let context = pw::context::ContextRc::new(&mainloop, None).expect("context");
    let core = context.connect_rc(None).expect("PipeWire");
    let stream = pw::stream::StreamRc::new(
        core,
        "tomoshibi-test-own-tone",
        pw::properties::properties! {
            *pw::keys::MEDIA_TYPE => "Audio",
            *pw::keys::MEDIA_CATEGORY => "Playback",
            *pw::keys::MEDIA_ROLE => "Communication",
        },
    )
    .expect("stream");
    let mut phase = 0.0f64;
    let _listener = stream
        .add_local_listener_with_user_data(())
        .process(move |stream, _| {
            let Some(mut buffer) = stream.dequeue_buffer() else {
                return;
            };
            let Some(data) = buffer.datas_mut().first_mut() else {
                return;
            };
            let frames = {
                let Some(bytes) = data.data() else {
                    return;
                };
                let frames = (bytes.len() / 4).min(480);
                for frame in 0..frames {
                    let sample = ((phase.sin() * 0.2) * 32767.0) as i16;
                    phase = (phase + TAU * hz / 48_000.0) % TAU;
                    for channel in 0..2 {
                        let at = frame * 4 + channel * 2;
                        bytes[at..at + 2].copy_from_slice(&sample.to_le_bytes());
                    }
                }
                frames
            };
            let chunk = data.chunk_mut();
            *chunk.offset_mut() = 0;
            *chunk.stride_mut() = 4;
            *chunk.size_mut() = (frames * 4) as u32;
        })
        .register()
        .expect("listener");
    let mut info = spa::param::audio::AudioInfoRaw::new();
    info.set_format(spa::param::audio::AudioFormat::S16LE);
    info.set_rate(48_000);
    info.set_channels(2);
    let object = spa::pod::Object {
        type_: spa::utils::SpaTypes::ObjectParamFormat.as_raw(),
        id: spa::param::ParamType::EnumFormat.as_raw(),
        properties: info.into(),
    };
    let bytes = spa::pod::serialize::PodSerializer::serialize(
        std::io::Cursor::new(Vec::new()),
        &spa::pod::Value::Object(object),
    )
    .expect("pod")
    .0
    .into_inner();
    let mut params = [spa::pod::Pod::from_bytes(&bytes).expect("pod")];
    stream
        .connect(
            spa::utils::Direction::Output,
            None,
            pw::stream::StreamFlags::AUTOCONNECT | pw::stream::StreamFlags::MAP_BUFFERS,
            &mut params,
        )
        .expect("connect");
    log("playing this process's own tone at 1 kHz");
    while !stop.load(Ordering::Acquire) {
        mainloop
            .loop_()
            .iterate(pw::loop_::Timeout::Finite(Duration::from_millis(50)));
    }
    let _ = stream.disconnect();
}
