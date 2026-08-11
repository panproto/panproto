import Foundation

// MARK: - The CBOR data model

/// A CBOR item, held in the shape RFC 8949 gives it.
///
/// The cases follow the eight major types rather than any Swift type
/// they might stand for, which is what lets a caller inspect a payload
/// that has no static Swift type: the engine's answers cross the C ABI
/// as CBOR produced by `serde` and `ciborium`, and a host that wants to
/// look at one field of an unfamiliar payload can decode it here and
/// walk it.
///
/// Two representational points are worth keeping in mind.
///
/// First, negative integers are stored the way the wire stores them.
/// ``negative(_:)`` carries `n` and denotes `-1 - n`, so the whole
/// range down to `-2^64` survives a round trip even though it does not
/// fit in `Int64`. Use ``int64Value`` to get the mathematical value
/// where it fits.
///
/// Second, maps are an ordered list of ``Entry`` rather than a
/// dictionary. Key order is part of the payload: `serde` writes struct
/// fields in declaration order, and preserving that order is what makes
/// a decoded ``CBORValue`` re-encode to the bytes it came from. Keys
/// need not be strings, and duplicate keys are representable, both of
/// which a dictionary would lose.
///
/// Equality follows the encoded form. Floats compare by bit pattern, so
/// `NaN` equals `NaN` and `+0.0` differs from `-0.0`, and the four
/// simple values that have dedicated cases compare equal to the same
/// values spelled as ``simple(_:)``.
public enum CBORValue: Sendable {
    /// An unsigned integer, major type 0.
    case unsigned(UInt64)

    /// A negative integer, major type 1, denoting `-1 - value`.
    case negative(UInt64)

    /// A byte string, major type 2.
    case byteString([UInt8])

    /// A text string, major type 3.
    case textString(String)

    /// An array, major type 4.
    case array([CBORValue])

    /// A map, major type 5, in wire order.
    case map([Entry])

    /// A tagged item, major type 6.
    indirect case tag(number: UInt64, item: CBORValue)

    /// A simple value other than the four that have dedicated cases.
    ///
    /// Build one with ``simpleValue(_:)``, which routes 20 through 23
    /// to ``bool(_:)``, ``null``, and ``undefined``; decoding always
    /// yields that normalized form.
    case simple(UInt8)

    /// The simple values 20 (false) and 21 (true).
    case bool(Bool)

    /// The simple value 22.
    case null

    /// The simple value 23.
    case undefined

    /// A floating-point value, major type 7 with additional information
    /// 25, 26, or 27.
    ///
    /// Half and single precision inputs widen to `Double` on the way
    /// in and narrow again on the way out, so the encoded width is
    /// whichever of the three reproduces the value exactly.
    case float(Double)
}

extension CBORValue {
    /// One key/value pair of a CBOR map.
    public struct Entry: Sendable, Hashable {
        /// The key, which CBOR allows to be any item.
        public var key: CBORValue
        /// The value bound to ``key``.
        public var value: CBORValue

        /// Pair a key with a value.
        public init(key: CBORValue, value: CBORValue) {
            self.key = key
            self.value = value
        }
    }

    /// The simple value `raw` denotes, routing the four named ones to
    /// their dedicated cases.
    public static func simpleValue(_ raw: UInt8) -> CBORValue {
        switch raw {
        case 20: .bool(false)
        case 21: .bool(true)
        case 22: .null
        case 23: .undefined
        default: .simple(raw)
        }
    }

    /// A map with text keys, in the order given.
    ///
    /// This is the shape a `serde` struct takes, so it is the shape a
    /// hand-built payload usually wants.
    public static func textMap(_ pairs: [(String, CBORValue)]) -> CBORValue {
        .map(pairs.map { Entry(key: .textString($0.0), value: $0.1) })
    }

    /// This value with ``simple(_:)`` folded into the named cases, so
    /// that equality and hashing see one spelling per encoded byte.
    private var normalized: CBORValue {
        if case .simple(let raw) = self { return Self.simpleValue(raw) }
        return self
    }
}

