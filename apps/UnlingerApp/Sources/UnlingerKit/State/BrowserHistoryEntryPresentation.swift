import Foundation

/// One incident-centric history row. Repeated observation events remain
/// available in detail, but the history index does not present each scan as a
/// separate browser session.
public struct BrowserHistoryEntryPresentation: Equatable, Identifiable, Sendable {
    public var incidentID: String
    public var familyKey: String
    public var productKey: String?
    public var observedVersion: String?
    public var state: IncidentState
    public var stateKey: String
    public var reasonKey: String?
    public var memberCount: Int?
    public var residentMemoryBytes: UInt64?
    public var latestAt: Date
    public var eventCount: Int
    public var isCurrent: Bool

    public var id: String { incidentID }
}
