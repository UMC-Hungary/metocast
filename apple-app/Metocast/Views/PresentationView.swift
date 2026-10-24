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
    @State private var isShowingOrder = true

    var body: some View {
        Group {
            if let session = model.session {
                page(session)
                    .toolbar {
                        // Page actions carry their names; the shell's icons around them would otherwise blur together.
                        ToolbarItemGroup {
                            Group {
                                Button("New Song", systemImage: "music.note.list") { Task { await newSong(session) } }
                                if isShowing(session) {
                                    Button("Close Slides", systemImage: "xmark.circle") { session.show(.close) }
                                }
                                Button("Presentation Settings", systemImage: "slider.horizontal.3") { isShowingSettings = true }
                            }
                            #if os(macOS)
                            .labelStyle(.titleAndIcon)
                            #endif
                        }
                        #if os(macOS)
                        if isShowing(session) {
                            ToolbarItemGroup { buttons(session) }
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
            } else {
                ContentUnavailableView("No Presentation", systemImage: "play.rectangle")
                    .navigationTitle("Presentation")
            }
        }
    }

    @ViewBuilder
    private func page(_ session: MetocastSession) -> some View {
        let library = sidebar(session)
        #if os(macOS)
        HStack(spacing: 0) {
            library.frame(width: 340)
            Divider()
            content(session)
                .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
        #else
        VStack(spacing: 0) {
            if isShowing(session) {
                content(session).frame(maxHeight: 360)
            }
            library
        }
        .navigationTitle("Presentation")
        #endif
    }

    /// The event to step through: the one on screen, else the one under way or next.
    private func orderEvent(_ session: MetocastSession) -> EventSummaryRecord? {
        session.events.first { $0.id == session.orderPosition?.eventId }
            ?? EventSchedule.current(in: session.events, now: .now)
    }

    /// The deck library, and the event's Order of Service beside it, so the service can be stepped
    /// through from here, with the web presenter or Keynote.
    @ViewBuilder
    private func sidebar(_ session: MetocastSession) -> some View {
        let event = orderEvent(session)
        VStack(spacing: 0) {
            if event != nil {
                Picker("Show", selection: $isShowingOrder) {
                    Text("Order of Service").tag(true)
                    Text("Library").tag(false)
                }
                .pickerStyle(.segmented)
                .labelsHidden()
                .padding(10)
            }
            if let event, isShowingOrder {
                Form { LiveOrderSection(session: session, event: event) }
                    .formStyle(.grouped)
            } else {
                DeckList(session: session, query: query) { file in
                    session.show(.open(filePath: file.path))
                }
                .librarySearch($query)
            }
        }
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

    @ViewBuilder
    private func content(_ session: MetocastSession) -> some View {
        if session.usesWebPresenter, let state = session.presenter, state.loaded {
            VStack(spacing: 20) {
                // The Mac puts the next slide beside the current one, like a presenter view; the iPhone below it.
                #if os(macOS)
                HStack(alignment: .top, spacing: 20) {
                    currentSlide(state, session: session)
                    nextSlide(state, session: session).frame(width: 240)
                }
                #else
                currentSlide(state, session: session)
                nextSlide(state, session: session).frame(maxWidth: 240)
                #endif
                slidePicker(current: state.currentSlide, total: state.totalSlides, session: session)
                #if os(iOS)
                controls(session)
                #endif
                Spacer(minLength: 0)
            }
            .padding()
            .navigationTitle(state.fileName)
        } else if !session.usesWebPresenter, let status = session.presentationStatus, status.appRunning {
            // Keynote holds the slides and sends only its position, so there is nothing to draw.
            VStack(spacing: 20) {
                ContentUnavailableView {
                    Label(status.documentName ?? "Keynote", systemImage: "play.display")
                } description: {
                    Text(status.slideshowActive ? "Playing in Keynote." : "Open in Keynote, not playing.")
                }
                if let current = status.currentSlide, let total = status.totalSlides {
                    slidePicker(current: current, total: total, session: session)
                }
                Button(status.slideshowActive ? "Stop Slideshow" : "Play Slideshow", systemImage: status.slideshowActive ? "stop" : "play") {
                    session.show(status.slideshowActive ? .stop : .start)
                }
                .buttonStyle(.bordered)
                #if os(iOS)
                controls(session)
                #endif
                Spacer(minLength: 0)
            }
            .padding()
            .navigationTitle(status.documentName ?? String(localized: "Presentation"))
        } else {
            ContentUnavailableView {
                Label("No Presentation", systemImage: "play.rectangle")
            } description: {
                Text(session.usesWebPresenter
                    ? "Pick a deck from the list to show it on the output screen."
                    : "Pick a deck from the list to show it in Keynote on the server's Mac.")
            }
            .navigationTitle("Presentation")
        }
    }

    private func isShowing(_ session: MetocastSession) -> Bool {
        session.usesWebPresenter
            ? session.presenter?.loaded == true
            : session.presentationStatus?.appRunning == true
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

    /// The slide after this one, small; a click moves to it.
    private func nextSlide(_ state: PresenterStateRecord, session: MetocastSession) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            Text("Next").font(.subheadline).foregroundStyle(.secondary)
            if state.currentSlide < state.totalSlides {
                Button { session.show(.next) } label: {
                    SlideView(state: state, number: state.currentSlide + 1, theme: session.presenterTheme)
                        .clipShape(.rect(cornerRadius: 8))
                }
                .buttonStyle(.plain)
                .accessibilityLabel("Next slide, \(state.currentSlide + 1)")
                .accessibilityHint("Shows it on the output screen.")
            } else {
                Text("End of deck").foregroundStyle(.secondary)
            }
        }
    }

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

    #if os(iOS)
    // Transport controls float on the content like other playback controls, so they share one glass group.
    private func controls(_ session: MetocastSession) -> some View {
        GlassEffectContainer(spacing: 12) {
            HStack(spacing: 12) {
                buttons(session)
            }
            .buttonStyle(.glass)
            .labelStyle(.iconOnly)
            .controlSize(.large)
        }
    }
    #endif

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
            Button(muted ? "Show Slides" : "Black Out", systemImage: muted ? "eye" : "eye.slash") {
                session.show(muted ? .unmute : .mute)
            }
            .keyboardShortcut("b", modifiers: [])
        }
        Button("Next Slide", systemImage: "chevron.forward") { session.show(.next) }
            #if os(iOS)
            .buttonStyle(.glassProminent)
            #endif
            .keyboardShortcut(.rightArrow, modifiers: [])
            .disabled(total > 0 && current >= total)
        Button("Last Slide", systemImage: "forward.end") { session.show(.last) }
            .disabled(total > 0 && current >= total)
    }
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
