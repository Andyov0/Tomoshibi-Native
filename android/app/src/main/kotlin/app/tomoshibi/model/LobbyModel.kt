package app.tomoshibi.model

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import app.tomoshibi.basics.readingLocale
import app.tomoshibi.basics.say
import app.tomoshibi.basics.t
import java.time.Instant
import java.time.ZoneId
import java.time.ZoneOffset
import java.time.format.DateTimeFormatter
import java.time.format.FormatStyle
import java.util.Locale
import java.util.TimeZone
import kotlinx.coroutines.CancellationException
import org.json.JSONArray
import org.json.JSONObject
import uniffi.tomoshibi_protocol.Arrangement
import uniffi.tomoshibi_protocol.ProtocolException
import uniffi.tomoshibi_protocol.Relay
import uniffi.tomoshibi_protocol.relayTimings

/** A refusal to arrange, as a sentence. */
class ArrangeError(val sentence: String) : Exception(sentence)

/**
 * What the screens before a call show beside their forms: the meetings this
 * account has arranged (Schedule.tsx), the rooms joined lately, and the
 * relays as measured from here (ServerPicker.tsx). Kept apart from the views
 * because a phone's home and a tablet's sidebar show the same things laid out
 * differently, and a list fetched twice is a list that disagrees with itself
 * for a moment.
 *
 * Every method is for the main thread.
 */
class LobbyModel internal constructor(private val app: AppModel, private val defaults: Defaults) {
    // MARK: Arranged meetings

    enum class Status { Loading, Ready, Failed, SignedOut }

    var arrangements by mutableStateOf<List<Arrangement>>(emptyList())
        private set
    var status by mutableStateOf(Status.Loading)
        private set
    private var reads = 0

    /** The ones still ahead or under way, soonest first. */
    val upcoming: List<Arrangement> get() = arrangements.filter { !it.ended }

    suspend fun refreshArrangements() {
        val deployment = app.deployment ?: return
        if (app.account == null) return
        reads++
        val mine = reads
        status = Status.Loading
        try {
            val found = deployment.arrangements()
            if (mine != reads) return
            arrangements = found
            status = Status.Ready
        } catch (fault: CancellationException) {
            throw fault
        } catch (fault: ProtocolException.Refused) {
            if (mine != reads) return
            status = if (fault.code == "not_signed_in") Status.SignedOut else Status.Failed
        } catch (fault: Exception) {
            if (mine != reads) return
            status = Status.Failed
        }
    }

    /** Arranges one and returns it; the failure is an [ArrangeError] with the sentence to say. */
    suspend fun arrange(room: String, at: Instant, relay: String?): Result<Arrangement> {
        val deployment = app.deployment ?: return Result.failure(ArrangeError(t("Something went wrong. Try again.")))
        return try {
            val made = deployment.arrangeMeeting(room, at.toEpochMilli(), relay ?: "")
            reads++
            arrangements = (arrangements + made).sortedBy { it.at }
            status = Status.Ready
            Result.success(made)
        } catch (fault: CancellationException) {
            throw fault
        } catch (fault: Exception) {
            Result.failure(ArrangeError(explain(fault)))
        }
    }

    /** Calls one off; the sentence to say when that did not take. */
    suspend fun cancel(one: Arrangement): String? {
        val deployment = app.deployment ?: return null
        return try {
            deployment.cancelMeeting(one.id)
            reads++
            arrangements = arrangements.filter { it.id != one.id }
            null
        } catch (fault: CancellationException) {
            throw fault
        } catch (fault: Exception) {
            explain(fault)
        }
    }

    /** meeting.ts `linkFor`: the page, the meeting's token, the room; the core writes it as the web does. */
    fun link(one: Arrangement): String? {
        val deployment = app.deployment ?: return null
        val token = one.token ?: return null
        return deployment.meetingLink(token, one.room)
    }

