package app.tomoshibi.ui.lobby

import androidx.compose.runtime.MutableState
import app.tomoshibi.media.CallAudio
import app.tomoshibi.media.NoAudioRouting
import app.tomoshibi.media.PictureInPicture
import app.tomoshibi.media.Sharing
import app.tomoshibi.model.AppModel
import app.tomoshibi.model.CallModel
import app.tomoshibi.model.LobbyModel
import app.tomoshibi.model.MemoryDefaults
import app.tomoshibi.model.MemorySecrets
import kotlinx.coroutines.Dispatchers
import org.json.JSONArray
import org.json.JSONObject
import uniffi.tomoshibi_core.Call
import uniffi.tomoshibi_core.LosslessSink
import uniffi.tomoshibi_core.NoHandle
import uniffi.tomoshibi_core.PcmSink
import uniffi.tomoshibi_protocol.Account
import uniffi.tomoshibi_protocol.Arrangement
import uniffi.tomoshibi_protocol.Deployment
import uniffi.tomoshibi_protocol.DeploymentInfo
import uniffi.tomoshibi_protocol.InviteInfo
import uniffi.tomoshibi_protocol.Joining
import uniffi.tomoshibi_protocol.Opening
import uniffi.tomoshibi_protocol.Relay
import uniffi.tomoshibi_protocol.RelayList
import java.time.Instant
import java.time.ZoneId
import java.time.temporal.ChronoUnit

/*
 * The screens before a call, drawn with what a deployment would have said
 * but with no deployment and no core: the core's library is built for the
 * phone and cannot be loaded on the JVM the tests run on. The model's fields
 * are set where its own code would set them after a reply, through their
 * snapshot state, so that the screens read them exactly as they do in the
 * app; nothing on these screens may ask the core anything while they are
 * drawn, which is a rule the screens keep anyway (a name typed is checked only
 * once there is one).
 */

const val BASE = "https://live.example.test/"

/**
 * The core's rules for room names written plainly (core/protocol
 * identity.rs: lowercase letters, digits and dashes, a scope after `@`),
 * standing in for the core while the screens are drawn on the JVM; put back
 * after each test.
 */
object PlainRoomNames {
    private val normalise = RoomNames.normalise
    private val valid = RoomNames.valid
    private val generate = RoomNames.generate
    private val scope = RoomNames.scope
    private val generated = RoomNames.generated

    fun install() {
        RoomNames.normalise = { it.trim().lowercase().replace(Regex("\\s+"), "-") }
        RoomNames.valid = { Regex("^[a-z0-9](?:[a-z0-9-]*[a-z0-9])?(@[a-z0-9-]+)?$").matches(it) }
        RoomNames.generate = { "quiet-lantern-42" }
        RoomNames.scope = { it.substringAfter('@', "") }
        RoomNames.generated = { false }
    }

    fun remove() {
        RoomNames.normalise = normalise
        RoomNames.valid = valid
        RoomNames.generate = generate
        RoomNames.scope = scope
        RoomNames.generated = generated
    }
}

/** A deployment that answers only what the lobby asks of it while it is drawn. */
class FakeDeployment(private val arranged: List<Arrangement> = emptyList()) : Deployment(uniffi.tomoshibi_protocol.NoHandle) {
    var live: Boolean? = null
    val asked = mutableListOf<String>()

    override fun baseUrl(): String = BASE
    override fun meetingLink(token: String, room: String): String = "${BASE}?meeting=$token#/$room"
    override suspend fun arrangements(): List<Arrangement> = arranged
    override suspend fun roomLive(room: String): Boolean? {
        asked += room
        return live
    }
}

/** A call with nobody in it, for the screens that are drawn over one. */
class FakeCall(private val room: String = "design-review") : Call(NoHandle) {
    override fun roomName() = room
    override fun localIdentity() = "a-me"
    override fun localName() = "Ada"
    override fun attachPcm(sid: String, sink: PcmSink) {}
    override fun detach(sid: String) {}
    override suspend fun setAttributes(attributes: Map<String, String>) {}
    override fun setLosslessSink(sink: LosslessSink) {}
    override suspend fun leave() {}
}

