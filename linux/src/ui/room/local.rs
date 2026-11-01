//! This participant's own devices in the call: the microphone and the
//! camera turned on and off, the devices picked in the devices panel and
//! remembered as the web remembers them, the original sound, and the screen
//! shared. Moved out of mod.rs whole; mod.rs takes every item back with
//! `use local::*`, so that `super::set_microphone` and the rest are reached
//! by the paths they always had.

use super::*;
use crate::media::{Device, DeviceKind};
use tomoshibi_protocol::share::{
    share_playout_ms, share_quality_key, share_rates_for, share_remembered_frame_rate,
    share_remembered_quality, sound_lossless_offered,
};

/// This participant's sharing beyond the screen itself, kept beside the
/// room's state (`AppModel::kept`) rather than in it: sound shared on its
/// own, what the running share sends, and the changes under way.
#[derive(Default)]
pub struct Shares {
    /// Sound shared on its own (the web's "Share only sound").
    pub listening: RefCell<Option<Rc<dyn ScreenShare>>>,
    pub listening_busy: Cell<bool>,
    /// What the running screen share sends (room.ts `runningShare`), which
    /// the menu shows while it runs: the uplink card can change it without
    /// the menu.
    pub running: Cell<Option<(ShareQuality, u32)>>,
    /// A change of the running share is out.
    pub changing: Cell<bool>,
    /// The uplink card's switch is being applied.
    pub strain_busy: Cell<bool>,
    /// When a camera that stopped by itself was last replaced by another,
    /// so that one that fails as soon as it starts is not started for ever.
    pub camera_rescued: Cell<Option<std::time::Instant>>,
}

/// room.ts `QUALITY_KEY` and `RATE_KEY`: the share menu's choices, kept
/// between calls under the web's own names.
pub(super) const SHARE_QUALITY_KEY: &str = "meet.share.quality";
pub(super) const SHARE_RATE_KEY: &str = "meet-live.share-frame-rate";
/// sound.ts `LOSSLESS_KEY`: sound shared on its own (and the song desk's
/// music) goes losslessly too unless this says "off".
pub(super) const LOSSLESS_KEY: &str = "meet-live.lossless";
/// delay.ts `DELAY_KEY`: how long shares are held before they are played.
const SHARE_PLAYOUT_KEY: &str = "meet-live.share-playout-ms";

// ---- devices -----------------------------------------------------------

/// The devices the pre-join opened, or the ones it said to turn on.
pub(super) fn devices_at_join(model: &AppModel, entry: &CallEntry, call: &Arc<Call>) {
    let room = model.kept::<RoomState>();
    if let Some(microphone) = entry.microphone.clone() {
        room.local.microphone.replace(Some(microphone));
    }
    if let Some(camera) = entry.camera.clone() {
        room.local.camera.replace(Some(camera));
    }
    // A fresh call starts with no sound shared and no running share; what
    // the last one left is let go of (its Drop takes its track down).
    model.kept::<Shares>().listening.replace(None);
    model.kept::<Shares>().running.set(None);
    // The cushion under shares, as delay.ts `shareDelay` reads it once when
    // a call starts: the core holds 200 ms already, which is the default,
    // so only a kept value is said.
    if let Some(kept) = model.preferences().string(SHARE_PLAYOUT_KEY) {
        call.set_share_playout_ms(share_playout_ms(Some(kept)));
    }
    let mute = model.launch().mute.clone();
    if mute.as_deref() == Some("speakers") {
        if let Some(media) = media_of(model) {
            media.set_silent(true);
        }
    }
    // TOMOSHIBI_MUTE is about what is played, not what is sent (app::Launch):
    // a test that silences the speakers still sends its test sound.
    let mut microphone = entry.microphone_on;
    let mut camera = entry.camera_on;
    let mut share = false;
    #[cfg(debug_assertions)]
    {
        microphone |= fake::on_at_join("microphone");
        camera |= fake::on_at_join("camera");
        share |= fake::on_at_join("share");
    }
    if microphone {
        set_microphone(model, true);
    }
    if camera {
        set_camera(model, true);
    }
    if share {
        start_share(model);
    }
    #[cfg(debug_assertions)]
    drive_devices(model);
}

