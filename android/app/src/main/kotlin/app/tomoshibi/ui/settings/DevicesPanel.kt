package app.tomoshibi.ui.settings

import android.content.Context
import android.hardware.camera2.CameraManager
import android.media.AudioDeviceCallback
import android.media.AudioDeviceInfo
import android.media.AudioManager
import android.os.Handler
import android.os.Looper
import android.util.Log
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.Crossfade
import androidx.compose.animation.animateContentSize
import androidx.compose.animation.expandVertically
import androidx.compose.animation.shrinkVertically
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import app.tomoshibi.basics.Language
import app.tomoshibi.basics.Localization
import app.tomoshibi.basics.Theme
import app.tomoshibi.basics.t
import app.tomoshibi.media.AudioDevice
import app.tomoshibi.media.CallAudio
import app.tomoshibi.media.CameraChoice
import app.tomoshibi.model.AppModel
import app.tomoshibi.model.CallModel
import app.tomoshibi.model.MicrophoneLevel
import app.tomoshibi.ui.kit.ChoiceRow
import app.tomoshibi.ui.kit.Font
import app.tomoshibi.ui.kit.GlassShell
import app.tomoshibi.ui.kit.GlassSwitch
import app.tomoshibi.ui.kit.Hairline
import app.tomoshibi.ui.kit.KitText
import app.tomoshibi.ui.kit.LevelMeter
import app.tomoshibi.ui.kit.SettingsRow
import app.tomoshibi.ui.kit.Symbol
import app.tomoshibi.ui.kit.keepDeparting
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

/**
 * Which microphone, speakers and camera, and how the voice is treated
 * (components/room/DeviceMenu.tsx; DevicesPanel.swift), with the language
 * beside them. Drawn on its own glass shell, as the Swift panel is: the call
 * room gives it its width and a scroll, and no second pane under it.
 *
 * Each list ticks what the call is using now and follows the device: a
 * headset plugged in while the panel is open is in it at once, as the web's
 * menu listens for `devicechange`. A press moves the tick at once and moves
 * it back if the device would not come (DeviceMenu.tsx `select`), and a
 * choice that took is remembered for the next call.
 *
 * The speakers are a list, as on the Mac and on the web, rather than the
 * iPhone's system route button: Android has no such button to embed, and
 * the call routes its sound through AudioRouting's devices.
 */
