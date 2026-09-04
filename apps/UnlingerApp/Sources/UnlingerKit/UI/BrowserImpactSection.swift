import SwiftUI

public struct BrowserImpactSection: View {
    public var impact: BrowserImpactPresentation

    public init(impact: BrowserImpactPresentation) {
        self.impact = impact
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Label(L10n.text("browser.impact.title"), systemImage: "sparkles")
                .font(.subheadline.weight(.semibold))
            if impact.provedReclaimCount == 0 {
                Text(L10n.text("browser.impact.none_since_tracking"))
                    .font(.caption)
                    .foregroundStyle(.secondary)
            } else {
                Text(
                    L10n.text(
                        "browser.impact.reclaims",
                        impact.provedReclaimCount
                    )
                )
                .font(.subheadline)
                if let processCount = impact.reclaimedProcessCount {
                    Text(L10n.text("browser.impact.processes", processCount))
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
                if let bytes = impact.estimatedReclaimedMemoryBytes {
                    Text(
                        L10n.text(
                            "browser.impact.memory",
                            ByteCountFormatter.string(fromByteCount: Int64(clamping: bytes), countStyle: .memory)
                        )
                    )
                    .font(.caption)
                    .foregroundStyle(.secondary)
                }
            }
            if impact.historicalCompleteness == .partialBackfill {
                Text(L10n.text("browser.impact.partial_history"))
                    .font(.caption2)
                    .foregroundStyle(.tertiary)
            }
        }
        .accessibilityElement(children: .combine)
    }
}
