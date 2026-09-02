import Foundation

/// One meaningful phase in an incident timeline. Consecutive observations
/// with the same family and state are summarized without hiding state changes
/// or cleanup receipts.
public struct BrowserTimelineEntryPresentation: Equatable, Identifiable, Sendable {
    public var latestEvent: HistoryEvent
    public var eventCount: Int

    public var id: String { latestEvent.eventToken }
}
