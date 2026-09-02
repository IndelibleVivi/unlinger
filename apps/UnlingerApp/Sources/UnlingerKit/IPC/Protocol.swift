import Foundation

// MARK: - Wire-string enums
//
// Every enum the UI branches on keeps an `unknown` case carrying the raw wire
// value. Contract rule: unknown reason/evidence/enum values get a generic
// fallback and never widen any authorization.

public enum Readiness: Equatable, Sendable {
    case starting
    case ready
    case draining
    case failed
    case unknown(String)
}

public enum ObservationFreshness: Equatable, Sendable {
    case current
    case scanInProgress
    case staleAfterFailure
    case neverObserved
    case unknown(String)
}

public enum MutationKind: Equatable, Sendable {
    case pause
    case resume
    case retryFailedCleanup
    case protectIncident
    case unprotectIncident
    case unknown(String)
}

public enum EffectiveMode: Equatable, Sendable {
    case reportOnly
    case enforce
    case unknown(String)
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

public enum ProcessRole: Equatable, Sendable {
    case controller, browserRoot, renderer, gpu, utility, browserHelper
    case crashHandler, recorder, incidentMember, unknown(String)
}

public enum EvidenceFamily: Equatable, Sendable {
    case automationProvenance, abandonment, isolation, protection, unknown(String)
}

public enum CleanupStage: Equatable, Sendable {
    case primaryTerm, memberTerm, exactKill, unknown(String)
}

public enum CleanupSignal: Equatable, Sendable {
    case term, kill, unknown(String)
}

public enum SignalDisposition: Equatable, Sendable {
    case delivered, alreadyExited, identityMismatch, rejected
    case cancelledBeforeDelivery, deliveryUnknown, unknown(String)
}

public enum RuntimeArtifactKind: Equatable, Sendable {
    case devToolsActivePort, unknown(String)
}

public enum ArtifactDisposition: Equatable, Sendable {
    case removed, alreadyAbsent, identityMismatch, referenced, unsafe, rejected
    case cancelledBeforeDelivery, deliveryUnknown, unknown(String)
}

public enum BrowserCompatibilityDecision: Equatable, Sendable {
    case automatic, observeOnly, protected, unknown(String)
}

public enum BrowserProduct: Equatable, Sendable {
    case chromeForTesting, chromium, googleChrome, other, unknown(String)
}

public enum BrowserAutomaticActionLevel: Equatable, Sendable {
    case automatic, observeOnly, unsupported, unknown(String)
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
        case "starting": self = .starting
        case "ready": self = .ready
        case "draining": self = .draining
        case "failed": self = .failed
        default: self = .unknown(wire)
        }
    }

    var wire: String {
        switch self {
        case .starting: "starting"
        case .ready: "ready"
        case .draining: "draining"
        case .failed: "failed"
        case .unknown(let raw): raw
        }
    }
}

extension ObservationFreshness: WireString {
    init(wire: String) {
        switch wire {
        case "current": self = .current
        case "scan_in_progress": self = .scanInProgress
        case "stale_after_failure": self = .staleAfterFailure
        case "never_observed": self = .neverObserved
        default: self = .unknown(wire)
        }
    }

    var wire: String {
        switch self {
        case .current: "current"
        case .scanInProgress: "scan_in_progress"
        case .staleAfterFailure: "stale_after_failure"
        case .neverObserved: "never_observed"
        case .unknown(let raw): raw
        }
    }
}

extension EffectiveMode: WireString {
    init(wire: String) {
        switch wire {
        case "report_only": self = .reportOnly
        case "enforce": self = .enforce
        default: self = .unknown(wire)
        }
    }

    var wire: String {
        switch self {
        case .reportOnly: "report_only"
        case .enforce: "enforce"
        case .unknown(let raw): raw
        }
    }
}

extension MutationKind: WireString {
    init(wire: String) {
        switch wire {
        case "pause": self = .pause
        case "resume": self = .resume
        case "retry_failed_cleanup": self = .retryFailedCleanup
        case "protect_incident": self = .protectIncident
        case "unprotect_incident": self = .unprotectIncident
        default: self = .unknown(wire)
        }
    }