/// TOMOSHIBI_DEBUG_DEVICES, a debug run's presses in the devices panel:
/// `<milliseconds>:<what>` separated by semicolons, counted from the join;
/// `microphone=<id>`, `camera=<id>` and `speaker=<id>` choose a device, and
/// `camera-gone` pulls the camera out (`Camera::pretend_unplugged`);
/// `listen=<on|off>` and `lossless=<on|off>` are the share menu's sound
/// alone and its switch; `strain` shows the uplink card with a made-up
/// reading, for a picture of it (a line that cannot carry a share is not
/// something a test can make on a loopback).
#[cfg(debug_assertions)]
fn drive_devices(model: &AppModel) {
    let Ok(script) = std::env::var("TOMOSHIBI_DEBUG_DEVICES") else {
        return;
    };
    for step in script
        .split(';')
        .map(str::trim)
        .filter(|step| !step.is_empty())
    {
        let Some((at, what)) = step.split_once(':') else {
            continue;
        };
        let Ok(at) = at.trim().parse::<u64>() else {
            continue;
        };
        let (weak, what) = (model.downgrade(), what.trim().to_owned());
        glib::timeout_add_local_once(Duration::from_millis(at), move || {
            let Some(model) = weak.upgrade() else { return };
            log(&format!("devices script: {what}"));
            let (verb, value) = what.split_once('=').unwrap_or((what.as_str(), ""));
            let id = Some(value.to_owned()).filter(|id| !id.is_empty());
            match verb {
                "microphone" => choose_device(&model, DeviceKind::Microphone, id),
                "camera" => choose_device(&model, DeviceKind::Camera, id),
                "speaker" => choose_device(&model, DeviceKind::Speaker, id),
                "listen" => {
                    if value == "on" {
                        start_listening(&model);
                    } else {
                        stop_listening(&model);
                    }
                }
                "lossless" => set_lossless(&model, value == "on"),
                "original" => set_original_sound(&model, value != "off"),
                "strain" => {
                    use tomoshibi_protocol::{share::ShareSetting, strain::StrainHint};
                    if let Some(talk) = model.kept::<RoomState>().talk.borrow_mut().as_mut() {
                        talk.strain = Some(StrainHint {
                            have: 2_100_000.0,
                            need: 6_400_000.0,
                            to: ShareSetting {
                                quality: ShareQuality::P1080,
                                frame_rate: 30,
                            },
                        });
                    }
                    changed(&model);
                }
                "camera-gone" => {
                    let camera = model.kept::<RoomState>().local.camera.borrow().clone();
                    if let Some(camera) = camera {
                        camera.pretend_unplugged();
                    }
                }
                _ => {}
            }
        });
    }
}

/// The microphone on or off: published once, then muted and unmuted at
/// the track (the Mac's `setMicrophone`), so that turning it back on is
/// immediate and a second voice is never published.
pub(crate) fn set_microphone(model: &AppModel, on: bool) {
    let room = model.kept::<RoomState>();
    let (Some(call), Some(media)) = (call_of(model), media_of(model)) else {
        return;
    };
    if room.local.microphone_busy.replace(true) {
        return;
    }
    let weak = model.downgrade();
    glib::spawn_future_local(async move {
        let Some(model) = weak.upgrade() else {
            return;
        };
        let room = model.kept::<RoomState>();
        let result = async {
            if !on {
                if let Some(microphone) = room.local.microphone.borrow().as_ref() {
                    microphone.set_muted(true);
                }
                return Ok(());
            }
            let existing = room.local.microphone.borrow().clone();
            let microphone = match existing {
                Some(microphone) => microphone,
                None => {
                    let chosen = chosen_device(&model, crate::media::DeviceKind::Microphone);
                    let opened = media.open_microphone(chosen).await?;
                    room.local.microphone.replace(Some(opened.clone()));
                    opened
                }
            };
            if !room.local.microphone_published.get() {
                microphone
                    .publish(call.clone(), original_sound(&model))
                    .await?;
                room.local.microphone_published.set(true);
            }
            microphone.set_muted(false);
            Ok::<(), MediaError>(())
        }
        .await;
        room.local.microphone_busy.set(false);
        match result {
            Ok(()) => room.local.microphone_on.set(on),
            Err(fault) => {
                log(&format!("the microphone did not start: {fault}"));
                if let Some(microphone) = room.local.microphone.borrow().as_ref() {
                    microphone.set_muted(true);
                }
                room.local.microphone_on.set(false);
                failed(&model, device_sentence(&fault, true));
            }
        }
        changed(&model);
    });
}

