import Foundation
import SwiftUI
import Metocast

// Presentation helpers for the records the Rust bridge hands over.

extension Date {
    /// Parses the RFC 3339 timestamps the core sends (`2026-01-11T10:00:00+00:00`,
    /// sometimes with fractional seconds).
    init?(rfc3339 text: String) {
        guard let date = RFC3339.whole.date(from: text) ?? RFC3339.fractional.date(from: text) else { return nil }
        self = date
    }

    /// "Today, 10:00" for today and "Sun, 11 Jan, 10:00" otherwise.
    var eventDisplay: String {
        if Calendar.current.isDateInToday(self) {
            return "Today, " + formatted(date: .omitted, time: .shortened)
        }
        return formatted(.dateTime.weekday(.abbreviated).day().month(.abbreviated).hour().minute())
    }
}

private enum RFC3339 {
    static let whole = formatter([.withInternetDateTime])
    static let fractional = formatter([.withInternetDateTime, .withFractionalSeconds])

    private static func formatter(_ options: ISO8601DateFormatter.Options) -> ISO8601DateFormatter {
        let formatter = ISO8601DateFormatter()
        formatter.formatOptions = options
        return formatter
    }
}

extension EventSummaryRecord {
    var date: Date? { Date(rfc3339: dateTime) }
}

extension EventRecord {
    var date: Date? { Date(rfc3339: dateTime) }
}

enum EventSchedule {
    /// Mirrors `metocast_core::events::current_event`: an event that started today within
    /// the last four hours, otherwise the next upcoming one.
    static func current(in events: [EventSummaryRecord], now: Date) -> EventSummaryRecord? {
        let open = events.filter { !$0.isCompleted }
        let running = open
            .compactMap { event in event.date.map { (event, $0) } }
            .filter { _, date in
                Calendar.current.isDate(date, inSameDayAs: now) && date <= now && now.timeIntervalSince(date) <= 4 * 60 * 60
            }
            .max { $0.1 < $1.1 }
        if let running { return running.0 }
        return open
            .compactMap { event in event.date.map { (event, $0) } }
            .filter { $0.1 >= now }
            .min { $0.1 < $1.1 }?.0
    }
}

/// Display names and symbols for the connectors the core reports.
struct ConnectorInfo {
    let id: String

    var name: String {
        switch id {
        case "obs": "OBS"
        case "atem": "ATEM"
        case "blackmagic-camera": "Camera"
        case "middlecontrol": "Middle Control"
        case "rodecaster": "RØDECaster"
        case "vmix": "vMix"
        case "youtube": "YouTube"
        case "facebook": "Facebook"
        case "broadlink": "Broadlink"
        case "keynote": "Keynote"
        case "discord": "Discord"
        case "szentiras": "Szentírás"
        default: id.capitalized
        }
    }

    var icon: String {
        switch id {
        case "obs": "record.circle"
        case "atem": "rectangle.split.2x2"
        case "blackmagic-camera": "video"
        case "middlecontrol": "slider.horizontal.3"
        case "rodecaster": "mic"
        case "vmix": "square.stack.3d.up"
        case "youtube": "play.rectangle"
        case "facebook": "person.2"
        case "broadlink": "dot.radiowaves.left.and.right"
        case "keynote": "play.display"
        case "discord": "bubble.left.and.bubble.right"
        case "szentiras": "book"
        default: "puzzlepiece.extension"
        }
    }

    static func sorted(_ statuses: [String: ConnectorStatusRecord]) -> [ConnectorStatusRecord] {
        statuses.values.sorted { ConnectorInfo(id: $0.connector).name < ConnectorInfo(id: $1.connector).name }
    }
}

extension ConnectorStatusCode {
    var title: String {
        switch self {
        case .connected: "Connected"
        case .connecting: "Connecting"
        case .disconnected: "Off"
        case .error: "Error"
        }
    }

    var symbol: String {
        switch self {
        case .connected: "checkmark.circle.fill"
        case .connecting: "arrow.trianglehead.2.clockwise.rotate.90"
        case .disconnected: "minus.circle"
        case .error: "exclamationmark.triangle.fill"
        }
    }

    var color: Color {
        switch self {
        case .connected: .green
        case .connecting: .orange
        case .disconnected: .secondary
        case .error: .red
        }
    }
}

extension PresenterStateRecord {
    var fileName: String {
        filePath.map { URL(filePath: $0).deletingPathExtension().lastPathComponent } ?? "Presentation"
    }
}

extension EventConnectionRecord {
    var platformName: String {
        switch platform {
        case "youtube": "YouTube"
        case "facebook": "Facebook"
        default: platform.capitalized
        }
    }
}
