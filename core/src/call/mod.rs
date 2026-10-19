//! A call: one LiveKit room, joined the way the web client joins it, with
//! everything that happens in it handed to the app as `CallEvent`s and every
//! picture and sound handed to whichever sink the app attaches.
//!
//! Follows web/src/live/room.ts for how the room is opened (adaptive stream,
//! dynacast, H.264, the sealed-call key, `forward` turning the connection into
//! relay-only ICE) and web/src/live/delay.ts for the cushion on shared screens.
//! What is published, and how, is in `publish`; the remote sinks are in
//! `remote`.

mod freeze;
mod lossless;
mod meter;
mod publish;
mod remote;
mod strain;
mod types;
#[cfg(target_os = "windows")]
mod panel;
#[cfg(target_os = "android")]
mod window;

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use livekit::{
    e2ee::{
        key_provider::{KeyProvider, KeyProviderOptions},
        E2eeOptions, EncryptionType,
    },
    prelude::*,
    webrtc::{
        native::frame_cryptor::{EncryptionState, KeyDerivationAlgorithm},
        peer_connection_factory::{IceServer as RtcIceServer, IceTransportsType},
    },
};
use tokio::sync::mpsc::UnboundedReceiver;

pub use lossless::LosslessSink;
pub use publish::{AudioInput, VideoInput, VideoKind};
pub use remote::{AudioSink, PcmSink, RemoteFrame, VideoSink};
pub use types::*;
#[cfg(any(target_os = "android", target_os = "windows"))]
pub use remote::PictureShape;

use crate::runtime::on_core;

/// What the app implements to hear about the call.
#[uniffi::export(with_foreign)]
pub trait CallObserver: Send + Sync {
    fn on_event(&self, event: CallEvent);
}

/// The least time a shared screen and its sound are held before they are
/// played (web/src/live/delay.ts). A screen is read, not talked over, and a
/// steady picture is worth a fifth of a second; cameras and voices are never
/// delayed. Zero turns it off.
const SHARE_PLAYOUT_MS: u32 = 200;

#[derive(uniffi::Object)]
pub struct Call {
    room: Room,
    room_name: String,
    observer: Arc<dyn CallObserver>,
    sinks: remote::Sinks,
    share_playout_ms: Mutex<u32>,
    /// The size each picture last asked for. A view usually asks before its
    /// track is subscribed, and the SDK drops a request for an unsubscribed
    /// publication; the first build showed a remote camera at its lowest rung
    /// for good because of it. The wish is kept and applied on subscription.
    wanted_sizes: Mutex<HashMap<String, (u32, u32)>>,
    freezes: freeze::Freezes,
    strain: strain::Strain,
    lossless: lossless::Lossless,
    meter: std::sync::Arc<tomoshibi_protocol::quality::ConnectionMeter>,
    /// The lossless streams this participant offers, for routing asks.
    senders: Mutex<Vec<Arc<Mutex<tomoshibi_protocol::lossless::sender::LosslessSender>>>>,
}

/// Opens the call described by `options` and starts telling `observer` about
/// it. The participants already there arrive as `ParticipantJoined` events
/// straight after.
#[uniffi::export]
pub async fn join_call(options: CallOptions, observer: Arc<dyn CallObserver>) -> Result<Arc<Call>, CallError> {
    on_core(async move { Call::open(options, observer).await }).await
}

impl Call {
    async fn open(options: CallOptions, observer: Arc<dyn CallObserver>) -> Result<Arc<Self>, CallError> {
        let mut room_options = RoomOptions::default();
        room_options.adaptive_stream = true;
        room_options.dynacast = true;
        room_options.auto_subscribe = true;

        // A sealed call, keyed exactly as web/src/live/secrecy.ts keys it: the
        // room's normalised name, a NUL, then the word. The older `e2ee` field
        // rather than `encryption`, because the newer one also encrypts the
        // data channel and no browser in the call would read a word of it.
        if let Some(word) = options.sealing_word.as_ref().filter(|w| !w.is_empty()) {
            let key = format!("{}\u{0}{}", options.room, word).into_bytes();
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
                room_options.e2ee = Some(E2eeOptions { encryption_type: EncryptionType::Gcm, key_provider: provider });
            }
        }

