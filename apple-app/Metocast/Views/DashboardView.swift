import SwiftUI
import Metocast

struct DashboardView: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 24) {
                ConnectionHeader()
                if let session = model.session {
                    let outputs = session.outputs
                    if !outputs.isEmpty {
                        section("Live") {
                            OnAirCard(outputs: outputs) { model.pane = .live }
                        }
                    }
                    section("Up Next") {
                        if let event = session.upNext {
                            NavigationLink(value: event) {
                                EventRow(event: event)
                                    .padding(16)
                                    .contentShape(.rect)
                            }
                            .buttonStyle(.plain)
                        } else {
                            Text("No upcoming events")
                                .foregroundStyle(.secondary)
                                .frame(maxWidth: .infinity, alignment: .leading)
                                .padding(16)
                        }
                    }
                    if !session.connectors.isEmpty {
                        VStack(alignment: .leading, spacing: 12) {
                            Text("Connectors").font(.title2.bold())
                            LazyVGrid(columns: [GridItem(.adaptive(minimum: 160), spacing: 12)], spacing: 12) {
                                ForEach(ConnectorInfo.sorted(session.connectors), id: \.connector) { status in
                                    ConnectorCard(status: status)
                                }
                            }
                        }
                    }
                    section("Presentation") {
                        PresenterSummary(state: session.presenter)
                            .padding(16)
                    }
                }
            }
            .padding()
        }
        #if os(iOS)
        .background(Color.groupedBackground)
        #endif
        .navigationTitle("Dashboard")
        .navigationDestination(for: EventSummaryRecord.self) { EventDetailView(summary: $0) }
        .refreshable { await model.session?.refresh() }
    }

    private func section(_ title: String, @ViewBuilder content: () -> some View) -> some View {
        VStack(alignment: .leading, spacing: 12) {
            Text(title).font(.title2.bold())
            content().card()
        }
    }
}

/// Where this device's server is and whether it's reachable, with a retry when it isn't.
private struct ConnectionHeader: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(caption)
                .font(.title3)
                .foregroundStyle(.secondary)
            Text(title)
                .font(.largeTitle.bold())
                .lineLimit(1)
                .minimumScaleFactor(0.6)
            HStack(spacing: 8) {
                if status.isBusy {
                    ProgressView().controlSize(.small)
                } else {
                    Image(systemName: status.symbol).foregroundStyle(status.color)
                }
                Text(status.text)
                    .foregroundStyle(.secondary)
                if status.canRetry {
                    Button("Try Again") { Task { await model.retry() } }
                        .buttonStyle(.borderless)
                }
            }
            .font(.subheadline)
        }
    }

    private var caption: String {
        model.mode == .server ? "Hosting on this Mac" : "Connected server"
    }

    private var title: String {
        #if os(macOS)
        if model.mode == .server {
            return ServerAddress.displayName(model.server.networkURL)
        }
        #endif
        return model.session.map { ServerAddress.displayName($0.baseURL) } ?? "Metocast"
    }

    private var status: ConnectionStatus {
        #if os(macOS)
        switch model.server.state {
        case .starting(let message): return .init(text: message, isBusy: true)
        case .failed(let message): return .init(text: message, symbol: "xmark.octagon.fill", color: .red, canRetry: true)
        case .stopped where model.mode == .server: return .init(text: "Server stopped", symbol: "stop.circle", canRetry: true)
        case .running, .stopped: break
        }
        #endif
        switch model.session?.status {
        case .connected: return .init(text: "Connected", symbol: "checkmark.circle.fill", color: .green)
        case .connecting, nil: return .init(text: "Connecting…", isBusy: true)
        case .reconnecting: return .init(text: "Reconnecting…", isBusy: true)
        case .unreachable: return .init(text: "Can't reach the server", symbol: "wifi.exclamationmark", color: .orange, canRetry: true)
        case .unauthorized: return .init(text: "The server rejected this device's token. Remove access in Settings and connect again.", symbol: "lock.fill", color: .red)
        case .closed: return .init(text: "Disconnected", symbol: "bolt.horizontal.circle", canRetry: true)
        }
    }
}

