import SwiftUI
import Metocast

/// Writes one rule: when it fires, what it does, and how often. A rule fires when any of its
/// triggers happens and does its steps in order.
struct AutomationEditorView: View {
    @Environment(\.dismiss) private var dismiss
    let session: MetocastSession
    /// The saved rule being changed; nil for a new one.
    let original: AutomationRecord?
    var onSaved: () -> Void = {}

    @State private var id = ""
    @State private var name = ""
    @State private var enabled = true
    @State private var triggers: [TriggerDraft] = []
    @State private var steps: [StepDraft] = []
    @State private var cooldown = 5
    @State private var requireEvent = false
    @State private var didLoad = false
    @State private var failure: String?
    @State private var isSaving = false
    @State private var isConfirmingRun = false
    @State private var isRunning = false
    @State private var finishedRun: AutomationRunRecord?
    @State private var isAskingReverse = false

    /// The rule as the controls say it now.
    private var draft: AutomationRecord {
        AutomationRecord(
            id: id,
            name: name.trimmed,
            enabled: enabled,
            triggers: triggers.map(\.trigger),
            actions: steps.flatMap(\.actions),
            cooldownSeconds: Float(cooldown),
            requireEvent: requireEvent
        )
    }

    /// What the server would refuse it for, in its own words.
    private var problem: String? { automationProblem(automation: draft) }

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    LabeledContent("Name") {
                        TextField("Name", text: $name, prompt: Text("Service start"))
                            .multilineTextAlignment(.trailing)
                            .labelsHidden()
                    }
                    Toggle("Enabled", isOn: $enabled)
                }
                whenSection
                doSection
                optionsSection
                if !triggers.isEmpty || !steps.isEmpty {
                    reverseSection
                }
                if !id.isEmpty {
                    runSection
                }
                if let failure {
                    Section {
                        Label(failure, systemImage: "exclamationmark.triangle").foregroundStyle(.red)
                    }
                } else if let problem {
                    Section {
                        Label("To save: \(problem)", systemImage: "info.circle").foregroundStyle(.secondary)
                    }
                }
            }
            .formStyle(.grouped)
            .navigationTitle(id.isEmpty ? "New Automation" : "Automation")
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Save") { Task { await save() } }
                        .disabled(problem != nil || isSaving)
                }
            }
            .onAppear(perform: load)
            .confirmationDialog("Run “\(name.trimmed)” now?", isPresented: $isConfirmingRun, titleVisibility: .visible) {
                Button("Run Now", role: .destructive) { Task { await runNow() } }
            } message: {
                Text("This really records and moves cameras: \(original?.summary ?? draft.summary)")
            }
            .sheet(isPresented: $isAskingReverse) {
                ReverseRuleSheet(hasCameraMove: steps.contains { $0.kind == .camera }) { preset in
                    loadReversed(preset)
                }
            }
            .sheet(item: $finishedRun) { run in
                AutomationRunDetail(run: run)
            }
        }
        #if os(macOS)
        .frame(minWidth: 560, minHeight: 640)
        #endif
    }

    // ── When ─────────────────────────────────────────────────────────────────────────────

    private var whenSection: some View {
        Section {
            ForEach($triggers) { $trigger in
                TriggerEditor(session: session, trigger: $trigger) {
                    triggers.removeAll { $0.id == trigger.id }
                }
            }
            Menu("Add Trigger", systemImage: "plus") {
                Button("Middle Control preset finished", systemImage: "slider.horizontal.3") {
                    triggers.append(TriggerDraft(kind: .preset))
                }
                Button("RØDECaster channel goes live or silent", systemImage: "mic") {
                    triggers.append(TriggerDraft(kind: .channel))
                }
            }
        } header: {
            Text("When")
        } footer: {
            VStack(alignment: .leading, spacing: 4) {
                Text("The rule fires when any of these happens.")
                if triggers.contains(where: { $0.kind == .channel && !$0.live }) {
                    Text("A “goes silent” trigger fires at every long mute, such as hymns or prayers. It fits a channel that is live only for the part being recorded, such as the sermon. To record a whole service, stop on a preset or wait longer.")
                }
            }
        }
    }

    // ── Do ───────────────────────────────────────────────────────────────────────────────

    private var doSection: some View {
        Section {
            ForEach($steps) { $step in
                StepEditor(
                    session: session,
                    step: $step,
                    isFirst: steps.first?.id == step.id,
                    isLast: steps.last?.id == step.id,
                    move: { offset in move(step.id, by: offset) },
                    remove: { steps.removeAll { $0.id == step.id } }
                )
            }
            .onMove { steps.move(fromOffsets: $0, toOffset: $1) }
            Menu("Add Step", systemImage: "plus") {
                Button("Move camera to preset", systemImage: "video") {
                    steps.append(StepDraft(kind: .camera))
                }
                Button("Start recording", systemImage: "record.circle") {
                    var step = StepDraft(kind: .start)
                    step.recorders = [.obs]
                    steps.append(step)
                }
                Button("Stop recording", systemImage: "stop.circle") {
                    var step = StepDraft(kind: .stop)
                    step.recorders = [.obs]
                    steps.append(step)
                }
            }
        } header: {
            Text("Do")
        } footer: {
            Text("Steps run in this order, and the recorders in one step start together. A start rule moves the camera first, so recording starts on the right shot; a stop rule stops recording first, so the recording doesn't end on a camera move.")
        }
    }

    private func move(_ id: UUID, by offset: Int) {
        guard let index = steps.firstIndex(where: { $0.id == id }) else { return }
        let target = index + offset
        guard steps.indices.contains(target) else { return }
        steps.swapAt(index, target)
    }

    // ── Options ──────────────────────────────────────────────────────────────────────────

    private var optionsSection: some View {
        Section {
            valueStepper("Cooldown", value: "\(cooldown) s", number: $cooldown, in: 1...600)
            Toggle("Only when today has an event", isOn: $requireEvent)
        } header: {
            Text("Options")
        } footer: {
            Text("The rule won't fire again until the cooldown has passed.")
        }
    }

    private var reverseSection: some View {
        Section {
            Button("Create Reverse Rule…", systemImage: "arrow.uturn.backward") {
                isAskingReverse = true
            }
        } footer: {
            Text("Makes a new rule from this one with the steps the other way round, such as a service end from a service start.")
        }
    }

    private var runSection: some View {
        Section {
            Button(isRunning ? "Running…" : "Run Now…", systemImage: "play.fill") {
                isConfirmingRun = true
            }
            .disabled(isRunning || draft != original)
        } footer: {
            Text(draft != original
                ? "Save your changes to run them."
                : "Runs the steps right now, whatever the cooldown and the armed switch say, to check a setup.")
        }
    }

    // ── Loading and saving ───────────────────────────────────────────────────────────────

    private func load() {
        guard !didLoad else { return }
        didLoad = true
        guard let original else { return }
        fill(from: original)
    }

    private func fill(from rule: AutomationRecord) {
        id = rule.id
        name = rule.name
        enabled = rule.enabled
        triggers = rule.triggers.map(TriggerDraft.init)
        steps = StepDraft.steps(rule.actions)
        cooldown = Int(rule.cooldownSeconds.rounded())
        requireEvent = rule.requireEvent
    }

    /// Replaces what is being written with the opposite rule, as a new one.
    private func loadReversed(_ cameraPreset: Int?) {
        let reversed = reverseAutomation(automation: draft, cameraPreset: cameraPreset.map { UInt8(clamping: $0) })
        fill(from: reversed)
        failure = nil
    }

    private func save() async {
        isSaving = true
        defer { isSaving = false }
        do {
            try await session.saveAutomation(draft)
            onSaved()
            dismiss()
        } catch {
            failure = error.metocastMessage
        }
    }

    private func runNow() async {
        isRunning = true
        defer { isRunning = false }
        do {
            finishedRun = try await session.runAutomation(id: id)
            failure = nil
        } catch {
            failure = error.metocastMessage
        }
    }
}