        // When the room is held on another relay than the one this person
        // came in through, the join answer carries a TURN server on the entry
        // relay and the web connects through it alone (room.ts ~775-786).
        if let Some(forward) = options.forward.as_ref() {
            room_options.rtc_config.ice_servers = vec![RtcIceServer {
                urls: vec![forward.url.clone()],
                username: forward.username.clone(),
                password: forward.credential.clone(),
            }];
            room_options.rtc_config.ice_transport_type = IceTransportsType::Relay;
        }

        let (room, events) = Room::connect(&options.url, &options.token, room_options)
            .await
            .map_err(|e| CallError::Connect { detail: e.to_string() })?;

        let call = Arc::new(Call {
            room,
            room_name: options.room.clone(),
            observer,
            sinks: remote::Sinks::default(),
            share_playout_ms: Mutex::new(SHARE_PLAYOUT_MS),
            wanted_sizes: Mutex::new(HashMap::new()),
            freezes: freeze::Freezes::default(),
            strain: strain::Strain::default(),
            lossless: lossless::Lossless::new(options.sealing_word.as_ref().is_some_and(|w| !w.is_empty())),
            senders: Mutex::new(Vec::new()),
            meter: tomoshibi_protocol::quality::ConnectionMeter::new(),
        });

        call.observer.on_event(CallEvent::StateChanged { state: CallState::Connected });
        for participant in call.room.remote_participants().values() {
            call.observer.on_event(CallEvent::ParticipantJoined { participant: describe(participant) });
        }
        tokio::spawn(Self::pump(Arc::downgrade(&call), events));
        tokio::spawn(freeze::keep(Arc::downgrade(&call)));
        tokio::spawn(strain::keep(Arc::downgrade(&call)));
        tokio::spawn(lossless::keep(Arc::downgrade(&call)));
        tokio::spawn(meter::keep(Arc::downgrade(&call)));
        Ok(call)
    }

    async fn pump(call: std::sync::Weak<Call>, mut events: UnboundedReceiver<RoomEvent>) {
        while let Some(event) = events.recv().await {
            let Some(call) = call.upgrade() else { return };
            let ended = matches!(event, RoomEvent::Disconnected { .. });
            call.handle(event);
            if ended {
                return;
            }
        }
    }

    fn handle(self: &Arc<Self>, event: RoomEvent) {
        let say = |event: CallEvent| self.observer.on_event(event);
        if matches!(
            event,
            RoomEvent::TrackPublished { .. }
                | RoomEvent::TrackUnpublished { .. }
                | RoomEvent::ParticipantConnected(_)
                | RoomEvent::ParticipantDisconnected(_)
        ) {
            let call = self.clone();
            tokio::spawn(async move { call.lossless_tick().await });
        }
        match event {
            RoomEvent::ParticipantConnected(p) => say(CallEvent::ParticipantJoined { participant: describe(&p) }),
            RoomEvent::ParticipantDisconnected(p) => {
                for sender in self.senders.lock().unwrap().iter() {
                    sender.lock().unwrap().on_left(p.identity().as_str());
                }
                say(CallEvent::ParticipantLeft { identity: p.identity().to_string() })
            }
            RoomEvent::ParticipantNameChanged { participant, .. }
            | RoomEvent::ParticipantMetadataChanged { participant, .. } => {
                if let Participant::Remote(p) = participant {
                    say(CallEvent::ParticipantChanged { participant: describe(&p) });
                }
            }
            RoomEvent::ParticipantAttributesChanged { participant, changed_attributes: _ } => {
                say(CallEvent::AttributesChanged {
                    identity: participant.identity().to_string(),
                    attributes: participant.attributes(),
                });
            }
            RoomEvent::TrackPublished { publication, participant } => say(CallEvent::TrackPublished {
                identity: participant.identity().to_string(),
                track: describe_remote_track(&publication),
            }),
            RoomEvent::TrackUnpublished { publication, participant } => {
                self.sinks.detach(&publication.sid().to_string());
                self.wanted_sizes.lock().unwrap().remove(&publication.sid().to_string());
                self.freezes.forget(&publication.sid().to_string());
                say(CallEvent::TrackUnpublished {
                    identity: participant.identity().to_string(),
                    sid: publication.sid().to_string(),
                })
            }
            RoomEvent::TrackSubscribed { track, publication, participant } => {
                let source = publication.source();
                if matches!(source, TrackSource::Screenshare | TrackSource::ScreenshareAudio) {
                    let ms = *self.share_playout_ms.lock().unwrap();
                    let _ = track.set_playout_delay((ms > 0).then(|| ms as f64 / 1000.0));
                }
                let sid = publication.sid().to_string();
                let wanted = self.wanted_sizes.lock().unwrap().get(&sid).copied();
                let cap = if source == TrackSource::Screenshare { self.freezes.subscribed(&sid) } else { None };
                if let Some((width, height)) = freeze::combined(cap, wanted) {
                    publication.update_video_dimensions(livekit::track::TrackDimension(width, height));
                }
                self.sinks.subscribed(&publication.sid().to_string(), track);
                say(CallEvent::TrackSubscribed {
                    identity: participant.identity().to_string(),
                    track: describe_remote_track(&publication),
                })
            }
            RoomEvent::TrackUnsubscribed { publication, participant, .. } => {
                self.sinks.unsubscribed(&publication.sid().to_string());
                say(CallEvent::TrackUnsubscribed {
                    identity: participant.identity().to_string(),
                    sid: publication.sid().to_string(),
                })
            }
            RoomEvent::TrackMuted { participant, publication } => say(CallEvent::TrackMuted {
                identity: participant.identity().to_string(),
                sid: publication.sid().to_string(),
                muted: true,
            }),
            RoomEvent::TrackUnmuted { participant, publication } => say(CallEvent::TrackMuted {
                identity: participant.identity().to_string(),
                sid: publication.sid().to_string(),
                muted: false,
            }),
            RoomEvent::LocalTrackPublished { publication, .. } => {
                say(CallEvent::LocalTrackPublished { track: describe_local_track(&publication) })
            }
            RoomEvent::LocalTrackUnpublished { publication, .. } => {
                say(CallEvent::LocalTrackUnpublished { sid: publication.sid().to_string() })
            }
            RoomEvent::ActiveSpeakersChanged { speakers } => say(CallEvent::Speaking {
                identities: speakers.iter().map(|p| p.identity().to_string()).collect(),
            }),
            RoomEvent::ConnectionQualityChanged { quality, participant } => {
                // Only this participant's own verdict reaches the meter:
                // somebody else's bad connection is their news.
                if participant.identity() == self.room.local_participant().identity() {
                    self.meter.heard(meter::server_quality(quality));
                }
                say(CallEvent::Quality { identity: participant.identity().to_string(), quality: quality.into() })
            }
            RoomEvent::ChatMessage { message, participant } => {
                let (identity, name) = participant
                    .map(|p| (p.identity().to_string(), p.name()))
                    .unwrap_or_default();
                say(CallEvent::Chat {
                    id: message.id,
                    identity,
                    name,
                    text: message.message,
                    timestamp: message.timestamp,
                })
            }
            // Lossless streams are fifty packets a second and are the core's to
            // play; the app never sees them. Read here, in the room's order
            // (`lossless_heard`); only what follows from a packet waits.
            RoomEvent::DataReceived { payload, topic: Some(topic), participant: Some(from), .. }
                if topic == "lossless" || topic == "lossless-music" =>
            {
                let (outcome, room) = self.lossless_heard(from.identity().as_str(), &topic, &payload);
                let call = self.clone();
                tokio::spawn(async move { call.lossless_apply(outcome, &room).await });
            }
            RoomEvent::DataReceived { payload, topic: Some(topic), participant: Some(from), .. }
                if topic == "lossless-ask" || topic == "lossless-music-ask" =>
            {
                let now = freeze::now_ms() as u64;
                for sender in self.senders.lock().unwrap().iter() {
                    sender.lock().unwrap().on_ask(now, from.identity().as_str(), &topic, &payload);
                }
            }
            RoomEvent::DataReceived { payload, topic, participant, .. } => say(CallEvent::Data {
                topic: topic.unwrap_or_default(),
                identity: participant.map(|p| p.identity().to_string()),
                payload: payload.to_vec(),
            }),
            RoomEvent::E2eeStateChanged { participant, state } => {
                let readable = match state {
                    EncryptionState::Ok | EncryptionState::KeyRatcheted => Some(true),
                    EncryptionState::DecryptionFailed | EncryptionState::MissingKey => Some(false),
                    _ => None,
                };
                if let Some(readable) = readable {
                    say(CallEvent::Sealed { identity: participant.identity().to_string(), readable });
                }
            }
            RoomEvent::Reconnecting => say(CallEvent::StateChanged { state: CallState::Reconnecting }),
            RoomEvent::Reconnected => say(CallEvent::StateChanged { state: CallState::Connected }),
            RoomEvent::Disconnected { reason } => {
                self.sinks.detach_all();
                say(CallEvent::StateChanged { state: CallState::Disconnected });
                say(CallEvent::Ended { reason: format!("{reason:?}") });
            }
            _ => {}
        }
    }

    fn publication(&self, sid: &str) -> Option<RemoteTrackPublication> {
        self.room
            .remote_participants()
            .values()
            .find_map(|p| p.track_publications().into_iter().find(|(s, _)| s.as_str() == sid).map(|(_, p)| p))
    }
}

