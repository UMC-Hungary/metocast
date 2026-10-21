import UserNotifications

/// A system banner for something the operator has to act on, shown even while Metocast is in front.
enum SystemNotice {
    private final class Presenter: NSObject, UNUserNotificationCenterDelegate {
        nonisolated func userNotificationCenter(
            _ center: UNUserNotificationCenter,
            willPresent notification: UNNotification
        ) async -> UNNotificationPresentationOptions {
            [.banner, .list, .sound]
        }

        nonisolated func userNotificationCenter(_ center: UNUserNotificationCenter, didReceive response: UNNotificationResponse) async {
            let pane = (response.notification.request.content.userInfo["pane"] as? String).flatMap(MetocastPane.init)
            await MainActor.run { if let pane { SystemNotice.open(pane) } }
        }
    }

    private static let presenter = Presenter()

    /// Shows the page a tapped notification names.
    static var open: (MetocastPane) -> Void = { _ in }

    /// Asks for permission now, while the operator is here to answer, instead of on the first banner.
    static func ask() {
        let center = UNUserNotificationCenter.current()
        center.delegate = presenter
        Task { _ = try? await center.requestAuthorization(options: [.alert, .sound]) }
    }

    static func post(_ message: String, opening pane: MetocastPane? = nil) {
        let center = UNUserNotificationCenter.current()
        center.delegate = presenter
        Task {
            // Asks once; later calls return the answer without a prompt.
            guard (try? await center.requestAuthorization(options: [.alert, .sound])) == true else { return }
            let content = UNMutableNotificationContent()
            content.title = "Metocast"
            content.body = message
            if let pane { content.userInfo = ["pane": pane.rawValue] }
            try? await center.add(UNNotificationRequest(identifier: UUID().uuidString, content: content, trigger: nil))
        }
    }
}
