package app.tomoshibi.ui.watch

import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import app.tomoshibi.basics.Theme
import app.tomoshibi.basics.t
import app.tomoshibi.feature.watch.WatchEngine
import app.tomoshibi.feature.watch.WatchInfo
import app.tomoshibi.feature.watch.clockOf
import app.tomoshibi.model.AppModel
import app.tomoshibi.model.CallModel
import app.tomoshibi.ui.kit.Font
import app.tomoshibi.ui.kit.KitEnabled
import app.tomoshibi.ui.kit.KitText
import app.tomoshibi.ui.kit.MediaArtwork
import app.tomoshibi.ui.kit.MediaButton
import app.tomoshibi.ui.kit.MediaEmpty
import app.tomoshibi.ui.kit.MediaField
import app.tomoshibi.ui.kit.MediaIconButton
import app.tomoshibi.ui.kit.MediaPanelGlass
import app.tomoshibi.ui.kit.MediaPanelHeader
import app.tomoshibi.ui.kit.MediaRow
import app.tomoshibi.ui.kit.MediaRule
import app.tomoshibi.ui.kit.MediaSpinner
import app.tomoshibi.ui.kit.MenuHeading
import app.tomoshibi.ui.kit.SegmentedChoice
import app.tomoshibi.ui.kit.Symbol
import kotlinx.coroutines.launch
import uniffi.tomoshibi_protocol.WatchVideo

/** Which part of the watch panel is showing. */
internal enum class WatchTab { Queue, Media }

/**
 * About as tall as the web's panel (`h-[min(34rem,...)]`), as the music
 * panel is (MusicPanelView.swift `height`): the room lets a panel be as tall
 * as it asks and scrolls it where the window is shorter, so the panel asks
 * for a height of its own rather than one its lists would collapse to.
 */
internal val PanelHeight = 548.dp

/**
 * Watching a video together: a link to paste, what is playing and coming up,
 * and the media servers where this person may browse them
 * (WatchPanelView.swift; WatchPanel.tsx, WatchTab.tsx).
 *
 * Drawn with the app's own pieces, as the music panel is: the web's tab strip
 * is a sliding segmented choice, its field a capsule that lights while it is
 * typed in, and each video a row with its poster.
 */
@Composable
fun WatchPanel(app: AppModel, call: CallModel, close: () -> Unit, modifier: Modifier = Modifier) {
    val engine = remember(call) { WatchEngine.of(call) }
    val context = LocalContext.current
    LaunchedEffect(engine) { engine.lend(context) }
    WatchPanelContent(engine, close, modifier)
}

@Composable
internal fun WatchPanelContent(engine: WatchEngine, close: () -> Unit, modifier: Modifier = Modifier, start: WatchTab = WatchTab.Queue) {
    var tab by remember { mutableStateOf(start) }
    // When it opens, and again once the server has said whether this person
    // may start a show: browsing is for choosing, and choosing is theirs.
    LaunchedEffect(engine.ready) { engine.loadServers() }
    MediaPanelGlass(modifier.height(PanelHeight)) {
        Column(Modifier.fillMaxSize()) {
            MediaPanelHeader(t("Watch together"), t("Close watch together"), close, symbol = "play.rectangle.fill")
            if (engine.sealed) {
                // The control bar offers no button in a sealed call; should
                // the panel be reached anyway, it says why there is nothing.
                Box(Modifier.weight(1f).fillMaxWidth(), contentAlignment = Alignment.Center) {
                    MediaEmpty("lock.fill", t("Not available in an encrypted call"))
                }
            } else {
                // The tab is there only for somebody the server lists media
                // servers for, which is somebody given them.
                AnimatedVisibility(engine.servers.isNotEmpty(), enter = Theme.arrive, exit = Theme.depart) {
                    SegmentedChoice(
                        listOf(WatchTab.Queue to t("Queue"), WatchTab.Media to t("Media servers")),
                        tab, { tab = it },
                        Modifier.padding(start = 12.dp, end = 12.dp, bottom = 10.dp),
                    )
                }
                MediaRule()
                AnimatedContent(
                    tab == WatchTab.Media && engine.servers.isNotEmpty(),
                    Modifier.weight(1f).fillMaxWidth(),
                    transitionSpec = { fadeIn(Theme.settle()) togetherWith fadeOut(Theme.leave()) },
                    contentAlignment = Alignment.TopCenter,
                    label = "tab",
                ) { browsing ->
                    if (browsing) MediaBrowser(engine, engine.servers) { tab = WatchTab.Queue } else QueueView(engine)
                }
            }
        }
    }
}

