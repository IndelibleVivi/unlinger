import Foundation
import Testing
@testable import UnlingerKit

@Suite("Browser overview mapping")
struct BrowserOverviewMappingTests {
    @Test("a coherent empty current roster permits only the scoped clear claim")
    func coherentEmptyRosterIsClear() async throws {
        let (status, roster) = try await coherentSnapshot(items: [])

        let overview = BrowserOverviewMapper.make(
            connection: .live,
            status: status,
            roster: roster,
            history: []
        )

        #expect(overview.phase == .clear)
        #expect(overview.snapshotTrusted)
        #expect(overview.headlineKey == "browser.overview.clear")
    }

    @Test("a stale roster cannot produce clear")
    func staleRosterIsUnknown() async throws {
        let (status, initialRoster) = try await coherentSnapshot(items: [])
        var roster = initialRoster
        roster.freshness = .staleAfterFailure

        let overview = BrowserOverviewMapper.make(
            connection: .live,
            status: status,
            roster: roster,
            history: []
        )

        #expect(overview.phase == .unknown)
        #expect(!overview.snapshotTrusted)
        #expect(overview.headlineKey == "browser.overview.unavailable")
    }

    @Test("never-observed state cannot produce clear")
    func neverObservedIsUnknown() async throws {
        let status = try await loadStatus("status-all-clear")
        let roster = ObservationRoster(
            cycleToken: nil,
            observedAtUnixMillis: nil,
            freshness: .neverObserved,
            items: []
        )

        let overview = BrowserOverviewMapper.make(
            connection: .live,
            status: status,
            roster: roster,
            history: []
        )

        #expect(overview.phase == .unknown)
        #expect(overview.headlineKey == "browser.overview.updating")
    }

    @Test("mismatched observation timestamps cannot produce a positive current claim")
    func mismatchedSnapshotIsUnknown() async throws {
        var (status, roster) = try await coherentSnapshot(items: [])
        status.latestObservationAtUnixMillis = (roster.observedAtUnixMillis ?? 0) + 1

        let overview = BrowserOverviewMapper.make(
            connection: .live,
            status: status,
            roster: roster,
            history: []
        )

        #expect(overview.phase == .unknown)
        #expect(!overview.snapshotTrusted)
        #expect(overview.requiresTrailingRefresh)
    }

    @Test("scan in progress uses updating and marks retained rows as previous")
    func scanInProgressUsesPreviousRows() async throws {
        var (status, roster) = try await coherentSnapshot()
        status.scanInProgress = true
        roster.freshness = .scanInProgress

        let overview = BrowserOverviewMapper.make(
            connection: .live,
            status: status,
            roster: roster,
            history: []
        )

        #expect(overview.phase == .unknown)
        #expect(overview.headlineKey == "browser.overview.updating")
        #expect(overview.sessions.allSatisfy { $0.isPreviousObservation })
    }

    @Test("durable attention remains visible above roster activity")
    func attentionOutranksCurrentPhases() async throws {
        var (status, roster) = try await coherentSnapshot()
        status.attention = AttentionProjection(
            totalCount: 1,
            items: [AttentionItem(
                eventToken: "event-attention",
                kind: .cleanupFailed,
                reasonId: "cleanup.failed",
                incidentId: roster.items.first?.incidentId,
                overallOutcome: .failed,
                occurredAtUnixMillis: roster.observedAtUnixMillis
            )]
        )
        roster.items[0].observation.state = .reclaiming

        let overview = BrowserOverviewMapper.make(
            connection: .live,
            status: status,
            roster: roster,
            history: []
        )

        #expect(overview.phase == .attention)
        #expect(overview.attention.count == 1)
    }

