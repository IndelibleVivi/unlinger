import Foundation

/// Presentation-only shaping for the bounded history and incident timelines.
/// A history row may use the coherent current browser snapshot or the exact
/// recent-settlement projection. It does not compute product phase,
/// compatibility, cleanup authority, or a synthetic timeline.
public enum BrowserHistoryMapper {
    public static func entries(
        events: [HistoryEvent],
        currentSessions: [BrowserSessionPresentation],
        recentSettlement: RecentBrowserSettlement?,
        mode: EffectiveMode?
    ) -> [BrowserHistoryEntryPresentation] {
        var groupedEvents = Dictionary(grouping: events, by: \HistoryEvent.incidentId)
        if let recentSettlement,
           groupedEvents[recentSettlement.incidentID] == nil
        {
            groupedEvents[recentSettlement.incidentID] = []
        }

        return groupedEvents
            .compactMap { incidentID, incidentEvents in
                historyEntry(
                    incidentID: incidentID,
                    events: incidentEvents,
                    currentSession: currentSessions.first { $0.incidentID == incidentID },
                    recentSettlement: recentSettlement?.incidentID == incidentID
                        ? recentSettlement
                        : nil,
                    mode: mode
                )
            }
            .sorted {
                if $0.latestAt == $1.latestAt { return $0.incidentID < $1.incidentID }
                return $0.latestAt > $1.latestAt
            }
    }

    public static func timelineEntries(
        events: [HistoryEvent]
    ) -> [BrowserTimelineEntryPresentation] {
        var result: [BrowserTimelineEntryPresentation] = []
        for event in events.sorted(by: {
            $0.occurredAtUnixMillis < $1.occurredAtUnixMillis
        }) {
            if var previous = result.last,
               observationsCanCoalesce(previous.latestEvent, event)
            {
                previous.latestEvent = event
                previous.eventCount += event.observationSpan?.observationCount ?? 1
                result[result.count - 1] = previous
            } else {
                result.append(BrowserTimelineEntryPresentation(
                    latestEvent: event,
                    eventCount: event.observationSpan?.observationCount ?? 1
                ))
            }
        }
        return result
    }

    private static func historyEntry(
        incidentID: String,
        events: [HistoryEvent],
        currentSession: BrowserSessionPresentation?,
        recentSettlement: RecentBrowserSettlement?,
        mode: EffectiveMode?
    ) -> BrowserHistoryEntryPresentation? {
        let terminalEvents = events.filter(isTerminalCleanup)
        let latestTerminal = terminalEvents.max(by: eventPrecedes)
        if latestTerminal == nil, let recentSettlement {
            return settlementEntry(
                recentSettlement,
                currentSession: currentSession,
                eventCount: 1
            )
        }
        guard let terminalEvent = latestTerminal
        else { return nil }
        if let recentSettlement,
           recentSettlement.eventToken != terminalEvent.eventToken,
           (recentSettlement.occurredAt > Date(unixMillis: terminalEvent.occurredAtUnixMillis)
               || (recentSettlement.occurredAt == Date(unixMillis: terminalEvent.occurredAtUnixMillis)
                   && terminalEvent.eventToken < recentSettlement.eventToken))
        {
            return settlementEntry(
                recentSettlement,
                currentSession: currentSession,
                eventCount: Set(terminalEvents.map(\.eventToken) + [recentSettlement.eventToken]).count
            )
        }
        return eventBasedEntry(
            incidentID: incidentID,
            events: events,
            terminalEvent: terminalEvent,
            currentSession: currentSession,
            recentSettlement: recentSettlement,
            mode: mode
        )
    }

