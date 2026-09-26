import Foundation
import Security

/// Where the app keeps auth tokens.
///
/// The iPhone uses the Keychain. The Mac uses a file only this user can read, like the
/// Tauri app's store: Mac builds are ad-hoc signed, and the login keychain ties items to
/// the exact binary, so every rebuild would ask for the account password. Move the Mac to
/// the data-protection keychain once it is signed with a team.
enum Credentials {
    enum Key: String {
        /// The token of the remote server this device connects to in client mode.
        case clientToken
        /// The token this Mac's own server accepts in server mode.
        case serverToken
    }

    static func value(for key: Key) -> String? {
        #if os(macOS)
        return FileStore.read()[key.rawValue]
        #else
        return KeychainStore.read(key.rawValue)
        #endif
    }

    static func set(_ value: String?, for key: Key) {
        #if os(macOS)
        var stored = FileStore.read()
        stored[key.rawValue] = value
        FileStore.write(stored)
        #else
        if let value {
            KeychainStore.write(value, for: key.rawValue)
        } else {
            KeychainStore.delete(key.rawValue)
        }
        #endif
    }
}

enum AppDirectories {
    /// `~/Library/Application Support/<bundle id>` (the app's container on iOS).
    static var support: URL {
        let base = URL.applicationSupportDirectory
        return base.appending(path: Bundle.main.bundleIdentifier ?? "com.metocast.native", directoryHint: .isDirectory)
    }
}

#if os(macOS)
private enum FileStore {
    private static var url: URL { AppDirectories.support.appending(path: "credentials.json") }

    static func read() -> [String: String] {
        guard let data = try? Data(contentsOf: url) else { return [:] }
        return (try? JSONDecoder().decode([String: String].self, from: data)) ?? [:]
    }

    static func write(_ values: [String: String]) {
        let directory = url.deletingLastPathComponent()
        do {
            try FileManager.default.createDirectory(
                at: directory, withIntermediateDirectories: true, attributes: [.posixPermissions: 0o700]
            )
            try JSONEncoder().encode(values).write(to: url, options: .atomic)
            try FileManager.default.setAttributes([.posixPermissions: 0o600], ofItemAtPath: url.path)
        } catch {
            assertionFailure("Could not save credentials: \(error)")
        }
    }
}
#else
private enum KeychainStore {
    private static let service = "com.metocast.native"

    private static func query(_ account: String) -> [CFString: Any] {
        [kSecClass: kSecClassGenericPassword, kSecAttrService: service, kSecAttrAccount: account]
    }

    static func read(_ account: String) -> String? {
        var query = query(account)
        query[kSecReturnData] = true
        query[kSecMatchLimit] = kSecMatchLimitOne
        var result: CFTypeRef?
        guard SecItemCopyMatching(query as CFDictionary, &result) == errSecSuccess,
              let data = result as? Data else { return nil }
        return String(data: data, encoding: .utf8)
    }

    static func write(_ value: String, for account: String) {
        let data = Data(value.utf8)
        let status = SecItemUpdate(query(account) as CFDictionary, [kSecValueData: data] as CFDictionary)
        if status == errSecItemNotFound {
            var item = query(account)
            item[kSecValueData] = data
            item[kSecAttrAccessible] = kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly
            SecItemAdd(item as CFDictionary, nil)
        }
    }

    static func delete(_ account: String) {
        SecItemDelete(query(account) as CFDictionary)
    }
}
#endif
