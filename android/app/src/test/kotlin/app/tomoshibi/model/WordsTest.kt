package app.tomoshibi.model

import java.time.Instant
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.tomoshibi_protocol.ProtocolException

/*
 * What the server's refusals read as (api.ts `explain`, Account.tsx,
 * Schedule.tsx), how a call's end is told apart, and the links the app opens.
 * What these guard is somebody being told the wrong next move: "Could not
 * join" where the room was only by invitation, "You left" after the host
 * closed the room, a link to another server followed.
 *
 * The scope branches ("Only members of {scope}") ask the core for the scope
 * and are left to the device; the core's own tests cover scopeOf.
 */
class WordsTest {
    private fun refused(code: String) = ProtocolException.Refused(403u, code)

    @Test
    fun theServersRefusalsReadAsTheWebSaysThem() {
        val room = "team-room"
        assertEquals("Could not reach the server.", AppModel.explain(ProtocolException.Network("timed out"), room))
        assertEquals("Too many attempts. Try again in a moment.", AppModel.explain(refused("rate_limited"), room))
        assertEquals("Room names can only use lowercase letters, numbers and dashes.", AppModel.explain(refused("invalid_room"), room))
        assertEquals("team-room isn't open. Ask the organiser for the link.", AppModel.explain(refused("room_not_open"), room))
        assertEquals("team-room is by invitation. Ask the organiser for a link.", AppModel.explain(refused("not_invited"), room))
        assertEquals("Access denied. That server is for administrators.", AppModel.explain(refused("relay_not_allowed"), room))
        assertEquals("This passphrase cannot join. Ask the organiser.", AppModel.explain(refused("blocked"), room))
        assertEquals("That name and passphrase do not go together.", AppModel.explain(refused("not_yours"), room))
        assertEquals("Something went wrong. Try again.", AppModel.explain(refused("server_error"), room))
        // A code nobody has a sentence for is about the room they tried.
        assertEquals("Could not join team-room.", AppModel.explain(refused("something_new"), room))
        // Anything that is not the server answering is not about the room.
        assertEquals("Something went wrong. Try again.", AppModel.explain(ProtocolException.Unexpected("garbled"), room))
        assertEquals("Something went wrong. Try again.", AppModel.explain(IllegalStateException(), room))
    }

    @Test
    fun aKnockIsOfferedOnlyWhereTheRoomWasByInvitation() {
        val room = "team-room"
        assertTrue(AppModel.canKnock(AppModel.explain(refused("not_invited"), room), room))
        assertFalse(AppModel.canKnock(null, room))
    }

    @Test
    fun guestLinkAndPassphraseRefusalsHaveTheirOwnSentences() {
        assertEquals("You can make links only into rooms of your own scopes.", AppModel.explainLink(refused("not_yours")))
        assertEquals("That name is too long.", AppModel.explainLink(refused("name_too_long")))
        assertEquals("That window cannot be used. It has to end after it starts, and after now.", AppModel.explainLink(refused("bad_time")))
        assertEquals("Guest links have not been opened to you.", AppModel.explainLink(refused("not_allowed")))
        assertEquals("That link could not be made. Try again.", AppModel.explainLink(ProtocolException.Network("down")))

        assertEquals("That is not your current passphrase.", AppModel.explainPassphrase(refused("not_yours")))
        assertEquals("At least eight characters.", AppModel.explainPassphrase(refused("passphrase_too_short")))
        assertEquals("That is the passphrase you already have.", AppModel.explainPassphrase(refused("passphrase_unchanged")))
        assertEquals("Somebody else already uses that passphrase. Choose another.", AppModel.explainPassphrase(refused("passphrase_in_use")))
        assertEquals("That could not be changed. Try again.", AppModel.explainPassphrase(refused("other")))
    }

    @Test
    fun arrangingRefusalsReadAsTheSchedulePageSaysThem() {
        assertEquals("That room already has a meeting arranged.", LobbyModel.explain(refused("room_already_arranged")))
        assertEquals("Your session has expired. Sign in again.", LobbyModel.explain(refused("not_signed_in")))
        assertEquals(
            "A room held under a scope cannot be arranged ahead. Send a link with a time on it instead.",
            LobbyModel.explain(refused("scoped_room")),
        )
        assertEquals("Something went wrong. Try again.", LobbyModel.explain(refused("nope")))
        assertEquals("Something went wrong. Try again.", LobbyModel.explain(RuntimeException()))
    }

    @Test
    fun threeEndingsThatLookAlikeFromInsideAreToldApart() {
        // The Rust SDK's name for the protocol's ROOM_DELETED; waiting only
        // for RoomClosed left the app in a call that had gone.
        assertEquals(AppModel.Ending.Closed, AppModel.endingFor("RoomDeleted"))
        assertEquals(AppModel.Ending.Closed, AppModel.endingFor("Disconnected(RoomClosed)"))
        assertEquals(AppModel.Ending.Removed, AppModel.endingFor("ParticipantRemoved"))
        assertEquals(AppModel.Ending.Left, AppModel.endingFor("ClientInitiated"))
        assertEquals(AppModel.Ending.Lost, AppModel.endingFor("SignalClose"))
        assertEquals(AppModel.Ending.Lost, AppModel.endingFor(""))
    }

