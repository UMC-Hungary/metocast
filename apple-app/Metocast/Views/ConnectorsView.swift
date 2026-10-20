import SwiftUI
import Metocast

/// Every connector the server can run, with its state. Picking one opens its settings.
struct ConnectorsView: View {
    let session: MetocastSession

    var body: some View {
        let connectors = session.configurableConnectors
        List {
            ForEach(ConnectorCategory.allCases, id: \.self) { category in
                let members = connectors.filter { ConnectorInfo(id: $0.id).category == category }
                if !members.isEmpty {
                    Section {
                        ForEach(members, id: \.id) { row($0) }
                    } header: {
                        Text(category.title)
                    } footer: {
                        if !category.footer.isEmpty { Text(category.footer) }
                    }
                }
            }
        }
        .navigationTitle("Connectors")
    }

    private func row(_ connector: ConnectorNameRecord) -> some View {
        NavigationLink {
            ConnectorFormView(session: session, connector: connector.id, name: connector.name)
                .windowActions()
        } label: {
            LabeledContent {
                if let status = session.connectors[connector.id] {
                    Label(status.title, systemImage: status.symbol)
                        .foregroundStyle(status.color)
                        .labelStyle(.titleAndIcon)
                        .font(.subheadline)
                }
            } label: {
                Label(connector.name, systemImage: ConnectorInfo(id: connector.id).icon)
            }
        }
        // A Mac list row link is otherwise an element of unknown role.
        .accessibilityAddTraits(.isButton)
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

    /// Devices that stream on their own and take the YouTube destination from the server.
    private var youtubePush: ProductionControl? {
        switch connector {
        case "blackmagic-camera": .cameraPushYoutube
        case "atem": .atemPushYoutube
        default: nil
        }
    }

    private var signsIn: Bool { ConnectorInfo(id: connector).signsIn }

    var body: some View {
        Form {
            if let status = session.connectors[connector] {
                Section {
                    LabeledContent("State") {
                        Text(status.title).foregroundStyle(status.color)
                    }
                    if let message = status.detail {
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
            if let push = youtubePush {
                let deviceReady = session.connectors[connector]?.status == .connected
                let youtubeReady = session.connectors["youtube"]?.status == .connected
                Section {
                    Button("Send YouTube Stream Settings", systemImage: "arrow.up.forward.square") {
                        session.control(push)
                    }
                    .disabled(!deviceReady || !youtubeReady)
                } header: {
                    Text("YouTube Stream")
                } footer: {
                    if !deviceReady {
                        Text("Connect the \(name) first.")
                    } else if !youtubeReady {
                        Text("Sign in to YouTube under Connectors first.")
                    } else {
                        Text("Sets the \(name)'s streaming destination to your YouTube channel's stream address and key.")
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
            #if os(macOS)
            if connector == "mcp", model.mode == .server {
                AgentSetupSection(
                    url: model.server.localURL + "/mcp",
                    token: model.server.token,
                    agents: session.clients.filter { $0.label.hasPrefix("MCP") }
                )
            }
            #endif
            if signsIn {
                Section {
                    Button("Sign In…", systemImage: "person.badge.key") {
                        Task { await signIn() }
                    }
                    Button("Sign Out", role: .destructive) {
                        Task { await signOut() }
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
                    .pointingHand()
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
                        .accessibilityValue(field.isSet && !cleared.contains(field.key) ? "Stored" : "Not set")
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
            guard let url = try await session.authURL(connector) else {
                // The server answered with something that is not a URL, which no retry fixes.
                failure = "The server did not send a usable sign-in address."
                return
            }
            failure = nil
            openURL(url)
        } catch {
            failure = error.metocastMessage
        }
    }

    /// Sign-out failures used to be dropped, so the button looked like it had worked.
    private func signOut() async {
        do {
            try await session.signOut(connector)
            failure = nil
        } catch {
            failure = error.metocastMessage
        }
    }
}

extension Optional where Wrapped == String {
    var orEmpty: String { self ?? "" }
}

#if os(macOS)
/// How to point an MCP client at this server, and who is connected. The copied text carries the
/// access token, so it is never shown on screen.
private struct AgentSetupSection: View {
    let url: String
    let token: String
    let agents: [ConnectedClientRecord]
    @State private var copied: String?
    @State private var testing = false
    @State private var testResult: String?

    private var claudeCode: String {
        "claude mcp add --transport http metocast \(url) --header \"Authorization: Bearer \(token)\""
    }

    /// Claude Desktop only starts programs, so it starts the bridge inside this app. Nothing else
    /// has to be installed. The path is the stable link the app keeps, not a path into the bundle.
    private var claudeDesktop: String {
        let config: [String: Any] = ["mcpServers": ["metocast": [
            "command": ServerHost.bridgeLink.path,
            "args": ["mcp-stdio"],
            "env": ["METOCAST_MCP_URL": url, "METOCAST_MCP_TOKEN": token],
        ]]]
        let json = try? JSONSerialization.data(withJSONObject: config, options: [.sortedKeys, .withoutEscapingSlashes])
        return String(decoding: json ?? Data(), as: UTF8.self)
    }

    var body: some View {
        Section {
            LabeledContent("Address") {
                Text(url).font(.footnote.monospaced()).textSelection(.enabled)
            }
            copyButton("Copy Command for Claude Code", text: claudeCode)
            copyButton("Copy Config for Claude Desktop", text: claudeDesktop)
            Button("Install in Claude Desktop", systemImage: "square.and.arrow.down", action: installExtension)
            copyButton("Copy Access Token", text: token)
            Button("Test Connection", systemImage: "checkmark.circle") { Task { await test() } }
                .disabled(testing)
            if let testResult {
                Text(testResult).font(.footnote).foregroundStyle(.secondary).textSelection(.enabled)
            }
            if let helper = ServerHost.helper {
                Button("Show Bridge in Finder", systemImage: "folder") {
                    NSWorkspace.shared.activateFileViewerSelecting([helper])
                }
            }
        } header: {
            Text("Connect an Agent")
        } footer: {
            Text("The copied text contains this server's access token. Paste it into your agent's settings and nowhere else. Save the switches above first.")
        }
        Section("Connected Agents") {
            if agents.isEmpty {
                Text("No agent connected").foregroundStyle(.secondary)
            }
            ForEach(agents, id: \.id) { agent in
                Text(agent.label)
            }
        }
    }

    /// Opens the extension that ships in the app, which Claude Desktop offers to install. It asks
    /// for the access token, which the button above copies.
    private func installExtension() {
        guard let package = Bundle.main.url(forResource: "Metocast", withExtension: "mcpb") else {
            testResult = "The Claude Desktop extension is missing from this build."
            return
        }
        if !NSWorkspace.shared.open(package) {
            NSWorkspace.shared.activateFileViewerSelecting([package])
        }
    }

    /// Runs the bridge's own check, the same one support asks for: `metocast-server mcp-stdio --check`.
    private func test() async {
        guard let helper = ServerHost.helper else {
            testResult = "The bridge is missing from this build."
            return
        }
        testing = true
        defer { testing = false }
        let environment = ["METOCAST_MCP_URL": url, "METOCAST_MCP_TOKEN": token]
        testResult = await Task.detached {
            let process = Process()
            process.executableURL = helper
            process.arguments = ["mcp-stdio", "--check"]
            process.environment = environment
            let output = Pipe()
            process.standardOutput = output
            process.standardError = output
            do {
                try process.run()
            } catch {
                return "Couldn't start the bridge: \(error.localizedDescription)"
            }
            let text = String(decoding: output.fileHandleForReading.readDataToEndOfFile(), as: UTF8.self)
            process.waitUntilExit()
            return text.trimmingCharacters(in: .whitespacesAndNewlines)
        }.value
    }

    private func copyButton(_ title: String, text: String) -> some View {
        Button(copied == title ? "Copied" : title, systemImage: "doc.on.doc") {
            NSPasteboard.general.clearContents()
            NSPasteboard.general.setString(text, forType: .string)
            copied = title
            Task {
                try? await Task.sleep(for: .seconds(2))
                copied = nil
            }
        }
    }
}
#endif