    var wire: String {
        switch self {
        case .pause: "pause"
        case .resume: "resume"
        case .retryFailedCleanup: "retry_failed_cleanup"
        case .protectIncident: "protect_incident"
        case .unprotectIncident: "unprotect_incident"
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

extension ProcessRole: WireString {
    init(wire: String) {
        switch wire {
        case "controller": self = .controller
        case "browser_root": self = .browserRoot
        case "renderer": self = .renderer
        case "gpu": self = .gpu
        case "utility": self = .utility
        case "browser_helper": self = .browserHelper
        case "crash_handler": self = .crashHandler
        case "recorder": self = .recorder
        case "incident_member": self = .incidentMember
        default: self = .unknown(wire)
        }
    }

    var wire: String {
        switch self {
        case .controller: "controller"
        case .browserRoot: "browser_root"
        case .renderer: "renderer"
        case .gpu: "gpu"
        case .utility: "utility"
        case .browserHelper: "browser_helper"
        case .crashHandler: "crash_handler"
        case .recorder: "recorder"
        case .incidentMember: "incident_member"
        case .unknown(let raw): raw
        }
    }
}

extension EvidenceFamily: WireString {
    init(wire: String) {
        switch wire {
        case "automation_provenance": self = .automationProvenance
        case "abandonment": self = .abandonment
        case "isolation": self = .isolation
        case "protection": self = .protection
        default: self = .unknown(wire)
        }
    }

    var wire: String {
        switch self {
        case .automationProvenance: "automation_provenance"
        case .abandonment: "abandonment"
        case .isolation: "isolation"
        case .protection: "protection"
        case .unknown(let raw): raw
        }
    }
}

extension CleanupStage: WireString {
    init(wire: String) {
        switch wire {
        case "primary_term": self = .primaryTerm
        case "member_term": self = .memberTerm
        case "exact_kill": self = .exactKill
        default: self = .unknown(wire)
        }
    }

    var wire: String {
        switch self {
        case .primaryTerm: "primary_term"
        case .memberTerm: "member_term"
        case .exactKill: "exact_kill"
        case .unknown(let raw): raw
        }
    }
}

extension CleanupSignal: WireString {
    init(wire: String) {
        switch wire {
        case "term": self = .term
        case "kill": self = .kill
        default: self = .unknown(wire)
        }
    }

    var wire: String {
        switch self {
        case .term: "term"
        case .kill: "kill"
        case .unknown(let raw): raw
        }
    }
}

extension SignalDisposition: WireString {
    init(wire: String) {
        switch wire {
        case "delivered": self = .delivered
        case "already_exited": self = .alreadyExited
        case "identity_mismatch": self = .identityMismatch
        case "rejected": self = .rejected
        case "cancelled_before_delivery": self = .cancelledBeforeDelivery
        case "delivery_unknown": self = .deliveryUnknown
        default: self = .unknown(wire)
        }
    }

    var wire: String {
        switch self {
        case .delivered: "delivered"
        case .alreadyExited: "already_exited"
        case .identityMismatch: "identity_mismatch"
        case .rejected: "rejected"
        case .cancelledBeforeDelivery: "cancelled_before_delivery"
        case .deliveryUnknown: "delivery_unknown"
        case .unknown(let raw): raw
        }
    }
}

extension RuntimeArtifactKind: WireString {
    init(wire: String) {
        self = wire == "dev_tools_active_port" ? .devToolsActivePort : .unknown(wire)
    }

    var wire: String {
        switch self {
        case .devToolsActivePort: "dev_tools_active_port"
        case .unknown(let raw): raw
        }
    }
}

extension ArtifactDisposition: WireString {
    init(wire: String) {
        switch wire {
        case "removed": self = .removed
        case "already_absent": self = .alreadyAbsent
        case "identity_mismatch": self = .identityMismatch
        case "referenced": self = .referenced
        case "unsafe": self = .unsafe
        case "rejected": self = .rejected
        case "cancelled_before_delivery": self = .cancelledBeforeDelivery
        case "delivery_unknown": self = .deliveryUnknown
        default: self = .unknown(wire)
        }
    }

    var wire: String {
        switch self {
        case .removed: "removed"
        case .alreadyAbsent: "already_absent"
        case .identityMismatch: "identity_mismatch"
        case .referenced: "referenced"
        case .unsafe: "unsafe"
        case .rejected: "rejected"
        case .cancelledBeforeDelivery: "cancelled_before_delivery"
        case .deliveryUnknown: "delivery_unknown"
        case .unknown(let raw): raw
        }
    }
}

extension BrowserCompatibilityDecision: WireString {
    init(wire: String) {
        switch wire {
        case "automatic": self = .automatic
        case "observe_only": self = .observeOnly
        case "protected": self = .protected
        default: self = .unknown(wire)
        }
    }

    var wire: String {
        switch self {
        case .automatic: "automatic"
        case .observeOnly: "observe_only"
        case .protected: "protected"
        case .unknown(let raw): raw
        }
    }
}

extension BrowserProduct: WireString {
    init(wire: String) {
        switch wire {
        case "chrome_for_testing": self = .chromeForTesting
        case "chromium": self = .chromium
        case "google_chrome": self = .googleChrome
        case "other": self = .other
        default: self = .unknown(wire)
        }
    }

    var wire: String {
        switch self {
        case .chromeForTesting: "chrome_for_testing"
        case .chromium: "chromium"
        case .googleChrome: "google_chrome"
        case .other: "other"
        case .unknown(let raw): raw
        }
    }
}

extension BrowserAutomaticActionLevel: WireString {
    init(wire: String) {
        switch wire {
        case "automatic": self = .automatic
        case "observe_only": self = .observeOnly
        case "unsupported": self = .unsupported
        default: self = .unknown(wire)
        }
    }

    var wire: String {
        switch self {
        case .automatic: "automatic"
        case .observeOnly: "observe_only"
        case .unsupported: "unsupported"
        case .unknown(let raw): raw
        }
    }
}

extension BrowserOverviewPhase: WireString {
    init(wire: String) {
        switch wire {
        case "unknown": self = .unknown
        case "clear": self = .clear
        case "active": self = .active
        case "verifying": self = .verifying
        case "confirmed": self = .confirmed
        case "reclaiming": self = .reclaiming
        case "protected": self = .protected
        case "attention": self = .attention
        default: self = .unknown
        }
    }

    var wire: String {
        switch self {
        case .unknown: "unknown"
        case .clear: "clear"
        case .active: "active"
        case .verifying: "verifying"
        case .confirmed: "confirmed"
        case .reclaiming: "reclaiming"
        case .protected: "protected"
        case .attention: "attention"
        }
    }
}

// MARK: - Status DTOs

public struct ComponentHealth: Codable, Equatable, Sendable {
    public var healthy: Bool
    public var reasonId: String?

    private enum CodingKeys: String, CodingKey {
        case healthy
        case reasonId = "reason_id"
    }
}

public struct StorageRecovery: Codable, Equatable, Sendable {
    public var recoveryToken: String
    public var occurredAtUnixMillis: UInt64
    public var reasonId: String
    public var quarantinedSidecarCount: Int

    private enum CodingKeys: String, CodingKey {
        case recoveryToken = "recovery_token"
        case occurredAtUnixMillis = "occurred_at_unix_millis"
        case reasonId = "reason_id"
        case quarantinedSidecarCount = "quarantined_sidecar_count"
    }
}

public struct StorageHealth: Codable, Equatable, Sendable {
    public var healthy: Bool
    public var lastRecovery: StorageRecovery?

    private enum CodingKeys: String, CodingKey {
        case healthy
        case lastRecovery = "last_recovery"
    }
}

public struct ReclaimSummary: Codable, Equatable, Sendable {
    public var eventToken: String
    public var incidentId: String
    public var occurredAtUnixMillis: UInt64
    public var processOutcome: ProcessOutcome
    public var artifactOutcome: ArtifactOutcome
    public var overallOutcome: OverallOutcome

    private enum CodingKeys: String, CodingKey {
        case eventToken = "event_token"
        case incidentId = "incident_id"
        case occurredAtUnixMillis = "occurred_at_unix_millis"
        case processOutcome = "process_outcome"
        case artifactOutcome = "artifact_outcome"
        case overallOutcome = "overall_outcome"
    }
}

public struct AttentionItem: Codable, Equatable, Sendable, Identifiable {
    public var eventToken: String?
    public var kind: AttentionKind
    public var reasonId: String
    public var incidentId: String?
    public var overallOutcome: OverallOutcome?
    public var occurredAtUnixMillis: UInt64?

    public var id: String {
        eventToken ?? "\(kind.wire)|\(reasonId)|\(incidentId ?? "")|\(occurredAtUnixMillis ?? 0)"
    }

    private enum CodingKeys: String, CodingKey {
        case eventToken = "event_token"
        case kind
        case reasonId = "reason_id"
        case incidentId = "incident_id"
        case overallOutcome = "overall_outcome"
        case occurredAtUnixMillis = "occurred_at_unix_millis"
    }
}

public struct AttentionProjection: Codable, Equatable, Sendable {
    public var totalCount: Int
    public var items: [AttentionItem]

    private enum CodingKeys: String, CodingKey {
        case totalCount = "total_count"
        case items
    }
}

public struct ProtectedIncidentSummary: Codable, Equatable, Sendable, Identifiable {
    public var incidentId: String
    public var protectedAtUnixMillis: UInt64
    public var lastExactObservedAtUnixMillis: UInt64?
    public var exactAbsenceSinceUnixMillis: UInt64?

    public var id: String { incidentId }

    private enum CodingKeys: String, CodingKey {
        case incidentId = "incident_id"
        case protectedAtUnixMillis = "protected_at_unix_millis"
        case lastExactObservedAtUnixMillis = "last_exact_observed_at_unix_millis"
        case exactAbsenceSinceUnixMillis = "exact_absence_since_unix_millis"
    }
}

public struct ProtectionProjection: Codable, Equatable, Sendable {
    public var totalCount: Int
    public var items: [ProtectedIncidentSummary]

    private enum CodingKeys: String, CodingKey {
        case totalCount = "total_count"
        case items
    }
}

public struct Capability: Codable, Equatable, Sendable {
    public var available: Bool
    public var unavailableReasonId: String?

    private enum CodingKeys: String, CodingKey {
        case available
        case unavailableReasonId = "unavailable_reason_id"
    }
}

public struct GlobalCapabilities: Codable, Equatable, Sendable {
    public var pause: Capability
    public var resume: Capability

    private enum CodingKeys: String, CodingKey {
        case pause, resume
    }
}

public struct IncidentCapabilities: Codable, Equatable, Sendable {
    public var retryFailedCleanup: Capability
    public var protectIncident: Capability
    public var unprotectIncident: Capability
    public var exportDiagnostics: Capability

    private enum CodingKeys: String, CodingKey {
        case retryFailedCleanup = "retry_failed_cleanup"
        case protectIncident = "protect_incident"
        case unprotectIncident = "unprotect_incident"
        case exportDiagnostics = "export_diagnostics"
    }
}

public struct MutationAuthority: Codable, Equatable, Sendable {
    public var namespaceToken: String
    public var minimumReconciliationWindowMillis: UInt64

    private enum CodingKeys: String, CodingKey {
        case namespaceToken = "namespace_token"
        case minimumReconciliationWindowMillis = "minimum_reconciliation_window_millis"
    }
}

public struct PublicStatus: Codable, Equatable, Sendable {
    public var daemonVersion: String
    public var healthy: Bool
    public var readiness: Readiness
    public var effectiveMode: EffectiveMode
    public var scanInProgress: Bool
    public var cleanupInProgress: Bool
    public var cycleStartedAtUnixMillis: UInt64?
    public var latestObservationAtUnixMillis: UInt64?
    public var lastScanAtUnixMillis: UInt64?
    public var pausedUntilUnixMillis: UInt64?
    public var confirmedIncidentCount: Int
    public var ambiguousIncidentCount: Int
    public var mostRecentReclaim: ReclaimSummary?
    public var eventSource: ComponentHealth
    public var storage: StorageHealth
    public var attention: AttentionProjection
    public var protection: ProtectionProjection
    public var capabilities: GlobalCapabilities
    public var mutationAuthority: MutationAuthority

    private enum CodingKeys: String, CodingKey {
        case daemonVersion = "daemon_version"
        case healthy
        case readiness
        case effectiveMode = "effective_mode"
        case scanInProgress = "scan_in_progress"
        case cleanupInProgress = "cleanup_in_progress"
        case cycleStartedAtUnixMillis = "cycle_started_at_unix_millis"
        case latestObservationAtUnixMillis = "latest_observation_at_unix_millis"
        case lastScanAtUnixMillis = "last_scan_at_unix_millis"
        case pausedUntilUnixMillis = "paused_until_unix_millis"
        case confirmedIncidentCount = "confirmed_incident_count"
        case ambiguousIncidentCount = "ambiguous_incident_count"
        case mostRecentReclaim = "most_recent_reclaim"
        case eventSource = "event_source"
        case storage
        case attention
        case protection
        case capabilities
        case mutationAuthority = "mutation_authority"
    }
}

// MARK: - Atomic browser overview DTOs

public struct BrowserCompatibility: Codable, Equatable, Sendable {
    public var product: BrowserProduct
    public var observedVersion: String?
    public var decision: BrowserCompatibilityDecision
    public var reasonId: String?

    private enum CodingKeys: String, CodingKey {
        case product
        case observedVersion = "observed_version"
        case decision
        case reasonId = "reason_id"
    }
}

public struct BrowserSessionCapabilities: Codable, Equatable, Sendable {
    public var openDetail: Capability

    private enum CodingKeys: String, CodingKey {
        case openDetail = "open_detail"
    }
}

public struct BrowserSessionSummary: Codable, Equatable, Sendable, Identifiable {
    public var incidentId: String
    public var family: String
    public var state: IncidentState
    public var memberCount: Int
    public var residentMemoryBytes: UInt64
    public var compatibility: BrowserCompatibility
    public var capabilities: BrowserSessionCapabilities

    public var id: String { incidentId }

    private enum CodingKeys: String, CodingKey {
        case incidentId = "incident_id"
        case family, state
        case memberCount = "member_count"
        case residentMemoryBytes = "resident_memory_bytes"
        case compatibility, capabilities
    }
}

public struct BrowserCoverageSummary: Codable, Equatable, Sendable, Identifiable {
    public var incidentId: String
    public var decision: BrowserCompatibilityDecision
    public var reasonId: String

    public var id: String { "\(incidentId)|\(reasonId)" }

    private enum CodingKeys: String, CodingKey {
        case incidentId = "incident_id"
        case decision
        case reasonId = "reason_id"
    }
}

public struct BrowserSettlementSummary: Codable, Equatable, Sendable {
    public var eventToken: String
    public var incidentId: String
    public var family: String
    public var occurredAtUnixMillis: UInt64
    public var processCount: Int?
    public var estimatedReclaimedMemoryBytes: UInt64?
    public var revivalChecksCompleted: Int
    public var artifactOutcome: ArtifactOutcome
    public var overallOutcome: OverallOutcome

    private enum CodingKeys: String, CodingKey {
        case eventToken = "event_token"
        case incidentId = "incident_id"
        case family
        case occurredAtUnixMillis = "occurred_at_unix_millis"
        case processCount = "process_count"
        case estimatedReclaimedMemoryBytes = "estimated_reclaimed_memory_bytes"
        case revivalChecksCompleted = "revival_checks_completed"
        case artifactOutcome = "artifact_outcome"
        case overallOutcome = "overall_outcome"
    }
}

public struct BrowserFamilySupport: Codable, Equatable, Sendable, Identifiable {
    public var family: String
    public var product: BrowserProduct
    public var admittedVersions: [String]
    public var automaticActionLevel: BrowserAutomaticActionLevel

    public var id: String { family }

    private enum CodingKeys: String, CodingKey {
        case family, product
        case admittedVersions = "admitted_versions"
        case automaticActionLevel = "automatic_action_level"
    }
}

public struct BrowserSupportCatalog: Codable, Equatable, Sendable {
    public var supportRevision: String
    public var families: [BrowserFamilySupport]

    private enum CodingKeys: String, CodingKey {
        case supportRevision = "support_revision"
        case families
    }
}

public struct BrowserOverviewSnapshot: Codable, Equatable, Sendable {
    public var generatedAtUnixMillis: UInt64
    public var cycleToken: String?
    public var observedAtUnixMillis: UInt64?
    public var freshness: ObservationFreshness
    public var healthy: Bool
    public var effectiveMode: EffectiveMode
    public var pausedUntilUnixMillis: UInt64?
    public var phase: BrowserOverviewPhase
    public var sessions: [BrowserSessionSummary]
    public var coverageNotices: [BrowserCoverageSummary]
    public var recentSettlement: BrowserSettlementSummary?
    public var attention: AttentionProjection
    public var protection: ProtectionProjection
    public var supportCatalog: BrowserSupportCatalog

    private enum CodingKeys: String, CodingKey {
        case generatedAtUnixMillis = "generated_at_unix_millis"
        case cycleToken = "cycle_token"
        case observedAtUnixMillis = "observed_at_unix_millis"
        case freshness, healthy
        case effectiveMode = "effective_mode"
        case pausedUntilUnixMillis = "paused_until_unix_millis"
        case phase, sessions
        case coverageNotices = "coverage_notices"
        case recentSettlement = "recent_settlement"
        case attention, protection
        case supportCatalog = "support_catalog"
    }
}

// MARK: - History / explain DTOs

public struct RoleCount: Codable, Equatable, Sendable {
    public var role: ProcessRole
    public var count: Int

    private enum CodingKeys: String, CodingKey {
        case role, count
    }
}

public struct Evidence: Codable, Equatable, Sendable {
    public var id: String
    public var family: EvidenceFamily

    private enum CodingKeys: String, CodingKey {
        case id, family
    }
}

public struct GateLedger: Codable, Equatable, Sendable {
    public var sameUser: Bool
    public var strongAutomationProvenance: Bool
    public var confirmedAbandonment: Bool
    public var isolatedSession: Bool
    public var stableAcrossTwoObservations: Bool
    public var processIdentityUnchanged: Bool
    public var noProtectionRule: Bool

    private enum CodingKeys: String, CodingKey {
        case sameUser = "same_user"
        case strongAutomationProvenance = "strong_automation_provenance"
        case confirmedAbandonment = "confirmed_abandonment"
        case isolatedSession = "isolated_session"
        case stableAcrossTwoObservations = "stable_across_two_observations"
        case processIdentityUnchanged = "process_identity_unchanged"
        case noProtectionRule = "no_protection_rule"
    }
}

public struct ObservationRecord: Codable, Equatable, Sendable {
    public var family: String
    public var familyVersion: String
    public var state: IncidentState
    public var executableBasename: String
    public var memberCount: Int
    public var residentMemoryBytes: UInt64
    public var roles: [RoleCount]
    public var evidence: [Evidence]
    public var gates: GateLedger
    public var browserCompatibility: BrowserCompatibility? = nil

    private enum CodingKeys: String, CodingKey {
        case family
        case familyVersion = "family_version"
        case state
        case executableBasename = "executable_basename"
        case memberCount = "member_count"
        case residentMemoryBytes = "resident_memory_bytes"
        case roles, evidence, gates
        case browserCompatibility = "browser_compatibility"
    }
}

public struct ProcessAction: Codable, Equatable, Sendable, Identifiable {
    public var actionToken: String
    public var stage: CleanupStage
    public var signal: CleanupSignal
    public var disposition: SignalDisposition
    public var id: String { actionToken }

    private enum CodingKeys: String, CodingKey {
        case actionToken = "action_token"
        case stage, signal, disposition
    }
}

public struct ArtifactAction: Codable, Equatable, Sendable, Identifiable {
    public var actionToken: String
    public var kind: RuntimeArtifactKind
    public var disposition: ArtifactDisposition
    public var id: String { actionToken }

    private enum CodingKeys: String, CodingKey {
        case actionToken = "action_token"
        case kind, disposition
    }
}

public struct ResourceSnapshot: Codable, Equatable, Sendable {
    public var processCount: Int
    public var residentMemoryBytes: UInt64

    private enum CodingKeys: String, CodingKey {
        case processCount = "process_count"
        case residentMemoryBytes = "resident_memory_bytes"
    }
}

public struct ResourceReceipt: Codable, Equatable, Sendable {
    public var before: ResourceSnapshot?
    public var after: ResourceSnapshot?
    public var estimatedReclaimedMemoryBytes: UInt64?

    private enum CodingKeys: String, CodingKey {
        case before, after
        case estimatedReclaimedMemoryBytes = "estimated_reclaimed_memory_bytes"
    }
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
    public var resources: ResourceReceipt

    private enum CodingKeys: String, CodingKey {
        case state
        case reasonId = "reason_id"
        case processOutcome = "process_outcome"
        case artifactOutcome = "artifact_outcome"
        case overallOutcome = "overall_outcome"
        case attentionRequired = "attention_required"
        case processActions = "process_actions"
        case artifactActions = "artifact_actions"
        case survivorCount = "survivor_count"
        case revivalChecksCompleted = "revival_checks_completed"
        case resources
    }
}

public enum HistoryPayload: Equatable, Sendable {
    case observation(ObservationRecord)
    case cleanup(CleanupReceipt)
    case unknown(String)
}

extension HistoryPayload: Decodable {
    private enum CodingKeys: String, CodingKey {
        case recordType = "record_type"
        case observation, cleanup
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
    public var eventToken: String
    public var incidentId: String
    public var occurredAtUnixMillis: UInt64
    public var state: IncidentState
    public var payload: HistoryPayload
    public var id: String { eventToken }

    private enum CodingKeys: String, CodingKey {
        case eventToken = "event_token"
        case incidentId = "incident_id"
        case occurredAtUnixMillis = "occurred_at_unix_millis"
        case state, payload
    }
}

public struct IncidentDetail: Decodable, Equatable, Sendable {
    public var incidentId: String
    public var events: [HistoryEvent]
    public var capabilities: IncidentCapabilities

    private enum CodingKeys: String, CodingKey {
        case incidentId = "incident_id"
        case events, capabilities
    }
}

/// One entry in the latest observation snapshot. It is not a work queue and
/// does not claim that the process remains live after cleanup/revival checks.
public struct CurrentIncident: Decodable, Equatable, Sendable, Identifiable {
    public var incidentId: String
    public var observation: ObservationRecord
    public var id: String { incidentId }

    private enum CodingKeys: String, CodingKey {
        case incidentId = "incident_id"
        case observation
    }
}

public struct ObservationRoster: Decodable, Equatable, Sendable {
    public var cycleToken: String?
    public var observedAtUnixMillis: UInt64?
    public var freshness: ObservationFreshness
    public var items: [CurrentIncident]

    private enum CodingKeys: String, CodingKey {
        case cycleToken = "cycle_token"
        case observedAtUnixMillis = "observed_at_unix_millis"
        case freshness, items
    }
}

public struct DiagnosticsBundle: Decodable, Equatable, Sendable {
    public var documentSchemaVersion: Int
    public var generatedAtUnixMillis: UInt64
    public var status: PublicStatus
    public var incident: IncidentDetail

    private enum CodingKeys: String, CodingKey {
        case documentSchemaVersion = "document_schema_version"
        case generatedAtUnixMillis = "generated_at_unix_millis"
        case status, incident
    }
}

// MARK: - Ordinary mutation DTOs

public struct MutationContext: Codable, Equatable, Sendable {
    public var namespaceToken: String
    public var mutationId: String

    private enum CodingKeys: String, CodingKey {
        case namespaceToken = "namespace_token"
        case mutationId = "mutation_id"
    }
}

public enum MutationResult: Codable, Equatable, Sendable {
    case paused(untilUnixMillis: UInt64)
    case resumed
    case retryScheduled(incidentID: String)
    case incidentProtected(ProtectedIncidentSummary)
    case incidentUnprotected(incidentID: String)
    case unknown(String)

    private enum CodingKeys: String, CodingKey {
        case result
        case untilUnixMillis = "until_unix_millis"
        case incidentId = "incident_id"
        case protection
    }

    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        let result = try container.decode(String.self, forKey: .result)
        switch result {
        case "paused":
            self = .paused(untilUnixMillis: try container.decode(UInt64.self, forKey: .untilUnixMillis))
        case "resumed": self = .resumed
        case "retry_scheduled":
            self = .retryScheduled(incidentID: try container.decode(String.self, forKey: .incidentId))
        case "incident_protected":
            self = .incidentProtected(try container.decode(ProtectedIncidentSummary.self, forKey: .protection))
        case "incident_unprotected":
            self = .incidentUnprotected(incidentID: try container.decode(String.self, forKey: .incidentId))
        default: self = .unknown(result)
        }
    }

    public func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        switch self {
        case .paused(let until):
            try container.encode("paused", forKey: .result)
            try container.encode(until, forKey: .untilUnixMillis)
        case .resumed:
            try container.encode("resumed", forKey: .result)
        case .retryScheduled(let incidentID):
            try container.encode("retry_scheduled", forKey: .result)
            try container.encode(incidentID, forKey: .incidentId)
        case .incidentProtected(let protection):
            try container.encode("incident_protected", forKey: .result)
            try container.encode(protection, forKey: .protection)
        case .incidentUnprotected(let incidentID):
            try container.encode("incident_unprotected", forKey: .result)
            try container.encode(incidentID, forKey: .incidentId)
        case .unknown(let raw):
            try container.encode(raw, forKey: .result)
        }
    }
}

public enum MutationOutcome: Codable, Equatable, Sendable {
    case applied(MutationResult)
    case noChange(reasonID: String)
    case rejected(reasonID: String)
    case unknown(String)

    private enum CodingKeys: String, CodingKey {
        case outcome, result
        case reasonId = "reason_id"
    }

    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        let outcome = try container.decode(String.self, forKey: .outcome)
        switch outcome {
        case "applied": self = .applied(try container.decode(MutationResult.self, forKey: .result))
        case "no_change": self = .noChange(reasonID: try container.decode(String.self, forKey: .reasonId))
        case "rejected": self = .rejected(reasonID: try container.decode(String.self, forKey: .reasonId))
        default: self = .unknown(outcome)
        }
    }

    public func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        switch self {
        case .applied(let result):
            try container.encode("applied", forKey: .outcome)
            try container.encode(result, forKey: .result)
        case .noChange(let reasonID):
            try container.encode("no_change", forKey: .outcome)
            try container.encode(reasonID, forKey: .reasonId)
        case .rejected(let reasonID):
            try container.encode("rejected", forKey: .outcome)
            try container.encode(reasonID, forKey: .reasonId)
        case .unknown(let raw):
            try container.encode(raw, forKey: .outcome)
        }
    }
}

