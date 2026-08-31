import Foundation

/// The schema-v2 ordinary command surface. Fixture-backed in previews/tests,
/// Unix-socket-backed in production. Every call opens a fresh connection and
/// makes exactly one attempt — never an automatic resend.
public protocol UnlingerClient: Sendable {
    func status() async throws(ClientError) -> PublicStatus
    func history(limit: Int) async throws(ClientError) -> [HistoryEvent]
    func explain(incidentID: String) async throws(ClientError) -> IncidentDetail
    /// The current-incident roster from the latest reconciliation cycle.
    func incidents() async throws(ClientError) -> [CurrentIncident]
    /// Returns the pause deadline (`until_unix_millis`).
    func pause(durationMillis: UInt64) async throws(ClientError) -> UInt64
    func resume() async throws(ClientError)
    func retryFailedCleanup(incidentID: String) async throws(ClientError)
    func protectIncident(incidentID: String) async throws(ClientError)
    func unprotectIncident(incidentID: String) async throws(ClientError)
    func exportDiagnostics(incidentID: String) async throws(ClientError) -> DiagnosticsExport
}

/// Diagnostics bundle plus the verbatim wire JSON for private file export —
/// re-encoding the typed DTO would drop any fields the app doesn't model.
public struct DiagnosticsExport: Sendable, Equatable {
    public var bundle: DiagnosticsBundle
    public var rawJSON: Data?
}

struct PauseResult: Decodable, Sendable {
    let untilUnixMillis: UInt64
}

struct IncidentIDResult: Decodable, Sendable {
    let incidentId: String
}

struct ProtectionResult: Decodable, Sendable {
    let protection: ProtectedIncidentSummary
}

extension UnlingerClient {
    /// Sends one command, validates the envelope, decodes the typed payload.
    static func decode<T: Decodable & Sendable>(_ type: T.Type, for command: Command, requestID: UInt64, line: Data) throws(ClientError) -> T {
        let expectedType: String = switch command {
        case .status: "status"
        case .history: "history"
        case .explain: "incident"
        case .incidents: "incidents"
        case .pause: "pause"
        case .resume: "resumed"
        case .retryFailedCleanup: "retry_scheduled"
        case .protectIncident: "incident_protected"
        case .unprotectIncident: "incident_unprotected"
        case .exportDiagnostics: "diagnostics"
        }
        return try ResponseDecoder.decode(T.self, expectedPayloadType: expectedType, requestID: requestID, line: line)
    }
}
