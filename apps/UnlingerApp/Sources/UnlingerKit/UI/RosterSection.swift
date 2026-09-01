import SwiftUI

/// Read-only latest observation snapshot. Rows may be retained or stale; this
/// surface never claims they are still live after cleanup/revival checks.
public struct RosterSection: View {
    let roster: ObservationRoster

    public init(roster: ObservationRoster) {
        self.roster = roster
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Label(L10n.text("roster.latest_observation"), systemImage: "eye")
                .font(.caption)
                .foregroundStyle(.tertiary)
            if let observedAt = roster.observedAtUnixMillis {
                Text(Format.relativeTime(Date(unixMillis: observedAt)))
                    .font(.caption)
                    .foregroundStyle(.tertiary)
            }
            ForEach(roster.items) { incident in
                NavigationLink(value: Route.incident(incident.incidentId)) {
                    row(incident)
                }
                .buttonStyle(.plain)
            }
            Text(L10n.text(freshnessCopyKey))
                .font(.caption)
                .foregroundStyle(.tertiary)
        }
    }

    private var freshnessCopyKey: String {
        switch roster.freshness {
        case .current: "roster.freshness.current"
        case .scanInProgress: "roster.freshness.scan_in_progress"
        case .staleAfterFailure: "roster.freshness.stale_after_failure"
        case .neverObserved: "roster.freshness.never_observed"
        case .unknown: "roster.freshness.unknown"
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
        observation.family
    }

    private func subtitle(for observation: ObservationRecord) -> String? {
        var parts: [String] = []
        if observation.executableBasename != observation.family {
            parts.append(observation.executableBasename)
        }
        parts.append(
            L10n.text(
                "detail.members",
                observation.memberCount,
                Format.bytes(observation.residentMemoryBytes)
            )
        )
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
