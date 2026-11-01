//! The Linux app's devices in a room on a local deployment, used exactly as
//! the room uses them, for the checks against a headless browser
//! (interop/): the microphone opened and published as the voice, the camera
//! opened and published, every remote sound played, and what the speakers
//! received written to the log every five seconds.
//!
//! It is the media module's own check, apart from any screen: it needs a
//! sound server and nothing else of a desktop (no compositor, no GTK
//! window), so it runs in a container beside a PulseAudio or PipeWire with
//! null sinks, where what the app plays can be recorded and what it sends
//! can be read in the browser.
//!
//!   TOMOSHIBI_BASE=http://<deployment>   the local deployment (required)
//!   PROBE_ROOM=<room>                     the room to open (default probe)
//!   PROBE_PASSPHRASE_FILE=<file>          the passphrase the probe opens
//!                                         the room with, as its host, so
//!                                         that it can make an invite
//!   PROBE_SECONDS=<n>                     how long to stay (default 60)
//!   PROBE_CAMERA=1                        publish the camera too
//!   PROBE_BLUR=1                          with its background blurred
//!   PROBE_ORIGINAL=1                      the voice as original sound
//!   PROBE_SPEAKER=<sink>                  play to this sink
//!   TOMOSHIBI_SOURCE, TOMOSHIBI_MUTE, TOMOSHIBI_DEBUG_NO_ECHO_CANCELLER,
//!   TOMOSHIBI_DEBUG_LOG                   as for the app
//!
//! It writes "made an invite: <token>" to the log once it is in, for the
//! browser to follow it with.
//!
//! A debug build only: it reads its deployment from the environment, which
//! a release build never does.

use std::{rc::Rc, sync::Arc, time::Duration};

use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};
use tomoshibi_core::call::{
    join_call, Call, CallEvent, CallObserver, CallOptions, IceServer, MediaKind,
};
use tomoshibi_linux::{
    app::{client, log},
    media::{self, DeviceKind, Media},
};
use tomoshibi_protocol::api::Deployment;

struct ToMainThread(UnboundedSender<CallEvent>);

impl CallObserver for ToMainThread {
    fn on_event(&self, event: CallEvent) {
        let _ = self.0.send(event);
    }
}

fn setting(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|v| v.trim().to_owned())
        .filter(|v| !v.is_empty())
}

fn main() {
    if !cfg!(debug_assertions) {
        eprintln!("media_probe is a debug build's tool");
        std::process::exit(64);
    }
    client::tell();
    let main_loop = glib::MainLoop::new(None, false);
    let quit = main_loop.clone();
    glib::spawn_future_local(async move {
        if let Err(fault) = run().await {
            log(&format!("probe: {fault}"));
        }
        quit.quit();
    });
    main_loop.run();
}

