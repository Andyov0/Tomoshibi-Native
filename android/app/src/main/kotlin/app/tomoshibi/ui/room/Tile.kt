package app.tomoshibi.ui.room

import androidx.camera.view.PreviewView
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.scaleIn
import androidx.compose.animation.scaleOut
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsHoveredAsState
import androidx.compose.foundation.hoverable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.dropShadow
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.shadow.Shadow
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.onClick
import androidx.compose.ui.semantics.onLongClick
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.DpOffset
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import app.tomoshibi.basics.Plan
import app.tomoshibi.basics.Theme
import app.tomoshibi.basics.t
import app.tomoshibi.media.RemoteVideo
import app.tomoshibi.model.CallModel
import app.tomoshibi.model.Surface
import app.tomoshibi.model.host.CrateRules
import app.tomoshibi.ui.kit.Capsule
import app.tomoshibi.ui.kit.Font
import app.tomoshibi.ui.kit.GlassButton
import app.tomoshibi.ui.kit.GlassButtonShape
import app.tomoshibi.ui.kit.KitText
import app.tomoshibi.ui.kit.PictureButton
import app.tomoshibi.ui.kit.Symbol
import app.tomoshibi.ui.call.FloatingReactions
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlin.math.min

// A picture and the frame every picture sits in (Tile.swift).

/** What a picture says about whoever is in it, beside the picture (components/room/Tile.tsx's props; `TileMarks`). */
data class TileMarks(
    val label: String,
    /** A name somebody proved with a passphrase: a dot beside it. */
    val proven: Boolean = false,
    /** Wearing a name somebody else in the room proved, without proving it (name.ts `impersonating`). */
    val unverified: Boolean = false,
    val speaking: Boolean = false,
    /** Their microphone is off, or they have none. */
    val muted: Boolean = false,
    /** Turned all the way down, or stopped at the server, by whoever is looking (hearing.ts `silenced`). */
    val silenced: Boolean = false,
    val hand: Boolean = false,
    /** The hand drawn at the start of the name rather than in the top left, where the room's own name and signal would cover it. */
    val handBesideName: Boolean = false,
    /** No name drawn, for a picture that names itself. */
    val unlabelled: Boolean = false,
)

/**
 * How the pictures in a call are drawn: a remote one through the core into a
 * TextureView, one's own through the camera's preview. A picture of the room
 * taken on a computer has neither the core nor a camera, and hands in
 * stand-ins of its own.
 */
class RoomPictures(
    val remote: @Composable (call: CallModel, sid: String, camera: Boolean, modifier: Modifier) -> Unit,
    val self: @Composable (call: CallModel, modifier: Modifier) -> Unit,
) {
    companion object {
        val live = RoomPictures(
            remote = { call, sid, camera, modifier -> RemoteVideo(call.call, sid, modifier, camera) },
            self = { call, modifier -> SelfView(call, modifier) },
        )
    }
}

val LocalRoomPictures = staticCompositionLocalOf { RoomPictures.live }

/**
 * The frame every picture appears in (components/room/Tile.tsx;
 * `TileFrame`): a camera, a shared screen, a person with their camera off and
 * a share that is not on the stage all sit in the same box with the same
 * marks, so the grid never jumps as cameras go on and off.
 *
 * A press puts it on the stage or takes it back off, and two in quick
 * succession fill the screen with it; the room tells the two apart, so that
 * the second press of a double one counts for the picture the first one
 * moved, wherever that picture has gone by then (RoomView `pressed`). A long
 * press opens what can be done to it, where it was pressed (`menu`).
 *
 * No mark in a corner saying that a press makes it larger. Every app drew
 * one under a mouse's pointer, and on Windows and the web it met the stage's
 * own controls in that corner however its showing was conditioned; the owner
 * had it taken away on every platform (2026-10-26), the whole picture being
 * what is pressed.
 */
