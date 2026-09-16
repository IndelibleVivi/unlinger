import AppKit
import SwiftUI
import Testing
@testable import UnlingerKit

@Suite("App window controller")
@MainActor
struct AppWindowControllerTests {
    @Test("materializes one reusable SwiftUI host only on first presentation")
    func configuresWindow() {
        _ = NSApplication.shared
        var contentBuildCount = 0
        let makeContent = {
            contentBuildCount += 1
            return Text("Status")
        }
        let controller = AppWindowController(title: "Unlinger") {
            makeContent()
        }

        #expect(controller.isConfiguredForTesting)
        #expect(!controller.isContentLoadedForTesting)
        #expect(contentBuildCount == 0)

        controller.prepareContentForPresentation()
        #expect(controller.isContentLoadedForTesting)
        #expect(contentBuildCount == 1)

        controller.prepareContentForPresentation()
        #expect(controller.isContentLoadedForTesting)
        #expect(contentBuildCount == 1)
        #expect(AppWindowController.initialContentSize == AppSurfaceLayout.contentSize)
        #expect(MenuBarPopoverController.contentSize == AppSurfaceLayout.contentSize)
        #expect(MenuPopover.contentSize == AppSurfaceLayout.contentSize)
    }
}
