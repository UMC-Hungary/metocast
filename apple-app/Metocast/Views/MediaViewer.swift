#if os(macOS)
import Metocast
import SwiftUI

/// The black viewer, laid out like Middle Control's LiveView: pills on top, the original left of
/// a draggable split line and the result right of it, the 16:9 frame a render keeps (moved with
/// the slider on the right), a bar of setting tiles, and the timeline.
struct MediaViewer: View {
    @Bindable var draft: MediaDraft
    let video: MediaProbeRecord
    let chooseLUT: () -> Void
    @State private var split = 0.5
    @State private var tile: Tile?

    var body: some View {
        VStack(spacing: 12) {
            HStack(spacing: 8) {
                Pill(video.fileName)
                Pill("\(video.width) × \(video.height) · \(fps) fps\(video.kind == .braw ? " · BRAW" : "")")
                if let timecode = video.startTimecode { Pill("TC \(timecode)") }
                Spacer()
                if draft.isPreviewing { ProgressView().controlSize(.small) }
            }
            HStack(spacing: 12) {
                picture
                if let crop = draft.crop, crop.active.height > crop.output.height, draft.settings?.aspect == .widescreen {
                    CropSlider(crop: crop, shift: Binding(
                        get: { Int(draft.settings?.cropShiftY ?? 0) },
                        set: { draft.settings?.cropShiftY = Int32($0) }
                    ))
                }
            }
            if let failure = draft.previewFailure {
                Label(failure, systemImage: "exclamationmark.triangle").font(.caption).foregroundStyle(.orange)
            }
            if let tile, let settings = Binding($draft.settings) {
                TileControl(tile: tile, settings: settings, video: video, chooseLUT: chooseLUT, useLook: { url in
                    Task { await draft.add([url]) }
                }) { self.tile = nil }
            }
            if let settings = Binding($draft.settings) {
                ScrollView(.horizontal, showsIndicators: false) {
                    HStack(spacing: 6) {
                        ForEach(Tile.allCases.filter { $0.applies(to: video) }, id: \.self) { item in
                            ParameterTile(
                                label: item.title,
                                spokenLabel: item.spokenTitle,
                                value: item.value(settings.wrappedValue, video: video),
                                isSelected: tile == item
                            ) {
                                switch item {
                                case .highlightRecovery: settings.wrappedValue.highlightRecovery.toggle()
                                case .gamutCompression: settings.wrappedValue.gamutCompression.toggle()
                                default: tile = tile == item ? nil : item
                                }
                            }
                        }
                    }
                }
            }
            Timeline(frame: $draft.frame, video: video)
        }
        .padding(16)
        .background(.black, in: RoundedRectangle(cornerRadius: 18))
        .environment(\.colorScheme, .dark)
    }

    private var fps: String {
        (Double(video.fpsNum) / Double(video.fpsDen)).formatted(.number.precision(.fractionLength(0...3)))
    }

    private var picture: some View {
        let active = draft.crop?.active ?? video.activeArea
        return ZStack {
            Color.black
            still(draft.result)
            still(draft.original)
                .mask(alignment: .leading) {
                    GeometryReader { geometry in
                        Rectangle().frame(width: geometry.size.width * split)
                    }
                }
        }
        .aspectRatio(CGFloat(active.width) / CGFloat(max(active.height, 1)), contentMode: .fit)
        .overlay {
            GeometryReader { geometry in
                if let crop = draft.crop, crop.output != crop.active {
                    CropShade(crop: crop, size: geometry.size)
                }
                Rectangle()
                    .fill(.white)
                    .frame(width: 2, height: geometry.size.height)
                    .position(x: geometry.size.width * split, y: geometry.size.height / 2)
                Color.clear
                    .contentShape(Rectangle())
                    .gesture(DragGesture(minimumDistance: 0).onChanged { drag in
                        split = min(max(drag.location.x / geometry.size.width, 0), 1)
                    })
            }
        }
        .overlay(alignment: .topLeading) { Pill(String(localized: "ORIGINAL")).padding(8) }
        .overlay(alignment: .topTrailing) { Pill(String(localized: "RESULT")).padding(8) }
        .frame(maxWidth: .infinity, minHeight: 240, maxHeight: 620)
        // The split line is dragged; VoiceOver and the keyboard move it as a slider.
        .accessibilityRepresentation {
            Slider(value: $split, in: 0...1, step: 0.1) { Text("Original and result") }
                .accessibilityValue("\((split * 100).formatted(.number.precision(.fractionLength(0))))% original")
        }
    }

