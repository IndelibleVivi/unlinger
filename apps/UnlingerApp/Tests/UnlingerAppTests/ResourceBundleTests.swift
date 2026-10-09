import Foundation
import Testing
@testable import UnlingerKit

@Suite("Packaged resource ownership")
struct ResourceBundleTests {
    private func application(at root: URL) throws -> Bundle {
        let contents = root.appending(path: "Synthetic.app/Contents")
        try FileManager.default.createDirectory(at: contents.appending(path: "Resources"), withIntermediateDirectories: true)
        let info: [String: String] = ["CFBundleIdentifier": "test.unlinger.resources", "CFBundlePackageType": "APPL"]
        try PropertyListSerialization.data(fromPropertyList: info, format: .xml, options: 0)
            .write(to: contents.appending(path: "Info.plist"))
        return try #require(Bundle(url: contents.deletingLastPathComponent()))
    }

    @Test func packagedApplicationUsesItsOwnResources() throws {
        let root = FileManager.default.temporaryDirectory.appending(path: UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: root) }
        let app = try application(at: root)
        let resourceURL = try #require(app.resourceURL).appending(path: "UnlingerApp_UnlingerKit.bundle")
        try FileManager.default.createDirectory(at: resourceURL, withIntermediateDirectories: true)
        let resolved = try UnlingerResources.resolve(application: app, development: {
            Issue.record("A packaged App must never evaluate the development accessor")
            return .main
        })
        #expect(resolved.bundleURL.standardizedFileURL.pathComponents == resourceURL.standardizedFileURL.pathComponents)
    }

    @Test func missingPackagedResourcesDoNotUseADevelopmentCopy() throws {
        let root = FileManager.default.temporaryDirectory.appending(path: UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: root) }
        let app = try application(at: root)
        #expect(throws: UnlingerResources.ResourceError.missingPackagedBundle) {
            try UnlingerResources.resolve(application: app, development: {
                Issue.record("Missing packaged resources must not be hidden by a build-tree copy")
                return .main
            })
        }
    }

    @Test func unbundledPackageTestUsesSwiftPMResources() throws {
        #expect(Bundle.main.bundleURL.pathExtension != "app")
        let resolved = try UnlingerResources.resolve(application: .main, development: { .module })
        #expect(resolved.bundleURL == Bundle.module.bundleURL)
    }
}
