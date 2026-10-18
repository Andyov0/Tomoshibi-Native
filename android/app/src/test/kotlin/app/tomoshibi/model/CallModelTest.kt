package app.tomoshibi.model

import app.tomoshibi.media.CallAudio
import app.tomoshibi.media.NoAudioRouting
import app.tomoshibi.media.PictureInPicture
import app.tomoshibi.media.Sharing
import kotlinx.coroutines.ExperimentalCoroutinesApi
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
import org.junit.Test
import uniffi.tomoshibi_core.Call
import uniffi.tomoshibi_core.CallEvent
import uniffi.tomoshibi_core.CallException
import uniffi.tomoshibi_core.LosslessSink
import uniffi.tomoshibi_core.MediaKind
import uniffi.tomoshibi_core.NoHandle
import uniffi.tomoshibi_core.ParticipantInfo
import uniffi.tomoshibi_core.PcmSink
import uniffi.tomoshibi_core.TrackInfo
import uniffi.tomoshibi_protocol.Deployment
import uniffi.tomoshibi_protocol.MediaSource

/*
 * How the call's events become what the screens draw (CallModel.apply), with
 * the core stood in for by UniFFI's fake objects. What these guard is the
 * room as somebody sees it disagreeing with the room as it is: everybody
 * already there announced as arriving, a voice subscribed and never played, a
 * move read as the end of the call, a message that did not leave silently
 * thrown away.
 */
@OptIn(ExperimentalCoroutinesApi::class)
class CallModelTest {
    private class FakeCall : Call(NoHandle) {
        val attached = mutableListOf<String>()
        val written = mutableListOf<Map<String, String>>()
        val said = mutableListOf<String>()
        var failChat = false

        override fun roomName() = "team-room"
        override fun localIdentity() = "a-me"
        override fun localName() = "Me"
        override fun attachPcm(sid: String, sink: PcmSink) {
            attached += sid
        }
        override fun detach(sid: String) {}
        override suspend fun setAttributes(attributes: Map<String, String>) {
            written += attributes
        }
        override suspend fun sendChat(text: String) {
            if (failChat) throw CallException.Failed("the line is down")
            said += text
        }
        override fun setLosslessSink(sink: LosslessSink) {}
        override suspend fun leave() {}
    }

    private val call = FakeCall()
    private val audio = CallAudio(NoAudioRouting, muted = true)

    @After
    fun stopTheMixer() {
        audio.stop()
    }

    private fun TestScope.model(): CallModel = CallModel(
        call, object : Deployment(uniffi.tomoshibi_protocol.NoHandle) {}, region = null, sealed = false,
        audio = audio, makeCamera = { error("no camera in a test") }, sharing = Sharing(), pictureInPicture = PictureInPicture(),
        context = StandardTestDispatcher(testScheduler),
    )

    private fun track(sid: String, source: MediaSource, kind: MediaKind = MediaKind.AUDIO, muted: Boolean = false, name: String = "") =
        TrackInfo(sid, kind, source, name, muted, 0u, 0u, false, true)

    private fun person(identity: String, name: String, vararg tracks: TrackInfo) =
        ParticipantInfo(identity, name, emptyMap(), tracks.toList())

    @Test
    fun thoseAlreadyHereAreNotNewsAndThoseWhoComeAre() = runTest {
        val model = model()
        model.apply(CallEvent.ParticipantJoined(person("g-ada", "Ada")))
        assertEquals(listOf("g-ada"), model.identities)
        assertEquals(emptyList<CallToast>(), model.toasts)
        model.settled = true
        model.apply(CallEvent.ParticipantJoined(person("g-bea", "")))
        // Somebody who gave no name is named by their identity.
        assertEquals(listOf("g-bea joined"), model.toasts.map { it.title })
        // Told again about somebody already here is not an arrival.
        model.apply(CallEvent.ParticipantJoined(person("g-ada", "Ada Lovelace")))
        assertEquals(1, model.toasts.size)
        assertEquals("Ada Lovelace", model.person("g-ada")?.name)
    }

