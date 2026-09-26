import SwiftUI
import Metocast

/// The server's housekeeping: what runs on a schedule, what is uploading, the remotes, the
/// sources OBS watches, recordings that belong to no event, the caption overlay and the log.
struct ServerToolsView: View {
    let session: MetocastSession

    var body: some View {
        List {
            NavigationLink { AutomationsView(session: session) } label: {
                Label("Automations", systemImage: "clock.arrow.trianglehead.2.counterclockwise.rotate.90")
            }
            NavigationLink { UploadsView(session: session) } label: {
                Label("Uploads", systemImage: "arrow.up.circle")
            }
            NavigationLink { RemotesView(session: session) } label: {
                Label("Remotes", systemImage: "av.remote")
            }
            NavigationLink { DeviceAlertsView(session: session) } label: {
                Label("Device Alerts", systemImage: "bell.badge")
            }
            NavigationLink { UntrackedRecordingsView(session: session) } label: {
                Label("Unassigned Recordings", systemImage: "questionmark.folder")
            }
            NavigationLink { CaptionOverlayView(session: session) } label: {
                Label("Caption Overlay", systemImage: "text.below.photo")
            }
            NavigationLink { CoreLogView(session: session) } label: {
                Label("Core Log", systemImage: "doc.text")
            }
        }
        .navigationTitle("Server Tools")
    }
}

/// Jobs the server runs on a schedule.
private struct AutomationsView: View {
    let session: MetocastSession
    @State private var jobs: [CronJobRecord] = []
    @State private var editing: CronJobRecord?
    @State private var isAdding = false
    @State private var failure: String?

    var body: some View {
        List {
            ForEach(jobs, id: \.id) { job in
                Button {
                    editing = job
                } label: {
                    VStack(alignment: .leading, spacing: 2) {
                        Text(job.name)
                        Text("\(job.cronExpression)\(job.enabled ? "" : " · off")")
                            .font(.subheadline)
                            .foregroundStyle(.secondary)
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .contentShape(.rect)
                }
                .buttonStyle(.plain)
            }
            .onDelete { offsets in
                Task { await delete(offsets) }
            }
            if let failure {
                Label(failure, systemImage: "exclamationmark.triangle").foregroundStyle(.red)
            }
        }
        .overlay {
            if jobs.isEmpty {
                ContentUnavailableView("No Automations", systemImage: "clock", description: Text("Scheduled pulls and uploads show up here."))
            }
        }
        .navigationTitle("Automations")
        .toolbar {
            ToolbarItem(placement: .primaryAction) {
                Button("New Automation", systemImage: "plus") { isAdding = true }
            }
        }
        .sheet(isPresented: $isAdding) {
            CronJobEditor(session: session, job: nil) { Task { await load() } }
        }
        .sheet(item: $editing) { job in
            CronJobEditor(session: session, job: job) { Task { await load() } }
        }
        .task { await load() }
    }

    private func load() async {
        do {
            jobs = try await session.cronJobs()
            failure = nil
        } catch {
            failure = error.metocastMessage
        }
    }

    private func delete(_ offsets: IndexSet) async {
        for index in offsets {
            try? await session.deleteCronJob(id: jobs[index].id)
        }
        await load()
    }
}

extension CronJobRecord: @retroactive Identifiable {}

/// One scheduled job. The expression is the server's, so it is entered as it is stored.
private struct CronJobEditor: View {
    let session: MetocastSession
    let job: CronJobRecord?
    var onSaved: () -> Void
    @Environment(\.dismiss) private var dismiss
    @State private var name = ""
    @State private var expression = "0 9 * * 0"
    @State private var enabled = true
    @State private var pullYoutube = false
    @State private var autoUpload = false
    @State private var failure: String?

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    LabeledContent("Name") {
                        TextField("Name", text: $name).multilineTextAlignment(.trailing).labelsHidden()
                    }
                    LabeledContent("Schedule") {
                        TextField("0 9 * * 0", text: $expression)
                            .multilineTextAlignment(.trailing)
                            .labelsHidden()
                            #if os(iOS)
                            .textInputAutocapitalization(.never)
                            .autocorrectionDisabled()
                            #endif
                    }
                    Toggle("Enabled", isOn: $enabled)
                } footer: {
                    Text("Five cron fields: minute, hour, day, month, weekday.")
                }
                Section("What It Does") {
                    Toggle("Pull YouTube Content", isOn: $pullYoutube)
                    Toggle("Upload Flagged Recordings", isOn: $autoUpload)
                }
                if let failure {
                    Label(failure, systemImage: "exclamationmark.triangle").foregroundStyle(.red)
                }
            }
            .formStyle(.grouped)
            .navigationTitle(job == nil ? "New Automation" : "Automation")
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Save") { Task { await save() } }
                        .disabled(name.trimmed.isEmpty || expression.trimmed.isEmpty)
                }
            }
            .onAppear {
                guard let job else { return }
                name = job.name
                expression = job.cronExpression
                enabled = job.enabled
                pullYoutube = job.pullYoutube
                autoUpload = job.autoUpload
            }
        }
    }

    private func save() async {
        do {
            try await session.saveCronJob(
                id: job?.id ?? "",
                job: CronJobRecord(
                    id: job?.id ?? "",
                    name: name.trimmed,
                    cronExpression: expression.trimmed,
                    enabled: enabled,
                    pullYoutube: pullYoutube,
                    autoUpload: autoUpload
                )
            )
            onSaved()
            dismiss()
        } catch {
            failure = error.metocastMessage
        }
    }
}

