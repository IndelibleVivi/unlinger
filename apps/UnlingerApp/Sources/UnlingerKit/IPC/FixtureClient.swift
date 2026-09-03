import Foundation

private struct FixtureEnvelopeHeader: Decodable {
    let requestID: UInt64

    private enum CodingKeys: String, CodingKey {
        case requestID = "request_id"
    }
}

/// Drives the app from canonical fixtures: SwiftUI previews, tests, and
/// offline development against every documented state without a daemon.
///
/// Mutation behavior is scriptable: `mutationError` injects a transport
/// outcome (e.g. `.deliveryUncertain`) so the readback flow can be exercised.
public struct FixtureClient: UnlingerClient {
    public var statusFixture: String
    public var historyFixture: String?
    public var incidentFixture: String?
    public var incidentsFixture: String?
    /// If the first entry is an error, every mutation throws it (e.g.
    /// `.deliveryUncertain` to preview the readback state). Tests that need a
    /// sequenced script use their own mock client.
    public var mutationResults: [ClientError?]

    public init(
        statusFixture: String,
        historyFixture: String? = nil,
        incidentFixture: String? = nil,
        incidentsFixture: String? = nil,
        mutationResults: [ClientError?] = []
    ) {
        self.statusFixture = statusFixture
        self.historyFixture = historyFixture
        self.incidentFixture = incidentFixture
        self.incidentsFixture = incidentsFixture
        self.mutationResults = mutationResults
    }

    public func status() async throws(ClientError) -> PublicStatus {
        try decodeFixture(statusFixture, command: .status)
    }

    public func browserOverview() async throws(ClientError) -> BrowserOverviewSnapshot {
        let status = try await status()
        let roster = try await incidents()
        let phase: BrowserOverviewPhase
        if !status.healthy || status.readiness != .ready || roster.freshness != .current {
            phase = .unknown
        } else if status.attention.totalCount > 0 {
            phase = .attention
        } else {
            phase = .clear
        }
        return BrowserOverviewSnapshot(
            generatedAtUnixMillis: UInt64(Date().timeIntervalSince1970 * 1_000),
            cycleToken: roster.cycleToken,
            observedAtUnixMillis: roster.observedAtUnixMillis,
            freshness: roster.freshness,
            healthy: status.healthy,
            effectiveMode: status.effectiveMode,
            pausedUntilUnixMillis: status.pausedUntilUnixMillis,
            phase: phase,
            sessions: [],
            coverageNotices: [],
            recentSettlement: nil,
            attention: status.attention,
            protection: status.protection,
            supportCatalog: Self.fixtureSupportCatalog
        )
    }

    public func history(limit: Int) async throws(ClientError) -> [HistoryEvent] {
        guard let historyFixture else { return [] }
        return try decodeFixture(historyFixture, command: .history(limit: limit))
    }

    public func explain(incidentID: String) async throws(ClientError) -> IncidentDetail {
        guard let incidentFixture else { throw .serverError(code: "not_found", message: "no incident fixture") }
        return try decodeFixture(incidentFixture, command: .explain(incidentID: incidentID))
    }

    public func incidents() async throws(ClientError) -> ObservationRoster {
        guard let incidentsFixture else {
            return ObservationRoster(
                cycleToken: nil,
                observedAtUnixMillis: nil,
                freshness: .neverObserved,
                items: []
            )
        }
        return try decodeFixture(incidentsFixture, command: .incidents)
    }

    public func mutationStatus(context: MutationContext) async throws(ClientError) -> MutationStatus {
        .notFound(context)
    }

    public func pause(context: MutationContext, durationMillis: UInt64) async throws(ClientError) -> MutationReceipt {
        try takeMutationResult()
        let deadline = UInt64(Date().timeIntervalSince1970 * 1000) + durationMillis
        return receipt(context: context, kind: .pause, result: .paused(untilUnixMillis: deadline))
    }

    public func resume(context: MutationContext) async throws(ClientError) -> MutationReceipt {
        try takeMutationResult()
        return receipt(context: context, kind: .resume, result: .resumed)
    }

    public func retryFailedCleanup(context: MutationContext, incidentID: String) async throws(ClientError) -> MutationReceipt {
        try takeMutationResult()
        return receipt(
            context: context,
            kind: .retryFailedCleanup,
            result: .retryScheduled(incidentID: incidentID)
        )
    }

    public func protectIncident(context: MutationContext, incidentID: String) async throws(ClientError) -> MutationReceipt {
        try takeMutationResult()
        return receipt(
            context: context,
            kind: .protectIncident,
            result: .incidentProtected(
                ProtectedIncidentSummary(
                    incidentId: incidentID,
                    protectedAtUnixMillis: UInt64(Date().timeIntervalSince1970 * 1000),
                    lastExactObservedAtUnixMillis: nil,
                    exactAbsenceSinceUnixMillis: nil
                )
            )
        )
    }