/** Sets a model's snapshot-state field as its own code would. */
@Suppress("UNCHECKED_CAST")
fun Any.stage(field: String, value: Any?) {
    var type: Class<*>? = javaClass
    while (type != null) {
        val found = try {
            type.getDeclaredField("$field\$delegate")
        } catch (_: NoSuchFieldException) {
            null
        }
        if (found != null) {
            found.isAccessible = true
            (found.get(this) as MutableState<Any?>).value = value
            return
        }
        type = type.superclass
    }
    error("${javaClass.simpleName} has no $field")
}

val ada = Account("Ada", "trip-ada", null, false, "2025-03-01T10:00:00Z", listOf("lab"), null)

val relays = RelayList(
    listOf(
        Relay("tokyo", null, "Asia / Japan", "Tokyo", null, null, null, false, false, false),
        Relay("hongkong", null, "Asia / Hong Kong", "Hong Kong", null, null, null, false, false, false),
        Relay("lax", null, "North America", "Los Angeles", null, null, null, true, false, false),
    ),
    measure = true,
    online = 2u,
    total = 3u,
)

/** Meetings on fixed days from now at fixed hours, so the words on them are the same each run of the day. */
fun meetings(now: Instant = Instant.now()): List<Arrangement> {
    val zone = ZoneId.of("Asia/Shanghai")
    val today = now.atZone(zone).truncatedTo(ChronoUnit.DAYS)
    return listOf(
        Arrangement("m1", "weekly-sync", now.minusSeconds(600).toString(), "tokyo", started = true, ended = false,
            from = null, token = "tok1", mine = true, invite = "inv1"),
        Arrangement("m2", "design-review", today.plusDays(1).withHour(10).toInstant().toString(), null, started = false, ended = false,
            from = null, token = "tok2", mine = true, invite = null),
        Arrangement("m3", "quarterly-plan", today.plusDays(9).withHour(15).withMinute(30).toInstant().toString(), "hongkong", started = false,
            ended = false, from = null, token = "tok3", mine = true, invite = null),
    )
}

/**
 * The app as a signed-in person sees it in the lobby: three recent rooms,
 * three meetings, three relays measured.
 */
fun fakeApp(
    signedIn: Boolean = true,
    withRelays: Boolean = true,
    withRecents: Boolean = true,
    withMeetings: Boolean = true,
    screen: AppModel.Screen = AppModel.Screen.Lobby,
): AppModel {
    val defaults = MemoryDefaults()
    if (withRecents) {
        val now = Instant.now()
        val kept = JSONArray()
        for ((room, ago) in listOf("design-review" to 2 * 3600L, "friday-standup" to 26 * 3600L, "lab-retro@lab" to 4 * 86_400L)) {
            kept.put(JSONObject().put("room", room).put("server", BASE).put("at", now.minusSeconds(ago).toEpochMilli()))
        }
        defaults.set(LobbyModel.RECENTS_KEY, kept.toString())
    }
    val arranged = if (withMeetings) meetings() else emptyList()
    val app = AppModel(defaults, MemorySecrets(), media = null, home = BASE, context = Dispatchers.Unconfined)
    app.stage("deployment", FakeDeployment(arranged))
    app.stage("info", DeploymentInfo(Opening.SIGNED, Joining.ANYONE, "https://example.test/source", "abc123"))
    if (signedIn) app.stage("account", ada)
    if (withRelays) {
        app.stage("relaysState", relays)
        app.lobby.stage("measured", mapOf("tokyo" to 42.0, "hongkong" to 88.0, "lax" to null))
        app.lobby.stage("order", listOf("tokyo", "hongkong", "lax"))
    }
    app.lobby.stage("arrangements", arranged)
    app.lobby.stage("status", LobbyModel.Status.Ready)
    app.stage("screenState", screen)
    return app
}

/** An invitation as the deployment describes one, for a guest's join screen. */
fun invitation(name: String? = null) = InviteInfo("design-review", name, null, Instant.now().plus(2, ChronoUnit.DAYS).toString(), null)

/** A call to draw the room over, with its sound going nowhere. */
fun fakeCall(audio: CallAudio): CallModel = CallModel(
    FakeCall(), FakeDeployment(), region = null, sealed = false,
    audio = audio, makeCamera = { error("no camera in a test") }, sharing = Sharing(), pictureInPicture = PictureInPicture(),
    context = Dispatchers.Unconfined,
)

fun silentAudio() = CallAudio(NoAudioRouting, muted = true)
