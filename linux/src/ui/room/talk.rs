//! What the room knows about the call, as the screens read it: who is
//! there with which tracks, who is speaking, the messages, the hands, the
//! connection (the Mac's CallModel, Call/CallModel.swift `apply`).
//!
//! Nothing here touches GTK or the core's `Call`. Events go in through
//! [`Talk::apply`] on the main thread, which says what the widgets and the
//! devices have to do about each as [`Effect`]s, so that the rules (a
//! person's tracks merged rather than replaced, a share announced only when
//! it is news, a message counted unread only while the messages are closed)
//! are tested without a window or a call.

use std::collections::{BTreeMap, HashMap, HashSet};

use tomoshibi_core::call::{
    CallEvent, CallState, LinkQuality, MediaKind, MediaSource, ParticipantInfo, TrackInfo,
};
use tomoshibi_protocol::{hands, quality::ConnectionReading, strain::StrainHint};

use super::plan::SpeakingRecord;
use crate::i18n::{t, t_with};

/// This participant's own camera picture, and their own share.
pub const ME: &str = "me/camera";
pub const MY_SHARE: &str = "me/screen";

/// The web keeps the last 500 messages (chat.ts).
const KEPT_MESSAGES: usize = 500;

/// One person in the call.
#[derive(Debug, Clone, PartialEq)]
pub struct Person {
    pub id: String,
    pub name: String,
    pub attributes: HashMap<String, String>,
    /// By sid, in a stable order.
    pub tracks: BTreeMap<String, TrackInfo>,
    pub quality: LinkQuality,
    pub speaking: bool,
    /// Whether their sealed frames can be read; `None` until the call says.
    pub readable: Option<bool>,
}

impl Person {
    fn of(participant: &ParticipantInfo) -> Person {
        Person {
            id: participant.identity.clone(),
            name: participant.name.clone(),
            attributes: participant.attributes.clone(),
            tracks: participant
                .tracks
                .iter()
                .map(|track| (track.sid.clone(), track.clone()))
                .collect(),
            quality: LinkQuality::Good,
            speaking: false,
            readable: None,
        }
    }

    /// What a person is called in a notice: their name, or their identity
    /// when they gave none (notices.ts `named`).
    pub fn label(&self) -> String {
        named(&self.name, &self.id)
    }

    pub fn hand_up(&self) -> bool {
        hands::hand_is_up(self.attributes.clone())
    }

    pub fn track(&self, source: MediaSource) -> Option<&TrackInfo> {
        self.tracks.values().find(|track| track.source == source)
    }

    /// The song desk's music: a track of no known source named "music" or
    /// "music-lossless" (lossless.ts).
    pub fn music_track(&self) -> Option<&TrackInfo> {
        self.tracks
            .values()
            .find(|track| track.source == MediaSource::Unknown && track.name.starts_with("music"))
    }

    /// Their microphone is off, or they have none (Tile.tsx `muted`).
    pub fn microphone_off(&self) -> bool {
        self.track(MediaSource::Microphone)
            .is_none_or(|track| track.muted)
    }

    /// Their camera's track while it is on.
    pub fn camera_sid(&self) -> Option<String> {
        self.track(MediaSource::Camera)
            .filter(|track| !track.muted)
            .map(|track| track.sid.clone())
    }

    pub fn screen_sid(&self) -> Option<String> {
        self.track(MediaSource::ScreenShare)
            .map(|track| track.sid.clone())
    }
}

pub fn named(name: &str, identity: &str) -> String {
    if name.trim().is_empty() {
        identity.to_owned()
    } else {
        name.to_owned()
    }
}

/// What a picture shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PictureKind {
    Camera,
    Screen,
}

/// The unit of layout: a picture, not a person (the web's `Surface`).
/// Someone sharing their screen with their camera on is two of these.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Picture {
    /// `<identity>/camera`, `<identity>/screen`, or [`ME`] and [`MY_SHARE`].
    pub id: String,
    /// Whose it is; this participant's own identity for their own.
    pub identity: String,
    pub kind: PictureKind,
    /// The track drawn in it, when there is one and it is not muted.
    pub sid: Option<String>,
    pub mine: bool,
}

/// One message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    pub id: String,
    pub identity: String,
    pub name: String,
    pub text: String,
    /// Milliseconds since the epoch.
    pub at: i64,
    pub mine: bool,
}