    public func unprotectIncident(context: MutationContext, incidentID: String) async throws(ClientError) -> MutationReceipt {
        try takeMutationResult()
        return receipt(
            context: context,
            kind: .unprotectIncident,
            result: .incidentUnprotected(incidentID: incidentID)
        )
    }

    public func exportDiagnostics(incidentID: String) async throws(ClientError) -> DiagnosticsExport {
        guard let incidentFixture else {
            throw .serverError(code: "not_found", message: "no incident fixture")
        }
        let status: PublicStatus = try decodeFixture(statusFixture, command: .status)
        let incident: IncidentDetail = try decodeFixture(
            incidentFixture,
            command: .explain(incidentID: incidentID)
        )
        return DiagnosticsExport(
            bundle: DiagnosticsBundle(
                documentSchemaVersion: 3,
                generatedAtUnixMillis: UInt64(Date().timeIntervalSince1970 * 1000),
                status: status,
                incident: incident
            ),
            rawJSON: nil
        )
    }

    private func receipt(
        context: MutationContext,
        kind: MutationKind,
        result: MutationResult
    ) -> MutationReceipt {
        let committedAt = UInt64(Date().timeIntervalSince1970 * 1000)
        return MutationReceipt(
            namespaceToken: context.namespaceToken,
            mutationId: context.mutationId,
            kind: kind,
            committedAtUnixMillis: committedAt,
            retainUntilUnixMillis: committedAt + 14 * 24 * 60 * 60 * 1_000,
            policyRevisionAfter: 2,
            outcome: .applied(result)
        )
    }

    private func takeMutationResult() throws(ClientError) {
        // FixtureClient is a value type shared read-only; mutation scripting is
        // handled by callers constructing a fresh client per scenario, so the
        // first scripted result (if any) applies to every mutation here.
        if let error = mutationResults.first, let error {
            throw error
        }
    }

    private func decodeFixture<T: Decodable & Sendable>(_ name: String, command: Command) throws(ClientError) -> T {
        let data: Data
        do {
            data = try FixtureStore.data(named: name)
        } catch {
            throw .protocolError("fixture \(name) unreadable: \(error.localizedDescription)")
        }
        // Canonical fixtures embed their own request_id; echo it so envelope
        // validation runs exactly as it does over the wire.
        let requestID = (try? JSONDecoder()
            .decode(FixtureEnvelopeHeader.self, from: data).requestID) ?? 0
        let expectedType: String = switch command {
        case .status: "status"
        case .browserOverview: "browser_overview"
        case .history: "history"
        case .explain: "incident"
        case .incidents: "incidents"
        case .mutationStatus: "mutation_status"
        case .pause, .resume, .retryFailedCleanup, .protectIncident,
             .unprotectIncident: "mutation_committed"
        case .exportDiagnostics: "diagnostics"
        }
        return try ResponseDecoder.decode(
            T.self,
            expectedPayloadType: expectedType,
            requestID: requestID,
            expectedSchemaVersion: 3,
            line: data
        )
    }

    private static let fixtureSupportCatalog = BrowserSupportCatalog(
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
}

extension FixtureClient {
    /// Named demo scenarios (`UNLINGER_FIXTURE=<name>`) pairing a status
    /// fixture with plausible history/incident fixtures so every surface has
    /// something to show. Unknown names fall back to `status-all-clear`.
    public static func scenario(_ name: String) -> any UnlingerClient {
        switch name {
        case "all-clear", "browser-clear":
            BrowserFixtureClient(scenario: .clear)
        case "browser-active":
            BrowserFixtureClient(scenario: .active)
        case "browser-verifying":
            BrowserFixtureClient(scenario: .verifying)
        case "report-only", "browser-confirmed-report-only":
            BrowserFixtureClient(scenario: .confirmedReportOnly)
        case "browser-reclaiming":
            BrowserFixtureClient(scenario: .reclaiming)
        case "browser-protected-unsupported":
            BrowserFixtureClient(scenario: .protectedUnsupported)
        case "scanning":
            FixtureClient(statusFixture: "status-scanning")
        case "paused":
            FixtureClient(statusFixture: "status-paused")
        case "recently-reclaimed", "browser-recent-settlement":
            BrowserFixtureClient(scenario: .recentSettlement)
        case "browser-history-stress":
            BrowserFixtureClient(scenario: .historyStress)
        case "needs-attention", "browser-attention":
            BrowserFixtureClient(scenario: .attention)
        case "delivery-uncertain":
            FixtureClient(
                statusFixture: "status-all-clear",
                mutationResults: [.deliveryUncertain]
            )
        default:
            BrowserFixtureClient(scenario: .clear)
        }
    }
}
