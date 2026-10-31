import SwiftUI
import WebKit
import Metocast

/// The slide library and the live presenter on one page: the decks in the watched folders, and
/// the slide on the output screen with the controls that move it. One click on a deck opens it.
/// In Keynote mode the server has the slides, so this shows what Keynote reports instead.
struct PresentationView: View {
    @Environment(AppModel.self) private var model
    @State private var query = ""
    @State private var isWritingSong = false
    @State private var isAskingForSongFolder = false
    @State private var isShowingSettings = false
    @State private var isShowingNotifications = false
    @State private var isShowingOrder = true
    @State private var selectedEventId: String?
    #if os(iOS)
    @Environment(\.horizontalSizeClass) private var horizontalSizeClass
    #endif

    private var isWideLayout: Bool {
        #if os(macOS)
        true
        #else
        horizontalSizeClass == .regular
        #endif
    }

    var body: some View {
        Group {
            if let session = model.session {
                page(session)
                    .toolbar {
                        #if os(macOS)
                        ToolbarItemGroup {
                            Button("New Song", systemImage: "music.note.list") { Task { await newSong(session) } }
                            if session.isShowingSlides {
                                Button("Close Slides", systemImage: "xmark.circle") { session.show(.close) }
                            }
                            Button("Presentation Settings", systemImage: "slider.horizontal.3") { isShowingSettings = true }
                        }
                        if session.isShowingSlides {
                            ToolbarItemGroup { buttons(session) }
                        }
                        #else
                        // The iPad's page starts right under this, with the sidebar button beside it.
                        if isWideLayout, orderEvent(session) != nil {
                            ToolbarItem(placement: .topBarLeading) {
                                listPicker
                                    .frame(width: 130)
                            }
                            .sharedBackgroundVisibility(.hidden)
                        }
                        #endif
                    }
                    .navigationDestination(isPresented: $isWritingSong) {
                        SongSlidesView(session: session).windowActions()
                    }
                    .songFolderPrompt(isPresented: $isAskingForSongFolder, session: session) { _ in isWritingSong = true }
                    .navigationDestination(isPresented: $isShowingSettings) {
                        PresentationSettingsView(session: session).windowActions()
                    }
                    .sheet(isPresented: $isShowingNotifications) {
                        NotificationsView()
                    }
            } else {
                ContentUnavailableView("No Presentation", systemImage: "play.rectangle")
                    #if os(macOS)
                    .navigationTitle("Presentation")
                    #endif
            }
        }
    }

    @ViewBuilder
    private func page(_ session: MetocastSession) -> some View {
        let library = sidebar(session)
        Group {
            if isWideLayout {
                HStack(spacing: 0) {
                    library.frame(width: 320)
                    Divider()
                        #if os(iOS)
                        // All the way up behind the top bar, like the sidebar's own edge.
                        .ignoresSafeArea(.container, edges: .top)
                        #endif
                    content(session)
                        .frame(maxWidth: .infinity, maxHeight: .infinity)
                }
            } else {
                GeometryReader { proxy in
                    VStack(spacing: 0) {
                        if session.isShowingSlides {
                            iphonePresenter(session, maxHeight: proxy.size.height * 0.25)
                            Divider()
                        }
                        library
                    }
                }
            }
        }
        #if os(iOS)
        // The iPad's bar holds the sidebar button.
        .toolbarVisibility(isWideLayout ? .automatic : .hidden, for: .navigationBar)
        #endif
        #if os(macOS)
        .navigationTitle(session.presenter?.loaded == true ? session.presenter?.fileName ?? "" : String(localized: "Presentation"))
        #endif
        .task { await session.refresh() }
    }

    /// The event to step through: the one chosen by the user, the one currently on screen,
    /// the one scheduled now or next, an open event from today, or the most recent open event.
    private func orderEvent(_ session: MetocastSession) -> EventSummaryRecord? {
        if let id = selectedEventId, let match = session.events.first(where: { $0.id == id }) {
            return match
        }
        if let current = session.events.first(where: { $0.id == session.orderPosition?.eventId }) {
            return current
        }
        if let scheduled = EventSchedule.current(in: session.events, now: .now) {
            return scheduled
        }
        let open = session.events.filter { !$0.isCompleted }
        let today = open.filter { event in
            event.date.map { Calendar.current.isDateInToday($0) } ?? false
        }
        if let latestToday = today.max(by: { ($0.date ?? .distantPast) < ($1.date ?? .distantPast) }) {
            return latestToday
        }
        return open.max(by: { ($0.date ?? .distantPast) < ($1.date ?? .distantPast) }) ?? session.events.first
    }

