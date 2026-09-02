import Foundation

public enum BrowserFixtureScenario: Equatable, Sendable {
    case clear
    case active
    case verifying
    case confirmedReportOnly
    case reclaiming
    case protectedUnsupported
    case attention
    case recentSettlement
}

/// Composes canonical v3 fixtures into deterministic product states for
/// previews and local visual QA. It changes no wire fixture or product logic.
public struct BrowserFixtureClient: UnlingerClient {
    public var scenario: BrowserFixtureScenario

    public init(scenario: BrowserFixtureScenario) {
        self.scenario = scenario
    }

    public func status() async throws(ClientError) -> PublicStatus {
        var status = try await base.status()
        status.latestObservationAtUnixMillis = Self.observedAt
        switch scenario {
        case .clear, .active, .verifying, .protectedUnsupported:
            status.effectiveMode = .reportOnly
            status.confirmedIncidentCount = 0
            status.ambiguousIncidentCount = 0
        case .confirmedReportOnly:
            status.effectiveMode = .reportOnly
            status.confirmedIncidentCount = 1
            status.ambiguousIncidentCount = 0
        case .reclaiming:
            status.effectiveMode = .enforce
            status.cleanupInProgress = true
            status.confirmedIncidentCount = 1
            status.ambiguousIncidentCount = 0
        case .attention:
            break
        case .recentSettlement:
            status.effectiveMode = .reportOnly
            status.mostRecentReclaim?.eventToken = "history-cleared-event-1"
        }
        return status
    }

    public func history(limit: Int) async throws(ClientError) -> [HistoryEvent] {
        var history = try await base.history(limit: limit)
        guard scenario == .recentSettlement,
              let cleanup = history.first,
              let sourceRoster = try? await FixtureClient(
                  statusFixture: "status-all-clear",
                  incidentsFixture: "roster-current"
              ).incidents(),
              let source = sourceRoster.items.first
        else {
            return history
        }
        history.append(HistoryEvent(
            eventToken: "browser-preview-prior-observation",
            incidentId: cleanup.incidentId,
            occurredAtUnixMillis: cleanup.occurredAtUnixMillis - 1,
            state: .confirmed,
            payload: .observation(source.observation)
        ))
        return history
    }

    public func incidents() async throws(ClientError) -> ObservationRoster {
        var roster = try await FixtureClient(
            statusFixture: "status-all-clear",
            incidentsFixture: "roster-current"
        ).incidents()
        roster.observedAtUnixMillis = Self.observedAt
        roster.freshness = scenario == .attention ? .staleAfterFailure : .current
        guard var first = roster.items.first else { return roster }

        switch scenario {
        case .clear, .recentSettlement:
            roster.items = []
        case .active:
            first.observation.state = .active
            roster.items = [first]
        case .verifying:
            first.observation.state = .cooling
            roster.items = [first]
        case .confirmedReportOnly:
            first.observation.state = .confirmed
            roster.items = [first]
        case .reclaiming:
            first.observation.state = .reclaiming
            roster.items = [first]
        case .protectedUnsupported:
            first.observation.state = .protected
            first.observation.evidence = [
                Evidence(
                    id: "protection.browser_version_unsupported",
                    family: .protection
                )
            ]
            roster.items = [first]
        case .attention:
            roster.items = [first]
        }
        return roster
    }

    public func explain(incidentID: String) async throws(ClientError) -> IncidentDetail {
        try await base.explain(incidentID: incidentID)
    }

    public func mutationStatus(context: MutationContext) async throws(ClientError) -> MutationStatus {
        try await base.mutationStatus(context: context)
    }

    public func pause(
        context: MutationContext,
        durationMillis: UInt64
    ) async throws(ClientError) -> MutationReceipt {
        try await base.pause(context: context, durationMillis: durationMillis)
    }

    public func resume(context: MutationContext) async throws(ClientError) -> MutationReceipt {
        try await base.resume(context: context)
    }

    public func retryFailedCleanup(
        context: MutationContext,
        incidentID: String
    ) async throws(ClientError) -> MutationReceipt {
        try await base.retryFailedCleanup(context: context, incidentID: incidentID)
    }

    public func protectIncident(
        context: MutationContext,
        incidentID: String
    ) async throws(ClientError) -> MutationReceipt {
        try await base.protectIncident(context: context, incidentID: incidentID)
    }

    public func unprotectIncident(
        context: MutationContext,
        incidentID: String
    ) async throws(ClientError) -> MutationReceipt {
        try await base.unprotectIncident(context: context, incidentID: incidentID)
    }

    public func exportDiagnostics(incidentID: String) async throws(ClientError) -> DiagnosticsExport {
        try await base.exportDiagnostics(incidentID: incidentID)
    }

    private var base: FixtureClient {
        switch scenario {
        case .clear, .active, .verifying, .protectedUnsupported:
            FixtureClient(
                statusFixture: "status-all-clear",
                incidentFixture: "incident-protected"
            )
        case .confirmedReportOnly:
            FixtureClient(
                statusFixture: "status-report-only",
                incidentFixture: "incident-protected"
            )
        case .reclaiming:
            FixtureClient(
                statusFixture: "status-enforce",
                incidentFixture: "incident-revived"
            )
        case .attention:
            FixtureClient(
                statusFixture: "status-needs-attention",
                historyFixture: "history-cleared-with-residue",
                incidentFixture: "incident-failed"
            )
        case .recentSettlement:
            FixtureClient(
                statusFixture: "status-recently-reclaimed",
                historyFixture: "history-cleared",
                incidentFixture: "incident-revived"
            )
        }
    }

    private static let observedAt: UInt64 = 1_788_148_800_000
}
