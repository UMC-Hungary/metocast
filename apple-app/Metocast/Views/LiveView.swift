import SwiftUI
import Metocast

/// Live production: OBS, the ATEM switcher, Middle Control cameras and the RØDECaster mixer.
/// A device shows up once its connector is connected on the server.
struct LiveView: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        Group {
            if let session = model.session,
               session.hasLiveDevices || EventSchedule.current(in: session.events, now: .now) != nil {
                let outputs = session.outputs
                Form {
                    if let event = EventSchedule.current(in: session.events, now: .now) {
                        LiveOrderSection(session: session, event: event)
                    }
                    if !session.automations.isEmpty {
                        Section {
                            AutomationsArmedRow(session: session)
                        }
                    }
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
                            Button("Push YouTube Destination", systemImage: "arrow.up.forward.square") {
                                session.control(.atemPushYoutube)
                            }
                        } header: {
                            Text("Switcher")
                        } footer: {
                            Text(atem.product)
                        }
                    }
                    if session.camera != nil {
                        Section {
                            outputRows(outputs, for: "blackmagic-camera", session: session)
                            Button("Push YouTube Destination", systemImage: "arrow.up.forward.square") {
                                session.control(.cameraPushYoutube)
                            }
                        } header: {
                            Text("Camera")
                        } footer: {
                            if let status = session.camera?.streamStatus, !status.isEmpty {
                                Text("Stream: \(status)")
                            }
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
                                LabeledContent("Camera", value: cameras.selectedCamera.map { String(localized: "Camera \($0)") } ?? String(localized: "None"))
                            }
                            if let preset = presetText(cameras) {
                                LabeledContent("Preset") {
                                    HStack(spacing: 6) {
                                        if cameras.runningPreset != nil { ProgressView().controlSize(.small) }
                                        Text(preset)
                                    }
                                }
                            }
                            outputRows(outputs, for: "middlecontrol", session: session)
                        }
                    }
                    if let transcription = session.transcription, transcription.status != .notConfigured {
                        TranscriptSection(session: session, state: transcription, outputs: outputs)
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

    /// "Preset 3 moving" while Middle Control runs a preset, then "Preset 3 reached" with the time.
    private func presetText(_ cameras: MiddlecontrolState) -> String? {
        if let running = cameras.runningPreset {
            return String(localized: "Preset \(running) moving")
        }
        guard let last = cameras.lastPreset else { return nil }
        let time = Date(rfc3339: last.at).map { " · " + $0.formatted(date: .omitted, time: .shortened) } ?? ""
        return String(localized: "Preset \(last.preset) reached\(time)")
    }

    private func outputRows(_ outputs: [LiveOutput], for connector: String, session: MetocastSession) -> some View {
        ForEach(outputs.filter { $0.connector == connector }) { output in
            OutputRow(output: output, send: session.control)
        }
    }
}

extension MetocastSession {
    var hasLiveDevices: Bool {
        obs != nil || atem != nil || middlecontrol != nil || rodecaster != nil || camera != nil
            || !outputs.isEmpty
    }
}

/// The master switch for automations, where the operator sees it during a service.
private struct AutomationsArmedRow: View {
    let session: MetocastSession

    var body: some View {
        Toggle(isOn: Binding(
            get: { session.automationsArmed },
            set: { armed in Task { try? await session.setAutomationsArmed(armed) } }
        )) {
            Label {
                Text("Automations armed")
                Text(session.automationsArmed ? "Rules fire by themselves" : "Disarmed: nothing fires")
                    .foregroundStyle(session.automationsArmed ? Color.secondary : Color.orange)
            } icon: {
                Image(systemName: session.automationsArmed ? "bolt.fill" : "bolt.slash")
                    .foregroundStyle(session.automationsArmed ? AnyShapeStyle(.tint) : AnyShapeStyle(.orange))
            }
        }
    }
}

/// The live English transcript: start and stop, the newest line, and why it can't run.
private struct TranscriptSection: View {
    let session: MetocastSession
    let state: TranscriptionRecord
    let outputs: [LiveOutput]

    var body: some View {
        Section {
            ForEach(outputs.filter { $0.connector == "openai" }) { output in
                OutputRow(output: output, send: session.control)
            }
            if !session.transcriptLine.isEmpty {
                Text(session.transcriptLine)
                    .italic()
                    .foregroundStyle(.secondary)
                    .lineLimit(3)
            }
            if let error = state.youtubeError {
                Label("YouTube captions: \(error)", systemImage: "exclamationmark.triangle")
                    .foregroundStyle(.orange)
            }
            if let url = URL(string: session.baseURL + "/transcript") {
                Link("Open Transcript Page", destination: url)
            }
        } header: {
            Text("English Transcript")
        } footer: {
            VStack(alignment: .leading, spacing: 2) {
                if let problem {
                    Text(problem)
                }
                if let credit = session.transcriptCredit {
                    Text("AI credit: \(credit.balanceUsd, format: .currency(code: "USD"))")
                        .foregroundStyle(credit.low ? .red : .secondary)
                } else {
                    Text("AI credit: add an admin key and your top-ups in Settings → Connectors → OpenAI Translation.")
                }
            }
        }
    }

