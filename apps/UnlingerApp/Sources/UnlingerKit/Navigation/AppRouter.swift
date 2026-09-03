import Observation

public enum Route: Hashable, Sendable {
    case history
    case settings
    case incident(String)
}

/// One navigation authority for exactly one NavigationStack host. Notification
/// callbacks use the ordinary-window instance rather than a view-local path.
@Observable
@MainActor
public final class AppRouter {
    public var path: [Route] = []
    private var windowOpener: (@MainActor () -> Void)?

    public init() {}

    public func registerWindowOpener(_ opener: @escaping @MainActor () -> Void) {
        windowOpener = opener
    }

    public func open(_ route: NotificationRoute) {
        switch route {
        case .status: path = []
        case .incident(let incidentID): path = [.incident(incidentID)]
        }
        windowOpener?()
    }

    public func openStatus() {
        path = []
        windowOpener?()
    }

    /// Requests presentation without changing this host's current route. The
    /// menu coordinator may consume that route into the ordinary-window host.
    public func presentCurrentRoute() {
        windowOpener?()
    }

    /// Explicit popover navigation must not rely on scene-provided toolbar
    /// chrome, which is not consistently visible in an AppKit-hosted view.
    public func goBack() {
        guard !path.isEmpty else { return }
        path.removeLast()
    }
}

/// Owns independent navigation storage for the two long-lived AppKit hosts.
/// Route handoff consumes the hidden popover destination graph after copying
/// it to the ordinary window, so two NavigationStacks never bind one path.
@MainActor
public final class AppNavigationCoordinator {
    public let menu = AppRouter()
    public let window = AppRouter()

    public init() {}

    public func handOffMenuRouteToWindow() {
        window.path = menu.path
        menu.path.removeAll(keepingCapacity: false)
    }
}