private struct ConnectionStatus {
    var text: String
    var symbol = "circle"
    var color = Color.secondary
    var isBusy = false
    var canRetry = false
}

struct EventRow: View {
    let event: EventSummaryRecord

    var body: some View {
        HStack(spacing: 12) {
            #if os(macOS)
            Image(systemName: "calendar")
                .font(.title2)
                .foregroundStyle(.tint)
                .frame(width: 32)
            #else
            RoundedRectangle(cornerRadius: 2)
                .fill(event.isCompleted ? AnyShapeStyle(.secondary) : AnyShapeStyle(.tint))
                .frame(width: 4, height: 36)
            #endif
            VStack(alignment: .leading, spacing: 2) {
                Text(event.title)
                Text([event.date?.eventDisplay, event.speaker.isEmpty ? nil : event.speaker]
                    .compactMap(\.self).joined(separator: " · "))
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
            }
            Spacer()
        }
    }
}

private struct ConnectorCard: View {
    let status: ConnectorStatusRecord

    var body: some View {
        let info = ConnectorInfo(id: status.connector)
        VStack(alignment: .leading, spacing: 8) {
            Label(info.name, systemImage: info.icon)
                .font(.subheadline)
                .foregroundStyle(.secondary)
            Label(status.status.title, systemImage: status.status.symbol)
                .font(.headline)
                .foregroundStyle(status.status.color)
            if let message = status.message {
                Text(message)
                    .font(.footnote)
                    .foregroundStyle(.secondary)
                    .lineLimit(2)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding()
        .card()
    }
}

/// What's streaming or recording right now; opens Live.
private struct OnAirCard: View {
    let outputs: [LiveOutput]
    let open: () -> Void

    var body: some View {
        let isLive = outputs.contains { $0.kind == .stream && $0.isOn }
        let running = outputs.filter { $0.state != .off }
        Button(action: open) {
            VStack(alignment: .leading, spacing: 10) {
                HStack {
                    Label(isLive ? "On Air" : "Off Air", systemImage: MetocastPane.live.icon)
                        .font(.headline)
                        .foregroundStyle(isLive ? AnyShapeStyle(.red) : AnyShapeStyle(.secondary))
                        .symbolEffect(.pulse, isActive: isLive)
                    Spacer()
                    #if os(iOS)
                    Image(systemName: "chevron.forward")
                        .foregroundStyle(.tertiary)
                    #endif
                }
                if running.isEmpty {
                    Text("Nothing is streaming or recording.")
                        .foregroundStyle(.secondary)
                }
                ForEach(running) { output in
                    LabeledContent(output.name) {
                        HStack(spacing: 4) {
                            Text(output.stateText)
                            if let since = output.since {
                                Text(since, style: .timer).monospacedDigit()
                            }
                        }
                        .foregroundStyle(output.color)
                    }
                }
            }
            .padding(16)
            .contentShape(.rect)
        }
        .buttonStyle(.plain)
        .help("Open Live")
    }
}

private struct PresenterSummary: View {
    let state: PresenterStateRecord?

    var body: some View {
        HStack(spacing: 12) {
            Image(systemName: state?.loaded == true ? "play.rectangle.fill" : "play.rectangle")
                .font(.title2)
                .foregroundStyle(.tint)
            if let state, state.loaded {
                VStack(alignment: .leading, spacing: 2) {
                    Text(state.fileName)
                    Text("Slide \(state.currentSlide) of \(state.totalSlides)\(state.muted ? " · Blacked out" : "")")
                        .font(.subheadline)
                        .foregroundStyle(.secondary)
                }
            } else {
                Text("No presentation open")
                    .foregroundStyle(.secondary)
            }
            Spacer()
        }
    }
}

extension View {
    @ViewBuilder
    func card() -> some View {
        #if os(macOS)
        GroupBox { self }
        #else
        background(Color.groupedSecondary, in: RoundedRectangle(cornerRadius: 12))
        #endif
    }
}
