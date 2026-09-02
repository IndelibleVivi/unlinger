import SwiftUI

public struct BrowserOverviewSection: View {
    let overview: BrowserOverview

    public init(overview: BrowserOverview) {
        self.overview = overview
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(alignment: .firstTextBaseline, spacing: 8) {
                Image(systemName: symbolName)
                    .foregroundStyle(symbolColor)
                    .accessibilityHidden(true)
                Text(L10n.text(overview.headlineKey))
                    .font(.headline)
                Spacer(minLength: 8)
                Text(L10n.text(overview.modeKey))
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .padding(.horizontal, 7)
                    .padding(.vertical, 3)
                    .background(Color.secondary.opacity(0.1), in: .rect(cornerRadius: 6))
            }

            if let detailKey = overview.detailKey {
                Text(L10n.text(detailKey))
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
            }

            if let pausedUntil = overview.pausedUntil {
                Label(
                    L10n.text("browser.mode.paused_until", Format.shortTime(pausedUntil)),
                    systemImage: "pause.circle"
                )
                .font(.caption)
                .foregroundStyle(.secondary)
            }

            if let observedAt = overview.observedAt {
                Text(L10n.text(
                    overview.snapshotTrusted
                        ? "browser.overview.verified_at"
                        : "browser.overview.last_known_at",
                    Format.relativeTime(observedAt)
                ))
                .font(.caption)
                .foregroundStyle(.tertiary)
            }
        }
        .padding(12)
        .background(Color.secondary.opacity(0.065), in: .rect(cornerRadius: 12))
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(BrowserProductCopy.overviewAccessibilityLabel(overview))
    }

    private var symbolName: String {
        switch overview.phase {
        case .unknown: "questionmark.circle"
        case .clear: "checkmark.circle"
        case .active: "play.circle"
        case .verifying: "hourglass.circle"
        case .confirmed: "exclamationmark.circle"
        case .reclaiming: "arrow.triangle.2.circlepath"
        case .protected: "hand.raised.circle"
        case .attention: "exclamationmark.triangle"
        }
    }

    private var symbolColor: Color {
        switch overview.tone {
        case .quiet: .secondary
        case .activity: .accentColor
        case .attention: .orange
        }
    }
}
