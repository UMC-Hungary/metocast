#if os(macOS)
import AppKit
import Metocast
import SwiftUI
import UniformTypeIdentifiers

/// One place to turn a service's files into uploads: drop the video, its audio, a transcript and
/// a Resolve look in one field, check the picture, and run the job on the server's `media` queue.
/// Laid out like Middle Things' Middle Control: cards, −/+ rows and a black viewer.
struct MediaView: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        if let session = model.session {
            MediaPage(session: session).id(ObjectIdentifier(session))
        } else {
            ContentUnavailableView("Not Connected", systemImage: "film.stack", description: Text("The Media page needs this Mac's server."))
        }
    }
}

/// What the dropped files make: a program (video with synced, cleaned sound), a render of the
/// video alone, or an audio master.
enum MediaWork {
    case program, render, audio

    var action: String {
        switch self {
        case .program: "Produce"
        case .render: "Render"
        case .audio: "Process"
        }
    }
}

/// What happens to a video's own sound when no recording comes with it.
enum VideoSoundChoice: Hashable {
    case cleaned, asRecorded, none
}

struct Transcript: Hashable {
    let path: String
    var language: String
}

/// The page's files, what the server read from them, and the settings for this one job.
@Observable
final class MediaDraft {
    let session: MetocastSession
    var videoPath: String?
    var audioPaths: [String] = []
    var transcripts: [Transcript] = []
    var sidecarPath: String?
    /// What the server read: `program` with a video, `audio` for audio alone.
    var program: ProgramProbeRecord?
    var audio: AudioProbeRecord?
    var settings: RenderSettings?
    var audioSettings: AudioSettings?
    var picture: PictureMode = .grade
    var sound: VideoSoundChoice = .cleaned
    var event: EventSummaryRecord?
    var frame = 0.0
    var original: NSImage?
    var result: NSImage?
    var crop: (active: CropRect, output: CropRect)?
    var isWorking = false
    var isPreviewing = false
    var failure: String?
    var previewFailure: String?
    /// The look last saved as the default, to show it's saved until it changes.
    var savedLook: RenderSettings?
    /// Where finished files go, from the Media Processing settings; empty is the server's own folder.
    var outputFolder = ""
    var audioOutputFolder = ""
    private var previewTask: Task<Void, Never>?
    private var originalFrame: UInt64?

    init(session: MetocastSession) { self.session = session }

    var work: MediaWork? {
        if program != nil { return !audioPaths.isEmpty || sound == .cleaned ? .program : .render }
        return audio != nil ? .audio : nil
    }

    /// Tools the server lacks for the job that would run.
    var missing: [String] {
        switch work {
        case .program:
            let missing = program?.missing ?? []
            // Copying the picture needs neither braw-decode nor the SDK.
            return picture == .copy ? missing.filter { $0 != "braw-decode" && $0 != "blackmagic-raw-sdk" } : missing
        case .render: return program?.video.missing ?? []
        case .audio: return audio?.missing ?? []
        case nil: return []
        }
    }

    /// Adds dropped or opened files: a video replaces the video, a look replaces the look, and
    /// audio files and transcripts join the rest.
    func add(_ urls: [URL]) async {
        var newSidecar = false
        var newLUT: String?
        for url in urls {
            switch MediaFileKind(url) {
            case .video: videoPath = url.path
            case .audio: audioPaths.append(url.path)
            case .transcript:
                // `sermon.en.srt` says its language; Hungarian otherwise, as the sermons are.
                let suffix = url.deletingPathExtension().pathExtension.lowercased()
                transcripts.removeAll { $0.path == url.path }
                transcripts.append(Transcript(path: url.path, language: ["hu", "en"].contains(suffix) ? suffix : "hu"))
            case .sidecar:
                sidecarPath = url.path
                newSidecar = true
            case .lut:
                newLUT = url.path
            }
        }
        // Recorders number the parts of a long take, so name order is playing order.
        audioPaths = Array(Set(audioPaths)).sorted { $0.localizedStandardCompare($1) == .orderedAscending }
        await reprobe(applySidecar: newSidecar)
        // After the probe, which sets a new video's settings: the LUT may come in the same drop.
        if let newLUT {
            settings?.lutPath = newLUT
            settings?.lutMode = .file
        }
    }

