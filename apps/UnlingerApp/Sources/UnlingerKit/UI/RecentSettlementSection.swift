import SwiftUI

public struct RecentSettlementSection: View {
    let settlement: RecentBrowserSettlement

    public init(settlement: RecentBrowserSettlement) {
        self.settlement = settlement
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: 7) {
            Text(L10n.text("browser.settlement.title"))
                .font(.caption)
                .foregroundStyle(.tertiary)
            NavigationLink(value: Route.incident(settlement.incidentID)) {
                HStack(alignment: .firstTextBaseline, spacing: 9) {
                    Image(systemName: symbolName)
                        .foregroundStyle(symbolColor)
                        .accessibilityHidden(true)
                    VStack(alignment: .leading, spacing: 3) {
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
                    Spacer(minLength: 0)
                    Image(systemName: "chevron.right")
                        .font(.caption)
                        .foregroundStyle(.tertiary)
                        .accessibilityHidden(true)
                }
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityElement(children: .ignore)
            .accessibilityLabel(BrowserProductCopy.settlementAccessibilityLabel(settlement))
            .accessibilityAddTraits(.isButton)
        }
    }

    private var symbolName: String {
        switch settlement.overallOutcome {
        case .cleared: "checkmark.circle"
        case .clearedWithResidue: "checkmark.circle.badge.exclamationmark"
        case .revived: "arrow.uturn.forward.circle"
        case .failed: "exclamationmark.circle"
        case .unknown: "questionmark.circle"
        }
    }

    private var symbolColor: Color {
        switch settlement.overallOutcome {
        case .cleared: .secondary
        case .clearedWithResidue, .revived, .failed, .unknown: .orange
        }
    }
}
