import Foundation

/// One user-initiated ordinary mutation before its receipt context is attached.
/// Diagnostics export is read-only and intentionally absent.
public enum Mutation: Codable, Equatable, Sendable {
    case pause(durationMillis: UInt64, label: String)
    case resume
    case retryFailedCleanup(incidentID: String)
    case protect(incidentID: String)
    case unprotect(incidentID: String)

    public var kind: MutationKind {
        switch self {
        case .pause: .pause
        case .resume: .resume
        case .retryFailedCleanup: .retryFailedCleanup
        case .protect: .protectIncident
        case .unprotect: .unprotectIncident
        }
    }

    public var semanticKey: String {
        switch self {
        case .pause(let duration, _): "pause|\(duration)"
        case .resume: "resume"
        case .retryFailedCleanup(let incidentID): "retry|\(incidentID)"
        case .protect(let incidentID): "protect|\(incidentID)"
        case .unprotect(let incidentID): "unprotect|\(incidentID)"
        }
    }

    func command(context: MutationContext) -> Command {
        switch self {
        case .pause(let duration, _): .pause(context: context, durationMillis: duration)
        case .resume: .resume(context: context)
        case .retryFailedCleanup(let incidentID):
            .retryFailedCleanup(context: context, incidentID: incidentID)
        case .protect(let incidentID):
            .protectIncident(context: context, incidentID: incidentID)
        case .unprotect(let incidentID):
            .unprotectIncident(context: context, incidentID: incidentID)
        }
    }

    private enum CodingKeys: String, CodingKey {
        case kind
        case durationMillis = "duration_millis"
        case label
        case incidentID = "incident_id"
    }

    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        switch try container.decode(String.self, forKey: .kind) {
        case "pause":
            self = .pause(
                durationMillis: try container.decode(UInt64.self, forKey: .durationMillis),
                label: try container.decode(String.self, forKey: .label)
            )
        case "resume": self = .resume
        case "retry_failed_cleanup":
            self = .retryFailedCleanup(incidentID: try container.decode(String.self, forKey: .incidentID))
        case "protect":
            self = .protect(incidentID: try container.decode(String.self, forKey: .incidentID))
        case "unprotect":
            self = .unprotect(incidentID: try container.decode(String.self, forKey: .incidentID))
        default:
            throw DecodingError.dataCorruptedError(
                forKey: .kind,
                in: container,
                debugDescription: "unknown persisted mutation kind"
            )
        }
    }

    public func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        switch self {
        case .pause(let duration, let label):
            try container.encode("pause", forKey: .kind)
            try container.encode(duration, forKey: .durationMillis)
            try container.encode(label, forKey: .label)
        case .resume:
            try container.encode("resume", forKey: .kind)
        case .retryFailedCleanup(let incidentID):
            try container.encode("retry_failed_cleanup", forKey: .kind)
            try container.encode(incidentID, forKey: .incidentID)
        case .protect(let incidentID):
            try container.encode("protect", forKey: .kind)
            try container.encode(incidentID, forKey: .incidentID)
        case .unprotect(let incidentID):
            try container.encode("unprotect", forKey: .kind)
            try container.encode(incidentID, forKey: .incidentID)
        }
    }
}

public struct PendingMutation: Codable, Equatable, Sendable, Identifiable {
    public var context: MutationContext
    public var mutation: Mutation
    public var semanticLockKey: String
    public var createdAtUnixMillis: UInt64
    public var visualDismissed: Bool

    public var id: String { context.mutationId }
    public var mutationID: String { context.mutationId }
    public var namespaceToken: String { context.namespaceToken }

    public init(
        context: MutationContext,
        mutation: Mutation,
        createdAtUnixMillis: UInt64,
        visualDismissed: Bool = false
    ) {
        self.context = context
        self.mutation = mutation
        self.semanticLockKey = mutation.semanticKey
        self.createdAtUnixMillis = createdAtUnixMillis
        self.visualDismissed = visualDismissed
    }

