package app.tomoshibi.ui.music

import android.os.Build
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.blur
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.dropShadow
import androidx.compose.ui.draw.scale
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.shadow.Shadow
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.DpOffset
import androidx.compose.ui.unit.dp
import app.tomoshibi.basics.Theme
import app.tomoshibi.basics.t
import app.tomoshibi.feature.music.MusicTab
import app.tomoshibi.feature.music.SongDesk
import app.tomoshibi.feature.watch.clockOf
import app.tomoshibi.model.AppModel
import app.tomoshibi.model.CallModel
import app.tomoshibi.ui.kit.Appear
import app.tomoshibi.ui.kit.Font
import app.tomoshibi.ui.kit.GlassSwitch
import app.tomoshibi.ui.kit.HearingRow
import app.tomoshibi.ui.kit.KitEnabled
import app.tomoshibi.ui.kit.KitText
import app.tomoshibi.ui.kit.MediaArtwork
import app.tomoshibi.ui.kit.MediaBadge
import app.tomoshibi.ui.kit.MediaButton
import app.tomoshibi.ui.kit.MediaEmpty
import app.tomoshibi.ui.kit.MediaField
import app.tomoshibi.ui.kit.MediaIconButton
import app.tomoshibi.ui.kit.MediaPanelGlass
import app.tomoshibi.ui.kit.MediaPanelHeader
import app.tomoshibi.ui.kit.MediaProblem
import app.tomoshibi.ui.kit.MediaProgress
import app.tomoshibi.ui.kit.MediaRow
import app.tomoshibi.ui.kit.MediaRule
import app.tomoshibi.ui.kit.MediaSlider
import app.tomoshibi.ui.kit.MediaSpinner
import app.tomoshibi.ui.kit.MenuHeading
import app.tomoshibi.ui.kit.SegmentedChoice
import app.tomoshibi.ui.kit.SwappingSymbol
import app.tomoshibi.ui.kit.Symbol
import app.tomoshibi.ui.kit.rounded
import kotlin.math.roundToInt
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import uniffi.tomoshibi_protocol.DeskEntry
import uniffi.tomoshibi_protocol.DeskState
import uniffi.tomoshibi_protocol.MusicQuality
import uniffi.tomoshibi_protocol.MusicTrack

/**
 * Whether the control bar offers the music panel (MusicFeature.swift
 * `available`; Room.tsx ~280): to somebody who can reach the library, before
 * there is any music; to everybody once there is a desk or music to turn down
 * or ask for; and while the panel is open, so the button that opened it is
 * there to close it after the music has stopped.
 *
 * A sealed call is no exception. The desk there plays for its holder alone,
 * saying nothing to the room; the music itself is a sealed track like any
 * other, and everybody else gets a volume for it. Reading it makes the call's
 * desk, which from then on follows the room and turns music down under speech.
 */
fun musicAvailable(call: CallModel): Boolean = SongDesk.of(call).offer.offered

/** Whether music is playing from anybody, for a control that wants to light up (Room.tsx `playing`). */
fun musicPlaying(call: CallModel): Boolean = SongDesk.of(call).offer.playing

/**
 * The music panel: the song desk, and the library to choose from
 * (MusicPanelView.swift; components/room/MusicPanel.tsx).
 *
 * Three tabs: the desk, which everybody has; and search and playlists, for
 * whoever can reach the library. Choosing a song adds it to the desk, and
 * starts the desk here if nobody runs one; somebody who cannot reach the
 * library asks the desk instead, by a song's name or a link as it was shared.
 *
 * The volume beside what is playing is the listener's own. For somebody
 * listening it is the setting the sound panel keeps for that person's music,
 * so muting here stops it at the media server and the lossless stream is not
 * even asked for; for whoever plays it, it is their own speakers alone, and
 * what everybody else hears is untouched.
 */
