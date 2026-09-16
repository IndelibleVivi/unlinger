import Foundation
import Testing
@testable import UnlingerKit

private actor FakeLoginItemService: LoginItemServicing {
    private var status: LoginItemStatus
    private let enableResult: LoginItemStatus
    private let disableResult: LoginItemStatus

    init(
        status: LoginItemStatus = .disabled,
        enableResult: LoginItemStatus = .enabled,
        disableResult: LoginItemStatus = .disabled
    ) {
        self.status = status
        self.enableResult = enableResult
        self.disableResult = disableResult
    }

    func currentStatus() async -> LoginItemStatus { status }

    func setEnabled(_ enabled: Bool) async -> LoginItemStatus {
        status = enabled ? enableResult : disableResult
        return status
    }
}

private actor CountingNotificationCoordinator: NotificationCoordinating {
    private var modes: [NotificationMode] = []

    func modeCount() -> Int { modes.count }
    func latestMode() -> NotificationMode? { modes.last }

    func prepareAuthorization() async -> NotificationAuthorization { .authorized }

    func setMode(_ mode: NotificationMode) async {
        modes.append(mode)
    }

    func receiveTrustedRefresh(
        status _: PublicStatus,
        history _: [HistoryEvent],
        atUnixMillis _: UInt64
    ) async {}

    func receiveUnavailable(atUnixMillis _: UInt64) async {}
}

@Suite("App settings")
@MainActor
struct SettingsTests {
    @Test("launch-at-login controls only the abstracted App service")
    func loginItemState() async throws {
        let domain = "UnlingerAppTests.\(UUID().uuidString)"
        let defaults = try #require(UserDefaults(suiteName: domain))
        defer { defaults.removePersistentDomain(forName: domain) }
        let service = FakeLoginItemService()
        let settings = AppSettings(defaults: defaults, loginItemService: service)

        await settings.refreshLaunchAtLogin()
        #expect(settings.launchAtLoginStatus == .disabled)
        await settings.setLaunchAtLogin(true)
        #expect(settings.launchAtLoginRequested)
        #expect(settings.launchAtLoginStatus == .enabled)
        #expect(defaults.bool(forKey: "launch_at_login_requested"))
    }

    @Test("registration failure remains typed and visible")
    func loginItemFailure() async throws {
        let domain = "UnlingerAppTests.\(UUID().uuidString)"
        let defaults = try #require(UserDefaults(suiteName: domain))
        defer { defaults.removePersistentDomain(forName: domain) }
        let service = FakeLoginItemService(
            enableResult: .failed(reasonId: "login_item.registration_failed")
        )
        let settings = AppSettings(defaults: defaults, loginItemService: service)

        await settings.setLaunchAtLogin(true)

        #expect(settings.launchAtLoginStatus == .failed(
            reasonId: "login_item.registration_failed"
        ))
    }

    @Test("notification mode persists without scheduling from settings")
    func notificationModePersists() async throws {
        let domain = "UnlingerAppTests.\(UUID().uuidString)"
        let defaults = try #require(UserDefaults(suiteName: domain))
        defer { defaults.removePersistentDomain(forName: domain) }
        let settings = AppSettings(
            defaults: defaults,
            loginItemService: FakeLoginItemService()
        )

        settings.setNotificationMode(.attentionAndReclaims)

        let restored = AppSettings(
            defaults: defaults,
            loginItemService: FakeLoginItemService()
        )
        #expect(restored.notificationMode == .attentionAndReclaims)
    }

    @Test("same notification mode does not republish preferences or coordinator work")
    func notificationModeIsIdempotent() async throws {
        let domain = "UnlingerAppTests.\(UUID().uuidString)"
        let defaults = try #require(UserDefaults(suiteName: domain))
        defer { defaults.removePersistentDomain(forName: domain) }
        let coordinator = CountingNotificationCoordinator()
        let settings = AppSettings(
            defaults: defaults,
            loginItemService: FakeLoginItemService()
        )
        settings.attachNotificationCoordinator(coordinator)
        for _ in 0 ..< 1_000 where await coordinator.modeCount() < 1 {
            await Task.yield()
        }

        settings.setNotificationMode(.attention)
        for _ in 0 ..< 100 { await Task.yield() }
        #expect(await coordinator.modeCount() == 1)

        settings.setNotificationMode(.attentionAndReclaims)
        for _ in 0 ..< 1_000 where await coordinator.modeCount() < 2 {
            await Task.yield()
        }
        #expect(await coordinator.modeCount() == 2)
        #expect(await coordinator.latestMode() == .attentionAndReclaims)
    }
}
