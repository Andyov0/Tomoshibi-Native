package app.tomoshibi.media

import app.tomoshibi.feature.watch.ExoBackend
import app.tomoshibi.feature.watch.WatchEngine
import android.content.Context
import android.content.Intent
import android.util.Log
import androidx.compose.runtime.snapshotFlow
import app.tomoshibi.BuildConfig
import app.tomoshibi.basics.Localization
import app.tomoshibi.basics.t
import app.tomoshibi.model.AppModel
import app.tomoshibi.model.CallModel
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
            sid to CallWindow.Wanted(inCall = app.screen == AppModel.Screen.Call, shape = shape, microphone = call.microphoneOn)
        }.collect { (sid, wanted) ->
            call.pictureInPicture.follow(sid)
            CallWindow.wanted = wanted
        }
    }

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
        val intent = context.packageManager.getLaunchIntentForPackage(context.packageName) ?: return null
        intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_SINGLE_TOP)
        if (control != null) intent.putExtra(CallWindow.CONTROL_EXTRA, control.name)
        return intent
    }

    companion object {
        const val HIDE_AFTER_MILLIS = 1_000L
    }
}