    @Test
    fun aSoundIsPlayedTheMomentItIsSubscribed() = runTest {
        val model = model()
        model.apply(CallEvent.ParticipantJoined(person("g-ada", "Ada")))
        val voice = track("TR_voice", MediaSource.MICROPHONE)
        model.apply(CallEvent.TrackSubscribed("g-ada", voice))
        assertEquals(listOf("TR_voice"), call.attached)
        assertEquals(voice, model.person("g-ada")?.track(MediaSource.MICROPHONE))
        model.apply(CallEvent.TrackMuted("g-ada", "TR_voice", true))
        assertTrue(model.person("g-ada")!!.track(MediaSource.MICROPHONE)!!.muted)
        model.apply(CallEvent.TrackUnpublished("g-ada", "TR_voice"))
        assertNull(model.person("g-ada")?.track(MediaSource.MICROPHONE))
    }

    @Test
    fun aPictureIsASurfaceAndAShareIsAnother() = runTest {
        val model = model()
        val camera = track("TR_cam", MediaSource.CAMERA, MediaKind.VIDEO, muted = true)
        model.apply(CallEvent.ParticipantJoined(person("g-ada", "Ada", camera)))
        // A muted camera is a tile with no picture in it.
        assertEquals(listOf("g-ada/camera" to null), model.surfaces.map { it.id to it.sid })
        model.apply(CallEvent.TrackMuted("g-ada", "TR_cam", false))
        model.apply(CallEvent.TrackPublished("g-ada", track("TR_screen", MediaSource.SCREEN_SHARE, MediaKind.VIDEO)))
        assertEquals(listOf("g-ada/camera" to "TR_cam", "g-ada/screen" to "TR_screen"), model.surfaces.map { it.id to it.sid })
        // The small window shows a shared screen first.
        assertEquals("TR_screen", model.pictureInPictureSid)
    }

    @Test
    fun aShareStartedAfterArrivingIsNewsAndMusicIsNot() = runTest {
        val model = model()
        model.apply(CallEvent.ParticipantJoined(person("g-ada", "Ada")))
        model.settled = true
        model.apply(CallEvent.TrackPublished("g-ada", track("TR_music", MediaSource.UNKNOWN, name = "music")))
        assertEquals(emptyList<String>(), model.toasts.map { it.title })
        model.apply(CallEvent.TrackPublished("g-ada", track("TR_listen", MediaSource.SCREEN_SHARE_AUDIO, name = "listening")))
        assertEquals(listOf("Ada is sharing sound"), model.toasts.map { it.title })
        assertEquals(CallToast.Kind.Signal, model.toasts.single().kind)
    }

    @Test
    fun whoIsSpeakingAndWhoseHandIsUp() = runTest {
        val model = model()
        model.apply(CallEvent.ParticipantJoined(person("g-ada", "Ada")))
        model.apply(CallEvent.ParticipantJoined(person("g-bea", "Bea")))
        model.apply(CallEvent.Speaking(listOf("g-bea")))
        assertEquals(listOf(false, true), model.people.map { it.speaking })
        model.apply(CallEvent.AttributesChanged("g-ada", mapOf("hand" to "up")))
        assertTrue(model.person("g-ada")!!.handUp)
        // This participant's own hand is the model's, not a person's.
        model.apply(CallEvent.AttributesChanged("a-me", mapOf("hand" to "up")))
        assertTrue(model.handUp)
        model.setHand(true)
        assertEquals(emptyList<Map<String, String>>(), call.written)
        model.setHand(false)
        assertEquals(listOf(mapOf("hand" to "")), call.written)
    }

    @Test
    fun whatOthersSayIsInTheCornerForSixSecondsWhileTheMessagesAreClosed() = runTest {
        val model = model()
        model.apply(CallEvent.Chat("m1", "g-ada", "Ada", "hello", 1_000L))
        assertEquals(listOf("hello"), model.messages.map { it.text })
        assertTrue(model.unread)
        assertEquals(1, model.arriving.size)
        advanceTimeBy(5_900)
        runCurrent()
        assertEquals(1, model.arriving.size)
        advanceTimeBy(200)
        runCurrent()
        assertEquals(0, model.arriving.size)
        model.chatOpen = true
        assertFalse(model.unread)
        model.apply(CallEvent.Chat("m2", "g-ada", "Ada", "again", 2_000L))
        assertFalse(model.unread)
        assertEquals(0, model.arriving.size)
    }

    @Test
    fun aMessageThatDidNotLeaveIsPutBackInTheBox() = runTest {
        val model = model()
        call.failChat = true
        model.chatDraft = "  see you at three  "
        model.sendDraft()
        assertTrue(model.chatFailed)
        assertEquals("see you at three", model.chatDraft)
        assertEquals(emptyList<ChatLine>(), model.messages)
        // Typing again takes the warning down.
        model.chatDraft = "see you at four"
        assertFalse(model.chatFailed)
        call.failChat = false
        model.sendDraft()
        assertEquals(listOf("see you at four"), call.said)
        assertEquals(listOf(true), model.messages.map { it.mine })
        assertEquals("", model.chatDraft)
    }

