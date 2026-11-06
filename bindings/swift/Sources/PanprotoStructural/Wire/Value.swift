import Foundation

// MARK: - Exact scalar payloads

/// An exact decimal as Rust serializes it.
///
/// The exponent is a decimal string because the engine's `WideInt` has no
/// fixed-width bound. A finite value leaves `class` off the wire; infinities
/// and NaNs carry it explicitly.
public struct DecimalValue: Codable, Hashable, Sendable {
    /// Whether the written value is negative.
    public var negative: Bool
    /// Canonical coefficient digits.
    public var digits: String
    /// The unbounded power of ten, as canonical decimal text.
    public var exponent: String
    /// Whether the value is finite, infinite, or a NaN.
    public var decimalClass: DecimalClass

    /// Assemble an exact decimal payload.
    public init(
        negative: Bool = false,
        digits: String,
        exponent: String,
        decimalClass: DecimalClass = .finite
    ) {
        self.negative = negative
        self.digits = digits
        self.exponent = exponent
        self.decimalClass = decimalClass
    }

    private enum CodingKeys: String, CodingKey {
        case negative
        case digits
        case exponent
        case decimalClass = "class"
    }

    /// Read the engine's named-field decimal representation.
    public init(from decoder: any Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        let value = Self(
            negative: try container.decode(Bool.self, forKey: .negative),
            digits: try container.decode(String.self, forKey: .digits),
            exponent: try container.decode(String.self, forKey: .exponent),
            decimalClass: try container.decodeIfPresent(DecimalClass.self, forKey: .decimalClass)
                ?? .finite
        )
        guard value.isWellFormed else {
            throw DecodingError.dataCorrupted(
                .init(codingPath: container.codingPath, debugDescription: "noncanonical decimal"))
        }
        self = value
    }

    /// Write the engine's named-field decimal representation.
    public func encode(to encoder: any Encoder) throws {
        guard isWellFormed else {
            throw EncodingError.invalidValue(
                self,
                .init(codingPath: encoder.codingPath, debugDescription: "noncanonical decimal"))
        }
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(negative, forKey: .negative)
        try container.encode(digits, forKey: .digits)
        try container.encode(exponent, forKey: .exponent)
        if decimalClass != .finite {
            try container.encode(decimalClass, forKey: .decimalClass)
        }
    }

    private var isWellFormed: Bool {
        guard Self.isCanonicalUnsigned(digits), Self.isCanonicalInteger(exponent) else {
            return false
        }
        switch decimalClass {
        case .finite:
            return true
        case .infinite:
            return digits == "0" && exponent == "0"
        case .nan(_, let payload):
            return digits == "0" && exponent == "0" && Self.isCanonicalUnsigned(payload)
        }
    }

    private static func isCanonicalUnsigned(_ text: String) -> Bool {
        !text.isEmpty && text.allSatisfy(\.isASCIIWholeNumber)
            && (text == "0" || text.first != "0")
    }

    private static func isCanonicalInteger(_ text: String) -> Bool {
        if let first = text.first, first == "-" {
            let magnitude = String(text.dropFirst())
            return magnitude != "0" && isCanonicalUnsigned(magnitude)
        }
        return isCanonicalUnsigned(text)
    }
}

/// The General Decimal Arithmetic class of an exact decimal.
public enum DecimalClass: Codable, Hashable, Sendable {
    case finite
    case infinite
    case nan(signalling: Bool, payload: String)

    private enum Tag: String, CodingKey { case naN = "na_n" }
    private enum NaNKeys: String, CodingKey { case signalling, payload }

    /// Read a unit class name or the `na_n` payload.
    public init(from decoder: any Decoder) throws {
        if let single = try? decoder.singleValueContainer(),
            let tag = try? single.decode(String.self)
        {
            switch tag {
            case "finite": self = .finite
            case "infinite": self = .infinite
            default:
                throw DecodingError.dataCorruptedError(
                    in: single, debugDescription: "\(tag) is not a decimal class")
            }
            return
        }
        let container = try decoder.container(keyedBy: Tag.self)
        guard container.allKeys == [.naN] else {
            throw DecodingError.dataCorrupted(
                .init(codingPath: decoder.codingPath, debugDescription: "unknown decimal class"))
        }
        let payload = try container.nestedContainer(keyedBy: NaNKeys.self, forKey: .naN)
        self = .nan(
            signalling: try payload.decode(Bool.self, forKey: .signalling),
            payload: try payload.decode(String.self, forKey: .payload)
        )
    }

    /// Write a unit class name or the `na_n` payload.
    public func encode(to encoder: any Encoder) throws {
        switch self {
        case .finite, .infinite:
            var single = encoder.singleValueContainer()
            try single.encode(self == .finite ? "finite" : "infinite")
        case .nan(let signalling, let diagnostic):
            var container = encoder.container(keyedBy: Tag.self)
            var payload = container.nestedContainer(keyedBy: NaNKeys.self, forKey: .naN)
            try payload.encode(signalling, forKey: .signalling)
            try payload.encode(diagnostic, forKey: .payload)
        }
    }
}