/// The camera on or off: published when turned on and taken down when
/// turned off, the device let go with it so that its light goes out (the
/// Mac's `setCamera`).
pub(crate) fn set_camera(model: &AppModel, on: bool) {
    let room = model.kept::<RoomState>();
    let (Some(call), Some(media)) = (call_of(model), media_of(model)) else {
        return;
    };
    if room.local.camera_busy.replace(true) {
        return;
    }
    let weak = model.downgrade();
    glib::spawn_future_local(async move {
        let Some(model) = weak.upgrade() else {
            return;
        };
        let room = model.kept::<RoomState>();
        let result = async {
            if !on {
                if let Some(camera) = room.local.camera.take() {
                    camera.unpublish().await;
                    camera.close();
                }
                return Ok(());
            }
            let existing = room.local.camera.borrow().clone();
            let camera = match existing {
                Some(camera) => camera,
                None => {
                    let chosen = chosen_device(&model, crate::media::DeviceKind::Camera);
                    let opened = media.open_camera(chosen).await?;
                    // A camera the pre-join opened was handed over with
                    // the blur already set; one opened here is set the
                    // same way, from the same switch and the deployment's
                    // word on it.
                    opened.set_blur(model.blur());
                    room.local.camera.replace(Some(opened.clone()));
                    opened
                }
            };
            if let Err(fault) = camera.publish(call.clone()).await {
                // A camera published and never fed is a black tile for
                // everybody else; one that did not publish is let go.
                camera.close();
                room.local.camera.replace(None);
                return Err(fault);
            }
            Ok::<(), MediaError>(())
        }
        .await;
        room.local.camera_busy.set(false);
        match result {
            Ok(()) => {
                room.local.camera_on.set(on);
                if on {
                    if let Some(camera) = room.local.camera.borrow().as_ref() {
                        watch_camera(&model, camera);
                    }
                }
            }
            Err(fault) => {
                log(&format!("the camera did not start: {fault}"));
                room.local.camera_on.set(false);
                failed(&model, device_sentence(&fault, false));
            }
        }
        changed(&model);
    });
}

/// The web's own record of the devices picked in a call (preferences.ts
/// `deviceChoice`): one JSON object under one key, a device id by the
/// browser's kind names. The settings write the same.
pub(super) const CHOICE_KEY: &str = "meet-live.device-choice";
/// sound.ts `ORIGINAL_KEY`, "on" when the voice is sent as it is.
pub(super) const ORIGINAL_SOUND_KEY: &str = "meet-live.original-sound";

pub(super) fn kind_name(kind: crate::media::DeviceKind) -> &'static str {
    use crate::media::DeviceKind;
    match kind {
        DeviceKind::Microphone => "audioinput",
        DeviceKind::Camera => "videoinput",
        DeviceKind::Speaker => "audiooutput",
    }
}

/// The device last picked of a kind, read as the web reads it: anything
/// that is not a short string is nothing picked.
pub(crate) fn chosen_device(model: &AppModel, kind: crate::media::DeviceKind) -> Option<String> {
    let stored = model.preferences().string(CHOICE_KEY)?;
    let parsed: serde_json::Value = serde_json::from_str(&stored).ok()?;
    parsed
        .get(kind_name(kind))
        .and_then(serde_json::Value::as_str)
        .filter(|id| !id.is_empty() && id.len() <= 256)
        .map(str::to_owned)
}

