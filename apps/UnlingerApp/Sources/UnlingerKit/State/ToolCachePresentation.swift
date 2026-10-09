import Foundation

/// Presentation-only translation of a producer-owned cache-maintenance
/// summary. It selects localized copy keys and display shapes; it never
/// recomputes eligibility, outcome, or a reclaim estimate.
///
/// The uv/npm families carry producer `native_removed_*` self-report counters.
/// The Node compile-cache family carries Unlinger's own logical
/// `removed_entry_count`/`removed_logical_bytes`; both are surfaced through
/// distinct fields so a reader can never confuse them.
public struct ToolCachePresentation: Equatable, Sendable {
    public var titleKey: String
    public var scopeKey: String
    public var accountingKey: String
    public var observedAt: Date
    public var availabilityKey: String
    public var outcomeKey: String?
    public var attemptedAt: Date?
    /// Logical entry/file count removed, as recorded by Unlinger. Present only
    /// when the outcome actually reports a removal; never inferred.
    public var removedEntryCount: UInt64?
    /// Logical bytes removed, as recorded by Unlinger. Never a measured
    /// physical APFS reclaim.
    public var removedLogicalBytes: UInt64?
    /// Producer self-report count, retained for the uv/npm families only.
    public var nativeRemovedEntryCount: UInt64?
    /// Producer self-report logical bytes, retained for the uv/npm families only.
    public var nativeRemovedLogicalBytes: UInt64?

    init(_ summary: ToolCacheMaintenanceSummary, connection: ConnectionState) {
        let kind = summary.kind
        titleKey = Self.titleKey(for: kind)
        scopeKey = Self.scopeKey(for: kind)
        accountingKey = Self.accountingKey(for: kind)
        observedAt = Date(unixMillis: summary.observedAtUnixMillis)
        availabilityKey = Self.availabilityKey(
            prefix: Self.availabilityPrefix(for: kind),
            availability: summary.availability,
            eligible: summary.automaticMaintenanceEligible,
            connection: connection
        )
        if let attempt = summary.lastAttempt {
            outcomeKey = "cache.maintenance.\(attempt.outcome.rawValue)"
            attemptedAt = Date(unixMillis: attempt.completedAtUnixMillis ?? attempt.preparedAtUnixMillis)
            nativeRemovedEntryCount = attempt.nativeRemovedEntryCount
            nativeRemovedLogicalBytes = attempt.nativeRemovedLogicalBytes
        }
        removedEntryCount = nil
        removedLogicalBytes = nil
    }

    init(_ summary: NodeCompileCacheMaintenanceSummary, connection: ConnectionState) {
        let kind = summary.kind
        titleKey = Self.titleKey(for: kind)
        scopeKey = Self.scopeKey(for: kind)
        accountingKey = Self.accountingKey(for: kind)
        observedAt = Date(unixMillis: summary.observedAtUnixMillis)
        availabilityKey = Self.availabilityKey(
            prefix: Self.availabilityPrefix(for: kind),
            availability: summary.availability,
            eligible: summary.automaticMaintenanceEligible,
            connection: connection
        )
        if let attempt = summary.lastAttempt {
            outcomeKey = "cache.maintenance.\(attempt.outcome.rawValue)"
            attemptedAt = Date(unixMillis: attempt.completedAtUnixMillis ?? attempt.preparedAtUnixMillis)
            removedEntryCount = attempt.removedEntryCount
            removedLogicalBytes = attempt.removedLogicalBytes
        }
        nativeRemovedEntryCount = nil
        nativeRemovedLogicalBytes = nil
    }

    private static func titleKey(for kind: ToolCacheKind) -> String {
        switch kind {
        case .uvCache: "cache.uv.title"
        case .nodeCompileCache: "cache.node.title"
        case .npmDownloadCache: "cache.maintenance.title"
        }
    }

    private static func scopeKey(for kind: ToolCacheKind) -> String {
        switch kind {
        case .uvCache: "cache.uv.scope"
        case .nodeCompileCache: "cache.node.scope"
        case .npmDownloadCache: "cache.maintenance.scope"
        }
    }

    private static func accountingKey(for kind: ToolCacheKind) -> String {
        switch kind {
        case .uvCache: "cache.uv.estimate"
        case .nodeCompileCache: "cache.node.estimate"
        case .npmDownloadCache: "cache.maintenance.estimate"
        }
    }

    private static func availabilityPrefix(for kind: ToolCacheKind) -> String {
        switch kind {
        case .uvCache: "cache.uv"
        case .nodeCompileCache: "cache.node"
        case .npmDownloadCache: "cache.maintenance"
        }
    }

    private static func availabilityKey(
        prefix: String,
        availability: ToolCacheAvailability,
        eligible: Bool,
        connection: ConnectionState
    ) -> String {
        switch availability {
        case .available:
            eligible && connection == .live
                ? "\(prefix).enabled" : "\(prefix).observing"
        case .absent: "\(prefix).absent"
        case .unsupported: "\(prefix).unsupported"
        case .unavailable: "cache.maintenance.unavailable"
        }
    }
}
