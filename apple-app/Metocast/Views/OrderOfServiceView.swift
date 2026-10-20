import SwiftUI
import Metocast

/// A step being edited. New steps have no server id yet, so the list keys them by a local one.
private struct EditableStep: Identifiable {
    let id = UUID()
    var item: OrderItemRecord
}

extension OrderItemRecord {
    var icon: String {
        switch kind {
        case .song: "music.note"
        case .bible: "book"
        case .block: "text.alignleft"
        }
    }

    /// What the step shows, under its title: the deck, or which reading.
    var detail: String? {
        switch kind {
        case .song: deckPath.map { URL(filePath: $0).deletingPathExtension().lastPathComponent }
        case .bible: bibleType == "textus" ? "Textus" : "Lekció"
        case .block: nil
        }
    }
}

/// One step as every list shows it. The step on screen is marked, so the person running the
/// service always sees where they are.
struct OrderStepRow: View {
    let item: OrderItemRecord
    let number: Int
    var isCurrent = false

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: 12) {
            Text("\(number)")
                .monospacedDigit()
                .foregroundStyle(.secondary)
                .frame(minWidth: 20, alignment: .trailing)
            Image(systemName: item.icon)
                .foregroundStyle(isCurrent ? AnyShapeStyle(.tint) : AnyShapeStyle(.secondary))
                .frame(width: 20)
            VStack(alignment: .leading, spacing: 2) {
                Text(item.title.isEmpty ? (item.detail ?? String(localized: "Untitled")) : item.title)
                    .fontWeight(isCurrent ? .semibold : .regular)
                if let detail = item.detail, !item.title.isEmpty, detail != item.title {
                    Text(detail).font(.subheadline).foregroundStyle(.secondary)
                }
                if !item.notes.isEmpty {
                    Text(item.notes).font(.subheadline).foregroundStyle(.secondary)
                }
            }
            Spacer(minLength: 0)
            if isCurrent {
                Image(systemName: "play.fill")
                    .foregroundStyle(.tint)
                    .accessibilityHidden(true)
            }
        }
        .contentShape(.rect)
        .accessibilityElement(children: .combine)
        .accessibilityValue(isCurrent ? "On screen" : "")
    }
}

/// The Order of Service on an event's page: the steps, and the way into editing them.
struct EventOrderSection: View {
    let session: MetocastSession
    let event: EventRecord
    @State private var items: [OrderItemRecord] = []

    var body: some View {
        Section {
            ForEach(Array(items.enumerated()), id: \.offset) { index, item in
                OrderStepRow(item: item, number: index + 1, isCurrent: isCurrent(item))
            }
            NavigationLink {
                OrderEditorView(session: session, event: event).windowActions()
            } label: {
                Label(items.isEmpty ? "Add Order of Service" : "Edit Order of Service", systemImage: "list.number")
            }
            .accessibilityAddTraits(.isButton)
        } header: {
            Text("Order of Service")
        } footer: {
            Text("The songs, readings and parts of the service, in order. Step through them on the Live page.")
        }
        .task(id: session.orderRevision) {
            items = (try? await session.order(eventId: event.id)) ?? items
        }
    }

    private func isCurrent(_ item: OrderItemRecord) -> Bool {
        session.orderPosition?.eventId == event.id && session.orderPosition?.itemId == item.id
    }
}

/// Builds the Order of Service: add songs from the library, the event's readings and blocks,
/// drag to reorder, and save. Steps keep their ids, so editing during the service is safe.
struct OrderEditorView: View {
    let session: MetocastSession
    let event: EventRecord
    @State private var steps: [EditableStep] = []
    @State private var isLoaded = false
    @State private var isSaving = false
    @State private var isPickingSong = false
    @State private var failure: String?

    var body: some View {
        List {
            ForEach($steps) { $step in
                StepEditor(item: $step.item)
            }
            .onMove { steps.move(fromOffsets: $0, toOffset: $1) }
            .onDelete { steps.remove(atOffsets: $0) }
        }
        .overlay {
            if isLoaded && steps.isEmpty {
                ContentUnavailableView {
                    Label("No Steps Yet", systemImage: "list.number")
                } description: {
                    Text("Add the songs, the readings and the other parts of the service.")
                } actions: {
                    if !event.bibleReferences.isEmpty {
                        Button("Start with the Readings") { addReadings() }
                            .buttonStyle(.glassProminent)
                    }
                }
            }
        }
        .navigationTitle("Order of Service")
        .toolbar {
            ToolbarItem {
                addMenu
            }
            ToolbarItem(placement: .confirmationAction) {
                SaveOrderButton(isDisabled: isSaving || !isLoaded) { await save() }
            }
        }
        .navigationDestination(isPresented: $isPickingSong) {
            SongPickerView(session: session) { file in
                let name = file.name.replacing(/\.pptx?$/, with: "")
                steps.append(EditableStep(item: OrderItemRecord(
                    id: nil, kind: .song, title: name, deckPath: file.path, bibleType: nil, notes: ""
                )))
            }
            .windowActions()
        }
        .alert("Couldn't Save", isPresented: Binding(get: { failure != nil }, set: { if !$0 { failure = nil } })) {
            Button("OK", role: .cancel) {}
        } message: {
            Text(failure ?? "")
        }
        .task {
            guard !isLoaded else { return }
            do {
                steps = try await session.order(eventId: event.id).map { EditableStep(item: $0) }
                isLoaded = true
            } catch {
                failure = error.metocastMessage
            }
        }
    }