@Composable
fun MusicPanel(app: AppModel, call: CallModel, close: () -> Unit, modifier: Modifier = Modifier) {
    val desk = remember(call) { SongDesk.of(call) }
    MusicPanelContent(desk, close, modifier)
}

/** About as tall as the web's panel (`h-[min(34rem,...)]`), as MusicPanelView.swift's `height`. */
private val PanelHeight = 548.dp

@Composable
internal fun MusicPanelContent(desk: SongDesk, close: () -> Unit, modifier: Modifier = Modifier) {
    DisposableEffect(desk) {
        desk.panelOpen = true
        onDispose { desk.panelOpen = false }
    }
    val tab = if (desk.libraries == null) MusicTab.Desk else desk.tab
    MediaPanelGlass(modifier.height(PanelHeight)) {
        Column(Modifier.fillMaxSize()) {
            MediaPanelHeader(t("Music"), t("Close music"), close, symbol = "music.note")
            AnimatedVisibility(desk.libraries != null, enter = Theme.arrive, exit = Theme.depart) {
                SegmentedChoice(
                    listOf(MusicTab.Desk to t("Song desk"), MusicTab.Search to t("Search"), MusicTab.Playlist to t("Playlist")),
                    desk.tab, { desk.tab = it },
                    Modifier.padding(start = 12.dp, end = 12.dp, bottom = 10.dp),
                )
            }
            MediaRule()
            AnimatedContent(
                tab,
                Modifier.weight(1f).fillMaxWidth(),
                transitionSpec = { fadeIn(Theme.settle()) togetherWith fadeOut(Theme.leave()) },
                contentAlignment = Alignment.TopCenter,
                label = "tab",
            ) { shown ->
                when (shown) {
                    MusicTab.Desk -> DeskTab(desk)
                    MusicTab.Search -> SearchTab(desk)
                    MusicTab.Playlist -> PlaylistTab(desk)
                }
            }
        }
    }
}

private fun Modifier.arriving(scope: androidx.compose.foundation.lazy.LazyItemScope) =
    with(scope) { this@arriving.animateItem(Theme.settle(), Theme.settle(), Theme.leave()) }

// MARK: - The desk

