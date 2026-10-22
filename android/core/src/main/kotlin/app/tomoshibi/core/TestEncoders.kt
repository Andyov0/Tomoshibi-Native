package app.tomoshibi.core

/**
 * Whether an encoder factory with no hardware H.264 may make one from a
 * software codec (livekit.org.webrtc.TestH264EncoderFactory says why): set by
 * a debug build running on an emulator before its first call, and false
 * everywhere else, a phone with hardware H.264 included.
 */
object TestEncoders {
    @Volatile
    @JvmField
    var softwareH264: Boolean = false
}
