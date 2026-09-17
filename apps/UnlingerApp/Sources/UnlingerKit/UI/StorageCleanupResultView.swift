import SwiftUI

public struct StorageCleanupResultView: View {
    public var result: StorageCleanupResultPresentation

    public init(result: StorageCleanupResultPresentation) {
        self.result = result
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(L10n.text(result.outcomeKey))
                .font(.subheadline)
            Text(
                L10n.text(
                    "browser.storage_cleanup.planned",
                    result.plannedCandidateCount
                )
            )
            .font(.caption)
            .foregroundStyle(.secondary)
            Text(
                L10n.text(
                    "browser.storage_cleanup.before",
                    result.beforeCandidateCount,
                    logicalSize(result.beforeLogicalBytes)
                )
            )
            .font(.caption)
            .foregroundStyle(.secondary)
            if let removedCandidateCount = result.removedCandidateCount {
                Text(
                    L10n.text(
                        "browser.storage_cleanup.removed",
                        removedCandidateCount
                    )
                )
                .font(.caption)
                .foregroundStyle(.secondary)
            }
            if let afterCandidateCount = result.afterCandidateCount,
               let afterLogicalBytes = result.afterLogicalBytes
            {
                Text(
                    L10n.text(
                        "browser.storage_cleanup.after",
                        afterCandidateCount,
                        logicalSize(afterLogicalBytes)
                    )
                )
                .font(.caption)
                .foregroundStyle(.secondary)
            }
            if let retainedNotPlannedCount = result.retainedNotPlannedCount,
               retainedNotPlannedCount > 0
            {
                Text(
                    L10n.text(
                        "browser.storage_cleanup.retained_not_planned",
                        retainedNotPlannedCount
                    )
                )
                .font(.caption)
                .foregroundStyle(.secondary)
            }
            Text(
                L10n.text(
                    "browser.storage_cleanup.prepared_at",
                    Format.shortTime(result.preparedAt)
                )
            )
            .font(.caption)
            .foregroundStyle(.tertiary)
            if let completedAt = result.completedAt {
                Text(
                    L10n.text(
                        "browser.storage_cleanup.completed_at",
                        Format.shortTime(completedAt)
                    )
                )
                .font(.caption)
                .foregroundStyle(.tertiary)
            }
            Text(L10n.text("browser.storage_cleanup.apfs_caveat"))
                .font(.caption)
                .foregroundStyle(.tertiary)
        }
    }

    private func logicalSize(_ bytes: UInt64) -> String {
        ByteCountFormatter.string(
            fromByteCount: Int64(clamping: bytes),
            countStyle: .file
        )
    }
}
