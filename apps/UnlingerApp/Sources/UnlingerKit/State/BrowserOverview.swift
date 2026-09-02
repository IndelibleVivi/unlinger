import Foundation

public enum BrowserOverviewPhase: Equatable, Sendable {
    case unknown
    case clear
    case active
    case verifying
    case confirmed
    case reclaiming
    case protected
    case attention
}

public enum BrowserOverviewTone: Equatable, Sendable {
    case quiet
    case activity
    case attention
}

public enum BrowserCoverageNotice: Equatable, Hashable, Sendable {
    case mixedVersions
    case unsupportedProduct
    case unsupportedVersion
    case versionUnavailable
    case controllerUnverified
    case observationOnly
    case controlPathIncomplete
    case unknown(reasonID: String)

    public var copyKey: String {
        switch self {
        case .mixedVersions: "browser.coverage.mixed_versions"
        case .unsupportedProduct: "browser.coverage.unsupported_product"
        case .unsupportedVersion: "browser.coverage.unsupported_version"
        case .versionUnavailable: "browser.coverage.version_unavailable"
        case .controllerUnverified: "browser.coverage.controller_unverified"
        case .observationOnly: "browser.coverage.observation_only"
        case .controlPathIncomplete: "browser.coverage.control_path_incomplete"
        case .unknown: "browser.coverage.generic"
        }
    }
}

public struct BrowserSessionPresentation: Equatable, Identifiable, Sendable {
    public var incidentID: String
    public var familyKey: String
    public var productKey: String?
    public var observedVersion: String?
    public var state: IncidentState
    public var stateKey: String
    public var reasonKey: String?
    public var memberCount: Int
    public var residentMemoryBytes: UInt64
    public var coverageNotice: BrowserCoverageNotice?
    public var isPreviousObservation: Bool

    public var id: String { incidentID }
}

public struct BrowserAttentionPresentation: Equatable, Identifiable, Sendable {
    public var eventToken: String?
    public var copyKey: String
    public var incidentID: String?
    public var overallOutcome: OverallOutcome?
    public var occurredAt: Date?

    public var id: String {
        eventToken ?? "\(copyKey)|\(incidentID ?? "")|\(occurredAt?.timeIntervalSince1970 ?? 0)"
    }
}

public struct RecentBrowserSettlement: Equatable, Sendable {
    public var incidentID: String
    public var eventToken: String
    public var familyKey: String?
    public var occurredAt: Date
    public var processCount: Int?
    public var estimatedReclaimedMemoryBytes: UInt64?
    public var revivalChecksCompleted: Int?
    public var artifactOutcome: ArtifactOutcome
    public var overallOutcome: OverallOutcome
    public var isFallback: Bool
}

public enum BrowserPopoverSection: Equatable, Hashable, Sendable {
    case overview
    case connection
    case sessions
    case coverage
    case savedProtections
    case attention
    case recentSettlement
    case history
    case settings
    case actions
}

public struct BrowserOverview: Equatable, Sendable {
    public var phase: BrowserOverviewPhase
    public var tone: BrowserOverviewTone
    public var headlineKey: String
    public var detailKey: String?
    public var modeKey: String
    public var snapshotTrusted: Bool
    public var requiresTrailingRefresh: Bool
    public var observedAt: Date?
    public var pausedUntil: Date?
    public var sessions: [BrowserSessionPresentation]
    public var coverageNotices: [BrowserCoverageNotice]
    public var attention: [BrowserAttentionPresentation]
    public var attentionOverflow: Int
    public var savedProtections: [ProtectedIncidentSummary]
    public var recentSettlement: RecentBrowserSettlement?

    public func visibleSections(connection: ConnectionState) -> [BrowserPopoverSection] {
        var result: [BrowserPopoverSection] = [.overview]
        if connection != .live { result.append(.connection) }
        if !sessions.isEmpty { result.append(.sessions) }
        if !coverageNotices.isEmpty { result.append(.coverage) }
        if !savedProtections.isEmpty { result.append(.savedProtections) }
        if !attention.isEmpty || attentionOverflow > 0 { result.append(.attention) }
        if recentSettlement != nil { result.append(.recentSettlement) }
        if connection == .live {
            result.append(contentsOf: [.history, .settings, .actions])
        }
        return result
    }
}
