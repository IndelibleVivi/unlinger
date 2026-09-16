import AppKit
import SwiftUI

/// AppKit owns the ordinary window lifecycle so menu and notification routes
/// never depend on a lazily instantiated SwiftUI scene. The window exists for
/// routing from launch, while its hosted SwiftUI tree is created only on first
/// presentation and then owns navigation and all product state.
@MainActor
public final class AppWindowController: NSWindowController {
    public static let initialContentSize = AppSurfaceLayout.contentSize

    private var makeContent: (@MainActor () -> AnyView)?

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
    /// but an ordinary window may never be requested. Delay the SwiftUI and
    /// Accessibility graph until first presentation so a hidden host cannot
    /// accumulate observation generations while five-second polling runs.
    func prepareContentForPresentation() {
        guard window?.contentViewController == nil, let makeContent else { return }
        window?.contentViewController = NSHostingController(rootView: makeContent())
        self.makeContent = nil
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
