package app.tomoshibi.model

import app.tomoshibi.basics.Language
import uniffi.tomoshibi_protocol.Deployment
import uniffi.tomoshibi_protocol.RemoteConfig
import uniffi.tomoshibi_protocol.parseRemoteConfig
import uniffi.tomoshibi_protocol.remoteNoticeText
import uniffi.tomoshibi_protocol.remotePhrases

/**
 * What the deployment says to the apps, through the core (api/remote.rs),
 * which asks as the app (the platform and version in its user agent decide
 * the answer), reads it a section at a time, and works out each language's
 * share of it. CoreRemote.swift on Apple.
 *
 * A deployment of its own rather than the app's, as the updates have
 * (CoreUpdateSource): what is said is said to whoever has the app, signed in
 * or not, and is needed before the app's own deployment has answered, or
 * when it never does.
 */
class CoreRemoteSource(private val base: String) : RemoteSource {
    private val deployment: Deployment by lazy {
        // Before the deployment is made, which reads it then, so that the
        // user agent carries the version the answer is for.
        ClientDescription.tell()
        Deployment(base)
    }

    override suspend fun fetch(): RemoteAnswer = answer(deployment.remoteConfig())

    override fun read(kept: String): RemoteAnswer? = parseRemoteConfig(kept)?.let(::answer)

    /**
     * The core's answer, worked out for each of the app's four languages by
     * the core's own rules: the phrases in that language or in it without
     * its region or script, with no English stand-in (`remote_phrases`); the
     * notice in that language, else without its suffix, else English
     * (`remote_notice_text`).
     */
    private fun answer(config: RemoteConfig): RemoteAnswer {
        val phrases = Language.entries.mapNotNull { language ->
            remotePhrases(config, language.code).takeIf { it.isNotEmpty() }?.let { language to it }
        }.toMap()
        val notice = config.notice?.let { notice ->
            LobbyNotice(
                id = notice.id,
                warning = notice.warning,
                text = Language.entries.mapNotNull { language -> remoteNoticeText(notice, language.code)?.let { language to it } }.toMap(),
                until = notice.until?.let { it * 1000 },
            )
        }
        return RemoteAnswer(config.revision, phrases, config.off.toSet(), config.params, notice, config.kept)
    }
}
