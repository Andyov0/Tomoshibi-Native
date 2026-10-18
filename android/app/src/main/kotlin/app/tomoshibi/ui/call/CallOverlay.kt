package app.tomoshibi.ui.call

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.expandVertically
import androidx.compose.animation.shrinkVertically
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import app.tomoshibi.basics.Theme
import app.tomoshibi.basics.say
import app.tomoshibi.basics.t
import app.tomoshibi.model.host.HostFeature
import app.tomoshibi.ui.kit.Font
import app.tomoshibi.ui.kit.KitEnabled
import app.tomoshibi.ui.kit.KitText
import app.tomoshibi.ui.kit.MediaSpinner
import app.tomoshibi.ui.kit.Monogram
import app.tomoshibi.ui.kit.PillButton
import app.tomoshibi.ui.kit.PillRole
import app.tomoshibi.ui.kit.ToastAttachment
import app.tomoshibi.ui.kit.keepDeparting
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

/**
 * Where the host's feature runs from, and nothing it draws any more
 * (CallOverlay.swift `CallOverlayView`): what a press did and who is at the
 * door are said in the corner with everything else the call says, as the web
 * says them (notices.ts).
 *
 * The room places this once, so its coroutines are the feature's lifetime:
 * it starts when the call is on screen and stops when the call view goes. It
 * also hands the call what the door's notice draws under its words.
 */
@Composable
internal fun CallOverlay(feature: HostFeature) {
    val call = feature.call
    LaunchedEffect(feature) {
        // The door's notice carries the people outside with both answers, so
        // a knock is answered where it is read (DoorList).
        call.attach(HostFeature.DOOR, ToastAttachment { DoorList(feature) })
        feature.run()
    }
    // App.tsx: a hard limit on how long the move is shown. The rejoin takes
    // two or three seconds; a move that never ends the call (the
    // announcement went out and the close did not) would otherwise leave
    // "you will be back in a moment" over the controls for good.
    val moving = call.moving
    LaunchedEffect(moving?.atNanos) {
        val at = moving?.atNanos ?: return@LaunchedEffect
        val left = MOVE_SHOWN_NANOS - (System.nanoTime() - at)
        if (left > 0) delay(left / 1_000_000)
        feature.giveUpMoving(at)
    }
}

/** App.tsx: the overlay's twenty seconds. */
private const val MOVE_SHOWN_NANOS = 20_000_000_000L

/**
 * Who is at the door, each with the two answers (AtTheDoor.tsx;
 * CallOverlay.swift `DoorList`): what they typed to be called, which is a
 * claim, and where they knocked from, which is not. Drawn in the door's
 * notice and inside the host panel alike.
 */
@Composable
internal fun DoorList(feature: HostFeature, modifier: Modifier = Modifier) {
    Column(modifier, verticalArrangement = Arrangement.spacedBy(8.dp)) {
        for (entry in keepDeparting(feature.waiting) { it.id }) {
            val one = entry.item
            AnimatedVisibility(
                entry.visible,
                enter = Theme.arrive + expandVertically(Theme.settle()),
                exit = Theme.depart + shrinkVertically(Theme.leave()),
            ) {
                KitEnabled(feature.answering != one.id) {
                    Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(9.dp), verticalAlignment = Alignment.CenterVertically) {
                        Monogram(one.name.ifEmpty { "?" }, one.id, size = 26.dp)
                        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(1.dp)) {
                            KitText(one.name.ifEmpty { t("Somebody") }, font = Font(12.5f, Font.Medium), lines = 1)
                            KitText(one.address, font = Font(10.5f, mono = true), color = Theme.muted, lines = 1, overflow = TextOverflow.MiddleEllipsis)
                        }
                        PillButton(t("Not now"), { feature.call.scope.launch { feature.answer(one, admit = false) } })
                        PillButton(t("Let in"), { feature.call.scope.launch { feature.answer(one, admit = true) } },
                            role = PillRole.Prominent, busy = feature.answering == one.id)
                    }
                }
            }
        }
    }
}

/**
 * A call being moved to another machine, from a chair
 * (components/room/Moving.tsx; `MovingOverlay`).
 *
 * A move ends the call and everybody comes straight back; what that looked
 * like was being thrown out. So this is drawn over the call for the few
 * seconds it takes, says where the meeting is going, and offers nothing to
 * press: it takes every press, so that the bar under it cannot be pressed
 * through "you will be back in a moment". The destination is what the server
 * announced, put through `say()` as the web puts it.
 */
@Composable
fun MovingOverlay(to: String, modifier: Modifier = Modifier) {
    val named = say(to.trim())
    Box(
        modifier
            .fillMaxSize()
            .background(Theme.background.copy(alpha = 0.86f))
            .pointerInputBlocking()
            .semantics(mergeDescendants = true) { liveRegion = LiveRegionMode.Polite },
        contentAlignment = Alignment.Center,
    ) {
        Column(
            Modifier.padding(horizontal = 24.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            MediaSpinner(size = 18f, tint = Theme.muted)
            KitText(
                if (named.isEmpty()) t("Moving this meeting") else t("Moving this meeting to {name}", mapOf("name" to named)),
                font = Font(14f, Font.Medium), align = TextAlign.Center,
            )
            // The one thing worth saying: nobody has to do anything.
            KitText(t("You will be back in a moment."), font = Font(12f), color = Theme.muted, align = TextAlign.Center)
        }
    }
}

/** Takes every press made on it, so nothing beneath can be pressed. */
private fun Modifier.pointerInputBlocking(): Modifier = pointerInput(Unit) {
    awaitPointerEventScope {
        while (true) awaitPointerEvent().changes.forEach { it.consume() }
    }
}