    private enum CodingKeys: String, CodingKey {
        case context, mutation
        case semanticLockKey = "semantic_lock_key"
        case createdAtUnixMillis = "created_at_unix_millis"
        case visualDismissed = "visual_dismissed"
    }
}

public enum MutationState: Equatable, Sendable {
    case idle
    case inFlight(PendingMutation)
    case confirmed(PendingMutation, MutationReceipt)
    case definitelyNotApplied(PendingMutation)
    case unresolved(PendingMutation)
    case authorityLost(PendingMutation)
    case rejected(PendingMutation, reasonId: String)
    case failedBeforeSend(PendingMutation, reasonId: String)
}

public protocol MutationJournalStore: Sendable {
    func load() async throws -> PendingMutation?
    func persist(_ pending: PendingMutation) async throws
    func remove(expectedMutationID: String) async throws
}

public enum MutationJournalError: Error, Equatable, Sendable {
    case unsafePath
    case occupied
    case invalidRecord
    case ioFailure
}

/// One owner-private, crash-durable pending record. The App persists this file
/// before connect/send, then removes it only after an authoritative resolution.
public actor FileMutationJournalStore: MutationJournalStore {
    public let fileURL: URL

    public init(fileURL: URL? = nil, fileManager: FileManager = .default) {
        self.fileURL = fileURL ?? fileManager.homeDirectoryForCurrentUser
            .appendingPathComponent("Library/Application Support/Unlinger/app-state", isDirectory: true)
            .appendingPathComponent("pending-mutation-v1.json", isDirectory: false)
    }

    public func load() async throws -> PendingMutation? {
        let data: Data
        do {
            guard let stored = try OwnerPrivateFile.read(from: fileURL) else { return nil }
            data = stored
        } catch OwnerPrivateFileError.unsafePath {
            throw MutationJournalError.unsafePath
        } catch {
            throw MutationJournalError.ioFailure
        }
        let pending: PendingMutation
        do {
            pending = try JSONDecoder().decode(PendingMutation.self, from: data)
        } catch {
            throw MutationJournalError.invalidRecord
        }
        guard pending.semanticLockKey == pending.mutation.semanticKey,
              Self.validNamespace(pending.context.namespaceToken),
              Self.validMutationID(pending.context.mutationId)
        else {
            throw MutationJournalError.invalidRecord
        }
        return pending
    }

    public func persist(_ pending: PendingMutation) async throws {
        guard pending.semanticLockKey == pending.mutation.semanticKey,
              Self.validNamespace(pending.context.namespaceToken),
              Self.validMutationID(pending.context.mutationId)
        else {
            throw MutationJournalError.invalidRecord
        }
        if let existing = try await load(), existing.mutationID != pending.mutationID {
            throw MutationJournalError.occupied
        }
        let data: Data
        do {
            let encoder = JSONEncoder()
            encoder.outputFormatting = [.sortedKeys]
            data = try encoder.encode(pending)
            try OwnerPrivateFile.write(data, to: fileURL)
        } catch let error as MutationJournalError {
            throw error
        } catch OwnerPrivateFileError.unsafePath {
            throw MutationJournalError.unsafePath
        } catch {
            throw MutationJournalError.ioFailure
        }
    }

    public func remove(expectedMutationID: String) async throws {
        guard let existing = try await load() else { return }
        guard existing.mutationID == expectedMutationID else {
            throw MutationJournalError.occupied
        }
        do {
            try OwnerPrivateFile.remove(fileURL)
        } catch OwnerPrivateFileError.unsafePath {
            throw MutationJournalError.unsafePath
        } catch {
            throw MutationJournalError.ioFailure
        }
    }

    private static func validNamespace(_ value: String) -> Bool {
        value.utf8.count == 32 && value.utf8.allSatisfy {
            (UInt8(ascii: "0") ... UInt8(ascii: "9")).contains($0)
                || (UInt8(ascii: "a") ... UInt8(ascii: "f")).contains($0)
        }
    }

    private static func validMutationID(_ value: String) -> Bool {
        guard value == value.lowercased(), let uuid = UUID(uuidString: value) else { return false }
        return uuid.uuidString.lowercased() == value
    }
}
