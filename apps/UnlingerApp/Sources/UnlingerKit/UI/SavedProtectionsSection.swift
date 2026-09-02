import SwiftUI

public struct SavedProtectionsSection: View {
    let protections: [ProtectedIncidentSummary]

    public init(protections: [ProtectedIncidentSummary]) {
        self.protections = protections
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: 7) {
            Label(
                L10n.text("browser.protections.title", protections.count),
                systemImage: "hand.raised"
            )
            .font(.caption)
            .foregroundStyle(.tertiary)
            ForEach(protections.prefix(3)) { protection in
                NavigationLink(value: Route.incident(protection.incidentId)) {
                    HStack {
                        Text(L10n.text(
                            "browser.protections.item",
                            Format.relativeTime(Date(unixMillis: protection.protectedAtUnixMillis))
                        ))
                        .font(.subheadline)
                        .foregroundStyle(.secondary)
                        Spacer(minLength: 0)
                        Image(systemName: "chevron.right")
                            .font(.caption)
                            .foregroundStyle(.tertiary)
                            .accessibilityHidden(true)
                    }
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
            }
        }
    }
}