async fn run() -> Result<(), String> {
    let base = setting("TOMOSHIBI_BASE").ok_or("TOMOSHIBI_BASE is required: a local deployment")?;
    let room = setting("PROBE_ROOM").unwrap_or_else(|| "probe".into());
    let seconds: u64 = setting("PROBE_SECONDS")
        .and_then(|s| s.parse().ok())
        .unwrap_or(60);
    let passphrase = setting("PROBE_PASSPHRASE_FILE")
        .and_then(|file| std::fs::read_to_string(file).ok())
        .map(|text| text.trim().to_owned())
        .unwrap_or_default();

    let media = media::standard();
    // The device lists come from the sound server's thread: a moment to
    // arrive.
    glib::timeout_future(Duration::from_millis(500)).await;
    for kind in [
        DeviceKind::Microphone,
        DeviceKind::Speaker,
        DeviceKind::Camera,
    ] {
        for device in media.devices(kind) {
            log(&format!(
                "probe: {kind:?} {} \"{}\"{}",
                device.id,
                device.label,
                if device.is_default { " (default)" } else { "" }
            ));
        }
    }

    let deployment = Deployment::new(base).map_err(|e| e.to_string())?;
    deployment
        .clone()
        .deployment_info()
        .await
        .map_err(|e| e.to_string())?;
    let grant = deployment
        .clone()
        .join(
            room.clone(),
            "Linux probe".into(),
            passphrase.clone(),
            None,
            None,
            String::new(),
            None,
        )
        .await
        .map_err(|e| format!("the join was refused: {e}"))?;
    let options = CallOptions {
        url: grant.url.clone(),
        token: grant.token.clone(),
        room: grant.room.clone(),
        sealing_word: None,
        forward: grant.forward.as_ref().map(|f| IceServer {
            url: f.url.clone(),
            username: f.username.clone(),
            credential: f.credential.clone(),
        }),
    };
    let (sender, events) = unbounded_channel();
    let call = join_call(options, Arc::new(ToMainThread(sender)))
        .await
        .map_err(|e| e.to_string())?;
    log(&format!(
        "probe: joined {} as {}",
        grant.room,
        call.local_identity()
    ));
    media.attach_call(&call);
    if let Some(speaker) = setting("PROBE_SPEAKER") {
        media.set_speaker(Some(speaker));
    }

    let microphone = media
        .open_microphone(None)
        .await
        .map_err(|e| format!("no microphone: {e}"))?;
    microphone
        .publish(call.clone(), setting("PROBE_ORIGINAL").is_some())
        .await
        .map_err(|e| format!("the voice was not published: {e}"))?;
    let camera = if setting("PROBE_CAMERA").is_some() {
        let camera = media
            .open_camera(None)
            .await
            .map_err(|e| format!("no camera: {e}"))?;
        camera.set_blur(setting("PROBE_BLUR").is_some());
        camera
            .publish(call.clone())
            .await
            .map_err(|e| format!("the camera was not published: {e}"))?;
        Some(camera)
    } else {
        None
    };

    if !passphrase.is_empty() {
        match deployment
            .clone()
            .create_invite(grant.room.clone(), grant.token.clone())
            .await
        {
            Ok(made) => log(&format!("made an invite: {}", made.token)),
            Err(fault) => log(&format!("no invite was made: {fault}")),
        }
    }

    // Whoever is in already, then whoever comes.
    for participant in call.participants() {
        for track in participant.tracks {
            if track.kind == MediaKind::Audio && track.subscribed {
                media.play(&call, &track.sid, &participant.identity);
            }
        }
    }
    glib::spawn_future_local(follow(events, media.clone(), call.clone()));

    let level = {
        let microphone = microphone.clone();
        glib::timeout_add_local(Duration::from_secs(2), move || {
            log(&format!(
                "probe: microphone level {:.3}",
                microphone.level()
            ));
            glib::ControlFlow::Continue
        })
    };
    glib::timeout_future(Duration::from_secs(seconds)).await;
    level.remove();

    log("probe: leaving");
    microphone.unpublish().await;
    if let Some(camera) = camera.as_ref() {
        camera.unpublish().await;
        camera.close();
    }
    microphone.close();
    call.leave().await;
    media.detach_call();
    Ok(())
}

/// Plays every sound that arrives, as the room does.
async fn follow(mut events: UnboundedReceiver<CallEvent>, media: Rc<dyn Media>, call: Arc<Call>) {
    while let Some(event) = events.recv().await {
        match event {
            CallEvent::TrackSubscribed { identity, track } if track.kind == MediaKind::Audio => {
                log(&format!(
                    "probe: playing {identity}'s {:?} {}",
                    track.source, track.sid
                ));
                media.play(&call, &track.sid, &identity);
            }
            CallEvent::TrackUnsubscribed { sid, .. } => media.stop_playing(&call, &sid),
            CallEvent::ParticipantJoined { participant } => {
                log(&format!("probe: {} joined", participant.identity))
            }
            CallEvent::Ended { reason } => {
                log(&format!("probe: the call ended: {reason}"));
                return;
            }
            _ => {}
        }
    }
}
