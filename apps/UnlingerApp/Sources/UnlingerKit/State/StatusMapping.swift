import Foundation

/// Pure mapping from `PublicStatus` to what the UI may say.
///
/// Rules encoded here come from FRONTEND_BOUNDARY.md "UI state mapping boundary":
/// - healthy + ready + report_only is quiet observation; report_only must be
///   stated plainly as "automatic cleanup off";
/// - scan/cleanup in progress is transient activity, never a warning;
/// - attention items map by kind + reason_id, unknown values get a generic
///   fallback and never widen authorization;
/// - `cleared_with_residue` is a process success with kept residue, not a
///   process failure;
/// - ambiguous count is low-key information only.
public enum StatusTone: String, Equatable, Sendable {
    case quiet
    case activity
    case attention
}

public struct AttentionViewData: Equatable, Sendable, Identifiable {
    public var copyKey: String
    public var incidentID: String?
    public var overallOutcome: OverallOutcome?
    public var occurredAt: Date?

    public var id: String { "\(copyKey)|\(incidentID ?? "")|\(occurredAt?.timeIntervalSince1970 ?? 0)" }
}

public struct ReclaimViewData: Equatable, Sendable {
    public var copyKey: String
    public var incidentID: String
    public var occurredAt: Date
}

public struct StatusViewModel: Equatable, Sendable {
    public var tone: StatusTone
    public var headlineKey: String
    public var detailKey: String?
    public var modeIsReportOnly: Bool
    public var isPaused: Bool
    public var pausedUntil: Date?
    public var scanInProgress: Bool
    public var cleanupInProgress: Bool
    public var lastScanAt: Date?
    public var confirmedCount: Int
    public var ambiguousCount: Int
    public var attention: [AttentionViewData]
    public var attentionOverflow: Int
    public var protections: [ProtectedIncidentSummary]
    public var recentReclaim: ReclaimViewData?
}

public enum StatusMapper {
    public static func viewModel(for status: PublicStatus, now: Date = Date()) -> StatusViewModel {
        let tone: StatusTone
        let headlineKey: String
        if !status.healthy || status.readiness == .failed || status.attention.totalCount > 0 {
            tone = .attention
            headlineKey = "status.headline.attention"
        } else if status.scanInProgress || status.cleanupInProgress {
            tone = .activity
            headlineKey = "status.headline.activity"
        } else {
            tone = .quiet
            headlineKey = "status.headline.quiet"
        }

        let detailKey: String? = if !status.eventSource.healthy {
            "status.detail.event_source_degraded"
        } else if !status.storage.healthy {
            "status.detail.storage_degraded"
        } else {
            nil
        }

        let pausedUntil = status.pausedUntilUnixMillis.map { Date(unixMillis: $0) }

        return StatusViewModel(
            tone: tone,
            headlineKey: headlineKey,
            detailKey: detailKey,
            modeIsReportOnly: status.effectiveMode == .reportOnly,
            isPaused: pausedUntil != nil,
            pausedUntil: pausedUntil,
            scanInProgress: status.scanInProgress,
            cleanupInProgress: status.cleanupInProgress,
            lastScanAt: status.lastScanAtUnixMillis.map { Date(unixMillis: $0) },
            confirmedCount: status.confirmedIncidentCount,
            ambiguousCount: status.ambiguousIncidentCount,
            attention: status.attention.items.map(Self.attentionViewData),
            attentionOverflow: max(0, status.attention.totalCount - status.attention.items.count),
            protections: status.protection.items,
            recentReclaim: status.mostRecentReclaim.map(Self.reclaimViewData)
        )
    }

    static func attentionViewData(for item: AttentionItem) -> AttentionViewData {
        let key = switch (item.kind, item.reasonId) {
        case (.daemonUnhealthy, _):
            "attention.daemon_unhealthy"
        case (.cleanupResidue, _), (.cleanupFailed, "cleanup.artifact_unsafe"):
            // cleared_with_residue: process tree proved gone, low-risk residue
            // kept. Never phrase this as a process failure.
            "attention.residue"
        case (.cleanupFailed, "cleanup.delivery_unknown"):
            "attention.failed.delivery_unknown"
        case (.cleanupFailed, _):
            "attention.failed.generic"
        case (.cleanupRevived, _):
            "attention.revived"
        case (.eventSourceDegraded, _):
            "attention.event_source"
        case (.storageRecovered, _):
            "attention.storage_recovered"
        case (.unknown, _):
            "attention.generic"
        }
        return AttentionViewData(
            copyKey: key,
            incidentID: item.incidentId,
            overallOutcome: item.overallOutcome,
            occurredAt: item.occurredAtUnixMillis.map { Date(unixMillis: $0) }
        )
    }

    static func reclaimViewData(for reclaim: ReclaimSummary) -> ReclaimViewData {
        let key = switch reclaim.overallOutcome {
        case .cleared: "reclaim.cleared"
        case .clearedWithResidue: "reclaim.cleared_with_residue"
        case .revived: "reclaim.revived"
        case .failed: "reclaim.failed"
        case .unknown: "reclaim.generic"
        }
        return ReclaimViewData(
            copyKey: key,
            incidentID: reclaim.incidentId,
            occurredAt: Date(unixMillis: reclaim.occurredAtUnixMillis)
        )
    }
}

extension Date {
    init(unixMillis: UInt64) {
        self.init(timeIntervalSince1970: TimeInterval(unixMillis) / 1000)
    }

    var unixMillis: UInt64 { UInt64(timeIntervalSince1970 * 1000) }
}
