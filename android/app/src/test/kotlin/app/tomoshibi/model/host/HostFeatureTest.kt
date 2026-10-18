package app.tomoshibi.model.host

import app.tomoshibi.media.CallAudio
import app.tomoshibi.media.NoAudioRouting
import app.tomoshibi.media.PictureInPicture
import app.tomoshibi.media.Sharing
import app.tomoshibi.model.CallModel
import app.tomoshibi.model.Sound
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import uniffi.tomoshibi_core.Call
import uniffi.tomoshibi_core.CallEvent
import uniffi.tomoshibi_core.LosslessSink
import uniffi.tomoshibi_core.MediaKind
import uniffi.tomoshibi_core.NoHandle
import uniffi.tomoshibi_core.ParticipantInfo
import uniffi.tomoshibi_core.PcmSink
import uniffi.tomoshibi_core.TrackInfo
import uniffi.tomoshibi_protocol.Deployment
import uniffi.tomoshibi_protocol.HostStanding
import uniffi.tomoshibi_protocol.Knocker
import uniffi.tomoshibi_protocol.MediaSource
import uniffi.tomoshibi_protocol.ProtocolException

/*
 * The host's feature against a call and a deployment stood in for: the door
 * said in the corner while the panel is shut and withdrawn while it is open,
 * a sound stopped at the server only by whoever stopped it, a reaction
 * arriving shown for four seconds, the room changing hands taking the door
 * away, and the server's codes said as sentences. What these guard is a
 * knock nobody sees, a sound somebody else held back let flow again, and a
 * host shown controls that all answer "not yours".
 */
@OptIn(ExperimentalCoroutinesApi::class)
class HostFeatureTest {
    private class FakeCall : Call(NoHandle) {
        val enabled = mutableListOf<Pair<String, Boolean>>()
        override fun roomName() = "standup"
        override fun localIdentity() = "p-me"
        override fun localName() = "Me"
        override fun attachPcm(sid: String, sink: PcmSink) {}
        override fun detach(sid: String) {}
        override fun setLosslessSink(sink: LosslessSink) {}
        override fun setRemoteEnabled(sid: String, enabled: Boolean) {
            this.enabled += sid to enabled
        }
        override fun token() = "token"
        override suspend fun leave() {}
    }

    private class FakeDeployment : Deployment(uniffi.tomoshibi_protocol.NoHandle) {
        var yours = true
        var knockers = listOf(Knocker("k1", "Ada", "203.0.113.24", ""))
        override suspend fun hostStanding(room: String, bearer: String) = HostStanding(yours, admin = false)
        override suspend fun knocking(room: String, bearer: String) = knockers
    }

    private val call = FakeCall()
    private val deployment = FakeDeployment()
    private val audio = CallAudio(NoAudioRouting, muted = true)

    @Before
    fun rules() = standInRules()

    @After
    fun stopTheMixer() = audio.stop()

    private fun TestScope.model(): CallModel = CallModel(
        call, deployment, region = null, sealed = false,
        audio = audio, makeCamera = { error("no camera in a test") }, sharing = Sharing(), pictureInPicture = PictureInPicture(),
        context = StandardTestDispatcher(testScheduler),
    )

    private fun track(sid: String, source: MediaSource) = TrackInfo(sid, MediaKind.AUDIO, source, "", false, 0u, 0u, false, true)

    @Test
    fun somebodyAtTheDoorIsSaidUntilThePanelListsThem() = runTest {
        val model = model()
        val feature = HostFeature.of(model)
        backgroundScope.launch { feature.run() }
        runCurrent()
        val door = model.toasts.single { it.key == HostFeature.DOOR }
        assertEquals("Somebody is waiting to be let in", door.title)
        // Nothing to close it with and no end: somebody is outside until
        // they are answered.
        assertNull(door.lasts)
        assertFalse(door.closable)
        assertEquals(HostFeature.DOOR, door.attachment)
        feature.panelOpen = true
        assertTrue(model.toasts.none { it.key == HostFeature.DOOR })
        feature.panelOpen = false
        val again = model.toasts.single { it.key == HostFeature.DOOR }
        // A second knock is counted in the same notice, in place.
        deployment.knockers = deployment.knockers + Knocker("k2", "Bo", "198.51.100.7", "")
        advanceTimeBy(HostFeature.LOOK_EVERY_MILLIS + 1)
        runCurrent()
        assertEquals(listOf("2 people are waiting to be let in"), model.toasts.filter { it.key == HostFeature.DOOR }.map { it.title })
        assertEquals(again.id, model.toasts.single { it.key == HostFeature.DOOR }.id)
    }

    @Test
    fun aRoomHandedToSomebodyElseTakesTheDoorAway() = runTest {
        val model = model()
        val feature = HostFeature.of(model)
        backgroundScope.launch { feature.run() }
        runCurrent()
        assertEquals(1, feature.waiting.size)
        deployment.yours = false
        feature.refreshStanding()
        assertTrue(feature.waiting.isEmpty())
        assertTrue(model.toasts.none { it.key == HostFeature.DOOR })
    }

    @Test
    fun aSoundIsLetFlowAgainOnlyByWhoeverStoppedIt() = runTest {
        val model = model()
        val feature = HostFeature.of(model)
        model.apply(CallEvent.ParticipantJoined(ParticipantInfo("g-stopper", "Ada", emptyMap(), listOf(track("TR_voice", MediaSource.MICROPHONE)))))
        backgroundScope.launch { feature.run() }
        runCurrent()
        // Nothing decided: nothing asked of the server.
        assertTrue(call.enabled.isEmpty())
        val ada = model.person("g-stopper")!!
        feature.setBlocked(ada, Sound.Voice, true)
        assertEquals("TR_voice" to false, call.enabled.last())
        assertTrue(feature.setting(ada, Sound.Voice).silenced)
        feature.setBlocked(ada, Sound.Voice, false)
        assertEquals("TR_voice" to true, call.enabled.last())
        // Applied again with nothing stopped, nothing more is let flow.
        val asked = call.enabled.size
        feature.applyHearing()
        assertEquals(asked, call.enabled.size)
    }

    @Test
    fun aReactionIsSeenForFourSecondsOnWhoeverSentIt() = runTest {
        val model = model()
        val feature = HostFeature.of(model)
        backgroundScope.launch { feature.run() }
        runCurrent()
        val thumbs = CrateRules.reactionChoices().first()
        model.apply(CallEvent.Data("reaction", "p-mei", thumbs.toByteArray()))
        assertEquals(listOf(thumbs), ReactionBoard.of(model).on("p-mei").map { it.what })
        // A server message has no sender, and is not anybody's reaction.
        model.apply(CallEvent.Data("reaction", null, thumbs.toByteArray()))
        assertEquals(1, ReactionBoard.of(model).shown.size)
        advanceTimeBy(4_001)
        runCurrent()
        assertTrue(ReactionBoard.of(model).shown.isEmpty())
    }

    @Test
    fun theServersCodesAreSaidAsSentences() {
        assertEquals("You no longer run this room.", HostFeature.explain(ProtocolException.Refused(403u, "not_yours")))
        assertEquals("They have already left.", HostFeature.explain(ProtocolException.Refused(404u, "no_such_person")))
        assertEquals("That could not be done. Try again.", HostFeature.explain(ProtocolException.Refused(500u, "who_knows")))
        assertEquals("That could not be done. Try again.", HostFeature.explain(IllegalStateException()))
    }
}
