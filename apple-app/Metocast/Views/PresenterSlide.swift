import SwiftUI
import CoreText

/// The presenter's two text designs, drawn natively. This is a port of Sanctum's `/presenter` page
/// (`ui/sanctum/src/routes/presenter/+page.svelte`), so the operator's preview is the output screen's
/// design: Classic is bold white Helvetica on black, Editorial is the warm-black, ivory and gold
/// treatment. Lengths that the page gives in `vw` are fractions of the slide's width here.
enum PresenterDesign {
    case classic
    case editorial
}

struct PresenterParagraph: Hashable {
    var lines: [String]
    var align: String
    var fontSizePt: Double
}

/// One text slide in the chosen design. The slide fills the width it is given at the deck's aspect ratio.
struct PresenterTextSlide: View {
    let paragraphs: [PresenterParagraph]
    /// The slide's number in the deck; the first slide of a song is its title.
    let index: Int
    /// The deck's file name without its extension, shown on Editorial song slides.
    let fileName: String?
    let design: PresenterDesign
    var slideWidthEmu: Double = 12_192_000
    var slideHeightEmu: Double = 6_858_000

    var body: some View {
        GeometryReader { geometry in
            let parts = SlideParts(paragraphs)
            switch design {
            case .classic:
                ClassicSlide(parts: parts, size: geometry.size, slideWidthEmu: slideWidthEmu, slideHeightEmu: slideHeightEmu)
            case .editorial:
                EditorialSlide(parts: parts, index: index, fileName: fileName, size: geometry.size)
            }
        }
        .aspectRatio(slideWidthEmu / max(slideHeightEmu, 1), contentMode: .fit)
        .clipped()
    }
}

// MARK: - Fonts

/// Cormorant Garamond and Geist Mono ship in `Fonts/` (SIL OFL) and are registered for this process,
/// the same faces Sanctum's presenter loads. Without them a system serif and monospace stand in.
enum PresenterFont {
    private static let serifName = "CormorantGaramond-Regular"
    private static let monoName = "GeistMono-Regular"

    static func register(urls: [URL]? = nil) {
        let found = urls ?? [serifName, monoName].compactMap {
            Bundle.main.url(forResource: $0, withExtension: "ttf")
                ?? Bundle.main.url(forResource: $0, withExtension: "ttf", subdirectory: "Fonts")
        }
        CTFontManagerRegisterFontURLs(found as CFArray, .process, true, nil)
    }

    static func serif(_ size: CGFloat, oldstyleFigures: Bool = false) -> CTFont {
        let font = named(serifName, fallback: "Georgia", size: size)
        guard oldstyleFigures else { return font }
        let descriptor = CTFontDescriptorCreateCopyWithFeature(
            CTFontCopyFontDescriptor(font),
            NSNumber(value: kNumberCaseType) as CFNumber,
            NSNumber(value: kLowerCaseNumbersSelector) as CFNumber
        )
        return CTFontCreateWithFontDescriptor(descriptor, size, nil)
    }

    static func mono(_ size: CGFloat) -> CTFont {
        named(monoName, fallback: "Menlo", size: size)
    }

    /// A font's line height with no extra leading, in points at its size.
    static func lineHeight(_ font: CTFont) -> CGFloat {
        CTFontGetAscent(font) + CTFontGetDescent(font) + CTFontGetLeading(font)
    }

    /// How wide a line of text sets in `font`.
    static func width(_ text: String, font: CTFont) -> CGFloat {
        let line = CTLineCreateWithAttributedString(NSAttributedString(string: text, attributes: [.font: font]))
        return CTLineGetTypographicBounds(line, nil, nil, nil)
    }

    /// The advance of "0": CSS `ch`.
    static func zeroAdvance(_ font: CTFont) -> CGFloat {
        var glyph = CGGlyph()
        var zero: UniChar = 0x30
        CTFontGetGlyphsForCharacters(font, &zero, &glyph, 1)
        return CTFontGetAdvancesForGlyphs(font, .horizontal, &glyph, nil, 1)
    }

    private static func named(_ name: String, fallback: String, size: CGFloat) -> CTFont {
        let font = CTFontCreateWithName(name as CFString, size, nil)
        return CTFontCopyPostScriptName(font) as String == name ? font : CTFontCreateWithName(fallback as CFString, size, nil)
    }
}

// MARK: - What the page works out before it draws

private struct SlideParts {
    let main: [PresenterParagraph]
    /// The slide number or verse reference, drawn apart from the text.
    let counter: PresenterParagraph?

