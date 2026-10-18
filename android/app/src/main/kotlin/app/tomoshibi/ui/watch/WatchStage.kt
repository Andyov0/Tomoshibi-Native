package app.tomoshibi.ui.watch

import android.view.TextureView
import android.view.ViewGroup
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.pointer.PointerEventPass
import androidx.compose.ui.input.pointer.PointerEventType
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import app.tomoshibi.basics.Theme
import app.tomoshibi.basics.t
import app.tomoshibi.feature.music.SongDesk
import app.tomoshibi.feature.watch.ExoBackend
import app.tomoshibi.feature.watch.WatchEngine
import app.tomoshibi.feature.watch.WatchInfo
import app.tomoshibi.feature.watch.WatchPlayer
import app.tomoshibi.feature.watch.WatchWords
import app.tomoshibi.feature.watch.YouTubeBackend
import app.tomoshibi.feature.watch.clockOf
import app.tomoshibi.model.AppModel
import app.tomoshibi.model.CallModel
import app.tomoshibi.ui.kit.Appear
import app.tomoshibi.ui.kit.Capsule
import app.tomoshibi.ui.kit.Font
import app.tomoshibi.ui.kit.KitFont
import app.tomoshibi.ui.kit.KitInk
import app.tomoshibi.ui.kit.KitText
import app.tomoshibi.ui.kit.MediaSlider
import app.tomoshibi.ui.kit.MediaSpinner
import app.tomoshibi.ui.kit.Motion
import app.tomoshibi.ui.kit.SwappingSymbol
import app.tomoshibi.ui.kit.Symbol
import app.tomoshibi.ui.kit.pressable
import app.tomoshibi.ui.kit.rememberPress
import kotlinx.coroutines.delay
import uniffi.tomoshibi_protocol.WatchVideo

/**
 * Whether a show is running and its picture belongs on the stage
 * (WatchFeature.swift `onStage`): the web lists the watched video as a
 * picture whenever a show is believed, with or without a video in it
 * (surface.ts `surfaces`), and its tile says nothing is playing until there
 * is.
 *
 * The room asks this from the first time it draws, before any panel is
 * opened, which is when the Swift room makes both features
 * (RoomView.swift's initialiser makes WatchFeature and MusicFeature): so
 * this makes the call's show and its song desk, each of which then follows
 * the room and puts what it offers the bar in the call's parts. Made later,
 * the desk would turn nobody's music down under speech until somebody
 * opened its panel.
 */
fun watchShowing(call: CallModel): Boolean {
    SongDesk.of(call)
    return WatchEngine.of(call).onStage
}

/** Whether the control bar offers the watch button; see [WatchEngine.offer]. */
fun watchOffered(call: CallModel): Boolean = WatchEngine.of(call).offer.offered

/** Whether the show is playing, for the dot on the button while the panel is closed. */
fun watchPlaying(call: CallModel): Boolean = WatchEngine.of(call).offer.playing

/**
 * The video everybody is watching, drawn on the stage (WatchStageView.swift;
 * WatchScreen.tsx).
 *
 * The controls drive the show, not this player: pressing pause asks the show
 * to pause, and everybody pauses. This player also does it at once, so the
 * press is felt without waiting for the show's answer to come back.
 *
 * In the strip, when somebody has pinned another picture, it is a tile like
 * the others: the picture goes on playing, labelled, with no controls.
 * [openPanel] opens the watch panel, from the empty stage and from the
 * controls.
 */
@Composable
fun WatchStage(app: AppModel, call: CallModel, modifier: Modifier = Modifier, openPanel: () -> Unit = {}) {
    val engine = remember(call) { WatchEngine.of(call) }
    val context = LocalContext.current
    LaunchedEffect(engine) { engine.lend(context) }
    WatchStageContent(engine, modifier, openPanel)
}

/** Which of its faces the stage shows, carrying what that face draws, so a face that is leaving still draws what it showed. */
private sealed class Face(val key: String) {
    data object Nothing : Face("nothing")
    data object Waiting : Face("waiting")
    class Playing(val player: WatchPlayer) : Face("playing " + player.video.key)
    class Cannot(val video: WatchVideo, val unavailable: Boolean) : Face("cannot " + video.key)
}

