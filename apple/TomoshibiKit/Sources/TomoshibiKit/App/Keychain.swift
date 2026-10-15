import Foundation
import Security

/// Secrets kept between launches: the account's session cookie and the
/// passphrase typed at the join form.
///
/// The web keeps the passphrase in local storage in the clear, on purpose, so
/// that the field is filled next time (the server repository's CLAUDE.md
/// argues it at length). An app has a better home for it than a browser
/// does: the Keychain, encrypted and behind the device's lock. The sealing
/// word for an encrypted call is never kept anywhere, here as on the web.
enum Keychain {
    private static let service = "app.tomoshibi"

    static func read(_ account: String) -> String? {
        var query = base(account)
        query[kSecReturnData as String] = true
        query[kSecMatchLimit as String] = kSecMatchLimitOne
        var out: CFTypeRef?
        guard SecItemCopyMatching(query as CFDictionary, &out) == errSecSuccess, let data = out as? Data else { return nil }
        return String(data: data, encoding: .utf8)
    }

    static func write(_ account: String, _ value: String?) {
        SecItemDelete(base(account) as CFDictionary)
        guard let value, !value.isEmpty else { return }
        var item = base(account)
        item[kSecValueData as String] = Data(value.utf8)
        item[kSecAttrAccessible as String] = kSecAttrAccessibleAfterFirstUnlock
        SecItemAdd(item as CFDictionary, nil)
    }

    private static func base(_ account: String) -> [String: Any] {
        [kSecClass as String: kSecClassGenericPassword,
         kSecAttrService as String: service,
         kSecAttrAccount as String: account]
    }
}
