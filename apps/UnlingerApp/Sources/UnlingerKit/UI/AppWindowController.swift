import AppKit
import SwiftUI

/// AppKit owns the ordinary window lifecycle so menu and notification routes
/// never depend on a lazily instantiated SwiftUI scene. The window exists for
/// routing from launch, while its hosted SwiftUI tree exists only while the
/// window is presented. The persistent router restores navigation on reopen.
@MainActor
public final class AppWindowController: NSWindowController, NSWindowDelegate {
    public static let initialContentSize = AppSurfaceLayout.contentSize

    private let makeContent: @MainActor () -> AnyView

    public init<Content: View>(
        title: String,
        @ViewBuilder content: @escaping () -> Content
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
        makeContent = {
            AnyView(
                content()
                    .frame(
                        width: Self.initialContentSize.width,
                        height: Self.initialContentSize.height
                    )
            )
        }
        window.center()
        super.init(window: window)
        window.delegate = self
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("init(coder:) is unavailable")
    }

    public func show() {
        prepareContentForPresentation()
        NSApplication.shared.activate(ignoringOtherApps: true)
        showWindow(nil)
        window?.makeKeyAndOrderFront(nil)
    }

    /// The menu client owns this controller for its whole process lifetime,
    /// but an ordinary window may never be requested. Materialize the SwiftUI
    /// and Accessibility graph only while the window is presented so a hidden
    /// host cannot accumulate observation generations while polling runs.
    func prepareContentForPresentation() {
        guard window?.contentViewController == nil else { return }
        window?.contentViewController = NSHostingController(rootView: makeContent())
    }

    public func windowWillClose(_ notification: Notification) {
        window?.contentViewController = nil
    }

    var isConfiguredForTesting: Bool {
        guard let window else { return false }
        return window.styleMask.contains(.titled)
            && window.styleMask.contains(.closable)
            && window.styleMask.contains(.miniaturizable)
            && !window.isReleasedWhenClosed
    }

    var isContentLoadedForTesting: Bool {
        window?.contentViewController != nil
    }

}
