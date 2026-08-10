import Foundation

// MARK: - Value

/// A leaf datum carried by a W-type instance node.
///
/// `Value` is the free term algebra of JSON-like data: primitive atoms,
/// two record constructors (``unknown(_:)`` and ``opaque(type:fields:)``),
/// and one list constructor (``list(_:)``). Closing over the record and
/// list constructors is what lets data with no schema anchor round-trip
/// through `extra_fields` without being flattened into strings.
///
/// The type reaches the wire as an externally tagged Rust enum: every
/// variant but one is a single-entry CBOR map keyed by the Rust variant
/// name, and ``null`` is the bare text string `Null`. That asymmetry is
/// load-bearing. CBOR null (`0xf6`) stands for a field that is absent
/// altogether, which is how `Node.value` spells "no value at all";
/// `Null` stands for a value that is present and is null.
///
/// Two field spellings keep a trailing underscore, because the Rust
/// fields do and neither carries a rename: ``blob(ref:mime:size:)``
/// writes `ref_` and ``opaque(type:fields:)`` writes `type_`.
public indirect enum Value: Codable, Hashable, Sendable {
    /// A boolean.
    case bool(Bool)
    /// A 64-bit signed integer.
    case int(Int64)
    /// A 64-bit floating-point number.
    case float(Double)
    /// A UTF-8 string.
    case string(String)
    /// Raw bytes, which reach the wire as an array of one integer per
    /// byte rather than as a CBOR byte string.
    case bytes([UInt8])
    /// A content-identifier link.
    case cidLink(String)
    /// A blob reference: its identifier, its MIME type, and its size in
    /// bytes.
    case blob(ref: String, mime: String, size: UInt64)
    /// A token, which names one variant of an enumeration.
    case token(String)
    /// An explicit null, which the wire spells as the text string
    /// `Null`.
    case null
    /// A protocol-specific typed value: a type identifier and the
    /// fields it carries.
    case opaque(type: String, fields: [String: Value])
    /// A record with no schema anchor, indexed by field name.
    case unknown([String: Value])
    /// An ordered list.
    case list([Value])
    /// A placeholder standing for an unknown value with an identity of
    /// its own.
    ///
    /// The term-level chase in the migration engine fills the
    /// existentially quantified positions of a dependency's head with
    /// these. Two of them sharing an identity are the same unknown
    /// value, which is what distinguishes them from ``null``.
    case labeledNull(UInt64)
}

// MARK: - Value coding

extension Value {
    /// The variant names, which are the Rust spellings unchanged.
    private enum CodingKeys: String, CodingKey {
        case bool = "Bool"
        case int = "Int"
        case float = "Float"
        case string = "Str"
        case bytes = "Bytes"
        case cidLink = "CidLink"
        case blob = "Blob"
        case token = "Token"
        case opaque = "Opaque"
        case unknown = "Unknown"
        case list = "List"
        case labeledNull = "LabeledNull"
    }

    /// The fields of a ``blob(ref:mime:size:)`` payload, in Rust
    /// declaration order.
    private enum BlobKeys: String, CodingKey {
        case ref = "ref_"
        case mime
        case size
    }

    /// The fields of an ``opaque(type:fields:)`` payload, in Rust
    /// declaration order.
    private enum OpaqueKeys: String, CodingKey {
        case type = "type_"
        case fields
    }

    /// The text string ``null`` occupies on the wire.
    private static let nullSpelling = "Null"

    /// Read one value.
    ///
    /// A text string is tried first, because ``null`` is the one
    /// variant that is not a map; everything else is a single-entry map
    /// keyed by the variant name.
    ///
    /// - Throws: `DecodingError` when the payload is neither the `Null`
    ///   string nor a map naming exactly one known variant.
    public init(from decoder: any Decoder) throws {
        if let single = try? decoder.singleValueContainer(),
            let text = try? single.decode(String.self)
        {
            guard text == Self.nullSpelling else {
                throw DecodingError.dataCorrupted(
                    DecodingError.Context(
                        codingPath: decoder.codingPath,
                        debugDescription:
                            "\(text) is not a Value; the only bare string a Value takes is "
                            + Self.nullSpelling
                    )
                )
            }
            self = .null
            return
        }

        let container = try decoder.container(keyedBy: CodingKeys.self)
        guard container.allKeys.count == 1, let key = container.allKeys.first else {
            throw DecodingError.dataCorrupted(
                DecodingError.Context(
                    codingPath: decoder.codingPath,
                    debugDescription: "a Value is one entry keyed by a variant name"
                )
            )
        }

        switch key {
        case .bool:
            self = .bool(try container.decode(Bool.self, forKey: .bool))
        case .int:
            self = .int(try container.decode(Int64.self, forKey: .int))
        case .float:
            self = .float(try container.decode(Double.self, forKey: .float))
        case .string:
            self = .string(try container.decode(String.self, forKey: .string))
        case .bytes:
            self = .bytes(try container.decode([UInt8].self, forKey: .bytes))
        case .cidLink:
            self = .cidLink(try container.decode(String.self, forKey: .cidLink))
        case .blob:
            let payload = try container.nestedContainer(keyedBy: BlobKeys.self, forKey: .blob)
            self = .blob(
                ref: try payload.decode(String.self, forKey: .ref),
                mime: try payload.decode(String.self, forKey: .mime),
                size: try payload.decode(UInt64.self, forKey: .size)
            )
        case .token:
            self = .token(try container.decode(String.self, forKey: .token))
        case .opaque:
            let payload = try container.nestedContainer(keyedBy: OpaqueKeys.self, forKey: .opaque)
            self = .opaque(
                type: try payload.decode(String.self, forKey: .type),
                fields: try payload.decode([String: Value].self, forKey: .fields)
            )
        case .unknown:
            self = .unknown(try container.decode([String: Value].self, forKey: .unknown))
        case .list:
            self = .list(try container.decode([Value].self, forKey: .list))
        case .labeledNull:
            self = .labeledNull(try container.decode(UInt64.self, forKey: .labeledNull))
        }
    }

