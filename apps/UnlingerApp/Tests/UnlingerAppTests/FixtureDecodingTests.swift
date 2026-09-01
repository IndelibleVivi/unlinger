import Foundation
import Testing
@testable import UnlingerKit

/// Every canonical wire fixture in `Contract/v3/` must decode through the
/// same envelope validation the socket client uses.
@Suite("Canonical fixture decoding")
struct FixtureDecodingTests {
    @Test("status fixtures decode as PublicStatus")
    func statusFixtures() throws {
        let names = [
            "status-all-clear",
            "status-report-only",
            "status-enforce",
            "status-scanning",
            "status-starting",
            "status-draining",
            "status-failed",
            "status-paused",
            "status-recently-reclaimed",
            "status-needs-attention",
            "status-storage-recovered"
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

    @Test("all roster freshness fixtures decode without invented time")
    func rosterFreshnessFixtures() throws {
        for name in ["roster-current", "roster-scanning", "roster-stale", "roster-never-observed"] {
            let data = try FixtureStore.data(named: name)
            let requestID = try #require(Self.requestID(in: data))
            _ = try ResponseDecoder.decode(
                ObservationRoster.self,
                expectedPayloadType: "incidents",
                requestID: requestID,
                line: data
            )
        }
        let neverData = try FixtureStore.data(named: "roster-never-observed")
        let never = try ResponseDecoder.decode(
            ObservationRoster.self,
            expectedPayloadType: "incidents",
            requestID: try #require(Self.requestID(in: neverData)),
            line: neverData
        )
        #expect(never.freshness == .neverObserved)
        #expect(never.cycleToken == nil)
        #expect(never.observedAtUnixMillis == nil)
    }

    @Test("diagnostics fixture has an exact typed document version")
    func diagnosticsFixture() throws {
        let data = try FixtureStore.data(named: "diagnostics")
        let diagnostics = try ResponseDecoder.decode(
            DiagnosticsBundle.self,
            expectedPayloadType: "diagnostics",
            requestID: try #require(Self.requestID(in: data)),
            line: data
        )
        #expect(diagnostics.documentSchemaVersion == 3)
        #expect(diagnostics.incident.incidentId == "redacted-incident-5")
    }

    @Test("mutation receipts and namespace-aware statuses decode")
    func mutationFixtures() throws {
        let committedData = try FixtureStore.data(named: "mutation-committed")
        let committed = try ResponseDecoder.decode(
            MutationReceipt.self,
            expectedPayloadType: "mutation_committed",
            requestID: try #require(Self.requestID(in: committedData)),
            line: committedData
        )
        #expect(committed.policyRevisionAfter == 2)

        for name in ["mutation-not-found", "mutation-authority-lost"] {
            let data = try FixtureStore.data(named: name)
            _ = try ResponseDecoder.decode(
                MutationStatus.self,
                expectedPayloadType: "mutation_status",
                requestID: try #require(Self.requestID(in: data)),
                line: data
            )
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

    @Test("incidents roster fixture decodes as ObservationRoster")
    func incidentsFixture() throws {
        let data = try FixtureStore.data(named: "incidents-current")
        let requestID = try #require(Self.requestID(in: data))
        let roster = try ResponseDecoder.decode(
            ObservationRoster.self,
            expectedPayloadType: "incidents",
            requestID: requestID,
            line: data
        )
        #expect(roster.items.count == 2)
        #expect(roster.items.first?.observation.state == .confirmed)
        #expect(roster.items.last?.observation.state == .ambiguous)
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
        let both = Data(#"{"schema_version":3,"request_id":1,"ok":true,"payload":{"type":"status","data":{}},"error":{"code":"x","message":"y"}}"#.utf8)
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

        for name in ["daemon-incompatible", "app-mutation-unresolved"] {
            let value = try JSONSerialization.jsonObject(with: FixtureStore.data(named: name))
            #expect(value is [String: Any])
        }
    }

    @Test("envelope integers are exact, not coerced")
    func exactIntegerEnvelope() throws {
        for invalid in [
            #"{"schema_version":3.0,"request_id":1,"ok":true,"payload":{"type":"status","data":{}}}"#,
            #"{"schema_version":3,"request_id":1.5,"ok":true,"payload":{"type":"status","data":{}}}"#,
            #"{"schema_version":"3","request_id":1,"ok":true,"payload":{"type":"status","data":{}}}"#
        ] {
            #expect(throws: ClientError.self) {
                try ResponseDecoder.decode(
                    PublicStatus.self,
                    expectedPayloadType: "status",
                    requestID: 1,
                    line: Data(invalid.utf8)
                )
            }
        }
    }

    @Test("missing required v3 DTO facts are rejected")
    func requirednessIsStrict() throws {
        let incomplete = Data(#"{"schema_version":3,"request_id":1,"ok":true,"payload":{"type":"incidents","data":{"freshness":"current","items":[{"incident_id":"i","observation":{"family":"x"}}]}}}"#.utf8)
        #expect(throws: ClientError.self) {
            try ResponseDecoder.decode(
                ObservationRoster.self,
                expectedPayloadType: "incidents",
                requestID: 1,
                line: incomplete
            )
        }
    }

    private static func requestID(in data: Data) -> UInt64? {
        ((try? JSONSerialization.jsonObject(with: data)) as? [String: Any])
            .flatMap { $0["request_id"] as? NSNumber }?
            .uint64Value
    }
}
