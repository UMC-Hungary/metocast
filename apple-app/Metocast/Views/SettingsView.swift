import SwiftUI
import Metocast
#if os(macOS)
import CoreImage.CIFilterBuiltins
#endif

// One settings form for both platforms: the iPhone's Settings tab and the Mac window's inspector.
struct SettingsView: View {
    @Environment(AppModel.self) private var model
    @AppStorage("appearance") private var appearance = Appearance.system
    @State private var isConfirmingReset = false
    @State private var isShowingDevices = false
    @State private var isShowingConnectors = false
    @State private var isShowingTools = false

    var body: some View {
        Form {
            #if os(macOS)
            if model.mode == .server {
                ServerSection()
            }
            #endif
            if model.mode == .client, let session = model.session {
                Section("Server") {
                    LabeledContent("Address", value: ServerAddress.displayName(session.baseURL))
                }
            }

            Section {
                Button(resetTitle, role: .destructive) { isConfirmingReset = true }
            } footer: {
                Text(model.mode == .server
                    ? "Stops the server and shows setup again. Events and settings stay on this Mac."
                    : "This device forgets the server and its token.")
            }

            Section("Equipment") {
                #if os(iOS)
                NavigationLink("Connectors") {
                    if let session = model.session { ConnectorsView(session: session) }
                }
                .disabled(model.session == nil)
                NavigationLink("Server Tools") {
                    if let session = model.session { ServerToolsView(session: session) }
                }
                .disabled(model.session == nil)
                #else
                Button("Connectors…", systemImage: "puzzlepiece.extension") { isShowingConnectors = true }
                    .disabled(model.session == nil)
                Button("Server Tools…", systemImage: "wrench.and.screwdriver") { isShowingTools = true }
                    .disabled(model.session == nil)
                Button("Discover Devices…", systemImage: "network") { isShowingDevices = true }
                    .disabled(model.session == nil)
                #endif
            }

            if let session = model.session {
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
                            Text(client.label.isEmpty ? "Unnamed device" : client.label)
                            if let hostname = client.hostname {
                                Text(hostname)
                            }
                        }
                    }
                }
            }

            if model.session != nil {
                SlidesSettingsSection()
            }

            Section("Appearance") {
                Picker("Appearance", selection: $appearance) {
                    ForEach(Appearance.allCases, id: \.self) { Text($0.title) }
                }
                .pickerStyle(.segmented)
                .labelsHidden()
            }

            Section("About") {
                LabeledContent("Version", value: Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? "")
                LabeledContent("Core Bridge", value: bridgeVersion())
            }
        }
        .formStyle(.grouped)
        #if os(macOS)
        .sheet(isPresented: $isShowingDevices) {
            if let session = model.session { DevicesView(session: session) }
        }
        .sheet(isPresented: $isShowingTools) {
            if let session = model.session {
                NavigationStack {
                    ServerToolsView(session: session)
                        .toolbar {
                            ToolbarItem(placement: .confirmationAction) {
                                Button("Done") { isShowingTools = false }
                            }
                        }
                }
                .frame(minWidth: 560, minHeight: 460)
            }
        }
        // The Mac's settings live in an inspector, which has no navigation stack of its own.
        .sheet(isPresented: $isShowingConnectors) {
            if let session = model.session {
                NavigationStack {
                    ConnectorsView(session: session)
                        .toolbar {
                            ToolbarItem(placement: .confirmationAction) {
                                Button("Done") { isShowingConnectors = false }
                            }
                        }
                }
                .frame(minWidth: 520, minHeight: 420)
            }
        }
        #endif
        .confirmationDialog(resetTitle + "?", isPresented: $isConfirmingReset) {
            Button(resetTitle, role: .destructive) { Task { await model.reset() } }
        }
    }

    private var resetTitle: String {
        model.mode == .server ? "Stop Hosting" : "Remove Access"
    }
}

#if os(macOS)
/// This Mac's server: how other devices reach it, its token, and its log.
private struct ServerSection: View {
    @Environment(AppModel.self) private var model
    @State private var isShowingToken = false
    @State private var isShowingLog = false
    @State private var isConfirmingNewToken = false