@Composable
private fun DeskTab(desk: SongDesk) {
    var typed by remember { mutableStateOf("") }
    val state = desk.state
    fun ask() {
        // Kept while the last ask is still being looked up, as the web's box
        // refuses a second one then.
        if (typed.isBlank() || desk.asking) return
        desk.ask(typed)
        typed = ""
    }
    Column(Modifier.fillMaxSize()) {
        LazyColumn(Modifier.weight(1f).fillMaxWidth(), contentPadding = PaddingValues(12.dp)) {
            item(key = "top") {
                val now = state?.now
                val player = desk.player
                AnimatedContent(
                    when {
                        state != null && now != null -> "now " + now.key
                        player != null -> "player " + player.id
                        else -> "idle"
                    },
                    transitionSpec = { Theme.arrive togetherWith Theme.depart },
                    label = "top",
                ) { face ->
                    when {
                        face.startsWith("now") && state != null && now != null -> NowPlaying(desk, state, now)
                        face.startsWith("player") && player != null -> {
                            // Music arriving from somebody whose desk is not
                            // heard here -- in a sealed call desks say
                            // nothing -- still has a volume.
                            val setting = desk.hearing(player.id)
                            HearingRow(
                                t("{name} (music)", mapOf("name" to player.name.ifEmpty { player.id })),
                                setting.volume, setting.blocked,
                                { desk.setVolume(player.id, it) }, { desk.setBlocked(player.id, it) },
                                Modifier.background(Theme.wash.copy(alpha = 0.05f), rounded(16f)).padding(12.dp),
                            )
                        }
                        else -> Idle(desk)
                    }
                }
            }
            state?.next?.let { next ->
                item(key = "next") {
                    Row(
                        Modifier.padding(top = 12.dp).padding(horizontal = 10.dp).arriving(this),
                        horizontalArrangement = Arrangement.spacedBy(10.dp),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        MediaSpinner(size = 13f)
                        KitText(t("Loading {title}", mapOf("title" to next.title)), Modifier.weight(1f), font = Font(12f), color = Theme.muted, lines = 1)
                        // Out of the queue already, and still the holder's to
                        // take out: it was on the list a moment ago.
                        if (desk.mine) MediaIconButton("xmark", t("Take {title} off", mapOf("title" to next.title)), { desk.remove(next.key) }, size = 24.dp)
                    }
                }
            }
            val queue = state?.queue.orEmpty()
            if (queue.isNotEmpty()) {
                item(key = "queue") { MenuHeading(t("Queue"), Modifier.padding(top = 12.dp).arriving(this)) }
                itemsIndexed(queue, key = { _, entry -> "entry " + entry.key }) { index, entry ->
                    QueueRow(desk, entry, index, Modifier.padding(top = 2.dp).arriving(this))
                }
            }
        }

        MediaRule()
        val canAsk = state != null || desk.libraries != null
        KitEnabled(canAsk) {
            Row(
                Modifier.alpha(if (canAsk) 1f else 0.5f).padding(12.dp),
                horizontalArrangement = Arrangement.spacedBy(8.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                MediaField(t("A song's name, or a playlist link"), typed, { typed = it }, ::ask, Modifier.weight(1f).semantics {
                    contentDescription = t("Ask for a song")
                }, symbol = "music.note.list", limit = 2000)
                AnimatedContent(
                    desk.asking, Modifier.size(34.dp),
                    transitionSpec = { Theme.arrive togetherWith Theme.depart },
                    contentAlignment = Alignment.Center, label = "asking",
                ) { busy ->
                    if (busy) MediaSpinner(size = 16f)
                    else MediaIconButton("plus", t("Ask for a song"), ::ask, size = 34.dp, prominent = typed.isNotBlank())
                }
            }
        }
    }
}

/** What is playing, on a card lit from behind by its own cover. */
@Composable
private fun NowPlaying(desk: SongDesk, state: DeskState, now: DeskEntry) {
    val voted = desk.call.localIdentity in state.votes
    val shape = rounded(18f)
    Box(Modifier.fillMaxWidth().clip(shape).border(1.dp, Theme.wash.copy(alpha = 0.09f), shape)) {
        // The card's size, whatever the cover's: the cover fills the card and
        // is clipped to it, blurred, under a shade. On Apple a cover laid in
        // the background made the background its own square, which stood out
        // below the card. Blurred where the system can (Android 12 and
        // later); fainter where it cannot, so a sharp cover does not fight
        // the words.
        Box(Modifier.matchParentSize()) {
            MediaArtwork(
                now.cover,
                Modifier.fillMaxSize().graphicsLayer {
                    scaleX = 1.6f
                    scaleY = 1.6f
                    alpha = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) 0.55f else 0.25f
                }.blur(36.dp),
                corner = 0f,
            )
            // An even dimming for the words over the blurred cover, at the
            // darkest the fade from top to foot reached: the card has words
            // at its top as well as its foot, so the fade bought them nothing
            // but a gradient, which the owner asked for fewer of (2026-10-20).
            Box(Modifier.fillMaxSize().background(Color.Black.copy(alpha = 0.45f)))
        }
        Column(Modifier.padding(14.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            Row(horizontalArrangement = Arrangement.spacedBy(12.dp), verticalAlignment = Alignment.CenterVertically) {
                MediaArtwork(
                    now.cover,
                    Modifier.size(66.dp).dropShadow(rounded(10f), Shadow(radius = 10.dp, offset = DpOffset(0.dp, 5.dp), alpha = 0.4f)),
                    corner = 10f,
                )
                Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(3.dp)) {
                    KitText(now.title, font = Font(14.5f, Font.Semibold), color = Theme.foreground, lines = 2)
                    if (now.artists.isNotEmpty()) {
                        KitText(now.artists.joinToString(" / "), font = Font(12f), color = Theme.foreground.copy(alpha = 0.72f), lines = 1)
                    }
                    KitText(t("Asked for by {name}", mapOf("name" to now.by)), font = Font(11f), color = Theme.muted, lines = 1)
                }
            }

            AnimatedVisibility(state.paused || state.quality != null, enter = Theme.arrive, exit = Theme.depart) {
                Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                    AnimatedVisibility(state.paused, enter = Theme.arrive, exit = Theme.depart) { MediaBadge(t("Paused"), symbol = "pause.fill") }
                    // The badge saying what the file is arrives once the song
                    // is decoded, and changes with the next song: on Apple it
                    // popped in, alone of everything on this card.
                    AnimatedContent(state.quality, transitionSpec = { Theme.arrive togetherWith Theme.depart }, label = "quality") { quality ->
                        if (quality != null) MediaBadge(quality, symbol = "waveform", lit = true)
                    }
                }
            }

            AnimatedContent(desk.mine, transitionSpec = { Theme.arrive togetherWith Theme.depart }, label = "mine") { mine ->
                if (mine) {
                    Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                        Position(desk)
                        Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
                            SwappingSymbol(
                                if (desk.monitorVolume == 0f) "speaker.slash.fill" else "speaker.wave.2.fill",
                                Modifier.width(18.dp), size = 11f, weight = Font.Semibold, tint = Theme.muted,
                            )
                            KitText(t("In your speakers"), font = Font(11.5f), color = Theme.muted, lines = 1)
                            MediaSlider(
                                desk.monitorVolume.toDouble(), { desk.monitorVolume = it.toFloat() }, Modifier.weight(1f),
                                step = 0.05, label = t("Music volume in your speakers"),
                            )
                            KitText(
                                "${(desk.monitorVolume * 100).roundToInt()}%", Modifier.width(38.dp),
                                font = Font(11f, Font.Medium, digits = true), color = Theme.muted, lines = 1, align = TextAlign.End,
                            )
                        }
                    }
                } else {
                    val setting = desk.hearing(state.holder)
                    HearingRow(t("Music"), setting.volume, setting.blocked, { desk.setVolume(state.holder, it) }, { desk.setBlocked(state.holder, it) })
                }
            }

            // This participant's own, like the volume above it: see duck.ts.
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
                Symbol("person.wave.2.fill", Modifier.width(18.dp), size = 11f, weight = Font.Semibold, tint = Theme.muted)
                KitText(t("Lower the music while anybody talks"), Modifier.weight(1f), font = Font(11.5f), color = Theme.muted)
                GlassSwitch(desk.ducks, { desk.ducks = it }, Modifier.scale(0.85f), label = t("Lower the music while anybody talks"))
            }

            Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
                if (desk.mine) {
                    MediaIconButton(
                        if (state.paused) "play.fill" else "pause.fill", if (state.paused) t("Play") else t("Pause"),
                        { desk.pause(!state.paused) }, size = 34.dp, prominent = true,
                    )
                    MediaButton({ desk.skip() }, Modifier.weight(1f), height = 34.dp) {
                        Symbol("forward.fill")
                        KitText(t("Next song"), lines = 1)
                    }
                    // A round button with its words for the screen reader:
                    // written out, they took the room the next song's button
                    // needs and were cut short.
                    MediaIconButton("stop.circle", t("Stop the desk"), { desk.close() }, size = 34.dp)
                } else {
                    // Not while the next is on its way: the desk does not
                    // count those votes (jukebox.ts `Desk.vote`).
                    MediaButton({ desk.vote() }, Modifier.weight(1f), height = 34.dp, enabled = !voted && !state.loading) {
                        SwappingSymbol(if (voted) "checkmark" else "forward.fill")
                        AnimatedContent(
                            t("Vote to skip ({votes}/{needed})", mapOf("votes" to state.votes.size, "needed" to state.needed.toInt())),
                            transitionSpec = { fadeIn(Theme.settle()) togetherWith fadeOut(Theme.leave()) },
                            label = "votes",
                        ) { said -> KitText(said, lines = 1) }
                    }
                }
            }
        }
    }
}

