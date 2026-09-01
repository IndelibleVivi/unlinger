import Foundation
import Testing
@testable import UnlingerKit

private actor DelayedProjectionClient: UnlingerClient {
    private var statusCalls = 0
    private var firstContinuation: CheckedContinuation<Void, Never>?
    private var firstDidStart = false

    func firstStarted() -> Bool { firstDidStart }

    func releaseFirst() {
        firstContinuation?.resume()
        firstContinuation = nil
    }

    func status() async throws(ClientError) -> PublicStatus {
        statusCalls += 1
        if statusCalls == 1 {
            firstDidStart = true
            await withCheckedContinuation { continuation in
                firstContinuation = continuation
            }
            return try await FixtureClient(statusFixture: "status-all-clear").status()
        }
        return try await FixtureClient(statusFixture: "status-scanning").status()
    }

    func history(limit _: Int) async throws(ClientError) -> [HistoryEvent] { [] }

    func incidents() async throws(ClientError) -> ObservationRoster {
        ObservationRoster(
            cycleToken: "cycle-current",
            observedAtUnixMillis: 1,
            freshness: .current,
            items: []
        )
    }

    func explain(incidentID _: String) async throws(ClientError) -> IncidentDetail { throw .unavailable }
    func mutationStatus(context _: MutationContext) async throws(ClientError) -> MutationStatus { throw .unavailable }
    func pause(context _: MutationContext, durationMillis _: UInt64) async throws(ClientError) -> MutationReceipt { throw .unavailable }
    func resume(context _: MutationContext) async throws(ClientError) -> MutationReceipt { throw .unavailable }
    func retryFailedCleanup(context _: MutationContext, incidentID _: String) async throws(ClientError) -> MutationReceipt { throw .unavailable }
    func protectIncident(context _: MutationContext, incidentID _: String) async throws(ClientError) -> MutationReceipt { throw .unavailable }
    func unprotectIncident(context _: MutationContext, incidentID _: String) async throws(ClientError) -> MutationReceipt { throw .unavailable }
    func exportDiagnostics(incidentID _: String) async throws(ClientError) -> DiagnosticsExport { throw .unavailable }
}

@Suite("Refresh concurrency")
@MainActor
struct StateConcurrencyTests {
    @Test("a stopped polling session cannot apply after restart")
    func pollingSessionCannotApplyAfterRestart() async throws {
        let client = DelayedProjectionClient()
        let state = AppState(client: client, mutationLedger: TestMutationJournal())

        state.startPolling(interval: .seconds(3_600))
        for _ in 0 ..< 1_000 where !(await client.firstStarted()) {
            await Task.yield()
        }
        #expect(await client.firstStarted())

        state.stopPolling()
        state.startPolling(interval: .seconds(3_600))
        for _ in 0 ..< 1_000 where state.status?.scanInProgress != true {
            await Task.yield()
        }
        #expect(state.status?.scanInProgress == true)

        await client.releaseFirst()
        for _ in 0 ..< 100 { await Task.yield() }
        #expect(state.status?.scanInProgress == true)
        state.stopPolling()
    }
}
