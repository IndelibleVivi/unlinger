import Testing
@testable import UnlingerKit

@Suite("Browser fixture scenarios")
@MainActor
struct BrowserFixtureScenarioTests {
    @Test(
        "every browser fixture scenario reaches its intended product phase",
        arguments: [
            ("browser-clear", BrowserOverviewPhase.clear),
            ("browser-active", BrowserOverviewPhase.active),
            ("browser-verifying", BrowserOverviewPhase.verifying),
            ("browser-confirmed-report-only", BrowserOverviewPhase.confirmed),
            ("browser-reclaiming", BrowserOverviewPhase.reclaiming),
            ("browser-protected-unsupported", BrowserOverviewPhase.protected),
            ("browser-attention", BrowserOverviewPhase.attention)
        ]
    )
    func phaseScenario(name: String, expected: BrowserOverviewPhase) async {
        let state = AppState(
            client: FixtureClient.scenario(name),
            mutationLedger: TestMutationJournal()
        )

        await state.refresh()

        #expect(state.browserOverview.phase == expected)
    }

    @Test("recent settlement scenario exposes the atomic typed summary")
    func recentSettlementScenario() async throws {
        let state = AppState(
            client: FixtureClient.scenario("browser-recent-settlement"),
            mutationLedger: TestMutationJournal()
        )

        await state.refresh()

        let settlement = try #require(state.browserOverview.recentSettlement)
        #expect(!settlement.isFallback)
        #expect(settlement.familyKey == "browser.family.chrome_for_testing")
        #expect(settlement.processCount == 8)
        #expect(settlement.revivalChecksCompleted == 2)
    }

    @Test("unsupported scenario exposes typed coverage and no raw evidence")
    func unsupportedScenario() async throws {
        let state = AppState(
            client: FixtureClient.scenario("browser-protected-unsupported"),
            mutationLedger: TestMutationJournal()
        )

        await state.refresh()

        let row = try #require(state.browserOverview.sessions.first)
        #expect(row.coverageNotice == .unsupportedVersion)
        #expect(!String(describing: row).contains("protection.browser_version_unsupported"))
    }
}
