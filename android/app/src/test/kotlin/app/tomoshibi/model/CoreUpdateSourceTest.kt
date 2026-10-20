package app.tomoshibi.model

import java.io.File
import java.nio.file.Files
import java.security.MessageDigest
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.tomoshibi_protocol.AppBuild
import uniffi.tomoshibi_protocol.UpdatePlatform

/*
 * The build kept in the app's cache past the end of a run, for the run after
 * it, where Android 11 ends the app as the person allows it to install. What
 * these guard is a build being installed that is not the one on offer (the
 * deployment put another under the same version meanwhile, or the file was
 * changed), and the cache filling with builds nobody will install.
 *
 * Nothing here reaches the core's library: the deployment is made only when
 * something is asked of it.
 */
class CoreUpdateSourceTest {
    private val folder: File = Files.createTempDirectory("updates").toFile().apply { deleteOnExit() }
    private val source = CoreUpdateSource("http://127.0.0.1:9", folder)
    private val bytes = ByteArray(300_000) { (it * 7).toByte() }

    private fun hex(data: ByteArray) = MessageDigest.getInstance("SHA-256").digest(data).joinToString("") { "%02x".format(it) }

    private fun offer(version: String = "0.1.0.8", data: ByteArray = bytes): Offer {
        val build = AppBuild(UpdatePlatform.ANDROID, version, data.size.toULong(), hex(data), emptyMap())
        return Offer(version, data.size.toLong(), handle = build)
    }

    /** A build as a download leaves it, with a part of another beside it. */
    private fun arrived(version: String = "0.1.0.8"): File {
        File(folder, "Tomoshibi-0.1.0.9.apk.0123456789abcdef.part").writeBytes(ByteArray(10))
        return File(folder, "Tomoshibi-$version.apk").apply { writeBytes(bytes) }
    }

    @Test
    fun theBuildKeptOutlivesTheStartAndIsTakenUpForItsOwnOffer() = runBlocking {
        val file = arrived()
        source.keep(file)
        source.clear(keepingKept = true)
        assertEquals(listOf(file.name, "kept").sorted(), folder.list()!!.sorted())
        assertEquals(file, source.kept(offer()))
        // Letting go of everything takes it too.
        source.clear()
        assertTrue(folder.list()!!.isEmpty())
        assertNull(source.kept(offer()))
    }

    @Test
    fun nothingIsKeptAtTheStartUnlessItWasAskedFor() = runBlocking {
        val file = arrived()
        source.keep(file)
        source.keep(null)
        source.clear(keepingKept = true)
        assertFalse(file.exists())
        assertTrue(folder.list()!!.isEmpty())
    }

    @Test
    fun aKeptBuildThatIsNotTheOneOnOfferIsNotTakenUp() = runBlocking {
        val file = arrived()
        source.keep(file)
        assertNull("taken up for another version", source.kept(offer(version = "0.1.0.9")))
        // The same version and size, other bytes: another build under the
        // same number, or the file changed since.
        val other = bytes.copyOf().also { it[1234] = (it[1234] + 1).toByte() }
        assertNull("taken up though its bytes are not the build on offer", source.kept(offer(data = other)))
        assertEquals(file, source.kept(offer()))
    }
}
