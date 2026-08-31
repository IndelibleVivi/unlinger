import Foundation
import Testing
@testable import UnlingerKit

/// Every canonical wire fixture in `Contract/v2/` must decode through the
/// same envelope validation the socket client uses.
@Suite("Canonical fixture decoding")
struct FixtureDecodingTests {
    @Test("status fixtures decode as PublicStatus")
    func statusFixtures() throws {
        let names = [
            "status-all-clear",
            "status-report-only",
            "status-scanning",
            "status-paused",
            "status-recently-reclaimed",
            "status-needs-attention"
        ]
        for name in names {
            let data = try FixtureStore.data(named: name)
            let requestID = try #require(Self.requestID(in: data))
            let status = try ResponseDecoder.decode(
                PublicStatus.self,
                expectedPayloadType: "status",
                requestID: requestID,
                line: data
            )
            #expect(status.daemonVersion == "0.1.0")
        }
    }

    @Test("history fixtures decode as [HistoryEvent]")
    func historyFixtures() throws {
        for name in ["history-cleared", "history-cleared-with-residue"] {
            let data = try FixtureStore.data(named: name)
            let requestID = try #require(Self.requestID(in: data))
            let events = try ResponseDecoder.decode(
                [HistoryEvent].self,
                expectedPayloadType: "history",
                requestID: requestID,
                line: data
            )
            #expect(!events.isEmpty)
        }
    }

    @Test("incident fixtures decode as IncidentDetail")
    func incidentFixtures() throws {
        for name in ["incident-protected", "incident-revived", "incident-failed"] {
            let data = try FixtureStore.data(named: name)
            let requestID = try #require(Self.requestID(in: data))
            let detail = try ResponseDecoder.decode(
                IncidentDetail.self,
                expectedPayloadType: "incident",
                requestID: requestID,
                line: data
            )
            #expect(!detail.events.isEmpty)
        }
    }

    @Test("incidents roster fixture decodes as [CurrentIncident]")
    func incidentsFixture() throws {
        let data = try FixtureStore.data(named: "incidents-current")
        let requestID = try #require(Self.requestID(in: data))
        let roster = try ResponseDecoder.decode(
            [CurrentIncident].self,
            expectedPayloadType: "incidents",
            requestID: requestID,
            line: data
        )
        #expect(roster.count == 2)
        #expect(roster.first?.observation.state == .confirmed)
        #expect(roster.last?.observation.state == .ambiguous)
    }

    @Test("cleared_with_residue coexists with whole-plan FAILED")
    func residueSemantics() throws {
        let data = try FixtureStore.data(named: "history-cleared-with-residue")
        let requestID = try #require(Self.requestID(in: data))
        let events = try ResponseDecoder.decode(
            [HistoryEvent].self,
            expectedPayloadType: "history",
            requestID: requestID,
            line: data
        )
        let event = try #require(events.first)
        #expect(event.state == .failed)
        guard case .cleanup(let receipt) = event.payload else {
            Issue.record("expected cleanup payload")
            return
        }
        #expect(receipt.processOutcome == .cleared)
        #expect(receipt.artifactOutcome == .residue)
        #expect(receipt.overallOutcome == .clearedWithResidue)
        #expect(receipt.attentionRequired)
    }

    @Test("envelope mutual exclusion is enforced")
    func mutualExclusion() throws {
        let both = Data(#"{"schema_version":2,"request_id":1,"ok":true,"payload":{"type":"status","data":{}},"error":{"code":"x","message":"y"}}"#.utf8)
        #expect(throws: ClientError.self) {
            try ResponseDecoder.decode(
                PublicStatus.self,
                expectedPayloadType: "status",
                requestID: 1,
                line: both
            )
        }
    }

    @Test("request_id mismatch is a protocol error")
    func requestIDMismatch() throws {
        let data = try FixtureStore.data(named: "status-all-clear")
        #expect(throws: ClientError.self) {
            try ResponseDecoder.decode(
                PublicStatus.self,
                expectedPayloadType: "status",
                requestID: 999,
                line: data
            )
        }
    }

    @Test("wrong schema_version is rejected")
    func schemaVersionRejected() throws {
        let v1 = Data(#"{"schema_version":1,"request_id":1,"ok":true,"payload":{"type":"status","data":{}}}"#.utf8)
        #expect(throws: ClientError.self) {
            try ResponseDecoder.decode(
                PublicStatus.self,
                expectedPayloadType: "status",
                requestID: 1,
                line: v1
            )
        }
    }

    @Test("app-local fixtures parse")
    func appLocalFixtures() throws {
        let unavailable = try JSONSerialization.jsonObject(
            with: FixtureStore.data(named: "app-daemon-unavailable")
        ) as? [String: Any]
        #expect(unavailable?["state"] as? String == "daemon_unavailable")

        let uncertain = try JSONSerialization.jsonObject(
            with: FixtureStore.data(named: "app-mutation-delivery-uncertain")
        ) as? [String: Any]
        #expect(uncertain?["automatic_retry"] as? Bool == false)
    }

    private static func requestID(in data: Data) -> UInt64? {
        ((try? JSONSerialization.jsonObject(with: data)) as? [String: Any])
            .flatMap { $0["request_id"] as? NSNumber }?
            .uint64Value
    }
}
