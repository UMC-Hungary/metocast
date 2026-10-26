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
            return String(localized: "Today, \(formatted(date: .omitted, time: .shortened))")
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

/// Launching with `-MetocastRedact YES` swaps what identifies this Mac (computer name, LAN
/// address, token) for placeholders on screen, so documentation screenshots are safe to commit.
/// `scripts/docs-screenshot.sh` sets it; nothing stored or sent changes.
enum Redaction {
    static let isOn = UserDefaults.standard.bool(forKey: "MetocastRedact")
    static let host = "studio-mac"
    /// A documentation address (RFC 5737), the one the server's own redaction uses.
    static let address = "192.0.2.10"
}

/// Display names and symbols for the connectors the core reports.
struct ConnectorInfo {
    let id: String

    var name: String {
        switch id {
        case "obs": "OBS"
        case "atem": "ATEM"
        case "blackmagic-camera": String(localized: "Camera")
        case "middlecontrol": "Middle Control"
        case "rodecaster": "RØDECaster"
        case "vmix": "vMix"
        case "youtube": "YouTube"
        case "facebook": "Facebook"
        case "broadlink": "Broadlink"
        case "keynote": "Keynote"
        case "discord": "Discord"
        case "szentiras": "Szentírás"
        case "openai": "OpenAI"
        case "mcp": String(localized: "AI Agents")
        case "media": String(localized: "Media")
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
        case "openai": "captions.bubble"
        case "mcp": "sparkles"
        default: "puzzlepiece.extension"
        }
    }

    /// Connectors that sign in to an account rather than connect to a device.
    var signsIn: Bool { id == "youtube" || id == "facebook" }

    var category: ConnectorCategory {
        switch id {
        case "atem", "blackmagic-camera", "obs", "vmix", "rodecaster": .production
        case "middlecontrol", "broadlink": .control
        case "keynote", "szentiras": .presentation
        case "youtube", "facebook", "discord": .publishing
        case "openai", "mcp": .services
        default: .other
        }
    }

    static func sorted(_ statuses: [String: ConnectorStatusRecord]) -> [ConnectorStatusRecord] {
        statuses.values.sorted { ConnectorInfo(id: $0.connector).name < ConnectorInfo(id: $1.connector).name }
    }
}

/// How the Connectors page groups connectors, in display order.
enum ConnectorCategory: CaseIterable {
    case production, control, presentation, publishing, services, other

    var title: String {
        switch self {
        case .production: String(localized: "Video & Audio")
        case .control: String(localized: "Controllers & Room")
        case .presentation: String(localized: "Presentation")
        case .publishing: String(localized: "Streaming & Sharing")
        case .services: String(localized: "AI Services")
        case .other: String(localized: "Other")
        }
    }

    var footer: String {
        switch self {
        case .production: String(localized: "Switchers, cameras, recording and the audio mixer.")
        case .control: String(localized: "Control surfaces and infrared remotes for the room's equipment.")
        case .presentation: String(localized: "Slides and Bible verses shown on screen.")
        case .publishing: String(localized: "Where streams, recordings and announcements go.")
        case .services: String(localized: "Transcription, captions and AI agents.")
        case .other: ""
        }
    }
}

