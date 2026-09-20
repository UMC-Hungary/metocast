import SwiftUI
import Metocast

/// Live production: OBS, the ATEM switcher, Middle Control cameras and the RØDECaster mixer.
/// A device shows up once its connector is connected on the server.
struct LiveView: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        Group {
            if let session = model.session, session.hasLiveDevices {
                let outputs = session.outputs
                Form {
                    if session.obs != nil {
                        Section("OBS") {
                            outputRows(outputs, for: "obs", session: session)
                        }
                    }
                    if let atem = session.atem {
                        Section {
                            InputBus(title: "Program", inputs: atem.inputs, selected: atem.program, tint: .red) {
                                session.control(.atemProgram(input: $0))
                            }
                            InputBus(title: "Preview", inputs: atem.inputs, selected: atem.preview, tint: .green) {
                                session.control(.atemPreview(input: $0))
                            }
                            HStack {
                                Button { session.control(.atemCut) } label: {
                                    Text("Cut").frame(maxWidth: .infinity)
                                }
                                Button { session.control(.atemAuto) } label: {
                                    Text("Auto").frame(maxWidth: .infinity)
                                }
                            }
                            .buttonStyle(.bordered)
                            .controlSize(.large)
                            outputRows(outputs, for: "atem", session: session)
                        } header: {
                            Text("Switcher")
                        } footer: {
                            Text(atem.product)
                        }
                    }
                    if let cameras = session.middlecontrol {
                        Section("Cameras") {
                            if let ids = cameras.connectedCameraIds, !ids.isEmpty {
                                Picker("Camera", selection: Binding(
                                    get: { cameras.selectedCamera },
                                    set: { if let id = $0 { session.control(.middlecontrolCamera(cameraId: id)) } }
                                )) {
                                    ForEach(Array(ids), id: \.self) { id in
                                        Text("Cam \(id)").tag(Optional(id))
                                    }
                                }
                                .pickerStyle(.segmented)
                            } else {
                                LabeledContent("Camera", value: cameras.selectedCamera.map { "Camera \($0)" } ?? "None")
                            }
                            outputRows(outputs, for: "middlecontrol", session: session)
                        }
                    }
                    if session.rodecaster != nil || outputs.contains(where: { $0.connector == "rodecaster" }) {
                        Section {
                            ForEach(session.rodecaster?.channels ?? [], id: \.channel) { channel in
                                MixerChannelRow(channel: channel) { mute in
                                    session.control(.rodecasterMute(channel: channel.channel, mute: mute))
                                }
                            }
                            outputRows(outputs, for: "rodecaster", session: session)
                        } header: {
                            Text("Audio")
                        } footer: {
                            if let error = session.recorder?.error {
                                Text(error)
                            } else if let model = session.rodecaster?.model {
                                Text(model)
                            }
                        }
                    }
                }
                .formStyle(.grouped)
            } else {
                ContentUnavailableView(
                    "No Live Devices",
                    systemImage: MetocastPane.live.icon,
                    description: Text("OBS, an ATEM switcher, Middle Control and a RØDECaster show up here once they're connected on the server.")
                )
            }
        }
        .navigationTitle("Live")
    }

    private func outputRows(_ outputs: [LiveOutput], for connector: String, session: MetocastSession) -> some View {
        ForEach(outputs.filter { $0.connector == connector }) { output in
            OutputRow(output: output, send: session.control)
        }
    }
}

extension MetocastSession {
    var hasLiveDevices: Bool {
        obs != nil || atem != nil || middlecontrol != nil || rodecaster != nil || !outputs.isEmpty
    }
}

/// One row of switcher inputs. The input on the bus is filled in its tally color, like the
/// switcher's own buttons, so it reads at a glance.
private struct InputBus: View {
    let title: String
    let inputs: [AtemInput]
    let selected: UInt16?
    let tint: Color
    let select: (UInt16) -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(title)
                .font(.subheadline)
                .foregroundStyle(.secondary)
            LazyVGrid(columns: [GridItem(.adaptive(minimum: 72), spacing: 8)], spacing: 8) {
                ForEach(inputs, id: \.id) { input in
                    let label = Text(input.shortName.isEmpty ? input.name : input.shortName)
                        .lineLimit(1)
                        .frame(maxWidth: .infinity)
                    Group {
                        if input.id == selected {
                            Button {} label: { label }
                                .buttonStyle(.borderedProminent)
                                .tint(tint)
                                .accessibilityAddTraits(.isSelected)
                        } else {
                            // Neutral, so the tally colors are the only ones on the bus.
                            Button { select(input.id) } label: { label }
                                .buttonStyle(.bordered)
                                .tint(.primary)
                        }
                    }
                    .help(input.name)
                }
            }
        }
    }
}

/// A mixer channel's desk mute, plus the Wireless PRO transmitter's own mute, which the desk
/// can't undo.
private struct MixerChannelRow: View {
    let channel: RodecasterChannel
    let setMute: (Bool) -> Void

    var body: some View {
        Toggle(isOn: Binding(get: { !channel.mute }, set: { setMute(!$0) })) {
            Label {
                Text(channel.label)
                if channel.wirelessMute {
                    Text("Muted on the wireless mic")
                        .foregroundStyle(.orange)
                }
            } icon: {
                Image(systemName: channel.mute || channel.wirelessMute ? "mic.slash" : "mic")
                    .foregroundStyle(channel.mute || channel.wirelessMute ? AnyShapeStyle(.orange) : AnyShapeStyle(.tint))
            }
        }
    }
}

/// A stream or recording with its state and a start or stop button. Stopping asks first:
/// a stream or recording ended by accident mid-service can't be picked up again.
private struct OutputRow: View {
    let output: LiveOutput
    let send: (ProductionControl) -> Void
    @State private var isConfirmingStop = false

    var body: some View {
        HStack {
            Label {
                Text(output.kind == .stream ? "Stream" : "Recording")
                HStack(spacing: 4) {
                    // A running timer says it's on, so it stands in for the state.
                    Group {
                        if let since = output.since {
                            Text(since, style: .timer).monospacedDigit()
                        } else {
                            Text(output.stateText)
                        }
                    }
                    .foregroundStyle(output.color)
                    .layoutPriority(1)
                    if let detail = output.detail {
                        Text("· \(detail)")
                    }
                }
                .lineLimit(1)
            } icon: {
                Image(systemName: output.symbol)
                    .foregroundStyle(output.color)
                    .symbolEffect(.pulse, isActive: output.isOn)
            }
            Spacer()
            if output.isBusy {
                ProgressView().controlSize(.small)
            } else if let control = output.control {
                // The row names the output, so the stop button only needs the verb.
                Button(output.isOn ? (output.kind == .stream ? "End" : "Stop") : output.startTitle) {
                    if output.isOn {
                        isConfirmingStop = true
                    } else {
                        send(control(true))
                    }
                }
                .buttonStyle(.bordered)
                .tint(output.isOn ? .red : .accentColor)
                .confirmationDialog("\(output.stopTitle)?", isPresented: $isConfirmingStop) {
                    Button(output.stopTitle, role: .destructive) { send(control(false)) }
                } message: {
                    Text("This stops the \(output.name) for everyone.")
                }
            }
        }
    }
}
