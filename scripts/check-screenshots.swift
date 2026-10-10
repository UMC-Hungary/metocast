#!/usr/bin/env swift
// Fails when a documentation screenshot shows something that identifies a machine or person:
// this Mac's computer name, host name or user name, a private LAN address, a path under another
// user's home folder, or a token-shaped string.
//
//   swift scripts/check-screenshots.swift [folder ...]      (default: docs/images)
//
// Reads every PNG with Vision OCR. The name checks use the machine this runs on, so a CI runner
// only checks the generic patterns. Take screenshots with scripts/docs-screenshot.sh, which
// turns on the app's redaction.
import AppKit
import Foundation
import Vision

func output(_ tool: String, _ arguments: [String]) -> String {
    let process = Process()
    process.executableURL = URL(fileURLWithPath: tool)
    process.arguments = arguments
    let pipe = Pipe()
    process.standardOutput = pipe
    process.standardError = FileHandle.nullDevice
    try? process.run()
    process.waitUntilExit()
    return String(decoding: pipe.fileHandleForReading.readDataToEndOfFile(), as: UTF8.self)
        .trimmingCharacters(in: .whitespacesAndNewlines)
}

let names = ["ComputerName", "LocalHostName", "HostName"]
    .map { output("/usr/sbin/scutil", ["--get", $0]) }
    + [NSUserName(), NSFullUserName()]
let own = Set(names.map { $0.lowercased() }).filter { $0.count >= 4 }

let patterns: [(String, String)] = [
    ("LAN address", #"\b(10\.\d{1,3}\.\d{1,3}\.\d{1,3}|192\.168\.\d{1,3}\.\d{1,3}|172\.(1[6-9]|2\d|3[01])\.\d{1,3}\.\d{1,3})\b"#),
    ("token (UUID)", #"[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}"#),
    ("token-like string", #"\b[A-Za-z0-9_\-]{32,}\b"#),
    ("home folder", #"/Users/(?!operator\b)[A-Za-z0-9._\-]+"#),
]

func lines(in url: URL) -> [String] {
    guard let image = NSImage(contentsOf: url),
          let cgImage = image.cgImage(forProposedRect: nil, context: nil, hints: nil)
    else { return [] }
    let request = VNRecognizeTextRequest()
    request.recognitionLevel = .accurate
    request.usesLanguageCorrection = false
    try? VNImageRequestHandler(cgImage: cgImage).perform([request])
    return (request.results ?? []).compactMap { $0.topCandidates(1).first?.string }
}

let folders = CommandLine.arguments.dropFirst().isEmpty ? ["docs/images"] : Array(CommandLine.arguments.dropFirst())
var failures = 0
var checked = 0
for folder in folders {
    let found = (try? FileManager.default.subpathsOfDirectory(atPath: folder)) ?? []
    for relative in found where relative.lowercased().hasSuffix(".png") {
        let path = (folder as NSString).appendingPathComponent(relative)
        checked += 1
        for line in lines(in: URL(fileURLWithPath: path)) {
            let lower = line.lowercased()
            for name in own where lower.contains(name) {
                print("\(path): shows this machine's name or user name (\"\(name)\")")
                failures += 1
            }
            for (label, pattern) in patterns where line.range(of: pattern, options: .regularExpression) != nil {
                print("\(path): shows a \(label)")
                failures += 1
            }
        }
    }
}
print("checked \(checked) screenshot(s), \(failures) problem(s)")
exit(failures == 0 ? 0 : 1)
