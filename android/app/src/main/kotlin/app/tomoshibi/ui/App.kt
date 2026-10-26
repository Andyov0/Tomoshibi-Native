package app.tomoshibi.ui

import androidx.activity.compose.BackHandler
import androidx.activity.compose.LocalActivity
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.SizeTransform
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.background
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalView
import androidx.core.view.WindowCompat
import app.tomoshibi.basics.LocalPalette
import app.tomoshibi.basics.Palette
import app.tomoshibi.basics.Theme
import app.tomoshibi.model.AppModel
import app.tomoshibi.model.AppModel.Screen
import app.tomoshibi.model.CallModel
import app.tomoshibi.ui.kit.GlassBackdrop
import app.tomoshibi.ui.kit.KitPalette
import app.tomoshibi.ui.kit.LocalInk
import app.tomoshibi.ui.lobby.FeedSelfViews
import app.tomoshibi.ui.lobby.LobbyView
import app.tomoshibi.ui.room.RoomView
import app.tomoshibi.ui.screens.EndedScreen
import app.tomoshibi.ui.screens.KnockingScreen
import app.tomoshibi.ui.screens.MovingOverlay
import app.tomoshibi.ui.screens.PreJoinScreen
import app.tomoshibi.ui.screens.SignInView
import app.tomoshibi.ui.screens.StartingScreen
import app.tomoshibi.ui.screens.UnreachableScreen
import app.tomoshibi.ui.screens.WaitingScreen
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import uniffi.tomoshibi_protocol.Account

/**
 * Where what a press sets going runs: the app's own scope rather than the
 * screen's. A join changes the screen halfway through itself (AppModel
 * `join` puts the call up, then turns the devices on), and run in the join
 * screen's scope it was cancelled the moment that screen left, after the
 * room was entered and before the microphone came on.
 */
val LocalAppScope = staticCompositionLocalOf<CoroutineScope> { Detached }

/** The scope a screen drawn without the app around it (a test, a preview) works in. */
private val Detached = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)

/**
 * The whole app (Screens.swift `AppRoot`): one screen at a time, each
 * arriving and leaving with the same two curves, over the ground of the
 * screen that is up.
 *
 * Every screen is drawn in the system's appearance, followed as it changes
 * and turned over with Theme.settle rather than at a stroke, except the
 * call's, which is drawn in the meeting's (Theme.MEETING_STAYS_DARK): the
 * room, what it opens and its small window stay dark in the daylight, as
 * FaceTime and Meet keep theirs. Each screen keeps the palette of its own
 * key while it leaves, so a lobby fading out under the call that replaced it
 * does not go dark on its way out.
 */
@Composable
fun TomoshibiApp(app: AppModel, modifier: Modifier = Modifier) {
    val scope = rememberCoroutineScope()
    val activity = LocalActivity.current
    // The window's hinge, or, where the window reports none (a test, which
    // cannot fold), whatever was provided around the app.
    val hinge = activity?.let { rememberHinge(it) } ?: LocalHinge.current
    val system = rememberSystemPalette()
    val now = shown(app)
    // What is over every screen (the question before signing out) and the
    // window's own ground and bars are in the palette of the screen that is up.
    val around = paletteOf(now.key, system)
    SystemBars(around)
    CompositionLocalProvider(
        LocalAppScope provides scope,
        LocalHinge provides hinge,
        LocalPalette provides around,
        LocalInk provides around.foreground,
    ) {
        FeedSelfViews(app)
        Box(modifier.fillMaxSize().background(Theme.ground)) {
            // Back in a call does not leave it: the room's own handlers,
            // registered inside it and so after this one, close what it has
            // open. With nothing open the app goes behind the others as Home
            // takes it, the call still in it, rather than the activity
            // finishing, which leaves the call (MainActivity `onDestroy`).
            BackHandler(enabled = app.call != null) { activity?.moveTaskToBack(true) }
            // The screens that have a way back say it as their Back button
            // does (Screens.swift `BackButton`).
            BackHandler(enabled = backFor(app) != null) { backFor(app)?.invoke() }
            // The question before signing out, over whichever screen and
            // sheet it was asked from.
            SignOutPrompt(app) {
                AnimatedContent(
                    targetState = now,
                    contentKey = { it.key },
                    transitionSpec = { (Theme.arrive togetherWith Theme.depart).using(SizeTransform(clip = false)) },
                    label = "screen",
                ) { now ->
                    KitPalette(paletteOf(now.key, system)) { GlassBackdrop { ScreenFor(app, now) } }
                }
            }
        }
    }
}

