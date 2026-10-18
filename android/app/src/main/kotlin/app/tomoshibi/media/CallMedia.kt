package app.tomoshibi.media

import android.content.Context

/**
 * What a call needs from the device: its sound, its camera, its sharing and
 * the small window, made fresh for every call so that nothing one call opened
 * is still open in the next.
 *
 * [muted] is a test's `mute`: nothing is played, every sound is measured.
 */
class CallMedia(context: Context, val muted: Boolean = false) {
    private val context = context.applicationContext

    /** Where a call's sound is routed; nowhere at all for a call that plays nothing. */
    fun routing(): AudioRouting = if (muted) NoAudioRouting else SystemAudioRouting(context)

    fun audio(): CallAudio = CallAudio(routing(), muted)
    fun camera(): CameraCapture = CameraCapture(context)
    fun sharing(): Sharing = Sharing()
    fun pictureInPicture(): PictureInPicture = PictureInPicture()
}