/// What the upload workers are doing, and the jobs that failed.
private struct UploadsView: View {
    let session: MetocastSession
    @State private var queues: [QueueSummaryRecord] = []
    @State private var jobs: [String: [QueueJobRecord]] = [:]
    @State private var failure: String?

    var body: some View {
        List {
            Section {
                Button("Run Uploads Now", systemImage: "arrow.up.circle") {
                    Task {
                        do { try await session.triggerUploads() } catch { failure = error.metocastMessage }
                    }
                }
            }
            ForEach(queues, id: \.queue) { queue in
                Section(queue.queue.capitalized) {
                    Text("\(queue.pending) waiting · \(queue.processing) running · \(queue.succeeded) done · \(queue.dead) failed")
                        .font(.subheadline)
                        .foregroundStyle(.secondary)
                    ForEach(jobs[queue.queue] ?? [], id: \.id) { job in
                        VStack(alignment: .leading, spacing: 4) {
                            Text("\(job.jobType) · \(job.status)")
                            if let error = job.lastError {
                                Text(error).font(.subheadline).foregroundStyle(.red).lineLimit(3)
                            }
                            if job.status == "dead" {
                                HStack {
                                    Button("Retry") { Task { try? await session.retryJob(id: job.id); await load() } }
                                    Button("Discard", role: .destructive) { Task { try? await session.purgeJob(id: job.id); await load() } }
                                }
                                .font(.subheadline)
                                .buttonStyle(.bordered)
                            }
                        }
                    }
                }
            }
            if let failure {
                Label(failure, systemImage: "exclamationmark.triangle").foregroundStyle(.red)
            }
        }
        .overlay {
            if queues.isEmpty {
                ContentUnavailableView("Nothing Queued", systemImage: "tray", description: Text("Uploads and other background work show up here."))
            }
        }
        .navigationTitle("Uploads")
        .refreshable { await load() }
        .task { await load() }
    }

    private func load() async {
        do {
            queues = try await session.queues()
            for queue in queues {
                // Only the interesting ones: a queue of finished jobs is noise.
                jobs[queue.queue] = try await session.queueJobs(queue.queue)
                    .filter { $0.status != "succeeded" }
            }
            failure = nil
        } catch {
            failure = error.metocastMessage
        }
    }
}

/// The saved RF and IR commands, for the projector and the screen.
private struct RemotesView: View {
    let session: MetocastSession
    @State private var commands: [BroadlinkCommandRecord] = []
    @State private var failure: String?
    @State private var sent: String?

