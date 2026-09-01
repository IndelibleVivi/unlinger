import Foundation

/// Strictly validates the schema-v3 response envelope, then decodes the
/// concrete `payload.data` DTO with exact CodingKeys. JSONSerialization is used
/// only after typed header validation to isolate and semantically re-encode the
/// data object; it is never used to coerce trusted integer fields.
enum ResponseDecoder {
    struct Empty: Decodable, Equatable, Sendable {}

    struct Decoded<T: Decodable & Sendable>: Sendable {
        let value: T
        /// Semantic-lossless JSON for the modeled payload object. Unknown JSON
        /// fields are retained, but whitespace and object key order may change.
        let rawData: Data?
    }

    private struct Header: Decodable {
        let schemaVersion: Int
        let requestID: UInt64
        let ok: Bool
        let payload: PayloadHeader?
        let error: ErrorHeader?

        private enum CodingKeys: String, CodingKey {
            case schemaVersion = "schema_version"
            case requestID = "request_id"
            case ok, payload, error
        }
    }

    private struct PayloadHeader: Decodable {
        let type: String
    }

    private struct ErrorHeader: Decodable {
        let code: String
        let message: String
    }

    static func decode<T: Decodable & Sendable>(
        _ type: T.Type,
        expectedPayloadType: String,
        requestID: UInt64,
        line: Data
    ) throws(ClientError) -> T {
        try decodeWithRaw(
            type,
            expectedPayloadType: expectedPayloadType,
            requestID: requestID,
            line: line
        ).value
    }

    static func decodeWithRaw<T: Decodable & Sendable>(
        _ type: T.Type,
        expectedPayloadType: String,
        requestID: UInt64,
        line: Data
    ) throws(ClientError) -> Decoded<T> {
        let header: Header
        do {
            header = try JSONDecoder().decode(Header.self, from: line)
        } catch {
            throw .protocolError("response envelope does not match the typed contract")
        }

        if header.schemaVersion == 1,
           header.requestID == requestID,
           header.ok == false,
           header.payload == nil,
           header.error?.code == "unsupported_schema",
           let message = header.error?.message
        {
            throw .incompatibleDaemon(message)
        }
        guard header.schemaVersion == 3 else {
            throw .protocolError("unexpected schema_version")
        }
        guard header.requestID == requestID else {
            throw .protocolError("request_id mismatch")
        }
        switch (header.ok, header.payload != nil, header.error != nil) {
        case (true, true, false), (false, false, true): break
        default:
            throw .protocolError("ok/payload/error are not mutually consistent")
        }

        if !header.ok {
            guard let error = header.error else {
                throw .protocolError("error envelope missing code/message")
            }
            throw .serverError(code: error.code, message: error.message)
        }

        guard header.payload?.type == expectedPayloadType else {
            throw .protocolError("unexpected payload type")
        }
        if T.self == Empty.self {
            return Decoded(value: Empty() as! T, rawData: nil)
        }

        let object: Any
        do {
            object = try JSONSerialization.jsonObject(with: line)
        } catch {
            throw .protocolError("response is not valid JSON")
        }
        guard let envelope = object as? [String: Any],
              let payload = envelope["payload"] as? [String: Any],
              let data = payload["data"],
              !(data is NSNull)
        else {
            throw .protocolError("payload missing data")
        }
        let dataJSON: Data
        do {
            dataJSON = try JSONSerialization.data(withJSONObject: data, options: [.sortedKeys])
        } catch {
            throw .protocolError("payload data is not serializable")
        }
        do {
            return Decoded(value: try JSONDecoder().decode(T.self, from: dataJSON), rawData: dataJSON)
        } catch {
            throw .protocolError("payload data does not match contract: \(error)")
        }
    }
}
