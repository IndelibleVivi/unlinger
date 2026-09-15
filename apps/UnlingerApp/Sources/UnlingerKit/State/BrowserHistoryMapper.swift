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
        let settledEvents = events.filter { event in
            guard case .cleanup = event.payload else { return false }
            return [.cleared, .failed, .revived].contains(event.state)
        }
        return Dictionary(grouping: settledEvents, by: \HistoryEvent.incidentId)
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
        guard let latestEvent = events.max(by: {
            $0.occurredAtUnixMillis < $1.occurredAtUnixMillis
        }) else { return nil }
        let latestObservation = events
            .compactMap { event -> (HistoryEvent, ObservationRecord)? in
                guard case .observation(let observation) = event.payload else { return nil }
                return (event, observation)
            }
            .max { $0.0.occurredAtUnixMillis < $1.0.occurredAtUnixMillis }?.1
        let compatibility = latestObservation?.browserCompatibility
        let latestCleanup = events
            .compactMap { event -> (HistoryEvent, CleanupReceipt)? in
                guard case .cleanup(let receipt) = event.payload else { return nil }
                return (event, receipt)
            }
            .max { $0.0.occurredAtUnixMillis < $1.0.occurredAtUnixMillis }?.1
        let withoutIntervention: Bool = if case .cleanup(let receipt) = latestEvent.payload {
            receipt.endedWithoutIntervention
        } else {
            false
        }
        let state = currentSession?.state ?? latestEvent.state
        let coverageNotice = compatibility?.reasonId.map(BrowserOverviewMapper.coverageNotice)

        return BrowserHistoryEntryPresentation(
            incidentID: incidentID,
            familyKey: currentSession?.familyKey
                ?? recentSettlement?.familyKey
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
                ?? latestCleanup?.resources.before?.processCount,
            residentMemoryBytes: currentSession?.residentMemoryBytes
                ?? latestObservation?.residentMemoryBytes
                ?? latestCleanup?.resources.before?.residentMemoryBytes,
            latestAt: Date(unixMillis: latestEvent.occurredAtUnixMillis),
            eventCount: events.count,
            isCurrent: currentSession != nil
        )
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
