import SwiftUI

/// Read-only roster of what the latest reconciliation cycle is actually
/// seeing. Observability only: rows open the incident detail, and no action
/// availability is derived from this list.
public struct RosterSection: View {
    let incidents: [CurrentIncident]

    public init(incidents: [CurrentIncident]) {
        self.incidents = incidents
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Label(L10n.text("roster.title"), systemImage: "eye")
                .font(.caption)
                .foregroundStyle(.tertiary)
            ForEach(incidents) { incident in
                NavigationLink(value: Route.incident(incident.incidentId)) {
                    row(incident)
                }
                .buttonStyle(.plain)
            }
            Text(L10n.text("roster.hint"))
                .font(.caption)
                .foregroundStyle(.tertiary)
        }
    }

    private func row(_ incident: CurrentIncident) -> some View {
        let observation = incident.observation
        let visual = stateVisual(for: observation.state)
        return HStack(alignment: .firstTextBaseline, spacing: 8) {
            Image(systemName: visual.symbol)
                .foregroundStyle(visual.color)
                .font(.subheadline)
                .accessibilityHidden(true)
            VStack(alignment: .leading, spacing: 2) {
                Text(title(for: observation))
                    .font(.subheadline)
                if let subtitle = subtitle(for: observation) {
                    Text(subtitle)
                        .font(.caption)
                        .foregroundStyle(.tertiary)
                }
            }
            Spacer(minLength: 0)
            Text(stateLabel(for: observation.state))
                .font(.caption)
                .foregroundStyle(visual.color)
            Image(systemName: "chevron.right")
                .font(.caption2)
                .foregroundStyle(.tertiary)
                .accessibilityHidden(true)
        }
        .contentShape(Rectangle())
    }

    /// Symbol + tint per roster state, aligned with the app's tone system:
    /// confirmed tracks toward reclaim (activity), ambiguous is borderline
    /// (attention-adjacent), everything else stays quiet.
    private func stateVisual(for state: IncidentState?) -> (symbol: String, color: Color) {
        switch state {
        case .confirmed: ("checkmark.circle", .accentColor)
        case .ambiguous: ("questionmark.circle", .orange)
        case .protected: ("hand.raised", .secondary)
        default: ("circle.dotted", .secondary)
        }
    }

    private func title(for observation: ObservationRecord) -> String {
        observation.family ?? observation.executableBasename ?? L10n.text("roster.item.unknown")
    }

    private func subtitle(for observation: ObservationRecord) -> String? {
        var parts: [String] = []
        if let basename = observation.executableBasename, basename != observation.family {
            parts.append(basename)
        }
        if let count = observation.memberCount, let rss = observation.residentMemoryBytes {
            parts.append(L10n.text("detail.members", count, Format.bytes(rss)))
        }
        return parts.isEmpty ? nil : parts.joined(separator: " · ")
    }

    private func stateLabel(for state: IncidentState?) -> String {
        switch state {
        case .confirmed: L10n.text("roster.state.confirmed")
        case .ambiguous: L10n.text("roster.state.ambiguous")
        case .some(let other): OutcomeCopy.label(for: other)
        case .none: L10n.text("outcome.observing")
        }
    }
}
