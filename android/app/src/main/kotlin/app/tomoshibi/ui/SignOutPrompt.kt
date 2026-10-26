package app.tomoshibi.ui

import android.os.Build
import androidx.activity.compose.BackHandler
import androidx.compose.animation.core.MutableTransitionState
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.rememberTransition
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.focusable
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.State
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.draw.dropShadow
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.graphics.BlurEffect
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.TileMode
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.shadow.Shadow
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalLayoutDirection
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.DpOffset
import androidx.compose.ui.unit.dp
import app.tomoshibi.basics.Theme
import app.tomoshibi.basics.t
import app.tomoshibi.model.AppModel
import app.tomoshibi.ui.kit.ContentLayer
import app.tomoshibi.ui.kit.ContinuousShape
import app.tomoshibi.ui.kit.Font
import app.tomoshibi.ui.kit.GlassButton
import app.tomoshibi.ui.kit.GlassButtonShape
import app.tomoshibi.ui.kit.KitText
import app.tomoshibi.ui.kit.Symbol
import app.tomoshibi.ui.kit.glassLayer
import kotlinx.coroutines.launch

/**
 * Asked before signing out (SignOutPrompt.swift, the Windows app's
 * SignOutPrompt.cs, in the same words): Sign out signed out on the press,
 * and a press meant for the item beside it did it too (2026-10-22). The
 * window behind goes to frosted glass, a little dimmed, so that the question
 * is the one thing in it; the card grows in from 96 per cent as the dialogs
 * do, and goes on either answer. A press on the glass, Back and Escape are
 * Cancel. Either place Sign out is pressed in, the account menu or the
 * account's settings, asks through [AppModel.signOutAsked].
 *
 * Around everything the app draws, in its own window, rather than in a
 * dialog of the system's: the frost is everything under it blurred as one
 * RenderEffect, which a window of its own could not reach, and it moves
 * with the app's two curves like the sheets (Sheets.kt). Over every screen
 * and every sheet, the settings' included; the popovers, in windows of their
 * own, cannot be opened under it, since the glass takes every press, and
 * the account menu's has closed on its way out as the question arrives.
 *
 * Android 12 is the first that blurs (RenderEffect); before it the window is
 * dimmed further instead, the nearest the system comes.
 */
@Composable
internal fun SignOutPrompt(app: AppModel, content: @Composable () -> Unit) {
    val scope = LocalAppScope.current
    val asked = app.signOutAsked
    // Kept until its departure has played, as every popover is.
    val shown = remember { MutableTransitionState(false) }
    shown.targetState = asked
    val transition = rememberTransition(shown, label = "sign out")
    val progress = transition.animateFloat(
        transitionSpec = { if (targetState) Theme.settle() else Theme.leave() },
        label = "sign out",
    ) { if (it) 1f else 0f }
    val frost = with(LocalDensity.current) { Frost.toPx() }

    fun answer(signOut: Boolean) {
        // Once: the card is still there, and pressable, while it leaves.
        if (!app.signOutAsked) return
        app.signOutAsked = false
        if (signOut) scope.launch { app.signOut() }
    }

    Box(Modifier.fillMaxSize()) {
        Box(
            Modifier
                .fillMaxSize()
                // Read as the layer is drawn, so the blur deepens frame by
                // frame without anything under it composed or measured again.
                .graphicsLayer {
                    val radius = progress.value * frost
                    renderEffect = if (Frosts && radius >= 0.5f) BlurEffect(radius, radius, TileMode.Clamp) else null
                }
                // Behind the question, nothing to read out or to reach.
                .then(if (asked) Modifier.clearAndSetSemantics {} else Modifier),
        ) { content() }
        if (shown.currentState || shown.targetState || !shown.isIdle) {
            Question(progress, arriving = transition.targetState, asked = asked, answer = ::answer)
        }
    }
}