    func remove(_ path: String) async {
        if path == videoPath {
            videoPath = nil
        } else if path == sidecarPath {
            sidecarPath = nil
            settings?.sidecarPath = nil
            if settings?.lutMode == .sidecar { settings?.lutMode = .embedded }
        }
        audioPaths.removeAll { $0 == path }
        transcripts.removeAll { $0.path == path }
        await reprobe(applySidecar: false)
    }

    private func reprobe(applySidecar: Bool) async {
        isWorking = true
        defer { isWorking = false }
        do {
            if let videoPath {
                let program = try await session.probeProgram(videoPath: videoPath, audioPaths: audioPaths)
                let isNewVideo = program.video.path != self.program?.video.path
                self.program = program
                audio = nil
                if isNewVideo {
                    settings = program.video.defaults
                    picture = program.picture
                    sound = program.videoSound.usable ? .cleaned : .none
                    // The middle is more likely the sermon than the first frame is.
                    frame = Double(program.video.frameCount / 2)
                    original = nil
                    result = nil
                    originalFrame = nil
                    crop = (program.video.activeArea, program.video.output)
                }
                if let sidecarPath, program.video.kind == .braw, applySidecar || isNewVideo {
                    var look = try await session.probeMedia(path: videoPath, sidecarPath: sidecarPath).defaults
                    // A look made on another clip can't carry white balance, ISO or exposure (the SDK
                    // keys those to the clip's own timecode), so what's set stays unless the sidecar
                    // changed it for this clip.
                    if let shot = program.video.rawOptions?.asShot, let current = settings {
                        if look.iso == shot.iso { look.iso = current.iso }
                        if look.kelvin == shot.kelvin { look.kelvin = current.kelvin }
                        if look.tint == shot.tint { look.tint = current.tint }
                        if look.exposure == shot.exposure { look.exposure = current.exposure }
                    }
                    settings = look
                }
                audioSettings = program.audio?.defaults ?? audioSettings ?? AudioSettings(channels: [], targetLufs: -14)
                refreshPreview()
            } else {
                program = nil
                settings = nil
                original = nil
                result = nil
                audio = audioPaths.isEmpty ? nil : try await session.probeAudio(paths: audioPaths)
                if let audio { audioSettings = audio.defaults }
            }
            failure = videoPath == nil && !transcripts.isEmpty ? "A transcript needs the video it belongs to." : nil
        } catch {
            failure = error.metocastMessage
        }
    }

    /// Asks for the frame under the playhead, graded, then as recorded when the frame moved.
    /// Newer requests cancel older ones, so scrubbing settles on the last position.
    func refreshPreview() {
        previewTask?.cancel()
        guard let video = program?.video, let settings else { return }
        let frame = UInt64(max(0, self.frame.rounded()))
        previewTask = Task {
            try? await Task.sleep(for: .milliseconds(120))
            guard !Task.isCancelled else { return }
            isPreviewing = true
            defer { isPreviewing = false }
            do {
                let graded = try await session.previewMedia(path: video.path, frame: frame, settings: settings)
                guard !Task.isCancelled else { return }
                result = Data(base64Encoded: graded.imageJpeg).flatMap(NSImage.init(data:))
                crop = (graded.active, graded.output)
                if originalFrame != frame {
                    let recorded = try await session.previewMedia(path: video.path, frame: frame, settings: nil)
                    guard !Task.isCancelled else { return }
                    original = Data(base64Encoded: recorded.imageJpeg).flatMap(NSImage.init(data:))
                    originalFrame = frame
                }
                previewFailure = nil
            } catch {
                if !Task.isCancelled { previewFailure = error.metocastMessage }
            }
        }
    }

    func start() async {
        guard let work else { return }
        isWorking = true
        defer { isWorking = false }
        do {
            switch work {
            case .program:
                guard let program else { return }
                try await session.startProgram(ProgramRequestRecord(
                    videoPath: program.video.path,
                    audioPaths: program.audio?.sources.map(\.path) ?? [],
                    eventId: event?.id,
                    picture: picture,
                    settings: picture == .grade ? settings : nil,
                    audioSettings: audioSettings,
                    startSeconds: nil,
                    subtitles: transcripts.map { SubtitleSource(path: $0.path, language: $0.language) }
                ))
            case .render:
                guard let program, var settings else { return }
                if sound == .none { settings.audioChannels = [] }
                try await session.startRender(path: program.video.path, eventId: event?.id, settings: settings)
            case .audio:
                guard let audio else { return }
                try await session.startAudio(paths: audio.sources.map(\.path), eventId: event?.id, settings: audioSettings)
            }
            failure = nil
        } catch {
            failure = error.metocastMessage
        }
    }

