package app.tomoshibi.ui.lobby

import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.expandVertically
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.shrinkVertically
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.consumeWindowInsets
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.dropShadow
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.shadow.Shadow
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.DpOffset
import androidx.compose.ui.unit.dp
import app.tomoshibi.basics.Localization
import app.tomoshibi.basics.Theme
import app.tomoshibi.basics.t
import app.tomoshibi.model.AppModel
import app.tomoshibi.model.LobbyNotice
import app.tomoshibi.model.Remote
import app.tomoshibi.ui.kit.ContinuousShape
import app.tomoshibi.ui.kit.Font
import app.tomoshibi.ui.kit.KitText
import app.tomoshibi.ui.kit.RoundButton
import app.tomoshibi.ui.kit.Symbol

/**
 * The deployment's notice (Remote `notice`) at the top of the lobby
 * (LobbyNoticeCard.swift): what it says, in the reader's language, and a
 * close that puts this one away for good, until the deployment puts up one
 * with another id. Shown only while the lobby is the screen, so never over a
 * call. It arrives opening a room for itself, fading in as it does, and
 * leaves the same way, closing the room up behind it.
 *
 * Drawn as the update's card is (UpdateBanner `Card`), being content the
 * person reads rather than a control over a picture: the surface colour with
 * the theme's edge, a shadow where it lies over the picture of yourself, and
 * where the deployment calls it a warning the red edge and mark the app draws
 * a problem with.
 *
 * [insets] are kept clear of inside it, and [padding] around the card, only
 * while it is there: with no notice it takes no room at all.
 */
@Composable
fun LobbyNoticeCard(
    app: AppModel,
    modifier: Modifier = Modifier,
    insets: WindowInsets = WindowInsets(0),
    padding: PaddingValues = PaddingValues(0.dp),
    floating: Boolean = false,
) {
    val remote = app.remote
    val notice = remote.notice
    // Read here, so that a change of language draws the notice again in the
    // new one: its words are the deployment's, and do not go through t().
    val words = notice?.text?.get(Localization.shared.language)
    // What it last said, kept while it leaves.
    var said by remember { mutableStateOf<Pair<LobbyNotice, String>?>(null) }
    if (notice != null && words != null) said = notice to words
    AnimatedVisibility(
        notice != null && words != null,
        modifier,
        enter = expandVertically(Theme.settle(), expandFrom = Alignment.Top) + fadeIn(Theme.settle()),
        exit = shrinkVertically(Theme.leave(), shrinkTowards = Alignment.Top) + fadeOut(Theme.leave()),
    ) {
        Box(Modifier.fillMaxWidth().windowInsetsPadding(insets).padding(padding), contentAlignment = Alignment.TopCenter) {
            said?.let { (shown, text) -> Card(remote, shown, text, floating) }
        }
    }
}

/** Whether the notice is up for this reader: there is one, and it says something in their language ([LobbyNoticeCard]). */
@Composable
internal fun noticeShown(app: AppModel): Boolean = app.remote.notice?.text?.get(Localization.shared.language) != null

@Composable
private fun Card(remote: Remote, notice: LobbyNotice, text: String, floating: Boolean) {
    val shape = ContinuousShape(20.dp)
    val tint = if (notice.warning) Theme.danger else Theme.tally
    Row(
        Modifier
            .widthIn(max = 560.dp)
            .fillMaxWidth()
            .then(if (floating) Modifier.dropShadow(shape, Shadow(radius = 18.dp, color = Color.Black, offset = DpOffset(0.dp, 8.dp), alpha = 0.35f)) else Modifier)
            .background(Theme.surface, shape)
            .border(1.dp, if (notice.warning) Theme.danger.copy(alpha = 0.45f) else Theme.border, shape)
            .padding(start = 14.dp, end = 10.dp, top = 12.dp, bottom = 12.dp)
            .testTag("lobby.notice"),
        horizontalArrangement = Arrangement.spacedBy(12.dp),
        verticalAlignment = Alignment.Top,
    ) {
        Box(Modifier.size(30.dp).background(tint.copy(alpha = 0.18f), CircleShape), contentAlignment = Alignment.Center) {
            Symbol(if (notice.warning) "exclamationmark.circle.fill" else "megaphone.fill", size = 13f, weight = Font.Semibold, tint = tint)
        }
        // New words for the same notice fade over the old rather than cut.
        AnimatedContent(
            text,
            Modifier.weight(1f).heightIn(min = 30.dp),
            transitionSpec = { fadeIn(Theme.settle()) togetherWith fadeOut(Theme.leave()) },
            contentAlignment = Alignment.CenterStart,
            label = "notice",
        ) { now ->
            Box(Modifier.heightIn(min = 30.dp), contentAlignment = Alignment.CenterStart) {
                KitText(now, font = Font(13.5f), lines = 12)
            }
        }
        RoundButton("xmark", t("Dismiss"), { remote.close(notice) }, Modifier.testTag("lobby.notice.close"))
    }
}

/**
 * [content] under the lobby's notice, in a wide lobby's section: moved down
 * by as much of the notice as is showing, rather than lain over by it, and
 * the window's top inset left to the notice wherever it covers it, so that
 * the section does not keep clear of the status bar a second time below it.
 * With no notice, [content] is where it always was.
 */
@Composable
internal fun UnderLobbyNotice(app: AppModel, insets: WindowInsets, modifier: Modifier = Modifier, content: @Composable BoxScope.() -> Unit) {
    val density = LocalDensity.current
    var tall by remember { mutableIntStateOf(0) }
    Column(modifier) {
        // Measured from a box that is always there: the notice's own room is
        // taken away once it has left, and would leave its last height behind.
        Box(Modifier.fillMaxWidth().onSizeChanged { tall = it.height }) {
            LobbyNoticeCard(app, Modifier.fillMaxWidth(), insets = insets, padding = PaddingValues(start = 16.dp, end = 16.dp, top = 12.dp), floating = true)
        }
        Box(
            Modifier
                .weight(1f)
                .fillMaxWidth()
                .consumeWindowInsets(PaddingValues(top = with(density) { tall.toDp() })),
            content = content,
        )
    }
}
