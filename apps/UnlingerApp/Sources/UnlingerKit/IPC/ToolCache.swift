import Foundation

public enum ToolCacheKind: String, Codable, Equatable, Sendable {
    case npmDownloadCache = "npm_download_cache"
    case uvCache = "uv_cache"
}

public enum ToolCacheAvailability: String, Codable, Equatable, Sendable {
    case available, absent, unsupported, unavailable
}

public enum ToolCacheOutcome: String, Codable, Equatable, Sendable {
    case running, completed, failed, busy
    case noOp = "no_op"
    case deliveryUnknown = "delivery_unknown"
}

public struct ToolCacheAttemptSummary: Codable, Equatable, Sendable {
    public var outcome: ToolCacheOutcome
    public var preparedAtUnixMillis: UInt64
    public var completedAtUnixMillis: UInt64?
    public var nativeRemovedEntryCount: UInt64?
    public var nativeRemovedLogicalBytes: UInt64?

    private enum CodingKeys: String, CodingKey {
        case outcome
        case preparedAtUnixMillis = "prepared_at_unix_millis"
        case completedAtUnixMillis = "completed_at_unix_millis"
        case nativeRemovedEntryCount = "native_removed_entry_count"
        case nativeRemovedLogicalBytes = "native_removed_logical_bytes"
    }
}

public struct ToolCacheMaintenanceSummary: Codable, Equatable, Sendable {
    public var kind: ToolCacheKind
    public var observedAtUnixMillis: UInt64
    public var availability: ToolCacheAvailability
    public var automaticMaintenanceEligible: Bool
    public var lastAttempt: ToolCacheAttemptSummary?

    private enum CodingKeys: String, CodingKey {
        case kind, availability
        case observedAtUnixMillis = "observed_at_unix_millis"
        case automaticMaintenanceEligible = "automatic_maintenance_eligible"
        case lastAttempt = "last_attempt"
    }
}
