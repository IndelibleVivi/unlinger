import ServiceManagement

public enum LoginItemStatus: Equatable, Sendable {
    case disabled
    case enabled
    case requiresApproval
    case notFound
    case failed(reasonId: String)
    case unknown
}

public protocol LoginItemServicing: Sendable {
    func currentStatus() async -> LoginItemStatus
    func setEnabled(_ enabled: Bool) async -> LoginItemStatus
}

/// Registers only the menu App through SMAppService.mainApp. It never edits,
/// reloads, or otherwise controls the daemon LaunchAgent.
public struct SystemLoginItemService: LoginItemServicing {
    public init() {}

    public func currentStatus() async -> LoginItemStatus {
        Self.map(SMAppService.mainApp.status)
    }

    public func setEnabled(_ enabled: Bool) async -> LoginItemStatus {
        do {
            if enabled {
                try SMAppService.mainApp.register()
            } else {
                try await SMAppService.mainApp.unregister()
            }
            return Self.map(SMAppService.mainApp.status)
        } catch {
            return .failed(reasonId: enabled
                ? "login_item.registration_failed"
                : "login_item.unregistration_failed")
        }
    }

    private static func map(_ status: SMAppService.Status) -> LoginItemStatus {
        switch status {
        case .notRegistered: .disabled
        case .enabled: .enabled
        case .requiresApproval: .requiresApproval
        case .notFound: .notFound
        @unknown default: .unknown
        }
    }
}