    @Test("typed attention kinds retain their safe copy mapping")
    func attentionCopyMapping() async throws {
        let status = try await loadStatus("status-needs-attention")
        let roster = try await loadRoster("roster-stale")
        let overview = BrowserOverviewMapper.make(
            connection: .live,
            status: status,
            roster: roster,
            history: []
        )

        #expect(overview.attention.map(\.copyKey).contains("attention.event_source"))
        #expect(overview.attention.map(\.copyKey).contains("attention.daemon_unhealthy"))
        #expect(overview.attention.map(\.copyKey).contains("attention.residue"))
    }

    @Test("unknown attention kind falls back without displaying its raw reason")
    func unknownAttentionIsGeneric() async throws {
        var (status, roster) = try await coherentSnapshot()
        status.attention = AttentionProjection(
            totalCount: 1,
            items: [AttentionItem(
                eventToken: nil,
                kind: .unknown("future_kind"),
                reasonId: "future.raw.reason",
                incidentId: nil,
                overallOutcome: nil,
                occurredAtUnixMillis: nil
            )]
        )
        let overview = map(status, roster)

        #expect(overview.attention[0].copyKey == "attention.generic")
        #expect(!String(describing: overview.attention[0]).contains("future.raw.reason"))
        _ = roster
    }

    @Test("starting and draining readiness never produce a quiet positive state")
    func transitionalReadinessIsUnknown() async throws {
        let roster = try await loadRoster("roster-current")
        for fixture in ["status-starting", "status-draining"] {
            var status = try await loadStatus(fixture)
            status.latestObservationAtUnixMillis = roster.observedAtUnixMillis
            let overview = map(status, roster)
            #expect(overview.phase == .unknown)
            #expect(overview.tone == .attention)
        }
    }

    @Test("paused status receives a visible paused mode and deadline")
    func pausedMode() async throws {
        var status = try await loadStatus("status-paused")
        let roster = try await loadRoster("roster-current")
        status.latestObservationAtUnixMillis = roster.observedAtUnixMillis

        let overview = map(status, roster)
        #expect(overview.modeKey == "browser.mode.paused")
        #expect(overview.pausedUntil != nil)
    }

    @Test("reclaiming outranks confirmed, verifying, active, and protected")
    func reclaimingPriority() async throws {
        var (status, roster) = try await coherentSnapshot()
        roster.items = sessions(
            from: roster,
            states: [.protected, .active, .cooling, .confirmed, .reclaiming]
        )

        #expect(map(status, roster).phase == .reclaiming)
    }

    @Test("confirmed outranks verifying, active, and protected")
    func confirmedPriority() async throws {
        let (status, roster) = try await coherentSnapshot()
        var ordered = roster
        ordered.items = sessions(from: roster, states: [.protected, .active, .cooling, .confirmed])

        #expect(map(status, ordered).phase == .confirmed)
    }

    @Test("verifying outranks active and protected")
    func verifyingPriority() async throws {
        let (status, roster) = try await coherentSnapshot()
        var ordered = roster
        ordered.items = sessions(from: roster, states: [.protected, .active, .cooling])

        #expect(map(status, ordered).phase == .verifying)
    }

    @Test("active outranks protected")
    func activePriority() async throws {
        let (status, roster) = try await coherentSnapshot()
        var ordered = roster
        ordered.items = sessions(from: roster, states: [.protected, .active])

        #expect(map(status, ordered).phase == .active)
    }

    @Test("confirmed report-only state explicitly says no cleanup occurred")
    func confirmedReportOnlyCopy() async throws {
        var (status, roster) = try await coherentSnapshot()
        status.effectiveMode = .reportOnly
        roster.items = sessions(from: roster, states: [.confirmed])

        let overview = map(status, roster)
        #expect(overview.phase == .confirmed)
        #expect(overview.detailKey == "browser.overview.confirmed.report_only")
        #expect(overview.modeKey == "browser.mode.observe_only")
    }

