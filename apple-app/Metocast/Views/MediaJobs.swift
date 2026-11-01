#if os(macOS)
import AppKit
#endif
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
        .accessibilityValue(session.runningMediaJobs.first?.fraction.map { $0.formatted(.percent.precision(.fractionLength(0))) } ?? String(localized: "Running"))
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
            guard job.totalFrames > 0, job.frame > 0, job.fps > 0 else { return String(localized: "Waiting to start") }
            let left = Double(job.totalFrames - min(job.frame, job.totalFrames)) / job.fps
            let fps = job.fps.formatted(.number.precision(.fractionLength(1)))
            let remaining = Duration.seconds(left).formatted(.units(allowed: [.hours, .minutes], width: .abbreviated))
            return String(localized: "\(fps) fps · \(remaining) left")
        case .audio(let job):
            return switch job.step {
            case .download: String(localized: "Downloading jive-vocals")
            case .prepare: String(localized: "1 of 4 · Joining and mixing")
            case .clean: String(localized: "2 of 4 · Cleaning with jive-vocals")
            case .align: String(localized: "3 of 4 · Lining up with the source")
            case .finish: String(localized: "4 of 4 · Setting the loudness")
            case nil: String(localized: "Waiting to start")
            }
        case .program(let job):
            return switch job.step {
            case .download: String(localized: "Downloading tools (first time only)")
            case .picture: String(localized: "Grading and encoding the picture")
            case .prepare: String(localized: "Joining the audio")
            case .sync: String(localized: "Finding where the video starts in the audio")
            case .clean: String(localized: "Cleaning the voice with jive-vocals")
            case .merge: job.report == nil ? String(localized: "Putting the sound under the picture") : String(localized: "Moving the sound")
            case .portrait: job.fraction.map {
                String(localized: "Making the portrait video · \($0.formatted(.percent.precision(.fractionLength(0))))")
            } ?? String(localized: "Making the portrait video")
            case .captions: job.fraction.map {
                String(localized: "Placing the transcript on the speech · \($0.formatted(.percent.precision(.fractionLength(0))))")
            } ?? String(localized: "Placing the transcript on the speech")
            case nil: String(localized: "Waiting to start")
            }
        }
    }
}

struct MediaJobRow: View {
    let job: MediaJob
    let session: MetocastSession
    /// Why a nudge or added transcripts were refused.
    @State private var adjustFailure: String?
    @State private var transcriptTargeted = false
    /// The filed recordings, once the job has made them under an event: landscape, then portrait.
    @State private var recordings: [RecordingRecord] = []
    /// Bumped when an upload starts, which restarts `watchUpload`.
    @State private var uploadsStarted = 0

