import AppKit
import SwiftUI

/// AppKit owns the ordinary window lifecycle so menu and notification routes
/// never depend on a lazily instantiated SwiftUI scene. The hosted SwiftUI
/// tree still owns navigation and all product state.
@MainActor
public final class AppWindowController: NSWindowController {
    public static let initialContentSize = NSSize(width: 340, height: 420)

    public init<Content: View>(
        title: String,
        @ViewBuilder content: () -> Content
    ) {
        let window = NSWindow(
            contentRect: NSRect(origin: .zero, size: Self.initialContentSize),
            styleMask: [.titled, .closable, .miniaturizable],
            backing: .buffered,
            defer: false
        )
        window.title = title
        window.isReleasedWhenClosed = false
        window.tabbingMode = .disallowed
        window.contentViewController = NSHostingController(
            rootView: content()
                .frame(
                    width: Self.initialContentSize.width,
                    height: Self.initialContentSize.height
                )
        )
        window.center()
        super.init(window: window)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("init(coder:) is unavailable")
    }

    public func show() {
        NSApplication.shared.activate(ignoringOtherApps: true)
        showWindow(nil)
        window?.makeKeyAndOrderFront(nil)
    }

    var isConfiguredForTesting: Bool {
        guard let window else { return false }
        return window.contentViewController != nil
            && window.styleMask.contains(.titled)
            && window.styleMask.contains(.closable)
            && window.styleMask.contains(.miniaturizable)
            && !window.isReleasedWhenClosed
    }
}
