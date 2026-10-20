#if os(macOS)
import AppKit
import Metocast
import SwiftUI
import UniformTypeIdentifiers

extension MetocastSession {
    /// Every media job this session knows, by file name: renders, audio masters and programs.
    var mediaJobs: [MediaJob] {
        (renders.values.map(MediaJob.render)
            + audioJobs.values.map(MediaJob.audio)
            + programJobs.values.map(MediaJob.program))
            .sorted { $0.outputPath < $1.outputPath }
    }

    var runningMediaJobs: [MediaJob] { mediaJobs.filter { $0.state == .running } }
}

/// The jobs that have ended, newest first, with what they made. Running ones show in `RunningJobs`.
struct MediaJobsCard: View {
    let session: MetocastSession

    var body: some View {
        let ended = session.mediaJobsEnded
        let jobs = session.mediaJobs.filter { $0.state != .running }
            .sorted { (ended[$0.id] ?? .distantPast) > (ended[$1.id] ?? .distantPast) }
        if !jobs.isEmpty {
            VStack(alignment: .leading, spacing: 12) {
                CardTitle("Finished")
                ForEach(jobs, id: \.id) { job in
                    MediaJobRow(job: job, session: session)
                    if job.id != jobs.last?.id { Divider() }
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .card()
        }
    }
}

/// The jobs still running, with their progress and Cancel: in the Start card in place of Produce,
/// and in the window header's popover.
struct RunningJobs: View {
    let session: MetocastSession

    var body: some View {
        let jobs = session.runningMediaJobs
        ForEach(jobs, id: \.id) { job in
            MediaJobRow(job: job, session: session)
            if job.id != jobs.last?.id { Divider() }
        }
    }
}

/// The window header's progress ring, shown while a media job runs on any page.
struct MediaJobsButton: View {
    let session: MetocastSession
    @State private var isShowing = false

    var body: some View {
        Button {
            isShowing.toggle()
        } label: {
            if let fraction = session.runningMediaJobs.first?.fraction {
                ProgressView(value: fraction).progressViewStyle(.circular)
            } else {
                ProgressView().controlSize(.small)
            }
        }
        .help("Media jobs")
        .accessibilityLabel("Media jobs")
        .accessibilityValue(session.runningMediaJobs.first?.fraction.map { $0.formatted(.percent.precision(.fractionLength(0))) } ?? "Running")
        .popover(isPresented: $isShowing, arrowEdge: .bottom) {
            VStack(alignment: .leading, spacing: 12) { RunningJobs(session: session) }
                .padding(14)
                .frame(width: 340)
        }
    }
}

enum MediaJob {
    case render(RenderProgressRecord)
    case audio(AudioProgressRecord)
    case program(ProgramProgressRecord)

    var id: String {
        switch self {
        case .render(let job): job.jobId
        case .audio(let job): job.jobId
        case .program(let job): job.jobId
        }
    }

    var outputPath: String {
        switch self {
        case .render(let job): job.outputPath
        case .audio(let job): job.outputPath
        case .program(let job): job.outputPath
        }
    }

    var state: RenderState {
        switch self {
        case .render(let job): job.state
        case .audio(let job): job.state
        case .program(let job): job.state
        }
    }

    var error: String? {
        switch self {
        case .render(let job): job.error
        case .audio(let job): job.error
        case .program(let job): job.error
        }
    }

    /// 0–1 through the job (or its current step); `nil` while it can't tell.
    var fraction: Double? {
        switch self {
        case .render(let job): job.totalFrames > 0 && job.frame > 0 ? Double(job.frame) / Double(job.totalFrames) : nil
        case .audio(let job): job.fraction
        case .program(let job): job.fraction
        }
    }

    /// What a running job is doing.
    var step: String {
        switch self {
        case .render(let job):
            guard job.totalFrames > 0, job.frame > 0, job.fps > 0 else { return "Waiting to start" }
            let left = Double(job.totalFrames - min(job.frame, job.totalFrames)) / job.fps
            return "\(job.fps.formatted(.number.precision(.fractionLength(1)))) fps · "
                + Duration.seconds(left).formatted(.units(allowed: [.hours, .minutes], width: .abbreviated)) + " left"
        case .audio(let job):
            return switch job.step {
            case .download: "Downloading jive-vocals"
            case .prepare: "1 of 4 · Joining and mixing"
            case .clean: "2 of 4 · Cleaning with jive-vocals"
            case .align: "3 of 4 · Lining up with the source"
            case .finish: "4 of 4 · Setting the loudness"
            case nil: "Waiting to start"
            }
        case .program(let job):
            return switch job.step {
            case .download: "Downloading jive-vocals"
            case .picture: "Grading and encoding the picture"
            case .prepare: "Joining the audio"
            case .sync: "Finding where the video starts in the audio"
            case .clean: "Cleaning the voice with jive-vocals"
            case .merge: job.report == nil ? "Putting the sound under the picture" : "Moving the sound"
            case nil: "Waiting to start"
            }
        }
    }
}

struct MediaJobRow: View {
    let job: MediaJob
    let session: MetocastSession
    @State private var nudgeFailure: String?
    /// The filed recording, once the job has made one under an event.
    @State private var recording: RecordingRecord?
    @State private var uploadFailure: String?
    /// Bumped when an upload starts, which restarts `watchUpload`.
    @State private var uploads = 0

    private struct Watch: Equatable {
        var succeeded: Bool
        var uploads: Int
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack {
                Text(URL(fileURLWithPath: job.outputPath).lastPathComponent)
                Spacer()
                switch job.state {
                case .running:
                    Button("Cancel", role: .destructive) {
                        Task { try? await session.cancelRender(jobId: job.id) }
                    }
                case .succeeded:
                    Button("Play") { NSWorkspace.shared.open(URL(fileURLWithPath: job.outputPath)) }
                    Button("Show in Finder") {
                        NSWorkspace.shared.activateFileViewerSelecting([URL(fileURLWithPath: job.outputPath)])
                    }
                case .failed, .cancelled:
                    EmptyView()
                }
            }
            .buttonStyle(.borderless)
            switch job.state {
            case .running:
                if let fraction = job.fraction {
                    ProgressView(value: fraction)
                } else {
                    ProgressView().controlSize(.small)
                }
                Text(job.step).font(.caption).foregroundStyle(.secondary)
            case .succeeded:
                result
                youtube
            case .cancelled:
                Label("Cancelled", systemImage: "xmark.circle").font(.caption).foregroundStyle(.secondary)
            case .failed:
                Label(job.error ?? "Failed", systemImage: "exclamationmark.triangle")
                    .font(.caption)
                    .foregroundStyle(.red)
                if case .program(let program) = job, let report = program.report {
                    ProgramResult(report: report)
                }
            }
            if let nudgeFailure {
                Label(nudgeFailure, systemImage: "exclamationmark.triangle").font(.caption).foregroundStyle(.red)
            }
            if let uploadFailure {
                Label(uploadFailure, systemImage: "exclamationmark.triangle").font(.caption).foregroundStyle(.red)
            }
        }
        // Says which file a "Cancel" or "Play" among several jobs is for.
        .accessibilityElement(children: .contain)
        .accessibilityLabel(URL(fileURLWithPath: job.outputPath).lastPathComponent)
        .task(id: Watch(succeeded: job.state == .succeeded, uploads: uploads)) { await watchUpload() }
    }

    /// Uploading to YouTube: audio masters don't go there, and a file needs its event for a title.
    @ViewBuilder
    private var youtube: some View {
        switch job {
        case .audio:
            EmptyView()
        case .render, .program:
            if let eventId = session.jobEvents[job.id] {
                HStack(spacing: 8) {
                    if let link = recording?.videoUrl.flatMap(URL.init(string:)) {
                        Label("On YouTube (private)", systemImage: "checkmark.circle").foregroundStyle(.green)
                        Link("Watch", destination: link)
                    } else if let state = recording?.uploadStates.first, !state.contains("failed") {
                        Label(state, systemImage: "arrow.up.circle").foregroundStyle(.secondary)
                    } else {
                        Button(recording?.uploadStates.isEmpty == false ? "Try Again" : "Upload to YouTube", systemImage: "arrow.up.circle") {
                            upload(eventId: eventId)
                        }
                        .disabled(recording == nil || !youtubeIsConnected)
                        if recording?.uploadStates.isEmpty == false {
                            Text("The last upload failed.").foregroundStyle(.red)
                        } else if !youtubeIsConnected {
                            Text("Connect YouTube in Settings → Connectors.").foregroundStyle(.secondary)
                        }
                    }
                }
                .font(.caption)
                .controlSize(.small)
            } else {
                Text("Assign this file to an event to upload it to YouTube.").font(.caption).foregroundStyle(.secondary)
            }
        }
    }

    private var youtubeIsConnected: Bool { session.connectors["youtube"]?.status == .connected }

    private func upload(eventId: String) {
        guard let recording else { return }
        Task {
            do {
                try await session.flagUpload(eventId: eventId, recordingId: recording.id, platforms: ["youtube"])
                try await session.triggerUploads()
                uploadFailure = nil
                uploads += 1
            } catch {
                uploadFailure = error.metocastMessage
            }
        }
    }

    /// Finds the file's recording under its event, and follows its upload while one is going.
    /// ponytail: asks every 2 s because the Mac doesn't receive `upload.progress`; bind that in the
    /// Apple bridge if this feels coarse.
    private func watchUpload() async {
        guard job.state == .succeeded, let eventId = session.jobEvents[job.id] else { return }
        let name = URL(fileURLWithPath: job.outputPath).lastPathComponent
        var misses = 0
        repeat {
            if let found = try? await session.recordings(eventId: eventId).first(where: { $0.fileName == name }) {
                recording = found
            } else {
                misses += 1
            }
            // Done once it is filed and not on its way up; a file that isn't filed yet (the job
            // has only just ended) gets a few more looks.
            if let recording {
                if !recording.isUploading { return }
            } else if misses > 3 {
                return
            }
            try? await Task.sleep(for: .seconds(2))
        } while !Task.isCancelled
    }

    @ViewBuilder
    private var result: some View {
        switch job {
        case .audio(let audio):
            if let report = audio.report {
                let delay = Double(report.removedDelaySamples) / 48.0
                Label(
                    "\(lufs(report.inputLufs)) → \(lufs(report.outputLufs)) LUFS · peak \(lufs(report.outputTruePeakDbtp)) dBTP · \(delay.formatted(.number.precision(.fractionLength(0)))) ms delay removed",
                    systemImage: "checkmark.circle"
                )
                .font(.caption)
                .foregroundStyle(.green)
            }
        case .program(let program):
            if let report = program.report {
                ProgramResult(report: report)
                HStack {
                    Text("Lips and sound out of step?").font(.caption).foregroundStyle(.secondary)
                    Button("Audio 1 Frame Earlier") { nudge(-1) }
                    Button("Audio 1 Frame Later") { nudge(1) }
                }
                .controlSize(.small)
            }
        case .render:
            Label("Done", systemImage: "checkmark.circle").font(.caption).foregroundStyle(.green)
        }
    }

    private func nudge(_ frames: Int32) {
        Task {
            do {
                try await session.nudgeProgram(jobId: job.id, frames: frames)
                nudgeFailure = nil
            } catch {
                nudgeFailure = error.metocastMessage
            }
        }
    }
}

/// How a finished program's sound (and transcripts) were placed, in words.
private struct ProgramResult: View {
    let report: ProgramReport

