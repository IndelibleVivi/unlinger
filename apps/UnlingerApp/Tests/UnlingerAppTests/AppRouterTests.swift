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

    @Test("window presentation preserves the current route")
    func presentsCurrentRoute() {
        let router = AppRouter()
        router.path = [.history, .incident("redacted-incident")]
        var openCount = 0
        router.registerWindowOpener {
            openCount += 1
        }

        router.presentCurrentRoute()

        #expect(router.presentationRequested)
        #expect(openCount == 1)
        #expect(router.path == [.history, .incident("redacted-incident")])
    }
}
