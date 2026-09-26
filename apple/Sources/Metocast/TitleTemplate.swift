import Foundation

/// The values a title template can place: `{date}`, `{title}`, `{textus}`, `{leckio}`, `{speaker}`.
public struct TitleValues: Sendable {
    public var date: Date?
    public var title: String
    public var textus: String
    public var leckio: String
    public var speaker: String

    public init(date: Date?, title: String, textus: String = "", leckio: String = "", speaker: String = "") {
        self.date = date
        self.title = title
        self.textus = textus
        self.leckio = leckio
        self.speaker = speaker
    }
}

/// Renders an event's published title from the server's title template. Kept in step with
/// `renderTitle` in `packages/core-client/src/utils/title-template.ts`: `{name}` placeholders,
/// a `|pattern` only `date` reads (`{date|YYYY.MM.DD.}`), and `[ … ]` groups that drop out
/// whole when any placeholder inside them is empty.
public func renderTitle(_ template: String, _ values: TitleValues, locale: Locale = .current) -> String {
    let placeholder = /\{(\w+)(?:\|([^}]*))?\}/
    func read(_ name: Substring, _ pattern: Substring?) -> String {
        switch name {
        case "date": values.date.map { formatDate($0, String(pattern ?? "YYYY.MM.DD."), locale) } ?? ""
        case "title": values.title.trimmingCharacters(in: .whitespaces)
        case "textus": values.textus.trimmingCharacters(in: .whitespaces)
        case "leckio": values.leckio.trimmingCharacters(in: .whitespaces)
        case "speaker": values.speaker.trimmingCharacters(in: .whitespaces)
        default: ""
        }
    }
    let kept = template.replacing(/\[([^\]]*)\]/) { group in
        let inner = group.output.1
        return inner.matches(of: placeholder).allSatisfy { !read($0.output.1, nil).isEmpty } ? String(inner) : ""
    }
    let filled = kept.replacing(placeholder) { read($0.output.1, $0.output.2) }
    return filled.split(whereSeparator: \.isWhitespace).joined(separator: " ")
}

private func formatDate(_ date: Date, _ pattern: String, _ locale: Locale) -> String {
    let parts = Calendar.current.dateComponents([.year, .month, .day, .hour, .minute], from: date)
    let pad = { (value: Int?) in String(format: "%02d", value ?? 0) }
    // Longest first: the alternation is leftmost-wins, so YYYY must beat YY.
    return pattern.replacing(/YYYY|MMMM|dddd|MM|DD|HH|mm|YY|M|D/) { token in
        switch token.output {
        case "YYYY": String(parts.year ?? 0)
        case "YY": String(String(parts.year ?? 0).suffix(2))
        case "MMMM": date.formatted(.dateTime.month(.wide).locale(locale))
        case "dddd": date.formatted(.dateTime.weekday(.wide).locale(locale))
        case "MM": pad(parts.month)
        case "DD": pad(parts.day)
        case "HH": pad(parts.hour)
        case "mm": pad(parts.minute)
        case "M": String(parts.month ?? 0)
        default: String(parts.day ?? 0)
        }
    }
}
