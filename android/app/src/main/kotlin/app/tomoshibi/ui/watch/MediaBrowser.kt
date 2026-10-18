package app.tomoshibi.ui.watch

import android.graphics.BitmapFactory
import android.util.LruCache
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.dp
import app.tomoshibi.basics.Theme
import app.tomoshibi.basics.t
import app.tomoshibi.feature.watch.WatchEngine
import app.tomoshibi.feature.watch.clockOf
import app.tomoshibi.ui.kit.Font
import app.tomoshibi.ui.kit.ImageLoader
import app.tomoshibi.ui.kit.KitText
import app.tomoshibi.ui.kit.LocalImageLoader
import app.tomoshibi.ui.kit.MediaArtwork
import app.tomoshibi.ui.kit.MediaButton
import app.tomoshibi.ui.kit.MediaEmpty
import app.tomoshibi.ui.kit.MediaField
import app.tomoshibi.ui.kit.MediaIconButton
import app.tomoshibi.ui.kit.MediaRow
import app.tomoshibi.ui.kit.MediaRule
import app.tomoshibi.ui.kit.MediaSpinner
import app.tomoshibi.ui.kit.RemoteImage
import app.tomoshibi.ui.kit.SegmentedChoice
import app.tomoshibi.ui.kit.Symbol
import app.tomoshibi.ui.kit.rounded
import java.net.HttpURLConnection
import java.net.URL
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.tomoshibi_protocol.MediaItem
import uniffi.tomoshibi_protocol.MediaServer
import uniffi.tomoshibi_protocol.watchMediaLink

/**
 * The deployment's media servers, browsed: their libraries, what is in them,
 * and a search, for somebody given them choosing what everybody will watch
 * (MediaBrowserView.swift; MediaBrowser.tsx).
 *
 * Choosing something hands its link to the show the way a pasted one is, so
 * it is read here under this person's grant even in somebody else's show: a
 * film from a media server is relayed for everybody on the chooser's grant,
 * and a link the holder read for them would be refused as somebody else's.
 */
