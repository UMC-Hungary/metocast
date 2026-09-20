import SwiftUI
import Metocast

/// A stream or recording on one device, as the Live tab and the Dashboard's On Air card show it.
struct LiveOutput: Identifiable {
    enum Kind { case stream, recording }
    enum State { case off, starting, on, stopping, failed }

    var id: String { "\(connector).\(kind)" }
    /// The connector it belongs to, which is also the Live section it shows in.
    let connector: String
    let kind: Kind
    let state: State
    /// Builds the command that starts (`true`) or stops it; nil when it can't be started now.
    var control: ((Bool) -> ProductionControl)?
    /// What it's for, such as the event an audio recording belongs to.
    var detail: String?
    var since: Date?

    /// "OBS stream", "ATEM recording".
    var name: String {
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
        if let recording = middlecontrol?.recording {
            outputs.append(LiveOutput(connector: "middlecontrol", kind: .recording, state: recording ? .on : .off) { .middlecontrolRecordingAll(on: $0) })
        }
        if let recorder, rodecaster != nil || recorder.status != .idle {
            outputs.append(audioRecording(recorder))
        }
        return outputs
    }

    /// A running recording names the event it's bound to; an idle one offers the current event,
    /// the one the server records for, like Sanctum.
    private func audioRecording(_ recorder: RecorderStateRecord) -> LiveOutput {
        let recording = recorder.status == .recording
        let event = recording ? events.first { $0.id == recorder.eventId } : upNext
        var output = LiveOutput(
            connector: "rodecaster",
            kind: .recording,
            state: recording ? .on : recorder.status == .failed ? .failed : .off,
            detail: event?.title ?? (recording ? nil : "No event to record for"),
            since: recording ? recorder.startedAt.flatMap(Date.init(rfc3339:)) : nil
        )
        if recording {
            output.control = { _ in .rodecasterRecordStop }
        } else if let event, rodecaster != nil {
            output.control = { _ in .rodecasterRecordStart(eventId: event.id) }
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

    var symbol: String { kind == .stream ? "dot.radiowaves.left.and.right" : "record.circle" }
    var startTitle: String { kind == .stream ? "Go Live" : "Record" }
    var stopTitle: String { kind == .stream ? "End Stream" : "Stop Recording" }
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
