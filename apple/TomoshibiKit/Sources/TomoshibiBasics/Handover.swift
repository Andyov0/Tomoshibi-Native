import Foundation
import Security

/// What the iPhone app leaves for its Messages extension, which is a process
/// of its own with nothing of the app's: not its session, not its defaults,
/// not the core that holds the conversation with the deployment.
///
/// Two halves, for two kinds of thing. The session cookie is a secret and goes
/// in the Keychain, in an item both can read because its access group is the
/// app group they share (`SharedKeychain`); the app's own copy stays where it
/// always was, readable by the app alone. The rest is not secret and goes in
/// the app group's defaults: which account is signed in, whether it may
/// arrange meetings, the meetings the app last saw (shown at once, while the
/// extension asks the deployment again), and the language and colour the
/// person chose in the app, which the extension cannot read from the app's
/// own defaults and would otherwise ignore.
///
/// Kept by the deployment's address, the core's `base_url`. A release build
/// talks to one deployment only and reads that one; a debug build driven by a
/// test talks to a local one, and keeps its record under that address, where
/// only a debug extension pointed at the same test deployment (`testBase`)
/// looks.
public struct Handover {
    public struct Record: Codable, Equatable, Sendable {
        /// The deployment, as an origin ending in "/".
        public var base: String
        /// The signed-in account's name, nil when nobody is.
        public var account: String?
        /// Whether that account may arrange meetings (manage/grants.ts
        /// `arrange`).
        public var arranges: Bool
        /// The meetings as the app last saw them.
        public var meetings: [Meeting]
        /// The language chosen in the app, nil while it follows the system's.
        public var language: String?
        /// The colour chosen in the app (Appearance `accent`).
        public var accent: String?

        public init(base: String, account: String?, arranges: Bool, meetings: [Meeting], language: String?, accent: String?) {
            self.base = base
            self.account = account
            self.arranges = arranges
            self.meetings = meetings
            self.language = language
            self.accent = accent
        }
    }

    let defaults: UserDefaults?
    let secrets: any Secrets

    public init(defaults: UserDefaults?, secrets: any Secrets) {
        self.defaults = defaults
        self.secrets = secrets
    }

    /// The app group's, which the app and the extension both have.
    public static var shared: Handover {
        Handover(defaults: UserDefaults(suiteName: BroadcastWire.group), secrets: SharedKeychain())
    }

    /// Leaves `record`, and the session beside it; a nil cookie takes the
    /// kept one away, as signing out does.
    public func keep(_ record: Record, cookie: String?) {
        let base = Self.origin(record.base) ?? record.base
        var record = record
        record.base = base
        if let data = try? JSONEncoder().encode(record) { defaults?.set(data, forKey: Self.recordKey(base)) }
        secrets.write(Self.cookieKey(base), record.account == nil ? nil : cookie)
    }

    public func record(for base: String) -> Record? {
        let base = Self.origin(base) ?? base
        guard let data = defaults?.data(forKey: Self.recordKey(base)) else { return nil }
        return try? JSONDecoder().decode(Record.self, from: data)
    }

    public func cookie(for base: String) -> String? {
        secrets.read(Self.cookieKey(Self.origin(base) ?? base))
    }

    /// The deployment a test drove the app against, for a debug build of
    /// the extension to talk to instead of home's. The app's tests name their
    /// deployment in TOMOSHIBI_BASE, and an extension is started by Messages
    /// with none of the environment the app or the simulator was given, so
    /// the app passes it on here: a rehearsal asked to hand over
    /// (TOMOSHIBI_DEBUG_HANDOVER) leaves its address, and any other start of
    /// the app takes it away again, so that the extension of an app opened
    /// by hand afterwards goes back to home. A release build of the extension
    /// never reads it.
    public var testBase: String? {
        get { defaults?.string(forKey: Self.testBaseKey) }
        nonmutating set {
            if let newValue { defaults?.set(newValue, forKey: Self.testBaseKey) } else { defaults?.removeObject(forKey: Self.testBaseKey) }
        }
    }

    static let testBaseKey = "tomoshibi.handover.test-base"
    static func recordKey(_ base: String) -> String { "tomoshibi.handover:" + base }
    static func cookieKey(_ base: String) -> String { "account:" + base }

    /// An address as the core's `base_url` writes it: the origin alone, the
    /// host in lower case, a default port left out, and "/" after it. A test
    /// names its deployment as "http://127.0.0.1:8080", and the app's
    /// deployment calls itself "http://127.0.0.1:8080/"; the two are one key.
    public static func origin(_ address: String) -> String? {
        guard let parts = URLComponents(string: address.trimmingCharacters(in: .whitespacesAndNewlines)),
              let scheme = parts.scheme?.lowercased(), ["http", "https"].contains(scheme),
              let host = parts.host?.lowercased(), !host.isEmpty else { return nil }
        let port = parts.port.flatMap { ($0 == 80 && scheme == "http") || ($0 == 443 && scheme == "https") ? nil : $0 }
        return "\(scheme)://\(host)\(port.map { ":\($0)" } ?? "")/"
    }
}

/// Where a secret is kept: the Keychain in the apps, memory in the tests,
/// which run on a Mac with no entitlement to an app group's items.
public protocol Secrets {
    func read(_ account: String) -> String?
    func write(_ account: String, _ value: String?)
}

/// The Keychain items the app and its extensions share: those whose access
/// group is the app group. An iOS app's app groups are keychain access groups
/// as well, with no entitlement beyond the group itself, so the extension can
/// read what the app writes here and nothing the app keeps for itself.
public struct SharedKeychain: Secrets {
    private let service = "app.tomoshibi.shared"
    private let group: String

    public init(group: String = BroadcastWire.group) {
        self.group = group
    }

    public func read(_ account: String) -> String? {
        var query = base(account)
        query[kSecReturnData as String] = true
        query[kSecMatchLimit as String] = kSecMatchLimitOne
        var out: CFTypeRef?
        guard SecItemCopyMatching(query as CFDictionary, &out) == errSecSuccess, let data = out as? Data else { return nil }
        return String(data: data, encoding: .utf8)
    }

    public func write(_ account: String, _ value: String?) {
        SecItemDelete(base(account) as CFDictionary)
        guard let value, !value.isEmpty else { return }
        var item = base(account)
        item[kSecValueData as String] = Data(value.utf8)
        // Behind the device's lock until it is first unlocked, as the app's
        // own copy is (Keychain.swift), and unlike it never restored onto
        // another phone from a backup: this is a copy, and the app writes it
        // again whenever it runs.
        item[kSecAttrAccessible as String] = kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly
        SecItemAdd(item as CFDictionary, nil)
    }

    private func base(_ account: String) -> [String: Any] {
        [kSecClass as String: kSecClassGenericPassword,
         kSecAttrService as String: service,
         kSecAttrAccount as String: account,
         kSecAttrAccessGroup as String: group,
         kSecUseDataProtectionKeychain as String: true]
    }
}
