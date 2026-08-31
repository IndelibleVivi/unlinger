import Foundation

/// Validates a raw response line against the schema-v2 envelope contract and
/// decodes `payload.data` into a concrete DTO.
///
/// The seam uses JSONSerialization once (to validate the envelope and isolate
/// `payload.data` without a lossy intermediate JSON model), then decodes the
/// data object with a typed Codable pass.
enum ResponseDecoder {
    /// Unit payload variants such as `resumed` carry no `data`.
    struct Empty: Decodable, Equatable, Sendable {}

    /// Decoded payload plus the verbatim `payload.data` JSON, so callers like
    /// diagnostics export can write the bundle without a lossy re-encode.
    struct Decoded<T: Decodable & Sendable>: Sendable {
        let value: T
        let rawData: Data?
    }

    static func decode<T: Decodable & Sendable>(
        _ type: T.Type,
        expectedPayloadType: String,
        requestID: UInt64,
        line: Data
    ) throws(ClientError) -> T {
        try decodeWithRaw(type, expectedPayloadType: expectedPayloadType, requestID: requestID, line: line).value
    }

    static func decodeWithRaw<T: Decodable & Sendable>(
        _ type: T.Type,
        expectedPayloadType: String,
        requestID: UInt64,
        line: Data
    ) throws(ClientError) -> Decoded<T> {
        let object: Any
        do {
            object = try JSONSerialization.jsonObject(with: line)
        } catch {
            throw .protocolError("response is not valid JSON")
        }
        guard let envelope = object as? [String: Any] else {
            throw .protocolError("response envelope is not an object")
        }
        guard let schemaVersion = envelope["schema_version"] as? Int, schemaVersion == 2 else {
            throw .protocolError("unexpected schema_version")
        }
        guard let echoedID = (envelope["request_id"] as? NSNumber)?.uint64Value,
              echoedID == requestID
        else {
            throw .protocolError("request_id mismatch")
        }
        guard let ok = envelope["ok"] as? Bool else {
            throw .protocolError("missing ok")
        }
        let payload = envelope["payload"] as? [String: Any]
        let errorObject = envelope["error"] as? [String: Any]
        // ok/payload/error must be mutually consistent.
        switch (ok, payload != nil, errorObject != nil) {
        case (true, true, false), (false, false, true):
            break
        default:
            throw .protocolError("ok/payload/error are not mutually consistent")
        }

        if ok {
            guard let payload,
                  let payloadType = payload["type"] as? String
            else {
                throw .protocolError("payload missing type")
            }
            guard payloadType == expectedPayloadType else {
                throw .protocolError("unexpected payload type \(payloadType)")
            }
            if T.self == Empty.self {
                return Decoded(value: Empty() as! T, rawData: nil)
            }
            guard let data = payload["data"], !(data is NSNull) else {
                throw .protocolError("payload missing data")
            }
            let dataJSON: Data
            do {
                dataJSON = try JSONSerialization.data(withJSONObject: data)
            } catch {
                throw .protocolError("payload data is not serializable")
            }
            let decoder = JSONDecoder()
            decoder.keyDecodingStrategy = .convertFromSnakeCase
            do {
                return Decoded(value: try decoder.decode(T.self, from: dataJSON), rawData: dataJSON)
            } catch {
                throw .protocolError("payload data does not match contract: \(error)")
            }
        } else {
            guard let errorObject,
                  let code = errorObject["code"] as? String,
                  let message = errorObject["message"] as? String
            else {
                throw .protocolError("error envelope missing code/message")
            }
            throw .serverError(code: code, message: message)
        }
    }
}
