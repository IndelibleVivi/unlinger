import SwiftUI

public struct StorageResidueSection: View {
    public var residue: StorageResiduePresentation?
    public var cleanupResult: StorageCleanupResultPresentation?

    public init(
        residue: StorageResiduePresentation?,
        cleanupResult: StorageCleanupResultPresentation?
    ) {
        self.residue = residue
        self.cleanupResult = cleanupResult
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Label(
                L10n.text(
                    cleanupResult == nil
                        ? "browser.storage_residue.title"
                        : "browser.storage_cleanup.title"
                ),
                systemImage: cleanupResult?.systemImageName
                    ?? "externaldrive.badge.exclamationmark"
            )
                .font(.subheadline.weight(.semibold))
            if let residue {
                switch residue.status {
                case .detected:
                    Text(L10n.text("browser.storage_residue.detected", residue.candidateCount))
                        .font(.subheadline)
                    Text(
                        L10n.text(
                            "browser.storage_residue.logical_size",
                            ByteCountFormatter.string(
                                fromByteCount: Int64(clamping: residue.logicalBytes),
                                countStyle: .file
                            )
                        )
                    )
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    Text(
                        L10n.text(
                            residue.automaticCleanupEligible
                                ? "browser.storage_residue.eligible"
                                : "browser.storage_residue.waiting"
                        )
                    )
                    .font(.caption)
                    .foregroundStyle(.tertiary)
                case .unavailable:
                    Text(L10n.text("browser.storage_residue.unavailable"))
                        .font(.caption)
                        .foregroundStyle(.secondary)
                case .clear:
                    if cleanupResult == nil {
                        Text(L10n.text("browser.storage_residue.clear"))
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }
                }
            }
            if let cleanupResult {
                StorageCleanupResultView(result: cleanupResult)
            }
        }
        .accessibilityElement(children: .combine)
    }
}
