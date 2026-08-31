import Foundation
import Testing
@testable import UnlingerKit

/// Scripted client for mutation-flow tests: counts every call so tests can
/// prove the app never resends a mutation after uncertain delivery.
actor ScriptClient: UnlingerClient {
    let statusFixture: String

    private(set) var statusCalls = 0
    private(set) var explainCalls = 0
    private(set) var pauseCalls = 0
    private(set) var retryCalls = 0

    var pauseError: ClientError?
    var retryError: ClientError?
    var explainError: ClientError?
    var historyError: ClientError?
    var incidentsError: ClientError?

    init(statusFixture: String = "status-all-clear") {
        self.statusFixture = statusFixture
    }

    func status() async throws(ClientError) -> PublicStatus {
        statusCalls += 1
        return try await FixtureClient(statusFixture: statusFixture).status()
    }

    func history(limit: Int) throws(ClientError) -> [HistoryEvent] {
        if let historyError { throw historyError }
        return []
    }

    func incidents() throws(ClientError) -> [CurrentIncident] {
        if let incidentsError { throw incidentsError }
        return []
    }

    func explain(incidentID: String) throws(ClientError) -> IncidentDetail {
        explainCalls += 1
        if let explainError { throw explainError }
        return IncidentDetail(
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

    func pause(durationMillis: UInt64) throws(ClientError) -> UInt64 {
        pauseCalls += 1
        if let pauseError { throw pauseError }
        return 1_800_000_000_000
    }

    func resume() throws(ClientError) {}
    func protectIncident(incidentID: String) throws(ClientError) {}
    func unprotectIncident(incidentID: String) throws(ClientError) {}

    func exportDiagnostics(incidentID: String) throws(ClientError) -> DiagnosticsExport {
        DiagnosticsExport(
            bundle: DiagnosticsBundle(schemaVersion: nil, generatedAtUnixMillis: nil, status: nil, incident: nil),
            rawJSON: nil
        )
    }

    func retryFailedCleanup(incidentID: String) throws(ClientError) {
        retryCalls += 1
        if let retryError { throw retryError }
    }
}

@Suite("Mutation delivery-uncertainty flow")
@MainActor
struct MutationFlowTests {
    @Test("trusted success confirms and refreshes")
    func successPath() async throws {
        let client = ScriptClient()
        let state = AppState(client: client)
        await state.perform(.pause(durationMillis: 7_200_000, label: "2h"))
        #expect(state.mutationState == .confirmed(.pause(durationMillis: 7_200_000, label: "2h")))
        #expect(await client.pauseCalls == 1)
        #expect(state.connection == .live)
    }

    @Test("uncertain pause: no resend, status read back")
    func uncertainPause() async throws {
        let client = ScriptClient()
        await client.setPauseError(.deliveryUncertain)
        let state = AppState(client: client)
        await state.perform(.pause(durationMillis: 7_200_000, label: "2h"))
        #expect(state.mutationState == .uncertain(.pause(durationMillis: 7_200_000, label: "2h")))
        // Sent exactly once — never resent — and status was read back.
        #expect(await client.pauseCalls == 1)
        #expect(await client.statusCalls >= 1)
    }

    @Test("uncertain retry: no resend, incident read back via explain")
    func uncertainRetry() async throws {
        let client = ScriptClient()
        await client.setRetryError(.deliveryUncertain)
        let state = AppState(client: client)
        await state.perform(.retryFailedCleanup(incidentID: "redacted-incident-5"))
        #expect(state.mutationState == .uncertain(.retryFailedCleanup(incidentID: "redacted-incident-5")))
        #expect(await client.retryCalls == 1)
        #expect(await client.explainCalls == 1)
    }

    @Test("explicit user retry is a fresh mutation")
    func explicitRetryAllowed() async throws {
        let client = ScriptClient()
        await client.setPauseError(.deliveryUncertain)
        let state = AppState(client: client)
        await state.perform(.pause(durationMillis: 7_200_000, label: "2h"))
        await client.setPauseError(nil)
        // User explicitly chooses the action again.
        await state.perform(.pause(durationMillis: 7_200_000, label: "2h"))
        #expect(await client.pauseCalls == 2)
        #expect(state.mutationState == .confirmed(.pause(durationMillis: 7_200_000, label: "2h")))
    }

    @Test("typed server error fails with its reason id")
    func serverError() async throws {
        let client = ScriptClient()
        await client.setPauseError(.serverError(code: "invalid_argument", message: "bad duration"))
        let state = AppState(client: client)
        await state.perform(.pause(durationMillis: 0, label: "0"))
        #expect(state.mutationState == .failed(reasonId: "invalid_argument"))
    }

    @Test("connect failure during mutation surfaces unavailable, not uncertainty")
    func unavailable() async throws {
        let client = ScriptClient()
        await client.setPauseError(.unavailable)
        let state = AppState(client: client)
        await state.perform(.pause(durationMillis: 7_200_000, label: "2h"))
        #expect(state.connection == .unavailable)
        #expect(state.mutationState == .failed(reasonId: "transport.daemon_unavailable"))
    }

    @Test("read-side transport failure never masquerades as all-clear")
    func refreshUnavailable() async throws {
        struct DeadClient: UnlingerClient {
            func status() throws(ClientError) -> PublicStatus { throw .unavailable }
            func history(limit: Int) throws(ClientError) -> [HistoryEvent] { throw .unavailable }
            func incidents() throws(ClientError) -> [CurrentIncident] { throw .unavailable }
            func explain(incidentID: String) throws(ClientError) -> IncidentDetail { throw .unavailable }
            func pause(durationMillis: UInt64) throws(ClientError) -> UInt64 { throw .unavailable }
            func resume() throws(ClientError) { throw .unavailable }
            func retryFailedCleanup(incidentID: String) throws(ClientError) { throw .unavailable }
            func protectIncident(incidentID: String) throws(ClientError) { throw .unavailable }
            func unprotectIncident(incidentID: String) throws(ClientError) { throw .unavailable }
            func exportDiagnostics(incidentID: String) throws(ClientError) -> DiagnosticsExport { throw .unavailable }
        }
        let state = AppState(client: DeadClient())
        await state.refresh()
        #expect(state.connection == .unavailable)
        #expect(state.status == nil)
    }

    @Test("secondary read failure never leaves a fresh-looking live projection")
    func secondaryReadUnavailable() async throws {
        let client = ScriptClient()
        await client.setIncidentsError(.unavailable)
        let state = AppState(client: client)
        await state.refresh()
        #expect(state.connection == .unavailable)
        #expect(state.status == nil)
        #expect(state.currentIncidents.isEmpty)
    }
}

extension ScriptClient {
    func setPauseError(_ error: ClientError?) { pauseError = error }
    func setRetryError(_ error: ClientError?) { retryError = error }
    func setIncidentsError(_ error: ClientError?) { incidentsError = error }
}