    @Test
    fun aRoomThatClosesJustAfterTheServerSaidItWouldMoveIsAMove() = runTest {
        val model = model()
        var movedTo: String? = null
        var endedFor: String? = null
        model.onMoved = { movedTo = it }
        model.onEnded = { endedFor = it }
        // Only the server's word counts; a participant cannot move the room.
        model.apply(CallEvent.Data("moving", "g-ada", "elsewhere".toByteArray()))
        assertNull(model.moving)
        model.apply(CallEvent.Data("moving", null, "tokyo".toByteArray()))
        assertEquals("tokyo", model.moving?.relay)
        model.apply(CallEvent.Ended("RoomDeleted"))
        assertEquals("tokyo", movedTo)
        assertNull(endedFor)
        assertEquals("RoomDeleted", model.endedReason)
    }

    @Test
    fun anyOtherEndIsTheEnd() = runTest {
        val model = model()
        var endedFor: String? = null
        model.onEnded = { endedFor = it }
        model.apply(CallEvent.Ended("ParticipantRemoved"))
        assertEquals("ParticipantRemoved", endedFor)
    }

    @Test
    fun aNoticeSaidAgainUnderItsKeyStaysWhereItIs() = runTest {
        val model = model()
        model.announce(CallToast("Reconnecting", key = "reconnect", lasts = null))
        val first = model.toasts.single().id
        model.announce(CallToast("Reconnected", key = "reconnect"))
        assertEquals(listOf("Reconnected"), model.toasts.map { it.title })
        assertEquals(first, model.toasts.single().id)
    }

    @Test
    fun theFourthPushesOutTheOldestThatGoesByItselfAndNeverTheDoor() = runTest {
        val model = model()
        model.announce(CallToast("Somebody is at the door", lasts = null))
        model.announce(CallToast("one"))
        model.announce(CallToast("two"))
        model.announce(CallToast("three"))
        assertEquals(listOf("Somebody is at the door", "two", "three"), model.toasts.map { it.title })
        assertTrue(model.toasts.first().closable)
        assertFalse(model.toasts.last().closable)
    }

    @Test
    fun aNoticeGoesByItselfButNotWhileTheCornerIsRead() = runTest {
        val model = model()
        model.announce(CallToast("Ada joined"))
        model.holdToasts(true)
        advanceTimeBy(5_000)
        runCurrent()
        assertEquals(1, model.toasts.size)
        model.holdToasts(false)
        // Let go, it has at least a second more.
        advanceTimeBy(900)
        runCurrent()
        assertEquals(1, model.toasts.size)
        advanceTimeBy(2_400)
        runCurrent()
        assertEquals(0, model.toasts.size)
    }

    @Test
    fun aNoticeWithAnActionCallsItOnceAndGoes() = runTest {
        val model = model()
        var pressed = 0
        model.announce(CallToast("Can't use your camera", actionTitle = "Open Settings", lasts = null)) { pressed++ }
        model.act(model.toasts.single().id)
        assertEquals(1, pressed)
        assertEquals(emptyList<CallToast>(), model.toasts)
    }

    @Test
    fun somebodyLeavingIsSaidAndTheirSoundStops() = runTest {
        val model = model()
        model.apply(CallEvent.ParticipantJoined(person("g-ada", "Ada", track("TR_voice", MediaSource.MICROPHONE))))
        model.apply(CallEvent.TrackSubscribed("g-ada", track("TR_voice", MediaSource.MICROPHONE)))
        assertEquals(1f, audio.volume("TR_voice"))
        model.apply(CallEvent.ParticipantLeft("g-ada"))
        assertEquals(emptyList<Person>(), model.people)
        assertEquals(listOf("Ada left"), model.toasts.map { it.title })
        assertNull(audio.volume("TR_voice"))
    }

    @Test
    fun theProtocolsLayeredOverTheCallHearEveryEventAfterTheModel() = runTest {
        val model = model()
        val heard = mutableListOf<Int>()
        model.listen { heard += model.people.size }
        model.apply(CallEvent.ParticipantJoined(person("g-ada", "Ada")))
        assertEquals(listOf(1), heard)
    }
}