    var body: some View {
        let sync = report.sync
        VStack(alignment: .leading, spacing: 3) {
            Label(placement, systemImage: sync.confident ? "checkmark.circle" : "questionmark.circle")
                .foregroundStyle(sync.confident ? .green : .orange)
            if !sync.confident {
                Text("Metocast isn't sure about this sync. Play the file and check the lips; move the sound below if needed.")
                    .foregroundStyle(.orange)
            }
            ForEach(details, id: \.self) { line in
                Text(line).foregroundStyle(.secondary)
            }
            ForEach(report.subtitles, id: \.language) { subtitle in
                Label(subtitleText(subtitle), systemImage: subtitle.confident ? "captions.bubble" : "exclamationmark.bubble")
                    .foregroundStyle(subtitle.confident ? Color.secondary : .orange)
            }
        }
        .font(.caption)
    }

    private var placement: String {
        let sync = report.sync
        let method = switch sync.method {
        case .none: "The video's own sound"
        case .waveform: "Synced by the video's sound"
        case .lips: "Synced by the lips"
        case .clock: "Placed by the clocks"
        case .manual: "Placed where you said"
        }
        guard sync.method != .none else { return method }
        let file = sync.startFile.map { " into \($0)" } ?? " into the recording"
        return "\(method) · starts \(timestamp(sync.startFileSeconds ?? sync.offsetSeconds))\(file)"
    }

