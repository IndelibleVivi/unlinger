import Foundation
import Testing
@testable import UnlingerKit

actor MutationTrace {
    private(set) var events: [String] = []

    func append(_ event: String) {
        events.append(event)
    }
}

actor TestMutationJournal: MutationJournalStore {
    private var record: PendingMutation?
    private let trace: MutationTrace?
    private let failPersist: Bool

    init(
        record: PendingMutation? = nil,
        trace: MutationTrace? = nil,
        failPersist: Bool = false
    ) {
        self.record = record
        self.trace = trace
        self.failPersist = failPersist
    }

    func load() async throws -> PendingMutation? {
        record
    }

    func persist(_ pending: PendingMutation) async throws {
        if failPersist { throw MutationJournalError.ioFailure }
        if let record, record.mutationID != pending.mutationID {
            throw MutationJournalError.occupied
        }
        record = pending
        await trace?.append("journal.persist")
    }

    func remove(expectedMutationID: String) async throws {
        if let record, record.mutationID != expectedMutationID {
            throw MutationJournalError.occupied
        }
        record = nil
        await trace?.append("journal.remove")
    }

    func stored() -> PendingMutation? {
        record
    }
}

private enum LookupScript: Sendable {
    case notFound
    case authorityLost
    case committed
    case unavailable
}

/// Scripted client with call accounting. Its mutation-status path is always
/// context-bound, so tests cannot accidentally pass by matching only an ID.
private actor ScriptClient: UnlingerClient {
    private let fixture = FixtureClient(statusFixture: "status-all-clear")
    private let trace: MutationTrace?

    private(set) var statusCalls = 0
    private(set) var pauseCalls = 0
    private(set) var retryCalls = 0
    private(set) var mutationStatusCalls = 0

    private var pauseError: ClientError?
    private var incidentsError: ClientError?
    private var lookup: LookupScript = .notFound

    init(trace: MutationTrace? = nil) {
        self.trace = trace
    }

    func setPauseError(_ error: ClientError?) { pauseError = error }
    func setIncidentsError(_ error: ClientError?) { incidentsError = error }
    func setLookup(_ value: LookupScript) { lookup = value }

    func status() async throws(ClientError) -> PublicStatus {
        statusCalls += 1
        return try await fixture.status()
    }

    func browserOverview() async throws(ClientError) -> BrowserOverviewSnapshot {
        if let incidentsError { throw incidentsError }
        return try await BrowserFixtureClient(scenario: .active).browserOverview()
    }

    func history(limit _: Int) async throws(ClientError) -> [HistoryEvent] {
        []
    }

    func incidents() async throws(ClientError) -> ObservationRoster {
        if let incidentsError { throw incidentsError }
        return try await FixtureClient(
            statusFixture: "status-all-clear",
            incidentsFixture: "incidents-current"
        ).incidents()
    }

    func explain(incidentID: String) async throws(ClientError) -> IncidentDetail {
        IncidentDetail(
            incidentId: incidentID,
            events: [],
            capabilities: IncidentCapabilities(
                retryFailedCleanup: Capability(available: true),
                protectIncident: Capability(available: true),
                unprotectIncident: Capability(available: true),
                exportDiagnostics: Capability(available: true)
            )
        )
    }

    func mutationStatus(context: MutationContext) async throws(ClientError) -> MutationStatus {
        mutationStatusCalls += 1
        switch lookup {
        case .notFound: return .notFound(context)
        case .authorityLost: return .authorityLost(context)
        case .committed:
            return .committed(receipt(
                context: context,
                kind: .pause,
                result: .paused(untilUnixMillis: 1_900_000_000_000)
            ))
        case .unavailable: throw .unavailable
        }
    }

    func pause(
        context: MutationContext,
        durationMillis _: UInt64
    ) async throws(ClientError) -> MutationReceipt {
        pauseCalls += 1
        await trace?.append("client.send")
        if let pauseError { throw pauseError }
        return receipt(
            context: context,
            kind: .pause,
            result: .paused(untilUnixMillis: 1_900_000_000_000)
        )
    }

    func resume(context: MutationContext) async throws(ClientError) -> MutationReceipt {
        receipt(context: context, kind: .resume, result: .resumed)
    }

    func retryFailedCleanup(
        context: MutationContext,
        incidentID: String
    ) async throws(ClientError) -> MutationReceipt {
        retryCalls += 1
        return receipt(
            context: context,
            kind: .retryFailedCleanup,
            result: .retryScheduled(incidentID: incidentID)
        )
    }

    func protectIncident(
        context: MutationContext,
        incidentID: String
    ) async throws(ClientError) -> MutationReceipt {
        receipt(
            context: context,
            kind: .protectIncident,
            result: .incidentProtected(
                ProtectedIncidentSummary(
                    incidentId: incidentID,
                    protectedAtUnixMillis: 1,
                    lastExactObservedAtUnixMillis: nil,
                    exactAbsenceSinceUnixMillis: nil
                )
            )
        )
    }

    func unprotectIncident(
        context: MutationContext,
        incidentID: String
    ) async throws(ClientError) -> MutationReceipt {
        receipt(
            context: context,
            kind: .unprotectIncident,
            result: .incidentUnprotected(incidentID: incidentID)
        )
    }

    func exportDiagnostics(incidentID _: String) async throws(ClientError) -> DiagnosticsExport {
        throw .unavailable
    }

    private func receipt(
        context: MutationContext,
        kind: MutationKind,
        result: MutationResult
    ) -> MutationReceipt {
        MutationReceipt(
            namespaceToken: context.namespaceToken,
            mutationId: context.mutationId,
            kind: kind,
            committedAtUnixMillis: 1_800_000_000_000,
            retainUntilUnixMillis: 1_801_209_600_000,
            policyRevisionAfter: 2,
            outcome: .applied(result)
        )
    }
}