    /// The deck library, and the event's Order of Service beside it, so the service can be stepped
    /// through from here, with the web presenter or Keynote.
    @ViewBuilder
    private func sidebar(_ session: MetocastSession) -> some View {
        let event = orderEvent(session)
        #if os(iOS)
        if isWideLayout {
            switchableSidebar(session, event: event)
        } else if let event {
            TabView(selection: $isShowingOrder) {
                Form { LiveOrderSection(session: session, event: event) }
                    .formStyle(.grouped)
                    .tag(true)

                deckList(session)
                .tag(false)
            }
            .tabViewStyle(.page(indexDisplayMode: .never))
        } else {
            deckList(session)
        }
        #else
        switchableSidebar(session, event: event)
        #endif
    }

    /// The sidebar on the wide layouts, with a control to switch between its two lists.
    private func switchableSidebar(_ session: MetocastSession, event: EventSummaryRecord?) -> some View {
        VStack(spacing: 0) {
            if event != nil {
                #if os(macOS)
                HStack(spacing: 8) {
                    listPicker

                    if isShowingOrder, session.events.count > 1 {
                        Menu {
                            eventChoices(session)
                        } label: {
                            Image(systemName: "calendar")
                                .imageScale(.medium)
                        }
                        .buttonStyle(.plain)
                        .accessibilityLabel("Choose Event")
                    }
                }
                .padding(10)
                #endif
            }
            if let event, isShowingOrder {
                Form { LiveOrderSection(session: session, event: event) }
                    .formStyle(.grouped)
            } else {
                deckList(session)
            }
        }
    }

    private func eventChoices(_ session: MetocastSession) -> some View {
        let chosen = orderEvent(session)?.id
        return ForEach(session.events, id: \.id) { event in
            Button {
                selectedEventId = event.id
            } label: {
                HStack {
                    Text(event.title)
                    if event.id == chosen {
                        Image(systemName: "checkmark")
                    }
                }
            }
        }
    }

    private func deckList(_ session: MetocastSession) -> some View {
        DeckList(session: session, query: query) { file in
            session.show(.open(filePath: file.path))
        }
        .librarySearch($query)
    }

    private var listPicker: some View {
        Picker("Show", selection: $isShowingOrder) {
            #if os(iOS)
            // Icons, so the switch fits its column beside the iPad's sidebar button.
            Image(systemName: "list.number").accessibilityLabel(Text("Order of Service")).tag(true)
            Image(systemName: "books.vertical").accessibilityLabel(Text("Library")).tag(false)
            #else
            Text("Order of Service").tag(true)
            Text("Library").tag(false)
            #endif
        }
        .pickerStyle(.segmented)
        .labelsHidden()
    }

    /// A song needs somewhere to go, so a missing folder is asked for before the page opens.
    private func newSong(_ session: MetocastSession) async {
        let folder = (try? await session.serverSettings().songSlideFolder.trimmed) ?? ""
        if folder.isEmpty {
            isAskingForSongFolder = true
        } else {
            isWritingSong = true
        }
    }

    private func isLoading(_ session: MetocastSession) -> Bool {
        session.loadingOrderItemId != nil || session.isSteppingOrder || session.loadingDeckPath != nil
    }

    @ViewBuilder
    private func floatingActions(_ session: MetocastSession) -> some View {
        GlassEffectContainer(spacing: 8) {
            HStack(spacing: 8) {
                actionButtons(session)
            }
            .buttonStyle(.glass)
        }
    }

    #if os(iOS)
    /// The iPad has room for the iPhone's menu as buttons, floating under the slides. Black Out and
    /// Close Slides are in its sidebar, or float beside Settings while the sidebar is closed.
    private func bottomActions(_ session: MetocastSession) -> some View {
        GlassEffectContainer(spacing: 8) {
            HStack(spacing: 8) {
                HStack(spacing: 0) {
                    Button("New Song", systemImage: "music.note.list") { Task { await newSong(session) } }
                        .frame(width: 40, height: 44)
                    Button("Presentation Settings", systemImage: "slider.horizontal.3") { isShowingSettings = true }
                        .frame(width: 40, height: 44)
                    Button("Notifications", systemImage: "bell") { isShowingNotifications = true }
                        .frame(width: 40, height: 44)
                    if session.events.count > 1 {
                        Menu("Event", systemImage: "calendar") { eventChoices(session) }
                        .frame(width: 40, height: 44)
                    }
                }
                .padding(.horizontal, 4)
                .glassEffect(.regular.interactive(), in: .capsule)
            }
            .labelStyle(.iconOnly)
            .buttonStyle(.borderless)
        }
        .fixedSize()
    }
    #endif

