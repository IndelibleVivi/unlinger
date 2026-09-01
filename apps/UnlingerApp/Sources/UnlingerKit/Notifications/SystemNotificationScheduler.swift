import AppKit
import UserNotifications

/// Long-lived notification center delegate. It deliberately exposes only
/// public route kind, redacted incident ID, and public event token in userInfo.
public final class SystemNotificationScheduler: NSObject, @unchecked Sendable, NotificationScheduling,
    UNUserNotificationCenterDelegate
{
    private let center: UNUserNotificationCenter
    private let router: AppRouter

    @MainActor
    public init(
        router: AppRouter,
        center: UNUserNotificationCenter = .current()
    ) {
        self.router = router
        self.center = center
        super.init()
        center.delegate = self
    }

    public func currentAuthorization() async -> NotificationAuthorization {
        let settings = await center.notificationSettings()
        return Self.map(settings.authorizationStatus)
    }

    public func requestAuthorization() async throws -> NotificationAuthorization {
        _ = try await center.requestAuthorization(options: [.alert])
        return await currentAuthorization()
    }

    public func schedule(_ notification: LocalNotification) async throws {
        let (title, body) = await MainActor.run {
            (L10n.text(notification.titleKey), L10n.text(notification.bodyKey))
        }
        let content = UNMutableNotificationContent()
        content.title = title
        content.body = body
        content.sound = nil
        var userInfo: [String: String] = [:]
        switch notification.route {
        case .status:
            userInfo["route_kind"] = "status"
        case .incident(let incidentID):
            userInfo["route_kind"] = "incident"
            userInfo["incident_id"] = incidentID
        }
        if let eventToken = notification.eventToken {
            userInfo["event_token"] = eventToken
        }
        content.userInfo = userInfo
        try await center.add(
            UNNotificationRequest(
                identifier: notification.identifier,
                content: content,
                trigger: nil
            )
        )
    }

    public func userNotificationCenter(
        _ center: UNUserNotificationCenter,
        willPresent notification: UNNotification,
        withCompletionHandler completionHandler: @escaping (UNNotificationPresentationOptions) -> Void
    ) {
        // Foreground delivery is intentionally quiet: no banner, list, sound,
        // or badge. The in-App projection is already visible.
        completionHandler([])
    }

    public func userNotificationCenter(
        _ center: UNUserNotificationCenter,
        didReceive response: UNNotificationResponse,
        withCompletionHandler completionHandler: @escaping () -> Void
    ) {
        let userInfo = response.notification.request.content.userInfo
        let route: NotificationRoute
        if userInfo["route_kind"] as? String == "incident",
           let incidentID = userInfo["incident_id"] as? String
        {
            route = .incident(incidentID)
        } else {
            route = .status
        }
        Task { @MainActor [router] in
            router.open(route)
            NSApplication.shared.activate(ignoringOtherApps: true)
        }
        completionHandler()
    }

    private static func map(_ status: UNAuthorizationStatus) -> NotificationAuthorization {
        switch status {
        case .notDetermined: .notDetermined
        case .denied: .denied
        case .authorized: .authorized
        case .provisional: .provisional
        case .ephemeral: .ephemeral
        @unknown default: .unknown
        }
    }
}