    private struct Watch: Equatable {
        var succeeded: Bool
        var uploadsStarted: Int
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
                    #if os(macOS)
                    Button("Play") { NSWorkspace.shared.open(URL(fileURLWithPath: job.outputPath)) }
                    Button("Show in Finder") {
                        NSWorkspace.shared.activateFileViewerSelecting([URL(fileURLWithPath: job.outputPath)])
                    }
                    #else
                    EmptyView()
                    #endif
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
                uploads
            case .cancelled:
                Label("Cancelled", systemImage: "xmark.circle").font(.caption).foregroundStyle(.secondary)
            case .failed:
                Label(job.error ?? String(localized: "Failed"), systemImage: "exclamationmark.triangle")
                    .font(.caption)
                    .foregroundStyle(.red)
                if case .program(let program) = job, let report = program.report {
                    ProgramResult(report: report)
                }
            }
            if let adjustFailure {
                Label(adjustFailure, systemImage: "exclamationmark.triangle").font(.caption).foregroundStyle(.red)
            }
        }
        // A finished program takes transcripts dropped on its row: they're placed on it as the job
        // would have, without producing it again.
        .dropDestination(for: URL.self) { urls, _ in
            addTranscripts(urls)
        } isTargeted: { transcriptTargeted = $0 }
        .background(transcriptTargeted ? Color.accentColor.opacity(0.12) : .clear, in: .rect(cornerRadius: 6))
        // Says which file a "Cancel" or "Play" among several jobs is for.
        .accessibilityElement(children: .contain)
        .accessibilityLabel(URL(fileURLWithPath: job.outputPath).lastPathComponent)
        .task(id: Watch(succeeded: job.state == .succeeded, uploadsStarted: uploadsStarted)) { await watchUpload() }
    }

    /// Where each file goes: audio masters go nowhere, and a file needs its event for a title.
    @ViewBuilder
    private var uploads: some View {
        switch job {
        case .audio:
            EmptyView()
        case .render, .program:
            if let eventId = session.jobEvents[job.id] {
                UploadChoices(session: session, eventId: eventId, recordings: recordings) {
                    uploadsStarted += 1
                }
            } else {
                Text("Assign this file to an event to upload it.").font(.caption).foregroundStyle(.secondary)
            }
        }
    }

    /// Finds the job's recordings under its event, and follows their uploads while any is going.
    /// ponytail: asks every 2 s because the Mac doesn't receive `upload.progress`; bind that in the
    /// Apple bridge if this feels coarse.
    private func watchUpload() async {
        guard job.state == .succeeded, let eventId = session.jobEvents[job.id] else { return }
        let name = URL(fileURLWithPath: job.outputPath).lastPathComponent
        let names = [name, portraitName(fileName: name)]
        var misses = 0
        repeat {
            let filed = (try? await session.recordings(eventId: eventId)) ?? []
            let found = names.compactMap { name in filed.first { $0.fileName == name } }
            if found.isEmpty {
                misses += 1
            } else {
                recordings = found
            }
            // Done once it is filed and not on its way up; a file that isn't filed yet (the job
            // has only just ended) gets a few more looks.
            if !recordings.isEmpty {
                if !recordings.contains(where: \.isUploading) { return }
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
                Text("Drop a transcript (.vtt or .srt) here to place it on this program.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
        case .render:
            Label("Done", systemImage: "checkmark.circle").font(.caption).foregroundStyle(.green)
        }
    }

    /// Sends the transcripts among `urls` to a finished program; `false` when there are none or
    /// this row isn't a finished program.
    private func addTranscripts(_ urls: [URL]) -> Bool {
        guard case .program = job, job.state == .succeeded else { return false }
        let subtitles = urls
            .filter { MediaFileKind($0) == .transcript }
            .map { SubtitleSource(path: $0.path, language: MediaFileKind.transcriptLanguage($0)) }
        guard !subtitles.isEmpty else { return false }
        Task {
            do {
                try await session.addProgramSubtitles(jobId: job.id, subtitles: subtitles)
                adjustFailure = nil
            } catch {
                adjustFailure = error.metocastMessage
            }
        }
        return true
    }

    private func nudge(_ frames: Int32) {
        Task {
            do {
                try await session.nudgeProgram(jobId: job.id, frames: frames)
                adjustFailure = nil
            } catch {
                adjustFailure = error.metocastMessage
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
            if report.damagedFrames > 0 {
                Label(String(localized: "The recording didn't end cleanly: the last good frame stands in for \(report.damagedFrames) damaged frames at its end"),
                      systemImage: "exclamationmark.triangle")
                    .foregroundStyle(.orange)
            }
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
        case .none: String(localized: "The video's own sound")
        case .waveform: String(localized: "Synced by the video's sound")
        case .lips: String(localized: "Synced by the lips")
        case .clock: String(localized: "Placed by the clocks")
        case .manual: String(localized: "Placed where you said")
        }
        guard sync.method != .none else { return method }
        let start = timestamp(sync.startFileSeconds ?? sync.offsetSeconds)
        return sync.startFile.map { String(localized: "\(method) · starts \(start) into \($0)") }
            ?? String(localized: "\(method) · starts \(start) into the recording")
    }

    private var details: [String] {
        let sync = report.sync
        var lines: [String] = []
        if let difference = sync.clockDifferenceSeconds, abs(difference) >= 1 {
            let gap = timestamp(abs(difference))
            lines.append(difference > 0
                ? String(localized: "The camera's clock is \(gap) ahead of the recorder's")
                : String(localized: "The camera's clock is \(gap) behind the recorder's"))
        }
        if sync.driftPpm != 0 {
            lines.append(String(localized: "Clock drift corrected: \(sync.driftPpm.formatted(.number.precision(.fractionLength(1)))) ppm"))
        }
        if sync.manualFrames != 0 {
            let frames = abs(sync.manualFrames)
            lines.append(sync.manualFrames > 0
                ? String(localized: "Moved \(frames) frames later by hand")
                : String(localized: "Moved \(frames) frames earlier by hand"))
        }
        if report.silentStartSeconds > 0.05 || report.silentEndSeconds > 0.05 {
            var silence: [String] = []
            if report.silentStartSeconds > 0.05 { silence.append(String(localized: "\(timestamp(report.silentStartSeconds)) at the start")) }
            if report.silentEndSeconds > 0.05 { silence.append(String(localized: "\(timestamp(report.silentEndSeconds)) at the end")) }
            lines.append(String(localized: "No recording for \(silence.formatted(.list(type: .and))); that part is silent"))
        }
        lines.append(String(localized: "Voice \(lufs(report.audio.inputLufs)) → \(lufs(report.audio.outputLufs)) LUFS · peak \(lufs(report.audio.outputTruePeakDbtp)) dBTP"))
        return lines
    }

    private func subtitleText(_ subtitle: SubtitleReport) -> String {
        let name = Locale.current.localizedString(forLanguageCode: subtitle.language) ?? subtitle.language
        if subtitle.onSpeech {
            return String(localized: "\(name) transcript: \(subtitle.cues) sentences placed on the speech, \(subtitle.matched) matched by word, saved beside the video")
        }
        guard subtitle.confident else {
            guard abs(subtitle.shiftSeconds) >= 0.05 else {
                return String(localized: "\(name) transcript (\(subtitle.cues) lines) left as it was: it didn't clearly match the voice")
            }
            return String(localized: "\(name) transcript (\(subtitle.cues) lines) moved \(timestamp(subtitle.shiftSeconds)) with the recording; it didn't clearly match the voice, so check it")
        }
        if abs(subtitle.shiftSeconds) < 0.05 {
            return String(localized: "\(name) transcript (\(subtitle.cues) lines) already in place, saved beside the video")
        }
        return String(localized: "\(name) transcript (\(subtitle.cues) lines) moved \(timestamp(subtitle.shiftSeconds)), saved beside the video")
    }
}

/// Picks where each of a job's files uploads, and starts it. A portrait file starts on Facebook
/// and any other on YouTube; the operator can send either file to either place, or both.
struct UploadChoices: View {
    let session: MetocastSession
    let eventId: String
    let recordings: [RecordingRecord]
    let onStarted: () -> Void
    /// What the operator changed; a recording not here has its default.
    @State private var chosen: [String: Set<UploadPlatform>] = [:]
    @State private var failure: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            ForEach(recordings, id: \.id) { recording in
                row(recording)
            }
            HStack(spacing: 8) {
                if needsChoice {
                    Button("Upload", systemImage: "arrow.up.circle") { upload() }
                        .disabled(ready.isEmpty)
                }
                ForEach(UploadPlatform.allCases.filter { !isConnected($0) }, id: \.self) { platform in
                    Text("Connect \(platform.name) in Settings → Connectors.").foregroundStyle(.secondary)
                }
            }
            if let failure {
                Label(failure, systemImage: "exclamationmark.triangle").foregroundStyle(.red)
            }
        }
        .font(.caption)
        .controlSize(.small)
    }

    private func row(_ recording: RecordingRecord) -> some View {
        HStack(spacing: 8) {
            Label(
                recording.isPortrait ? String(localized: "Portrait") : String(localized: "Landscape"),
                systemImage: recording.isPortrait ? "rectangle.portrait" : "rectangle"
            )
            .frame(minWidth: 90, alignment: .leading)
            ForEach(UploadPlatform.allCases, id: \.self) { platform in
                if let state = recording.uploadState(platform), !state.contains("failed") {
                    Label(state, systemImage: state.contains("completed") ? "checkmark.circle" : "arrow.up.circle")
                        .foregroundStyle(state.contains("completed") ? .green : .secondary)
                } else {
                    Toggle(platform.name, isOn: Binding(
                        get: { selection(recording).contains(platform) },
                        set: { isOn in
                            var platforms = selection(recording)
                            if isOn { platforms.insert(platform) } else { platforms.remove(platform) }
                            chosen[recording.id] = platforms
                        }
                    ))
                    .toggleStyle(.button)
                    .disabled(!isConnected(platform))
                    if recording.uploadState(platform) != nil {
                        Text("The last upload failed.").foregroundStyle(.red)
                    }
                }
            }
            if let link = recording.videoUrl.flatMap(URL.init(string:)) {
                Link("Watch", destination: link)
            }
        }
        // Says which file a platform's toggle among several is for.
        .accessibilityElement(children: .contain)
        .accessibilityLabel(recording.fileName)
    }

    private func isConnected(_ platform: UploadPlatform) -> Bool {
        session.connectors[platform.rawValue]?.status == .connected
    }

    private func selection(_ recording: RecordingRecord) -> Set<UploadPlatform> {
        chosen[recording.id] ?? [recording.isPortrait ? .facebook : .youtube]
    }

    /// Whether any file still has a platform it could go to.
    private var needsChoice: Bool {
        recordings.contains { recording in
            UploadPlatform.allCases.contains { recording.uploadState($0)?.contains("failed") ?? true }
        }
    }

    /// Each file with the connected platforms picked for it that it hasn't already gone to.
    private var ready: [(recording: RecordingRecord, platforms: [String])] {
        recordings.compactMap { recording in
            let platforms = UploadPlatform.allCases
                .filter { selection(recording).contains($0) && isConnected($0) }
                .filter { recording.uploadState($0)?.contains("failed") ?? true }
                .map(\.rawValue)
            return platforms.isEmpty ? nil : (recording, platforms)
        }
    }

    private func upload() {
        let ready = ready
        Task {
            do {
                for (recording, platforms) in ready {
                    try await session.flagUpload(eventId: eventId, recordingId: recording.id, platforms: platforms)
                }
                try await session.triggerUploads()
                failure = nil
                onStarted()
            } catch {
                failure = error.metocastMessage
            }
        }
    }
}

