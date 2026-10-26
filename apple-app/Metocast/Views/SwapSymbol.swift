import SwiftUI

/// An SF Symbol that swaps in place: it always takes the size of the largest of `symbols`,
/// so a button whose icon toggles (copy → checkmark, eye → eye.slash) never changes size.
struct SwapSymbol: View {
    let name: String
    let symbols: [String]

    init(_ name: String, among symbols: [String]) {
        self.name = name
        self.symbols = symbols
    }

    var body: some View {
        ZStack {
            ForEach(symbols, id: \.self) { Image(systemName: $0).hidden() }
            Image(systemName: name).contentTransition(.symbolEffect(.replace))
        }
    }
}
