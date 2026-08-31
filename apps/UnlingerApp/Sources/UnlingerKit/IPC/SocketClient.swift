import Foundation

/// Schema-v2 Unix-domain socket client.
///
/// Contract anchors (docs/IPC.md):
/// - one LF-delimited JSON request and one response per connection;
/// - request line ≤ 64 KiB, response line ≤ 4 MiB;
/// - 15-second read/write timeout, single attempt, no automatic resend;
/// - connect failure means the request was never sent (`unavailable`);
///   any failure after a successful connect means delivery is uncertain
///   (`deliveryUncertain`) and the caller must read back, never resend.
public actor SocketClient: UnlingerClient {
    public static let maxRequestBytes = 64 * 1024
    public static let maxResponseBytes = 4 * 1024 * 1024

    public let socketPath: String
    private let timeout: TimeInterval
    private var nextRequestID: UInt64 = 1

    public init(socketPath: String, timeout: TimeInterval = 15) {
        self.socketPath = socketPath
        self.timeout = timeout
    }

    /// Default socket: effective user's home from the account database
    /// (not $HOME), matching the daemon's own path resolution.
    /// `UNLINGER_SOCKET_PATH` overrides it for isolated source-daemon dev.
    public static func `default`() -> SocketClient {
        if let override = ProcessInfo.processInfo.environment["UNLINGER_SOCKET_PATH"],
           !override.isEmpty
        {
            return SocketClient(socketPath: override)
        }
        let home: String
        if let pw = getpwuid(geteuid()), let dir = pw.pointee.pw_dir {
            home = String(cString: dir)
        } else {
            home = NSHomeDirectory()
        }
        return SocketClient(
            socketPath: home + "/Library/Application Support/Unlinger/run/unlingerd.sock"
        )
    }

    public func status() async throws(ClientError) -> PublicStatus {
        try await request(.status)
    }

    public func history(limit: Int) async throws(ClientError) -> [HistoryEvent] {
        try await request(.history(limit: limit))
    }

    public func explain(incidentID: String) async throws(ClientError) -> IncidentDetail {
        try await request(.explain(incidentID: incidentID))
    }

    public func incidents() async throws(ClientError) -> [CurrentIncident] {
        try await request(.incidents)
    }

    public func pause(durationMillis: UInt64) async throws(ClientError) -> UInt64 {
        let result: PauseResult = try await request(.pause(durationMillis: durationMillis))
        return result.untilUnixMillis
    }

    public func resume() async throws(ClientError) {
        let _: ResponseDecoder.Empty = try await request(.resume)
    }

    public func retryFailedCleanup(incidentID: String) async throws(ClientError) {
        let _: IncidentIDResult = try await request(.retryFailedCleanup(incidentID: incidentID))
    }

    public func protectIncident(incidentID: String) async throws(ClientError) {
        let _: ProtectionResult = try await request(.protectIncident(incidentID: incidentID))
    }

    public func unprotectIncident(incidentID: String) async throws(ClientError) {
        let _: IncidentIDResult = try await request(.unprotectIncident(incidentID: incidentID))
    }

    public func exportDiagnostics(incidentID: String) async throws(ClientError) -> DiagnosticsExport {
        let command = Command.exportDiagnostics(incidentID: incidentID)
        let requestID = nextRequestID
        nextRequestID &+= 1
        let line = try encodeRequest(requestID: requestID, command: command)
        let responseLine = try roundTrip(line)
        let decoded = try ResponseDecoder.decodeWithRaw(
            DiagnosticsBundle.self,
            expectedPayloadType: "diagnostics",
            requestID: requestID,
            line: responseLine
        )
        return DiagnosticsExport(bundle: decoded.value, rawJSON: decoded.rawData)
    }

    private func encodeRequest(requestID: UInt64, command: Command) throws(ClientError) -> Data {
        let envelope = RequestEnvelope(requestID: requestID, command: command)
        do {
            var encoded = try JSONEncoder().encode(envelope)
            encoded.append(0x0A) // LF
            guard encoded.count <= Self.maxRequestBytes else {
                throw ClientError.protocolError("request exceeds 64 KiB")
            }
            return encoded
        } catch let error as ClientError {
            throw error
        } catch {
            throw .protocolError("failed to encode request")
        }
    }

    private func request<T: Decodable & Sendable>(_ command: Command, as type: T.Type = T.self) async throws(ClientError) -> T {
        let requestID = nextRequestID
        nextRequestID &+= 1
        let line = try encodeRequest(requestID: requestID, command: command)
        let responseLine = try roundTrip(line)
        return try Self.decode(T.self, for: command, requestID: requestID, line: responseLine)
    }

    // MARK: - POSIX transport

    private func roundTrip(_ requestLine: Data) throws(ClientError) -> Data {
        let fd = socket(AF_UNIX, SOCK_STREAM, 0)
        guard fd >= 0 else { throw .unavailable }
        defer { close(fd) }

        var one: Int32 = 1
        setsockopt(fd, SOL_SOCKET, SO_NOSIGPIPE, &one, socklen_t(MemoryLayout<Int32>.size))

        var timeoutValue = timeval(
            tv_sec: Int(timeout),
            tv_usec: suseconds_t((timeout - TimeInterval(Int(timeout))) * 1_000_000)
        )
        setsockopt(fd, SOL_SOCKET, SO_RCVTIMEO, &timeoutValue, socklen_t(MemoryLayout<timeval>.size))
        setsockopt(fd, SOL_SOCKET, SO_SNDTIMEO, &timeoutValue, socklen_t(MemoryLayout<timeval>.size))

        var address = sockaddr_un()
        address.sun_family = sa_family_t(AF_UNIX)
        address.sun_len = UInt8(MemoryLayout<sockaddr_un>.size)
        let pathBytes = Array(socketPath.utf8)
        let maxPathLength = MemoryLayout.size(ofValue: address.sun_path) - 1
        guard pathBytes.count <= maxPathLength else {
            throw .protocolError("socket path too long")
        }
        withUnsafeMutableBytes(of: &address.sun_path) { buffer in
            for (index, byte) in pathBytes.enumerated() {
                buffer[index] = byte
            }
        }

        let connected = withUnsafePointer(to: &address) { pointer in
            pointer.withMemoryRebound(to: sockaddr.self, capacity: 1) { sockaddrPointer in
                connect(fd, sockaddrPointer, socklen_t(MemoryLayout<sockaddr_un>.size))
            }
        }
        // Connect failure: nothing was sent, so there is no delivery uncertainty.
        guard connected == 0 else { throw .unavailable }

        try sendAll(fd: fd, data: requestLine)
        return try receiveLine(fd: fd)
    }

    private func sendAll(fd: Int32, data: Data) throws(ClientError) {
        // `withUnsafeBytes` rethrows as untyped, so capture the outcome.
        var failure: ClientError?
        data.withUnsafeBytes { (buffer: UnsafeRawBufferPointer) in
            var sent = 0
            while sent < buffer.count {
                let written = send(fd, buffer.baseAddress! + sent, buffer.count - sent, 0)
                guard written > 0 else {
                    // The peer may already hold a partial or complete line;
                    // delivery is uncertain.
                    failure = .deliveryUncertain
                    return
                }
                sent += written
            }
        }
        if let failure { throw failure }
    }

    private func receiveLine(fd: Int32) throws(ClientError) -> Data {
        var data = Data()
        var chunk = [UInt8](repeating: 0, count: 16 * 1024)
        while true {
            let received = recv(fd, &chunk, chunk.count, 0)
            if received < 0 {
                // Timeout, reset, or other I/O failure after a fully written
                // request: delivery is uncertain.
                throw ClientError.deliveryUncertain
            }
            if received == 0 {
                // EOF before LF: the contract says not to rely on EOF framing.
                throw ClientError.deliveryUncertain
            }
            data.append(contentsOf: chunk[0..<received])
            guard data.count <= Self.maxResponseBytes else {
                throw ClientError.protocolError("response exceeds 4 MiB")
            }
            if let newlineIndex = data.firstIndex(of: 0x0A) {
                return data[data.startIndex..<newlineIndex]
            }
        }
    }
}