    @Test("confirmed enforce state does not claim completion before a receipt")
    func confirmedEnforceCopy() async throws {
        var (status, roster) = try await coherentSnapshot()
        status.effectiveMode = .enforce
        roster.items = sessions(from: roster, states: [.confirmed])

        let overview = map(status, roster)
        #expect(overview.phase == .confirmed)
        #expect(overview.detailKey == "browser.overview.confirmed.enforce")
        #expect(overview.detailKey != "browser.settlement.cleared")
    }

    @Test("all current incident states receive browser product copy")
    func rowStateCopy() async throws {
        let (status, roster) = try await coherentSnapshot()
        var allStates = roster
        let states: [IncidentState] = [.active, .cooling, .confirmed, .reclaiming, .protected, .ambiguous]
        allStates.items = sessions(from: roster, states: states)

        let rows = map(status, allStates).sessions
        #expect(rows.map(\.stateKey) == [
            "browser.session.active",
            "browser.session.verifying",
            "browser.session.confirmed",
            "browser.session.reclaiming",
            "browser.session.protected",
            "browser.session.ambiguous"
        ])
    }

    @Test("coverage notices derive only from the seven admitted evidence IDs")
    func coverageEvidenceMapping() async throws {
        let (status, roster) = try await coherentSnapshot()
        let cases: [(String, BrowserCoverageNotice)] = [
            ("protection.browser_product_unsupported", .unsupportedProduct),
            ("protection.browser_version_unsupported", .unsupportedVersion),
            ("protection.browser_version_missing", .versionUnavailable),
            ("protection.browser_version_mixed", .mixedVersions),
            ("protection.controller_version_unverified", .controllerUnverified),
            ("protection.version_observational_only", .observationOnly),
            ("protection.debug_peer_visibility_incomplete", .controlPathIncomplete)
        ]

        for (evidenceID, expected) in cases {
            var one = roster
            one.items = sessions(from: roster, states: [.protected])
            one.items[0].observation.evidence = [Evidence(id: evidenceID, family: .protection)]
            let overview = map(status, one)
            #expect(overview.sessions[0].coverageNotice == expected)
            #expect(overview.coverageNotices == [expected])
        }
    }

    @Test("coverage precedence chooses mixed version before every weaker notice")
    func coveragePrecedence() async throws {
        let (status, roster) = try await coherentSnapshot()
        var one = roster
        one.items = sessions(from: roster, states: [.protected])
        one.items[0].observation.evidence = [
            Evidence(id: "protection.version_observational_only", family: .protection),
            Evidence(id: "protection.browser_version_unsupported", family: .protection),
            Evidence(id: "protection.browser_version_mixed", family: .protection)
        ]

        #expect(map(status, one).sessions[0].coverageNotice == .mixedVersions)
    }

    @Test("protected sessions without admitted coverage evidence use generic safe-retention copy")
    func genericProtectedCopy() async throws {
        let (status, roster) = try await coherentSnapshot()
        var one = roster
        one.items = sessions(from: roster, states: [.protected])
        one.items[0].observation.evidence = [
            Evidence(id: "protection.future_reason", family: .protection)
        ]

        let row = map(status, one).sessions[0]
        #expect(row.coverageNotice == nil)
        #expect(row.reasonKey == "browser.session.reason.protected_generic")
    }

    @Test("recent reclaim joins its exact event token and nearest prior observation")
    func recentSettlementJoin() async throws {
        var status = try await loadStatus("status-recently-reclaimed")
        let roster = try await loadRoster("roster-current")
        var history = try await loadHistory("history-cleared")
        let cleanup = try #require(history.first)
        status.mostRecentReclaim?.eventToken = cleanup.eventToken
        status.mostRecentReclaim?.incidentId = cleanup.incidentId
        status.mostRecentReclaim?.occurredAtUnixMillis = cleanup.occurredAtUnixMillis
        history.append(HistoryEvent(
            eventToken: "prior-observation",
            incidentId: cleanup.incidentId,
            occurredAtUnixMillis: cleanup.occurredAtUnixMillis - 1,
            state: .confirmed,
            payload: .observation(roster.items[0].observation)
        ))

        let settlement = BrowserOverviewMapper.make(
            connection: .live,
            status: status,
            roster: roster,
            history: history
        ).recentSettlement

        #expect(settlement?.eventToken == cleanup.eventToken)
        #expect(settlement?.familyKey == "browser.family.chrome_for_testing")
        #expect(settlement?.processCount == 8)
        #expect(settlement?.estimatedReclaimedMemoryBytes == 93_913_088)
        #expect(settlement?.revivalChecksCompleted == 2)
        #expect(settlement?.artifactOutcome == .reconciled)
        #expect(settlement?.overallOutcome == .cleared)
    }

    @Test("recent reclaim falls back conservatively when its event is outside the loaded page")
    func recentSettlementFallback() async throws {
        let status = try await loadStatus("status-recently-reclaimed")
        let roster = try await loadRoster("roster-current")

        let settlement = BrowserOverviewMapper.make(
            connection: .live,
            status: status,
            roster: roster,
            history: []
        ).recentSettlement

        #expect(settlement?.isFallback == true)
        #expect(settlement?.familyKey == nil)
        #expect(settlement?.processCount == nil)
        #expect(settlement?.estimatedReclaimedMemoryBytes == nil)
        #expect(settlement?.revivalChecksCompleted == nil)
    }

    @Test("same-time observation is not treated as prior settlement evidence")
    func settlementRequiresStrictlyEarlierObservation() async throws {
        let (status, roster, initialHistory) = try await joinedInputs()
        var history = initialHistory
        history[1].occurredAtUnixMillis = history[0].occurredAtUnixMillis

        let settlement = BrowserOverviewMapper.make(
            connection: .live,
            status: status,
            roster: roster,
            history: history
        ).recentSettlement

        #expect(settlement?.isFallback == false)
        #expect(settlement?.familyKey == nil)
        #expect(settlement?.processCount == 8)
    }

    @Test("settlement process count comes from resources.before, never action count")
    func settlementCountUsesResourceSnapshot() async throws {
        let settlement = try await joinedSettlement()
        #expect(settlement.processCount == 8)
        #expect(settlement.processCount != 1)
    }

    @Test("settlement memory is omitted when no estimate exists")
    func settlementMemoryIsOptional() async throws {
        let (status, roster, initialHistory) = try await joinedInputs()
        var history = initialHistory
        guard case .cleanup(var receipt) = history[0].payload else {
            Issue.record("expected cleanup fixture")
            return
        }
        receipt.resources.estimatedReclaimedMemoryBytes = nil
        history[0].payload = .cleanup(receipt)

        let settlement = BrowserOverviewMapper.make(
            connection: .live,
            status: status,
            roster: roster,
            history: history
        ).recentSettlement

        #expect(settlement?.estimatedReclaimedMemoryBytes == nil)
    }

    @Test("settlement revival count comes from typed cleanup data")
    func settlementRevivalCountIsTyped() async throws {
        #expect(try await joinedSettlement().revivalChecksCompleted == 2)
    }

    @Test("unknown state and unknown evidence remain non-authorizing")
    func unknownValuesAreConservative() async throws {
        let (status, roster) = try await coherentSnapshot()
        var one = roster
        one.items = sessions(from: roster, states: [.unknown("future_state")])
        one.items[0].observation.evidence = [
            Evidence(id: "protection.future_reason", family: .unknown("future_family"))
        ]

        let overview = map(status, one)
        #expect(overview.phase == .unknown)
        #expect(overview.sessions[0].stateKey == "browser.session.unknown")
        #expect(overview.sessions[0].coverageNotice == nil)
    }

    @Test("v3 roster rows do not create global current process or memory totals")
    func noGlobalCurrentTotals() async throws {
        let (status, roster) = try await coherentSnapshot()
        let labels = Mirror(reflecting: map(status, roster)).children.compactMap(\.label)

        #expect(!labels.contains("totalProcessCount"))
        #expect(!labels.contains("totalResidentMemoryBytes"))
    }

    @Test("ordinary session presentation excludes raw executable and evidence identifiers")
    func sessionPresentationIsPublicSafe() async throws {
        let (status, roster) = try await coherentSnapshot()
        var one = roster
        one.items[0].observation.evidence = [
            Evidence(id: "protection.browser_version_unsupported", family: .protection)
        ]

        let presentation = String(describing: map(status, one).sessions[0])
        #expect(!presentation.contains("protection.browser_version_unsupported"))
        #expect(!presentation.contains("executableBasename"))
        #expect(!presentation.contains("tracking"))
        #expect(!presentation.contains("fingerprint"))
        #expect(!presentation.contains("generation"))
        #expect(!presentation.contains("epoch"))
    }

    @Test("popover section order starts with overview and keeps actions secondary")
    func popoverSectionOrder() async throws {
        let (status, roster) = try await coherentSnapshot()
        let sections = map(status, roster).visibleSections(connection: .live)

        #expect(sections == [.overview, .sessions, .history, .settings, .actions])
    }

    private func map(_ status: PublicStatus, _ roster: ObservationRoster) -> BrowserOverview {
        BrowserOverviewMapper.make(
            connection: .live,
            status: status,
            roster: roster,
            history: []
        )
    }

    private func coherentSnapshot(
        items: [CurrentIncident]? = nil
    ) async throws -> (PublicStatus, ObservationRoster) {
        var status = try await loadStatus("status-all-clear")
        var roster = try await loadRoster("roster-current")
        if let items { roster.items = items }
        status.latestObservationAtUnixMillis = roster.observedAtUnixMillis
        status.scanInProgress = false
        status.cleanupInProgress = false
        status.attention = AttentionProjection(totalCount: 0, items: [])
        return (status, roster)
    }

    private func sessions(
        from roster: ObservationRoster,
        states: [IncidentState]
    ) -> [CurrentIncident] {
        let source = roster.items[0]
        return states.enumerated().map { index, state in
            var item = source
            item.incidentId = "session-\(index)"
            item.observation.state = state
            return item
        }
    }

    private func joinedInputs() async throws -> (PublicStatus, ObservationRoster, [HistoryEvent]) {
        var status = try await loadStatus("status-recently-reclaimed")
        let roster = try await loadRoster("roster-current")
        var history = try await loadHistory("history-cleared")
        let cleanup = try #require(history.first)
        status.mostRecentReclaim?.eventToken = cleanup.eventToken
        status.mostRecentReclaim?.incidentId = cleanup.incidentId
        status.mostRecentReclaim?.occurredAtUnixMillis = cleanup.occurredAtUnixMillis
        history.append(HistoryEvent(
            eventToken: "prior-observation",
            incidentId: cleanup.incidentId,
            occurredAtUnixMillis: cleanup.occurredAtUnixMillis - 1,
            state: .confirmed,
            payload: .observation(roster.items[0].observation)
        ))
        return (status, roster, history)
    }

    private func joinedSettlement() async throws -> RecentBrowserSettlement {
        let (status, roster, history) = try await joinedInputs()
        return try #require(BrowserOverviewMapper.make(
            connection: .live,
            status: status,
            roster: roster,
            history: history
        ).recentSettlement)
    }

    private func loadStatus(_ fixture: String) async throws -> PublicStatus {
        try await FixtureClient(statusFixture: fixture).status()
    }

    private func loadRoster(_ fixture: String) async throws -> ObservationRoster {
        try await FixtureClient(statusFixture: "status-all-clear", incidentsFixture: fixture).incidents()
    }

    private func loadHistory(_ fixture: String) async throws -> [HistoryEvent] {
        try await FixtureClient(statusFixture: "status-all-clear", historyFixture: fixture).history(limit: 50)
    }
}