    /// Keeps this look for every later clip: the Media Processing defaults, with the Resolve
    /// sidecar and LUT copied into Metocast's own folder so they outlive the camera's drive.
    func saveAsDefault() async {
        guard var settings else { return }
        do {
            settings.sidecarPath = try settings.sidecarPath.map(keep)
            settings.lutPath = try settings.lutPath.map(keep)
            let lutMode = switch settings.lutMode {
            case .embedded: "embedded"
            case .file: "file"
            case .sidecar: "sidecar"
            case .none: "none"
            }
            let look: [String: String] = [
                "gamma": settings.gamma,
                "gamut": settings.gamut,
                "iso": "\(settings.iso ?? 0)",
                "kelvin": "\(settings.kelvin ?? 0)",
                "tint": settings.tint.map(String.init) ?? "",
                "exposure": settings.exposure.map { "\($0)" } ?? "",
                "highlightRecovery": "\(settings.highlightRecovery)",
                "gamutCompression": "\(settings.gamutCompression)",
                "lutMode": lutMode,
                "lutPath": settings.lutPath ?? "",
                "sidecarPath": settings.sidecarPath ?? "",
                "aspect": settings.aspect == .widescreen ? "16:9" : "native",
                "cropShiftY": "\(settings.cropShiftY)",
                "bitrateMbps": "\(settings.bitrateMbps)",
            ]
            try await updateMediaConfig(look)
            self.settings = settings
            savedLook = settings
            failure = nil
        } catch {
            failure = error.metocastMessage
        }
    }

    /// Changes some Media Processing settings and keeps the others: the connector's PUT replaces
    /// the whole config, so the stored values are read back first.
    private func updateMediaConfig(_ changes: [String: String]) async throws {
        var values = Dictionary(uniqueKeysWithValues: try await session.connectorForm("media").map { ($0.key, $0.value) })
        values.merge(changes) { _, new in new }
        try await session.saveConnector("media", values: values.map { ConfigValueRecord(key: $0.key, value: $0.value, clear: false) })
    }

    /// The folder the job that would run writes to: the audio one for audio alone.
    var saveFolder: String { work == .audio ? audioOutputFolder : outputFolder }

    func loadFolders() async {
        guard let form = try? await session.connectorForm("media") else { return }
        outputFolder = form.first { $0.key == "outputFolder" }?.value ?? ""
        audioOutputFolder = form.first { $0.key == "audioOutputFolder" }?.value ?? ""
    }

    /// Keeps `path` as where finished files go (empty: the server's own folder).
    func setSaveFolder(_ path: String) async {
        let audioOnly = work == .audio
        do {
            try await updateMediaConfig([audioOnly ? "audioOutputFolder" : "outputFolder": path])
            if audioOnly { audioOutputFolder = path } else { outputFolder = path }
            failure = nil
        } catch {
            failure = error.metocastMessage
        }
    }

    /// Where saved looks live: Resolve sidecars and LUTs kept by "Save as Default".
    static let looksFolder = URL.applicationSupportDirectory.appending(path: "Metocast/Looks", directoryHint: .isDirectory)

    /// The looks: `.sidecar` and `.cube` files saved in `looksFolder`, then those the app ships
    /// (the church's Resolve Look) unless a saved one has the same name, by name.
    static var looks: [URL] {
        let saved = (try? FileManager.default.contentsOfDirectory(at: looksFolder, includingPropertiesForKeys: nil)) ?? []
        let shipped = ["sidecar", "cube"].flatMap { Bundle.main.urls(forResourcesWithExtension: $0, subdirectory: nil) ?? [] }
        let names = Set(saved.map(\.lastPathComponent))
        return (saved + shipped.filter { !names.contains($0.lastPathComponent) })
            .filter { ["sidecar", "cube"].contains($0.pathExtension.lowercased()) }
            .sorted { $0.lastPathComponent.localizedStandardCompare($1.lastPathComponent) == .orderedAscending }
    }

