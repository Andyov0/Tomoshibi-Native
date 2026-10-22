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
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
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
import app.tomoshibi.media.CameraCapture
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
import kotlin.coroutines.CoroutineContext
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.awaitCancellation
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.flow.drop
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

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
    // The lists the room read ahead and keeps with the call (DeviceLists
    // `keep`), drawn as they are: nothing is asked of the system in the frame
    // the panel opens in. Asking then, twice, for every camera's whole
    // description and every sound device, held the main thread through the
    // first frames of the panel's slide, and the system's answer to the
    // device listeners registered then came back mid-slide as a third read.
    val lists = remember(call, cameras) { DeviceLists.of(call, cameras) }
    LaunchedEffect(lists, context) { lists.follow(context) }
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
        lists.reread()
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
            lists.reread()
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
                Arriving(lists.known && lists.microphones.isEmpty()) { Empty() }
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
                Arriving(lists.known && lists.speakers.isEmpty()) { Empty() }
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
                Arriving(lists.known && lists.cameras.isEmpty()) { Empty() }
                // The last camera ends the section where the blur below it
                // is not offered.
                val blurOffered = call.blurOffered
                Rows(lists.cameras, { it.id }) { one, last ->
                    ChoiceRow(
                        one.name,
                        { chooseCamera(one.id) },
                        Modifier.testTag("devices.camera.${one.id}"),
                        symbol = "video",
                        checked = lists.camera == one.id,
                        last = last && !blurOffered,
                    )
                }
                // The call's own blur, from the next frame, in what is sent
                // and in the picture of oneself. Gone where the deployment has
                // switched blur off (Remote); a blur already on stays on for
                // this call (CallModel `blurKept`).
                AnimatedVisibility(blurOffered, enter = Theme.arrive + expandVertically(Theme.settle()), exit = Theme.depart + shrinkVertically(Theme.leave())) {
                    SettingsRow(t("Blur my background"), Modifier.testTag("devices.blur"), symbol = "person.and.background.dotted", last = true) {
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
 *
 * The rows already listed as the panel opens come in with the panel, which
 * slides and fades in whole, rather than each opening its own room as well:
 * a dozen rows growing from nothing at once made the panel itself grow under
 * its glass for the length of its slide, measured again and its blur and
 * shadow drawn again at a new size every frame.
 */
@Composable
private fun <T> ColumnScope.Rows(items: List<T>, key: (T) -> Any, row: @Composable (T, Boolean) -> Unit) {
    val kept = keepDeparting(items, arriveAtFirst = false, key = key).sortedBy { it.place }
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
 *
 * Kept with the call rather than with the panel ([of]), and read ahead by the
 * room ([keep]), so that the panel opens on lists already made. The reading
 * itself is a question to the audio service for each direction and to the
 * camera service for every camera's description, over binder, which is done
 * on [CallCameras.reading] (a background thread on a phone); only the naming
 * and the ticks, which ask the call, are done on the main thread. A list that
 * is not known yet is not "No devices found": that showing and leaving as
 * the panel arrived is what reading in the panel's first frame was for.
 */
internal class DeviceLists(private val call: CallModel, private val cameraSource: CallCameras = DeviceCallCameras) {
    /** What the system listed, as it listed it: named where it is drawn, so a language chosen renames it at once. */
    data class Found(val microphones: List<AudioDevice>, val speakers: List<AudioDevice>, val cameras: List<CameraChoice>)

    private var found by mutableStateOf<Found?>(null)

    val microphones: List<NamedDevice> by derivedStateOf { named(found?.microphones.orEmpty(), input = true) }
    val speakers: List<NamedDevice> by derivedStateOf { named(found?.speakers.orEmpty(), input = false) }
    val cameras: List<DeviceOption> by derivedStateOf { DeviceNames.cameras(found?.cameras.orEmpty()) }

    /** Whether the system has been asked yet. */
    val known: Boolean get() = found != null

    var microphone by mutableStateOf<String?>(null)
    var speaker by mutableStateOf<String?>(null)
    var camera by mutableStateOf<String?>(null)
    var choosable by mutableStateOf(true)
    /** Whether the voice goes as it is, read from the call: it is also turned on from outside the panel (a remembered choice at the join). */
    var original by mutableStateOf(false)
    private var pending: Job? = null
    /** Reads begun and the latest one listed: two may overlap (the room's and the panel's), and an older answer is not listed over a newer one. */
    private var asked = 0
    private var listed = 0

    /**
     * Asks the system what there is, away from the main thread, and lists it
     * here. Called on the main thread.
     */
    suspend fun refresh(context: Context) {
        val ticket = ++asked
        val answer = withContext(cameraSource.reading) { read(context) }
        if (ticket < listed) return
        listed = ticket
        found = answer
        reread()
    }

    /** What the system lists: safe on any thread, and not for the main one (binder, per camera). */
    internal fun read(context: Context): Found {
        fun audio(input: Boolean): List<AudioDevice> = try {
            call.audio.devices(input)
        } catch (fault: Exception) {
            Log.w("Tomoshibi", "the sound devices could not be listed: ${fault.message}")
            emptyList()
        }
        val cameras = try {
            cameraSource.list(context, call)
        } catch (fault: Exception) {
            Log.w("Tomoshibi", "the cameras could not be listed: ${fault.message}")
            emptyList()
        }
        return Found(audio(input = true), audio(input = false), cameras)
    }

    /**
     * The ticks alone, read again from the call after a choice: what it holds,
     * and the way out its routing takes. The cameras ticked among are the ones
     * last listed, rather than the camera service asked again on a press.
     */
    fun reread() {
        microphone = call.audio.current(input = true)
        speaker = call.audio.current(input = false)
        camera = try {
            cameraSource.current(call, found?.cameras.orEmpty())
        } catch (fault: Exception) {
            Log.w("Tomoshibi", "the camera in use could not be read: ${fault.message}")
            null
        }
        choosable = call.audio.choosable
        original = call.audio.originalSound
    }

    /**
     * While the room is shown: the lists read once the room has settled, and
     * again whenever the call hears a microphone or a way out come or go
     * (CallAudio `devicesChanged`), so that whenever the panel opens what it
     * lists is already there and already current.
     */
    suspend fun keep(context: Context) {
        delay(AHEAD)
        refresh(context)
        snapshotFlow { call.audio.devicesChanged }.drop(1).collectLatest {
            delay(BURST)
            refresh(context)
        }
    }

    /**
     * While the panel is open: the lists read again once it has arrived, and
     * whenever a device comes or goes (DeviceWatch), until cancelled. Lists
     * the room had not read yet are read at once, still off the main thread.
     * Reading again after the arrival rather than during it means a list that
     * moves moves nothing that is still moving.
     */
    suspend fun follow(context: Context): Nothing = coroutineScope {
        if (known) {
            // The ticks are the call's, and cheap to ask: current from the
            // first frame on, in case the camera was turned or a choice made
            // somewhere else since the lists were read.
            reread()
            delay(SETTLING)
        } else {
            refresh(context)
        }
        val watch = DeviceWatch(context) { soon(this, context) }
        try {
            // A listener is answered at once with everything already there
            // (AudioDeviceCallback's onAudioDevicesAdded and the camera
            // service's AvailabilityCallback both say so): that burst and the
            // read after the arrival are one read.
            soon(this, context)
            awaitCancellation()
        } finally {
            watch.stop()
            pending?.cancel()
        }
    }

    /**
     * One read for a burst of changes: a headset connecting is added for
     * calls and for music, and the camera service says every camera is
     * there as the panel starts listening, within a few milliseconds.
     */
    private fun soon(scope: CoroutineScope, context: Context) {
        if (pending?.isActive == true) return
        pending = scope.launch {
            delay(BURST)
            refresh(context)
        }
    }

    private fun named(devices: List<AudioDevice>, input: Boolean): List<NamedDevice> {
        val byId = devices.associateBy { it.id }
        return DeviceNames.audio(devices, input).mapNotNull { option -> byId[option.id]?.let { NamedDevice(it, option.name) } }
    }

    companion object {
        private const val PART = "devices"

        /** How long after the room appears the lists are first read: out of the way of the join, which opens the camera. */
        const val AHEAD = 1500L

        /** How long the panel takes to arrive (Theme.settle's 320 ms), and a frame or two. */
        const val SETTLING = 360L

        /** How long a burst of device news is waited out for. */
        const val BURST = 150L

        /** The call's lists, made the first time either the room or the panel asks. */
        fun of(call: CallModel, cameras: CallCameras = DeviceCallCameras): DeviceLists =
            call.parts.getOrPut(PART) { DeviceLists(call, cameras) } as DeviceLists
    }
}

/**
 * The call's cameras as the panel reads them, so a test that has no core and
 * no camera service hands in its own.
 */
internal interface CallCameras {
    /** The cameras there are: the camera service asked about each, so read on [reading]. */
    fun list(context: Context, call: CallModel): List<CameraChoice>

    /** Which of [present] is sending, or would be if the camera went on now: from what the call holds, on the main thread. */
    fun current(call: CallModel, present: List<CameraChoice>): String?

    /**
     * Where the devices are read: the system's services answer over binder,
     * which does not belong on the main thread; a test's stand-ins answer
     * where they are asked, so that what a test draws does not depend on a
     * thread finishing in time.
     */
    val reading: CoroutineContext get() = Dispatchers.IO
}

internal object DeviceCallCameras : CallCameras {
    override fun list(context: Context, call: CallModel): List<CameraChoice> = CameraCapture.cameras(context)

    // A camera not made yet has nothing picked and nothing running, which
    // leaves the rule for when nothing was picked; asking it would make one.
    override fun current(call: CallModel, present: List<CameraChoice>): String? =
        call.cameraIfMade?.current(present) ?: CameraCapture.fallback(present)?.id
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
            Box(Modifier.semantics { contentDescription = t("Microphone level") }) { Meter { level } }
            Box(Modifier.weight(1f).widthIn(min = 8.dp))
            val words = if (silent) t("No sound from it") else if (heard) t("Hearing you") else t("Say something")
            Crossfade(words, animationSpec = Theme.settle(), label = "heard") { said ->
                KitText(said, font = Font(11.5f), color = if (heard && !silent) Theme.foreground else Theme.muted, lines = 1)
            }
        }
        Hairline(52.dp)
    }
}

/**
 * The meter alone, reading the level where it is drawn: read in the row, the
 * whole row (its symbol, its words and their crossfade) was composed again
 * thirty times a second for as long as the panel was open.
 */
@Composable
private fun Meter(level: () -> Float) = LevelMeter(level(), segments = 18)
