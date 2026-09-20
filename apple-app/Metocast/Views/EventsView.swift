import SwiftUI
import Metocast

struct EventsView: View {
    @Environment(AppModel.self) private var model
    @State private var query = ""

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
        .navigationDestination(for: EventSummaryRecord.self) { EventDetailView(summary: $0) }
        .refreshable { await model.session?.refresh() }
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
                if !event.connections.isEmpty {
                    Section("Streaming") {
                        ForEach(event.connections, id: \.platform) { connection in
                            let link = (connection.eventUrl ?? connection.streamUrl).flatMap(URL.init(string:))
                            // "not_scheduled" → "Not Scheduled"
                            let status = connection.scheduleStatus.replacing("_", with: " ").localizedCapitalized
                            LabeledContent(connection.platformName) {
                                if let link {
                                    Link(status, destination: link)
                                } else {
                                    Text(status)
                                }
                            }
                        }
                    }
                }
                ForEach(event.bibleReferences, id: \.referenceType) { reference in
                    Section("\(reference.referenceType.capitalized) · \(reference.reference) (\(reference.translation))") {
                        ForEach(reference.verses, id: \.self) { verse in
                            Text("\(Text("\(verse.chapter):\(verse.verse)").foregroundStyle(.secondary)) \(verse.text)")
                        }
                    }
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
            guard let files = try await model.session?.generateSlides(eventId: summary.id) else { return }
            let names = files.map { URL(filePath: $0).lastPathComponent }.joined(separator: ", ")
            slidesResult = files.isEmpty ? "Nothing to write." : "Wrote \(names)."
        } catch {
            slidesResult = error.metocastMessage
        }
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
        if recording.uploadable && !recording.uploaded { parts.append("queued for upload") }
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
