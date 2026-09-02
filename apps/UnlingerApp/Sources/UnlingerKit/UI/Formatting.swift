import Foundation

@MainActor
public enum Format {
    public static func bytes(_ value: UInt64) -> String {
        ByteCountFormatter.string(fromByteCount: Int64(value), countStyle: .memory)
    }

    public static func relativeTime(_ date: Date) -> String {
        date.formatted(
            .relative(presentation: .named)
                .locale(LanguageSettings.shared.locale)
        )
    }

    public static func shortTime(_ date: Date) -> String {
        date.formatted(
            .dateTime
                .locale(LanguageSettings.shared.locale)
                .year()
                .month(.abbreviated)
                .day()
                .hour()
                .minute()
        )
    }
}

/// Maps backend `unavailable_reason_id` to display copy. Unknown IDs fall
/// back to a generic line and never get interpreted.
@MainActor
public enum CapabilityCopy {
    public static func unavailableReason(_ reasonId: String?) -> String {
        switch reasonId {
        case "action.not_paused": L10n.text("cap.action.not_paused")
        case "action.no_blocked_cleanup": L10n.text("cap.action.no_blocked_cleanup")
        case "action.already_protected": L10n.text("cap.action.already_protected")
        case "action.not_protected": L10n.text("cap.action.not_protected")
        default: L10n.text("cap.generic")
        }
    }
}

@MainActor
public enum OutcomeCopy {
    public static func label(for outcome: OverallOutcome) -> String {
        switch outcome {
        case .cleared: L10n.text("outcome.cleared")
        case .clearedWithResidue: L10n.text("outcome.cleared_with_residue")
        case .revived: L10n.text("outcome.revived")
        case .failed: L10n.text("outcome.failed")
        case .unknown: L10n.text("outcome.unknown")
        }
    }

    public static func label(for state: IncidentState) -> String {
        switch state {
        case .cleared: L10n.text("outcome.cleared")
        case .revived: L10n.text("outcome.revived")
        case .failed: L10n.text("outcome.failed")
        case .protected: L10n.text("outcome.protected")
        case .active, .cooling, .confirmed, .ambiguous, .reclaiming:
            L10n.text("outcome.observing")
        case .unknown: L10n.text("outcome.observing")
        }
    }

    public static func roleLabel(_ role: ProcessRole) -> String {
        switch role {
        case .controller: L10n.text("detail.roles.controller")
        case .browserRoot: L10n.text("detail.roles.browser_root")
        case .renderer: L10n.text("detail.roles.renderer")
        case .unknown(let raw): raw
        default: role.wire
        }
    }
}

extension Date {
    init(unixMillis: UInt64) {
        self.init(timeIntervalSince1970: TimeInterval(unixMillis) / 1_000)
    }

    var unixMillis: UInt64 { UInt64(timeIntervalSince1970 * 1_000) }
}