@Composable
fun DevicesPanel(app: AppModel, call: CallModel, close: () -> Unit, modifier: Modifier = Modifier) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    val cameras = LocalCallCameras.current
    // Read as the panel is first drawn: lists filled a frame later had "No
    // devices found" showing, and leaving, as the panel arrived.
    val lists = remember(call, cameras) { DeviceLists(call, cameras).also { it.refresh() } }
    DisposableEffect(lists, context) {
        lists.start(context, scope)
        onDispose { lists.stop() }
    }
    // Shown from the camera's own state, which is not observed: read when the
    // panel opens and written by the switch.
    var blurring by remember(call) { mutableStateOf(call.blurring) }
    var changingOriginal by remember { mutableStateOf(false) }

    fun choose(device: AudioDevice, input: Boolean) {
        val before = if (input) lists.microphone else lists.speaker
        if (input) lists.microphone = device.id else lists.speaker = device.id
        try {
            call.useAudio(device, input)
        } catch (_: Throwable) {
            if (input) lists.microphone = before else lists.speaker = before
        }
        lists.refresh()
        app.preferences.refresh()
    }

    // A camera picked with the camera off is the one it comes on with: the
    // tick moves, and nothing is opened until then.
    fun chooseCamera(id: String) {
        val before = lists.camera
        lists.camera = id
        scope.launch {
            try {
                call.useCamera(id)
            } catch (fault: CancellationException) {
                throw fault
            } catch (_: Throwable) {
                lists.camera = before
            }
            app.preferences.refresh()
        }
    }

    fun setOriginal(on: Boolean) {
        if (changingOriginal) return
        lists.original = on
        changingOriginal = true
        scope.launch {
            // A failure is said by the call (deviceFailed), and the refresh
            // puts the switch back where the voice really is.
            try {
                call.setOriginalSound(on)
            } catch (fault: CancellationException) {
                throw fault
            } catch (_: Throwable) {
            }
            changingOriginal = false
            lists.refresh()
            app.preferences.refresh()
        }
    }

    GlassShell(modifier, cornerRadius = 22.dp) {
        Column(
            Modifier.padding(16.dp).animateContentSize(Theme.settle()),
            verticalArrangement = Arrangement.spacedBy(18.dp),
        ) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                KitText(t("Devices"), Modifier.weight(1f), font = Font(15f, Font.Semibold), color = Theme.foreground, lines = 1)
                CloseButton(t("Close"), close, Modifier.testTag("devices.close"))
            }

            // Why the lists cannot be chosen from: only the system's own
            // routing takes a device for each direction (CallAudio
            // `choosable`). Android's can always be asked, except in a call
            // that plays nothing, which has no devices to list either, so
            // only Original sound has anything to say here.
            Section(
                t("Microphone"),
                footer = if (!lists.choosable && call.audio.originalSound) t("Turn off Original sound to choose another here.") else null,
            ) {
                Arriving(lists.microphones.isEmpty()) { Empty() }
                Rows(lists.microphones, { it.device.id }) { one, _ ->
                    ChoiceRow(
                        one.name,
                        { choose(one.device, input = true) },
                        Modifier.testTag("devices.microphone.${one.device.id}"),
                        symbol = "mic",
                        checked = lists.microphone == one.device.id,
                        enabled = lists.choosable,
                    )
                }
                // How loud it is as it goes, while it is on (MicLevel.tsx):
                // the question somebody opening this panel is asking is "is it
                // this one", and a meter that moves when they speak answers it.
                Arriving(call.microphoneOn) { MicrophoneMeterRow(call.audio) }
                // Under the microphone because it is a property of the voice,
                // the way blur is a property of the picture; applied to the
                // live microphone, so the difference can be heard before
                // deciding.
                SettingsRow(
                    t("Original sound"),
                    detail = t("No noise suppression or automatic volume. For headphones and music."),
                    symbol = "waveform.path",
                    last = true,
                ) {
                    GlassSwitch(lists.original, ::setOriginal, Modifier.testTag("devices.original"), enabled = !changingOriginal,
                        label = t("Original sound"))
                }
            }

            Section(t("Speakers")) {
                Arriving(lists.speakers.isEmpty()) { Empty() }
                Rows(lists.speakers, { it.device.id }) { one, last ->
                    ChoiceRow(
                        one.name,
                        { choose(one.device, input = false) },
                        Modifier.testTag("devices.speakers.${one.device.id}"),
                        symbol = "speaker.wave.2",
                        checked = lists.speaker == one.device.id,
                        last = last,
                        enabled = lists.choosable,
                    )
                }
            }

            Section(t("Camera")) {
                Arriving(lists.cameras.isEmpty()) { Empty() }
                Rows(lists.cameras, { it.id }) { one, _ ->
                    ChoiceRow(
                        one.name,
                        { chooseCamera(one.id) },
                        Modifier.testTag("devices.camera.${one.id}"),
                        symbol = "video",
                        checked = lists.camera == one.id,
                    )
                }
                // The call's own blur, from the next frame, in what is sent
                // and in the picture of oneself.
                SettingsRow(t("Blur my background"), symbol = "person.and.background.dotted", last = true) {
                    GlassSwitch(
                        blurring,
                        { on ->
                            blurring = on
                            call.setBlur(on)
                        },
                        label = t("Blur my background"),
                    )
                }
            }

            Section(t("Language")) {
                val localization = Localization.shared
                Language.entries.forEachIndexed { index, one ->
                    ChoiceRow(
                        one.displayName,
                        { localization.choose(one) },
                        Modifier.testTag("devices.language.${one.code}"),
                        checked = localization.language == one,
                        last = index == Language.entries.lastIndex,
                    )
                }
            }
        }
    }
}

