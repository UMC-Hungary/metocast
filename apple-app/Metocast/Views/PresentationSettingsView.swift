import SwiftUI
import UniformTypeIdentifiers
import Metocast
#if os(macOS)
import AppKit
#else
import UIKit
#endif

/// Settings that matter only while presenting, a page inside Presentation rather than kept in
/// Settings: how decks are shown, the welcome screen, the Raspberry Pi receiver, and on a Mac the
/// slide fonts.
struct PresentationSettingsView: View {
    @Environment(AppModel.self) private var model
    let session: MetocastSession

    var body: some View {
            Form {
                Section {
                    Toggle("Web Presenter", isOn: Binding(
                        get: { session.usesWebPresenter },
                        set: { session.show(.setUseWebPresenter(enabled: $0)) }
                    ))
                    Picker("Slide Design", selection: Binding(
                        get: { session.presenterTheme },
                        set: { session.show(.setPresenterTheme(theme: $0)) }
                    )) {
                        Text("Classic").tag(PresenterTheme.classic)
                        Text("Editorial").tag(PresenterTheme.editorial)
                    }
                } header: {
                    Text("Presentation")
                } footer: {
                    Text(session.usesWebPresenter
                        ? "Decks open in Metocast's own presenter on the server's screen."
                        : "Decks open in Keynote on the server's Mac.")
                }
                WelcomeScreenSection(session: session)
                Section("Connected Devices") {
                    if session.clients.isEmpty {
                        Text("No other devices").foregroundStyle(.secondary)
                    }
                    ForEach(session.clients, id: \.id) { client in
                        LabeledContent {
                            if let latency = client.latencyMs {
                                Text("\(latency) ms").foregroundStyle(.secondary).monospacedDigit()
                            }
                        } label: {
                            Text(client.label.isEmpty ? String(localized: "Unnamed device") : client.label)
                            if let hostname = client.hostname {
                                Text(hostname)
                            }
                        }
                    }
                }
                PresenterInstallSection(baseURL: receiverBaseURL)
                #if os(macOS)
                PresentationFontsSection()
                #endif
            }
            .formStyle(.grouped)
            .navigationTitle("Presentation Settings")
    }

    /// An address the Pi can reach: the Mac hosting the server knows its network one, while a
    /// client's own connection is already a network address.
    private var receiverBaseURL: String {
        #if os(macOS)
        if model.mode == .server { return model.server.networkURL }
        #endif
        return session.baseURL
    }
}

/// The welcome screen a Slide step shows: its text, how often the photo changes, the 12 photos
/// (each replaceable from this computer) and the logo. Text saves itself once typing pauses.
private struct WelcomeScreenSection: View {
    let session: MetocastSession
    @State private var settings: WelcomeSettingsRecord?
    /// Thumbnails by photo number; reloaded after a replace or reset.
    @State private var thumbnails: [UInt32: Data] = [:]
    @State private var importTarget: ImportTarget?
    @State private var busy: UInt32?
    @State private var failure: String?
    /// The month whose slide is being previewed.
    @State private var previewing: ImportTarget?
    /// A replace asked for in the preview, opened once the preview has closed.
    @State private var pendingImport: ImportTarget?

    /// The default greeting and blessing in each language, swapped in on a language change
    /// unless the operator wrote their own.
    private static let defaults = [
        "hu": ("Isten hozott!", "Az ÚR áldása legyen életeden!"),
        "en": ("Welcome!", "The Lord bless you and keep you!"),
    ]

    /// What the file picker is choosing for: a photo (1–12) or the logo (0).
    private struct ImportTarget: Identifiable {
        let number: UInt32
        var id: UInt32 { number }
    }

