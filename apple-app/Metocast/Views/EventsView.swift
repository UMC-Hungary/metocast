import SwiftUI
import UniformTypeIdentifiers
import Metocast

struct EventsView: View {
    @Environment(AppModel.self) private var model
    @State private var query = ""
    @State private var pendingBroadcast: BroadcastAction?

    var body: some View {
        let events = matching
        let now = Date.now
        let upcoming = events.filter { !$0.isCompleted && ($0.date ?? now) >= now.addingTimeInterval(-4 * 60 * 60) }
            .sorted { ($0.date ?? now) < ($1.date ?? now) }
        let past = events.filter { event in !upcoming.contains { $0.id == event.id } }
            .sorted { ($0.date ?? now) > ($1.date ?? now) }

        List {
            if !upcoming.isEmpty {
                Section("Upcoming") { rows(upcoming) }
            }
            if !past.isEmpty {
                Section("Past") { rows(past) }
            }
        }
        .overlay {
            if events.isEmpty {
                if query.isEmpty {
                    ContentUnavailableView("No Events", systemImage: "calendar")
                } else {
                    ContentUnavailableView.search(text: query)
                }
            }
        }
        .navigationTitle("Events")
        .searchable(text: $query, prompt: "Search")
        .navigationDestination(for: EventSummaryRecord.self) { EventDetailView(summary: $0).windowActions() }
        .refreshable { await model.session?.refresh() }
        .confirmingBroadcast($pendingBroadcast, session: model.session)
    }

    private var matching: [EventSummaryRecord] {
        let events = model.session?.events ?? []
        guard !query.isEmpty else { return events }
        return events.filter { $0.title.localizedStandardContains(query) || $0.speaker.localizedStandardContains(query) }
    }

    private func rows(_ events: [EventSummaryRecord]) -> some View {
        ForEach(events, id: \.id) { event in
            NavigationLink(value: event) {
                EventRow(event: event)
                    .padding(.vertical, 4)
            }
            // A Mac list row link is otherwise an element of unknown role, which doesn't say it opens.
            .accessibilityAddTraits(.isButton)
            // Off the row's tap target: tapping still opens the event.
            .contextMenu {
                BroadcastButtons(eventId: event.id, connections: event.connections, pending: $pendingBroadcast)
            }
            .swipeActions(edge: .leading) {
                BroadcastButtons(eventId: event.id, connections: event.connections, pending: $pendingBroadcast)
            }
        }
    }
}

struct EventDetailView: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    let summary: EventSummaryRecord
    @State private var event: EventRecord?
    @State private var loadFailed = false
    @State private var isEditing = false
    @State private var isConfirmingDelete = false
    @State private var isWritingSlides = false
    @State private var slidesResult: String?
    @State private var isAskingForSlideFolder = false
    @State private var isChoosingSlideFolder = false
    @State private var pendingBroadcast: BroadcastAction?

    var body: some View {
        // The full record once loaded, so edits show without going back to the list.
        let title = event?.title ?? summary.title
        let speaker = event?.speaker ?? summary.speaker
        let computedTitle = event?.computedTitle ?? summary.computedTitle
        Form {
            Section {
                LabeledContent("Date", value: (event?.date ?? summary.date)?.eventDisplay ?? summary.dateTime)
                if !speaker.isEmpty {
                    LabeledContent("Speaker", value: speaker)
                }
                if !computedTitle.isEmpty, computedTitle != title {
                    LabeledContent("Published Title") {
                        Text(computedTitle).textSelection(.enabled)
                    }
                }
            }
            if let event {
                if !event.description.isEmpty {
                    Section("Description") {
                        Text(event.description).textSelection(.enabled)
                    }
                }
                // The list's copy follows `event.changed`, so a broadcast going live shows here too.
                let connections = model.session?.events.first { $0.id == summary.id }?.connections ?? event.connections
                if !connections.isEmpty {
                    Section("Streaming") {
                        ForEach(connections, id: \.platform) { connection in
                            let link = connection.link
                            LabeledContent(connection.platformName) {
                                if connection.hasBroadcast {
                                    BroadcastBadge(connection: connection)
                                }
                                if let link {
                                    Link(connection.hasBroadcast ? String(localized: "Open") : connection.stateName, destination: link)
                                } else if !connection.hasBroadcast {
                                    Text(connection.stateName)
                                }
                            }
                        }
                        BroadcastButtons(eventId: summary.id, connections: connections, pending: $pendingBroadcast)
                            .buttonStyle(.glassProminent)
                    }
                }
                if let session = model.session {
                    // Lekció before Textus, as the service reads them.
                    ForEach(["leckio", "textus"], id: \.self) { type in
                        EventReadingSection(session: session, event: event, type: type) { self.event = $0 }
                    }
                }
                if let session = model.session {
                    EventOrderSection(session: session, event: event)
                }
                RecordingsSection(eventId: summary.id)
                if !event.bibleReferences.isEmpty {
                    Section {
                        Button("Generate Slides", systemImage: "rectangle.on.rectangle") {
                            Task { await writeSlides() }
                        }
                        .disabled(isWritingSlides)
                    } footer: {
                        Text("Writes a deck for each Bible reference into the server's slide folder.")
                    }
                }
                Section {
                    // Attached to the button so the iOS 26 popover points at it.
                    Button("Delete Event", role: .destructive) { isConfirmingDelete = true }
                        .confirmationDialog("Delete this event?", isPresented: $isConfirmingDelete) {
                            Button("Delete Event", role: .destructive) {
                                Task {
                                    try? await model.session?.deleteEvent(id: summary.id)
                                    dismiss()
                                }
                            }
                        } message: {
                            Text("This can't be undone.")
                        }
                }
            } else if loadFailed {
                Section {
                    Label("Couldn't load the rest of this event.", systemImage: "exclamationmark.triangle")
                        .foregroundStyle(.secondary)
                }
            } else {
                Section {
                    ProgressView().frame(maxWidth: .infinity)
                }
            }
        }
        .formStyle(.grouped)
        .navigationTitle(title)
        .confirmingBroadcast($pendingBroadcast, session: model.session)
        .toolbar {
            ToolbarItem(placement: .primaryAction) {
                Button("Edit") { isEditing = true }
                    .disabled(event == nil)
            }
        }
        .sheet(isPresented: $isEditing) {
            if let event {
                EventEditorView(event: event) { self.event = $0 }
            }
        }
        .alert("Slides", isPresented: Binding(get: { slidesResult != nil }, set: { if !$0 { slidesResult = nil } })) {
        } message: {
            Text(slidesResult ?? "")
        }
        .confirmationDialog("Bible slides folder", isPresented: $isAskingForSlideFolder, titleVisibility: .visible) {
            // A folder picked here is a path on this device, so it only fits when this Mac is the server.
            if model.mode == .server {
                Button("Choose Folder…") { isChoosingSlideFolder = true }
            }
            Button("Open Slide Settings") { openSlideSettings() }
        } message: {
            Text("Decks are written to a folder on the server's Mac, and none is set yet.")
        }
        .fileImporter(isPresented: $isChoosingSlideFolder, allowedContentTypes: [.folder]) { result in
            if case .success(let folder) = result { Task { await useSlideFolder(folder) } }
        }
        .task(id: summary.id) {
            do {
                event = try await model.session?.event(id: summary.id)
                loadFailed = event == nil
            } catch {
                loadFailed = true
            }
        }
    }

    /// Writes this event's Bible decks, and says where they went or why it couldn't.
    private func writeSlides() async {
        isWritingSlides = true
        defer { isWritingSlides = false }
        do {
            // Asked first, so a missing folder gets a way to fix it instead of a dead-end error.
            if try await model.session?.serverSettings().slideFolder.trimmingCharacters(in: .whitespaces).isEmpty == true {
                isAskingForSlideFolder = true
                return
            }
            guard let files = try await model.session?.generateSlides(eventId: summary.id) else { return }
            let names = files.map { URL(filePath: $0).lastPathComponent }.joined(separator: ", ")
            slidesResult = files.isEmpty ? String(localized: "Nothing to write.") : String(localized: "Wrote \(names).")
        } catch {
            slidesResult = error.metocastMessage
        }
    }
}

