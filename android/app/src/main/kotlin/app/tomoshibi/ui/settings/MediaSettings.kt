package app.tomoshibi.ui.settings

import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleEventObserver
import androidx.lifecycle.compose.LocalLifecycleOwner
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.expandVertically
import androidx.compose.animation.shrinkVertically
import app.tomoshibi.basics.Theme
import app.tomoshibi.basics.t
import app.tomoshibi.model.AppModel
import app.tomoshibi.model.CallPreferences
import app.tomoshibi.model.DeviceMemory
import app.tomoshibi.model.RemoteFeature
import app.tomoshibi.ui.kit.Appear
import app.tomoshibi.ui.kit.Capsule
import app.tomoshibi.ui.kit.Font
import app.tomoshibi.ui.kit.GlassButton
import app.tomoshibi.ui.kit.GlassButtonShape
import app.tomoshibi.ui.kit.GlassChoice
import app.tomoshibi.ui.kit.GlassChoiceOption
import app.tomoshibi.ui.kit.GlassSwitch
import app.tomoshibi.ui.kit.KitText
import app.tomoshibi.ui.kit.LevelMeter
import app.tomoshibi.ui.kit.SettingsRow
import app.tomoshibi.ui.kit.Symbol
import app.tomoshibi.ui.kit.glassSurface
import app.tomoshibi.ui.kit.rounded
import kotlinx.coroutines.flow.drop

// The Audio & video page (Settings.swift `MediaSettings`): what starts on
// when somebody joins, and which devices, with the camera and the microphone
// open while the page shows so that they can be seen and heard before a call.

@Composable
internal fun MediaSettings(app: AppModel, preferences: CallPreferences) {
    val platform = platform()
    val check = remember(platform) { platform.deviceCheck() }
    // Listed as the page is first drawn, so that the speakers' row is there
    // from the first frame rather than arriving a frame later.
    val cameras = remember(platform) { platform.cameras() }
    val microphones = remember(platform) { platform.microphones() }
    val speakers = remember(platform) { platform.speakers() }
    // The blur switch, read from what is remembered when the page opens.
    var blurWanted by remember { mutableStateOf(app.deviceMemory.blur) }
    val lifecycle = LocalLifecycleOwner.current.lifecycle

    fun checkDevices() {
        if (!lifecycle.currentState.isAtLeast(Lifecycle.State.STARTED)) return
        check.start(preferences.devices[DeviceMemory.Kind.Camera.key], preferences.devices[DeviceMemory.Kind.Microphone.key])
    }

    DisposableEffect(check, lifecycle) {
        preferences.refresh()
        checkDevices()
        // Stopped when the app goes to the background, not only when the
        // page goes: the page stays where it was under another app, and a
        // camera and a microphone left recording for as long as the app was
        // away were what the iPhone did before its check listened for this.
        // Back in front, the page is still showing, and the devices open again.
        val observer = LifecycleEventObserver { _, event ->
            when (event) {
                Lifecycle.Event.ON_STOP -> check.stop()
                Lifecycle.Event.ON_START -> if (!check.started) checkDevices()
                else -> {}
            }
        }
        lifecycle.addObserver(observer)
        onDispose {
            lifecycle.removeObserver(observer)
            check.close()
        }
    }
    // A device picked here is the one the check shows from then on.
    LaunchedEffect(check) {
        snapshotFlow { preferences.devices }.drop(1).collect { checkDevices() }
    }

    val picked = preferences.devices.isNotEmpty()
    Column(verticalArrangement = Arrangement.spacedBy(22.dp)) {
        DevicePreview(check)
        Section(t("When you join")) {
            SettingsRow(t("Start with the microphone on"), symbol = "mic.fill") {
                GlassSwitch(app.startWithMicrophone, { app.startWithMicrophone = it }, Modifier.testTag("settings.start.microphone"),
                    label = t("Start with the microphone on"))
            }
            SettingsRow(t("Start with the camera on"), symbol = "video.fill") {
                GlassSwitch(app.startWithCamera, { app.startWithCamera = it }, label = t("Start with the camera on"))
            }
            SettingsRow(
                t("Original sound"),
                detail = t("No noise suppression or automatic volume. For headphones and music."),
                symbol = "waveform",
                last = true,
            ) { GlassSwitch(preferences.originalSound, { preferences.originalSound = it }, label = t("Original sound")) }
        }
        Section(
            t("Devices"),
            footer = if (picked) t("Calls start on the microphone, camera and speakers you last picked in one, while they are plugged in.") else null,
        ) {
            AdaptiveRow(t("Camera"), symbol = "video") {
                DeviceChoice("settings.camera", cameras, preferences, DeviceMemory.Kind.Camera)
            }
            // The web's "Blur my background" tick, kept as the web keeps it
            // (`meet-live.blur`) and applied by the call's own blur from the
            // next call, and at once in a call that is on. Gone while the
            // deployment has blur switched off (Remote), when the next call
            // starts unblurred whatever is remembered here (CallModel
            // `blurKept`).
            AnimatedVisibility(
                app.remote.allows(RemoteFeature.BackgroundBlur),
                enter = Theme.arrive + expandVertically(Theme.settle()),
                exit = Theme.depart + shrinkVertically(Theme.leave()),
            ) {
                SettingsRow(t("Blur my background"), Modifier.testTag("settings.blur"), symbol = "person.and.background.dotted") {
                    GlassSwitch(
                        blurWanted,
                        { on ->
                            blurWanted = on
                            app.deviceMemory.rememberBlur(on)
                            app.call?.setBlur(on)
                        },
                        label = t("Blur my background"),
                    )
                }
            }
            AdaptiveRow(t("Microphone"), symbol = "mic", last = speakers.isEmpty() && !picked) {
                DeviceChoice("settings.microphone", microphones, preferences, DeviceMemory.Kind.Microphone)
            }
            if (speakers.isNotEmpty()) {
                AdaptiveRow(t("Speakers"), symbol = "speaker.wave.2", last = !picked) {
                    DeviceChoice("settings.speakers", speakers, preferences, DeviceMemory.Kind.Speakers)
                }
            }
            Arriving(picked) {
                Row(Modifier.fillMaxWidth().padding(horizontal = 14.dp, vertical = 9.dp)) {
                    Spacer(Modifier.weight(1f))
                    GlassButton({ preferences.forgetDevices() }, Modifier.testTag("settings.forget"), shape = GlassButtonShape.Capsule(30.dp)) {
                        KitText(t("Forget"))
                    }
                }
            }
        }
    }
}

