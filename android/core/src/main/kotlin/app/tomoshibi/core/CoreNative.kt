package app.tomoshibi.core

import android.content.Context
import android.view.Surface
import com.sun.jna.Callback
import com.sun.jna.CallbackThreadInitializer
import com.sun.jna.Native
import livekit.org.webrtc.VideoFrame

/**
 * The few calls into the core that carry Java objects, which JNA cannot, and
 * so go through JNI instead (core/src/android.rs).
 *
 * The library is loaded here with System.loadLibrary before the bindings'
 * JNA opens it: JNA uses dlopen, which runs no JNI_OnLoad and gives WebRTC no
 * class loader, and [initialize] has to run on a thread that has the app's,
 * or WebRTC's Java codec factories are not found and no picture is encoded
 * or decoded.
 */
object CoreNative {
    @Volatile private var ready = false

    /** Once, on the main thread, before anything else touches the core. */
    @Synchronized
    fun load(context: Context) {
        if (ready) return
        System.loadLibrary("tomoshibi_core")
        check(initialize(context.applicationContext)) { "WebRTC refused the application context" }
        ready = true
    }

    private external fun initialize(context: Context): Boolean

    /**
     * The window behind [surface], acquired once for the core: handed to
     * `Call.attachVideoWindow`, which takes it over, or else given back with
     * [releaseWindow]. Zero when the surface has none.
     */
    external fun windowFromSurface(surface: Surface): Long

    external fun releaseWindow(window: Long)

    /**
     * One frame held by WebRTC itself, a texture rather than bytes, sent
     * through the input behind [input] (`VideoInput.frameHandle`): retained
     * until every encoder has drawn it, then released. [width] and [height]
     * are the buffer's own. False where it could not be sent that way, for
     * the caller to send it as planes instead.
     */
    external fun pushFrame(input: Long, buffer: VideoFrame.Buffer, width: Int, height: Int, timestampUs: Long): Boolean

    /** Gives back a handle from `VideoInput.frameHandle`. */
    external fun releaseFrameInput(input: Long)

    /** Attached for good, never detached on return; a daemon, as a thread the core made should not hold the VM. */
    private val attached = CallbackThreadInitializer(true, false, "tomoshibi-core")

    /**
     * Called by every callback object in the generated bindings as it is
     * made (android/keep-attached.py says why): a thread JNA attaches for it
     * stays attached until it ends, so that JNA never detaches a thread
     * WebRTC has since attached for itself.
     */
    fun keepAttached(callback: Callback) {
        Native.setCallbackThreadInitializer(callback, attached)
    }
}
