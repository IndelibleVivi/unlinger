import AppKit
import SwiftUI
import Testing
@testable import UnlingerKit

@Suite("Menu bar popover controller")
@MainActor
struct MenuBarPopoverControllerTests {
    @Test("owns an actionable status item and explicit popover size")
    func configuresStatusItemAndPopover() {
        _ = NSApplication.shared
        let controller = MenuBarPopoverController(title: "Unlinger") {
            Text("Status")
        }

        #expect(controller.isConfiguredForTesting)
        #expect(
            controller.statusItemAutosaveNameForTesting
                == MenuBarPopoverController.statusItemAutosaveName
        )
        #expect(controller.isStatusItemVisibleForTesting)
        #expect(!controller.isContentLoadedForTesting)
        controller.prepareContentForPresentation()
        #expect(controller.isContentLoadedForTesting)
        controller.popoverDidClose(Notification(name: NSPopover.didCloseNotification))
        #expect(!controller.isContentLoadedForTesting)
        #expect(controller.contentSizeForTesting == MenuBarPopoverController.contentSize)
        #expect(controller.contentSizeForTesting.width == 340)
        #expect(controller.contentSizeForTesting.height == 420)
    }

    @Test("status item mirrors the existing browser presentation without replacing its identity")
    func mirrorsBrowserPresentation() async throws {
        _ = NSApplication.shared
        let state = AppState(
            client: FixtureClient(
                statusFixture: "status-failed",
                incidentsFixture: "roster-current"
            ),
            mutationLedger: TestMutationJournal()
        )
        await state.refresh()
        let controller = MenuBarPopoverController(title: "Unlinger") {
            Text("Status")
        }

        controller.observe(state)

        #expect(controller.statusItemAutosaveNameForTesting
            == MenuBarPopoverController.statusItemAutosaveName)
        let expectedFailure = L10n.text("browser.overview.failed")
        #expect(controller.statusItemLabelForTesting == expectedFailure)
        #expect(controller.statusItemTintForTesting == .systemRed)
        #expect(controller.statusItemToolTipForTesting
            == expectedFailure + " · " + L10n.text(state.browserOverview.modeKey))
        #expect(controller.isStatusItemVisibleForTesting)
    }

    @Test("report-only and paused states stay distinct from terminal failure")
    func nonFailureAvailabilityIsNotFailed() async throws {
        _ = NSApplication.shared
        for fixture in ["status-report-only", "status-paused"] {
            let state = AppState(
                client: FixtureClient(
                    statusFixture: fixture,
                    incidentsFixture: "roster-current"
                ),
                mutationLedger: TestMutationJournal()
            )
            await state.refresh()
            let controller = MenuBarPopoverController(title: "Unlinger") {
                Text("Status")
            }

            controller.observe(state)

            #expect(!state.browserOverview.requiresServiceAttention)
            #expect(controller.statusItemTintForTesting == nil)
            #expect(controller.statusItemToolTipForTesting
                != L10n.text("browser.overview.failed"))
        }
    }
}
