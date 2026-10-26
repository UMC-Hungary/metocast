import SwiftUI
#if os(macOS)
import AppKit
#else
import UIKit
#endif

/// Links another device can open, best first, each shown in full with its own copy button.
/// Under `-MetocastRedact` the token in a link is masked on screen; the copy is the real link.
struct CopyableLinks: View {
    let urls: [String]
    @State private var copied: String?

    var body: some View {
        GlassEffectContainer(spacing: 8) {
            VStack(alignment: .leading, spacing: 8) {
                ForEach(urls, id: \.self) { url in
                    HStack {
                        Text(Self.display(url))
                            .font(.footnote.monospaced())
                            .textSelection(.enabled)
                            .frame(maxWidth: .infinity, alignment: .leading)
                        Button {
                            #if os(macOS)
                            NSPasteboard.general.clearContents()
                            NSPasteboard.general.setString(url, forType: .string)
                            #else
                            UIPasteboard.general.string = url
                            #endif
                            copied = url
                        } label: {
                            Label {
                                Text(copied == url ? "Copied" : "Copy")
                            } icon: {
                                SwapSymbol(copied == url ? "checkmark" : "doc.on.doc", among: ["checkmark", "doc.on.doc"])
                            }
                        }
                        .labelStyle(.iconOnly)
                        .buttonStyle(.glass)
                        .accessibilityLabel(copied == url ? "Copied" : "Copy Link")
                    }
                    .padding(.horizontal, 10)
                    .padding(.vertical, 6)
                    .glassEffect(.regular, in: .rect(cornerRadius: 10))
                }
            }
        }
        .task(id: copied) {
            guard copied != nil else { return }
            try? await Task.sleep(for: .seconds(2))
            copied = nil
        }
    }

    private static func display(_ url: String) -> String {
        guard Redaction.isOn, var components = URLComponents(string: url) else { return url }
        components.queryItems = components.queryItems?.map {
            $0.name == "token" ? URLQueryItem(name: "token", value: "•••••") : $0
        }
        return components.string ?? url
    }
}
