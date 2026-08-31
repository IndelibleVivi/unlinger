import Foundation
import Testing
@testable import UnlingerKit

@Suite("Diagnostics export")
struct DiagnosticsExporterTests {
    @Test("private atomic write replaces an existing export")
    func replacesExistingExport() throws {
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("unlinger-diagnostics-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: directory) }

        let target = directory.appendingPathComponent("diagnostics.json")
        try Data("old".utf8).write(to: target)
        let replacement = Data(#"{"redacted":true}"#.utf8)

        #expect(DiagnosticsExporter.writePrivateAtomically(replacement, to: target))
        #expect(try Data(contentsOf: target) == replacement)

        let attributes = try FileManager.default.attributesOfItem(atPath: target.path)
        let permissions = try #require(attributes[.posixPermissions] as? NSNumber)
        #expect(permissions.intValue & 0o077 == 0)
    }
}