    var body: some View {
        Section {
            if let settings {
                TextField("Greeting", text: field(\.title), prompt: Text("Isten hozott!"))
                TextField("Blessing", text: field(\.subtitle), prompt: Text("Az ÚR áldása legyen életeden!"))
                TextField("Church Name", text: field(\.churchName), prompt: Text("Nyíregyházi Metodista Gyülekezet"))
                Picker("Language", selection: Binding(get: { settings.language }, set: { switchLanguage(to: $0) })) {
                    Text(verbatim: "Magyar").tag("hu")
                    Text(verbatim: "English").tag("en")
                }
                LazyVGrid(columns: Array(repeating: GridItem(.flexible(), spacing: 8), count: 4), spacing: 12) {
                    ForEach(UInt32(1)...12, id: \.self) { number in
                        photo(number, replaced: settings.replacedPhotos.contains(number))
                    }
                }
                .padding(.vertical, 4)
                LabeledContent("Logo") {
                    HStack {
                        Text(settings.hasLogo ? "Set" : "None").foregroundStyle(.secondary)
                        Button(settings.hasLogo ? "Replace…" : "Choose…") { importTarget = ImportTarget(number: 0) }
                        if settings.hasLogo {
                            Button("Remove", role: .destructive) { Task { await change { try await session.removeWelcomeLogo() } } }
                        }
                    }
                }
            } else {
                ProgressView()
            }
            if let failure {
                Label(failure, systemImage: "exclamationmark.triangle").foregroundStyle(.red)
            }
        } header: {
            Text("Welcome Screen")
        } footer: {
            Text("A Slide step in the Order of Service shows this with the web presenter: the photo of the event's month, the logo, its title and the blessing, and the church name with the date. Click a month to preview it. The text stays readable: a darker overlay is added behind it on bright photos.")
        }
        .fileImporter(
            isPresented: Binding(get: { importTarget != nil }, set: { if !$0 { importTarget = nil } }),
            allowedContentTypes: importTarget?.number == 0 ? [.png, .svg] : [.image]
        ) { result in
            guard let target = importTarget else { return }
            importTarget = nil
            Task { await upload(result, to: target.number) }
        }
        .task { await load() }
        // Saved once typing pauses; a newer edit cancels the wait.
        .sheet(item: $previewing, onDismiss: {
            importTarget = pendingImport
            pendingImport = nil
        }) { target in
            WelcomePreview(
                session: session,
                month: target.number,
                isReplaced: settings?.replacedPhotos.contains(target.number) == true,
                replace: { pendingImport = target; previewing = nil },
                reset: { Task { await change(photo: target.number) { try await session.resetWelcomePhoto(target.number) } } }
            )
        }
        .task(id: settings.map { "\($0.title)|\($0.subtitle)|\($0.churchName)|\($0.language)" }) {
            guard let settings else { return }
            do {
                try await Task.sleep(for: .milliseconds(800))
                _ = try await session.saveWelcomeSettings(settings)
                failure = nil
            } catch is CancellationError {
                return
            } catch {
                failure = error.metocastMessage
            }
        }
    }

    private func field(_ path: WritableKeyPath<WelcomeSettingsRecord, String>) -> Binding<String> {
        Binding(get: { settings?[keyPath: path] ?? "" }, set: { settings?[keyPath: path] = $0 })
    }