// ── One trigger ──────────────────────────────────────────────────────────────────────────

private struct TriggerEditor: View {
    let session: MetocastSession
    @Binding var trigger: TriggerDraft
    var remove: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack {
                Label(
                    trigger.kind == .preset ? "Middle Control preset" : "RØDECaster channel",
                    systemImage: trigger.kind == .preset ? "slider.horizontal.3" : "mic"
                )
                .font(.headline)
                .accessibilityAddTraits(.isHeader)
                Spacer()
                Button("Remove \(trigger.kind == .preset ? "Preset" : "Channel") Trigger", systemImage: "trash", role: .destructive, action: remove)
                    .labelStyle(.iconOnly)
                    .buttonStyle(.borderless)
            }
            switch trigger.kind {
            case .preset: presetFields
            case .channel: channelFields
            }
        }
        .padding(.vertical, 4)
    }

    @ViewBuilder
    private var presetFields: some View {
        Picker("Preset", selection: $trigger.preset) {
            ForEach(1...12, id: \.self) { Text("Preset \($0)").tag($0) }
        }
        valueStepper("Camera", value: trigger.cameraId == 0 ? "Any" : "Camera \(trigger.cameraId)", number: $trigger.cameraId, in: 0...99)
        Text("Preset \(trigger.preset) has finished moving" + (trigger.cameraId == 0 ? "" : " on camera \(trigger.cameraId)"))
            .font(.subheadline)
            .foregroundStyle(.secondary)
        // Detection works when Middle Control's own feedback shows moves, so this is how to check
        // it before Sunday.
        if let last = session.middlecontrol?.lastPreset {
            Text("Last seen: preset \(last.preset) on camera \(last.cameraId)" + (Date(rfc3339: last.at).map { ", " + $0.formatted(date: .omitted, time: .shortened) } ?? ""))
                .font(.caption)
                .foregroundStyle(.secondary)
        } else {
            Text("No preset move seen yet. Click a preset in Middle Control to check that it is noticed.")
                .font(.caption)
                .foregroundStyle(.secondary)
        }
    }

    @ViewBuilder
    private var channelFields: some View {
        if let channels = session.rodecaster?.channels, !channels.isEmpty {
            Picker("Channel", selection: Binding(
                get: { trigger.channel },
                set: { number in
                    trigger.channel = number
                    if let channel = channels.first(where: { Int($0.channel) == number }) {
                        trigger.label = channel.label
                    }
                }
            )) {
                ForEach(channels, id: \.channel) { Text($0.label).tag(Int($0.channel)) }
                // A saved channel the desk doesn't list now still has to show.
                if !channels.contains(where: { Int($0.channel) == trigger.channel }) {
                    Text(savedChannelName).tag(trigger.channel)
                }
            }
        } else {
            // The desk is offline, so there are no names to pick from.
            valueStepper("Fader", value: savedChannelName, number: $trigger.channel, in: 0...63)
        }
        Picker("When", selection: $trigger.live) {
            Text("Goes live").tag(true)
            Text("Goes silent").tag(false)
        }
        .pickerStyle(.segmented)
        valueStepper("For at least", value: trigger.holdSeconds == 0 ? "No wait" : "\(trigger.holdSeconds) s", number: $trigger.holdSeconds, in: 0...60)
        DisclosureGroup("Advanced") {
            Picker("Counts as live", selection: $trigger.source) {
                Text("Desk and transmitter").tag(ChannelSource.either)
                Text("Desk only").tag(ChannelSource.desk)
                Text("Transmitter only").tag(ChannelSource.transmitter)
            }
            Text(sourceHelp)
                .font(.caption)
                .foregroundStyle(.secondary)
        }
    }

    private var savedChannelName: String {
        trigger.label.trimmed.isEmpty ? "Fader \(trigger.channel + 1)" : "\(trigger.label) (fader \(trigger.channel + 1))"
    }

    private var sourceHelp: String {
        switch trigger.source {
        case .either: "Live only while both the desk and the Wireless PRO transmitter are unmuted, so a pastor who mutes on the transmitter goes silent."
        case .desk: "Only the desk's button (or Metocast's own mute toggle) counts."
        case .transmitter: "Only the Wireless PRO transmitter's button counts."
        }
    }
}

