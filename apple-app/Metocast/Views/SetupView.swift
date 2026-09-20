import SwiftUI

/// First run: the Mac chooses between hosting the server and connecting to one; the iPhone
/// only connects. A `metocast://connect` link fills in and submits the client fields.
struct SetupView: View {
    @Environment(AppModel.self) private var model
    @State private var address = ""
    @State private var token = ""
    @State private var isConnecting = false
    @State private var error: String?
    #if os(macOS)
    @State private var role = AppMode.server
    #endif

    var body: some View {
        form
            .onChange(of: model.pendingLink, initial: true) { _, link in
                guard let link else { return }
                model.pendingLink = nil
                address = link.url
                token = link.token
                #if os(macOS)
                role = .client
                #endif
                connect()
            }
    }

    #if os(macOS)
    private var form: some View {
        Form {
            Section {
                Picker("Use this Mac as", selection: $role) {
                    Text("Server").tag(AppMode.server)
                    Text("Client").tag(AppMode.client)
                }
                .pickerStyle(.segmented)
            } footer: {
                Text(role == .server
                    ? "This Mac runs Metocast: the database, connectors and schedules. Other devices connect to it."
                    : "This Mac controls a Metocast server running on another computer.")
            }
            if role == .client {
                clientFields
            }
        }
        .formStyle(.grouped)
        .frame(maxWidth: 520)
        .navigationTitle("Set Up Metocast")
        .toolbar {
            ToolbarItem(placement: .confirmationAction) {
                if isConnecting {
                    ProgressView().controlSize(.small)
                } else if role == .server {
                    Button("Start Server") { Task { await model.startServer() } }
                        .keyboardShortcut(.defaultAction)
                } else {
                    Button("Connect", action: connect)
                        .keyboardShortcut(.defaultAction)
                        .disabled(address.isEmpty || token.isEmpty)
                }
            }
        }
    }
    #else
    private var form: some View {
        NavigationStack {
            Form {
                clientFields
            }
            .navigationTitle("Connect to Metocast")
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    if isConnecting {
                        ProgressView()
                    } else {
                        Button("Connect", systemImage: "checkmark", role: .confirm, action: connect)
                            .disabled(address.isEmpty || token.isEmpty)
                    }
                }
            }
        }
    }
    #endif

    private var clientFields: some View {
        Section {
            TextField("Server", text: $address, prompt: Text("192.168.1.10:3737"))
                #if os(iOS)
                .keyboardType(.URL)
                .textInputAutocapitalization(.never)
                #endif
                .autocorrectionDisabled()
                .textContentType(.URL)
            SecureField("Token", text: $token)
                // Not a password: keeps iOS from offering to save it to Passwords.
                .textContentType(.oneTimeCode)
        } footer: {
            if let error {
                Text(error).foregroundStyle(.red)
            } else {
                #if os(iOS)
                Text("Find the address and token in Settings on the Mac running Metocast, or scan its connection code with the Camera app.")
                #else
                Text("Find the address and token in Settings on the Mac running the Metocast server.")
                #endif
            }
        }
    }

    private func connect() {
        guard !isConnecting else { return }
        isConnecting = true
        error = nil
        Task {
            error = await model.useServer(ConnectLink(url: address, token: token))
            isConnecting = false
        }
    }
}
