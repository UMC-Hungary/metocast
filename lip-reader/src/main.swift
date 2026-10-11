// lip-reader: how open the mouths in a video are, frame by frame, for Metocast's audio/video sync.
//
//   lip-reader [--start SECONDS] [--duration SECONDS] [--max-width PIXELS] VIDEO
//
// Writes one JSON line per frame to stdout:
//   {"t":600.0417,"faces":[{"x":0.41,"y":0.38,"w":0.12,"h":0.18,"open":0.214,"confidence":0.81}]}
// `t` is the frame's presentation time in seconds; `faces` holds up to three faces, largest first,
// with their box (normalised, origin bottom-left) and `open`, the inner lips' height over the
// outer lips' width. Errors go to stderr and the exit status is 1.
//
// Every frame gets a fresh face detection: reusing a box between detections lets the landmarks
// drift as the head moves, and that drift repeats at the detection rate, which ruins the
// correlation with the voice that Metocast runs on these numbers.
import AVFoundation
import Foundation
import Vision

struct Options {
    var video = ""
    var start = 0.0
    var duration: Double?
    var maxWidth = 1920
}

func fail(_ message: String) -> Never {
    FileHandle.standardError.write(Data("lip-reader: \(message)\n".utf8))
    exit(1)
}

func parse(_ arguments: [String]) -> Options {
    var options = Options()
    var rest = arguments.dropFirst()
    func value(_ flag: String) -> String {
        guard let value = rest.popFirst() else { fail("\(flag) needs a value") }
        return value
    }
    while let argument = rest.popFirst() {
        switch argument {
        case "--start":
            guard let start = Double(value(argument)), start >= 0 else { fail("--start must be seconds") }
            options.start = start
        case "--duration":
            guard let duration = Double(value(argument)), duration > 0 else { fail("--duration must be seconds") }
            options.duration = duration
        case "--max-width":
            guard let width = Int(value(argument)), width >= 320 else { fail("--max-width must be at least 320") }
            options.maxWidth = width
        case "-h", "--help":
            print("usage: lip-reader [--start SECONDS] [--duration SECONDS] [--max-width PIXELS] VIDEO")
            exit(0)
        default:
            if argument.hasPrefix("-") || !options.video.isEmpty { fail("unexpected argument \(argument)") }
            options.video = argument
        }
    }
    if options.video.isEmpty { fail("no video given") }
    return options
}

/// How Vision should turn the decoded picture to see it upright.
func orientation(of transform: CGAffineTransform) -> CGImagePropertyOrientation {
    switch (transform.a, transform.b, transform.c, transform.d) {
    case (0, 1, -1, 0): .right
    case (0, -1, 1, 0): .left
    case (-1, 0, 0, -1): .down
    default: .up
    }
}

struct Face: Encodable {
    let x, y, w, h, open, confidence: Double
}

struct Frame: Encodable {
    let t: Double
    let faces: [Face]
}

func faces(in observations: [VNFaceObservation]) -> [Face] {
    observations
        .sorted { $0.boundingBox.width * $0.boundingBox.height > $1.boundingBox.width * $1.boundingBox.height }
        .prefix(3)
        .compactMap { face in
            guard let inner = face.landmarks?.innerLips?.normalizedPoints,
                  let outer = face.landmarks?.outerLips?.normalizedPoints,
                  let innerTop = inner.map(\.y).max(), let innerBottom = inner.map(\.y).min(),
                  let outerLeft = outer.map(\.x).min(), let outerRight = outer.map(\.x).max(),
                  outerRight > outerLeft
            else { return nil }
            let box = face.boundingBox
            return Face(
                x: box.origin.x, y: box.origin.y, w: box.width, h: box.height,
                open: Double((innerTop - innerBottom) / (outerRight - outerLeft)),
                confidence: Double(face.landmarks?.confidence ?? 0)
            )
        }
}

let options = parse(CommandLine.arguments)
let asset = AVURLAsset(url: URL(fileURLWithPath: options.video))
let track: AVAssetTrack
let size: CGSize
let transform: CGAffineTransform
do {
    guard let first = try await asset.loadTracks(withMediaType: .video).first else {
        fail("\(options.video) has no video track")
    }
    track = first
    (size, transform) = try await track.load(.naturalSize, .preferredTransform)
} catch {
    fail("cannot read \(options.video): \(error.localizedDescription)")
}

// The decoder scales the picture down in hardware; faces in a sermon shot stay large enough.
var outputSettings: [String: Any] = [
    kCVPixelBufferPixelFormatTypeKey as String: kCVPixelFormatType_420YpCbCr8BiPlanarFullRange,
]
if Int(size.width) > options.maxWidth {
    let height = (Double(options.maxWidth) * size.height / size.width / 2).rounded() * 2
    outputSettings[kCVPixelBufferWidthKey as String] = options.maxWidth
    outputSettings[kCVPixelBufferHeightKey as String] = Int(height)
}

let reader: AVAssetReader
do {
    reader = try AVAssetReader(asset: asset)
} catch {
    fail("cannot read \(options.video): \(error.localizedDescription)")
}
let start = CMTime(seconds: options.start, preferredTimescale: 600_000)
reader.timeRange = CMTimeRange(
    start: start,
    duration: options.duration.map { CMTime(seconds: $0, preferredTimescale: 600_000) } ?? .positiveInfinity
)
let output = AVAssetReaderTrackOutput(track: track, outputSettings: outputSettings)
output.alwaysCopiesSampleData = false
reader.add(output)
guard reader.startReading() else {
    fail("cannot decode \(options.video): \(reader.error?.localizedDescription ?? "unknown error")")
}

let upright = orientation(of: transform)
let encoder = JSONEncoder()
let stdout = FileHandle.standardOutput
var lines = Data()
var count = 0
while let sample = output.copyNextSampleBuffer() {
    guard let pixels = CMSampleBufferGetImageBuffer(sample) else { continue }
    let time = CMSampleBufferGetPresentationTimeStamp(sample).seconds
    let request = VNDetectFaceLandmarksRequest()
    do {
        try VNImageRequestHandler(cvPixelBuffer: pixels, orientation: upright).perform([request])
    } catch {
        fail("face detection failed at \(time) s: \(error.localizedDescription)")
    }
    let frame = Frame(t: (time * 10_000).rounded() / 10_000, faces: faces(in: request.results ?? []))
    guard let line = try? encoder.encode(frame) else { fail("cannot encode frame \(count)") }
    lines.append(line)
    lines.append(0x0A)
    count += 1
    if count % 24 == 0 {
        stdout.write(lines)
        lines.removeAll(keepingCapacity: true)
    }
}
stdout.write(lines)
if reader.status == .failed {
    fail("decoding stopped after \(count) frames: \(reader.error?.localizedDescription ?? "unknown error")")
}
FileHandle.standardError.write(Data("lip-reader: \(count) frames\n".utf8))
