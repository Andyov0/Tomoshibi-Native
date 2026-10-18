package app.tomoshibi.media

import android.app.Activity
import android.app.PendingIntent
import android.app.PictureInPictureParams
import android.app.RemoteAction
import android.content.Intent
import android.graphics.Rect
import android.graphics.drawable.Icon
import android.os.Build
import android.util.Log
import android.util.Rational
import androidx.activity.ComponentActivity
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.core.util.Consumer
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleEventObserver
import app.tomoshibi.R
import app.tomoshibi.basics.t
import java.lang.ref.WeakReference

private const val TAG = "Tomoshibi"

/**
 * The call in a small window when the app goes to the background: whatever is
 * on the stage, or the person being talked to (the Apple apps'
 * PictureInPictureHost). CallModel.pictureInPictureSid says which picture,
 * and CallMedia follows it into [sid].
 *
 * On Android the whole activity goes into the small window, so the room view
 * reads [inSmallWindow] and, while it is true, draws only [sid], full size, through
 * a RemoteVideo like any tile's; the window's size is then the view's, which
 * is what the hub asks the server for. The window itself, its shape and its
 * buttons are [CallWindow]'s.
 */
class PictureInPicture {
    /** The picture the small window shows: snapshot state. */
    var sid by mutableStateOf<String?>(null)
        private set

    /** Follows the call's choice of picture. */
    fun follow(sid: String?) {
        this.sid = sid
    }

    /**
     * Whether the app is in the small window now, which the room view draws
     * as the stage alone: snapshot state, followed from the activity's own
     * picture-in-picture changes (CallWindow), so the room need not ask the
     * activity.
     */
    val inSmallWindow: Boolean get() = CallWindow.inPictureInPicture

    /** Whether the small window can be opened here. */
    val available: Boolean get() = CallWindow.supported

    /** Opens the small window now, rather than when the app is left; false where it could not. */
    fun enter(activity: Activity): Boolean = CallWindow.enter(activity)

    /**
     * Where the stage is on screen, in the window's pixels, for the small
     * window to grow out of and back into (`setSourceRectHint`); the room
     * view says it as its stage is laid out, and null where there is none.
     */
    fun stage(bounds: Rect?) {
        CallWindow.stage = bounds
    }
}

/**
 * The activity's side of the small window: its shape, its buttons, and
 * opening it as the app is left.
 *
 * The Apple apps learned that the small window has to open on the first
 * swipe home and close when the app comes back (PictureInPicture.swift: the
 * owner had to leave the app twice). On Android 12 and later the system
 * opens it on the way home by itself once it has been told, ahead of time,
 * that it may (`setAutoEnterEnabled`), so that is said whenever a call starts
 * or ends rather than at the moment of leaving, which is too late for the
 * gesture; before 12 it is asked for as the app is left (onUserLeaveHint).
 * Coming back, Android puts the window back to full size itself.
 *
 * Installed by the activity on screen (DeviceAccess.install); main thread
 * only.
 */
object CallWindow {
    /** Whether the activity is in the small window: snapshot state, for the room view and the pictures it keeps. */
    var inPictureInPicture by mutableStateOf(false)
        private set

    /** Whether the activity is visible at all, in the small window or full size: snapshot state. */
    var visible by mutableStateOf(false)
        private set

    /** What the window should be like, from the call (CallMedia.follow). */
    data class Wanted(
        /** A call is showing: leaving the app opens the small window. */
        val inCall: Boolean = false,
        /** The shown picture's width and height as it is shown, or null for none yet. */
        val shape: Pair<Int, Int>? = null,
        val microphone: Boolean = false,
    )

    var wanted = Wanted()
        set(value) {
            if (value == field) return
            val ended = field.inCall && !value.inCall
            field = value
            apply()
            if (ended) closeIfShowing()
        }

    /** Where the stage is, for the window to grow from. */
    var stage: Rect? = null
        set(value) {
            if (value == field) return
            field = value
            apply()
        }

    private var activity: WeakReference<ComponentActivity>? = null

    val supported: Boolean
        get() = activity?.get()?.packageManager?.hasSystemFeature(android.content.pm.PackageManager.FEATURE_PICTURE_IN_PICTURE) == true

    /** The extra on the activity's intent that carries a call control (the notification's Unmute). */
    const val CONTROL_EXTRA = "app.tomoshibi.call.control"

