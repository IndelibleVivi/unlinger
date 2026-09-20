import Foundation

public struct ToolCachePresentation: Equatable, Sendable {
    public var titleKey: String
    public var scopeKey: String
    public var accountingKey: String
    public var observedAt: Date
    public var availabilityKey: String
    public var outcomeKey: String?
    public var attemptedAt: Date?
    public var nativeRemovedEntryCount: UInt64?
    public var nativeRemovedLogicalBytes: UInt64?

    init(_ summary: ToolCacheMaintenanceSummary, connection: ConnectionState) {
        titleKey = summary.kind == .uvCache ? "cache.uv.title" : "cache.maintenance.title"
        scopeKey = summary.kind == .uvCache ? "cache.uv.scope" : "cache.maintenance.scope"
        accountingKey = summary.kind == .uvCache ? "cache.uv.estimate" : "cache.maintenance.estimate"
        let availabilityPrefix = summary.kind == .uvCache ? "cache.uv" : "cache.maintenance"
        observedAt = Date(unixMillis: summary.observedAtUnixMillis)
        switch summary.availability {
        case .available:
            availabilityKey = summary.automaticMaintenanceEligible && connection == .live
                ? "\(availabilityPrefix).enabled" : "\(availabilityPrefix).observing"
        case .absent: availabilityKey = "\(availabilityPrefix).absent"
        case .unsupported: availabilityKey = "\(availabilityPrefix).unsupported"
        case .unavailable: availabilityKey = "cache.maintenance.unavailable"
        }
        if let attempt = summary.lastAttempt {
            outcomeKey = "cache.maintenance.\(attempt.outcome.rawValue)"
            attemptedAt = Date(unixMillis: attempt.completedAtUnixMillis ?? attempt.preparedAtUnixMillis)
            nativeRemovedEntryCount = attempt.nativeRemovedEntryCount
            nativeRemovedLogicalBytes = attempt.nativeRemovedLogicalBytes
        }
    }
}
