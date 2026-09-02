import Foundation

protocol SocketExchanging: Sendable {
    func exchange(
        requestLine: Data,
        socketPath: String,
        timeout: TimeInterval
    ) async throws(ClientError) -> Data
}

/// Schema-v4 Unix-domain socket client. Every command gets one connection and
/// one write attempt. A mutation is never automatically resent.
public actor SocketClient: UnlingerClient {
    public static let maxRequestBytes = 64 * 1024
    public static let maxResponseBytes = 4 * 1024 * 1024

    public let socketPath: String
    private let timeout: TimeInterval
    private let exchanger: any SocketExchanging
    private var nextRequestID: UInt64 = 1

    public init(socketPath: String, timeout: TimeInterval = 15) {
        self.socketPath = socketPath
        self.timeout = timeout
        self.exchanger = UnixSocketExchanger()
    }

    init(
        socketPath: String,
        timeout: TimeInterval = 15,
        exchanger: any SocketExchanging
    ) {
        self.socketPath = socketPath
        self.timeout = timeout
        self.exchanger = exchanger
    }

    /// Default socket: effective user's home from the account database, not
    /// `$HOME`. The override is only for isolated source-daemon development.
    public static func `default`() -> SocketClient {
        if let override = ProcessInfo.processInfo.environment["UNLINGER_SOCKET_PATH"],
           !override.isEmpty
        {
            return SocketClient(socketPath: override)
        }
        guard let pw = getpwuid(geteuid()), let dir = pw.pointee.pw_dir else {
            // Empty is intentionally invalid and fails before connect. Never
            // fall back to HOME/NSHomeDirectory for the authority path.
            return SocketClient(socketPath: "")
        }
        let home = String(cString: dir)
        return SocketClient(
            socketPath: home + "/Library/Application Support/Unlinger/run/unlingerd.sock"
        )
    }

    public func status() async throws(ClientError) -> PublicStatus {
        try await request(.status)
    }

    public func browserOverview() async throws(ClientError) -> BrowserOverviewSnapshot {
        try await request(.browserOverview)
    }

    public func history(limit: Int) async throws(ClientError) -> [HistoryEvent] {
        try await request(.history(limit: limit))
    }

    public func explain(incidentID: String) async throws(ClientError) -> IncidentDetail {
        try await request(.explain(incidentID: incidentID))
    }

    public func incidents() async throws(ClientError) -> ObservationRoster {
        try await request(.incidents)
    }

    public func mutationStatus(context: MutationContext) async throws(ClientError) -> MutationStatus {
        try await request(.mutationStatus(context: context))
    }

    public func pause(
        context: MutationContext,
        durationMillis: UInt64
    ) async throws(ClientError) -> MutationReceipt {
        try await request(.pause(context: context, durationMillis: durationMillis))
    }

    public func resume(context: MutationContext) async throws(ClientError) -> MutationReceipt {
        try await request(.resume(context: context))
    }

    public func retryFailedCleanup(
        context: MutationContext,
        incidentID: String
    ) async throws(ClientError) -> MutationReceipt {
        try await request(.retryFailedCleanup(context: context, incidentID: incidentID))
    }

    public func protectIncident(
        context: MutationContext,
        incidentID: String
    ) async throws(ClientError) -> MutationReceipt {
        try await request(.protectIncident(context: context, incidentID: incidentID))
    }

    public func unprotectIncident(
        context: MutationContext,
        incidentID: String
    ) async throws(ClientError) -> MutationReceipt {
        try await request(.unprotectIncident(context: context, incidentID: incidentID))
    }

    public func exportDiagnostics(incidentID: String) async throws(ClientError) -> DiagnosticsExport {
        let command = Command.exportDiagnostics(incidentID: incidentID)
        let requestID = takeRequestID()
        let line = try encodeRequest(requestID: requestID, command: command)
        try validateSocketPath()
        let responseLine = try await exchanger.exchange(
            requestLine: line,
            socketPath: socketPath,
            timeout: timeout
        )
        let decoded = try ResponseDecoder.decodeWithRaw(
            DiagnosticsBundle.self,
            expectedPayloadType: "diagnostics",
            requestID: requestID,
            expectedSchemaVersion: 4,
            line: responseLine
        )
        return DiagnosticsExport(bundle: decoded.value, rawJSON: decoded.rawData)
    }

    private func takeRequestID() -> UInt64 {
        let value = nextRequestID
        nextRequestID &+= 1
        return value
    }

    private func encodeRequest(
        requestID: UInt64,
        command: Command
    ) throws(ClientError) -> Data {
        do {
            var encoded = try JSONEncoder().encode(
                RequestEnvelope(requestID: requestID, command: command)
            )
            encoded.append(0x0A)
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

    private func request<T: Decodable & Sendable>(
        _ command: Command,
        as type: T.Type = T.self
    ) async throws(ClientError) -> T {
        let requestID = takeRequestID()
        // Encoding and path validation happen before connect and are therefore
        // never promoted to mutation delivery uncertainty.
        let line: Data
        do {
            line = try encodeRequest(requestID: requestID, command: command)
            try validateSocketPath()
        } catch let error {
            guard command.isMutation else { throw error }
            if case .protocolError(let reason) = error {
                throw .failedBeforeSend(reason)
            }
            throw error
        }
        do {
            let responseLine = try await exchanger.exchange(
                requestLine: line,
                socketPath: socketPath,
                timeout: timeout
            )
            return try Self.decode(T.self, for: command, requestID: requestID, line: responseLine)
        } catch let error {
            guard command.isMutation else { throw error }
            switch error {
            case .unavailable, .failedBeforeSend, .serverError, .incompatibleDaemon:
                throw error
            case .deliveryUncertain, .protocolError:
                // Once any request byte may have crossed the socket, malformed,
                // oversized, mismatched, or semantically invalid responses are
                // uncertain for mutations. Reconcile; never resend.
                throw .deliveryUncertain
            }
        }
    }

    private func validateSocketPath() throws(ClientError) {
        guard socketPath.utf8.first == UInt8(ascii: "/"),
              !socketPath.utf8.contains(0)
        else {
            throw .protocolError("socket path must be an absolute path without NUL bytes")
        }
        let maxPathLength = MemoryLayout<sockaddr_un>.size
            - MemoryLayout<sa_family_t>.size
            - MemoryLayout<UInt8>.size
            - 1
        guard socketPath.utf8.count <= maxPathLength else {
            throw .protocolError("socket path too long")
        }
    }
}

private struct UnixSocketExchanger: SocketExchanging {
    func exchange(
        requestLine: Data,
        socketPath: String,
        timeout: TimeInterval
    ) async throws(ClientError) -> Data {
        try await SocketOperation(
            requestLine: requestLine,
            socketPath: socketPath,
            timeout: timeout
        ).run()
    }
}

/// Bridges blocking POSIX socket I/O onto a bounded Foundation worker pool.
/// Task cancellation closes the active read/write path via shutdown, so a
/// stopped polling session cannot leave a blocking call parked on an actor
/// executor.
private final class SocketOperation: @unchecked Sendable {
    /// One App refresh performs three reads in parallel. A fourth slot keeps an
    /// explicit detail/action read responsive without creating three fresh OS
    /// threads every five seconds for the lifetime of the menu client.
    private static let workerQueue: OperationQueue = {
        let queue = OperationQueue()
        queue.name = "app.unlinger.ipc"
        queue.maxConcurrentOperationCount = 4
        queue.qualityOfService = .userInitiated
        return queue
    }()

    private let requestLine: Data
    private let socketPath: String
    private let timeout: TimeInterval
    private let lock = NSLock()
    private var activeFD: Int32 = -1
    private var cancelled = false

    init(requestLine: Data, socketPath: String, timeout: TimeInterval) {
        self.requestLine = requestLine
        self.socketPath = socketPath
        self.timeout = timeout
    }

    func run() async throws(ClientError) -> Data {
        let result: Result<Data, ClientError> = await withTaskCancellationHandler {
            await withCheckedContinuation { continuation in
                Self.workerQueue.addOperation { [self] in
                    // Drain every request's temporary Foundation objects so
                    // five-second polling cannot accumulate autoreleased
                    // Data/JSON/socket state on a reused worker thread.
                    let result = autoreleasepool(invoking: execute)
                    continuation.resume(returning: result)
                }
            }
        } onCancel: { [self] in
            cancel()
        }
        return try result.get()
    }

    private func cancel() {
        lock.lock()
        cancelled = true
        let fd = activeFD
        lock.unlock()
        if fd >= 0 {
            _ = shutdown(fd, SHUT_RDWR)
        }
    }

    private func execute() -> Result<Data, ClientError> {
        do {
            return .success(try executeThrowing())
        } catch let error {
            return .failure(error)
        }
    }

    private func executeThrowing() throws(ClientError) -> Data {
        lock.lock()
        let wasCancelled = cancelled
        lock.unlock()
        guard !wasCancelled else {
            throw .failedBeforeSend("transport.cancelled_before_connect")
        }

        let fd = socket(AF_UNIX, SOCK_STREAM, 0)
        guard fd >= 0 else { throw .unavailable }
        lock.lock()
        activeFD = fd
        let cancelledAfterOpen = cancelled
        lock.unlock()
        defer {
            lock.lock()
            activeFD = -1
            lock.unlock()
            close(fd)
        }
        guard !cancelledAfterOpen else {
            throw .failedBeforeSend("transport.cancelled_before_connect")
        }

        var one: Int32 = 1
        guard setsockopt(
            fd,
            SOL_SOCKET,
            SO_NOSIGPIPE,
            &one,
            socklen_t(MemoryLayout<Int32>.size)
        ) == 0 else {
            throw .failedBeforeSend("transport.socket_configuration_failed")
        }
        var timeoutValue = timeval(
            tv_sec: Int(timeout),
            tv_usec: suseconds_t((timeout - TimeInterval(Int(timeout))) * 1_000_000)
        )
        guard setsockopt(
            fd,
            SOL_SOCKET,
            SO_RCVTIMEO,
            &timeoutValue,
            socklen_t(MemoryLayout<timeval>.size)
        ) == 0,
        setsockopt(
            fd,
            SOL_SOCKET,
            SO_SNDTIMEO,
            &timeoutValue,
            socklen_t(MemoryLayout<timeval>.size)
        ) == 0 else {
            throw .failedBeforeSend("transport.socket_configuration_failed")
        }

        var address = sockaddr_un()
        address.sun_family = sa_family_t(AF_UNIX)
        address.sun_len = UInt8(MemoryLayout<sockaddr_un>.size)
        let pathBytes = Array(socketPath.utf8)
        let maxPathLength = MemoryLayout.size(ofValue: address.sun_path) - 1
        guard pathBytes.count <= maxPathLength else {
            throw .failedBeforeSend("transport.socket_path_too_long")
        }
        withUnsafeMutableBytes(of: &address.sun_path) { buffer in
            for (index, byte) in pathBytes.enumerated() {
                buffer[index] = byte
            }
        }
        let connected = withUnsafePointer(to: &address) { pointer in
            pointer.withMemoryRebound(to: sockaddr.self, capacity: 1) {
                connect(fd, $0, socklen_t(MemoryLayout<sockaddr_un>.size))
            }
        }
        guard connected == 0 else { throw .unavailable }

        try sendAll(fd: fd)
        return try receiveLine(fd: fd)
    }

    private func sendAll(fd: Int32) throws(ClientError) {
        var failure: ClientError?
        requestLine.withUnsafeBytes { (buffer: UnsafeRawBufferPointer) in
            var sent = 0
            while sent < buffer.count {
                let written = send(fd, buffer.baseAddress! + sent, buffer.count - sent, 0)
                if written < 0, errno == EINTR { continue }
                guard written > 0 else {
                    failure = sent == 0
                        ? .failedBeforeSend("transport.zero_byte_write")
                        : .deliveryUncertain
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
            if received < 0, errno == EINTR { continue }
            guard received > 0 else { throw .deliveryUncertain }
            data.append(contentsOf: chunk[0 ..< received])
            guard data.count <= SocketClient.maxResponseBytes else {
                throw .deliveryUncertain
            }
            if let newline = data.firstIndex(of: 0x0A) {
                return Data(data[data.startIndex ..< newline])
            }
        }
    }
}
