package app.tomoshibi.media

import android.content.Context
import android.media.projection.MediaProjection
import android.os.Build
import android.util.Log
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import app.tomoshibi.model.Defaults
import app.tomoshibi.model.Turns
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.launch
import uniffi.tomoshibi_core.AudioInput
import uniffi.tomoshibi_core.Call
import uniffi.tomoshibi_core.VideoInput
import uniffi.tomoshibi_core.VideoKind
import uniffi.tomoshibi_protocol.ShareQuality
import uniffi.tomoshibi_protocol.SoundKind
import uniffi.tomoshibi_protocol.shareSoundAlongside
import uniffi.tomoshibi_protocol.soundLosslessOffered

private const val TAG = "Tomoshibi"

/** Sharing where nothing can be captured: a model made without a device (a test). */
class SharingUnavailable : Exception("screen sharing is not available here")

/**
 * Sharing a screen, and sound on its own, each with its own tracks (the
 * Apple apps' Sharing, whose order of steps this keeps).
 *
 * They are two controls and can run at once, as the web allows a screen
 * share on top of sound shared on its own (room.ts `share`). What the web
 * does about the screen's own sound then is kept: while sound is shared on
 * its own, the screen goes without its own, because the two would be one
 * song twice a beat apart; and the screen takes its sound back when the
 * sound share stops, as the Mac does (a browser cannot).
 *
 * On Android both are captured through the one screen projection the system
 * allows (ScreenProjection), the picture by a virtual display
 * (ScreenCapture) and the sound by AudioPlaybackCapture (PlaybackCapture),
 * so sound shared on its own also asks the person through the screen's
 * dialog: Android has no other consent for another app's sound. Android 14
 * asks once per screen share, and lets the person share one app rather than
 * the whole screen.
 *
 * The settings are the web's share menu (room.ts ~39-63, ShareButton.tsx): a
 * size and a frame rate, remembered between calls under the web's own
 * storage names, and the lossless switch (sound.ts `rememberedLossless`).
 *
 * Main thread only.
 */
class Sharing(context: Context? = null) {
    private val context = context?.applicationContext

    /** Whether the screen is being shared. */
    var active by mutableStateOf(false)
        private set
    /** Whether sound is being shared on its own (the web's "listening"). */
    var listening by mutableStateOf(false)
        private set
    /** Whether the sound shared on its own went losslessly, which its control says while it runs. */
    var sendingLossless by mutableStateOf(false)
        private set
    /**
     * The screen went without its sound because the microphone permission,
     * which Android asks for another app's sound as well, was refused: the
     * web's "No sound was shared" is what the control says then.
     */
    var soundless by mutableStateOf(false)
        private set

    private var screenWork by mutableIntStateOf(0)
    private var soundWork by mutableIntStateOf(0)
    /**
     * A share being started, changed or stopped: its control shows it, and a
     * second start is not taken. Never the system's dialog itself, which a
     * press while it is out answers with nothing (DeviceAccess.screen), as
     * on the Mac a second press releases a picker that never answered.
     */
    val screenBusy: Boolean get() = screenWork > 0
    val soundBusy: Boolean get() = soundWork > 0

    /**
     * Moved on by every stop of the screen (the person's, the system's,
     * leaving), and of the sound: whatever was under way before it is no
     * longer wanted. A start part way through its publishing, a change of
     * size waiting on the network or a consent answered late sees the move
     * when it next wakes and takes back what it made, rather than finishing
     * into a share that was stopped or a call that was left. A stop never
     * waits for any of them, so leaving never hangs behind the network.
     */
    private var screenEpoch = 0
    private var soundEpoch = 0

    /** Changes of a running share's size, one at a time. */
    private val changes = Turns()

    /** One setting of a screen share: a size and a rate. */
    data class Setting(val quality: ShareQuality, val fps: Int)

    /** What the running share is sending (room.ts `runningShare`), which the menu shows while it runs. */
    var running by mutableStateOf<Setting?>(null)
        private set

