package app.tomoshibi.feature.watch

import app.tomoshibi.basics.t
import java.net.URI
import uniffi.tomoshibi_protocol.ResolvedVideo
import uniffi.tomoshibi_protocol.UnresolvedReason

/**
 * What a ticket plays, as this app has learned it (watch-api.ts `infos`;
 * WatchWords.swift): what the server said, or one of the two ways it can say
 * nothing. Told apart because they are different things to say: "could not
 * be read, try again" sent somebody retrying a video that the switch on the
 * management pages had turned off, and that no retry would play.
 */
sealed class WatchInfo {
    data class Known(val resolved: ResolvedVideo) : WatchInfo()
    data object Unavailable : WatchInfo()
    data object Failed : WatchInfo()

    val video: ResolvedVideo? get() = (this as? Known)?.resolved
}

object WatchWords {
    /** The code a refusal crosses the call as (watch.ts `REFUSALS`), from the reason the deployment gave. */
    fun code(reason: UnresolvedReason): String = when (reason) {
        UnresolvedReason.NOT_A_VIDEO -> "not_a_video"
        UnresolvedReason.UNAVAILABLE -> "unavailable"
        UnresolvedReason.MEMBERS_ONLY -> "members_only"
        UnresolvedReason.COPY_PROTECTED -> "copy_protected"
        UnresolvedReason.REGION_LOCKED -> "region_locked"
        UnresolvedReason.NOT_FOUND -> "not_found"
        UnresolvedReason.NOT_ALLOWED -> "not_allowed"
        UnresolvedReason.EMBY_NOT_ALLOWED -> "emby_not_allowed"
        UnresolvedReason.FAILED -> "failed"
    }

    /**
     * What to say about a video that was not added (WatchTab.tsx `refusal`).
     *
     * The two refusals of a person are said as what the person seeing them was
     * not given, because by the time one reaches somebody it is about them: a
     * link is read by whoever adds it wherever they can, a link read on
     * somebody else's behalf is refused a media server's film whoever they
     * are, and the holder's own refusal is not passed on as one.
     */
    fun refusal(why: String): String = when (why) {
        "not_a_video" -> t("That link is not a video that can be played here.")
        "unavailable" -> t("That video cannot be played here.")
        // Bilibili's shows, today. "Its own site" because that is where it
        // does play; the deployment's own premium account is given only a
        // preview for a page elsewhere.
        "members_only" -> t("That video is for paying members only, and plays only on its own site.")
        "copy_protected" -> t("That video is copy-protected, and plays only on its own site.")
        "region_locked" -> t("That video is not available in this server's region.")
        // Both, because which it is cannot be told from here.
        "not_found" -> t("That episode could not be found: it may have been taken down, or not be offered in this server's region.")
        "not_allowed" -> t("Watching together has not been opened to you.")
        "emby_not_allowed" -> t("The media servers have not been opened to you.")
        else -> t("The video could not be read. Try again.")
    }

    /**
     * The site a link is on, as people say it: "youtube.com", not
     * "www.youtube.com" (WatchScreen.tsx `siteOf`). Nothing for anything that
     * is not a web address, which is never offered to be opened.
     */
    fun site(link: String): String {
        val host = try {
            val uri = URI(link)
            val scheme = uri.scheme?.lowercase()
            if (scheme != "http" && scheme != "https") return ""
            uri.host
        } catch (_: Exception) {
            // java.net.URI refuses what a browser's address bar takes, a path
            // in Chinese among it, which is most of what Bilibili links are;
            // the host is read by its shape then.
            HOST.find(link)?.groupValues?.get(1)
        }
        if (host.isNullOrEmpty()) return ""
        return host.lowercase().removePrefix("www.")
    }

    private val HOST = Regex("^(?i:https?)://(?:[^@/?#\\s]*@)?([^:/?#\\s\\[\\]]+)")

    /** Whether a link may be opened elsewhere: only a web address, never a scheme that would start some other app. */
    fun openable(link: String): Boolean = site(link).isNotEmpty()

    /**
     * At most [count] UTF-16 units, as a JavaScript `slice(0, count)` cuts:
     * the web's field takes 2000 and the protocol counts the same way. A
     * Kotlin string is UTF-16 already, so this is a slice, keeping half a pair
     * where JavaScript would.
     */
    fun cut(text: String, count: Int): String = if (text.length <= count) text else text.substring(0, count)
}

/**
 * The one clock every call into the theatre and the keeper is given: a
 * monotonic count of milliseconds. The protocol takes any such clock as long
 * as it is always the same one; the wall clock is not one, since it jumps
 * when the system sets it. System.nanoTime rather than SystemClock's, which
 * the unit tests' stub of the platform answers with nought.
 */
fun watchClock(): Long = System.nanoTime() / 1_000_000

/**
 * A time as the panels and the stage say it, "4:29" (the core's
 * `music_clock`, api/music.rs, which is `musicClock` in its bindings): whole
 * seconds rounded half up as JavaScript's `Math.round` does, minutes not
 * carried into hours, and anything that is not a time, or before nought,
 * nought. Restated here rather than asked of the core, because a scrubber
 * says it on every frame it is dragged, and each ask is a crossing into the
 * native library.
 */
fun clockOf(seconds: Double): String {
    val whole = if (seconds.isFinite()) {
        val floor = kotlin.math.floor(seconds)
        maxOf(0.0, if (seconds - floor >= 0.5) floor + 1 else floor).toLong()
    } else {
        0L
    }
    return "${whole / 60}:${(whole % 60).toString().padStart(2, '0')}"
}
