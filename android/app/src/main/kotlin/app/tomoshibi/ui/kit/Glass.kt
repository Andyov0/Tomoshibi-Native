package app.tomoshibi.ui.kit

import android.os.Build
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.AnimatedVisibilityScope
import androidx.compose.animation.EnterExitState
import androidx.compose.animation.ExitTransition
import androidx.compose.animation.scaleIn
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.State
import androidx.compose.runtime.compositionLocalOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.dropShadow
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ColorMatrix
import androidx.compose.ui.graphics.ColorFilter
import androidx.compose.ui.graphics.CompositingStrategy
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.graphics.TransformOrigin
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.shadow.Shadow
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.DpOffset
import androidx.compose.ui.unit.dp
import app.tomoshibi.basics.Theme
import dev.chrisbanes.haze.HazeInput
import dev.chrisbanes.haze.HazeState
import dev.chrisbanes.haze.blur.HazeBlurStyle
import dev.chrisbanes.haze.blur.HazeColorEffect
import dev.chrisbanes.haze.blur.hazeBlur
import dev.chrisbanes.haze.hazeSource
import dev.chrisbanes.haze.rememberHazeState

// Liquid Glass on Android (Glass.swift).
//
// The owner asked for the apps to look like Apple's own, which on iOS 26
// means Liquid Glass: controls and panels as glass floating over the
// content, the content blurred and lit through them. Android draws the
// nearest thing it has: what is behind a pane, captured and blurred by a
// RenderEffect (Haze), lifted a little and edged in light, over a soft
// shadow. Android before 12 has no RenderEffect, and there the same shapes
// are a translucent fill of the same dark tone: the nearest that system
// comes, as the thin material is the nearest iOS 17 and 18 come for the
// Apple apps.
//
// Glass is kept to the controls and navigation, as Apple's guidance says
// (Human Interface Guidelines, Materials: "Don't use Liquid Glass in the
// content layer"; "Use Liquid Glass effects sparingly"), and as the owner
// found with a phone hot in a call (2026-10-17): every blurred surface is
// the picture under it captured and worked out again on every frame. A part
// of a screen marked as content ([ContentLayer]) draws every surface the kit
// draws inside it as a plain one (a light translucent fill and a hairline,
// no blur and no shadow); what floats over the content (the call's bar, a
// panel's shell, a sheet) is glass, and a shell's insides are content again,
// so nothing is glass on glass.

/** The debug switches Glass.swift reads from the environment. */
object GlassSupport {
    /** TOMOSHIBI_NO_GLASS: the stand-in drawn even where blur is available. */
    var forcedFallback by mutableStateOf(false)

    /** TOMOSHIBI_ALL_GLASS: glass in the content layer too, to set the two side by side. */
    var everywhere by mutableStateOf(false)

    /** Whether this system can blur what is behind a view (RenderEffect, Android 12). */
    val blurs: Boolean get() = Build.VERSION.SDK_INT >= Build.VERSION_CODES.S && !forcedFallback
}

/** Whether this part of the screen is content: SwiftUI's `inContentLayer`. */
val LocalContentLayer = compositionLocalOf { false }

/**
 * What the glass on this screen refracts: the content marked with
 * [glassSource]. Null where nothing is, and the glass is then the stand-in.
 */
val LocalGlassBackdrop = compositionLocalOf<HazeState?> { null }

/**
 * Marks this part of the screen as content: surfaces inside it are plain
 * (`.contentLayer()`). A composable rather than a modifier: Compose hands
 * values down to what is inside through the composition, which a modifier
 * cannot reach.
 */
@Composable
fun ContentLayer(content: @Composable () -> Unit) {
    CompositionLocalProvider(LocalContentLayer provides true, content = content)
}

/**
 * A control floating over content inside a content layer (a button over a
 * picture), which is glass again (`.controlLayer()`).
 */
@Composable
fun ControlLayer(content: @Composable () -> Unit) {
    CompositionLocalProvider(LocalContentLayer provides false, content = content)
}

/**
 * A screen whose glass refracts what is drawn on it: the content below the
 * controls marks itself with [glassSource], and every glass surface inside
 * blurs it. One per screen; a glass surface with none above it is drawn as
 * the stand-in.
 */
