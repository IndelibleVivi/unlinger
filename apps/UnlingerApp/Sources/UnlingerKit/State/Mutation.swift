import Foundation

/// A user-initiated ordinary mutation. Carries its display label so the
/// uncertain/confirmed states can talk about the exact action taken.
public enum Mutation: Equatable, Sendable {
    case pause(durationMillis: UInt64, label: String)
    case resume
    case retryFailedCleanup(incidentID: String)
    case protect(incidentID: String)
    case unprotect(incidentID: String)
    case exportDiagnostics(incidentID: String)

    var command: Command {
        switch self {
        case .pause(let durationMillis, _): .pause(durationMillis: durationMillis)
        case .resume: .resume
        case .retryFailedCleanup(let id): .retryFailedCleanup(incidentID: id)
        case .protect(let id): .protectIncident(incidentID: id)
        case .unprotect(let id): .unprotectIncident(incidentID: id)
        case .exportDiagnostics(let id): .exportDiagnostics(incidentID: id)
        }
    }

    /// Read-only command whose result shows whether the mutation committed.
    var readbackCommand: Command { command.readback }
}

/// Mutation lifecycle per the delivery-uncertainty contract:
///
/// - `uncertain` means the mutation may already have committed. The app has
///   read back current state and shows it; it never resends on its own. Only
///   an explicit fresh user action starts a new mutation.
public enum MutationState: Equatable, Sendable {
    case idle
    case inFlight(Mutation)
    case confirmed(Mutation)
    case uncertain(Mutation)
    case failed(reasonId: String)
}