/** The box a link is pasted into, and the show's videos. */
@Composable
private fun QueueView(engine: WatchEngine) {
    var pasted by remember { mutableStateOf("") }
    var busy by remember { mutableStateOf(false) }
    val show = engine.show
    // Pasting starts the show here when nobody runs one and this person may
    // read links; anybody may add to a show that is running.
    val canAdd = show != null || engine.ready
    fun add() {
        val text = pasted.trim()
        if (text.isEmpty() || busy) return
        busy = true
        // In the call's own scope rather than the panel's: a panel closed
        // while the link is read would otherwise take the adding with it.
        engine.scope.launch {
            try {
                if (engine.add(text)) pasted = ""
            } finally {
                busy = false
            }
        }
    }
    Column(Modifier.fillMaxSize()) {
        KitEnabled(canAdd) {
            Row(
                Modifier.alpha(if (canAdd) 1f else 0.5f).padding(12.dp),
                horizontalArrangement = Arrangement.spacedBy(8.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                MediaField(
                    t("A video link, or the text it was shared in"), pasted, { pasted = it }, ::add,
                    Modifier.weight(1f), symbol = "link", limit = WatchEngine.LINK_LENGTH, url = true,
                )
                AnimatedContent(
                    busy, Modifier.size(34.dp),
                    transitionSpec = { Theme.arrive togetherWith Theme.depart },
                    contentAlignment = Alignment.Center, label = "busy",
                ) { working ->
                    if (working) MediaSpinner(size = 16f)
                    else MediaIconButton("plus", t("Add a video"), ::add, size = 34.dp, prominent = pasted.isNotBlank())
                }
            }
        }
        MediaRule()
        LazyColumn(Modifier.weight(1f).fillMaxWidth(), contentPadding = PaddingValues(8.dp), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            val now = show?.now
            if (show != null && now != null) {
                item(key = "now " + now.key) {
                    VideoRow(engine, now, playing = true, remove = null, Modifier.animateItem(Theme.settle(), Theme.settle(), Theme.leave()))
                }
                if (show.queue.isNotEmpty()) {
                    item(key = "heading") {
                        MenuHeading(t("Queue"), Modifier.padding(top = 6.dp).animateItem(Theme.settle(), Theme.settle(), Theme.leave()))
                    }
                }
                items(show.queue, key = { it.key }) { video ->
                    VideoRow(engine, video, playing = false, remove = { engine.remove(video.key) },
                        Modifier.animateItem(Theme.settle(), Theme.settle(), Theme.leave()))
                }
            } else {
                item(key = "empty") {
                    MediaEmpty(
                        "film",
                        if (canAdd) t("Paste a link and everybody watches it together, in step.")
                        else t("When somebody signed in starts a video, it shows up here."),
                        Modifier.animateItem(Theme.settle(), Theme.settle(), Theme.leave()),
                    )
                }
            }
        }
        AnimatedVisibility(engine.mine, enter = Theme.slideUp, exit = Theme.slideDown) {
            Column {
                MediaRule()
                Row(Modifier.fillMaxWidth().padding(10.dp), horizontalArrangement = Arrangement.End) {
                    MediaButton({ engine.stop() }, quiet = true) {
                        Symbol("stop.circle")
                        KitText(t("Stop watching together"), lines = 1)
                    }
                }
            }
        }
    }
}

/** One video in the show. Its title and picture come from the server by the ticket, never from the call. */
@Composable
private fun VideoRow(engine: WatchEngine, video: WatchVideo, playing: Boolean, remove: (() -> Unit)?, modifier: Modifier = Modifier) {
    LaunchedEffect(video.ticket) { engine.info(video.ticket) }
    val info = engine.infos[video.ticket]
    val known = info?.video
    MediaRow(modifier, lit = playing) {
        Row(Modifier.weight(1f), horizontalArrangement = Arrangement.spacedBy(10.dp), verticalAlignment = Alignment.CenterVertically) {
            Box(Modifier.size(72.dp, 40.dp), contentAlignment = Alignment.BottomStart) {
                MediaArtwork(known?.cover ?: "", Modifier.fillMaxSize(), symbol = "film", corner = 6f)
                if (playing) {
                    Box(Modifier.padding(4.dp).size(16.dp).background(Theme.tally, CircleShape), contentAlignment = Alignment.Center) {
                        Symbol("play.fill", size = 8f, weight = Font.Bold, tint = Theme.tallyForeground)
                    }
                }
            }
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                KitText(
                    known?.title ?: when (info) {
                        WatchInfo.Unavailable -> t("That video cannot be played here.")
                        WatchInfo.Failed -> t("The video could not be read. Try again.")
                        else -> "…"
                    },
                    font = Font(12.5f, Font.Medium), color = Theme.foreground, lines = 1,
                )
                KitText(
                    listOfNotNull(
                        if (playing) t("Playing") else null,
                        t("Added by {name}", mapOf("name" to video.by)),
                        if (video.duration > 0) clockOf(video.duration) else null,
                    ).joinToString(" · "),
                    font = Font(10.5f), color = if (playing) Theme.tallyInk else Theme.muted, lines = 1,
                )
            }
            // Always there on a touch screen, which has no pointer to bring it.
            if (remove != null) {
                MediaIconButton("xmark", t("Take {title} off", mapOf("title" to (known?.title ?: ""))), remove, size = 24.dp)
            }
        }
    }
}
