import Observation

public enum Route: Hashable, Sendable {
    case history
    case settings
    case incident(String)
}

/// One navigation authority shared by the menu popover and compact notification
/// window. Notification callbacks never mutate a view-local NavigationPath.
@Observable
@MainActor
public final class AppRouter {
    public var path: [Route] = []
    public private(set) var presentationRequested = false
    private var windowOpener: (@MainActor () -> Void)?

    public init() {}

    public func registerWindowOpener(_ opener: @escaping @MainActor () -> Void) {
        windowOpener = opener
    }

    public func open(_ route: NotificationRoute) {
        presentationRequested = true
        switch route {
        case .status: path = []
        case .incident(let incidentID): path = [.incident(incidentID)]
        }
        windowOpener?()
    }

    public func openStatus() {
        presentationRequested = true
        path = []
        windowOpener?()
    }

    /// Presents the ordinary window without changing the route already chosen
    /// in the menu-bar popover.
    public func presentCurrentRoute() {
        presentationRequested = true
        windowOpener?()
    }

    /// Explicit popover navigation must not rely on scene-provided toolbar
    /// chrome, which is not consistently visible in an AppKit-hosted view.
    public func goBack() {
        guard !path.isEmpty else { return }
        path.removeLast()
    }
}