enum UploadPlatform: String, CaseIterable {
    case youtube, facebook

    var name: String {
        switch self {
        case .youtube: "YouTube"
        case .facebook: "Facebook"
        }
    }
}

extension RecordingRecord {
    var isPortrait: Bool { isPortraitSource(source: source) }

    /// How this file's upload to `platform` is going ("youtube: uploading 12%"), when it has one.
    func uploadState(_ platform: UploadPlatform) -> String? {
        uploadStates.first { $0.hasPrefix("\(platform.rawValue): ") }
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
    let title: LocalizedStringKey
    init(_ title: LocalizedStringKey) { self.title = title }

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
    let rate = String(localized: "\(audio.channels) ch, \(audio.sampleRate / 1000) kHz")
    return audio.bits.map { String(localized: "\(rate), \($0)-bit") } ?? rate
}

/// What to do about a tool the server lacks.
func advice(_ tool: String) -> String {
    switch tool {
    case "ffmpeg": String(localized: "ffmpeg is missing from this build of Metocast")
    case "jive-vocals": String(localized: "The jive-vocals path in Media Processing doesn't point to a program")
    case "lip-reader": String(localized: "lip-reader is missing from this build of Metocast")
    case "braw-decode": String(localized: "braw-decode is missing: install Blackmagic RAW and rebuild Metocast")
    case "blackmagic-raw-sdk": String(localized: "Install Blackmagic RAW (free, from blackmagicdesign.com)")
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

    /// `sermon.en.srt` (and Transcripts' downloads) say their language; Hungarian otherwise, as
    /// the sermons are.
    static func transcriptLanguage(_ url: URL) -> String {
        let suffix = url.deletingPathExtension().pathExtension.lowercased()
        return ["hu", "en"].contains(suffix) ? suffix : "hu"
    }
}

extension UTType {
    static let blackmagicRAW = UTType(filenameExtension: "braw") ?? .movie
    static let cubeLUT = UTType(filenameExtension: "cube") ?? .data
    static let brawSidecar = UTType(filenameExtension: "sidecar") ?? .data
    static let subRip = UTType(filenameExtension: "srt") ?? .text
    static let webVTT = UTType(filenameExtension: "vtt") ?? .text
}
