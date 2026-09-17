import Foundation

/// Presentation-only shaping for the bounded history and incident timelines.
/// It may enrich a history row from the coherent current browser snapshot, but
/// it does not compute product phase, compatibility, or cleanup authority.
public enum BrowserHistoryMapper {
    public static func entries(
        events: [HistoryEvent],
        currentSessions: [BrowserSessionPresentation],
        recentSettlement: RecentBrowserSettlement?,
        mode: EffectiveMode?
    ) -> [BrowserHistoryEntryPresentation] {
        return Dictionary(grouping: events, by: \HistoryEvent.incidentId)
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
        guard let terminalEvent = terminalEvents.max(by: eventPrecedes)
        else { return nil }
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
        let state = currentSession?.state ?? terminalEvent.state
        let coverageNotice = compatibility?.reasonId.map(BrowserOverviewMapper.coverageNotice)

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
            reasonKey: currentSession?.reasonKey
                ?? (currentSession == nil && withoutIntervention
                    ? "detail.cleanup.without_intervention"
                    : BrowserOverviewMapper.sessionReasonKey(
                    state: state,
                    mode: mode,
                    coverageNotice: coverageNotice
                )),
            memberCount: currentSession?.memberCount
                ?? latestObservation?.memberCount
                ?? terminalCleanup.resources.before?.processCount,
            residentMemoryBytes: currentSession?.residentMemoryBytes
                ?? latestObservation?.residentMemoryBytes
                ?? terminalCleanup.resources.before?.residentMemoryBytes,
            latestAt: Date(unixMillis: terminalEvent.occurredAtUnixMillis),
            eventCount: terminalEvents.count,
            isCurrent: currentSession != nil
        )
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