public struct MutationReceipt: Codable, Equatable, Sendable {
    public var namespaceToken: String
    public var mutationId: String
    public var kind: MutationKind
    public var committedAtUnixMillis: UInt64
    public var retainUntilUnixMillis: UInt64
    public var policyRevisionAfter: UInt64
    public var outcome: MutationOutcome

    private enum CodingKeys: String, CodingKey {
        case namespaceToken = "namespace_token"
        case mutationId = "mutation_id"
        case kind
        case committedAtUnixMillis = "committed_at_unix_millis"
        case retainUntilUnixMillis = "retain_until_unix_millis"
        case policyRevisionAfter = "policy_revision_after"
        case outcome
    }
}

public enum MutationStatus: Codable, Equatable, Sendable {
    case notFound(MutationContext)
    case authorityLost(MutationContext)
    case committed(MutationReceipt)

    private enum CodingKeys: String, CodingKey {
        case status, context, receipt
    }

    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        switch try container.decode(String.self, forKey: .status) {
        case "not_found": self = .notFound(try container.decode(MutationContext.self, forKey: .context))
        case "authority_lost": self = .authorityLost(try container.decode(MutationContext.self, forKey: .context))
        case "committed": self = .committed(try container.decode(MutationReceipt.self, forKey: .receipt))
        default:
            throw DecodingError.dataCorruptedError(
                forKey: .status,
                in: container,
                debugDescription: "unknown mutation status"
            )
        }
    }

    public func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        switch self {
        case .notFound(let context):
            try container.encode("not_found", forKey: .status)
            try container.encode(context, forKey: .context)
        case .authorityLost(let context):
            try container.encode("authority_lost", forKey: .status)
            try container.encode(context, forKey: .context)
        case .committed(let receipt):
            try container.encode("committed", forKey: .status)
            try container.encode(receipt, forKey: .receipt)
        }
    }
}

