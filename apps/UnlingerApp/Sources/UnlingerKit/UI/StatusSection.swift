import SwiftUI

/// Quiet status header: a small tone dot, the headline, the mode truth, and
/// any pause/activity detail. No warnings for transient scan/cleanup work.
public struct StatusSection: View {
    let viewModel: StatusViewModel

    public init(viewModel: StatusViewModel) {
        self.viewModel = viewModel
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack(spacing: 8) {
                Circle()
                    .fill(toneColor)
                    .frame(width: 8, height: 8)
                Text(L10n.text(viewModel.headlineKey))
                    .font(.headline)
            }

            Text(L10n.text(viewModel.modeIsReportOnly ? "mode.report_only" : "mode.enforce"))
                .font(.subheadline)
                .foregroundStyle(.secondary)

            if let pausedUntil = viewModel.pausedUntil {
                Label(L10n.text("paused.until", Format.shortTime(pausedUntil)), systemImage: "pause.circle")
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
            }

            if let detailKey = viewModel.detailKey {
                Text(L10n.text(detailKey))
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
            }

            HStack(spacing: 12) {
                if let lastScanAt = viewModel.lastScanAt {
                    Text(L10n.text("last_scan", Format.relativeTime(lastScanAt)))
                }
                if viewModel.ambiguousCount > 0 {
                    Text(L10n.text("ambiguous.count", viewModel.ambiguousCount))
                }
            }
            .font(.caption)
            .foregroundStyle(.tertiary)
        }
        .accessibilityElement(children: .combine)
    }

    private var toneColor: Color {
        switch viewModel.tone {
        case .quiet: .secondary
        case .activity: .accentColor
        case .attention: .orange
        }
    }
}
