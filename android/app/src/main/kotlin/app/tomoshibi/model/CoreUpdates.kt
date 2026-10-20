package app.tomoshibi.model

import java.io.File
import java.security.MessageDigest
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import uniffi.tomoshibi_protocol.AppBuild
import uniffi.tomoshibi_protocol.Deployment
import uniffi.tomoshibi_protocol.UpdateCheck
import uniffi.tomoshibi_protocol.UpdateDownload
import uniffi.tomoshibi_protocol.UpdateException
import uniffi.tomoshibi_protocol.UpdatePlatform
import uniffi.tomoshibi_protocol.UpdateProgress
import uniffi.tomoshibi_protocol.updateNotes

/**
 * Builds from the deployment, through the core (api/updates.rs), which asks
 * as the app, compares the versions, and lets nothing through that is not
 * the build it described.
 *
 * A deployment of its own rather than the app's: an update is for whoever
 * has the app, signed in or not and reachable or not when it started, and
 * needs no cookie.
 */
class CoreUpdateSource(private val base: String, private val folder: File) : UpdateSource {
    private val deployment: Deployment by lazy {
        // Before the deployment is made, which reads it then, so that the
        // user agent carries the version the deployment is asked about.
        ClientDescription.tell()
        Deployment(base)
    }

    override suspend fun latest(running: String): Offer? =
        when (val found = deployment.checkForUpdate(UpdatePlatform.ANDROID, running)) {
            UpdateCheck.UpToDate -> null
            is UpdateCheck.Available -> {
                val build = found.build
                Offer(build.version, build.size.toLong(), { language -> updateNotes(build, language) }, build)
            }
        }

    override fun download(offer: Offer): Transfer {
        val build = offer.handle as AppBuild
        // In the app's cache, which is its own and needs no permission, and
        // which the system may clear when it is short of room; anything left
        // there is cleared at the next start anyway (Updates.start).
        val file = apk(build.version)
        return CoreTransfer(deployment.downloadUpdate(build, file.path), file)
    }

    /** Names the build [keep] kept, beside it. */
    private val note: File get() = File(folder, "kept")

    override fun clear(keepingKept: Boolean) {
        val kept = if (keepingKept) runCatching { note.readText().trim() }.getOrNull() else null
        folder.listFiles()?.forEach { if (kept == null || (it.name != note.name && it.name != kept)) it.delete() }
    }

    override fun keep(file: File?) {
        if (file == null) {
            note.delete()
        } else {
            note.writeText(file.name)
        }
    }

    /**
     * Read again rather than trusted: the core checked the build as it
     * arrived, in a run of the app that has since ended, and the deployment
     * may have put another under the same version meanwhile. Only a build
     * whose SHA-256 is the one the deployment describes now goes on to be
     * installed.
     */
    override suspend fun kept(offer: Offer): File? {
        val build = offer.handle as? AppBuild ?: return null
        val name = runCatching { note.readText().trim() }.getOrNull() ?: return null
        val file = File(folder, name)
        if (name != apk(build.version).name || file.length() != build.size.toLong()) return null
        return withContext(Dispatchers.IO) {
            val digest = MessageDigest.getInstance("SHA-256")
            file.inputStream().use { input ->
                val chunk = ByteArray(1 shl 16)
                while (true) {
                    val read = input.read(chunk)
                    if (read < 0) break
                    digest.update(chunk, 0, read)
                }
            }
            val hash = digest.digest().joinToString("") { "%02x".format(it) }
            file.takeIf { hash.equals(build.sha256, ignoreCase = true) }
        }
    }

    private fun apk(version: String) = File(folder, "Tomoshibi-$version.apk")
}

private class CoreTransfer(private val download: UpdateDownload, override val file: File) : Transfer {
    override suspend fun run(progress: (received: Long, total: Long) -> Unit) {
        try {
            download.run(object : UpdateProgress {
                override fun progressed(received: ULong, total: ULong) = progress(received.toLong(), total.toLong())
            })
        } catch (fault: UpdateException) {
            throw when (fault) {
                is UpdateException.Cancelled -> Failure.Cancelled()
                is UpdateException.Network -> Failure.Broken(fault.detail)
                is UpdateException.Mismatch -> Failure.Damaged(fault.detail)
                is UpdateException.Superseded -> Failure.Superseded()
                is UpdateException.Storage -> Failure.Storage(fault.detail)
                is UpdateException.Refused -> Failure.Refused(fault.code)
                is UpdateException.Unexpected -> Failure.Other(fault.detail)
            }
        }
    }

    override fun cancel() = download.cancel()
}
