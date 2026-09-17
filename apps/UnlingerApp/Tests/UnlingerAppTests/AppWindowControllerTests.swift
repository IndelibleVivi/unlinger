import AppKit
import SwiftUI
import Testing
@testable import UnlingerKit

@Suite("App window controller")
@MainActor
struct AppWindowControllerTests {
    @Test("materializes a SwiftUI host only while the reusable window is presented")
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

        controller.windowWillClose(Notification(name: NSWindow.willCloseNotification))
        #expect(!controller.isContentLoadedForTesting)

        controller.prepareContentForPresentation()
        #expect(controller.isContentLoadedForTesting)
        #expect(contentBuildCount == 2)
        #expect(AppWindowController.initialContentSize == AppSurfaceLayout.contentSize)
        #expect(MenuBarPopoverController.contentSize == AppSurfaceLayout.contentSize)
        #expect(MenuPopover.contentSize == AppSurfaceLayout.contentSize)
    }

    @Test("closing clears and releases the exact hosted controller")
    func closingReleasesHostedController() async throws {
        _ = NSApplication.shared
        let controller = AppWindowController(title: "Unlinger") {
            Text("Status")
        }

        controller.prepareContentForPresentation()
        weak var hostedController: NSViewController?
        let hostWasLoaded = autoreleasepool {
            guard let host = controller.window?.contentViewController else { return false }
            hostedController = host
            return true
        }

        controller.windowWillClose(Notification(name: NSWindow.willCloseNotification))
        await Task.yield()

        #expect(hostWasLoaded)
        #expect(controller.window?.contentViewController == nil)
        #expect(hostedController == nil)
    }
}
