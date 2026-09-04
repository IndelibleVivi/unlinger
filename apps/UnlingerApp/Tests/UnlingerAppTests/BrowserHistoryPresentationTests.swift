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
}
