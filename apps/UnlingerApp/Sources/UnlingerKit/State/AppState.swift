import Foundation
import Observation

public enum ConnectionState: Equatable, Sendable {
    case connecting
    case live
    case unavailable
    case incompatibleDaemon(String)
}

private struct RefreshSnapshot: Sendable {
    var status: PublicStatus
    var history: [HistoryEvent]
    var roster: ObservationRoster
}

private enum RefreshOutcome: Sendable {
    case success(RefreshSnapshot)
    case failure(ClientError)
}

private final class TaskCanceller: @unchecked Sendable {
    var poll: Task<Void, Never>?
    var refresh: Task<Void, Never>?
    deinit {
        poll?.cancel()
        refresh?.cancel()
    }
}

@Observable
@MainActor
public final class AppState {
    public let client: any UnlingerClient

    public private(set) var connection: ConnectionState = .connecting
    public private(set) var status: PublicStatus?
    public private(set) var history: [HistoryEvent] = []
    public private(set) var observationRoster = ObservationRoster(
        cycleToken: nil,
        observedAtUnixMillis: nil,
        freshness: .neverObserved,
        items: []
    )
    public private(set) var lastRefreshAt: Date?
    public private(set) var mutationState: MutationState = .idle
    public private(set) var pendingMutation: PendingMutation?
    public private(set) var mutationJournalAvailable = true

    public var unresolvedMutations: [PendingMutation] {
        pendingMutation.map { [$0] } ?? []
    }

    public var currentIncidents: [CurrentIncident] { observationRoster.items }
    public var browserOverview: BrowserOverview {
        BrowserOverviewMapper.make(
            connection: connection,
            status: status,
            roster: observationRoster,
            history: history
        )
    }
    public var ordinaryMutationsLocked: Bool { pendingMutation != nil || !mutationJournalAvailable }

    private let taskCanceller = TaskCanceller()
    private let historyLimit: Int
    private let mutationJournal: any MutationJournalStore
    private let notificationCoordinator: (any NotificationCoordinating)?
    private var journalLoaded = false
    private var refreshRequested = false
    private var refreshEpoch: UInt64 = 0
    private var activeRefreshID: UUID?
    private var pollingSession: UInt64 = 0
    private var reconcilingMutationID: String?

    public init(
        client: any UnlingerClient,
        historyLimit: Int = 50,
        mutationLedger: any MutationJournalStore = FileMutationJournalStore(),
        notificationCoordinator: (any NotificationCoordinating)? = nil
    ) {
        self.client = client
        self.historyLimit = historyLimit
        self.mutationJournal = mutationLedger
        self.notificationCoordinator = notificationCoordinator
    }

    public func startPolling(interval: Duration = .seconds(5)) {
        guard taskCanceller.poll == nil else { return }
        pollingSession &+= 1
        let session = pollingSession
        taskCanceller.poll = Task { [weak self] in
            while !Task.isCancelled {
                guard let self, self.pollingSession == session else { return }
                await self.refresh()
                guard self.pollingSession == session else { return }
                do {
                    try await Task.sleep(for: interval)
                } catch {
                    return
                }
            }
        }
    }

    public func stopPolling() {
        pollingSession &+= 1
        refreshEpoch &+= 1
        taskCanceller.poll?.cancel()
        taskCanceller.poll = nil
        taskCanceller.refresh?.cancel()
        taskCanceller.refresh = nil
        activeRefreshID = nil
        refreshRequested = false
    }

    /// Coalesces overlapping callers onto one full refresh loop. A request
    /// arriving while reads are active schedules exactly one trailing refresh.
    public func refresh() async {
        await loadJournalIfNeeded()
        if let active = taskCanceller.refresh {
            refreshRequested = true
            await active.value
            return
        }

        let refreshID = UUID()
        activeRefreshID = refreshID
        let epoch = refreshEpoch
        let task = Task { @MainActor [weak self] in
            guard let self else { return }
            await self.runRefreshLoop(epoch: epoch)
        }
        taskCanceller.refresh = task
        await task.value
        if activeRefreshID == refreshID {
            taskCanceller.refresh = nil
            activeRefreshID = nil
        }
    }

    public func explain(incidentID: String) async throws(ClientError) -> IncidentDetail {
        try await client.explain(incidentID: incidentID)
    }

    public func exportDiagnostics(incidentID: String) async throws(ClientError) -> DiagnosticsExport {
        try await client.exportDiagnostics(incidentID: incidentID)
    }