    /// A month's photo with the month's name; a click previews that month's slide.
    private func photo(_ number: UInt32, replaced: Bool) -> some View {
        Button { previewing = ImportTarget(number: number) } label: {
            VStack(spacing: 4) {
                ZStack {
                    Rectangle().fill(.quaternary)
                    if let data = thumbnails[number], let image = PlatformImage(data: data) {
                        Image(platformImage: image).resizable().scaledToFill()
                    }
                    if busy == number { ProgressView() }
                }
                .aspectRatio(16 / 9, contentMode: .fit)
                .clipShape(.rect(cornerRadius: 6))
                .overlay(alignment: .bottomTrailing) {
                    if replaced {
                        Image(systemName: "pencil.circle.fill").padding(4).foregroundStyle(.white).shadow(radius: 2)
                    }
                }
                Text(Calendar.current.standaloneMonthSymbols[Int(number) - 1].localizedCapitalized)
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
        }
        .buttonStyle(.plain)
        .accessibilityLabel(replaced
            ? Text("\(Calendar.current.standaloneMonthSymbols[Int(number) - 1]), photo replaced")
            : Text(Calendar.current.standaloneMonthSymbols[Int(number) - 1]))
        .accessibilityHint("Shows that month's welcome slide")
    }

    /// The other language's default text becomes this one's; text the operator wrote stays.
    private func switchLanguage(to language: String) {
        guard var updated = settings, let old = Self.defaults[updated.language], let new = Self.defaults[language] else { return }
        if updated.title == old.0 { updated.title = new.0 }
        if updated.subtitle == old.1 { updated.subtitle = new.1 }
        updated.language = language
        settings = updated
    }

    private func load() async {
        do {
            settings = try await session.welcomeSettings()
            for number in UInt32(1)...12 {
                thumbnails[number] = try await session.welcomePhoto(number)
            }
        } catch {
            failure = error.metocastMessage
        }
    }

    /// Runs one upload, reset or removal, then re-reads what changed.
    private func change(photo number: UInt32? = nil, _ action: () async throws -> Void) async {
        busy = number
        defer { busy = nil }
        do {
            try await action()
            let saved = settings
            settings = try await session.welcomeSettings()
            // Keep text typed since the last save.
            if let saved {
                settings?.title = saved.title
                settings?.subtitle = saved.subtitle
                settings?.churchName = saved.churchName
                settings?.language = saved.language
            }
            if let number { thumbnails[number] = try await session.welcomePhoto(number) }
            failure = nil
        } catch {
            failure = error.metocastMessage
        }
    }

    private func upload(_ result: Result<URL, Error>, to number: UInt32) async {
        do {
            let url = try result.get()
            let access = url.startAccessingSecurityScopedResource()
            defer { if access { url.stopAccessingSecurityScopedResource() } }
            let data = try Data(contentsOf: url)
            if number == 0 {
                await change { try await session.setWelcomeLogo(data) }
            } else {
                await change(photo: number) { try await session.replaceWelcomePhoto(number, with: data) }
            }
        } catch {
            failure = error.metocastMessage
        }
    }
}

/// One month's welcome slide as the screens will show it, with that photo's actions.
private struct WelcomePreview: View {
    @Environment(\.dismiss) private var dismiss
    let session: MetocastSession
    let month: UInt32
    let isReplaced: Bool
    let replace: () -> Void
    let reset: () -> Void
    @State private var svg: String?
    @State private var failure: String?

    var body: some View {
        NavigationStack {
            VStack(spacing: 16) {
                ZStack {
                    Color.black
                    if let svg {
                        SVGView(svg: SVGView.scalable(svg, width: 1280, height: 720))
                    } else if let failure {
                        Label(failure, systemImage: "exclamationmark.triangle").foregroundStyle(.white)
                    } else {
                        ProgressView()
                    }
                }
                .aspectRatio(16 / 9, contentMode: .fit)
                .clipShape(.rect(cornerRadius: 10))
                HStack {
                    if isReplaced {
                        Button("Reset to Default", systemImage: "arrow.uturn.backward") { reset(); dismiss() }
                    }
                    Spacer()
                    Button("Replace…", systemImage: "photo") { replace() }
                        .buttonStyle(.glassProminent)
                }
            }
            .padding()
            .navigationTitle(Calendar.current.standaloneMonthSymbols[Int(month) - 1].localizedCapitalized)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Done") { dismiss() }
                }
            }
        }
        #if os(macOS)
        .frame(minWidth: 640, minHeight: 460)
        #endif
        .task {
            do {
                svg = try await session.welcomePreview(month)
            } catch {
                failure = error.metocastMessage
            }
        }
    }
}

#if os(macOS)
private typealias PlatformImage = NSImage
private extension Image {
    init(platformImage: NSImage) { self.init(nsImage: platformImage) }
}
#else
private typealias PlatformImage = UIImage
private extension Image {
    init(platformImage: UIImage) { self.init(uiImage: platformImage) }
}
#endif

/// The Raspberry Pi installer, with the same auto-start choice as the web UI's.
private struct PresenterInstallSection: View {
    let baseURL: String
    @State private var isAutoStarting = false
    @State private var didCopy = false