/// How a notice in the corner is edged (Notices.tsx: "the edge is the whole
/// of the severity system").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastKind {
    Info,
    /// Somebody else taking the stage, anything else happening now.
    Signal,
    Failure,
}

/// One thing said in the corner (notices.ts).
#[derive(Debug, Clone, PartialEq)]
pub struct Toast {
    pub title: String,
    pub detail: Option<String>,
    pub kind: ToastKind,
    /// How long it stays, in milliseconds; `None` until closed.
    pub lasts_ms: Option<u64>,
    pub closable: bool,
    /// Said again under the same key, it replaces the one there.
    pub key: Option<String>,
}

impl Toast {
    pub fn said(title: impl Into<String>) -> Toast {
        Toast {
            title: title.into(),
            detail: None,
            kind: ToastKind::Info,
            lasts_ms: Some(NOTICE_MOMENT_MS),
            closable: false,
            key: None,
        }
    }

    pub fn failure(title: impl Into<String>) -> Toast {
        Toast {
            kind: ToastKind::Failure,
            lasts_ms: Some(4000),
            ..Toast::said(title)
        }
    }
}

/// notices.ts `aMoment`: how long something that happened is said.
pub const NOTICE_MOMENT_MS: u64 = 3200;
/// notices.ts `BACK_WITHIN`: how long a leave is held before it is said, in
/// case it is somebody coming straight back (a reload, a reconnect).
pub const BACK_WITHIN_MS: u64 = 5000;
/// App.tsx `MOVING_HOLDS_FOR`: how long a server's word that the meeting is
/// moving makes the end of the call a move rather than an ending.
pub const MOVING_HOLDS_FOR_S: f64 = 15.0;
/// How long a message said while the messages are closed stays in the
/// corner (the Mac's `saidFor`, Said.tsx).
pub const SAID_FOR_MS: u64 = 6000;

/// What the widgets and the devices do about an event.
#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    /// A remote sound to play, as `identity`'s.
    Play {
        sid: String,
        identity: String,
    },
    StopPlaying {
        sid: String,
    },
    /// Something to say in the corner.
    Toast(Toast),
    /// A message said while the messages are closed, for the corner.
    Said(Line),
    /// A reaction to float over `from`'s picture.
    Reaction {
        from: String,
        what: String,
    },
    /// Whoever runs the room may have changed: ask again.
    Standing,
    /// Somebody left; whether to say so is asked again
    /// ([`Talk::leave_due`]) once [`BACK_WITHIN_MS`] has passed.
    LeaveLater {
        identity: String,
    },
    /// The server says the meeting is about to move to `to` (the relay's
    /// name, or nothing where it named none).
    Moving {
        to: String,
    },
    /// A sound arrived: the volumes chosen for it are put back.
    Hearing,
    /// The call is over, with LiveKit's word for why.
    Ended {
        reason: String,
    },
}

/// The call as the room knows it.
#[derive(Debug, Clone)]
pub struct Talk {
    pub room: String,
    pub local_identity: String,
    pub local_name: String,
    pub state: CallState,
    pub people: Vec<Person>,
    pub messages: Vec<Line>,
    pub unread: bool,
    /// Whether the messages are open, so that a message is not unread.
    pub chat_open: bool,
    pub hand_up: bool,
    /// Whether this participant is speaking, from the same event as
    /// everybody else (the web marks your own picture as it marks theirs).
    pub me_speaking: bool,
    pub speaking_record: SpeakingRecord,
    pub local_quality: LinkQuality,
    pub connection: Option<ConnectionReading>,
    pub strain: Option<StrainHint>,
    pub local_tracks: BTreeMap<String, TrackInfo>,
    /// Where a move by the host is taking the meeting, and when it was said.
    pub moving: Option<(String, f64)>,
    /// Until the join has told who was already here, nobody arriving is
    /// announced (CallModel `settled`).
    pub settled: bool,
    /// Leaves not yet said, by identity, with the name to say: somebody
    /// back within [`BACK_WITHIN_MS`] is neither said to have left nor to
    /// have joined (notices.ts `leaving`).
    pub leaving: HashMap<String, String>,
    /// When this participant's own connection was last made (seconds, as
    /// `now`): everybody the core said had left while it was being made
    /// again is waited for from then, not from when they were said to go.
    pub connected_at: f64,
}

