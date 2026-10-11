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
    /// The desk's own SD or USB recorder, which is not Metocast's audio recording above.
    private(set) var deskRecorder: RodecasterDeskRecorder?
    /// The live English transcript, its newest line and the OpenAI credit left.
    private(set) var transcription: TranscriptionRecord?
    private(set) var transcriptLine = ""
    private(set) var transcriptCredit: CreditRecord?
    /// Why the server turned down the last transcript start; the next start clears it.
    private(set) var transcriptRefusal: String?
    private(set) var camera: CameraState?
    /// The OBS sources the server warns about.
    private(set) var deviceAlerts: [DeviceListenerRecord] = []
    /// Rules that run inside the server: when one device does something, it does something on
    /// others. Every client sees the same ones.
    private(set) var automations: [AutomationRecord] = []
    /// The master switch: nothing fires while it is off.
    private(set) var automationsArmed = true
    /// The last 50 runs, newest first.
    private(set) var automationRuns: [AutomationRunRecord] = []
    // Slides. The server decides whether a presentation opens in the web presenter or Keynote.
    private(set) var usesWebPresenter = true
    private(set) var presenterTheme: PresenterTheme = .classic
    private(set) var presentationStatus: PresentationStatus?
    /// The Order of Service step on screen, as (event id, step id), on every client alike.
    private(set) var orderPosition: (eventId: String, itemId: String)?
    /// Bumped when any event's Order of Service changes, so open screens reload it.
    private(set) var orderRevision = 0
    private(set) var clients: [ConnectedClientRecord] = []
    /// Video renders this session has queued or heard about, by job id.
    private(set) var renders: [String: RenderProgressRecord] = [:]
    /// Audio masters this session has queued or heard about, by job id.
    private(set) var audioJobs: [String: AudioProgressRecord] = [:]
    /// Programs (a video with its audio, synced) this session has queued or heard about, by job id.
    private(set) var programJobs: [String: ProgramProgressRecord] = [:]
    /// The event each video job was filed under, by job id; a job with none isn't listed.
    private(set) var jobEvents: [String: String] = [:]

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
        await loadAutomations()

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

    // ── Server tools ─────────────────────────────────────────────────────────

    /// Manages the OBS sources the server watches; the list arrives as an event.
    func alerts(_ command: DeviceAlertControl) {
        Task { try? await client.deviceAlertControl(command: command) }
    }

    // ── Automations ──────────────────────────────────────────────────────────

    /// Loads the rules and the run history. After this, changes arrive as events.
    func loadAutomations() async {
        if let list = try? await client.automations() {
            automations = list.automations
            automationsArmed = list.armed
        }
        if let runs = try? await client.automationRuns() {
            automationRuns = runs
        }
    }

    /// Creates the rule when its id is empty, otherwise replaces it. A rule the server refuses
    /// throws with the server's own words.
    @discardableResult
    func saveAutomation(_ rule: AutomationRecord) async throws -> AutomationRecord {
        let saved = try await client.saveAutomation(automation: rule)
        await loadAutomations()
        return saved
    }

    func deleteAutomation(id: String) async throws {
        try await client.deleteAutomation(id: id)
        await loadAutomations()
    }

    /// Runs the rule now, whatever its cooldown and the master switch say. It really records and
    /// moves cameras. Returns when the run ends.
    func runAutomation(id: String) async throws -> AutomationRunRecord {
        try await client.runAutomation(id: id)
    }

    func setAutomationsArmed(_ armed: Bool) async throws {
        try await client.setAutomationsArmed(armed: armed)
        automationsArmed = armed
    }

    /// The newest run of a rule, for its row in the list.
    func lastRun(of automationId: String) -> AutomationRunRecord? {
        automationRuns.first { $0.automationId == automationId }
    }

    func cronJobs() async throws -> [CronJobRecord] { try await client.cronJobs() }

    @discardableResult
    func saveCronJob(id: String, job: CronJobRecord) async throws -> CronJobRecord {
        try await client.saveCronJob(id: id, job: job)
    }

    func deleteCronJob(id: String) async throws { try await client.deleteCronJob(id: id) }

    func queues() async throws -> [QueueSummaryRecord] { try await client.queues() }

    func queueJobs(_ queue: String) async throws -> [QueueJobRecord] {
        try await client.queueJobs(queue: queue)
    }

    func retryJob(id: String) async throws { try await client.retryJob(id: id) }
    func purgeJob(id: String) async throws { try await client.purgeJob(id: id) }
    func triggerUploads() async throws { try await client.triggerUploads() }

    func broadlinkCommands() async throws -> [BroadlinkCommandRecord] {
        try await client.broadlinkCommands()
    }

    func sendBroadlinkCommand(id: String) async throws {
        try await client.sendBroadlinkCommand(id: id)
    }

    func untrackedRecordings() async throws -> [UntrackedRecordingRecord] {
        try await client.untrackedRecordings()
    }

    func assignUntracked(id: String, eventId: String) async throws {
        try await client.assignUntracked(id: id, eventId: eventId)
    }

    func deleteUntracked(id: String) async throws { try await client.deleteUntracked(id: id) }

    // ── Video ────────────────────────────────────────────────────────────────

    /// A video file on the server's disk and the render the stored settings would make of it.
    /// With a `.sidecar` (such as Resolve's), its settings become the defaults.
    func probeMedia(path: String, sidecarPath: String?) async throws -> MediaProbeRecord {
        try await client.probeMedia(path: path, sidecarPath: sidecarPath)
    }

    /// One frame as recorded (`settings` nil) or graded, for the Media viewer.
    func previewMedia(path: String, frame: UInt64, settings: RenderSettings?) async throws -> PreviewFrame {
        try await client.previewMedia(path: path, frame: frame, settings: settings, maxWidth: 1600)
    }

    /// Queues a render; its progress then arrives in `renders`.
    func startRender(path: String, eventId: String?, settings: RenderSettings?) async throws {
        let queued = try await client.startRender(path: path, eventId: eventId, settings: settings)
        jobEvents[queued.jobId] = eventId
        renders[queued.jobId] = RenderProgressRecord(
            jobId: queued.jobId,
            state: .running,
            frame: 0,
            totalFrames: 0,
            fps: 0,
            outputPath: queued.outputPath,
            error: nil
        )
    }

    /// Stops a queued or running render, audio or program job.
    func cancelRender(jobId: String) async throws { try await client.cancelRender(jobId: jobId) }

    // ── Audio ────────────────────────────────────────────────────────────────

    /// Audio files on the server's disk, joined in the order given.
    func probeAudio(paths: [String]) async throws -> AudioProbeRecord {
        try await client.probeAudio(paths: paths)
    }

    /// Queues an audio master; its progress then arrives in `audioJobs`.
    func startAudio(paths: [String], eventId: String?, settings: AudioSettings?) async throws {
        let queued = try await client.startAudio(paths: paths, eventId: eventId, settings: settings)
        audioJobs[queued.jobId] = AudioProgressRecord(
            jobId: queued.jobId,
            state: .running,
            step: nil,
            fraction: nil,
            outputPath: queued.outputPath,
            error: nil,
            report: nil
        )
    }

    // ── Video + audio ────────────────────────────────────────────────────────

    /// A video and its audio (none: the video's own sound), and how the server would sync them.
    func probeProgram(videoPath: String, audioPaths: [String]) async throws -> ProgramProbeRecord {
        try await client.probeProgram(videoPath: videoPath, audioPaths: audioPaths)
    }

    /// Queues a program; its progress then arrives in `programJobs`.
    func startProgram(_ request: ProgramRequestRecord) async throws {
        let queued = try await client.startProgram(request: request)
        jobEvents[queued.jobId] = request.eventId
        programJobs[queued.jobId] = ProgramProgressRecord(
            jobId: queued.jobId,
            state: .running,
            step: nil,
            fraction: nil,
            outputPath: queued.outputPath,
            error: nil,
            report: nil
        )
    }

    /// Moves a finished program's sound by whole frames (positive: later).
    func nudgeProgram(jobId: String, frames: Int32) async throws {
        try await client.nudgeProgram(jobId: jobId, frames: frames)
    }

    /// Book and chapter autocomplete for a Bible reference.
    func bibleSuggestions(_ term: String) async throws -> [BibleSuggestionRecord] {
        try await client.bibleSuggestions(term: term)
    }

    /// The core's own log; only a core inside the desktop app has one.
    func applicationLog() async throws -> String { try await client.applicationLog() }

    /// The stored secrets of one connector. Only the Mac hosting the server can read them.
    func connectorSecrets(_ connector: String, adminToken: String) async throws -> [ConfigFieldRecord] {
        try await client.connectorSecrets(connector: connector, adminToken: adminToken)
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

    /// A Bible translation to look a reference up in. The `_v2` ones come from our own API; the others from
    /// szentiras.eu, which needs the key held by the Szentírás connector. Mirrors `TRANSLATIONS` in core-client.
    struct BibleTranslationOption: Hashable {
        let code: String
        let name: String
        var isOurs: Bool { code.hasSuffix("_v2") }
    }

    static let bibleTranslations = [
        BibleTranslationOption(code: "UF_v2", name: "UF (v2)"),
        BibleTranslationOption(code: "RUF_v2", name: "RUF (v2)"),
        BibleTranslationOption(code: "RUF", name: "RUF"),
        BibleTranslationOption(code: "KG", name: "KG"),
        BibleTranslationOption(code: "KNB", name: "KNB"),
        BibleTranslationOption(code: "SZIT", name: "SZIT"),
        BibleTranslationOption(code: "BD", name: "BD"),
        BibleTranslationOption(code: "STL", name: "STL"),
    ]

    /// What a new reference is looked up in, as in Sanctum.
    static let defaultBibleTranslation = "RUF_v2"

    func verses(for reference: String, translation: String) async throws -> [BibleVerseRecord] {
        try await client.bibleVerses(reference: reference, translation: translation)
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

    /// The folders the server watches for decks.
    func slideFolders() async throws -> [PptFolderRecord] {
        try await client.pptFolders()
    }

    /// Watches a folder on the server's disk; the server refuses a path that isn't a folder there.
    func addSlideFolder(path: String) async throws {
        try await client.addPptFolder(path: path, name: URL(filePath: path).lastPathComponent)
    }

    func removeSlideFolder(id: String) async throws {
        try await client.removePptFolder(id: id)
    }

    /// An event's Order of Service, in order.
    func order(eventId: String) async throws -> [OrderItemRecord] {
        try await client.eventOrder(id: eventId)
    }

    /// Replaces an event's Order of Service. Steps that keep their id stay the live step.
    func saveOrder(eventId: String, items: [OrderItemRecord]) async throws -> [OrderItemRecord] {
        try await client.setEventOrder(id: eventId, items: items)
    }

    func createSongSlides(title: String, lyrics: String) async throws -> SongSlides {
        try await client.createSongSlides(title: title, lyrics: lyrics)
    }

    /// How this device names itself in everyone's connected-devices list.
    static var deviceLabel: String {
        #if os(macOS)
        "Metocast on " + (Redaction.isOn ? Redaction.host : ProcessInfo.processInfo.hostName.replacingOccurrences(of: ".local", with: ""))
        #else
        "Metocast on " + UIDevice.current.name
        #endif
    }

    /// Sends a live production command. Its effect arrives as a state update, and a refusal
    /// arrives as a notification.
    func control(_ control: ProductionControl) {
        if case .transcriptionStart = control { transcriptRefusal = nil }
        Task { try? await client.productionControl(control: control) }
    }

    /// A refused command's server code, in words.
    private static func refusalText(_ message: String) -> String {
        if message.contains("rodecaster_multitrack_endpoint_missing") {
            return "The RØDECaster's multitrack USB audio isn't reaching this Mac. Connect USB 1 and enable multitrack."
        }
        if message.hasPrefix("transcription_not_configured") {
            return "Add an OpenAI API key in Settings → Connectors."
        }
        if message.hasPrefix("rodecaster_audio_recording_disabled") {
            return "Audio recording is off. Turn on Record Audio and choose a folder in Settings → Connectors → RØDECaster."
        }
        if message.hasPrefix("recording_directory_not_found") {
            return "The recording folder doesn't exist on the server's Mac. Set it in Settings → Connectors → RØDECaster."
        }
        return message.replacingOccurrences(of: "_", with: " ")
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
        case .event(.orderState(let eventId, let itemId)):
            orderPosition = eventId.flatMap { event in itemId.map { (event, $0) } }
        case .event(.eventOrderChanged):
            orderRevision += 1
        case .event(.notification(let level, let message)):
            let icon = switch level {
            case "error": "exclamationmark.triangle"
            case "warn": "exclamationmark.circle"
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
        case .event(.rodecasterDeskRecorder(let state)):
            deskRecorder = state
        case .event(.automationsChanged(let armed, let automations)):
            automationsArmed = armed
            self.automations = automations
        case .event(.automationRun(let run)):
            // The caller of Run Now gets the run back too, so it may be here already.
            if !automationRuns.contains(where: { $0.id == run.id }) {
                automationRuns.insert(run, at: 0)
                if automationRuns.count > 50 { automationRuns.removeLast(automationRuns.count - 50) }
            }
            // A failure already reached the Notifications list as the server's own warning.
            if !run.didFail {
                onActivity(ActivityItem(icon: "bolt.circle", title: "\(run.name) ran: \(run.cause)"))
            }
        case .event(.transcription(let state)):
            transcription = state
            if state.status == .connecting || state.status == .running { transcriptRefusal = nil }
        case .event(.transcriptionSnapshot(let state, let line, let credit)):
            transcription = state
            transcriptLine = line
            transcriptCredit = credit
        case .event(.transcriptLine(let text)):
            transcriptLine = text
        case .event(.transcriptionCredit(let credit)):
            transcriptCredit = credit
        case .event(.cameraState(let streaming, let recording, let status)):
            camera = CameraState(streaming: streaming, recording: recording, streamStatus: status)
        case .event(.deviceAlerts(let listeners)):
            deviceAlerts = listeners
        case .event(.mediaRenderProgress(let progress)):
            renders[progress.jobId] = progress
            let file = URL(fileURLWithPath: progress.outputPath).lastPathComponent
            switch progress.state {
            case .succeeded:
                onActivity(ActivityItem(icon: "film", title: "Render finished: \(file)"))
            case .failed:
                onActivity(ActivityItem(icon: "exclamationmark.triangle", title: "Render failed: \(progress.error ?? file)"))
            case .running, .cancelled:
                break
            }
        case .event(.mediaAudioProgress(let progress)):
            // A pending job cancelled before it ran reports no output path; keep the one we know.
            var progress = progress
            if progress.outputPath.isEmpty, let known = audioJobs[progress.jobId] {
                progress.outputPath = known.outputPath
            }
            audioJobs[progress.jobId] = progress
            let file = URL(fileURLWithPath: progress.outputPath).lastPathComponent
            switch progress.state {
            case .succeeded:
                onActivity(ActivityItem(icon: "waveform", title: "Audio ready: \(file)"))
            case .failed:
                onActivity(ActivityItem(icon: "exclamationmark.triangle", title: "Audio failed: \(progress.error ?? file)"))
            case .running, .cancelled:
                break
            }
        case .event(.mediaProgramProgress(let progress)):
            // A cancelled pending job reports no output path, and a nudge in progress no report:
            // keep the ones we know, so the row still says what the program is.
            var progress = progress
            let known = programJobs[progress.jobId]
            if progress.outputPath.isEmpty, let known {
                progress.outputPath = known.outputPath
            }
            if progress.report == nil, let report = known?.report {
                progress.report = report
            }
            programJobs[progress.jobId] = progress
            let file = URL(fileURLWithPath: progress.outputPath).lastPathComponent
            switch progress.state {
            case .succeeded:
                onActivity(ActivityItem(icon: "film.stack", title: "Video with audio ready: \(file)"))
            case .failed:
                onActivity(ActivityItem(icon: "exclamationmark.triangle", title: "Video with audio failed: \(progress.error ?? file)"))
            case .running, .cancelled:
                break
            }
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
            let words = Self.refusalText(message)
            if message.hasPrefix("transcription_") { transcriptRefusal = words }
            discoveryFailure = message.replacingOccurrences(of: "_", with: " ")
            // ponytail: codes show as words ("atem not connected"); map them to prose if they read badly.
            onActivity(ActivityItem(icon: "exclamationmark.triangle", title: "Couldn't do that: \(words)", isAlert: true))
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
                Task {
                    await refresh()
                    await loadAutomations()
                }
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
            case "rodecaster":
                rodecaster = nil
                deskRecorder = nil
            case "blackmagic-camera": camera = nil
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

/// The camera reports its own word for streaming as well as the two flags.
struct CameraState: Equatable {
    var streaming: Bool
    var recording: Bool
    var streamStatus: String
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
    /// Something the operator has to act on, so it also goes out as a system notification.
    var isAlert = false
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
