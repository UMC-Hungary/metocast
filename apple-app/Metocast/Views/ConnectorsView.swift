import SwiftUI
import Metocast

/// Every connector the server can run, with its state. Picking one opens its settings.
struct ConnectorsView: View {
    let session: MetocastSession

    var body: some View {
        List(session.configurableConnectors, id: \.id) { connector in
            NavigationLink {
                ConnectorFormView(session: session, connector: connector.id, name: connector.name)
            } label: {
                LabeledContent {
                    if let status = session.connectors[connector.id] {
                        Label(status.status.title, systemImage: status.status.symbol)
                            .foregroundStyle(status.status.color)
                            .labelStyle(.titleAndIcon)
                            .font(.subheadline)
                    }
                } label: {
                    Label(connector.name, systemImage: ConnectorInfo(id: connector.id).icon)
                }
            }
        }
        .navigationTitle("Connectors")
    }
}

/// One connector's settings, built from the fields the bridge says it has. Secrets can be
/// replaced or forgotten, never read back.
struct ConnectorFormView: View {
    @Environment(AppModel.self) private var model
    let session: MetocastSession
    let connector: String
    let name: String
    @Environment(\.openURL) private var openURL
    @State private var fields: [ConfigFieldRecord] = []
    @State private var values: [String: String] = [:]
    @State private var cleared: Set<String> = []
    @State private var isLoading = true
    @State private var isSaving = false
    @State private var failure: String?
    @State private var discovered: [DiscoveredDeviceRecord] = []
    @State private var isDiscovering = false

    /// The tool that finds this connector's device on the network, when it has one.
    private var discoveryTool: DiscoveryTool? {
        switch connector {
        case "atem": .atem
        case "middlecontrol": .middlecontrol
        case "blackmagic-camera": .camera
        case "broadlink": .broadlink
        default: nil
        }
    }

    private var signsIn: Bool { connector == "youtube" || connector == "facebook" }

    var body: some View {
        Form {
            if let status = session.connectors[connector] {
                Section {
                    LabeledContent("State") {
                        Text(status.status.title).foregroundStyle(status.status.color)
                    }
                    if let message = status.message, !message.isEmpty {
                        Text(message).foregroundStyle(.secondary)
                    }
                }
            }
            if let failure {
                Section {
                    Label(failure, systemImage: "exclamationmark.triangle").foregroundStyle(.red)
                }
            }
            Section {
                ForEach(fields, id: \.key) { field in
                    row(field)
                }
            } footer: {
                if fields.contains(where: { $0.kind == .secret }) {
                    Text("Saved passwords and keys stay on the server and are never shown again.")
                }
            }
            if discoveryTool != nil {
                Section("Find on the Network") {
                    Button(isDiscovering ? "Scanning…" : "Discover Devices", systemImage: "antenna.radiowaves.left.and.right") {
                        Task { await discover() }
                    }
                    .disabled(isDiscovering)
                    ForEach(discovered, id: \.id) { device in
                        Button {
                            values["host"] = device.host
                            if device.port > 0 { values["port"] = String(device.port) }
                        } label: {
                            VStack(alignment: .leading, spacing: 2) {
                                Text(device.name.isEmpty ? device.host : device.name)
                                Text(device.port == 0 ? device.host : "\(device.host):\(device.port)")
                                    .font(.subheadline)
                                    .foregroundStyle(.secondary)
                            }
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .contentShape(.rect)
                        }
                        .buttonStyle(.plain)
                    }
                }
            }
            #if os(macOS)
            if model.mode == .server, fields.contains(where: { $0.kind == .secret }) {
                Section {
                    Button("Show Stored Secrets", systemImage: "eye") { Task { await reveal() } }
                } footer: {
                    Text("Only this Mac can read back what it stored.")
                }
            }
            #endif
            if signsIn {
                Section {
                    Button("Sign In…", systemImage: "person.badge.key") {
                        Task { await signIn() }
                    }
                    Button("Sign Out", role: .destructive) {
                        Task { try? await session.signOut(connector) }
                    }
                } footer: {
                    Text("Sign-in opens a browser and finishes on the server.")
                }
            }
        }
        .formStyle(.grouped)
        .navigationTitle(name)
        .toolbar {
            ToolbarItem(placement: .confirmationAction) {
                Button("Save") { Task { await save() } }
                    .disabled(isLoading || isSaving)
            }
        }
        .overlay {
            if isLoading { ProgressView() }
        }
        .task(id: connector) { await load() }
    }