    @ViewBuilder
    private func still(_ image: NSImage?) -> some View {
        if let image {
            Image(nsImage: image).resizable().interpolation(.high)
        }
    }
}

/// Dims what a 16:9 render cuts away and outlines what it keeps.
private struct CropShade: View {
    let crop: (active: CropRect, output: CropRect)
    let size: CGSize

    var body: some View {
        let scale = size.width / CGFloat(crop.active.width)
        let kept = CGRect(
            x: CGFloat(Int(crop.output.x) - Int(crop.active.x)) * scale,
            y: CGFloat(Int(crop.output.y) - Int(crop.active.y)) * scale,
            width: CGFloat(crop.output.width) * scale,
            height: CGFloat(crop.output.height) * scale
        )
        ZStack {
            Path { path in
                path.addRect(CGRect(origin: .zero, size: size))
                path.addRect(kept)
            }
            .fill(.black.opacity(0.5), style: FillStyle(eoFill: true))
            Path { $0.addRect(kept) }.stroke(.white.opacity(0.8), lineWidth: 1)
        }
        .allowsHitTesting(false)
    }
}

/// Middle Control's vertical T/W slider, here moving the 16:9 frame up or down.
private struct CropSlider: View {
    let crop: (active: CropRect, output: CropRect)
    @Binding var shift: Int

    var body: some View {
        let spare = Double(crop.active.height - crop.output.height)
        let fraction = min(max((Double(shift) + spare / 2) / spare, 0), 1)
        VStack(spacing: 6) {
            Image(systemName: "chevron.up").font(.caption.bold())
            GeometryReader { geometry in
                ZStack(alignment: .top) {
                    Capsule().fill(.white.opacity(0.18)).frame(width: 6).frame(maxWidth: .infinity)
                    Circle().fill(.white).frame(width: 22, height: 22)
                        .offset(y: fraction * (geometry.size.height - 22))
                }
                .contentShape(Rectangle())
                .gesture(DragGesture(minimumDistance: 0).onChanged { drag in
                    let at = min(max((drag.location.y - 11) / (geometry.size.height - 22), 0), 1)
                    // Even rows, as 4:2:0 video needs.
                    shift = Int(((at - 0.5) * spare / 2).rounded()) * 2
                })
            }
            Image(systemName: "chevron.down").font(.caption.bold())
        }
        .frame(width: 34)
        .padding(.vertical, 8)
        .background(.white.opacity(0.08), in: Capsule())
        .accessibilityRepresentation {
            Stepper("Move the 16:9 frame") { shift += 20 } onDecrement: { shift -= 20 }
                .accessibilityValue("\(shift) pixels down")
        }
    }
}

/// The settings the bar offers, in Middle Control's order of importance.
enum Tile: CaseIterable {
    case iso, whiteBalance, tint, exposure, gamma, gamut, lut, highlightRecovery, gamutCompression

    var title: String {
        switch self {
        case .iso: String(localized: "ISO")
        case .whiteBalance: String(localized: "WB")
        case .tint: String(localized: "TINT")
        case .exposure: String(localized: "EXPOSURE")
        case .gamma: String(localized: "GAMMA")
        case .gamut: String(localized: "GAMUT")
        case .lut: String(localized: "LUT")
        case .highlightRecovery: String(localized: "HIGHLIGHTS")
        case .gamutCompression: String(localized: "GAMUT COMP.")
        }
    }

    /// The title in words, for VoiceOver: "WB" would be spelled out.
    var spokenTitle: String {
        switch self {
        case .iso: String(localized: "ISO")
        case .whiteBalance: String(localized: "White balance")
        case .tint: String(localized: "Tint")
        case .exposure: String(localized: "Exposure")
        case .gamma: String(localized: "Gamma")
        case .gamut: String(localized: "Gamut")
        case .lut: String(localized: "LUT")
        case .highlightRecovery: String(localized: "Highlight recovery")
        case .gamutCompression: String(localized: "Gamut compression")
        }
    }