// MARK: - Equality and hashing

extension CBORValue: Hashable {
    /// Whether two items encode to the same bytes.
    ///
    /// Floats compare by bit pattern rather than by value, so `NaN`
    /// equals `NaN` and `+0.0` differs from `-0.0`, and a simple value
    /// spelled as ``simple(_:)`` equals the dedicated case for it.
    public static func == (lhs: CBORValue, rhs: CBORValue) -> Bool {
        switch (lhs.normalized, rhs.normalized) {
        case (.unsigned(let a), .unsigned(let b)): a == b
        case (.negative(let a), .negative(let b)): a == b
        case (.byteString(let a), .byteString(let b)): a == b
        case (.textString(let a), .textString(let b)): a == b
        case (.array(let a), .array(let b)): a == b
        case (.map(let a), .map(let b)): a == b
        case (.tag(let an, let ai), .tag(let bn, let bi)): an == bn && ai == bi
        case (.simple(let a), .simple(let b)): a == b
        case (.bool(let a), .bool(let b)): a == b
        case (.null, .null): true
        case (.undefined, .undefined): true
        case (.float(let a), .float(let b)): a.bitPattern == b.bitPattern
        default: false
        }
    }

    /// Feed the item's encoded form to `hasher`, so that items which
    /// encode alike hash alike.
    public func hash(into hasher: inout Hasher) {
        switch normalized {
        case .unsigned(let value):
            hasher.combine(0)
            hasher.combine(value)
        case .negative(let value):
            hasher.combine(1)
            hasher.combine(value)
        case .byteString(let value):
            hasher.combine(2)
            hasher.combine(value)
        case .textString(let value):
            hasher.combine(3)
            hasher.combine(value)
        case .array(let value):
            hasher.combine(4)
            hasher.combine(value)
        case .map(let value):
            hasher.combine(5)
            hasher.combine(value)
        case .tag(let number, let item):
            hasher.combine(6)
            hasher.combine(number)
            hasher.combine(item)
        case .simple(let value):
            hasher.combine(7)
            hasher.combine(value)
        case .bool(let value):
            hasher.combine(8)
            hasher.combine(value)
        case .null:
            hasher.combine(9)
        case .undefined:
            hasher.combine(10)
        case .float(let value):
            hasher.combine(11)
            hasher.combine(value.bitPattern)
        }
    }
}

// MARK: - Inspection

extension CBORValue {
    /// This value with every enclosing semantic tag removed.
    ///
    /// A host that does not care why a payload was tagged reads through
    /// the tag to the item it decorates, which is what the engine's own
    /// decoder does for all tags but the bignum pair.
    public var untagged: CBORValue {
        var current = self
        while case .tag(_, let item) = current { current = item }
        return current
    }

    /// The boolean this value carries, or `nil` if it carries
    /// something else.
    public var boolValue: Bool? {
        if case .bool(let value) = untagged { return value }
        return nil
    }

    /// Whether this value is the simple value 22.
    public var isNull: Bool {
        if case .null = untagged { return true }
        return false
    }

    /// Whether this value is the simple value 23.
    public var isUndefined: Bool {
        if case .undefined = untagged { return true }
        return false
    }

    /// The mathematical value of an integer item, when it fits in
    /// `Int64`.
    public var int64Value: Int64? {
        switch untagged {
        case .unsigned(let value): Int64(exactly: value)
        case .negative(let value):
            value <= UInt64(Int64.max) ? Int64(bitPattern: ~value) : nil
        default: nil
        }
    }

    /// The value of a non-negative integer item, which reaches
    /// `UInt64.max` without loss.
    public var uint64Value: UInt64? {
        if case .unsigned(let value) = untagged { return value }
        return nil
    }

    /// The value of a floating-point item.
    public var doubleValue: Double? {
        if case .float(let value) = untagged { return value }
        return nil
    }

    /// The contents of a text string item.
    public var stringValue: String? {
        if case .textString(let value) = untagged { return value }
        return nil
    }

