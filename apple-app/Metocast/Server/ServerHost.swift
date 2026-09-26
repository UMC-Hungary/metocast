#if os(macOS)
import AppKit
import Foundation
import Metocast

/// Runs this Mac's Metocast core in server mode: the headless `metocast-server` that the
/// build embeds in Contents/MacOS (`scripts/embed-apple-server.sh`). The app then talks to
/// it over loopback like any other client, as the Tauri app's server mode does.
@Observable
final class ServerHost {
    enum State: Equatable {
        case stopped
        case starting(String)
        case running
        case failed(String)
    }

    static let port = 3737
    private static let logLimit = 2_000

    private(set) var state: State = .stopped
    private(set) var log: [String] = []
    private(set) var token: String

    private var process: Process?
    private var input: Pipe?

    var localURL: String { "http://127.0.0.1:\(Self.port)" }
    /// The address other devices on the network use.
    var networkURL: String { "http://\(LocalNetwork.hostName ?? "127.0.0.1"):\(Self.port)" }
    var connectLink: ConnectLink { ConnectLink(url: networkURL, token: token) }

    var dataDirectory: URL {
        AppDirectories.support.appending(path: "server", directoryHint: .isDirectory)
    }

    init() {
        if let stored = Credentials.value(for: .serverToken) {
            token = stored
        } else {
            token = UUID().uuidString
            Credentials.set(token, for: .serverToken)
        }
        NotificationCenter.default.addObserver(
            forName: NSApplication.willTerminateNotification, object: nil, queue: .main
        ) { [weak self] _ in
            MainActor.assumeIsolated { self?.stopBeforeExit() }
        }
    }

    /// Launches the helper and waits until it answers `/health`. The first launch
    /// downloads PostgreSQL, so this can take a few minutes.
    @discardableResult
    func start() async -> Bool {
        if state == .running { return true }
        guard process == nil else { return false }
        guard let helper = Bundle.main.url(forAuxiliaryExecutable: "metocast-server") else {
            state = .failed("The Metocast server is missing from the app bundle.")
            return false
        }
        guard let probe = try? AppleClient(baseUrl: localURL, authToken: token) else {
            state = .failed("The server address is invalid.")
            return false
        }
        // Another Metocast core (e.g. the Tauri app) already owns the ports. Starting ours would
        // make its PostgreSQL startup kill the other one's database on port 15432.
        if (try? await probe.health()) != nil {
            state = .failed("Another Metocast server is already running on this Mac. Quit it, then try again.")
            return false
        }
        log.removeAll()
        state = .starting("Starting the server… The first start downloads PostgreSQL and can take a few minutes.")

        let process = Process()
        process.executableURL = helper
        process.environment = environment()
        // The helper exits when this pipe closes, so it can't outlive the app even after a crash.
        let input = Pipe()
        let output = Pipe()
        process.standardInput = input
        process.standardOutput = output
        process.standardError = output
        output.fileHandleForReading.readabilityHandler = { handle in
            let data = handle.availableData
            guard !data.isEmpty else { return }
            let text = String(decoding: data, as: UTF8.self)
            Task { @MainActor [weak self] in self?.append(text) }
        }
        process.terminationHandler = { process in
            let status = process.terminationStatus
            Task { @MainActor [weak self] in self?.processExited(status: status) }
        }
        do {
            try process.run()
        } catch {
            state = .failed("Couldn't start the server: \(error.localizedDescription)")
            return false
        }
        self.process = process
        self.input = input

        let deadline = Date.now.addingTimeInterval(10 * 60)
        while Date.now < deadline {
            // processExited already explained why.
            guard self.process != nil else { return false }
            if (try? await probe.health()) != nil {
                state = .running
                return true
            }
            try? await Task.sleep(for: .seconds(1))
        }
        await stop()
        state = .failed("The server didn't start within 10 minutes. See the server log.")
        return false
    }

    /// SIGTERM lets the helper stop PostgreSQL cleanly; SIGKILL only if it hangs.
    func stop() async {
        state = .stopped
        guard let process else { return }
        process.terminate()
        for _ in 0..<150 where self.process != nil {
            try? await Task.sleep(for: .milliseconds(100))
        }
        if process.isRunning {
            kill(process.processIdentifier, SIGKILL)
        }
    }

    /// Lets this Mac read back a credential it stored. Regenerated every launch, never saved.
    let adminToken = UUID().uuidString

    /// A new token locks out every device that had the old one. The helper reads the token at
    /// launch, so a running server restarts.
    func regenerateToken() async {
        token = UUID().uuidString
        Credentials.set(token, for: .serverToken)
        if process != nil {
            await stop()
            await start()
        }
    }

    private func stopBeforeExit() {
        guard let process, process.isRunning else { return }
        state = .stopped
        process.terminate()
        let deadline = Date.now.addingTimeInterval(10)
        while process.isRunning, Date.now < deadline {
            usleep(50_000)
        }
    }

    private func environment() -> [String: String] {
        // Xcode injects debugging libraries through DYLD_*; they have no business in the
        // helper or the PostgreSQL processes it starts.
        var environment = ProcessInfo.processInfo.environment.filter { key, _ in
            !key.hasPrefix("DYLD_") && !key.hasPrefix("__XPC_DYLD_")
        }
        environment["METOCAST_AUTH_TOKEN"] = token
        // Reading a stored secret back needs this as well as loopback, so it only ever lives
        // in this app and its helper, and only for one run.
        environment["METOCAST_ADMIN_TOKEN"] = adminToken
        environment["METOCAST_PORT"] = String(Self.port)
        environment["METOCAST_DATA_DIR"] = dataDirectory.path
        environment["METOCAST_EXIT_ON_STDIN_EOF"] = "1"
        environment["NO_COLOR"] = "1"
        environment["RUST_LOG"] = environment["RUST_LOG"] ?? "info"
        // Apps launched from the Finder get a bare PATH, and the server looks up ffprobe there.
        environment["PATH"] = "/opt/homebrew/bin:/usr/local/bin:" + (environment["PATH"] ?? "/usr/bin:/bin:/usr/sbin:/sbin")
        return environment
    }

    private func append(_ text: String) {
        let lines = text.split(whereSeparator: \.isNewline).map(String.init)
        log.append(contentsOf: lines)
        if log.count > Self.logLimit {
            log.removeFirst(log.count - Self.logLimit)
        }
    }

    private func processExited(status: Int32) {
        process = nil
        input = nil
        guard state != .stopped else { return }
        // The binary prints `Error: …` when it gives up; otherwise use the last logged error.
        let reason = log.last(where: { $0.hasPrefix("Error:") })
            ?? log.last(where: { $0.contains(" ERROR ") })
            ?? "exit code \(status)"
        state = .failed("The server stopped. \(reason)")
    }
}
#endif