pub(super) fn remember_device(model: &AppModel, kind: crate::media::DeviceKind, id: Option<&str>) {
    let mut choice = model
        .preferences()
        .string(CHOICE_KEY)
        .and_then(|stored| serde_json::from_str::<serde_json::Value>(&stored).ok())
        .filter(serde_json::Value::is_object)
        .unwrap_or_else(|| serde_json::json!({}));
    if let Some(object) = choice.as_object_mut() {
        match id {
            Some(id) if id.len() <= 256 => {
                object.insert(kind_name(kind).to_owned(), serde_json::Value::from(id));
            }
            _ => {
                object.remove(kind_name(kind));
            }
        }
    }
    model.preferences().set(CHOICE_KEY, choice.to_string());
}

pub(super) fn original_sound(model: &AppModel) -> bool {
    model.preferences().string(ORIGINAL_SOUND_KEY).as_deref() == Some("on")
}

/// The web's "original sound" (sound.ts `applyOriginalSound`): the voice
/// captured again without noise suppression or levelling, and remembered.
/// A voice already published is published again with the new capture,
/// which the Microphone contract says it does when asked again.
pub(crate) fn set_original_sound(model: &AppModel, on: bool) {
    if on {
        model.preferences().set(ORIGINAL_SOUND_KEY, "on");
    } else {
        model.preferences().remove(ORIGINAL_SOUND_KEY);
    }
    let room = model.kept::<RoomState>();
    let microphone = room.local.microphone.borrow().clone();
    if let (Some(microphone), Some(call)) = (microphone, call_of(model)) {
        if room.local.microphone_published.get() {
            let weak = model.downgrade();
            glib::spawn_future_local(async move {
                if let Err(fault) = microphone.publish(call, on).await {
                    log(&format!("the voice was not published again: {fault}"));
                    if let Some(model) = weak.upgrade() {
                        failed(&model, t("That could not be done. Try again."));
                    }
                }
            });
        }
    }
    changed(model);
}

pub(super) fn device_sentence(fault: &MediaError, microphone: bool) -> String {
    match (fault, microphone) {
        (MediaError::Denied | MediaError::Unsupported | MediaError::NotFound, true) => {
            t("Can't use your microphone")
        }
        (MediaError::Denied | MediaError::Unsupported | MediaError::NotFound, false) => {
            t("Can't use your camera")
        }
        _ => t("That could not be done. Try again."),
    }
}

/// A device chosen in the devices panel: the microphone or camera the call
/// is sending moved to it in place, or the speakers changed; remembered as
/// the web remembers it.
///
/// In place, because taking a track down and publishing another is seen by
/// everybody: the voice vanished and came back, for a moment there were two,
/// and a camera's picture fell back to the initials (a browser's
/// `switchActiveDevice` and the Mac's `useCamera` keep the track). A device
/// whose capture cannot be moved (the contract's default) is published anew
/// before the old one is taken down, never the other way round.
pub(crate) fn choose_device(model: &AppModel, kind: crate::media::DeviceKind, id: Option<String>) {
    let room = model.kept::<RoomState>();
    // Not kept in a rehearsal (Launch::rehearsal), as nothing is.
    remember_device(model, kind, id.as_deref());
    match kind {
        DeviceKind::Speaker => {
            if let Some(media) = media_of(model) {
                media.set_speaker(id);
            }
        }
        DeviceKind::Microphone => {
            let Some(microphone) = room.local.microphone.borrow().clone() else {
                // Opened on the one chosen when it is next turned on.
                changed(model);
                return;
            };
            if room.local.microphone_busy.replace(true) {
                return;
            }
            let weak = model.downgrade();
            glib::spawn_future_local(async move {
                let moved = microphone.use_device(id.clone()).await;
                let Some(model) = weak.upgrade() else { return };
                let room = model.kept::<RoomState>();
                let result = match moved {
                    Err(MediaError::Unsupported) => {
                        replace_microphone(&model, &microphone, id).await
                    }
                    other => other,
                };
                room.local.microphone_busy.set(false);
                if let Err(fault) = result {
                    log(&format!("the microphone was not changed: {fault}"));
                    failed(&model, device_sentence(&fault, true));
                }
                changed(&model);
            });
        }
        DeviceKind::Camera => {
            let camera = room.local.camera.borrow().clone();
            let Some(camera) = camera.filter(|_| room.local.camera_on.get()) else {
                // A camera chosen with the camera off is the one it comes on
                // with (the Mac's `useCamera`): nothing is opened until then.
                changed(model);
                return;
            };
            if room.local.camera_busy.replace(true) {
                return;
            }
            let weak = model.downgrade();
            glib::spawn_future_local(async move {
                let moved = camera.use_device(id.clone()).await;
                let Some(model) = weak.upgrade() else { return };
                let room = model.kept::<RoomState>();
                let result = match moved {
                    Err(MediaError::Unsupported) => replace_camera(&model, &camera, id).await,
                    other => other,
                };
                room.local.camera_busy.set(false);
                if let Err(fault) = result {
                    log(&format!("the camera was not changed: {fault}"));
                    failed(&model, device_sentence(&fault, false));
                }
                changed(&model);
            });
        }
    }
    changed(model);
}

