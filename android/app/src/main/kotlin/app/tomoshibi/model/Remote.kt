package app.tomoshibi.model

import android.util.Log
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import app.tomoshibi.basics.Language
import app.tomoshibi.basics.Localization
import app.tomoshibi.media.ShareCeiling
import kotlin.math.roundToLong
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import org.json.JSONArray

/**
 * What the deployment says to the apps between their releases (the core's
 * api/remote.rs, `GET /api/apps/remote`): wording laid over the app's own
 * phrases, features switched off, ceilings over what the app would otherwise
 * use, and a notice for the lobby. The owner, 2026-10-22: a small change
 * should not each time need a new build. Remote.swift on Apple, by the same
 * rules.
 *
 * The last answer is kept, and read back before the first screen is drawn
 * ([restore]), so that a feature switched off stays off on a train and a
 * phrase laid over does not flicker back to the app's own at every start.
 * Then the deployment is asked: at start, every half hour after, and again
 * whenever the app comes to the front or back to the lobby ([refresh]). An
 * answer replaces what applied and is kept; a question that is not answered
 * changes nothing and says nothing, since what it would have said is whatever
 * was said last time. With nothing kept and nothing heard, the app is exactly
 * what it was built as.
 *
 * The asking, the reading and the per-language working out are the core's
 * (RemoteSource, CoreRemoteSource); here are when to ask, what to keep, and
 * what follows, apart from the core so that the rules are tested on the JVM.
 * Every method is for the main thread.
 */
class Remote(
    /** Null asks nothing and reads nothing: a debug build started by hand, which has no test deployment to ask and would ask the real one. */
    private val source: RemoteSource?,
    /** Where the answer and the notices closed are kept; null for nowhere (a rehearsal, whose switches and notices are no business of the owner's next start). */
    private val store: Defaults?,
    private val scope: CoroutineScope,
    /** Where the phrases laid over go: the app's localization. */
    private val localization: Localization = Localization.shared,
    private val every: Long = EVERY,
    private val soonest: Long = SOONEST,
    private val now: () -> Long = System::currentTimeMillis,
) {
    /** What applies now: the last answer, or what was kept from the one before, or nothing. */
    var answer by mutableStateOf(RemoteAnswer.NOTHING)
        private set

    /** On the lobby's screen, the one place the notice is shown. */
    private var inLobby by mutableStateOf(false)

    /** Notices somebody closed, by id: not shown again, until the deployment puts up one with another id. */
    private var closed by mutableStateOf(readClosed())

    /** Told whenever an answer is applied, kept or heard, for what follows it outside the screens: the app's updates. */
    var onApply: ((RemoteAnswer) -> Unit)? = null

    private var started = false
    private var asking = false
    private var lastAsked: Long? = null
    private var schedule: Job? = null
    private var expiry: Job? = null

    /** Whether a feature is offered (the answer's `off`). */
    fun allows(feature: RemoteFeature): Boolean = answer.allows(feature)

    /** The ceiling over a shared screen. */
    val ceiling: ShareCeiling get() = answer.ceiling

    /** The notice for the lobby, while the lobby is the screen: none in a call, none once it has run out, and none once closed here. */
    val notice: LobbyNotice?
        get() {
            if (!inLobby) return null
            val notice = answer.notice ?: return null
            if (notice.id in closed) return null
            val until = notice.until
            if (until != null && until <= now()) return null
            return notice
        }

    /** The notice's close: this id is not shown again on this device, even after the app is started again. */
    fun close(notice: LobbyNotice) {
        if (notice.id in closed) return
        closed = (closed + notice.id).takeLast(CLOSED_HELD)
        store?.set(CLOSED_KEY, JSONArray(closed).toString())
    }

    /** Applies the answer kept from before, for the first screen to be drawn with: called before it is, as the language is settled before it is. */
    fun restore() {
        val source = source ?: return
        val kept = store?.string(KEPT_KEY) ?: return
        val read = try {
            source.read(kept)
        } catch (fault: Exception) {
            Log.i(TAG, "the kept remote answer could not be read: ${fault.javaClass.simpleName}")
            null
        } ?: return
        apply(read)
    }

    /** Starts asking: now, and every half hour after. Once. */
    fun start() {
        if (started || source == null) return
        started = true
        schedule = scope.launch {
            while (true) {
                ask()
                delay(every)
            }
        }
    }

    /** Stops asking, for a test that is done with the model. */
    fun stop() {
        schedule?.cancel()
        schedule = null
        expiry?.cancel()
        expiry = null
    }

    /**
     * Asks again, on coming back to the front or to the lobby, unless a
     * question is out or one went a moment ago: the app comes to the front
     * and lands in the lobby in the same second it starts, which would be
     * three questions for one answer.
     */
    fun refresh() {
        if (!started || asking) return
        val last = lastAsked
        if (last != null && now() - last < soonest) return
        scope.launch { ask() }
    }

    /** Told whenever the screen changes. Coming back to the lobby asks again, since a call can be long and the lobby is where what was said shows. */
    fun lobby(here: Boolean) {
        if (here == inLobby) return
        inLobby = here
        if (here) refresh()
    }

    private suspend fun ask() {
        val source = source ?: return
        if (asking) return
        asking = true
        lastAsked = now()
        try {
            val heard = source.fetch()
            store?.set(KEPT_KEY, heard.kept)
            apply(heard)
        } catch (fault: CancellationException) {
            throw fault
        } catch (fault: Exception) {
            // Kept quiet: what applied goes on applying.
            Log.i(TAG, "the deployment's word to the apps was not heard: ${fault.javaClass.simpleName}")
        } finally {
            asking = false
        }
    }

    private fun apply(heard: RemoteAnswer) {
        if (heard != answer) answer = heard
        localization.laidOver = heard.phrases
        expire()
        onApply?.invoke(heard)
    }

    /**
     * Takes the notice down when it runs out, if nothing is heard before: a
     * notice that says the server is down until nine would still be saying so
     * at ten to somebody who had not left the lobby, since nothing else draws
     * it again then.
     */
    private fun expire() {
        expiry?.cancel()
        expiry = null
        val notice = answer.notice ?: return
        val until = notice.until ?: return
        expiry = scope.launch {
            val left = until - now()
            if (left > 0) delay(left)
            if (answer.notice?.id == notice.id) answer = answer.copy(notice = null)
        }
    }

    private fun readClosed(): List<String> {
        val raw = store?.string(CLOSED_KEY) ?: return emptyList()
        return try {
            val array = JSONArray(raw)
            (0 until array.length()).map { array.getString(it) }
        } catch (_: Exception) {
            emptyList()
        }
    }

    companion object {
        private const val TAG = "Tomoshibi"
        /** Every half hour (the owner's, in the spec of 2026-10-22). */
        const val EVERY = 30 * 60 * 1000L
        /** How soon a question asked for by coming back may follow the one before. */
        const val SOONEST = 60 * 1000L
        /** Where the last answer and the notices closed are kept, under the Apple apps' names. */
        const val KEPT_KEY = "tomoshibi.remote"
        const val CLOSED_KEY = "tomoshibi.remote.closed"
        /**
         * How many closed notices are remembered: a notice's id is the
         * deployment's and is never put up again once it is taken down, so
         * the oldest only take up room.
         */
        const val CLOSED_HELD = 20
    }
}