#[uniffi::export]
impl Call {
    /// The current join token, refreshed by the server while the call lasts.
    /// The host controls authorise with it, and unlike the web client's copy
    /// it does not go stale five minutes in (host.go `whoIs`).
    pub fn token(&self) -> String {
        self.room.token()
    }

    pub fn room_name(&self) -> String {
        self.room_name.clone()
    }

    pub fn local_identity(&self) -> String {
        self.room.local_participant().identity().to_string()
    }

    pub fn local_name(&self) -> String {
        self.room.local_participant().name()
    }

    pub fn participants(&self) -> Vec<ParticipantInfo> {
        self.room.remote_participants().values().map(describe).collect()
    }

    /// Says something in the call's chat, as the web client does: the old
    /// chat packet, which every browser in the room shows.
    pub async fn send_chat(self: Arc<Self>, text: String) -> Result<(), CallError> {
        on_core(async move {
            self.room
                .local_participant()
                .send_chat_message(text, None, None)
                .await
                .map(|_| ())
                .map_err(|e| CallError::Failed { detail: e.to_string() })
        })
        .await
    }

    /// Sends a data packet: `destinations` empty means everybody. The watch
    /// and song-desk protocols and reactions all go this way.
    pub async fn send_data(
        self: Arc<Self>,
        topic: String,
        payload: Vec<u8>,
        destinations: Vec<String>,
        reliable: bool,
    ) -> Result<(), CallError> {
        on_core(async move {
            let packet = DataPacket {
                payload,
                topic: Some(topic),
                reliable,
                destination_identities: destinations.into_iter().map(ParticipantIdentity).collect(),
                ..Default::default()
            };
            self.room
                .local_participant()
                .publish_data(packet)
                .await
                .map_err(|e| CallError::Failed { detail: e.to_string() })
        })
        .await
    }

