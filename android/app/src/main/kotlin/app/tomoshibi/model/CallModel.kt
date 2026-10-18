package app.tomoshibi.model

import android.os.Handler
import android.os.Looper
import android.util.Log
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import app.tomoshibi.basics.t
import app.tomoshibi.media.AudioDevice
import app.tomoshibi.media.CallAudio
import app.tomoshibi.media.CallMedia
import app.tomoshibi.media.CameraCapture
import app.tomoshibi.media.DeviceAccess
import app.tomoshibi.media.DeviceKind
import app.tomoshibi.media.DeviceRefused
import app.tomoshibi.media.LosslessPlayback
import app.tomoshibi.media.PictureInPicture
import app.tomoshibi.media.Sharing
import app.tomoshibi.media.VideoHub
import java.time.Instant
import java.util.UUID
import java.util.concurrent.atomic.AtomicLong
import kotlin.coroutines.CoroutineContext
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import uniffi.tomoshibi_core.AudioInput
import uniffi.tomoshibi_core.Call
import uniffi.tomoshibi_core.CallEvent
import uniffi.tomoshibi_core.CallObserver
import uniffi.tomoshibi_core.CallOptions
import uniffi.tomoshibi_core.CallState
import uniffi.tomoshibi_core.LinkQuality
import uniffi.tomoshibi_core.MediaKind
import uniffi.tomoshibi_core.ParticipantInfo
import uniffi.tomoshibi_protocol.StrainAccept
import uniffi.tomoshibi_core.TrackInfo
import uniffi.tomoshibi_core.VideoInput
import uniffi.tomoshibi_core.VideoKind
import uniffi.tomoshibi_core.joinCall
import uniffi.tomoshibi_protocol.ConnectionReading
import uniffi.tomoshibi_protocol.Deployment
import uniffi.tomoshibi_protocol.MediaSource
import uniffi.tomoshibi_protocol.SoundKind
import uniffi.tomoshibi_protocol.StrainHint

/** One person in the call, as the screens see them. */
data class Person(
    val id: String,
    val name: String,
    val attributes: Map<String, String>,
    val tracks: Map<String, TrackInfo>,
    val quality: LinkQuality = LinkQuality.GOOD,
    val speaking: Boolean = false,
    /** Whether their sealed frames can be read; null until the call says. */
    val readable: Boolean? = null,
) {
    val handUp: Boolean get() = attributes["hand"] == "up"

    fun track(source: MediaSource): TrackInfo? = tracks.values.firstOrNull { it.source == source }

    val musicTrack: TrackInfo? get() = tracks.values.firstOrNull { it.source == MediaSource.UNKNOWN && it.name.startsWith("music") }
}

/**
 * The unit of layout: a picture, not a person (the web's `Surface`). Someone
 * sharing their screen with their camera on is two of these.
 */
data class Surface(
    val id: String,
    val person: Person,
    val kind: Kind,
    /** The picture's track, when there is one and it is not muted. */
    val sid: String?,
) {
    enum class Kind { Camera, Screen }
}

/**
 * One thing said in the corner of a call: a notice (web/src/live/notices.ts).
 *
 * The web's rule is about lifetime rather than severity. Something that
 * happened is said here and fades; something still true is a banner over the
 * stage (`CallModel.notice`, "Reconnecting"). A notice about something over
 * carries no way to close it, because a target that lives two seconds is one
 * nobody hits; one somebody may want to deal with now, or not at all, has the
 * close button ([closable]).
 */
data class CallToast(
    val title: String,
    val detail: String? = null,
    /** What it is about, said by its edge (Notices.tsx: "the edge is the whole of the severity system"). */
    val kind: Kind = Kind.Info,
    /** Seconds before it goes by itself, or null to stay until dismissed. */
    val lasts: Double? = NoticeTimes.BRIEFLY,
    /** The words on its button, when it has one. */
    val actionTitle: String? = null,
    /** A second toast with the same key replaces the first, in place: one "Reconnecting" rather than three. */
    val key: String? = null,
    /** Whether it carries a close button (notices.ts `closeButton`); unless told, only one that would otherwise never go. */
    val closable: Boolean = lasts == null,
    /** Something drawn under its words, by the name whoever raised it gave `CallModel.attach`. */
    val attachment: String? = null,
    /** Kept when it is said again under the same key, so the card stays where it is and only its words change. */
    val id: Long = nextToastId.incrementAndGet(),
) {
    enum class Kind { Info, Signal, Warning, Failure }

    private companion object {
        val nextToastId = AtomicLong()
    }
}

data class ChatLine(
    val id: String,
    val identity: String,
    val name: String,
    val text: String,
    val at: Instant,
    val mine: Boolean,
)

/** Where a move the server announced is taking this call, and when it said so. */
data class Moving(val relay: String, val atNanos: Long)

/**
 * A call and everything the screens draw from it (the Apple apps' CallModel).
 *
 * The core speaks on its own threads; every event is carried to the main
 * thread in the order it was said, applied here, and Compose redraws from the
 * snapshot state this keeps. Remote sound is played the moment it is
 * subscribed, because a participant nobody can hear is a fault nobody can
 * see.
 *
 * Every method is for the main thread.
 */
