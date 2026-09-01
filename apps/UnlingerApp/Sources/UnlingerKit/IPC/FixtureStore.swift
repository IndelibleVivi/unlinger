import Foundation

/// Locates the canonical v3 fixtures that live in `Contract/v3/` next to this
/// package. Fixtures are read in place — never copied into the app bundle —
/// so previews and tests always exercise the Rust-roundtripped wire truth.
///
/// Two lookup modes: source-tree path (previews/tests/dev runs from the repo)
/// and bundled copies under `Fixtures/` in the resource bundle, which
/// `scripts/bundle.sh` copies in so a standalone `.app` can still run fixture
/// scenarios away from the repo.
public enum FixtureStore {
    private static let sourceTreeDirectory = URL(fileURLWithPath: #filePath)
        .deletingLastPathComponent() // IPC/
        .deletingLastPathComponent() // UnlingerKit/
        .deletingLastPathComponent() // Sources/
        .deletingLastPathComponent() // UnlingerApp/
        .appending(path: "Contract/v3", directoryHint: .isDirectory)

    public static func url(named name: String) -> URL {
        let sourceURL = sourceTreeDirectory.appending(path: "\(name).json")
        if FileManager.default.isReadableFile(atPath: sourceURL.path()) {
            return sourceURL
        }
        return bundledDirectory.appending(path: "\(name).json")
    }

    public static func data(named name: String) throws -> Data {
        // Prefer the in-place source-tree copy; if it is unreachable (e.g.
        // macOS removable-volume permission on an external drive), fall back
        // to the copy bundled inside the .app.
        if let data = try? Data(contentsOf: sourceTreeDirectory.appending(path: "\(name).json")) {
            return data
        }
        return try Data(contentsOf: bundledDirectory.appending(path: "\(name).json"))
    }

    private static var bundledDirectory: URL {
        Bundle.module.bundleURL.appending(path: "Fixtures", directoryHint: .isDirectory)
    }
}
