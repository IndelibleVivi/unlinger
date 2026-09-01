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
@MainActor
final class UnlingerAppDelegate: NSObject, NSApplicationDelegate {
    let environment = LaunchMode.makeEnvironment()
    private var menuBarController: MenuBarPopoverController?
    private var appWindowController: AppWindowController?

    func applicationWillFinishLaunching(_ notification: Notification) {
        NSApplication.shared.setActivationPolicy(LaunchMode.windowed ? .regular : .accessory)
    }

    func applicationDidFinishLaunching(_ notification: Notification) {
        let appWindowController = AppWindowController(title: "Unlinger") {
            RootView(
                environment: self.environment,
                allowsWindowPresentation: false
            )
        }
        self.appWindowController = appWindowController
        environment.router.registerWindowOpener { [weak self, weak appWindowController] in
            self?.menuBarController?.close()
            appWindowController?.show()
        }

        if LaunchMode.windowed {
            appWindowController.show()
        } else {
            menuBarController = MenuBarPopoverController(
                title: "Unlinger",
                icon: MenuBarIcon.image
            ) {
                RootView(
                    environment: self.environment,
                    allowsWindowPresentation: true
                )
            }
        }
    }

    func applicationShouldHandleReopen(
        _ sender: NSApplication,
        hasVisibleWindows flag: Bool
    ) -> Bool {
        if !flag { appWindowController?.show() }
        return true
    }

    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool {
        false
    }
}

private struct RootView: View {
    let environment: UnlingerEnvironment
    let allowsWindowPresentation: Bool

    var body: some View {
        MenuPopover(allowsWindowPresentation: allowsWindowPresentation)
            .environment(environment.state)
            .environment(environment.router)
            .environment(environment.settings)
    }
}

@main
enum UnlingerApplication {
    @MainActor
    static func main() {
        let application = NSApplication.shared
        let delegate = UnlingerAppDelegate()
        application.delegate = delegate

        // NSApplication.delegate is weak. Keep the single owner of the
        // status item, ordinary window, router, and polling state alive for
        // exactly the blocking AppKit run-loop lifetime.
        withExtendedLifetime(delegate) {
            application.run()
        }
    }
}
