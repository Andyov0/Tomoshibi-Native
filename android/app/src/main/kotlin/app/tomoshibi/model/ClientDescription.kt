package app.tomoshibi.model

import android.os.Build
import app.tomoshibi.BuildConfig
import uniffi.tomoshibi_protocol.describeClient

/**
 * What this app tells the deployment it is, for the deployment's management
 * pages: Android, the version as the download page writes it (0.1.0 with
 * version code 1 is 0.1.0.1), the system and the maker and model, such as
 * "samsung SM-F956B" for a Fold6. The core puts it in the user agent of every
 * request (core/protocol/src/api/agent.rs). As the Apple apps' does.
 */
object ClientDescription {
    /** The version as the download page and the update check write it. */
    val version: String get() = "${BuildConfig.VERSION_NAME}.${BuildConfig.VERSION_CODE}"

    fun tell() {
        describeClient(
            "Android",
            version,
            "Android ${Build.VERSION.RELEASE}",
            listOf(Build.MANUFACTURER, Build.MODEL).filter { !it.isNullOrBlank() }.joinToString(" "),
        )
    }
}