/// What [`Talk::leave_due`] makes of a leave being held.
#[derive(Debug, Clone, PartialEq)]
pub enum LeaveDue {
    /// Gone for good: say so.
    Say(Toast),
    /// Ask again in this many milliseconds.
    Wait(u64),
    /// Back, or already said: nothing.
    Nothing,
}

impl Talk {
    pub fn new(room: &str, local_identity: &str, local_name: &str) -> Talk {
        Talk {
            room: room.to_owned(),
            local_identity: local_identity.to_owned(),
            local_name: local_name.to_owned(),
            state: CallState::Connected,
            people: Vec::new(),
            messages: Vec::new(),
            unread: false,
            chat_open: false,
            hand_up: false,
            me_speaking: false,
            speaking_record: SpeakingRecord::default(),
            local_quality: LinkQuality::Good,
            connection: None,
            strain: None,
            local_tracks: BTreeMap::new(),
            moving: None,
            settled: false,
            leaving: HashMap::new(),
            connected_at: 0.0,
        }
    }

    /// Whether to say that `identity` left, asked once their leave has been
    /// held [`BACK_WITHIN_MS`].
    ///
    /// A leave seen while this participant's own connection was being made
    /// again is not news about them: the core says everybody left when it
    /// reconnects and joined again when it is back, and "Web One left"
    /// appeared for somebody who had not moved (the network tests of
    /// 2026-11-01). So it waits until the connection has been back as long
    /// as any leave is held, and is said only if they are still gone.
    pub fn leave_due(&mut self, identity: &str, now: f64) -> LeaveDue {
        if !self.leaving.contains_key(identity) {
            return LeaveDue::Nothing;
        }
        if self.person(identity).is_some() {
            self.leaving.remove(identity);
            return LeaveDue::Nothing;
        }
        if self.state != CallState::Connected {
            return LeaveDue::Wait(BACK_WITHIN_MS);
        }
        let back_for_ms = ((now - self.connected_at).max(0.0) * 1000.0) as u64;
        if back_for_ms < BACK_WITHIN_MS {
            return LeaveDue::Wait(BACK_WITHIN_MS - back_for_ms);
        }
        let name = self.leaving.remove(identity).unwrap_or_default();
        LeaveDue::Say(Toast::said(t_with("{name} left", &[("name", &name)])))
    }

    /// Where the meeting went, when the call ending for `reason` at `now` is
    /// a move rather than an ending: the room closed (a move) or this
    /// participant put out of it (`placing`, one person moved) within
    /// [`MOVING_HOLDS_FOR_S`] of the server saying so (App.tsx `ended`, the
    /// Mac's CallModel). LiveKit's Rust SDK names the room's closing
    /// `RoomDeleted`, the web's `RoomClosed`; both are read.
    pub fn moved_to(&self, reason: &str, now: f64) -> Option<String> {
        let (to, at) = self.moving.as_ref()?;
        let gone = ["RoomDeleted", "RoomClosed", "ParticipantRemoved"]
            .iter()
            .any(|word| reason.contains(word));
        (gone && now - at < MOVING_HOLDS_FOR_S).then(|| to.clone())
    }

    pub fn person(&self, identity: &str) -> Option<&Person> {
        self.people.iter().find(|person| person.id == identity)
    }

    fn person_mut(&mut self, identity: &str) -> Option<&mut Person> {
        self.people.iter_mut().find(|person| person.id == identity)
    }

    pub fn identities(&self) -> Vec<String> {
        self.people.iter().map(|person| person.id.clone()).collect()
    }

    /// The people already here when the call was joined, before any event.
    pub fn arrive(&mut self, already: &[ParticipantInfo]) {
        for participant in already {
            self.upsert(participant);
        }
    }

    /// Everybody in the call, this participant included.
    pub fn count(&self) -> usize {
        self.people.len() + 1
    }

    /// The opened messages read everything there was.
    pub fn set_chat_open(&mut self, open: bool) {
        self.chat_open = open;
        if open {
            self.unread = false;
        }
    }

    /// A message this participant sent, once the media server has it: data
    /// does not come back to whoever sent it.
    pub fn sent(&mut self, text: &str, at: i64, id: String) {
        self.messages.push(Line {
            id,
            identity: self.local_identity.clone(),
            name: self.local_name.clone(),
            text: text.to_owned(),
            at,
            mine: true,
        });
        self.trim_messages();
    }

    fn trim_messages(&mut self) {
        if self.messages.len() > KEPT_MESSAGES {
            let over = self.messages.len() - KEPT_MESSAGES;
            self.messages.drain(..over);
        }
    }