    private var details: [String] {
        let sync = report.sync
        var lines: [String] = []
        if let difference = sync.clockDifferenceSeconds, abs(difference) >= 1 {
            lines.append("The camera's clock is \(timestamp(abs(difference))) \(difference > 0 ? "ahead of" : "behind") the recorder's")
        }
        if sync.driftPpm != 0 {
            lines.append("Clock drift corrected: \(sync.driftPpm.formatted(.number.precision(.fractionLength(1)))) ppm")
        }
        if sync.manualFrames != 0 {
            let frames = abs(sync.manualFrames)
            lines.append("Moved \(frames) frame\(frames == 1 ? "" : "s") \(sync.manualFrames > 0 ? "later" : "earlier") by hand")
        }
        if report.silentStartSeconds > 0.05 || report.silentEndSeconds > 0.05 {
            var silence: [String] = []
            if report.silentStartSeconds > 0.05 { silence.append("\(timestamp(report.silentStartSeconds)) at the start") }
            if report.silentEndSeconds > 0.05 { silence.append("\(timestamp(report.silentEndSeconds)) at the end") }
            lines.append("No recording for " + silence.joined(separator: " and ") + "; that part is silent")
        }
        lines.append("Voice \(lufs(report.audio.inputLufs)) → \(lufs(report.audio.outputLufs)) LUFS · peak \(lufs(report.audio.outputTruePeakDbtp)) dBTP")
        return lines
    }

