import SwiftUI

/// Root of the menu-bar and ordinary-window navigation surface. BrowserHomeView
/// owns the one canonical product overview used by both AppKit hosts.
public struct MenuPopover: View {
    @Environment(AppRouter.self) private var router
    private let allowsWindowPresentation: Bool

    public init(allowsWindowPresentation: Bool = false) {
        self.allowsWindowPresentation = allowsWindowPresentation
    }

    public var body: some View {
        @Bindable var router = router
        VStack(spacing: 0) {
            if !router.path.isEmpty, allowsWindowPresentation {
                routeControls
                Divider()
            }

            NavigationStack(path: $router.path) {
                BrowserHomeView()
                .padding()
                .navigationDestination(for: Route.self) { route in
                    switch route {
                    case .history:
                        HistoryView()
                    case .settings:
                        SettingsView()
                    case .incident(let incidentID):
                        IncidentDetailView(incidentID: incidentID)
                    }
                }
            }

            if router.path.isEmpty, allowsWindowPresentation {
                Divider()
                windowButton
                    .padding(.horizontal)
                    .padding(.vertical, 10)
            }
        }
        .frame(width: 360)
    }

    private var routeControls: some View {
        HStack(spacing: 12) {
            Button(L10n.text("navigation.back"), systemImage: "chevron.left") {
                router.goBack()
            }
            Spacer(minLength: 0)
            if allowsWindowPresentation {
                windowButton
            }
        }
        .controlSize(.small)
        .padding(.horizontal)
        .padding(.vertical, 10)
    }

    private var windowButton: some View {
        Button(L10n.text("navigation.open_window"), systemImage: "macwindow") {
            router.presentCurrentRoute()
        }
    }

}
