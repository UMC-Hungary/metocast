import SwiftUI
import Metocast

extension EventConnectionRecord {
    /// A broadcast exists on the platform: something to show, whatever its state.
    var hasBroadcast: Bool {
        externalId.map { !$0.isEmpty } == true && !["not_scheduled", "failed"].contains(scheduleStatus)
    }

    var canGoLive: Bool { hasBroadcast && scheduleStatus == "scheduled" }
    var canEnd: Bool { ["starting", "live"].contains(scheduleStatus) }

    /// "Scheduled", "Starting", "Live", "Ended".
    var stateName: String { scheduleStatus.replacing("_", with: " ").localizedCapitalized }

    var stateTint: Color? {
        switch scheduleStatus {
        case "live": .red
        case "starting": .orange
        default: nil
        }
    }
}

/// One broadcast: the platform's glyph and its state, on glass tinted by that state.
struct BroadcastBadge: View {
    let connection: EventConnectionRecord

    var body: some View {
        HStack(spacing: 4) {
            Image(systemName: ConnectorInfo(id: connection.platform).icon)
            if connection.scheduleStatus == "starting" {
                ProgressView().controlSize(.mini)
            } else if connection.scheduleStatus == "live" {
                Circle().fill(.red).frame(width: 6, height: 6)
                    .symbolEffect(.pulse)
            }
            Text(connection.stateName)
        }
        .font(.caption.weight(.medium))
        .foregroundStyle(connection.scheduleStatus == "ended" ? AnyShapeStyle(.secondary) : AnyShapeStyle(.primary))
        .padding(.horizontal, 8)
        .padding(.vertical, 4)
        .glassEffect(connection.stateTint.map { .regular.tint($0.opacity(0.35)) } ?? .regular, in: .capsule)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("\(connection.platformName), \(connection.stateName)")
    }
}

/// Every broadcast of an event, side by side on one glass container.
struct BroadcastBadges: View {
    let connections: [EventConnectionRecord]

    var body: some View {
        let shown = connections.filter(\.hasBroadcast)
        if !shown.isEmpty {
            GlassEffectContainer(spacing: 6) {
                HStack(spacing: 6) {
                    ForEach(shown, id: \.platform) { BroadcastBadge(connection: $0) }
                }
            }
        }
    }
}

/// Going live or ending one platform's broadcast, waiting for the operator's confirmation.
struct BroadcastAction: Identifiable {
    let eventId: String
    let connection: EventConnectionRecord
    let start: Bool

    var id: String { "\(eventId)-\(connection.platform)-\(start)" }
    var title: String {
        start ? "Go live on \(connection.platformName) now?" : "End the \(connection.platformName) broadcast?"
    }
    var message: String {
        start ? "Viewers will see the stream right away." : "Viewers will see it end, and it can't be restarted."
    }
    var verb: String { start ? "Go Live" : "End Broadcast" }
}

/// The Go Live / End items for an event's broadcasts, for a context menu, swipe actions or a form.
struct BroadcastButtons: View {
    let eventId: String
    let connections: [EventConnectionRecord]
    @Binding var pending: BroadcastAction?

    var body: some View {
        ForEach(connections.filter { $0.canGoLive || $0.canEnd }, id: \.platform) { connection in
            if connection.canEnd {
                Button("End \(connection.platformName) Broadcast", systemImage: "stop.circle", role: .destructive) {
                    pending = BroadcastAction(eventId: eventId, connection: connection, start: false)
                }
            } else {
                Button("Go Live on \(connection.platformName)", systemImage: "dot.radiowaves.left.and.right") {
                    pending = BroadcastAction(eventId: eventId, connection: connection, start: true)
                }
                .tint(.red)
            }
        }
    }
}

extension View {
    /// Asks before `pending` runs, runs it, and explains a refusal.
    func confirmingBroadcast(_ pending: Binding<BroadcastAction?>, session: MetocastSession?) -> some View {
        modifier(BroadcastConfirmation(pending: pending, session: session))
    }
}

private struct BroadcastConfirmation: ViewModifier {
    @Binding var pending: BroadcastAction?
    let session: MetocastSession?
    @State private var failure: String?

    func body(content: Content) -> some View {
        content
            .confirmationDialog(
                pending?.title ?? "",
                isPresented: Binding { pending != nil } set: { if !$0 { pending = nil } },
                titleVisibility: .visible,
                presenting: pending
            ) { action in
                Button(action.verb, role: action.start ? nil : .destructive) {
                    Task { await run(action) }
                }
                .buttonStyle(.glassProminent)
                Button("Cancel", role: .cancel) {}
            } message: { action in
                Text(action.message)
            }
            .alert(
                "Couldn't Change the Broadcast",
                isPresented: Binding { failure != nil } set: { if !$0 { failure = nil } }
            ) {
                Button("OK", role: .cancel) {}
            } message: {
                Text(failure ?? "")
            }
    }

    private func run(_ action: BroadcastAction) async {
        guard let session else { return }
        do {
            try await session.eventStream(eventId: action.eventId, platform: action.connection.platform, start: action.start)
        } catch {
            failure = Self.explain(error, platform: action.connection.platformName)
        }
    }

    private static func explain(_ error: Error, platform: String) -> String {
        guard case AppleError.Server(let code) = error else { return error.metocastMessage }
        return switch code {
        case "encoder_not_streaming": "No video is reaching \(platform) yet. Start OBS (or your encoder) and try again."
        case "not_authenticated": "Sign in to \(platform) under Connectors first."
        case "not_scheduled": "This event has no \(platform) broadcast yet."
        case "already_live": "It's already live."
        case "not_live": "It isn't live."
        case "broadcast_ended": "This broadcast has ended. Schedule a new one."
        case "youtube_testing_timeout": "YouTube didn't get the stream ready in time. Try again."
        default: code
        }
    }
}