class CallModel internal constructor(
    val call: Call,
    /** The deployment the call was joined through: host controls, watching together and the song desk all speak to it. */
    val deployment: Deployment,
    /** The join answer's region for the relay this call came in through, when the server said. */
    val region: String?,
    /** Whether the call is sealed; watching together, the song desk's broadcast and the lossless stream switch themselves off in one. */
    val sealed: Boolean,
    val audio: CallAudio,
    makeCamera: () -> CameraCapture,
    /** The share from this device, kept with the call so that leaving, or being ended, always stops it. */
    val sharing: Sharing,
    val pictureInPicture: PictureInPicture,
    context: CoroutineContext = Dispatchers.Main.immediate,
) {
    val room: String = call.roomName()
    val localIdentity: String = call.localIdentity()
    val localName: String = call.localName()

    /**
     * The camera, made the first time it is wanted: a call that never turns
     * it on never opens the camera service, and leaving closes only one that
     * was made.
     */
    val camera: CameraCapture by lazy {
        cameraMade = true
        makeCamera().also { it.onFailure = { fault -> deviceFailed(DeviceKind.Camera, fault) } }
    }
    private var cameraMade = false

    /** Lossless streams standing in for somebody's shared sound or music. */
    val lossless = LosslessPlayback(audio)

    private val job = SupervisorJob()
    /** Work that belongs to this call and ends with it: notices' timers, the corner's messages. */
    val scope = CoroutineScope(job + context)

    /**
     * What the join answer called the machine this call was sent to, and the
     * machine the meeting is actually on where that is another one (the web's
     * `relay` and `carrying`, which the signal badge's detail names).
     */
    var relay by mutableStateOf<String?>(null)
        internal set
    var holding by mutableStateOf<String?>(null)
        internal set

    var state by mutableStateOf(CallState.CONNECTED)
        private set
    var people by mutableStateOf<List<Person>>(emptyList())
        private set
    var messages by mutableStateOf<List<ChatLine>>(emptyList())
        private set
    var endedReason by mutableStateOf<String?>(null)
        private set
    var localTracks by mutableStateOf<Map<String, TrackInfo>>(emptyMap())
        private set

    /** What others said in the last six seconds, for the corner while the messages are closed (live/chat.ts SAID_FOR). */
    var arriving by mutableStateOf<List<ChatLine>>(emptyList())
        private set
    /** Something was said while the messages were closed (useChat.ts). */
    var unread by mutableStateOf(false)
        private set

    private var chatOpenState by mutableStateOf(false)
    /** Whether the messages panel is open; set by the room view. */
    var chatOpen: Boolean
        get() = chatOpenState
        set(value) {
            chatOpenState = value
            if (value) {
                unread = false
                arriving = emptyList()
            }
        }

    /**
     * Something still true, said as a line across the top of the call for as
     * long as it is (the web's banners). What happened and is over is a toast
     * instead, in the corner.
     */
    var notice by mutableStateOf<String?>(null)

    /** What the call says in the corner, oldest first (notices.ts). At most three that go by themselves. */
    var toasts by mutableStateOf<List<CallToast>>(emptyList())
        private set
    /** Whether the corner is being read, under a finger: every countdown waits until it is not. */
    var toastsHeld by mutableStateOf(false)
        private set
    private val toastActions = HashMap<Long, () -> Unit>()
    private val toastClocks = HashMap<Long, NoticeClock>()
    private val toastTimers = HashMap<Long, Job>()
    /** What a toast's `attachment` names, drawn under its words: a composable, kept as Any so the model needs no UI. */
    val attachments = HashMap<String, Any>()

    /** Says [toast]; one with an action calls it when its button is pressed. */
    fun announce(toast: CallToast, action: (() -> Unit)? = null) {
        var said = toast
        val existing = toast.key?.let { key -> toasts.indexOfFirst { it.key == key } } ?: -1
        if (existing >= 0) {
            said = toast.copy(id = toasts[existing].id)
            stopClock(said.id)
            toasts = toasts.toMutableList().also { it[existing] = said }
        } else {
            val next = (toasts + said).toMutableList()
            // The fourth pushes out the oldest one that goes by itself; one
            // somebody has to act on is never what is lost.
            for (index in NoticeTimes.overflow(next.map { it.lasts == null }).reversed()) {
                forget(next[index].id)
                next.removeAt(index)
            }
            toasts = next
        }
        if (action != null) toastActions[said.id] = action else toastActions.remove(said.id)
        said.lasts?.let { lasts ->
            toastClocks[said.id] = NoticeClock(lasts, now(), running = !toastsHeld)
            runClock(said.id)
        }
        Log.i(TAG, "notice: ${said.title}${said.detail?.let { " ($it)" } ?: ""}")
    }

    fun dismiss(id: Long) {
        forget(id)
        toasts = toasts.filter { it.id != id }
    }

    /** Takes down whatever was said under [key], if anything still is. */
    fun dismiss(key: String) {
        val toast = toasts.firstOrNull { it.key == key } ?: return
        dismiss(toast.id)
    }

    /** The toast's button was pressed. */
    fun act(id: Long) {
        val action = toastActions[id]
        dismiss(id)
        action?.invoke()
    }

    /** Holds every countdown while the corner is read, and lets them run on once it is not, each with what it had left. */
    fun holdToasts(held: Boolean) {
        if (held == toastsHeld) return
        toastsHeld = held
        val now = now()
        for (id in toastClocks.keys.toList()) {
            val clock = toastClocks[id] ?: continue
            if (held) {
                toastClocks[id] = clock.hold(now)
                toastTimers.remove(id)?.cancel()
            } else {
                toastClocks[id] = clock.run(now)
                runClock(id)
            }
        }
    }

    /** Registers what a toast's `attachment` draws. */
    fun attach(name: String, draw: Any) {
        attachments[name] = draw
    }

    private fun runClock(id: Long) {
        toastTimers.remove(id)?.cancel()
        val due = toastClocks[id]?.due(now()) ?: return
        toastTimers[id] = scope.launch {
            delay((due * 1000).toLong())
            dismiss(id)
        }
    }

    private fun stopClock(id: Long) {
        toastTimers.remove(id)?.cancel()
        toastClocks.remove(id)
    }

    private fun forget(id: Long) {
        stopClock(id)
        toastActions.remove(id)
    }

    private fun now(): Double = System.nanoTime() / 1e9

    // MARK: The web's notices (notices.ts), for whatever raises them

    /** Something somebody pressed did not take (`actionFailed`): it fades, because pressing again is the whole of the remedy. */
    fun actionFailed(reason: String) {
        announce(CallToast(reason, kind = CallToast.Kind.Failure, lasts = NoticeTimes.AT_MOST))
    }

    /** Something worked where working is not visible on its own (`actionDone`). */
    fun actionDone(said: String) {
        announce(CallToast(said, lasts = NoticeTimes.AT_MOST))
    }

    /**
     * Says that a camera or a microphone would not start (notices.ts
     * `deviceFailed`): ten seconds at most, with a close button, and taken
     * down at once by the next start of the same device that works.
     *
     * Two whole phrases rather than one with the device put in, because a
     * sentence built around a noun has to agree with it in most languages.
     * Where the system refused, the toast says where to allow it and has a
     * button that goes there; anything else is the web's "Something else may
     * be using it.", because pointing somebody at a permission they already
     * gave sends them looking in the one place the fault is not.
     */
    fun deviceFailed(kind: DeviceKind, error: Throwable) {
        val refused = error is DeviceRefused
        val title = if (kind == DeviceKind.Camera) t("Can't use your camera") else t("Can't use your microphone")
        val toast = CallToast(
            title,
            detail = if (refused) t("Allow Tomoshibi in Settings, then try again.") else t("Something else may be using it."),
            kind = CallToast.Kind.Failure,
            lasts = NoticeTimes.AT_MOST,
            actionTitle = if (refused) t("Open Settings") else null,
            key = kind.noticeKey,
            closable = true,
        )
        if (refused) announce(toast) { DeviceAccess.openSettings() } else announce(toast)
    }

    /** The uplink card for this participant's own share, while it stands (strain.ts). */
    var strainHint by mutableStateOf<StrainHint?>(null)
        private set

    /** The person pressed the uplink card's button: Apply carries the setting to restart the share at. */
    fun strainAccept(): StrainAccept = call.strainAccept()

    /** How the restart the card asked for went; whether the card goes. */
    fun strainAccepted(took: Boolean): Boolean = call.strainAccepted(took)

    /** The person put the card away for this share. */
    fun strainDecline() = call.strainDecline()

    /** This participant's own connection, as the server grades it. */
    var localQuality by mutableStateOf(LinkQuality.GOOD)
        private set
    /** How the call is going from here, every two seconds (connection.ts): null until the first sample. */
    var connection by mutableStateOf<ConnectionReading?>(null)
        private set

    private var connectionOpenState by mutableStateOf(false)
    /** Whether the badge's figures are open, kept between calls under the web's name (Signal.tsx `DETAIL_KEY`), where [memory] keeps things. */
    var connectionOpen: Boolean
        get() = connectionOpenState
        set(value) {
            if (value == connectionOpenState) return
            connectionOpenState = value
            memory.defaults?.set(SIGNAL_DETAIL_KEY, value)
        }

    var microphoneOn by mutableStateOf(false)
        private set
    var cameraOn by mutableStateOf(false)
        private set
    var handUp by mutableStateOf(false)
        private set

    /** The relay a move is taking this call to, between the server saying so and the room closing (web App.tsx ~560-627). */
    var moving by mutableStateOf<Moving?>(null)
        private set

    /** The call ended because the server moved it; the app joins again. */
    var onMoved: ((String) -> Unit)? = null

    /** The call ended under this participant for any other reason: the app says which end it was. */
    var onEnded: ((String) -> Unit)? = null

    private val listeners = ArrayList<(CallEvent) -> Unit>()

    /**
     * Everybody else who wants to hear what the call says: the protocols
     * layered over it (watching together, the song desk, hosting) each
     * subscribe here and pick out what concerns them. Called on the main
     * thread after the model itself has applied the event.
     */
    fun listen(listener: (CallEvent) -> Unit) {
        listeners.add(listener)
    }

    /** The identities of everybody else here, for protocols that keep a roster. */
    val identities: List<String> get() = people.map { it.id }

    /** Panels and stage pieces layered over the call, created once per call by whoever owns them. */
    val parts = HashMap<String, Any>()

    private var microphoneInput: AudioInput? = null
    private var cameraInput: VideoInput? = null
    private var cameraStarting = false
    private var microphoneStarting = false
    /**
     * Left, or ended under it. Leave can be pressed while the microphone or
     * the camera is still starting, and a start that woke after the call had
     * gone would have opened the device again with nobody to send to.
     */
    private var over = false
    /** Whether the join has finished telling this model who was already here; until then nobody arriving is announced. */
    internal var settled = false

    init {
        audio.onNews = { heard(it) }
    }

    // MARK: What the screens read

    val surfaces: List<Surface>
        get() = people.flatMap { person ->
            val out = mutableListOf(
                Surface(person.id + "/camera", person, Surface.Kind.Camera, person.track(MediaSource.CAMERA)?.takeIf { !it.muted }?.sid),
            )
            person.track(MediaSource.SCREEN_SHARE)?.let { out.add(Surface(person.id + "/screen", person, Surface.Kind.Screen, it.sid)) }
            out
        }

    /**
     * What the small window shows when the app is in the background: a shared
     * screen first, then the one other person in a call of two, then whoever
     * is speaking with a camera on.
     */
    val pictureInPictureSid: String?
        get() {
            val all = surfaces
            all.firstOrNull { it.kind == Surface.Kind.Screen && it.sid != null }?.let { return it.sid }
            val cameras = all.filter { it.kind == Surface.Kind.Camera && it.sid != null }
            if (cameras.size == 1) return cameras[0].sid
            return cameras.firstOrNull { it.person.speaking }?.sid ?: cameras.firstOrNull()?.sid
        }

    fun person(identity: String): Person? = people.firstOrNull { it.id == identity }

    // MARK: What the person does

    /**
     * Where this participant's choices are kept, the devices and the rest a
     * call remembers: under the web's names for a real call, nowhere for a
     * rehearsal. Nowhere until it is set, so that what the join turns on
     * before setting it is never remembered into the owner's settings from a
     * rehearsal.
     */
    var memory: DeviceMemory = DeviceMemory.nothing
        set(value) {
            field = value
            connectionOpenState = value.defaults?.bool(SIGNAL_DETAIL_KEY) ?: false
            sharing.keep(value.defaults)
        }

    /**
     * The microphone on or off. A microphone that will not start says so
     * itself (`deviceFailed`), wherever it was asked from: the bar, the join.
     */
    suspend fun setMicrophone(on: Boolean) {
        if (on) {
            // Once, whoever asks: a press while the first was still
            // publishing, or still asking the system, published a second
            // voice track and left the first one up and unmuted.
            if (microphoneStarting) return
            microphoneStarting = true
            try {
                try {
                    DeviceAccess.ensure(DeviceKind.Microphone)
                    if (over) return
                    if (microphoneInput == null) {
                        val input = call.publishAudio(SoundKind.Voice(audio.originalSound))
                        Log.i(TAG, "voice published as ${input.sid()}")
                        microphoneInput = input
                        if (over) return
                    }
                    audio.startMicrophone(microphoneInput!!)
                    microphoneInput?.setMuted(false)
                } catch (fault: CancellationException) {
                    throw fault
                } catch (fault: Throwable) {
                    // Muted at the track, so that nobody sees a microphone on
                    // that sends nothing.
                    microphoneInput?.setMuted(true)
                    deviceFailed(DeviceKind.Microphone, fault)
                    throw fault
                }
                // What said it would not start is no longer true.
                dismiss(DeviceKind.Microphone.noticeKey)
                dismiss(SOUND_STOPPED_KEY)
            } finally {
                microphoneStarting = false
            }
        } else {
            microphoneInput?.setMuted(true)
            audio.stopMicrophone()
        }
        microphoneOn = on
    }

    /** Blurs the background behind the camera or stops, and remembers it for the next call as the web does. */
    val blurring: Boolean get() = cameraMade && camera.blurring

    fun setBlur(on: Boolean) {
        camera.blurring = on
        memory.rememberBlur(on)
    }

    /**
     * The web's "original sound" (sound.ts `applyOriginalSound`): the system's
     * voice processing off and the voice republished at the higher rate,
     * because nothing is taken out of it any more. Remembered once it took.
     */
    suspend fun setOriginalSound(original: Boolean) {
        if (original == audio.originalSound) return
        try {
            audio.setOriginalSound(original)
            microphoneInput?.let { old ->
                val fresh = call.publishAudio(SoundKind.Voice(original))
                microphoneInput = fresh
                quietly { old.unpublish() }
                if (microphoneOn) audio.startMicrophone(fresh) else fresh.setMuted(true)
            }
        } catch (fault: CancellationException) {
            throw fault
        } catch (fault: Throwable) {
            deviceFailed(DeviceKind.Microphone, fault)
            throw fault
        }
        dismiss(DeviceKind.Microphone.noticeKey)
        memory.remember(originalSound = original)
    }

    /** Sends another camera instead, keeping the same track; with the camera off, the one it turns on with. Remembered once it took. */
    suspend fun useCamera(id: String) {
        val input = cameraInput
        if (input != null) {
            try {
                camera.start(id, input)
            } catch (fault: CancellationException) {
                throw fault
            } catch (fault: Throwable) {
                deviceFailed(DeviceKind.Camera, fault)
                throw fault
            }
            dismiss(DeviceKind.Camera.noticeKey)
        } else {
            camera.preferred = id
        }
        memory.remember(DeviceMemory.Kind.Camera, id)
    }

    /**
     * Another microphone or speakers, remembered once they took. A pair of
     * speakers that will not take says nothing, as on the web: the sound stays
     * where it was.
     */
    fun useAudio(device: AudioDevice, input: Boolean) {
        try {
            audio.use(device, input)
        } catch (fault: Throwable) {
            if (input) deviceFailed(DeviceKind.Microphone, fault)
            throw fault
        }
        if (input && microphoneOn) dismiss(DeviceKind.Microphone.noticeKey)
        memory.remember(if (input) DeviceMemory.Kind.Microphone else DeviceMemory.Kind.Speakers, device.id)
    }

    /**
     * The devices last picked, where they are still there, and the voice as
     * it was last sent (preferences.ts `restoreDevices`). Before the
     * microphone and camera start, so the first track is from the right
     * device. It cannot fail the join: a device that is gone leaves the
     * system's default.
     */
    fun restoreDevices() {
        memory.restorable(DeviceMemory.Kind.Camera, camera.cameras().map { it.id })?.let { camera.preferred = it }
        val microphone = memory.restorable(DeviceMemory.Kind.Microphone, audio.devices(input = true).map { it.id })
        val speakers = memory.restorable(DeviceMemory.Kind.Speakers, audio.devices(input = false).map { it.id })
        audio.prefer(microphone, speakers, memory.originalSound)
    }

    suspend fun setCamera(on: Boolean) {
        if (on) {
            // Once: a second press while the first was still asking would
            // publish a second camera.
            if (cameraInput != null || cameraStarting) return
            cameraStarting = true
            try {
                try {
                    DeviceAccess.ensure(DeviceKind.Camera)
                    if (over) return
                    val input = call.publishVideo(VideoKind.Camera, camera.width, camera.height)
                    if (over) {
                        quietly { input.unpublish() }
                        return
                    }
                    try {
                        // Blurred from the first frame where blur is chosen.
                        camera.blurring = memory.blur
                        camera.start(into = input)
                    } catch (fault: Throwable) {
                        // A camera published and never fed is a black tile
                        // for everybody else.
                        quietly { input.unpublish() }
                        throw fault
                    }
                    cameraInput = input
                } catch (fault: CancellationException) {
                    throw fault
                } catch (fault: Throwable) {
                    deviceFailed(DeviceKind.Camera, fault)
                    throw fault
                }
                dismiss(DeviceKind.Camera.noticeKey)
            } finally {
                cameraStarting = false
            }
        } else {
            if (cameraMade) camera.stop()
            cameraInput?.let { input ->
                cameraInput = null
                quietly { input.unpublish() }
            }
        }
        cameraOn = on
    }

    /** A message is on its way (useChat.ts `sending`): the box sends nothing more until it has gone or failed. */
    var sending by mutableStateOf(false)
        private set

    /**
     * Sends a message, and adds it to the call's own once the media server has
     * it. Throws when it could not be sent, so that the box can put the words
     * back: a send that failed silently looks exactly like one that worked.
     */
    suspend fun say(text: String) {
        val trimmed = text.trim()
        if (trimmed.isEmpty()) return
        sending = true
        try {
            call.sendChat(trimmed)
            messages = (messages + ChatLine(UUID.randomUUID().toString(), localIdentity, localName, trimmed, Instant.now(), mine = true)).takeLast(MESSAGES_KEPT)
        } finally {
            sending = false
        }
    }

    private var chatDraftState by mutableStateOf("")
    /** What is in the messages box, kept with the call rather than the panel, so that closing the panel half way through a sentence does not lose it. */
    var chatDraft: String
        get() = chatDraftState
        set(value) {
            val changed = value != chatDraftState
            chatDraftState = value
            if (changed && value.isNotEmpty()) chatFailed = false
        }

    /** The last message did not leave. Its words are back in the box, and the box says so until the next is typed or sent. */
    var chatFailed by mutableStateOf(false)
        private set

    /**
     * Sends what is in the box (ChatPanel.tsx `send`): cleared at once and put
     * back if it did not leave, because losing what somebody typed is worse
     * than showing it twice. Nothing goes while a message is already on its
     * way, or while the call is reconnecting.
     */
    suspend fun sendDraft() {
        val body = chatDraft.trim()
        if (body.isEmpty() || sending || state != CallState.CONNECTED) return
        chatDraft = ""
        chatFailed = false
        try {
            say(body)
        } catch (fault: CancellationException) {
            throw fault
        } catch (fault: Throwable) {
            // Whatever was typed since goes after it, rather than being
            // replaced by it.
            chatDraft = if (chatDraft.isEmpty()) body else body + "\n" + chatDraft
            chatFailed = true
        }
    }

    private var watchingWritten = ""

    /** The `watching` attribute, written only when it changes (watching.ts). */
    suspend fun setWatching(sharer: String) {
        if (sharer == watchingWritten) return
        call.setAttributes(mapOf("watching" to sharer))
        watchingWritten = sharer
    }

    /** The hand is an attribute, written only when it changes (hands.ts). */
    suspend fun setHand(up: Boolean) {
        if (up == handUp) return
        call.setAttributes(mapOf("hand" to if (up) "up" else ""))
        handUp = up
    }

    suspend fun send(topic: String, payload: ByteArray, to: List<String> = emptyList(), reliable: Boolean = true) {
        call.sendData(topic, payload, to, reliable)
    }

    /** Whether the app is in the background: readings taken there are not evidence of a freeze (the web skips a hidden tab). */
    fun setBackground(background: Boolean) {
        call.setBackground(background)
    }

    /**
     * What the sound said without being asked (CallAudio `News`). Given up,
     * it is said, and the microphone's button goes off with the microphone, so
     * that nothing on the screen says a voice is going out that is not; the
     * notice says how to try again, which is to turn the microphone on.
     */
    private fun heard(news: CallAudio.News) {
        when (news) {
            is CallAudio.News.GaveUp -> {
                Log.e(TAG, "the call's sound stopped: ${news.error.message}")
                if (microphoneOn) {
                    microphoneInput?.setMuted(true)
                    audio.stopMicrophone()
                    microphoneOn = false
                }
                announce(CallToast(
                    t("The call's sound stopped"),
                    detail = t("The microphone or speakers kept changing. Turn the microphone on to try again."),
                    kind = CallToast.Kind.Failure, lasts = NoticeTimes.AT_MOST, key = SOUND_STOPPED_KEY, closable = true,
                ))
            }
            CallAudio.News.Back -> dismiss(SOUND_STOPPED_KEY)
            is CallAudio.News.MicrophoneLost -> {
                microphoneInput?.setMuted(true)
                microphoneOn = false
                deviceFailed(DeviceKind.Microphone, news.error)
            }
        }
    }

    /**
     * The listener's setting for one of a person's sounds (hearing.ts): the
     * level on the Opus track's source and on any lossless stream standing in
     * for it, and a block that the core applies at the server to both.
     *
     * A blocked sound is also turned to nothing here, on both: the core can
     * only stop a track it is subscribed to, and the packets already on their
     * way would otherwise play at the old level until the block took.
     */
    suspend fun hear(identity: String, sound: MediaSource, level: Float, blocked: Boolean) {
        val audible = if (blocked) 0f else level
        person(identity)?.tracks?.values?.firstOrNull { track ->
            if (sound == MediaSource.UNKNOWN) track.source == MediaSource.UNKNOWN && track.name.startsWith("music") else track.source == sound
        }?.sid?.let { audio.setVolume(it, audible) }
        if (sound != MediaSource.SCREEN_SHARE_AUDIO && sound != MediaSource.UNKNOWN) return
        val music = sound == MediaSource.UNKNOWN
        lossless.setLevel(identity, music, audible)
        call.setHearing(identity, music, level, blocked)
    }

    /** What the parts layered over the call say before it goes, each awaited while the call can still carry it. */
    val beforeLeaving = HashMap<String, suspend () -> Unit>()

    /** Whether [leave] has run; a second is nothing, the first having let everything go. */
    private var left = false

    suspend fun leave() {
        if (left) return
        left = true
        over = true
        for (last in beforeLeaving.values.toList()) quietly { last() }
        beforeLeaving.clear()
        lossless.closeAll()
        quietly { sharing.stop() }
        audio.stop()
        if (cameraMade) camera.close()
        VideoHub.forget(call)
        call.leave()
        scope.cancel()
    }

    // MARK: Events

    internal fun apply(event: CallEvent) {
        when (event) {
            is CallEvent.StateChanged -> {
                state = event.state
                Log.i(TAG, "call ${event.state.name.lowercase()}")
            }
            is CallEvent.Ended -> {
                endedReason = event.reason
                over = true
                audio.stop()
                if (cameraMade) camera.stop()
                scope.launch { quietly { sharing.stop() } }
                Log.i(TAG, "call ended: ${event.reason}")
                // The server deleting the room is `RoomDeleted` in the Rust
                // SDK (the protocol's ROOM_DELETED, which the web's SDK reads
                // as its room-closed reason); waiting only for `RoomClosed`
                // left the app sitting in a call that had gone, never
                // rejoining.
                val moving = moving
                if (moving != null && System.nanoTime() - moving.atNanos < MOVE_WINDOW_NANOS && movedAway(event.reason)) {
                    onMoved?.invoke(moving.relay)
                } else {
                    onEnded?.invoke(event.reason)
                }
            }
            is CallEvent.ParticipantJoined -> {
                val arriving = person(event.participant.identity) == null
                upsert(event.participant)
                if (arriving) Log.i(TAG, "participant joined: ${event.participant.identity}")
                if (arriving && settled) {
                    announce(CallToast(t("{name} joined", mapOf("name" to named(event.participant.name, event.participant.identity)))))
                }
            }
            is CallEvent.ParticipantChanged -> upsert(event.participant)
            is CallEvent.ParticipantLeft -> {
                person(event.identity)?.let { person ->
                    for (sid in person.tracks.keys) audio.stopPlaying(sid)
                    announce(CallToast(t("{name} left", mapOf("name" to named(person.name, event.identity)))))
                }
                Log.i(TAG, "participant left: ${event.identity}")
                people = people.filter { it.id != event.identity }
            }
            is CallEvent.TrackPublished -> {
                val person = person(event.identity)
                if (settled && person != null && event.track.sid !in person.tracks) announceShare(event.track, person)
                update(event.identity) { it.copy(tracks = it.tracks + (event.track.sid to event.track)) }
            }
            is CallEvent.TrackSubscribed -> {
                update(event.identity) { it.copy(tracks = it.tracks + (event.track.sid to event.track)) }
                Log.i(TAG, "subscribed ${event.track.kind.name.lowercase()} ${event.track.source.name.lowercase()} ${event.track.sid} from ${event.identity}")
                if (event.track.kind == MediaKind.AUDIO) {
                    val stereo = event.track.source == MediaSource.SCREEN_SHARE_AUDIO || event.track.source == MediaSource.UNKNOWN
                    try {
                        audio.play(event.track.sid, if (stereo) 2 else 1, call)
                    } catch (fault: Exception) {
                        Log.w(TAG, "${event.track.sid} could not be played: ${fault.message}")
                    }
                }
            }
            is CallEvent.TrackUnpublished -> {
                audio.stopPlaying(event.sid)
                update(event.identity) { it.copy(tracks = it.tracks - event.sid) }
            }
            is CallEvent.TrackUnsubscribed -> {
                audio.stopPlaying(event.sid)
                update(event.identity) { person ->
                    val track = person.tracks[event.sid] ?: return@update person
                    person.copy(tracks = person.tracks + (event.sid to track.copy(subscribed = false)))
                }
            }
            is CallEvent.TrackMuted -> update(event.identity) { person ->
                val track = person.tracks[event.sid] ?: return@update person
                person.copy(tracks = person.tracks + (event.sid to track.copy(muted = event.muted)))
            }
            is CallEvent.LocalTrackPublished -> localTracks = localTracks + (event.track.sid to event.track)
            is CallEvent.LocalTrackUnpublished -> localTracks = localTracks - event.sid
            is CallEvent.Speaking -> {
                val now = event.identities.toSet()
                people = people.map { if (it.speaking == (it.id in now)) it else it.copy(speaking = it.id in now) }
            }
            is CallEvent.Quality -> {
                if (event.identity == localIdentity) localQuality = event.quality
                update(event.identity) { it.copy(quality = event.quality) }
            }
            is CallEvent.Chat -> {
                val line = ChatLine(event.id, event.identity, event.name, event.text, Instant.ofEpochMilli(event.timestamp), mine = false)
                // The web keeps the last 500 (chat.ts).
                messages = (messages + line).takeLast(MESSAGES_KEPT)
                if (!chatOpen) {
                    unread = true
                    arriving = arriving + line
                    scope.launch {
                        delay(SAID_FOR_MILLIS)
                        arriving = arriving.filter { it.id != line.id }
                    }
                }
            }
            is CallEvent.Data -> {
                // Server messages come with no sender, and only those are
                // believed (host.ts, App.tsx).
                if (event.identity == null && (event.topic == "moving" || event.topic == "placing")) {
                    moving = Moving(String(event.payload, Charsets.UTF_8), System.nanoTime())
                }
            }
            is CallEvent.AttributesChanged -> {
                if (event.identity == localIdentity) handUp = event.attributes["hand"] == "up"
                else update(event.identity) { it.copy(attributes = event.attributes) }
            }
            is CallEvent.Sealed -> update(event.identity) { it.copy(readable = event.readable) }
            is CallEvent.Strain -> strainHint = event.hint
            is CallEvent.Connection -> connection = event.reading
            CallEvent.LosslessGaveUp -> {
                // Said to the person sharing, because it is their upload that
                // could not keep up; it stays a while, with a close button,
                // and the share itself carries on (notices.ts `losslessGaveUp`).
                announce(CallToast(
                    t("Lossless sound stopped"),
                    detail = t("Your connection could not keep up. Everyone still hears the sound in real time."),
                    lasts = NoticeTimes.AT_MOST, closable = true,
                ))
            }
        }
        for (listener in listeners.toList()) listener(event)
    }

    /**
     * Somebody else taking the stage with a share, or starting sound on its
     * own, which has no picture to announce it (notices.ts `onPublished`).
     * Only a track that was not already there, so the shares running when
     * this participant arrived are not news.
     */
    private fun announceShare(track: TrackInfo, person: Person) {
        val name = named(person.name, person.id)
        // sound.ts `soundOnly`: the shared sound's channel, in either form, and not the song desk's music.
        val soundOnly = track.source == MediaSource.SCREEN_SHARE_AUDIO && (track.name == "listening" || track.name == "listening-lossless")
        if (track.source != MediaSource.SCREEN_SHARE && !soundOnly) return
        announce(CallToast(
            if (soundOnly) t("{name} is sharing sound", mapOf("name" to name)) else t("{name} started sharing", mapOf("name" to name)),
            kind = CallToast.Kind.Signal, lasts = NoticeTimes.A_MOMENT,
        ))
    }

    private fun upsert(participant: ParticipantInfo) {
        val tracks = participant.tracks.associateBy { it.sid }
        val index = people.indexOfFirst { it.id == participant.identity }
        people = if (index >= 0) {
            people.toMutableList().also {
                val was = it[index]
                it[index] = was.copy(name = participant.name, attributes = participant.attributes, tracks = was.tracks + tracks)
            }
        } else {
            people + Person(participant.identity, participant.name, participant.attributes, tracks)
        }
    }

    private fun update(identity: String, change: (Person) -> Person) {
        val index = people.indexOfFirst { it.id == identity }
        if (index < 0) return
        val changed = change(people[index])
        if (changed != people[index]) people = people.toMutableList().also { it[index] = changed }
    }

    /**
     * Carries the core's events to the main thread in order. Events that
     * arrive before the model exists (the participants already in the room
     * are announced during the join itself) wait for it.
     */
    internal class Bridge : CallObserver {
        @Volatile var model: CallModel? = null
        private val early = ArrayList<CallEvent>()
        private val lock = Any()
        private val main by lazy { Handler(Looper.getMainLooper()) }

        override fun onEvent(event: CallEvent) {
            synchronized(lock) {
                if (model == null) {
                    early.add(event)
                    return
                }
            }
            main.post { model?.apply(event) }
        }

        /** On the main thread, once the model is set. */
        fun flush() {
            val pending = synchronized(lock) {
                val out = ArrayList(early)
                early.clear()
                out
            }
            for (event in pending) model?.apply(event)
        }

        fun attach(model: CallModel) {
            synchronized(lock) { this.model = model }
        }
    }

    companion object {
        const val SIGNAL_DETAIL_KEY = "meet-live.signal-detail"
        const val SOUND_STOPPED_KEY = "device.sound"
        /** What others said stays in the corner this long (live/chat.ts SAID_FOR). */
        const val SAID_FOR_MILLIS = 6_000L
        /** The web keeps the last 500 messages (chat.ts). */
        const val MESSAGES_KEPT = 500
        /** A room that closes within this long of a `moving` message is the move, not the end (App.tsx ~560-627). */
        const val MOVE_WINDOW_NANOS = 15_000_000_000L

        /**
         * Joins the call that [options] describes. The people already here
         * when this participant arrives are told during the join itself, and
         * nobody is told they joined: the web listens only once its room is
         * connected (notices.ts `watch`).
         */
        suspend fun join(options: CallOptions, deployment: Deployment, region: String?, media: CallMedia): CallModel {
            val bridge = Bridge()
            val call = joinCall(options, bridge)
            val model = CallModel(
                call, deployment, region, sealed = options.sealingWord != null,
                audio = media.audio(), makeCamera = media::camera, sharing = media.sharing(), pictureInPicture = media.pictureInPicture(),
            )
            bridge.attach(model)
            bridge.flush()
            model.settled = true
            call.setLosslessSink(model.lossless)
            return model
        }

        /** What a person is called in a notice: their name, or their identity when they gave none (notices.ts `named`). */
        fun named(name: String, identity: String): String = name.ifEmpty { identity }

        /** Whether a call that ended for [reason] was taken away by the server (the room closed, or this participant removed). */
        fun movedAway(reason: String): Boolean = listOf("RoomDeleted", "RoomClosed", "ParticipantRemoved").any { reason.contains(it) }
    }
}

/** Runs [work], ignoring a failure but never a cancellation: Swift's `try?`. */
internal suspend fun quietly(work: suspend () -> Unit) {
    try {
        work()
    } catch (fault: CancellationException) {
        throw fault
    } catch (fault: Exception) {
        Log.d(TAG, "ignored: ${fault.message}")
    }
}
