import SwiftUI

@main
struct MetocastApp: App {
    @State private var model = AppModel()
    @AppStorage("appearance") private var appearance = Appearance.system

    var body: some Scene {
        WindowGroup {
            RootView()
                .environment(model)
                .tint(.accentColor)
                .preferredColorScheme(appearance.colorScheme)
        }
        #if os(macOS)
        .defaultSize(width: 1080, height: 720)
        .commands {
            // Menu commands live outside the window's environment, so they use the model directly.
            CommandGroup(replacing: .newItem) {
                Button("New Event") { model.isAddingEvent = true }
                    .keyboardShortcut("n")
                    .disabled(model.session == nil)
            }
            // No Settings window: Metocast → Settings… shows or hides the inspector instead.
            CommandGroup(replacing: .appSettings) {
                Button("Settings…") { model.isShowingSettings.toggle() }
                    .keyboardShortcut(",")
                    .disabled(model.mode == nil)
            }
            CommandGroup(after: .toolbar) {
                Button("Refresh") { Task { await model.session?.refresh() } }
                    .keyboardShortcut("r")
                    .disabled(model.session == nil)
            }
        }
        #endif
    }
}

/// Setup until the device has a mode, then the app. Also handles `metocast://connect` links.
private struct RootView: View {
    @Environment(AppModel.self) private var model
    @Environment(\.scenePhase) private var scenePhase
    @State private var linkError: String?

    var body: some View {
        @Bindable var model = model
        Group {
            if model.mode == nil {
                SetupView()
            } else {
                #if os(macOS)
                MacRootView()
                #else
                TabRootView()
                #endif
            }
        }
        .task { await model.resume() }
        .onOpenURL { model.open($0) }
        .onChange(of: scenePhase) { _, phase in
            // The socket may have slept in the background, so coming back is a good time to reload.
            if phase == .active { Task { await model.session?.refresh() } }
        }
        .confirmationDialog(
            "Connect to a different server?",
            isPresented: Binding(
                get: { model.mode != nil && model.pendingLink != nil },
                set: { if !$0 { model.pendingLink = nil } }
            ),
            presenting: model.pendingLink
        ) { link in
            Button("Connect to \(ServerAddress.displayName(link.url))") {
                Task { linkError = await model.useServer(link) }
            }
        } message: { _ in
            Text("This device will stop using its current server.")
        }
        .alert("Couldn't Connect", isPresented: Binding(get: { linkError != nil }, set: { if !$0 { linkError = nil } })) {
        } message: {
            Text(linkError ?? "")
        }
    }
}

#if os(macOS)
private struct MacRootView: View {
    @Environment(AppModel.self) private var model
    @State private var isShowingNotifications = false

    var body: some View {
        @Bindable var model = model
        NavigationSplitView {
            List(selection: $model.pane) {
                // On the Mac, settings live in the inspector (⌘,) rather than the sidebar.
                ForEach(MetocastPane.allCases.filter { $0 != .settings }, id: \.self) { item in
                    Label(item.title, systemImage: item.icon)
                }
            }
            .navigationSplitViewColumnWidth(min: 180, ideal: 220)
        } detail: {
            NavigationStack {
                switch model.pane {
                case .dashboard: DashboardView()
                case .events: EventsView()
                case .live: LiveView()
                case .presentation: PresentationView()
                case .settings: EmptyView()
                }
            }
        }
        .inspector(isPresented: $model.isShowingSettings) {
            SettingsView()
                .inspectorColumnWidth(340)
        }
        .toolbar {
            ToolbarItem {
                Button("Refresh", systemImage: "arrow.clockwise") {
                    Task { await model.session?.refresh() }
                }
                .disabled(model.session == nil)
            }
            ToolbarSpacer(.fixed)
            ToolbarItem {
                // The Mac has room for a popover, which the HIG reserves for wide layouts.
                NotificationsBell { isShowingNotifications = true }
                    .popover(isPresented: $isShowingNotifications, arrowEdge: .bottom) {
                        NotificationsView()
                    }
            }
            ToolbarSpacer(.fixed)
            ToolbarItem {
                NewEventButton()
            }
            ToolbarItem {
                Button("Settings", systemImage: "sidebar.trailing") {
                    model.isShowingSettings.toggle()
                }
            }
        }
        .sheet(isPresented: $model.isAddingEvent) {
            EventEditorView()
        }
    }
}
#endif

#if os(iOS)
private struct TabRootView: View {
    @Environment(AppModel.self) private var model
    @State private var isShowingNotifications = false
    @Namespace private var newEventTransition
    @Namespace private var notificationsTransition

