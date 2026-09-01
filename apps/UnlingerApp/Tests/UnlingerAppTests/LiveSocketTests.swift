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
        #expect(status.effectiveMode == .reportOnly)
    }

    @Test(.enabled(if: liveSocketAvailable, "requires UNLINGER_LIVE_SOCKET"))
    func pauseResume() async throws {
        let path = try #require(socketPath, "UNLINGER_LIVE_SOCKET not set")
        let client = SocketClient(socketPath: path)
        let status = try await client.status()
        let pauseContext = MutationContext(
            namespaceToken: status.mutationAuthority.namespaceToken,
            mutationId: "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeee1"
        )
        let pauseReceipt = try await client.pause(
            context: pauseContext,
            durationMillis: 3_600_000
        )
        #expect(pauseReceipt.namespaceToken == pauseContext.namespaceToken)
        let pauseLookup = try await client.mutationStatus(context: pauseContext)
        guard case .committed(let replayedPause) = pauseLookup else {
            Issue.record("pause receipt was not durable")
            return
        }
        #expect(replayedPause == pauseReceipt)
        let resumeContext = MutationContext(
            namespaceToken: status.mutationAuthority.namespaceToken,
            mutationId: "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeee2"
        )
        let resumeReceipt = try await client.resume(context: resumeContext)
        guard case .committed(let replayedResume) = try await client.mutationStatus(
            context: resumeContext
        ) else {
            Issue.record("resume receipt was not durable")
            return
        }
        #expect(replayedResume == resumeReceipt)
    }

    @Test(.enabled(if: liveSocketAvailable, "requires UNLINGER_LIVE_SOCKET"))
    func incidentsRosterRoundTrip() async throws {
        let path = try #require(socketPath, "UNLINGER_LIVE_SOCKET not set")
        let client = SocketClient(socketPath: path)
        // An isolated daemon with no observed incidents still proves the wire
        // path: the roster decodes as a (usually empty) typed list.
        _ = try await client.incidents()
    }

    @Test(.enabled(if: liveSocketAvailable, "requires UNLINGER_LIVE_SOCKET"))
    func historyAndMissingIncidentRoundTrip() async throws {
        let path = try #require(socketPath, "UNLINGER_LIVE_SOCKET not set")
        let client = SocketClient(socketPath: path)
        _ = try await client.history(limit: 50)
        await #expect(throws: ClientError.self) {
            _ = try await client.explain(incidentID: "smoke-missing-incident")
        }
        await #expect(throws: ClientError.self) {
            _ = try await client.exportDiagnostics(incidentID: "smoke-missing-incident")
        }
    }

    @Test(.enabled(if: liveSocketAvailable, "requires UNLINGER_LIVE_SOCKET"))
    func missingProtectionIsAStoredTypedOutcome() async throws {
        let path = try #require(socketPath, "UNLINGER_LIVE_SOCKET not set")
        let client = SocketClient(socketPath: path)
        let status = try await client.status()
        let context = MutationContext(
            namespaceToken: status.mutationAuthority.namespaceToken,
            mutationId: "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeee3"
        )
        let receipt = try await client.protectIncident(
            context: context,
            incidentID: "smoke-missing-incident"
        )
        guard case .rejected = receipt.outcome else {
            Issue.record("missing protection should be a stored typed rejection")
            return
        }
        guard case .committed(let replayed) = try await client.mutationStatus(context: context) else {
            Issue.record("typed rejection receipt was not durable")
            return
        }
        #expect(replayed == receipt)
    }

    @Test("connect failure is unavailable, not uncertain")
    func missingSocket() async throws {
        let client = SocketClient(socketPath: "/tmp/unlinger-definitely-missing.sock")
        await #expect(throws: ClientError.unavailable) {
            try await client.status()
        }
    }
}