    /// Replaces this participant's attributes with `attributes` merged over
    /// the current ones: `hand` and `watching` (web/src/live/hands.ts,
    /// watching.ts).
    pub async fn set_attributes(self: Arc<Self>, attributes: HashMap<String, String>) -> Result<(), CallError> {
        on_core(async move {
            let mut merged = self.room.local_participant().attributes();
            merged.extend(attributes);
            self.room
                .local_participant()
                .set_attributes(merged)
                .await
                .map_err(|e| CallError::Failed { detail: e.to_string() })
        })
        .await
    }

    /// How long shared screens are held before they are played, from now on
    /// and for those already arriving. Zero turns the cushion off.
    pub fn set_share_playout_ms(&self, ms: u32) {
        let _core = crate::runtime::enter();
        let ms = ms.min(1000);
        *self.share_playout_ms.lock().unwrap() = ms;
        for participant in self.room.remote_participants().values() {
            for publication in participant.track_publications().values() {
                if matches!(publication.source(), TrackSource::Screenshare | TrackSource::ScreenshareAudio) {
                    if let Some(track) = publication.track() {
                        let _ = track.set_playout_delay((ms > 0).then(|| ms as f64 / 1000.0));
                    }
                }
            }
        }
    }

    /// Stops a remote track at the server (the web's block, hearing.ts) or
    /// lets it flow again.
    pub fn set_remote_enabled(&self, sid: String, enabled: bool) -> Result<(), CallError> {
        let _core = crate::runtime::enter();
        let publication = self.publication(&sid).ok_or(CallError::NoSuchTrack { sid })?;
        publication.set_enabled(enabled);
        Ok(())
    }