    /// Video ffmpeg decodes itself only takes a LUT; the rest is Blackmagic RAW development.
    func applies(to video: MediaProbeRecord) -> Bool {
        self == .lut || video.rawOptions != nil
    }

    func value(_ settings: RenderSettings, video: MediaProbeRecord) -> String {
        let shot = video.rawOptions?.asShot
        return switch self {
        case .iso: (settings.iso ?? shot?.iso).map(String.init) ?? "—"
        case .whiteBalance: (settings.kelvin ?? shot?.kelvin).map { "\($0)K" } ?? "—"
        case .tint: (settings.tint ?? shot?.tint).map(String.init) ?? "—"
        case .exposure: (settings.exposure ?? shot?.exposure).map { $0.formatted(.number.precision(.fractionLength(1))) } ?? "—"
        case .gamma: settings.gamma.replacingOccurrences(of: "Blackmagic Design ", with: "")
        case .gamut: settings.gamut.replacingOccurrences(of: "Blackmagic Design", with: "BMD")
        case .lut: switch settings.lutMode {
            case .embedded: String(localized: "Camera")
            case .file: settings.lutPath.map { URL(fileURLWithPath: $0).deletingPathExtension().lastPathComponent } ?? String(localized: "File")
            case .sidecar: String(localized: "Sidecar")
            case .none: String(localized: "Off")
            }
        case .highlightRecovery: settings.highlightRecovery ? String(localized: "ON") : String(localized: "OFF")
        case .gamutCompression: settings.gamutCompression ? String(localized: "ON") : String(localized: "OFF")
        }
    }
}

private struct ParameterTile: View {
    let label: String
    let spokenLabel: String
    let value: String
    let isSelected: Bool
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            VStack(spacing: 3) {
                Text(label).font(.caption.weight(.semibold)).foregroundStyle(.secondary)
                Text(value).font(.headline.monospacedDigit()).lineLimit(1)
            }
            .frame(minWidth: 86)
            .padding(.vertical, 8)
            .padding(.horizontal, 6)
            .overlay {
                RoundedRectangle(cornerRadius: 8).strokeBorder(.white.opacity(isSelected ? 0.9 : 0), lineWidth: 1.5)
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityLabel(spokenLabel)
        .accessibilityValue(value)
        .accessibilityAddTraits(isSelected ? .isSelected : [])
    }
}

/// What a selected tile opens above the bar: a ruler for numbers, a strip of choices for names.
private struct TileControl: View {
    let tile: Tile
    @Binding var settings: RenderSettings
    let video: MediaProbeRecord
    let chooseLUT: () -> Void
    let useLook: (URL) -> Void
    let close: () -> Void

    var body: some View {
        HStack(spacing: 12) {
            control
            Button("Close", systemImage: "xmark", action: close).labelStyle(.iconOnly).buttonStyle(.borderless)
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 8)
        .background(.white.opacity(0.08), in: RoundedRectangle(cornerRadius: 22))
    }

