import SwiftUI

/// Durable attention from typed backend facts. Incident-bound items navigate
/// to capability-gated actions; global items remain explanatory only.
public struct AttentionList: View {
    let items: [BrowserAttentionPresentation]
    let overflow: Int

    public init(items: [BrowserAttentionPresentation], overflow: Int) {
        self.items = items
        self.overflow = overflow
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            VStack(alignment: .leading, spacing: 2) {
                Text(L10n.text("attention.section"))
                    .font(.caption)
                    .foregroundStyle(.tertiary)
                Text(L10n.text("attention.hint"))
                    .font(.caption)
                    .foregroundStyle(.tertiary)
            }
            ForEach(items) { item in
                row(item)
            }
            if overflow > 0 {
                Text(L10n.text("attention.more", overflow))
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
        }
    }

    /// Items carrying an incident ID navigate to the incident detail, where
    /// its capability actions (retry/protect/export) live.
    @ViewBuilder
    private func row(_ item: BrowserAttentionPresentation) -> some View {
        if let incidentID = item.incidentID {
            NavigationLink(value: Route.incident(incidentID)) {
                rowContent(item, navigable: true)
            }
            .buttonStyle(.plain)
        } else {
            rowContent(item, navigable: false)
        }
    }

    private func rowContent(_ item: BrowserAttentionPresentation, navigable: Bool) -> some View {
        HStack(alignment: .firstTextBaseline, spacing: 8) {
            Image(systemName: "exclamationmark.circle")
                .foregroundStyle(.orange)
                .font(.subheadline)
                .accessibilityHidden(true)
            VStack(alignment: .leading, spacing: 2) {
                Text(L10n.text(item.copyKey))
                    .font(.subheadline)
                if let occurredAt = item.occurredAt {
                    Text(Format.relativeTime(occurredAt))
                        .font(.caption)
                        .foregroundStyle(.tertiary)
                }
            }
            Spacer(minLength: 0)
            if navigable {
                Image(systemName: "chevron.right")
                    .font(.caption)
                    .foregroundStyle(.tertiary)
                    .accessibilityHidden(true)
            }
        }
    }
}