    private var store: Defaults? = null
    private var loading = false

    // The menu's choices are snapshot state, so the share menu redraws from
    // them as it does from everything else here, rather than keeping copies.
    private var qualityState by mutableStateOf(ShareQuality.AUTO)
    private var fpsState by mutableIntStateOf(60)
    private var withSoundState by mutableStateOf(true)
    private var losslessState by mutableStateOf(true)

    /** The size chosen in the menu, kept between calls. */
    var quality: ShareQuality
        get() = qualityState
        set(value) {
            qualityState = value
            if (!loading) store?.set(QUALITY_KEY, shorterSide(value).toString())
        }

    /** The rate chosen in the menu, kept between calls; [rate] is what is sent. */
    var fps: Int
        get() = fpsState
        set(value) {
            fpsState = value
            if (!loading) store?.set(RATE_KEY, value.toString())
        }

    /** Whether the shared thing's sound goes too. On by default: a share of a video with no sound is the complaint that started all of this. */
    var withSound: Boolean
        get() = withSoundState
        set(value) {
            withSoundState = value
        }

    /** Whether sound shared alone goes losslessly as well: on unless turned off. */
    var lossless: Boolean
        get() = losslessState
        set(value) {
            losslessState = value
            if (!loading) store?.set(LOSSLESS_KEY, if (value) null else "off")
        }

    private val screen = context?.let { ScreenCapture(it) }
    private val playback = PlaybackCapture()
    /** The call the screen was shared into, for giving it its sound back. */
    private var call: Call? = null
    private var video: VideoInput? = null
    /** The screen's own sound, while it has one. */
    private var screenSound: AudioInput? = null
    /** Sound shared on its own. */
    private var sound: AudioInput? = null
    /** Whether the screen hears its sound, even while it has nowhere to send it (sound shared on its own has it). */
    private var hearing = false
    private var screenLease: ScreenProjection.Lease? = null
    private var soundLease: ScreenProjection.Lease? = null

    /** The screen's hold on the projection. */
    private val screenHolder = object : ScreenProjection.Holder {
        override fun moved(projection: MediaProjection) {
            // Only the sound can be on a projection being replaced: the
            // screen asks for a new one only when it is not running.
            playback.moved(projection)
        }

        override fun revoked() {
            Log.i(TAG, "the system ended the screen share")
            screenLease = null
            later { stopScreen() }
        }

        override fun resized(width: Int, height: Int) {
            screen?.resize(width, height)
        }
    }

    private val soundHolder = object : ScreenProjection.Holder {
        override fun moved(projection: MediaProjection) {
            playback.moved(projection)
        }

        override fun revoked() {
            Log.i(TAG, "the system ended the sound share")
            soundLease = null
            later { stopSound() }
        }
    }

    init {
        screen?.onStopped = { later { if (active) stopScreen() } }
    }

    /** For the system's callbacks, which cannot wait: made on first use, so a test's model never touches a main dispatcher. */
    private val scope by lazy { kotlinx.coroutines.MainScope() }

