package app.tomoshibi.model

import kotlinx.coroutines.Dispatchers
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/*
 * The app's model where it needs no deployment: what it keeps and where, the
 * sentence that stays and the one that fades, and the rule that a room is
 * reached by name only by somebody the deployment knows. What these guard is
 * a test's server or name ending up as the owner's, and a refusal read away
 * before it was read.
 */
class AppModelTest {
    private val defaults = MemoryDefaults()
    private val secrets = MemorySecrets()

    private fun model(launch: DebugLaunch? = null) =
        AppModel(defaults, secrets, media = null, launch = launch, home = "https://example.test/", context = Dispatchers.Unconfined)

    @Test
    fun opensOnTheMarkAndKeepsWhatIsTypedUnderTheWebsNames() {
        val app = model()
        assertEquals(AppModel.Screen.Starting, app.screen)
        assertFalse(app.busy)
        assertFalse(app.rehearsal)
        app.name = "Ada"
        assertEquals("Ada", defaults.string("meet-live.name"))
        app.passphrase = "correct horse"
        assertEquals("correct horse", secrets.read("passphrase:https://example.test/"))
        app.relayChoice = "tokyo"
        assertEquals("tokyo", defaults.string("meet-live.relay"))
        app.startWithCamera = true
        assertEquals("""{"camera":true,"microphone":true}""", defaults.string("meet-live.devices"))
        // The next launch starts where this one was left.
        val next = model()
        assertEquals("Ada", next.name)
        assertEquals("tokyo", next.relayChoice)
        assertTrue(next.startWithCamera)
    }

    @Test
    fun theAppleAppsTypedServerIsClearedAndNeverRead() {
        defaults.set(AppModel.SERVER_KEY, "https://somebody-else.example/")
        model()
        assertNull(defaults.string(AppModel.SERVER_KEY))
    }

    @Test
    fun aRehearsalKeepsNothingAndReadsNothingOfTheOwners() {
        defaults.set("meet-live.name", "Owner")
        defaults.set("meet-live.relay", "home-relay")
        val app = model(DebugLaunch(base = "http://127.0.0.1:8080"))
        assertTrue(app.rehearsal)
        assertTrue(Rehearsal.running)
        assertNull(Rehearsal.choices)
        assertEquals("", app.name)
        assertNull(app.relayChoice)
        app.name = "Native App"
        app.passphrase = "test"
        app.startWithMicrophone = false
        assertEquals("Owner", defaults.string("meet-live.name"))
        assertNull(secrets.read("passphrase:https://example.test/"))
        assertNull(defaults.string("meet-live.devices"))
        assertFalse(app.deviceMemory.keeps)
    }

    @Test
    fun aRoomIsReachedByNameOnlyBySomebodyTheDeploymentKnows() {
        val app = model()
        app.prepare("team-room")
        assertEquals(AppModel.Screen.Starting, app.screen)
        assertEquals("Rooms here are by invitation. Open the link you were sent, or sign in.", app.problem)
        // Why they are not in a meeting stays until it is closed.
        assertTrue(app.problemStays)
        assertEquals("", app.room)
    }

    @Test
    fun aRehearsalMayNameAnyRoomAndTheNameIsTidied() {
        val app = model(DebugLaunch(base = "http://127.0.0.1:8080"))
        app.prepare("  Team-Room \n")
        assertEquals(AppModel.Screen.Prejoin, app.screen)
        assertEquals("team-room", app.room)
        assertNull(app.problem)
    }

    @Test
    fun aSentenceSaidPlainlyFadesAndOneToldStays() {
        val app = model()
        app.tell("That meeting has ended.")
        assertTrue(app.problemStays)
        app.problem = "That could not be sent."
        assertFalse(app.problemStays)
        app.tell("That invitation is no longer good. Ask for another.")
        assertTrue(app.problemStays)
        app.problem = null
        assertFalse(app.problemStays)
    }

    @Test
    fun backToTheLobbyForgetsTheInvitationAndTheProblem() {
        val app = model()
        app.tell("That meeting has ended.")
        app.backToLobby()
        assertEquals(AppModel.Screen.Lobby, app.screen)
        assertNull(app.problem)
        assertNull(app.invite)
        assertNull(app.invitation)
        assertFalse(app.joining)
    }

    @Test
    fun withNobodySignedInNothingIsHeldAndTheTypedNameIsWorn() {
        val app = model()
        assertFalse(app.holds("watch"))
        app.name = "  Ada  "
        assertEquals("Ada", app.joiningName)
        assertFalse(app.arrivedInvited)
        assertNull(app.invitedName)
        assertFalse(app.offersRelayChoice)
    }

    @Test
    fun aRoomJoinedLatelyIsRememberedOnlyOutsideARehearsal() {
        val app = model()
        // Nothing reached yet, so there is no deployment to file it under.
        app.lobby.remember("team-room")
        assertEquals(emptyList<LobbyModel.Recent>(), app.lobby.recents)
        defaults.set(LobbyModel.RECENTS_KEY, """[{"room":"a","server":"https://example.test/","at":1000},{"room":"","server":"x","at":2}]""")
        val again = model()
        assertEquals(listOf("a"), again.lobby.recents.map { it.room })
    }
}