    /// Every picture, in the order the web lists them (surface.ts
    /// `surfaces`): this participant's own camera, their share, then each
    /// person's camera and screen together. Once there are more people than
    /// a page holds, whoever spoke last comes forward (useSpeakingOrder.ts);
    /// below that the order holds still, because a grid that rearranges
    /// itself whenever somebody speaks is worse than one that does not.
    pub fn pictures(&self, my_camera: bool, my_share: bool) -> Vec<Picture> {
        let mut out = vec![Picture {
            id: ME.to_owned(),
            identity: self.local_identity.clone(),
            kind: PictureKind::Camera,
            sid: my_camera.then(String::new),
            mine: true,
        }];
        if my_share {
            out.push(Picture {
                id: MY_SHARE.to_owned(),
                identity: self.local_identity.clone(),
                kind: PictureKind::Screen,
                sid: None,
                mine: true,
            });
        }
        let crowded = self.count() > super::plan::TILES_PER_PAGE;
        let order = if crowded {
            self.speaking_record.order(&self.identities())
        } else {
            self.identities()
        };
        for identity in order {
            let Some(person) = self.person(&identity) else {
                continue;
            };
            out.push(Picture {
                id: format!("{}/camera", person.id),
                identity: person.id.clone(),
                kind: PictureKind::Camera,
                sid: person.camera_sid(),
                mine: false,
            });
            if let Some(screen) = person.screen_sid() {
                out.push(Picture {
                    id: format!("{}/screen", person.id),
                    identity: person.id.clone(),
                    kind: PictureKind::Screen,
                    sid: Some(screen),
                    mine: false,
                });
            }
        }
        out
    }

    /// What takes the stage by itself when it starts (usePin.ts): somebody
    /// else's shared screen. Not this participant's own, which has no
    /// picture here to put there (the Mac's reasoning, RoomView `shares`).
    pub fn shares(&self) -> Vec<String> {
        self.people
            .iter()
            .filter(|person| person.screen_sid().is_some())
            .map(|person| format!("{}/screen", person.id))
            .collect()
    }

    /// Who has their hand up, this participant first, then in the order they
    /// joined (hands.ts `raisedHands` reads the roster in order).
    pub fn raised_hands(&self) -> Vec<String> {
        let mut out = Vec::new();
        if self.hand_up {
            out.push(self.local_identity.clone());
        }
        out.extend(
            self.people
                .iter()
                .filter(|person| person.hand_up())
                .map(|person| person.id.clone()),
        );
        out
    }

    fn upsert(&mut self, participant: &ParticipantInfo) {
        if let Some(person) = self.person_mut(&participant.identity) {
            person.name = participant.name.clone();
            person.attributes = participant.attributes.clone();
            for track in &participant.tracks {
                person.tracks.insert(track.sid.clone(), track.clone());
            }
        } else {
            self.people.push(Person::of(participant));
        }
    }