// ── One step ─────────────────────────────────────────────────────────────────────────────

private struct StepEditor: View {
    let session: MetocastSession
    @Binding var step: StepDraft
    let isFirst: Bool
    let isLast: Bool
    var move: (Int) -> Void
    var remove: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack {
                Label(title, systemImage: symbol).font(.headline).accessibilityAddTraits(.isHeader)
                Spacer()
                Menu("\(title) Step", systemImage: "ellipsis.circle") {
                    Button("Move Up", systemImage: "arrow.up") { move(-1) }.disabled(isFirst)
                    Button("Move Down", systemImage: "arrow.down") { move(1) }.disabled(isLast)
                    Button("Remove Step", systemImage: "trash", role: .destructive, action: remove)
                }
                .labelStyle(.iconOnly)
                .menuStyle(.borderlessButton)
                .fixedSize()
            }
            switch step.kind {
            case .camera: cameraFields
            case .start, .stop: recorderFields
            }
        }
        .padding(.vertical, 4)
    }

    private var title: String {
        switch step.kind {
        case .camera: "Move camera to preset"
        case .start: "Start recording"
        case .stop: "Stop recording"
        }
    }

    private var symbol: String {
        switch step.kind {
        case .camera: "video"
        case .start: "record.circle"
        case .stop: "stop.circle"
        }
    }

    @ViewBuilder
    private var cameraFields: some View {
        Picker("Preset", selection: $step.preset) {
            ForEach(1...12, id: \.self) { Text("Preset \($0)").tag($0) }
        }
        valueStepper("Camera", value: step.cameraId == 0 ? "Selected" : "Camera \(step.cameraId)", number: $step.cameraId, in: 0...99)
        Toggle("Wait until it arrives", isOn: $step.waitForArrival)
        Text(step.waitForArrival
            ? "The steps after this one start once the camera has reached the preset."
            : "The steps after this one start right away.")
            .font(.caption)
            .foregroundStyle(.secondary)
    }

    @ViewBuilder
    private var recorderFields: some View {
        ForEach(RecorderChoice.allCases, id: \.self) { recorder in
            VStack(alignment: .leading, spacing: 2) {
                Toggle(recorder.title, isOn: Binding(
                    get: { step.recorders.contains(recorder) },
                    set: { isOn in
                        if isOn { step.recorders.insert(recorder) } else { step.recorders.remove(recorder) }
                    }
                ))
                if step.recorders.contains(recorder) {
                    if recorder == .middlecontrol {
                        valueStepper("Camera", value: step.middlecontrolCameraId == 0 ? "Every camera" : "Camera \(step.middlecontrolCameraId)", number: $step.middlecontrolCameraId, in: 0...99)
                        .font(.subheadline)
                    }
                    if let warning = warning(for: recorder) {
                        Label(warning, systemImage: "exclamationmark.triangle")
                            .font(.caption)
                            .foregroundStyle(.orange)
                    }
                }
            }
        }
        if step.recorders.contains(.camera), step.recorders.contains(.middlecontrol) {
            Label("The Blackmagic camera and Middle Control may drive the same camera, so it could be started twice.", systemImage: "exclamationmark.triangle")
                .font(.caption)
                .foregroundStyle(.orange)
        }
    }

    /// A device that isn't connected right now is a warning, not a block: the rule is for later.
    private func warning(for recorder: RecorderChoice) -> String? {
        if session.connectors[recorder.connector]?.status != .connected {
            return "\(ConnectorInfo(id: recorder.connector).name) isn't connected right now."
        }
        if recorder == .deskSD, session.deskRecorder?.mediaPresent == false {
            return "The RØDECaster has no SD card or USB drive to record to."
        }
        return nil
    }
}

