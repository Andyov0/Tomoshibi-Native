package app.tomoshibi.media

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import app.tomoshibi.model.Defaults
import app.tomoshibi.model.Turns
import uniffi.tomoshibi_core.Call
import uniffi.tomoshibi_protocol.ShareQuality

/** Sharing from this device is not built yet; what a press of the share button says until it is. */
class SharingUnavailable : Exception("screen sharing is not available on Android yet")

/**
 * Sharing a screen, and sound on its own, each with its own capture and its
 * own tracks (the Apple apps' Sharing): the settings are the web's share menu
 * (room.ts ~39-63, ShareButton.tsx), a size and a frame rate remembered
 * between calls under the web's own storage names, and the lossless switch
 * (sound.ts `rememberedLossless`).
 *
 * TODO(media): the capture itself. On Android a screen is shared through
 * MediaProjection (a prompt from an activity, then a foreground service of
 * type mediaProjection while it runs), its frames pushed with
 * `VideoInput.pushPlanes` after `Call.publishVideo(VideoKind.Screen(fps,
 * quality), width, height)`, and its sound through AudioPlaybackCapture into
 * `Call.publishAudio(SoundKind.ScreenSound)` (or `Listening(lossless)` for
 * sound alone, with `pushLossless` beside `pushPcm`). The order of the steps,
 * the epochs that make a stop win over a start still publishing, and the
 * screen giving its sound up while sound is shared on its own are all in
 * apple/.../Call/Sharing.swift and should be kept. Until then every start
 * throws [SharingUnavailable], and every stop is a no-op that succeeds, so
 * that leaving a call never waits on this.
 */
class Sharing {
    /** Whether the screen is being shared. */
    var active by mutableStateOf(false)
        private set
    /** Whether sound is being shared on its own (the web's "listening"). */
    var listening by mutableStateOf(false)
        private set
    /** Whether the sound shared on its own went losslessly, which its control says while it runs. */
    var sendingLossless by mutableStateOf(false)
        private set

    private var screenWork by mutableIntStateOf(0)
    private var soundWork by mutableIntStateOf(0)
    /** A share being started, changed or stopped: its control shows it, and a second start is not taken. */
    val screenBusy: Boolean get() = screenWork > 0
    val soundBusy: Boolean get() = soundWork > 0

    /** Changes of a running share's size, one at a time. */
    private val changes = Turns()

    /** One setting of a screen share: a size and a rate. */
    data class Setting(val quality: ShareQuality, val fps: Int)

    /** What the running share is sending (room.ts `runningShare`), which the menu shows while it runs. */
    var running by mutableStateOf<Setting?>(null)
        private set

    private var store: Defaults? = null
    private var loading = false

    /** The size chosen in the menu, kept between calls. */
    var quality: ShareQuality = ShareQuality.AUTO
        set(value) {
            field = value
            if (!loading) store?.set(QUALITY_KEY, shorterSide(value).toString())
        }

    /** The rate chosen in the menu, kept between calls; [rate] is what is sent. */
    var fps: Int = 60
        set(value) {
            field = value
            if (!loading) store?.set(RATE_KEY, value.toString())
        }

    /** Whether the shared thing's sound goes too. On by default. */
    var withSound = true

    /** Whether sound shared alone goes losslessly as well: on unless turned off. */
    var lossless = true
        set(value) {
            field = value
            if (!loading) store?.set(LOSSLESS_KEY, if (value) null else "off")
        }

    /** Reads the menu's choices from where this call keeps them, and keeps them there from now on. */
    fun keep(store: Defaults?) {
        this.store = store
        loading = true
        try {
            quality = when (store?.string(QUALITY_KEY)?.toIntOrNull()) {
                1080 -> ShareQuality.P1080
                1440 -> ShareQuality.P1440
                2160 -> ShareQuality.P4K
                else -> ShareQuality.AUTO
            }
            fps = store?.string(RATE_KEY)?.toIntOrNull()?.takeIf { it > 0 } ?: 60
            lossless = store?.string(LOSSLESS_KEY) != "off"
        } finally {
            loading = false
        }
    }

    /** The rate that goes out for [quality] (ShareButton.tsx `clamp`). */
    fun rate(quality: ShareQuality, wanted: Int? = null): Int = ShareRates.clamp(wanted ?: fps, shorterSide(quality))

    /** What the menu shows: the running share's setting while there is one, otherwise the remembered choice. */
    val shown: Setting get() = running ?: Setting(quality, rate(quality))

    suspend fun start(call: Call) {
        throw SharingUnavailable()
    }

    /** A change of size or rate from the menu. */
    suspend fun choose(quality: ShareQuality, fps: Int? = null) {
        this.quality = quality
        if (fps != null) this.fps = fps
    }

    /** Restarts the running share at another setting (the uplink card's Switch). */
    suspend fun restart(quality: ShareQuality, fps: Int, call: Call) {
        changes.take { throw SharingUnavailable() }
    }

    suspend fun stopScreen() {}

    suspend fun startSoundOnly(call: Call, sealed: Boolean) {
        throw SharingUnavailable()
    }

    suspend fun stopSound() {}

    /** Stops everything this call shares; never waits on a change under way. */
    suspend fun stop() {
        stopScreen()
        stopSound()
    }

    companion object {
        const val QUALITY_KEY = "meet.share.quality"
        const val RATE_KEY = "meet-live.share-frame-rate"
        const val LOSSLESS_KEY = "meet-live.lossless"

        /** The shorter side a size names, as the web's menu keys it; 0 is automatic. */
        fun shorterSide(quality: ShareQuality): Int = when (quality) {
            ShareQuality.AUTO -> 0
            ShareQuality.P1080 -> 1080
            ShareQuality.P1440 -> 1440
            ShareQuality.P4K -> 2160
        }
    }
}

/**
 * The frame rates each size of screen share may be asked for (room.ts
 * `RATES_FOR`), by the shorter side the size names, 0 being automatic.
 * Ceilings rather than preferences: 4K at 120 frames is a billion pixels a
 * second, which the encoder does not refuse but falls behind on.
 */
object ShareRates {
    val all = listOf(15, 30, 60, 120, 240)

    /** What automatic sends: a ceiling of 1440p at sixty (room.ts `AUTOMATIC`). */
    const val AUTOMATIC = 60

    fun allowed(shorterSide: Int): List<Int> = when (shorterSide) {
        1080 -> listOf(15, 30, 60, 120, 240)
        1440 -> listOf(15, 30, 60, 120)
        2160 -> listOf(15, 30, 60)
        else -> listOf(AUTOMATIC)
    }

    /** The rate a size can carry: the one wanted where it is allowed, and otherwise the size's highest. */
    fun clamp(wanted: Int, shorterSide: Int): Int {
        val rates = allowed(shorterSide)
        return if (wanted in rates) wanted else rates.last()
    }
}
