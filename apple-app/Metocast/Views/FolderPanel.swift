#if os(macOS)
import AppKit

/// The system's folder panel. Used instead of `.fileImporter`: a second importer in the same
/// window silently stops working, so every folder prompt goes through this one function.
@MainActor
func chooseFolder() -> URL? {
    let panel = NSOpenPanel()
    panel.canChooseFiles = false
    panel.canChooseDirectories = true
    panel.canCreateDirectories = true
    panel.prompt = "Choose"
    return panel.runModal() == .OK ? panel.url : nil
}
#endif
