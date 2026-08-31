import Foundation
import Observation

/// Connection truth for the UI. `unavailable` is an app-local state (see
/// `app-daemon-unavailable.json`): the daemon cannot be reached and the app
/// must not pretend everything is clear.
public enum ConnectionState: Equatable, Sendable {
    case connecting
    case live
    case unavailable
}

/// Cancels the polling task from a nonisolated deinit path — `AppState` is
/// MainActor-isolated, so it cannot touch its own task property in `deinit`.
private final class PollCanceller: @unchecked Sendable {
    var task: Task<Void, Never>?

    deinit { task?.cancel() }
}

@Observable
@MainActor
public final class AppState {
    public let client: any UnlingerClient

    public private(set) var connection: ConnectionState = .connecting
    public private(set) var status: PublicStatus?
    public private(set) var history: [HistoryEvent] = []
    /// The current-incidents roster: what the latest reconciliation cycle is
    /// actually seeing. Read-only observability — no actions derive from it.
    public private(set) var currentIncidents: [CurrentIncident] = []
    public private(set) var lastRefreshAt: Date?
    public private(set) var mutationState: MutationState = .idle

    /// Result of a successful diagnostics export, kept for UI confirmation.
    public private(set) var lastExport: DiagnosticsExport?

    public var viewModel: StatusViewModel? {
        status.map { StatusMapper.viewModel(for: $0) }
    }

    private let pollCanceller = PollCanceller()
    private let historyLimit: Int

    public init(client: any UnlingerClient, historyLimit: Int = 50) {
        self.client = client
        self.historyLimit = historyLimit
    }

    public func startPolling(interval: Duration = .seconds(5)) {
        guard pollCanceller.task == nil else { return }
        pollCanceller.task = Task { [weak self] in
            while !Task.isCancelled {
                await self?.refresh()
                try? await Task.sleep(for: interval)
            }
        }
    }

    public func stopPolling() {
        pollCanceller.task?.cancel()
        pollCanceller.task = nil
    }

    /// Reads the public status, retained history, and current roster as one
    /// UI refresh. Any read-side failure makes this projection unavailable;
    /// never retain a stale roster under a fresh-looking "live" status.
    public func refresh() async {
        do {
            let newStatus = try await client.status()
            let newHistory = try await client.history(limit: historyLimit)
            let newCurrentIncidents = try await client.incidents()
            status = newStatus
            history = newHistory
            currentIncidents = newCurrentIncidents
            connection = .live
            lastRefreshAt = .now
        } catch {
            connection = .unavailable
            status = nil
            history = []
            currentIncidents = []
            return
        }
    }

    public func explain(incidentID: String) async throws(ClientError) -> IncidentDetail {
        try await client.explain(incidentID: incidentID)
    }

    /// Runs a mutation with the delivery-uncertainty contract:
    /// trusted success → confirmed; timeout/EOF/disconnect after send →
    /// read back durable state, show `uncertain`, never resend automatically.
    public func perform(_ mutation: Mutation) async {
        if case .inFlight = mutationState { return }
        mutationState = .inFlight(mutation)
        do {
            try await send(mutation)
            mutationState = .confirmed(mutation)
            await refresh()
        } catch .deliveryUncertain {
            await readback(for: mutation)
            mutationState = .uncertain(mutation)
        } catch .unavailable {
            connection = .unavailable
            mutationState = .failed(reasonId: "transport.daemon_unavailable")
        } catch .serverError(let code, _) {
            mutationState = .failed(reasonId: code)
            await refresh()
        } catch {
            mutationState = .failed(reasonId: "transport.protocol_error")
        }
    }

    public func dismissMutationState() {
        if case .inFlight = mutationState { return }
        mutationState = .idle
    }

    private func send(_ mutation: Mutation) async throws(ClientError) {
        switch mutation {
        case .pause(let durationMillis, _):
            _ = try await client.pause(durationMillis: durationMillis)
        case .resume:
            try await client.resume()
        case .retryFailedCleanup(let incidentID):
            try await client.retryFailedCleanup(incidentID: incidentID)
        case .protect(let incidentID):
            try await client.protectIncident(incidentID: incidentID)
        case .unprotect(let incidentID):
            try await client.unprotectIncident(incidentID: incidentID)
        case .exportDiagnostics(let incidentID):
            lastExport = try await client.exportDiagnostics(incidentID: incidentID)
        }
    }

    /// Read-only readback after an uncertain mutation. pause/resume read
    /// status; incident-scoped mutations read explain. A failed readback still
    /// leaves the app honest: the state stays `uncertain`.
    private func readback(for mutation: Mutation) async {
        switch mutation.readbackCommand {
        case .status:
            await refresh()
        case .explain(let incidentID):
            _ = try? await client.explain(incidentID: incidentID)
            await refresh()
        default:
            await refresh()
        }
    }
}