/** One kind's device, or the system's: the first choice, written as nothing. */
@Composable
private fun DeviceChoice(id: String, devices: List<DeviceOption>, preferences: CallPreferences, kind: DeviceMemory.Kind) {
    val options = listOf(GlassChoiceOption("", t("Same as the system"), symbol = "sparkles")) +
        devices.map { GlassChoiceOption(it.id, it.name) }
    GlassChoice(
        id,
        options,
        preferences.devices[kind.key] ?: "",
        { chosen -> preferences.choose(kind.key, chosen.ifEmpty { null }) },
        width = 280.dp,
    )
}

/**
 * The camera as the room will see it, with how loudly the microphone hears
 * you over its foot: the web's join screen answers exactly those two
 * questions about the two devices (PreJoin.tsx, MicLevel.tsx), and this is
 * where they are chosen ahead of any call.
 */
@Composable
private fun DevicePreview(check: DeviceCheck) {
    val shape = rounded(20f)
    val shown by animateFloatAsState(if (check.cameraRunning) 1f else 0f, Theme.settle(), label = "camera")
    Box(
        Modifier
            .fillMaxWidth()
            .aspectRatio(16f / 9f)
            .clip(shape)
            .border(1.dp, Theme.wash.copy(alpha = 0.08f), shape),
    ) {
        // A camera-off tile's grey, as CameraStage's framed preview is.
        Box(Modifier.matchParentSize().background(Theme.surface))
        check.Picture(Modifier.matchParentSize().alpha(shown))
        Appear(!check.cameraRunning, Modifier.align(Alignment.Center)) {
            Symbol(if (check.cameraRefused) "video.slash.fill" else "video.fill", size = 28f, weight = Font.Medium, tint = Theme.muted)
        }
        Row(
            Modifier
                .align(Alignment.BottomStart)
                .padding(12.dp)
                .height(32.dp)
                .glassSurface(Capsule)
                .padding(horizontal = 12.dp),
            horizontalArrangement = Arrangement.spacedBy(9.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Symbol(
                if (check.microphoneRefused) "mic.slash.fill" else "mic.fill",
                size = 12f,
                weight = Font.Semibold,
                tint = if (check.level > 0.08f) Theme.tally else Theme.foreground,
            )
            KitText(t("Hearing you"), font = Font(12f, Font.Medium), lines = 1)
            LevelMeter(check.level, segments = 12)
        }
    }
}