    /// One event of the call, and what is to be done about it. `now` is in
    /// seconds, for the speaking record and a move's age.
    pub fn apply(&mut self, event: CallEvent, now: f64) -> Vec<Effect> {
        let mut effects = Vec::new();
        match event {
            CallEvent::StateChanged { state } => {
                if state == CallState::Connected && self.state != CallState::Connected {
                    self.connected_at = now;
                }
                self.state = state;
                effects.push(Effect::Standing);
            }
            CallEvent::Ended { reason } => effects.push(Effect::Ended { reason }),
            CallEvent::ParticipantJoined { participant } => {
                let arriving = self.person(&participant.identity).is_none();
                // Back before their leave was said: neither half is
                // (notices.ts `onJoin`).
                let back = self.leaving.remove(&participant.identity).is_some();
                self.upsert(&participant);
                if arriving && self.settled && !back {
                    effects.push(Effect::Toast(Toast::said(t_with(
                        "{name} joined",
                        &[("name", &named(&participant.name, &participant.identity))],
                    ))));
                }
                effects.push(Effect::Hearing);
            }
            CallEvent::ParticipantChanged { participant } => self.upsert(&participant),
            CallEvent::ParticipantLeft { identity } => {
                if let Some(person) = self.person(&identity) {
                    for sid in person.tracks.keys() {
                        effects.push(Effect::StopPlaying { sid: sid.clone() });
                    }
                    // Held rather than said, in case they are back at once.
                    if self.settled {
                        self.leaving.insert(identity.clone(), person.label());
                        effects.push(Effect::LeaveLater {
                            identity: identity.clone(),
                        });
                    }
                }
                self.people.retain(|person| person.id != identity);
                self.speaking_record.hear(&[], &self.identities(), now);
                effects.push(Effect::Standing);
            }
            CallEvent::TrackPublished { identity, track } => {
                let news = self.settled
                    && self
                        .person(&identity)
                        .is_some_and(|person| !person.tracks.contains_key(&track.sid));
                if news {
                    if let Some(person) = self.person(&identity) {
                        if let Some(toast) = share_news(&track, &person.label()) {
                            effects.push(Effect::Toast(toast));
                        }
                    }
                }
                if let Some(person) = self.person_mut(&identity) {
                    person.tracks.insert(track.sid.clone(), track);
                }
                effects.push(Effect::Hearing);
            }
            CallEvent::TrackSubscribed { identity, track } => {
                if track.kind == MediaKind::Audio {
                    effects.push(Effect::Play {
                        sid: track.sid.clone(),
                        identity: identity.clone(),
                    });
                }
                if let Some(person) = self.person_mut(&identity) {
                    person.tracks.insert(track.sid.clone(), track);
                }
                effects.push(Effect::Hearing);
            }
            CallEvent::TrackUnpublished { identity, sid } => {
                effects.push(Effect::StopPlaying { sid: sid.clone() });
                if let Some(person) = self.person_mut(&identity) {
                    person.tracks.remove(&sid);
                }
            }
            CallEvent::TrackUnsubscribed { identity, sid } => {
                effects.push(Effect::StopPlaying { sid: sid.clone() });
                if let Some(track) = self
                    .person_mut(&identity)
                    .and_then(|person| person.tracks.get_mut(&sid))
                {
                    track.subscribed = false;
                }
            }
            CallEvent::TrackMuted {
                identity,
                sid,
                muted,
            } => {
                if let Some(track) = self
                    .person_mut(&identity)
                    .and_then(|person| person.tracks.get_mut(&sid))
                {
                    track.muted = muted;
                } else if identity == self.local_identity {
                    if let Some(track) = self.local_tracks.get_mut(&sid) {
                        track.muted = muted;
                    }
                }
            }
            CallEvent::LocalTrackPublished { track } => {
                self.local_tracks.insert(track.sid.clone(), track);
            }
            CallEvent::LocalTrackUnpublished { sid } => {
                self.local_tracks.remove(&sid);
            }
            CallEvent::Speaking { identities } => {
                let now_speaking: HashSet<&String> = identities.iter().collect();
                for person in &mut self.people {
                    person.speaking = now_speaking.contains(&person.id);
                }
                self.me_speaking = now_speaking.contains(&self.local_identity);
                let present = self.identities();
                self.speaking_record.hear(&identities, &present, now);
            }
            CallEvent::Quality { identity, quality } => {
                if identity == self.local_identity {
                    self.local_quality = quality;
                }
                if let Some(person) = self.person_mut(&identity) {
                    person.quality = quality;
                }
            }
            CallEvent::Chat {
                id,
                identity,
                name,
                text,
                timestamp,
            } => {
                let line = Line {
                    id,
                    identity,
                    name,
                    text,
                    at: timestamp,
                    mine: false,
                };
                self.messages.push(line.clone());
                self.trim_messages();
                if !self.chat_open {
                    self.unread = true;
                    effects.push(Effect::Said(line));
                }
            }
            CallEvent::Data {
                topic,
                identity,
                payload,
            } => {
                // Server messages come with no sender, and only those are
                // believed (host.ts, App.tsx).
                match identity {
                    None if topic == "moving" || topic == "placing" => {
                        let to = String::from_utf8_lossy(&payload).trim().to_owned();
                        self.moving = Some((to.clone(), now));
                        effects.push(Effect::Moving { to });
                    }
                    None if topic == "host" => effects.push(Effect::Standing),
                    Some(from) => {
                        if let Some(what) =
                            hands::received_reaction(Some(topic), Some(from.clone()), payload)
                        {
                            effects.push(Effect::Reaction { from, what });
                        }
                    }
                    None => {}
                }
            }
            CallEvent::AttributesChanged {
                identity,
                attributes,
            } => {
                if identity == self.local_identity {
                    self.hand_up = hands::hand_is_up(attributes);
                } else if let Some(person) = self.person_mut(&identity) {
                    person.attributes = attributes;
                }
            }
            CallEvent::Sealed { identity, readable } => {
                if let Some(person) = self.person_mut(&identity) {
                    person.readable = Some(readable);
                }
            }
            CallEvent::Strain { hint } => self.strain = hint,
            CallEvent::Connection { reading } => self.connection = Some(reading),
            CallEvent::LosslessGaveUp => {
                // Said to the person sharing, because it is their upload
                // that could not keep up (notices.ts `losslessGaveUp`).
                effects.push(Effect::Toast(Toast {
                    detail: Some(t(
                        "Your connection could not keep up. Everyone still hears the sound in real time.",
                    )),
                    lasts_ms: Some(12_000),
                    closable: true,
                    ..Toast::said(t("Lossless sound stopped"))
                }));
            }
        }
        effects
    }
}

