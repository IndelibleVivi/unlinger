import AppKit
import SwiftUI

struct StablePopUpItem: Equatable, Identifiable {
    let id: String
    let title: String
    var isMarked = false
}

/// A native popup whose item list is rebuilt only when its semantic inputs
/// change. This keeps macOS Accessibility updates on an idempotent AppKit path
/// instead of SwiftUI's repeatedly regenerated popup-item adaptor.
struct StablePopUpButton: NSViewRepresentable {
    enum Style: Equatable {
        case action
        case selection
    }

    let style: Style
    let title: String
    var systemImageName: String?
    var showsTitle = true
    let accessibilityLabel: String
    var toolTip = ""
    var isEnabled = true
    var selectedID: String?
    let items: [StablePopUpItem]
    let onSelect: @MainActor (String) -> Void

    func makeCoordinator() -> Coordinator {
        Coordinator(onSelect: onSelect)
    }

    func makeNSView(context: Context) -> NSPopUpButton {
        let button = NSPopUpButton(frame: .zero, pullsDown: style == .action)
        button.target = context.coordinator
        button.action = #selector(Coordinator.didSelect(_:))
        return button
    }

    func updateNSView(_ button: NSPopUpButton, context: Context) {
        context.coordinator.onSelect = onSelect
        let configuration = Configuration(
            style: style,
            title: title,
            systemImageName: systemImageName,
            showsTitle: showsTitle,
            accessibilityLabel: accessibilityLabel,
            toolTip: toolTip,
            isEnabled: isEnabled,
            selectedID: selectedID,
            items: items
        )
        context.coordinator.apply(configuration, to: button)
    }

    @MainActor
    final class Coordinator: NSObject {
        var onSelect: @MainActor (String) -> Void
        var configuration: Configuration?

        init(onSelect: @escaping @MainActor (String) -> Void) {
            self.onSelect = onSelect
        }

        func apply(_ next: Configuration, to button: NSPopUpButton) {
            guard configuration != next else { return }
            configuration = next

            button.removeAllItems()
            button.isEnabled = next.isEnabled
            button.toolTip = next.toolTip
            button.setAccessibilityLabel(next.accessibilityLabel)

            switch next.style {
            case .action:
                button.addItem(withTitle: next.showsTitle ? next.title : "")
                if let header = button.item(at: 0), let systemImageName = next.systemImageName {
                    header.image = NSImage(
                        systemSymbolName: systemImageName,
                        accessibilityDescription: next.accessibilityLabel
                    )
                }
                for item in next.items {
                    button.addItem(withTitle: item.title)
                    guard let menuItem = button.lastItem else { continue }
                    menuItem.representedObject = item.id as NSString
                    menuItem.state = item.isMarked ? .on : .off
                }
                button.imagePosition = next.showsTitle ? .imageLeading : .imageOnly
                button.selectItem(at: 0)
            case .selection:
                for item in next.items {
                    button.addItem(withTitle: item.title)
                    button.lastItem?.representedObject = item.id as NSString
                }
                if let selectedID = next.selectedID,
                   let index = next.items.firstIndex(where: { $0.id == selectedID }) {
                    button.selectItem(at: index)
                }
            }
        }

        @objc
        func didSelect(_ sender: NSPopUpButton) {
            guard let identifier = sender.selectedItem?.representedObject as? String else {
                return
            }
            if configuration?.style == .action {
                sender.selectItem(at: 0)
            }
            onSelect(identifier)
        }
    }

    struct Configuration: Equatable {
        let style: Style
        let title: String
        let systemImageName: String?
        let showsTitle: Bool
        let accessibilityLabel: String
        let toolTip: String
        let isEnabled: Bool
        let selectedID: String?
        let items: [StablePopUpItem]
    }
}