@Composable
fun GlassBackdrop(content: @Composable () -> Unit) {
    val state = rememberHazeState()
    CompositionLocalProvider(LocalGlassBackdrop provides state, content = content)
}

/**
 * The content the screen's glass is over: the call's pictures, the evening
 * room, a page a sheet slides over. `zIndex` orders several of them, the one
 * drawn over the others highest.
 *
 * Glass blurs the sources under it that it is not inside of: a pane cannot
 * blur the very content it is drawn in, and glass inside a source sees only
 * the sources ordered below that one. So mark what is looked through (the
 * room, each picture) and put the controls beside it in a box, not inside it:
 * a page arrow inside the picture it floats over blurs the room behind the
 * picture instead. A popover, in a window of its own, is inside nothing and
 * sees every source under it.
 */
@Composable
fun Modifier.glassSource(zIndex: Float = 0f): Modifier {
    val state = LocalGlassBackdrop.current ?: return this
    return hazeSource(state, zIndex)
}

/**
 * Glass in `shape`, tinted where it says something (the primary action,
 * leaving); a plain surface instead inside a [ContentLayer]
 * (`.glassSurface(_:tint:interactive:)`). The content is clipped to the
 * shape; for children that reach past it, [GlassBacking].
 */
@Suppress("UNUSED_PARAMETER")
@Composable
fun Modifier.glassSurface(shape: Shape, tint: Color? = null, interactive: Boolean = false): Modifier {
    // `interactive` is the system's own answer to a touch on iOS 26 glass,
    // which has nothing like it here: the kit's buttons answer a press
    // themselves, as the Apple apps' do on the systems before glass.
    if (LocalContentLayer.current && !GlassSupport.everywhere) return plainSurface(shape, tint)
    return liquidGlass(shape, tint, LocalGlassBackdrop.current)
}

/** A larger pane of glass: a sidebar, a sheet, a panel beside a call. */
@Composable
fun Modifier.glassPane(cornerRadius: Dp = 22.dp): Modifier = glassSurface(ContinuousShape(cornerRadius))

/**
 * A panel, sheet or menu floating over the content: glass itself, and
 * content inside, so that nothing in it is glass on glass ("avoid ...
 * layering Liquid Glass elements on top of each other", Adopting Liquid
 * Glass). Glass wherever it is put, even inside a content layer
 * (`.glassShell(cornerRadius:)`).
 */
@Composable
fun GlassShell(
    modifier: Modifier = Modifier,
    cornerRadius: Dp = 22.dp,
    content: @Composable BoxScope.() -> Unit,
) {
    val shape = ContinuousShape(cornerRadius)
    Box(modifier.liquidGlass(shape, null, LocalGlassBackdrop.current)) {
        CompositionLocalProvider(LocalContentLayer provides true) { content() }
    }
}

/**
 * Glass in `shape` drawn behind the content rather than around it, for a
 * surface whose children reach past its edge (`.glassBacking(_:)`). On iOS
 * this is what lets a popover hanging off the bar be pressed; on Android a
 * child outside its parent's bounds is drawn and pressed either way, and
 * what this keeps is the children unclipped.
 */
@Composable
fun GlassBacking(shape: Shape, modifier: Modifier = Modifier, content: @Composable BoxScope.() -> Unit) {
    Box(modifier) {
        Box(Modifier.matchParentSize().glassSurface(shape))
        content()
    }
}

/**
 * Several pieces of glass that should merge and morph as one where the
 * system can (`GlassGroup`, iOS 26's `GlassEffectContainer`). Nothing on
 * Android merges glass, so it is the content as it is, kept so that the
 * screens port line for line.
 */
@Suppress("UNUSED_PARAMETER")
@Composable
fun GlassGroup(spacing: Dp = 8.dp, content: @Composable () -> Unit) = content()

/** A content-layer surface: what [glassSurface] draws inside [ContentLayer]. */
fun Modifier.plainSurface(shape: Shape, tint: Color? = null): Modifier {
    val filled = background(tint ?: PlainFill, shape)
    return if (tint == null) filled.border(1.dp, PlainEdge, shape) else filled
}

private val PlainFill = Color.White.copy(alpha = 0.07f)
private val PlainEdge = Color.White.copy(alpha = 0.09f)

