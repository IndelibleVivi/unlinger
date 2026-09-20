import SwiftUI

public struct ToolCacheMaintenanceSection: View {
    public var cache: ToolCachePresentation

    public var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Label(L10n.text(cache.titleKey), systemImage: "shippingbox")
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
            if let count = cache.nativeRemovedEntryCount {
                Text(L10n.text("cache.maintenance.native_count", count)).font(.caption)
            }
            if let bytes = cache.nativeRemovedLogicalBytes {
                Text(L10n.text("cache.maintenance.native_size",
                    ByteCountFormatter.string(fromByteCount: Int64(clamping: bytes), countStyle: .file)))
                    .font(.caption)
            }
            if cache.nativeRemovedEntryCount != nil || cache.nativeRemovedLogicalBytes != nil {
                Text(L10n.text(cache.accountingKey))
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            Text(L10n.text(cache.scopeKey))
                .font(.caption)
                .foregroundStyle(.secondary)
        }
        .accessibilityElement(children: .combine)
    }
}