    @ViewBuilder
    private var control: some View {
        let options = video.rawOptions
        let shot = options?.asShot
        switch tile {
        case .iso:
            let isos = (options?.isos ?? []).map { Double($0) }
            RulerDial(title: tile.spokenTitle, options: isos, current: Double(settings.iso ?? shot?.iso ?? 0), majorEvery: 1, isAsShot: settings.iso == nil,
                      label: { "\(Int($0))" }) { settings.iso = $0.map { UInt32($0) } }
        case .whiteBalance:
            RulerDial(title: tile.spokenTitle, options: steps(options?.kelvin, by: 50), current: Double(settings.kelvin ?? shot?.kelvin ?? 5600), majorEvery: 10,
                      isAsShot: settings.kelvin == nil, label: { "\(Int($0))K" }) { settings.kelvin = $0.map { UInt32($0) } }
        case .tint:
            RulerDial(title: tile.spokenTitle, options: steps(options?.tint, by: 1), current: Double(settings.tint ?? shot?.tint ?? 0), majorEvery: 10,
                      isAsShot: settings.tint == nil, label: { "\(Int($0))" }) { settings.tint = $0.map { Int32($0) } }
        case .exposure:
            RulerDial(title: tile.spokenTitle, options: steps(options?.exposure, by: 0.1), current: Double(settings.exposure ?? shot?.exposure ?? 0), majorEvery: 10,
                      isAsShot: settings.exposure == nil, label: { $0.formatted(.number.precision(.fractionLength(1))) }) {
                settings.exposure = $0.map { Float($0) }
            }
        case .gamma:
            ChoiceStrip(choices: (options?.gammas ?? []).map { ($0, $0) }, selected: settings.gamma) { settings.gamma = $0 }
        case .gamut:
            ChoiceStrip(choices: (options?.gamuts ?? []).map { ($0, $0) }, selected: settings.gamut) { settings.gamut = $0 }
        case .lut:
            ChoiceStrip(choices: lutChoices, selected: settings.lutMode) { settings.lutMode = $0 }
            // Saved looks, one click each: a Resolve sidecar sets gamma, gamut and its LUT.
            ForEach(MediaDraft.looks.filter { $0.pathExtension.lowercased() != "sidecar" || video.kind == .braw }, id: \.self) { look in
                let inUse = settings.sidecarPath == look.path || (settings.lutMode == .file && settings.lutPath == look.path)
                if inUse {
                    Button(look.deletingPathExtension().lastPathComponent) { useLook(look) }
                        .buttonStyle(.borderedProminent)
                        .accessibilityAddTraits(.isSelected)
                } else {
                    Button(look.deletingPathExtension().lastPathComponent) { useLook(look) }.buttonStyle(.bordered)
                }
            }
            .buttonBorderShape(.capsule)
            // A Resolve .sidecar or a .cube from Generate LUT.
            Button("Choose Look…", action: chooseLUT)
        case .highlightRecovery, .gamutCompression:
            EmptyView()
        }
    }

    private var lutChoices: [(String, LutMode)] {
        var choices: [(String, LutMode)] = []
        if video.kind == .braw {
            choices.append((String(localized: "Camera: \(video.embeddedLut ?? String(localized: "embedded"))"), .embedded))
        }
        if settings.sidecarPath != nil { choices.append((String(localized: "Resolve Sidecar"), .sidecar)) }
        if let path = settings.lutPath { choices.append((URL(fileURLWithPath: path).lastPathComponent, .file)) }
        choices.append((String(localized: "None"), .none))
        return choices
    }

    /// Every `step` across the SDK's range, or nothing when the clip offers none.
    private func steps(_ range: ValueRange?, by step: Double) -> [Double] {
        guard let range, range.max > range.min else { return [] }
        return stride(from: range.min, through: range.max + step / 2, by: step).map { ($0 / step).rounded() * step }
    }
}

/// Middle Control's value dial: the big value over a tick ruler dragged sideways, and a capsule
/// that returns to what the camera recorded.
private struct RulerDial: View {
    let title: String
    let options: [Double]
    let current: Double
    let majorEvery: Int
    let isAsShot: Bool
    let label: (Double) -> String
    /// `nil`: as shot.
    let pick: (Double?) -> Void
    @State private var dragStart: Int?
    /// Wide enough for a label on every tick when every tick is labelled (ISO).
    private var spacing: CGFloat { majorEvery == 1 ? 46 : 9 }

