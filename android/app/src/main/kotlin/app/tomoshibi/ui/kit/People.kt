package app.tomoshibi.ui.kit

import android.graphics.BitmapFactory
import android.util.LruCache
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.FlowRowScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.compositionLocalOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.produceState
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.PathEffect
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.drawOutline
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.drawscope.inset
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.dp
import app.tomoshibi.basics.Theme
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import java.net.HttpURLConnection
import java.net.URL

// People and small labels (Kit+Settings.swift): the person's picture, the
// capsules that name what an account has, and rows that wrap.

/**
 * The account's picture where it has one, the initial in a circle where it
 * has none or the picture will not load (Lobby.tsx `Corner`, Account.tsx
 * `Face`; `AccountAvatar`). `address` is already resolved against the
 * deployment, with its counter (AppModel.avatarAddress).
 */
@Composable
fun AccountAvatar(name: String, address: String?, modifier: Modifier = Modifier, size: Dp = 32.dp) {
    Box(
        modifier
            .size(size)
            .clearAndSetSemantics {}
            .border(1.dp, Color.White.copy(alpha = 0.12f), CircleShape),
        contentAlignment = Alignment.Center,
    ) {
        Initial(name, size = size)
        if (address != null) RemoteImage(address, Modifier.fillMaxSize().clip(CircleShape))
    }
}

/**
 * A name's first letter on a colour of its own, the same colour for the same
 * name everywhere it appears (Lobby.swift `Initial`).
 */
@Composable
fun Initial(name: String, modifier: Modifier = Modifier, size: Dp = 32.dp) {
    Box(
        modifier
            .size(size)
            .clearAndSetSemantics {}
            .background(Initials.colour(name), CircleShape),
        contentAlignment = Alignment.Center,
    ) {
        KitText(Initials.letter(name), font = Font(size.value * 0.42f, Font.Semibold), color = Color.White, lines = 1)
    }
}

/** The rules [Initial] is drawn by. */
object Initials {
    private val palette = listOf(
        Color(0.369f, 0.361f, 0.902f), Color(0.180f, 0.545f, 0.604f),
        Color(0.604f, 0.361f, 0.784f), Color(0.204f, 0.471f, 0.965f),
        Color(0.776f, 0.420f, 0.247f), Color(0.247f, 0.596f, 0.388f),
    )

    /** The sum of the name's code points, as Swift sums its Unicode scalars, picks the colour. */
    fun colour(name: String): Color {
        var sum = 0
        name.codePoints().forEach { sum += it }
        return palette[Math.floorMod(sum, palette.size)]
    }

    fun letter(name: String): String =
        if (name.isEmpty()) "" else Monograms.firstGrapheme(name).uppercase()
}

/**
 * Fetches the pictures the kit draws from an address: the account's face, a
 * song's cover. The default asks the address directly; the app may hand in
 * its own (one that carries the deployment's cookie, say) through
 * [LocalImageLoader].
 */
fun interface ImageLoader {
    /** The picture at `address`, decoded to about `size`, or null when it will not load. */
    suspend fun load(address: String, size: IntSize): ImageBitmap?
}

val LocalImageLoader = compositionLocalOf<ImageLoader> { PlainImageLoader }

private object PlainImageLoader : ImageLoader {
    private val cache = LruCache<String, ImageBitmap>(48)

    override suspend fun load(address: String, size: IntSize): ImageBitmap? {
        cache.get(address)?.let { return it }
        return withContext(Dispatchers.IO) {
            runCatching {
                val connection = URL(address).openConnection() as HttpURLConnection
                connection.connectTimeout = 10_000
                connection.readTimeout = 15_000
                val bytes = try {
                    connection.inputStream.use { it.readBytes() }
                } finally {
                    connection.disconnect()
                }
                val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
                BitmapFactory.decodeByteArray(bytes, 0, bytes.size, bounds)
                var sample = 1
                val wanted = maxOf(1, maxOf(size.width, size.height))
                while (minOf(bounds.outWidth, bounds.outHeight) / (sample * 2) >= wanted) sample *= 2
                val options = BitmapFactory.Options().apply { inSampleSize = sample }
                BitmapFactory.decodeByteArray(bytes, 0, bytes.size, options)?.asImageBitmap()
            }.getOrNull()?.also { cache.put(address, it) }
        }
    }
}

/**
 * A picture from an address, faded in with Theme.settle when it arrives
 * (`AsyncImage` with `Transaction(animation: Theme.settle)`), and nothing at
 * all, so that what is under it shows, until then or if it never does.
 */
@Composable
fun RemoteImage(address: String, modifier: Modifier = Modifier, contentScale: ContentScale = ContentScale.Crop) {
    val loader = LocalImageLoader.current
    val picture by produceState<ImageBitmap?>(null, address, loader) { value = loader.load(address, IntSize(256, 256)) }
    val shown by animateFloatAsState(if (picture != null) 1f else 0f, Theme.settle(), label = "picture")
    picture?.let { Image(it, null, modifier.alpha(shown), contentScale = contentScale) }
}

/**
 * A capsule naming one thing an account has, or has not been given
 * (Account.tsx `Ability`; `Chip`): drawn dashed when it was withheld, so a
 * missing button in a call can be seen to have been a decision.
 */
@Composable
fun Chip(
    title: String,
    modifier: Modifier = Modifier,
    symbol: String? = null,
    withheld: Boolean = false,
    lit: Boolean = false,
    mono: Boolean = false,
) {
    val tally = Theme.tally
    val edge = if (withheld) Color.White.copy(alpha = 0.18f) else if (lit) tally.copy(alpha = 0.4f) else Color.White.copy(alpha = 0.08f)
    Row(
        modifier
            .background(if (withheld) Color.Transparent else if (lit) tally.copy(alpha = 0.14f) else Color.White.copy(alpha = 0.08f), Capsule)
            .drawBehind {
                val line = 1.dp.toPx()
                inset(line / 2) {
                    drawOutline(
                        Capsule.createOutline(size, layoutDirection, this),
                        edge,
                        style = Stroke(
                            width = line,
                            pathEffect = if (withheld) PathEffect.dashPathEffect(floatArrayOf(3.dp.toPx(), 3.dp.toPx())) else null,
                        ),
                    )
                }
            }
            .padding(horizontal = 9.dp, vertical = 5.dp),
        horizontalArrangement = Arrangement.spacedBy(5.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (symbol != null) {
            Symbol(
                symbol,
                size = 10.5f,
                weight = Font.Semibold,
                tint = if (lit) tally else if (withheld) Theme.muted.copy(alpha = Theme.muted.alpha * 0.7f) else Theme.muted,
            )
        }
        KitText(
            title,
            font = if (mono) Font(11.5f, mono = true) else Font(12f),
            color = if (withheld) Theme.muted else Theme.foreground,
            lines = 1,
        )
    }
}

/**
 * Things that wrap onto as many lines as they need, `spacing` apart both
 * ways (Swift's `FlowRow`, named apart from Compose's own, which this is
 * built on). A thing wider than the line is given the line, never more: one
 * too wide for a phone held upright pushed the whole page past the screen's
 * edge.
 */
@Composable
fun WrappingRow(modifier: Modifier = Modifier, spacing: Dp = 6.dp, content: @Composable FlowRowScope.() -> Unit) {
    FlowRow(
        modifier,
        horizontalArrangement = Arrangement.spacedBy(spacing),
        verticalArrangement = Arrangement.spacedBy(spacing),
        content = content,
    )
}