private struct DeadClient: UnlingerClient {
    func status() async throws(ClientError) -> PublicStatus { throw .unavailable }
    func browserOverview() async throws(ClientError) -> BrowserOverviewSnapshot { throw .unavailable }
    func history(limit _: Int) async throws(ClientError) -> [HistoryEvent] { throw .unavailable }
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

@Suite("Durable mutation reconciliation")
@MainActor
struct MutationFlowTests {
    @Test("journal commit precedes the first mutation byte")
    func journalBeforeSend() async throws {
        let trace = MutationTrace()
        let journal = TestMutationJournal(trace: trace)
        let client = ScriptClient(trace: trace)
        let state = AppState(client: client, mutationLedger: journal)
        await state.refresh()

        await state.perform(.pause(durationMillis: 7_200_000, label: "2h"))

        #expect(await trace.events == ["journal.persist", "client.send", "journal.remove"])
        #expect(await client.pauseCalls == 1)
        #expect(await journal.stored() == nil)
        guard case .confirmed = state.mutationState else {
            Issue.record("expected trusted confirmation")
            return
        }
    }

    @Test("post-send uncertainty never resends and retains the global lock")
    func uncertainNeverResends() async throws {
        let journal = TestMutationJournal()
        let client = ScriptClient()
        await client.setPauseError(.deliveryUncertain)
        await client.setLookup(.unavailable)
        let state = AppState(client: client, mutationLedger: journal)
        await state.refresh()

        await state.perform(.pause(durationMillis: 7_200_000, label: "2h"))
        await state.perform(.retryFailedCleanup(incidentID: "redacted-incident-5"))

        #expect(await client.pauseCalls == 1)
        #expect(await client.retryCalls == 0)
        #expect(await client.mutationStatusCalls == 1)
        #expect(await journal.stored() != nil)
        #expect(state.ordinaryMutationsLocked)
        guard case .unresolved = state.mutationState else {
            Issue.record("expected unresolved delivery")
            return
        }
    }