    /**
     * calendar.ts `icsFor`: the meeting as a calendar file's text, for the
     * screen to write where the system can hand it to a calendar (a
     * FileProvider share), which needs a Context this model does not hold.
     */
    fun calendarText(one: Arrangement, now: Instant = Instant.now()): String? {
        val link = link(one) ?: return null
        val start = AppModel.instant(one.at) ?: return null
        val stamp = DateTimeFormatter.ofPattern("yyyyMMdd'T'HHmmss'Z'", Locale.ROOT).withZone(ZoneOffset.UTC)
        fun escape(text: String) = text.replace("\\", "\\\\").replace(";", "\\;").replace(",", "\\,").replace("\n", "\\n")
        val lines = listOf(
            "BEGIN:VCALENDAR", "VERSION:2.0", "PRODID:-//Tomoshibi//Arranged meeting//EN", "CALSCALE:GREGORIAN",
            "METHOD:PUBLISH", "BEGIN:VEVENT", "UID:${one.id}@tomoshibi", "DTSTAMP:${stamp.format(now)}",
            "DTSTART:${stamp.format(start)}", "SUMMARY:${escape(one.room)}", "URL:$link",
            "DESCRIPTION:${escape(link)}", "END:VEVENT", "END:VCALENDAR",
        )
        return lines.joinToString("\r\n") + "\r\n"
    }

    // MARK: Rooms lately

    /**
     * A room this device joined, kept so it can be joined again in one press:
     * a call app's recent list, which the web has no place for. Kept on this
     * device only and per deployment, and never anything but the name and
     * when.
     */
    data class Recent(val room: String, val server: String, val at: Instant) {
        val id: String get() = "$server#$room"
    }

    var recents by mutableStateOf(loadRecents())
        private set

    /** The ones joined on the deployment showing now, latest first. */
    val recentHere: List<Recent>
        get() {
            val here = app.deployment?.baseUrl() ?: ""
            return recents.filter { it.server == here }.sortedByDescending { it.at }
        }

    internal fun remember(room: String) {
        val server = app.deployment?.baseUrl() ?: return
        if (app.rehearsal) return
        recents = (listOf(Recent(room, server, Instant.now())) + recents.filter { !(it.server == server && it.room == room) }).take(RECENTS_KEPT)
        saveRecents()
    }

    /** Takes a room off the recent list, which is the person's own record of where they have been and theirs to tidy. */
    fun forget(recent: Recent) {
        recents = recents.filter { it.id != recent.id }
        if (!app.rehearsal) saveRecents()
    }

    private fun saveRecents() {
        val array = JSONArray()
        for (recent in recents) {
            array.put(JSONObject().put("room", recent.room).put("server", recent.server).put("at", recent.at.toEpochMilli()))
        }
        defaults.set(RECENTS_KEY, array.toString())
    }

    private fun loadRecents(): List<Recent> {
        val stored = defaults.string(RECENTS_KEY) ?: return emptyList()
        return try {
            val array = JSONArray(stored)
            (0 until array.length()).mapNotNull { index ->
                val one = array.optJSONObject(index) ?: return@mapNotNull null
                val room = one.optString("room").takeIf { it.isNotEmpty() } ?: return@mapNotNull null
                val server = one.optString("server").takeIf { it.isNotEmpty() } ?: return@mapNotNull null
                Recent(room, server, Instant.ofEpochMilli(one.optLong("at")))
            }
        } catch (_: Exception) {
            emptyList()
        }
    }

    // MARK: The relays from here

    /** Round trips by relay name, null until measured; a relay that did not answer is present with no number. */
    var measured by mutableStateOf<Map<String, Double?>?>(null)
        private set
    var measuring by mutableStateOf(false)
        private set
    /** The order the first measurement settled on, so a row does not move out from under a finger already reaching for it. */
    var order by mutableStateOf<List<String>>(emptyList())
        private set