/// A microphone that cannot be moved in place: another opened and published
/// first, the old one taken down once the new one is up.
async fn replace_microphone(
    model: &AppModel,
    old: &Rc<dyn Microphone>,
    id: Option<String>,
) -> Result<(), MediaError> {
    let room = model.kept::<RoomState>();
    let (Some(call), Some(media)) = (call_of(model), media_of(model)) else {
        return Ok(());
    };
    let fresh = media.open_microphone(id).await?;
    if room.local.microphone_published.get() {
        fresh.set_muted(!room.local.microphone_on.get());
        fresh.publish(call, original_sound(model)).await?;
    }
    room.local.microphone.replace(Some(fresh));
    old.close();
    Ok(())
}

/// A camera that cannot be moved in place, likewise.
async fn replace_camera(
    model: &AppModel,
    old: &Rc<dyn Camera>,
    id: Option<String>,
) -> Result<(), MediaError> {
    let room = model.kept::<RoomState>();
    let (Some(call), Some(media)) = (call_of(model), media_of(model)) else {
        return Ok(());
    };
    let fresh = media.open_camera(id).await?;
    fresh.set_blur(model.blur());
    if let Err(fault) = fresh.publish(call).await {
        fresh.close();
        return Err(fault);
    }
    room.local.camera.replace(Some(fresh.clone()));
    watch_camera(model, &fresh);
    old.unpublish().await;
    old.close();
    Ok(())
}

/// Hears of the call's camera stopping by itself (unplugged, its stream
/// ended): before, that was a line in the log, `camera_on` stayed true, and
/// everybody else watched a frozen or black picture with nothing said.
fn watch_camera(model: &AppModel, camera: &Rc<dyn Camera>) {
    let weak = model.downgrade();
    let which = Rc::downgrade(camera);
    let lost = camera.device();
    camera.on_ended(Box::new(move || {
        if let Some(model) = weak.upgrade() {
            camera_gone(&model, which.clone(), lost.clone());
        }
    }));
}

/// The camera stopped by itself (the Mac's camera trouble, CallModel
/// `cameraTrouble`): another camera there is sent into the same picture and
/// said; with none, or one that will not start, the camera goes off and
/// that is said.
fn camera_gone(model: &AppModel, which: std::rc::Weak<dyn Camera>, lost: Option<String>) {
    let room = model.kept::<RoomState>();
    let Some(camera) = room.local.camera.borrow().clone() else {
        return;
    };
    if !which
        .upgrade()
        .is_some_and(|gone| Rc::ptr_eq(&gone, &camera))
    {
        return;
    }
    log("the camera stopped by itself");
    let shares = model.kept::<Shares>();
    // Once in ten seconds at most: a camera that fails as soon as it is
    // started again would otherwise be started again for ever.
    let may_rescue = shares
        .camera_rescued
        .get()
        .is_none_or(|at| at.elapsed() > Duration::from_secs(10));
    let other: Option<Device> = media_of(model).and_then(|media| {
        media
            .devices(DeviceKind::Camera)
            .into_iter()
            .find(|device| Some(&device.id) != lost.as_ref())
    });
    room.local.camera_busy.set(true);
    let weak = model.downgrade();
    glib::spawn_future_local(async move {
        let mut moved = false;
        if let Some(other) = other.filter(|_| may_rescue) {
            if let Some(model) = weak.upgrade() {
                model
                    .kept::<Shares>()
                    .camera_rescued
                    .set(Some(std::time::Instant::now()));
            }
            match camera.use_device(Some(other.id.clone())).await {
                Ok(()) => {
                    moved = true;
                    if let Some(model) = weak.upgrade() {
                        watch_camera(&model, &camera);
                        announce(
                            &model,
                            Toast {
                                detail: Some(t_with(
                                    "Now using {camera}.",
                                    &[("camera", &other.label)],
                                )),
                                lasts_ms: Some(10_000),
                                key: Some("camera".into()),
                                ..Toast::said(t("Your camera was disconnected."))
                            },
                        );
                    }
                }
                Err(fault) => log(&format!("no other camera would start: {fault}")),
            }
        }
        let Some(model) = weak.upgrade() else { return };
        let room = model.kept::<RoomState>();
        if !moved {
            if let Some(camera) = room.local.camera.take() {
                camera.unpublish().await;
                camera.close();
            }
            room.local.camera_on.set(false);
            announce(
                &model,
                Toast {
                    detail: Some(t("Your camera was disconnected.")),
                    lasts_ms: Some(10_000),
                    closable: true,
                    key: Some("camera".into()),
                    ..Toast::failure(t("Can't use your camera"))
                },
            );
        }
        room.local.camera_busy.set(false);
        changed(&model);
    });
    changed(model);
}

