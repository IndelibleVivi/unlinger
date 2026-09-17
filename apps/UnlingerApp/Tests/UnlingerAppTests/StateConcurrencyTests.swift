import Foundation
import Observation
import Testing
@testable import UnlingerKit

private final class ObservationFlag: @unchecked Sendable {
    private let lock = NSLock()
    private var value = false

    func mark() {
        lock.lock()
        value = true
        lock.unlock()
    }

    func read() -> Bool {
        lock.lock()
        defer { lock.unlock() }
        return value
    }
}

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

    func browserOverview() async throws(ClientError) -> BrowserOverviewSnapshot {
        try await BrowserFixtureClient(scenario: .clear).browserOverview()
    }

    func history(limit _: Int) async throws(ClientError) -> [HistoryEvent] { [] }
    func incidents() async throws(ClientError) -> ObservationRoster { throw .unavailable }
    func explain(incidentID _: String) async throws(ClientError) -> IncidentDetail { throw .unavailable }
    func mutationStatus(context _: MutationContext) async throws(ClientError) -> MutationStatus { throw .unavailable }
    func pause(context _: MutationContext, durationMillis _: UInt64) async throws(ClientError) -> MutationReceipt { throw .unavailable }
    func resume(context _: MutationContext) async throws(ClientError) -> MutationReceipt { throw .unavailable }
    func retryFailedCleanup(context _: MutationContext, incidentID _: String) async throws(ClientError) -> MutationReceipt { throw .unavailable }
    func protectIncident(context _: MutationContext, incidentID _: String) async throws(ClientError) -> MutationReceipt { throw .unavailable }
    func unprotectIncident(context _: MutationContext, incidentID _: String) async throws(ClientError) -> MutationReceipt { throw .unavailable }
    func exportDiagnostics(incidentID _: String) async throws(ClientError) -> DiagnosticsExport { throw .unavailable }
}

private actor CountingProjectionClient: UnlingerClient {
    private(set) var overviewCalls = 0

    func status() async throws(ClientError) -> PublicStatus {
        try await FixtureClient(statusFixture: "status-all-clear").status()
    }

    func browserOverview() async throws(ClientError) -> BrowserOverviewSnapshot {
        overviewCalls += 1
        return try await BrowserFixtureClient(scenario: .clear).browserOverview()
    }

    func history(limit _: Int) async throws(ClientError) -> [HistoryEvent] { [] }
    func incidents() async throws(ClientError) -> ObservationRoster { throw .unavailable }
    func explain(incidentID _: String) async throws(ClientError) -> IncidentDetail { throw .unavailable }
    func mutationStatus(context _: MutationContext) async throws(ClientError) -> MutationStatus { throw .unavailable }
    func pause(context _: MutationContext, durationMillis _: UInt64) async throws(ClientError) -> MutationReceipt { throw .unavailable }
    func resume(context _: MutationContext) async throws(ClientError) -> MutationReceipt { throw .unavailable }
    func retryFailedCleanup(context _: MutationContext, incidentID _: String) async throws(ClientError) -> MutationReceipt { throw .unavailable }
    func protectIncident(context _: MutationContext, incidentID _: String) async throws(ClientError) -> MutationReceipt { throw .unavailable }
    func unprotectIncident(context _: MutationContext, incidentID _: String) async throws(ClientError) -> MutationReceipt { throw .unavailable }
    func exportDiagnostics(incidentID _: String) async throws(ClientError) -> DiagnosticsExport { throw .unavailable }
}

private actor SnapshotSequenceClient: UnlingerClient {
    private let snapshots: [BrowserOverviewSnapshot]
    private var nextSnapshot = 0

    init(snapshots: [BrowserOverviewSnapshot]) {
        self.snapshots = snapshots
    }

    func status() async throws(ClientError) -> PublicStatus {
        try await FixtureClient(statusFixture: "status-all-clear").status()
    }

    func browserOverview() async throws(ClientError) -> BrowserOverviewSnapshot {
        let index = min(nextSnapshot, snapshots.count - 1)
        nextSnapshot += 1
        return snapshots[index]
    }

    func history(limit _: Int) async throws(ClientError) -> [HistoryEvent] { [] }
    func incidents() async throws(ClientError) -> ObservationRoster { throw .unavailable }
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
    @Test("one refresh consumes one canonical browser snapshot")
    func refreshUsesOneOverviewRequest() async {
        let client = CountingProjectionClient()
        let state = AppState(client: client, mutationLedger: TestMutationJournal())

        await state.refresh()

        #expect(await client.overviewCalls == 1)
        #expect(state.browserOverview.phase == .clear)
        #expect(!state.browserOverview.requiresTrailingRefresh)
    }

    @Test("equal refreshes do not republish observable source state")
    func equalRefreshDoesNotRepublishSourceState() async {
        let client = CountingProjectionClient()
        let state = AppState(client: client, mutationLedger: TestMutationJournal())
        await state.refresh()

        let changed = ObservationFlag()
        withObservationTracking {
            _ = state.status
            _ = state.history
            _ = state.browserSnapshot
            _ = state.connection
        } onChange: {
            changed.mark()
        }

        await state.refresh()

        #expect(!changed.read())
    }

    @Test("generation-only refreshes do not republish source or presentation state")
    func generationOnlyRefreshDoesNotRepublish() async throws {
        let initial = try await BrowserFixtureClient(scenario: .clear).browserOverview()
        var regenerated = initial
        regenerated.generatedAtUnixMillis += 1
        let client = SnapshotSequenceClient(snapshots: [initial, regenerated])
        let state = AppState(client: client, mutationLedger: TestMutationJournal())
        await state.refresh()

        let sourceChanged = ObservationFlag()
        withObservationTracking {
            _ = state.browserSnapshot
        } onChange: {
            sourceChanged.mark()
        }
        let presentationChanged = ObservationFlag()
        withObservationTracking {
            _ = state.browserOverview
            _ = state.browserHistoryEntries
        } onChange: {
            presentationChanged.mark()
        }

        await state.refresh()

        #expect(!sourceChanged.read())
        #expect(!presentationChanged.read())
        #expect(state.browserSnapshot?.generatedAtUnixMillis == initial.generatedAtUnixMillis)
    }

    @Test("meaningful snapshot changes still republish observable source state")
    func meaningfulSnapshotChangesStillRepublish() async throws {
        let initial = try await BrowserFixtureClient(scenario: .active).browserOverview()
        var freshness = initial
        freshness.generatedAtUnixMillis += 1
        freshness.freshness = .staleAfterFailure
        var observedAt = freshness
        observedAt.generatedAtUnixMillis += 1
        observedAt.observedAtUnixMillis = (observedAt.observedAtUnixMillis ?? 0) + 1
        var capability = observedAt
        capability.generatedAtUnixMillis += 1
        capability.sessions[0].capabilities.openDetail.available.toggle()
        var mode = capability
        mode.generatedAtUnixMillis += 1
        mode.effectiveMode = .enforce
        let client = SnapshotSequenceClient(
            snapshots: [initial, freshness, observedAt, capability, mode]
        )
        let state = AppState(client: client, mutationLedger: TestMutationJournal())
        await state.refresh()

        for expected in [freshness, observedAt, capability, mode] {
            let changed = ObservationFlag()
            withObservationTracking {
                _ = state.browserSnapshot
            } onChange: {
                changed.mark()
            }

            await state.refresh()

            #expect(changed.read())
            #expect(state.browserSnapshot == expected)
        }
    }

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
