import AppKit
import Observation
import SwiftUI
import UnlingerKit

/// Play/debug modes via environment:
/// - `UNLINGER_FIXTURE=<scenario>` drives the UI from canonical v3 fixtures;
/// - `UNLINGER_WINDOW=1` shows a regular window for local visual QA.
enum LaunchMode {
    static var windowed: Bool {
        ProcessInfo.processInfo.environment["UNLINGER_WINDOW"] == "1"
    }

    static var fixtureScenario: String? {
        let value = ProcessInfo.processInfo.environment["UNLINGER_FIXTURE"]
        return value?.isEmpty == false ? value : nil
    }

    static var isBundledApp: Bool {
        Bundle.main.bundleURL.pathExtension == "app"
    }

    @MainActor
    static func makeEnvironment() -> UnlingerEnvironment {
        UnlingerEnvironment(fixtureScenario: fixtureScenario)
    }
}

@Observable
@MainActor
final class UnlingerEnvironment {
    let state: AppState
    let router: AppRouter
    let settings: AppSettings

    // Strong lifetime is part of the notification contract: the center keeps
    // its delegate weakly and click routing must survive for the whole App run.
    private let notificationScheduler: SystemNotificationScheduler?
    private let notificationCoordinator: NotificationCoordinator?

    init(fixtureScenario: String?) {
        let router = AppRouter()
        let settings = AppSettings()
        let client: any UnlingerClient
        if let fixtureScenario {
            client = FixtureClient.scenario(fixtureScenario)
        } else {
            client = SocketClient.default()
        }

        let scheduler: SystemNotificationScheduler?
        let coordinator: NotificationCoordinator?
        // Previews, fixture mode, `swift run`, and tests do not touch the real
        // Notification Center. Authorization belongs to the packaged App.
        if fixtureScenario == nil, LaunchMode.isBundledApp {
            let systemScheduler = SystemNotificationScheduler(router: router)
            scheduler = systemScheduler
            coordinator = NotificationCoordinator(
                scheduler: systemScheduler,
                mode: settings.notificationMode
            )
        } else {
            scheduler = nil
            coordinator = nil
        }

        self.router = router
        self.settings = settings
        self.notificationScheduler = scheduler
        self.notificationCoordinator = coordinator
        self.state = AppState(
            client: client,
            notificationCoordinator: coordinator
        )
        settings.attachNotificationCoordinator(coordinator)
        state.startPolling()
        Task { await settings.prepareNotifications() }
    }
}

/// Shared delegate: the bundle deliberately ships `LSUIElement = false` and
/// picks its activation policy at launch. The explicit App quit leaves the
/// daemon untouched.
final class UnlingerAppDelegate: NSObject, NSApplicationDelegate {
    func applicationWillFinishLaunching(_ notification: Notification) {
        NSApplication.shared.setActivationPolicy(LaunchMode.windowed ? .regular : .accessory)
    }

    func applicationShouldHandleReopen(
        _ sender: NSApplication,
        hasVisibleWindows flag: Bool
    ) -> Bool {
        if LaunchMode.windowed && !flag { sender.activate() }
        return true
    }
}

private struct RootView: View {
    let environment: UnlingerEnvironment

    var body: some View {
        MenuPopover()
            .environment(environment.state)
            .environment(environment.router)
            .environment(environment.settings)
    }
}

/// The menu label is always instantiated, so it is a reliable place to bind
/// AppRouter to SwiftUI's window-opening authority before a notification click.
private struct MenuLabel: View {
    @Environment(\.openWindow) private var openWindow
    let router: AppRouter

    var body: some View {
        Group {
            if let icon = MenuBarIcon.image {
                Image(nsImage: icon)
                    .accessibilityLabel("Unlinger")
            } else {
                Image(systemName: "circle.dashed")
                    .accessibilityLabel("Unlinger")
            }
        }
        .onAppear {
            router.registerWindowOpener {
                openWindow(id: "notification")
            }
        }
    }
}

/// macOS 14 lacks Scene.defaultLaunchBehavior(.suppressed). If SwiftUI creates
/// this secondary window at launch, it closes itself immediately; subsequent
/// exact notification routes reopen the same compact WindowGroup.
private struct NotificationWindowRoot: View {
    @Environment(\.dismissWindow) private var dismissWindow
    @Environment(\.openWindow) private var openWindow
    let environment: UnlingerEnvironment

    var body: some View {
        RootView(environment: environment)
            .onAppear {
                environment.router.registerWindowOpener {
                    openWindow(id: "notification")
                }
                if !environment.router.presentationRequested {
                    DispatchQueue.main.async {
                        dismissWindow(id: "notification")
                    }
                }
            }
    }
}

struct MenuBarUnlingerApp: App {
    @NSApplicationDelegateAdaptor(UnlingerAppDelegate.self) private var appDelegate
    @State private var environment = LaunchMode.makeEnvironment()

    var body: some Scene {
        MenuBarExtra(isInserted: .constant(true)) {
            RootView(environment: environment)
        } label: {
            MenuLabel(router: environment.router)
        }
        .menuBarExtraStyle(.window)

        WindowGroup("Unlinger", id: "notification") {
            NotificationWindowRoot(environment: environment)
        }
        .windowResizability(.contentSize)
    }
}

struct WindowedUnlingerApp: App {
    @NSApplicationDelegateAdaptor(UnlingerAppDelegate.self) private var appDelegate
    @State private var environment = LaunchMode.makeEnvironment()

    var body: some Scene {
        WindowGroup("Unlinger", id: "main") {
            RootView(environment: environment)
        }
        .windowResizability(.contentSize)

        WindowGroup("Unlinger", id: "notification") {
            NotificationWindowRoot(environment: environment)
        }
        .windowResizability(.contentSize)
    }
}
