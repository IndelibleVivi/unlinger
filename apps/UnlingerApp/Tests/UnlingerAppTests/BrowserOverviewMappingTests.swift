import Foundation
import Testing
@testable import UnlingerKit

@Suite("Schema-v5 browser overview presentation")
struct BrowserOverviewMappingTests {
    @Test("Swift preserves the daemon phase instead of recomputing it")
    func preservesDaemonPhase() async throws {
        var snapshot = try await BrowserFixtureClient(
            scenario: .confirmedReportOnly
        ).browserOverview()
        snapshot.phase = .clear

        let overview = BrowserOverviewMapper.make(connection: .live, snapshot: snapshot)

        #expect(overview.phase == .clear)
        #expect(overview.sessions.first?.state == .confirmed)
        #expect(overview.snapshotTrusted)
        #expect(!overview.requiresTrailingRefresh)
    }

    @Test("transport loss stays App-local unknown while retaining last rows")
    func transportLossOverridesPhase() async throws {
        let snapshot = try await BrowserFixtureClient(scenario: .active).browserOverview()

        let overview = BrowserOverviewMapper.make(
            connection: .unavailable,
            snapshot: snapshot
        )

        #expect(overview.phase == .unknown)
        #expect(!overview.snapshotTrusted)
        #expect(overview.sessions.count == 1)
        #expect(overview.headlineKey == "browser.overview.unavailable")
    }

    @Test("transport loss keeps local settings without exposing stale history or actions")
    func transportLossKeepsOnlyLocalSettings() async throws {
        let snapshot = try await BrowserFixtureClient(scenario: .active).browserOverview()
        let overview = BrowserOverviewMapper.make(
            connection: .unavailable,
            snapshot: snapshot
        )

        let sections = overview.visibleSections(connection: .unavailable)

        #expect(sections.contains(.settings))
        #expect(!sections.contains(.history))
        #expect(!sections.contains(.actions))
    }

    @Test("typed compatibility reason selects coverage copy without evidence scanning")
    func typedCoverage() async throws {
        let snapshot = try await BrowserFixtureClient(
            scenario: .protectedUnsupported
        ).browserOverview()

        let overview = BrowserOverviewMapper.make(connection: .live, snapshot: snapshot)

        #expect(overview.coverageNotices == [.unsupportedVersion])
        #expect(overview.sessions.first?.coverageNotice == .unsupportedVersion)
        #expect(overview.sessions.first?.reasonKey == "browser.coverage.unsupported_version")
        #expect(overview.sessions.first?.productKey == "browser.product.chrome_for_testing")
        #expect(overview.sessions.first?.observedVersion == "151.0.7922.35")
    }

    @Test("unknown typed reason stays generic and never grants a stronger phase")
    func unknownCoverageReason() async throws {
        var snapshot = try await BrowserFixtureClient(
            scenario: .protectedUnsupported
        ).browserOverview()
        snapshot.coverageNotices[0].reasonId = "future.coverage.reason"
        snapshot.sessions[0].compatibility.reasonId = "future.coverage.reason"

        let overview = BrowserOverviewMapper.make(connection: .live, snapshot: snapshot)

        #expect(overview.phase == .protected)
        #expect(overview.coverageNotices == [.unknown(reasonID: "future.coverage.reason")])
        #expect(overview.sessions.first?.reasonKey == "browser.coverage.generic")
    }

    @Test("recent settlement is a direct atomic DTO projection")
    func directSettlement() async throws {
        let snapshot = try await BrowserFixtureClient(
            scenario: .recentSettlement
        ).browserOverview()

        let settlement = try #require(
            BrowserOverviewMapper.make(connection: .live, snapshot: snapshot).recentSettlement
        )

