import SwiftUI
import Metocast

/// Settings → Equipment → Automations: the rules that run inside the server, the master switch,
/// and what they did lately. Every client sees and edits the same rules.
struct AutomationsView: View {
    let session: MetocastSession
    @State private var failure: String?

    var body: some View {
        List {
            Section {
                Toggle(isOn: Binding(
                    get: { session.automationsArmed },
                    set: { armed in Task { await setArmed(armed) } }
                )) {
                    Label("Automations armed", systemImage: session.automationsArmed ? "bolt.fill" : "bolt.slash")
                        .foregroundStyle(session.automationsArmed ? Color.primary : Color.orange)
                }
            } footer: {
                Text(session.automationsArmed
                    ? "Rules fire by themselves when their devices do something."
                    : "Disarmed: nothing fires, for example during a rehearsal. “Run Now” in a rule still works.")
            }

            Section {
                ForEach(session.automations) { rule in
                    NavigationLink {
                        AutomationEditorView(session: session, original: rule)
                            .windowActions()
                    } label: {
                        AutomationRow(session: session, rule: rule)
                    }
                    // Mac lists have no swipe to delete.
                    .contextMenu {
                        Button("Delete", role: .destructive) {
                            Task { await delete([rule.id]) }
                        }
                    }
                }
                .onDelete { offsets in
                    let ids = offsets.map { session.automations[$0].id }
                    Task { await delete(ids) }
                }
            } header: {
                Text("Rules")
            } footer: {
                if !session.automations.isEmpty {
                    Text("A rule fires when any of its triggers happens and does its steps in order.")
                }
            }

            Section {
                NavigationLink {
                    AutomationRunsView(session: session).windowActions()
                } label: {
                    Label("Run History", systemImage: "clock.arrow.circlepath")
                }
                .accessibilityAddTraits(.isButton)
            }

            if let failure {
                Section {
                    Label(failure, systemImage: "exclamationmark.triangle").foregroundStyle(.red)
                }
            }
        }
        .overlay {
            if session.automations.isEmpty {
                ContentUnavailableView {
                    Label("No Automations", systemImage: "bolt")
                } description: {
                    Text("A rule links devices: when a Middle Control preset finishes moving or the pastor's channel goes live, Metocast moves the camera and starts the recorders.")
                } actions: {
                    NavigationLink {
                        AutomationEditorView(session: session, original: nil)
                            .windowActions()
                    } label: {
                        Text("New Automation")
                    }
                    .buttonStyle(.borderedProminent)
                }
            }
        }
        .navigationTitle("Automations")
        .toolbar {
            ToolbarItem(placement: .primaryAction) {
                NavigationLink {
                    AutomationEditorView(session: session, original: nil)
                        .windowActions()
                } label: {
                    Label("New Automation", systemImage: "plus")
                }
            }
        }
        .task { await session.loadAutomations() }
    }

    private func setArmed(_ armed: Bool) async {
        do {
            try await session.setAutomationsArmed(armed)
            failure = nil
        } catch {
            failure = error.metocastMessage
        }
    }

    private func delete(_ ids: [String]) async {
        for id in ids {
            do {
                try await session.deleteAutomation(id: id)
            } catch {
                failure = error.metocastMessage
            }
        }
    }
}

/// A rule's name, what it does in one line, how its last run went, and its own switch.
private struct AutomationRow: View {
    let session: MetocastSession
    let rule: AutomationRecord

    var body: some View {
        HStack(alignment: .top) {
            VStack(alignment: .leading, spacing: 3) {
                Text(rule.name)
                    .foregroundStyle(.primary)
                Text(rule.summary)
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
                    .lineLimit(3)
                if let run = session.lastRun(of: rule.id) {
                    Label {
                        let time = run.date.map { $0.formatted(date: .omitted, time: .shortened) + " · " } ?? ""
                        Text("Last run \(time)\(run.outcomeText)")
                    } icon: {
                        Image(systemName: run.didFail ? "exclamationmark.triangle.fill" : "checkmark.circle")
                    }
                    .font(.caption)
                    .foregroundStyle(run.didFail ? Color.orange : Color.secondary)
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)

            // Named for its rule: a list of switches all called "Enabled" says nothing.
            Toggle("\(rule.name) Enabled", isOn: Binding(
                get: { rule.enabled },
                set: { enabled in
                    var changed = rule
                    changed.enabled = enabled
                    Task { try? await session.saveAutomation(changed) }
                }
            ))
            .labelsHidden()
        }
    }
}

/// The last 50 runs, newest first.
struct AutomationRunsView: View {
    let session: MetocastSession
    @State private var shown: AutomationRunRecord?

    var body: some View {
        List(session.automationRuns) { run in
            Button {
                shown = run
            } label: {
                VStack(alignment: .leading, spacing: 2) {
                    HStack {
                        Text(run.name)
                        if run.manual {
                            Text("Run Now").font(.caption).foregroundStyle(.secondary)
                        }
                        Spacer()
                        Text(run.outcomeText)
                            .font(.subheadline)
                            .foregroundStyle(run.didFail ? Color.orange : Color.secondary)
                    }
                    Text(run.cause + (run.date.map { " · " + $0.formatted(date: .abbreviated, time: .shortened) } ?? ""))
                        .font(.subheadline)
                        .foregroundStyle(.secondary)
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                .contentShape(.rect)
            }
            .buttonStyle(.plain)
        }
        .overlay {
            if session.automationRuns.isEmpty {
                ContentUnavailableView("No Runs Yet", systemImage: "clock", description: Text("Each time a rule runs, what it did shows up here."))
            }
        }
        .navigationTitle("Run History")
        .sheet(item: $shown) { run in
            AutomationRunDetail(run: run)
        }
        .task { await session.loadAutomations() }
    }
}

/// One run: what fired it and what each step came to.
struct AutomationRunDetail: View {
    @Environment(\.dismiss) private var dismiss
    let run: AutomationRunRecord

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    LabeledContent("Rule", value: run.name)
                    LabeledContent("Fired by", value: run.cause)
                    if let date = run.date {
                        LabeledContent("Started", value: date.formatted(date: .abbreviated, time: .standard))
                    }
                } footer: {
                    if run.manual {
                        Text("Started by hand with Run Now, which ignores the cooldown and the armed switch.")
                    }
                }
                Section("Steps") {
                    ForEach(Array(run.results.enumerated()), id: \.offset) { _, result in
                        VStack(alignment: .leading, spacing: 2) {
                            Label(result.action, systemImage: result.symbol)
                                .foregroundStyle(result.outcome == .failed ? Color.orange : Color.primary)
                            if let message = result.messageText {
                                Text(message)
                                    .font(.subheadline)
                                    .foregroundStyle(.secondary)
                            }
                        }
                    }
                }
            }
            .formStyle(.grouped)
            .navigationTitle(run.outcomeText)
            .toolbar {
                ToolbarItem(placement: .confirmationAction) { Button("Done") { dismiss() } }
            }
        }
        #if os(macOS)
        .frame(minWidth: 460, minHeight: 420)
        #endif
    }
}