    /// Write one value.
    public func encode(to encoder: any Encoder) throws {
        if case .null = self {
            var single = encoder.singleValueContainer()
            try single.encode(Self.nullSpelling)
            return
        }

        var container = encoder.container(keyedBy: CodingKeys.self)
        switch self {
        case .bool(let flag):
            try container.encode(flag, forKey: .bool)
        case .int(let number):
            try container.encode(number, forKey: .int)
        case .float(let number):
            try container.encode(number, forKey: .float)
        case .string(let text):
            try container.encode(text, forKey: .string)
        case .bytes(let payload):
            try container.encode(payload, forKey: .bytes)
        case .cidLink(let link):
            try container.encode(link, forKey: .cidLink)
        case .blob(let ref, let mime, let size):
            var payload = container.nestedContainer(keyedBy: BlobKeys.self, forKey: .blob)
            try payload.encode(ref, forKey: .ref)
            try payload.encode(mime, forKey: .mime)
            try payload.encode(size, forKey: .size)
        case .token(let token):
            try container.encode(token, forKey: .token)
        case .null:
            // Handled above, before the keyed container was opened.
            break
        case .opaque(let type, let fields):
            var payload = container.nestedContainer(keyedBy: OpaqueKeys.self, forKey: .opaque)
            try payload.encode(type, forKey: .type)
            try payload.encode(fields, forKey: .fields)
        case .unknown(let fields):
            try container.encode(fields, forKey: .unknown)
        case .list(let elements):
            try container.encode(elements, forKey: .list)
        case .labeledNull(let identity):
            try container.encode(identity, forKey: .labeledNull)
        }
    }
}

// MARK: - Value accessors

extension Value {
    /// The boolean this value carries, or `nil` when it carries
    /// something else.
    public var asBool: Bool? {
        if case .bool(let flag) = self { return flag }
        return nil
    }

    /// The integer this value carries, or `nil` when it carries
    /// something else.
    ///
    /// A ``float(_:)`` does not answer here even when it holds a whole
    /// number, because the two are distinct on the wire.
    public var asInt: Int64? {
        if case .int(let number) = self { return number }
        return nil
    }

    /// The floating-point number this value carries, or `nil` when it
    /// carries something else.
    public var asDouble: Double? {
        if case .float(let number) = self { return number }
        return nil
    }

    /// The string this value carries, or `nil` when it carries
    /// something else.
    ///
    /// ``cidLink(_:)`` and ``token(_:)`` are strings on the wire but
    /// name different things, so neither answers here; read them with
    /// ``asCidLink`` and ``asToken``.
    public var asString: String? {
        if case .string(let text) = self { return text }
        return nil
    }

    /// The link this value carries, or `nil` when it carries something
    /// else.
    public var asCidLink: String? {
        if case .cidLink(let link) = self { return link }
        return nil
    }

    /// The token this value carries, or `nil` when it carries something
    /// else.
    public var asToken: String? {
        if case .token(let token) = self { return token }
        return nil
    }

    /// The bytes this value carries, or `nil` when it carries something
    /// else.
    public var asBytes: [UInt8]? {
        if case .bytes(let payload) = self { return payload }
        return nil
    }

    /// The elements this value carries, or `nil` when it is not a list.
    public var asList: [Value]? {
        if case .list(let elements) = self { return elements }
        return nil
    }

    /// The fields this value carries, or `nil` when it is neither
    /// record constructor.
    ///
    /// Both ``unknown(_:)`` and ``opaque(type:fields:)`` are finite
    /// products indexed by field name, so both answer here. The type
    /// identifier of an opaque value is not part of the answer; read it
    /// with ``asOpaqueType``.
    public var asRecord: [String: Value]? {
        switch self {
        case .unknown(let fields): fields
        case .opaque(_, let fields): fields
        default: nil
        }
    }

    /// The type identifier of an opaque value, or `nil` when this value
    /// is not one.
    public var asOpaqueType: String? {
        if case .opaque(let type, _) = self { return type }
        return nil
    }

    /// The identity of a labeled null, or `nil` when this value is not
    /// one.
    public var asLabeledNull: UInt64? {
        if case .labeledNull(let identity) = self { return identity }
        return nil
    }