/// Shares the screen with the setting the share popover shows.
pub(crate) fn start_share(model: &AppModel) {
    let room = model.kept::<RoomState>();
    let (Some(call), Some(media)) = (call_of(model), media_of(model)) else {
        return;
    };
    if room.local.sharing_busy.get() || room.local.share.borrow().is_some() {
        return;
    }
    room.local.sharing_busy.set(true);
    let (quality, fps) = share_chosen(model);
    // The screen's own sound unless sound is already shared on its own,
    // which carries on and is not doubled (room.ts `alongside`).
    let listening = model.kept::<Shares>().listening.borrow().is_some();
    let choice = ShareChoice {
        fps,
        quality,
        with_sound: !listening,
    };
    changed(model);
    let weak = model.downgrade();
    glib::spawn_future_local(async move {
        let shared = media.share_screen(call, choice).await;
        let Some(model) = weak.upgrade() else {
            return;
        };
        let room = model.kept::<RoomState>();
        room.local.sharing_busy.set(false);
        match shared {
            Ok(share) => {
                let weak = model.downgrade();
                share.on_ended(Box::new(move || {
                    if let Some(model) = weak.upgrade() {
                        model.kept::<RoomState>().local.share.replace(None);
                        model.kept::<Shares>().running.set(None);
                        changed(&model);
                    }
                }));
                // Asked for with its sound and sent without it (no PipeWire
                // to capture it from, or its capture failed): said, as the
                // web says it (notices.ts `noSoundShared`), rather than left
                // as a share the others watch in silence. The web's second
                // line is about a browser's picker, which is not this one.
                if choice.with_sound && !share.sends_sound() {
                    announce(
                        &model,
                        Toast {
                            closable: true,
                            lasts_ms: Some(8000),
                            key: Some("no-sound-shared".into()),
                            ..Toast::failure(t("No sound was shared"))
                        },
                    );
                }
                model.kept::<Shares>().running.set(Some((quality, fps)));
                room.local.share.replace(Some(share));
            }
            // The person closing the system's picker is no failure.
            Err(MediaError::Denied) => {}
            Err(fault) => {
                log(&format!("the screen was not shared: {fault}"));
                failed(&model, t("That could not be done. Try again."));
            }
        }
        changed(&model);
    });
}

pub(crate) fn stop_share(model: &AppModel) {
    let weak = model.downgrade();
    glib::spawn_future_local(async move {
        if let Some(model) = weak.upgrade() {
            stop_sharing_now(&model).await;
            changed(&model);
        }
    });
}

pub(super) async fn stop_sharing_now(model: &AppModel) {
    let share = model.kept::<RoomState>().local.share.take();
    model.kept::<Shares>().running.set(None);
    if let Some(share) = share {
        share.stop().await;
    }
    let listening = model.kept::<Shares>().listening.take();
    if let Some(listening) = listening {
        listening.stop().await;
    }
}

