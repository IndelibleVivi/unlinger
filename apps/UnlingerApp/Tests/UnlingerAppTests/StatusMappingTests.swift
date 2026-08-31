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
        let vm = StatusMapper.viewModel(for: try await status("status-all-clear"))
        #expect(vm.tone == .quiet)
        #expect(vm.headlineKey == "status.headline.quiet")
        #expect(vm.modeIsReportOnly)
        #expect(vm.attention.isEmpty)
    }

    @Test("scanning is transient activity, not a warning")
    func scanning() async throws {
        let vm = StatusMapper.viewModel(for: try await status("status-scanning"))
        #expect(vm.tone == .activity)
        #expect(vm.headlineKey == "status.headline.activity")
    }

    @Test("paused shows the deadline")
    func paused() async throws {
        let vm = StatusMapper.viewModel(for: try await status("status-paused"))
        #expect(vm.isPaused)
        #expect(vm.pausedUntil != nil)
        #expect(!vm.modeIsReportOnly) // fixture is enforce mode
    }

    @Test("needs-attention maps kinds to copy keys with generic fallback")
    func needsAttention() async throws {
        let vm = StatusMapper.viewModel(for: try await status("status-needs-attention"))
        #expect(vm.tone == .attention)
        #expect(vm.attention.count == 2)
        #expect(vm.attention.map(\.copyKey).contains("attention.daemon_unhealthy"))
        #expect(vm.attention.map(\.copyKey).contains("attention.residue"))
        #expect(vm.detailKey == "status.detail.event_source_degraded")
    }

    @Test("recent reclaim is expressed by overall outcome")
    func recentlyReclaimed() async throws {
        let vm = StatusMapper.viewModel(for: try await status("status-recently-reclaimed"))
        #expect(vm.recentReclaim?.copyKey == "reclaim.cleared")
    }

    @Test("cleared_with_residue never maps to a process-failure copy key")
    func residueNotFailure() {
        let reclaim = ReclaimSummary(
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
        let item = AttentionItem(kind: .unknown("future_kind"), reasonId: "future.reason")
        #expect(StatusMapper.attentionViewData(for: item).copyKey == "attention.generic")
    }

    @Test("unknown outcome wire values decode as unknown, not crash")
    func unknownOutcome() throws {
        let json = Data(#"{"incident_id":"i","occurred_at_unix_millis":1,"process_outcome":"cleared","artifact_outcome":"reconciled","overall_outcome":"teleported"}"#.utf8)
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        let summary = try decoder.decode(ReclaimSummary.self, from: json)
        #expect(summary.overallOutcome == .unknown("teleported"))
    }
}
