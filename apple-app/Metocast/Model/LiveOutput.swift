import SwiftUI
import Metocast

/// A stream or recording on one device, as the Live tab and the Dashboard's On Air card show it.
struct LiveOutput: Identifiable {
    enum Kind { case stream, recording, transcript }
    enum State { case off, starting, on, stopping, failed }

    var id: String { "\(connector).\(kind).\(title ?? "")" }
    /// The connector it belongs to, which is also the Live section it shows in.
    let connector: String
    let kind: Kind
    let state: State
    /// Builds the command that starts (`true`) or stops it; nil when it can't be started now.
    var control: ((Bool) -> ProductionControl)?
    /// What it's for, such as the event an audio recording belongs to.
    var detail: String?
    var since: Date?
    /// Names it where one connector has two of a kind, such as the RØDECaster's own SD recorder
    /// next to Metocast's audio recording.
    var title: String?
    /// The outputs that run with or without an event, which the Mac toolbar keeps within reach
    /// from every page.
    var showsInToolbar = false

    /// "OBS stream", "ATEM recording".
    var name: String {
        if kind == .transcript { return "English transcript" }
        if let title { return "\(ConnectorInfo(id: connector).name) \(title)" }
        let device = connector == "middlecontrol" ? "Camera" : ConnectorInfo(id: connector).name
        return "\(device) \(kind == .stream ? "stream" : "recording")"
    }
}

extension MetocastSession {
    /// Every stream and recording the connected devices offer.
    var outputs: [LiveOutput] {
        var outputs: [LiveOutput] = []
        if let obs {
            outputs.append(LiveOutput(connector: "obs", kind: .stream, state: obs.streaming ? .on : .off) { .obsStreaming(on: $0) })
            outputs.append(LiveOutput(connector: "obs", kind: .recording, state: obs.recording ? .on : .off) { .obsRecording(on: $0) })
        }
        // Models without a streaming or recording engine report those as nil.
        if let streaming = atem?.streaming {
            outputs.append(LiveOutput(connector: "atem", kind: .stream, state: LiveOutput.State(streaming)) { .atemStreaming(on: $0) })
        }
        if let recording = atem?.recording {
            outputs.append(LiveOutput(connector: "atem", kind: .recording, state: LiveOutput.State(recording)) { .atemRecording(on: $0) })
        }
        if let camera {
            outputs.append(LiveOutput(connector: "blackmagic-camera", kind: .stream, state: camera.streaming ? .on : .off) { .cameraStreaming(on: $0) })
            outputs.append(LiveOutput(connector: "blackmagic-camera", kind: .recording, state: camera.recording ? .on : .off) { .cameraRecording(on: $0) })
        }
        if let recording = middlecontrol?.recording {
            outputs.append(LiveOutput(connector: "middlecontrol", kind: .recording, state: recording ? .on : .off) { .middlecontrolRecordingAll(on: $0) })
        }
        if let recorder, rodecaster != nil || recorder.status != .idle {
            outputs.append(audioRecording(recorder))
        }
        if let deskRecorder {
            outputs.append(deskRecording(deskRecorder))
        }
        if let transcription, transcription.status != .notConfigured {
            outputs.append(transcript(transcription))
        }
        return outputs
    }

    /// Like the audio recording: a running transcript names its event, an idle one offers the
    /// current event, or runs with none. It needs the RØDECaster, so it can't start while the
    /// server reports it offline.
    private func transcript(_ state: TranscriptionRecord) -> LiveOutput {
        let running = state.status == .running || state.status == .connecting
        let event = running ? events.first { $0.id == state.eventId } : upNext
        let outputState: LiveOutput.State = switch state.status {
        case .connecting: .starting
        case .running: .on
        case .error: .failed
        default: .off
        }
        var output = LiveOutput(
            connector: "openai",
            kind: .transcript,
            state: outputState,
            detail: event?.title ?? "No event",
            since: running ? state.startedAt.flatMap(Date.init(rfc3339:)) : nil,
            showsInToolbar: true
        )
        if running {
            output.control = { _ in .transcriptionStop }
        } else if state.status == .ready || state.status == .error {
            output.control = { _ in .transcriptionStart(eventId: event?.id) }
        }
        return output
    }

