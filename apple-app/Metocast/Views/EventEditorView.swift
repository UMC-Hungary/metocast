import SwiftUI
import Metocast

/// Creates or edits an event, with the fields Sanctum's editor has: Bible references are
/// looked up as they're typed, and the published title is rendered from the server's template.
struct EventEditorView: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    private let event: EventRecord?
    private let onSaved: (EventRecord) -> Void

    @State private var title: String
    @State private var date: Date
    @State private var speaker: String
    @State private var textus: String
    @State private var leckio: String
    /// Book hints per field, keyed by its label.
    @State private var suggestions: [String: [String]] = [:]
    /// Verses by the reference they were looked up for; empty when the core found none. Keyed by
    /// reference so a slow lookup for a half-typed one can never be saved with the finished one.
    @State private var lookups: [String: [BibleVerseRecord]] = [:]
    @State private var summary: String
    @State private var privacy: String
    @State private var autoUpload: Bool
    @State private var template = MetocastSession.defaultTitleTemplate
    @State private var isSaving = false
    @State private var error: String?

    /// A new event when `event` is nil.
    init(event: EventRecord? = nil, onSaved: @escaping (EventRecord) -> Void = { _ in }) {
        self.event = event
        self.onSaved = onSaved
        let reference = { (type: String) in event?.bibleReferences.first { $0.referenceType == type }?.reference ?? "" }
        _title = State(initialValue: event?.title ?? "")
        _date = State(initialValue: event?.date
            ?? Calendar.current.nextDate(after: .now, matching: DateComponents(minute: 0), matchingPolicy: .nextTime)
            ?? .now)
        _speaker = State(initialValue: event?.speaker ?? "")
        _textus = State(initialValue: reference("textus"))
        _leckio = State(initialValue: reference("leckio"))
        _summary = State(initialValue: event?.description ?? "")
        _privacy = State(initialValue: event?.connections.first { $0.platform == "youtube" }?.privacyStatus ?? "public")
        _autoUpload = State(initialValue: event?.autoUploadEnabled ?? true)
    }

    private var publishedTitle: String {
        renderTitle(template, TitleValues(date: date, title: title, textus: textus, leckio: leckio, speaker: speaker))
    }

    private var canSave: Bool {
        !title.trimmingCharacters(in: .whitespaces).isEmpty && !isSaving
    }

    private var hasChanges: Bool {
        event == nil ? !(title.isEmpty && speaker.isEmpty && textus.isEmpty && leckio.isEmpty && summary.isEmpty) : true
    }

    var body: some View {
        #if os(macOS)
        // The form is the whole sheet and the actions are standard sheet items, which macOS draws as the
        // bottom-trailing Cancel/Save buttons. A hand-built VStack of heading, form and button row let the
        // form's scroll view spread past its frame and swallow clicks on the switches and buttons.
        form
            .navigationTitle(event == nil ? "New Event" : "Edit Event")
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel", role: .cancel) { dismiss() }
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button(event == nil ? "Add" : "Save", role: .confirm, action: save)
                        .disabled(!canSave)
                }
            }
            .presentationSizing(.form)
        #else
        NavigationStack {
            form
                .navigationTitle(event == nil ? "New Event" : "Edit Event")
                .navigationBarTitleDisplayMode(.inline)
                .toolbar {
                    ToolbarItem(placement: .cancellationAction) {
                        Button(role: .close) { dismiss() }
                    }
                    ToolbarItem(placement: .confirmationAction) {
                        if isSaving {
                            ProgressView()
                        } else {
                            Button(event == nil ? "Add" : "Save", systemImage: "checkmark", role: .confirm, action: save)
                                .disabled(!canSave)
                        }
                    }
                }
        }
        .interactiveDismissDisabled(hasChanges)
        #endif
    }

    private var form: some View {
        Form {
            Section {
                TextField("Title", text: $title)
                TextField("Speaker", text: $speaker)
                    #if os(iOS)
                    .textInputAutocapitalization(.words)
                    #endif
                DatePicker("Date & Time", selection: $date, displayedComponents: [.date, .hourAndMinute])
            } footer: {
                VStack(alignment: .leading, spacing: 4) {
                    Text(publishedTitle)
                    if publishedTitle.count > 100 {
                        Text("YouTube titles can be at most 100 characters; this one has \(publishedTitle.count).")
                            .foregroundStyle(.red)
                    }
                }
            }
            Section {
                referenceField("Textus", text: $textus)
                referenceField("Lekció", text: $leckio)
            } header: {
                Text("Bible")
            }
            Section {
                TextField("Description", text: $summary, axis: .vertical)
                    .lineLimit(3...8)
            }
            Section {
                Picker("Privacy", selection: $privacy) {
                    Text("Public").tag("public")
                    Text("Unlisted").tag("unlisted")
                    Text("Private").tag("private")
                }
                Toggle("Auto-upload after event", isOn: $autoUpload)
            } header: {
                Text("YouTube")
            } footer: {
                if let error {
                    Text(error).foregroundStyle(.red)
                } else {
                    Text("Scheduled on YouTube and Facebook automatically when those accounts are connected.")
                }
            }
        }
        .formStyle(.grouped)
        // Switches on both platforms; the Mac would otherwise draw checkboxes.
        .toggleStyle(.switch)
        .task { template = await model.session?.titleTemplate() ?? template }
        .task(id: textus) { await lookUp(textus) }
        .task(id: leckio) { await lookUp(leckio) }
        .task(id: textus) { await suggest("Textus", text: textus) }
        .task(id: leckio) { await suggest("Lekció", text: leckio) }
    }

    private func referenceField(_ label: String, text: Binding<String>) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            referenceRow(label, text: text)
            // The book list comes from the core; a tap fills the field in, chapter and all.
            if let hits = suggestions[label], !hits.isEmpty {
                ScrollView(.horizontal) {
                    HStack(spacing: 8) {
                        ForEach(hits, id: \.self) { hit in
                            Button(hit) { text.wrappedValue = hit }
                                .buttonStyle(.bordered)
                                .font(.subheadline)
                        }
                    }
                }
                .scrollIndicators(.hidden)
            }
        }
    }

    private func referenceRow(_ label: String, text: Binding<String>) -> some View {
        LabeledContent {
            HStack {
                TextField(label, text: text, prompt: Text("Jn 3,16-21"))
                    .labelsHidden()
                    .autocorrectionDisabled()
                if let verses = lookups[text.wrappedValue.trimmingCharacters(in: .whitespaces)] {
                    if verses.isEmpty {
                        Image(systemName: "exclamationmark.triangle")
                            .foregroundStyle(.orange)
                            .accessibilityLabel("Reference not found")
                    } else {
                        Text("^[\(verses.count) verse](inflect: true)")
                            .font(.footnote)
                            .foregroundStyle(.secondary)
                            .fixedSize()
                    }
                }
            }
        } label: {
            Text(label)
        }
    }

    /// Book and chapter hints while the reference is still being typed. A newer edit cancels
    /// this task, so the core is asked once the typing pauses.
    private func suggest(_ label: String, text: String) async {
        let term = text.trimmingCharacters(in: .whitespaces)
        guard term.count >= 2, !lookups.keys.contains(term) else {
            suggestions[label] = []
            return
        }
        do {
            try await Task.sleep(for: .milliseconds(300))
        } catch {
            return
        }
        let hits = (try? await model.session?.bibleSuggestions(term)) ?? []
        suggestions[label] = hits.prefix(6).map(\.label)
    }

    /// Waits for typing to pause (a newer edit cancels this task), then asks the core.
    private func lookUp(_ text: String) async {
        let reference = text.trimmingCharacters(in: .whitespaces)
        guard !reference.isEmpty, lookups[reference] == nil else { return }
        do {
            try await Task.sleep(for: .milliseconds(500))
        } catch {
            return
        }
        if let verses = try? await model.session?.verses(for: reference) {
            lookups[reference] = verses
        }
    }

    private func save() {
        guard canSave, let session = model.session else { return }
        isSaving = true
        error = nil
        Task {
            var references: [BibleReferenceRecord] = []
            for (type, text) in [("textus", textus), ("leckio", leckio)] {
                if let reference = await reference(type, text: text, session: session) {
                    references.append(reference)
                }
            }
            let draft = EventDraftRecord(
                title: title.trimmingCharacters(in: .whitespaces),
                computedTitle: publishedTitle,
                dateTime: date.ISO8601Format(),
                speaker: speaker.trimmingCharacters(in: .whitespaces),
                description: summary.trimmingCharacters(in: .whitespacesAndNewlines),
                autoUploadEnabled: autoUpload,
                youtubePrivacy: privacy,
                bibleReferences: references
            )
            do {
                let saved = try await session.save(draft, id: event?.id)
                onSaved(saved)
                dismiss()
            } catch {
                self.error = "Couldn't save the event. Check the connection and try again."
                isSaving = false
            }
        }
    }

    /// An empty reference tells the core to remove it; one that doesn't resolve is left as stored.
    /// Saving before the typing pause is over looks the reference up now.
    private func reference(_ type: String, text: String, session: MetocastSession) async -> BibleReferenceRecord? {
        let reference = text.trimmingCharacters(in: .whitespaces)
        var verses: [BibleVerseRecord] = []
        if !reference.isEmpty {
            guard let found = await lookedUp(reference, session: session), !found.isEmpty else { return nil }
            verses = found
        }
        return BibleReferenceRecord(
            referenceType: type, reference: reference, translation: MetocastSession.bibleTranslation, verses: verses
        )
    }

    private func lookedUp(_ reference: String, session: MetocastSession) async -> [BibleVerseRecord]? {
        if let verses = lookups[reference] { return verses }
        return try? await session.verses(for: reference)
    }
}