    private var problem: String? {
        switch (state.status, state.reason) {
        case (.unavailable, _): String(localized: "Connect the RØDECaster to transcribe.")
        case (.error, "noCredit"): String(localized: "OpenAI has no credit left.")
        case (.error, "invalidKey"): String(localized: "OpenAI rejected the API key. Check it in Settings → Connectors.")
        case (.error, let reason?): reason
        case (.connecting, let reason?): String(localized: "Retrying: \(reason)")
        default: session.transcriptRefusal
        }
    }
}

/// One row of switcher inputs. The input on the bus is filled in its tally color, like the
/// switcher's own buttons, so it reads at a glance.
private struct InputBus: View {
    let title: LocalizedStringKey
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
        // "Cam 1" alone doesn't say whether it's on Program or Preview.
        .accessibilityElement(children: .contain)
        .accessibilityLabel(title)
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
        // The switch is "on" when the channel is live, which "on/off" alone doesn't say.
        .accessibilityValue(channel.mute ? "Muted" : "Live")
    }
}

/// A stream or recording with its state and a start or stop button.
private struct OutputRow: View {
    let output: LiveOutput
    let send: (ProductionControl) -> Void

    var body: some View {
        HStack {
            Label {
                Text(output.kindTitle)
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
            } else if output.control != nil {
                // The row names the output, so the stop button only needs the verb.
                OutputButton(output: output, send: send) {
                    Text(output.isOn ? (output.kind == .stream ? String(localized: "End") : String(localized: "Stop")) : output.startTitle)
                }
                .buttonStyle(.bordered)
                .tint(output.isOn ? .red : .accentColor)
            }
        }
        // Going live or stopping happens out of view (a rule, another device), so say it.
        .onChange(of: output.state) { _, _ in
            AccessibilityNotification.Announcement("\(output.name): \(output.stateText)").post()
        }
    }
}

/// Starts or stops one output. Stopping asks first: a stream or recording ended by accident
/// mid-service can't be picked up again. The caller supplies the label and the style, and
/// the button is disabled while the output can't start. With `askToStop`, the caller presents
/// the question; without it, the button does.
struct OutputButton<Content: View>: View {
    let output: LiveOutput
    let send: (ProductionControl) -> Void
    var askToStop: ((LiveOutput) -> Void)?
    @ViewBuilder let label: Content
    @State private var isConfirmingStop = false

    var body: some View {
        Button {
            guard let control = output.control else { return }
            if !output.isOn {
                send(control(true))
            } else if let askToStop {
                askToStop(output)
            } else {
                isConfirmingStop = true
            }
        } label: {
            label
        }
        .disabled(output.control == nil || output.isBusy)
        // The label may be a bare verb or an icon; VoiceOver always hears what it acts on.
        .accessibilityLabel(output.actionName)
        .stopConfirmation(for: output, isPresented: $isConfirmingStop, send: send)
    }
}

extension View {
    /// Asks before stopping `output`, and stops it on a yes.
    func stopConfirmation(for output: LiveOutput?, isPresented: Binding<Bool>, send: @escaping (ProductionControl) -> Void) -> some View {
        confirmationDialog(output.map { Text("\($0.stopTitle)?") } ?? Text(verbatim: ""), isPresented: isPresented, presenting: output) { output in
            Button(output.stopTitle, role: .destructive) {
                if let control = output.control { send(control(false)) }
            }
        } message: { output in
            Text("This stops the \(output.name) for everyone.")
        }
    }
}

#if os(macOS)
/// Record and Transcript in the Mac toolbar, so they are within reach from every page, whether
/// or not an event exists. The toolbar gives them its glass; a running output's symbol fills in red.
/// A custom button style would turn the item into a plain view that the toolbar can't label. One that
/// can't start is dimmed, and both of these only wait for the RØDECaster.
///
/// The stop question is not presented from here: SwiftUI rebuilds toolbar items on every session
/// update, and a dialog attached to one comes back after each answer. The window asks instead,
/// for the output named in `stopping`.
struct OutputToolbarButtons: View {
    let session: MetocastSession
    @Binding var stopping: LiveOutput.ID?

    var body: some View {
        ForEach(session.outputs.filter(\.showsInToolbar)) { output in
            OutputButton(output: output, send: session.control, askToStop: { stopping = $0.id }) {
                // A short title that doesn't change with the state: in Icon and Text mode the toolbar
                // prints it under the icon. The action is in the tooltip and VoiceOver's label.
                Label {
                    Text(output.kind == .transcript ? String(localized: "Transcript") : output.startTitle)
                } icon: {
                    if output.isBusy {
                        ProgressView().controlSize(.small)
                    } else {
                        Image(systemName: output.symbol)
                            .symbolVariant(output.isOn ? .fill : .none)
                            .foregroundStyle(output.isOn ? AnyShapeStyle(.red) : AnyShapeStyle(.primary))
                    }
                }
            }
            .help(output.control == nil ? String(localized: "The RØDECaster isn't connected.") : output.actionName)
        }
    }
}
#endif
