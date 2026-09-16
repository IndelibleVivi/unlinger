import Foundation
import Observation

/// UI language preference, persisted in UserDefaults. `system` follows macOS;
/// explicit choices resolve copy from the matching `.lproj` in `Bundle.module`.
/// Tracked by Observation: every view body reads copy through `L10n`, which
/// reads `bundle`, so changing `preference` re-renders the UI immediately.
@MainActor
@Observable
public final class LanguageSettings {
    /// UI-only state, enforced on the main actor rather than relying on an
    /// unchecked shared-mutable exemption.
    public static let shared = LanguageSettings()

    public enum Preference: String, CaseIterable, Sendable {
        case system
        case en
        case zhHans = "zh-Hans"

        public var copyKey: String {
            switch self {
            case .system: "language.system"
            case .en: "language.en"
            case .zhHans: "language.zh_hans"
            }
        }
    }

    public private(set) var preference: Preference

    /// Stable localization resources. Popup and Accessibility updates reuse
    /// these objects until the user actually changes language.
    public private(set) var bundle: Bundle

    /// Locale for formatted values that sit beside localized copy. Explicit
    /// language choices must not leave relative dates in the system language.
    public private(set) var locale: Locale

    private static func resolveLocale(for preference: Preference) -> Locale {
        switch preference {
        case .system: .autoupdatingCurrent
        case .en: Locale(identifier: "en")
        case .zhHans: Locale(identifier: "zh-Hans")
        }
    }

    private static let defaultsKey = "ui.language"

    private init() {
        let stored = UserDefaults.standard.string(forKey: Self.defaultsKey)
        let preference = stored.flatMap(Preference.init(rawValue:)) ?? .system
        self.preference = preference
        self.bundle = Self.resolveBundle(for: preference)
        self.locale = Self.resolveLocale(for: preference)
    }

    public func setPreference(_ preference: Preference) {
        guard preference != self.preference else { return }
        self.preference = preference
        bundle = Self.resolveBundle(for: preference)
        locale = Self.resolveLocale(for: preference)
        UserDefaults.standard.set(preference.rawValue, forKey: Self.defaultsKey)
    }

    private static func resolveBundle(for preference: Preference) -> Bundle {
        guard preference != .system else { return .module }
        // SwiftPM lowercases the region subtag: zh-Hans.lproj → zh-hans.lproj
        let names = preference == .zhHans ? ["zh-hans", "zh-Hans"] : [preference.rawValue]
        for name in names {
            if let path = Bundle.module.path(forResource: name, ofType: "lproj"),
               let bundle = Bundle(path: path) {
                return bundle
            }
        }
        return .module
    }
}
