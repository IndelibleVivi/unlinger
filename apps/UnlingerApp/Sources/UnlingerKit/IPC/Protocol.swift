import Foundation

// MARK: - Wire-string enums
//
// Every enum the UI branches on keeps an `unknown` case carrying the raw wire
// value. Contract rule: unknown reason/evidence/enum values get a generic
// fallback and never widen any authorization.

public enum Readiness: Equatable, Sendable {
    case ready
    case failed
    case unknown(String)
}

public enum EffectiveMode: String, CaseIterable, Codable, Sendable {
    case reportOnly = "report_only"
    case enforce
}

public enum ProcessOutcome: Equatable, Sendable {
    case cleared, revived, failed, deliveryUnknown, unknown(String)
}

public enum ArtifactOutcome: Equatable, Sendable {
    case notApplicable, reconciled, residue, deliveryUnknown, unknown(String)
}

public enum OverallOutcome: Equatable, Sendable {
    case cleared, clearedWithResidue, revived, failed, unknown(String)
}

public enum IncidentState: Equatable, Sendable {
    case protected, active, cooling, confirmed, ambiguous, reclaiming
    case cleared, revived, failed
    case unknown(String)
}

public enum AttentionKind: Equatable, Sendable {
    case cleanupFailed, cleanupRevived, cleanupResidue, daemonUnhealthy, eventSourceDegraded, storageRecovered
    case unknown(String)
}

// MARK: - Wire-string decoding helper

protocol WireString: Codable {
    init(wire: String)
    var wire: String { get }
}

extension WireString {
    public init(from decoder: Decoder) throws {
        let container = try decoder.singleValueContainer()
        self.init(wire: try container.decode(String.self))
    }

    public func encode(to encoder: Encoder) throws {
        var container = encoder.singleValueContainer()
        try container.encode(wire)
    }
}

extension Readiness: WireString {
    init(wire: String) {
        switch wire {
        case "ready": self = .ready
        case "failed": self = .failed
        default: self = .unknown(wire)
        }
    }

    var wire: String {
        switch self {
        case .ready: "ready"
        case .failed: "failed"
        case .unknown(let raw): raw
        }
    }
}

extension ProcessOutcome: WireString {
    init(wire: String) {
        switch wire {
        case "cleared": self = .cleared
        case "revived": self = .revived
        case "failed": self = .failed
        case "delivery_unknown": self = .deliveryUnknown
        default: self = .unknown(wire)
        }
    }

    var wire: String {
        switch self {
        case .cleared: "cleared"
        case .revived: "revived"
        case .failed: "failed"
        case .deliveryUnknown: "delivery_unknown"
        case .unknown(let raw): raw
        }
    }
}

extension ArtifactOutcome: WireString {
    init(wire: String) {
        switch wire {
        case "not_applicable": self = .notApplicable
        case "reconciled": self = .reconciled
        case "residue": self = .residue
        case "delivery_unknown": self = .deliveryUnknown
        default: self = .unknown(wire)
        }
    }

    var wire: String {
        switch self {
        case .notApplicable: "not_applicable"
        case .reconciled: "reconciled"
        case .residue: "residue"
        case .deliveryUnknown: "delivery_unknown"
        case .unknown(let raw): raw
        }
    }
}

extension OverallOutcome: WireString {
    init(wire: String) {
        switch wire {
        case "cleared": self = .cleared
        case "cleared_with_residue": self = .clearedWithResidue
        case "revived": self = .revived
        case "failed": self = .failed
        default: self = .unknown(wire)
        }
    }

    var wire: String {
        switch self {
        case .cleared: "cleared"
        case .clearedWithResidue: "cleared_with_residue"
        case .revived: "revived"
        case .failed: "failed"
        case .unknown(let raw): raw
        }
    }
}