/**
 * Where the song playing from here is: only the holder knows, and the web
 * shows nobody, but a holder with a queue wants to see how long is left.
 * Read every half second; arrives when the song begins and goes with it.
 */
@Composable
private fun Position(desk: SongDesk) {
    var progress by remember { mutableStateOf(desk.progress) }
    LaunchedEffect(desk) {
        while (true) {
            progress = desk.progress
            delay(500)
        }
    }
    val shown = progress?.takeIf { it.second > 0 }
    AnimatedVisibility(shown != null, enter = Theme.arrive, exit = Theme.depart) {
        val (at, of) = shown ?: (0.0 to 1.0)
        Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
            MediaProgress((at / of).toFloat())
            Row {
                KitText(clockOf(at), font = Font(10f, Font.Medium, digits = true), color = Theme.muted, lines = 1)
                Spacer(Modifier.weight(1f))
                KitText(clockOf(of), font = Font(10f, Font.Medium, digits = true), color = Theme.muted, lines = 1)
            }
        }
    }
}

/**
 * Nothing playing: what to do about it, and for whoever runs an idle desk,
 * the way to put it away (it still tells the room it is there every fifteen
 * seconds).
 */
@Composable
private fun Idle(desk: SongDesk) {
    Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(4.dp), horizontalAlignment = Alignment.CenterHorizontally) {
        MediaEmpty(
            "music.note.list",
            when {
                desk.state != null -> t("Nothing is playing. Ask for a song below.")
                desk.libraries != null -> t("Search a song or paste a playlist link to start the song desk.")
                else -> t("When somebody plays music in this call, it shows up here.")
            },
        )
        AnimatedVisibility(desk.mine, enter = Theme.arrive, exit = Theme.depart) {
            MediaButton({ desk.close() }, quiet = true) {
                Symbol("stop.circle")
                KitText(t("Stop the desk"), lines = 1)
            }
        }
    }
}

