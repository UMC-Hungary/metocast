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
    /// Where a stream goes, shown as "Live → YouTube (…)".
    var destination: String?
    /// The outputs that run with or without an event, which the Mac toolbar keeps within reach
    /// from every page.
    var showsInToolbar = false

    /// "OBS stream", "ATEM recording".
    var name: String {
        if kind == .transcript { return String(localized: "English transcript") }
        if let title { return "\(ConnectorInfo(id: connector).name) \(title)" }
        let device = connector == "middlecontrol" ? String(localized: "Camera") : ConnectorInfo(id: connector).name
        return kind == .stream ? String(localized: "\(device) stream") : String(localized: "\(device) recording")
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
            var stream = LiveOutput(connector: "blackmagic-camera", kind: .stream, state: camera.streaming ? .on : .off) { .cameraStreaming(on: $0) }
            stream.destination = camera.destination?.name
            outputs.append(stream)
            outputs.append(cameraRecording(LiveOutput(connector: "blackmagic-camera", kind: .recording, state: camera.recording ? .on : .off) { .cameraRecording(on: $0) }))
        }
        if let recording = middlecontrol?.recording {
            // ponytail: assumes Middle Control records with the one camera Metocast knows; map
            // Middle Control's camera ids to cameras when there is a second one.
            outputs.append(cameraRecording(LiveOutput(connector: "middlecontrol", kind: .recording, state: recording ? .on : .off) { .middlecontrolRecordingAll(on: $0) }))
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
            detail: event?.title ?? String(localized: "No event"),
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

    /// The camera records to a card or SSD, so with none it can't start, and says why.
    private func cameraRecording(_ output: LiveOutput) -> LiveOutput {
        guard camera?.hasMedia == false, output.state == .off else { return output }
        var output = output
        output.control = nil
        output.detail = String(localized: "No card or SSD in the camera")
        return output
    }

    /// The RØDECaster's own recorder writes to its SD card or a USB drive, so it can't start
    /// without one.
    private func deskRecording(_ desk: RodecasterDeskRecorder) -> LiveOutput {
        var output = LiveOutput(
            connector: "rodecaster",
            kind: .recording,
            state: desk.status == .recording ? .on : .off,
            detail: desk.status == .paused ? String(localized: "Paused") : desk.mediaPresent || desk.status != .stopped ? nil : String(localized: "No SD card or USB drive"),
            title: String(localized: "SD recording")
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
            detail: event?.title ?? String(localized: "No event: saved as untracked"),
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

extension CameraDestination {
    /// "YouTube (Vasárnapi istentisztelet)", "Middle Control", or `rtmp://host/…` for anything else.
    var name: String? {
        let service: String = switch service {
        case "youtube": "YouTube"
        case "facebook": "Facebook"
        case "discord": "Discord"
        case "middlecontrol": "Middle Control"
        default: url
        }
        guard !service.isEmpty else { return nil }
        return broadcast.map { "\(service) (\($0))" } ?? service
    }
}

extension LiveOutput {
    var isBusy: Bool { state == .starting || state == .stopping }
    var isOn: Bool { state == .on }

    var stateText: String {
        switch (kind, state) {
        case (.stream, .off): String(localized: "Off air")
        case (.stream, .starting): String(localized: "Going live…")
        case (.stream, .on): String(localized: "Live")
        case (.stream, .stopping): String(localized: "Ending…")
        case (.recording, .off): String(localized: "Not recording")
        case (.recording, .starting): String(localized: "Starting…")
        case (.recording, .on): String(localized: "Recording")
        case (.recording, .stopping): String(localized: "Stopping…")
        case (.transcript, .off): String(localized: "Not running")
        case (.transcript, .starting): String(localized: "Connecting…")
        case (.transcript, .on): String(localized: "Transcribing")
        case (.transcript, .stopping): String(localized: "Stopping…")
        case (_, .failed): String(localized: "Failed")
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
        case .stream: String(localized: "Stream")
        case .recording: String(localized: "Recording")
        case .transcript: String(localized: "Transcript")
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
        String(localized: "\(isOn ? stopTitle : startTitle): \(name)", comment: "Button action, then the output it acts on")
    }

    var startTitle: String {
        switch kind {
        case .stream: String(localized: "Go Live")
        case .recording: String(localized: "Record")
        case .transcript: String(localized: "Start")
        }
    }
    var stopTitle: String {
        switch kind {
        case .stream: String(localized: "End Stream")
        case .recording: String(localized: "Stop Recording")
        case .transcript: String(localized: "Stop Transcript")
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
