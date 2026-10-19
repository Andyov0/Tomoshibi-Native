package app.tomoshibi.model

import app.tomoshibi.ui.lobby.BASE
import app.tomoshibi.ui.lobby.fakeApp
import app.tomoshibi.ui.lobby.relays
import app.tomoshibi.ui.lobby.stage
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/*
 * Who is offered the server picker on the screens before a call. The lobby
 * has its own, in the sheet a signed-in person presses Join in, and that join
 * goes straight into the call; somebody who arrives by an invitation or a
 * meeting's link never sees it. What these guard is that person, signed in or
 * not, being left with no way to choose where their call is held (it was
 * offered only to somebody signed out, and never on the waiting screen), and
 * the picker turning up twice for somebody who already chose in the lobby.
 */
class RelayChoiceAtTheDoorTest {
    private val invitation = "${BASE}?invite=tok-invite#/design-review"
    private val meeting = "${BASE}?meeting=tok-meeting#/design-review"

    @Test
    fun anInvitationOffersItToSomebodySignedIn() = runBlocking {
        val app = fakeApp(signedIn = true)
        app.open(invitation)
        assertEquals(AppModel.Screen.Prejoin, app.screen)
        assertTrue(app.arrivedByLink)
        assertTrue(app.offersRelayChoiceAtTheDoor)
    }

    @Test
    fun aMeetingsLinkOffersItWhileWaiting() = runBlocking {
        val app = fakeApp(signedIn = true)
        app.open(meeting)
        assertEquals(AppModel.Screen.Waiting, app.screen)
        assertTrue(app.offersRelayChoiceAtTheDoor)
    }

    @Test
    fun somebodySignedOutIsOfferedItHoweverTheyCame() = runBlocking {
        val app = fakeApp(signedIn = false)
        assertTrue(app.offersRelayChoiceAtTheDoor)
        app.open(invitation)
        assertTrue(app.offersRelayChoiceAtTheDoor)
    }

    @Test
    fun theLobbysOwnWayInIsNotOfferedItAgain() = runBlocking {
        val app = fakeApp(signedIn = true)
        app.open(invitation)
        app.backToLobby()
        assertFalse(app.arrivedByLink)
        app.prepare("design-review")
        assertEquals(AppModel.Screen.Prejoin, app.screen)
        assertFalse(app.offersRelayChoiceAtTheDoor)
    }

    @Test
    fun leavingTheWaitForgetsTheLink() = runBlocking {
        val app = fakeApp(signedIn = true)
        app.open(meeting)
        app.stopWaiting()
        assertFalse(app.arrivedByLink)
        assertFalse(app.offersRelayChoiceAtTheDoor)
    }

    @Test
    fun onlyWhereThereIsAChoice() = runBlocking {
        val app = fakeApp(signedIn = true)
        app.stage("relaysState", relays.copy(measure = false))
        app.open(invitation)
        assertTrue(app.arrivedByLink)
        assertFalse(app.offersRelayChoiceAtTheDoor)
        app.stage("relaysState", relays.copy(relays = relays.relays.take(1)))
        assertFalse(app.offersRelayChoiceAtTheDoor)
    }
}
