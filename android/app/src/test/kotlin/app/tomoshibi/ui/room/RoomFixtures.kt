package app.tomoshibi.ui.room

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import app.tomoshibi.media.CallAudio
import app.tomoshibi.media.NoAudioRouting
import app.tomoshibi.media.PictureInPicture
import app.tomoshibi.media.Sharing
import app.tomoshibi.model.AppModel
import app.tomoshibi.model.CallModel
import app.tomoshibi.model.MemoryDefaults
import app.tomoshibi.model.MemorySecrets
import kotlinx.coroutines.Dispatchers
import uniffi.tomoshibi_core.Call
import uniffi.tomoshibi_core.CallEvent
import uniffi.tomoshibi_core.LosslessSink
import uniffi.tomoshibi_core.MediaKind
import uniffi.tomoshibi_core.NoHandle
import uniffi.tomoshibi_core.ParticipantInfo
import uniffi.tomoshibi_core.PcmSink
import uniffi.tomoshibi_core.TrackInfo
import uniffi.tomoshibi_protocol.Deployment
import uniffi.tomoshibi_protocol.DeploymentInfo
import uniffi.tomoshibi_protocol.DoorState
import uniffi.tomoshibi_protocol.HostStanding
import uniffi.tomoshibi_protocol.Joining
import uniffi.tomoshibi_protocol.Knocker
import uniffi.tomoshibi_protocol.MadeInvite
import uniffi.tomoshibi_protocol.MediaSource
import uniffi.tomoshibi_protocol.Opening
import uniffi.tomoshibi_protocol.Relay
import uniffi.tomoshibi_protocol.RelayList

/*
 * A room with nobody really in it: the core stood in for by UniFFI's fake
 * objects, the deployment by one that answers from memory, and the
 * protocol crate's rules by their plain meanings, so that the room can be
 * drawn on a computer, where the core's library cannot be loaded. Remote
 * pictures and one's own camera are coloured stand-ins where the picture
 * would be.
 */

internal class FakeCall(private val room: String = "design-review") : Call(NoHandle) {
    val sent = mutableListOf<String>()
    val written = mutableListOf<Map<String, String>>()

    override fun roomName() = room
    override fun localIdentity() = "p-andy"
    override fun localName() = "Andy"
    override fun attachPcm(sid: String, sink: PcmSink) {}
    override fun detach(sid: String) {}
    override suspend fun setAttributes(attributes: Map<String, String>) {
        written += attributes
    }
    override suspend fun sendChat(text: String) {
        sent += text
    }
    override suspend fun sendData(topic: String, payload: ByteArray, destinations: List<String>, reliable: Boolean) {}
    override fun setLosslessSink(sink: LosslessSink) {}
    override fun setRemoteEnabled(sid: String, enabled: Boolean) {}
    override fun setBackground(background: Boolean) {}
    override fun token() = "token"
    var left = false
    override suspend fun leave() {
        left = true
    }
}

internal class FakeDeployment(
    var hosting: Boolean = false,
    var knockers: List<Knocker> = emptyList(),
    var joining: Joining = Joining.ANYONE,
) : Deployment(uniffi.tomoshibi_protocol.NoHandle) {
    override fun baseUrl() = "https://meet.example/"
    override fun inviteLink(token: String, room: String) = "https://meet.example/?invite=$token#/$room"
    override suspend fun deploymentInfo() = DeploymentInfo(Opening.ANYONE, joining, "", "")
    override suspend fun hostStanding(room: String, bearer: String) = HostStanding(yours = hosting, admin = false)
    override suspend fun knocking(room: String, bearer: String) = knockers
    override suspend fun answerKnock(room: String, id: String, bearer: String, admit: Boolean) = DoorState.ADMITTED
    override suspend fun createInvite(room: String, bearer: String) = MadeInvite("t0k3n", "", room, null, null, null)
    override suspend fun relays() = RelayList(
        listOf(relay("hk-1", "Hong Kong"), relay("sg-2", "Singapore"), relay("tyo-3", "Tokyo")),
        measure = true, online = 3u, total = 3u,
    )

    private fun relay(name: String, label: String) = Relay(name, null, null, label, null, null, null, false, false, false)
}

internal fun app(): AppModel = AppModel(MemoryDefaults(), MemorySecrets(), media = null, home = "https://meet.example/", context = Dispatchers.Main.immediate)

internal fun callModel(call: FakeCall = FakeCall(), deployment: FakeDeployment = FakeDeployment(), sealed: Boolean = false): CallModel =
    CallModel(
        call, deployment, region = null, sealed = sealed,
        audio = CallAudio(NoAudioRouting, muted = true), makeCamera = { error("no camera in a picture") },
        sharing = Sharing(), pictureInPicture = PictureInPicture(),
    )

internal fun track(sid: String, source: MediaSource, kind: MediaKind = MediaKind.VIDEO, muted: Boolean = false, name: String = "") =
    TrackInfo(sid, kind, source, name, muted, 1280u, 720u, false, true)

/** Somebody in the room, with a camera that is on unless [camera] is false, and a microphone. */
internal fun CallModel.joins(
    identity: String,
    name: String,
    camera: Boolean = true,
    microphone: Boolean = true,
    screen: Boolean = false,
    attributes: Map<String, String> = emptyMap(),
) {
    val tracks = buildList {
        add(track("TR_${identity}_mic", MediaSource.MICROPHONE, MediaKind.AUDIO, muted = !microphone))
        if (camera) add(track("TR_${identity}_cam", MediaSource.CAMERA))
        if (screen) add(track("TR_${identity}_screen", MediaSource.SCREEN_SHARE))
    }
    apply(CallEvent.ParticipantJoined(ParticipantInfo(identity, name, attributes, tracks)))
}

internal val PEOPLE = listOf(
    "p-mei" to "Mei Lin", "p-kenji" to "Kenji Sato", "g-ola" to "Ola Nordmann", "p-ines" to "Inês Duarte",
    "g-tom" to "Tom Becker", "p-amara" to "Amara Okafor", "g-li" to "Li Wei", "p-sofia" to "Sofía Ruiz",
    "g-noah" to "Noah Kim", "p-yuki" to "Yuki Tanaka", "g-omar" to "Omar Haddad", "p-lena" to "Lena Vogel",
    "g-ravi" to "Ravi Patel", "p-chloe" to "Chloé Martin", "g-sam" to "Sam Taylor",
)

/** Coloured stand-ins where a picture would be: a camera's warm wash, a screen's cool slide. */
internal val standInPictures = RoomPictures(
    remote = { _, sid, _, modifier -> StandIn(sid, modifier) },
    self = { _, modifier -> StandIn("self", modifier) },
)

private val washes = listOf(
    listOf(Color(0xFF6B4E3D), Color(0xFF2E2620)), listOf(Color(0xFF3D5A6B), Color(0xFF1F2A33)),
    listOf(Color(0xFF5B6B3D), Color(0xFF262E1F)), listOf(Color(0xFF6B3D5A), Color(0xFF2E1F29)),
    listOf(Color(0xFF4A3D6B), Color(0xFF221F2E)), listOf(Color(0xFF6B5E3D), Color(0xFF2E2A1F)),
)

@Composable
private fun StandIn(seed: String, modifier: Modifier) {
    val slide = seed.contains("screen")
    val colours = if (slide) listOf(Color(0xFFE9EEF4), Color(0xFF8FA6BF)) else washes[Math.floorMod(seed.hashCode(), washes.size)]
    Box(modifier.background(Brush.linearGradient(colours)))
}