    init(_ paragraphs: [PresenterParagraph]) {
        let index = Self.counterIndex(paragraphs)
        counter = index.map { paragraphs[$0] }
        main = paragraphs.enumerated().filter { $0.offset != index }.map(\.element)
    }

    /// A centred paragraph under 85 % of the largest size, first or last — a deck puts its text box
    /// before or after the lyrics.
    private static func counterIndex(_ paragraphs: [PresenterParagraph]) -> Int? {
        guard paragraphs.count >= 2 else { return nil }
        let largest = paragraphs.map(\.fontSizePt).max() ?? 0
        guard largest > 0 else { return nil }
        func isCounter(_ paragraph: PresenterParagraph) -> Bool {
            paragraph.fontSizePt > 0 && paragraph.fontSizePt < largest * 0.85 && paragraph.align == "center"
        }
        if isCounter(paragraphs[0]) { return 0 }
        return isCounter(paragraphs[paragraphs.count - 1]) ? paragraphs.count - 1 : nil
    }
}

// MARK: - Classic

private struct ClassicSlide: View {
    let parts: SlideParts
    let size: CGSize
    let slideWidthEmu: Double
    let slideHeightEmu: Double

    var body: some View {
        // The page pads the slide 3vw 4vw 1.5vw and fits the deck's own size into what is left.
        let pad = (top: size.width * 0.03, side: size.width * 0.04, bottom: size.width * 0.015)
        let area = CGSize(width: size.width - 2 * pad.side, height: size.height - pad.top - pad.bottom)
        let pointsToScreen = min(area.width / slideWidthEmu, area.height / slideHeightEmu) * 12_700
        let wraps = parts.counter != nil

        ZStack {
            Color.black
            VStack(spacing: 0) {
                ForEach(Array(parts.main.enumerated()), id: \.offset) { offset, paragraph in
                    ParagraphText(paragraph: paragraph, pointsToScreen: pointsToScreen, wraps: wraps, available: area.width)
                        .padding(.top, offset > 0 ? 0.35 * Self.pixels(paragraph, pointsToScreen) : 0)
                }
            }
            .padding(EdgeInsets(top: pad.top, leading: pad.side, bottom: pad.bottom, trailing: pad.side))
            .frame(maxWidth: .infinity, maxHeight: .infinity)

            if let counter = parts.counter {
                Text(counter.lines.joined(separator: "\n"))
                    .font(.custom("Helvetica-Bold", fixedSize: 18 * pointsToScreen))
                    .multilineTextAlignment(.center)
                    .foregroundStyle(.white)
                    .padding(.horizontal, size.width * 0.10)
                    .padding(.bottom, size.height * 0.08)
                    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .bottom)
            }
        }
        .frame(width: size.width, height: size.height)
    }

    fileprivate static func pixels(_ paragraph: PresenterParagraph, _ pointsToScreen: Double) -> Double {
        (paragraph.fontSizePt > 0 ? paragraph.fontSizePt : 28) * pointsToScreen
    }

    private struct ParagraphText: View {
        let paragraph: PresenterParagraph
        let pointsToScreen: Double
        let wraps: Bool
        let available: Double

        var body: some View {
            let px = ClassicSlide.pixels(paragraph, pointsToScreen)
            let font = Font.custom("Helvetica-Bold", fixedSize: px)
            let measuring = CTFontCreateWithName("Helvetica-Bold" as CFString, px, nil)
            let frameAlignment: Alignment = switch paragraph.align {
            case "center": .center
            case "right": .trailing
            default: .leading
            }
            if wraps {
                Text(paragraph.lines.joined(separator: "\n"))
                    .font(font)
                    .multilineTextAlignment(paragraph.align == "center" ? .center : paragraph.align == "right" ? .trailing : .leading)
                    .foregroundStyle(.white)
                    .frame(maxWidth: .infinity, alignment: frameAlignment)
            } else {
                // Entered lines stay on one line. As in the browser, a line too wide for the slide
                // starts at the left edge and runs off the right instead of being centred.
                VStack(spacing: 0) {
                    ForEach(Array(paragraph.lines.enumerated()), id: \.offset) { _, line in
                        let overflows = PresenterFont.width(line, font: measuring) > available
                        Text(line.isEmpty ? " " : line)
                            .font(font)
                            .foregroundStyle(.white)
                            .lineLimit(1)
                            .fixedSize()
                            .frame(width: available, height: 1.2 * px, alignment: overflows ? .leading : frameAlignment)
                    }
                }
            }
        }
    }
}