extension IncidentState: WireString {
    init(wire: String) {
        switch wire {
        case "PROTECTED": self = .protected
        case "ACTIVE": self = .active
        case "COOLING": self = .cooling
        case "CONFIRMED": self = .confirmed
        case "AMBIGUOUS": self = .ambiguous
        case "RECLAIMING": self = .reclaiming
        case "CLEARED": self = .cleared
        case "REVIVED": self = .revived
        case "FAILED": self = .failed
        default: self = .unknown(wire)
        }
    }

    var wire: String {
        switch self {
        case .protected: "PROTECTED"
        case .active: "ACTIVE"
        case .cooling: "COOLING"
        case .confirmed: "CONFIRMED"
        case .ambiguous: "AMBIGUOUS"
        case .reclaiming: "RECLAIMING"
        case .cleared: "CLEARED"
        case .revived: "REVIVED"
        case .failed: "FAILED"
        case .unknown(let raw): raw
        }
    }
}

extension AttentionKind: WireString {
    init(wire: String) {
        switch wire {
        case "cleanup_failed": self = .cleanupFailed
        case "cleanup_revived": self = .cleanupRevived
        case "cleanup_residue": self = .cleanupResidue
        case "daemon_unhealthy": self = .daemonUnhealthy
        case "event_source_degraded": self = .eventSourceDegraded
        case "storage_recovered": self = .storageRecovered
        default: self = .unknown(wire)
        }
    }

    var wire: String {
        switch self {
        case .cleanupFailed: "cleanup_failed"
        case .cleanupRevived: "cleanup_revived"
        case .cleanupResidue: "cleanup_residue"
        case .daemonUnhealthy: "daemon_unhealthy"
        case .eventSourceDegraded: "event_source_degraded"
        case .storageRecovered: "storage_recovered"
        case .unknown(let raw): raw
        }
    }
}

// MARK: - Status DTOs

public struct ComponentHealth: Codable, Equatable, Sendable {
    public var healthy: Bool
    public var reasonId: String?
}

public struct ReclaimSummary: Codable, Equatable, Sendable {
    public var incidentId: String
    public var occurredAtUnixMillis: UInt64
    public var processOutcome: ProcessOutcome
    public var artifactOutcome: ArtifactOutcome
    public var overallOutcome: OverallOutcome
}

public struct AttentionItem: Codable, Equatable, Sendable, Identifiable {
    public var kind: AttentionKind
    public var reasonId: String
    public var incidentId: String?
    public var overallOutcome: OverallOutcome?
    public var occurredAtUnixMillis: UInt64?

    public var id: String { "\(kind.wire)|\(reasonId)|\(incidentId ?? "")|\(occurredAtUnixMillis ?? 0)" }
}

public struct AttentionProjection: Codable, Equatable, Sendable {
    public var totalCount: Int
    public var items: [AttentionItem]
}

public struct ProtectedIncidentSummary: Codable, Equatable, Sendable, Identifiable {
    public var incidentId: String
    public var protectedAtUnixMillis: UInt64
    public var lastExactObservedAtUnixMillis: UInt64?

    public var id: String { incidentId }
}

public struct ProtectionProjection: Codable, Equatable, Sendable {
    public var totalCount: Int
    public var items: [ProtectedIncidentSummary]
}

public struct Capability: Codable, Equatable, Sendable {
    public var available: Bool
    public var unavailableReasonId: String?
}

public struct GlobalCapabilities: Codable, Equatable, Sendable {
    public var pause: Capability
    public var resume: Capability
    public var retryFailedCleanup: Capability
    public var protectIncident: Capability
    public var unprotectIncident: Capability
    public var exportDiagnostics: Capability
}

public struct IncidentCapabilities: Codable, Equatable, Sendable {
    public var retryFailedCleanup: Capability
    public var protectIncident: Capability
    public var unprotectIncident: Capability
    public var exportDiagnostics: Capability
}

