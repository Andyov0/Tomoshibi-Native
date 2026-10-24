package app.tomoshibi.ui.call

import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.AnimatedVisibilityScope
import androidx.compose.animation.EnterTransition
import androidx.compose.animation.ExitTransition
import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.MutableTransitionState
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.expandHorizontally
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.scaleIn
import androidx.compose.animation.scaleOut
import androidx.compose.animation.shrinkHorizontally
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.TransformOrigin
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.unit.dp
import app.tomoshibi.basics.Theme
import app.tomoshibi.basics.t
import app.tomoshibi.feature.music.SongDesk
import app.tomoshibi.feature.watch.WatchEngine
import app.tomoshibi.model.CallModel
import app.tomoshibi.ui.kit.ContinuousShape
import app.tomoshibi.ui.kit.Font
import app.tomoshibi.ui.kit.KitText
import app.tomoshibi.ui.kit.Symbol
import app.tomoshibi.ui.kit.glassSurface
import app.tomoshibi.ui.room.Avatars
import app.tomoshibi.ui.room.steady
import uniffi.tomoshibi_protocol.MediaSource

/** The bubble's card, the same at every screen size: it is meant to be a thumb's width, not a share of the screen. */
internal val BubbleWidth = 72.dp

/** What the bubble shows beside the call, when there is something to go back for. */
internal enum class BubbleNews { None, Sharing, Watching }

/** Something to see has started in the call: a screen shared, or a video put up for everybody. */
internal fun bubbleNews(call: CallModel): Pair<BubbleNews, String?> {
    val sharer = call.people.firstOrNull { it.track(MediaSource.SCREEN_SHARE) != null }
    return when {
        sharer != null -> BubbleNews.Sharing to sharer.name
        WatchEngine.of(call).onStage -> BubbleNews.Watching to null
        else -> BubbleNews.None to null
    }
}

/**
 * The call while the app is behind others and there is no picture to show
 * (media/CallBubble.kt): a card at the edge of the screen with the person
 * talking, the tally around them while they talk, the microphone's state, and
 * the song where one is playing and nobody is talking, as the small window's
 * voice face says them (RoomView `SmallVoice`). A screen shared or a video
 * put up meanwhile widens it to say so, since the system's small window,
 * which would show it, cannot be opened from behind: a tap brings the app
 * back, and leaving it again then opens the small window with the picture.
 *
 * [shown] is the bubble's arrival and departure; [pressed] shrinks it a
 * little under a finger, as the kit's buttons answer one; [docked] is the
 * edge it sits against, which it grows away from and shrinks toward.
 */
@Composable
internal fun CallBubble(call: CallModel, shown: MutableTransitionState<Boolean>, pressed: Boolean, docked: Docked) {
    val origin = TransformOrigin(if (docked == Docked.Start) 0f else 1f, 0.5f)
    AnimatedVisibility(
        shown,
        enter = fadeIn(Theme.settle()) + scaleIn(Theme.settle(), initialScale = 0.6f, transformOrigin = origin),
        exit = fadeOut(Theme.leave()) + scaleOut(Theme.leave(), targetScale = 0.8f, transformOrigin = origin),
    ) {
        val squeeze by animateFloatAsState(if (pressed) 0.94f else 1f, Theme.settle(), label = "press")
        // Room for the glass's shadow, which the window would otherwise cut
        // off at its edge.
        Box(Modifier.padding(10.dp).graphicsLayer {
            scaleX = squeeze
            scaleY = squeeze
            transformOrigin = origin
        }) {
            Card(call, docked)
        }
    }
}

/** Which edge of the screen the bubble sits against. */
internal enum class Docked { Start, End }

@Composable
private fun Card(call: CallModel, docked: Docked) {
    val (news, who) = bubbleNews(call)
    Row(
        Modifier
            .glassSurface(ContinuousShape(20.dp))
            // Over another app's screen, where no blur can reach (a window of
            // its own blurs the whole screen behind it or nothing): the glass's
            // stand-in alone let a home screen's icons show through the words.
            .background(Theme.surface.copy(alpha = 0.7f))
            .clearAndSetSemantics {},
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (docked == Docked.End) News(news, who, docked)
        Voice(call)
        if (docked == Docked.Start) News(news, who, docked)
    }
}