// MARK: - Editorial

private enum Gold {
    static let background = Color(red: 0x11 / 255, green: 0x10 / 255, blue: 0x0E / 255)
    static let ivory = Color(red: 0xF4 / 255, green: 0xEF / 255, blue: 0xE3 / 255)
    static let accent = Color(red: 0xCF / 255, green: 0xAB / 255, blue: 0x69 / 255)
}

private struct EditorialSlide: View {
    let parts: SlideParts
    let index: Int
    let fileName: String?
    let size: CGSize

    private var counterText: String {
        (parts.counter?.lines.joined(separator: " ") ?? "").trimmingCharacters(in: .whitespacesAndNewlines)
    }

    private var isBible: Bool {
        counterText.range(of: #"^(Textus|Lekció)\b"#, options: .regularExpression) != nil
    }

    private var isBlank: Bool {
        parts.main.allSatisfy { $0.lines.allSatisfy { $0.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty } }
    }

    var body: some View {
        ZStack {
            if isBlank {
                Gold.background
            } else {
                Backdrop(size: size)
                if isBible {
                    BibleLayout(parts: parts, counter: counterText, size: size)
                } else {
                    SongLayout(lines: parts.main.flatMap(\.lines), isTitle: index == 1, index: index, fileName: fileName, size: size)
                }
                MonoLabel(text: "Metocast", vw: 0.55, spacing: 0.27, color: Gold.ivory.opacity(0.44), width: size.width)
                    .padding(.trailing, size.width * 0.048)
                    .padding(.bottom, size.height * 0.052)
                    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .bottomTrailing)
            }
        }
        .frame(width: size.width, height: size.height)
    }
}

/// Warm black with a gold glow at the top right, a faint lift in the middle and one slanted band of light.
private struct Backdrop: View {
    let size: CGSize

    var body: some View {
        let w = size.width, h = size.height
        ZStack {
            Gold.background
            glow(x: 0.86, y: 0.08, color: Gold.accent.opacity(0.105), reach: 0.27)
            glow(x: 0.54, y: 0.56, color: Color(red: 1, green: 246 / 255, blue: 224 / 255).opacity(0.024), reach: 0.48)
            Rectangle()
                .fill(LinearGradient(stops: [
                    .init(color: band(0), location: 0),
                    .init(color: band(0.022), location: 0.44),
                    .init(color: band(0.052), location: 0.5),
                    .init(color: band(0.016), location: 0.57),
                    .init(color: band(0), location: 1),
                ], startPoint: .leading, endPoint: .trailing))
                .frame(width: 0.54 * w, height: 2.35 * h)
                .rotationEffect(.degrees(28))
                .position(x: 1.08 * w - 0.27 * w, y: -0.7 * h + 1.175 * h)
        }
        .frame(width: w, height: h)
        .clipped()
    }

    private func band(_ opacity: Double) -> Color {
        Color(red: 240 / 255, green: 222 / 255, blue: 184 / 255).opacity(opacity)
    }

    /// A circle that fades out `reach` of the way to the farthest corner, like `farthest-corner` in CSS.
    private func glow(x: Double, y: Double, color: Color, reach: Double) -> some View {
        let cx = x * size.width, cy = y * size.height
        let farthest = [(0.0, 0.0), (size.width, 0.0), (0.0, size.height), (size.width, size.height)]
            .map { hypot($0.0 - cx, $0.1 - cy) }.max() ?? 0
        return RadialGradient(
            colors: [color, color.opacity(0)],
            center: UnitPoint(x: x, y: y),
            startRadius: 0,
            endRadius: reach * farthest
        )
    }
}

/// Small gold monospace caps, spaced wide.
private struct MonoLabel: View {
    let text: String
    let vw: Double
    let spacing: Double
    let color: Color
    let width: Double

    var body: some View {
        let px = vw * width / 100
        Text(text.uppercased())
            .font(Font(PresenterFont.mono(px)))
            .tracking(spacing * px)
            .foregroundStyle(color)
            .lineLimit(1)
            .fixedSize()
    }
}

private struct SongLayout: View {
    let lines: [String]
    let isTitle: Bool
    let index: Int
    let fileName: String?
    let size: CGSize