    private var command: String {
        let ws = baseURL
            .replacingOccurrences(of: "https://", with: "wss://")
            .replacingOccurrences(of: "http://", with: "ws://") + "/ws"
        let service = isAutoStarting ? " --service" : ""
        return "curl -fsSL https://raw.githubusercontent.com/UMC-Hungary/metocast/master/presenter-receiver/install.sh | bash -s -- \(ws)\(service)"
    }

    var body: some View {
        Section {
            Toggle("Auto-start on boot", isOn: $isAutoStarting)
            Text(command)
                .font(.caption.monospaced())
                .textSelection(.enabled)
            Button(didCopy ? "Copied" : "Copy Install Command", systemImage: "doc.on.doc") {
                #if os(macOS)
                NSPasteboard.general.clearContents()
                NSPasteboard.general.setString(command, forType: .string)
                #else
                UIPasteboard.general.string = command
                #endif
                didCopy = true
            }
        } header: {
            Text("Presenter Receiver")
        } footer: {
            Text("Run this on the Raspberry Pi display. Auto-start sets up console auto-login on TTY1 (Linux / systemd only).")
        }
        .task(id: didCopy) {
            guard didCopy else { return }
            try? await Task.sleep(for: .seconds(2))
            didCopy = false
        }
    }
}

#if os(macOS)
/// The faces generated decks are written in. Keynote and PowerPoint show them only once they're
/// installed, which Font Book does after the person confirms.
private struct PresentationFontsSection: View {
    @State private var isInstalled = PresentationFonts.isInstalled
    @State private var failure: String?

    var body: some View {
        Section {
            LabeledContent("Cormorant Garamond, Geist Mono") {
                Text(isInstalled ? "Installed" : "Not installed")
                    .foregroundStyle(isInstalled ? .green : .orange)
            }
            Button(isInstalled ? "Install Again…" : "Install Fonts…", systemImage: "textformat") {
                do {
                    try PresentationFonts.openInFontBook()
                    failure = nil
                } catch {
                    failure = error.localizedDescription
                }
            }
            if let failure {
                Label(failure, systemImage: "exclamationmark.triangle").foregroundStyle(.red)
            }
        } header: {
            Text("Slide Fonts")
        } footer: {
            Text("Opens them in Font Book; choose Install there. Generated decks use these fonts.")
        }
        // Font Book installs them while this app is in the background.
        .onReceive(NotificationCenter.default.publisher(for: NSApplication.didBecomeActiveNotification)) { _ in
            isInstalled = PresentationFonts.isInstalled
        }
    }
}

/// The same fonts and checks as the desktop app's `install_presentation_fonts`.
private enum PresentationFonts {
    static let files = ["CormorantGaramond-Regular.ttf", "GeistMono-Regular.ttf"]
    /// Earlier variable versions, which would shadow these.
    static let legacyFiles = ["Metocast-CormorantGaramond-Variable.ttf", "Metocast-GeistMono-Variable.ttf"]

    static var folder: URL { URL.libraryDirectory.appending(path: "Fonts") }

    static var isInstalled: Bool {
        files.allSatisfy { FileManager.default.fileExists(atPath: folder.appending(path: $0).path) }
    }

    static func openInFontBook() throws {
        let sources = try files.map { name in
            guard let url = Bundle.main.url(forResource: name, withExtension: nil) else {
                throw CocoaError(.fileNoSuchFile, userInfo: [NSLocalizedDescriptionKey: String(localized: "The app is missing its copy of \(name).")])
            }
            return url
        }
        for name in legacyFiles {
            let legacy = folder.appending(path: name)
            if FileManager.default.fileExists(atPath: legacy.path) {
                try FileManager.default.removeItem(at: legacy)
            }
        }
        guard let fontBook = NSWorkspace.shared.urlForApplication(withBundleIdentifier: "com.apple.FontBook") else {
            throw CocoaError(.fileNoSuchFile, userInfo: [NSLocalizedDescriptionKey: String(localized: "Font Book isn't on this Mac.")])
        }
        NSWorkspace.shared.open(sources, withApplicationAt: fontBook, configuration: NSWorkspace.OpenConfiguration())
    }
}
#endif