/** One screen, drawn from what [shown] said of it. */
@Composable
private fun ScreenFor(app: AppModel, now: Shown) {
    when (now.key) {
        Key.Starting -> StartingScreen()
        Key.Unreachable -> UnreachableScreen(app)
        Key.SignIn -> SignInView(app)
        Key.Lobby -> now.account?.let { LobbyView(app, it) }
        Key.Prejoin -> PreJoinScreen(app)
        Key.Knocking -> KnockingScreen(app)
        Key.Waiting -> WaitingScreen(app)
        Key.Call -> now.call?.let { RoomView(app, it, Modifier.fillMaxSize()) }
        Key.Moving -> MovingOverlay(now.moving ?: "")
        Key.Ended -> EndedScreen(app)
    }
}

/**
 * The system's appearance as a palette, followed while the app runs: the
 * activity turns round for itself on a change of night mode (the manifest's
 * configChanges), so the configuration the composition reads is what says
 * the system turned, and the palette crosses from one to the other over
 * Theme.settle as everything else arrives.
 */
@Composable
private fun rememberSystemPalette(): Palette {
    val light = !isSystemInDarkTheme()
    val daylight by animateFloatAsState(if (light) 1f else 0f, Theme.settle(), label = "daylight")
    return Palette.between(daylight)
}

/** The palette a screen is drawn in: the meeting's for a call and a move between rooms, the system's for the rest. */
private fun paletteOf(key: Key, system: Palette): Palette =
    if (key == Key.Call || key == Key.Moving) Theme.meeting(system) else system

/**
 * The status bar's and the navigation bar's icons, dark over a light
 * screen and light over a dark one. The window is drawn from edge to edge
 * with both bars clear (MainActivity), so their icons are all there is of
 * them; light icons over the light lobby could not be seen.
 */
@Composable
private fun SystemBars(palette: Palette) {
    val view = LocalView.current
    val window = (LocalActivity.current)?.window ?: return
    val light = !palette.dark
    SideEffect {
        val bars = WindowCompat.getInsetsController(window, view)
        if (bars.isAppearanceLightStatusBars != light) bars.isAppearanceLightStatusBars = light
        if (bars.isAppearanceLightNavigationBars != light) bars.isAppearanceLightNavigationBars = light
    }
}

/** Which screen is up, and what the one leaving needs to go on being drawn while it leaves. */
private enum class Key { Starting, Unreachable, SignIn, Lobby, Prejoin, Knocking, Waiting, Call, Moving, Ended }

/**
 * The screen and what it shows. The account and the call ride along so
 * that a lobby or a room fading out after its account or call has gone is
 * still drawn as it was; only the key decides when one screen gives way to
 * another, so a new picture on the account does not cross-fade the lobby.
 */
private data class Shown(val key: Key, val account: Account? = null, val call: CallModel? = null, val moving: String? = null)

private fun shown(app: AppModel): Shown = when (app.screen) {
    Screen.Starting -> Shown(Key.Starting)
    Screen.Unreachable -> Shown(Key.Unreachable)
    // Signed in or not is the whole difference (Lobby.swift `LobbyView`).
    Screen.Lobby -> app.account?.let { Shown(Key.Lobby, account = it) } ?: Shown(Key.SignIn)
    Screen.Prejoin -> Shown(Key.Prejoin)
    Screen.Knocking -> Shown(Key.Knocking)
    Screen.Waiting -> Shown(Key.Waiting)
    Screen.Call -> {
        val call = app.call
        val moving = app.movingTo
        when {
            call != null -> Shown(Key.Call, call = call)
            moving != null -> Shown(Key.Moving, moving = moving)
            // Never nothing. A call screen with no call and no move in
            // progress was a black window after a rejoin that failed; the
            // join screen, with the reason on it, is where that person can do
            // something (Screens.swift).
            else -> Shown(Key.Prejoin)
        }
    }
    Screen.Ended -> Shown(Key.Ended)
}

/** What Back does on the screens before and after a call that have a way back; null where the system's is right. */
private fun backFor(app: AppModel): (() -> Unit)? = when (app.screen) {
    Screen.Prejoin -> app::backToLobby
    Screen.Knocking -> app::stopKnocking
    Screen.Waiting -> app::stopWaiting
    Screen.Ended -> app::backToLobby
    Screen.Call -> if (app.call == null && app.movingTo == null) app::backToLobby else null
    else -> null
}