    /// Whether this value is the explicit null.
    ///
    /// This is a value that is present and null. A field that carries
    /// no value at all is a Swift `nil` on ``Node/value``, not a
    /// ``null``.
    public var isNull: Bool {
        if case .null = self { return true }
        return false
    }

    /// The value bound to `key` in a record, or `nil` when this value
    /// is not a record or the field is absent.
    public subscript(key: String) -> Value? {
        asRecord?[key]
    }

    /// The element at `index` of a list, or `nil` when this value is
    /// not a list or the index is out of bounds.
    public subscript(index: Int) -> Value? {
        guard let elements = asList, elements.indices.contains(index) else { return nil }
        return elements[index]
    }
}

// MARK: - Value literals

extension Value: ExpressibleByBooleanLiteral {
    /// A boolean literal is a ``bool(_:)``.
    public init(booleanLiteral value: Bool) {
        self = .bool(value)
    }
}

extension Value: ExpressibleByIntegerLiteral {
    /// An integer literal is an ``int(_:)``.
    public init(integerLiteral value: Int64) {
        self = .int(value)
    }
}

extension Value: ExpressibleByFloatLiteral {
    /// A floating-point literal is a ``float(_:)``.
    public init(floatLiteral value: Double) {
        self = .float(value)
    }
}

extension Value: ExpressibleByStringLiteral {
    /// A string literal is a ``string(_:)``, which is the `Str` variant
    /// and not a token or a link.
    public init(stringLiteral value: String) {
        self = .string(value)
    }
}

extension Value: ExpressibleByArrayLiteral {
    /// An array literal is a ``list(_:)``.
    public init(arrayLiteral elements: Value...) {
        self = .list(elements)
    }
}

extension Value: ExpressibleByDictionaryLiteral {
    /// A dictionary literal is an ``unknown(_:)``, the record
    /// constructor for data with no schema anchor. A repeated key keeps
    /// its last binding.
    public init(dictionaryLiteral elements: (String, Value)...) {
        var fields: [String: Value] = [:]
        fields.reserveCapacity(elements.count)
        for (key, value) in elements {
            fields[key] = value
        }
        self = .unknown(fields)
    }
}

// MARK: - FieldPresence

/// Whether a node's field is present, explicitly null, or absent.
///
/// The three cases are not the same CBOR major type: ``present(_:)`` is
/// a single-entry map, and the other two are bare text strings. A
/// decoder branches on that before anything else.
///
/// ``Node/value`` is an optional `FieldPresence`, so the full range a
/// node's value spans is four-way: Swift `nil` for a key that carries
/// CBOR null, ``null`` for the string `Null`, ``absent``, and
/// ``present(_:)``.
public enum FieldPresence: Codable, Hashable, Sendable {
    /// The field is present and carries `value`.
    case present(Value)
    /// The field is present and explicitly null.
    case null
    /// The field was not provided.
    case absent
}

extension FieldPresence {
    /// The one variant that carries a payload.
    private enum CodingKeys: String, CodingKey {
        case present = "Present"
    }

    /// The text string ``null`` occupies on the wire.
    private static let nullSpelling = "Null"

    /// The text string ``absent`` occupies on the wire.
    private static let absentSpelling = "Absent"

    /// Read a presence marker.
    ///
    /// - Throws: `DecodingError` when the payload is neither `Null` nor
    ///   `Absent` nor a map keyed by `Present`.
    public init(from decoder: any Decoder) throws {
        if let single = try? decoder.singleValueContainer(),
            let text = try? single.decode(String.self)
        {
            switch text {
            case Self.nullSpelling:
                self = .null
            case Self.absentSpelling:
                self = .absent
            default:
                throw DecodingError.dataCorrupted(
                    DecodingError.Context(
                        codingPath: decoder.codingPath,
                        debugDescription:
                            "\(text) is not a FieldPresence; the bare strings it takes are "
                            + "\(Self.nullSpelling) and \(Self.absentSpelling)"
                    )
                )
            }
            return
        }

        let container = try decoder.container(keyedBy: CodingKeys.self)
        self = .present(try container.decode(Value.self, forKey: .present))
    }

    /// Write a presence marker.
    public func encode(to encoder: any Encoder) throws {
        switch self {
        case .present(let value):
            var container = encoder.container(keyedBy: CodingKeys.self)
            try container.encode(value, forKey: .present)
        case .null:
            var single = encoder.singleValueContainer()
            try single.encode(Self.nullSpelling)
        case .absent:
            var single = encoder.singleValueContainer()
            try single.encode(Self.absentSpelling)
        }
    }
}

extension FieldPresence {
    /// The value carried by a present field, or `nil` for the other two
    /// cases.
    public var asValue: Value? {
        if case .present(let value) = self { return value }
        return nil
    }

    /// Whether the field is present and carries a value.
    public var isPresent: Bool {
        if case .present = self { return true }
        return false
    }

    /// Whether the field is present and explicitly null.
    public var isNull: Bool {
        if case .null = self { return true }
        return false
    }

    /// Whether the field was not provided.
    public var isAbsent: Bool {
        if case .absent = self { return true }
        return false
    }
}
