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

    public var preference: Preference {
        didSet {
            UserDefaults.standard.set(preference.rawValue, forKey: Self.defaultsKey)
        }
    }

    /// The bundle copy resolves from. Computed off `preference` so Observation
    /// tracking flows through it.
    public var bundle: Bundle {
        Self.resolveBundle(for: preference)
    }

    private static let defaultsKey = "ui.language"

    private init() {
        let stored = UserDefaults.standard.string(forKey: Self.defaultsKey)
        preference = stored.flatMap(Preference.init(rawValue:)) ?? .system
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