    /// Asks the server for the layer of a remote video that answers this size
    /// in device pixels: the picture's size on screen, or a lower rung while a
    /// freeze is being sat out.
    pub fn request_video_size(&self, sid: String, width: u32, height: u32) -> Result<(), CallError> {
        let _core = crate::runtime::enter();
        self.wanted_sizes.lock().unwrap().insert(sid.clone(), (width, height));
        let cap = self.freezes.capped(&sid);
        let publication = self.publication(&sid).ok_or(CallError::NoSuchTrack { sid })?;
        if let Some((width, height)) = freeze::combined(cap, Some((width, height))) {
            publication.update_video_dimensions(livekit::track::TrackDimension(width, height));
        }
        Ok(())
    }

    /// The person pressed the uplink card's button. `Apply` carries the
    /// setting to restart the share at; say how that went with
    /// `strain_accepted`.
    pub fn strain_accept(&self) -> tomoshibi_protocol::strain::StrainAccept {
        let _core = crate::runtime::enter();
        self.strain.watch.accept(self.strain.running())
    }

    pub fn strain_accepted(&self, took: bool) -> bool {
        let _core = crate::runtime::enter();
        self.strain.watch.accepted(took)
    }

    /// The person put the card away for this share.
    pub fn strain_decline(&self) {
        let _core = crate::runtime::enter();
        self.strain.watch.decline()
    }

    /// Where lossless streams are played.
    pub fn set_lossless_sink(&self, sink: Arc<dyn LosslessSink>) {
        self.lossless.set_sink(sink);
    }

    /// The listener's own setting for one of a person's sounds (hearing.ts):
    /// a level from 0 to 1 and whether it is blocked. Screen sound and music
    /// tracks are switched at the server here, together with the lossless
    /// streams that may be standing in for them; the app turns the level.
    pub async fn set_hearing(self: Arc<Self>, identity: String, music: bool, level: f32, blocked: bool) {
        on_core(async move {
            self.lossless.settings.lock().unwrap().levels_set(identity, music, level, blocked);
            self.lossless_tick().await;
        })
        .await
    }

    /// Whether the app is in the background: readings taken there are not
    /// evidence of a freeze (the web skips a hidden tab for the same reason).
    pub fn set_background(&self, background: bool) {
        *self.freezes.background.lock().unwrap() = background;
    }

    /// Sends this remote video's frames to `sink` from now until it is
    /// detached, replaced or the track goes away.
    pub fn attach_video(&self, sid: String, sink: Arc<dyn VideoSink>) {
        let _core = crate::runtime::enter();
        let track = self.publication(&sid).and_then(|p| p.track());
        self.sinks.attach_video(&sid, sink, track);
    }

    /// Sends this remote sound to `sink` as 48 kHz PCM, in the channel count
    /// it was published with.
    pub fn attach_audio(&self, sid: String, sink: Arc<dyn AudioSink>) {
        let _core = crate::runtime::enter();
        let publication = self.publication(&sid);
        let channels = sound_channels(publication.as_ref());
        let track = publication.and_then(|p| p.track());
        self.sinks.attach_audio(&sid, sink, track, channels);
    }

    /// `attach_audio` with the samples as bytes (`PcmSink`).
    pub fn attach_pcm(&self, sid: String, sink: Arc<dyn PcmSink>) {
        let _core = crate::runtime::enter();
        let publication = self.publication(&sid);
        let channels = sound_channels(publication.as_ref());
        let track = publication.and_then(|p| p.track());
        self.sinks.attach_pcm(&sid, sink, track, channels);
    }

    pub fn detach(&self, sid: String) {
        let _core = crate::runtime::enter();
        self.sinks.detach(&sid);
    }

    pub async fn leave(self: Arc<Self>) {
        on_core(async move {
            self.sinks.detach_all();
            let _ = self.room.close().await;
        })
        .await
    }
}

fn describe(participant: &RemoteParticipant) -> ParticipantInfo {
    ParticipantInfo {
        identity: participant.identity().to_string(),
        name: participant.name(),
        attributes: participant.attributes(),
        tracks: participant.track_publications().values().map(describe_remote_track).collect(),
    }
}

fn describe_remote_track(publication: &RemoteTrackPublication) -> TrackInfo {
    let livekit::track::TrackDimension(width, height) = publication.dimension();
    TrackInfo {
        sid: publication.sid().to_string(),
        kind: match publication.kind() {
            TrackKind::Audio => MediaKind::Audio,
            TrackKind::Video => MediaKind::Video,
        },
        source: media_source(publication.source()),
        name: publication.name(),
        muted: publication.is_muted(),
        width,
        height,
        simulcast: publication.simulcasted(),
        subscribed: publication.is_subscribed(),
    }
}

