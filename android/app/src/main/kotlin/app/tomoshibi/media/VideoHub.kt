package app.tomoshibi.media

import android.util.Log
import java.util.IdentityHashMap
import uniffi.tomoshibi_core.Call
import uniffi.tomoshibi_core.PictureShape

private const val TAG = "Tomoshibi"

/**
 * Every place one remote picture is drawn at once, and the size it is asked
 * for (the Apple apps' VideoHub).
 *
 * A picture is often wanted in two places, its tile and the picture-in-picture
 * window. The core draws one track into any number of windows itself, so what
 * is kept here is who wants which, and at what size: the server is asked for
 * the largest any of them wants, because the smaller can always scale down.
 * Asked for less, the tile on the stage was drawn from the layer meant for a
 * thumbnail.
 *
 * Main thread only.
 */
class VideoHub private constructor(private val call: Call) {
    private class Fan {
        val windows = IdentityHashMap<Any, Long>()
        val wants = IdentityHashMap<Any, Pair<Int, Int>>()
    }

    private val fans = HashMap<String, Fan>()

    /**
     * Draws [sid] into [window] (from CoreNative.windowFromSurface) for
     * [owner], replacing what that owner drew before. The core takes the
     * window over and releases it on detach or when the call ends.
     */
    fun attach(owner: Any, sid: String, window: Long, shape: PictureShape) {
        detach(owner)
        val fan = fans.getOrPut(sid) { Fan() }
        fan.windows[owner] = window
        call.attachVideoWindow(sid, window.toULong(), shape)
        ask(sid)
    }

    /** Stops drawing whatever [owner] drew. */
    fun detach(owner: Any) {
        val iterator = fans.entries.iterator()
        while (iterator.hasNext()) {
            val (sid, fan) = iterator.next()
            val window = fan.windows.remove(owner) ?: continue
            fan.wants.remove(owner)
            call.detachVideoWindow(sid, window.toULong())
            if (fan.windows.isEmpty()) iterator.remove() else ask(sid)
        }
        // The last picture of the call gone, the hub goes too: a hub holds
        // its call, and the table of hubs kept every call that had shown a
        // remote camera alive after it was left (2026-10-17).
        if (fans.isEmpty()) forget(call)
    }

    /** [owner]'s wish for [sid], in device pixels: the size it is drawn at. */
    fun want(owner: Any, sid: String, width: Int, height: Int) {
        val fan = fans[sid] ?: return
        if (owner !in fan.windows) return
        fan.wants[owner] = width to height
        ask(sid)
    }

    private fun ask(sid: String) {
        val largest = fans[sid]?.wants?.values?.maxByOrNull { it.first.toLong() * it.second } ?: return
        if (largest.first <= 0 || largest.second <= 0) return
        try {
            call.requestVideoSize(sid, largest.first.toUInt(), largest.second.toUInt())
        } catch (fault: Exception) {
            // A track gone between the tile's last layout and this: nothing
            // to ask for any more.
            Log.d(TAG, "no size asked for $sid: ${fault.message}")
        }
    }

    companion object {
        private val hubs = IdentityHashMap<Call, VideoHub>()

        /** The hub for a call, made on first use. */
        fun of(call: Call): VideoHub = hubs.getOrPut(call) { VideoHub(call) }

        /** The hub for a call if there is one, without making one for a call already left. */
        fun existing(call: Call): VideoHub? = hubs[call]

        fun forget(call: Call) {
            hubs.remove(call)
        }
    }
}