    fun install(activity: ComponentActivity) {
        this.activity = WeakReference(activity)
        activity.addOnPictureInPictureModeChangedListener { info ->
            inPictureInPicture = info.isInPictureInPictureMode
            Log.i(TAG, if (inPictureInPicture) "in the small window" else "out of the small window")
        }
        activity.addOnUserLeaveHintListener {
            // Before Android 12 nothing opens the window on the way home but
            // this; from 12 the system has already done it.
            if (Build.VERSION.SDK_INT < Build.VERSION_CODES.S && wanted.inCall) enter(activity)
        }
        activity.lifecycle.addObserver(LifecycleEventObserver { _, event ->
            when (event) {
                Lifecycle.Event.ON_START -> visible = true
                Lifecycle.Event.ON_STOP -> visible = false
                else -> {}
            }
        })
        val controls = Consumer<Intent> { intent -> controlIn(intent) }
        activity.addOnNewIntentListener(controls)
        controlIn(activity.intent)
        apply()
    }

    /** A control the activity was opened with, from the call's notification. */
    private fun controlIn(intent: Intent?) {
        val name = intent?.getStringExtra(CONTROL_EXTRA) ?: return
        intent.removeExtra(CONTROL_EXTRA)
        val control = CallKeeper.Control.entries.firstOrNull { it.name == name } ?: return
        CallKeeper.control(control)
    }

    /** A debug build's `pip`: the small window opened as if the app had been left. */
    internal fun enterFromDebug(): Boolean = activity?.get()?.let(::enter) ?: false

    fun enter(activity: Activity): Boolean {
        if (!supported) return false
        return try {
            activity.enterPictureInPictureMode(params(activity))
        } catch (fault: IllegalStateException) {
            Log.w(TAG, "the small window would not open: ${fault.message}")
            false
        }
    }

    /** Tells the system what the small window is now: whether it opens by itself, its shape, its buttons. */
    private fun apply() {
        val activity = activity?.get() ?: return
        if (!supported) return
        try {
            activity.setPictureInPictureParams(params(activity))
        } catch (fault: IllegalArgumentException) {
            Log.w(TAG, "the small window's settings were refused: ${fault.message}")
        }
    }

    /**
     * The call ended in the small window: it goes, rather than staying on
     * the screen with the end of a call in it, as a FaceTime window does
     * when its call is put down.
     */
    private fun closeIfShowing() {
        val activity = activity?.get() ?: return
        if (!inPictureInPicture) return
        Log.i(TAG, "the call ended in the small window; closing it")
        activity.moveTaskToBack(true)
    }

    private fun params(activity: Activity): PictureInPictureParams {
        val builder = PictureInPictureParams.Builder()
        val (w, h) = wanted.shape ?: (16 to 9)
        val (across, down) = aspect(w, h)
        builder.setAspectRatio(Rational(across, down))
        builder.setActions(actions(activity))
        stage?.let { builder.setSourceRectHint(it) }
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            builder.setAutoEnterEnabled(wanted.inCall)
            // A live picture: drawn again at the new size rather than
            // stretched while the window is resized (the platform's advice
            // for video).
            builder.setSeamlessResizeEnabled(false)
        }
        return builder.build()
    }

    /** The microphone's button and Leave, as the notification has them, as many as the system shows. */
    private fun actions(activity: Activity): List<RemoteAction> {
        val microphone = if (wanted.microphone) {
            action(activity, R.drawable.call_microphone_off, t("Mute microphone"), CallKeeper.Control.Mute)
        } else {
            // The window is on screen, so the app may take the microphone
            // from here, as it may not from the notification behind it.
            action(activity, R.drawable.call_microphone, t("Unmute microphone"), CallKeeper.Control.Unmute)
        }
        val leave = action(activity, R.drawable.call_leave, t("Leave the call"), CallKeeper.Control.Leave)
        return listOf(microphone, leave).take(activity.maxNumPictureInPictureActions.coerceAtLeast(0))
    }

    private fun action(activity: Activity, icon: Int, title: String, control: CallKeeper.Control): RemoteAction {
        val intent: PendingIntent = CallKeeper.broadcast(activity, control)
        return RemoteAction(Icon.createWithResource(activity, icon), title, title, intent)
    }

    /** The picture's shape within what Android allows a small window to be (between 1:2.39 and 2.39:1). */
    internal fun aspect(width: Int, height: Int): Pair<Int, Int> {
        if (width <= 0 || height <= 0) return 16 to 9
        val ratio = width.toDouble() / height
        return when {
            ratio > 2.38 -> 238 to 100
            ratio < 1 / 2.38 -> 100 to 238
            else -> width to height
        }
    }
}
