import AppKit

/// The menu-bar template icon, generated from the imagegen source art by
/// `scripts/make-menubar-icon.swift`: black-with-alpha PNGs at @1x/@2x/@3x,
/// tinted by the system (auto light/dark menu bar). Falls back to nil — and
/// the caller falls back to an SF Symbol — if the assets are missing.
public enum MenuBarIcon {
    public static let image: NSImage? = {
        var reps: [NSImageRep] = []
        for name in ["menubar-icon", "menubar-icon@2x", "menubar-icon@3x"] {
            guard let url = Bundle.module.url(forResource: name, withExtension: "png"),
                  let decoded = NSImage(contentsOf: url),
                  let rep = decoded.representations.first else { continue }
            rep.size = NSSize(width: 18, height: 18)
            reps.append(rep)
        }
        guard !reps.isEmpty else { return nil }
        let image = NSImage(size: NSSize(width: 18, height: 18))
        reps.forEach(image.addRepresentation)
        image.isTemplate = true
        return image
    }()
}
