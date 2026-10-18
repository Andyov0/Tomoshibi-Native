package app.tomoshibi.core

import android.content.Context
import android.view.Surface

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
}
