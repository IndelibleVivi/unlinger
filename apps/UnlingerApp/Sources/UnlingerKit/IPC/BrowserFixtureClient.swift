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

/// Composes canonical fixtures into deterministic product states for previews
/// and local visual QA. Production product logic remains daemon-owned.
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

    public func browserOverview() async throws(ClientError) -> BrowserOverviewSnapshot {
        let status = try await status()
        let roster = try await incidents()
        let phase: BrowserOverviewPhase = switch scenario {
        case .clear, .recentSettlement: .clear
        case .active: .active
        case .verifying: .verifying
        case .confirmedReportOnly: .confirmed
        case .reclaiming: .reclaiming
        case .protectedUnsupported: .protected
        case .attention: .attention
        }
        let sessions = roster.items.map { item in
            let compatibility = scenario == .protectedUnsupported
                ? BrowserCompatibility(
                    product: .chromeForTesting,
                    observedVersion: "151.0.7922.35",
                    decision: .protected,
                    reasonId: "protection.browser_version_unsupported"
                )
                : BrowserCompatibility(
                    product: .chromeForTesting,
                    observedVersion: "151.0.7922.34",
                    decision: .automatic,
                    reasonId: nil
                )
            return BrowserSessionSummary(
                incidentId: item.incidentId,
                family: item.observation.family,
                state: item.observation.state,
                memberCount: item.observation.memberCount,
                residentMemoryBytes: item.observation.residentMemoryBytes,
                compatibility: compatibility,
                capabilities: BrowserSessionCapabilities(
                    openDetail: Capability(available: true)
                )
            )
        }
        let coverage = sessions.compactMap { session -> BrowserCoverageSummary? in
            guard let reasonId = session.compatibility.reasonId else { return nil }
            return BrowserCoverageSummary(
                incidentId: session.incidentId,
                decision: session.compatibility.decision,
                reasonId: reasonId
            )
        }
        let settlement = scenario == .recentSettlement
            ? BrowserSettlementSummary(
                eventToken: "history-cleared-event-1",
                incidentId: "redacted-incident-1",
                family: "chrome-for-testing",
                occurredAtUnixMillis: Self.observedAt,
                processCount: 8,
                estimatedReclaimedMemoryBytes: 912_261_120,
                revivalChecksCompleted: 2,
                artifactOutcome: .reconciled,
                overallOutcome: .cleared
            )
            : nil
        return BrowserOverviewSnapshot(
            generatedAtUnixMillis: Self.observedAt,
            cycleToken: roster.cycleToken,
            observedAtUnixMillis: roster.observedAtUnixMillis,
            freshness: scenario == .attention ? .current : roster.freshness,
            healthy: status.healthy,
            effectiveMode: status.effectiveMode,
            pausedUntilUnixMillis: status.pausedUntilUnixMillis,
            phase: phase,
            sessions: sessions,
            coverageNotices: coverage,
            recentSettlement: settlement,
            attention: status.attention,
            protection: status.protection,
            supportCatalog: BrowserSupportCatalog(
                supportRevision: "fixture:rules-v1",
                families: ["agent-browser", "playwright", "puppeteer"].map {
                    BrowserFamilySupport(
                        family: $0,
                        product: .chromeForTesting,
                        admittedVersions: ["151.0.7922.34"],
                        automaticActionLevel: .automatic
                    )
                }
            )
        )
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