    /// The contents of a byte string item.
    public var byteStringValue: [UInt8]? {
        if case .byteString(let value) = untagged { return value }
        return nil
    }

    /// The elements of an array item.
    public var arrayValue: [CBORValue]? {
        if case .array(let value) = untagged { return value }
        return nil
    }

    /// The entries of a map item, in wire order.
    public var mapValue: [Entry]? {
        if case .map(let value) = untagged { return value }
        return nil
    }

    /// The value bound to the text key `key`, or `nil` when this is not
    /// a map or the key is absent.
    ///
    /// The first occurrence wins, matching how the decoder resolves a
    /// map that repeats a key.
    public subscript(key: String) -> CBORValue? {
        guard case .map(let entries) = untagged else { return nil }
        return entries.first { $0.key.untagged == .textString(key) }?.value
    }

    /// The element at `index`, or `nil` when this is not an array or
    /// the index is out of bounds.
    public subscript(index: Int) -> CBORValue? {
        guard case .array(let elements) = untagged else { return nil }
        guard elements.indices.contains(index) else { return nil }
        return elements[index]
    }
}

// MARK: - Bytes

extension CBORValue {
    /// Parse exactly one CBOR item from `data`.
    ///
    /// Decoding is tolerant in the ways a forward-compatible host needs
    /// it to be: definite and indefinite lengths both parse, all three
    /// float widths parse, and tags are kept rather than rejected.
    /// Trailing bytes are refused, because a payload crossing the
    /// panproto C ABI is exactly one item.
    ///
    /// - Throws: ``CBORError`` describing where the bytes stop making
    ///   sense.
    public init(decoding data: Data) throws {
        self = try data.withUnsafeBytes { raw in
            var reader = CBORReader(bytes: raw)
            let value = try reader.parseItem(depth: 0)
            try reader.requireExhausted()
            return value
        }
    }

    /// Encode this item.
    ///
    /// The output is deterministic: definite lengths throughout,
    /// the shortest integer head that fits, and the narrowest of half,
    /// single, and double precision that reproduces a float exactly.
    /// Map entries keep the order they are stored in, so a value that
    /// came from ``init(decoding:)`` re-encodes to the bytes it was
    /// parsed from.
    public func encodedBytes() -> Data {
        var writer = CBORWriter()
        writer.write(self)
        return Data(writer.bytes)
    }
}

// MARK: - Codable

extension CBORValue: Codable {
    /// The single-key shape a foreign coder sees.
    ///
    /// ``CBOREncoder`` and ``CBORDecoder`` recognize ``CBORValue`` and
    /// pass its bytes through untouched, so these keys are reached only
    /// when some other coder, a JSON one for instance, is asked to
    /// carry a CBOR item.
    private enum CodingKeys: String, CodingKey {
        case unsigned
        case negative
        case byteString
        case textString
        case array
        case map
        case tag
        case simple
        case bool
        case float
    }

    /// The payload of the ``CodingKeys/tag`` key.
    private struct TaggedItem: Codable {
        var number: UInt64
        var item: CBORValue
    }

    /// The spellings ``null`` and ``undefined`` take in a foreign
    /// coder's single-value container.
    private enum UnitSpelling: String {
        case null
        case undefined
    }

