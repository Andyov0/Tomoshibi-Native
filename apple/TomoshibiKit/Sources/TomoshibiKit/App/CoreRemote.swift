import Foundation
import TomoshibiBasics
import TomoshibiCore

/// What the deployment says to the apps, through the core (api/remote.rs),
/// which asks as the app (the platform and version in its user agent decide
/// the answer), reads it a section at a time, and works out each language's
/// share of it.
///
/// A deployment of its own rather than the app's, as the updates have: what
/// is said is said to whoever has the app, signed in or not, and is needed
/// before the app's own deployment has answered, or when it never does.
@MainActor
final class CoreRemoteSource: RemoteSource {
    private let base: String
    private var made: Deployment?

    init(base: String) {
        self.base = base
    }

    private func deployment() throws -> Deployment {
        if let made { return made }
        // Before the deployment is made, which reads it then, so that the
        // user agent carries the version the answer is for.
        ClientDescription.tell()
        let deployment = try Deployment(base: base)
        made = deployment
        return deployment
    }

    func fetch() async throws -> RemoteAnswer {
        RemoteAnswer(try await deployment().remoteConfig())
    }

    func read(_ kept: String) -> RemoteAnswer? {
        parseRemoteConfig(kept: kept).map(RemoteAnswer.init)
    }
}

extension RemoteAnswer {
    /// The core's answer, worked out for each of the app's four languages by
    /// the core's own rules, so that a change of language mid-session needs
    /// nothing asked or read again: the phrases in that language or in it
    /// without its region or script, with no English stand-in
    /// (`remote_phrases`); the notice in that language, else without its
    /// suffix, else English (`remote_notice_text`).
    init(_ config: RemoteConfig) {
        var phrases: [Language: [String: String]] = [:]
        for language in Language.allCases {
            let laid = remotePhrases(config: config, language: language.rawValue)
            if !laid.isEmpty { phrases[language] = laid }
        }
        let notice = config.notice.map { notice in
            var text: [Language: String] = [:]
            for language in Language.allCases {
                text[language] = remoteNoticeText(notice: notice, language: language.rawValue)
            }
            return LobbyNotice(id: notice.id, warning: notice.warning, text: text,
                               until: notice.until.map { Date(timeIntervalSince1970: TimeInterval($0)) })
        }
        self.init(revision: config.revision, phrases: phrases, off: Set(config.off), params: config.params,
                  notice: notice, kept: config.kept)
    }
}

#if DEBUG
/// TOMOSHIBI_DEBUG_REMOTE=<file>, in a rehearsal: the file's JSON as the
/// deployment's answer, read again at every question, so that a test can
/// see what the app does with a notice or a feature switched off against a
/// local deployment that has no /api/apps/remote; or the JSON itself, where
/// the value starts with a brace, for a UI test, whose runner's files the
/// app may not be able to read. Read by the core, as an answer from the
/// deployment is; a file that is not there, or not one, is a question that
/// went unanswered.
@MainActor
final class FileRemoteSource: RemoteSource {
    private let said: String

    init(said: String) {
        self.said = said
    }

    struct Unreadable: Error {}

    func fetch() async throws -> RemoteAnswer {
        let raw: String? = said.hasPrefix("{") ? said : (try? String(contentsOfFile: said, encoding: .utf8))
        guard let raw, let read = read(raw) else { throw Unreadable() }
        return read
    }

    func read(_ kept: String) -> RemoteAnswer? {
        parseRemoteConfig(kept: kept).map(RemoteAnswer.init)
    }
}
#endif
