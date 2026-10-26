import SwiftUI

@main
struct MetocastApp: App {
    @State private var model = AppModel()
    @AppStorage("appearance") private var appearance = Appearance.system
    #if os(macOS)
    @NSApplicationDelegateAdaptor private var delegate: AppDelegate
    #endif

    init() {
        PresenterFont.register()
    }

    var body: some Scene {
        WindowGroup {
            RootView()
                .environment(model)
                .tint(.accentColor)
                .preferredColorScheme(appearance.colorScheme)
                #if os(macOS)
                .onAppear { delegate.model = model }
                #endif
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
    /// The toolbar output waiting on a stop confirmation, asked here rather than in the toolbar.
    @State private var stoppingOutput: LiveOutput.ID?
    /// Kept here so a collapsed sidebar stays collapsed when the split view is rebuilt.
    @State private var columns = NavigationSplitViewVisibility.automatic

    var body: some View {
        @Bindable var model = model
        let stopping = model.session?.outputs.first { $0.id == stoppingOutput && $0.isOn }
        NavigationSplitView(columnVisibility: $columns) {
            // An open equipment page leaves the sidebar unselected; picking any item, even the current one, closes it.
            List(selection: Binding(
                get: { model.equipment == nil ? model.pane : nil },
                set: { if let pane = $0 { model.pane = pane; model.equipment = nil } }
            )) {
                // On the Mac, settings live in the inspector (⌘,) rather than the sidebar. Media reads
                // files on the server's disk, so only the Mac hosting the server offers it.
                ForEach(MetocastPane.allCases.filter { $0 != .settings && ($0 != .media || model.mode == .server) }, id: \.self) { item in
                    Label(item.title, systemImage: item.icon)
                        .badge(item == .dashboard && model.session?.accountsNeedingSignIn.isEmpty == false
                            ? Text(Image(systemName: "exclamationmark.triangle.fill")) : nil)
                }
            }
            .navigationSplitViewColumnWidth(min: 180, ideal: 220)
        } detail: {
            NavigationStack {
                if let page = model.equipment, let session = model.session {
                    switch page {
                    case .connectors: ConnectorsView(session: session)
                    case .automations: AutomationsView(session: session)
                    case .serverTools: ServerToolsView(session: session)
                    case .devices: DevicesView(session: session)
                    }
                } else {
                    switch model.pane {
                    case .dashboard: DashboardView()
                    case .events: EventsView()
                    case .live: LiveView()
                    case .presentation: PresentationView()
                    case .media: MediaView()
                    case .settings: EmptyView()
                    }
                }
            }
            // Root pages; each pushed page applies it too, since a push drops the stack's own.
            .windowActions()
            // In Icon and Text mode the toolbar's labels sit over scrolled content; a hard edge keeps them legible.
            .scrollEdgeEffectStyle(.hard, for: .top)
        }
        // A fresh split view per page, so screens pushed on one page don't linger on the next. A new
        // detail stack alone isn't enough: the split view keeps pages pushed by `NavigationLink { }`
        // and `navigationDestination(isPresented:)` on screen after the page that pushed them is gone.
        // ponytail: a dragged sidebar width snaps back to ideal on page change; bind the
        // stack to a path and use value links everywhere if that matters.
        .id(model.equipment.map { "\($0)" } ?? model.pane.rawValue)
        .inspector(isPresented: $model.isShowingSettings) {
            SettingsView()
                .inspectorColumnWidth(340)
                .scrollEdgeEffectStyle(.hard, for: .top)
        }
        .toolbar {
            if let session = model.session {
                // Record and Transcript stay within reach from every page, event or not.
                ToolbarItemGroup { OutputToolbarButtons(session: session, stopping: $stoppingOutput) }
                ToolbarSpacer(.fixed)
                // A running export stays in view from every page.
                if !session.runningMediaJobs.isEmpty {
                    ToolbarItem { MediaJobsButton(session: session) }
                    ToolbarSpacer(.fixed)
                }
            }
            ToolbarItem { NewEventButton() }
        }
        // The current output, so the answer acts on its state now; one that stopped meanwhile drops the question.
        .stopConfirmation(
            for: stopping,
            isPresented: Binding(get: { stopping != nil }, set: { if !$0 { stoppingOutput = nil } }),
            send: { model.session?.control($0) }
        )
        .sheet(isPresented: $model.isAddingEvent) {
            EventEditorView()
        }
    }
}
#endif

#if os(macOS)
/// Asks before quitting stops a media job: the server is this app's helper, and goes with it.
/// Closing the window leaves the app, and the job, running. An iPhone never hosts the server, so
/// its jobs run on in a Mac's.
final class AppDelegate: NSObject, NSApplicationDelegate {
    var model: AppModel?

    func applicationShouldTerminate(_ sender: NSApplication) -> NSApplication.TerminateReply {
        guard let model, model.mode == .server, let session = model.session else { return .terminateNow }
        let jobs = session.runningMediaJobs
        guard let first = jobs.first else { return .terminateNow }
        let alert = NSAlert()
        alert.messageText = String(localized: "Stop the export and quit?")
        alert.informativeText = String(localized: "\(URL(fileURLWithPath: first.outputPath).lastPathComponent) is still being made. Quitting Metocast stops this Mac's server, and the export with it.")
        alert.addButton(withTitle: String(localized: "Keep Running"))
        let quit = alert.addButton(withTitle: String(localized: "Cancel Export and Quit"))
        quit.hasDestructiveAction = true
        guard alert.runModal() == .alertSecondButtonReturn else { return .terminateCancel }
        // A job the server loses mid-run starts over at its next launch; a cancelled one doesn't.
        // A running job ends a moment after Cancel, so this waits (up to 10 s) to hear it did.
        Task {
            for job in jobs { try? await session.cancelRender(jobId: job.id) }
            for _ in 0..<100 where !session.runningMediaJobs.isEmpty {
                try? await Task.sleep(for: .milliseconds(100))
            }
            sender.reply(toApplicationShouldTerminate: true)
        }
        return .terminateLater
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
            // The Dashboard's banner says which account needs signing in.
            .badge(model.session?.accountsNeedingSignIn.isEmpty == false ? Text("!") : nil)
            Tab(MetocastPane.events.title, systemImage: MetocastPane.events.icon, value: .events) {
                root(.events) {
                    EventsView()
                        // Events swaps the tab bar for a Notes-style bottom toolbar: the tabs share one
                        // glass group and New Event floats beside it. Both bars claim the same spot,
                        // so the tab bar has to hide for the toolbar to show.
                        .toolbarVisibility(.hidden, for: .tabBar)
                        .toolbar {
                            ToolbarItemGroup(placement: .bottomBar) {
                                ForEach(MetocastPane.allCases.filter { $0 != .media }, id: \.self) { pane in
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
                    if let pending = model.slideIndexPending {
                        ToolbarItem(placement: .topBarTrailing) { SlideIndexBadge(pending: pending) }
                    }
                    ToolbarItem(placement: .topBarTrailing) {
                        NotificationsBell { isShowingNotifications = true }
                    }
                    .matchedTransitionSource(id: pane, in: notificationsTransition)
                }
        }
    }
}
#endif

extension View {
    /// Notifications and Settings, which end the Mac's toolbar on every page. The toolbar lists an
    /// outer view's items before the page's, whatever their placement, so these are declared in an
    /// overlay, after the page. Apply it to each page the detail column shows, pushed ones included.
    func windowActions() -> some View { modifier(WindowActions()) }
}

private struct WindowActions: ViewModifier {
    @Environment(AppModel.self) private var model
    @State private var isShowingNotifications = false

    func body(content: Content) -> some View {
        #if os(macOS)
        content.overlay {
            Color.clear
                .allowsHitTesting(false)
                .toolbar {
                    if let pending = model.slideIndexPending {
                        ToolbarItem { SlideIndexBadge(pending: pending) }
                    }
                    ToolbarItemGroup {
                        // The Mac has room for a popover, which the HIG reserves for wide layouts.
                        NotificationsBell { isShowingNotifications = true }
                            .popover(isPresented: $isShowingNotifications, arrowEdge: .bottom) {
                                NotificationsView()
                            }
                        Button("Settings", systemImage: "sidebar.trailing") {
                            model.isShowingSettings.toggle()
                        }
                    }
                }
        }
        #else
        // The iPhone puts Notifications in each tab's bar and Settings in its own tab.
        content
        #endif
    }
}

struct NewEventButton: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        Button("New Event", systemImage: "calendar.badge.plus") {
            model.isAddingEvent = true
        }
    }
}

/// Shown in the toolbar while the server parses decks into the slide search index. The Mac shows
/// its text as a tooltip; the iPhone has no hover, so a tap shows it in a popover.
struct SlideIndexBadge: View {
    let pending: UInt32
    @State private var isShowingInfo = false

    var body: some View {
        let text = String(localized: "Indexing the slide library: \(pending) decks")
        Button(text, systemImage: "text.magnifyingglass") { isShowingInfo = true }
            .symbolEffect(.pulse)
            .help(text)
            .popover(isPresented: $isShowingInfo) {
                Text(text)
                    .padding()
                    .presentationCompactAdaptation(.popover)
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
        .accessibilityValue(hasUnread ? String(localized: "\(model.unreadCount) unread") : "")
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

/// Settings' equipment screens, shown in the Mac's content column (the iPhone pushes them from Settings).
enum EquipmentPage: Hashable {
    case connectors, automations, serverTools, devices
}

enum MetocastPane: String, CaseIterable, Hashable {
    case dashboard, events, live, presentation, media, settings

    var title: String {
        switch self {
        case .dashboard: String(localized: "Dashboard")
        case .events: String(localized: "Events")
        case .live: String(localized: "Live")
        case .presentation: String(localized: "Presentation")
        case .media: String(localized: "Media")
        case .settings: String(localized: "Settings")
        }
    }

    var icon: String {
        switch self {
        case .dashboard: "square.grid.2x2"
        case .events: "calendar"
        case .live: "dot.radiowaves.left.and.right"
        case .presentation: "play.rectangle"
        case .media: "film.stack"
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
