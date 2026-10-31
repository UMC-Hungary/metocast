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
                #else
                .background(KeyboardDismissal())
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
            // No Settings window: Metocast → Settings… shows or hides the panel beside the page instead.
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
            // The socket may have slept in the background, so coming back is a good time to reload or reconnect.
            if phase == .active {
                Task {
                    if model.session == nil || model.session?.status == .unreachable || model.session?.status == .closed {
                        await model.retry()
                    } else {
                        await model.session?.refresh()
                    }
                }
            }
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

private struct EquipmentPageView: View {
    let page: EquipmentPage
    let session: MetocastSession

    var body: some View {
        switch page {
        case .connectors: ConnectorsView(session: session)
        case .automations: AutomationsView(session: session)
        case .serverTools: ServerToolsView(session: session)
        case .devices: DevicesView(session: session)
        }
    }
}

/// The sidebar of the Mac and the iPad: the pages, then the equipment.
private struct SidebarList: View {
    private enum Item: Hashable {
        case pane(MetocastPane)
        case equipment(EquipmentPage)
    }

    let panes: [MetocastPane]
    @Environment(AppModel.self) private var model

    var body: some View {
        List(selection: Binding<Item?>(
            get: { model.equipment.map(Item.equipment) ?? .pane(model.pane) },
            set: { item in
                switch item {
                case .pane(let pane): model.pane = pane; model.equipment = nil
                case .equipment(let page): model.equipment = page
                case nil: break
                }
            }
        )) {
            ForEach(panes, id: \.self) { pane in
                Label(pane.title, systemImage: pane.icon)
                    .badge(pane == .dashboard && model.session?.accountsNeedingSignIn.isEmpty == false
                        ? Text(Image(systemName: "exclamationmark.triangle.fill")) : nil)
                    .tag(Item.pane(pane))
            }
            Section("Equipment") {
                Label("Connectors", systemImage: "puzzlepiece.extension").tag(Item.equipment(.connectors))
                Label("Automations", systemImage: "bolt").tag(Item.equipment(.automations))
                Label("Server Tools", systemImage: "wrench.and.screwdriver").tag(Item.equipment(.serverTools))
                Label("Discover Devices", systemImage: "network").tag(Item.equipment(.devices))
            }
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
        let stopping = model.session.flatMap { session in
            (session.outputs + [session.recordButton]).first { $0.id == stoppingOutput && $0.isOn }
        }
        // Settings sits beside the split view rather than in an inspector, which widens the window
        // to make room. Here the page gives up the width instead.
        HStack(spacing: 0) {
            NavigationSplitView(columnVisibility: $columns) {
                // On the Mac, settings open beside the page (⌘,) rather than in the sidebar. Media reads
                // files on the server's disk, so only the Mac hosting the server offers it.
                SidebarList(panes: MetocastPane.allCases.filter { $0 != .settings && ($0 != .media || model.mode == .server) })
                    .navigationSplitViewColumnWidth(min: 180, ideal: 220)
            } detail: {
                NavigationStack {
                    if let page = model.equipment, let session = model.session {
                        EquipmentPageView(page: page, session: session)
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
            if model.isShowingSettings {
                Divider()
                SettingsView()
                    .frame(width: 340)
                    .scrollEdgeEffectStyle(.hard, for: .top)
            }
        }
        .toolbar {
            if let session = model.session {
                // Record and Transcript stay within reach from every page, event or not.
                ToolbarItemGroup {
                    RecordButton(session: session) { stoppingOutput = session.recordButton.id }
                    OutputToolbarButtons(session: session, stopping: $stoppingOutput)
                }
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
/// A tap outside a text field closes the keyboard, on every page and sheet of the window. The tap
/// still reaches whatever is under it.
private struct KeyboardDismissal: UIViewRepresentable {
    func makeUIView(context: Context) -> InstallingView { InstallingView() }
    func updateUIView(_ view: InstallingView, context: Context) {}

    final class InstallingView: UIView, UIGestureRecognizerDelegate {
        private lazy var tap: UITapGestureRecognizer = {
            let tap = UITapGestureRecognizer(target: self, action: #selector(dismiss))
            tap.cancelsTouchesInView = false
            tap.delegate = self
            return tap
        }()

        override func didMoveToWindow() {
            super.didMoveToWindow()
            window?.addGestureRecognizer(tap)
        }

        @objc private func dismiss() {
            tap.view?.endEditing(true)
        }

        func gestureRecognizer(_ gestureRecognizer: UIGestureRecognizer, shouldReceive touch: UITouch) -> Bool {
            // A tap inside a field moves its cursor.
            var view = touch.view
            while let current = view {
                if current is UITextField || current is UITextView { return false }
                view = current.superview
            }
            return true
        }

        func gestureRecognizer(_ gestureRecognizer: UIGestureRecognizer, shouldRecognizeSimultaneouslyWith other: UIGestureRecognizer) -> Bool {
            true
        }
    }
}

/// Black out and close the open deck, with their names: at the foot of the iPad's sidebar, or
/// floating over the page while the sidebar is closed.
private struct SlideControls: View {
    let session: MetocastSession
    var isFloating = false
    /// Just the icons, when Settings leaves the page too little room for the names.
    var isCompact = false

    var body: some View {
        let muted = session.presenter?.muted == true
        let layout = isFloating ? AnyLayout(HStackLayout(spacing: 8)) : AnyLayout(VStackLayout(spacing: 8))
        GlassEffectContainer(spacing: 8) {
            layout {
                if session.usesWebPresenter {
                    Button {
                        session.show(muted ? .unmute : .mute)
                    } label: {
                        Label(muted ? "Show Slides" : "Black Out", systemImage: muted ? "eye" : "eye.slash")
                            .frame(maxWidth: isFloating ? nil : .infinity, alignment: .leading)
                    }
                }
                Button {
                    session.show(.close)
                } label: {
                    Label("Close Slides", systemImage: "xmark.circle")
                        .frame(maxWidth: isFloating ? nil : .infinity, alignment: .leading)
                }
            }
            .labelStyle(SlideControlLabelStyle(isCompact: isCompact))
            .buttonStyle(.glass)
            .controlSize(.large)
            .lineLimit(1)
        }
        .fixedSize(horizontal: isFloating, vertical: false)
    }
}

private struct SlideControlLabelStyle: LabelStyle {
    let isCompact: Bool

    func makeBody(configuration: Configuration) -> some View {
        if isCompact {
            Label { configuration.title } icon: { configuration.icon }.labelStyle(.iconOnly)
        } else {
            Label { configuration.title } icon: { configuration.icon }.labelStyle(.titleAndIcon)
        }
    }
}

private struct TabRootView: View {
    @Environment(AppModel.self) private var model
    @Environment(\.horizontalSizeClass) private var sizeClass
    @State private var isShowingNotifications = false
    /// Kept here so a collapsed sidebar stays collapsed when the split view is rebuilt.
    @State private var columns = NavigationSplitViewVisibility.all
    /// The Record button's stop question, asked here: a dialog on a bar item comes back after each answer.
    @State private var isStoppingRecorders = false
    @Namespace private var notificationsTransition

    var body: some View {
        @Bindable var model = model
        Group {
            if sizeClass == .regular { sidebar } else { tabs }
        }
        .scrollDismissesKeyboard(.interactively)
        .sheet(isPresented: $model.isAddingEvent) {
            EventEditorView()
        }
        .sheet(isPresented: $isShowingNotifications) {
            NotificationsView()
                .navigationTransition(.zoom(sourceID: model.pane, in: notificationsTransition))
        }
        .stopConfirmation(
            for: isStoppingRecorders ? model.session?.recordButton : nil,
            isPresented: $isStoppingRecorders,
            send: { model.session?.control($0) }
        )
    }

    /// The iPad's navigation, laid out like the Mac's: the pages and the equipment in a sidebar,
    /// Settings in an inspector.
    private var sidebar: some View {
        @Bindable var model = model
        return NavigationSplitView(columnVisibility: $columns) {
            SidebarList(panes: MetocastPane.allCases.filter { $0 != .settings && $0 != .media })
                .navigationTitle(Text(verbatim: "Metocast"))
            // What's on the output screen stays in reach from every page.
            .safeAreaInset(edge: .bottom) {
                if let session = model.session, session.isShowingSlides {
                    SlideControls(session: session).padding()
                }
            }
        } detail: {
            if let page = model.equipment, let session = model.session {
                root(.settings) {
                    EquipmentPageView(page: page, session: session)
                }
            } else {
                root(model.pane) {
                    switch model.pane {
                    case .events: EventsView()
                    case .live: LiveView()
                    case .presentation: PresentationView()
                    case .dashboard, .media, .settings: DashboardView()
                    }
                }
            }
        }
        // A fresh split view per page, so screens pushed on one page don't linger on the next.
        .id(model.equipment.map { "\($0)" } ?? model.pane.rawValue)
        // The page is too narrow for both panels, so opening one closes the other.
        .onChange(of: model.isShowingSettings) { _, isShowing in
            if isShowing { columns = .detailOnly }
        }
        .onChange(of: columns) { _, columns in
            if columns != .detailOnly { model.isShowingSettings = false }
        }
        .overlay(alignment: .bottomTrailing) {
            HStack(spacing: 12) {
                if columns == .detailOnly, let session = model.session, session.isShowingSlides {
                    SlideControls(session: session, isFloating: true, isCompact: model.isShowingSettings)
                }
                Button {
                    model.isShowingSettings.toggle()
                } label: {
                    Label("Settings", systemImage: "gearshape")
                        .labelStyle(.iconOnly)
                        .font(.title2)
                        .padding(8)
                }
                .buttonStyle(.glass)
                .buttonBorderShape(.circle)
            }
            .padding(20)
        }
        .inspector(isPresented: $model.isShowingSettings) {
            NavigationStack {
                SettingsView(showsEquipment: false).navigationTitle("Settings")
            }
            .inspectorColumnWidth(340)
        }
    }

    private var tabs: some View {
        @Bindable var model = model
        return TabView(selection: $model.pane) {
            Tab(MetocastPane.dashboard.title, systemImage: MetocastPane.dashboard.icon, value: .dashboard) {
                root(.dashboard) { DashboardView() }
            }
            // The Dashboard's banner says which account needs signing in.
            .badge(model.session?.accountsNeedingSignIn.isEmpty == false ? Text("!") : nil)
            Tab(MetocastPane.events.title, systemImage: MetocastPane.events.icon, value: .events) {
                root(.events) { EventsView() }
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
    }

    // Every tab keeps its own bell, so each one is a separate zoom source keyed by its tab.
    private func root(_ pane: MetocastPane, @ViewBuilder content: () -> some View) -> some View {
        NavigationStack {
            content()
                .toolbar {
                    ToolbarItem(placement: .keyboard) {
                        Button("Done") {
                            UIApplication.shared.sendAction(#selector(UIResponder.resignFirstResponder), to: nil, from: nil, for: nil)
                        }
                    }
                    // The Live tab keeps the Record button in reach; the Mac has it on every page.
                    if pane == .live, let session = model.session {
                        ToolbarItem(placement: .topBarLeading) {
                            RecordButton(session: session) { isStoppingRecorders = true }
                        }
                    }
                    if pane != .presentation {
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
        // The iPhone and iPad put Notifications in each page's bar.
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

/// The equipment screens, in the sidebar on the Mac and iPad (the iPhone pushes them from Settings).
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
