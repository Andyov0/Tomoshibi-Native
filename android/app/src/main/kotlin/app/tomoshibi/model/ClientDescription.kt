package app.tomoshibi.model

import android.os.Build
import app.tomoshibi.BuildConfig
import uniffi.tomoshibi_protocol.describeClient

/**
 * What this app tells the deployment it is, for the deployment's management
 * pages: Android, the version as the download page writes it, the system and
 * the maker and model, such as "samsung SM-F956B" for a Fold6. The core puts
 * it in the user agent of every request (core/protocol/src/api/agent.rs). As
 * the Apple apps' does.
 *
 * The version is the version name and nothing else: 0.1.4 for a version's
 * first build and 0.1.4.2 for its second (app/build.gradle.kts, the core's
 * version_with_build), never the version code after it (owner, 2026-10-22).
 * The update check is asked about the same string (Updates `running`) and the
 * deployment's manifest writes its builds the same way, so the user agent,
 * the check and the manifest all say one build by one name; a version code
 * joined on here would make every build look newer than the manifest's.
 * People are shown it through the core's version_label, as "0.1.4 (2)".
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
