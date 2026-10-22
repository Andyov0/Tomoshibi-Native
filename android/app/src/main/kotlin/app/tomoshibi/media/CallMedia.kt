package app.tomoshibi.media

import app.tomoshibi.feature.watch.ExoBackend
import app.tomoshibi.feature.watch.WatchEngine
import android.content.Context
import android.content.Intent
import android.util.Log
import androidx.compose.runtime.snapshotFlow
import app.tomoshibi.BuildConfig
import app.tomoshibi.MainActivity
import app.tomoshibi.basics.Localization
import app.tomoshibi.basics.t
import app.tomoshibi.model.AppModel
import app.tomoshibi.model.CallModel
import app.tomoshibi.model.CallToast
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.launch
import uniffi.tomoshibi_core.MediaKind

private const val TAG = "Tomoshibi"

/**
 * What a call needs from the device: its sound, its camera, its sharing and
 * the small window, made fresh for every call so that nothing one call opened
 * is still open in the next.
 *
 * [muted] is a test's `mute`: nothing is played, every sound is measured.
 */
class CallMedia(context: Context, val muted: Boolean = false) {
    private val context = context.applicationContext
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
    private var app: AppModel? = null

    /** Where a call's sound is routed; nowhere at all for a call that plays nothing. */
    fun routing(): AudioRouting = if (muted) NoAudioRouting else SystemAudioRouting(context)

    fun audio(): CallAudio = CallAudio(routing(), muted)

    fun camera(): CameraCapture = CameraCapture(context).also { camera ->
        // Blur that cannot be done here is turned off and said, rather than
        // the whole picture staying blurred with nobody in it.
        camera.onBlurFailed = {
            app?.call?.let { call ->
                call.setBlur(false)
                call.actionFailed(t("That could not be done. Try again."))
            }
        }
    }

    fun sharing(): Sharing = Sharing(context)
    fun pictureInPicture(): PictureInPicture = PictureInPicture()

    /**
     * Keeps every call of [app] alive and visible outside the app: the
     * service and its notification (CallKeeper), the small window
     * (CallWindow), and the pictures nobody can see let go. Once, as the
     * app's model is made.
     */
    fun follow(app: AppModel) {
        if (this.app != null) return
        this.app = app
        CallKeeper.install(context)
        Thermal.install(context)
        CallKeeper.openApp = { control -> openApp(control) }
        CallKeeper.onControl = { control -> scope.launch { perform(app, control) } }
        if (BuildConfig.DEBUG) MediaDebug.install(context, app)
        scope.launch {
            snapshotFlow { app.call to (app.movingTo != null) }.collectLatest { (call, moving) ->
                if (call == null) {
                    // Between the two rooms of a move the service is kept, so
                    // that the call coming back is not a service started from
                    // the background, which Android refuses.
                    if (!moving) CallKeeper.hold(null)
                    CallWindow.wanted = CallWindow.Wanted()
                    return@collectLatest
                }
                coroutineScope {
                    launch { DeviceAccess.notifications() }
                    launch { hold(call) }
                    launch { window(app, call) }
                    launch { bubble(app, call) }
                    launch { offerBubble(app, call) }
                    launch { pictures(call) }
                }
            }
        }
    }

    private suspend fun perform(app: AppModel, control: CallKeeper.Control) {
        val call = app.call ?: return
        when (control) {
            CallKeeper.Control.Mute -> if (call.microphoneOn) app.toggleMicrophone()
            CallKeeper.Control.Unmute -> if (!call.microphoneOn) app.toggleMicrophone()
            CallKeeper.Control.Leave -> app.leave()
            CallKeeper.Control.StopSharing -> call.sharing.stop()
        }
    }

    /** The service holds what the call uses, and the notification says it, in the reader's language. */
    private suspend fun hold(call: CallModel) {
        snapshotFlow {
            CallKeeper.Held(
                room = call.room,
                microphone = call.microphoneOn,
                camera = call.cameraOn,
                screen = call.sharing.active,
                sound = call.sharing.listening,
                language = Localization.shared.language,
            )
        }.collect { CallKeeper.hold(it) }
    }