/// Somebody else taking the stage with a share, or starting sound on its
/// own, which has no picture to announce it (notices.ts `onPublished`).
fn share_news(track: &TrackInfo, name: &str) -> Option<Toast> {
    let sound_only = track.source == MediaSource::ScreenShareAudio
        && (track.name == "listening" || track.name == "listening-lossless");
    if track.source != MediaSource::ScreenShare && !sound_only {
        return None;
    }
    let title = if sound_only {
        t_with("{name} is sharing sound", &[("name", &name)])
    } else {
        t_with("{name} started sharing", &[("name", &name)])
    };
    Some(Toast {
        kind: ToastKind::Signal,
        ..Toast::said(title)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(sid: &str, kind: MediaKind, source: MediaSource) -> TrackInfo {
        TrackInfo {
            sid: sid.into(),
            kind,
            source,
            name: String::new(),
            muted: false,
            width: 0,
            height: 0,
            simulcast: false,
            subscribed: false,
        }
    }

    fn participant(identity: &str, tracks: Vec<TrackInfo>) -> ParticipantInfo {
        ParticipantInfo {
            identity: identity.into(),
            name: format!("Name {identity}"),
            attributes: HashMap::new(),
            tracks,
        }
    }

    fn talk() -> Talk {
        let mut talk = Talk::new("room", "me-id", "Me");
        talk.arrive(&[participant(
            "a",
            vec![track("TR_cam", MediaKind::Video, MediaSource::Camera)],
        )]);
        talk.settled = true;
        talk
    }

    #[test]
    fn a_person_s_tracks_are_merged_not_replaced() {
        let mut talk = talk();
        talk.apply(
            CallEvent::ParticipantChanged {
                participant: participant("a", vec![]),
            },
            0.0,
        );
        assert!(
            talk.person("a").unwrap().tracks.contains_key("TR_cam"),
            "a change that lists no tracks keeps the ones there were"
        );
    }

    #[test]
    fn pictures_are_mine_first_then_each_camera_and_screen() {
        let mut talk = talk();
        talk.apply(
            CallEvent::TrackPublished {
                identity: "a".into(),
                track: track("TR_screen", MediaKind::Video, MediaSource::ScreenShare),
            },
            0.0,
        );
        let ids: Vec<String> = talk
            .pictures(false, false)
            .into_iter()
            .map(|picture| picture.id)
            .collect();
        assert_eq!(ids, vec![ME, "a/camera", "a/screen"]);
        assert_eq!(talk.shares(), vec!["a/screen".to_owned()]);
    }

    #[test]
    fn a_muted_camera_is_a_picture_without_a_track() {
        let mut talk = talk();
        assert_eq!(
            talk.pictures(false, false)[1].sid.as_deref(),
            Some("TR_cam")
        );
        talk.apply(
            CallEvent::TrackMuted {
                identity: "a".into(),
                sid: "TR_cam".into(),
                muted: true,
            },
            0.0,
        );
        assert_eq!(talk.pictures(false, false)[1].sid, None);
    }

    #[test]
    fn a_message_is_unread_only_while_the_messages_are_closed() {
        let mut talk = talk();
        let chat = |id: &str| CallEvent::Chat {
            id: id.into(),
            identity: "a".into(),
            name: "A".into(),
            text: "hello".into(),
            timestamp: 1,
        };
        let effects = talk.apply(chat("1"), 0.0);
        assert!(talk.unread);
        assert!(matches!(effects.as_slice(), [Effect::Said(_)]));
        talk.set_chat_open(true);
        assert!(!talk.unread);
        let effects = talk.apply(chat("2"), 0.0);
        assert!(!talk.unread);
        assert!(effects.is_empty(), "nothing said in the corner while open");
        assert_eq!(talk.messages.len(), 2);
    }

    #[test]
    fn only_the_last_five_hundred_messages_are_kept() {
        let mut talk = talk();
        for n in 0..510 {
            talk.sent("x", n, n.to_string());
        }
        assert_eq!(talk.messages.len(), 500);
        assert_eq!(talk.messages[0].id, "10");
    }

    #[test]
    fn an_arrival_is_news_only_once_settled() {
        let mut talk = Talk::new("room", "me-id", "Me");
        let effects = talk.apply(
            CallEvent::ParticipantJoined {
                participant: participant("b", vec![]),
            },
            0.0,
        );
        assert!(!effects
            .iter()
            .any(|effect| matches!(effect, Effect::Toast(_))));
        talk.settled = true;
        let effects = talk.apply(
            CallEvent::ParticipantJoined {
                participant: participant("c", vec![]),
            },
            0.0,
        );
        assert!(effects.iter().any(
            |effect| matches!(effect, Effect::Toast(toast) if toast.title == "Name c joined")
        ));
    }

    #[test]
    fn a_share_running_when_somebody_arrives_is_not_news() {
        let mut talk = talk();
        let screen = track("TR_s", MediaKind::Video, MediaSource::ScreenShare);
        // Told with the person it belongs to: already among their tracks.
        talk.apply(
            CallEvent::ParticipantChanged {
                participant: participant("a", vec![screen.clone()]),
            },
            0.0,
        );
        let effects = talk.apply(
            CallEvent::TrackPublished {
                identity: "a".into(),
                track: screen,
            },
            0.0,
        );
        assert!(!effects
            .iter()
            .any(|effect| matches!(effect, Effect::Toast(_))));
        let effects = talk.apply(
            CallEvent::TrackPublished {
                identity: "a".into(),
                track: track("TR_s2", MediaKind::Video, MediaSource::ScreenShare),
            },
            0.0,
        );
        assert!(effects.iter().any(
            |effect| matches!(effect, Effect::Toast(toast) if toast.kind == ToastKind::Signal)
        ));
    }

    #[test]
    fn a_subscribed_sound_is_played_and_a_left_one_stopped() {
        let mut talk = talk();
        let effects = talk.apply(
            CallEvent::TrackSubscribed {
                identity: "a".into(),
                track: track("TR_mic", MediaKind::Audio, MediaSource::Microphone),
            },
            0.0,
        );
        assert!(effects.contains(&Effect::Play {
            sid: "TR_mic".into(),
            identity: "a".into()
        }));
        let effects = talk.apply(
            CallEvent::ParticipantLeft {
                identity: "a".into(),
            },
            0.0,
        );
        assert!(effects.contains(&Effect::StopPlaying {
            sid: "TR_mic".into()
        }));
        assert!(talk.people.is_empty());
    }

    #[test]
    fn hands_and_reactions_are_read_as_the_web_writes_them() {
        let mut talk = talk();
        talk.apply(
            CallEvent::AttributesChanged {
                identity: "a".into(),
                attributes: HashMap::from([("hand".into(), "up".into())]),
            },
            0.0,
        );
        talk.apply(
            CallEvent::AttributesChanged {
                identity: "me-id".into(),
                attributes: HashMap::from([("hand".into(), "up".into())]),
            },
            0.0,
        );
        assert_eq!(
            talk.raised_hands(),
            vec!["me-id".to_owned(), "a".to_owned()]
        );
        let what = hands::reaction_choices()[0].clone();
        let effects = talk.apply(
            CallEvent::Data {
                topic: hands::REACTION_TOPIC.into(),
                identity: Some("a".into()),
                payload: hands::reaction_payload(what.clone()).unwrap(),
            },
            0.0,
        );
        assert_eq!(
            effects,
            vec![Effect::Reaction {
                from: "a".into(),
                what
            }]
        );
        // A reaction with no sender is a server message, and nothing a
        // server says is a reaction.
        let effects = talk.apply(
            CallEvent::Data {
                topic: hands::REACTION_TOPIC.into(),
                identity: None,
                payload: hands::reaction_payload(hands::reaction_choices()[0].clone()).unwrap(),
            },
            0.0,
        );
        assert!(effects.is_empty());
    }

    #[test]
    fn somebody_back_at_once_is_neither_said_to_leave_nor_to_join() {
        let mut talk = talk();
        let effects = talk.apply(
            CallEvent::ParticipantLeft {
                identity: "a".into(),
            },
            10.0,
        );
        assert!(effects.contains(&Effect::LeaveLater {
            identity: "a".into()
        }));
        assert!(!effects
            .iter()
            .any(|effect| matches!(effect, Effect::Toast(_))));
        let effects = talk.apply(
            CallEvent::ParticipantJoined {
                participant: participant("a", vec![]),
            },
            11.0,
        );
        assert!(!effects
            .iter()
            .any(|effect| matches!(effect, Effect::Toast(_))));
        assert_eq!(talk.leave_due("a", 15.0), LeaveDue::Nothing);
    }

    #[test]
    fn a_leave_is_said_once_it_has_been_held() {
        let mut talk = talk();
        talk.apply(
            CallEvent::ParticipantLeft {
                identity: "a".into(),
            },
            10.0,
        );
        match talk.leave_due("a", 15.0) {
            LeaveDue::Say(toast) => assert_eq!(toast.title, "Name a left"),
            other => panic!("not said: {other:?}"),
        }
        assert_eq!(talk.leave_due("a", 16.0), LeaveDue::Nothing, "said once");
    }

    #[test]
    fn nobody_is_said_to_leave_while_this_app_reconnects() {
        let mut talk = talk();
        talk.apply(
            CallEvent::StateChanged {
                state: CallState::Reconnecting,
            },
            9.0,
        );
        talk.apply(
            CallEvent::ParticipantLeft {
                identity: "a".into(),
            },
            10.0,
        );
        // Still reconnecting when the leave comes due: asked again later.
        assert!(matches!(talk.leave_due("a", 15.0), LeaveDue::Wait(_)));
        talk.apply(
            CallEvent::StateChanged {
                state: CallState::Connected,
            },
            16.0,
        );
        // Back for a second only: they may not have been told about yet.
        assert_eq!(talk.leave_due("a", 17.0), LeaveDue::Wait(4000));
        // They were there all along.
        talk.apply(
            CallEvent::ParticipantJoined {
                participant: participant("a", vec![]),
            },
            17.5,
        );
        assert_eq!(talk.leave_due("a", 21.0), LeaveDue::Nothing);
    }

    #[test]
    fn the_end_of_the_call_is_a_move_only_soon_after_the_server_says_so() {
        let mut talk = talk();
        assert_eq!(talk.moved_to("RoomDeleted", 1.0), None, "no word of a move");
        let effects = talk.apply(
            CallEvent::Data {
                topic: "moving".into(),
                identity: None,
                payload: b"tokyo".to_vec(),
            },
            100.0,
        );
        assert_eq!(effects, vec![Effect::Moving { to: "tokyo".into() }]);
        assert_eq!(
            talk.moved_to("RoomDeleted", 101.0).as_deref(),
            Some("tokyo")
        );
        assert_eq!(talk.moved_to("RoomClosed", 101.0).as_deref(), Some("tokyo"));
        assert_eq!(
            talk.moved_to("ParticipantRemoved", 101.0).as_deref(),
            Some("tokyo")
        );
        assert_eq!(
            talk.moved_to("SignalClose", 101.0),
            None,
            "a lost connection"
        );
        assert_eq!(talk.moved_to("RoomDeleted", 115.5), None, "too late");
        // Said by a participant, it is nobody's word.
        let mut talk = super::tests::talk();
        talk.apply(
            CallEvent::Data {
                topic: "moving".into(),
                identity: Some("a".into()),
                payload: b"tokyo".to_vec(),
            },
            100.0,
        );
        assert_eq!(talk.moved_to("RoomDeleted", 101.0), None);
    }

    #[test]
    fn a_crowded_room_brings_the_last_speaker_forward() {
        let mut talk = Talk::new("room", "me-id", "Me");
        let everybody: Vec<ParticipantInfo> = (0..12)
            .map(|n| participant(&format!("p{n}"), vec![]))
            .collect();
        talk.arrive(&everybody);
        talk.apply(
            CallEvent::Speaking {
                identities: vec!["p11".into()],
            },
            5.0,
        );
        let pictures = talk.pictures(false, false);
        assert_eq!(pictures[1].identity, "p11");
        assert!(talk.person("p11").unwrap().speaking);
    }
}
