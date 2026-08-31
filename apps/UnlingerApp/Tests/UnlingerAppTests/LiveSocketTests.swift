import Foundation
import Testing
@testable import UnlingerKit

/// Opt-in live smoke test against an isolated source report-only daemon.
/// Runs only when UNLINGER_LIVE_SOCKET points at such a daemon's socket;
/// skipped silently otherwise. Never touches the installed generation.
@Suite("Live socket smoke (opt-in)")
struct LiveSocketTests {
    private var socketPath: String? {
        ProcessInfo.processInfo.environment["UNLINGER_LIVE_SOCKET"]
    }

    private static var liveSocketAvailable: Bool {
        ProcessInfo.processInfo.environment["UNLINGER_LIVE_SOCKET"]?.isEmpty == false
    }

    @Test(.enabled(if: liveSocketAvailable, "requires UNLINGER_LIVE_SOCKET"))
    func statusRoundTrip() async throws {
        let path = try #require(socketPath, "UNLINGER_LIVE_SOCKET not set")
        let client = SocketClient(socketPath: path)
        let status = try await client.status()
        #expect(status.daemonVersion.isEmpty == false)
    }

    @Test(.enabled(if: liveSocketAvailable, "requires UNLINGER_LIVE_SOCKET"))
    func pauseResume() async throws {
        let path = try #require(socketPath, "UNLINGER_LIVE_SOCKET not set")
        let client = SocketClient(socketPath: path)
        let until = try await client.pause(durationMillis: 3_600_000)
        #expect(until > 0)
        try await client.resume()
    }

    @Test(.enabled(if: liveSocketAvailable, "requires UNLINGER_LIVE_SOCKET"))
    func incidentsRosterRoundTrip() async throws {
        let path = try #require(socketPath, "UNLINGER_LIVE_SOCKET not set")
        let client = SocketClient(socketPath: path)
        // An isolated daemon with no observed incidents still proves the wire
        // path: the roster decodes as a (usually empty) typed list.
        _ = try await client.incidents()
    }

    @Test("connect failure is unavailable, not uncertain")
    func missingSocket() async throws {
        let client = SocketClient(socketPath: "/tmp/unlinger-definitely-missing.sock")
        await #expect(throws: ClientError.unavailable) {
            try await client.status()
        }
    }
}