// ---- the share's settings, sound alone, the uplink card -----------------

/// The rate a size may go at (ShareButton.tsx `clamp`): the one chosen
/// where the size allows it, and the size's highest where it does not.
pub(crate) fn share_clamp(rate: u32, quality: ShareQuality) -> u32 {
    let allowed = share_rates_for(quality);
    if allowed.contains(&rate) {
        rate
    } else {
        allowed.last().copied().unwrap_or(30)
    }
}

/// The size and rate the menu remembers (room.ts `rememberedQuality`,
/// `rememberedFrameRate`): automatic and sixty unless chosen. Automatic is
/// sixty frames a second, as the web's automatic preset is; it was thirty
/// here, half what a browser sends.
pub(crate) fn share_chosen(model: &AppModel) -> (ShareQuality, u32) {
    let quality = share_remembered_quality(model.preferences().string(SHARE_QUALITY_KEY));
    let rate = share_remembered_frame_rate(model.preferences().string(SHARE_RATE_KEY));
    let fps = if quality == ShareQuality::Auto {
        60
    } else {
        share_clamp(rate, quality)
    };
    (quality, fps)
}

/// What the menu shows: what the running share sends while there is one,
/// the remembered choice otherwise.
pub(crate) fn share_shown(model: &AppModel) -> (ShareQuality, u32) {
    let sharing = model.kept::<RoomState>().local.share.borrow().is_some();
    match model.kept::<Shares>().running.get() {
        Some(running) if sharing => running,
        _ => share_chosen(model),
    }
}

/// A size or rate chosen in the menu: remembered, and applied at once to a
/// share that is running (ShareButton.tsx: a setting that waits for the
/// next share is one nobody can judge).
pub(crate) fn choose_share(model: &AppModel, quality: ShareQuality, fps: Option<u32>) {
    model
        .preferences()
        .set(SHARE_QUALITY_KEY, share_quality_key(quality));
    if let Some(fps) = fps {
        model.preferences().set(SHARE_RATE_KEY, fps.to_string());
    }
    let (_, shown_fps) = share_shown(model);
    let fps = if quality == ShareQuality::Auto {
        60
    } else {
        share_clamp(fps.unwrap_or(shown_fps), quality)
    };
    changed(model);
    retune(model, quality, fps, None);
}

/// The running share at another size and rate, its sound as it is;
/// `then` is told whether it took (the uplink card says so to the core).
fn retune(model: &AppModel, quality: ShareQuality, fps: u32, then: Option<Box<dyn FnOnce(bool)>>) {
    let room = model.kept::<RoomState>();
    let shares = model.kept::<Shares>();
    let Some(share) = room.local.share.borrow().clone() else {
        if let Some(then) = then {
            then(false);
        }
        return;
    };
    if shares.running.get() == Some((quality, fps)) {
        if let Some(then) = then {
            then(true);
        }
        return;
    }
    let choice = ShareChoice {
        fps,
        quality,
        with_sound: share.sends_sound(),
    };
    shares.changing.set(true);
    let weak = model.downgrade();
    glib::spawn_future_local(async move {
        let done = share.change(choice).await;
        let Some(model) = weak.upgrade() else { return };
        let shares = model.kept::<Shares>();
        shares.changing.set(false);
        let took = done.is_ok();
        match done {
            Ok(()) => shares.running.set(Some((quality, fps))),
            Err(fault) => {
                log(&format!("the share was not changed: {fault}"));
                failed(&model, t("That could not be done. Try again."));
            }
        }
        if let Some(then) = then {
            then(took);
        }
        changed(&model);
    });
}

/// Whether sound shared on its own goes losslessly too, and whether it
/// can (sound.ts `losslessUnavailable`): never in a sealed call, whose data
/// is not sealed and would carry the samples across the relay in the clear.
pub(crate) fn lossless_wanted(model: &AppModel) -> bool {
    model.preferences().string(LOSSLESS_KEY).as_deref() != Some("off")
}

pub(crate) fn sealed(model: &AppModel) -> bool {
    model
        .call_entry()
        .and_then(|entry| entry.sealing_word)
        .is_some_and(|word| !word.is_empty())
}

