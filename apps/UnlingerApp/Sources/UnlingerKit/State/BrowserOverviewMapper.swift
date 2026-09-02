import Foundation

/// Pure product projection over frontend schema v3. It translates typed
/// backend facts into browser language without changing backend authority.
public enum BrowserOverviewMapper {
    public static func make(
        connection: ConnectionState,
        status: PublicStatus?,
        roster: ObservationRoster,
        history: [HistoryEvent]
    ) -> BrowserOverview {
        let trusted = snapshotIsTrusted(
            connection: connection,
            status: status,
            roster: roster
        )
        let needsTrailingRefresh = snapshotNeedsTrailingRefresh(
            connection: connection,
            status: status,
            roster: roster
        )
        let rows = sessionPresentations(
            roster: roster,
            mode: status?.effectiveMode,
            snapshotTrusted: trusted
        )
        let attention = status?.attention.items.map(attentionPresentation) ?? []
        let attentionOverflow = max(
            0,
            (status?.attention.totalCount ?? 0) - attention.count
        )
        let phase = productPhase(
            connection: connection,
            status: status,
            roster: roster,
            sessions: rows,
            snapshotTrusted: trusted
        )
        let copy = overviewCopy(
            phase: phase,
            connection: connection,
            status: status,
            roster: roster
        )
        let currentIDs = Set(rows.map(\.incidentID))
        let savedProtections = status?.protection.items.filter {
            !currentIDs.contains($0.incidentId)
        } ?? []

        return BrowserOverview(
            phase: phase,
            tone: tone(for: phase),
            headlineKey: copy.headline,
            detailKey: copy.detail,
            modeKey: modeKey(for: status),
            snapshotTrusted: trusted,
            requiresTrailingRefresh: needsTrailingRefresh,
            observedAt: roster.observedAtUnixMillis.map(Date.init(unixMillis:)),
            pausedUntil: status?.pausedUntilUnixMillis.map(Date.init(unixMillis:)),
            sessions: rows,
            coverageNotices: uniqueCoverageNotices(in: rows),
            attention: attention,
            attentionOverflow: attentionOverflow,
            savedProtections: savedProtections,
            recentSettlement: recentSettlement(status: status, history: history)
        )
    }

    public static func familyKey(for family: String) -> String {
        switch family.lowercased() {
        case "playwright": "browser.family.playwright"
        case "agent-browser": "browser.family.agent_browser"
        case "puppeteer": "browser.family.puppeteer"
        case "chrome-for-testing": "browser.family.chrome_for_testing"
        default: "browser.family.automation"
        }
    }

    public static func detailPresentation(
        incidentID: String,
        currentSessions: [BrowserSessionPresentation],
        events: [HistoryEvent],
        mode: EffectiveMode?
    ) -> BrowserSessionPresentation? {
        if let current = currentSessions.first(where: { $0.incidentID == incidentID }) {
            return current
        }
        guard let latest = events
            .compactMap({ event -> (HistoryEvent, ObservationRecord)? in
                guard event.incidentId == incidentID,
                      case .observation(let observation) = event.payload
                else {
                    return nil
                }
                return (event, observation)
            })
            .max(by: { $0.0.occurredAtUnixMillis < $1.0.occurredAtUnixMillis })?
            .1
        else {
            return nil
        }
        return sessionPresentation(
            incidentID: incidentID,
            observation: latest,
            mode: mode,
            isPreviousObservation: true
        )
    }

    private static func snapshotIsTrusted(
        connection: ConnectionState,
        status: PublicStatus?,
        roster: ObservationRoster
    ) -> Bool {
        guard connection == .live,
              let status,
              status.healthy,
              status.readiness == .ready,
              !status.scanInProgress,
              roster.freshness == .current,
              let statusObservedAt = status.latestObservationAtUnixMillis,
              let rosterObservedAt = roster.observedAtUnixMillis
        else {
            return false
        }
        return statusObservedAt == rosterObservedAt
    }

    private static func snapshotNeedsTrailingRefresh(
        connection: ConnectionState,
        status: PublicStatus?,
        roster: ObservationRoster
    ) -> Bool {
        guard connection == .live,
              let status,
              status.healthy,
              status.readiness == .ready,
              !status.scanInProgress,
              roster.freshness == .current,
              let statusObservedAt = status.latestObservationAtUnixMillis,
              let rosterObservedAt = roster.observedAtUnixMillis
        else {
            return false
        }
        return statusObservedAt != rosterObservedAt
    }

    private static func sessionPresentations(
        roster: ObservationRoster,
        mode: EffectiveMode?,
        snapshotTrusted: Bool
    ) -> [BrowserSessionPresentation] {
        roster.items.map { item in
            sessionPresentation(
                incidentID: item.incidentId,
                observation: item.observation,
                mode: mode,
                isPreviousObservation: !snapshotTrusted
            )
        }
    }