    @Test
    fun aMoveIsOnlyARoomTakenAway() {
        assertTrue(CallModel.movedAway("RoomDeleted"))
        assertTrue(CallModel.movedAway("ParticipantRemoved"))
        assertFalse(CallModel.movedAway("ClientInitiated"))
        assertFalse(CallModel.movedAway("SignalClose"))
    }

    @Test
    fun aNoticeNamesSomebodyByTheirIdentityWhenTheyGaveNoName() {
        assertEquals("Ada", CallModel.named("Ada", "g123"))
        assertEquals("g123", CallModel.named("", "g123"))
    }

    @Test
    fun theServersInstantsAreReadWithOrWithoutFractions() {
        assertEquals(Instant.parse("2026-10-18T12:00:00Z"), AppModel.instant("2026-10-18T12:00:00Z"))
        assertEquals(Instant.parse("2026-10-18T12:00:00.250Z"), AppModel.instant("2026-10-18T12:00:00.250Z"))
        assertEquals(Instant.parse("2026-10-18T04:00:00Z"), AppModel.instant("2026-10-18T12:00:00+08:00"))
        assertNull(AppModel.instant("tomorrow"))
    }

    @Test
    fun aLinkSaysWhereItGoesAndWhatItCarries() {
        val invite = OpenedLink.parse("https://meet.example.com/?invite=T0k%2Ben+x#/team-room")!!
        assertEquals("https://meet.example.com", invite.origin)
        assertEquals("team-room", invite.room)
        // A plus in a query item is a plus, as URLComponents reads it.
        assertEquals("T0k+en+x", invite.invite)
        assertNull(invite.meeting)
        assertTrue(invite.isFor("https://meet.example.com/"))

        val meeting = OpenedLink.parse("tomoshibi://meet.example.com/?meeting=abc#/standup")!!
        assertEquals("https://meet.example.com", meeting.origin)
        assertEquals("abc", meeting.meeting)
        assertEquals("standup", meeting.room)

        // What the deployment's invitation page hands the app when it opens
        // it: tomoshibi://<host>/?invite=<token>#/<room>.
        val handed = OpenedLink.parse("tomoshibi://meet.example.com/?invite=Ab-_9xYz#/team-room")!!
        assertEquals("Ab-_9xYz", handed.invite)
        assertEquals("team-room", handed.room)
        assertTrue(handed.isFor("https://meet.example.com/"))

        val bare = OpenedLink.parse("https://meet.example.com/#/plain")!!
        assertNull(bare.invite)
        assertEquals("plain", bare.room)
    }

    @Test
    fun aLinkIsReadOnEveryAndroidTheAppRunsOn() {
        // Words in a room's invitation as a browser writes them, and an
        // escape that is not one, which is kept as it was typed.
        val written = OpenedLink.parse("https://meet.example.com/?invite=%E4%BD%A0%E5%A5%BD%20x+y#/r")!!
        assertEquals("\u4f60\u597d x+y", written.invite)
        assertEquals("50%", OpenedLink.decode("50%"))
        // Read on Android 10 to 12 too: URLDecoder.decode(String, Charset) is
        // Android 13's, and the app is for 10 and later. Its descriptor in
        // the compiled reader is the call that closed the app there.
        val compiled = OpenedLink::class.java.classLoader!!
            .getResourceAsStream("app/tomoshibi/model/OpenedLink\$Companion.class")!!
            .use { it.readBytes() }
            .toString(Charsets.ISO_8859_1)
        assertTrue(compiled.contains("java/net/URLDecoder"))
        assertFalse(compiled.contains("(Ljava/lang/String;Ljava/nio/charset/Charset;)Ljava/lang/String;"))
    }

    @Test
    fun aLinkToAnotherServerIsNotThisOnesHoweverItIsWritten() {
        assertFalse(OpenedLink.parse("https://meet.example.com.example.org/?invite=x")!!.isFor("https://meet.example.com/"))
        assertFalse(OpenedLink.parse("http://meet.example.com/?invite=x")!!.isFor("https://meet.example.com/"))
        assertFalse(OpenedLink.parse("https://meet.example.com:8443/?invite=x")!!.isFor("https://meet.example.com/"))
        // The default port and the host's case are the same server.
        assertTrue(OpenedLink.parse("https://MEET.example.com:443/?invite=x")!!.isFor("https://meet.example.com/"))
        assertTrue(OpenedLink.parse("http://127.0.0.1:8080/?invite=x#/r")!!.isFor("http://127.0.0.1:8080/"))
        assertNull(OpenedLink.parse("not a link"))
    }
}
