package app.tomoshibi.ui

import android.app.Activity
import androidx.activity.compose.BackHandler
import androidx.activity.compose.LocalActivity
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.SizeTransform
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.runtime.compositionLocalOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.window.layout.FoldingFeature
import androidx.window.layout.WindowInfoTracker
import app.tomoshibi.basics.Theme
import app.tomoshibi.model.AppModel
import app.tomoshibi.model.AppModel.Screen
import app.tomoshibi.model.CallModel
import app.tomoshibi.ui.kit.GlassBackdrop
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
 * A foldable's hinge where it divides the window: half open, as a book or
 * on a table. In the window's own pixels, which are the screen's own pixels
 * here, since the app draws from edge to edge. Null where there is none, or
 * where the screen is flat and the hinge divides nothing (an unfolded Fold
 * lying flat reports its fold as not separating).
 */
@Immutable
data class Hinge(val left: Float, val top: Float, val right: Float, val bottom: Float, val vertical: Boolean)

val LocalHinge = compositionLocalOf<Hinge?> { null }

/**
 * The whole app (Screens.swift `AppRoot`): one screen at a time, each
 * arriving and leaving with the same two curves, over black.
 */
@Composable
fun TomoshibiApp(app: AppModel, modifier: Modifier = Modifier) {
    val scope = rememberCoroutineScope()
    val activity = LocalActivity.current
    val hinge = activity?.let { rememberHinge(it) }
    CompositionLocalProvider(LocalAppScope provides scope, LocalHinge provides hinge) {
        FeedSelfViews(app)
        Box(modifier.fillMaxSize().background(Color.Black)) {
            // Back in a call does not leave it: the room's own handlers,
            // registered inside it and so after this one, close what it has
            // open. With nothing open the app goes behind the others as Home
            // takes it, the call still in it, rather than the activity
            // finishing, which leaves the call (MainActivity `onDestroy`).
            BackHandler(enabled = app.call != null) { activity?.moveTaskToBack(true) }
            // The screens that have a way back say it as their Back button
            // does (Screens.swift `BackButton`).
            BackHandler(enabled = backFor(app) != null) { backFor(app)?.invoke() }
            AnimatedContent(
                targetState = shown(app),
                contentKey = { it.key },
                transitionSpec = { (Theme.arrive togetherWith Theme.depart).using(SizeTransform(clip = false)) },
                label = "screen",
            ) { now ->
                GlassBackdrop {
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
            }
        }
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

/**
 * Where the hinge is while it divides the window, from the window library's
 * folding feature: half open (a book held open, a phone on a table), the
 * two halves are two screens to lay out for, and a sheet or a column of the
 * wide lobby lying across the fold is read bent.
 */
@Composable
private fun rememberHinge(activity: Activity): Hinge? {
    val flow = remember(activity) { WindowInfoTracker.getOrCreate(activity).windowLayoutInfo(activity) }
    val layout by flow.collectAsStateWithLifecycle(initialValue = null)
    val fold = layout?.displayFeatures?.filterIsInstance<FoldingFeature>()?.firstOrNull { it.isSeparating } ?: return null
    val bounds = fold.bounds
    return Hinge(
        bounds.left.toFloat(), bounds.top.toFloat(), bounds.right.toFloat(), bounds.bottom.toFloat(),
        vertical = fold.orientation == FoldingFeature.Orientation.VERTICAL,
    )
}