    var body: some View {
        let server = model.server
        Section {
            LabeledContent("Address") {
                Text(ServerAddress.displayName(server.networkURL)).textSelection(.enabled)
            }
            LabeledContent("Token") {
                HStack {
                    Text(isShowingToken ? server.token : String(repeating: "•", count: 12))
                        .textSelection(.enabled)
                        .monospaced()
                    Button(isShowingToken ? "Hide Token" : "Show Token", systemImage: isShowingToken ? "eye.slash" : "eye") {
                        isShowingToken.toggle()
                    }
                    .labelStyle(.iconOnly)
                    .buttonStyle(.borderless)
                }
            }
            Button("New Token…") { isConfirmingNewToken = true }
            Button("Server Log") { isShowingLog = true }
        } header: {
            Text("Server")
        } footer: {
            Text("Enter the address and token on another device, or scan the code below with an iPhone's camera.")
        }
        .confirmationDialog("Create a new token?", isPresented: $isConfirmingNewToken) {
            Button("New Token", role: .destructive) { Task { await model.regenerateServerToken() } }
        } message: {
            Text("Devices using the current token lose access until they connect again with the new one. The server restarts.")
        }
        .sheet(isPresented: $isShowingLog) {
            ServerLogView()
        }
        if server.state == .running, let link = server.connectLink.link, let code = qrCode(link.absoluteString) {
            Section("Connect a Device") {
                code
                    .resizable()
                    .interpolation(.none)
                    .scaledToFit()
                    .frame(maxWidth: 200)
                    .frame(maxWidth: .infinity)
                    .accessibilityLabel("Connection code")
            }
        }
    }
}

private struct ServerLogView: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        ScrollView {
            Text(model.server.log.joined(separator: "\n"))
                .font(.caption.monospaced())
                .textSelection(.enabled)
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding()
        }
        .defaultScrollAnchor(.bottom)
        .frame(minWidth: 640, minHeight: 400)
        .navigationTitle("Server Log")
        .toolbar {
            ToolbarItem(placement: .confirmationAction) {
                Button("Done") { dismiss() }
            }
        }
    }
}

/// A QR code for `text`, drawn with Core Image's built-in generator.
private func qrCode(_ text: String) -> Image? {
    let filter = CIFilter.qrCodeGenerator()
    filter.message = Data(text.utf8)
    guard let output = filter.outputImage,
          let image = CIContext().createCGImage(output, from: output.extent)
    else { return nil }
    return Image(decorative: image, scale: 1)
}
#endif

enum Appearance: String, CaseIterable {
    case system, light, dark

    var title: String {
        switch self {
        case .system: "System"
        case .light: "Light"
        case .dark: "Dark"
        }
    }

    var colorScheme: ColorScheme? {
        switch self {
        case .system: nil
        case .light: .light
        case .dark: .dark
        }
    }
}

/// Where the server writes generated decks, and how it builds a published title.
private struct SlidesSettingsSection: View {
    @Environment(AppModel.self) private var model
    @State private var settings: ServerSettingsRecord?
    @State private var isSaving = false
    @State private var failure: String?
    @State private var saved = false

    var body: some View {
        Section {
            if var current = settings {
                let binding = Binding(get: { current }, set: { settings = $0 })
                LabeledContent("Title Template") {
                    TextField("Title Template", text: Binding(
                        get: { current.titleTemplate },
                        set: { current.titleTemplate = $0; binding.wrappedValue = current }
                    ))
                    .multilineTextAlignment(.trailing)
                    .labelsHidden()
                }
                LabeledContent("Bible Slides Folder") {
                    TextField("Path on the server", text: Binding(
                        get: { current.slideFolder },
                        set: { current.slideFolder = $0; binding.wrappedValue = current }
                    ))
                    .multilineTextAlignment(.trailing)
                    .labelsHidden()
                }
                LabeledContent("Song Slides Folder") {
                    TextField("Path on the server", text: Binding(
                        get: { current.songSlideFolder },
                        set: { current.songSlideFolder = $0; binding.wrappedValue = current }
                    ))
                    .multilineTextAlignment(.trailing)
                    .labelsHidden()
                }
                Button(saved ? "Saved" : "Save Slide Settings") { Task { await save() } }
                    .disabled(isSaving)
            } else {
                ProgressView()
            }
            if let failure {
                Label(failure, systemImage: "exclamationmark.triangle").foregroundStyle(.red)
            }
        } header: {
            Text("Slides & Events")
        } footer: {
            Text("Folders are paths on the server's Mac. Generated decks are written there.")
        }
        .task { await load() }
    }

    private func load() async {
        do {
            settings = try await model.session?.serverSettings()
        } catch {
            failure = error.metocastMessage
        }
    }

    private func save() async {
        guard let settings else { return }
        isSaving = true
        defer { isSaving = false }
        do {
            try await model.session?.saveServerSettings(settings)
            failure = nil
            saved = true
            try? await Task.sleep(for: .seconds(2))
            saved = false
        } catch {
            failure = error.metocastMessage
        }
    }
}