    var body: some View {
        @Bindable var model = model
        TabView(selection: $model.pane) {
            Tab(MetocastPane.dashboard.title, systemImage: MetocastPane.dashboard.icon, value: .dashboard) {
                root(.dashboard) { DashboardView() }
            }
            Tab(MetocastPane.events.title, systemImage: MetocastPane.events.icon, value: .events) {
                root(.events) {
                    EventsView()
                        // Events swaps the tab bar for a Notes-style bottom toolbar: the tabs share one
                        // glass group and New Event floats beside it. Both bars claim the same spot,
                        // so the tab bar has to hide for the toolbar to show.
                        .toolbarVisibility(.hidden, for: .tabBar)
                        .toolbar {
                            ToolbarItemGroup(placement: .bottomBar) {
                                ForEach(MetocastPane.allCases, id: \.self) { pane in
                                    Button(pane.title, systemImage: pane.icon) { model.pane = pane }
                                        .symbolVariant(pane == .events ? .fill : .none)
                                }
                            }
                            ToolbarSpacer(.flexible, placement: .bottomBar)
                            ToolbarItem(placement: .bottomBar) {
                                NewEventButton()
                            }
                            .matchedTransitionSource(id: "newEvent", in: newEventTransition)
                        }
                }
            }
            Tab(MetocastPane.live.title, systemImage: MetocastPane.live.icon, value: .live) {
                root(.live) { LiveView() }
            }
            Tab(MetocastPane.presentation.title, systemImage: MetocastPane.presentation.icon, value: .presentation) {
                root(.presentation) { PresentationView() }
            }
            Tab(MetocastPane.settings.title, systemImage: MetocastPane.settings.icon, value: .settings) {
                root(.settings) { SettingsView().navigationTitle("Settings") }
            }
        }
        .tabBarMinimizeBehavior(.onScrollDown)
        .sheet(isPresented: $model.isAddingEvent) {
            EventEditorView()
                .navigationTransition(.zoom(sourceID: "newEvent", in: newEventTransition))
        }
        .sheet(isPresented: $isShowingNotifications) {
            NotificationsView()
                .navigationTransition(.zoom(sourceID: model.pane, in: notificationsTransition))
        }
    }

    // Every tab keeps its own bell, so each one is a separate zoom source keyed by its tab.
    private func root(_ pane: MetocastPane, @ViewBuilder content: () -> some View) -> some View {
        NavigationStack {
            content()
                .toolbar {
                    ToolbarItem(placement: .topBarTrailing) {
                        NotificationsBell { isShowingNotifications = true }
                    }
                    .matchedTransitionSource(id: pane, in: notificationsTransition)
                }
        }
    }
}
#endif

struct NewEventButton: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        Button("New Event", systemImage: "plus") {
            model.isAddingEvent = true
        }
    }
}

struct NotificationsBell: View {
    @Environment(AppModel.self) private var model
    let action: () -> Void

    var body: some View {
        // Toolbar buttons can't show a count badge on iOS, so unread state is the symbol's own dot.
        let hasUnread = model.unreadCount > 0
        Button(action: action) {
            Label {
                Text("Notifications")
            } icon: {
                // The toolbar tints multicolor symbols, so the dot's red has to be set explicitly. The plain
                // bell has a single layer that would take the first color, so red applies only with the dot.
                Image(systemName: hasUnread ? "bell.badge" : "bell")
                    .symbolRenderingMode(.palette)
                    .foregroundStyle(hasUnread ? AnyShapeStyle(.red) : Self.barStyle, Self.barStyle)
            }
        }
        // Keyed to the total, not the unread count, so it rings for arrivals but not when they're read.
        .symbolEffect(.wiggle, value: model.activity.count)
        .accessibilityValue(hasUnread ? "\(model.unreadCount) unread" : "")
    }

    // iOS bar buttons take the app tint; Mac toolbar icons stay neutral like their neighbours.
    private static var barStyle: AnyShapeStyle {
        #if os(macOS)
        AnyShapeStyle(.primary)
        #else
        AnyShapeStyle(.tint)
        #endif
    }
}

enum MetocastPane: String, CaseIterable, Hashable {
    case dashboard, events, live, presentation, settings

    var title: String {
        switch self {
        case .dashboard: "Dashboard"
        case .events: "Events"
        case .live: "Live"
        case .presentation: "Presentation"
        case .settings: "Settings"
        }
    }

    var icon: String {
        switch self {
        case .dashboard: "square.grid.2x2"
        case .events: "calendar"
        case .live: "dot.radiowaves.left.and.right"
        case .presentation: "play.rectangle"
        case .settings: "gearshape"
        }
    }
}

#if os(iOS)
extension Color {
    static var groupedBackground: Color { Color(.systemGroupedBackground) }
    static var groupedSecondary: Color { Color(.secondarySystemGroupedBackground) }
}
#endif
