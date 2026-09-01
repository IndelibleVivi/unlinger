import SwiftUI
import Testing
@testable import UnlingerKit

@Suite("Menu bar popover controller")
@MainActor
struct MenuBarPopoverControllerTests {
    @Test("owns an actionable status item and explicit popover size")
    func configuresStatusItemAndPopover() {
        let controller = MenuBarPopoverController(
            title: "Unlinger",
            icon: nil
        ) {
            Text("Status")
        }

        #expect(controller.isConfiguredForTesting)
        #expect(controller.contentSizeForTesting == MenuBarPopoverController.contentSize)
        #expect(controller.contentSizeForTesting.width == 340)
        #expect(controller.contentSizeForTesting.height == 420)
    }
}
