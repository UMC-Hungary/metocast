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
    }

    private static let presenter = Presenter()

    static func post(_ message: String) {
        let center = UNUserNotificationCenter.current()
        center.delegate = presenter
        Task {
            // Asks once; later calls return the answer without a prompt.
            guard (try? await center.requestAuthorization(options: [.alert, .sound])) == true else { return }
            let content = UNMutableNotificationContent()
            content.title = "Metocast"
            content.body = message
            try? await center.add(UNNotificationRequest(identifier: UUID().uuidString, content: content, trigger: nil))
        }
    }
}
