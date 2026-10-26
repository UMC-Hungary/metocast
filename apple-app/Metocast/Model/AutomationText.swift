import Foundation
import Metocast

// Automations in words, and the editor's working copy of a rule.

extension AutomationRecord: @retroactive Identifiable {}
extension AutomationRunRecord: @retroactive Identifiable {}

private func seconds(_ value: Float) -> String {
    String(localized: "\(value.formatted(.number.precision(.fractionLength(0...1)))) s", comment: "A duration in seconds")
}

extension RecordTarget {
    /// "OBS", "ATEM", "Camera": what a summary calls it.
    var shortName: String {
        switch self {
        case .obs: "OBS"
        case .atem: "ATEM"
        case .blackmagicCamera: String(localized: "Camera")
        case .middlecontrol(let cameraId): cameraId.map { String(localized: "Middle Control cam \($0)") } ?? "Middle Control"
        case .rodecaster: "RØDECaster SD"
        case .rodecasterAudio: String(localized: "Metocast audio")
        }
    }
}

extension AutomationTrigger {
    /// "Preset 3", "Wireless 1 live", "Pastor silent for 5 s".
    var shortText: String {
        switch self {
        case .middlecontrolPreset(let preset, let cameraId):
            cameraId.map { String(localized: "Preset \(preset) (cam \($0))") } ?? String(localized: "Preset \(preset)")
        case .rodecasterChannel(let channel, let label, let live, _, let hold):
            shortText(
                name: label.trimmed.isEmpty ? String(localized: "Channel \(channel + 1)") : label.trimmed,
                live: live,
                hold: hold
            )
        }
    }

    private func shortText(name: String, live: Bool, hold: Float) -> String {
        let state = live ? String(localized: "\(name) live") : String(localized: "\(name) silent")
        return hold > 0 ? String(localized: "\(state) for \(seconds(hold))", comment: "A channel's state, then how long it has to last") : state
    }
}

extension AutomationRecord {
    /// One line: "Preset 3 or Wireless 1 live → camera to preset 3, then start OBS, ATEM, Camera".
    var summary: String {
        let when = triggers.map(\.shortText).joined(separator: String(localized: " or "))
        var parts: [String] = []
        var group: (verb: String, names: [String])?
        func flush() {
            if let group {
                let names = group.names.joined(separator: ", ")
                parts.append(group.verb == "start" ? String(localized: "start \(names)") : String(localized: "stop \(names)"))
            }
            group = nil
        }
        for action in actions {
            switch action {
            case .recordStart(let target), .recordStop(let target):
                let verb: String
                if case .recordStart = action { verb = "start" } else { verb = "stop" }
                if group?.verb == verb {
                    group?.names.append(target.shortName)
                } else {
                    flush()
                    group = (verb, [target.shortName])
                }
            case .presetRecall(let preset, let cameraId, _):
                flush()
                parts.append(cameraId.map { String(localized: "camera \($0) to preset \(preset)") } ?? String(localized: "camera to preset \(preset)"))
            }
        }
        flush()
        let what = parts.isEmpty ? String(localized: "does nothing") : parts.joined(separator: String(localized: ", then "))
        return when.isEmpty ? what : "\(when) → \(what)"
    }
}

extension AutomationRunRecord {
    var date: Date? { Date(rfc3339: startedAt) }
    var failures: Int { results.filter { $0.outcome == .failed }.count }
    var didFail: Bool { failures > 0 }

    /// "All done", "1 failed", "Nothing to do".
    var outcomeText: String {
        if failures > 0 { return String(localized: "\(failures) failed") }
        let done = results.filter { $0.outcome == .done }.count
        if done > 0 { return results.count == done ? String(localized: "All done") : String(localized: "\(done) done") }
        return String(localized: "Nothing to do")
    }
}

extension ActionResult {
    var symbol: String {
        switch outcome {
        case .done: "checkmark.circle.fill"
        case .skipped: "minus.circle"
        case .failed: "exclamationmark.triangle.fill"
        }
    }

    /// The server's own code or note, in words.
    var messageText: String? {
        message.map(ServerCode.text)
    }
}

// ── The editor's working copy ────────────────────────────────────────────────────────────

/// A trigger as its controls hold it: every field at once, so switching a control never loses
/// what the others say.
struct TriggerDraft: Identifiable {
    enum Kind { case preset, channel }

    let id = UUID()
    var kind: Kind
    var preset = 3
    /// 0 is any camera.
    var cameraId = 0
    var channel = 0
    var label = ""
    var live = true
    var holdSeconds = 0
    var source = ChannelSource.either