    @Test("startup reconciles a committed journal without resending")
    func startupCommitReconciliation() async throws {
        let pending = PendingMutation(
            context: MutationContext(
                namespaceToken: "0123456789abcdef0123456789abcdef",
                mutationId: "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee"
            ),
            mutation: .pause(durationMillis: 7_200_000, label: "2h"),
            createdAtUnixMillis: 1
        )
        let journal = TestMutationJournal(record: pending)
        let client = ScriptClient()
        await client.setLookup(.committed)
        let state = AppState(client: client, mutationLedger: journal)

        await state.refresh()

        #expect(await client.pauseCalls == 0)
        #expect(await client.mutationStatusCalls == 1)
        #expect(await journal.stored() == nil)
        guard case .confirmed(let resolved, _) = state.mutationState else {
            Issue.record("expected journal reconciliation")
            return
        }
        #expect(resolved == pending)
    }

    @Test("authority loss preserves the record and mutation lock")
    func authorityLossPreservesJournal() async throws {
        let pending = PendingMutation(
            context: MutationContext(
                namespaceToken: "0123456789abcdef0123456789abcdef",
                mutationId: "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee"
            ),
            mutation: .resume,
            createdAtUnixMillis: 1
        )
        let journal = TestMutationJournal(record: pending)
        let client = ScriptClient()
        await client.setLookup(.authorityLost)
        let state = AppState(client: client, mutationLedger: journal)

        await state.refresh()

        #expect(await journal.stored() == pending)
        #expect(state.ordinaryMutationsLocked)
        guard case .authorityLost(let unresolved) = state.mutationState else {
            Issue.record("expected authority_lost")
            return
        }
        #expect(unresolved == pending)
    }

    @Test("a journal failure blocks writes but leaves status readable")
    func journalFailureIsFailClosed() async throws {
        let journal = TestMutationJournal(failPersist: true)
        let client = ScriptClient()
        let state = AppState(client: client, mutationLedger: journal)
        await state.refresh()

        await state.perform(.pause(durationMillis: 7_200_000, label: "2h"))

        #expect(state.connection == .live)
        #expect(state.status != nil)
        #expect(state.ordinaryMutationsLocked)
        #expect(await client.pauseCalls == 0)
        guard case .failedBeforeSend(_, let reasonID) = state.mutationState else {
            Issue.record("expected journal failure")
            return
        }
        #expect(reasonID == "mutation.journal_unavailable")
    }

    @Test("read-side failure never masquerades as all-clear")
    func refreshUnavailable() async throws {
        let state = AppState(client: DeadClient(), mutationLedger: TestMutationJournal())
        await state.refresh()
        #expect(state.connection == .unavailable)
        #expect(state.status == nil)
    }

    @Test("an initial secondary read failure cannot publish a partial projection")
    func secondaryReadUnavailable() async throws {
        let client = ScriptClient()
        await client.setIncidentsError(.unavailable)
        let state = AppState(client: client, mutationLedger: TestMutationJournal())
        await state.refresh()
        #expect(state.connection == .unavailable)
        #expect(state.status == nil)
        #expect(state.browserSnapshot == nil)
    }

    @Test("a replacement read failure retains the prior browser snapshot and marks it stale")
    func replacementReadFailureRetainsBrowserSnapshot() async throws {
        let client = ScriptClient()
        let state = AppState(client: client, mutationLedger: TestMutationJournal())
        await state.refresh()

        let priorToken = state.browserSnapshot?.cycleToken
        let priorTime = state.browserSnapshot?.observedAtUnixMillis
        let priorSessions = state.browserSnapshot?.sessions
        #expect(state.connection == .live)
        #expect(state.browserSnapshot?.freshness == .current)
        #expect(!(priorSessions ?? []).isEmpty)

        await client.setIncidentsError(.unavailable)
        await state.refresh()

        #expect(state.connection == .unavailable)
        #expect(state.status != nil)
        #expect(state.browserSnapshot?.freshness == .staleAfterFailure)
        #expect(state.browserSnapshot?.cycleToken == priorToken)
        #expect(state.browserSnapshot?.observedAtUnixMillis == priorTime)
        #expect(state.browserSnapshot?.sessions == priorSessions)
    }
}