/// Saves the folder picked in the dialog as the Bible slides folder, then writes the decks into it.
extension EventDetailView {
    fileprivate func useSlideFolder(_ folder: URL) async {
        do {
            guard let session = model.session else { return }
            var settings = try await session.serverSettings()
            settings.slideFolder = folder.path
            try await session.saveServerSettings(settings)
            await writeSlides()
        } catch {
            slidesResult = error.metocastMessage
        }
    }

    /// The Mac shows its settings in the inspector; the iPhone has a Settings tab.
    fileprivate func openSlideSettings() {
        #if os(macOS)
        model.isShowingSettings = true
        #else
        model.pane = .settings
        #endif
    }
}

/// What was recorded for this event, and what has been uploaded.
private struct RecordingsSection: View {
    @Environment(AppModel.self) private var model
    let eventId: String
    @State private var recordings: [RecordingRecord] = []
    @State private var failure: String?

    var body: some View {
        Group {
            if !recordings.isEmpty {
                Section("Recordings") {
                    ForEach(recordings, id: \.id) { recording in
                        VStack(alignment: .leading, spacing: 4) {
                            Text(recording.fileName)
                            Text(detail(recording))
                                .font(.subheadline)
                                .foregroundStyle(.secondary)
                            ForEach(recording.uploadStates, id: \.self) { state in
                                Text(state).font(.subheadline).foregroundStyle(.secondary)
                            }
                            if let link = recording.videoUrl.flatMap(URL.init(string:)) {
                                Link("Watch", destination: link).font(.subheadline)
                            } else if !recording.uploadable {
                                Button("Upload to YouTube") {
                                    Task { await flag(recording) }
                                }
                                .font(.subheadline)
                            }
                        }
                        // Names the recording that a "Watch" or "Upload" among several belongs to.
                        .accessibilityElement(children: .contain)
                        .accessibilityLabel(recording.fileName)
                    }
                    if let failure {
                        Label(failure, systemImage: "exclamationmark.triangle").foregroundStyle(.red)
                    }
                }
            }
        }
        .task(id: eventId) { await load() }
    }

    /// "1.2 GB · 1h 0m · obs" — what the operator checks before uploading.
    private func detail(_ recording: RecordingRecord) -> String {
        var parts: [String] = []
        if recording.fileSize > 0 {
            parts.append(ByteCountFormatter.string(fromByteCount: recording.fileSize, countStyle: .file))
        }
        if recording.durationSeconds > 0 {
            let duration = Duration.seconds(recording.durationSeconds)
            parts.append(duration.formatted(.units(allowed: [.hours, .minutes], width: .narrow)))
        }
        if !recording.source.isEmpty { parts.append(recording.source) }
        if recording.uploadable && !recording.uploaded { parts.append(String(localized: "queued for upload")) }
        return parts.joined(separator: " · ")
    }

    private func load() async {
        do {
            recordings = try await model.session?.recordings(eventId: eventId) ?? []
            failure = nil
        } catch {
            failure = error.metocastMessage
        }
    }

    private func flag(_ recording: RecordingRecord) async {
        do {
            try await model.session?.flagUpload(eventId: eventId, recordingId: recording.id, platforms: ["youtube"])
            await load()
        } catch {
            failure = error.metocastMessage
        }
    }
}