    private static func eventBasedEntry(
        incidentID: String,
        events: [HistoryEvent],
        terminalEvent: HistoryEvent,
        currentSession: BrowserSessionPresentation?,
        recentSettlement: RecentBrowserSettlement?,
        mode: EffectiveMode?
    ) -> BrowserHistoryEntryPresentation? {
        let anchoredEvents = events.filter {
            $0.occurredAtUnixMillis <= terminalEvent.occurredAtUnixMillis
        }
        let latestObservation = anchoredEvents
            .compactMap { event -> (HistoryEvent, ObservationRecord)? in
                guard case .observation(let observation) = event.payload else { return nil }
                return (event, observation)
            }
            .max { eventPrecedes($0.0, $1.0) }?.1
        let compatibility = latestObservation?.browserCompatibility
        guard case .cleanup(let terminalCleanup) = terminalEvent.payload else { return nil }
        let matchingSettlement = recentSettlement?.eventToken == terminalEvent.eventToken
            ? recentSettlement
            : nil
        let withoutIntervention = terminalCleanup.endedWithoutIntervention
        let receiptState: IncidentState = switch matchingSettlement?.overallOutcome {
        case .clearedWithResidue:
            .failed
        case .revived:
            .revived
        case .unknown:
            .unknown("settlement_outcome_unknown")
        case .cleared, .failed, .none:
            terminalEvent.state
        }
        let state = currentSession?.state ?? receiptState
        let coverageNotice = compatibility?.reasonId.map(BrowserOverviewMapper.coverageNotice)
        let receiptReasonKey = withoutIntervention
                ? "detail.cleanup.without_intervention"
                : matchingSettlement.flatMap(settlementReasonKey)
                    ?? BrowserOverviewMapper.sessionReasonKey(
                    state: state,
                    mode: mode,
                    coverageNotice: coverageNotice
                )

        return BrowserHistoryEntryPresentation(
            incidentID: incidentID,
            familyKey: currentSession?.familyKey
                ?? matchingSettlement?.familyKey
                ?? latestObservation.map { BrowserOverviewMapper.familyKey(for: $0.family) }
                ?? "browser.family.automation",
            productKey: currentSession?.productKey
                ?? compatibility.map { BrowserOverviewMapper.productKey(for: $0.product) },
            observedVersion: currentSession?.observedVersion
                ?? compatibility?.observedVersion,
            state: state,
            stateKey: currentSession?.stateKey ?? (withoutIntervention
                ? "browser.session.ended_without_intervention"
                : BrowserOverviewMapper.stateKey(for: state)),
            reasonKey: currentSession?.reasonKey ?? receiptReasonKey,
            memberCount: currentSession?.memberCount
                ?? latestObservation?.memberCount
                ?? terminalCleanup.resources.before?.processCount,
            residentMemoryBytes: currentSession?.residentMemoryBytes
                ?? latestObservation?.residentMemoryBytes
                ?? terminalCleanup.resources.before?.residentMemoryBytes,
            latestAt: Date(unixMillis: terminalEvent.occurredAtUnixMillis),
            eventCount: events.filter(isTerminalCleanup).count,
            isCurrent: currentSession != nil
        )
    }

    private static func settlementEntry(
        _ settlement: RecentBrowserSettlement,
        currentSession: BrowserSessionPresentation?,
        eventCount: Int
    ) -> BrowserHistoryEntryPresentation {
        let settlementState: IncidentState = switch settlement.overallOutcome {
        case .cleared: .cleared
        case .clearedWithResidue, .failed: .failed
        case .revived: .revived
        case .unknown: .unknown("settlement_outcome_unknown")
        }
        let state = currentSession?.state ?? settlementState
        let stateKey = currentSession?.stateKey
            ?? BrowserOverviewMapper.stateKey(for: state)
        let reasonKey = currentSession?.reasonKey
            ?? settlementReasonKey(settlement)
            ?? "browser.settlement.cleared"

        return BrowserHistoryEntryPresentation(
            incidentID: settlement.incidentID,
            familyKey: currentSession?.familyKey
                ?? settlement.familyKey
                ?? "browser.family.automation",
            productKey: currentSession?.productKey,
            observedVersion: currentSession?.observedVersion,
            state: state,
            stateKey: stateKey,
            reasonKey: reasonKey,
            memberCount: currentSession?.memberCount,
            residentMemoryBytes: currentSession?.residentMemoryBytes,
            latestAt: settlement.occurredAt,
            eventCount: eventCount,
            isCurrent: currentSession != nil
        )
    }

    private static func settlementReasonKey(
        _ settlement: RecentBrowserSettlement
    ) -> String? {
        switch settlement.overallOutcome {
        case .cleared: nil
        case .clearedWithResidue: "browser.settlement.residue"
        case .revived: "browser.settlement.revived"
        case .failed: "browser.settlement.failed"
        case .unknown: "browser.settlement.unknown"
        }
    }

    private static func isTerminalCleanup(_ event: HistoryEvent) -> Bool {
        guard case .cleanup = event.payload else { return false }
        return [.cleared, .failed, .revived].contains(event.state)
    }

    private static func eventPrecedes(_ lhs: HistoryEvent, _ rhs: HistoryEvent) -> Bool {
        if lhs.occurredAtUnixMillis == rhs.occurredAtUnixMillis {
            return lhs.eventToken < rhs.eventToken
        }
        return lhs.occurredAtUnixMillis < rhs.occurredAtUnixMillis
    }

    private static func observationsCanCoalesce(
        _ earlier: HistoryEvent,
        _ later: HistoryEvent
    ) -> Bool {
        guard earlier.incidentId == later.incidentId,
              case .observation(let earlierObservation) = earlier.payload,
              case .observation(let laterObservation) = later.payload
        else { return false }
        return earlierObservation.family == laterObservation.family
            && earlierObservation.state == laterObservation.state
    }
}