    /**
     * The small window: open on the way out of a call, in the shape of what
     * it shows, with the microphone's button as it is.
     *
     * What it shows decides how large it is, since Android sizes the window
     * from its shape and nothing else an app can say. Something to watch --
     * the video everybody is watching, or a shared screen -- keeps its own
     * wide shape and so the large window. A camera keeps the camera's. With
     * no picture at all, a call of voices or a song playing, the window is
     * square, the least long Android makes it (CallWindow `windowShape` says
     * how it is sized): it was drawn sixteen by nine around a single initial,
     * the owner's "the picture in picture is too big" (2026-10-19).
     */
    private suspend fun window(app: AppModel, call: CallModel) {
        snapshotFlow {
            val watch = WatchEngine.of(call)
            val sid = call.pictureInPictureSid
            val shape = if (watch.onStage) {
                // The video's own shape, or the shape videos nearly always are
                // until it has said.
                val aspect = (watch.player?.backend as? ExoBackend)?.aspect ?: (16f / 9f)
                Math.round(aspect * 1000) to 1000
            } else {
                // The shape it is drawn in, or before it has been drawn the one
                // it was published at.
                sid?.let { VideoHub.existing(call.call)?.shapes?.get(it)?.shown }
                    ?: sid?.let { s -> call.people.firstNotNullOfOrNull { it.tracks[s] }?.let { it.width.toInt() to it.height.toInt() } }
            }
            val inCall = app.screen == AppModel.Screen.Call
            sid to CallWindow.Wanted(
                inCall = inCall, shape = shape, microphone = call.microphoneOn,
                bubble = inCall && voiceOnly(call) && CallBubble.allowed,
            )
        }.collect { (sid, wanted) ->
            call.pictureInPicture.follow(sid)
            CallWindow.wanted = wanted
        }
    }

    /**
     * The bubble (CallBubble): up while the app is out of sight in a call it
     * was left with nothing to see in, down the moment the app is back or the
     * call is over. What it was put up for is decided as the app is left and
     * kept: a camera turned on or a screen shared behind it cannot open the
     * system's window from there, and a bubble that went away for it would
     * leave the call with nothing on screen at all; the bubble says what has
     * started instead, and a tap on it brings the app back to see it.
     *
     * The system's window, open on a share and left with nothing once the
     * share ends, gives way to the bubble after a moment's grace, so that a
     * share stopped and started again does not make the window come and go.
     */
    private suspend fun bubble(app: AppModel, call: CallModel) {
        try {
            coroutineScope {
                launch {
                    snapshotFlow { CallWindow.inPictureInPicture && voiceOnly(call) && CallBubble.allowed }.collectLatest { empty ->
                        if (!empty) return@collectLatest
                        delay(GIVE_WAY_MILLIS)
                        CallWindow.giveWayToBubble()
                    }
                }
                snapshotFlow { Triple(app.screen == AppModel.Screen.Call, CallWindow.visible, voiceOnly(call) && CallBubble.allowed) }
                    .collect { (inCall, visible, wanted) ->
                        when {
                            !inCall || visible -> CallBubble.hide()
                            // Not for a screen gone dark with the call in
                            // front: the activity stops for the lock screen,
                            // where no bubble is drawn, and is back in front
                            // the moment it is unlocked.
                            wanted && !CallBubble.showing && interactive() ->
                                CallBubble.show(context, call) { openApp(null)?.let(::startFromBubble) }
                        }
                    }
            }
        } finally {
            CallBubble.hide()
        }
    }

    private fun interactive(): Boolean = context.getSystemService(android.os.PowerManager::class.java)?.isInteractive != false

    /** The app's activity, started from a press on the bubble: an app drawing over others may start one from behind. */
    private fun startFromBubble(intent: Intent) {
        try {
            context.startActivity(intent)
        } catch (fault: RuntimeException) {
            Log.w(TAG, "the call could not be brought back from the bubble: ${fault.message}")
        }
    }

