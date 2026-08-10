import Foundation

// Two views of a `CBORValue` that neither the data model nor the codec
// owns: the JSON value it stands for, and the diagnostic notation RFC
// 8949 renders it in.
//
// The JSON bridge exists because some engine payloads are `serde_json`
// values that were encoded to CBOR on the way out, so the cases that
// survive the trip are the JSON ones and a host often wants them back in
// that form. The diagnostic notation exists because a failing expectation
// on a payload should print something a person can read against the
// committed fixtures, which are written in that notation.

// MARK: - JSON

extension CBORValue {
    /// The JSON value this item stands for, or `nil` where CBOR holds
    /// something JSON has no spelling for.
    ///
    /// Objects come back as `[String: Any]`, arrays as `[Any]`, text as
    /// `String`, integers as `Int64`, floats as `Double`, booleans as
    /// `Bool`, and null as `NSNull`, which is the shape
    /// `JSONSerialization` reads. A tag is looked through rather than
    /// refused, since a tag carries no JSON meaning.
    ///
    /// The cases that answer `nil` are byte strings, undefined and the
    /// other simple values, a map with a non-text key, and an integer
    /// magnitude past `Int64`. Each of them means the payload was not a
    /// JSON value to begin with.
    public var jsonObject: Any? {
        switch untagged {
        case .null:
            return NSNull()
        case .bool(let flag):
            return flag
        case .unsigned(let magnitude):
            guard let exact = Int64(exactly: magnitude) else { return nil }
            return exact
        case .negative(let magnitude):
            guard magnitude <= UInt64(Int64.max) else { return nil }
            return Int64(bitPattern: ~magnitude)
        case .float(let number):
            return number
        case .textString(let text):
            return text
        case .array(let elements):
            var built: [Any] = []
            built.reserveCapacity(elements.count)
            for element in elements {
                guard let converted = element.jsonObject else { return nil }
                built.append(converted)
            }
            return built
        case .map(let entries):
            var built: [String: Any] = [:]
            built.reserveCapacity(entries.count)
            for entry in entries {
                guard let key = entry.key.stringValue, let converted = entry.value.jsonObject
                else { return nil }
                built[key] = converted
            }
            return built
        case .byteString, .tag, .simple, .undefined:
            return nil
        }
    }

    /// The CBOR item a `JSONSerialization` object stands for, or `nil`
    /// where the object is not one.
    ///
    /// This is the inverse of ``jsonObject`` over the values that type
    /// produces. Map entries are ordered by key so that one object
    /// always encodes to one byte string, since a Swift dictionary has
    /// no order of its own to preserve.
    ///
    /// - Parameter jsonObject: a value `JSONSerialization` produced or
    ///   would accept.
    public init?(jsonObject: Any) {
        switch jsonObject {
        case is NSNull:
            self = .null
        case let number as NSNumber:
            // Every JSON scalar arrives boxed, and the box is what tells
            // a boolean from the integer one. `as? Bool` does not: an
            // `NSNumber` holding `1` answers that cast affirmatively, so
            // an integer would silently become `true`.
            if CFGetTypeID(number) == CFBooleanGetTypeID() {
                self = .bool(number.boolValue)
            } else if number.objCType.pointee == Int8(UInt8(ascii: "d"))
                || number.objCType.pointee == Int8(UInt8(ascii: "f"))
            {
                self = .float(number.doubleValue)
            } else {
                let wide = number.int64Value
                self = wide < 0 ? .negative(~UInt64(bitPattern: wide)) : .unsigned(UInt64(wide))
            }
        case let flag as Bool:
            self = .bool(flag)
        case let number as Int64:
            self = number < 0 ? .negative(~UInt64(bitPattern: number)) : .unsigned(UInt64(number))
        case let number as Double:
            self = .float(number)
        case let text as String:
            self = .textString(text)
        case let elements as [Any]:
            var built: [CBORValue] = []
            built.reserveCapacity(elements.count)
            for element in elements {
                guard let converted = CBORValue(jsonObject: element) else { return nil }
                built.append(converted)
            }
            self = .array(built)
        case let fields as [String: Any]:
            var built: [Entry] = []
            built.reserveCapacity(fields.count)
            for key in fields.keys.sorted() {
                guard let value = fields[key], let converted = CBORValue(jsonObject: value) else {
                    return nil
                }
                built.append(Entry(key: .textString(key), value: converted))
            }
            self = .map(built)
        default:
            return nil
        }
    }
}

// MARK: - Diagnostic notation

extension CBORValue: CustomStringConvertible {
    /// This item in the diagnostic notation of RFC 8949 section 8.
    ///
    /// Integers print bare, text strings quoted, byte strings as
    /// `h'…'`, arrays as `[a, b]`, maps as `{k: v}`, a tag as
    /// `N(item)`, and the four named simple values as `true`, `false`,
    /// `null`, and `undefined`. Any other simple value prints as
    /// `simple(N)`.
    ///
    /// That is the notation the RFC's own test vectors use, and the one
    /// the committed fixtures are written against, so a failing
    /// expectation prints something that can be read against them.
    public var description: String {
        switch self {
        case .unsigned(let magnitude):
            return "\(magnitude)"
        case .negative(let magnitude):
            guard magnitude <= UInt64(Int64.max) else { return "-\(magnitude)-1" }
            return "\(Int64(bitPattern: ~magnitude))"
        case .byteString(let payload):
            let digits = payload.map { byte in
                let text = String(byte, radix: 16)
                return text.count == 1 ? "0" + text : text
            }
            return "h'\(digits.joined())'"
        case .textString(let text):
            let escaped =
                text
                .replacingOccurrences(of: "\\", with: "\\\\")
                .replacingOccurrences(of: "\"", with: "\\\"")
            return "\"\(escaped)\""
        case .array(let elements):
            return "[\(elements.map(\.description).joined(separator: ", "))]"
        case .map(let entries):
            let rendered = entries.map { "\($0.key): \($0.value)" }
            return "{\(rendered.joined(separator: ", "))}"
        case .tag(let number, let item):
            return "\(number)(\(item))"
        case .simple(let value):
            return "simple(\(value))"
        case .bool(let flag):
            return flag ? "true" : "false"
        case .null:
            return "null"
        case .undefined:
            return "undefined"
        case .float(let number):
            return "\(number)"
        }
    }
}
