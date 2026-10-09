import Foundation

/// Locates versioned canonical frontend fixtures next to this package.
/// Fixtures are read in place — never rewritten by the app —
/// so previews and tests always exercise the Rust-roundtripped wire truth.
///
/// Unbundled previews/tests/dev runs use the source-tree path. Packaged Apps
/// use only copies under `Fixtures/` in their owned resource bundle, which
/// `scripts/bundle.sh` copies in so a standalone `.app` can still run fixture
/// scenarios away from the repo.
public enum FixtureStore {
    private static let contractDirectory = URL(fileURLWithPath: #filePath)
        .deletingLastPathComponent() // IPC/
        .deletingLastPathComponent() // UnlingerKit/
        .deletingLastPathComponent() // Sources/
        .deletingLastPathComponent() // UnlingerApp/
        .appending(path: "Contract", directoryHint: .isDirectory)

    public static func url(named name: String, schemaVersion: Int = 3) -> URL {
        let sourceURL = sourceDirectory(schemaVersion: schemaVersion)
            .appending(path: "\(name).json")
        if !UnlingerResources.isPackagedApp,
           FileManager.default.isReadableFile(atPath: sourceURL.path()) {
            return sourceURL
        }
        return bundledDirectory(schemaVersion: schemaVersion)
            .appending(path: "\(name).json")
    }

    public static func data(named name: String, schemaVersion: Int = 3) throws -> Data {
        if !UnlingerResources.isPackagedApp, let data = try? Data(
            contentsOf: sourceDirectory(schemaVersion: schemaVersion)
                .appending(path: "\(name).json")
        ) {
            return data
        }
        return try Data(
            contentsOf: bundledDirectory(schemaVersion: schemaVersion)
                .appending(path: "\(name).json")
        )
    }

    private static func sourceDirectory(schemaVersion: Int) -> URL {
        contractDirectory.appending(path: "v\(schemaVersion)", directoryHint: .isDirectory)
    }

    private static func bundledDirectory(schemaVersion: Int) -> URL {
        UnlingerResources.bundle.bundleURL
            .appending(path: "Fixtures", directoryHint: .isDirectory)
            .appending(path: "v\(schemaVersion)", directoryHint: .isDirectory)
    }
}