    /// A copy of `path` in `looksFolder`.
    private func keep(_ path: String) throws -> String {
        let folder = Self.looksFolder
        let source = URL(fileURLWithPath: path)
        let copy = folder.appending(path: source.lastPathComponent)
        guard copy.path != path else { return path }
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        try? FileManager.default.removeItem(at: copy)
        try FileManager.default.copyItem(at: source, to: copy)
        return copy.path
    }
}

/// The system's open panel: files the page understands, or one folder to save into. It is a panel
/// of its own, not a `.fileImporter`: two of those on one view leave only the last working.
@MainActor
private func choose(_ types: [UTType]) -> [URL] {
    let panel = NSOpenPanel()
    panel.allowsMultipleSelection = true
    panel.allowedContentTypes = types
    return panel.runModal() == .OK ? panel.urls : []
}

@MainActor
private func chooseFolder() -> URL? {
    let panel = NSOpenPanel()
    panel.canChooseFiles = false
    panel.canChooseDirectories = true
    panel.canCreateDirectories = true
    panel.prompt = "Choose"
    return panel.runModal() == .OK ? panel.url : nil
}

private struct MediaPage: View {
    @State private var draft: MediaDraft
    @State private var isTargeted = false
    @State private var isPickingEvent = false

    init(session: MetocastSession) {
        _draft = State(initialValue: MediaDraft(session: session))
    }

    var body: some View {
        ScrollView {
            VStack(spacing: 16) {
                Text("Media").font(.system(size: 34, weight: .light)).padding(.top, 4).accessibilityAddTraits(.isHeader)
                // Cards in a row share the tallest one's height.
                HStack(alignment: .top, spacing: 16) {
                    FilesCard(draft: draft, isTargeted: isTargeted) {
                        let urls = choose([.movie, .audio, .blackmagicRAW, .subRip, .webVTT, .brawSidecar, .cubeLUT])
                        Task { await draft.add(urls) }
                    }
                    .dropDestination(for: URL.self) { urls, _ in
                        Task { await draft.add(urls) }
                        return !urls.isEmpty
                    } isTargeted: { isTargeted = $0 }
                    if let program = draft.program, program.audio != nil {
                        SyncCard(probe: program)
                    }
                    StartCard(draft: draft) { isPickingEvent = true }
                }
                .fixedSize(horizontal: false, vertical: true)
                if let video = draft.program?.video {
                    MediaViewer(draft: draft, video: video) {
                        let urls = choose([.cubeLUT])
                        Task { await draft.add(urls) }
                    }
                }
                if draft.work != nil {
                    OutputCard(draft: draft)
                }
                if !draft.transcripts.isEmpty {
                    SubtitlesCard(draft: draft)
                }
                MediaJobsCard(session: draft.session)
            }
            .padding(20)
            .frame(maxWidth: 1400)
            .frame(maxWidth: .infinity)
        }
        .navigationTitle("Media")
        .task { await draft.loadFolders() }
        .sheet(isPresented: $isPickingEvent) {
            EventPicker(session: draft.session) { draft.event = $0 }
        }
        .onChange(of: draft.frame) { draft.refreshPreview() }
        .onChange(of: draft.settings) { draft.refreshPreview() }
    }
}

/// The one drop field, and what's in it.
private struct FilesCard: View {
    let draft: MediaDraft
    let isTargeted: Bool
    let open: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            CardTitle("Files")
            VStack(spacing: 8) {
                HStack(spacing: 12) {
                    Image(systemName: "film")
                    Image(systemName: "waveform")
                    Image(systemName: "captions.bubble")
                    Image(systemName: "camera.filters")
                }
                .font(.title2)
                .foregroundStyle(isTargeted ? Color.accentColor : .secondary)
                .accessibilityHidden(true)
                Text(draft.videoPath == nil ? "Drop the video, its audio and a transcript" : "Drop more files")
                    .font(.headline)
                Button("Open Files…", action: open)
                if draft.isWorking { ProgressView().controlSize(.small) }
            }
            .frame(maxWidth: .infinity, minHeight: 120)
            .background {
                RoundedRectangle(cornerRadius: 12)
                    .strokeBorder(isTargeted ? Color.accentColor : .secondary.opacity(0.5), style: StrokeStyle(lineWidth: 2, dash: [7, 5]))
            }
            if let videoPath = draft.videoPath {
                row(videoPath, "film", detail: videoDetail)
            }
            ForEach(draft.audioPaths, id: \.self) { path in
                row(path, "waveform", detail: audioDetail(path))
            }
            if draft.videoPath != nil, draft.audioPaths.isEmpty {
                Label("The video's own sound", systemImage: "speaker.wave.2").foregroundStyle(.secondary)
            }
            ForEach(draft.transcripts, id: \.path) { transcript in
                row(transcript.path, "captions.bubble", detail: "Transcript")
            }
            if let sidecarPath = draft.sidecarPath {
                row(sidecarPath, "camera.filters", detail: "Resolve look (Camera RAW settings)")
            }
            if let lutPath = draft.settings?.lutPath, draft.settings?.lutMode == .file {
                Label(URL(fileURLWithPath: lutPath).lastPathComponent, systemImage: "camera.filters")
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        .card()
    }