/// An integer outside the engine's `i64` carrier.
public struct BigIntValue: Codable, Hashable, Sendable {
    /// Whether the integer is negative.
    public var negative: Bool
    /// Magnitude digits with leading and trailing zeros removed.
    public var significand: String
    /// The unbounded trailing-zero count, as canonical decimal text.
    public var zeros: String

    /// Assemble an exact wide-integer payload.
    public init(negative: Bool = false, significand: String, zeros: String = "0") {
        self.negative = negative
        self.significand = significand
        self.zeros = zeros
    }

    private enum CodingKeys: String, CodingKey { case negative, significand, zeros }

    /// Read and validate the engine's wide-integer representation.
    public init(from decoder: any Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        let value = Self(
            negative: try container.decode(Bool.self, forKey: .negative),
            significand: try container.decode(String.self, forKey: .significand),
            zeros: try container.decode(String.self, forKey: .zeros)
        )
        guard value.isWellFormed else {
            throw DecodingError.dataCorrupted(
                .init(
                    codingPath: container.codingPath, debugDescription: "noncanonical big integer"))
        }
        self = value
    }

    /// Write a validated wide-integer representation.
    public func encode(to encoder: any Encoder) throws {
        guard isWellFormed else {
            throw EncodingError.invalidValue(
                self,
                .init(codingPath: encoder.codingPath, debugDescription: "noncanonical big integer"))
        }
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(negative, forKey: .negative)
        try container.encode(significand, forKey: .significand)
        try container.encode(zeros, forKey: .zeros)
    }

    private var isWellFormed: Bool {
        guard !significand.isEmpty, significand.allSatisfy(\.isASCIIWholeNumber),
            significand.first != "0", significand.last != "0",
            !zeros.isEmpty, zeros.allSatisfy(\.isASCIIWholeNumber),
            zeros == "0" || zeros.first != "0"
        else { return false }

        let trailingZeroCount: Int
        if zeros == "0" {
            trailingZeroCount = 0
        } else {
            // Any count larger than nineteen necessarily puts the value
            // outside i64; avoid converting an otherwise unbounded WideInt.
            guard zeros.count <= 2, let count = Int(zeros), count <= 19 else { return true }
            trailingZeroCount = count
        }

        let digitCount = significand.count + trailingZeroCount
        if digitCount > 19 { return true }
        if digitCount < 19 { return false }

        let magnitude = significand + String(repeating: "0", count: trailingZeroCount)
        let limit = negative ? "9223372036854775808" : "9223372036854775807"
        return magnitude > limit
    }
}

extension Character {
    fileprivate var isASCIIWholeNumber: Bool {
        unicodeScalars.count == 1
            && (unicodeScalars.first.map { $0.value >= 48 && $0.value <= 57 } ?? false)
    }
}

/// A date, time, date-time, or extended temporal payload.
///
/// Temporal syntax is deliberately retained as its exact serde value. The
/// engine has a large, extensible temporal carrier; validating the external
/// tag here prevents an unknown variant from being accepted while preserving
/// every lexical-form field byte for byte across decode and encode.
public struct TemporalValue: Codable, Hashable, Sendable {
    /// The exact externally tagged temporal representation.
    public var representation: CBORValue

    /// Admit one known temporal variant.
    public init(representation: CBORValue) throws {
        try Self.validate(representation)
        self.representation = representation
    }

    /// Read and validate a temporal representation.
    public init(from decoder: any Decoder) throws {
        try self.init(representation: decoder.singleValueContainer().decode(CBORValue.self))
    }

    /// Write the retained temporal representation.
    public func encode(to encoder: any Encoder) throws {
        var single = encoder.singleValueContainer()
        try single.encode(representation)
    }

    private static func validate(_ value: CBORValue) throws {
        guard case .map(let entries) = value.untagged, entries.count == 1,
            case .textString(let tag) = entries[0].key,
            ["date", "time", "date_time", "extended"].contains(tag)
        else {
            throw DecodingError.dataCorrupted(
                .init(codingPath: [], debugDescription: "unknown temporal variant"))
        }
    }
}

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
    /// An exact decimal, including its written scale.
    case decimal(DecimalValue)
    /// An integer outside the range of `Int64`.
    case bigInt(BigIntValue)
    /// A date, time, or date-time with its written form.
    case temporal(TemporalValue)
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
        case decimal = "Decimal"
        case bigInt = "BigInt"
        case temporal = "Temporal"
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
        case .decimal:
            self = .decimal(try container.decode(DecimalValue.self, forKey: .decimal))
        case .bigInt:
            self = .bigInt(try container.decode(BigIntValue.self, forKey: .bigInt))
        case .temporal:
            self = .temporal(try container.decode(TemporalValue.self, forKey: .temporal))
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
        case .decimal(let value):
            try container.encode(value, forKey: .decimal)
        case .bigInt(let value):
            try container.encode(value, forKey: .bigInt)
        case .temporal(let value):
            try container.encode(value, forKey: .temporal)
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