    private static func sessionPresentation(
        incidentID: String,
        observation: ObservationRecord,
        mode: EffectiveMode?,
        isPreviousObservation: Bool
    ) -> BrowserSessionPresentation {
        let notice = coverageNotice(for: observation.evidence)
        return BrowserSessionPresentation(
            incidentID: incidentID,
            familyKey: familyKey(for: observation.family),
            state: observation.state,
            stateKey: stateKey(for: observation.state),
            reasonKey: sessionReasonKey(
                state: observation.state,
                mode: mode,
                coverageNotice: notice
            ),
            memberCount: observation.memberCount,
            residentMemoryBytes: observation.residentMemoryBytes,
            coverageNotice: notice,
            isPreviousObservation: isPreviousObservation
        )
    }

    private static func productPhase(
        connection: ConnectionState,
        status: PublicStatus?,
        roster: ObservationRoster,
        sessions: [BrowserSessionPresentation],
        snapshotTrusted: Bool
    ) -> BrowserOverviewPhase {
        guard connection == .live, let status else { return .unknown }

        // Durable backend attention stays visible even when the current roster
        // is stale or being replaced.
        if !status.healthy || status.readiness == .failed || status.attention.totalCount > 0 {
            return .attention
        }
        guard status.readiness == .ready else { return .unknown }
        guard snapshotTrusted else { return .unknown }

        if status.cleanupInProgress || sessions.contains(where: { $0.state == .reclaiming }) {
            return .reclaiming
        }
        if sessions.contains(where: { $0.state == .failed || $0.state == .revived }) {
            return .attention
        }
        if sessions.contains(where: { stateIsUnknownOrUnexpected($0.state) }) {
            return .unknown
        }
        if sessions.contains(where: { $0.state == .confirmed }) { return .confirmed }
        if sessions.contains(where: { $0.state == .cooling }) { return .verifying }
        if sessions.contains(where: { $0.state == .active }) { return .active }
        if !sessions.isEmpty,
           sessions.allSatisfy({ $0.state == .protected || $0.state == .ambiguous })
        {
            return .protected
        }
        if sessions.isEmpty, roster.freshness == .current { return .clear }
        return .unknown
    }

    private static func stateIsUnknownOrUnexpected(_ state: IncidentState) -> Bool {
        switch state {
        case .unknown, .cleared:
            true
        case .protected, .active, .cooling, .confirmed, .ambiguous, .reclaiming, .revived, .failed:
            false
        }
    }

    private static func overviewCopy(
        phase: BrowserOverviewPhase,
        connection: ConnectionState,
        status: PublicStatus?,
        roster: ObservationRoster
    ) -> (headline: String, detail: String?) {
        switch phase {
        case .clear:
            ("browser.overview.clear", nil)
        case .active:
            ("browser.overview.active", "browser.overview.active.detail")
        case .verifying:
            ("browser.overview.verifying", "browser.overview.verifying.detail")
        case .confirmed:
            if status?.effectiveMode == .reportOnly {
                ("browser.overview.confirmed", "browser.overview.confirmed.report_only")
            } else {
                ("browser.overview.confirmed", "browser.overview.confirmed.enforce")
            }
        case .reclaiming:
            ("browser.overview.reclaiming", "browser.overview.reclaiming.detail")
        case .protected:
            ("browser.overview.protected", "browser.overview.protected.detail")
        case .attention:
            ("browser.overview.attention", "browser.overview.attention.detail")
        case .unknown:
            if connection == .connecting || status?.readiness == .starting ||
                status?.scanInProgress == true || roster.freshness == .scanInProgress ||
                roster.freshness == .neverObserved ||
                snapshotNeedsTrailingRefresh(connection: connection, status: status, roster: roster)
            {
                ("browser.overview.updating", "browser.overview.updating.detail")
            } else {
                ("browser.overview.unavailable", "browser.overview.unavailable.detail")
            }
        }
    }

    private static func tone(for phase: BrowserOverviewPhase) -> BrowserOverviewTone {
        switch phase {
        case .clear, .active, .protected: .quiet
        case .verifying, .confirmed, .reclaiming: .activity
        case .unknown, .attention: .attention
        }
    }

    private static func modeKey(for status: PublicStatus?) -> String {
        if status?.pausedUntilUnixMillis != nil { return "browser.mode.paused" }
        return switch status?.effectiveMode {
        case .reportOnly: "browser.mode.observe_only"
        case .enforce: "browser.mode.auto_cleanup"
        case .unknown, .none: "browser.mode.unknown"
        }
    }

    private static func stateKey(for state: IncidentState) -> String {
        switch state {
        case .active: "browser.session.active"
        case .cooling: "browser.session.verifying"
        case .confirmed: "browser.session.confirmed"
        case .reclaiming: "browser.session.reclaiming"
        case .protected: "browser.session.protected"
        case .ambiguous: "browser.session.ambiguous"
        case .cleared: "browser.session.cleared"
        case .revived: "browser.session.revived"
        case .failed: "browser.session.failed"
        case .unknown: "browser.session.unknown"
        }
    }