    @ViewBuilder
    private func actionButtons(_ session: MetocastSession) -> some View {
        if session.usesWebPresenter {
            let muted = session.presenter?.muted == true
            Button {
                session.show(muted ? .unmute : .mute)
            } label: {
                Image(systemName: muted ? "eye" : "eye.slash")
                    .imageScale(.medium)
                    .frame(width: 28, height: 28)
            }
            .accessibilityLabel(muted ? "Show Slides" : "Black Out")
        }

        Menu {
            Button("Presentation Settings", systemImage: "slider.horizontal.3") {
                isShowingSettings = true
            }
            Button("New Song", systemImage: "music.note.list") {
                Task { await newSong(session) }
            }
            if session.isShowingSlides {
                Button("Close Slides", systemImage: "xmark.circle") {
                    session.show(.close)
                }
            }
            Button("Notifications", systemImage: "bell") {
                isShowingNotifications = true
            }
            if session.events.count > 1 {
                Menu("Event") { eventChoices(session) }
            }
        } label: {
            Image(systemName: "ellipsis")
                .imageScale(.medium)
                .frame(width: 28, height: 28)
        }
        .accessibilityLabel("Actions")
    }

    @ViewBuilder
    private func iphonePresenter(_ session: MetocastSession, maxHeight: CGFloat) -> some View {
        VStack(alignment: .trailing, spacing: 8) {
            // In a row of its own, under the status bar and clear of the slides.
            floatingActions(session)
            HStack(spacing: 8) {
                if session.usesWebPresenter, let state = session.presenter, state.loaded {
                    previousSlideButton(session) {
                        currentSlide(state, session: session)
                            .aspectRatio(16 / 9, contentMode: .fit)
                            .frame(maxHeight: maxHeight - 16)
                    }
                    .accessibilityLabel("Current slide \(state.currentSlide). Tap for previous.")

                    if state.currentSlide < state.totalSlides {
                        nextSlideButton(session) {
                            SlideView(state: state, number: state.currentSlide + 1, theme: session.presenterTheme)
                                .clipShape(.rect(cornerRadius: 12))
                                .aspectRatio(16 / 9, contentMode: .fit)
                                .frame(maxHeight: maxHeight - 16)
                        }
                        .accessibilityLabel("Next slide \(state.currentSlide + 1). Tap for next.")
                    } else {
                        endPlaceholder
                            .aspectRatio(16 / 9, contentMode: .fit)
                            .frame(maxHeight: maxHeight - 16)
                    }
                } else if !session.usesWebPresenter, let status = session.presentationStatus, status.appRunning {
                    keynotePreview(status, session: session, maxHeight: maxHeight)
                }
            }
            .frame(maxWidth: .infinity)
        }
        .padding(8)
        .fixedSize(horizontal: false, vertical: true)
        .overlay {
            if isLoading(session) {
                openingIndicator
            }
        }
    }

    @ViewBuilder
    private func keynotePreview(_ status: PresentationStatus, session: MetocastSession, maxHeight: CGFloat) -> some View {
        let current = status.currentSlide ?? 1
        let total = status.totalSlides ?? 1
        HStack(spacing: 8) {
            previousSlideButton(session) {
            ZStack {
                Color.black.opacity(0.8)
                VStack(spacing: 4) {
                    Image(systemName: "play.display")
                    Text("\(current)")
                        .font(.headline)
                }
                .foregroundStyle(.white)
            }
            .clipShape(.rect(cornerRadius: 12))
            .aspectRatio(16 / 9, contentMode: .fit)
            .frame(maxHeight: maxHeight - 16)
            }

            if current < total {
                nextSlideButton(session) {
                ZStack {
                    Color.black.opacity(0.5)
                    VStack(spacing: 4) {
                        Image(systemName: "chevron.forward")
                        Text("\(current + 1)")
                            .font(.subheadline)
                    }
                    .foregroundStyle(.secondary)
                }
                .clipShape(.rect(cornerRadius: 12))
                .aspectRatio(16 / 9, contentMode: .fit)
                .frame(maxHeight: maxHeight - 16)
                }
            }
        }
    }