    /**
     * The first call with nothing to see, a few seconds in and in front:
     * says once what the bubble is and asks for the leave it needs, in the
     * call's own corner, with the system's page behind the button. Closing
     * it is the answer no; either way it is not said again, and without the
     * leave the system's window opens as it always has (CallBubble).
     */
    private suspend fun offerBubble(app: AppModel, call: CallModel) {
        if (!CallBubble.mayOffer(context)) return
        snapshotFlow { app.screen == AppModel.Screen.Call && CallWindow.visible && !CallWindow.inPictureInPicture && voiceOnly(call) }
            .collectLatest { ready ->
                if (!ready) return@collectLatest
                delay(OFFER_AFTER_MILLIS)
                if (!CallBubble.mayOffer(context)) return@collectLatest
                CallBubble.offered(context)
                call.announce(
                    CallToast(
                        t("Voice calls in a small bubble"),
                        t("When there is nothing to see, leaving Tomoshibi can keep the call in a small bubble at the edge of the screen instead of the large picture-in-picture window. Android asks you to allow this once: on the next screen, choose Tomoshibi and allow it. Without it, the large window stays."),
                        lasts = null,
                        actionTitle = t("Allow"),
                        key = "call.bubble",
                        closable = true,
                    ),
                ) { CallBubble.askToBeAllowed(context) }
            }
    }

    /** Nothing to see: no video watched together, and no picture the small window would show. */
    private fun voiceOnly(call: CallModel): Boolean = !WatchEngine.of(call).onStage && call.pictureInPictureSid == null

    /**
     * Remote pictures nobody can see are let go at the server, as a browser
     * lets them go (livekit-client RemoteVideoTrack `updateVisibility`, with
     * `pauseVideoInBackground` on by default): all of them while the app is
     * out of sight, all but the small window's while it is in one. A phone
     * behind its lock screen otherwise went on receiving and decoding every
     * camera in the room, and a nine-person call in a small window decoded
     * nine pictures to show one. Each comes back the moment it can be seen,
     * after the key frame the server sends for it.
     *
     * Hidden after a second rather than at once (livekit's REACTION_DELAY,
     * longer here for the moment the window takes to move between full size
     * and small), so a quick look at another app does not cost every picture
     * a key frame; shown again at once.
     */
    private suspend fun pictures(call: CallModel) {
        var paused = emptySet<String>()
        snapshotFlow {
            val sids = call.people.flatMap { person -> person.tracks.values.filter { it.kind == MediaKind.VIDEO }.map { it.sid } }.toSet()
            val shown: Set<String>? = when {
                !CallWindow.visible -> emptySet()
                CallWindow.inPictureInPicture -> setOfNotNull(call.pictureInPicture.sid)
                else -> null
            }
            if (shown == null) emptySet() else sids - shown
        }.collectLatest { hidden ->
            if (!paused.containsAll(hidden)) delay(HIDE_AFTER_MILLIS)
            for (sid in paused - hidden) enable(call, sid, true)
            for (sid in hidden - paused) enable(call, sid, false)
            if (hidden != paused) Log.i(TAG, "pictures let go: ${hidden.size}")
            paused = hidden
        }
    }

    private fun enable(call: CallModel, sid: String, on: Boolean) {
        try {
            call.call.setRemoteEnabled(sid, on)
        } catch (fault: Exception) {
            // A track gone between the look and this: nothing to do.
            Log.d(TAG, "no picture to ${if (on) "take back" else "let go"} for $sid: ${fault.message}")
        }
    }

    /** The app's own activity, brought to the front, carrying [control] if there is one (the notification's Unmute). */
    private fun openApp(control: CallKeeper.Control?): Intent? {
        // Named rather than asked of the package manager: a debug build has a
        // second launcher activity, the kit's gallery, and the package's
        // launch intent opened that instead of the call.
        val intent = Intent(context, MainActivity::class.java)
            .setAction(Intent.ACTION_MAIN)
            .addCategory(Intent.CATEGORY_LAUNCHER)
        intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_SINGLE_TOP)
        if (control != null) intent.putExtra(CallWindow.CONTROL_EXTRA, control.name)
        return intent
    }

    companion object {
        const val HIDE_AFTER_MILLIS = 1_000L
        const val GIVE_WAY_MILLIS = 2_000L
        const val OFFER_AFTER_MILLIS = 4_000L
    }
}