// MARK: - Commands

public enum Command: Sendable, Equatable {
    case status
    case browserOverview
    case history(limit: Int)
    case explain(incidentID: String)
    case incidents
    case mutationStatus(context: MutationContext)
    case pause(context: MutationContext, durationMillis: UInt64)
    case resume(context: MutationContext)
    case retryFailedCleanup(context: MutationContext, incidentID: String)
    case protectIncident(context: MutationContext, incidentID: String)
    case unprotectIncident(context: MutationContext, incidentID: String)
    case exportDiagnostics(incidentID: String)

    public var mutationContext: MutationContext? {
        switch self {
        case .pause(let context, _), .resume(let context),
             .retryFailedCleanup(let context, _),
             .protectIncident(let context, _),
             .unprotectIncident(let context, _): context
        case .status, .browserOverview, .history, .explain, .incidents,
             .mutationStatus, .exportDiagnostics: nil
        }
    }

    public var isMutation: Bool { mutationContext != nil }
}

extension Command: Encodable {
    private struct DynamicKey: CodingKey {
        var stringValue: String
        var intValue: Int?
        init?(stringValue: String) { self.stringValue = stringValue }
        init?(intValue: Int) { nil }
    }

    public func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: DynamicKey.self)
        let commandKey = DynamicKey(stringValue: "command")!
        let contextKey = DynamicKey(stringValue: "context")!
        switch self {
        case .status:
            try container.encode("status", forKey: commandKey)
        case .browserOverview:
            try container.encode("browser_overview", forKey: commandKey)
        case .history(let limit):
            try container.encode("history", forKey: commandKey)
            try container.encode(limit, forKey: DynamicKey(stringValue: "limit")!)
        case .explain(let incidentID):
            try container.encode("explain", forKey: commandKey)
            try container.encode(incidentID, forKey: DynamicKey(stringValue: "incident_id")!)
        case .incidents:
            try container.encode("incidents", forKey: commandKey)
        case .mutationStatus(let context):
            try container.encode("mutation_status", forKey: commandKey)
            try container.encode(context, forKey: contextKey)
        case .pause(let context, let duration):
            try container.encode("pause", forKey: commandKey)
            try container.encode(context, forKey: contextKey)
            try container.encode(duration, forKey: DynamicKey(stringValue: "duration_millis")!)
        case .resume(let context):
            try container.encode("resume", forKey: commandKey)
            try container.encode(context, forKey: contextKey)
        case .retryFailedCleanup(let context, let incidentID):
            try container.encode("retry_failed_cleanup", forKey: commandKey)
            try container.encode(context, forKey: contextKey)
            try container.encode(incidentID, forKey: DynamicKey(stringValue: "incident_id")!)
        case .protectIncident(let context, let incidentID):
            try container.encode("protect_incident", forKey: commandKey)
            try container.encode(context, forKey: contextKey)
            try container.encode(incidentID, forKey: DynamicKey(stringValue: "incident_id")!)
        case .unprotectIncident(let context, let incidentID):
            try container.encode("unprotect_incident", forKey: commandKey)
            try container.encode(context, forKey: contextKey)
            try container.encode(incidentID, forKey: DynamicKey(stringValue: "incident_id")!)
        case .exportDiagnostics(let incidentID):
            try container.encode("export_diagnostics", forKey: commandKey)
            try container.encode(incidentID, forKey: DynamicKey(stringValue: "incident_id")!)
        }
    }
}

// MARK: - Envelope

struct RequestEnvelope: Encodable {
    let schemaVersion = 4
    let requestID: UInt64
    let command: Command

    enum CodingKeys: String, CodingKey {
        case schemaVersion = "schema_version"
        case requestID = "request_id"
        case command
    }
}