/** The features the deployment can switch off, by the names it uses. A name the app does not know is a feature of a later app, and nothing to it. */
enum class RemoteFeature(val key: String) {
    ScreenShare("screen-share"),
    BackgroundBlur("background-blur"),
    WatchTogether("watch-together"),
    Music("music"),
    AppUpdates("app-updates"),
}

/**
 * What the deployment said, as the app uses it: worked out for each of the
 * app's languages by the core when it was heard (CoreRemoteSource), so that
 * a change of language needs nothing asked or read again.
 */
data class RemoteAnswer(
    /** The deployment's name for this answer. */
    val revision: String = "",
    /** The phrases laid over the app's own, by language and then by English key (the core's `remote_phrases` for each language). */
    val phrases: Map<Language, Map<String, String>> = emptyMap(),
    /** The features switched off. */
    val off: Set<String> = emptySet(),
    /** The numbers, by the deployment's names. */
    val params: Map<String, Double> = emptyMap(),
    val notice: LobbyNotice? = null,
    /** The answer as it came, to keep and read back (the core's `kept`). */
    val kept: String = "",
) {
    fun allows(feature: RemoteFeature): Boolean = feature.key !in off

    val ceiling: ShareCeiling get() = ShareCeiling.of(params)

    /**
     * How long between looks for an update (update_check_hours), in
     * milliseconds, or null for the app's own six hours. Never under ten
     * minutes, the wait after a look that failed (Updates `RETRY`): a
     * fraction of an hour written by mistake would otherwise have every copy
     * of the app asking the deployment every few seconds.
     */
    val updateInterval: Long?
        get() {
            val hours = params["update_check_hours"] ?: return null
            if (!hours.isFinite() || hours <= 0) return null
            return maxOf(hours * 3_600_000, Updates.RETRY.toDouble()).roundToLong()
        }

    companion object {
        /** What applies when nothing has ever been heard: the app as it was built. */
        val NOTHING = RemoteAnswer()
    }
}

/** A notice the lobby shows above everything else in it. */
data class LobbyNotice(
    /** The same while the notice is the same one; a closed notice is remembered by it. */
    val id: String,
    /** Something to take care about (the deployment's "warning"), drawn as the app draws a problem, rather than only to know. */
    val warning: Boolean = false,
    /**
     * The words by language: that language, else it without its region or
     * script, else English (the core's `remote_notice_text` for each). A
     * language missing here has nothing it can read, and is shown nothing.
     */
    val text: Map<Language, String>,
    /** When it stops being shown, in milliseconds since 1970; null until it is taken down. */
    val until: Long? = null,
)

/** Where the answers come from: the core, or a test's stand-in. */
interface RemoteSource {
    /** What the deployment says now; throws where it could not be asked or did not answer with something that can be read. */
    suspend fun fetch(): RemoteAnswer

    /** An answer kept from before, read again; null where it is not one. A notice that has run out since is left out. */
    fun read(kept: String): RemoteAnswer?
}
