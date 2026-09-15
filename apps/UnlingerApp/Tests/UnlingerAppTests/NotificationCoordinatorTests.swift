import Foundation
import Testing
@testable import UnlingerKit

private actor MemoryNotificationLedger: NotificationLedgerStore {
    private var state: NotificationLedgerState

    init(state: NotificationLedgerState = NotificationLedgerState()) {
        self.state = state
    }

    func load() async throws -> NotificationLedgerState { state }
    func save(_ state: NotificationLedgerState) async throws { self.state = state }
    func snapshot() -> NotificationLedgerState { state }
}

private actor FakeNotificationScheduler: NotificationScheduling {
    private(set) var scheduled: [LocalNotification] = []
    private(set) var authorizationRequests = 0
    private var authorization: NotificationAuthorization
    private var requestResult: NotificationAuthorization

    init(
        authorization: NotificationAuthorization = .authorized,
        requestResult: NotificationAuthorization = .authorized
    ) {
        self.authorization = authorization
        self.requestResult = requestResult
    }

    func currentAuthorization() async -> NotificationAuthorization { authorization }

    func requestAuthorization() async throws -> NotificationAuthorization {
        authorizationRequests += 1
        authorization = requestResult
        return requestResult
    }

    func schedule(_ notification: LocalNotification) async throws {
        scheduled.append(notification)
    }
}

@Suite("Notification baseline, dedupe, and episodes")
struct NotificationCoordinatorTests {
    private func status(_ name: String = "status-all-clear") async throws -> PublicStatus {
        try await FixtureClient(statusFixture: name).status()
    }

    private func history(_ name: String) async throws -> [HistoryEvent] {
        try await FixtureClient(
            statusFixture: "status-all-clear",
            historyFixture: name
        ).history(limit: 50)
    }

    @Test("first trusted refresh baselines retained events")
    func firstRefreshBaselines() async throws {
        let scheduler = FakeNotificationScheduler()
        let ledger = MemoryNotificationLedger()
        let coordinator = NotificationCoordinator(
            scheduler: scheduler,
            ledger: ledger,
            mode: .attention
        )
        let retained = try await history("history-cleared-with-residue")

        await coordinator.receiveTrustedRefresh(
            status: try await status(),
            history: retained,
            atUnixMillis: 100
        )

        #expect(await scheduler.scheduled.isEmpty)
        #expect(await ledger.snapshot().seenEventTokens[retained[0].eventToken] == 100)
    }

    @Test("attention event sends once and routes to the exact incident")
    func attentionDeduplicates() async throws {
        let scheduler = FakeNotificationScheduler()
        let ledger = MemoryNotificationLedger()
        let coordinator = NotificationCoordinator(
            scheduler: scheduler,
            ledger: ledger,
            mode: .attention
        )
        let failed = try await history("history-cleared-with-residue").map { event in
            var event = event
            event.eventToken = "new-failed-event"
            if case .cleanup(var cleanup) = event.payload {
                cleanup.state = .failed
                cleanup.processOutcome = .failed
                cleanup.artifactOutcome = .notApplicable
                cleanup.overallOutcome = .failed
                event.payload = .cleanup(cleanup)
            }
            return event
        }
        await coordinator.receiveTrustedRefresh(
            status: try await status(),
            history: [],
            atUnixMillis: 1
        )

        await coordinator.receiveTrustedRefresh(
            status: try await status(),
            history: failed,
            atUnixMillis: 2
        )
        await coordinator.receiveTrustedRefresh(
            status: try await status(),
            history: failed,
            atUnixMillis: 3
        )

        let notices = await scheduler.scheduled
        #expect(notices.count == 1)
        #expect(notices[0].route == .incident(failed[0].incidentId))
        #expect(notices[0].eventToken == "new-failed-event")
    }

    @Test("restart does not replay a seen event")
    func restartDoesNotReplay() async throws {
        let firstScheduler = FakeNotificationScheduler()
        let ledger = MemoryNotificationLedger()
        let event = try await history("history-cleared")
        let first = NotificationCoordinator(
            scheduler: firstScheduler,
            ledger: ledger,
            mode: .attentionAndReclaims
        )
        await first.receiveTrustedRefresh(
            status: try await status(),
            history: [],
            atUnixMillis: 1
        )
        await first.receiveTrustedRefresh(
            status: try await status(),
            history: event,
            atUnixMillis: 2
        )
        #expect(await firstScheduler.scheduled.count == 1)

        let restartedScheduler = FakeNotificationScheduler()
        let restarted = NotificationCoordinator(
            scheduler: restartedScheduler,
            ledger: ledger,
            mode: .attentionAndReclaims
        )
        await restarted.receiveTrustedRefresh(
            status: try await status(),
            history: event,
            atUnixMillis: 3
        )
        #expect(await restartedScheduler.scheduled.isEmpty)
    }

    @Test("mode change does not replay a suppressed reclaim")
    func modeChangeNoBacklog() async throws {
        let scheduler = FakeNotificationScheduler()
        let coordinator = NotificationCoordinator(
            scheduler: scheduler,
            ledger: MemoryNotificationLedger(),
            mode: .attention
        )
        let reclaimed = try await history("history-cleared")
        await coordinator.receiveTrustedRefresh(
            status: try await status(),
            history: [],
            atUnixMillis: 1
        )
        await coordinator.receiveTrustedRefresh(
            status: try await status(),
            history: reclaimed,
            atUnixMillis: 2
        )
        await coordinator.setMode(.attentionAndReclaims)
        await coordinator.receiveTrustedRefresh(
            status: try await status(),
            history: reclaimed,
            atUnixMillis: 3
        )
        #expect(await scheduler.scheduled.isEmpty)
    }