@Composable
internal fun TileFrame(
    marks: TileMarks,
    press: () -> Unit,
    modifier: Modifier = Modifier,
    menu: ((Offset) -> Unit)? = null,
    compact: Boolean = false,
    /** On the stage, so a press sends it back to the grid. */
    selected: Boolean = false,
    /** Square while the picture fills the screen, whose own edges are the picture's. */
    radius: Dp = Theme.tileRadius,
    /**
     * Laid over another picture, as the card in a call of two is: lifted off
     * it by a shadow and a hairline edge, since both are the same grey with
     * nothing on them and the card otherwise vanished into the other.
     */
    floating: Boolean = false,
    /** What shows around a picture of another shape: the tile's grey in a grid, black when the picture fills the screen, as a player's bars are. */
    ground: Color = Theme.surface,
    /** What the marks keep clear of at the picture's edges: the rounded corners and the system's bars, when the picture fills a phone. */
    edge: PaddingValues = PaddingValues(0.dp),
    content: @Composable BoxScope.() -> Unit,
    overlay: @Composable BoxScope.(hovering: Boolean) -> Unit = {},
) {
    val shape = RoundedCornerShape(radius)
    val hover = remember { MutableInteractionSource() }
    val hovering by hover.collectIsHoveredAsState()
    val ring by animateFloatAsState(if (marks.speaking) 1f else 0f, Theme.settle(), label = "tally")
    val lift by animateFloatAsState(if (floating) 1f else 0f, Theme.settle(), label = "floating")
    val pressNow by rememberUpdatedState(press)
    val menuNow by rememberUpdatedState(menu)
    val label = if (selected) t("Show everybody") else t("Show {name} larger", mapOf("name" to marks.label))
    Box(
        modifier
            .dropShadow(shape, Shadow(radius = 18.dp, color = Color.Black, offset = DpOffset(0.dp, 8.dp), alpha = 0.45f * lift))
            .hoverable(hover)
            // A tap at once, with no wait to see whether a second follows:
            // the room counts the second itself. A long press says where the
            // finger was, for the menu to open beside it.
            .pointerInput(Unit) {
                detectTapGestures(
                    onLongPress = { at -> menuNow?.invoke(at) },
                    onTap = { pressNow() },
                )
            }
            .semantics {
                role = Role.Button
                contentDescription = label
                this.selected = selected
                onClick(label) {
                    pressNow()
                    true
                }
                if (menuNow != null) {
                    onLongClick(t("More")) {
                        menuNow?.invoke(Offset.Zero)
                        true
                    }
                }
            },
    ) {
        Box(Modifier.matchParentSize().background(ground, shape))
        Box(Modifier.matchParentSize().clip(shape), contentAlignment = Alignment.Center) { content() }
        AnimatedVisibility(
            !marks.unlabelled,
            Modifier.align(Alignment.BottomStart).padding(edge).padding(if (compact) 4.dp else 8.dp),
            enter = Theme.arrive, exit = Theme.depart,
        ) { NameLabel(marks, compact) }
        AnimatedVisibility(
            marks.hand && !marks.handBesideName,
            Modifier.align(Alignment.TopStart).padding(edge).padding(if (compact) 5.dp else 8.dp),
            enter = scaleIn(Theme.settle(), initialScale = 0.5f) + fadeIn(Theme.settle()),
            exit = scaleOut(Theme.leave(), targetScale = 0.8f) + fadeOut(Theme.leave()),
        ) { HandMark(compact) }
        overlay(hovering)
        Box(
            Modifier
                .matchParentSize()
                .border(1.dp, Color.White.copy(alpha = 0.14f * lift), shape),
        )
        // The tally around a speaker, held through a pause (useSteady).
        Box(
            Modifier
                .matchParentSize()
                .border(2.dp, Theme.tally.copy(alpha = ring), shape),
        )
    }
}

/**
 * The name at the foot of a picture, and what is true of it (`NameLabel`):
 * in the person's colour while they speak, a dot for a proven name, a badge
 * for one worn by somebody else, and one mark for having no sound from here,
 * where the reader's own doing wins: a microphone somebody else turned off is
 * nothing to act on, a person talking into silence that you caused is.
 */