    @ViewBuilder
    private func row(_ field: ConfigFieldRecord) -> some View {
        switch field.kind {
        case .toggle:
            Toggle(field.label, isOn: Binding(
                get: { values[field.key] == "true" },
                set: { values[field.key] = $0 ? "true" : "false" }
            ))
        case .number:
            LabeledContent(field.label) {
                TextField(field.label, text: binding(field.key))
                    .multilineTextAlignment(.trailing)
                    .labelsHidden()
                    #if os(iOS)
                    .keyboardType(.numberPad)
                    #endif
            }
        case .text:
            LabeledContent(field.label) {
                TextField(field.label, text: binding(field.key))
                    .multilineTextAlignment(.trailing)
                    .labelsHidden()
                    #if os(iOS)
                    .textInputAutocapitalization(.never)
                    .autocorrectionDisabled()
                    #endif
            }
        case .secret:
            VStack(alignment: .leading, spacing: 4) {
                LabeledContent(field.label) {
                    SecureField(field.label, text: binding(field.key), prompt: Text(field.isSet && !cleared.contains(field.key) ? "Stored" : "None"))
                        .multilineTextAlignment(.trailing)
                        .labelsHidden()
                }
                if field.isSet, values[field.key].orEmpty.isEmpty {
                    Button(cleared.contains(field.key) ? "Keep Stored \(field.label)" : "Forget Stored \(field.label)") {
                        if cleared.contains(field.key) { cleared.remove(field.key) } else { cleared.insert(field.key) }
                    }
                    .font(.subheadline)
                }
            }
        }
    }

    private func binding(_ key: String) -> Binding<String> {
        Binding(get: { values[key] ?? "" }, set: { values[key] = $0 })
    }

    private func load() async {
        isLoading = true
        defer { isLoading = false }
        do {
            fields = try await session.connectorForm(connector)
            values = Dictionary(uniqueKeysWithValues: fields.map { ($0.key, $0.value) })
            cleared = []
            failure = nil
        } catch {
            failure = error.metocastMessage
        }
    }

    private func save() async {
        isSaving = true
        defer { isSaving = false }
        do {
            try await session.saveConnector(connector, values: fields.map { field in
                ConfigValueRecord(key: field.key, value: values[field.key] ?? "", clear: cleared.contains(field.key))
            })
            await load()
        } catch {
            failure = error.metocastMessage
        }
    }

    private func discover() async {
        guard let tool = discoveryTool else { return }
        isDiscovering = true
        defer { isDiscovering = false }
        do {
            discovered = try await session.discover(tool).devices
            failure = discovered.isEmpty ? "No devices answered. Enter the address by hand." : nil
        } catch {
            failure = error.metocastMessage
        }
    }

    #if os(macOS)
    /// Fills the secret fields in with what the server stored, which needs this run's admin
    /// token and a request from this Mac itself.
    private func reveal() async {
        do {
            for field in try await session.connectorSecrets(connector, adminToken: model.server.adminToken) {
                values[field.key] = field.value
            }
            failure = nil
        } catch {
            failure = error.metocastMessage
        }
    }
    #endif

    private func signIn() async {
        do {
            guard let url = try await session.authURL(connector) else { return }
            openURL(url)
        } catch {
            failure = error.metocastMessage
        }
    }
}

extension Optional where Wrapped == String {
    var orEmpty: String { self ?? "" }
}