    var body: some View {
        List {
            ForEach(commands, id: \.id) { command in
                Button {
                    Task { await send(command) }
                } label: {
                    LabeledContent {
                        if sent == command.id {
                            Image(systemName: "checkmark").foregroundStyle(.green)
                        }
                    } label: {
                        Text(command.name)
                        if !command.category.isEmpty {
                            Text(command.category.capitalized)
                        }
                    }
                    .contentShape(.rect)
                }
                .buttonStyle(.plain)
            }
            if let failure {
                Label(failure, systemImage: "exclamationmark.triangle").foregroundStyle(.red)
            }
        }
        .overlay {
            if commands.isEmpty {
                ContentUnavailableView(
                    "No Commands",
                    systemImage: "av.remote",
                    description: Text("Commands learned on the server show up here.")
                )
            }
        }
        .navigationTitle("Remotes")
        .task { await load() }
    }

    private func load() async {
        do {
            commands = try await session.broadlinkCommands()
            failure = nil
        } catch {
            failure = error.metocastMessage
        }
    }

    private func send(_ command: BroadlinkCommandRecord) async {
        do {
            try await session.sendBroadlinkCommand(id: command.id)
            sent = command.id
            try? await Task.sleep(for: .seconds(2))
            if sent == command.id { sent = nil }
        } catch {
            failure = error.metocastMessage
        }
    }
}

/// The OBS sources the server watches, so it can warn when one goes missing.
private struct DeviceAlertsView: View {
    let session: MetocastSession
    @State private var isScanning = false

    var body: some View {
        List {
            Section {
                ForEach(session.deviceAlerts, id: \.id) { listener in
                    VStack(alignment: .leading, spacing: 2) {
                        Text(listener.friendlyName)
                        Text("\(listener.deviceItemName) · \(listener.category.replacing("_", with: " "))")
                            .font(.subheadline)
                            .foregroundStyle(.secondary)
                    }
                }
                .onDelete { offsets in
                    for index in offsets {
                        session.alerts(.delete(id: session.deviceAlerts[index].id))
                    }
                    session.alerts(.list)
                }
            } footer: {
                Text("The server warns when a watched source stops appearing in OBS.")
            }
            Section {
                Button(isScanning ? "Scanning…" : "Scan OBS Devices", systemImage: "arrow.clockwise") {
                    isScanning = true
                    session.alerts(.scan)
                    Task {
                        try? await Task.sleep(for: .seconds(3))
                        isScanning = false
                    }
                }
                .disabled(session.connectors["obs"]?.status != .connected)
            } footer: {
                if session.connectors["obs"]?.status != .connected {
                    Text("Connect OBS to scan its devices.")
                }
            }
        }
        .overlay {
            if session.deviceAlerts.isEmpty {
                ContentUnavailableView(
                    "No Watched Sources",
                    systemImage: "bell.slash",
                    description: Text("Sources added on the server show up here.")
                )
            }
        }
        .navigationTitle("Device Alerts")
        .task { session.alerts(.list) }
    }
}

/// Recordings the server found that belong to no event yet.
private struct UntrackedRecordingsView: View {
    let session: MetocastSession
    @State private var recordings: [UntrackedRecordingRecord] = []
    @State private var assigning: UntrackedRecordingRecord?
    @State private var failure: String?

    var body: some View {
        List {
            ForEach(recordings, id: \.id) { recording in
                VStack(alignment: .leading, spacing: 4) {
                    Text(recording.fileName)
                    Text(ByteCountFormatter.string(fromByteCount: recording.fileSize, countStyle: .file))
                        .font(.subheadline)
                        .foregroundStyle(.secondary)
                    Button("Assign to an Event") { assigning = recording }
                        .font(.subheadline)
                }
            }
            .onDelete { offsets in
                Task {
                    for index in offsets { try? await session.deleteUntracked(id: recordings[index].id) }
                    await load()
                }
            }
            if let failure {
                Label(failure, systemImage: "exclamationmark.triangle").foregroundStyle(.red)
            }
        }
        .overlay {
            if recordings.isEmpty {
                ContentUnavailableView(
                    "Nothing Unassigned",
                    systemImage: "folder",
                    description: Text("Recordings the server can't match to an event show up here.")
                )
            }
        }
        .navigationTitle("Unassigned")
        .sheet(item: $assigning) { recording in
            EventPicker(session: session) { event in
                Task {
                    do {
                        try await session.assignUntracked(id: recording.id, eventId: event.id)
                        await load()
                    } catch {
                        failure = error.metocastMessage
                    }
                }
            }
        }
        .task { await load() }
    }