    /// The RØDECaster's own recorder writes to its SD card or a USB drive, so it can't start
    /// without one.
    private func deskRecording(_ desk: RodecasterDeskRecorder) -> LiveOutput {
        var output = LiveOutput(
            connector: "rodecaster",
            kind: .recording,
            state: desk.status == .recording ? .on : .off,
            detail: desk.status == .paused ? "Paused" : desk.mediaPresent || desk.status != .stopped ? nil : "No SD card or USB drive",
            title: "SD recording"
        )
        if desk.status != .stopped {
            output.control = { _ in .rodecasterDeskRecording(on: false) }
        } else if desk.mediaPresent {
            output.control = { _ in .rodecasterDeskRecording(on: true) }
        }
        return output
    }

    /// A running recording names the event it's bound to; an idle one offers the current event,
    /// the one the server records for, like Sanctum. With none, the server keeps the files as
    /// untracked recordings, to assign to an event later.
    private func audioRecording(_ recorder: RecorderStateRecord) -> LiveOutput {
        let recording = recorder.status == .recording
        let event = recording ? events.first { $0.id == recorder.eventId } : upNext
        var output = LiveOutput(
            connector: "rodecaster",
            kind: .recording,
            state: recording ? .on : recorder.status == .failed ? .failed : .off,
            detail: event?.title ?? "No event: saved as untracked",
            since: recording ? recorder.startedAt.flatMap(Date.init(rfc3339:)) : nil,
            showsInToolbar: true
        )
        if recording {
            output.control = { _ in .rodecasterRecordStop }
        } else if rodecaster != nil {
            output.control = { _ in .rodecasterRecordStart(eventId: event?.id) }
        }
        return output
    }
}

extension LiveOutput {
    var isBusy: Bool { state == .starting || state == .stopping }
    var isOn: Bool { state == .on }

    var stateText: String {
        switch (kind, state) {
        case (.stream, .off): "Off air"
        case (.stream, .starting): "Going live…"
        case (.stream, .on): "Live"
        case (.stream, .stopping): "Ending…"
        case (.recording, .off): "Not recording"
        case (.recording, .starting): "Starting…"
        case (.recording, .on): "Recording"
        case (.recording, .stopping): "Stopping…"
        case (.transcript, .off): "Not running"
        case (.transcript, .starting): "Connecting…"
        case (.transcript, .on): "Transcribing"
        case (.transcript, .stopping): "Stopping…"
        case (_, .failed): "Failed"
        }
    }

    var color: Color {
        switch state {
        case .on: .red
        case .starting, .stopping: .orange
        case .failed: .orange
        case .off: .secondary
        }
    }

    var kindTitle: String {
        if let title { return title }
        return switch kind {
        case .stream: "Stream"
        case .recording: "Recording"
        case .transcript: "Transcript"
        }
    }
    var symbol: String {
        switch kind {
        case .stream: "dot.radiowaves.left.and.right"
        case .recording: "record.circle"
        case .transcript: "captions.bubble"
        }
    }
    /// "Record: RØDECaster recording" or "Stop Recording: RØDECaster recording".
    var actionName: String {
        "\(isOn ? stopTitle : startTitle): \(name)"
    }

    var startTitle: String {
        switch kind {
        case .stream: "Go Live"
        case .recording: "Record"
        case .transcript: "Start"
        }
    }
    var stopTitle: String {
        switch kind {
        case .stream: "End Stream"
        case .recording: "Stop Recording"
        case .transcript: "Stop Transcript"
        }
    }
}

extension LiveOutput.State {
    init(_ status: StreamStatus) {
        self = switch status {
        case .idle: .off
        case .connecting: .starting
        case .streaming: .on
        case .stopping: .stopping
        }
    }

    init(_ status: RecordStatus) {
        self = switch status {
        case .idle: .off
        case .recording: .on
        case .stopping: .stopping
        }
    }
}