/** The person and the microphone: the bubble's own face. */
@Composable
private fun Voice(call: CallModel) {
    val talking = call.people.firstOrNull { it.speaking }
    val person = talking ?: call.people.firstOrNull()
    val name = person?.name ?: call.localName
    val identity = person?.id ?: call.localIdentity
    val speaking = steady(talking != null)
    val song = SongDesk.of(call).state?.now?.title?.takeIf { talking == null }
    val ring by animateColorAsState(if (speaking) Theme.tally else Color.Transparent, Theme.settle(), label = "tally")
    Column(
        Modifier.width(BubbleWidth).padding(top = 10.dp, bottom = 9.dp, start = 6.dp, end = 6.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        Box(Modifier.size(46.dp), contentAlignment = Alignment.Center) {
            Box(Modifier.size(46.dp).border(2.dp, ring, CircleShape))
            AnimatedContent(
                identity to name,
                transitionSpec = { fadeIn(Theme.settle()) togetherWith fadeOut(Theme.leave()) },
                label = "who",
            ) { (shownIdentity, shownName) ->
                Box(Modifier.size(38.dp).background(Avatars.colour(shownIdentity), CircleShape), contentAlignment = Alignment.Center) {
                    KitText(Avatars.initials(shownName), font = Font(14f, Font.Semibold), lines = 1)
                }
            }
            Appearing(
                !call.microphoneOn,
                Modifier.align(Alignment.BottomEnd).offset(x = 2.dp, y = 2.dp),
            ) {
                Box(Modifier.size(18.dp).background(Theme.surfaceHigh, CircleShape), contentAlignment = Alignment.Center) {
                    Symbol("mic.slash.fill", size = 9.5f, weight = Font.Semibold, tint = Theme.foreground)
                }
            }
        }
        AnimatedContent(
            song?.let { "song $it" } ?: "name $name",
            transitionSpec = { fadeIn(Theme.settle()) togetherWith fadeOut(Theme.leave()) },
            label = "label",
        ) { label ->
            if (label.startsWith("song ")) {
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(3.dp)) {
                    Symbol("music.note", size = 9.5f, tint = Theme.muted)
                    KitText(label.removePrefix("song "), font = Font(10.5f), color = Theme.muted, lines = 1)
                }
            } else {
                KitText(label.removePrefix("name "), font = Font(11f, Font.Semibold), lines = 1)
            }
        }
    }
}

/** A screen shared or a video put up, beside the face, on the side away from the edge. */
@Composable
private fun News(news: BubbleNews, who: String?, docked: Docked) {
    val from = if (docked == Docked.Start) Alignment.Start else Alignment.End
    AnimatedVisibility(
        news != BubbleNews.None,
        enter = fadeIn(Theme.settle()) + expandHorizontally(Theme.settle(), expandFrom = from),
        exit = fadeOut(Theme.leave()) + shrinkHorizontally(Theme.leave(), shrinkTowards = from),
    ) {
        Row(
            Modifier.widthIn(max = 168.dp).padding(start = if (docked == Docked.Start) 0.dp else 12.dp, end = if (docked == Docked.Start) 12.dp else 0.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Symbol(if (news == BubbleNews.Watching) "play.rectangle.fill" else "rectangle.on.rectangle", size = 15f, tint = Theme.tally)
            Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
                KitText(
                    if (news == BubbleNews.Watching) t("Watching together") else t("{name} is sharing", mapOf("name" to (who ?: ""))),
                    font = Font(12f, Font.Semibold), lines = 2,
                )
                KitText(t("Tap to watch"), font = Font(11f), color = Theme.muted, lines = 1)
            }
        }
    }
}

/** A mark that fades and grows in and out rather than popping, in a box. */
@Composable
private fun Appearing(visible: Boolean, modifier: Modifier, content: @Composable AnimatedVisibilityScope.() -> Unit) {
    val enter: EnterTransition = fadeIn(Theme.settle()) + scaleIn(Theme.settle(), initialScale = 0.5f)
    val exit: ExitTransition = fadeOut(Theme.leave()) + scaleOut(Theme.leave(), targetScale = 0.5f)
    AnimatedVisibility(visible, modifier, enter = enter, exit = exit, content = content)
}
