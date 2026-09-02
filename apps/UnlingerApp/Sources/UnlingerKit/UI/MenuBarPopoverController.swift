import AppKit
import SwiftUI

/// AppKit owns the status item and popover lifecycle. This avoids coupling the
/// menu client to macOS 26's hosted `MenuBarExtra` scene replication while the
/// existing SwiftUI content continues to own presentation and data flow.
@MainActor
public final class MenuBarPopoverController: NSObject {
    public static let contentSize = AppSurfaceLayout.contentSize

    private let statusItem: NSStatusItem
    private let popover = NSPopover()

    public init<Content: View>(
        title: String,
        icon: NSImage?,
        @ViewBuilder content: () -> Content
    ) {
        statusItem = NSStatusBar.system.statusItem(withLength: NSStatusItem.squareLength)
        let root = content()
            .frame(width: Self.contentSize.width, height: Self.contentSize.height)

        super.init()

        popover.behavior = .transient
        popover.animates = true
        popover.contentSize = Self.contentSize
        popover.contentViewController = NSHostingController(rootView: root)

        guard let button = statusItem.button else { return }
        button.image = icon ?? NSImage(
            systemSymbolName: "circle.dashed",
            accessibilityDescription: title
        )
        button.image?.isTemplate = true
        button.toolTip = title
        button.setAccessibilityLabel(title)
        button.target = self
        button.action = #selector(togglePopover(_:))
    }

    var isConfiguredForTesting: Bool {
        guard let button = statusItem.button else { return false }
        return button.target === self
            && button.action == #selector(togglePopover(_:))
            && popover.contentViewController != nil
    }

    var contentSizeForTesting: NSSize {
        popover.contentSize
    }

    public func close() {
        if popover.isShown {
            popover.performClose(nil)
        }
    }

    @objc
    private func togglePopover(_ sender: NSStatusBarButton) {
        if popover.isShown {
            popover.performClose(sender)
            return
        }

        NSApplication.shared.activate(ignoringOtherApps: true)
        popover.show(
            relativeTo: sender.bounds,
            of: sender,
            preferredEdge: .minY
        )
    }
}
