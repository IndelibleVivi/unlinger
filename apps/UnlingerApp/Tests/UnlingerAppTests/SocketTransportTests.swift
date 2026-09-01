import Foundation
import Testing
@testable import UnlingerKit

private struct StubExchanger: SocketExchanging {
    enum Output: Sendable {
        case response(Data)
        case failure(ClientError)
    }

    let output: Output

    func exchange(
        requestLine _: Data,
        socketPath _: String,
        timeout _: TimeInterval
    ) async throws(ClientError) -> Data {
        switch output {
        case .response(let data): return data
        case .failure(let error): throw error
        }
    }
}

@Suite("Mutation transport phase semantics")
struct SocketTransportTests {
    private let context = MutationContext(
        namespaceToken: "0123456789abcdef0123456789abcdef",
        mutationId: "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee"
    )

    @Test("bad JSON after send is delivery uncertain")
    func postSendBadJSON() async throws {
        let client = SocketClient(
            socketPath: "/tmp/unlinger-test.sock",
            exchanger: StubExchanger(output: .response(Data("not-json".utf8)))
        )
        await #expect(throws: ClientError.deliveryUncertain) {
            _ = try await client.pause(context: context, durationMillis: 1)
        }
    }

    @Test("wrong payload after send is delivery uncertain")
    func postSendWrongPayload() async throws {
        let response = Data(
            #"{"schema_version":3,"request_id":1,"ok":true,"payload":{"type":"status","data":{}}}"#.utf8
        )
        let client = SocketClient(
            socketPath: "/tmp/unlinger-test.sock",
            exchanger: StubExchanger(output: .response(response))
        )
        await #expect(throws: ClientError.deliveryUncertain) {
            _ = try await client.pause(context: context, durationMillis: 1)
        }
    }

    @Test("wrong request ID after send is delivery uncertain")
    func postSendWrongRequestID() async throws {
        let response = Data(
            #"{"schema_version":3,"request_id":99,"ok":false,"error":{"code":"invalid_argument","message":"rejected"}}"#.utf8
        )
        let client = SocketClient(
            socketPath: "/tmp/unlinger-test.sock",
            exchanger: StubExchanger(output: .response(response))
        )
        await #expect(throws: ClientError.deliveryUncertain) {
            _ = try await client.pause(context: context, durationMillis: 1)
        }
    }

    @Test("zero-byte failure remains definitely before send")
    func zeroByteFailure() async throws {
        let expected = ClientError.failedBeforeSend("transport.zero_byte_write")
        let client = SocketClient(
            socketPath: "/tmp/unlinger-test.sock",
            exchanger: StubExchanger(output: .failure(expected))
        )
        await #expect(throws: expected) {
            _ = try await client.pause(context: context, durationMillis: 1)
        }
    }

    @Test("request preparation failure remains definitely before connect")
    func invalidSocketPath() async throws {
        let client = SocketClient(
            socketPath: String(repeating: "x", count: 256),
            exchanger: StubExchanger(output: .failure(.deliveryUncertain))
        )
        await #expect(
            throws: ClientError.failedBeforeSend(
                "socket path must be an absolute path without NUL bytes"
            )
        ) {
            _ = try await client.pause(context: context, durationMillis: 1)
        }
    }

    @Test("trusted schema rejection stays incompatible")
    func exactLegacyRejection() async throws {
        let response = Data(
            #"{"schema_version":1,"request_id":1,"ok":false,"error":{"code":"unsupported_schema","message":"schema 3 unsupported"}}"#.utf8
        )
        let client = SocketClient(
            socketPath: "/tmp/unlinger-test.sock",
            exchanger: StubExchanger(output: .response(response))
        )
        await #expect(throws: ClientError.incompatibleDaemon("schema 3 unsupported")) {
            _ = try await client.pause(context: context, durationMillis: 1)
        }
    }
}
