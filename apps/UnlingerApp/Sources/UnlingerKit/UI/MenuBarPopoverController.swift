import AppKit
import SwiftUI

/// AppKit owns the status item and popover lifecycle. This avoids coupling the
/// menu client to macOS 26's hosted `MenuBarExtra` scene replication while the
/// existing SwiftUI content continues to own presentation and data flow.
@MainActor
public final class MenuBarPopoverController: NSObject, NSPopoverDelegate {
    public static let contentSize = AppSurfaceLayout.contentSize
    public static let statusItemAutosaveName = "app.unlinger.menu.primary"

    private let statusItem: NSStatusItem
    private let popover = NSPopover()
    private let makeContent: @MainActor () -> AnyView

    public init<Content: View>(
        title: String,
        @ViewBuilder content: @escaping () -> Content
    ) {
        statusItem = NSStatusBar.system.statusItem(withLength: NSStatusItem.squareLength)
        statusItem.autosaveName = Self.statusItemAutosaveName
        makeContent = {
            AnyView(
                content()
                    .frame(width: Self.contentSize.width, height: Self.contentSize.height)
            )
        }

        super.init()

        popover.behavior = .transient
        popover.animates = true
        popover.contentSize = Self.contentSize
        popover.delegate = self

        guard let button = statusItem.button else { return }
        button.image = NSImage(
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
        return button.image != nil
            && button.target === self
            && button.action == #selector(togglePopover(_:))
            && popover.delegate === self
    }

    var contentSizeForTesting: NSSize {
        popover.contentSize
    }

    var statusItemAutosaveNameForTesting: String? {
        statusItem.autosaveName
    }

    var isStatusItemVisibleForTesting: Bool {
        statusItem.isVisible
    }

    var isContentLoadedForTesting: Bool {
        popover.contentViewController != nil
    }

    public func close() {
        if popover.isShown {
            popover.performClose(nil)
        }
    }

    public func popoverDidClose(_ notification: Notification) {
        popover.contentViewController = nil
    }

    @objc
    private func togglePopover(_ sender: NSStatusBarButton) {
        if popover.isShown {
            popover.performClose(sender)
            return
        }

        NSApplication.shared.activate(ignoringOtherApps: true)
        prepareContentForPresentation()
        popover.show(
            relativeTo: sender.bounds,
            of: sender,
            preferredEdge: .minY
        )
    }

    func prepareContentForPresentation() {
        guard popover.contentViewController == nil else { return }
        popover.contentViewController = NSHostingController(rootView: makeContent())
    }
}
