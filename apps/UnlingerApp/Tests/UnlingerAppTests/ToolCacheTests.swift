import Foundation
import Testing
@testable import UnlingerKit

struct ToolCacheTests {
    @Test("new cache fixture maps separately and old v5 remains compatible")
    func fixtureAndCompatibility() async throws {
        let data = try FixtureStore.data(named: "browser-overview-cache-maintenance", schemaVersion: 5)
        let snapshot = try ResponseDecoder.decode(BrowserOverviewSnapshot.self,
            expectedPayloadType: "browser_overview", requestID: 505, expectedSchemaVersion: 5, line: data)
        let mapped = BrowserOverviewMapper.make(connection: .live, snapshot: snapshot)
        #expect(mapped.toolCacheMaintenance?.nativeRemovedEntryCount == 2)
        #expect(mapped.toolCacheMaintenance?.nativeRemovedLogicalBytes == 4096)
        #expect(mapped.visibleSections(connection: .live).contains(.toolCacheMaintenance))
        let old = try await BrowserFixtureClient(scenario: .clear).browserOverview()
        #expect(old.toolCacheMaintenance == nil)
        #expect(!BrowserOverviewMapper.make(connection: .live, snapshot: old).visibleSections(connection: .live).contains(.toolCacheMaintenance))
    }

    @Test("uncertain cache results never turn into success copy or inferred reclaim")
    func unknownIsNotSuccess() async throws {
        var snapshot = try await BrowserFixtureClient(scenario: .clear).browserOverview()
        snapshot.toolCacheMaintenance = ToolCacheMaintenanceSummary(kind: .npmDownloadCache,
            observedAtUnixMillis: 3000, availability: .available, automaticMaintenanceEligible: false,
            lastAttempt: ToolCacheAttemptSummary(outcome: .deliveryUnknown, preparedAtUnixMillis: 1000,
                completedAtUnixMillis: 2000, nativeRemovedEntryCount: nil, nativeRemovedLogicalBytes: nil))
        let mapped = BrowserOverviewMapper.make(connection: .live, snapshot: snapshot)
        #expect(mapped.toolCacheMaintenance?.outcomeKey == "cache.maintenance.delivery_unknown")
        #expect(mapped.toolCacheMaintenance?.nativeRemovedLogicalBytes == nil)
        #expect(mapped.toolCacheMaintenance?.attemptedAt == Date(unixMillis: 2000))
        #expect(mapped.toolCacheMaintenance?.observedAt == Date(unixMillis: 3000))
    }
    @Test("uv busy is a separate native result and preserves browser impact")
    func uvBusyAndOldCompatibility() async throws {
        let data = try FixtureStore.data(named: "browser-overview-uv-cache-maintenance", schemaVersion: 5)
        var snapshot = try ResponseDecoder.decode(BrowserOverviewSnapshot.self,
            expectedPayloadType: "browser_overview", requestID: 505, expectedSchemaVersion: 5, line: data)
        #expect(snapshot.toolCacheMaintenance == nil)
        var mapped = BrowserOverviewMapper.make(connection: .live, snapshot: snapshot)
        #expect(mapped.uvCacheMaintenance?.nativeRemovedEntryCount == 2)
        #expect(mapped.uvCacheMaintenance?.titleKey == "cache.uv.title")
        #expect(mapped.uvCacheMaintenance?.availabilityKey == "cache.uv.enabled")
        #expect(mapped.uvCacheMaintenance?.accountingKey == "cache.uv.estimate")
        #expect(mapped.visibleSections(connection: .live).contains(.uvCacheMaintenance))
        let previousImpact = mapped.impact
        snapshot.uvCacheMaintenance?.lastAttempt?.nativeRemovedLogicalBytes = nil
        mapped = BrowserOverviewMapper.make(connection: .live, snapshot: snapshot)
        #expect(mapped.uvCacheMaintenance?.nativeRemovedEntryCount == 2)
        #expect(mapped.uvCacheMaintenance?.nativeRemovedLogicalBytes == nil)
        snapshot.uvCacheMaintenance?.lastAttempt?.outcome = .busy
        snapshot.uvCacheMaintenance?.lastAttempt?.nativeRemovedEntryCount = nil
        snapshot.uvCacheMaintenance?.lastAttempt?.nativeRemovedLogicalBytes = nil
        mapped = BrowserOverviewMapper.make(connection: .live, snapshot: snapshot)
        #expect(mapped.uvCacheMaintenance?.outcomeKey == "cache.maintenance.busy")
        #expect(mapped.uvCacheMaintenance?.nativeRemovedLogicalBytes == nil)
        #expect(mapped.impact == previousImpact)
        #expect(mapped.phase == .clear)
    }
}
