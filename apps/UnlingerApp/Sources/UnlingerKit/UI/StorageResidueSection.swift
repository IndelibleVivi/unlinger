import SwiftUI

public struct StorageResidueSection: View {
    public var residue: StorageResiduePresentation

    public init(residue: StorageResiduePresentation) {
        self.residue = residue
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Label(L10n.text("browser.storage_residue.title"), systemImage: "externaldrive.badge.exclamationmark")
                .font(.subheadline.weight(.semibold))
            switch residue.status {
            case .detected:
                Text(L10n.text("browser.storage_residue.detected", residue.candidateCount))
                    .font(.subheadline)
                Text(
                    L10n.text(
                        "browser.storage_residue.logical_size",
                        ByteCountFormatter.string(fromByteCount: Int64(clamping: residue.logicalBytes), countStyle: .file)
                    )
                )
                .font(.caption)
                .foregroundStyle(.secondary)
                Text(L10n.text("browser.storage_residue.observe_only"))
                    .font(.caption2)
                    .foregroundStyle(.tertiary)
            case .unavailable:
                Text(L10n.text("browser.storage_residue.unavailable"))
                    .font(.caption)
                    .foregroundStyle(.secondary)
            case .clear:
                EmptyView()
            }
        }
        .accessibilityElement(children: .combine)
    }
}