    suspend fun measure() {
        if (measuring) return
        val relays = app.relays?.relays ?: return
        if (relays.isEmpty()) return
        measuring = true
        try {
            val timings = relayTimings(relays)
            measured = timings.associate { it.name to it.millis }
            if (order.isEmpty()) order = timings.sortedBy { it.millis ?: Double.POSITIVE_INFINITY }.map { it.name }
        } finally {
            measuring = false
        }
    }

    /** The relays under their region, the region's words said in the reader's language. */
    data class RegionGroup(val region: String, val relays: List<Relay>)

    /** The relays under their region, in the settled order. */
    fun grouped(): List<RegionGroup> {
        val relays = app.relays?.relays ?: emptyList()
        val order = order
        val ranked = if (order.isEmpty()) relays else relays.sortedBy { order.indexOf(it.name).let { at -> if (at < 0) Int.MAX_VALUE else at } }
        val out = ArrayList<Pair<String, MutableList<Relay>>>()
        for (relay in ranked) {
            val region = (relay.region ?: "").split("/").map { it.trim() }.filter { it.isNotEmpty() }.joinToString(" · ") { say(it) }
            val group = out.firstOrNull { it.first == region }
            if (group != null) group.second.add(relay) else out.add(region to mutableListOf(relay))
        }
        return out.map { RegionGroup(it.first, it.second) }
    }

    /** The fastest relay that answered, and its round trip. */
    data class Fastest(val name: String, val millis: Double)

    /**
     * The fastest relay that answered, which is what "automatic" settles on.
     * Never one kept for administrators, and never one out of service
     * (relays.ts `fastest`): the join measures without them, so naming one
     * beside "Automatic" told an administrator the call would go somewhere it
     * never would.
     */
    val fastest: Fastest?
        get() {
            val measured = measured ?: return null
            val usable = (app.relays?.relays ?: emptyList()).filter { !it.adminOnly && !it.maintenance }.map { it.name }.toSet()
            return measured.mapNotNull { (name, millis) -> if (name in usable && millis != null) Fastest(name, millis) else null }
                .minByOrNull { it.millis }
        }

    fun label(name: String): String {
        val relay = app.relays?.relays?.firstOrNull { it.name == name } ?: return name
        return say(relay.label ?: relay.name)
    }

    companion object {
        const val RECENTS_KEY = "tomoshibi.recents"
        const val RECENTS_KEPT = 24

        /**
         * meeting.ts `whenSaid`: the date, the time and the zone, in the
         * reader's language and the reader's own zone.
         */
        fun said(at: Instant, locale: Locale = readingLocale(), zone: ZoneId = ZoneId.systemDefault()): String {
            val when_ = DateTimeFormatter.ofLocalizedDateTime(FormatStyle.MEDIUM, FormatStyle.SHORT).withLocale(locale).withZone(zone)
            val timeZone = TimeZone.getTimeZone(zone)
            val daylight = timeZone.inDaylightTime(java.util.Date.from(at))
            return when_.format(at) + " " + timeZone.getDisplayName(daylight, TimeZone.SHORT, locale)
        }

        /** Schedule.tsx `explainArranging`. */
        fun explain(error: Throwable): String {
            val code = (error as? ProtocolException.Refused)?.code ?: return t("Something went wrong. Try again.")
            return when (code) {
                "room_already_arranged" -> t("That room already has a meeting arranged.")
                "bad_time" -> t("That time is not one a meeting can be arranged for.")
                "meetings_need_invites" -> t("Meetings here ask for an account, so a link cannot let anybody in.")
                "not_yours" -> t("That is somebody else's room.")
                "too_many_meetings" -> t("You have as many meetings arranged as you can have.")
                "invalid_room" -> t("Room names can only use lowercase letters, numbers and dashes.")
                "scoped_room" -> t("A room held under a scope cannot be arranged ahead. Send a link with a time on it instead.")
                "relay_not_allowed" -> t("Access denied. That server is for administrators.")
                "not_signed_in" -> t("Your session has expired. Sign in again.")
                "not_allowed" -> t("Arranging meetings has not been opened to you.")
                else -> t("Something went wrong. Try again.")
            }
        }
    }
}
