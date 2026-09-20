import Foundation
import Metocast
#if os(iOS)
import UIKit
#endif

/// A live connection to one Metocast core. In server mode it points at this Mac's own
/// helper over loopback; in client mode at another machine's server. Both use the same
/// HTTP/WebSocket API, exactly like the Tauri app's two modes.
@Observable
final class MetocastSession {
    enum Status: Equatable {
        case connecting
        case connected
        case reconnecting
        case unauthorized
        case unreachable
        case closed
    }

    let baseURL: String
    private(set) var status: Status = .connecting
    private(set) var events: [EventSummaryRecord] = []
    private(set) var connectors: [String: ConnectorStatusRecord] = [:]
    private(set) var presenter: PresenterStateRecord?
    // Live production. Each is nil while its connector isn't connected.
    private(set) var obs: OBSOutputs?
    private(set) var atem: AtemState?
    private(set) var middlecontrol: MiddlecontrolState?
    private(set) var rodecaster: RodecasterProfile?
    private(set) var recorder: RecorderStateRecord?
    // Slides. The server decides whether a presentation opens in the web presenter or Keynote.
    private(set) var usesWebPresenter = true
    private(set) var presenterTheme: PresenterTheme = .classic
    private(set) var presentationStatus: PresentationStatus?
    private(set) var clients: [ConnectedClientRecord] = []

    /// Receives server notifications and connection or connector changes worth one.
    var onActivity: (ActivityItem) -> Void = { _ in }

    private var discoveryResults: [DiscoveryTool: DiscoverySnapshot] = [:]
    private var discoveryFailure: String?

    private let client: AppleClient
    private var signals: Task<Void, Never>?
    private var settledStatuses: [String: ConnectorStatusCode] = [:]

    init(baseURL: String, token: String) throws {
        self.baseURL = baseURL
        client = try AppleClient(baseUrl: baseURL, authToken: token)
    }

    /// Checks the server, loads the first snapshot and opens the event stream.
    func start() async {
        status = .connecting
        do {
            try await client.health()
        } catch {
            status = .unreachable
            return
        }
        guard await refresh() else { return }

        let (stream, continuation) = AsyncStream.makeStream(of: SessionSignal.self)
        signals = Task { [weak self] in
            for await signal in stream {
                self?.apply(signal)
            }
        }
        do {
            try await client.startEvents(listener: SessionListener(continuation: continuation))
        } catch {
            status = .unreachable
        }
    }

    func stop() {
        client.shutdownEvents()
        signals?.cancel()
        signals = nil
        status = .closed
    }

    /// Reloads events and connector statuses. Returns false when the server refused the token.
    @discardableResult
    func refresh() async -> Bool {
        do {
            events = try await client.listEvents()
            for status in try await client.connectorStatuses() {
                update(status)
            }
            return true
        } catch AppleError.Authentication {
            status = .unauthorized
            return false
        } catch {
            return status != .unauthorized
        }
    }

    func discover(_ tool: DiscoveryTool) async throws -> DiscoverySnapshot {
        discoveryFailure = nil
        let previous = discoveryResults[tool]?.id
        let devices = try await client.discoverDevices(tool: tool)
        if tool == .obs || tool == .rodecaster {
            for _ in 0..<100 {
                if let result = discoveryResults[tool], result.id != previous { return result }
                if let discoveryFailure { throw DiscoveryFailure(message: discoveryFailure) }
                guard status == .connected else { throw DiscoveryFailure(message: "The server connection was lost.") }
                try await Task.sleep(for: .milliseconds(200))
            }
            throw DiscoveryFailure(message: "The device scan timed out. Check the connector and try again.")
        }
        return DiscoverySnapshot(devices: devices)
    }

    /// The files recorded for one event, newest first.
    func recordings(eventId: String) async throws -> [RecordingRecord] {
        try await client.recordings(eventId: eventId)
    }

    /// Marks a recording for upload, then reloads the list.
    func flagUpload(eventId: String, recordingId: String, platforms: [String]) async throws {
        try await client.flagUpload(eventId: eventId, recordingId: recordingId, platforms: platforms)
    }

    /// The server's slide folders and title template.
    func serverSettings() async throws -> ServerSettingsRecord {
        try await client.serverSettings()
    }

    func saveServerSettings(_ settings: ServerSettingsRecord) async throws {
        try await client.saveServerSettings(settings: settings)
    }

    /// Every connector that can be configured, in display order.
    var configurableConnectors: [ConnectorNameRecord] { client.connectors() }

    /// One connector's settings. Stored secrets come back blank but flagged as set.
    func connectorForm(_ connector: String) async throws -> [ConfigFieldRecord] {
        try await client.connectorForm(connector: connector)
    }

    func saveConnector(_ connector: String, values: [ConfigValueRecord]) async throws {
        try await client.saveConnectorForm(connector: connector, values: values)
        await refresh()
    }