    public func perform(_ mutation: Mutation) async {
        await loadJournalIfNeeded()
        guard mutationJournalAvailable, pendingMutation == nil else {
            if let pendingMutation { mutationState = .unresolved(pendingMutation) }
            return
        }
        guard connection == .live, let status else { return }

        let context = MutationContext(
            namespaceToken: status.mutationAuthority.namespaceToken,
            mutationId: UUID().uuidString.lowercased()
        )
        let pending = PendingMutation(
            context: context,
            mutation: mutation,
            createdAtUnixMillis: Date().unixMillis
        )
        do {
            // The durable journal is the first side effect. No connect or send
            // is permitted before this succeeds.
            try await mutationJournal.persist(pending)
        } catch {
            mutationJournalAvailable = false
            mutationState = .failedBeforeSend(
                pending,
                reasonId: "mutation.journal_unavailable"
            )
            return
        }

        pendingMutation = pending
        mutationState = .inFlight(pending)
        do {
            let receipt = try await send(pending)
            try validate(receipt: receipt, for: pending)
            await resolve(receipt: receipt, pending: pending)
        } catch let error {
            await handleMutationError(error, pending: pending)
        }
    }

    /// Hides the presentation only. The journal and global semantic lock stay
    /// until a trusted receipt/not_found resolution removes them.
    public func dismissMutationState() {
        guard case .inFlight = mutationState else {
            mutationState = .idle
            guard var pending = pendingMutation else { return }
            pending.visualDismissed = true
            pendingMutation = pending
            Task { [mutationJournal] in
                try? await mutationJournal.persist(pending)
            }
            return
        }
    }

    public func checkAgain(mutationID: String) async {
        await loadJournalIfNeeded()
        guard let pending = pendingMutation, pending.mutationID == mutationID else { return }
        mutationState = .unresolved(pending)
        await reconcile(pending, requestRefreshAfterResolution: true)
    }

    /// pre-v0.1 intentionally uses one global unresolved ordinary-mutation
    /// lock, so any pending record blocks every fresh mutation.
    public func hasUnresolvedEquivalent(to mutation: Mutation) -> Bool {
        _ = mutation
        return ordinaryMutationsLocked
    }

    private func runRefreshLoop(epoch: UInt64) async {
        var coherenceRetryAvailable = true
        repeat {
            refreshRequested = false
            let outcome = await fetchRefreshSnapshot()
            guard !Task.isCancelled, epoch == refreshEpoch else { return }
            switch outcome {
            case .success(let snapshot):
                status = snapshot.status
                history = snapshot.history
                observationRoster = snapshot.roster
                connection = .live
                lastRefreshAt = .now
                if coherenceRetryAvailable,
                   BrowserOverviewMapper.make(
                       connection: .live,
                       status: snapshot.status,
                       roster: snapshot.roster,
                       history: snapshot.history
                   ).requiresTrailingRefresh
                {
                    coherenceRetryAvailable = false
                    refreshRequested = true
                }
                await notificationCoordinator?.receiveTrustedRefresh(
                    status: snapshot.status,
                    history: snapshot.history,
                    roster: snapshot.roster,
                    atUnixMillis: Date().unixMillis
                )
                if let pending = pendingMutation {
                    await reconcile(pending, requestRefreshAfterResolution: false)
                }
            case .failure(.incompatibleDaemon(let reason)):
                connection = .incompatibleDaemon(reason)
                markRosterStaleAfterFailure()
            case .failure(.unavailable):
                connection = .unavailable
                markRosterStaleAfterFailure()
                await notificationCoordinator?.receiveUnavailable(
                    atUnixMillis: Date().unixMillis
                )
            case .failure:
                connection = .unavailable
                markRosterStaleAfterFailure()
            }
        } while refreshRequested && !Task.isCancelled && epoch == refreshEpoch
    }

    private func fetchRefreshSnapshot() async -> RefreshOutcome {
        let client = self.client
        let historyLimit = self.historyLimit
        do {
            async let status = client.status()
            async let history = client.history(limit: historyLimit)
            async let roster = client.incidents()
            let values = try await (status, history, roster)
            return .success(RefreshSnapshot(status: values.0, history: values.1, roster: values.2))
        } catch let error as ClientError {
            return .failure(error)
        } catch {
            return .failure(.protocolError("unexpected refresh failure"))
        }
    }

    private func loadJournalIfNeeded() async {
        guard !journalLoaded else { return }
        journalLoaded = true
        do {
            pendingMutation = try await mutationJournal.load()
            if let pendingMutation, !pendingMutation.visualDismissed {
                mutationState = .unresolved(pendingMutation)
            }
        } catch {
            mutationJournalAvailable = false
        }
    }

    private func send(_ pending: PendingMutation) async throws(ClientError) -> MutationReceipt {
        switch pending.mutation {
        case .pause(let duration, _):
            try await client.pause(context: pending.context, durationMillis: duration)
        case .resume:
            try await client.resume(context: pending.context)
        case .retryFailedCleanup(let incidentID):
            try await client.retryFailedCleanup(context: pending.context, incidentID: incidentID)
        case .protect(let incidentID):
            try await client.protectIncident(context: pending.context, incidentID: incidentID)
        case .unprotect(let incidentID):
            try await client.unprotectIncident(context: pending.context, incidentID: incidentID)
        }
    }

