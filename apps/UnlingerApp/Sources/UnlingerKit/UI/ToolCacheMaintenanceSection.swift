import SwiftUI

public struct ToolCacheMaintenanceSection: View {
    public var cache: ToolCachePresentation

    public var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Label(L10n.text("cache.maintenance.title"), systemImage: "shippingbox")
                .font(.subheadline.weight(.semibold))
            Text(L10n.text(cache.availabilityKey))
                .font(.caption)
            Text(L10n.text("cache.maintenance.observed", cache.observedAt.formatted()))
                .font(.caption)
                .foregroundStyle(.secondary)
            if let key = cache.outcomeKey, let attemptedAt = cache.attemptedAt {
                Text(L10n.text(key)).font(.subheadline)
                Text(attemptedAt.formatted())
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            if let count = cache.nativeRemovedEntryCount, let bytes = cache.nativeRemovedLogicalBytes {
                Text(L10n.text("cache.maintenance.native_result", count,
                    ByteCountFormatter.string(fromByteCount: Int64(clamping: bytes), countStyle: .file)))
                    .font(.caption)
                Text(L10n.text("cache.maintenance.estimate"))
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            Text(L10n.text("cache.maintenance.scope"))
                .font(.caption)
                .foregroundStyle(.secondary)
        }
        .accessibilityElement(children: .combine)
    }
}