    private static func sessionReasonKey(
        state: IncidentState,
        mode: EffectiveMode?,
        coverageNotice: BrowserCoverageNotice?
    ) -> String? {
        if let coverageNotice { return coverageNotice.copyKey }
        return switch state {
        case .protected: "browser.session.reason.protected_generic"
        case .ambiguous: "browser.session.reason.ambiguous_generic"
        case .confirmed where mode == .reportOnly: "browser.session.reason.confirmed_report_only"
        case .failed: "browser.session.reason.failed"
        case .revived: "browser.session.reason.revived"
        case .unknown: "browser.session.reason.unknown"
        default: nil
        }
    }

    private static func coverageNotice(for evidence: [Evidence]) -> BrowserCoverageNotice? {
        let ids = Set(evidence.map(\.id))
        let precedence: [(String, BrowserCoverageNotice)] = [
            ("protection.browser_version_mixed", .mixedVersions),
            ("protection.browser_product_unsupported", .unsupportedProduct),
            ("protection.browser_version_unsupported", .unsupportedVersion),
            ("protection.browser_version_missing", .versionUnavailable),
            ("protection.controller_version_unverified", .controllerUnverified),
            ("protection.version_observational_only", .observationOnly),
            ("protection.debug_peer_visibility_incomplete", .controlPathIncomplete)
        ]
        return precedence.first(where: { ids.contains($0.0) })?.1
    }

    private static func uniqueCoverageNotices(
        in sessions: [BrowserSessionPresentation]
    ) -> [BrowserCoverageNotice] {
        BrowserCoverageNotice.allCases.filter { notice in
            sessions.contains(where: { $0.coverageNotice == notice })
        }
    }

    private static func attentionPresentation(
        _ item: AttentionItem
    ) -> BrowserAttentionPresentation {
        let key = switch (item.kind, item.reasonId) {
        case (.daemonUnhealthy, _):
            "attention.daemon_unhealthy"
        case (.cleanupResidue, _), (.cleanupFailed, "cleanup.artifact_unsafe"):
            "attention.residue"
        case (.cleanupFailed, "cleanup.delivery_unknown"):
            "attention.failed.delivery_unknown"
        case (.cleanupFailed, _):
            "attention.failed.generic"
        case (.cleanupRevived, _):
            "attention.revived"
        case (.eventSourceDegraded, _):
            "attention.event_source"
        case (.storageRecovered, _):
            "attention.storage_recovered"
        case (.unknown, _):
            "attention.generic"
        }
        return BrowserAttentionPresentation(
            eventToken: item.eventToken,
            copyKey: key,
            incidentID: item.incidentId,
            overallOutcome: item.overallOutcome,
            occurredAt: item.occurredAtUnixMillis.map(Date.init(unixMillis:))
        )
    }

    private static func recentSettlement(
        status: PublicStatus?,
        history: [HistoryEvent]
    ) -> RecentBrowserSettlement? {
        guard let summary = status?.mostRecentReclaim else { return nil }
        guard let event = history.first(where: {
            $0.eventToken == summary.eventToken && $0.incidentId == summary.incidentId
        }),
              event.occurredAtUnixMillis == summary.occurredAtUnixMillis,
              case .cleanup(let receipt) = event.payload,
              receipt.processOutcome == summary.processOutcome,
              receipt.artifactOutcome == summary.artifactOutcome,
              receipt.overallOutcome == summary.overallOutcome
        else {
            return RecentBrowserSettlement(
                incidentID: summary.incidentId,
                eventToken: summary.eventToken,
                familyKey: nil,
                occurredAt: Date(unixMillis: summary.occurredAtUnixMillis),
                processCount: nil,
                estimatedReclaimedMemoryBytes: nil,
                revivalChecksCompleted: nil,
                artifactOutcome: summary.artifactOutcome,
                overallOutcome: summary.overallOutcome,
                isFallback: true
            )
        }

        let priorObservation = history
            .compactMap { candidate -> (HistoryEvent, ObservationRecord)? in
                guard candidate.incidentId == event.incidentId,
                      candidate.occurredAtUnixMillis < event.occurredAtUnixMillis,
                      case .observation(let observation) = candidate.payload
                else {
                    return nil
                }
                return (candidate, observation)
            }
            .max(by: { $0.0.occurredAtUnixMillis < $1.0.occurredAtUnixMillis })?
            .1

        return RecentBrowserSettlement(
            incidentID: event.incidentId,
            eventToken: event.eventToken,
            familyKey: priorObservation.map { familyKey(for: $0.family) },
            occurredAt: Date(unixMillis: event.occurredAtUnixMillis),
            processCount: receipt.resources.before?.processCount,
            estimatedReclaimedMemoryBytes: receipt.resources.estimatedReclaimedMemoryBytes,
            revivalChecksCompleted: receipt.revivalChecksCompleted,
            artifactOutcome: receipt.artifactOutcome,
            overallOutcome: receipt.overallOutcome,
            isFallback: false
        )
    }

}