    var body: some View {
        let longest = max(1, lines.map { $0.utf16.count }.max() ?? 1)
        // `vw`: the largest size that fits 77 vw across and 28 vw down.
        let fontVw = min(isTitle ? 5.4 : 4.1, 77 / (Double(longest) * 0.55), 28 / Double(max(1, lines.count)))
        let px = fontVw * size.width / 100
        let lineHeight = (isTitle ? 1.08 : 1.18) * px

        ZStack {
            VStack(spacing: 0) {
                ForEach(Array(lines.enumerated()), id: \.offset) { _, line in
                    Text(line.isEmpty ? "\u{00A0}" : line)
                        .font(Font(PresenterFont.serif(px)))
                        .tracking(-0.018 * px)
                        .foregroundStyle(Gold.ivory)
                        .lineLimit(1)
                        .fixedSize()
                        .frame(height: lineHeight)
                }
            }
            .frame(width: size.width * 0.86)
            .offset(y: size.width * 0.005 + 0.02 * lineHeight * Double(lines.count))

            MonoLabel(text: eyebrow, vw: 0.78, spacing: 0.34, color: Gold.accent, width: size.width)
                .frame(width: size.width * 0.8)
                .padding(.top, size.height * 0.14)
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)

            if !isTitle {
                MonoLabel(text: "\(index). dia", vw: 0.64, spacing: 0.31, color: Gold.accent, width: size.width)
                    .padding(.bottom, size.height * 0.125)
                    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .bottom)
            }
        }
    }

    private var eyebrow: String {
        if isTitle { return "Ének" }
        return fileName.map { "Ének · \($0)" } ?? "Ének"
    }
}

/// A verse number in large gold figures beside the passage.
private struct BibleLayout: View {
    let parts: SlideParts
    let counter: String
    let size: CGSize

    var body: some View {
        let width = size.width
        let padLeading = width * 0.074, padTrailing = width * 0.083
        let content = width - padLeading - padTrailing
        let gap = content * 0.072
        let numberColumn = content * 0.19
        let textColumn = content - numberColumn - gap

        let length = parts.main.reduce(0) { $0 + $1.lines.reduce(0) { $0 + $1.utf16.count } }
        let (textVw, chars): (Double, Double) = length > 300 ? (2.7, 31) : length > 180 ? (3.35, 25) : (4.1, 19)
        let textPx = textVw * width / 100
        let serif = PresenterFont.serif(textPx)
        let numberPx = 12.8 * width / 100

        VStack(alignment: .leading, spacing: 0) {
            HStack(spacing: gap) {
                Color.clear.frame(width: numberColumn, height: 1)
                MonoLabel(text: label, vw: 0.78, spacing: 0.31, color: Gold.accent, width: width)
                    .padding(.bottom, 0.058 * textColumn)
                    .frame(width: textColumn, alignment: .leading)
            }
            HStack(alignment: .top, spacing: gap) {
                Text(verseNumber)
                    .font(Font(PresenterFont.serif(numberPx, oldstyleFigures: true)))
                    .foregroundStyle(Gold.accent)
                    .lineLimit(1)
                    .fixedSize()
                    .frame(width: numberColumn, height: 0.76 * numberPx, alignment: .trailing)
                VStack(alignment: .leading, spacing: 0.28 * textPx) {
                    ForEach(Array(parts.main.enumerated()), id: \.offset) { _, paragraph in
                        Text(paragraph.lines.joined(separator: "\n"))
                            .font(Font(serif))
                            .tracking(-0.018 * textPx)
                            .lineSpacing(1.14 * textPx - PresenterFont.lineHeight(serif))
                            .foregroundStyle(Gold.ivory)
                            .fixedSize(horizontal: false, vertical: true)
                    }
                }
                .frame(maxWidth: chars * PresenterFont.zeroAdvance(serif), alignment: .leading)
                .frame(width: textColumn, alignment: .leading)
            }
        }
        .padding(.leading, padLeading)
        .padding(.trailing, padTrailing)
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .leading)
    }

    /// "Textus · Jn 3:16 · 1-2", without the trailing "(1/3)".
    private var label: String {
        counter
            .replacingOccurrences(of: #"\s*\(\d+\/\d+\)\s*$"#, with: "", options: .regularExpression)
            .replacingOccurrences(of: #"\s*\|\s*"#, with: " · ", options: .regularExpression)
    }

    /// The verse the page ends on, from the last `chapter:verse` in the reference.
    private var verseNumber: String {
        guard let regex = try? NSRegularExpression(pattern: #"\d+:(\d+)"#),
              let match = regex.matches(in: counter, range: NSRange(counter.startIndex..., in: counter)).last,
              let digits = Range(match.range(at: 1), in: counter) else { return "" }
        return String(counter[digits])
    }
}
