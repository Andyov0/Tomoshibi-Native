package app.tomoshibi.model

import android.os.Build
import app.tomoshibi.BuildConfig
import uniffi.tomoshibi_protocol.describeClient

/**
 * What this app tells the deployment it is, for the deployment's management
 * pages: Android, the version as the download page writes it (the version
 * name alone, 0.1.2: no version code after it, owner 2026-10-22), the system and the maker and model, such as
 * "samsung SM-F956B" for a Fold6. The core puts it in the user agent of every
 * request (core/protocol/src/api/agent.rs). As the Apple apps' does.
 */
object ClientDescription {
    /** The version as the download page and the update check write it. */
    val version: String get() = BuildConfig.VERSION_NAME

    fun tell() {
        describeClient(
            "Android",
            version,
            "Android ${Build.VERSION.RELEASE}",
            listOf(Build.MANUFACTURER, Build.MODEL).filter { !it.isNullOrBlank() }.joinToString(" "),
        )
    }
}