    private func reconcile(
        _ pending: PendingMutation,
        requestRefreshAfterResolution: Bool
    ) async {
        guard reconcilingMutationID != pending.mutationID else { return }
        reconcilingMutationID = pending.mutationID
        defer { reconcilingMutationID = nil }
        do {
            let result = try await client.mutationStatus(context: pending.context)
            switch result {
            case .notFound(let context):
                try validate(context: context, pending: pending)
                if await clearPending(pending) {
                    mutationState = .definitelyNotApplied(pending)
                }
            case .authorityLost(let context):
                try validate(context: context, pending: pending)
                mutationState = .authorityLost(pending)
            case .committed(let receipt):
                try validate(receipt: receipt, for: pending)
                await resolve(receipt: receipt, pending: pending)
            }
            if requestRefreshAfterResolution, pendingMutation == nil {
                refreshRequested = true
                await refresh()
            }
        } catch let error {
            switch error {
            case .incompatibleDaemon(let reason): connection = .incompatibleDaemon(reason)
            case .unavailable: connection = .unavailable
            default: break
            }
            if pendingMutation != nil { mutationState = .unresolved(pending) }
        }
    }

    private func resolve(receipt: MutationReceipt, pending: PendingMutation) async {
        switch receipt.outcome {
        case .applied:
            if await clearPending(pending) {
                mutationState = .confirmed(pending, receipt)
                refreshRequested = true
            }
        case .noChange(let reasonID), .rejected(let reasonID):
            if await clearPending(pending) {
                mutationState = .rejected(pending, reasonId: reasonID)
                refreshRequested = true
            }
        case .unknown:
            mutationState = .unresolved(pending)
        }
    }

    private func handleMutationError(_ error: ClientError, pending: PendingMutation) async {
        switch error {
        case .deliveryUncertain, .protocolError:
            mutationState = .unresolved(pending)
            await reconcile(pending, requestRefreshAfterResolution: true)
        case .serverError(let code, _):
            if code == "authority_lost" {
                mutationState = .authorityLost(pending)
            } else if await clearPending(pending) {
                mutationState = .rejected(pending, reasonId: code)
                refreshRequested = true
            }
        case .unavailable:
            connection = .unavailable
            if await clearPending(pending) {
                mutationState = .failedBeforeSend(
                    pending,
                    reasonId: "transport.daemon_unavailable"
                )
            }
        case .failedBeforeSend(let reasonID):
            if await clearPending(pending) {
                mutationState = .failedBeforeSend(pending, reasonId: reasonID)
            }
        case .incompatibleDaemon(let reason):
            connection = .incompatibleDaemon(reason)
            if await clearPending(pending) {
                mutationState = .failedBeforeSend(
                    pending,
                    reasonId: "transport.incompatible_daemon"
                )
            }
        }
    }

    private func validate(context: MutationContext, pending: PendingMutation) throws(ClientError) {
        guard context == pending.context else { throw .protocolError("mutation context mismatch") }
    }

    private func validate(
        receipt: MutationReceipt,
        for pending: PendingMutation
    ) throws(ClientError) {
        guard receipt.namespaceToken == pending.namespaceToken,
              receipt.mutationId == pending.mutationID,
              receipt.kind == pending.mutation.kind,
              receipt.retainUntilUnixMillis >= receipt.committedAtUnixMillis
        else {
            throw .deliveryUncertain
        }
        guard case .applied(let result) = receipt.outcome else {
            if case .unknown = receipt.outcome { throw .deliveryUncertain }
            return
        }
        let matches = switch (pending.mutation, result) {
        case (.pause, .paused), (.resume, .resumed): true
        case (.retryFailedCleanup(let expected), .retryScheduled(let actual)): expected == actual
        case (.protect(let expected), .incidentProtected(let value)): expected == value.incidentId
        case (.unprotect(let expected), .incidentUnprotected(let actual)): expected == actual
        default: false
        }
        guard matches else { throw .deliveryUncertain }
    }

    private func clearPending(_ pending: PendingMutation) async -> Bool {
        do {
            try await mutationJournal.remove(expectedMutationID: pending.mutationID)
            if pendingMutation?.mutationID == pending.mutationID { pendingMutation = nil }
            mutationJournalAvailable = true
            return true
        } catch {
            mutationJournalAvailable = false
            mutationState = .unresolved(pending)
            return false
        }
    }

    private func markRosterStaleAfterFailure() {
        guard observationRoster.freshness != .neverObserved else { return }
        observationRoster.freshness = .staleAfterFailure
    }
}
