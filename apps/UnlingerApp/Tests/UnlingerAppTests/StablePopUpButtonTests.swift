import AppKit
import Testing
@testable import UnlingerKit

@Suite("Stable native popup")
@MainActor
struct StablePopUpButtonTests {
    @Test("equal configuration retains the exact native menu items")
    func equalConfigurationIsIdempotent() throws {
        let coordinator = StablePopUpButton.Coordinator(onSelect: { _ in })
        let button = NSPopUpButton(frame: .zero, pullsDown: true)
        let configuration = actionConfiguration()

        coordinator.apply(configuration, to: button)
        let firstActionItem = try #require(button.item(at: 1))
        coordinator.apply(configuration, to: button)

        #expect(button.item(at: 1) === firstActionItem)
        #expect(button.numberOfItems == 3)
        #expect(button.item(at: 2)?.state == .on)
        #expect(button.accessibilityLabel() == "Language")
    }

    @Test("selection dispatches its stable identifier")
    func selectionDispatchesIdentifier() throws {
        var selectedID: String?
        let coordinator = StablePopUpButton.Coordinator { selectedID = $0 }
        let button = NSPopUpButton(frame: .zero, pullsDown: false)
        let configuration = StablePopUpButton.Configuration(
            style: .selection,
            title: "Notifications",
            systemImageName: nil,
            showsTitle: true,
            accessibilityLabel: "Notifications",
            toolTip: "",
            isEnabled: true,
            selectedID: "attention",
            items: [
                StablePopUpItem(id: "off", title: "Off"),
                StablePopUpItem(id: "attention", title: "Attention")
            ]
        )

        coordinator.apply(configuration, to: button)
        button.selectItem(at: 0)
        coordinator.didSelect(button)

        #expect(selectedID == "off")
    }

    @Test("a reused native popup converts between action and selection modes")
    func reusedButtonSynchronizesPullsDownInBothDirections() throws {
        let coordinator = StablePopUpButton.Coordinator(onSelect: { _ in })
        let button = NSPopUpButton(frame: .zero, pullsDown: true)

        coordinator.apply(actionConfiguration(), to: button)
        #expect(button.pullsDown)

        coordinator.apply(selectionConfiguration(), to: button)
        #expect(!button.pullsDown)
        #expect(button.selectedItem?.representedObject as? String == "attention")

        coordinator.apply(actionConfiguration(), to: button)
        #expect(button.pullsDown)
        #expect(button.item(at: 0)?.representedObject == nil)
        #expect(button.item(at: 1)?.representedObject as? String == "system")
    }

    private func actionConfiguration() -> StablePopUpButton.Configuration {
        StablePopUpButton.Configuration(
            style: .action,
            title: "Language",
            systemImageName: "globe",
            showsTitle: false,
            accessibilityLabel: "Language",
            toolTip: "Language",
            isEnabled: true,
            selectedID: nil,
            items: [
                StablePopUpItem(id: "system", title: "System"),
                StablePopUpItem(id: "en", title: "English", isMarked: true)
            ]
        )
    }

    private func selectionConfiguration() -> StablePopUpButton.Configuration {
        StablePopUpButton.Configuration(
            style: .selection,
            title: "Notifications",
            systemImageName: nil,
            showsTitle: true,
            accessibilityLabel: "Notifications",
            toolTip: "",
            isEnabled: true,
            selectedID: "attention",
            items: [
                StablePopUpItem(id: "off", title: "Off"),
                StablePopUpItem(id: "attention", title: "Attention")
            ]
        )
    }
}