    @Test("attention-and-reclaims permits a new successful reclaim")
    func reclaimModePermitsSuccess() async throws {
        let scheduler = FakeNotificationScheduler()
        let coordinator = NotificationCoordinator(
            scheduler: scheduler,
            ledger: MemoryNotificationLedger(),
            mode: .attentionAndReclaims
        )
        await coordinator.receiveTrustedRefresh(
            status: try await status(),
            history: [],
            atUnixMillis: 1
        )
        await coordinator.receiveTrustedRefresh(
            status: try await status(),
            history: try await history("history-cleared"),
            atUnixMillis: 2
        )
        #expect(await scheduler.scheduled.count == 1)
        #expect(await scheduler.scheduled[0].bodyKey == "notification.reclaimed")
    }

    @Test("no-intervention completion does not send a reclaim notification or replay later")
    func noInterventionDoesNotNotify() async throws {
        let scheduler = FakeNotificationScheduler()
        let ledger = MemoryNotificationLedger()
        let coordinator = NotificationCoordinator(
            scheduler: scheduler, ledger: ledger, mode: .attentionAndReclaims
        )
        let completed = try await history("history-cleared").map { source in
            var event = source
            if case .cleanup(var cleanup) = event.payload {
                cleanup.reasonId = "cleanup.tree_gone_without_signal"
                cleanup.artifactOutcome = .notApplicable
                cleanup.artifactActions = []
                cleanup.processActions = []
                cleanup.resources.estimatedReclaimedMemoryBytes = nil
                cleanup.processActions = []
                event.payload = .cleanup(cleanup)
            }
            return event
        }
        await coordinator.receiveTrustedRefresh(status: try await status(), history: [], atUnixMillis: 1)
        await coordinator.receiveTrustedRefresh(status: try await status(), history: completed, atUnixMillis: 2)
        #expect(await scheduler.scheduled.isEmpty)
        #expect(await ledger.snapshot().seenEventTokens[completed[0].eventToken] == 2)
        let restarted = NotificationCoordinator(
            scheduler: scheduler, ledger: ledger, mode: .attentionAndReclaims
        )
        await restarted.receiveTrustedRefresh(status: try await status(), history: completed, atUnixMillis: 3)
        #expect(await scheduler.scheduled.isEmpty)
    }

    @Test("cleared-with-residue copy preserves process success")
    func residueCopy() async throws {
        let scheduler = FakeNotificationScheduler()
        let coordinator = NotificationCoordinator(
            scheduler: scheduler,
            ledger: MemoryNotificationLedger(),
            mode: .attentionAndReclaims
        )
        await coordinator.receiveTrustedRefresh(
            status: try await status(),
            history: [],
            atUnixMillis: 1
        )
        await coordinator.receiveTrustedRefresh(
            status: try await status(),
            history: try await history("history-cleared-with-residue"),
            atUnixMillis: 2
        )
        #expect(await scheduler.scheduled[0].bodyKey == "notification.cleared_with_residue")
    }

    @Test("unavailable requires three failures and fifteen seconds")
    func unavailableThreshold() async throws {
        let scheduler = FakeNotificationScheduler()
        let coordinator = NotificationCoordinator(
            scheduler: scheduler,
            ledger: MemoryNotificationLedger(),
            mode: .attention
        )
        await coordinator.receiveUnavailable(atUnixMillis: 1_000)
        await coordinator.receiveUnavailable(atUnixMillis: 8_500)
        #expect(await scheduler.scheduled.isEmpty)
        await coordinator.receiveUnavailable(atUnixMillis: 16_000)
        #expect(await scheduler.scheduled.count == 1)
        #expect(await scheduler.scheduled[0].bodyKey == "notification.daemon_unavailable")
    }

    @Test("trusted recovery resets the unavailable episode")
    func recoveryResetsEpisode() async throws {
        let scheduler = FakeNotificationScheduler()
        let coordinator = NotificationCoordinator(
            scheduler: scheduler,
            ledger: MemoryNotificationLedger(),
            mode: .attention
        )
        for time in [1_000, 8_500, 16_000] {
            await coordinator.receiveUnavailable(atUnixMillis: UInt64(time))
        }
        await coordinator.receiveTrustedRefresh(
            status: try await status(),
            history: [],
            atUnixMillis: 20_000
        )
        for time in [30_000, 37_500, 45_000] {
            await coordinator.receiveUnavailable(atUnixMillis: UInt64(time))
        }
        #expect(await scheduler.scheduled.count == 2)
    }

    @Test("permission denial is prompted only once")
    func deniedDoesNotReprompt() async throws {
        let scheduler = FakeNotificationScheduler(
            authorization: .notDetermined,
            requestResult: .denied
        )
        let coordinator = NotificationCoordinator(
            scheduler: scheduler,
            ledger: MemoryNotificationLedger(),
            mode: .attention
        )
        #expect(await coordinator.prepareAuthorization() == .denied)
        #expect(await coordinator.prepareAuthorization() == .denied)
        #expect(await scheduler.authorizationRequests == 1)
    }
}