@Composable
internal fun MediaBrowser(engine: WatchEngine, servers: List<MediaServer>, chose: () -> Unit) {
    var server by remember { mutableStateOf(servers.firstOrNull()?.key ?: "") }
    /** Where in the server: a trail of folders, the first being the server's libraries. */
    var trail by remember { mutableStateOf<List<Pair<String, String>>>(emptyList()) }
    var items by remember { mutableStateOf<List<MediaItem>?>(null) }
    var loading by remember { mutableStateOf(false) }
    var failed by remember { mutableStateOf(false) }
    var words by remember { mutableStateOf("") }
    var searched by remember { mutableStateOf<String?>(null) }
    var adding by remember { mutableStateOf<String?>(null) }
    val scope = rememberCoroutineScope()

    LaunchedEffect(server, trail.lastOrNull()?.first, searched == null) {
        if (server.isEmpty() || searched != null) return@LaunchedEffect
        loading = true
        failed = false
        val found = try {
            engine.deployment.browseMedia(server, trail.lastOrNull()?.first)
        } catch (fault: CancellationException) {
            throw fault
        } catch (_: Exception) {
            null
        }
        items = found
        failed = found == null
        loading = false
    }

    fun search() {
        val asked = words.trim()
        if (asked.isEmpty()) {
            searched = null
            return
        }
        loading = true
        failed = false
        scope.launch {
            val found = try {
                engine.deployment.searchMedia(server, asked)
            } catch (fault: CancellationException) {
                throw fault
            } catch (_: Exception) {
                null
            }
            items = found
            failed = found == null
            searched = asked
            loading = false
        }
    }

    fun open(item: MediaItem) {
        if (item.playable) {
            adding = item.id
            engine.scope.launch {
                val added = try {
                    engine.add(watchMediaLink(server, item.id))
                } finally {
                    adding = null
                }
                if (added) chose()
            }
        } else if (item.folder) {
            searched = null
            trail = trail + (item.id to item.name)
        }
    }

    fun back() {
        if (searched != null) {
            searched = null
            words = ""
        } else if (trail.isNotEmpty()) {
            trail = trail.dropLast(1)
        }
    }

    Column(Modifier.fillMaxSize()) {
        Column(Modifier.padding(12.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
            if (servers.size > 1) {
                SegmentedChoice(servers.map { it.key to it.name }, server, { chosen ->
                    server = chosen
                    trail = emptyList()
                    searched = null
                }, height = 28.dp)
            }
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
                MediaField(t("Search films and series"), words, { words = it }, ::search, Modifier.weight(1f), symbol = "magnifyingglass", limit = 100)
                MediaIconButton("arrow.right", t("Search"), ::search, size = 34.dp, prominent = words.isNotBlank())
            }
            AnimatedVisibility(trail.isNotEmpty() || searched != null, enter = Theme.arrive, exit = Theme.depart) {
                MediaButton(::back, quiet = true, height = 26.dp) {
                    Symbol("chevron.left")
                    KitText(searched?.let { t("Results for {words}", mapOf("words" to it)) } ?: trail.lastOrNull()?.second ?: "", lines = 1)
                }
            }
        }
        MediaRule()

        AnimatedContent(
            when {
                loading -> "loading"
                failed -> "failed"
                items?.isEmpty() == true -> "empty"
                else -> "items"
            },
            Modifier.weight(1f).fillMaxWidth(),
            transitionSpec = { (Theme.arrive togetherWith fadeOut(Theme.leave())) },
            contentAlignment = Alignment.TopCenter,
            label = "browse",
        ) { face ->
            when (face) {
                "loading" -> Box(Modifier.fillMaxWidth().padding(vertical = 40.dp), contentAlignment = Alignment.Center) { MediaSpinner(size = 20f) }
                "failed" -> MediaEmpty("externaldrive.badge.exclamationmark", t("The media server did not answer. Try again."))
                "empty" -> MediaEmpty("magnifyingglass", t("Nothing found."))
                else -> CompositionLocalProvider(LocalImageLoader provides remember(engine) { PosterLoader(engine) }) {
                    LazyColumn(Modifier.fillMaxSize(), contentPadding = PaddingValues(8.dp), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                        items(items.orEmpty(), key = { it.id }) { item ->
                            ItemRow(item, adding == item.id, { open(item) }, Modifier.animateItem(Theme.settle(), Theme.settle(), Theme.leave()))
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun ItemRow(item: MediaItem, adding: Boolean, open: () -> Unit, modifier: Modifier = Modifier) {
    val opens = item.playable || item.folder
    MediaRow(
        modifier
            .alpha(if (opens) 1f else 0.5f)
            .semantics { contentDescription = if (item.playable) t("Watch {title}", mapOf("title" to item.name)) else item.name },
        onClick = if (opens) open else null,
    ) { lit ->
        Row(Modifier.weight(1f), horizontalArrangement = Arrangement.spacedBy(10.dp), verticalAlignment = Alignment.CenterVertically) {
            Box(Modifier.size(34.dp, 50.dp).clip(rounded(6f))) {
                MediaArtwork("", Modifier.fillMaxSize(), symbol = if (item.folder) "folder.fill" else "play.fill", corner = 6f)
                // A media server's poster, which the deployment fetches with
                // its account and hands only to somebody whose session may
                // browse: so it is asked for with this app's session.
                if (item.image.isNotEmpty()) RemoteImage(item.image, Modifier.fillMaxSize())
            }
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                KitText(item.name, font = Font(13f, Font.Medium), color = Theme.foreground, lines = 1)
                // Nothing at all for a folder, which has no year or length:
                // an empty line under its name held the name off centre.
                val detail = listOfNotNull(if (item.year > 0u) item.year.toString() else null, if (item.duration > 0) clockOf(item.duration) else null)
                    .joinToString(" · ")
                if (detail.isNotEmpty()) KitText(detail, font = Font(11f), color = Theme.muted, lines = 1)
            }
            AnimatedContent(
                if (adding) 1 else if (lit && opens) 2 else 0,
                Modifier.width(20.dp),
                transitionSpec = { Theme.arrive togetherWith Theme.depart },
                contentAlignment = Alignment.CenterEnd,
                label = "row",
            ) { face ->
                when (face) {
                    1 -> MediaSpinner(size = 14f)
                    2 -> Symbol(
                        if (item.folder) "chevron.right" else "play.circle.fill",
                        size = if (item.folder) 12f else 17f, weight = Font.Semibold,
                        tint = if (item.folder) Theme.muted else Theme.tally,
                    )
                    else -> {}
                }
            }
        }
    }
}

/**
 * Posters asked for with the app's session cookie, which the platform's own
 * fetching would not send (MediaBrowserView.swift `Poster`): the deployment
 * hands a media server's picture only to a session that may browse it.
 */
private class PosterLoader(private val engine: WatchEngine) : ImageLoader {
    override suspend fun load(address: String, size: IntSize): ImageBitmap? {
        CACHE.get(address)?.let { return it }
        val cookie = try {
            engine.deployment.cookieHeader()
        } catch (_: Exception) {
            null
        }
        return withContext(Dispatchers.IO) {
            runCatching {
                val connection = URL(address).openConnection() as HttpURLConnection
                connection.connectTimeout = 10_000
                connection.readTimeout = 15_000
                if (cookie != null) connection.setRequestProperty("Cookie", cookie)
                val bytes = try {
                    if (connection.responseCode != 200) return@runCatching null
                    connection.inputStream.use { it.readBytes() }
                } finally {
                    connection.disconnect()
                }
                val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
                BitmapFactory.decodeByteArray(bytes, 0, bytes.size, bounds)
                var sample = 1
                val wanted = maxOf(1, maxOf(size.width, size.height))
                while (minOf(bounds.outWidth, bounds.outHeight) / (sample * 2) >= wanted) sample *= 2
                BitmapFactory.decodeByteArray(bytes, 0, bytes.size, BitmapFactory.Options().apply { inSampleSize = sample })?.asImageBitmap()
            }.getOrNull()?.also { CACHE.put(address, it) }
        }
    }

    companion object {
        private val CACHE = LruCache<String, ImageBitmap>(64)
    }
}
