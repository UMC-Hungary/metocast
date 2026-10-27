import SwiftUI
import Metocast

/// Creates or edits an event, with the fields Sanctum's editor has: Bible references are
/// looked up as they're typed, and the published title is rendered from the server's template.
struct EventEditorView: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss

    /// "YouTube" or "YouTube and Facebook" while they're set up but not signed in.
    private var signInNeeded: String? {
        let names = (model.session?.accountsNeedingSignIn ?? []).map { ConnectorInfo(id: $0.connector).name }
        return names.isEmpty ? nil : names.formatted(.list(type: .and))
    }
    private let event: EventRecord?
    private let onSaved: (EventRecord) -> Void

    @State private var title: String
    @State private var date: Date
    @State private var speaker: String
    @State private var textus: String
    @State private var leckio: String
    @State private var textusTranslation: String
    @State private var leckioTranslation: String
    /// Book hints per field, keyed by its label.
    @State private var suggestions: [String: [String]] = [:]
    /// Verses by the reference and translation they were looked up for; empty when the core found none.
    /// Keyed like that so a slow lookup for a half-typed reference can never be saved with the finished one.
    @State private var lookups: [Lookup: [BibleVerseRecord]] = [:]
    /// Why a lookup failed, such as a missing szentiras.eu key, so it isn't mistaken for "not found".
    @State private var lookupFailures: [Lookup: String] = [:]
    @State private var summary: String
    @State private var privacy: String
    @State private var autoUpload: Bool
    @State private var template = MetocastSession.defaultTitleTemplate
    @State private var isSaving = false
    @State private var error: String?
    /// The server's scheduled jobs; nil until loaded, and left nil when they can't be, so no hint shows.
    @State private var jobs: [CronJobRecord]?

    /// A new event when `event` is nil.
    init(event: EventRecord? = nil, onSaved: @escaping (EventRecord) -> Void = { _ in }) {
        self.event = event
        self.onSaved = onSaved
        let reference = { (type: String) in event?.bibleReferences.first { $0.referenceType == type }?.reference ?? "" }
        _title = State(initialValue: event?.title ?? "")
        // A new event starts at the next Sunday service, 10:00, like Sanctum and Classic (weekday 1 is Sunday).
        _date = State(initialValue: event?.date
            ?? Calendar.current.nextDate(
                after: .now, matching: DateComponents(hour: 10, minute: 0, weekday: 1), matchingPolicy: .nextTime
            )
            ?? .now)
        _speaker = State(initialValue: event?.speaker ?? "")
        _textus = State(initialValue: reference("textus"))
        _leckio = State(initialValue: reference("leckio"))
        let translation = { (type: String) in
            event?.bibleReferences.first { $0.referenceType == type }?.translation ?? MetocastSession.defaultBibleTranslation
        }
        _textusTranslation = State(initialValue: translation("textus"))
        _leckioTranslation = State(initialValue: translation("leckio"))
        _summary = State(initialValue: event?.description ?? "")
        _privacy = State(initialValue: event?.connections.first { $0.platform == "youtube" }?.privacyStatus ?? "public")
        _autoUpload = State(initialValue: event?.autoUploadEnabled ?? true)
    }

    private var textusLookup: Lookup { Lookup(translation: textusTranslation, reference: textus.trimmingCharacters(in: .whitespaces)) }
    private var leckioLookup: Lookup { Lookup(translation: leckioTranslation, reference: leckio.trimmingCharacters(in: .whitespaces)) }

    private var publishedTitle: String {
        renderTitle(template, TitleValues(date: date, title: title, textus: textus, leckio: leckio, speaker: speaker))
    }

    /// What no enabled scheduled job does yet, of what this event relies on.
    private var jobHint: String? {
        guard let enabled = jobs?.filter(\.enabled) else { return nil }
        let pulls = enabled.contains(where: \.pullYoutube)
        let uploads = enabled.contains(where: \.autoUpload)
        return switch (pulls, uploads || !autoUpload) {
        case (true, true): nil
        case (false, true): String(localized: "No scheduled job pulls YouTube yet.")
        case (true, false): String(localized: "No scheduled job uploads recordings yet.")
        case (false, false): String(localized: "No scheduled job pulls YouTube or uploads recordings yet.")
        }
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
                        .pointingHand()
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
                                .pointingHand()
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
                        .accessibilityLabel("Published title: \(publishedTitle)")
                    if publishedTitle.count > 100 {
                        Text("YouTube titles can be at most 100 characters; this one has \(publishedTitle.count).")
                            .foregroundStyle(.red)
                    }
                }
            }
            Section {
                referenceField("Textus", text: $textus, translation: $textusTranslation)
                referenceField("Lekció", text: $leckio, translation: $leckioTranslation)
            } header: {
                Text("Bible")
            } footer: {
                Text("UF and RUF (v2) come from our own Bible API. The other translations come from szentiras.eu and need its API key under Connectors.")
            }
            Section {
                TextField("Description", text: $summary, axis: .vertical)
                    .lineLimit(3...8)
            }
            Section {
                if let youtube = model.session?.connectors["youtube"] {
                    HStack {
                        Text("Account")
                        Spacer()
                        Label(youtube.title, systemImage: youtube.symbol)
                            .foregroundStyle(youtube.color)
                    }
                }
                HStack {
                    Text("Privacy")
                    Spacer()
                    Menu {
                        Button {
                            privacy = "public"
                        } label: {
                            if privacy == "public" {
                                Label("Public", systemImage: "checkmark")
                            } else {
                                Text("Public")
                            }
                        }
                        Button {
                            privacy = "unlisted"
                        } label: {
                            if privacy == "unlisted" {
                                Label("Unlisted", systemImage: "checkmark")
                            } else {
                                Text("Unlisted")
                            }
                        }
                        Button {
                            privacy = "private"
                        } label: {
                            if privacy == "private" {
                                Label("Private", systemImage: "checkmark")
                            } else {
                                Text("Private")
                            }
                        }
                    } label: {
                        HStack(spacing: 4) {
                            Text(privacy.capitalized)
                            Image(systemName: "chevron.up.chevron.down")
                                .font(.footnote)
                        }
                    }
                }
                Toggle("Auto-upload after event", isOn: $autoUpload)
                if let hint = jobHint, let session = model.session {
                    ScheduledJobHint(text: hint, session: session)
                }
            } header: {
                Text("YouTube")
            } footer: {
                if let error {
                    Text(error).foregroundStyle(.red)
                } else if let names = signInNeeded {
                    Text("Not scheduled on \(names) until you sign in under Connectors.").foregroundStyle(.orange)
                } else {
                    Text("Scheduled on YouTube and Facebook automatically when those accounts are connected.")
                }
            }
        }
        .formStyle(.grouped)
        // Switches on both platforms; the Mac would otherwise draw checkboxes.
        .toggleStyle(.switch)
        .task { template = await model.session?.titleTemplate() ?? template }
        .task { jobs = try? await model.session?.cronJobs() }
        .task(id: textusLookup) { await lookUp(textusLookup) }
        .task(id: leckioLookup) { await lookUp(leckioLookup) }
        .task(id: textus) { await suggest("Textus", text: textus) }
        .task(id: leckio) { await suggest("Lekció", text: leckio) }
    }

    private func referenceField(_ label: String, text: Binding<String>, translation: Binding<String>) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            BibleTranslationPicker(label: label, selection: translation)
            referenceRow(label, text: text, translation: translation.wrappedValue)
            if let message = lookupFailures[Lookup(translation: translation.wrappedValue, reference: text.wrappedValue.trimmingCharacters(in: .whitespaces))] {
                Text(message)
                    .font(.footnote)
                    .foregroundStyle(.red)
            }
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

    private func referenceRow(_ label: String, text: Binding<String>, translation: String) -> some View {
        LabeledContent {
            HStack {
                TextField(label, text: text, prompt: Text("Jn 3,16-21"))
                    .labelsHidden()
                    .autocorrectionDisabled()
                if let verses = lookups[Lookup(translation: translation, reference: text.wrappedValue.trimmingCharacters(in: .whitespaces))] {
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
        guard term.count >= 2, !lookups.keys.contains(where: { $0.reference == term }) else {
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
    private func lookUp(_ key: Lookup) async {
        guard !key.reference.isEmpty, lookups[key] == nil, let session = model.session else { return }
        do {
            try await Task.sleep(for: .milliseconds(500))
            lookups[key] = try await session.verses(for: key.reference, translation: key.translation)
            lookupFailures[key] = nil
        } catch {
            if !Task.isCancelled { lookupFailures[key] = error.metocastMessage }
        }
    }

    private func save() {
        guard canSave, let session = model.session else { return }
        isSaving = true
        error = nil
        Task {
            var references: [BibleReferenceRecord] = []
            for (type, key) in [("textus", textusLookup), ("leckio", leckioLookup)] {
                if let reference = await reference(type, key, session: session) {
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
                self.error = String(localized: "Couldn't save the event. Check the connection and try again.")
                isSaving = false
            }
        }
    }

    /// An empty reference tells the core to remove it; one that doesn't resolve is left as stored.
    /// Saving before the typing pause is over looks the reference up now.
    private func reference(_ type: String, _ key: Lookup, session: MetocastSession) async -> BibleReferenceRecord? {
        var verses: [BibleVerseRecord] = []
        if !key.reference.isEmpty {
            guard let found = await lookedUp(key, session: session), !found.isEmpty else { return nil }
            verses = found
        }
        return BibleReferenceRecord(
            referenceType: type, reference: key.reference, translation: key.translation, verses: verses
        )
    }

    private func lookedUp(_ key: Lookup, session: MetocastSession) async -> [BibleVerseRecord]? {
        if let verses = lookups[key] { return verses }
        return try? await session.verses(for: key.reference, translation: key.translation)
    }
}

/// A reference as looked up in one translation.
private struct Lookup: Hashable {
    let translation: String
    let reference: String
}

/// A quiet note that a scheduled job is missing, how to add one, and a way there.
struct ScheduledJobHint: View {
    let text: String
    let session: MetocastSession
    @State private var isExplaining = false
    private let explanation = String(localized: "A scheduled job imports broadcasts made in YouTube Studio and uploads flagged recordings. Add one with a schedule like */5 * * * * and turn on what it should do.")

    var body: some View {
        LabeledContent {
            NavigationLink("Set Up…") { ScheduledJobsView(session: session) }
        } label: {
            HStack(spacing: 6) {
                Text(text)
                Button("About Scheduled Jobs", systemImage: "info.circle") { isExplaining = true }
                    .labelStyle(.iconOnly)
                    .buttonStyle(.borderless)
                    .help(explanation)
                    .popover(isPresented: $isExplaining) {
                        Text(explanation).padding().frame(idealWidth: 280).presentationCompactAdaptation(.popover)
                    }
            }
        }
        .font(.footnote)
        .foregroundStyle(.secondary)
    }
}

/// Picks a reading's translation: ours first, then szentiras.eu's.
struct BibleTranslationPicker: View {
    let label: String
    @Binding var selection: String

    var body: some View {
        // A translation stored by another app, such as plain "UF", stays selectable.
        var options = MetocastSession.bibleTranslations
        if !options.contains(where: { $0.code == selection }) {
            options.append(.init(code: selection, name: selection))
        }
        let currentName = options.first(where: { $0.code == selection })?.name ?? selection
        return LabeledContent("\(label) Translation") {
            Menu {
                Picker("\(label) Translation", selection: $selection) {
                    Section("Our API") {
                        ForEach(options.filter(\.isOurs), id: \.code) { Text($0.name).tag($0.code) }
                    }
                    Section("Szentírás.eu") {
                        ForEach(options.filter { !$0.isOurs }, id: \.code) { Text($0.name).tag($0.code) }
                    }
                }
            } label: {
                HStack(spacing: 4) {
                    Text(currentName)
                    Image(systemName: "chevron.up.chevron.down")
                        .font(.footnote)
                }
            }
        }
        .pointingHand()
    }
}

/// One of the event's readings on its page, edited in place: once typing pauses, a changed
/// reference or translation looks the verses up and saves the event, and its Order of Service
/// step follows.
struct EventReadingSection: View {
    let session: MetocastSession
    let event: EventRecord
    /// `leckio` or `textus`.
    let type: String
    let onSaved: (EventRecord) -> Void
    @State private var reference = ""
    @State private var translation = MetocastSession.defaultBibleTranslation
    @State private var isSaving = false
    @State private var failure: String?
    @State private var isExpanded = false

    private var stored: BibleReferenceRecord? { event.bibleReferences.first { $0.referenceType == type } }
    private var label: String { type == "textus" ? "Textus" : "Lekció" }

    var body: some View {
        Section {
            DisclosureGroup(isExpanded: $isExpanded) {
                LabeledContent(String(localized: "Reference")) {
                    HStack {
                        TextField(label, text: $reference, prompt: Text("Jn 3,16-21"))
                            .labelsHidden()
                            .autocorrectionDisabled()
                        if isSaving { ProgressView().controlSize(.small) }
                    }
                }
                BibleTranslationPicker(label: label, selection: $translation)
                if let failure {
                    Text(failure).font(.footnote).foregroundStyle(.red)
                }
                ForEach(stored?.verses ?? [], id: \.self) { verse in
                    Text("\(Text("\(verse.chapter):\(verse.verse)").foregroundStyle(.secondary)) \(verse.text)")
                }
            } label: {
                HStack {
                    Label(label, systemImage: "book")
                    Spacer()
                    if let ref = stored?.reference, !ref.isEmpty {
                        Text(ref)
                            .foregroundStyle(.secondary)
                    } else {
                        Text("Not set")
                            .foregroundStyle(.tertiary)
                    }
                }
            }
        }
        .task(id: event.updatedAt) {
            reference = stored?.reference ?? ""
            translation = stored?.translation ?? MetocastSession.defaultBibleTranslation
        }
        // Like the editor: saved once typing pauses; a newer edit cancels the wait.
        .task(id: "\(reference)|\(translation)") {
            do {
                try await Task.sleep(for: .milliseconds(800))
            } catch {
                return
            }
            await save()
        }
    }

    /// Saves only a real change, so loading the stored values saves nothing. An empty reference
    /// removes the reading; one that finds no verses is not saved.
    private func save() async {
        let key = reference.trimmingCharacters(in: .whitespaces)
        let changed = key != (stored?.reference ?? "") || (!key.isEmpty && translation != stored?.translation)
        guard changed, !isSaving else { return }
        isSaving = true
        defer { isSaving = false }
        failure = nil
        do {
            let verses = key.isEmpty ? [] : try await session.verses(for: key, translation: translation)
            if !key.isEmpty, verses.isEmpty {
                failure = String(localized: "No verses found for \(key).")
                return
            }
            let other = { (type: String) in event.bibleReferences.first { $0.referenceType == type }?.reference ?? "" }
            let values = TitleValues(
                date: event.date, title: event.title,
                textus: type == "textus" ? key : other("textus"),
                leckio: type == "leckio" ? key : other("leckio"),
                speaker: event.speaker
            )
            let draft = EventDraftRecord(
                title: event.title,
                computedTitle: renderTitle(await session.titleTemplate(), values),
                dateTime: event.dateTime,
                speaker: event.speaker,
                description: event.description,
                autoUploadEnabled: event.autoUploadEnabled,
                youtubePrivacy: event.connections.first { $0.platform == "youtube" }?.privacyStatus ?? "public",
                bibleReferences: [BibleReferenceRecord(referenceType: type, reference: key, translation: translation, verses: verses)]
            )
            onSaved(try await session.save(draft, id: event.id))
        } catch {
            failure = error.metocastMessage
        }
    }
}
