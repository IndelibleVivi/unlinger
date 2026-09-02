import Foundation

/// Localized, user-facing browser copy. Typed facts enter here only after the
/// mapper has selected a conservative product state.
@MainActor
public enum BrowserProductCopy {
    public static func overviewAccessibilityLabel(_ overview: BrowserOverview) -> String {
        var parts = [L10n.text(overview.headlineKey)]
        if let detailKey = overview.detailKey { parts.append(L10n.text(detailKey)) }
        parts.append(L10n.text(overview.modeKey))
        if let observedAt = overview.observedAt {
            parts.append(L10n.text(
                overview.snapshotTrusted
                    ? "browser.overview.verified_at"
                    : "browser.overview.last_known_at",
                Format.relativeTime(observedAt)
            ))
        }
        return parts.joined(separator: " ")
    }

    public static func sessionAccessibilityLabel(
        _ session: BrowserSessionPresentation
    ) -> String {
        var parts = [
            L10n.text(session.familyKey),
            L10n.text(session.stateKey),
            L10n.text(
                "browser.session.metrics",
                session.memberCount,
                Format.bytes(session.residentMemoryBytes)
            )
        ]
        if let reasonKey = session.reasonKey { parts.append(L10n.text(reasonKey)) }
        if session.isPreviousObservation {
            parts.append(L10n.text("browser.session.previous_observation"))
        }
        return parts.joined(separator: " ")
    }

    public static func settlementHeadline(
        _ settlement: RecentBrowserSettlement
    ) -> String {
        let family = settlement.familyKey.map(L10n.text)
        return switch (settlement.overallOutcome, family) {
        case (.cleared, .some(let family)):
            L10n.text("browser.settlement.cleared.family", family)
        case (.clearedWithResidue, .some(let family)):
            L10n.text("browser.settlement.residue.family", family)
        case (.revived, .some(let family)):
            L10n.text("browser.settlement.revived.family", family)
        case (.failed, .some(let family)):
            L10n.text("browser.settlement.failed.family", family)
        case (.cleared, .none):
            L10n.text("browser.settlement.cleared")
        case (.clearedWithResidue, .none):
            L10n.text("browser.settlement.residue")
        case (.revived, .none):
            L10n.text("browser.settlement.revived")
        case (.failed, .none):
            L10n.text("browser.settlement.failed")
        case (.unknown, _):
            L10n.text("browser.settlement.unknown")
        }
    }

    public static func settlementDetails(
        _ settlement: RecentBrowserSettlement
    ) -> [String] {
        guard !settlement.isFallback else { return [] }
        var details: [String] = []
        if let processCount = settlement.processCount {
            details.append(L10n.text("browser.settlement.process_count", processCount))
        }
        if let memory = settlement.estimatedReclaimedMemoryBytes {
            details.append(L10n.text("browser.settlement.memory", Format.bytes(memory)))
        }
        if let checks = settlement.revivalChecksCompleted {
            details.append(L10n.text("browser.settlement.revival_checks", checks))
        }
        switch settlement.artifactOutcome {
        case .reconciled:
            details.append(L10n.text("browser.settlement.artifact_reconciled"))
        case .residue:
            details.append(L10n.text("browser.settlement.artifact_residue"))
        case .deliveryUnknown:
            details.append(L10n.text("browser.settlement.artifact_unknown"))
        case .notApplicable, .unknown:
            break
        }
        return details
    }

    public static func settlementAccessibilityLabel(
        _ settlement: RecentBrowserSettlement
    ) -> String {
        ([settlementHeadline(settlement)] + settlementDetails(settlement) + [
            Format.relativeTime(settlement.occurredAt),
        ]).joined(separator: " ")
    }
}