pub(crate) fn set_lossless(model: &AppModel, on: bool) {
    if on {
        model.preferences().remove(LOSSLESS_KEY);
    } else {
        model.preferences().set(LOSSLESS_KEY, "off");
    }
    changed(model);
}

/// Shares the computer's sound alone (sound.ts `startListening`). A screen
/// already shared gives up its own sound meanwhile: the two would be one
/// song twice a beat apart.
pub(crate) fn start_listening(model: &AppModel) {
    let shares = model.kept::<Shares>();
    let (Some(call), Some(media)) = (call_of(model), media_of(model)) else {
        return;
    };
    if shares.listening_busy.get() || shares.listening.borrow().is_some() {
        return;
    }
    shares.listening_busy.set(true);
    let lossless = sound_lossless_offered(lossless_wanted(model), sealed(model), true);
    changed(model);
    let weak = model.downgrade();
    glib::spawn_future_local(async move {
        let shared = media.share_sound(call, lossless).await;
        let Some(model) = weak.upgrade() else { return };
        let shares = model.kept::<Shares>();
        shares.listening_busy.set(false);
        match shared {
            Ok(listening) => {
                let weak = model.downgrade();
                listening.on_ended(Box::new(move || {
                    if let Some(model) = weak.upgrade() {
                        model.kept::<Shares>().listening.replace(None);
                        changed(&model);
                    }
                }));
                shares.listening.replace(Some(listening));
                screen_sound(&model, false);
            }
            Err(MediaError::Denied) => {}
            Err(fault) => {
                log(&format!("the sound was not shared: {fault}"));
                failed(&model, t("That could not be done. Try again."));
            }
        }
        changed(&model);
    });
}

pub(crate) fn stop_listening(model: &AppModel) {
    let listening = model.kept::<Shares>().listening.take();
    changed(model);
    let Some(listening) = listening else { return };
    let weak = model.downgrade();
    glib::spawn_future_local(async move {
        listening.stop().await;
        if let Some(model) = weak.upgrade() {
            // And the screen, still shared, has its sound back.
            screen_sound(&model, true);
            changed(&model);
        }
    });
}

/// The running screen share with or without its own sound.
fn screen_sound(model: &AppModel, on: bool) {
    let Some(share) = model.kept::<RoomState>().local.share.borrow().clone() else {
        return;
    };
    if share.sends_sound() == on {
        return;
    }
    let Some((quality, fps)) = model.kept::<Shares>().running.get() else {
        return;
    };
    glib::spawn_future_local(async move {
        let choice = ShareChoice {
            fps,
            quality,
            with_sound: on,
        };
        if let Err(fault) = share.change(choice).await {
            log(&format!("the screen's sound was not changed: {fault}"));
        }
    });
}

/// The uplink card's "Switch" (StrainCard, strain.ts): the setting that
/// fits, through the same retune the menu uses, and the core told how it
/// went. Not remembered: what one afternoon's line could carry is not a
/// thing to keep.
pub(crate) fn strain_take(model: &AppModel) {
    let Some(call) = call_of(model) else { return };
    let shares = model.kept::<Shares>();
    if shares.strain_busy.get() {
        return;
    }
    let tomoshibi_protocol::strain::StrainAccept::Apply { setting } = call.strain_accept() else {
        clear_strain(model);
        return;
    };
    shares.strain_busy.set(true);
    changed(model);
    let weak = model.downgrade();
    let fps = share_clamp(setting.frame_rate, setting.quality);
    retune(
        model,
        setting.quality,
        fps,
        Some(Box::new(move |took| {
            let Some(model) = weak.upgrade() else { return };
            model.kept::<Shares>().strain_busy.set(false);
            if call.strain_accepted(took) {
                clear_strain(&model);
            }
            changed(&model);
        })),
    );
}

/// "Not now": the card is put away for this share.
pub(crate) fn strain_not_now(model: &AppModel) {
    if let Some(call) = call_of(model) {
        call.strain_decline();
    }
    clear_strain(model);
}

fn clear_strain(model: &AppModel) {
    if let Some(talk) = model.kept::<RoomState>().talk.borrow_mut().as_mut() {
        talk.strain = None;
    }
    changed(model);
}
