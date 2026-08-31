import Foundation

/// Transport/client errors, modeled per the IPC contract:
///
/// - `unavailable`: the connection could not be established (missing socket,
///   refused, peer mismatch). The request was never sent — no uncertainty.
/// - `deliveryUncertain`: the request line was written, then the connection
///   timed out, hit EOF, or dropped before a trusted response. The daemon may
///   already have committed the mutation. Never auto-resend; read back first.
/// - `serverError`: a structured `error.code` response from the daemon.
/// - `protocolError`: framing/envelope violation or a response that fails
///   schema/request-id/mutual-exclusion validation.
public enum ClientError: Error, Equatable, Sendable {
    case unavailable
    case deliveryUncertain
    case serverError(code: String, message: String)
    case protocolError(String)
}