    init(kind: Kind) { self.kind = kind }

    init(_ trigger: AutomationTrigger) {
        switch trigger {
        case .middlecontrolPreset(let preset, let cameraId):
            kind = .preset
            self.preset = Int(preset)
            self.cameraId = Int(cameraId ?? 0)
        case .rodecasterChannel(let channel, let label, let live, let source, let hold):
            kind = .channel
            self.channel = Int(channel)
            self.label = label
            self.live = live
            self.source = source
            holdSeconds = Int(hold.rounded())
        }
    }

    var trigger: AutomationTrigger {
        switch kind {
        case .preset:
            .middlecontrolPreset(preset: UInt8(clamping: preset), cameraId: cameraId == 0 ? nil : UInt8(clamping: cameraId))
        case .channel:
            .rodecasterChannel(channel: UInt32(clamping: channel), label: label, live: live, source: source, holdSeconds: Float(holdSeconds))
        }
    }
}

/// A recorder a step can start or stop. The Metocast audio recording is the FLAC capture of the
/// desk's USB audio; the SD recorder is the desk's own.
enum RecorderChoice: CaseIterable, Hashable {
    case obs, atem, camera, middlecontrol, deskSD, audio

    var title: String {
        switch self {
        case .obs: "OBS"
        case .atem: "ATEM"
        case .camera: String(localized: "Blackmagic camera")
        case .middlecontrol: String(localized: "Middle Control cameras")
        case .deskSD: String(localized: "RØDECaster SD recorder")
        case .audio: String(localized: "Metocast audio recording")
        }
    }

    /// The connector that has to be up for it to work.
    var connector: String {
        switch self {
        case .obs: "obs"
        case .atem: "atem"
        case .camera: "blackmagic-camera"
        case .middlecontrol: "middlecontrol"
        case .deskSD, .audio: "rodecaster"
        }
    }
}

/// One step of what a rule does: a camera move, or a set of recorders started or stopped
/// together. Recorder actions next to each other in a rule run together, so a rule's actions read
/// as steps this way.
struct StepDraft: Identifiable {
    enum Kind { case camera, start, stop }

    let id = UUID()
    var kind: Kind
    var preset = 3
    /// 0 is the selected camera.
    var cameraId = 0
    var waitForArrival = true
    var recorders: Set<RecorderChoice> = []
    /// Which camera Middle Control records; 0 is every camera.
    var middlecontrolCameraId = 0

    init(kind: Kind) { self.kind = kind }

    var actions: [AutomationAction] {
        switch kind {
        case .camera:
            return [.presetRecall(preset: UInt8(clamping: preset), cameraId: cameraId == 0 ? nil : UInt8(clamping: cameraId), waitForArrival: waitForArrival)]
        case .start, .stop:
            return RecorderChoice.allCases.filter(recorders.contains).map { recorder in
                let target: RecordTarget = switch recorder {
                case .obs: .obs
                case .atem: .atem
                case .camera: .blackmagicCamera
                case .middlecontrol: .middlecontrol(cameraId: middlecontrolCameraId == 0 ? nil : UInt8(clamping: middlecontrolCameraId))
                case .deskSD: .rodecaster
                case .audio: .rodecasterAudio
                }
                return kind == .start ? .recordStart(target: target) : .recordStop(target: target)
            }
        }
    }

    /// The steps a rule's actions make: each camera move on its own, and recorder actions of
    /// the same kind next to each other together.
    static func steps(_ actions: [AutomationAction]) -> [StepDraft] {
        var steps: [StepDraft] = []
        for action in actions {
            switch action {
            case .presetRecall(let preset, let cameraId, let wait):
                var step = StepDraft(kind: .camera)
                step.preset = Int(preset)
                step.cameraId = Int(cameraId ?? 0)
                step.waitForArrival = wait
                steps.append(step)
            case .recordStart(let target), .recordStop(let target):
                let kind: Kind = if case .recordStart = action { .start } else { .stop }
                if steps.last?.kind != kind { steps.append(StepDraft(kind: kind)) }
                let recorder: RecorderChoice
                switch target {
                case .obs: recorder = .obs
                case .atem: recorder = .atem
                case .blackmagicCamera: recorder = .camera
                case .middlecontrol(let cameraId):
                    recorder = .middlecontrol
                    steps[steps.count - 1].middlecontrolCameraId = Int(cameraId ?? 0)
                case .rodecaster: recorder = .deskSD
                case .rodecasterAudio: recorder = .audio
                }
                steps[steps.count - 1].recorders.insert(recorder)
            }
        }
        return steps
    }
}
