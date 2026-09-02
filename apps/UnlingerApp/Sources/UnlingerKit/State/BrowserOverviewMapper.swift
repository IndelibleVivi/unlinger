import Foundation

/// Presentation-only translation of the daemon-owned schema-v4 browser
/// snapshot. Phase, coverage, compatibility, and settlement authority stay in
/// the daemon; this mapper selects localized copy keys and display shapes.
public enum BrowserOverviewMapper {
    public static func make(
        connection: ConnectionState,
        snapshot: BrowserOverviewSnapshot?
    ) -> BrowserOverview {
        let phase = connection == .live ? (snapshot?.phase ?? .unknown) : .unknown
        let trusted = connection == .live
            && snapshot?.healthy == true
            && snapshot?.freshness == .current
            && phase != .unknown
        let sessions = snapshot.map { snapshot in
            snapshot.sessions.map { sessionPresentation($0, mode: snapshot.effectiveMode) }
        } ?? []
        let attention = snapshot?.attention.items.map(attentionPresentation) ?? []
        let attentionOverflow = max(0, (snapshot?.attention.totalCount ?? 0) - attention.count)
        let copy = overviewCopy(
            phase: phase,
            connection: connection,
            freshness: snapshot?.freshness,
            mode: snapshot?.effectiveMode
        )
        let currentIDs = Set(sessions.map(\.incidentID))
        let savedProtections = snapshot?.protection.items.filter {
            !currentIDs.contains($0.incidentId)
        } ?? []
        let notices = uniqueCoverageNotices(snapshot?.coverageNotices ?? [])

        return BrowserOverview(
            phase: phase,
            tone: tone(for: phase),
            headlineKey: copy.headline,
            detailKey: copy.detail,
            modeKey: modeKey(mode: snapshot?.effectiveMode, paused: snapshot?.pausedUntilUnixMillis),
            snapshotTrusted: trusted,
            requiresTrailingRefresh: false,
            observedAt: snapshot?.observedAtUnixMillis.map(Date.init(unixMillis:)),
            pausedUntil: snapshot?.pausedUntilUnixMillis.map(Date.init(unixMillis:)),
            sessions: sessions,
            coverageNotices: notices,
            attention: attention,
            attentionOverflow: attentionOverflow,
            savedProtections: savedProtections,
            recentSettlement: snapshot?.recentSettlement.map(settlementPresentation)
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
                else { return nil }
                return (event, observation)
            })
            .max(by: { $0.0.occurredAtUnixMillis < $1.0.occurredAtUnixMillis })?.1
        else { return nil }
        return BrowserSessionPresentation(
            incidentID: incidentID,
            familyKey: familyKey(for: latest.family),
            state: latest.state,
            stateKey: stateKey(for: latest.state),
            reasonKey: sessionReasonKey(state: latest.state, mode: mode, coverageNotice: nil),
            memberCount: latest.memberCount,
            residentMemoryBytes: latest.residentMemoryBytes,
            coverageNotice: nil,
            isPreviousObservation: true
        )
    }

    private static func sessionPresentation(
        _ session: BrowserSessionSummary,
        mode: EffectiveMode
    ) -> BrowserSessionPresentation {
        let notice = session.compatibility.reasonId.map(coverageNotice)
        return BrowserSessionPresentation(
            incidentID: session.incidentId,
            familyKey: familyKey(for: session.family),
            state: session.state,
            stateKey: stateKey(for: session.state),
            reasonKey: sessionReasonKey(state: session.state, mode: mode, coverageNotice: notice),
            memberCount: session.memberCount,
            residentMemoryBytes: session.residentMemoryBytes,
            coverageNotice: notice,
            isPreviousObservation: false
        )
    }

    private static func settlementPresentation(
        _ settlement: BrowserSettlementSummary
    ) -> RecentBrowserSettlement {
        RecentBrowserSettlement(
            incidentID: settlement.incidentId,
            eventToken: settlement.eventToken,
            familyKey: familyKey(for: settlement.family),
            occurredAt: Date(unixMillis: settlement.occurredAtUnixMillis),
            processCount: settlement.processCount,
            estimatedReclaimedMemoryBytes: settlement.estimatedReclaimedMemoryBytes,
            revivalChecksCompleted: settlement.revivalChecksCompleted,
            artifactOutcome: settlement.artifactOutcome,
            overallOutcome: settlement.overallOutcome,
            isFallback: false
        )
    }

    private static func uniqueCoverageNotices(
        _ notices: [BrowserCoverageSummary]
    ) -> [BrowserCoverageNotice] {
        var seen = Set<BrowserCoverageNotice>()
        return notices.compactMap { summary in
            let notice = coverageNotice(summary.reasonId)
            return seen.insert(notice).inserted ? notice : nil
        }
    }

    private static func coverageNotice(_ reasonID: String) -> BrowserCoverageNotice {
        switch reasonID {
        case "protection.browser_version_mixed": .mixedVersions
        case "protection.browser_product_unsupported": .unsupportedProduct
        case "protection.browser_version_unsupported": .unsupportedVersion
        case "protection.browser_version_missing": .versionUnavailable
        case "protection.controller_version_unverified": .controllerUnverified
        case "protection.version_observational_only": .observationOnly
        case "protection.debug_peer_visibility_incomplete": .controlPathIncomplete
        default: .unknown(reasonID: reasonID)
        }
    }

    private static func overviewCopy(
        phase: BrowserOverviewPhase,
        connection: ConnectionState,
        freshness: ObservationFreshness?,
        mode: EffectiveMode?
    ) -> (headline: String, detail: String?) {
        switch phase {
        case .clear: ("browser.overview.clear", "browser.overview.clear.detail")
        case .active: ("browser.overview.active", "browser.overview.active.detail")
        case .verifying: ("browser.overview.verifying", "browser.overview.verifying.detail")
        case .confirmed:
            mode == .reportOnly
                ? ("browser.overview.confirmed", "browser.overview.confirmed.report_only")
                : ("browser.overview.confirmed", "browser.overview.confirmed.enforce")
        case .reclaiming: ("browser.overview.reclaiming", "browser.overview.reclaiming.detail")
        case .protected: ("browser.overview.protected", "browser.overview.protected.detail")
        case .attention: ("browser.overview.attention", "browser.overview.attention.detail")
        case .unknown:
            if connection == .connecting || freshness == .scanInProgress || freshness == .neverObserved {
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

    private static func modeKey(mode: EffectiveMode?, paused: UInt64?) -> String {
        if paused != nil { return "browser.mode.paused" }
        return switch mode {
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

    private static func attentionPresentation(
        _ item: AttentionItem
    ) -> BrowserAttentionPresentation {
        let key = switch (item.kind, item.reasonId) {
        case (.daemonUnhealthy, _): "attention.daemon_unhealthy"
        case (.cleanupResidue, _), (.cleanupFailed, "cleanup.artifact_unsafe"):
            "attention.residue"
        case (.cleanupFailed, "cleanup.delivery_unknown"):
            "attention.failed.delivery_unknown"
        case (.cleanupFailed, _): "attention.failed.generic"
        case (.cleanupRevived, _): "attention.revived"
        case (.eventSourceDegraded, _): "attention.event_source"
        case (.storageRecovered, _): "attention.storage_recovered"
        case (.unknown, _): "attention.generic"
        }
        return BrowserAttentionPresentation(
            eventToken: item.eventToken,
            copyKey: key,
            incidentID: item.incidentId,
            overallOutcome: item.overallOutcome,
            occurredAt: item.occurredAtUnixMillis.map(Date.init(unixMillis:))
        )
    }
}