    /// Where the operator signs in to YouTube or Facebook; the browser returns to the server.
    func authURL(_ platform: String) async throws -> URL? {
        URL(string: try await client.authUrl(platform: platform))
    }

    func signOut(_ platform: String) async throws {
        try await client.signOut(platform: platform)
        await refresh()
    }

    func savedBroadlinkDevices() async throws -> [DiscoveredDeviceRecord] {
        try await client.savedBroadlinkDevices()
    }

    func networkDeviceConfig(atem: Bool) async throws -> NetworkDeviceConfig {
        try await client.networkDeviceConfig(atem: atem)
    }

    func saveNetworkDevice(atem: Bool, config: NetworkDeviceConfig) async throws {
        try await client.saveNetworkDevice(atem: atem, config: config)
    }

    func event(id: String) async throws -> EventRecord {
        try await client.event(id: id)
    }

    /// Creates the event when `id` is nil, otherwise updates it.
    func save(_ draft: EventDraftRecord, id: String?) async throws -> EventRecord {
        let event = if let id {
            try await client.updateEvent(id: id, draft: draft)
        } else {
            try await client.createEvent(draft: draft)
        }
        await refresh()
        return event
    }

    func deleteEvent(id: String) async throws {
        try await client.deleteEvent(id: id)
        await refresh()
    }

    func titleTemplate() async -> String {
        (try? await client.titleTemplate()) ?? Self.defaultTitleTemplate
    }

    /// `metocast_core::events::DEFAULT_TITLE_TEMPLATE`, for when the server can't be asked.
    static let defaultTitleTemplate = "{date|YYYY.MM.DD.} {title}[ | Textus: {textus}][ Lekció: {leckio}][ | {speaker}]"

    /// The translation the event editor looks references up in, as in Sanctum.
    static let bibleTranslation = "RUF_v2"

    func verses(for reference: String) async throws -> [BibleVerseRecord] {
        try await client.bibleVerses(reference: reference, translation: Self.bibleTranslation)
    }

    /// Sends a presentation command. The server routes it to the web presenter or to Keynote.
    func show(_ command: PresentationCommand) {
        Task { try? await client.presentationControl(command: command) }
    }

    /// PowerPoint files in the server's watched folders whose name contains `filter`.
    func slideFiles(matching filter: String) async throws -> [PptFile] {
        try await client.pptFiles(filter: filter)
    }

    /// Writes a deck per Bible reference on the event. Returns the files written.
    func generateSlides(eventId: String) async throws -> [String] {
        try await client.createEventSlides(id: eventId)
    }

    /// Folder id to name, for labelling search results.
    func folderNames() async throws -> [(String, String)] {
        try await client.pptFolders().map { ($0.id, $0.name) }
    }

    func createSongSlides(title: String, lyrics: String) async throws -> SongSlides {
        try await client.createSongSlides(title: title, lyrics: lyrics)
    }

    /// How this device names itself in everyone's connected-devices list.
    static var deviceLabel: String {
        #if os(macOS)
        "Metocast on " + ProcessInfo.processInfo.hostName.replacingOccurrences(of: ".local", with: "")
        #else
        "Metocast on " + UIDevice.current.name
        #endif
    }

    /// Sends a live production command. Its effect arrives as a state update, and a refusal
    /// arrives as a notification.
    func control(_ control: ProductionControl) {
        Task { try? await client.productionControl(control: control) }
    }

    /// The event happening now or next, by the same rule as `metocast_core::events::current_event`.
    var upNext: EventSummaryRecord? {
        EventSchedule.current(in: events, now: .now)
    }

    private func apply(_ signal: SessionSignal) {
        switch signal {
        case .state(let state):
            apply(state)
        case .event(.connectorStatus(let status)):
            update(status)
        case .event(.connectorStatuses(let statuses)):
            statuses.forEach(update)
        case .event(.eventsList(let events)):
            self.events = events
        case .event(.presenterState(let state)):
            presenter = state
        case .event(.presenterSlideChanged(let current, let total)):
            presenter?.currentSlide = current
            presenter?.totalSlides = total
        case .event(.eventChanged):
            Task { await refresh() }
        case .event(.notification(let level, let message)):
            let icon = switch level {
            case "error": "exclamationmark.triangle"
            case "warning": "exclamationmark.circle"
            default: "info.circle"
            }
            onActivity(ActivityItem(icon: icon, title: message))
        case .event(.obsState(let streaming, let recording)):
            obs = OBSOutputs(streaming: streaming, recording: recording)
        case .event(.atemState(let state)):
            atem = state
        case .event(.middlecontrolState(let state)):
            middlecontrol = state
        case .event(.rodecasterProfile(let profile)):
            rodecaster = profile
        case .event(.rodecasterMute(let channel, let label, let muted, let remote, let notify)):
            if let index = rodecaster?.channels.firstIndex(where: { $0.channel == channel }) {
                if remote {
                    rodecaster?.channels[index].wirelessMute = muted
                } else {
                    rodecaster?.channels[index].mute = muted
                }
            }
            if notify {
                let what = remote ? "\(label)'s wireless mic" : label
                onActivity(ActivityItem(icon: muted ? "mic.slash" : "mic", title: "\(what) \(muted ? "muted" : "unmuted")"))
            }
        case .event(.rodecasterRecorder(let state)):
            recorder = state
        case .event(.presentationSettings(let useWebPresenter, let theme)):
            usesWebPresenter = useWebPresenter
            presenterTheme = theme
        case .event(.presentationStatus(let status)):
            presentationStatus = status
        case .event(.clients(let clients)):
            self.clients = clients
        case .event(.discovery(let tool, let devices, let message)):
            discoveryResults[tool] = DiscoverySnapshot(devices: devices, message: message)
        case .event(.commandFailed(let message)):
            discoveryFailure = message.replacingOccurrences(of: "_", with: " ")
            // ponytail: codes show as words ("atem not connected"); map them to prose if they read badly.
            onActivity(ActivityItem(icon: "exclamationmark.triangle", title: "Couldn't do that: \(message.replacingOccurrences(of: "_", with: " "))"))
        case .event(.connected), .event(.event), .event(.unknown):
            break
        }
    }