    var body: some View {
        let index = options.indices.min { abs(options[$0] - current) < abs(options[$1] - current) } ?? 0
        HStack(spacing: 14) {
            VStack(spacing: 0) {
                Text(label(current)).font(.title3.monospacedDigit().weight(.semibold))
                Canvas { context, size in
                    let mid = size.width / 2
                    let reach = Int(mid / spacing)
                    for offset in -reach...reach {
                        let at = index + offset
                        guard options.indices.contains(at) else { continue }
                        let x = mid + CGFloat(offset) * spacing
                        let major = at % majorEvery == 0
                        var tick = Path()
                        tick.move(to: CGPoint(x: x, y: major ? 2 : 8))
                        tick.addLine(to: CGPoint(x: x, y: 18))
                        context.stroke(tick, with: .color(.white.opacity(major ? 0.85 : 0.35)), lineWidth: 1)
                        if major, offset != 0 {
                            context.draw(Text(label(options[at])).font(.caption2).foregroundStyle(.white.opacity(0.6)), at: CGPoint(x: x, y: 30))
                        }
                    }
                    var marker = Path()
                    marker.move(to: CGPoint(x: mid, y: 0))
                    marker.addLine(to: CGPoint(x: mid, y: 20))
                    context.stroke(marker, with: .color(.white), lineWidth: 3)
                }
                .frame(height: 38)
                .contentShape(Rectangle())
                .gesture(DragGesture(minimumDistance: 1)
                    .onChanged { drag in
                        let start = dragStart ?? index
                        dragStart = start
                        let next = min(max(start - Int((drag.translation.width / spacing).rounded()), 0), options.count - 1)
                        if next != index { pick(options[next]) }
                    }
                    .onEnded { _ in dragStart = nil })
            }
            .frame(minWidth: 260, maxWidth: 520)
            .accessibilityRepresentation {
                Stepper(title) { step(index, 1) } onDecrement: { step(index, -1) }
                    .accessibilityValue(isAsShot ? String(localized: "\(label(current)), as shot") : label(current))
            }
            Button("As Shot") { pick(nil) }
                .buttonStyle(.bordered)
                .buttonBorderShape(.capsule)
                .tint(isAsShot ? .accentColor : .secondary)
                .accessibilityAddTraits(isAsShot ? .isSelected : [])
        }
        .disabled(options.isEmpty)
    }

    private func step(_ index: Int, _ by: Int) {
        if options.indices.contains(index + by) { pick(options[index + by]) }
    }
}

/// A row of capsules, one per choice; the chosen one is filled.
private struct ChoiceStrip<Value: Hashable>: View {
    let choices: [(String, Value)]
    let selected: Value
    let choose: (Value) -> Void

    var body: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 6) {
                ForEach(choices, id: \.1) { choice in
                    if choice.1 == selected {
                        Button(choice.0) { choose(choice.1) }.buttonStyle(.borderedProminent).accessibilityAddTraits(.isSelected)
                    } else {
                        Button(choice.0) { choose(choice.1) }.buttonStyle(.bordered)
                    }
                }
            }
            .buttonBorderShape(.capsule)
        }
        .frame(maxWidth: .infinity)
    }
}

/// The scrubber under the picture: frame steps (← →), the position and the length.
private struct Timeline: View {
    @Binding var frame: Double
    let video: MediaProbeRecord

    var body: some View {
        let last = Double(max(video.frameCount, 1) - 1)
        let seconds = frame * Double(video.fpsDen) / Double(video.fpsNum)
        HStack(spacing: 10) {
            Button("Previous Frame", systemImage: "backward.frame.fill") { frame = max(0, frame.rounded() - 1) }
                .keyboardShortcut(.leftArrow, modifiers: [])
            Button("Next Frame", systemImage: "forward.frame.fill") { frame = min(last, frame.rounded() + 1) }
                .keyboardShortcut(.rightArrow, modifiers: [])
            Text(timestamp(seconds)).monospacedDigit().frame(minWidth: 70, alignment: .leading)
            Slider(value: $frame, in: 0...max(last, 1))
                .accessibilityLabel("Position")
                .accessibilityValue(timestamp(seconds))
            Text(durationText(video.durationSeconds)).monospacedDigit().foregroundStyle(.secondary)
        }
        .labelStyle(.iconOnly)
        .buttonStyle(.borderless)
    }
}

/// A small capsule label, like Middle Control's camera and status pills.
struct Pill: View {
    let text: String
    init(_ text: String) { self.text = text }

    var body: some View {
        Text(text)
            .font(.caption.weight(.semibold))
            .lineLimit(1)
            .padding(.horizontal, 10)
            .padding(.vertical, 5)
            .background(.white.opacity(0.14), in: Capsule())
    }
}
#endif