extension ConnectorStatusCode {
    var title: String {
        switch self {
        case .connected: String(localized: "Connected")
        case .connecting: String(localized: "Connecting")
        case .disconnected: String(localized: "Off")
        case .error: String(localized: "Error")
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

/// A status in words. YouTube and Facebook report sign-in problems as codes in `message`; on their
/// own those read as a red "Error" and a raw `login_required`.
extension ConnectorStatusRecord {
    private var code: String? { status == .error ? message : nil }

    /// Set up, but no account signed in or its sign-in expired: worth showing wherever it matters.
    var needsSignIn: Bool { code == "login_required" || code == "relogin_required" }

    var title: String {
        switch code {
        case "credentials_required": String(localized: "Not set up")
        case "login_required": String(localized: "Not signed in")
        case "relogin_required": String(localized: "Sign-in expired")
        default: status == .connected && ConnectorInfo(id: connector).signsIn ? String(localized: "Signed in") : status.title
        }
    }

    var symbol: String {
        if code == "credentials_required" { return "minus.circle" }
        return needsSignIn ? "person.crop.circle.badge.exclamationmark" : status.symbol
    }

    var color: Color {
        if code == "credentials_required" { return .secondary }
        return needsSignIn ? .orange : status.color
    }

    /// The server's message, unless the title already says it.
    var detail: String? {
        switch code {
        case "credentials_required", "login_required", "relogin_required": nil
        default: message.flatMap { $0.isEmpty ? nil : ServerCode.text($0) }
        }
    }
}

/// The server answers with codes such as `rodecaster_not_connected` or
/// `recording_directory_not_found: /path`. This puts them in the app's language; a code it
/// doesn't know reads as words, and text that isn't a code passes through.
enum ServerCode {
    static func text(_ message: String) -> String {
        let code = String(message.prefix { $0.isASCII && ($0.isLowercase || $0.isNumber || $0 == "_") })
        let rest = message.dropFirst(code.count)
        guard code.contains("_"), rest.isEmpty || rest.first == ":" || rest.first == ";" else { return message }
        let detail = rest.dropFirst().trimmingCharacters(in: .whitespaces)
        // "recording_start_failed: rodecaster_multitrack_endpoint_missing: …" says why in its detail.
        let cause = text(detail)
        if cause != detail { return cause }
        if let words = known(code, detail: detail) { return words }
        // ponytail: rare internal codes read as English words; give one a case when it shows up.
        let words = code.replacing("_", with: " ").capitalizedFirst
        return detail.isEmpty ? words : "\(words) (\(detail))"
    }

    private static func known(_ code: String, detail: String) -> String? {
        switch code {
        case "recording_directory_not_found":
            detail.isEmpty
                ? String(localized: "The recording folder doesn't exist on the server's Mac. Choose one with the folder button.")
                : String(localized: "The recording folder doesn't exist on the server's Mac: \(detail). Choose one with the folder button.")
        case "recording_directory_not_writable", "recording_directory_unavailable":
            String(localized: "Metocast can't write to the recording folder. Choose another one with the folder button.")
        case "rodecaster_audio_recording_disabled":
            String(localized: "Audio recording is off. Turn on Record Audio and choose a folder in Settings → Connectors → RØDECaster.")
        case "rodecaster_multitrack_endpoint_missing":
            String(localized: "The RØDECaster's multitrack USB audio isn't reaching this Mac. Connect USB 1 and enable multitrack.")
        case "rodecaster_multitrack_endpoint_ambiguous":
            String(localized: "More than one RØDECaster multitrack audio device is connected to this Mac. Disconnect the extra one.")
        case "rodecaster_no_media":
            String(localized: "No SD card or USB drive in the RØDECaster.")
        case "camera_no_media":
            String(localized: "There's no card or SSD in the camera.")
        case "blackmagic_camera_not_connected":
            String(localized: "The camera isn't reachable. Check that it's on and on the network.")
        case "camera_stream_held_by_middlecontrol":
            String(localized: "The camera's stream is Middle Control's camera feed, not a broadcast. Stop it in Middle Control.")
        case "camera_ignored_command":
            String(localized: "The camera took the command but nothing changed. Check the camera; Middle Control may be controlling it.")
        case "middlecontrol_ignored_command":
            String(localized: "Middle Control didn't report the change. Check the camera in Middle Control.")
        case "rodecaster_not_connected", "obs_not_connected", "atem_not_connected", "middlecontrol_not_connected":
            String(localized: "\(ConnectorInfo(id: String(code.dropLast("_not_connected".count)).replacing("_", with: "-")).name) isn't connected right now.")
        case "atem_timeout", "atem_no_response", "atem_no_ack":
            String(localized: "The ATEM didn't answer. Check its network connection.")
        case "middlecontrol_timeout":
            String(localized: "Middle Control didn't answer. Check its network connection.")
        case "atem_recording_unsupported":
            String(localized: "This ATEM model can't record.")
        case "atem_streaming_unsupported":
            String(localized: "This ATEM model can't stream.")
        case "atem_unknown_input":
            String(localized: "The ATEM has no such input.")
        case "transcription_not_configured":
            String(localized: "Add an OpenAI API key in Settings → Connectors.")
        case "transcription_unavailable":
            String(localized: "Transcription isn't available right now.")
        case "recording_sources_required":
            String(localized: "Choose at least one audio source to record.")
        case "recording_sources_must_be_unique":
            String(localized: "Each audio source can be chosen only once.")
        case "configured_rodecaster_source_is_unavailable":
            String(localized: "A chosen RØDECaster source isn't available.")
        case "main_mix_cannot_be_combined_with_fader_sources":
            String(localized: "The main mix can't be recorded together with fader sources.")
        case "multichannel_flac_supports_at_most_8_channels":
            String(localized: "One FLAC file holds at most 8 channels. Record to separate files instead.")
        case "recording_file_already_exists":
            String(localized: "A recording with this name already exists.")
        case "recording_event_unavailable", "event_not_found":
            String(localized: "The event wasn't found.")
        case "recording_start_failed", "recording_stream_failed", "recording_stream_open_failed",
             "recording_stream_start_failed", "audio_stream_failed", "audio_stream_open_failed",
             "audio_stream_start_failed", "audio_discovery_failed", "audio_device_discovery_failed",
             "audio_device_identity_failed", "audio_thread_unavailable":
            String(localized: "The RØDECaster's audio couldn't be opened on this Mac.")
        case "recording_writer_unavailable", "recording_writer_panicked", "recording_metadata_failed",
             "recording_stop_failed", "flac_encode_failed", "flac_encoder_init_failed",
             "flac_metadata_failed", "flac_rename_failed", "flac_sync_failed":
            String(localized: "The recording file couldn't be written.")
        case "credentials_required": String(localized: "Not set up")
        case "login_required", "not_authenticated": String(localized: "Not signed in")
        case "relogin_required": String(localized: "Sign-in expired")
        case "already_live": String(localized: "It's already live.")
        case "not_live": String(localized: "It isn't live.")
        case "broadcast_ended": String(localized: "This broadcast has ended. Schedule a new one.")
        default: nil
        }
    }
}

private extension String {
    var capitalizedFirst: String { prefix(1).uppercased() + dropFirst() }
}

extension PresenterStateRecord {
    var fileName: String {
        filePath.map { URL(filePath: $0).deletingPathExtension().lastPathComponent } ?? String(localized: "Presentation")
    }
}

extension EventConnectionRecord {
    /// YouTube opens the broadcast's live control room in YouTube Studio, where it's run, not its
    /// public watch page; Facebook opens its event.
    var link: URL? {
        if platform == "youtube", let id = externalId, !id.isEmpty {
            return URL(string: "https://studio.youtube.com/video/\(id)/livestreaming")
        }
        return (eventUrl ?? streamUrl).flatMap(URL.init(string:))
    }

    var platformName: String {
        switch platform {
        case "youtube": "YouTube"
        case "facebook": "Facebook"
        default: platform.capitalized
        }
    }
}
