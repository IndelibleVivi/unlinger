import Foundation

public enum NotificationClaimDisposition: String, Codable, Equatable, Sendable {
    case claimed
    case scheduled
    case failedToSchedule = "failed_to_schedule"
    case suppressed
}

public struct NotificationClaim: Codable, Equatable, Sendable {
    public var claimedAtUnixMillis: UInt64
    public var disposition: NotificationClaimDisposition
}

public struct UnavailableNotificationEpisode: Codable, Equatable, Sendable {
    public var firstFailureAtUnixMillis: UInt64
    public var consecutiveFailures: Int
    public var handled: Bool
    public var claimKey: String?
}

public struct NotificationLedgerState: Codable, Equatable, Sendable {
    public var schemaVersion: Int
    public var baselineComplete: Bool
    public var seenEventTokens: [String: UInt64]
    public var claims: [String: NotificationClaim]
    public var activeEpisodes: [String: String]
    public var unavailableEpisode: UnavailableNotificationEpisode?
    public var promptAttempted: Bool
    public var authorizationSummary: NotificationAuthorization

    public init(
        schemaVersion: Int = 1,
        baselineComplete: Bool = false,
        seenEventTokens: [String: UInt64] = [:],
        claims: [String: NotificationClaim] = [:],
        activeEpisodes: [String: String] = [:],
        unavailableEpisode: UnavailableNotificationEpisode? = nil,
        promptAttempted: Bool = false,
        authorizationSummary: NotificationAuthorization = .notDetermined
    ) {
        self.schemaVersion = schemaVersion
        self.baselineComplete = baselineComplete
        self.seenEventTokens = seenEventTokens
        self.claims = claims
        self.activeEpisodes = activeEpisodes
        self.unavailableEpisode = unavailableEpisode
        self.promptAttempted = promptAttempted
        self.authorizationSummary = authorizationSummary
    }

    private enum CodingKeys: String, CodingKey {
        case schemaVersion = "schema_version"
        case baselineComplete = "baseline_complete"
        case seenEventTokens = "seen_event_tokens"
        case claims
        case activeEpisodes = "active_episodes"
        case unavailableEpisode = "unavailable_episode"
        case promptAttempted = "prompt_attempted"
        case authorizationSummary = "authorization_summary"
    }
}

public protocol NotificationLedgerStore: Sendable {
    func load() async throws -> NotificationLedgerState
    func save(_ state: NotificationLedgerState) async throws
}

public enum NotificationLedgerError: Error, Equatable, Sendable {
    case unsafePath
    case invalidRecord
    case ioFailure
}

public actor FileNotificationLedgerStore: NotificationLedgerStore {
    public let fileURL: URL

    public init(fileURL: URL? = nil, fileManager: FileManager = .default) {
        self.fileURL = fileURL ?? fileManager.homeDirectoryForCurrentUser
            .appendingPathComponent("Library/Application Support/Unlinger/app-state", isDirectory: true)
            .appendingPathComponent("notification-ledger-v1.json", isDirectory: false)
    }

    public func load() async throws -> NotificationLedgerState {
        let data: Data
        do {
            guard let stored = try OwnerPrivateFile.read(from: fileURL) else {
                return NotificationLedgerState()
            }
            data = stored
        } catch OwnerPrivateFileError.unsafePath {
            throw NotificationLedgerError.unsafePath
        } catch {
            throw NotificationLedgerError.ioFailure
        }
        do {
            let state = try JSONDecoder().decode(NotificationLedgerState.self, from: data)
            guard state.schemaVersion == 1 else { throw NotificationLedgerError.invalidRecord }
            return state
        } catch let error as NotificationLedgerError {
            throw error
        } catch {
            throw NotificationLedgerError.invalidRecord
        }
    }

    public func save(_ state: NotificationLedgerState) async throws {
        guard state.schemaVersion == 1 else { throw NotificationLedgerError.invalidRecord }
        do {
            let encoder = JSONEncoder()
            encoder.outputFormatting = [.sortedKeys]
            try OwnerPrivateFile.write(try encoder.encode(state), to: fileURL)
        } catch OwnerPrivateFileError.unsafePath {
            throw NotificationLedgerError.unsafePath
        } catch {
            throw NotificationLedgerError.ioFailure
        }
    }
}
