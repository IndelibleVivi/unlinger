import AppKit
import SwiftUI
import Testing
@testable import UnlingerKit

@Suite("App window controller")
@MainActor
struct AppWindowControllerTests {
    @Test("owns one reusable ordinary window with hosted SwiftUI content")
    func configuresWindow() {
        _ = NSApplication.shared
        let controller = AppWindowController(title: "Unlinger") {
            Text("Status")
        }

        #expect(controller.isConfiguredForTesting)
    }
}
