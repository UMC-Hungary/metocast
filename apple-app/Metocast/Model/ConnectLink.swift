import Foundation

/// A server address and token handed to a client device by QR code or deep link:
/// `metocast://connect?url=…&token=…`, the same format as `connectLink` in
/// `packages/core-client/src/host/index.ts`.
struct ConnectLink: Equatable, Identifiable {
    var url: String
    var token: String

    var id: String { url + token }

    init(url: String, token: String) {
        self.url = url
        self.token = token
    }

    init?(_ link: URL) {
        guard link.scheme == "metocast",
              let items = URLComponents(url: link, resolvingAgainstBaseURL: false)?.queryItems,
              let url = items.first(where: { $0.name == "url" })?.value, !url.isEmpty,
              let token = items.first(where: { $0.name == "token" })?.value, !token.isEmpty
        else { return nil }
        self.init(url: url, token: token)
    }

    init?(scanned text: String) {
        guard let link = URL(string: text) else { return nil }
        self.init(link)
    }

    var link: URL? {
        var components = URLComponents()
        components.scheme = "metocast"
        components.host = "connect"
        components.queryItems = [URLQueryItem(name: "url", value: url), URLQueryItem(name: "token", value: token)]
        return components.url
    }
}

enum ServerAddress {
    static let defaultPort = 3737

    /// Accepts what people type ("192.168.1.10", "mac-mini.local:3737/") and returns a
    /// base URL like `http://192.168.1.10:3737`, or nil when it can't be one.
    static func normalized(_ input: String) -> String? {
        var text = input.trimmingCharacters(in: .whitespacesAndNewlines)
        while text.hasSuffix("/") { text.removeLast() }
        guard !text.isEmpty else { return nil }
        if !text.contains("://") { text = "http://" + text }
        guard var components = URLComponents(string: text),
              let scheme = components.scheme?.lowercased(), ["http", "https"].contains(scheme),
              let host = components.host, !host.isEmpty
        else { return nil }
        // Metocast serves plain HTTP on 3737 unless someone put it behind a proxy.
        if scheme == "http", components.port == nil, components.path.isEmpty {
            components.port = defaultPort
        }
        return components.string
    }

    /// "mac-mini.local:3737" for display.
    static func displayName(_ url: String) -> String {
        guard let components = URLComponents(string: url), let host = components.host else { return url }
        return components.port.map { "\(host):\($0)" } ?? host
    }
}
