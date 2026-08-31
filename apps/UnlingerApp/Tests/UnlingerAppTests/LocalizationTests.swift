import Foundation
import Testing
@testable import UnlingerKit

@Suite("Localization")
@MainActor
struct LocalizationTests {
    @Test("every copy key used by the mapping layer resolves to real text")
    func keysResolve() {
        let keys = [
            "status.headline.quiet", "status.headline.activity", "status.headline.attention",
            "status.detail.event_source_degraded", "status.detail.storage_degraded",
            "mode.report_only", "mode.enforce",
            "attention.daemon_unhealthy", "attention.residue", "attention.failed.delivery_unknown",
            "attention.failed.generic", "attention.revived", "attention.event_source",
            "attention.storage_recovered", "attention.generic", "attention.more",
            "reclaim.cleared", "reclaim.cleared_with_residue", "reclaim.revived",
            "reclaim.failed", "reclaim.generic",
            "roster.title", "roster.item.unknown",
            "roster.state.confirmed", "roster.state.ambiguous",
            "attention.section", "attention.hint", "roster.hint",
            "history.hint", "actions.hint",
            "language.menu", "language.system", "language.en", "language.zh_hans",
            "cap.action.not_paused", "cap.action.no_blocked_cleanup",
            "cap.action.already_protected", "cap.action.not_protected", "cap.generic",
            "mutation.uncertain.title", "mutation.uncertain.body",
            "unavailable.title", "unavailable.body"
        ]
        for key in keys {
            #expect(L10n.text(key) != key, "missing localization for \(key)")
        }
    }

    @Test("language override resolves the chosen bundle immediately")
    func languageOverride() {
        let settings = LanguageSettings.shared
        let original = settings.preference
        defer { settings.preference = original }
        settings.preference = .zhHans
        #expect(L10n.text("status.headline.quiet") == "一切如常")
        settings.preference = .en
        #expect(L10n.text("status.headline.quiet") == "Nothing needs attention")
    }

    @Test("zh-Hans translations resolve")
    func chineseResolves() {
        let bundle = Bundle.module
        // SwiftPM lowercases the region subtag: zh-Hans.lproj → zh-hans.lproj
        guard let zhPath = bundle.path(forResource: "zh-hans", ofType: "lproj")
            ?? bundle.path(forResource: "zh-Hans", ofType: "lproj"),
              let zhBundle = Bundle(path: zhPath)
        else {
            Issue.record("zh-Hans.lproj missing from resource bundle")
            return
        }
        let value = zhBundle.localizedString(forKey: "status.headline.quiet", value: nil, table: nil)
        #expect(value == "一切如常")
    }
}