/** The stage's face, read without asking anything of the server (`info` would ask). */
private fun faceOf(engine: WatchEngine): Face {
    val video = engine.show?.now ?: return Face.Nothing
    return when (engine.infos[video.ticket]) {
        is WatchInfo.Known -> engine.player?.takeIf { it.video.key == video.key }?.let { Face.Playing(it) } ?: Face.Waiting
        WatchInfo.Unavailable -> Face.Cannot(video, unavailable = true)
        WatchInfo.Failed -> Face.Cannot(video, unavailable = false)
        null -> Face.Waiting
    }
}

@Composable
internal fun WatchStageContent(engine: WatchEngine, modifier: Modifier = Modifier, openPanel: () -> Unit = {}) {
    val ticket = engine.show?.now?.ticket
    LaunchedEffect(ticket) { if (ticket != null) engine.info(ticket) }
    BoxWithConstraints(modifier.background(Color.Black)) {
        val compact = maxHeight < 200.dp
        // The stage's faces replace each other with Theme's arrival and
        // departure; on Apple nothing animated them at first, and the video,
        // the spinner before it and "cannot be played" swapped in one frame.
        AnimatedContent(
            faceOf(engine),
            Modifier.fillMaxSize(),
            transitionSpec = { Theme.arrive togetherWith Theme.depart },
            contentAlignment = Alignment.Center,
            contentKey = { it.key },
            label = "face",
        ) { face ->
            Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
                when (face) {
                    Face.Nothing -> Column(
                        Modifier.padding(16.dp),
                        verticalArrangement = Arrangement.spacedBy(12.dp),
                        horizontalAlignment = Alignment.CenterHorizontally,
                    ) {
                        Symbol("film", size = if (compact) 18f else 28f, tint = Theme.muted)
                        if (!compact) {
                            KitText(t("Nothing is playing. Paste a video link to watch together."), font = Font(13f), color = Theme.muted,
                                align = TextAlign.Center)
                            StageButton(openPanel, prominent = true) {
                                Symbol("plus")
                                KitText(t("Add a video"))
                            }
                        }
                    }
                    Face.Waiting -> StageSpinner()
                    is Face.Cannot -> Column(
                        Modifier.padding(16.dp),
                        verticalArrangement = Arrangement.spacedBy(12.dp),
                        horizontalAlignment = Alignment.CenterHorizontally,
                    ) {
                        // Not "try again" for a video that plays only through
                        // a relay the server will not run for it: no retry
                        // plays it.
                        KitText(
                            if (face.unavailable) t("That video cannot be played here.") else t("The video could not be read. Try again."),
                            font = Font(13f), color = Color.White.copy(alpha = 0.7f), align = TextAlign.Center,
                        )
                        if (!compact) {
                            StageButton({ engine.skip(face.video.key) }) {
                                Symbol("forward.end.fill")
                                KitText(t("Next video"))
                            }
                        }
                    }
                    is Face.Playing -> PlayerStage(engine, face.player, compact, openPanel)
                }
            }
        }
        Appear(compact, Modifier.align(Alignment.BottomStart)) {
            KitText(
                t("Watching together"),
                Modifier.padding(4.dp).background(Color.Black.copy(alpha = 0.55f), Capsule).padding(horizontal = 6.dp, vertical = 3.dp),
                font = Font(10.5f, Font.Medium), color = Theme.foreground, lines = 1,
            )
        }
    }
}

@Composable
private fun StageSpinner() {
    MediaSpinner(size = 26f, tint = Color.White.copy(alpha = 0.85f))
}