    /// Read an item from a coder that is not this package's.
    public init(from decoder: any Decoder) throws {
        if let single = try? decoder.singleValueContainer(),
            let spelling = try? single.decode(String.self)
        {
            switch UnitSpelling(rawValue: spelling) {
            case .null:
                self = .null
                return
            case .undefined:
                self = .undefined
                return
            case nil:
                throw DecodingError.dataCorruptedError(
                    in: single,
                    debugDescription: "\(spelling) is not a CBOR unit spelling"
                )
            }
        }

        let container = try decoder.container(keyedBy: CodingKeys.self)
        guard let key = container.allKeys.first, container.allKeys.count == 1 else {
            throw DecodingError.dataCorrupted(
                DecodingError.Context(
                    codingPath: container.codingPath,
                    debugDescription: "a CBOR item is spelled as exactly one keyed case"
                )
            )
        }

        switch key {
        case .unsigned: self = .unsigned(try container.decode(UInt64.self, forKey: .unsigned))
        case .negative: self = .negative(try container.decode(UInt64.self, forKey: .negative))
        case .byteString:
            self = .byteString(try container.decode([UInt8].self, forKey: .byteString))
        case .textString: self = .textString(try container.decode(String.self, forKey: .textString))
        case .array: self = .array(try container.decode([CBORValue].self, forKey: .array))
        case .map: self = .map(try container.decode([Entry].self, forKey: .map))
        case .simple: self = Self.simpleValue(try container.decode(UInt8.self, forKey: .simple))
        case .bool: self = .bool(try container.decode(Bool.self, forKey: .bool))
        case .float: self = .float(try container.decode(Double.self, forKey: .float))
        case .tag:
            let tagged = try container.decode(TaggedItem.self, forKey: .tag)
            self = .tag(number: tagged.number, item: tagged.item)
        }
    }

    /// Write an item to a coder that is not this package's.
    public func encode(to encoder: any Encoder) throws {
        switch self {
        case .null:
            var single = encoder.singleValueContainer()
            try single.encode(UnitSpelling.null.rawValue)
        case .undefined:
            var single = encoder.singleValueContainer()
            try single.encode(UnitSpelling.undefined.rawValue)
        case .unsigned(let value):
            var container = encoder.container(keyedBy: CodingKeys.self)
            try container.encode(value, forKey: .unsigned)
        case .negative(let value):
            var container = encoder.container(keyedBy: CodingKeys.self)
            try container.encode(value, forKey: .negative)
        case .byteString(let value):
            var container = encoder.container(keyedBy: CodingKeys.self)
            try container.encode(value, forKey: .byteString)
        case .textString(let value):
            var container = encoder.container(keyedBy: CodingKeys.self)
            try container.encode(value, forKey: .textString)
        case .array(let value):
            var container = encoder.container(keyedBy: CodingKeys.self)
            try container.encode(value, forKey: .array)
        case .map(let value):
            var container = encoder.container(keyedBy: CodingKeys.self)
            try container.encode(value, forKey: .map)
        case .tag(let number, let item):
            var container = encoder.container(keyedBy: CodingKeys.self)
            try container.encode(TaggedItem(number: number, item: item), forKey: .tag)
        case .simple(let value):
            var container = encoder.container(keyedBy: CodingKeys.self)
            try container.encode(value, forKey: .simple)
        case .bool(let value):
            var container = encoder.container(keyedBy: CodingKeys.self)
            try container.encode(value, forKey: .bool)
        case .float(let value):
            var container = encoder.container(keyedBy: CodingKeys.self)
            try container.encode(value, forKey: .float)
        }
    }
}

extension CBORValue.Entry: Codable {
    private enum CodingKeys: String, CodingKey {
        case key
        case value
    }

    /// Read a pair from a coder that is not this package's.
    public init(from decoder: any Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        self.init(
            key: try container.decode(CBORValue.self, forKey: .key),
            value: try container.decode(CBORValue.self, forKey: .value)
        )
    }

    /// Write a pair to a coder that is not this package's.
    public func encode(to encoder: any Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(key, forKey: .key)
        try container.encode(value, forKey: .value)
    }
}

// MARK: - Map keys

/// A map key in the form the `Codable` containers address it by.
///
/// CBOR admits any item as a key, but `CodingKey` reaches only two
/// spellings: a name and an index. A key outside these two is carried
/// by ``CBORValue`` and skipped by the keyed containers, in the same
/// way an unrecognized name is skipped.
enum CBORMapKey: Hashable, Sendable {
    case text(String)
    case integer(Int64)

