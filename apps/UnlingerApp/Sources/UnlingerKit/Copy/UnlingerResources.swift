import Foundation

/// The assembled App owns its resources. SwiftPM's generated accessor remains
/// appropriate only for unbundled development executables and package tests.
public enum UnlingerResources {
    public static let isPackagedApp = Bundle.main.bundleURL.pathExtension == "app"

    public static let bundle: Bundle = {
        do {
            return try resolve(application: .main, development: { .module })
        } catch {
            fatalError("Unlinger packaged resources are missing")
        }
    }()

    enum ResourceError: Error {
        case missingPackagedBundle
        case notPackagedApp
        case invalidLocalization
        case invalidFixture
    }

    static func resolve(application: Bundle, development: () -> Bundle) throws -> Bundle {
        guard application.bundleURL.pathExtension == "app" else { return development() }
        guard let resources = application.resourceURL,
              let bundle = Bundle(url: resources.appending(path: "UnlingerApp_UnlingerKit.bundle")) else {
            throw ResourceError.missingPackagedBundle
        }
        return bundle
    }

    /// Runs before AppKit, IPC, preferences or notification setup in the bundle
    /// gate. It exercises the same resource and fixture owners as ordinary use.
    public static func verifyPackagedResources() throws {
        guard isPackagedApp else { throw ResourceError.notPackagedApp }
        for (language, expected) in [
            ("en", "No supported browser leftovers found"),
            ("zh-hans", "未发现受支持的浏览器遗留"),
        ] {
            guard let path = bundle.path(forResource: language, ofType: "lproj"),
                  let localized = Bundle(path: path),
                  localized.localizedString(forKey: "browser.overview.clear", value: nil, table: nil) == expected else {
                throw ResourceError.invalidLocalization
            }
        }
        for (schema, name) in [
            (3, "status-report-only"),
            (4, "browser-overview-protected"),
            (5, "browser-overview-impact-empty"),
        ] {
            let data = try FixtureStore.data(named: name, schemaVersion: schema)
            let resourceComponents = bundle.bundleURL.standardizedFileURL.pathComponents
            let fixtureComponents = FixtureStore.url(named: name, schemaVersion: schema).standardizedFileURL.pathComponents
            guard let document = try JSONSerialization.jsonObject(with: data) as? [String: Any],
                  document["schema_version"] as? Int == schema,
                  Array(fixtureComponents.prefix(resourceComponents.count)) == resourceComponents else {
                throw ResourceError.invalidFixture
            }
        }
    }
}