    private func subtitleText(_ subtitle: SubtitleReport) -> String {
        let name = Locale.current.localizedString(forLanguageCode: subtitle.language) ?? subtitle.language
        guard subtitle.confident else {
            return "\(name) transcript (\(subtitle.cues) lines) left as it was: it didn't clearly match the voice"
        }
        let moved = abs(subtitle.shiftSeconds) < 0.05 ? "already in place" : "moved \(timestamp(subtitle.shiftSeconds))"
        return "\(name) transcript (\(subtitle.cues) lines) \(moved), saved beside the video"
    }
}

private extension RecordingRecord {
    /// An upload is queued or running ("youtube: pending", "youtube: uploading 12%", "youtube: paused").
    var isUploading: Bool {
        uploadStates.contains { state in ["pending", "uploading", "paused"].contains { state.contains(": \($0)") } }
    }
}

/// A card's heading, in Middle Control's quiet style.
struct CardTitle: View {
    let title: String
    init(_ title: String) { self.title = title }

    var body: some View {
        Text(title).font(.title3).foregroundStyle(.secondary).accessibilityAddTraits(.isHeader)
    }
}

// ── Shared formatting ───────────────────────────────────────────────────────

private func lufs(_ value: Double) -> String { value.formatted(.number.precision(.fractionLength(1))) }

/// `5:46.75`, or `1:05:46.75` past an hour; negative positions keep their sign.
func timestamp(_ seconds: Double) -> String {
    let sign = seconds < 0 ? "−" : ""
    let total = abs(seconds)
    let hours = Int(total) / 3600
    let minutes = (Int(total) % 3600) / 60
    let rest = total - Double(hours * 3600 + minutes * 60)
    let secondsText = rest.formatted(.number.precision(.integerAndFractionLength(integer: 2, fraction: 2)))
    return hours > 0
        ? "\(sign)\(hours):\(String(format: "%02d", minutes)):\(secondsText)"
        : "\(sign)\(minutes):\(secondsText)"
}

func durationText(_ seconds: Double) -> String {
    Duration.seconds(seconds).formatted(.time(pattern: .hourMinuteSecond))
}

func audioText(_ audio: AudioInfo) -> String {
    "\(audio.channels) ch, \(audio.sampleRate / 1000) kHz\(audio.bits.map { ", \($0)-bit" } ?? "")"
}

/// What to do about a tool the server lacks.
func advice(_ tool: String) -> String {
    switch tool {
    case "ffmpeg": "ffmpeg is missing from this build of Metocast"
    case "jive-vocals": "The jive-vocals path in Media Processing doesn't point to a program"
    case "lip-reader": "lip-reader is missing from this build of Metocast"
    case "braw-decode": "braw-decode is missing: install Blackmagic RAW and rebuild Metocast"
    case "blackmagic-raw-sdk": "Install Blackmagic RAW (free, from blackmagicdesign.com)"
    default: tool
    }
}

/// What a dropped file is to the Media page, by its extension.
enum MediaFileKind {
    case video, audio, transcript, sidecar, lut

    init(_ url: URL) {
        self = switch url.pathExtension.lowercased() {
        case "braw", "mov", "mp4": .video
        case "srt", "vtt": .transcript
        case "sidecar": .sidecar
        case "cube": .lut
        default: .audio
        }
    }
}

extension UTType {
    static let blackmagicRAW = UTType(filenameExtension: "braw") ?? .movie
    static let cubeLUT = UTType(filenameExtension: "cube") ?? .data
    static let brawSidecar = UTType(filenameExtension: "sidecar") ?? .data
    static let subRip = UTType(filenameExtension: "srt") ?? .text
    static let webVTT = UTType(filenameExtension: "vtt") ?? .text
}
#endif
