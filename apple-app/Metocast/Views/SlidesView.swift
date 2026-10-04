import SwiftUI
import Metocast

extension PptFile: @retroactive Identifiable {}

/// The PowerPoint files in the server's watched folders. Picking one opens it on the server,
/// in the web presenter or Keynote, whichever it is set to use.
struct SlideBrowserView: View {
    let session: MetocastSession
    @Environment(\.dismiss) private var dismiss
    @State private var query = ""
    @State private var files: [PptFile] = []
    @State private var folders: [String: String] = [:]
    @State private var failure: String?
    @State private var isLoading = true
    @State private var isWritingSong = false

    var body: some View {
        NavigationStack {
            List(files) { file in
                Button {
                    session.show(.open(filePath: file.path))
                    dismiss()
                } label: {
                    VStack(alignment: .leading, spacing: 2) {
                        Text(file.name)
                        Text(folders[file.folderId] ?? file.path)
                            .font(.subheadline)
                            .foregroundStyle(.secondary)
                            .lineLimit(1)
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .contentShape(.rect)
                }
                .buttonStyle(.plain)
            }
            .overlay {
                if isLoading && files.isEmpty {
                    ProgressView()
                } else if let failure {
                    ContentUnavailableView("Couldn't Load Slides", systemImage: "exclamationmark.triangle", description: Text(failure))
                } else if files.isEmpty {
                    if query.isEmpty {
                        ContentUnavailableView(
                            "No Slides",
                            systemImage: "rectangle.on.rectangle",
                            description: Text("Add a slide folder on the server, then its decks show up here.")
                        )
                    } else {
                        ContentUnavailableView.search(text: query)
                    }
                }
            }
            .searchable(text: $query, prompt: "Search Slides")
            .navigationTitle("Open Slides")
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button("Done") { dismiss() }
                }
                ToolbarItem(placement: .primaryAction) {
                    Button("New Song", systemImage: "music.note.list") { isWritingSong = true }
                }
            }
            .sheet(isPresented: $isWritingSong) {
                SongSlidesView(session: session) { dismiss() }
            }
        }
        .task(id: query) {
            // Typing shouldn't ask the server on every keystroke.
            if !query.isEmpty {
                try? await Task.sleep(for: .milliseconds(250))
                guard !Task.isCancelled else { return }
            }
            await load()
        }
    }

    private func load() async {
        isLoading = true
        defer { isLoading = false }
        do {
            async let matches = session.slideFiles(matching: query)
            if folders.isEmpty {
                folders = Dictionary(uniqueKeysWithValues: (try? await session.folderNames()) ?? [])
            }
            files = try await matches
            failure = nil
        } catch {
            failure = error.metocastMessage
        }
    }
}

/// Pasted lyrics become a deck on the server: a title slide, one slide per blank-line-separated
/// block, and a blank slide at the end.
struct SongSlidesView: View {
    let session: MetocastSession
    var onCreated: () -> Void = {}
    @Environment(\.dismiss) private var dismiss
    @State private var title = ""
    @State private var lyrics = ""
    @State private var isWriting = false
    @State private var failure: String?

    var body: some View {
        NavigationStack {
            Form {
                Section("Title") {
                    TextField("Song title", text: $title)
                }
                Section {
                    TextEditor(text: $lyrics)
                        .frame(minHeight: 220)
                        .font(.body)
                } header: {
                    Text("Lyrics")
                } footer: {
                    Text("Leave a blank line between slides.")
                }
                if let failure {
                    Section {
                        Label(failure, systemImage: "exclamationmark.triangle")
                            .foregroundStyle(.red)
                    }
                }
            }
            .formStyle(.grouped)
            .navigationTitle("New Song")
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { dismiss() }
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Create") { Task { await create() } }
                        .disabled(isWriting || title.trimmed.isEmpty || lyrics.trimmed.isEmpty)
                }
            }
            .overlay {
                if isWriting {
                    ProgressView().controlSize(.large)
                }
            }
        }
    }

    /// Writes the deck, then opens it: making a song is always followed by showing it.
    private func create() async {
        isWriting = true
        defer { isWriting = false }
        do {
            let slides = try await session.createSongSlides(title: title.trimmed, lyrics: lyrics)
            session.show(.open(filePath: slides.filePath))
            dismiss()
            onCreated()
        } catch {
            failure = error.metocastMessage
        }
    }
}

extension String {
    var trimmed: String { trimmingCharacters(in: .whitespacesAndNewlines) }
}

extension Error {
    /// What went wrong, in the server's own words when it sent any.
    var metocastMessage: String {
        guard let error = self as? AppleError else { return localizedDescription }
        return switch error {
        case .Server(let message): message
        case .Authentication: "The server rejected this device's token."
        case .Transport: "The server couldn't be reached."
        case .NotConnected: "Not connected to the server."
        case .InvalidInput: "Check what you entered and try again."
        default: "Something went wrong."
        }
    }
}
