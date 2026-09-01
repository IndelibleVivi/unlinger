import Foundation
import Observation

@Observable
@MainActor
public final class AppSettings {
    public var notificationMode: NotificationMode {
        didSet {
            defaults.set(notificationMode.rawValue, forKey: Keys.notificationMode)
            let coordinator = notificationCoordinator
            Task { await coordinator?.setMode(notificationMode) }
        }
    }
    public private(set) var notificationAuthorization: NotificationAuthorization
    public private(set) var launchAtLoginStatus: LoginItemStatus = .unknown
    public private(set) var launchAtLoginRequested: Bool

    private let defaults: UserDefaults
    private let loginItemService: any LoginItemServicing
    private var notificationCoordinator: (any NotificationCoordinating)?

    public init(
        defaults: UserDefaults = .standard,
        loginItemService: any LoginItemServicing = SystemLoginItemService()
    ) {
        self.defaults = defaults
        self.loginItemService = loginItemService
        self.notificationMode = defaults.string(forKey: Keys.notificationMode)
            .flatMap(NotificationMode.init(rawValue:)) ?? .attention
        self.notificationAuthorization = .notDetermined
        self.launchAtLoginRequested = defaults.bool(forKey: Keys.launchAtLoginRequested)
    }

    public func attachNotificationCoordinator(_ coordinator: (any NotificationCoordinating)?) {
        notificationCoordinator = coordinator
        Task { await coordinator?.setMode(notificationMode) }
    }

    public func prepareNotifications() async {
        guard let notificationCoordinator else {
            notificationAuthorization = .unavailable
            return
        }
        notificationAuthorization = await notificationCoordinator.prepareAuthorization()
    }

    public func refreshLaunchAtLogin() async {
        launchAtLoginStatus = await loginItemService.currentStatus()
    }

    public func setLaunchAtLogin(_ enabled: Bool) async {
        launchAtLoginRequested = enabled
        defaults.set(enabled, forKey: Keys.launchAtLoginRequested)
        launchAtLoginStatus = await loginItemService.setEnabled(enabled)
    }

    private enum Keys {
        static let notificationMode = "notification_mode"
        static let launchAtLoginRequested = "launch_at_login_requested"
    }
}
