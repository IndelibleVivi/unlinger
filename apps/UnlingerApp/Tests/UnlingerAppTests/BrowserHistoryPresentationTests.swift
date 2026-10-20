import Foundation
import Testing
@testable import UnlingerKit

@Suite("Browser history presentation")
struct BrowserHistoryPresentationTests {
    @Test("cleanup outcomes collapse into one browser entry with current product context")
    func groupsCleanupOutcomesByIncident() async throws {
        let client = BrowserFixtureClient(scenario: .protectedUnsupported)
        let snapshot = try await client.browserOverview()
        let overview = BrowserOverviewMapper.make(connection: .live, snapshot: snapshot)
        let current = try #require(overview.sessions.first)
        let history = try await FixtureClient(
            statusFixture: "status-recently-reclaimed",
            historyFixture: "history-cleared"
        ).history(limit: 10)
        let source = try #require(history.first)
        let earlier = HistoryEvent(
            eventToken: "history-protected-earlier",
            incidentId: current.incidentID,
            occurredAtUnixMillis: source.occurredAtUnixMillis,
            state: source.state,
            payload: source.payload
        )
        let later = HistoryEvent(
            eventToken: "history-protected-later",
            incidentId: current.incidentID,
            occurredAtUnixMillis: source.occurredAtUnixMillis + 60_000,
            state: source.state,
            payload: source.payload
        )

        let entries = BrowserHistoryMapper.entries(
            events: [earlier, later],
            currentSessions: overview.sessions,
            recentSettlement: overview.recentSettlement,
            mode: .reportOnly
        )
        let entry = try #require(entries.first)

        #expect(entries.count == 1)
        #expect(entry.incidentID == current.incidentID)
        #expect(entry.eventCount == 2)
        #expect(entry.isCurrent)
        #expect(entry.familyKey == current.familyKey)
        #expect(entry.familyKey != "browser.family.automation")
        #expect(entry.productKey == "browser.product.chrome_for_testing")
        #expect(entry.observedVersion == "151.0.7922.35")
        #expect(entry.state == .protected)
        #expect(entry.reasonKey == "browser.coverage.unsupported_version")
        #expect(entry.memberCount == current.memberCount)
    }

    @Test("repeated observations collapse while cleanup remains a distinct timeline phase")
    func groupsRepeatedTimelineObservations() async throws {
        let protectedDetail = try await FixtureClient(
            statusFixture: "status-report-only",
            incidentFixture: "incident-protected"
        ).explain(incidentID: "redacted-incident-1")
        let cleanupHistory = try await FixtureClient(
            statusFixture: "status-recently-reclaimed",
            historyFixture: "history-cleared"
        ).history(limit: 10)
        let sourceObservation = try #require(protectedDetail.events.first)
        let sourceCleanup = try #require(cleanupHistory.first)
        let first = HistoryEvent(
            eventToken: "timeline-observation-1",
            incidentId: sourceObservation.incidentId,
            occurredAtUnixMillis: 1_000,
            state: sourceObservation.state,
            payload: sourceObservation.payload
        )
        let second = HistoryEvent(
            eventToken: "timeline-observation-2",
            incidentId: sourceObservation.incidentId,
            occurredAtUnixMillis: 2_000,
            state: sourceObservation.state,
            payload: sourceObservation.payload
        )
        let cleanup = HistoryEvent(
            eventToken: "timeline-cleanup-1",
            incidentId: sourceObservation.incidentId,
            occurredAtUnixMillis: 3_000,
            state: sourceCleanup.state,
            payload: sourceCleanup.payload
        )

        let timeline = BrowserHistoryMapper.timelineEntries(
            events: [second, cleanup, first]
        )

        #expect(timeline.count == 2)
        #expect(timeline[0].eventCount == 2)
        #expect(timeline[0].latestEvent.eventToken == "timeline-observation-2")
        #expect(timeline[1].eventCount == 1)
        #expect(timeline[1].latestEvent.eventToken == "timeline-cleanup-1")
    }