    /// The spelling a decoded key maps onto, or `nil` for a key that no
    /// `CodingKey` can name.
    init?(_ value: CBORValue) {
        switch value.untagged {
        case .textString(let text): self = .text(text)
        case .unsigned(let magnitude):
            guard let integer = Int64(exactly: magnitude) else { return nil }
            self = .integer(integer)
        case .negative(let magnitude):
            guard magnitude <= UInt64(Int64.max) else { return nil }
            self = .integer(Int64(bitPattern: ~magnitude))
        default: return nil
        }
    }

    /// The `CodingKey` of type `Key` this spelling names, if the type
    /// admits it.
    func codingKey<Key: CodingKey>(as type: Key.Type) -> Key? {
        switch self {
        case .text(let text): Key(stringValue: text)
        case .integer(let integer):
            Int(exactly: integer).flatMap { Key(intValue: $0) } ?? Key(stringValue: String(integer))
        }
    }
}

// MARK: - Half precision

/// Conversions between IEEE 754 binary16 and `Double`.
///
/// The half-precision format has no Swift type on every platform the
/// package supports, so both directions are spelled out over bit
/// patterns. That also keeps the encoder exact: ``bitPattern(of:)``
/// answers only for values binary16 represents without rounding, which
/// is the test the deterministic float width turns on.
enum CBORHalf {
    /// The `Double` a binary16 bit pattern denotes, exactly.
    static func double(fromBitPattern bits: UInt16) -> Double {
        let sign = UInt64(bits & 0x8000) << 48
        let exponent = Int((bits >> 10) & 0x1F)
        let mantissa = UInt64(bits & 0x03FF)

        if exponent == 0x1F {
            // Infinity or NaN: widen the payload in place.
            return Double(bitPattern: sign | 0x7FF0_0000_0000_0000 | (mantissa << 42))
        }
        if exponent == 0 {
            // Zero or subnormal: the value is mantissa x 2^-24.
            let magnitude = Double(mantissa) * 0x1p-24
            return Double(bitPattern: sign | magnitude.bitPattern)
        }
        let widened = UInt64(exponent - 15 + 1023) << 52
        return Double(bitPattern: sign | widened | (mantissa << 42))
    }

    /// The binary16 bit pattern for `value`, or `nil` when binary16
    /// cannot hold it exactly.
    static func bitPattern(of value: Double) -> UInt16? {
        let bits = value.bitPattern
        let sign = UInt16(truncatingIfNeeded: bits >> 48) & 0x8000
        let exponent = Int((bits >> 52) & 0x7FF)
        let mantissa = bits & 0x000F_FFFF_FFFF_FFFF

        let candidate: UInt16
        switch exponent {
        case 0x7FF where mantissa == 0:
            candidate = sign | 0x7C00
        case 0x7FF:
            // A NaN keeps its leading payload bits; forcing one bit on
            // stops the truncation from turning it into an infinity.
            let payload = UInt16(truncatingIfNeeded: mantissa >> 42) & 0x03FF
            candidate = sign | 0x7C00 | (payload == 0 ? 0x0200 : payload)
        case 0:
            // Zero, or a subnormal `Double` far below binary16's range.
            guard mantissa == 0 else { return nil }
            candidate = sign
        default:
            let unbiased = exponent - 1023
            if unbiased >= -14 && unbiased <= 15 {
                guard mantissa & 0x0000_03FF_FFFF_FFFF == 0 else { return nil }
                let shifted = UInt16(truncatingIfNeeded: mantissa >> 42) & 0x03FF
                candidate = sign | (UInt16(unbiased + 15) << 10) | shifted
            } else if unbiased >= -24 && unbiased < -14 {
                // A binary16 subnormal holds m x 2^-24 for m in 1...1023.
                let significand = mantissa | 0x0010_0000_0000_0000
                let shift = 28 - unbiased
                guard significand & ((1 << UInt64(shift)) - 1) == 0 else { return nil }
                candidate = sign | UInt16(truncatingIfNeeded: significand >> UInt64(shift))
            } else {
                return nil
            }
        }

        guard double(fromBitPattern: candidate).bitPattern == bits else { return nil }
        return candidate
    }
}
