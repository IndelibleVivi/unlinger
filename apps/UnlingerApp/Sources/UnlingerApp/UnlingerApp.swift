import SwiftUI
import UnlingerKit
/// Play/debug modes via environment:
/// - `UNLINGER_FIXTURE=<scenario>` drives the UI from a canonical fixture
///   scenario instead of a live socket;
/// - `UNLINGER_WINDOW=1` shows a regular window instead of the menu-bar
///   extra — handy on crowded menu bars and for screenshots.
enum LaunchMode {
    static var windowed: Bool {
        ProcessInfo.processInfo.environment["UNLINGER_WINDOW"] == "1"
    }

    @MainActor
    static func makeState() -> AppState {
        let env = ProcessInfo.processInfo.environment
        let client: any UnlingerClient = if let scenario = env["UNLINGER_FIXTURE"], !scenario.isEmpty {
            FixtureClient.scenario(scenario)
        } else {
            SocketClient.default()
        }
        let state = AppState(client: client)
        state.startPolling()
        return state
    }
}

/// Shared delegate: the bundle deliberately ships `LSUIElement = false` and
/// picks its activation policy at launch (a statically agent app can never
/// regain a Dock icon). Windowed/debug mode runs `.regular` so a closed
/// window can be reopened from the Dock; menu-bar mode runs `.accessory`.
final class UnlingerAppDelegate: NSObject, NSApplicationDelegate {
    func applicationWillFinishLaunching(_ notification: Notification) {
        NSApplication.shared.setActivationPolicy(LaunchMode.windowed ? .regular : .accessory)
    }

    /// Dock click with no windows: let SwiftUI's default reopen bring the
    /// window back, and make sure the app actually comes forward.
    func applicationShouldHandleReopen(_ sender: NSApplication, hasVisibleWindows flag: Bool) -> Bool {
        if LaunchMode.windowed && !flag {
            sender.activate()
        }
        return true
    }
}

struct MenuBarUnlingerApp: App {
    @NSApplicationDelegateAdaptor(UnlingerAppDelegate.self) private var appDelegate
    @State private var state = LaunchMode.makeState()

    var body: some Scene {
        MenuBarExtra(isInserted: .constant(true)) {
            MenuPopover()
                .environment(state)
        } label: {
            if let icon = MenuBarIcon.image {
                Image(nsImage: icon)
                    .accessibilityLabel("Unlinger")
            } else {
                Image(systemName: "circle.dashed")
                    .accessibilityLabel("Unlinger")
            }
        }
        .menuBarExtraStyle(.window)
    }
}

struct WindowedUnlingerApp: App {
    @NSApplicationDelegateAdaptor(UnlingerAppDelegate.self) private var appDelegate
    @State private var state = LaunchMode.makeState()

    var body: some Scene {
        WindowGroup("Unlinger", id: "main") {
            MenuPopover()
                .environment(state)
        }
        .windowResizability(.contentSize)
    }
}