    private func audioDetail(_ path: String) -> String? {
        let sources = draft.program?.audio?.sources ?? draft.audio?.sources ?? []
        return sources.first(where: { $0.path == path }).map { "\(durationText($0.durationSeconds)) · \(audioText($0.audio))" }
    }

    private var videoDetail: String? {
        guard let video = draft.program?.video else { return nil }
        return ["\(durationText(video.durationSeconds))", video.camera, video.audio.map(audioText)]
            .compactMap { $0 }
            .joined(separator: " · ")
    }

    private func row(_ path: String, _ symbol: String, detail: String?) -> some View {
        HStack {
            Label {
                VStack(alignment: .leading, spacing: 2) {
                    Text(URL(fileURLWithPath: path).lastPathComponent).lineLimit(1)
                    if let detail { Text(detail).font(.caption).foregroundStyle(.secondary) }
                }
            } icon: {
                Image(systemName: symbol)
            }
            Spacer()
            Button("Remove \(URL(fileURLWithPath: path).lastPathComponent)", systemImage: "xmark.circle.fill") { Task { await draft.remove(path) } }
                .labelStyle(.iconOnly)
                .buttonStyle(.borderless)
                .foregroundStyle(.secondary)
        }
    }
}

/// How the program will line the recording up with the video: always found by Metocast.
private struct SyncCard: View {
    let probe: ProgramProbeRecord

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            CardTitle("Sync")
            LabeledContent("Sync", value: "Automatic")
            LabeledContent("Method", value: methodText)
            LabeledContent("Video's Own Sound", value: soundText)
            ForEach(probe.warnings, id: \.self) { warning in
                Label(warning, systemImage: "exclamationmark.triangle").font(.caption).foregroundStyle(.orange)
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        .card()
    }

    private var soundText: String {
        let sound = probe.videoSound
        guard sound.hasTrack else { return "None" }
        let level = sound.levelDbfs.map { " (\($0.formatted(.number.precision(.fractionLength(0)))) dBFS)" } ?? ""
        return (sound.usable ? "Usable" : "Silent") + level
    }

    private var methodText: String {
        switch probe.syncMethod {
        case .none: "None needed"
        case .waveform: "The video's sound"
        case .lips: "The speaker's lips"
        case .clock: "The clocks"
        case .manual: "Where you say"
        }
    }
}

/// Middle Control's record button: what will run, where it's filed, and why it can't run yet.
private struct StartCard: View {
    let draft: MediaDraft
    let pickEvent: () -> Void

    var body: some View {
        let isRunning = !draft.session.runningMediaJobs.isEmpty
        VStack(spacing: 12) {
            CardTitle(isRunning ? "Job" : "Start").frame(maxWidth: .infinity, alignment: .leading)
            if isRunning {
                // The job takes the Produce button's place, so its progress is at the top of the page.
                RunningJobs(session: draft.session)
            } else {
                produce
            }
            if let failure = draft.failure {
                Label(failure, systemImage: "exclamationmark.triangle").font(.caption).foregroundStyle(.red)
            }
        }
        .frame(width: 300)
        .frame(maxHeight: .infinity, alignment: .top)
        .card()
    }