public struct PublicStatus: Codable, Equatable, Sendable {
    public var daemonVersion: String
    public var healthy: Bool
    public var readiness: Readiness
    public var effectiveMode: EffectiveMode
    public var scanInProgress: Bool
    public var cleanupInProgress: Bool
    public var lastScanAtUnixMillis: UInt64?
    public var pausedUntilUnixMillis: UInt64?
    public var confirmedIncidentCount: Int
    public var ambiguousIncidentCount: Int
    public var mostRecentReclaim: ReclaimSummary?
    public var eventSource: ComponentHealth
    public var storage: ComponentHealth
    public var attention: AttentionProjection
    public var protection: ProtectionProjection
    public var capabilities: GlobalCapabilities
}

// MARK: - History / explain DTOs

public struct RoleCount: Codable, Equatable, Sendable {
    public var role: String
    public var count: Int
}

public struct Evidence: Codable, Equatable, Sendable {
    public var id: String
    public var family: String
}

public struct GateLedger: Codable, Equatable, Sendable {
    public var sameUser: Bool?
    public var strongAutomationProvenance: Bool?
    public var confirmedAbandonment: Bool?
    public var isolatedSession: Bool?
    public var stableAcrossTwoObservations: Bool?
    public var processIdentityUnchanged: Bool?
    public var noProtectionRule: Bool?
}

public struct ObservationRecord: Codable, Equatable, Sendable {
    public var family: String?
    public var familyVersion: String?
    public var state: IncidentState?
    public var executableBasename: String?
    public var memberCount: Int?
    public var residentMemoryBytes: UInt64?
    public var roles: [RoleCount]?
    public var evidence: [Evidence]?
    public var gates: GateLedger?
}

public struct ProcessAction: Codable, Equatable, Sendable {
    public var stage: String
    public var signal: String
    public var disposition: String
}

public struct ArtifactAction: Codable, Equatable, Sendable {
    public var kind: String
    public var disposition: String
}

public struct ResourceSnapshot: Codable, Equatable, Sendable {
    public var processCount: Int
    public var residentMemoryBytes: UInt64
}

public struct ResourceReceipt: Codable, Equatable, Sendable {
    public var before: ResourceSnapshot?
    public var after: ResourceSnapshot?
    public var estimatedReclaimedMemoryBytes: UInt64?
}

public struct CleanupReceipt: Codable, Equatable, Sendable {
    public var state: IncidentState
    public var reasonId: String?
    public var processOutcome: ProcessOutcome
    public var artifactOutcome: ArtifactOutcome
    public var overallOutcome: OverallOutcome
    public var attentionRequired: Bool
    public var processActions: [ProcessAction]
    public var artifactActions: [ArtifactAction]
    public var survivorCount: Int
    public var revivalChecksCompleted: Int
    public var resources: ResourceReceipt?
}

public enum HistoryPayload: Equatable, Sendable {
    case observation(ObservationRecord)
    case cleanup(CleanupReceipt)
    case unknown(String)
}

extension HistoryPayload: Decodable {
    // Note: the shared decoder uses .convertFromSnakeCase, so keys arrive
    // already converted — "recordType", not "record_type".
    private enum CodingKeys: String, CodingKey {
        case recordType
        case observation
        case cleanup
    }

    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        let recordType = try container.decode(String.self, forKey: .recordType)
        switch recordType {
        case "observation":
            self = .observation(try container.decode(ObservationRecord.self, forKey: .observation))
        case "cleanup":
            self = .cleanup(try container.decode(CleanupReceipt.self, forKey: .cleanup))
        default:
            self = .unknown(recordType)
        }
    }
}

public struct HistoryEvent: Decodable, Equatable, Sendable, Identifiable {
    public var incidentId: String
    public var occurredAtUnixMillis: UInt64
    public var state: IncidentState
    public var payload: HistoryPayload

    public var id: String { "\(incidentId)|\(occurredAtUnixMillis)|\(state.wire)" }
}

public struct IncidentDetail: Decodable, Equatable, Sendable {
    public var incidentId: String
    public var events: [HistoryEvent]
    public var capabilities: IncidentCapabilities
}

/// One entry of the read-only current-incidents roster: what the latest
/// reconciliation cycle is actually seeing. Read-only observability — never
/// a work queue, and no action availability is derived from it.
public struct CurrentIncident: Decodable, Equatable, Sendable, Identifiable {
    public var incidentId: String
    public var observation: ObservationRecord