@Composable
private fun QueueRow(desk: SongDesk, entry: DeskEntry, index: Int, modifier: Modifier = Modifier) {
    MediaRow(modifier) {
        Row(Modifier.weight(1f), horizontalArrangement = Arrangement.spacedBy(10.dp), verticalAlignment = Alignment.CenterVertically) {
            KitText("${index + 1}", Modifier.width(16.dp), font = Font(11f, Font.Medium, digits = true), color = Theme.muted, lines = 1,
                align = TextAlign.End)
            MediaArtwork(entry.cover, Modifier.size(34.dp), corner = 6f)
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(1.dp)) {
                KitText(entry.title, font = Font(12.5f, Font.Medium), color = Theme.foreground, lines = 1)
                KitText(
                    listOf(entry.artists.joinToString(" / "), t("Asked for by {name}", mapOf("name" to entry.by))).filter { it.isNotEmpty() }
                        .joinToString(" · "),
                    font = Font(10.5f), color = Theme.muted, lines = 1,
                )
            }
            // The holder's, and always there on a touch screen, which has no
            // pointer to bring them.
            if (desk.mine) {
                Row(horizontalArrangement = Arrangement.spacedBy(2.dp)) {
                    if (index > 0) {
                        MediaIconButton("arrow.up.to.line", t("Play {title} next", mapOf("title" to entry.title)), { desk.toTop(entry.key) }, size = 24.dp)
                    }
                    MediaIconButton("xmark", t("Take {title} off", mapOf("title" to entry.title)), { desk.remove(entry.key) }, size = 24.dp)
                }
            }
        }
    }
}

// MARK: - Searching the library

