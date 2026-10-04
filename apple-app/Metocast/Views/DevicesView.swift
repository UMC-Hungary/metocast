#if os(macOS)
import SwiftUI
import Metocast

/// All discovery runs on the connected server, including when this Mac is a client.
struct DevicesView: View {
    let session: MetocastSession
    @Environment(\.dismiss) private var dismiss
    @State private var selected: DiscoveryTool = .atem
    @State private var results: [DiscoveryTool: DiscoverySnapshot] = [:]
    @State private var scanning = false
    @State private var error: String?
    @State private var scanRequest = 0
    @State private var host = ""
    @State private var port = "9910"
    @State private var enabled = false
    @State private var loadingConfig = false
    @State private var configLoaded = false
    @State private var configRequest = 0
    @State private var saving = false
    @State private var saved = false

    private let tools: [DiscoveryTool] = [.atem, .camera, .middlecontrol, .broadlink, .obs, .rodecaster]
    private var isNetwork: Bool { selected == .atem || selected == .middlecontrol }
    private var connected: Bool { session.status == .connected }
    private var connectorReady: Bool {
        (selected != .obs && selected != .rodecaster) || session.connectors[selected.connector]?.status == .connected
    }

    var body: some View {
        NavigationSplitView {
            List(tools, id: \.self, selection: $selected) { tool in
                Label(tool.title, systemImage: tool.icon).tag(tool)
            }
            .disabled(scanning || saving)
            .navigationSplitViewColumnWidth(190)
        } detail: {
            Form {
                Section {
                    Text(selected.explanation)
                    LabeledContent("Scanning from", value: ServerAddress.displayName(session.baseURL))
                    if let status = session.connectors[selected.connector] {
                        LabeledContent("Connector", value: String(describing: status.status).capitalized)
                        if let message = status.message { Text(message).foregroundStyle(.secondary) }
                    }
                    if !connected { Text("Reconnect to the server to discover devices.").foregroundStyle(.secondary) }
                    if !connectorReady { Text("Connect \(selected.title) before scanning its devices.").foregroundStyle(.secondary) }
                }
                if let error {
                    Section { Label(error, systemImage: "exclamationmark.triangle").foregroundStyle(.red) }
                }
                Section(selected == .broadlink ? "Saved Devices" : "Discovered Devices") {
                    if scanning {
                        ProgressView("Scanning…")
                    } else if let result = results[selected] {
                        if let message = result.message { Text(message).foregroundStyle(.secondary) }
                        if result.devices.isEmpty {
                            Text("No devices found. Check the equipment connection and try again.")
                                .foregroundStyle(.secondary)
                        }
                        ForEach(result.devices, id: \.id) { device in
                            HStack {
                                VStack(alignment: .leading, spacing: 4) {
                                    Text(device.name.isEmpty ? device.host : device.name).font(.headline)
                                    if !device.detail.isEmpty { Text(device.detail).font(.caption).foregroundStyle(.secondary) }
                                    if !device.host.isEmpty {
                                        Text(device.port == 0 ? device.host : "\(device.host):\(device.port)")
                                            .font(.caption.monospaced()).textSelection(.enabled)
                                    }
                                }
                                Spacer()
                                if isNetwork {
                                    Button("Use") {
                                        host = device.host
                                        port = String(device.port)
                                        enabled = true
                                        saved = false
                                    }
                                    .disabled(!configLoaded || saving)
                                    .help("Fill the connection fields below")
                                }
                            }
                        }
                    } else {
                        Text("Choose Scan to look for devices.").foregroundStyle(.secondary)
                    }
                    if selected == .broadlink {
                        Button("Refresh Saved Devices") { refreshOnly = true; scanRequest += 1 }
                            .disabled(scanning || !connected)
                    }
                }
                if isNetwork {
                    Section {
                        Toggle("Enabled", isOn: $enabled)
                        TextField("Host or IP address", text: $host)
                        TextField("Port", text: $port)
                        Button(saving ? "Saving…" : "Save Connection") { save() }
                            .disabled(!configLoaded || !connected || saving || scanning || host.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || UInt16(port) == nil || UInt16(port) == 0)
                        if loadingConfig { ProgressView("Loading connection…") }
                        if !loadingConfig && !configLoaded {
                            Button("Retry Loading Connection") { configRequest += 1 }
                        }
                        if saved { Text("Connection saved. The connector status above shows its progress.").foregroundStyle(.secondary) }
                    } header: {
                        Text("Connection")
                    } footer: {
                        Text("Use a discovered device or enter its address manually. Saving applies these settings to the connected server.")
                    }
                    .disabled(loadingConfig || saving)
                }
            }
            .formStyle(.grouped)
            .navigationTitle(selected.title)
        }
        .frame(minWidth: 760, idealWidth: 820, minHeight: 560, idealHeight: 640)
        .toolbar {
            ToolbarItem(placement: .primaryAction) {
                Button("Scan", systemImage: "antenna.radiowaves.left.and.right") {
                    refreshOnly = false
                    scanRequest += 1
                }
                .disabled(scanning || saving || !connected || !connectorReady)
            }
            ToolbarItem(placement: .confirmationAction) { Button("Done") { dismiss() }.disabled(saving) }
        }
        .task(id: "\(selected)-\(configRequest)") { await loadConfig() }
        .task(id: scanRequest) {
            guard scanRequest > 0 else { return }
            await scan()
        }
    }

