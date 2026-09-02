import Foundation

/// All user-visible copy resolves through here, from `Localizable.strings`
/// bundled in `Bundle.module`. Unknown reason/evidence IDs fall back to
/// generic keys chosen by the mapping layer — never interpolated raw.
@MainActor
public enum L10n {
    public static func text(_ key: String) -> String {
        LanguageSettings.shared.bundle.localizedString(forKey: key, value: nil, table: nil)
    }

    public static func text(_ key: String, _ arguments: CVarArg...) -> String {
        String(
            format: text(key),
            locale: LanguageSettings.shared.locale,
            arguments: arguments
        )
    }
}
