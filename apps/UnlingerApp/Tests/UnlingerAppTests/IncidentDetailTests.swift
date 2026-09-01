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
}
