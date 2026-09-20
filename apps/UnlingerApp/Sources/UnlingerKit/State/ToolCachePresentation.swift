import Foundation

public struct ToolCachePresentation: Equatable, Sendable {
    public var observedAt: Date
    public var availabilityKey: String
    public var outcomeKey: String?
    public var attemptedAt: Date?
    public var nativeRemovedEntryCount: UInt64?
    public var nativeRemovedLogicalBytes: UInt64?

    init(_ summary: ToolCacheMaintenanceSummary, connection: ConnectionState) {
        observedAt = Date(unixMillis: summary.observedAtUnixMillis)
        switch summary.availability {
        case .available:
            availabilityKey = summary.automaticMaintenanceEligible && connection == .live
                ? "cache.maintenance.enabled" : "cache.maintenance.observing"
        case .absent: availabilityKey = "cache.maintenance.absent"
        case .unsupported: availabilityKey = "cache.maintenance.unsupported"
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