    @ViewBuilder
    private var produce: some View {
        let blocker = draft.work == .program ? draft.program?.blocker : nil
        Button {
            Task { await draft.start() }
        } label: {
            Image(systemName: "play.fill").font(.title).frame(width: 36, height: 36)
        }
        .buttonStyle(.glassProminent)
        .buttonBorderShape(.circle)
        .tint(.red)
        .controlSize(.extraLarge)
        .disabled(draft.work == nil || blocker != nil || !draft.missing.isEmpty || draft.isWorking)
        .accessibilityLabel(draft.work?.action ?? "Start")
        Text(draft.work?.action ?? "Drop files to start").font(.headline)
        LabeledContent("Event") {
            Button(draft.event?.title ?? "Unassigned", action: pickEvent)
                .accessibilityLabel("Event")
                .accessibilityValue(draft.event?.title ?? "Unassigned")
        }
        if !draft.transcripts.isEmpty, draft.event == nil {
            Text("Choose an event so the transcript goes to YouTube and Facebook with the video.")
                .font(.caption).foregroundStyle(.orange)
        }
        if !draft.transcripts.isEmpty, draft.work == .render {
            Text("A transcript is lined up with the voice: add the recording, or keep the video's sound cleaned.")
                .font(.caption).foregroundStyle(.orange)
        }
        if let blocker {
            Label(blocker, systemImage: "xmark.octagon").font(.caption).foregroundStyle(.red)
        }
        ForEach(draft.missing, id: \.self) { tool in
            Label(advice(tool), systemImage: "exclamationmark.triangle").font(.caption).foregroundStyle(.orange)
        }
        if draft.program?.downloads.contains("jive-vocals") == true || draft.audio?.downloads.contains("jive-vocals") == true {
            Text("The first job downloads jive-vocals (about 53 MB).").font(.caption).foregroundStyle(.secondary)
        }
        if draft.program?.video.kind == .braw {
            let saved = draft.settings != nil && draft.settings == draft.savedLook
            Button(saved ? "Saved as Default" : "Save Look as Default", systemImage: saved ? "checkmark" : "square.and.arrow.down") {
                Task { await draft.saveAsDefault() }
            }
            .buttonStyle(.borderless)
            .disabled(saved)
        }
    }
}

/// What the job makes: the picture's frame and bitrate, the sound's voice and loudness, and where
/// the file goes. Audio alone has only the sound.
private struct OutputCard: View {
    @Bindable var draft: MediaDraft

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            CardTitle("Output")
            HStack(alignment: .top, spacing: 32) {
                if let video = draft.program?.video, draft.work != .audio {
                    picture(video)
                }
                sound
            }
            Divider()
            saveTo
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .card()
    }

    private func picture(_ video: MediaProbeRecord) -> some View {
        VStack(alignment: .leading, spacing: 10) {
            Text("Picture").font(.headline).accessibilityAddTraits(.isHeader)
            if video.kind != .braw, draft.work == .program {
                Picker("Picture", selection: $draft.picture) {
                    Text("Grade").tag(PictureMode.grade)
                    Text("Keep as Is").tag(PictureMode.copy)
                }
                .pickerStyle(.segmented)
            }
            if draft.picture == .grade, let settings = Binding($draft.settings) {
                Picker("Frame", selection: settings.aspect) {
                    Text("16:9 for YouTube").tag(OutputAspect.widescreen)
                    Text("Whole Picture").tag(OutputAspect.native)
                }
                .pickerStyle(.segmented)
                StepperRow(
                    "Bitrate",
                    value: "\(settings.wrappedValue.bitrateMbps) Mbps",
                    choices: Array(stride(from: UInt32(10), through: 400, by: 10)),
                    selection: settings.bitrateMbps
                )
                if let crop = draft.crop {
                    LabeledContent("Size", value: "\(crop.output.width) × \(crop.output.height) HEVC 10-bit")
                }
            }
        }
        .frame(maxWidth: .infinity, alignment: .topLeading)
    }

    /// Which voice channel and how loud, or what to do with a video's own sound.
    private var sound: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text("Sound").font(.headline).accessibilityAddTraits(.isHeader)
            if draft.program != nil, draft.audioPaths.isEmpty {
                Picker("Video's Sound", selection: $draft.sound) {
                    Text("Cleaned").tag(VideoSoundChoice.cleaned)
                    Text("As Recorded").tag(VideoSoundChoice.asRecorded)
                    Text("None").tag(VideoSoundChoice.none)
                }
                .pickerStyle(.segmented)
            }
            if draft.work == .render, draft.sound == .asRecorded, let settings = Binding($draft.settings),
               let channels = draft.program?.video.audio?.channels, channels > 1 {
                StepperRow(
                    "Channels",
                    value: settings.wrappedValue.audioChannels.map(String.init).joined(separator: " + "),
                    choices: channelChoices(channels),
                    selection: settings.audioChannels
                )
            }
            if draft.work != .render, let settings = Binding($draft.audioSettings) {
                let channels = draft.program?.audio?.channels ?? draft.audio?.channels ?? draft.program?.video.audio?.channels ?? 1
                if channels > 1 {
                    StepperRow(
                        "Voice",
                        value: settings.wrappedValue.channels.first.map { "Channel \($0)" } ?? "All Mixed",
                        choices: [[]] + (1...channels).map { [$0] },
                        selection: settings.channels
                    )
                }
                StepperRow(
                    "Loudness",
                    value: "\(Int(settings.wrappedValue.targetLufs)) LUFS\(settings.wrappedValue.targetLufs == -14 ? " (YouTube)" : "")",
                    choices: Array(stride(from: Float(-24), through: -9, by: 1)),
                    selection: settings.targetLufs
                )
            }
        }
        .frame(maxWidth: .infinity, alignment: .topLeading)
    }

    private func channelChoices(_ count: UInt32) -> [[UInt32]] {
        stride(from: UInt32(1), to: count, by: 2).map { [$0, $0 + 1] } + (1...count).map { [$0] }
    }

    /// Where the finished file goes: a saved setting, so it stays for the next job too.
    private var saveTo: some View {
        HStack(spacing: 10) {
            Text(draft.work == .audio ? "Save Audio To" : "Save To").foregroundStyle(.secondary)
            Label(folderText, systemImage: "folder").lineLimit(1).truncationMode(.middle).help(draft.saveFolder)
            Spacer()
            if !draft.saveFolder.isEmpty {
                Button("Use Default Folder") { Task { await draft.setSaveFolder("") } }.buttonStyle(.borderless)
            }
            Button("Choose…") {
                if let url = chooseFolder() { Task { await draft.setSaveFolder(url.path) } }
            }
        }
    }

    private var folderText: String {
        if !draft.saveFolder.isEmpty { return URL(fileURLWithPath: draft.saveFolder).lastPathComponent }
        return draft.work == .audio ? "Music › Metocast Audio" : "Next to the video, in Metocast Renders"
    }
}