    @ViewBuilder
    private func keynoteContent(_ status: PresentationStatus, session: MetocastSession) -> some View {
        VStack(spacing: 20) {
            ContentUnavailableView {
                Label(status.documentName ?? "Keynote", systemImage: "play.display")
            } description: {
                Text(status.slideshowActive ? "Playing in Keynote." : "Open in Keynote, not playing.")
            }
            #if os(macOS)
            if let current = status.currentSlide, let total = status.totalSlides {
                slidePicker(current: current, total: total, session: session)
            }
            #else
            HStack(spacing: 16) {
                Button("Previous Slide", systemImage: "chevron.backward") { session.show(.previous) }
                    .keyboardShortcut(.leftArrow, modifiers: [])
                    .buttonStyle(.glass)
                if let current = status.currentSlide, let total = status.totalSlides {
                    Text("Slide \(current) of \(total)")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
                Button("Next Slide", systemImage: "chevron.forward") { session.show(.next) }
                    .keyboardShortcut(.rightArrow, modifiers: [])
                    .buttonStyle(.glassProminent)
            }
            .labelStyle(.iconOnly)
            .controlSize(.extraLarge)
            #endif
            Button(status.slideshowActive ? "Stop Slideshow" : "Play Slideshow", systemImage: status.slideshowActive ? "stop" : "play") {
                session.show(status.slideshowActive ? .stop : .start)
            }
            .buttonStyle(.bordered)
            Spacer(minLength: 0)
        }
    }

    private var openingIndicator: some View {
        HStack(spacing: 8) {
            ProgressView()
                .controlSize(.small)
            Text("Opening…")
                .font(.subheadline.weight(.medium))
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 8)
        .glassEffect(.regular, in: .capsule)
        .shadow(radius: 6)
    }

    @ViewBuilder
    private func content(_ session: MetocastSession) -> some View {
        ZStack(alignment: .topTrailing) {
            if session.usesWebPresenter, let state = session.presenter, state.loaded {
                VStack(spacing: 20) {
                    slideLayout {
                        previousSlideButton(session) {
                            currentSlide(state, session: session)
                        }
                        .accessibilityLabel("Current slide \(state.currentSlide). Tap for previous.")

                        slideButtons(state, session: session) {
                            if state.currentSlide < state.totalSlides {
                                nextSlideButton(session) {
                                    SlideView(state: state, number: state.currentSlide + 1, theme: session.presenterTheme)
                                        .clipShape(.rect(cornerRadius: 12))
                                        .frame(width: 240)
                                }
                                .accessibilityLabel("Next slide \(state.currentSlide + 1). Tap for next.")
                            } else {
                                endPlaceholder
                                .frame(width: 240)
                                .aspectRatio(16 / 9, contentMode: .fit)
                            }
                        }
                    }
                    #if os(macOS)
                    slidePicker(current: state.currentSlide, total: state.totalSlides, session: session)
                    #endif
                    Spacer(minLength: 0)
                }
                .padding()
            } else if !session.usesWebPresenter, let status = session.presentationStatus, status.appRunning {
                keynoteContent(status, session: session)
                    .padding()
            } else {
                ContentUnavailableView {
                    Label("No Presentation", systemImage: "play.rectangle")
                } description: {
                    Text(session.usesWebPresenter
                        ? "Pick a deck from the list to show it on the output screen."
                        : "Pick a deck from the list to show it in Keynote on the server's Mac.")
                }
            }

            if isLoading(session) {
                openingIndicator
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }
        #if os(iOS)
        // On the left, clear of the floating Settings button on the right.
        .overlay(alignment: .bottomLeading) {
            bottomActions(session).padding(16)
        }
        #endif
    }

    /// The current and next slides: side by side on the Mac, stacked on the narrower iPad.
    @ViewBuilder
    private func slideLayout(@ViewBuilder slides: () -> some View) -> some View {
        #if os(macOS)
        HStack(alignment: .top, spacing: 20, content: slides)
        #else
        VStack(spacing: 20, content: slides)
        #endif
    }

    /// The next slide, with Previous and Next on either side where there is no toolbar to hold them.
    @ViewBuilder
    private func slideButtons(_ state: PresenterStateRecord, session: MetocastSession, @ViewBuilder preview: () -> some View) -> some View {
        #if os(macOS)
        preview()
        #else
        VStack(spacing: 8) {
            HStack(spacing: 16) {
                Button("Previous Slide", systemImage: "chevron.backward") { session.show(.previous) }
                    .keyboardShortcut(.leftArrow, modifiers: [])
                    .buttonStyle(.glass)
                    .disabled(state.currentSlide <= 1)
                preview()
                Button("Next Slide", systemImage: "chevron.forward") { session.show(.next) }
                    .keyboardShortcut(.rightArrow, modifiers: [])
                    .buttonStyle(.glassProminent)
                    .disabled(state.currentSlide >= state.totalSlides)
            }
            .labelStyle(.iconOnly)
            .controlSize(.extraLarge)
            Text("Slide \(state.currentSlide) of \(state.totalSlides)")
                .font(.caption)
                .foregroundStyle(.secondary)
        }
        #endif
    }

    /// A tap steps straight away, and First Slide stays out of reach of a hurried second tap.
    private func previousSlideButton(_ session: MetocastSession, @ViewBuilder label: () -> some View) -> some View {
        Button { session.show(.previous) } label: { label().contentShape(.rect) }
            .buttonStyle(.plain)
            .contextMenu { Button("First Slide", systemImage: "backward.end") { session.show(.first) } }
    }

    private func nextSlideButton(_ session: MetocastSession, @ViewBuilder label: () -> some View) -> some View {
        Button { session.show(.next) } label: { label().contentShape(.rect) }
            .buttonStyle(.plain)
            .contextMenu { Button("Last Slide", systemImage: "forward.end") { session.show(.last) } }
    }

    private var endPlaceholder: some View {
        ZStack {
            Color.black.opacity(0.3)
            Text("End")
                .font(.caption)
                .foregroundStyle(.secondary)
        }
        .clipShape(.rect(cornerRadius: 12))
    }

    /// "Slide 3 of 12", which also jumps to any slide.
    private func currentSlide(_ state: PresenterStateRecord, session: MetocastSession) -> some View {
        SlideView(state: state, number: state.currentSlide, theme: session.presenterTheme)
            .clipShape(.rect(cornerRadius: 12))
            .overlay {
                // The output screen is black; the operator still sees what comes back.
                if state.muted {
                    ZStack {
                        Color.black.opacity(0.75)
                        Label("Blacked Out", systemImage: "eye.slash")
                            .font(.headline)
                            .foregroundStyle(.white)
                    }
                }
            }
    }

    #if os(macOS)
    private func slidePicker(current: UInt32, total: UInt32, session: MetocastSession) -> some View {
        Menu("Slide \(current) of \(total)") {
            ForEach(1...max(total, 1), id: \.self) { slide in
                Button("Slide \(slide)") { session.show(.goTo(slide: slide)) }
            }
        }
        .menuStyle(.button)
        .buttonStyle(.borderless)
        .fixedSize()
    }

    // On the Mac these sit in the toolbar, which draws its own glass.
    @ViewBuilder
    private func buttons(_ session: MetocastSession) -> some View {
        let muted = session.presenter?.muted == true
        let current = session.usesWebPresenter ? (session.presenter?.currentSlide ?? 0) : (session.presentationStatus?.currentSlide ?? 0)
        let total = session.usesWebPresenter ? (session.presenter?.totalSlides ?? 0) : (session.presentationStatus?.totalSlides ?? 0)
        Button("First Slide", systemImage: "backward.end") { session.show(.first) }
            .disabled(current <= 1)
        Button("Previous Slide", systemImage: "chevron.backward") { session.show(.previous) }
            .keyboardShortcut(.leftArrow, modifiers: [])
            .disabled(current <= 1)
        if session.usesWebPresenter {
            // Keynote has no blackout through this API.
            Button {
                session.show(muted ? .unmute : .mute)
            } label: {
                Label {
                    Text(muted ? "Show Slides" : "Black Out")
                } icon: {
                    SwapSymbol(muted ? "eye" : "eye.slash", among: ["eye", "eye.slash"])
                }
            }
            .keyboardShortcut("b", modifiers: [])
        }
        Button("Next Slide", systemImage: "chevron.forward") { session.show(.next) }
            .keyboardShortcut(.rightArrow, modifiers: [])
            .disabled(total > 0 && current >= total)
        Button("Last Slide", systemImage: "forward.end") { session.show(.last) }
            .disabled(total > 0 && current >= total)
    }
    #endif
}

/// One slide as the output screen shows it. Like Sanctum's presenter, Classic shows an imported
/// deck's own artwork and Editorial always shows the styled text.
struct SlideView: View {
    let state: PresenterStateRecord
    let number: UInt32
    let theme: PresenterTheme

    var body: some View {
        ZStack {
            Color.black
            if theme == .classic, state.renderMode == "svg", let slide = state.svgSlides.first(where: { $0.index == number }) {
                SVGView(svg: SVGView.scalable(slide))
                    // The web view would swallow the taps that step through the slides.
                    .allowsHitTesting(false)
            } else if let slide = state.slides.first(where: { $0.index == number }) {
                PresenterTextSlide(
                    paragraphs: slide.paragraphs.map {
                        PresenterParagraph(lines: $0.lines, align: $0.align, fontSizePt: Double($0.fontSizePt))
                    },
                    index: Int(number),
                    fileName: state.filePath == nil ? nil : state.fileName,
                    design: theme == .editorial ? .editorial : .classic,
                    slideWidthEmu: Double(state.slideWidthEmu),
                    slideHeightEmu: Double(max(state.slideHeightEmu, 1))
                )
            }
        }
        .aspectRatio(Double(state.slideWidthEmu) / Double(max(state.slideHeightEmu, 1)), contentMode: .fit)
        // One element with the slide's words, instead of a web view of SVG paths.
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("Slide \(number)")
        .accessibilityValue(
            state.slides.first { $0.index == number }?.paragraphs.flatMap(\.lines).joined(separator: "\n") ?? ""
        )
    }
}

/// An imported slide's artwork. WebKit's SwiftUI `WebPage` renders these blank, so this uses
/// `WKWebView` directly, which both platforms have had for years.
struct SVGView {
    let svg: String

    /// The server's SVG is sized in pixels with no `viewBox`, so scaling it to the view would
    /// show only its top-left corner. This gives it one.
    static func scalable(_ slide: SvgSlideRecord) -> String {
        scalable(slide.svg, width: Int(slide.widthPx), height: Int(slide.heightPx))
    }

    static func scalable(_ svg: String, width: Int, height: Int) -> String {
        guard !svg.contains("viewBox"), let tag = svg.range(of: "<svg") else { return svg }
        return svg.replacingCharacters(
            in: tag,
            with: "<svg viewBox=\"0 0 \(width) \(height)\" preserveAspectRatio=\"xMidYMid meet\""
        )
    }

    static func html(_ svg: String) -> String {
        """
        <html><head><meta name="viewport" content="width=device-width,initial-scale=1">\
        <style>html,body{margin:0;height:100%;background:#000;overflow:hidden}\
        svg{width:100%;height:100%;display:block}</style></head><body>\(svg)</body></html>
        """
    }

    static func makeView() -> WKWebView {
        let view = WKWebView()
        #if os(iOS)
        // The page paints its own black; without this the web view flashes white first.
        view.isOpaque = false
        view.backgroundColor = .black
        view.scrollView.isScrollEnabled = false
        #endif
        return view
    }
}

extension SVGView: @MainActor PlatformViewRepresentable {
    func makeCoordinator() -> Coordinator { Coordinator() }

    final class Coordinator {
        /// What the view already shows, so a re-layout doesn't reload the same slide.
        var loaded: String?
    }

    func make(context: Context) -> WKWebView { Self.makeView() }

    func update(_ view: WKWebView, context: Context) {
        guard context.coordinator.loaded != svg else { return }
        context.coordinator.loaded = svg
        view.loadHTMLString(Self.html(svg), baseURL: nil)
    }
}

#if os(iOS)
protocol PlatformViewRepresentable: UIViewRepresentable where UIViewType == WKWebView {
    func make(context: Context) -> WKWebView
    func update(_ view: WKWebView, context: Context)
}

extension PlatformViewRepresentable {
    func makeUIView(context: Context) -> WKWebView { make(context: context) }
    func updateUIView(_ view: WKWebView, context: Context) { update(view, context: context) }
}
#else
protocol PlatformViewRepresentable: NSViewRepresentable where NSViewType == WKWebView {
    func make(context: Context) -> WKWebView
    func update(_ view: WKWebView, context: Context)
}

extension PlatformViewRepresentable {
    func makeNSView(context: Context) -> WKWebView { make(context: context) }
    func updateNSView(_ view: WKWebView, context: Context) { update(view, context: context) }
}
#endif
