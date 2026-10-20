import SwiftUI
import Metocast

extension PptFile: @retroactive Identifiable {}

/// The decks in the server's watched folders, found by title, hymn number or lyrics and narrowed
/// to a folder; the server ranks them and marks what matched. A click hands the deck to `onPick`:
/// Presentation opens it, the Order of Service adds it as a song.
/// The folder menu also adds and removes watched folders, so no separate screen is needed.
struct DeckList: View {
    let session: MetocastSession
    let query: String
    let onPick: (PptFile) -> Void
    @State private var files: [PptFile] = []
    @State private var folders: [PptFolderRecord] = []
    @State private var folderId: String?
    @State private var failure: String?
    @State private var isLoading = true
    @State private var isAddingFolder = false

    var body: some View {
        List(files) { file in
            Button { onPick(file) } label: { row(file) }
                .buttonStyle(.plain)
        }
        .overlay {
            if isLoading && files.isEmpty {
                ProgressView()
            } else if let failure {
                ContentUnavailableView("Couldn't Load Slides", systemImage: "exclamationmark.triangle", description: Text(failure))
            } else if folders.isEmpty {
                ContentUnavailableView {
                    Label("No Slide Folders", systemImage: "folder.badge.plus")
                } description: {
                    Text("Add a folder of PowerPoint decks on the server's Mac.")
                } actions: {
                    Button("Add Folder…") { isAddingFolder = true }
                        .buttonStyle(.glassProminent)
                }
            } else if files.isEmpty {
                if query.isEmpty {
                    ContentUnavailableView("No Decks", systemImage: "rectangle.on.rectangle", description: Text("This folder has no PowerPoint decks."))
                } else {
                    ContentUnavailableView.search(text: query)
                }
            }
        }
        .toolbar {
            ToolbarItem { folderMenu }
        }
        .serverFolderPrompt(
            "Add Slide Folder",
            message: "Decks in this folder on the server's Mac show up in the list.",
            isPresented: $isAddingFolder
        ) { path in
            try await session.addSlideFolder(path: path)
            await load()
        }
        // A finished index run searches again, so new decks show up.
        .task(id: SearchKey(query: query, folderId: folderId, revision: session.slideIndexRevision)) {
            // Typing shouldn't ask the server on every keystroke.
            if !query.isEmpty {
                try? await Task.sleep(for: .milliseconds(250))
                guard !Task.isCancelled else { return }
            }
            await load()
        }
    }

    private func row(_ file: PptFile) -> some View {
        let isOpen = session.usesWebPresenter
            ? session.presenter?.filePath == file.path
            : session.presentationStatus?.documentName == file.name
        return HStack {
            VStack(alignment: .leading, spacing: 2) {
                Text(file.titleParts.isEmpty ? AttributedString(file.name.replacing(/\.pptx?$/, with: "")) : marked(file.titleParts))
                if !file.snippetParts.isEmpty {
                    Text(marked(file.snippetParts))
                        .font(.subheadline)
                        .foregroundStyle(.secondary)
                        .lineLimit(2)
                }
                Text(folders.first { $0.id == file.folderId }?.name ?? file.path)
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
            Spacer()
            if let number = file.number {
                Text(number, format: .number.grouping(.never))
                    .monospacedDigit()
                    .foregroundStyle(.secondary)
                    .accessibilityLabel("Hymn \(number)")
            }
            if isOpen {
                Image(systemName: "play.fill")
                    .foregroundStyle(.tint)
                    .accessibilityLabel("On screen")
            }
        }
        .contentShape(.rect)
    }

    /// The words the search matched, in bold and the accent color.
    private func marked(_ parts: [HighlightPart]) -> AttributedString {
        parts.reduce(into: AttributedString()) { text, part in
            var run = AttributedString(part.text)
            if part.hit {
                run.inlinePresentationIntent = .stronglyEmphasized
                run.foregroundColor = .accentColor
            }
            text += run
        }
    }

    /// Which folder the list shows, plus adding and removing watched folders.
    private var folderMenu: some View {
        Menu {
            Picker("Show", selection: $folderId) {
                Text("All Folders").tag(String?.none)
                ForEach(folders, id: \.id) { folder in
                    Text(folder.name).tag(Optional(folder.id))
                }
            }
            .pickerStyle(.inline)
            Divider()
            Button("Add Folder…", systemImage: "folder.badge.plus") { isAddingFolder = true }
            if !folders.isEmpty {
                Menu("Stop Watching", systemImage: "folder.badge.minus") {
                    ForEach(folders, id: \.id) { folder in
                        Button(folder.name, role: .destructive) { Task { await remove(folder) } }
                    }
                }
            }
        } label: {
            Label(folders.first { $0.id == folderId }?.name ?? "All Folders", systemImage: "folder")
        }
        #if os(macOS)
        .labelStyle(.titleAndIcon)
        #endif
    }

    private func remove(_ folder: PptFolderRecord) async {
        do {
            try await session.removeSlideFolder(id: folder.id)
            if folderId == folder.id { folderId = nil }
            await load()
        } catch {
            failure = error.metocastMessage
        }
    }

    private func load() async {
        isLoading = true
        defer { isLoading = false }
        do {
            async let matches = session.slideFiles(matching: query, folderId: folderId)
            folders = try await session.slideFolders()
            files = try await matches
            failure = nil
        } catch {
            failure = error.metocastMessage
        }
    }
}

/// What a deck search depends on; a change to any of them searches again.
private struct SearchKey: Equatable {
    let query: String
    let folderId: String?
    let revision: Int
}

/// Pasted lyrics become a deck on the server: a title slide, the lyric slides and a blank
/// slide at the end. Pushed from Presentation once the server has a song folder.
struct SongSlidesView: View {
    let session: MetocastSession
    @Environment(\.dismiss) private var dismiss
    @State private var title = ""
    @State private var lyrics = ""
    @State private var folder = ""
    @State private var isChoosingFolder = false
    @State private var isWriting = false
    @State private var failure: String?

