import Foundation

public enum NotificationMode: String, Codable, CaseIterable, Equatable, Sendable {
    case off
    case attention
    case attentionAndReclaims = "attention_and_reclaims"
}

public enum NotificationAuthorization: String, Codable, Equatable, Sendable {
    case notDetermined = "not_determined"
    case denied
    case authorized
    case provisional
    case ephemeral
    case unavailable
    case unknown

    public var permitsScheduling: Bool {
        switch self {
        case .authorized, .provisional, .ephemeral: true
        default: false
        }
    }
}

public enum NotificationRoute: Hashable, Sendable {
    case status
    case incident(String)
}

public struct LocalNotification: Equatable, Sendable, Identifiable {
    public var identifier: String
    public var titleKey: String
    public var bodyKey: String
    public var route: NotificationRoute
    public var eventToken: String?

    public var id: String { identifier }

    public init(
        identifier: String,
        titleKey: String,
        bodyKey: String,
        route: NotificationRoute,
        eventToken: String?
    ) {
        self.identifier = identifier
        self.titleKey = titleKey
        self.bodyKey = bodyKey
        self.route = route
        self.eventToken = eventToken
    }
}

public protocol NotificationScheduling: Sendable {
    func currentAuthorization() async -> NotificationAuthorization
    func requestAuthorization() async throws -> NotificationAuthorization
    func schedule(_ notification: LocalNotification) async throws
}

public protocol NotificationCoordinating: Sendable {
    func prepareAuthorization() async -> NotificationAuthorization
    func setMode(_ mode: NotificationMode) async
    func receiveTrustedRefresh(
        status: PublicStatus,
        history: [HistoryEvent],
        atUnixMillis: UInt64
    ) async
    func receiveUnavailable(atUnixMillis: UInt64) async
}
