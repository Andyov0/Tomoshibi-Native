package app.tomoshibi.ui.room

import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.expandVertically
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.shrinkVertically
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.slideOutVertically
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.unit.dp
import app.tomoshibi.basics.Theme
import app.tomoshibi.basics.t
import app.tomoshibi.model.CallModel
import app.tomoshibi.model.Person
import app.tomoshibi.model.host.CrateRules
import app.tomoshibi.ui.kit.Capsule
import app.tomoshibi.ui.kit.Font
import app.tomoshibi.ui.kit.KitText
import app.tomoshibi.ui.kit.Symbol
import app.tomoshibi.ui.kit.glassSurface
import app.tomoshibi.ui.kit.keepDeparting
import app.tomoshibi.ui.kit.rounded
import uniffi.tomoshibi_protocol.RosterMember

/**
 * Who has this participant's shared screen on their stage, from everybody's
 * `watching` attribute (components/room/Watchers.tsx, live/watching.ts;
 * WatchersCard.swift).
 *
 * Somebody sharing is doing it blind: they can see that eleven people are in
 * the call and not whether any of them has the picture in front of them. The
 * list is what people say rather than what the server knows, since the media
 * server tells a publisher nothing about who subscribed.
 *
 * Two sizes: the card, with names, where there is room beside whatever else
 * is open; the eye and the count alone where there is not, so that opening a
 * panel on a phone moves it rather than hiding it.
 */
@Composable
internal fun WatchersCard(call: CallModel, modifier: Modifier = Modifier, compact: Boolean = false) {
    val watching = watching(call)
    if (compact) {
        Row(
            modifier
                .height(28.dp)
                .glassSurface(Capsule)
                .padding(horizontal = 10.dp)
                .clearAndSetSemantics {
                    contentDescription = t("Watching")
                    stateDescription = watching.size.toString()
                },
            horizontalArrangement = Arrangement.spacedBy(5.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            val ink = if (watching.isEmpty()) Theme.muted else Theme.foreground
            Symbol("eye.fill", size = 10.5f, weight = Font.Semibold, tint = ink)
            Figure(watching.size, Font(12f, Font.Semibold, digits = true), ink)
        }
    } else {
        Column(
            modifier
                .width(172.dp)
                .glassSurface(rounded(16f))
                .padding(horizontal = 12.dp, vertical = 10.dp),
            verticalArrangement = Arrangement.spacedBy(6.dp),
        ) {
            Row(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalAlignment = Alignment.CenterVertically) {
                Symbol("eye.fill", size = 10f, weight = Font.Semibold, tint = Theme.muted)
                KitText(t("Watching"), font = Font(11f, Font.Semibold), color = Theme.muted, lines = 1)
                Spacer(Modifier.weight(1f))
                Figure(watching.size, Font(11f, Font.Semibold, digits = true), Theme.muted)
            }
            AnimatedContent(watching.isEmpty(), transitionSpec = { fadeIn(Theme.settle()) togetherWith fadeOut(Theme.leave()) }, label = "watchers") { nobody ->
                if (nobody) {
                    // Said rather than left blank: an empty box beside a share
                    // reads as something that has not loaded, and the useful
                    // reading is that nobody has opened it.
                    KitText(t("Nobody has this open yet."), font = Font(11.5f), color = Theme.muted.copy(alpha = Theme.muted.alpha * 0.85f))
                } else {
                    Column(verticalArrangement = Arrangement.spacedBy(3.dp)) {
                        for (entry in keepDeparting(watching) { it.id }) {
                            val person = entry.item
                            AnimatedVisibility(entry.visible, enter = Theme.arrive + expandVertically(Theme.settle()), exit = Theme.depart + shrinkVertically(Theme.leave())) {
                                Row(horizontalArrangement = Arrangement.spacedBy(7.dp), verticalAlignment = Alignment.CenterVertically) {
                                    Box(Modifier.size(5.dp).background(Theme.tally, CircleShape))
                                    KitText(person.name.ifEmpty { person.id }, font = Font(12.5f), lines = 1)
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/** A count that rolls to the next figure as it changes, as SwiftUI's numeric text does. */
@Composable
private fun Figure(count: Int, font: Font, ink: androidx.compose.ui.graphics.Color) {
    AnimatedContent(
        count,
        transitionSpec = {
            val up = targetState > initialState
            (slideInVertically(Theme.settle()) { if (up) it else -it } + fadeIn(Theme.settle())) togetherWith
                (slideOutVertically(Theme.leave()) { if (up) -it else it } + fadeOut(Theme.leave()))
        },
        label = "count",
    ) { shown -> KitText(shown.toString(), font = font, color = ink, lines = 1) }
}

/**
 * Everybody whose `watching` names this participant, in roster order: the
 * protocol crate's `watchers`, which is watching.ts `useWatchers`.
 */
internal fun watching(call: CallModel): List<Person> {
    val roster = call.people.map { RosterMember(it.id, it.attributes) }
    val ids = CrateRules.watchers(call.localIdentity, roster).toSet()
    return call.people.filter { it.id in ids }
}