    var body: some View {
        let slides = SongSlide.deck(title: title.trimmed, lyrics: lyrics)
        content(slides)
            .navigationTitle("New Song")
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button("Create", systemImage: "checkmark") { Task { await create() } }
                        .buttonStyle(.glassProminent)
                        .tint(.accentColor)
                        .disabled(isWriting || folder.isEmpty || title.trimmed.isEmpty || lyrics.trimmed.isEmpty)
                        #if os(macOS)
                        .labelStyle(.titleAndIcon)
                        #endif
                }
            }
            .overlay {
                if isWriting {
                    ProgressView().controlSize(.large)
                }
            }
            .songFolderPrompt(isPresented: $isChoosingFolder, session: session) { folder = $0 }
            .alert("Couldn't Create Song", isPresented: Binding(get: { failure != nil }, set: { if !$0 { failure = nil } })) {
                Button("OK", role: .cancel) {}
            } message: {
                Text(failure ?? "")
            }
            .task { folder = (try? await session.serverSettings().songSlideFolder.trimmed) ?? "" }
    }

    @ViewBuilder
    private func content(_ slides: [SongSlide]) -> some View {
        #if os(macOS)
        HStack(spacing: 0) {
            Form { fields }
                .formStyle(.grouped)
            ScrollView {
                SongPreview(slides: slides, theme: session.presenterTheme, fileName: SongSlide.fileStem(title)).padding()
            }
            .frame(width: 340)
        }
        #else
        Form {
            fields
            Section("Preview") {
                SongPreview(slides: slides, theme: session.presenterTheme, fileName: SongSlide.fileStem(title))
            }
        }
        #endif
    }

    @ViewBuilder
    private var fields: some View {
        Section("Title") {
            TextField("Song title", text: $title)
        }
        Section {
            TextEditor(text: $lyrics)
                .frame(minHeight: 260)
                .font(.body)
                .accessibilityLabel("Lyrics")
        } header: {
            Text("Lyrics")
        } footer: {
            Text("Leave two empty lines between slides. One empty line stays on the slide. A title slide and a blank last slide are added.")
        }
        Section {
            LabeledContent("Song Slides Folder") {
                Text(folder.isEmpty ? "Not set" : folder)
                    .lineLimit(1)
                    .truncationMode(.middle)
                    .textSelection(.enabled)
            }
            Button("Change Folder…") { isChoosingFolder = true }
        } footer: {
            Text("A folder on the server's Mac. The deck is saved there.")
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
        } catch {
            failure = error.metocastMessage
        }
    }
}

/// One slide of a song deck, split the way the server's `song_slides` (presenter.rs) splits it.
struct SongSlide: Identifiable {
    let id: Int
    let lines: [String]
    let isTitle: Bool

    // ponytail: mirrors presenter.rs `song_slides`; a server preview endpoint if the two ever drift.
    static func deck(title: String, lyrics: String) -> [SongSlide] {
        var pages: [[String]] = []
        var lines: [String] = []
        var empty = 0
        func finish() {
            while lines.last?.isEmpty == true { lines.removeLast() }
            if lines.contains(where: { !$0.isEmpty }) { pages.append(lines) }
            lines = []
        }
        for line in lyrics.trimmed.components(separatedBy: .newlines).map(\.trimmed) {
            if line.isEmpty {
                empty += 1
                if empty == 1 { lines.append("") } else if empty == 2 { finish() }
            } else {
                empty = 0
                lines.append(line)
            }
        }
        finish()
        let all = [[title]] + pages + [[]]
        return all.enumerated().map { SongSlide(id: $0.offset + 1, lines: $0.element, isTitle: $0.offset == 0) }
    }

    // ponytail: mirrors presenter.rs `song_file_name`, which names the deck the Editorial label shows.
    static func fileStem(_ title: String) -> String {
        let cleaned = title.map { $0.isLetter || $0.isNumber || " -_".contains($0) ? $0 : " " }
        return String(String(cleaned).split(whereSeparator: \.isWhitespace).joined(separator: " ").prefix(80))
    }
}