@Composable
private fun SearchTab(desk: SongDesk) {
    val libraries = desk.libraries.orEmpty()
    val library = libraries.firstOrNull { it.id == desk.searchSource }
    fun search() {
        // In the call's own scope: the answer is kept for the tab, which may
        // be closed and opened again before it comes.
        desk.call.scope.launch { desk.searchLibrary() }
    }
    Column(Modifier.fillMaxSize()) {
        Column(Modifier.padding(12.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
            if (libraries.size > 1) {
                SegmentedChoice(libraries.map { it.id to it.name }, desk.searchSource, { desk.chooseLibrary(it) }, height = 28.dp)
            }
            Row(horizontalArrangement = Arrangement.spacedBy(10.dp), verticalAlignment = Alignment.CenterVertically) {
                KitText(t("Quality"), font = Font(12f), color = Theme.muted, lines = 1)
                SegmentedChoice(
                    listOf(MusicQuality.LOSSLESS to t("Lossless"), MusicQuality.BEST to t("Highest")),
                    desk.quality, { desk.quality = it }, Modifier.weight(1f), height = 28.dp,
                )
            }
            val unsigned = library != null && !library.signedIn
            val best = desk.quality == MusicQuality.BEST
            AnimatedVisibility(unsigned || best, enter = Theme.arrive, exit = Theme.depart) {
                Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
                    AnimatedVisibility(unsigned, enter = Theme.arrive, exit = Theme.depart) { Note(t("Not signed in, so at most 320 kbps.")) }
                    AnimatedVisibility(best, enter = Theme.arrive, exit = Theme.depart) { Note(t("Hi-Res and masters are large, and take longer to start.")) }
                }
            }
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
                MediaField(t("Search songs"), desk.searchQuery, { desk.searchQuery = it }, ::search, Modifier.weight(1f),
                    symbol = "magnifyingglass", limit = 100)
                AnimatedContent(
                    desk.searching, Modifier.size(34.dp),
                    transitionSpec = { Theme.arrive togetherWith Theme.depart },
                    contentAlignment = Alignment.Center, label = "searching",
                ) { busy ->
                    if (busy) MediaSpinner(size = 16f)
                    else MediaIconButton("arrow.right", t("Search"), ::search, size = 34.dp, prominent = desk.searchQuery.isNotBlank())
                }
            }
        }
        MediaRule()
        LazyColumn(Modifier.weight(1f).fillMaxWidth(), contentPadding = PaddingValues(8.dp), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            desk.searchProblem?.let { problem ->
                item(key = "problem") { MediaProblem(problem, Modifier.padding(bottom = 6.dp).arriving(this)) }
            }
            val results = desk.searchResults
            if (results == null) {
                item(key = "hint") { MediaEmpty("music.magnifyingglass", t("Search for a song to play into the call."), Modifier.arriving(this)) }
            } else if (results.isEmpty()) {
                item(key = "none") { MediaEmpty("magnifyingglass", t("Nothing found."), Modifier.arriving(this)) }
            } else {
                items(results, key = { "track " + it.id }) { track ->
                    TrackRow(track, track.id in desk.searchAdded, Modifier.arriving(this)) {
                        desk.add(listOf(SongDesk.song(track, desk.searchSource)))
                        desk.searchAdded = desk.searchAdded + track.id
                    }
                }
            }
        }
    }
}

@Composable
private fun Note(text: String) {
    Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
        Symbol("info.circle", Modifier.padding(top = 1.dp), size = 11f, tint = Theme.muted)
        KitText(text, font = Font(11f), color = Theme.muted)
    }
}

// MARK: - A playlist or song link

