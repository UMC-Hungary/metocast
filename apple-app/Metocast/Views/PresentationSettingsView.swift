import SwiftUI
import Metocast
#if os(macOS)
import AppKit
#else
import UIKit
#endif

/// Settings that matter only while presenting, opened from the Presentation page rather than kept
/// in Settings: how decks are shown, the Raspberry Pi receiver, and on a Mac the slide fonts.
struct PresentationSettingsView: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    let session: MetocastSession

    var body: some View {
        NavigationStack {
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
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button("Done") { dismiss() }
                }
            }
        }
        #if os(macOS)
        .frame(minWidth: 520, minHeight: 480)
        #endif
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