/// The deck as the output screen will show it, one 16:9 slide under another, in the presenter's design.
private struct SongPreview: View {
    let slides: [SongSlide]
    let theme: PresenterTheme
    let fileName: String

    var body: some View {
        LazyVStack(alignment: .leading, spacing: 12) {
            ForEach(slides) { slide in
                VStack(alignment: .leading, spacing: 4) {
                    Text("Slide \(slide.id)")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                    PresenterTextSlide(
                        // The deck is 960 × 540 pt: 52 pt for the title, 40 pt for the lyrics, nothing on the blank slide.
                        paragraphs: slide.lines.isEmpty ? [] : [
                            PresenterParagraph(lines: slide.lines, align: "center", fontSizePt: slide.isTitle ? 52 : 40),
                        ],
                        index: slide.id,
                        fileName: fileName,
                        design: theme == .editorial ? .editorial : .classic
                    )
                    .clipShape(.rect(cornerRadius: 8))
                }
                .accessibilityElement(children: .ignore)
                .accessibilityLabel("Slide \(slide.id)")
                .accessibilityValue(slide.lines.isEmpty ? "Blank" : slide.lines.joined(separator: "\n"))
            }
        }
    }
}

extension View {
    /// Asks for a folder on the server's Mac, then hands its path to `save`.
    func serverFolderPrompt(
        _ title: String,
        message: String,
        isPresented: Binding<Bool>,
        save: @escaping (String) async throws -> Void
    ) -> some View {
        modifier(ServerFolderPrompt(title: title, message: message, isPresented: isPresented, save: save))
    }

    /// Asks for the server's song slides folder and saves it.
    func songFolderPrompt(isPresented: Binding<Bool>, session: MetocastSession, onSaved: @escaping (String) -> Void) -> some View {
        serverFolderPrompt(
            "Song Slides Folder",
            message: "New songs are saved to this folder on the server's Mac.",
            isPresented: isPresented
        ) { folder in
            var settings = try await session.serverSettings()
            settings.songSlideFolder = folder
            try await session.saveServerSettings(settings)
            onSaved(folder)
        }
    }
}

/// A folder picker when this Mac is the server; otherwise a path typed for the server's disk,
/// since a folder picked here would be on the wrong machine.
private struct ServerFolderPrompt: ViewModifier {
    @Environment(AppModel.self) private var model
    let title: String
    let message: String
    @Binding var isPresented: Bool
    let save: (String) async throws -> Void
    @State private var path = ""
    @State private var failure: String?

    func body(content: Content) -> some View {
        Group {
            if model.mode == .server {
                #if os(macOS)
                content.onChange(of: isPresented) { _, presenting in
                    guard presenting else { return }
                    isPresented = false
                    // After this update: the panel opened mid-update from a plain button never appeared.
                    DispatchQueue.main.async {
                        if let folder = chooseFolder() { submit(folder.path) }
                    }
                }
                #else
                content.fileImporter(isPresented: $isPresented, allowedContentTypes: [.folder]) { result in
                    if case .success(let folder) = result { submit(folder.path) }
                }
                #endif
            } else {
                content.alert(title, isPresented: $isPresented) {
                    TextField("Path on the server", text: $path)
                    Button("Cancel", role: .cancel) {}
                    Button("Save") { submit(path) }
                } message: {
                    Text(message)
                }
            }
        }
        .alert("Couldn't Use Folder", isPresented: Binding(get: { failure != nil }, set: { if !$0 { failure = nil } })) {
            Button("OK", role: .cancel) {}
        } message: {
            Text(failure ?? "")
        }
    }

    private func submit(_ folder: String) {
        let folder = folder.trimmed
        guard !folder.isEmpty else { return }
        Task {
            do {
                try await save(folder)
                path = ""
            } catch {
                failure = error.metocastMessage
            }
        }
    }
}

extension View {
    /// Search for the deck library. On the Mac it sits at the top of the list rather than in the
    /// toolbar, where it would land after Notifications and Settings; the iPhone keeps its bar.
    func librarySearch(_ query: Binding<String>) -> some View {
        #if os(macOS)
        safeAreaInset(edge: .top, spacing: 0) {
            HStack(spacing: 6) {
                Image(systemName: "magnifyingglass").foregroundStyle(.secondary)
                TextField("Search Slides", text: query, prompt: Text("Title, number or lyrics"))
                    .textFieldStyle(.plain)
                if !query.wrappedValue.isEmpty {
                    Button("Clear", systemImage: "xmark.circle.fill") { query.wrappedValue = "" }
                        .labelStyle(.iconOnly)
                        .buttonStyle(.plain)
                        .foregroundStyle(.secondary)
                }
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 7)
            .glassEffect(.regular.interactive(), in: .capsule)
            .padding(10)
        }
        #else
        searchable(text: query, prompt: "Title, number or lyrics")
        #endif
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
        case .Refused(let status): "The server refused this request (HTTP \(status))."
        case .Contract: "The server's answer didn't match what this app expects."
        case .NotConnected: "Not connected to the server."
        case .InvalidInput: "Check what you entered and try again."
        default: "Something went wrong."
        }
    }
}
