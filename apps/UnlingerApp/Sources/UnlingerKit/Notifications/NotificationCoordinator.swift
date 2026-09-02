import Foundation

private struct NotificationCandidate: Sendable {
    var eventToken: String
    var titleKey: String
    var bodyKey: String
    var route: NotificationRoute
    var attentionEligible: Bool
}

/// Converts trusted public projections into a quiet best-effort local notification
/// stream. Durable claim always precedes scheduling; a failed schedule is
/// terminal for that key and is never retried automatically.
public actor NotificationCoordinator: NotificationCoordinating {
    public static let maximumLedgerEntries = 512
    public static let retentionMillis: UInt64 = 30 * 24 * 60 * 60 * 1_000
    public static let unavailableFailureThreshold = 3
    public static let unavailableDurationThresholdMillis: UInt64 = 15_000

    private let scheduler: any NotificationScheduling
    private let ledger: any NotificationLedgerStore
    private var mode: NotificationMode
    private var loadedState: NotificationLedgerState?

    public init(
        scheduler: any NotificationScheduling,
        ledger: any NotificationLedgerStore = FileNotificationLedgerStore(),
        mode: NotificationMode = .attention
    ) {
        self.scheduler = scheduler
        self.ledger = ledger
        self.mode = mode
    }

    public func setMode(_ mode: NotificationMode) async {
        self.mode = mode
    }

    public func prepareAuthorization() async -> NotificationAuthorization {
        guard var state = await loadState() else { return .unavailable }
        var authorization = await scheduler.currentAuthorization()
        if mode != .off, authorization == .notDetermined, !state.promptAttempted {
            state.promptAttempted = true
            guard await persist(state) else { return .unavailable }
            do {
                authorization = try await scheduler.requestAuthorization()
            } catch {
                authorization = .unavailable
            }
        }
        // Current OS state is authoritative. The stored value is only a UI
        // summary/history and never overrides this observation.
        state.authorizationSummary = authorization
        _ = await persist(state)
        return authorization
    }

    public func receiveTrustedRefresh(
        status: PublicStatus,
        history: [HistoryEvent],
        atUnixMillis now: UInt64
    ) async {
        guard var state = await loadState() else { return }
        state.unavailableEpisode = nil
        prune(&state, now: now)

        let candidates = eventCandidates(status: status, history: history)
        let retainedTokens = Set(
            history.map(\.eventToken)
                + status.attention.items.compactMap(\.eventToken)
                + [status.mostRecentReclaim?.eventToken].compactMap { $0 }
                + [status.storage.lastRecovery?.recoveryToken].compactMap { $0 }
        )
        var pending: [LocalNotification] = []

        if !state.baselineComplete {
            for token in retainedTokens { state.seenEventTokens[token] = now }
            state.baselineComplete = true
        } else {
            for token in retainedTokens where state.seenEventTokens[token] == nil {
                state.seenEventTokens[token] = now
                guard let candidate = candidates[token] else { continue }
                let claimKey = "event:\(token)"
                let eligible = mode == .attentionAndReclaims
                    || (mode == .attention && candidate.attentionEligible)
                state.claims[claimKey] = NotificationClaim(
                    claimedAtUnixMillis: now,
                    disposition: eligible ? .claimed : .suppressed
                )
                if eligible {
                    pending.append(LocalNotification(
                        identifier: "unlinger.event.\(candidate.eventToken)",
                        titleKey: candidate.titleKey,
                        bodyKey: candidate.bodyKey,
                        route: candidate.route,
                        eventToken: candidate.eventToken
                    ))
                }
            }
        }

        pending += transitionHealthEpisodes(status: status, state: &state, now: now)
        guard await persist(state) else { return }
        await schedule(pending, state: &state)
    }

    public func receiveUnavailable(atUnixMillis now: UInt64) async {
        guard var state = await loadState() else { return }
        prune(&state, now: now)
        if state.unavailableEpisode == nil {
            state.unavailableEpisode = UnavailableNotificationEpisode(
                firstFailureAtUnixMillis: now,
                consecutiveFailures: 1,
                handled: false,
                claimKey: nil
            )
        } else {
            state.unavailableEpisode?.consecutiveFailures += 1
        }

        guard var episode = state.unavailableEpisode else { return }
        let elapsed = now &- episode.firstFailureAtUnixMillis
        var pending: [LocalNotification] = []
        if !episode.handled,
           episode.consecutiveFailures >= Self.unavailableFailureThreshold,
           elapsed >= Self.unavailableDurationThresholdMillis
        {
            let claimKey = "episode:daemon_unavailable:\(episode.firstFailureAtUnixMillis)"
            episode.handled = true
            episode.claimKey = claimKey
            let eligible = mode != .off
            state.claims[claimKey] = NotificationClaim(
                claimedAtUnixMillis: now,
                disposition: eligible ? .claimed : .suppressed
            )
            if eligible {
                pending.append(LocalNotification(
                    identifier: "unlinger.\(claimKey)",
                    titleKey: "notification.title.attention",
                    bodyKey: "notification.daemon_unavailable",
                    route: .status,
                    eventToken: nil
                ))
            }
            state.unavailableEpisode = episode
        }
        guard await persist(state) else { return }
        await schedule(pending, state: &state)
    }

    private func loadState() async -> NotificationLedgerState? {
        if let loadedState { return loadedState }
        do {
            let state = try await ledger.load()
            loadedState = state
            return state
        } catch {
            return nil
        }
    }

    @discardableResult
    private func persist(_ state: NotificationLedgerState) async -> Bool {
        do {
            try await ledger.save(state)
            loadedState = state
            return true
        } catch {
            return false
        }
    }

    private func transitionHealthEpisodes(
        status: PublicStatus,
        state: inout NotificationLedgerState,
        now: UInt64
    ) -> [LocalNotification] {
        var pending: [LocalNotification] = []
        transitionEpisode(
            type: "daemon_failed",
            active: !status.healthy || status.readiness == .failed,
            bodyKey: "notification.daemon_failed",
            state: &state,
            now: now,
            pending: &pending
        )
        transitionEpisode(
            type: "event_source_degraded",
            active: !status.eventSource.healthy,
            bodyKey: "notification.event_source_degraded",
            state: &state,
            now: now,
            pending: &pending
        )
        return pending
    }

    private func transitionEpisode(
        type: String,
        active: Bool,
        bodyKey: String,
        state: inout NotificationLedgerState,
        now: UInt64,
        pending: inout [LocalNotification]
    ) {
        if !active {
            state.activeEpisodes[type] = nil
            return
        }
        guard state.activeEpisodes[type] == nil else { return }
        let claimKey = "episode:\(type):\(now)"
        state.activeEpisodes[type] = claimKey
        let eligible = mode != .off
        state.claims[claimKey] = NotificationClaim(
            claimedAtUnixMillis: now,
            disposition: eligible ? .claimed : .suppressed
        )
        guard eligible else { return }
        pending.append(LocalNotification(
            identifier: "unlinger.\(claimKey)",
            titleKey: "notification.title.attention",
            bodyKey: bodyKey,
            route: .status,
            eventToken: nil
        ))
    }

    private func schedule(
        _ notifications: [LocalNotification],
        state: inout NotificationLedgerState
    ) async {
        for notification in notifications {
            let claimKey = notification.eventToken.map { "event:\($0)" }
                ?? String(notification.identifier.dropFirst("unlinger.".count))
            do {
                try await scheduler.schedule(notification)
                state.claims[claimKey]?.disposition = .scheduled
            } catch {
                state.claims[claimKey]?.disposition = .failedToSchedule
            }
            _ = await persist(state)
        }
    }

    private func eventCandidates(
        status: PublicStatus,
        history: [HistoryEvent]
    ) -> [String: NotificationCandidate] {
        var candidates: [String: NotificationCandidate] = [:]
        for event in history {
            guard case .cleanup(let cleanup) = event.payload else { continue }
            let attentionEligible = cleanup.overallOutcome == .failed
                || cleanup.overallOutcome == .revived
                || cleanup.processOutcome == .deliveryUnknown
            let bodyKey: String = switch cleanup.overallOutcome {
            case .failed where cleanup.processOutcome == .deliveryUnknown:
                "notification.cleanup_delivery_unknown"
            case .failed: "notification.cleanup_failed"
            case .revived: "notification.cleanup_revived"
            case .clearedWithResidue: "notification.cleared_with_residue"
            case .cleared: "notification.reclaimed"
            case .unknown: "notification.cleanup_failed"
            }
            candidates[event.eventToken] = NotificationCandidate(
                eventToken: event.eventToken,
                titleKey: attentionEligible
                    ? "notification.title.attention"
                    : "notification.title.reclaim",
                bodyKey: bodyKey,
                route: .incident(event.incidentId),
                attentionEligible: attentionEligible
            )
        }
        for item in status.attention.items {
            guard let token = item.eventToken else { continue }
            let bodyKey: String = switch item.kind {
            case .cleanupFailed: "notification.cleanup_failed"
            case .cleanupRevived: "notification.cleanup_revived"
            case .cleanupResidue: "notification.cleared_with_residue"
            case .storageRecovered: "notification.storage_recovered"
            case .daemonUnhealthy: "notification.daemon_failed"
            case .eventSourceDegraded: "notification.event_source_degraded"
            case .unknown: "notification.generic_attention"
            }
            let attentionEligible = item.kind != .cleanupResidue
            candidates[token] = NotificationCandidate(
                eventToken: token,
                titleKey: "notification.title.attention",
                bodyKey: bodyKey,
                route: item.incidentId.map(NotificationRoute.incident) ?? .status,
                attentionEligible: attentionEligible
            )
        }
        if let recovery = status.storage.lastRecovery {
            candidates[recovery.recoveryToken] = NotificationCandidate(
                eventToken: recovery.recoveryToken,
                titleKey: "notification.title.attention",
                bodyKey: "notification.storage_recovered",
                route: .status,
                attentionEligible: true
            )
        }
        if let reclaim = status.mostRecentReclaim, candidates[reclaim.eventToken] == nil {
            let attentionEligible = reclaim.overallOutcome == .failed
                || reclaim.overallOutcome == .revived
            candidates[reclaim.eventToken] = NotificationCandidate(
                eventToken: reclaim.eventToken,
                titleKey: attentionEligible
                    ? "notification.title.attention"
                    : "notification.title.reclaim",
                bodyKey: reclaim.overallOutcome == .clearedWithResidue
                    ? "notification.cleared_with_residue"
                    : "notification.reclaimed",
                route: .incident(reclaim.incidentId),
                attentionEligible: attentionEligible
            )
        }
        return candidates
    }

    private func prune(_ state: inout NotificationLedgerState, now: UInt64) {
        let cutoff = now > Self.retentionMillis ? now - Self.retentionMillis : 0
        state.seenEventTokens = bounded(
            state.seenEventTokens.filter { $0.value >= cutoff }
        )
        state.claims = bounded(
            state.claims.filter { $0.value.claimedAtUnixMillis >= cutoff },
            timestamp: { $0.value.claimedAtUnixMillis }
        )
    }

    private func bounded(_ values: [String: UInt64]) -> [String: UInt64] {
        Dictionary(
            uniqueKeysWithValues: values
                .sorted { $0.value > $1.value }
                .prefix(Self.maximumLedgerEntries)
                .map { ($0.key, $0.value) }
        )
    }

    private func bounded<T>(
        _ values: [String: T],
        timestamp: (Dictionary<String, T>.Element) -> UInt64
    ) -> [String: T] {
        Dictionary(
            uniqueKeysWithValues: values
                .sorted { timestamp($0) > timestamp($1) }
                .prefix(Self.maximumLedgerEntries)
                .map { ($0.key, $0.value) }
        )
    }
}