/// Each transcript's language; the job lines it up with the voice and sends it with the uploads.
private struct SubtitlesCard: View {
    @Bindable var draft: MediaDraft

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            CardTitle("Subtitles")
            ForEach($draft.transcripts, id: \.path) { $transcript in
                LabeledContent(URL(fileURLWithPath: transcript.path).lastPathComponent) {
                    Picker("Language", selection: $transcript.language) {
                        Text("Hungarian").tag("hu")
                        Text("English").tag("en")
                    }
                    .labelsHidden()
                    .fixedSize()
                }
            }
            Text("Timed to the video or to the recording: Metocast matches the lines with the voice, saves an .srt beside the video, and sends it with the YouTube and Facebook uploads.")
                .font(.caption)
                .foregroundStyle(.secondary)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .card()
    }
}

/// Middle Control's parameter row: the label, − and + buttons, and the value.
private struct StepperRow<Value: Equatable>: View {
    let title: String
    let value: String
    let choices: [Value]
    @Binding var selection: Value

    init(_ title: String, value: String, choices: [Value], selection: Binding<Value>) {
        self.title = title
        self.value = value
        self.choices = choices
        _selection = selection
    }

    var body: some View {
        let at = choices.firstIndex(of: selection)
        HStack(spacing: 8) {
            Text(title).foregroundStyle(.secondary).frame(minWidth: 70, alignment: .trailing)
            Button("Less", systemImage: "minus") { step(at, -1) }
                .disabled(at == 0)
            Button("More", systemImage: "plus") { step(at, 1) }
                .disabled(at == choices.count - 1)
            Text(value).monospacedDigit()
            Spacer()
        }
        .labelStyle(.iconOnly)
        .buttonStyle(.bordered)
        // A real stepper to VoiceOver; the −/+ row alone came across as an unknown element.
        .accessibilityRepresentation {
            Stepper(title) { step(at, 1) } onDecrement: { step(at, -1) }
                .accessibilityValue(value)
        }
    }

    private func step(_ at: Int?, _ by: Int) {
        let next = (at ?? 0) + by
        if choices.indices.contains(next) { selection = choices[next] }
    }
}
#endif