@Composable
private fun PlaylistTab(desk: SongDesk) {
    fun open() {
        desk.call.scope.launch { desk.openLink() }
    }
    Column(Modifier.fillMaxSize()) {
        Row(Modifier.padding(12.dp), horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
            MediaField(t("Paste a playlist or song link"), desk.pasted, { desk.pasted = it }, ::open, Modifier.weight(1f).semantics {
                contentDescription = t("Playlist link")
            }, symbol = "link", limit = 2000, url = true)
            MediaButton(::open, prominent = true, height = 34.dp) {
                Box(contentAlignment = Alignment.Center) {
                    KitText(t("Open"), Modifier.alpha(if (desk.reading) 0f else 1f), lines = 1)
                    Appear(desk.reading) { MediaSpinner(size = 13f, tint = Theme.tallyForeground) }
                }
            }
        }
        MediaRule()
        LazyColumn(Modifier.weight(1f).fillMaxWidth(), contentPadding = PaddingValues(8.dp), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            desk.linkProblem?.let { problem ->
                item(key = "problem") { MediaProblem(problem, Modifier.padding(bottom = 6.dp).arriving(this)) }
            }
            val found = desk.linked
            if (found != null) {
                item(key = "link " + found.title) {
                    Row(
                        Modifier.padding(bottom = 6.dp).arriving(this).fillMaxWidth()
                            .background(Theme.wash.copy(alpha = 0.05f), rounded(14f)).padding(10.dp),
                        horizontalArrangement = Arrangement.spacedBy(10.dp),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        MediaArtwork(found.tracks.firstOrNull()?.cover ?: "", Modifier.size(44.dp), symbol = "music.note.list", corner = 8f)
                        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                            KitText(found.title, font = Font(13f, Font.Semibold), color = Theme.foreground, lines = 2)
                            KitText(t("{count} songs", mapOf("count" to found.tracks.size)), font = Font(11f), color = Theme.muted, lines = 1)
                        }
                        MediaButton({
                            desk.add(found.tracks.map { SongDesk.song(it, found.source) })
                            desk.linkAdded = found.tracks.map { it.id }.toSet()
                        }, prominent = true, enabled = found.tracks.isNotEmpty()) { KitText(t("Add all"), lines = 1) }
                    }
                }
                items(found.tracks, key = { "track " + it.id }) { track ->
                    TrackRow(track, track.id in desk.linkAdded, Modifier.arriving(this)) {
                        desk.add(listOf(SongDesk.song(track, found.source)))
                        desk.linkAdded = desk.linkAdded + track.id
                    }
                }
            } else if (desk.linkProblem == null) {
                item(key = "hint") { MediaEmpty("link", t("Paste a link shared from a music app."), Modifier.arriving(this)) }
            }
        }
    }
}

// MARK: - Pieces

/** A song from the library: pressing it adds it to the desk. */
@Composable
private fun TrackRow(track: MusicTrack, added: Boolean, modifier: Modifier = Modifier, add: () -> Unit) {
    MediaRow(modifier.semantics { contentDescription = t("Add {title}", mapOf("title" to track.title)) }, onClick = add) { lit ->
        Row(Modifier.weight(1f), horizontalArrangement = Arrangement.spacedBy(10.dp), verticalAlignment = Alignment.CenterVertically) {
            MediaArtwork(track.cover, Modifier.size(40.dp), corner = 7f)
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(1.dp)) {
                KitText(track.title, font = Font(13f, Font.Medium), color = Theme.foreground, lines = 1)
                KitText(
                    listOf(track.artists.joinToString(" / "), track.album).filter { it.isNotEmpty() }.joinToString(" · "),
                    font = Font(11f), color = Theme.muted, lines = 1,
                )
            }
            AnimatedContent(
                if (added) 1 else if (lit) 2 else 0,
                Modifier.widthIn(min = 40.dp),
                transitionSpec = { Theme.arrive togetherWith Theme.depart },
                contentAlignment = Alignment.CenterEnd,
                label = "track",
            ) { face ->
                when (face) {
                    1 -> MediaBadge(t("Added"), symbol = "checkmark", lit = true)
                    2 -> Symbol("plus.circle.fill", size = 17f, tint = Theme.tally)
                    else -> KitText(clockOf(track.duration), font = Font(11f, Font.Medium, digits = true), color = Theme.muted, lines = 1)
                }
            }
        }
    }
}
