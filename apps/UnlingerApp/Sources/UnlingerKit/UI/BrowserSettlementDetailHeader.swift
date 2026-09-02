import SwiftUI

/// Browser-first context for a settled incident whose retained detail page no
/// longer contains an observation row. Facts come only from the exact-token
/// joined recent-settlement projection; missing estimates remain absent.
public struct BrowserSettlementDetailHeader: View {
    let settlement: RecentBrowserSettlement

    public init(settlement: RecentBrowserSettlement) {
        self.settlement = settlement
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: 7) {
            Text(L10n.text("browser.detail.title"))
                .font(.caption)
                .foregroundStyle(.tertiary)
            if let familyKey = settlement.familyKey {
                Text(L10n.text(familyKey))
                    .font(.headline)
            }
            Text(BrowserProductCopy.settlementHeadline(settlement))
                .font(.subheadline)
            ForEach(BrowserProductCopy.settlementDetails(settlement), id: \.self) { detail in
                Text(detail)
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            Text(Format.relativeTime(settlement.occurredAt))
                .font(.caption)
                .foregroundStyle(.tertiary)
        }
        .padding(12)
        .background(Color.secondary.opacity(0.065), in: .rect(cornerRadius: 12))
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(BrowserProductCopy.settlementAccessibilityLabel(settlement))
    }
}
