import SwiftUI
import WebKit
import Metocast

/// The live presenter: the slide on the output screen and the controls that move it. In Keynote
/// mode the server has the slides, so this shows what Keynote reports instead of the artwork.
struct PresentationView: View {
    @Environment(AppModel.self) private var model
    @State private var isBrowsingSlides = false

    var body: some View {
        Group {
            if let session = model.session {
                content(session)
                    .toolbar {
                        ToolbarItem(placement: .primaryAction) {
                            Button("Open Slides", systemImage: "folder") { isBrowsingSlides = true }
                        }
                        if isShowing(session) {
                            ToolbarItem {
                                Button("Close Slides", systemImage: "xmark.circle") { session.show(.close) }
                            }
                        }
                        #if os(macOS)
                        if isShowing(session) {
                            ToolbarItemGroup { buttons(session) }
                        }
                        #endif
                    }
                    .sheet(isPresented: $isBrowsingSlides) {
                        SlideBrowserView(session: session)
                    }
            } else {
                ContentUnavailableView("No Presentation", systemImage: "play.rectangle")
                    .navigationTitle("Presentation")
            }
        }
    }

    @ViewBuilder
    private func content(_ session: MetocastSession) -> some View {
        if session.usesWebPresenter, let state = session.presenter, state.loaded {
            VStack(spacing: 20) {
                SlideView(state: state, number: state.currentSlide)
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
            .navigationTitle(status.documentName ?? "Presentation")
        } else {
            ContentUnavailableView {
                Label("No Presentation", systemImage: "play.rectangle")
            } description: {
                Text(session.usesWebPresenter
                    ? "Open a deck to show it on the output screen."
                    : "Open a deck to show it in Keynote on the server's Mac.")
            } actions: {
                Button("Open Slides…") { isBrowsingSlides = true }
                    .buttonStyle(.borderedProminent)
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

/// One slide as the output screen shows it: the imported SVG artwork or the styled text.
struct SlideView: View {
    let state: PresenterStateRecord
    let number: UInt32

    var body: some View {
        ZStack {
            Color.black
            if state.renderMode == "svg", let slide = state.svgSlides.first(where: { $0.index == number }) {
                SVGView(svg: SVGView.scalable(slide))
            } else if let slide = state.slides.first(where: { $0.index == number }) {
                GeometryReader { geometry in
                    // Font sizes are in points on the slide; slide sizes are in EMU (12,700 per point).
                    let scale = geometry.size.height / (Double(max(state.slideHeightEmu, 1)) / 12_700)
                    VStack(spacing: 12 * scale) {
                        ForEach(Array(slide.paragraphs.enumerated()), id: \.offset) { _, paragraph in
                            Text(paragraph.lines.joined(separator: "\n"))
                                .font(.system(size: CGFloat(paragraph.fontSizePt > 0 ? paragraph.fontSizePt : 32) * scale))
                                .multilineTextAlignment(paragraph.textAlignment)
                                .frame(maxWidth: .infinity, alignment: paragraph.frameAlignment)
                        }
                    }
                    .foregroundStyle(.white)
                    .minimumScaleFactor(0.3)
                    .padding(36 * scale)
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                }
            }
        }
        .aspectRatio(Double(state.slideWidthEmu) / Double(max(state.slideHeightEmu, 1)), contentMode: .fit)
    }
}

/// An imported slide's artwork. WebKit's SwiftUI `WebPage` renders these blank, so this uses
/// `WKWebView` directly, which both platforms have had for years.
private struct SVGView {
    let svg: String

    /// The server's SVG is sized in pixels with no `viewBox`, so scaling it to the view would
    /// show only its top-left corner. This gives it one.
    static func scalable(_ slide: SvgSlideRecord) -> String {
        guard !slide.svg.contains("viewBox"), let tag = slide.svg.range(of: "<svg") else { return slide.svg }
        return slide.svg.replacingCharacters(
            in: tag,
            with: "<svg viewBox=\"0 0 \(slide.widthPx) \(slide.heightPx)\" preserveAspectRatio=\"xMidYMid meet\""
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

private extension ParagraphRecord {
    var textAlignment: TextAlignment {
        switch align {
        case "center": .center
        case "right": .trailing
        default: .leading
        }
    }

    var frameAlignment: Alignment {
        switch align {
        case "center": .center
        case "right": .trailing
        default: .leading
        }
    }
}
