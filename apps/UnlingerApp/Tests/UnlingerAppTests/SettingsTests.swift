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

        settings.notificationMode = .attentionAndReclaims

        let restored = AppSettings(
            defaults: defaults,
            loginItemService: FakeLoginItemService()
        )
        #expect(restored.notificationMode == .attentionAndReclaims)
    }
}