/** A device with the name it is listed under. */
internal data class NamedDevice(val device: AudioDevice, val name: String)

/**
 * The rows of a list that follows the devices, each arriving and leaving
 * with its room as a device is plugged in or taken away.
 */
@Composable
private fun <T> ColumnScope.Rows(items: List<T>, key: (T) -> Any, row: @Composable (T, Boolean) -> Unit) {
    val kept = keepDeparting(items, key).sortedBy { it.place }
    val lastShown = kept.lastOrNull { it.visible.targetState }
    for (entry in kept) {
        key(entry.key) {
            AnimatedVisibility(
                entry.visible,
                enter = Theme.arrive + expandVertically(Theme.settle()),
                exit = Theme.depart + shrinkVertically(Theme.leave()),
            ) { row(entry.item, entry === lastShown) }
        }
    }
}

/** An empty list says so, rather than being a heading over nothing. */
@Composable
private fun Empty() {
    Box(Modifier.fillMaxWidth().heightIn(min = 40.dp).padding(horizontal = 14.dp), contentAlignment = Alignment.CenterStart) {
        KitText(t("No devices found"), font = Font(13f), color = Theme.muted)
    }
}

/**
 * What the devices panel lists, and what the call is using, kept up to date
 * while the panel is open (DevicesPanel.swift `DeviceLists`).
 */
internal class DeviceLists(private val call: CallModel, private val cameraSource: CallCameras = DeviceCallCameras) {
    var microphones by mutableStateOf(emptyList<NamedDevice>())
    var speakers by mutableStateOf(emptyList<NamedDevice>())
    var cameras by mutableStateOf(emptyList<DeviceOption>())
    var microphone by mutableStateOf<String?>(null)
    var speaker by mutableStateOf<String?>(null)
    var camera by mutableStateOf<String?>(null)
    var choosable by mutableStateOf(true)
    /** Whether the voice goes as it is, read from the call: it is also turned on from outside the panel (a remembered choice at the join). */
    var original by mutableStateOf(false)
    private var watch: DeviceWatch? = null
    private var pending: Job? = null

    fun start(context: Context, scope: CoroutineScope) {
        refresh()
        watch = DeviceWatch(context) { soon(scope) }
    }

    fun stop() {
        watch?.stop()
        watch = null
        pending?.cancel()
    }

    /**
     * One refresh for a burst of changes: a headset connecting is added for
     * calls and for music, and the camera service says every camera is
     * there as the panel starts listening, within a few milliseconds.
     */
    private fun soon(scope: CoroutineScope) {
        if (pending?.isActive == true) return
        pending = scope.launch {
            delay(150)
            refresh()
        }
    }

    fun refresh() {
        microphones = named(call.audio.devices(input = true), input = true)
        speakers = named(call.audio.devices(input = false), input = false)
        cameras = try {
            DeviceNames.cameras(cameraSource.list(call))
        } catch (fault: Exception) {
            Log.w("Tomoshibi", "the cameras could not be listed: ${fault.message}")
            emptyList()
        }
        microphone = call.audio.current(input = true)
        speaker = call.audio.current(input = false)
        camera = cameraSource.current(call)
        choosable = call.audio.choosable
        original = call.audio.originalSound
    }

    private fun named(devices: List<AudioDevice>, input: Boolean): List<NamedDevice> {
        val byId = devices.associateBy { it.id }
        return DeviceNames.audio(devices, input).mapNotNull { option -> byId[option.id]?.let { NamedDevice(it, option.name) } }
    }
}

/**
 * The call's cameras as the panel reads them: the call's own capture, which
 * is made on first use and sized by the core, so a test that has no core
 * hands in its own.
 */
internal interface CallCameras {
    fun list(call: CallModel): List<CameraChoice>
    /** The camera that is sending, or that would be if the camera went on now. */
    fun current(call: CallModel): String?
}