    public var id: String { incidentId }
}

// Diagnostics v2 combines public status and public incident detail. Decoded
// loosely: the app writes the bundle to a user-chosen file without relying on
// every field being present.
public struct DiagnosticsBundle: Decodable, Equatable, Sendable {
    public var schemaVersion: Int?
    public var generatedAtUnixMillis: UInt64?
    public var status: PublicStatus?
    public var incident: IncidentDetail?
}

// MARK: - Commands

public enum Command: Sendable, Equatable {
    case status
    case history(limit: Int)
    case explain(incidentID: String)
    case incidents
    case pause(durationMillis: UInt64)
    case resume
    case retryFailedCleanup(incidentID: String)
    case protectIncident(incidentID: String)
    case unprotectIncident(incidentID: String)
    case exportDiagnostics(incidentID: String)

    /// Read-only command used to read back durable state when this mutation's
    /// delivery is uncertain. `nil` means status.
    var readback: Command {
        switch self {
        case .retryFailedCleanup(let id), .protectIncident(let id),
             .unprotectIncident(let id), .exportDiagnostics(let id):
            .explain(incidentID: id)
        case .status, .history, .explain, .incidents, .pause, .resume:
            .status
        }
    }

    var isMutation: Bool {
        switch self {
        case .pause, .resume, .retryFailedCleanup, .protectIncident,
             .unprotectIncident, .exportDiagnostics:
            true
        case .status, .history, .explain, .incidents:
            false
        }
    }
}

extension Command: Encodable {
    private struct DynamicKey: CodingKey {
        var stringValue: String
        var intValue: Int?

        init?(stringValue: String) { self.stringValue = stringValue }
        init?(intValue: Int) { return nil }
    }

    public func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: DynamicKey.self)
        switch self {
        case .status:
            try container.encode("status", forKey: DynamicKey(stringValue: "command")!)
        case .history(let limit):
            try container.encode("history", forKey: DynamicKey(stringValue: "command")!)
            try container.encode(limit, forKey: DynamicKey(stringValue: "limit")!)
        case .explain(let incidentID):
            try container.encode("explain", forKey: DynamicKey(stringValue: "command")!)
            try container.encode(incidentID, forKey: DynamicKey(stringValue: "incident_id")!)
        case .incidents:
            try container.encode("incidents", forKey: DynamicKey(stringValue: "command")!)
        case .pause(let durationMillis):
            try container.encode("pause", forKey: DynamicKey(stringValue: "command")!)
            try container.encode(durationMillis, forKey: DynamicKey(stringValue: "duration_millis")!)
        case .resume:
            try container.encode("resume", forKey: DynamicKey(stringValue: "command")!)
        case .retryFailedCleanup(let incidentID):
            try container.encode("retry_failed_cleanup", forKey: DynamicKey(stringValue: "command")!)
            try container.encode(incidentID, forKey: DynamicKey(stringValue: "incident_id")!)
        case .protectIncident(let incidentID):
            try container.encode("protect_incident", forKey: DynamicKey(stringValue: "command")!)
            try container.encode(incidentID, forKey: DynamicKey(stringValue: "incident_id")!)
        case .unprotectIncident(let incidentID):
            try container.encode("unprotect_incident", forKey: DynamicKey(stringValue: "command")!)
            try container.encode(incidentID, forKey: DynamicKey(stringValue: "incident_id")!)
        case .exportDiagnostics(let incidentID):
            try container.encode("export_diagnostics", forKey: DynamicKey(stringValue: "command")!)
            try container.encode(incidentID, forKey: DynamicKey(stringValue: "incident_id")!)
        }
    }
}

// MARK: - Envelope

struct RequestEnvelope: Encodable {
    let schemaVersion = 2
    let requestID: UInt64
    let command: Command

    enum CodingKeys: String, CodingKey {
        case schemaVersion = "schema_version"
        case requestID = "request_id"
        case command
    }
}