    private func load() async {
        do {
            recordings = try await session.untrackedRecordings()
            failure = nil
        } catch {
            failure = error.metocastMessage
        }
    }
}

extension UntrackedRecordingRecord: @retroactive Identifiable {}

/// Picks one of the server's events.
private struct EventPicker: View {
    let session: MetocastSession
    let onPick: (EventSummaryRecord) -> Void
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        NavigationStack {
            List(session.events, id: \.id) { event in
                Button {
                    onPick(event)
                    dismiss()
                } label: {
                    EventRow(event: event).contentShape(.rect)
                }
                .buttonStyle(.plain)
            }
            .navigationTitle("Choose an Event")
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } }
            }
        }
    }
}

/// The browser source OBS shows over the stream. This builds its address.
private struct CaptionOverlayView: View {
    let session: MetocastSession
    @State private var kind = "caption"
    @State private var resolution = "1080p"
    @State private var showLogo = true
    @State private var copied = false

    private var url: String {
        var components = URLComponents(string: session.baseURL.hasSuffix("/") ? session.baseURL + "caption" : session.baseURL + "/caption")
        components?.queryItems = [
            URLQueryItem(name: "type", value: kind),
            URLQueryItem(name: "resolution", value: resolution == "4k" ? "4k" : "1080p"),
            URLQueryItem(name: "showLogo", value: showLogo ? "true" : "false"),
        ]
        return components?.url?.absoluteString ?? ""
    }

    var body: some View {
        Form {
            Section {
                Picker("Overlay", selection: $kind) {
                    Text("Caption").tag("caption")
                    Text("Full Screen").tag("full")
                    Text("Preview").tag("preview")
                }
                Picker("Resolution", selection: $resolution) {
                    Text("1080p").tag("1080p")
                    Text("4K").tag("4k")
                }
                Toggle("Show Logo", isOn: $showLogo)
            } footer: {
                Text("The overlay shows the current event's title, which the server fills in.")
            }
            Section {
                Text(url).font(.footnote.monospaced()).textSelection(.enabled)
                Button(copied ? "Copied" : "Copy Address", systemImage: "doc.on.doc") {
                    #if os(macOS)
                    NSPasteboard.general.clearContents()
                    NSPasteboard.general.setString(url, forType: .string)
                    #else
                    UIPasteboard.general.string = url
                    #endif
                    copied = true
                    Task {
                        try? await Task.sleep(for: .seconds(2))
                        copied = false
                    }
                }
            } footer: {
                Text("Add it in OBS as a Browser source.")
            }
        }
        .formStyle(.grouped)
        .navigationTitle("Caption Overlay")
    }
}

/// The core's own log, when the core has one.
private struct CoreLogView: View {
    let session: MetocastSession
    @State private var log = ""
    @State private var failure: String?

    var body: some View {
        ScrollView {
            Text(failure ?? log)
                .font(.footnote.monospaced())
                .textSelection(.enabled)
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding()
        }
        .navigationTitle("Core Log")
        .toolbar {
            ToolbarItem(placement: .primaryAction) {
                Button("Reload", systemImage: "arrow.clockwise") { Task { await load() } }
            }
        }
        .task { await load() }
    }

    private func load() async {
        do {
            log = try await session.applicationLog()
            failure = log.isEmpty ? "The log is empty." : nil
        } catch {
            // A headless core has no log of its own; the Mac shows the helper's output instead.
            failure = error.metocastMessage
        }
    }
}
