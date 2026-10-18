package app.tomoshibi.ui.kit

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Assume.assumeTrue
import org.junit.Test
import java.io.File

/**
 * Every SF Symbol the Swift views name has an Android symbol, so that a
 * screen ported keeping the Swift names cannot draw a blank where a control
 * should be; and a name without one stops a debug build.
 */
class SymbolsTest {
    /** Every name found under apple/TomoshibiKit/Sources when the kit was made (2026-10-18). */
    private val named = listOf(
        "app.badge", "arrow.clockwise", "arrow.down.right.and.arrow.up.left",
        "arrow.down.right.and.arrow.up.left.rectangle", "arrow.right", "arrow.up",
        "arrow.up.left.and.arrow.down.right", "arrow.up.left.and.arrow.down.right.rectangle", "arrow.up.right",
        "arrow.up.right.square", "arrow.up.to.line", "bubble.left.and.bubble.right.fill",
        "bubble.left.and.text.bubble.right", "bubble.left.fill", "calendar", "calendar.badge.clock",
        "calendar.badge.plus", "camera.fill", "checkmark", "checkmark.circle.fill", "chevron.down", "chevron.left",
        "chevron.left.forwardslash.chevron.right", "chevron.right", "chevron.up", "chevron.up.chevron.down", "clock",
        "cpu", "crown", "crown.fill", "doc.on.doc", "door.left.hand.closed", "door.left.hand.open", "ellipsis",
        "exclamationmark.circle", "exclamationmark.circle.fill", "exclamationmark.triangle",
        "externaldrive.badge.exclamationmark", "eye", "eye.fill", "eye.slash", "film", "film.stack", "folder.fill",
        "forward.end.fill", "forward.fill", "gauge.with.dots.needle.33percent", "gearshape", "gearshape.fill",
        "globe", "hand.raised.fill", "hifispeaker", "hourglass", "house", "icloud.slash", "info.circle",
        "info.circle.fill", "key", "key.fill", "link", "list.bullet.rectangle", "lock", "lock.fill",
        "magnifyingglass", "mic", "mic.fill", "mic.slash.fill", "music.magnifyingglass", "music.note", "music.note.list", "number",
        "pause.fill", "person", "person.and.background.dotted", "person.badge.minus",
        "person.crop.circle.badge.exclamationmark", "person.crop.circle.badge.xmark", "person.crop.circle.fill",
        "person.fill", "person.fill.checkmark", "person.wave.2.fill", "phone.down.fill", "photo", "pip.enter",
        "pip.exit", "play.circle.fill", "play.fill", "play.rectangle", "play.rectangle.fill", "plus",
        "plus.circle.fill", "rectangle.inset.filled.and.person.filled", "rectangle.inset.filled.on.rectangle",
        "rectangle.on.rectangle", "rectangle.portrait.and.arrow.right", "return", "server.rack", "signature",
        "slider.horizontal.3", "sparkles", "sparkles.tv", "speaker.slash.fill", "speaker.wave.1.fill",
        "speaker.wave.2", "speaker.wave.2.fill", "stop.circle", "trash", "video", "video.fill", "video.slash.fill",
        "wand.and.stars", "waveform", "waveform.circle.fill", "waveform.path", "wifi.exclamationmark", "xmark",
        "xmark.circle",
    )

    @Test
    fun everyNameTheSwiftViewsUsedHasASymbol() {
        val missing = named.filterNot { it in symbolNames }
        assertEquals("no Android symbol for $missing", emptyList<String>(), missing)
        for (name in named) assertNotNull(symbol(name))
    }

    /**
     * Names added to the Swift views since: every name handed to a view as
     * `systemName:`, `systemImage:` or `symbol:`, given outright or as either
     * side of a choice (`symbol: on ? "mic.fill" : "mic.slash.fill"`).
     */
    @Test
    fun theSwiftSourcesNameNothingTheKitCannotDraw() {
        val sources = File("../../apple/TomoshibiKit/Sources")
        assumeTrue("the Apple sources are beside the Android project", sources.isDirectory)
        val name = "\"([a-z0-9]+(?:\\.[a-z0-9]+)*)\""
        val handed = Regex("(?:systemName|systemImage|symbol):\\s*(?:[^,()\"]*\\?\\s*)?$name(?:\\s*:\\s*$name)?")
        val found = sortedSetOf<String>()
        sources.walkTopDown().filter { it.extension == "swift" }.forEach { file ->
            for (line in file.readLines()) {
                handed.findAll(line).forEach { match -> match.groupValues.drop(1).filter { it.isNotEmpty() }.forEach(found::add) }
            }
        }
        assertTrue("found no symbols at all: the scan is broken", found.size > 60)
        val missing = found.filterNot { it in symbolNames }
        assertEquals("the Swift views name symbols Symbols.kt does not map", emptyList<String>(), missing)
    }

    @Test
    fun aNameWithNoSymbolStopsADebugBuild() {
        assertThrows(IllegalStateException::class.java) { symbol("no.such.symbol") }
    }
}