        #expect(settlement.familyKey == "browser.family.chrome_for_testing")
        #expect(settlement.processCount == 8)
        #expect(settlement.revivalChecksCompleted == 2)
        #expect(!settlement.isFallback)
    }

    @Test("impact and storage residue remain typed presentation facts")
    func impactAndStorageResidue() throws {
        let data = try FixtureStore.data(
            named: "browser-overview-impact-residue",
            schemaVersion: 5
        )
        let snapshot = try ResponseDecoder.decode(
            BrowserOverviewSnapshot.self,
            expectedPayloadType: "browser_overview",
            requestID: 502,
            expectedSchemaVersion: 5,
            line: data
        )

        let overview = BrowserOverviewMapper.make(connection: .live, snapshot: snapshot)

        #expect(overview.impact?.provedReclaimCount == 2)
        #expect(overview.impact?.historicalCompleteness == .partialBackfill)
        #expect(overview.storageResidue?.status == .detected)
        #expect(overview.storageResidue?.candidateCount == 49)
        #expect(overview.storageResidue?.automaticCleanupEligible == false)
        #expect(overview.storageCleanupResult?.disposition == .partial)
        #expect(overview.storageCleanupResult?.plannedCandidateCount == 8)
        #expect(overview.storageCleanupResult?.removedCandidateCount == 8)
        #expect(overview.storageCleanupResult?.afterCandidateCount == 1)
        #expect(overview.visibleSections(connection: .live).contains(.impact))
        #expect(overview.visibleSections(connection: .live).contains(.storageResidue))
    }

    @Test("an absent storage residue projection does not create an empty section")
    func absentStorageResidueHasNoSection() async throws {
        let snapshot = try await BrowserFixtureClient(scenario: .clear).browserOverview()
        let overview = BrowserOverviewMapper.make(connection: .live, snapshot: snapshot)

        #expect(overview.storageResidue == nil)
        #expect(!overview.visibleSections(connection: .live).contains(.storageResidue))
    }

    @Test("a terminal cleanup result keeps the storage section visible after residue clears")
    func cleanupResultKeepsClearStorageSectionVisible() async throws {
        var snapshot = try await BrowserFixtureClient(scenario: .clear).browserOverview()
        snapshot.storageResidue = StorageResidueSummary(
            kind: .chromeCodeSignClone,
            status: .clear,
            observedAtUnixMillis: 2_000,
            candidateCount: 0,
            logicalBytes: 0,
            shapeComplete: true,
            referenceCheck: .completeNoReferences,
            automaticCleanupEligible: false,
            reasonIds: []
        )
        snapshot.storageCleanupResult = StorageCleanupResultSummary(
            disposition: .complete,
            preparedAtUnixMillis: 1_000,
            completedAtUnixMillis: 2_000,
            plannedCandidateCount: 2,
            beforeCandidateCount: 2,
            beforeLogicalBytes: 4_096,
            removedCandidateCount: 2,
            afterCandidateCount: 0,
            afterLogicalBytes: 0,
            retainedNotPlannedCount: 0
        )

        let overview = BrowserOverviewMapper.make(connection: .live, snapshot: snapshot)

        #expect(overview.storageResidue?.status == .clear)
        #expect(overview.storageCleanupResult?.disposition == .complete)
        #expect(overview.visibleSections(connection: .live).contains(.storageResidue))
    }

    @Test(
        "every storage cleanup disposition maps to stable presentation copy",
        arguments: [
            (
                StorageCleanupDisposition.complete,
                "browser.storage_cleanup.complete",
                "externaldrive.badge.checkmark"
            ),
            (
                StorageCleanupDisposition.partial,
                "browser.storage_cleanup.partial",
                "externaldrive.badge.exclamationmark"
            ),
            (
                StorageCleanupDisposition.failed,
                "browser.storage_cleanup.failed",
                "exclamationmark.triangle"
            ),
            (
                StorageCleanupDisposition.deliveryUnknown,
                "browser.storage_cleanup.delivery_unknown",
                "exclamationmark.triangle"
            )
        ]
    )
    func storageCleanupDispositionCopy(
        disposition: StorageCleanupDisposition,
        expectedKey: String,
        expectedSystemImage: String
    ) {
        let presentation = StorageCleanupResultPresentation(
            disposition: disposition,
            preparedAt: Date(timeIntervalSince1970: 1),
            completedAt: nil,
            plannedCandidateCount: 1,
            beforeCandidateCount: 1,
            beforeLogicalBytes: 1,
            removedCandidateCount: nil,
            afterCandidateCount: nil,
            afterLogicalBytes: nil,
            retainedNotPlannedCount: nil
        )

        #expect(presentation.outcomeKey == expectedKey)
        #expect(presentation.systemImageName == expectedSystemImage)
    }
}
