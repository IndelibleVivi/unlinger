import SwiftUI

/// Attention items: the only genuinely actionable surface. Copy comes from
/// kind + reason_id mapping; unknown IDs render generic copy.
public struct AttentionList: View {
    let items: [AttentionViewData]
    let overflow: Int

    public init(items: [AttentionViewData], overflow: Int) {
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
    private func row(_ item: AttentionViewData) -> some View {
        if let incidentID = item.incidentID {
            NavigationLink(value: Route.incident(incidentID)) {
                rowContent(item, navigable: true)
            }
            .buttonStyle(.plain)
        } else {
            rowContent(item, navigable: false)
        }
    }

    private func rowContent(_ item: AttentionViewData, navigable: Bool) -> some View {
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
