import SwiftUI

struct BrowserHistoryRow: View {
    let entry: BrowserHistoryEntryPresentation

    var body: some View {
        HStack(alignment: .center, spacing: 10) {
            Image(systemName: symbolName)
                .foregroundStyle(symbolColor)
                .accessibilityHidden(true)
            VStack(alignment: .leading, spacing: 4) {
                HStack(alignment: .firstTextBaseline) {
                    Text(L10n.text(entry.familyKey))
                        .font(.headline)
                    Spacer(minLength: 8)
                    Text(L10n.text(
                        entry.isCurrent
                            ? "history.session.current"
                            : "history.session.recorded"
                    ))
                    .font(.caption)
                    .foregroundStyle(.tertiary)
                }
                if let identity = BrowserProductCopy.browserIdentity(
                    productKey: entry.productKey,
                    observedVersion: entry.observedVersion
                ) {
                    Text(identity)
                        .font(.subheadline)
                        .foregroundStyle(.secondary)
                }
                Text(L10n.text(entry.stateKey))
                    .font(.subheadline)
                    .bold()
                if let reasonKey = entry.reasonKey {
                    Text(L10n.text(reasonKey))
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                }
                HStack(spacing: 6) {
                    if let memberCount = entry.memberCount,
                       let residentMemoryBytes = entry.residentMemoryBytes
                    {
                        Text(L10n.text(
                            "browser.session.summary.compact",
                            memberCount,
                            Format.bytes(residentMemoryBytes)
                        ))
                    }
                    Text(L10n.text("history.event_count", entry.eventCount))
                    Spacer(minLength: 4)
                    Text(Format.relativeTime(entry.latestAt))
                }
                .font(.caption)
                .foregroundStyle(.tertiary)
            }
            Image(systemName: "chevron.right")
                .font(.caption)
                .foregroundStyle(.tertiary)
                .accessibilityHidden(true)
        }
        .padding(.vertical, 10)
        .contentShape(Rectangle())
    }

    private var symbolName: String {
        switch entry.state {
        case .active: "play.circle"
        case .cooling: "hourglass.circle"
        case .confirmed: "exclamationmark.circle"
        case .reclaiming: "arrow.triangle.2.circlepath"
        case .protected: "hand.raised.circle"
        case .ambiguous: "questionmark.circle"
        case .cleared: "checkmark.circle"
        case .revived: "arrow.uturn.forward.circle"
        case .failed: "exclamationmark.circle"
        case .unknown: "questionmark.circle"
        }
    }

    private var symbolColor: Color {
        switch entry.state {
        case .cooling, .confirmed, .reclaiming: .accentColor
        case .ambiguous, .revived, .failed, .unknown: .orange
        case .active, .protected, .cleared: .secondary
        }
    }
}