    @Test("terminal rows use the last observation at or before cleanup", arguments: [UInt64(0), 1_000])
    func terminalRowUsesObservationAtCleanupTime(afterGap: UInt64) async throws {
        let detail = try await FixtureClient(
            statusFixture: "status-report-only",
            incidentFixture: "incident-protected"
        ).explain(incidentID: "redacted-incident-1")
        let history = try await FixtureClient(
            statusFixture: "status-recently-reclaimed",
            historyFixture: "history-cleared"
        ).history(limit: 10)
        var before = try #require(detail.events.first)
        var cleanup = try #require(history.first)
        guard case .observation(var beforeObservation) = before.payload else {
            Issue.record("expected observation fixture")
            return
        }
        before.incidentId = cleanup.incidentId
        before.occurredAtUnixMillis = cleanup.occurredAtUnixMillis - 1_000
        beforeObservation.family = "playwright"
        beforeObservation.memberCount = 7
        beforeObservation.residentMemoryBytes = 73_400_320
        before.payload = .observation(beforeObservation)

        var after = before
        guard case .observation(var afterObservation) = after.payload else {
            Issue.record("expected observation fixture")
            return
        }
        after.eventToken = "observation-after-cleanup"
        after.occurredAtUnixMillis = cleanup.occurredAtUnixMillis + afterGap
        afterObservation.family = "puppeteer"
        afterObservation.memberCount = 99
        afterObservation.residentMemoryBytes = 999_000_000
        after.payload = .observation(afterObservation)
        cleanup.eventToken = "terminal-cleanup"

        let entries = BrowserHistoryMapper.entries(
            events: [after, cleanup, before],
            currentSessions: [],
            recentSettlement: nil,
            mode: .enforce
        )
        let entry = try #require(entries.first)

        #expect(entries.count == 1)
        #expect(entry.familyKey == "browser.family.playwright")
        #expect(entry.memberCount == 7)
        #expect(entry.residentMemoryBytes == 73_400_320)
        #expect(entry.state == .cleared)
        #expect(entry.latestAt == Date(unixMillis: cleanup.occurredAtUnixMillis))
        #expect(entry.eventCount == 1)
    }

    @Test("observations without terminal cleanup do not create history rows")
    func observationOnlyIncidentHasNoHistoryRow() async throws {
        let detail = try await FixtureClient(
            statusFixture: "status-report-only",
            incidentFixture: "incident-protected"
        ).explain(incidentID: "redacted-incident-1")

        let entries = BrowserHistoryMapper.entries(
            events: detail.events,
            currentSessions: [],
            recentSettlement: nil,
            mode: .reportOnly
        )

        #expect(entries.isEmpty)
    }

    @Test("recent settlement restores a pruned terminal history row")
    func recentSettlementRestoresPrunedHistoryRow() async throws {
        let snapshot = try await BrowserFixtureClient(
            scenario: .recentSettlement
        ).browserOverview()
        let settlement = try #require(snapshot.recentSettlement)
        let detail = try await FixtureClient(
            statusFixture: "status-report-only",
            incidentFixture: "incident-protected"
        ).explain(incidentID: settlement.incidentId)
        let observation = try #require(detail.events.first)

        let entry = try #require(
            BrowserHistoryMapper.entries(
                events: [observation],
                currentSessions: [],
                recentSettlement: BrowserOverviewMapper.make(
                    connection: .live,
                    snapshot: snapshot
                ).recentSettlement,
                mode: .enforce
            ).first
        )