internal object DeviceCallCameras : CallCameras {
    override fun list(call: CallModel): List<CameraChoice> = call.camera.cameras()
    override fun current(call: CallModel): String? = call.camera.current
}

internal val LocalCallCameras = staticCompositionLocalOf<CallCameras> { DeviceCallCameras }

/**
 * Told when microphones, speakers or cameras come and go: the panel listed
 * what was there when it opened, so a headset plugged in with it open was
 * not in it until it was closed and opened again (DeviceMenu.tsx listens to
 * `devicechange` for the same reason).
 */
private class DeviceWatch(context: Context, private val changed: () -> Unit) {
    private val handler = Handler(Looper.getMainLooper())
    private val audio = context.getSystemService(AudioManager::class.java)
    private val cameras = context.getSystemService(CameraManager::class.java)
    private val audioCallback = object : AudioDeviceCallback() {
        override fun onAudioDevicesAdded(addedDevices: Array<out AudioDeviceInfo>?) = changed()
        override fun onAudioDevicesRemoved(removedDevices: Array<out AudioDeviceInfo>?) = changed()
    }
    private val cameraCallback = object : CameraManager.AvailabilityCallback() {
        override fun onCameraAvailable(cameraId: String) = changed()
        override fun onCameraUnavailable(cameraId: String) = changed()
    }

    init {
        runCatching { audio?.registerAudioDeviceCallback(audioCallback, handler) }
        runCatching { cameras?.registerAvailabilityCallback(cameraCallback, handler) }
    }

    fun stop() {
        runCatching { audio?.unregisterAudioDeviceCallback(audioCallback) }
        runCatching { cameras?.unregisterAvailabilityCallback(cameraCallback) }
    }
}

/**
 * The microphone's level as it is sent, and what that means in words
 * (MicLevel.tsx; DevicesPanel.swift `MicrophoneMeterRow`): "Say something"
 * until anything has been heard, "Hearing you" once it has, "No sound from
 * it" when the microphone hands over nothing at all, which a meter that
 * never moves cannot tell from silence. A microphone that is there hears
 * the room's own hiss; one that delivers nothing, or silence written in its
 * place by a system that refused it, reads exactly nought for two seconds.
 */
@Composable
private fun MicrophoneMeterRow(audio: CallAudio) {
    var level by remember { mutableFloatStateOf(0f) }
    var heard by remember { mutableStateOf(false) }
    var silent by remember { mutableStateOf(false) }
    LaunchedEffect(audio) {
        val opened = withFrameNanos { it }
        var anything = false
        while (true) {
            withFrameNanos { now ->
                val rms = audio.microphoneLevel?.rms ?: 0.0
                if (rms > 0) anything = true
                val shown = MicrophoneLevel.shown(rms)
                level = shown.toFloat()
                if (shown > MicrophoneLevel.HEARD) heard = true
                silent = !anything && now - opened > 2_000_000_000L
            }
            // Read about thirty times a second, as the Swift's timeline is.
            delay(33)
        }
    }
    Column {
        Row(
            Modifier.fillMaxWidth().heightIn(min = 40.dp).padding(horizontal = 14.dp),
            horizontalArrangement = Arrangement.spacedBy(12.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Box(Modifier.width(26.dp), contentAlignment = Alignment.Center) {
                Symbol("waveform", size = 12.5f, weight = Font.Semibold, tint = Theme.muted)
            }
            Box(Modifier.semantics { contentDescription = t("Microphone level") }) { LevelMeter(level, segments = 18) }
            Box(Modifier.weight(1f).widthIn(min = 8.dp))
            val words = if (silent) t("No sound from it") else if (heard) t("Hearing you") else t("Say something")
            Crossfade(words, animationSpec = Theme.settle(), label = "heard") { said ->
                KitText(said, font = Font(11.5f), color = if (heard && !silent) Theme.foreground else Theme.muted, lines = 1)
            }
        }
        Hairline(52.dp)
    }
}
