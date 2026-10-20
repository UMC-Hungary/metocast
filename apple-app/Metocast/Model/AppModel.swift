import Foundation
import Metocast

/// How this device uses Metocast: hosting the core itself (Mac only) or talking to one elsewhere.
enum AppMode: String {
    case server, client
}

/// App-wide state: the chosen mode, the live session and the notification list.
@Observable
final class AppModel {
    private(set) var mode = UserDefaults.standard.string(forKey: Keys.mode).flatMap(AppMode.init) {
        didSet { UserDefaults.standard.set(mode?.rawValue, forKey: Keys.mode) }
    }
    private(set) var session: MetocastSession?
    var activity: [ActivityItem] = []
    /// A `metocast://connect` link waiting to be used (during setup) or confirmed (after it).
    var pendingLink: ConnectLink?
    // In the model so toolbar buttons and the Mac's menu commands (⌘N, ⌘,) share one state.
    var isShowingSettings = false
    var isAddingEvent = false
    /// The selected tab (iPhone) or sidebar item (Mac), here so the Dashboard's On Air card can open Live.
    var pane = MetocastPane.dashboard
    /// An equipment page opened from the Mac's settings; it takes over the content column until a sidebar item is picked.
    var equipment: EquipmentPage?

    #if os(macOS)
    let server = ServerHost()
    #endif

    private enum Keys {
        static let mode = "mode"
        static let serverURL = "serverURL"
    }

    var unreadCount: Int { activity.count { !$0.isRead } }

    /// Decks the server is parsing into the slide search index, or nil when it is not indexing.
    var slideIndexPending: UInt32? {
        session.flatMap { $0.slideIndexPending > 0 ? $0.slideIndexPending : nil }
    }

    /// Reconnects to whatever this device was set up for.
    func resume() async {
        guard session == nil else { return }
        switch mode {
        case .client:
            guard let url = UserDefaults.standard.string(forKey: Keys.serverURL),
                  let token = Credentials.value(for: .clientToken)
            else {
                mode = nil
                return
            }
            await connect(url: url, token: token)
        case .server:
            #if os(macOS)
            await startServer()
            #else
            mode = nil
            #endif
        case nil:
            break
        }
    }

    /// Makes `link` this device's server if it answers and accepts the token. Returns why not otherwise.
    func useServer(_ link: ConnectLink) async -> String? {
        guard let url = ServerAddress.normalized(link.url) else {
            return String(localized: "Enter a server address like 192.168.1.10:3737.")
        }
        let token = link.token.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !token.isEmpty else { return String(localized: "Enter the token shown in the server's Settings.") }
        guard let candidate = try? MetocastSession(baseURL: url, token: token) else {
            return String(localized: "Enter a server address like 192.168.1.10:3737.")
        }
        await candidate.start()
        switch candidate.status {
        case .unreachable:
            candidate.stop()
            return String(localized: "Can't reach \(ServerAddress.displayName(url)). Check the address and that both devices are on the same network.")
        case .unauthorized:
            candidate.stop()
            return String(localized: "The server rejected this token.")
        default:
            break
        }
        session?.stop()
        #if os(macOS)
        if mode == .server { await server.stop() }
        #endif
        UserDefaults.standard.set(url, forKey: Keys.serverURL)
        Credentials.set(token, for: .clientToken)
        mode = .client
        attach(candidate)
        return nil
    }

    /// Forgets the server; the setup screen shows again. Server data and the server token stay.
    func reset() async {
        session?.stop()
        session = nil
        #if os(macOS)
        await server.stop()
        #endif
        UserDefaults.standard.removeObject(forKey: Keys.serverURL)
        Credentials.set(nil, for: .clientToken)
        activity.removeAll()
        mode = nil
    }

    func retry() async {
        session?.stop()
        session = nil
        await resume()
    }

    func open(_ url: URL) {
        pendingLink = ConnectLink(url)
    }

    func markActivityRead() {
        for index in activity.indices { activity[index].isRead = true }
    }

    #if os(macOS)
    func startServer() async {
        mode = .server
        session?.stop()
        session = nil
        guard await server.start() else { return }
        await connect(url: server.localURL, token: server.token)
    }

    func regenerateServerToken() async {
        session?.stop()
        session = nil
        await server.regenerateToken()
        if server.state == .running {
            await connect(url: server.localURL, token: server.token)
        }
    }
    #endif

    private func connect(url: String, token: String) async {
        guard let session = try? MetocastSession(baseURL: url, token: token) else { return }
        attach(session)
        await session.start()
    }

    private func attach(_ session: MetocastSession) {
        session.onActivity = { [weak self] item in
            self?.activity.insert(item, at: 0)
            if item.isAlert { SystemNotice.post(item.title) }
        }
        self.session = session
    }
}
