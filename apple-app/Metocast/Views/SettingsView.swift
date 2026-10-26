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
                NavigationLink("Automations") {
                    if let session = model.session { AutomationsView(session: session) }
                }
                .disabled(model.session == nil)
                NavigationLink("Server Tools") {
                    if let session = model.session { ServerToolsView(session: session) }
                }
                .disabled(model.session == nil)
                #else
                // Each opens in the window's content column, like a sidebar item.
                equipmentButton("Connectors", systemImage: "puzzlepiece.extension", page: .connectors)
                equipmentButton("Automations", systemImage: "bolt", page: .automations)
                equipmentButton("Server Tools", systemImage: "wrench.and.screwdriver", page: .serverTools)
                equipmentButton("Discover Devices", systemImage: "network", page: .devices)
                #endif
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
        .confirmationDialog(resetTitle + "?", isPresented: $isConfirmingReset) {
            Button(resetTitle, role: .destructive) { Task { await model.reset() } }
        }
    }

    private var resetTitle: String {
        model.mode == .server ? String(localized: "Stop Hosting") : String(localized: "Remove Access")
    }

    #if os(macOS)
    private func equipmentButton(_ title: LocalizedStringKey, systemImage: String, page: EquipmentPage) -> some View {
        Button(title, systemImage: systemImage) { model.equipment = page }
            .fontWeight(model.equipment == page ? .semibold : nil)
            .accessibilityAddTraits(model.equipment == page ? .isSelected : [])
            .disabled(model.session == nil)
    }
    #endif
}

extension View {
    /// The pointing hand over a clickable control, as on the web; iPhone has no pointer.
    @ViewBuilder func pointingHand() -> some View {
        #if os(macOS)
        pointerStyle(.link)
        #else
        self
        #endif
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
                CopyableLinks(urls: server.networkURLs)
            }
            LabeledContent("Token") {
                HStack {
                    Text(isShowingToken && !Redaction.isOn ? server.token : String(repeating: "•", count: 12))
                        .textSelection(.enabled)
                        .monospaced()
                        // Not "bullet" twelve times.
                        .accessibilityLabel(isShowingToken && !Redaction.isOn ? server.token : String(localized: "Hidden"))
                    // Redaction always masks the token, so the toggle would do nothing.
                    if !Redaction.isOn {
                        Button {
                            isShowingToken.toggle()
                        } label: {
                            Label {
                                Text(isShowingToken ? "Hide Token" : "Show Token")
                            } icon: {
                                SwapSymbol(isShowingToken ? "eye.slash" : "eye", among: ["eye", "eye.slash"])
                            }
                        }
                        .labelStyle(.iconOnly)
                        .buttonStyle(.borderless)
                    }
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
                    .accessibilityHint("Scan it with an iPhone's camera to connect the iPhone to this server.")
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
    return Image(image, scale: 1, label: Text("Connection code"))
}
#endif

enum Appearance: String, CaseIterable {
    case system, light, dark

    var title: String {
        switch self {
        case .system: String(localized: "System")
        case .light: String(localized: "Light")
        case .dark: String(localized: "Dark")
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
                    TextField("Bible Slides Folder", text: Binding(
                        get: { current.slideFolder },
                        set: { current.slideFolder = $0; binding.wrappedValue = current }
                    ), prompt: Text("Path on the server"))
                    .multilineTextAlignment(.trailing)
                    .labelsHidden()
                }
                LabeledContent("Song Slides Folder") {
                    TextField("Song Slides Folder", text: Binding(
                        get: { current.songSlideFolder },
                        set: { current.songSlideFolder = $0; binding.wrappedValue = current }
                    ), prompt: Text("Path on the server"))
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