/**
 * The glass itself. Over a source: the content behind, blurred, lifted in
 * saturation as Apple's materials lift it (the colour of a picture comes
 * through a pane rather than turning grey), shaded toward the call's own
 * grey so that words on it read over a white slide; under no source, or
 * before Android 12, the stand-in fill. Where it says something (`tint`), it
 * is filled with the colour, as Glass.swift found on macOS 27: a tint alone
 * is barely there, and dark words on it could not be read.
 */
internal fun Modifier.liquidGlass(shape: Shape, tint: Color?, backdrop: HazeState?): Modifier {
    val blurred = backdrop != null && GlassSupport.blurs
    return this
        .dropShadow(shape, GlassShadow)
        .clip(shape)
        // Under the blur, so a pane over nothing it may sample (inside the
        // content it is drawn on) is still a pane.
        .background(StandIn)
        .then(if (blurred && backdrop != null) Modifier.hazeBlur(HazeInput.Sources(backdrop), GlassBlur) else Modifier)
        .then(if (tint != null) Modifier.background(tint) else Modifier)
        .border(1.dp, if (tint == null) GlassRim else GlassRimLit, shape)
}

/**
 * The stand-in for the blur: the system's dark ultra-thin material over the
 * call reads as the call's grey at about four fifths.
 */
private val StandIn = Theme.surface.copy(alpha = 0.80f)

/** Glass.swift's shadow for the glass before iOS 26: 28 per cent black, 14 soft, 6 down. */
private val GlassShadow = Shadow(radius = 14.dp, color = Color.Black, offset = DpOffset(0.dp, 6.dp), alpha = 0.28f)

/**
 * The edge of the glass, catching the light from above: brighter along the
 * top than the bottom, as Liquid Glass's edge is, and as bright on average
 * as the stand-in's sixteen per cent hairline (twenty-eight where tinted).
 */
private val GlassRim = Brush.verticalGradient(listOf(Color.White.copy(alpha = 0.26f), Color.White.copy(alpha = 0.07f)))
private val GlassRimLit = Brush.verticalGradient(listOf(Color.White.copy(alpha = 0.40f), Color.White.copy(alpha = 0.16f)))

private val Saturate = ColorMatrix().apply { setToSaturation(1.5f) }

private val GlassBlur = HazeBlurStyle {
    blurRadius(22.dp)
    // Apple's glass has no grain.
    noiseFactor(0f)
    backgroundColor(Theme.background)
    colorEffects(
        listOf(
            HazeColorEffect.colorFilter(ColorFilter.colorMatrix(Saturate)),
            HazeColorEffect.tint(Theme.surface.copy(alpha = 0.34f)),
            HazeColorEffect.tint(Color.White.copy(alpha = 0.05f)),
        ),
    )
    fallbackColorEffect(HazeColorEffect.tint(StandIn))
}

/**
 * Faded and scaled, for something wearing the glass's shadow. A layer made
 * translucent the ordinary way is drawn offscreen at its own size, which cut
 * the shadow off square at its edge: a hard grey square round each paging
 * arrow at sixty per cent, seen on an Android 15 emulator with the blur off
 * as well as on. Here the alpha is laid on each thing drawn instead.
 */
internal fun Modifier.glassLayer(alpha: () -> Float, scale: () -> Float = { 1f }, origin: () -> TransformOrigin = { TransformOrigin.Center }) =
    graphicsLayer {
        val s = scale()
        this.alpha = alpha()
        scaleX = s
        scaleY = s
        transformOrigin = origin()
        compositingStrategy = CompositingStrategy.ModulateAlpha
    }

/**
 * The fade of a piece of glass arriving or leaving inside `AnimatedVisibility`,
 * with Theme's two curves, to lay on with [glassLayer]: `fadeIn` would draw it
 * offscreen for as long as it lasts and cut its shadow off at its edge.
 */
@Composable
internal fun AnimatedVisibilityScope.glassFade(): State<Float> = transition.animateFloat(
    transitionSpec = { if (targetState == EnterExitState.Visible) Theme.settle() else Theme.leave() },
    label = "fade",
) { if (it == EnterExitState.Visible) 1f else 0f }