// ── Create Reverse Rule ──────────────────────────────────────────────────────────────────

private struct ReverseRuleSheet: View {
    @Environment(\.dismiss) private var dismiss
    let hasCameraMove: Bool
    /// The preset to move the camera to afterwards, or nil to leave the camera alone.
    var create: (Int?) -> Void
    @State private var movesCamera = true
    @State private var preset = 1

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    if hasCameraMove {
                        Toggle("Then move the camera", isOn: $movesCamera)
                        if movesCamera {
                            Picker("To preset", selection: $preset) {
                                ForEach(1...12, id: \.self) { Text("Preset \($0)").tag($0) }
                            }
                        }
                    }
                } header: {
                    Text("Reverse Rule")
                } footer: {
                    Text("Makes a new rule with the steps the other way round: what started now stops, and recording stops before the camera moves. A channel that went live now goes silent for at least 5 s. Preset triggers are left out, because the opposite of a preset is one you pick. This rule stays as it is.")
                }
            }
            .formStyle(.grouped)
            .navigationTitle("Create Reverse Rule")
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Create") {
                        create(hasCameraMove && movesCamera ? preset : nil)
                        dismiss()
                    }
                }
            }
        }
        #if os(macOS)
        .frame(minWidth: 420, minHeight: 300)
        #endif
    }
}

/// A stepper with its value written beside the title, such as "Any" for camera 0. VoiceOver reads
/// that too, instead of the bare number.
private func valueStepper<V: Strideable>(_ title: String, value: String, number: Binding<V>, in range: ClosedRange<V>) -> some View {
    Stepper(value: number, in: range) {
        LabeledContent(title, value: value)
    }
    .accessibilityValue(value)
}
