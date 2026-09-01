import Foundation
import Testing
@testable import UnlingerKit

@Suite("Status mapping")
struct StatusMappingTests {
    private func status(_ fixture: String) async throws -> PublicStatus {
        try await FixtureClient(statusFixture: fixture).status()
    }

    @Test("all-clear + report-only is quiet and says cleanup is off")
    func allClear() async throws {
        let vm = StatusMapper.viewModel(
            for: try await status("status-all-clear"),
            rosterFreshness: .current
        )
        #expect(vm.tone == .quiet)
        #expect(vm.headlineKey == "status.headline.quiet")
        #expect(vm.modeIsReportOnly)
        #expect(vm.attention.isEmpty)
    }

    @Test("scanning is transient activity, not a warning")
    func scanning() async throws {
        let vm = StatusMapper.viewModel(
            for: try await status("status-scanning"),
            rosterFreshness: .scanInProgress
        )
        #expect(vm.tone == .activity)
        #expect(vm.headlineKey == "status.headline.activity")
    }

    @Test("paused shows the deadline")
    func paused() async throws {
        let vm = StatusMapper.viewModel(
            for: try await status("status-paused"),
            rosterFreshness: .current
        )
        #expect(vm.isPaused)
        #expect(vm.pausedUntil != nil)
        #expect(!vm.modeIsReportOnly) // fixture is enforce mode
    }

    @Test("needs-attention maps kinds to copy keys with generic fallback")
    func needsAttention() async throws {
        let vm = StatusMapper.viewModel(
            for: try await status("status-needs-attention"),
            rosterFreshness: .staleAfterFailure
        )
        #expect(vm.tone == .attention)
        #expect(vm.attention.count == 3)
        #expect(vm.attention.map(\.copyKey).contains("attention.event_source"))
        #expect(vm.attention.map(\.copyKey).contains("attention.daemon_unhealthy"))
        #expect(vm.attention.map(\.copyKey).contains("attention.residue"))
        #expect(vm.detailKey == "status.detail.event_source_degraded")
    }

    @Test("recent reclaim is expressed by overall outcome")
    func recentlyReclaimed() async throws {
        let vm = StatusMapper.viewModel(
            for: try await status("status-recently-reclaimed"),
            rosterFreshness: .current
        )
        #expect(vm.recentReclaim?.copyKey == "reclaim.cleared")
    }

    @Test("starting and draining never map to quiet")
    func transitionalReadinessNotQuiet() async throws {
        for fixture in ["status-starting", "status-draining"] {
            let vm = StatusMapper.viewModel(
                for: try await status(fixture),
                rosterFreshness: .current
            )
            #expect(vm.tone != .quiet)
        }
    }

    @Test("a stale roster prevents quiet even when status is healthy")
    func staleRosterNotQuiet() async throws {
        let vm = StatusMapper.viewModel(
            for: try await status("status-all-clear"),
            rosterFreshness: .staleAfterFailure
        )
        #expect(vm.tone == .attention)
    }

    @Test("cleared_with_residue never maps to a process-failure copy key")
    func residueNotFailure() {
        let reclaim = ReclaimSummary(
            eventToken: "event-x",
            incidentId: "x",
            occurredAtUnixMillis: 1,
            processOutcome: .cleared,
            artifactOutcome: .residue,
            overallOutcome: .clearedWithResidue
        )
        let data = StatusMapper.reclaimViewData(for: reclaim)
        #expect(data.copyKey == "reclaim.cleared_with_residue")
        #expect(data.copyKey != "reclaim.failed")
    }

    @Test("unknown attention kind falls back generically")
    func unknownKindFallback() {
        let item = AttentionItem(
            eventToken: nil,
            kind: .unknown("future_kind"),
            reasonId: "future.reason",
            incidentId: nil,
            overallOutcome: nil,
            occurredAtUnixMillis: nil
        )
        #expect(StatusMapper.attentionViewData(for: item).copyKey == "attention.generic")
    }

    @Test("unknown outcome wire values decode as unknown, not crash")
    func unknownOutcome() throws {
        let json = Data(#"{"event_token":"event-i","incident_id":"i","occurred_at_unix_millis":1,"process_outcome":"cleared","artifact_outcome":"reconciled","overall_outcome":"teleported"}"#.utf8)
        let summary = try JSONDecoder().decode(ReclaimSummary.self, from: json)
        #expect(summary.overallOutcome == .unknown("teleported"))
    }
}
