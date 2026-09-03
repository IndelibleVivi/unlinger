import Testing
@testable import UnlingerKit

@Suite("App router")
@MainActor
struct AppRouterTests {
    @Test("back removes exactly one route and is safe at the root")
    func explicitBackNavigation() {
        let router = AppRouter()
        router.path = [.history, .incident("redacted-incident")]

        router.goBack()
        #expect(router.path == [.history])

        router.goBack()
        #expect(router.path.isEmpty)

        router.goBack()
        #expect(router.path.isEmpty)
    }

    @Test("one router presents without mutating its current route")
    func presentsCurrentRoute() {
        let router = AppRouter()
        router.path = [.history, .incident("redacted-incident")]
        var openCount = 0
        router.registerWindowOpener {
            openCount += 1
        }

        router.presentCurrentRoute()

        #expect(openCount == 1)
        #expect(router.path == [.history, .incident("redacted-incident")])
    }

    @Test("menu route handoff isolates hosts and consumes the hidden route")
    func menuRouteHandoff() {
        let navigation = AppNavigationCoordinator()
        #expect(navigation.menu !== navigation.window)
        navigation.menu.path = [.history, .incident("redacted-incident")]

        navigation.handOffMenuRouteToWindow()

        #expect(navigation.menu.path.isEmpty)
        #expect(navigation.window.path == [.history, .incident("redacted-incident")])

        navigation.menu.path = [.settings]
        #expect(navigation.window.path == [.history, .incident("redacted-incident")])
    }

    @Test("notification routing targets only the ordinary window router")
    func notificationRouteIsolation() {
        let navigation = AppNavigationCoordinator()
        navigation.menu.path = [.history]
        var windowOpenCount = 0
        navigation.window.registerWindowOpener {
            windowOpenCount += 1
        }

        navigation.window.open(.incident("redacted-notification-incident"))

        #expect(windowOpenCount == 1)
        #expect(navigation.window.path == [.incident("redacted-notification-incident")])
        #expect(navigation.menu.path == [.history])
    }
}
