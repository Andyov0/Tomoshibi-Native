package app.tomoshibi.model

import java.io.File
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
        val file = File(folder, "Tomoshibi-${build.version}.apk")
        return CoreTransfer(deployment.downloadUpdate(build, file.path), file)
    }

    override fun clear() {
        folder.listFiles()?.forEach { it.delete() }
    }
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