    @State private var refreshOnly = false

    private func loadConfig() async {
        error = nil
        saved = false
        configLoaded = false
        loadingConfig = isNetwork
        guard isNetwork else { return }
        let tool = selected
        defer { if !Task.isCancelled { loadingConfig = false } }
        do {
            let config = try await session.networkDeviceConfig(atem: tool == .atem)
            try Task.checkCancellation()
            host = config.host
            port = String(config.port)
            enabled = config.enabled
            configLoaded = true
        } catch is CancellationError {
        } catch {
            if !Task.isCancelled { self.error = "Couldn't load the connection. Choose Retry Loading Connection." }
        }
    }

    private func scan() async {
        let tool = selected
        scanning = true
        error = nil
        defer { scanning = false }
        do {
            if tool == .broadlink {
                if !refreshOnly { _ = try await session.discover(tool) }
                // ponytail: the API has no completion event. Show saved devices, never claim scan completion.
                if !refreshOnly { try await Task.sleep(for: .seconds(6)) }
                results[tool] = DiscoverySnapshot(
                    devices: try await session.savedBroadlinkDevices(),
                    message: "Saved devices may include earlier scans. Discovery runs in the background; refresh to see later results."
                )
            } else {
                results[tool] = try await session.discover(tool)
            }
        } catch is CancellationError {
        } catch {
            self.error = (error as? DiscoveryFailure)?.message ?? "Couldn't scan devices. Check the server connection and try again."
        }
    }

    private func save() {
        guard let port = UInt16(port), port > 0 else { return }
        saving = true
        error = nil
        saved = false
        let atem = selected == .atem
        let config = NetworkDeviceConfig(enabled: enabled, host: host.trimmingCharacters(in: .whitespacesAndNewlines), port: port)
        Task {
            defer { saving = false }
            do {
                try await session.saveNetworkDevice(atem: atem, config: config)
                saved = true
            } catch {
                self.error = "Couldn't save the connection. Check the server and try again."
            }
        }
    }
}

private extension DiscoveryTool {
    var connector: String {
        switch self {
        case .atem: "atem"
        case .camera: "blackmagic-camera"
        case .middlecontrol: "middlecontrol"
        case .broadlink: "broadlink"
        case .obs: "obs"
        case .rodecaster: "rodecaster"
        }
    }
    var title: String { self == .camera ? "Blackmagic Cameras" : ConnectorInfo(id: connector).name }
    var icon: String {
        switch self {
        case .atem: "switch.2"
        case .camera: "video"
        case .middlecontrol: "camera"
        case .broadlink: "sensor"
        case .obs: "display"
        case .rodecaster: "waveform"
        }
    }
    var explanation: String {
        switch self {
        case .atem: "Find ATEM switchers using Bonjour on the server's network. If discovery is unavailable, enter the switcher's IP address below."
        case .camera: "Find Blackmagic cameras on the server's network. When no camera is configured, the server automatically adopts and connects the first camera found."
        case .middlecontrol: "Find Middle Control on the server's network by checking its feedback ports."
        case .broadlink: "Find Broadlink remotes on the server's network. Discovered devices are automatically saved."
        case .obs: "Refresh displays, audio inputs and outputs, cameras, and capture cards available to the connected OBS instance."
        case .rodecaster: "Inspect the connected RØDECaster's audio endpoint, sample rate, and input channel mappings on the server."
        }
    }
}
#endif