    /** Starts [work] on the main thread, from a callback that cannot wait for it. */
    private fun later(work: suspend () -> Unit) {
        scope.launch { quietly(work) }
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

    /** The share is not there any more: stopped while a change to it was waiting its turn or on the network. */
    class Gone : Exception("the share stopped")

    // MARK: The screen

    /**
     * Asks the person (the system's dialog) and starts sending the screen.
     * Returns without anything shared where they declined, which says
     * nothing, being an answer rather than a failure; throws where the share
     * would not start, for the control to say so.
     */
    suspend fun start(call: Call) {
        if (active || screenBusy) return
        val context = context ?: throw SharingUnavailable()
        val screen = screen ?: throw SharingUnavailable()
        val epoch = screenEpoch
        // Another app's sound is a recording, which Android asks the
        // microphone permission for; asked before the screen's dialog, so the
        // two prompts come in the order they are read.
        val hears = withSound && heard()
        val lease = ScreenProjection.acquire(context, display = true, holder = screenHolder) ?: run {
            Log.i(TAG, "nothing shared: the screen was not given")
            return
        }
        if (epoch != screenEpoch || active || screenBusy) {
            Log.i(TAG, "a consent answered after the share was called off; nothing is shared")
            lease.release()
            return
        }
        val projection = lease.projection ?: run {
            lease.release()
            return
        }
        screenWork++
        try {
            val setting = Setting(quality, rate(quality))
            val rate = setting.fps
            val (width, height) = ScreenCapture.fit(ScreenCapture.screenSize(context), setting.quality, rate)
            val video = call.publishVideo(VideoKind.Screen(rate.toUInt(), setting.quality), width.toUInt(), height.toUInt())
            if (epoch != screenEpoch) {
                Log.i(TAG, "the share was called off while it was published; taking it back")
                quietly { video.unpublish() }
                lease.release()
                return
            }
            // Its own sound unless sound is already being shared on its own,
            // which carries on and is not doubled (room.ts `alongside`).
            val sound = try {
                if (hears && shareSoundAlongside(listening)) call.publishAudio(SoundKind.ScreenSound) else null
            } catch (fault: Exception) {
                quietly { video.unpublish() }
                lease.release()
                throw fault
            }
            if (epoch != screenEpoch) {
                Log.i(TAG, "the share was called off while its sound was published; taking it back")
                quietly { video.unpublish() }
                sound?.let { quietly { it.unpublish() } }
                lease.release()
                return
            }
            try {
                lease.usedForDisplay()
                screen.start(projection, setting.quality, rate, video)
                if (hears && !playback.started) playback.start(projection)
            } catch (fault: Exception) {
                Log.e(TAG, "the share would not start: ${fault.message}")
                screen.stop()
                if (!listening) playback.stop()
                quietly { video.unpublish() }
                sound?.let { quietly { it.unpublish() } }
                lease.release()
                throw fault
            }
            if (epoch != screenEpoch) {
                Log.i(TAG, "the share was called off while its capture started; stopping it")
                screen.stop()
                if (!listening) playback.stop()
                quietly { video.unpublish() }
                sound?.let { quietly { it.unpublish() } }
                lease.release()
                return
            }
            if (sound != null) {
                playback.sink = sound
                playback.lossless = false
            }
            this.call = call
            this.video = video
            screenSound = sound
            hearing = hears
            soundless = withSound && !hears
            screenLease = lease
            running = setting
            active = true
            Log.i(TAG, "sharing the screen at ${shorterSide(setting.quality)} and $rate fps, ${width}x$height, with its sound ${sound != null}")
            // Sound shared on its own may have started, or stopped, while
            // this was being published.
            soundStandsDown()
            soundBack()
        } finally {
            screenWork--
        }
    }

    /** Whether the share may hear other apps' sound, asking for the permission that needs. */
    private suspend fun heard(): Boolean = try {
        DeviceAccess.ensure(DeviceKind.Microphone)
        true
    } catch (fault: CancellationException) {
        throw fault
    } catch (_: DeviceRefused) {
        Log.i(TAG, "the share goes without its sound: the microphone permission was refused")
        false
    }

    /**
     * A change of size or rate from the menu: remembered, as a choice made
     * there is, and applied at once where a share is running (ShareButton.tsx:
     * a setting that takes effect on the next share is one nobody can judge).
     */
    suspend fun choose(quality: ShareQuality, fps: Int? = null) {
        this.quality = quality
        if (fps != null) this.fps = fps
        if (!active) return
        try {
            retune(Setting(quality, rate(quality, fps ?: shown.fps)))
        } catch (_: Gone) {
            // Stopped under the change: the choice is remembered, and there
            // is nothing left to apply it to.
        }
    }

    /**
     * The uplink card's switch (StrainCard): applied to the running share and
     * not remembered (strain.ts: the next share starts from what was chosen
     * in the menu). Throws [Gone] where the share stopped first, so the card
     * does not tell the core a switch took that never happened.
     */
    suspend fun restart(quality: ShareQuality, fps: Int, call: Call) {
        if (!active) throw Gone()
        retune(Setting(quality, rate(quality, fps)))
    }

    /**
     * The same capture at another size and rate, with a picture track of its
     * own (Sharing.swift `retune`, which says why): the picture is published
     * again at the new setting and the old one taken down once the capture
     * feeds the new one, so the share never leaves the stage for those who
     * watch it; the display is resized in place, nothing is asked again, and
     * the sound is untouched. One at a time, and given up where the share
     * stops meanwhile.
     */
    private suspend fun retune(setting: Setting) {
        changes.take {
            val epoch = screenEpoch
            val call = call
            val old = video
            val screen = screen
            if (!active || call == null || old == null || screen == null) throw Gone()
            if (setting == running) return@take
            screenWork++
            try {
                val rate = setting.fps
                val (width, height) = screen.fitted(setting.quality, rate)
                val fresh = call.publishVideo(VideoKind.Screen(rate.toUInt(), setting.quality), width.toUInt(), height.toUInt())
                if (epoch != screenEpoch) {
                    // The stop took the old track down; this one is nobody's.
                    quietly { fresh.unpublish() }
                    throw Gone()
                }
                try {
                    screen.retune(setting.quality, rate, fresh)
                } catch (fault: Exception) {
                    Log.e(TAG, "the share would not change: ${fault.message}")
                    quietly { fresh.unpublish() }
                    throw fault
                }
                if (epoch != screenEpoch) {
                    quietly { fresh.unpublish() }
                    throw Gone()
                }
                video = fresh
                running = setting
                quietly { old.unpublish() }
                Log.i(TAG, "share now at ${shorterSide(setting.quality)} and $rate fps, ${width}x$height")
            } finally {
                screenWork--
            }
        }
    }

    /**
     * Stops the screen at once, whatever is under way: what it stops is taken
     * off the share before anything is awaited, so a change or a start that
     * wakes after finds nothing of its own to carry on with.
     */
    suspend fun stopScreen() {
        screenEpoch++
        val video = video
        val screenSound = screenSound
        val lease = screenLease
        this.video = null
        this.screenSound = null
        screenLease = null
        running = null
        active = false
        soundless = false
        hearing = false
        screenWork++
        try {
            screen?.stop()
            if (listening) {
                playback.sink = sound
            } else {
                playback.sink = null
                playback.stop()
            }
            lease?.release()
            if (video != null) quietly { video.unpublish() }
            if (screenSound != null) quietly { screenSound.unpublish() }
        } finally {
            screenWork--
        }
    }

    // MARK: Sound on its own

    /**
     * A share of sound alone (the web's "Share only sound", sound.ts
     * `startListening`): 510 kb/s stereo on the share's sound channel, named
     * "listening", with no picture. Taken and called off as the screen's
     * start is. Throws [DeviceRefused] where the microphone permission, which
     * Android asks for another app's sound, was refused.
     */
    suspend fun startSoundOnly(call: Call, sealed: Boolean) {
        if (listening || soundBusy) return
        val context = context ?: throw SharingUnavailable()
        val epoch = soundEpoch
        DeviceAccess.ensure(DeviceKind.Microphone)
        val lease = ScreenProjection.acquire(context, display = false, holder = soundHolder) ?: run {
            Log.i(TAG, "nothing shared: the sound was not given")
            return
        }
        if (epoch != soundEpoch || listening || soundBusy) {
            Log.i(TAG, "a consent answered after the sound share was called off; nothing is shared")
            lease.release()
            return
        }
        val projection = lease.projection ?: run {
            lease.release()
            return
        }
        soundWork++
        try {
            // Never in a sealed call: the stream is data, which crosses the
            // relay in the clear, in a call whose promise is that the relay
            // hears nothing.
            val offered = soundLosslessOffered(lossless, sealed, true)
            val sound = call.publishAudio(SoundKind.Listening(offered))
            if (epoch != soundEpoch) {
                Log.i(TAG, "the sound share was called off while it was published; taking it back")
                quietly { sound.unpublish() }
                lease.release()
                return
            }
            try {
                if (!playback.started) playback.start(projection)
            } catch (fault: Exception) {
                Log.e(TAG, "the sound share would not start: ${fault.message}")
                quietly { sound.unpublish() }
                lease.release()
                throw fault
            }
            if (epoch != soundEpoch) {
                Log.i(TAG, "the sound share was called off while its capture started; stopping it")
                if (!hearing) playback.stop()
                quietly { sound.unpublish() }
                lease.release()
                return
            }
            playback.sink = sound
            playback.lossless = offered
            this.sound = sound
            soundLease = lease
            listening = true
            sendingLossless = offered
            Log.i(TAG, "sharing sound, lossless offered $offered")
            soundStandsDown()
        } finally {
            soundWork--
        }
    }

    suspend fun stopSound() {
        soundEpoch++
        val sound = sound
        val lease = soundLease
        this.sound = null
        soundLease = null
        listening = false
        sendingLossless = false
        soundWork++
        try {
            if (active && hearing) {
                playback.sink = null
                playback.lossless = false
            } else {
                playback.sink = null
                playback.stop()
            }
            lease?.release()
            if (sound != null) quietly { sound.unpublish() }
            soundBack()
        } finally {
            soundWork--
        }
    }

    /** The screen's own sound stands down while sound shared on its own carries the sound: the two would be one song twice a beat apart. */
    private suspend fun soundStandsDown() {
        if (!listening) return
        val screenSound = screenSound ?: return
        playback.sink = sound
        playback.lossless = sendingLossless
        this.screenSound = null
        quietly { screenSound.unpublish() }
    }

    /**
     * And the screen, still shared, has its sound back once sound is no
     * longer shared on its own. Whatever changed while it was published (the
     * screen stopped, sound shared again, another give-back first) takes it
     * straight back down.
     */
    private suspend fun soundBack() {
        val call = call
        if (!active || !hearing || listening || screenSound != null || call == null) return
        val epoch = screenEpoch
        val back = try {
            call.publishAudio(SoundKind.ScreenSound)
        } catch (fault: CancellationException) {
            throw fault
        } catch (_: Exception) {
            return
        }
        if (epoch != screenEpoch || !active || listening || screenSound != null) {
            quietly { back.unpublish() }
            return
        }
        screenSound = back
        playback.sink = back
        playback.lossless = false
    }

    /**
     * Everything stopped: leaving, or the call ending under it. A consent
     * dialog still out is answered with nothing, so nothing given in it
     * afterwards is shared into a call that has gone.
     */
    suspend fun stop() {
        DeviceAccess.cancelScreen()
        // The screen first, so that stopping the sound does not hand the
        // screen its sound back on the way out.
        stopScreen()
        stopSound()
        call = null
    }

    /** The last picture the screen sent, for a debug build to look at. */
    internal fun picture(): PlaneReader.Picture? = screen?.picture()

    /** What the share is doing, for a debug log. */
    val state: String
        get() = "screen ${if (active) "on" else "off"} busy $screenBusy running ${running?.let { "${shorterSide(it.quality)}@${it.fps}" } ?: "-"}" +
            " size ${screen?.size?.let { "${it.first}x${it.second}" } ?: "-"}, sound ${if (listening) "on" else "off"} busy $soundBusy lossless $sendingLossless" +
            " (Android ${Build.VERSION.SDK_INT})"

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

/** Runs [work], ignoring a failure but never a cancellation: Swift's `try?`. */
private suspend fun quietly(work: suspend () -> Unit) {
    try {
        work()
    } catch (fault: CancellationException) {
        throw fault
    } catch (fault: Exception) {
        Log.d(TAG, "ignored: ${fault.message}")
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
