package app.tomoshibi.model

/**
 * Whether a test is driving the app, and so whether anything chosen during a
 * call may be written down: a launch that names its own deployment, honoured
 * only in a debug build, the same rule as AppModel's `rehearsal`.
 *
 * The tests drive the very app somebody uses, under the same application id,
 * so whatever a rehearsal writes to the app's preferences is the owner's from
 * then on: on the Mac one run's step wrote half volume for the music into the
 * owner's own defaults. So the choices a call makes as it goes go through
 * [choices], which in a rehearsal keeps nothing.
 *
 * Set once, by AppModel, before anything reads it.
 */
object Rehearsal {
    @Volatile
    var running: Boolean = false
        internal set

    @Volatile
    private var kept: Defaults? = null

    /** Where a call's own choices are read from and kept: the person's preferences, or nowhere in a rehearsal. */
    val choices: Defaults? get() = if (running) null else kept

    internal fun install(running: Boolean, defaults: Defaults) {
        this.running = running
        kept = defaults
    }
}

/**
 * What a test asked of this launch, from the intent's extras: the Android
 * form of the Apple apps' TOMOSHIBI_* switches. Read only in a debug build
 * (MainActivity); a release build is never given one, and none of it is ever
 * kept: a test's server was once saved as the last one used, and the app
 * opened by hand went to a loopback deployment where no account existed.
 */
data class DebugLaunch(
    /** The deployment to talk to instead of the home one (TOMOSHIBI_BASE). */
    val base: String,
    /** Signs in as this account first (TOMOSHIBI_SIGNIN), with [pass]. */
    val name: String? = null,
    val pass: String? = null,
    /** A passphrase to join with, so a test can join as the host of the room it opens (TOMOSHIBI_PASSPHRASE). */
    val passphrase: String? = null,
    /** Seals the call with this word (TOMOSHIBI_SEALING_WORD). */
    val sealingWord: String? = null,
    /** Straight into this room (TOMOSHIBI_AUTOJOIN). */
    val autojoin: String? = null,
    /** Stops at this room's join screen (TOMOSHIBI_PREPARE). */
    val prepare: String? = null,
    /** An invitation or meeting link, as the system would hand it over (TOMOSHIBI_OPEN). */
    val open: String? = null,
    /** Joins with the microphone on (TOMOSHIBI_MIC); off otherwise, so no permission is asked for. */
    val microphone: Boolean = false,
    /**
     * Plays nothing and only measures what arrives (TOMOSHIBI_MUTE): a test
     * browser's fake microphone beeps once a second, and an app in the same
     * room would play that through somebody's real speakers.
     */
    val mute: Boolean = false,
    /** The screen after a call, for each of the ways one ends (TOMOSHIBI_DEBUG_ENDED). */
    val ended: String? = null,
)