@Composable
private fun NameLabel(marks: TileMarks, compact: Boolean) {
    // Light words on the dimming whatever the appearance: what is under them is a picture.
    val ink by animateColorAsState(if (marks.speaking) Theme.tally else Theme.onPicture, Theme.settle(), label = "name")
    Row(
        Modifier
            // Over the picture, a dimming rather than glass: one piece of
            // glass on every tile was worked out again from the moving video
            // beneath it on every frame, which is what Apple's guidance says
            // to limit, and a name on a picture is content. The dark fill is
            // what the guidance gives for legibility over rich content.
            .background(Color.Black.copy(alpha = 0.5f), Capsule)
            .border(0.5.dp, Color.White.copy(alpha = 0.1f), Capsule)
            .padding(horizontal = if (compact) 5.dp else 7.dp, vertical = if (compact) 2.dp else 3.5.dp),
        horizontalArrangement = Arrangement.spacedBy(if (compact) 3.dp else 5.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        AnimatedVisibility(
            marks.hand && marks.handBesideName,
            Modifier.offset(x = if (compact) (-3).dp else (-4).dp),
            enter = scaleIn(Theme.settle()) + fadeIn(Theme.settle()),
            exit = scaleOut(Theme.leave()) + fadeOut(Theme.leave()),
        ) { HandMark(compact = true, small = true) }
        KitText(marks.label, Modifier.weight(1f, fill = false), font = Font(if (compact) 10.5f else 12.5f, Font.Medium), color = ink, lines = 1)
        AnimatedVisibility(marks.proven, enter = fadeIn(Theme.settle()), exit = fadeOut(Theme.leave())) {
            Box(
                Modifier
                    .padding(horizontal = 1.dp)
                    .size(3.5.dp)
                    .background(Theme.muted, CircleShape)
                    .semantics { contentDescription = t("Only this person can use this name") },
            )
        }
        AnimatedVisibility(
            marks.unverified,
            enter = scaleIn(Theme.settle()) + fadeIn(Theme.settle()),
            exit = scaleOut(Theme.leave()) + fadeOut(Theme.leave()),
        ) {
            KitText(
                t("unverified"),
                Modifier
                    .background(Theme.danger, Capsule)
                    .padding(horizontal = 5.dp, vertical = 1.5.dp)
                    .semantics { contentDescription = t("Someone else has proved this name") },
                font = Font(if (compact) 8.5f else 9.5f, Font.Semibold),
                color = Theme.dangerForeground,
                lines = 1,
            )
        }
        AnimatedContent(
            when {
                marks.silenced -> 2
                marks.muted -> 1
                else -> 0
            },
            transitionSpec = {
                (scaleIn(Theme.settle()) + fadeIn(Theme.settle())) togetherWith (scaleOut(Theme.leave()) + fadeOut(Theme.leave()))
            },
            label = "sound",
        ) { mark ->
            when (mark) {
                2 -> Symbol("speaker.slash.fill", size = if (compact) 9f else 10.5f, weight = Font.Semibold, tint = Theme.danger,
                    contentDescription = t("Muted by you"))
                1 -> Symbol("mic.slash.fill", size = if (compact) 9f else 10.5f, weight = Font.Semibold, tint = Theme.muted,
                    contentDescription = t("microphone off"))
                else -> Box(Modifier.size(0.dp))
            }
        }
    }
}

/**
 * A raised hand, top left, away from the name and from the stage's own
 * controls (Reacting.tsx; `HandMark`). Said to a screen reader as it goes
 * up, because a hand nobody sees is a hand that was not put up; it is not a
 * button.
 */
@Composable
private fun HandMark(compact: Boolean, small: Boolean = false) {
    val side = if (small) 18.dp else if (compact) 22.dp else 28.dp
    Box(
        Modifier
            .then(if (small) Modifier else Modifier.dropShadow(CircleShape, Shadow(radius = 6.dp, offset = DpOffset(0.dp, 2.dp), alpha = 0.3f)))
            .size(side)
            .background(Theme.tally, CircleShape)
            .semantics {
                liveRegion = LiveRegionMode.Polite
                contentDescription = HAND
            },
        contentAlignment = Alignment.Center,
    ) {
        KitText(HAND, font = Font(if (small) 10f else if (compact) 12f else 15f), lines = 1, align = TextAlign.Center)
    }
}

/** The raised hand the web draws, U+270B. */
private const val HAND = "✋"

/**
 * Holds a flag on for a moment after it goes off (hooks/useSteady.ts;
 * `Steady`): speaking arrives as a flag a pause between two sentences turns
 * off, and drawn straight it makes the tally flicker through a single remark.
 * Rising is immediate; only the fall waits.
 */
@Composable
internal fun steady(value: Boolean, holdMillis: Long = 700): Boolean {
    var held by remember { mutableStateOf(false) }
    LaunchedEffect(value) {
        if (value) {
            held = true
        } else {
            delay(holdMillis)
            held = false
        }
    }
    return held || value
}

/**
 * A picture that belongs to somebody else: their camera, or their screen on
 * the stage (components/room/SurfaceTile.tsx; `SurfacePicture`). A camera
 * fills its tile when its shape is near the tile's, because a face is fine
 * cropped at the edges, and is shown whole on a blur of itself when it is
 * the other way round, which cropped it to its middle (Framing); a screen is
 * always shown whole, because cropping hides whatever was at the edge.
 */
@Composable
internal fun SurfacePicture(surface: Surface, call: CallModel, compact: Boolean = false) {
    val pictures = LocalRoomPictures.current
    Box(Modifier.fillMaxSize()) {
        AnimatedContent(
            surface.sid,
            Modifier.fillMaxSize(),
            transitionSpec = { fadeIn(Theme.settle()) togetherWith fadeOut(Theme.settle()) },
            contentKey = { it != null },
            label = "picture",
        ) { sid ->
            if (sid != null) {
                pictures.remote(call, sid, surface.kind == Surface.Kind.Camera, Modifier.fillMaxSize())
            } else {
                Avatar(surface.person.name, surface.person.id)
            }
        }
        // What they just reacted with, rising off every picture of theirs as
        // the web draws it on each (Room.tsx, Reacting.tsx).
        FloatingReactions(call, surface.person.id, compact)
    }
}

/** This participant's own camera, or their initials while it is off (`SelfPicture`). */
@Composable
internal fun SelfPicture(call: CallModel, compact: Boolean = false) {
    val pictures = LocalRoomPictures.current
    Box(Modifier.fillMaxSize()) {
        AnimatedContent(
            call.cameraOn,
            Modifier.fillMaxSize(),
            transitionSpec = { fadeIn(Theme.settle()) togetherWith fadeOut(Theme.settle()) },
            label = "self",
        ) { on ->
            if (on) pictures.self(call, Modifier.fillMaxSize()) else Avatar(call.localName, call.localIdentity)
        }
        // What this participant sent, over their own picture as everybody
        // else sees it there (the web draws reactions on every tile, its own
        // included).
        FloatingReactions(call, call.localIdentity, compact)
    }
}

/**
 * This participant's own camera, drawn from the capture as it runs
 * (`SelfView`): a CameraX preview, which mirrors a camera that faces its
 * owner as every camera preview does, a person expecting to see what a mirror
 * shows them. What is sent is not mirrored.
 *
 * Drawn on a TextureView rather than a SurfaceView ("compatible"), because a
 * surface of its own is not clipped by the tile's corners and does not move
 * with the tile as the stage changes.
 *
 * Not yet: the blurred frames while the background is blurred, which the
 * Apple apps draw instead of the session's preview (the preview shows the
 * room the call no longer sees). CameraCapture has no way to hand its frames
 * out to be drawn yet.
 */
@Composable
internal fun SelfView(call: CallModel, modifier: Modifier = Modifier) {
    val view = remember { mutableStateOf<PreviewView?>(null) }
    AndroidView(
        factory = { context ->
            PreviewView(context).apply {
                implementationMode = PreviewView.ImplementationMode.COMPATIBLE
                scaleType = PreviewView.ScaleType.FILL_CENTER
            }.also { view.value = it }
        },
        modifier = modifier,
    )
    val shown = view.value
    LaunchedEffect(shown) {
        if (shown != null) call.camera.showPreview(shown.surfaceProvider)
    }
    DisposableEffect(call) {
        onDispose { call.scope.launch { runCatching { call.camera.showPreview(null) } } }
    }
}

/**
 * Stands in for a screen share that is not on the stage
 * (components/room/ShareCard.tsx; `ShareCardFace`). A desktop scaled into a
 * grid cell is unreadable, so subscribing to it would spend the bandwidth and
 * deliver nothing: the card says who is sharing, and the whole card takes the
 * press that makes it big, rather than a button on a surface that already
 * answers one.
 *
 * Flat Theme.surfaceHigh, a step lighter than a person's tile, so a share
 * standing in for its picture is told from a camera that is off; the
 * person's colour is on the Watch button alone. It was a grey wash with a
 * glow of that colour in the middle (the owner, 2026-10-20: fewer
 * gradients), and the same in Tile.swift.
 */
@Composable
internal fun ShareCardFace(name: String, compact: Boolean = false) {
    val tally = Theme.tally
    Box(
        Modifier
            .fillMaxSize()
            .background(Theme.surfaceHigh),
        contentAlignment = Alignment.Center,
    ) {
        Column(
            Modifier.padding(horizontal = 12.dp),
            verticalArrangement = Arrangement.spacedBy(if (compact) 4.dp else 10.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            Box(
                Modifier
                    .size(if (compact) 30.dp else 48.dp)
                    .background(Theme.wash.copy(alpha = 0.08f), CircleShape),
                contentAlignment = Alignment.Center,
            ) {
                Symbol("rectangle.inset.filled.and.person.filled", size = if (compact) 15f else 22f, weight = Font.Medium,
                    tint = Theme.foreground.copy(alpha = 0.9f))
            }
            KitText(
                t("{name} is sharing", mapOf("name" to name)),
                font = Font(if (compact) 11f else 13.5f, Font.Semibold),
                lines = if (compact) 1 else 2,
                align = TextAlign.Center,
            )
            if (!compact) {
                Row(
                    Modifier
                        .height(26.dp)
                        .background(tally, Capsule)
                        .padding(horizontal = 12.dp),
                    horizontalArrangement = Arrangement.spacedBy(5.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Symbol("play.fill", size = 9f, weight = Font.Bold, tint = Theme.tallyForeground)
                    KitText(t("Watch"), font = Font(12f, Font.Semibold), color = Theme.tallyForeground, lines = 1)
                }
            }
        }
    }
}

/**
 * This participant's own shared screen (`MyShareFace`). There is no preview
 * of the capture to draw, so it says so, with the way to stop: a picture of
 * your own screen inside your own screen is a hall of mirrors anyway, and
 * what the sharer needs from this place is to know it is going out. The
 * same flat ground as [ShareCardFace]; the plate behind the symbol carries
 * the colour.
 */
@Composable
internal fun MyShareFace(compact: Boolean = false, stop: (() -> Unit)?) {
    val tally = Theme.tally
    Box(
        Modifier
            .fillMaxSize()
            .background(Theme.surfaceHigh),
        contentAlignment = Alignment.Center,
    ) {
        Column(
            Modifier.padding(horizontal = 12.dp),
            verticalArrangement = Arrangement.spacedBy(if (compact) 4.dp else 12.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            Box(
                Modifier
                    .size(if (compact) 34.dp else 64.dp)
                    .background(tally.copy(alpha = 0.18f), CircleShape),
                contentAlignment = Alignment.Center,
            ) { Symbol("rectangle.on.rectangle", size = if (compact) 14f else 24f, weight = Font.Semibold, tint = tally) }
            KitText(
                t("You are sharing your screen"),
                font = Font(if (compact) 11f else 14f, Font.Semibold),
                lines = if (compact) 1 else 2,
                align = TextAlign.Center,
            )
            if (stop != null && !compact) {
                GlassButton(stop, shape = GlassButtonShape.Capsule(30.dp)) { KitText(t("Stop sharing"), lines = 1) }
            }
        }
    }
}

/**
 * What can be done with whatever is on the stage (StageControls.tsx;
 * `StageControls`): the same person's other picture, when they have one, and
 * filling the screen. On a touch screen shown whenever the bar is, since
 * there is no pointer to hover. Its buttons take their own presses: on the
 * web a press on one carried on to the picture beneath, and "Their screen"
 * pinned the other picture only for the picture's own press to pin this one
 * straight back.
 */
@Composable
internal fun StageControls(other: Surface.Kind?, filled: Boolean, switchTo: () -> Unit, fill: () -> Unit, modifier: Modifier = Modifier) {
    Row(modifier, horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
        AnimatedVisibility(other != null, enter = Theme.arrive, exit = Theme.depart) {
            val screen = other == Surface.Kind.Screen
            PictureButton(switchTo, words = true) {
                Symbol(if (screen) "rectangle.on.rectangle" else "person.fill", size = 11.5f, weight = Font.Semibold)
                KitText(if (screen) t("Their screen") else t("Their camera"), lines = 1)
            }
        }
        PictureButton(fill, label = if (filled) t("Leave fullscreen") else t("Fill the screen")) {
            Symbol(if (filled) "arrow.down.right.and.arrow.up.left" else "arrow.up.left.and.arrow.down.right", size = 12.5f, weight = Font.Bold)
        }
    }
}

/**
 * A person with no picture: their initials on a colour of their own, steady
 * for the same identity, as the web's marble avatar is steady for its seed
 * (`Avatar`).
 */
@Composable
internal fun Avatar(name: String, identity: String) {
    BoxWithConstraints(Modifier.fillMaxSize().clearAndSetSemantics {}, contentAlignment = Alignment.Center) {
        val side = min(maxWidth.value, maxHeight.value) * 0.36f
        Box(Modifier.size(side.dp).background(Avatars.colour(identity), CircleShape), contentAlignment = Alignment.Center) {
            KitText(Avatars.initials(name), font = Font(side * 0.38f, Font.Semibold), lines = 1)
        }
    }
}

/** The rules an [Avatar] is drawn by, the Swift avatar's exactly. */
internal object Avatars {
    /** Warm hues only, close to the palette in web/src/live/avatar.ts. */
    private val palette = listOf(
        Color(0.55f, 0.33f, 0.24f), Color(0.42f, 0.36f, 0.27f),
        Color(0.33f, 0.38f, 0.33f), Color(0.48f, 0.30f, 0.33f),
        Color(0.36f, 0.33f, 0.40f), Color(0.50f, 0.42f, 0.25f),
    )

    /** FNV-1a over the identity's UTF-8, in 64-bit unsigned arithmetic as Swift's `&*` wraps it. */
    fun colour(identity: String): Color {
        var hash = 1469598103934665603UL
        for (byte in identity.encodeToByteArray()) hash = (hash xor byte.toUByte().toULong()) * 1099511628211UL
        return palette[(hash % palette.size.toULong()).toInt()]
    }

    /** The first letters of the first two words, in capitals; a question mark for a name with none. */
    fun initials(name: String): String {
        val words = name.split(' ').filter { it.isNotEmpty() }
        val letters = words.take(2).map { it.substring(0, it.offsetByCodePoints(0, 1)) }
        return if (letters.isEmpty()) "?" else letters.joinToString("").uppercase()
    }
}