/** A player and everything said over it. */
@Composable
private fun PlayerStage(engine: WatchEngine, player: WatchPlayer, compact: Boolean, openPanel: () -> Unit) {
    val playing = engine.show?.playing ?: false
    // No pointer to hover with: a touch shows the controls for a while, as a
    // video player on a phone does.
    var touches by remember { mutableIntStateOf(0) }
    var touched by remember { mutableStateOf(false) }
    var scrubbing by remember { mutableStateOf<Double?>(null) }
    LaunchedEffect(touches) {
        if (touches == 0) return@LaunchedEffect
        touched = true
        delay(3000)
        touched = false
    }
    val controls = !compact && (touched || !playing || scrubbing != null)
    Box(
        Modifier
            .fillMaxSize()
            // Seen before anything inside takes it, and never taken here:
            // YouTube's frame, while it is refused, needs the same touch.
            .pointerInput(Unit) {
                awaitPointerEventScope {
                    while (true) {
                        val event = awaitPointerEvent(PointerEventPass.Initial)
                        if (event.type == PointerEventType.Press) touches += 1
                    }
                }
            },
    ) {
        WatchSurface(player, Modifier.fillMaxSize())
        // YouTube's frame takes no touches except while it is refused; a
        // file's picture takes none either way. Covered rather than told to
        // refuse them, and covered by something that takes nothing either:
        // the room's own tap on the stage, which pins and fills it, still
        // sees every touch.
        if (!player.tapYoutube) Box(Modifier.matchParentSize().pointerInput(Unit) {
            awaitPointerEventScope {
                while (true) awaitPointerEvent()
            }
        })

        Appear(player.waiting && player.unreachable == null && player.halted != WatchPlayer.Halted.Stopped, Modifier.align(Alignment.Center)) {
            StageSpinner()
        }

        Appear(player.unreachable != null, Modifier.matchParentSize()) {
            // The last reason, kept while it leaves.
            val last = remember { arrayOf(WatchPlayer.Lost.Network) }
            player.unreachable?.let { last[0] = it }
            Unreachable(last[0], player.info.link)
        }

        Appear(player.halted == WatchPlayer.Halted.Stopped, Modifier.matchParentSize()) {
            Box(Modifier.fillMaxSize().background(Color.Black.copy(alpha = 0.85f)), contentAlignment = Alignment.Center) {
                Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(12.dp), horizontalAlignment = Alignment.CenterHorizontally) {
                    Symbol("icloud.slash", size = 22f, tint = Color.White.copy(alpha = 0.7f))
                    KitText(t("The video stopped coming through."), font = Font(13f), color = Color.White, align = TextAlign.Center)
                    StageButton({ player.tryAgain() }, prominent = true) {
                        Symbol("arrow.clockwise")
                        KitText(t("Try again"))
                    }
                }
            }
        }

        Column(
            Modifier.align(Alignment.TopCenter).padding(top = 12.dp),
            verticalArrangement = Arrangement.spacedBy(8.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            Appear(player.halted == WatchPlayer.Halted.Resuming) { StageNotice(t("Reconnecting…")) }
            Appear(player.tapYoutube && player.unreachable == null) {
                Row(
                    Modifier.background(Color.Black.copy(alpha = 0.52f), Capsule).border(1.dp, Color.White.copy(alpha = 0.14f), Capsule)
                        .padding(horizontal = 16.dp, vertical = 9.dp),
                    horizontalArrangement = Arrangement.spacedBy(6.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Symbol("play.fill", size = 13f, weight = Font.Semibold, tint = Color.White)
                    KitText(t("Tap the video to join"), font = Font(13f, Font.Semibold), color = Color.White, lines = 1)
                }
            }
        }

        if (!compact) {
            AnimatedVisibility(controls, Modifier.align(Alignment.BottomCenter).fillMaxWidth(), enter = Theme.slideUp, exit = Theme.slideDown) {
                Controls(engine, player, playing, scrubbing, { scrubbing = it }, openPanel)
            }
        }
    }
}

/**
 * Where the player's picture is drawn: Media3's surface, or YouTube's web
 * view, both owned by the player rather than this view (WatchSurface.swift).
 * The view only lends them a place, so the stage and the strip trading
 * places, or the room redrawing, moves the picture without reloading the
 * video. Shown whole, black either side, as AVPlayerLayer's `resizeAspect`.
 */
@Composable
internal fun WatchSurface(player: WatchPlayer, modifier: Modifier = Modifier) {
    Box(modifier.background(Color.Black), contentAlignment = Alignment.Center) {
        when (val backend = player.backend) {
            is ExoBackend -> key(backend) {
                AndroidView(
                    // Lent once, when the place is made: lent again on every
                    // redraw, the player would make its surface afresh each
                    // time and the picture blink.
                    factory = { context -> TextureView(context).also { backend.attach(it) } },
                    modifier = Modifier.aspectRatio(backend.aspect.coerceIn(0.2f, 5f))
                        .onSizeChanged { backend.shown(it.width, it.height) },
                    onRelease = { backend.detach(it) },
                )
            }
            is YouTubeBackend -> key(backend) {
                AndroidView(
                    factory = {
                        // Taken from wherever it was last lent, which the
                        // place it is leaving may not have let go of yet.
                        (backend.web.parent as? ViewGroup)?.removeView(backend.web)
                        backend.web
                    },
                    modifier = Modifier.fillMaxSize(),
                )
            }
            else -> {}
        }
    }
}

/** Where the video does not reach this viewer, with no relay to fall back on. */
@Composable
private fun Unreachable(lost: WatchPlayer.Lost, link: String) {
    val site = WatchWords.site(link)
    val uris = LocalUriHandler.current
    Box(Modifier.fillMaxSize().background(Color.Black.copy(alpha = 0.85f)), contentAlignment = Alignment.Center) {
        Column(
            Modifier.widthIn(max = 288.dp).padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(8.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            Box(
                Modifier.padding(bottom = 4.dp).size(48.dp).background(Color.White.copy(alpha = 0.08f), CircleShape),
                contentAlignment = Alignment.Center,
            ) {
                Symbol(if (lost == WatchPlayer.Lost.Format) "film.stack" else "icloud.slash", size = 20f, weight = Font.Medium,
                    tint = Color.White.copy(alpha = 0.75f))
            }
            KitText(t("This video does not reach you"), font = Font(14f, Font.Semibold), color = Color.White, align = TextAlign.Center)
            // Not "everybody else goes on watching" for what the site
            // refused: what it refuses here, it refuses everybody.
            KitText(sentence(lost, site), font = Font(11.5f), color = Color.White.copy(alpha = 0.6f), align = TextAlign.Center)
            if (WatchWords.openable(link)) {
                StageButton({ runCatching { uris.openUri(link) } }, Modifier.padding(top = 4.dp)) {
                    Symbol("arrow.up.right.square")
                    KitText(t("Open the link"))
                }
            }
        }
    }
}

private fun sentence(lost: WatchPlayer.Lost, site: String): String = when {
    // A kind of video the player here cannot open, from where it is and
    // through the relay alike: the browsers in the call can, so the show goes
    // on for them.
    lost == WatchPlayer.Lost.Format -> t("This app cannot play this kind of video. Everybody else goes on watching.")
    lost == WatchPlayer.Lost.Refused && site.isNotEmpty() -> t("It plays only on {site} itself.", mapOf("site" to site))
    lost == WatchPlayer.Lost.Failed && site.isNotEmpty() -> t("{site} would not play it here.", mapOf("site" to site))
    site.isEmpty() -> t("Your connection cannot reach where it is. Everybody else goes on watching.")
    else -> t("Your connection cannot reach {site}. Everybody else goes on watching.", mapOf("site" to site))
}

/** The controls under the video: its title, where it is, and the presses that drive the show. */
@Composable
private fun Controls(
    engine: WatchEngine,
    player: WatchPlayer,
    playing: Boolean,
    scrubbing: Double?,
    scrub: (Double?) -> Unit,
    openPanel: () -> Unit,
) {
    val video = player.video
    val length = if (video.duration > 0) video.duration else player.info.duration
    val shown = scrubbing ?: player.position
    Column(
        Modifier
            .fillMaxWidth()
            .background(Brush.verticalGradient(listOf(Color.Transparent, Color.Black.copy(alpha = 0.8f))))
            .padding(start = 12.dp, end = 12.dp, top = 32.dp, bottom = 8.dp),
        verticalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
            KitText(player.info.title, Modifier.weight(1f, fill = false), font = Font(13f, Font.Medium), color = Color.White, lines = 1)
            KitText(
                listOfNotNull(t("Added by {name}", mapOf("name" to video.by)), if (player.relayed) t("Relayed") else null).joinToString(" · "),
                font = Font(11f), color = Color.White.copy(alpha = 0.6f), lines = 1,
            )
        }

        if (length > 0 && !video.live) {
            MediaSlider(
                minOf(shown, length),
                { scrub(it) },
                Modifier.fillMaxWidth(),
                range = 0.0..length,
                tint = Theme.tally,
                label = t("Position in the video"),
                editing = { editing ->
                    if (!editing) {
                        val to = scrubbing
                        if (to != null) {
                            engine.seek(to)
                            scrub(null)
                        }
                    }
                },
            )
        }

        Row(horizontalArrangement = Arrangement.spacedBy(4.dp), verticalAlignment = Alignment.CenterVertically) {
            StageControl(if (playing) "pause.fill" else "play.fill", if (playing) t("Pause for everybody") else t("Play for everybody")) {
                engine.toggle()
            }
            StageControl("forward.end.fill", t("Next video")) { engine.skip(video.key) }
            // The exact position too, for whoever is checking two players
            // against each other, as the web carries it in data-position.
            KitText(
                if (video.live) t("Live") else if (length > 0) "${clockOf(shown)} / ${clockOf(length)}" else clockOf(shown),
                Modifier.padding(horizontal = 4.dp).semantics { stateDescription = "%.2f".format(player.position) },
                font = Font(11f, digits = true), color = Color.White.copy(alpha = 0.8f), lines = 1,
            )
            Spacer(Modifier.weight(1f))
            StageControl(
                if (player.muted || player.volume == 0.0) "speaker.slash.fill" else "speaker.wave.2.fill",
                if (player.muted) t("Unmute the video") else t("Mute the video"),
            ) { player.muted = !player.muted }
            MediaSlider(
                if (player.muted) 0.0 else player.volume,
                { level ->
                    player.muted = false
                    player.volume = level
                },
                Modifier.width(84.dp),
                step = 0.05,
                tint = Color.White,
                label = t("Video volume"),
            )
            StageControl("list.bullet.rectangle", t("Videos"), action = openPanel)
        }
    }
}

/** One round press over the video: white, or the chosen colour while what it turns on is on. */
@Composable
private fun StageControl(symbol: String, label: String, lit: Boolean = false, action: () -> Unit) {
    val press = rememberPress()
    val fill by animateColorAsState(
        if (lit) Theme.tally else Color.White.copy(alpha = if (press.lit) 0.16f else 0f),
        Motion.hover(), label = "fill",
    )
    Box(
        Modifier.size(34.dp).background(fill, CircleShape).pressable(press, CircleShape, true, Role.Button, label, onClick = action),
        contentAlignment = Alignment.Center,
    ) {
        SwappingSymbol(symbol, size = 13f, weight = Font.Semibold, tint = if (lit) Theme.tallyForeground else Color.White)
    }
}

/**
 * The few buttons said over the video (`StageButtonStyle`): capsules, the
 * chosen colour for the one that is the way on. Dark rather than glass, as
 * the kit's PictureButton is and for its reason: a video is drawn on a
 * surface a blur cannot sample.
 */
@Composable
internal fun StageButton(onClick: () -> Unit, modifier: Modifier = Modifier, prominent: Boolean = false, content: @Composable RowScope.() -> Unit) {
    val press = rememberPress()
    val shade by animateFloatAsState(if (press.lit) 0.64f else 0.52f, Motion.hover(), label = "shade")
    val scale by animateFloatAsState(if (press.pressed) 0.97f else 1f, Theme.settle(), label = "press")
    Row(
        modifier
            .graphicsLayer {
                scaleX = scale
                scaleY = scale
            }
            .heightIn(min = 32.dp)
            .background(if (prominent) Theme.tally else Color.Black.copy(alpha = shade), Capsule)
            .border(1.dp, Color.White.copy(alpha = if (prominent) 0f else 0.14f), Capsule)
            .pressable(press, Capsule, true, Role.Button, null, onClick = onClick)
            .padding(horizontal = 14.dp),
        horizontalArrangement = Arrangement.spacedBy(6.dp, Alignment.CenterHorizontally),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        KitInk(if (prominent) Theme.tallyForeground else Color.White) { KitFont(Font(12.5f, Font.Semibold)) { content() } }
    }
}

/** A line across the top of the stage that says something is happening (RoomView.swift `Notice`), dark over the video. */
@Composable
private fun StageNotice(text: String) {
    KitText(
        text,
        Modifier.background(Color.Black.copy(alpha = 0.52f), Capsule).border(1.dp, Color.White.copy(alpha = 0.14f), Capsule)
            .padding(horizontal = 14.dp, vertical = 8.dp),
        font = Font(13f, Font.Medium), color = Theme.foreground, lines = 1,
    )
}
