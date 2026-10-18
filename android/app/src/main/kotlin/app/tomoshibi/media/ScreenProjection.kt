package app.tomoshibi.media

import android.content.Context
import android.media.projection.MediaProjection
import android.media.projection.MediaProjectionManager
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.util.Log

private const val TAG = "Tomoshibi"

/**
 * The one screen projection this app holds, shared by the screen share and
 * the sound shared on its own (Sharing).
 *
 * Android has one projection at a time for the whole system: taking a second
 * stops the first, and with it whatever was captured through it. Sound on
 * Android is captured through a projection too (AudioPlaybackCapture), so
 * the screen and the sound, which the Apple apps run as two captures each
 * with its own picker, here share one, and the person is asked once for
 * both where that is allowed.
 *
 * Android 14 allows one virtual display per projection, and so one consent
 * per screen share: a projection already used for a picture is not used for
 * another, and a fresh consent is asked for; the sound still running on the
 * old one is moved to the new one ([Holder.moved]) before the old one is
 * stopped. Below 14 a projection is reused for as long as it lasts.
 *
 * When the system ends the projection (the person pressed Stop in the status
 * bar or the cast tile, another app took the screen, or the shared app went
 * away), everybody holding it is told ([Holder.revoked]) and stops: without
 * that the tracks stayed published with nothing feeding them, a frozen share
 * for everybody else, as on the Mac (ScreenCapture.swift `onStopped`).
 *
 * Main thread only.
 */
internal object ScreenProjection {
    /** Somebody capturing through the projection. */
    interface Holder {
        /** The projection was replaced by a new one: what was made on the old one is made again on this one. */
        fun moved(projection: MediaProjection) {}

        /** The system ended it: stop. */
        fun revoked()

        /** Android 14: what is shared changed size (turned, or a shared app's window resized). */
        fun resized(width: Int, height: Int) {}
    }

    /** One holder's hold on the projection, until [release]d. */
    class Lease internal constructor(val holder: Holder) {
        val projection: MediaProjection? get() = if (this in leases) current else null

        /** A virtual display is being made on it, which Android 14 allows once. */
        fun usedForDisplay() {
            displayUsed = true
        }

        fun release() {
            if (!leases.remove(this)) return
            if (leases.isEmpty()) close()
        }
    }

    private var current: MediaProjection? = null
    private var displayUsed = false
    private val leases = ArrayList<Lease>()
    private val main = Handler(Looper.getMainLooper())

    /**
     * A hold on a projection, for a picture ([display]) or for sound alone;
     * null where the person declined, and nothing is said about that, since
     * a declined dialog is an answer rather than a failure. Throws where the
     * projection could not be taken from a consent given.
     */
    suspend fun acquire(context: Context, display: Boolean, holder: Holder): Lease? {
        val reusable = current != null && (!display || !displayUsed || Build.VERSION.SDK_INT < Build.VERSION_CODES.UPSIDE_DOWN_CAKE)
        if (reusable) return Lease(holder).also { leases.add(it) }
        val consent = DeviceAccess.screen() ?: return null
        // The call's service has to hold `mediaProjection` before the
        // projection is taken (Android 14 throws a SecurityException), and
        // the consent before the service may hold it.
        if (!CallKeeper.project()) {
            if (current == null) CallKeeper.unproject()
            throw IllegalStateException("the call service could not hold the screen")
        }
        val manager = context.getSystemService(MediaProjectionManager::class.java)
            ?: throw IllegalStateException("no projection service")
        val made = try {
            manager.getMediaProjection(consent.resultCode, consent.data)
        } catch (fault: Exception) {
            if (current == null) CallKeeper.unproject()
            throw fault
        } ?: run {
            if (current == null) CallKeeper.unproject()
            throw IllegalStateException("the projection was not given")
        }
        // Registered before anything is made on it, which Android 14 requires.
        made.registerCallback(Watcher(made), main)
        val old = current
        current = made
        displayUsed = false
        if (old != null) {
            Log.i(TAG, "a new projection replaces the one in use")
            for (lease in leases.toList()) lease.holder.moved(made)
            old.stop()
        }
        Log.i(TAG, "screen projection open")
        return Lease(holder).also { leases.add(it) }
    }

    private fun close() {
        val projection = current ?: return
        current = null
        displayUsed = false
        projection.stop()
        Log.i(TAG, "screen projection closed")
        CallKeeper.unproject()
    }

    private class Watcher(private val projection: MediaProjection) : MediaProjection.Callback() {
        override fun onStop() {
            // Only the projection in use: one this app replaced or closed
            // itself has nothing left to tell.
            if (projection !== current) return
            Log.i(TAG, "the system ended the screen projection")
            current = null
            displayUsed = false
            val holders = leases.map { it.holder }
            leases.clear()
            for (holder in holders) holder.revoked()
            CallKeeper.unproject()
        }

        override fun onCapturedContentResize(width: Int, height: Int) {
            if (projection !== current) return
            Log.i(TAG, "what is shared is now ${width}x$height")
            for (lease in leases.toList()) lease.holder.resized(width, height)
        }

        override fun onCapturedContentVisibilityChanged(isVisible: Boolean) {
            if (projection !== current) return
            Log.i(TAG, "what is shared is ${if (isVisible) "visible" else "hidden"}")
        }
    }
}