    private var addMenu: some View {
        Menu {
            Button("Song…", systemImage: "music.note") { isPickingSong = true }
            // A verse sung without slides, such as the closing verse.
            Button("Song Without Slides", systemImage: "music.note") {
                steps.append(EditableStep(item: OrderItemRecord(
                    id: nil, kind: .song, title: "", deckPath: nil, bibleType: nil, notes: ""
                )))
            }
            ForEach(event.bibleReferences, id: \.referenceType) { reading in
                Button("\(reading.referenceType == "textus" ? "Textus" : "Lekció") · \(reading.reference)", systemImage: "book") {
                    add(reading: reading.referenceType)
                }
            }
            Button("Block", systemImage: "text.alignleft") {
                steps.append(EditableStep(item: OrderItemRecord(
                    id: nil, kind: .block, title: "", deckPath: nil, bibleType: nil, notes: ""
                )))
            }
        } label: {
            Label("Add Step", systemImage: "plus")
        }
        #if os(macOS)
        .labelStyle(.titleAndIcon)
        #endif
    }

    /// The service reads the Lekció before the Textus.
    private func addReadings() {
        for type in ["leckio", "textus"] where event.bibleReferences.contains(where: { $0.referenceType == type }) {
            add(reading: type)
        }
    }

    private func add(reading type: String) {
        steps.append(EditableStep(item: OrderItemRecord(
            id: nil, kind: .bible, title: type == "textus" ? "Textus" : "Lekció",
            deckPath: nil, bibleType: type, notes: ""
        )))
    }

    /// True once the server has the steps, so the Save button can close the page.
    private func save() async -> Bool {
        isSaving = true
        defer { isSaving = false }
        do {
            _ = try await session.saveOrder(eventId: event.id, items: steps.map(\.item))
            return true
        } catch {
            failure = error.metocastMessage
            return false
        }
    }
}

/// Saves, then closes the editor. It reads `\.dismiss` here rather than in the editor: the editor
/// hosts the song picker's `navigationDestination`, and a view that reads `dismiss` and re-declares
/// a destination re-evaluates on every change of it, which re-declares the destination, which
/// changes `dismiss` again, without end (the Mac app froze on Add Step > Song…).
private struct SaveOrderButton: View {
    @Environment(\.dismiss) private var dismiss
    let isDisabled: Bool
    let save: () async -> Bool

    var body: some View {
        Button("Save", systemImage: "checkmark") {
            Task { if await save() { dismiss() } }
        }
        .buttonStyle(.glassProminent)
        .tint(.accentColor)
        .disabled(isDisabled)
        #if os(macOS)
        .labelStyle(.titleAndIcon)
        #endif
    }
}

/// One step in the editor: its title and notes are typed in place.
private struct StepEditor: View {
    @Binding var item: OrderItemRecord

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: 12) {
            Image(systemName: item.icon)
                .foregroundStyle(.secondary)
                .frame(width: 20)
                .accessibilityHidden(true)
            VStack(alignment: .leading, spacing: 4) {
                TextField(item.kind == .bible ? "Title" : "Title, e.g. Prayer or the song's name", text: $item.title)
                    .fontWeight(.medium)
                if let detail = item.detail {
                    Text(detail).font(.subheadline).foregroundStyle(.secondary)
                }
                TextField("Notes for the leaders", text: $item.notes, axis: .vertical)
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
            }
        }
        .padding(.vertical, 2)
    }
}

/// The deck library, as a pushed page that picks one deck for a song step.
struct SongPickerView: View {
    let session: MetocastSession
    let onPick: (PptFile) -> Void
    @Environment(\.dismiss) private var dismiss
    @State private var query = ""

    var body: some View {
        DeckList(session: session, query: query) { file in
            onPick(file)
            dismiss()
        }
        .librarySearch($query)
        .navigationTitle("Choose Song")
    }
}

/// The Order of Service of the event under way, on the Live page: every step, the one on screen
/// marked, and a click on any step shows it.
struct LiveOrderSection: View {
    let session: MetocastSession
    let event: EventSummaryRecord
    @State private var items: [OrderItemRecord] = []

    var body: some View {
        Section {
            if items.isEmpty {
                Text("\(event.title) has no Order of Service yet. Add one on the event's page.")
                    .foregroundStyle(.secondary)
            }
            ForEach(Array(items.enumerated()), id: \.offset) { index, item in
                Button {
                    if let id = item.id { session.show(.orderGo(eventId: event.id, itemId: id)) }
                } label: {
                    OrderStepRow(item: item, number: index + 1, isCurrent: isCurrent(item))
                }
                .buttonStyle(.plain)
                .accessibilityHint("Shows this step on the output screen")
            }
            if !items.isEmpty {
                HStack {
                    Button("Previous Step", systemImage: "chevron.backward") {
                        session.show(.orderPrevious(eventId: event.id))
                    }
                    .disabled(currentIndex.map { $0 == 0 } ?? true)
                    Spacer()
                    Button("Next Step", systemImage: "chevron.forward") {
                        session.show(.orderNext(eventId: event.id))
                    }
                    .buttonStyle(.glassProminent)
                    .tint(.accentColor)
                    .disabled(currentIndex.map { $0 >= items.count - 1 } ?? false)
                }
            }
        } header: {
            Text("Order of Service · \(event.title)")
        }
        .task(id: "\(event.id)-\(session.orderRevision)") {
            items = (try? await session.order(eventId: event.id)) ?? items
        }
    }

    private var currentIndex: Int? {
        guard session.orderPosition?.eventId == event.id else { return nil }
        return items.firstIndex { $0.id == session.orderPosition?.itemId }
    }

    private func isCurrent(_ item: OrderItemRecord) -> Bool {
        session.orderPosition?.eventId == event.id && session.orderPosition?.itemId == item.id
    }
}