@Composable
private fun Question(progress: State<Float>, arriving: Boolean, asked: Boolean, answer: (Boolean) -> Unit) {
    // Composed with the question rather than standing by, so that it is the
    // last Back registered, and so the first asked: a sheet's own, standing
    // by since the sheet opened, would otherwise close the settings under it.
    if (asked) BackHandler { answer(false) }
    val focus = remember { FocusRequester() }
    LaunchedEffect(asked) { if (asked) runCatching { focus.requestFocus() } }
    val dimming = veil
    Box(
        Modifier
            .fillMaxSize()
            .drawBehind { drawRect(dimming, alpha = progress.value) }
            .pointerInput(Unit) { detectTapGestures { answer(false) } }
            .testTag("signout.glass"),
    )
    val density = LocalDensity.current
    val direction = LocalLayoutDirection.current
    val hinge = LocalHinge.current
    BoxWithConstraints(Modifier.fillMaxSize()) {
        // On the reading page of a book, as the lobby's dialogs are, rather
        // than across the fold.
        val area = hinge.pane(maxWidth.value, maxHeight.value, density, direction, Pane.Reading).settled("sign out")
        Box(
            Modifier
                .inPane(area, maxWidth.value, maxHeight.value)
                .windowInsetsPadding(WindowInsets.safeDrawing)
                .padding(horizontal = 24.dp),
            contentAlignment = Alignment.Center,
        ) {
            val shape = ContinuousShape(24.dp)
            Column(
                Modifier
                    .widthIn(max = 340.dp)
                    .fillMaxWidth()
                    // Faded on each thing drawn, so its shadow is not cut
                    // off square (Glass.kt `glassLayer`); grown from 96 per
                    // cent on the way in, to 98 on the way out.
                    .glassLayer(
                        alpha = { progress.value },
                        scale = {
                            val from = if (arriving) 0.96f else 0.98f
                            from + (1f - from) * progress.value
                        },
                    )
                    .dropShadow(shape, cardShadow(Theme.shade(0.4f)))
                    .clip(shape)
                    .background(Theme.surface)
                    .border(1.dp, Theme.border, shape)
                    // A press on the card is the card's, never the glass's.
                    .pointerInput(Unit) { detectTapGestures { } }
                    .focusRequester(focus)
                    .focusable()
                    .onKeyEvent { event ->
                        if (event.key == Key.Escape && event.type == KeyEventType.KeyUp) {
                            answer(false)
                            true
                        } else {
                            false
                        }
                    }
                    .testTag("signout.prompt")
                    .padding(22.dp),
                horizontalAlignment = Alignment.CenterHorizontally,
                verticalArrangement = Arrangement.spacedBy(14.dp),
            ) {
                Box(Modifier.size(46.dp).background(Theme.danger.copy(alpha = 0.16f), CircleShape), contentAlignment = Alignment.Center) {
                    Symbol("rectangle.portrait.and.arrow.right", size = 17f, weight = Font.Semibold, tint = Theme.dangerInk)
                }
                Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(6.dp)) {
                    KitText(t("Sign out of Tomoshibi?"), font = Font(17f, Font.Semibold), color = Theme.foreground, align = TextAlign.Center)
                    KitText(
                        t("To come back, you will sign in again with your name and passphrase."),
                        font = Font(13f), color = Theme.muted, align = TextAlign.Center,
                    )
                }
                // The card is a plain surface, and its buttons are drawn
                // plain on it, as the Swift's pills are.
                ContentLayer {
                    Row(Modifier.fillMaxWidth().padding(top = 4.dp), horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                        GlassButton(
                            { answer(false) },
                            Modifier.weight(1f).testTag("signout.cancel"),
                            shape = GlassButtonShape.Capsule(36.dp),
                        ) { KitText(t("Cancel"), lines = 1) }
                        GlassButton(
                            { answer(true) },
                            Modifier.weight(1f).testTag("signout.confirm"),
                            shape = GlassButtonShape.Capsule(36.dp),
                            destructive = true,
                        ) { KitText(t("Sign out"), lines = 1) }
                    }
                }
            }
        }
    }
}

/** Whether this system can frost what is behind the question (RenderEffect, Android 12). */
private val Frosts: Boolean get() = Build.VERSION.SDK_INT >= Build.VERSION_CODES.S

/** How deep the frost goes, about the depth of the system's thinnest material. */
private val Frost = 24.dp

/**
 * Over the frost, the Swift's black at 28 per cent; with no frost, darker,
 * so that the window behind still reads as set aside. Half as dark in the
 * light (Palette.shade).
 */
private val veil: Color @Composable get() = Color.Black.copy(alpha = Theme.shade(if (Frosts) 0.28f else 0.5f))

/** SignOutPrompt.swift's: 40 per cent black, 28 soft, 12 down. */
private fun cardShadow(alpha: Float) = Shadow(radius = 28.dp, color = Color.Black, offset = DpOffset(0.dp, 12.dp), alpha = alpha)
