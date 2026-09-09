import Foundation
import Testing
@testable import UnlingerKit

@Suite("Localization")
@MainActor
struct LocalizationTests {
    @Test("every copy key used by the mapping layer resolves to real text")
    func keysResolve() {
        let keys = [
            "browser.overview.clear", "browser.overview.active", "browser.overview.verifying",
            "browser.overview.confirmed", "browser.overview.reclaiming", "browser.overview.protected",
            "browser.overview.attention", "browser.overview.updating", "browser.overview.unavailable",
            "browser.overview.confirmed.report_only", "browser.overview.confirmed.enforce",
            "browser.mode.observe_only", "browser.mode.auto_cleanup", "browser.mode.paused",
            "browser.family.playwright", "browser.family.agent_browser",
            "browser.family.puppeteer", "browser.family.chrome_for_testing",
            "browser.session.active", "browser.session.verifying", "browser.session.confirmed",
            "browser.session.reclaiming", "browser.session.protected", "browser.session.ambiguous",
            "browser.session.reason.protected_generic", "browser.session.reason.ambiguous_generic",
            "browser.coverage.unsupported_product", "browser.coverage.unsupported_version",
            "browser.coverage.version_unavailable", "browser.coverage.mixed_versions",
            "browser.coverage.controller_unverified", "browser.coverage.observation_only",
            "browser.coverage.control_path_incomplete",
            "browser.settlement.cleared", "browser.settlement.residue",
            "browser.settlement.revived", "browser.settlement.failed",
            "attention.daemon_unhealthy", "attention.residue", "attention.failed.delivery_unknown",
            "attention.failed.generic", "attention.revived", "attention.event_source",
            "attention.storage_recovered", "attention.generic", "attention.more",
            "attention.section", "attention.hint", "actions.hint",
            "navigation.back", "navigation.open_window",
            "language.menu", "language.system", "language.en", "language.zh_hans",
            "cap.action.not_paused", "cap.action.no_blocked_cleanup",
            "cap.action.already_protected", "cap.action.not_protected", "cap.generic",
            "mutation.uncertain.title", "mutation.uncertain.body",
            "unavailable.body", "browser.session.ended_without_intervention",
            "outcome.ended_without_intervention", "detail.cleanup.without_intervention"
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
        #expect(L10n.text("browser.overview.clear") == "未发现受支持的浏览器遗留")
        settings.preference = .en
        #expect(L10n.text("browser.overview.clear") == "No supported browser leftovers found")
    }

    @Test("connection failure does not claim that the service stopped")
    func unavailableDoesNotClaimStopped() {
        let settings = LanguageSettings.shared
        let original = settings.preference
        defer { settings.preference = original }
        settings.preference = .en
        let english = L10n.text("unavailable.body")
        #expect(english.contains("may still be running"))
        #expect(!english.contains("no observation is taking place"))
        settings.preference = .zhHans
        let chinese = L10n.text("unavailable.body")
        #expect(chinese.contains("后台可能仍在观察或自动清理"))
        #expect(!chinese.contains("当前没有在进行任何观察"))
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
        let value = zhBundle.localizedString(forKey: "browser.overview.clear", value: nil, table: nil)
        #expect(value == "未发现受支持的浏览器遗留")
    }

    @Test("English and Chinese localization keys stay exactly in parity")
    func localizationParity() throws {
        let bundle = Bundle.module
        let English = try localizationKeys(bundle: bundle, localization: "en")
        let Chinese = try localizationKeys(bundle: bundle, localization: "zh-hans")
        #expect(English == Chinese)
    }

    @Test("VoiceOver copy names browser state, reason, mode, and freshness in both languages")
    func browserAccessibilityCopy() async throws {
        let settings = LanguageSettings.shared
        let original = settings.preference
        defer { settings.preference = original }

        let state = AppState(client: FixtureClient.scenario("browser-protected-unsupported"))
        await state.refresh()
        let overview = state.browserOverview
        let session = try #require(overview.sessions.first)

        settings.preference = .en
        let englishOverview = BrowserProductCopy.overviewAccessibilityLabel(overview)
        let englishSession = BrowserProductCopy.sessionAccessibilityLabel(session)
        #expect(englishOverview.contains("Observe only"))
        #expect(englishOverview.contains("Verified"))
        #expect(englishSession.contains("Left untouched"))
        #expect(englishSession.contains("supported"))
        #expect(englishSession.contains("Chrome for Testing 151.0.7922.35"))

        settings.preference = .zhHans
        let chineseOverview = BrowserProductCopy.overviewAccessibilityLabel(overview)
        let chineseSession = BrowserProductCopy.sessionAccessibilityLabel(session)
        #expect(chineseOverview.contains("仅观察"))
        #expect(chineseOverview.contains("验证"))
        #expect(chineseSession.contains("已安全保留"))
        #expect(chineseSession.contains("支持范围"))
        #expect(chineseSession.contains("Chrome for Testing 151.0.7922.35"))
    }

    private func localizationKeys(bundle: Bundle, localization: String) throws -> Set<String> {
        let path = try #require(
            bundle.path(forResource: localization, ofType: "lproj")
                ?? bundle.path(forResource: localization == "zh-hans" ? "zh-Hans" : localization, ofType: "lproj")
        )
        let stringsPath = URL(fileURLWithPath: path).appending(path: "Localizable.strings").path
        let dictionary = try #require(NSDictionary(contentsOfFile: stringsPath) as? [String: String])
        return Set(dictionary.keys)
    }
}