    private func apply(_ state: AppleConnectionState) {
        let previous = status
        switch state {
        case .connecting:
            if previous != .connected { status = .connecting }
        case .connected:
            status = .connected
            requestMixerProfile()
            show(.register(label: Self.deviceLabel, hostname: nil))
            if previous == .reconnecting {
                onActivity(ActivityItem(icon: "checkmark.circle", title: "Reconnected to \(ServerAddress.displayName(baseURL))"))
                // Anything could have changed while the socket was down.
                Task { await refresh() }
            }
        case .reconnecting:
            status = .reconnecting
            if previous == .connected {
                onActivity(ActivityItem(icon: "wifi.exclamationmark", title: "Lost connection to \(ServerAddress.displayName(baseURL))"))
            }
        case .authenticationFailed:
            status = .unauthorized
        case .closed:
            status = .closed
        }
    }

    /// The server pushes every other device's state when the socket opens, but the mixer's
    /// channels only on request.
    private func requestMixerProfile() {
        guard status == .connected, connectors["rodecaster"]?.status == .connected else { return }
        control(.rodecasterProfile)
    }

    private func update(_ status: ConnectorStatusRecord) {
        connectors[status.connector] = status
        if status.status == .connected {
            if status.connector == "rodecaster" { requestMixerProfile() }
        } else {
            // A connector that isn't connected has no live state; it's resent when it reconnects.
            switch status.connector {
            case "obs": obs = nil
            case "atem": atem = nil
            case "middlecontrol": middlecontrol = nil
            case "rodecaster": rodecaster = nil
            default: break
            }
        }
        // Retry loops pass through `connecting`, so only a change between settled states is news,
        // and the first snapshot isn't news at all.
        guard status.status != .connecting else { return }
        let previous = settledStatuses.updateValue(status.status, forKey: status.connector)
        guard let previous, previous != status.status else { return }
        let name = ConnectorInfo(id: status.connector).name
        switch status.status {
        case .connected:
            onActivity(ActivityItem(icon: "checkmark.circle", title: "\(name) connected"))
        case .error:
            onActivity(ActivityItem(icon: "exclamationmark.triangle", title: "\(name): \(status.message ?? "error")"))
        case .disconnected where previous == .connected:
            onActivity(ActivityItem(icon: "bolt.horizontal.circle", title: "\(name) disconnected"))
        case .disconnected, .connecting:
            break
        }
    }
}

struct OBSOutputs: Equatable {
    var streaming: Bool
    var recording: Bool
}

/// What the Rust event stream reports, carried from its threads to the main actor.
nonisolated enum SessionSignal: Sendable {
    case state(AppleConnectionState)
    case event(Metocast.AppleEvent)
}

/// Rust calls this from its own runtime threads, so it does nothing but hand each
/// callback to the session's stream.
nonisolated final class SessionListener: EventListener {
    private let continuation: AsyncStream<SessionSignal>.Continuation

    init(continuation: AsyncStream<SessionSignal>.Continuation) {
        self.continuation = continuation
    }

    deinit {
        continuation.finish()
    }

    func onStateChanged(state: AppleConnectionState) {
        continuation.yield(.state(state))
    }

    func onEvent(event: Metocast.AppleEvent) {
        continuation.yield(.event(event))
    }
}

struct ActivityItem: Identifiable {
    let id = UUID()
    var icon: String
    var title: String
    var date = Date.now
    var isRead = false
}

struct DiscoverySnapshot {
    let id = UUID()
    var devices: [DiscoveredDeviceRecord]
    var message: String?
}

struct DiscoveryFailure: LocalizedError {
    var message: String
    var errorDescription: String? { message }
}