fn describe_local_track(publication: &LocalTrackPublication) -> TrackInfo {
    let livekit::track::TrackDimension(width, height) = publication.dimension();
    TrackInfo {
        sid: publication.sid().to_string(),
        kind: match publication.kind() {
            TrackKind::Audio => MediaKind::Audio,
            TrackKind::Video => MediaKind::Video,
        },
        source: media_source(publication.source()),
        name: publication.name(),
        muted: publication.is_muted(),
        width,
        height,
        simulcast: publication.simulcasted(),
        subscribed: false,
    }
}

/// A screen's sound and the music are stereo; a voice is mono.
fn sound_channels(publication: Option<&RemoteTrackPublication>) -> u32 {
    match publication.map(|p| p.source()) {
        Some(TrackSource::ScreenshareAudio) | Some(TrackSource::Unknown) => 2,
        _ => 1,
    }
}

#[cfg(target_os = "android")]
#[uniffi::export]
impl Call {
    /// Draws this remote video into `window`, an ANativeWindow pointer from
    /// `CoreNative.windowFromSurface`, beside any other window it is already
    /// drawn into. The core takes over the reference and releases it when the
    /// window is detached or the call ends; `shape` hears the picture's size
    /// and turn whenever they change.
    pub fn attach_video_window(&self, sid: String, window: u64, shape: Arc<dyn PictureShape>) {
        let _core = crate::runtime::enter();
        if window == 0 {
            return;
        }
        let track = self.publication(&sid).and_then(|p| p.track());
        self.sinks.attach_window(&sid, window::Window::new(window, shape), track);
    }

    /// Stops drawing into one window, releasing it.
    pub fn detach_video_window(&self, sid: String, window: u64) {
        let _core = crate::runtime::enter();
        self.sinks.detach_window(&sid, window);
    }
}

#[cfg(target_os = "windows")]
#[uniffi::export]
impl Call {
    /// Draws this remote video into a new swap chain, beside any other panel
    /// it is already drawn into, and returns that swap chain: an
    /// IDXGISwapChain1 pointer with one reference that is the app's, to hand
    /// to `ISwapChainPanelNative::SetSwapChain` on the UI thread and then
    /// release. The value also names the panel to the calls below. Zero when
    /// no Direct3D device could be made. `shape` hears the picture's size and
    /// turn whenever they change, after the frame is drawn and with nothing
    /// in the core locked.
    pub fn attach_video_panel(&self, sid: String, shape: Arc<dyn PictureShape>) -> u64 {
        let _core = crate::runtime::enter();
        let Some(panel) = panel::Panel::new(shape) else { return 0 };
        let handed = panel.hand_over();
        let track = self.publication(&sid).and_then(|p| p.track());
        self.sinks.attach_panel(&sid, panel, track);
        handed
    }

    /// Where the picture sits: its panel's size in device-independent pixels,
    /// and whether it fills the panel, cut to its shape, or fits inside it.
    /// Takes effect with the next frame.
    pub fn place_video_panel(&self, sid: String, panel: u64, width: f32, height: f32, fill: bool) {
        if let Some(panels) = self.sinks.panels(&sid) {
            panels.place(panel, width, height, fill);
        }
    }

    /// How many frames have been drawn into this panel, for the app to tell
    /// a picture that moves from one that froze.
    pub fn video_panel_frames(&self, sid: String, panel: u64) -> u64 {
        self.sinks.panels(&sid).map_or(0, |panels| panels.frames(panel))
    }

    /// A few dozen pixels of the last picture drawn for this video, upright,
    /// its longer side `longest` pixels: what the app blurs behind a camera
    /// it shows whole (`panel::Glimpse`). None before the first frame, or
    /// when the video is not drawn into any panel.
    pub fn video_panel_glimpse(&self, sid: String, longest: u32) -> Option<panel::Glimpse> {
        self.sinks.panels(&sid).and_then(|panels| panels.glimpse(longest))
    }

    /// Stops drawing into one panel and lets its swap chain go; the app
    /// clears the panel's swap chain first.
    pub fn detach_video_panel(&self, sid: String, panel: u64) {
        let _core = crate::runtime::enter();
        self.sinks.detach_panel(&sid, panel);
    }
}
