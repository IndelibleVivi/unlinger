import Testing
@testable import UnlingerKit

@Suite("Incident detail error truth")
struct IncidentDetailTests {
    @Test("only trusted not_found maps to not found")
    func exactNotFoundOnly() {
        #expect(IncidentDetailFailure.classify(
            .serverError(code: "not_found", message: "missing")
        ) == .notFound)
        #expect(IncidentDetailFailure.classify(.unavailable) == .unavailable)
        #expect(IncidentDetailFailure.classify(
            .serverError(code: "store_error", message: "local")
        ) == .localHistoryUnavailable)
        #expect(IncidentDetailFailure.classify(
            .protocolError("malformed")
        ) == .protocolFailure)
        #expect(IncidentDetailFailure.classify(
            .incompatibleDaemon("old")
        ) == .incompatible)
    }

    @Test("history detail receives a browser-first summary from its latest observation")
    func browserSummaryFromHistory() async throws {
        let detail = try await FixtureClient(
            statusFixture: "status-report-only",
            incidentFixture: "incident-protected"
        ).explain(incidentID: "redacted-incident-1")

        let summary = try #require(BrowserOverviewMapper.detailPresentation(
            incidentID: detail.incidentId,
            currentSessions: [],
            events: detail.events,
            mode: .reportOnly
        ))

        #expect(summary.familyKey != "browser.family.automation")
        #expect(summary.isPreviousObservation)
        #expect(summary.stateKey.hasPrefix("browser.session."))
    }

    @Test("a coherent current session wins over retained history in detail")
    @MainActor
    func currentSessionWins() async throws {
        let state = AppState(
            client: FixtureClient.scenario("browser-active"),
            mutationLedger: TestMutationJournal()
        )
        await state.refresh()
        let current = try #require(state.browserOverview.sessions.first)

        let summary = try #require(BrowserOverviewMapper.detailPresentation(
            incidentID: current.incidentID,
            currentSessions: state.browserOverview.sessions,
            events: [],
            mode: .reportOnly
        ))

        #expect(summary == current)
        #expect(!summary.isPreviousObservation)
    }

    @Test("retained detail never borrows an observation from another incident")
    func retainedDetailRequiresExactIncident() async throws {
        let detail = try await FixtureClient(
            statusFixture: "status-report-only",
            incidentFixture: "incident-protected"
        ).explain(incidentID: "redacted-incident-1")
        let unrelated = detail.events.map { event in
            HistoryEvent(
                eventToken: event.eventToken,
                incidentId: "different-incident",
                occurredAtUnixMillis: event.occurredAtUnixMillis,
                state: event.state,
                payload: event.payload
            )
        }

        #expect(BrowserOverviewMapper.detailPresentation(
            incidentID: detail.incidentId,
            currentSessions: [],
            events: unrelated,
            mode: .reportOnly
        ) == nil)
    }

    @Test("recent settlement preserves browser context when detail has no observation")
    @MainActor
    func settlementDetailFallback() async throws {
        let state = AppState(
            client: FixtureClient.scenario("browser-recent-settlement"),
            mutationLedger: TestMutationJournal()
        )
        await state.refresh()
        let settlement = try #require(state.browserOverview.recentSettlement)
        let detail = try await state.explain(incidentID: settlement.incidentID)

        #expect(BrowserOverviewMapper.detailPresentation(
            incidentID: settlement.incidentID,
            currentSessions: state.browserOverview.sessions,
            events: detail.events,
            mode: state.status?.effectiveMode
        ) == nil)
        #expect(settlement.familyKey == "browser.family.chrome_for_testing")
        #expect(settlement.processCount == 8)
        #expect(!settlement.isFallback)
    }
}
