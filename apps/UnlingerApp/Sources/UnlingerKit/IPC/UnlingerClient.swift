import Foundation

/// The schema-v3 ordinary command surface. Fixture-backed in previews/tests,
/// Unix-socket-backed in production. Every call opens a fresh connection and
/// makes exactly one attempt — never an automatic resend.
public protocol UnlingerClient: Sendable {
    func status() async throws(ClientError) -> PublicStatus
    func history(limit: Int) async throws(ClientError) -> [HistoryEvent]
    func explain(incidentID: String) async throws(ClientError) -> IncidentDetail
    func incidents() async throws(ClientError) -> ObservationRoster
    func mutationStatus(context: MutationContext) async throws(ClientError) -> MutationStatus
    func pause(context: MutationContext, durationMillis: UInt64) async throws(ClientError) -> MutationReceipt
    func resume(context: MutationContext) async throws(ClientError) -> MutationReceipt
    func retryFailedCleanup(context: MutationContext, incidentID: String) async throws(ClientError) -> MutationReceipt
    func protectIncident(context: MutationContext, incidentID: String) async throws(ClientError) -> MutationReceipt
    func unprotectIncident(context: MutationContext, incidentID: String) async throws(ClientError) -> MutationReceipt
    func exportDiagnostics(incidentID: String) async throws(ClientError) -> DiagnosticsExport
}

/// Diagnostics bundle plus semantic-lossless JSON for private file export.
/// Unknown fields are retained; whitespace and key order are not contractual.
public struct DiagnosticsExport: Sendable, Equatable {
    public var bundle: DiagnosticsBundle
    public var rawJSON: Data?
}

extension UnlingerClient {
    /// Sends one command, validates the envelope, decodes the typed payload.
    static func decode<T: Decodable & Sendable>(_ type: T.Type, for command: Command, requestID: UInt64, line: Data) throws(ClientError) -> T {
        let expectedType: String = switch command {
        case .status: "status"
        case .history: "history"
        case .explain: "incident"
        case .incidents: "incidents"
        case .mutationStatus: "mutation_status"
        case .pause, .resume, .retryFailedCleanup, .protectIncident,
             .unprotectIncident: "mutation_committed"
        case .exportDiagnostics: "diagnostics"
        }
        return try ResponseDecoder.decode(T.self, expectedPayloadType: expectedType, requestID: requestID, line: line)
    }
}
