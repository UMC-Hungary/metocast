import SwiftUI

struct NotificationsView: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State private var filter = Filter.all

    private enum Filter: String, CaseIterable {
        case all = "All", unread = "Unread"
    }

    private var shown: [ActivityItem] {
        filter == .all ? model.activity : model.activity.filter { !$0.isRead }
    }

    var body: some View {
        #if os(macOS)
        // A popover has no toolbar, so the title and filter share a header row.
        VStack(spacing: 0) {
            HStack {
                Text("Notifications")
                    .font(.headline)
                Spacer()
                filterPicker
                    .labelsHidden()
                    .fixedSize()
            }
            .padding(12)
            Divider()
            list
        }
        .frame(width: 340, height: 360)
        // Unread items stay bold (and the Unread filter stays useful) until the popover closes.
        .onDisappear { model.markActivityRead() }
        #else
        NavigationStack {
            list
                .navigationTitle("Notifications")
                .navigationBarTitleDisplayMode(.inline)
                .toolbar {
                    ToolbarItem(placement: .topBarLeading) {
                        filterPicker
                    }
                    ToolbarItem(placement: .topBarTrailing) {
                        Button(role: .close) { dismiss() }
                    }
                }
        }
        .presentationDetents([.medium, .large])
        // Unread items stay bold (and the Unread filter stays useful) until the sheet closes.
        .onDisappear { model.markActivityRead() }
        #endif
    }

    private var list: some View {
        List(shown) { item in
            HStack(spacing: 12) {
                Image(systemName: item.icon)
                    .symbolVariant(.fill)
                    .foregroundStyle(.tint)
                    #if os(macOS)
                    .font(.title3)
                    .frame(width: 28)
                    #else
                    .frame(width: 36, height: 36)
                    .background(.tint.opacity(0.12), in: Circle())
                    #endif
                VStack(alignment: .leading, spacing: 2) {
                    Text(item.title)
                        .fontWeight(item.isRead ? .regular : .semibold)
                    Text(item.date, format: .relative(presentation: .named))
                        .font(.footnote)
                        .foregroundStyle(.secondary)
                }
            }
        }
        .overlay {
            if shown.isEmpty {
                ContentUnavailableView(
                    filter == .all ? "No Notifications" : "No Unread Notifications",
                    systemImage: "bell.slash",
                    description: Text("Connection and connector changes show up here.")
                )
            }
        }
    }

    // A pop-up button: one choice between mutually exclusive views of the same list.
    private var filterPicker: some View {
        Picker("Show", selection: $filter) {
            ForEach(Filter.allCases, id: \.self) { Text($0.rawValue) }
        }
        .pickerStyle(.menu)
    }
}
