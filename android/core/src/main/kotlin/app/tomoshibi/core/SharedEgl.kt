package app.tomoshibi.core

import android.util.Log
import livekit.org.webrtc.EglBase

/**
 * The EGL context the hardware encoders and the screen's capture share
 * textures through: one for the process, made the first time either asks and
 * never released, as an app built on WebRTC's Android SDK keeps its root
 * EglBase.
 *
 * WebRTC's encoders draw a texture frame into their codec's input surface from
 * a context of their own made to share with this one
 * (livekit.org.webrtc.DefaultVideoEncoderFactory, ours); the screen's frames
 * are textures made in another context sharing with it (ScreenCapture). A
 * texture is visible across contexts only within one share group, so both
 * have to start from the same root.
 */
object SharedEgl {
    private const val TAG = "Tomoshibi"

    private val root: EglBase? by lazy {
        try {
            EglBase.create()
        } catch (fault: RuntimeException) {
            // With no context the encoders take every frame as bytes, as they
            // did before there was one; nothing stops working.
            Log.e(TAG, "no shared EGL context: ${fault.message}")
            null
        }
    }

    /** The root context, or null where EGL would not make one. */
    @JvmStatic
    fun context(): EglBase.Context? = root?.eglBaseContext
}