/**
 * Glass arriving and leaving as every view does (`.transition(Theme.arrive)`):
 * grown in from 97 per cent with Theme.settle, faded in, faded out with
 * Theme.leave. [Appear] with Theme.arrive does the same for anything else;
 * this is for what wears the glass's shadow, which that fade would cut off
 * square at its edge while it lasted.
 */
@Composable
fun GlassAppear(visible: Boolean, modifier: Modifier = Modifier, content: @Composable () -> Unit) {
    AnimatedVisibility(visible, modifier, enter = scaleIn(Theme.settle(), initialScale = 0.97f), exit = ExitTransition.None) {
        val fade by glassFade()
        Box(Modifier.glassLayer({ fade })) { content() }
    }
}

/** The shape of a [GlassButton]: a circle around a symbol, a capsule around words. */
sealed interface GlassButtonShape {
    data class Circle(val size: Dp = 44.dp) : GlassButtonShape
    data class Capsule(val height: Dp = 36.dp) : GlassButtonShape
}

/**
 * A button made of glass (`GlassButtonStyle`, `.glassCircle` and
 * `.glassCapsule`): `prominent` wears the person's chosen colour,
 * `destructive` is red, for leaving and nothing else. Pressed, it gives a
 * little, and inside a content layer brightens as well, as the plain
 * surfaces of the Apple apps do. `label` is what a screen reader says for a
 * button of a symbol alone.
 */
@Composable
fun GlassButton(
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    shape: GlassButtonShape = GlassButtonShape.Capsule(),
    prominent: Boolean = false,
    destructive: Boolean = false,
    enabled: Boolean = true,
    label: String? = null,
    spacing: Dp = 6.dp,
    content: @Composable RowScope.() -> Unit,
) {
    val on = enabled && LocalEnabled.current
    val inContent = LocalContentLayer.current
    val tint = if (destructive) Theme.danger else if (prominent) Theme.tally else null
    val ink = if (destructive) Theme.dangerForeground else if (prominent) Theme.tallyForeground else Theme.foreground
    val press = rememberPress()
    val scale by animateFloatAsState(if (press.pressed) 0.96f else 1f, Theme.settle(), label = "press")
    val light by animateFloatAsState(if (press.pressed && inContent) 0.06f else 0f, Theme.settle(), label = "light")
    val alpha by animateFloatAsState(if (on) 1f else 0.45f, Theme.settle(), label = "enabled")
    val outline = when (shape) {
        is GlassButtonShape.Circle -> CircleShape
        is GlassButtonShape.Capsule -> Capsule
    }
    val sized = when (shape) {
        is GlassButtonShape.Circle -> Modifier.size(shape.size)
        is GlassButtonShape.Capsule -> Modifier.heightIn(min = shape.height)
    }
    Row(
        modifier
            .glassLayer(alpha = { alpha }, scale = { scale })
            .then(sized)
            .glassSurface(outline, tint, interactive = true)
            .brightened(outline) { light }
            .pressable(press, outline, on, Role.Button, label, onClick = onClick)
            .then(if (shape is GlassButtonShape.Capsule) Modifier.padding(horizontal = shape.height * 0.45f) else Modifier),
        horizontalArrangement = Arrangement.spacedBy(spacing, Alignment.CenterHorizontally),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        KitInk(ink) {
            KitFont(Font(13.5f, Font.Semibold)) { content() }
        }
    }
}

/**
 * What the screens before a call stand on when there is no picture yet: the
 * call's own ground, flat (`Backdrop`).
 *
 * It was an evening room, a dark blue wash with three coloured glows in it,
 * on the argument that glass needs something under it to be glass. The
 * owner found the gradients cheap (2026-10-20) and asked for fewer of them;
 * the glow behind the sign-in card and the lobby was the largest. The
 * darkest colour of the palette is what the call itself stands on, so the
 * screens before it and the call read as one place, and the panes over it
 * are told apart by their own fill and edge rather than by what they blur.
 * Still mark it with [glassSource]: the glass over it then shades it as it
 * shades a picture, which is the same tone the stand-in fill gives.
 */
@Composable
fun Backdrop(modifier: Modifier = Modifier) {
    Box(modifier.clearAndSetSemantics {}.background(Theme.background))
}