        #expect(entry.incidentID == settlement.incidentId)
        #expect(entry.familyKey == "browser.family.chrome_for_testing")
        #expect(entry.productKey == nil)
        #expect(entry.observedVersion == nil)
        #expect(entry.state == .cleared)
        #expect(entry.stateKey == "browser.session.cleared")
        #expect(entry.reasonKey == "browser.settlement.cleared")
        #expect(entry.memberCount == nil)
        #expect(entry.residentMemoryBytes == nil)
        #expect(entry.eventCount == 1) // The exact settlement is one known terminal result.
        #expect(entry.latestAt == Date(unixMillis: settlement.occurredAtUnixMillis))
        #expect(!entry.isCurrent)
    }

    @Test("settlement restoration excludes unrelated observation-only incidents")
    func settlementRestoresOnlyItsExactIncident() async throws {
        let snapshot = try await BrowserFixtureClient(
            scenario: .recentSettlement
        ).browserOverview()
        let settlement = try #require(snapshot.recentSettlement)
        let detail = try await FixtureClient(
            statusFixture: "status-report-only",
            incidentFixture: "incident-protected"
        ).explain(incidentID: "redacted-incident-1")
        let observation = try #require(detail.events.first)

        let entries = BrowserHistoryMapper.entries(
            events: [observation],
            currentSessions: [],
            recentSettlement: BrowserOverviewMapper.make(
                connection: .live,
                snapshot: snapshot
            ).recentSettlement,
            mode: .reportOnly
        )

        #expect(entries.map(\.incidentID) == [settlement.incidentId])
    }

    @Test("a matching terminal event is not duplicated by recent settlement")
    func matchingTerminalEventWins() async throws {
        let history = try await FixtureClient(
            statusFixture: "status-recently-reclaimed",
            historyFixture: "history-cleared"
        ).history(limit: 10)
        let terminal = try #require(history.first)
        guard case .cleanup(let receipt) = terminal.payload else {
            Issue.record("expected cleanup fixture")
            return
        }
        let settlement = RecentBrowserSettlement(
            incidentID: terminal.incidentId,
            eventToken: terminal.eventToken,
            familyKey: "browser.family.playwright",
            occurredAt: Date(unixMillis: terminal.occurredAtUnixMillis),
            processCount: receipt.resources.before?.processCount,
            estimatedReclaimedMemoryBytes:
                receipt.resources.estimatedReclaimedMemoryBytes,
            revivalChecksCompleted: receipt.revivalChecksCompleted,
            artifactOutcome: receipt.artifactOutcome,
            overallOutcome: receipt.overallOutcome,
            isFallback: false
        )
        let protectedDetail = try await FixtureClient(
            statusFixture: "status-report-only",
            incidentFixture: "incident-protected"
        ).explain(incidentID: "redacted-incident-1")
        let observation = try #require(protectedDetail.events.first)
        let later = HistoryEvent(
            eventToken: "later-observation",
            incidentId: terminal.incidentId,
            occurredAtUnixMillis: terminal.occurredAtUnixMillis + 1,
            state: observation.state,
            payload: observation.payload
        )

        let entries = BrowserHistoryMapper.entries(
            events: [later, terminal],
            currentSessions: [],
            recentSettlement: settlement,
            mode: .enforce
        )
        let entry = try #require(entries.first)

        #expect(entries.count == 1)
        #expect(entry.incidentID == terminal.incidentId)
        #expect(entry.eventCount == 1)
        #expect(entry.memberCount == 8)
        #expect(entry.residentMemoryBytes == 93_913_088)
        #expect(entry.latestAt == Date(unixMillis: terminal.occurredAtUnixMillis))
    }

    @Test("a matching cleared settlement keeps the retained terminal reason")
    func matchingClearedSettlementKeepsTerminalReason() async throws {
        let history = try await FixtureClient(
            statusFixture: "status-recently-reclaimed",
            historyFixture: "history-cleared"
        ).history(limit: 10)
        let terminal = try #require(history.first)
        guard case .cleanup(let receipt) = terminal.payload else {
            Issue.record("expected cleanup fixture")
            return
        }
        let settlement = RecentBrowserSettlement(
            incidentID: terminal.incidentId,
            eventToken: terminal.eventToken,
            familyKey: "browser.family.playwright",
            occurredAt: Date(unixMillis: terminal.occurredAtUnixMillis),
            processCount: nil,
            estimatedReclaimedMemoryBytes: nil,
            revivalChecksCompleted: receipt.revivalChecksCompleted,
            artifactOutcome: receipt.artifactOutcome,
            overallOutcome: receipt.overallOutcome,
            isFallback: false
        )
        let entry = try #require(
            BrowserHistoryMapper.entries(
                events: [terminal],
                currentSessions: [],
                recentSettlement: settlement,
                mode: .enforce
            ).first
        )
        let cleanupWithoutMatchingSettlement = try #require(
            BrowserHistoryMapper.entries(
                events: [terminal],
                currentSessions: [],
                recentSettlement: nil,
                mode: .enforce
            ).first
        )

        #expect(entry.state == .cleared)
        #expect(entry.reasonKey == cleanupWithoutMatchingSettlement.reasonKey)
    }

    @Test("same-incident terminals choose the latest terminal")
    func latestTerminalWins() async throws {
        let history = try await FixtureClient(
            statusFixture: "status-recently-reclaimed",
            historyFixture: "history-cleared"
        ).history(limit: 10)
        let terminal = try #require(history.first)
        var later = terminal
        later.eventToken = "later-terminal"
        later.occurredAtUnixMillis += 1
        later.state = .failed
        guard case .cleanup(var receipt) = later.payload else {
            Issue.record("expected cleanup fixture")
            return
        }
        receipt.overallOutcome = .failed
        receipt.processOutcome = .failed
        receipt.resources.before = nil
        receipt.resources.estimatedReclaimedMemoryBytes = nil
        later.payload = .cleanup(receipt)

        let entry = try #require(
            BrowserHistoryMapper.entries(
                events: [terminal, later],
                currentSessions: [],
                recentSettlement: nil,
                mode: .enforce
            ).first
        )

        #expect(entry.eventCount == 2)
        #expect(entry.state == .failed)
        #expect(entry.reasonKey == "browser.session.reason.failed")
        #expect(entry.latestAt == Date(unixMillis: later.occurredAtUnixMillis))
    }

    @Test("history array order, not same-millisecond token order, picks the latest terminal")
    func historyArrayOrderBreaksSameMillisecondTies() async throws {
        let history = try await FixtureClient(
            statusFixture: "status-recently-reclaimed",
            historyFixture: "history-cleared"
        ).history(limit: 10)
        let source = try #require(history.first)
        var older = source
        older.eventToken = "00000000000000000000000000000000"
        older.occurredAtUnixMillis = 2_000
        older.state = .cleared
        guard case .cleanup(var olderReceipt) = older.payload else {
            Issue.record("expected cleanup fixture")
            return
        }
        olderReceipt.state = .cleared
        olderReceipt.overallOutcome = .cleared
        olderReceipt.processOutcome = .cleared
        older.payload = .cleanup(olderReceipt)

        var newer = older
        newer.eventToken = "ffffffffffffffffffffffffffffffff"
        newer.state = .failed
        guard case .cleanup(var newerReceipt) = newer.payload else {
            Issue.record("expected cleanup fixture")
            return
        }
        newerReceipt.state = .failed
        newerReceipt.overallOutcome = .failed
        newerReceipt.processOutcome = .failed
        newer.payload = .cleanup(newerReceipt)

        let newestFirst = try #require(
            BrowserHistoryMapper.entries(
                events: [newer, older],
                currentSessions: [],
                recentSettlement: nil,
                mode: .enforce
            ).first
        )
        let oldestFirst = try #require(
            BrowserHistoryMapper.entries(
                events: [older, newer],
                currentSessions: [],
                recentSettlement: nil,
                mode: .enforce
            ).first
        )

        #expect(newestFirst.state == .failed)
        #expect(oldestFirst.state == .cleared)
        #expect(newestFirst.eventCount == 2)
        #expect(oldestFirst.eventCount == 2)
    }

    @Test("an equal-time retained terminal wins over an unorderable settlement token")
    func equalTimeSettlementDoesNotGuessFromToken() async throws {
        let history = try await FixtureClient(
            statusFixture: "status-recently-reclaimed",
            historyFixture: "history-cleared"
        ).history(limit: 10)
        var terminal = try #require(history.first)
        terminal.eventToken = "ffffffffffffffffffffffffffffffff"
        let settlement = RecentBrowserSettlement(
            incidentID: terminal.incidentId,
            eventToken: "00000000000000000000000000000000",
            familyKey: "browser.family.playwright",
            occurredAt: Date(unixMillis: terminal.occurredAtUnixMillis),
            processCount: 3,
            estimatedReclaimedMemoryBytes: 300,
            revivalChecksCompleted: 2,
            artifactOutcome: .reconciled,
            overallOutcome: .revived,
            isFallback: false
        )

        let entry = try #require(
            BrowserHistoryMapper.entries(
                events: [terminal],
                currentSessions: [],
                recentSettlement: settlement,
                mode: .enforce
            ).first
        )

        #expect(entry.state == terminal.state)
        #expect(entry.latestAt == Date(unixMillis: terminal.occurredAtUnixMillis))
        #expect(entry.eventCount == 1)
    }

    @Test("settlement outcomes remain truthful without invented active metrics")
    func settlementOutcomesRemainTruthful() throws {
        let outcomes: [(OverallOutcome, IncidentState, String, String)] = [
            (
                .clearedWithResidue,
                .failed,
                "browser.session.failed",
                "browser.settlement.residue"
            ),
            (
                .revived,
                .revived,
                "browser.session.revived",
                "browser.settlement.revived"
            ),
            (
                .failed,
                .failed,
                "browser.session.failed",
                "browser.settlement.failed"
            ),
            (
                .unknown("future_outcome"),
                .unknown("settlement_outcome_unknown"),
                "browser.session.unknown",
                "browser.settlement.unknown"
            ),
        ]

        for (index, expected) in outcomes.enumerated() {
            let settlement = RecentBrowserSettlement(
                incidentID: "settlement-incident-\(index)",
                eventToken: "settlement-event-\(index)",
                familyKey: "browser.family.playwright",
                occurredAt: Date(unixMillis: UInt64(1_000 + index)),
                processCount: expected.0 == .clearedWithResidue ? 8 : nil,
                estimatedReclaimedMemoryBytes: expected.0 == .clearedWithResidue
                    ? 100
                    : nil,
                revivalChecksCompleted: 2,
                artifactOutcome: expected.0 == .clearedWithResidue
                    ? .residue
                    : .notApplicable,
                overallOutcome: expected.0,
                isFallback: false
            )
            let entry = try #require(
                BrowserHistoryMapper.entries(
                    events: [],
                    currentSessions: [],
                    recentSettlement: settlement,
                    mode: .enforce
                ).first
            )

            #expect(entry.state == expected.1)
            #expect(entry.stateKey == expected.2)
            #expect(entry.reasonKey == expected.3)
            #expect(entry.memberCount == nil)
            #expect(entry.residentMemoryBytes == nil)
            #expect(entry.eventCount == 1) // The exact settlement is one known terminal result.
        }
    }

    @Test("a pruned residue settlement remains an attention state, not a success")
    func residueSettlementIsNotPresentedAsCleared() throws {
        let settlement = RecentBrowserSettlement(
            incidentID: "residue-incident",
            eventToken: "residue-event",
            familyKey: "browser.family.playwright",
            occurredAt: Date(unixMillis: 2_000),
            processCount: 8,
            estimatedReclaimedMemoryBytes: 100,
            revivalChecksCompleted: 2,
            artifactOutcome: .residue,
            overallOutcome: .clearedWithResidue,
            isFallback: false
        )
        let entry = try #require(
            BrowserHistoryMapper.entries(
                events: [],
                currentSessions: [],
                recentSettlement: settlement,
                mode: .enforce
            ).first
        )

        #expect(entry.state == .failed)
        #expect(entry.stateKey == "browser.session.failed")
        #expect(entry.reasonKey == "browser.settlement.residue")
        #expect(entry.memberCount == nil)
        #expect(entry.residentMemoryBytes == nil)
        #expect(entry.eventCount == 1) // The exact settlement is one known terminal result.
    }

    @Test("a retained residue terminal carries the receipt outcome and residue reason")
    func retainedResidueTerminalUsesSettlementOutcome() async throws {
        let history = try await FixtureClient(
            statusFixture: "status-needs-attention",
            historyFixture: "history-cleared-with-residue"
        ).history(limit: 10)
        let terminal = try #require(history.first)
        guard case .cleanup(let receipt) = terminal.payload else {
            Issue.record("expected cleanup fixture")
            return
        }
        let settlement = RecentBrowserSettlement(
            incidentID: terminal.incidentId,
            eventToken: terminal.eventToken,
            familyKey: "browser.family.playwright",
            occurredAt: Date(unixMillis: terminal.occurredAtUnixMillis),
            processCount: receipt.resources.before?.processCount,
            estimatedReclaimedMemoryBytes:
                receipt.resources.estimatedReclaimedMemoryBytes,
            revivalChecksCompleted: receipt.revivalChecksCompleted,
            artifactOutcome: receipt.artifactOutcome,
            overallOutcome: receipt.overallOutcome,
            isFallback: false
        )

        let entry = try #require(
            BrowserHistoryMapper.entries(
                events: [terminal],
                currentSessions: [],
                recentSettlement: settlement,
                mode: .enforce
            ).first
        )

        #expect(entry.state == .failed)
        #expect(entry.stateKey == "browser.session.failed")
        #expect(entry.reasonKey == "browser.settlement.residue")
        #expect(entry.latestAt == Date(unixMillis: terminal.occurredAtUnixMillis))
        #expect(entry.eventCount == 1)
    }

    @Test("a newer settlement must not be hidden by an older retained terminal")
    func newerSettlementWinsOverOlderTerminal() async throws {
        let history = try await FixtureClient(
            statusFixture: "status-recently-reclaimed",
            historyFixture: "history-cleared"
        ).history(limit: 10)
        let olderTerminal = try #require(history.first)
        let newerSettlement = RecentBrowserSettlement(
            incidentID: olderTerminal.incidentId,
            eventToken: "newer-pruned-terminal",
            familyKey: "browser.family.playwright",
            occurredAt: Date(unixMillis: olderTerminal.occurredAtUnixMillis + 1),
            processCount: 3,
            estimatedReclaimedMemoryBytes: 300,
            revivalChecksCompleted: 2,
            artifactOutcome: .reconciled,
            overallOutcome: .revived,
            isFallback: false
        )

        let entries = BrowserHistoryMapper.entries(
            events: [olderTerminal],
            currentSessions: [],
            recentSettlement: newerSettlement,
            mode: .enforce
        )
        let entry = try #require(entries.first)

        #expect(entries.count == 1)
        #expect(entry.incidentID == olderTerminal.incidentId)
        #expect(entry.state == .revived)
        #expect(entry.reasonKey == "browser.settlement.revived")
        #expect(entry.memberCount == nil)
        #expect(entry.residentMemoryBytes == nil)
        #expect(entry.eventCount == 2)
        #expect(entry.latestAt == newerSettlement.occurredAt)
    }

    @Test("an older settlement must not replace a newer retained terminal")
    func newerTerminalWinsOverOlderSettlement() async throws {
        let history = try await FixtureClient(
            statusFixture: "status-recently-reclaimed",
            historyFixture: "history-cleared"
        ).history(limit: 10)
        let terminal = try #require(history.first)
        let olderSettlement = RecentBrowserSettlement(
            incidentID: terminal.incidentId,
            eventToken: "older-pruned-terminal",
            familyKey: "browser.family.playwright",
            occurredAt: Date(unixMillis: terminal.occurredAtUnixMillis - 1_000),
            processCount: 3,
            estimatedReclaimedMemoryBytes: 300,
            revivalChecksCompleted: 2,
            artifactOutcome: .reconciled,
            overallOutcome: .revived,
            isFallback: false
        )

        let entries = BrowserHistoryMapper.entries(
            events: [terminal],
            currentSessions: [],
            recentSettlement: olderSettlement,
            mode: .enforce
        )
        let entry = try #require(entries.first)

        #expect(entries.count == 1)
        #expect(entry.incidentID == terminal.incidentId)
        #expect(entry.state == .cleared)
        #expect(entry.memberCount == 8)
        #expect(entry.residentMemoryBytes == 93_913_088)
        #expect(entry.eventCount == 1)
        #expect(entry.latestAt == Date(unixMillis: terminal.occurredAtUnixMillis))
    }

    @Test("settlement fallback does not outrank a later known terminal")
    func laterKnownTerminalKeepsOrdering() async throws {
        let snapshot = try await BrowserFixtureClient(
            scenario: .recentSettlement
        ).browserOverview()
        let pruned = try #require(snapshot.recentSettlement)
        let history = try await FixtureClient(
            statusFixture: "status-recently-reclaimed",
            historyFixture: "history-cleared"
        ).history(limit: 10)
        var later = try #require(history.first)
        later.eventToken = "later-terminal"
        later.incidentId = "later-incident"
        later.occurredAtUnixMillis = pruned.occurredAtUnixMillis + 1

        let entries = BrowserHistoryMapper.entries(
            events: [later],
            currentSessions: [],
            recentSettlement: BrowserOverviewMapper.make(
                connection: .live,
                snapshot: snapshot
            ).recentSettlement,
            mode: .enforce
        )

        #expect(entries.map(\.incidentID) == ["later-incident", pruned.incidentId])
    }

    @Test("current protection context wins for a settlement incident")
    func currentProtectionWinsOverSettlementFallback() async throws {
        let snapshot = try await BrowserFixtureClient(
            scenario: .protectedUnsupported
        ).browserOverview()
        let overview = BrowserOverviewMapper.make(connection: .live, snapshot: snapshot)
        let current = try #require(overview.sessions.first)
        let recent = RecentBrowserSettlement(
            incidentID: current.incidentID,
            eventToken: "prior-settlement",
            familyKey: "browser.family.playwright",
            occurredAt: Date(unixMillis: 1_000),
            processCount: 8,
            estimatedReclaimedMemoryBytes: 100,
            revivalChecksCompleted: 2,
            artifactOutcome: .reconciled,
            overallOutcome: .cleared,
            isFallback: false
        )

        let entry = try #require(
            BrowserHistoryMapper.entries(
                events: [],
                currentSessions: overview.sessions,
                recentSettlement: recent,
                mode: .reportOnly
            ).first
        )

        #expect(entry.isCurrent)
        #expect(entry.familyKey == current.familyKey)
        #expect(entry.productKey == current.productKey)
        #expect(entry.observedVersion == current.observedVersion)
        #expect(entry.state == current.state)
        #expect(entry.stateKey == current.stateKey)
        #expect(entry.reasonKey == current.reasonKey)
        #expect(entry.memberCount == current.memberCount)
    }

    @Test("an explicit no-intervention receipt remains visible without a reclaim label")
    @MainActor
    func endedWithoutInterventionIsNotAReclaim() async throws {
        let history = try await FixtureClient(
            statusFixture: "status-recently-reclaimed",
            historyFixture: "history-cleared"
        ).history(limit: 10)
        var event = try #require(history.first)
        guard case .cleanup(var receipt) = event.payload else {
            Issue.record("expected cleanup fixture")
            return
        }
        receipt.reasonId = "cleanup.tree_gone_without_signal"
        receipt.artifactOutcome = .notApplicable
        receipt.artifactActions = []
        receipt.processActions = []
        receipt.resources.estimatedReclaimedMemoryBytes = nil
        receipt.processActions = []
        event.payload = .cleanup(receipt)
        let entries = BrowserHistoryMapper.entries(
            events: [event], currentSessions: [], recentSettlement: nil, mode: .enforce
        )
        let entry = try #require(entries.first)
        #expect(entry.state == .cleared)
        #expect(entry.stateKey == "browser.session.ended_without_intervention")
        #expect(entry.reasonKey == "detail.cleanup.without_intervention")
        #expect(OutcomeCopy.label(for: event) == L10n.text("outcome.ended_without_intervention"))

        // An old/unknown reason is not interpreted from an empty action array.
        receipt.reasonId = "cleanup.tree_gone_no_revival"
        #expect(!receipt.endedWithoutIntervention)
        receipt.reasonId = "cleanup.tree_gone_without_signal"
        receipt.artifactOutcome = .notApplicable
        receipt.artifactActions = []
        receipt.processActions = []
        receipt.resources.estimatedReclaimedMemoryBytes = nil
        receipt.processOutcome = .failed
        #expect(!receipt.endedWithoutIntervention)
    }

    @Test("current protection still wins over a prior no-intervention receipt")
    func currentStateWinsOverNoIntervention() async throws {
        let snapshot = try await BrowserFixtureClient(scenario: .protectedUnsupported).browserOverview()
        let overview = BrowserOverviewMapper.make(connection: .live, snapshot: snapshot)
        let current = try #require(overview.sessions.first)
        let history = try await FixtureClient(
            statusFixture: "status-recently-reclaimed", historyFixture: "history-cleared"
        ).history(limit: 10)
        var event = try #require(history.first)
        event.incidentId = current.incidentID
        guard case .cleanup(var receipt) = event.payload else {
            Issue.record("expected cleanup fixture")
            return
        }
        receipt.reasonId = "cleanup.tree_gone_without_signal"
        receipt.artifactOutcome = .notApplicable
        receipt.artifactActions = []
        receipt.processActions = []
        receipt.resources.estimatedReclaimedMemoryBytes = nil
        event.payload = .cleanup(receipt)
        let entries = BrowserHistoryMapper.entries(
            events: [event], currentSessions: overview.sessions,
            recentSettlement: nil, mode: .reportOnly
        )
        let entry = try #require(entries.first)
        #expect(entry.stateKey == current.stateKey)
        #expect(entry.reasonKey == current.reasonKey)
        #expect(entry.isCurrent)
    }

}
