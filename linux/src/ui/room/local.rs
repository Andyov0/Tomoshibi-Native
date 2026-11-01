//! This participant's own devices in the call: the microphone and the
//! camera turned on and off, the devices picked in the devices panel and
//! remembered as the web remembers them, the original sound, and the screen
//! shared. Moved out of mod.rs whole; mod.rs takes every item back with
//! `use local::*`, so that `super::set_microphone` and the rest are reached
//! by the paths they always had.

use super::*;

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
    let _ = call;
    if microphone {
        set_microphone(model, true);
    }
    if camera {
        set_camera(model, true);
    }
    if share {
        start_share(model);
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
            Ok(()) => room.local.camera_on.set(on),
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

/// A device chosen in the devices panel: the microphone and camera opened
/// again on it, published again where they were on.
pub(crate) fn choose_device(model: &AppModel, kind: crate::media::DeviceKind, id: Option<String>) {
    use crate::media::DeviceKind;
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
            let on = room.local.microphone_on.get();
            if let Some(old) = room.local.microphone.take() {
                old.close();
            }
            room.local.microphone_published.set(false);
            room.local.microphone_on.set(false);
            if on {
                set_microphone(model, true);
            }
        }
        DeviceKind::Camera => {
            let on = room.local.camera_on.get();
            let old = room.local.camera.take();
            let weak = model.downgrade();
            glib::spawn_future_local(async move {
                if let Some(old) = old {
                    old.unpublish().await;
                    old.close();
                }
                if let Some(model) = weak.upgrade() {
                    model.kept::<RoomState>().local.camera_on.set(false);
                    if on {
                        set_camera(&model, true);
                    }
                    changed(&model);
                }
            });
        }
    }
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
    let quality = room
        .view
        .borrow()
        .share_quality
        .unwrap_or(ShareQuality::Auto);
    let choice = ShareChoice {
        fps: if quality == ShareQuality::Auto {
            30
        } else {
            60
        },
        quality,
        with_sound: true,
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
    if let Some(share) = share {
        share.stop().await;
    }
}
