#if os(iOS)
import SwiftUI
import VisionKit

/// Native VisionKit QR scanner sheet for connecting to a Metocast server.
struct QRScannerSheet: View {
    @Environment(\.dismiss) private var dismiss
    let onScan: (ConnectLink) -> Void

    @State private var scanError: String?

    var body: some View {
        NavigationStack {
            ZStack {
                if DataScannerViewController.isSupported && DataScannerViewController.isAvailable {
                    DataScannerRepresentable { text in
                        if let link = ConnectLink(scanned: text) {
                            UIImpactFeedbackGenerator(style: .medium).impactOccurred()
                            onScan(link)
                            dismiss()
                        } else {
                            UINotificationFeedbackGenerator().notificationOccurred(.warning)
                            scanError = String(localized: "This QR code isn't a Metocast server connection code.")
                        }
                    }
                    .ignoresSafeArea()
                } else {
                    ContentUnavailableView(
                        "Camera Unavailable",
                        systemImage: "camera.badge.ellipsis",
                        description: Text("This device or simulator cannot scan QR codes. Enter the server address manually.")
                    )
                }

                VStack {
                    GlassEffectContainer(spacing: 8) {
                        Text("Point the camera at the QR code on your server")
                            .font(.subheadline.weight(.medium))
                            .foregroundStyle(.primary)
                            .padding(.horizontal, 16)
                            .padding(.vertical, 10)
                            .glassEffect(.regular, in: .capsule)
                    }
                    .padding(.top, 16)

                    Spacer()

                    if let scanError {
                        Text(scanError)
                            .font(.caption.weight(.semibold))
                            .foregroundStyle(.white)
                            .padding(.horizontal, 16)
                            .padding(.vertical, 8)
                            .background(Color.red.opacity(0.85), in: .capsule)
                            .padding(.bottom, 8)
                    }

                    GlassEffectContainer(spacing: 12) {
                        Button("Cancel", role: .cancel) { dismiss() }
                            .buttonStyle(.glass)
                            .controlSize(.large)
                    }
                    .padding(.bottom, 28)
                }
            }
            .navigationTitle("Scan Server QR Code")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { dismiss() }
                }
            }
        }
    }
}

private struct DataScannerRepresentable: UIViewControllerRepresentable {
    let onScan: (String) -> Void

    func makeUIViewController(context: Context) -> DataScannerViewController {
        let scanner = DataScannerViewController(
            recognizedDataTypes: [.barcode(symbologies: [.qr])],
            qualityLevel: .balanced,
            recognizesMultipleItems: false,
            isHighFrameRateTrackingEnabled: false,
            isPinchToZoomEnabled: true,
            isGuidanceEnabled: true,
            isHighlightingEnabled: true
        )
        scanner.delegate = context.coordinator
        try? scanner.startScanning()
        return scanner
    }

    func updateUIViewController(_ uiViewController: DataScannerViewController, context: Context) {}

    func makeCoordinator() -> Coordinator { Coordinator(onScan: onScan) }

    final class Coordinator: NSObject, DataScannerViewControllerDelegate {
        let onScan: (String) -> Void
        private var didScan = false

        init(onScan: @escaping (String) -> Void) {
            self.onScan = onScan
        }

        func dataScanner(_ dataScanner: DataScannerViewController, didAdd addedItems: [RecognizedItem], allItems: [RecognizedItem]) {
            guard !didScan, let item = addedItems.first else { return }
            if case .barcode(let barcode) = item, let payload = barcode.payloadStringValue {
                didScan = true
                onScan(payload)
            }
        }
    }
}
#endif
