import Foundation
import Testing

@testable import PanprotoStructural

// MARK: - RFC 8949 test vectors

/// One row of RFC 8949's test vector appendix.
private struct Vector: Sendable, CustomStringConvertible {
    /// The bytes, as the appendix spells them.
    let hex: String
    /// The item those bytes denote.
    let value: CBORValue
    /// The bytes the item encodes to, which differ from ``hex`` for the
    /// rows the appendix writes in a non-deterministic form.
    let canonical: String

    init(_ hex: String, _ value: CBORValue, canonical: String? = nil) {
        self.hex = hex
        self.value = value
        self.canonical = canonical ?? hex
    }

    var description: String { hex }
}

/// Rows whose bytes are already in the deterministic form, so that
/// decoding and encoding are inverse.
private let roundTripVectors: [Vector] = [
    // Unsigned integers.
    Vector("00", .unsigned(0)),
    Vector("01", .unsigned(1)),
    Vector("0a", .unsigned(10)),
    Vector("17", .unsigned(23)),
    Vector("1818", .unsigned(24)),
    Vector("1819", .unsigned(25)),
    Vector("1864", .unsigned(100)),
    Vector("1903e8", .unsigned(1000)),
    Vector("1a000f4240", .unsigned(1_000_000)),
    Vector("1b000000e8d4a51000", .unsigned(1_000_000_000_000)),
    Vector("1bffffffffffffffff", .unsigned(18_446_744_073_709_551_615)),
    // Negative integers, which the wire stores as -1 - n.
    Vector("20", .negative(0)),
    Vector("29", .negative(9)),
    Vector("3863", .negative(99)),
    Vector("3903e7", .negative(999)),
    Vector("3bffffffffffffffff", .negative(18_446_744_073_709_551_615)),
    // Bignums.
    Vector(
        "c249010000000000000000",
        .tag(number: 2, item: .byteString([1, 0, 0, 0, 0, 0, 0, 0, 0]))
    ),
    Vector(
        "c349010000000000000000",
        .tag(number: 3, item: .byteString([1, 0, 0, 0, 0, 0, 0, 0, 0]))
    ),
    // Floats.
    Vector("f90000", .float(0.0)),
    Vector("f98000", .float(-0.0)),
    Vector("f93c00", .float(1.0)),
    Vector("fb3ff199999999999a", .float(1.1)),
    Vector("f93e00", .float(1.5)),
    Vector("f97bff", .float(65504.0)),
    Vector("fa47c35000", .float(100_000.0)),
    Vector("fa7f7fffff", .float(Double(Float.greatestFiniteMagnitude))),
    Vector("fb7e37e43c8800759c", .float(1.0e+300)),
    Vector("f90001", .float(0x1p-24)),
    Vector("f90400", .float(0x1p-14)),
    Vector("f9c400", .float(-4.0)),
    Vector("fbc010666666666666", .float(-4.1)),
    Vector("f97c00", .float(.infinity)),
    Vector("f97e00", .float(.nan)),
    Vector("f9fc00", .float(-.infinity)),
    // Simple values.
    Vector("f4", .bool(false)),
    Vector("f5", .bool(true)),
    Vector("f6", .null),
    Vector("f7", .undefined),
    Vector("f0", .simple(16)),
    Vector("f8ff", .simple(255)),
    // Tags.
    Vector(
        "c074323031332d30332d32315432303a30343a30305a",
        .tag(number: 0, item: .textString("2013-03-21T20:04:00Z"))
    ),
    Vector("c11a514b67b0", .tag(number: 1, item: .unsigned(1_363_896_240))),
    Vector("d74401020304", .tag(number: 23, item: .byteString([1, 2, 3, 4]))),
    Vector(
        "d82076687474703a2f2f7777772e6578616d706c652e636f6d",
        .tag(number: 32, item: .textString("http://www.example.com"))
    ),
    // Strings.
    Vector("40", .byteString([])),
    Vector("4401020304", .byteString([1, 2, 3, 4])),
    Vector("60", .textString("")),
    Vector("6161", .textString("a")),
    Vector("6449455446", .textString("IETF")),
    Vector("62225c", .textString("\"\\")),
    Vector("62c3bc", .textString("\u{00fc}")),
    Vector("63e6b0b4", .textString("\u{6c34}")),
    Vector("64f0908591", .textString("\u{10151}")),
    // Arrays and maps.
    Vector("80", .array([])),
    Vector("83010203", .array([.unsigned(1), .unsigned(2), .unsigned(3)])),
    Vector(
        "8301820203820405",
        .array([
            .unsigned(1),
            .array([.unsigned(2), .unsigned(3)]),
            .array([.unsigned(4), .unsigned(5)]),
        ])
    ),
    Vector(
        "98190102030405060708090a0b0c0d0e0f101112131415161718181819",
        .array((1...25).map { .unsigned(UInt64($0)) })
    ),
    Vector("a0", .map([])),
    Vector(
        "a201020304",
        .map([
            .init(key: .unsigned(1), value: .unsigned(2)),
            .init(key: .unsigned(3), value: .unsigned(4)),
        ])
    ),
    Vector(
        "a26161016162820203",
        .textMap([("a", .unsigned(1)), ("b", .array([.unsigned(2), .unsigned(3)]))])
    ),
    Vector("826161a161626163", .array([.textString("a"), .textMap([("b", .textString("c"))])])),
    Vector(
        "a56161614161626142616361436164614461656145",
        .textMap([
            ("a", .textString("A")),
            ("b", .textString("B")),
            ("c", .textString("C")),
            ("d", .textString("D")),
            ("e", .textString("E")),
        ])
    ),
]

/// Rows the appendix writes in a form the deterministic encoder does
/// not reproduce: indefinite lengths, and floats and integers in a
/// wider head than they need.
private let decodeOnlyVectors: [Vector] = [
    Vector("5f42010243030405ff", .byteString([1, 2, 3, 4, 5]), canonical: "450102030405"),
    Vector(
        "7f657374726561646d696e67ff",
        .textString("streaming"),
        canonical: "6973747265616d696e67"
    ),
    Vector("9fff", .array([]), canonical: "80"),
    Vector(
        "9f018202039f0405ffff",
        .array([
            .unsigned(1),
            .array([.unsigned(2), .unsigned(3)]),
            .array([.unsigned(4), .unsigned(5)]),
        ]),
        canonical: "8301820203820405"
    ),
    Vector(
        "83019f0203ff820405",
        .array([
            .unsigned(1),
            .array([.unsigned(2), .unsigned(3)]),
            .array([.unsigned(4), .unsigned(5)]),
        ]),
        canonical: "8301820203820405"
    ),
    Vector(
        "9f0102030405060708090a0b0c0d0e0f101112131415161718181819ff",
        .array((1...25).map { .unsigned(UInt64($0)) }),
        canonical: "98190102030405060708090a0b0c0d0e0f101112131415161718181819"
    ),
    Vector(
        "bf61610161629f0203ffff",
        .textMap([("a", .unsigned(1)), ("b", .array([.unsigned(2), .unsigned(3)]))]),
        canonical: "a26161016162820203"
    ),
    Vector(
        "826161bf61626163ff",
        .array([.textString("a"), .textMap([("b", .textString("c"))])]),
        canonical: "826161a161626163"
    ),
    Vector(
        "bf6346756ef563416d7421ff",
        .textMap([("Fun", .bool(true)), ("Amt", .negative(1))]),
        canonical: "a26346756ef563416d7421"
    ),
    Vector("fa7f800000", .float(.infinity), canonical: "f97c00"),
    Vector("fa7fc00000", .float(.nan), canonical: "f97e00"),
    Vector("faff800000", .float(-.infinity), canonical: "f9fc00"),
    Vector("fb7ff0000000000000", .float(.infinity), canonical: "f97c00"),
    Vector("fb7ff8000000000000", .float(.nan), canonical: "f97e00"),
    Vector("fbfff0000000000000", .float(-.infinity), canonical: "f9fc00"),
    Vector("1900ff", .unsigned(255), canonical: "18ff"),
    Vector("1b000000000000ffff", .unsigned(65535), canonical: "19ffff"),
    Vector("f818", .simple(24), canonical: "f818"),
]

@Suite("RFC 8949 vectors")
struct RFCVectorTests {
    @Test("bytes decode to the item the appendix names", arguments: roundTripVectors)
    fileprivate func decodes(_ vector: Vector) throws {
        #expect(try CBORValue(decoding: bytes(vector.hex)) == vector.value)
    }

    @Test("the item encodes back to the bytes it came from", arguments: roundTripVectors)
    fileprivate func encodes(_ vector: Vector) {
        #expect(hex(vector.value.encodedBytes()) == vector.canonical)
    }

    @Test("non-deterministic spellings decode", arguments: decodeOnlyVectors)
    fileprivate func decodesTolerantly(_ vector: Vector) throws {
        #expect(try CBORValue(decoding: bytes(vector.hex)) == vector.value)
    }

    @Test("non-deterministic spellings re-encode deterministically", arguments: decodeOnlyVectors)
    fileprivate func reEncodesDeterministically(_ vector: Vector) throws {
        let decoded = try CBORValue(decoding: bytes(vector.hex))
        #expect(hex(decoded.encodedBytes()) == vector.canonical)
    }
}

// MARK: - Model types

/// A struct with a field of every shape the engine writes.
private struct Outer: Codable, Equatable {
    var name: String
    var count: UInt64
    var maybe: String?
    var nothing: String?
    var inner: Inner
    var items: [Int32]
    var pair: Pair
    var raw: [UInt8]

    private enum CodingKeys: String, CodingKey {
        case name, count, maybe, nothing, inner, items, pair, raw
    }

    init(
        name: String,
        count: UInt64,
        maybe: String?,
        nothing: String?,
        inner: Inner,
        items: [Int32],
        pair: Pair,
        raw: [UInt8]
    ) {
        self.name = name
        self.count = count
        self.maybe = maybe
        self.nothing = nothing
        self.inner = inner
        self.items = items
        self.pair = pair
        self.raw = raw
    }

    init(from decoder: any Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        name = try container.decode(String.self, forKey: .name)
        count = try container.decode(UInt64.self, forKey: .count)
        maybe = try container.decodeIfPresent(String.self, forKey: .maybe)
        nothing = try container.decodeIfPresent(String.self, forKey: .nothing)
        inner = try container.decode(Inner.self, forKey: .inner)
        items = try container.decode([Int32].self, forKey: .items)
        pair = try container.decode(Pair.self, forKey: .pair)
        raw = try container.decode([UInt8].self, forKey: .raw)
    }

    /// Write the optional fields as CBOR null rather than leaving them
    /// out, which is the spelling `Option` takes on this wire.
    func encode(to encoder: any Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(name, forKey: .name)
        try container.encode(count, forKey: .count)
        try container.encode(maybe, forKey: .maybe)
        try container.encode(nothing, forKey: .nothing)
        try container.encode(inner, forKey: .inner)
        try container.encode(items, forKey: .items)
        try container.encode(pair, forKey: .pair)
        try container.encode(raw, forKey: .raw)
    }
}

private struct Inner: Codable, Equatable {
    var flag: Bool
    var ratio: Double
}

/// A Rust tuple, which reaches the wire as an array.
private struct Pair: Codable, Equatable {
    var first: UInt8
    var second: String

    init(first: UInt8, second: String) {
        self.first = first
        self.second = second
    }

    init(from decoder: any Decoder) throws {
        var container = try decoder.unkeyedContainer()
        first = try container.decode(UInt8.self)
        second = try container.decode(String.self)
    }

    func encode(to encoder: any Encoder) throws {
        var container = encoder.unkeyedContainer()
        try container.encode(first)
        try container.encode(second)
    }
}

/// A key naming an enum variant.
private struct VariantKey: CodingKey {
    var stringValue: String
    var intValue: Int? { nil }

    init(_ name: String) { stringValue = name }
    init?(stringValue: String) { self.stringValue = stringValue }
    init?(intValue: Int) { nil }
}

/// An externally tagged enum, in each of the four variant shapes
/// `serde` writes.
private enum Shape: Codable, Equatable {
    case point
    case radius(Double)
    case rect(UInt32, UInt32)
    case named(id: String, size: Int64)

    private enum FieldKey: String, CodingKey {
        case id, size
    }

    init(from decoder: any Decoder) throws {
        if let single = try? decoder.singleValueContainer(), !single.decodeNil(),
            let name = try? single.decode(String.self)
        {
            guard name == "Point" else {
                throw DecodingError.dataCorruptedError(
                    in: single,
                    debugDescription: "\(name) is not a unit variant of Shape"
                )
            }
            self = .point
            return
        }
        let container = try decoder.container(keyedBy: VariantKey.self)
        guard let key = container.allKeys.first else {
            throw DecodingError.dataCorrupted(
                DecodingError.Context(
                    codingPath: container.codingPath,
                    debugDescription: "an externally tagged enum has one entry"
                )
            )
        }
        switch key.stringValue {
        case "Radius":
            self = .radius(try container.decode(Double.self, forKey: key))
        case "Rect":
            var payload = try container.nestedUnkeyedContainer(forKey: key)
            self = .rect(try payload.decode(UInt32.self), try payload.decode(UInt32.self))
        case "Named":
            let payload = try container.nestedContainer(keyedBy: FieldKey.self, forKey: key)
            self = .named(
                id: try payload.decode(String.self, forKey: .id),
                size: try payload.decode(Int64.self, forKey: .size)
            )
        default:
            throw DecodingError.dataCorrupted(
                DecodingError.Context(
                    codingPath: container.codingPath,
                    debugDescription: "\(key.stringValue) is not a variant of Shape"
                )
            )
        }
    }

    func encode(to encoder: any Encoder) throws {
        switch self {
        case .point:
            var single = encoder.singleValueContainer()
            try single.encode("Point")
        case .radius(let radius):
            var container = encoder.container(keyedBy: VariantKey.self)
            try container.encode(radius, forKey: VariantKey("Radius"))
        case .rect(let width, let height):
            var container = encoder.container(keyedBy: VariantKey.self)
            var payload = container.nestedUnkeyedContainer(forKey: VariantKey("Rect"))
            try payload.encode(width)
            try payload.encode(height)
        case .named(let id, let size):
            var container = encoder.container(keyedBy: VariantKey.self)
            var payload = container.nestedContainer(
                keyedBy: FieldKey.self,
                forKey: VariantKey("Named")
            )
            try payload.encode(id, forKey: .id)
            try payload.encode(size, forKey: .size)
        }
    }
}

/// A struct that reserves a slot with `superEncoder()`.
private struct WithSuper: Codable, Equatable {
    var top: Int
    var nested: Int

    private enum CodingKeys: String, CodingKey {
        case top, nested
    }

    init(top: Int, nested: Int) {
        self.top = top
        self.nested = nested
    }

    init(from decoder: any Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        top = try container.decode(Int.self, forKey: .top)
        let parent = try container.superDecoder().container(keyedBy: CodingKeys.self)
        nested = try parent.decode(Int.self, forKey: .nested)
    }

    func encode(to encoder: any Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(top, forKey: .top)
        var parent = container.superEncoder().container(keyedBy: CodingKeys.self)
        try parent.encode(nested, forKey: .nested)
    }
}

/// A struct carrying a fragment the Swift model does not describe.
private struct Envelope: Codable, Equatable {
    var kind: String
    var payload: CBORValue
}

/// A struct whose bytes are a `Data` field.
private struct Blob: Codable, Equatable {
    var body: Data
}

/// A struct with one field, for the tolerance tests.
private struct OneField: Codable, Equatable {
    var a: Int
}

// MARK: - Interoperation with ciborium

@Suite("ciborium interoperation")
struct CiboriumTests {
    /// The bytes `ciborium` writes for the Rust twin of ``Outer``.
    static let outerHex = """
        a8646e616d65616e65636f756e741bffffffffffffffff656d617962656179676e6f7468696e67f6\
        65696e6e6572a264666c6167f565726174696ff93e00656974656d738320000164706169728207617a\
        6372617783010203
        """

    fileprivate static let outer = Outer(
        name: "n",
        count: .max,
        maybe: "y",
        nothing: nil,
        inner: Inner(flag: true, ratio: 1.5),
        items: [-1, 0, 1],
        pair: Pair(first: 7, second: "z"),
        raw: [1, 2, 3]
    )

    @Test("a struct encodes to the bytes ciborium writes")
    func structEncodes() throws {
        #expect(try encodedHex(Self.outer) == Self.outerHex)
    }

    @Test("the bytes ciborium writes decode to the struct")
    func structDecodes() throws {
        #expect(try CBORDecoder().decode(Outer.self, from: bytes(Self.outerHex)) == Self.outer)
    }

    @Test("a unit variant is a text string")
    func unitVariant() throws {
        #expect(try encodedHex(Shape.point) == "65506f696e74")
        #expect(try CBORDecoder().decode(Shape.self, from: bytes("65506f696e74")) == .point)
    }

    @Test("a newtype variant is a one-entry map")
    func newtypeVariant() throws {
        let encoded = "a166526164697573f93c00"
        #expect(try encodedHex(Shape.radius(1.0)) == encoded)
        #expect(try CBORDecoder().decode(Shape.self, from: bytes(encoded)) == .radius(1.0))
    }

    @Test("a tuple variant is a one-entry map holding an array")
    func tupleVariant() throws {
        let encoded = "a16452656374820102"
        #expect(try encodedHex(Shape.rect(1, 2)) == encoded)
        #expect(try CBORDecoder().decode(Shape.self, from: bytes(encoded)) == .rect(1, 2))
    }

    @Test("a struct variant is a one-entry map holding a map")
    func structVariant() throws {
        let encoded = "a1654e616d6564a262696461616473697a6522"
        let value = Shape.named(id: "a", size: -3)
        #expect(try encodedHex(value) == encoded)
        #expect(try CBORDecoder().decode(Shape.self, from: bytes(encoded)) == value)
    }

    @Test("an omitted optional field decodes as absent, as serde reads it")
    func omittedOptionalField() throws {
        // The same payload as `outerHex` with the null-valued entry left
        // out entirely, which is what a synthesized Swift encoding writes.
        let payload = CBORValue.textMap([
            ("name", .textString("n")),
            ("count", .unsigned(.max)),
            ("maybe", .textString("y")),
            ("inner", .textMap([("flag", .bool(true)), ("ratio", .float(1.5))])),
            ("items", .array([.negative(0), .unsigned(0), .unsigned(1)])),
            ("pair", .array([.unsigned(7), .textString("z")])),
            ("raw", .array([.unsigned(1), .unsigned(2), .unsigned(3)])),
        ])
        #expect(try CBORDecoder().decode(Outer.self, from: payload) == Self.outer)
    }

    @Test("the error envelope the C ABI leaves behind round-trips")
    func errorEnvelope() throws {
        let encoded = """
            a36673746174757303637461676e696e76616c69645f68616e646c65676d657373616765\
            7468616e646c652037206973206e6f74206c697665
            """
        let envelope = ErrorEnvelope(
            status: 3,
            tag: "invalid_handle",
            message: "handle 7 is not live"
        )
        #expect(try CBORDecoder().decode(ErrorEnvelope.self, from: bytes(encoded)) == envelope)
        #expect(try encodedHex(envelope) == encoded)
    }

    @Test("a byte string decodes into a field typed as a byte array")
    func byteStringAsSequence() throws {
        #expect(try CBORDecoder().decode([UInt8].self, from: bytes("4401020304")) == [1, 2, 3, 4])
    }

    @Test("a Data field is a byte string, and reads back from an array")
    func dataIsAByteString() throws {
        let blob = Blob(body: Data([1, 2, 3]))
        #expect(try encodedHex(blob) == "a164626f647943010203")
        #expect(try CBORDecoder().decode(Blob.self, from: bytes("a164626f647943010203")) == blob)
        #expect(try CBORDecoder().decode(Blob.self, from: bytes("a164626f647983010203")) == blob)
    }
}

// MARK: - Tolerant decoding

@Suite("tolerant decoding")
struct ToleranceTests {
    @Test("an unrecognized key is skipped")
    func unknownKeys() throws {
        let payload = bytes("a2616101627a7a02")
        #expect(try CBORDecoder().decode(OneField.self, from: payload) == OneField(a: 1))
    }

    @Test("an indefinite-length map decodes")
    func indefiniteMap() throws {
        let payload = bytes("bf616101ff")
        #expect(try CBORDecoder().decode(OneField.self, from: payload) == OneField(a: 1))
    }

    @Test("an indefinite-length array decodes into an array field")
    func indefiniteArray() throws {
        #expect(try CBORDecoder().decode([Int].self, from: bytes("9f010203ff")) == [1, 2, 3])
    }

    @Test("an indefinite-length text string decodes")
    func indefiniteText() throws {
        let payload = bytes("7f657374726561646d696e67ff")
        let decoded = try CBORDecoder().decode(String.self, from: payload)
        #expect(decoded == "streaming")
    }

    @Test("an indefinite-length byte string decodes")
    func indefiniteBytes() throws {
        let decoded = try CBORDecoder().decode(Data.self, from: bytes("5f42010243030405ff"))
        #expect(decoded == Data([1, 2, 3, 4, 5]))
    }

    @Test("a tag around a value is read through")
    func tagAroundValue() throws {
        let payload = bytes("a16161c101")
        #expect(try CBORDecoder().decode(OneField.self, from: payload) == OneField(a: 1))
    }

    @Test("a tag around a map is read through")
    func tagAroundMap() throws {
        let payload = bytes("c1a1616101")
        #expect(try CBORDecoder().decode(OneField.self, from: payload) == OneField(a: 1))
    }

    @Test("nested tags are read through")
    func nestedTags() throws {
        let payload = bytes("d9d9f7c1a16161d81e01")
        #expect(try CBORDecoder().decode(OneField.self, from: payload) == OneField(a: 1))
    }

    @Test("a bignum decodes as the integer it holds")
    func bignum() throws {
        let unsigned = bytes("c248ffffffffffffffff")
        #expect(try CBORDecoder().decode(UInt64.self, from: unsigned) == UInt64.max)
        let negative = bytes("c3480000000000000009")
        #expect(try CBORDecoder().decode(Int.self, from: negative) == -10)
    }

    @Test("undefined reads as an absent optional, as serde reads it")
    func undefinedIsNone() throws {
        struct Holder: Codable, Equatable {
            var a: String?
        }
        #expect(try CBORDecoder().decode(Holder.self, from: bytes("a16161f7")) == Holder(a: nil))
        #expect(try CBORDecoder().decode(Holder.self, from: bytes("a16161f6")) == Holder(a: nil))
    }

    @Test("an unsigned integer above Int64.max survives")
    func unsignedAboveInt64Max() throws {
        let payload = bytes("1bffffffffffffffff")
        #expect(try CBORDecoder().decode(UInt64.self, from: payload) == 18_446_744_073_709_551_615)
        #expect(try encodedHex(UInt64.max) == "1bffffffffffffffff")
        #expect(throws: DecodingError.self) {
            try CBORDecoder().decode(Int64.self, from: payload)
        }
    }

    @Test("the extremes of the integer range survive")
    func integerExtremes() throws {
        #expect(try encodedHex(Int64.min) == "3b7fffffffffffffff")
        #expect(try CBORDecoder().decode(Int64.self, from: bytes("3b7fffffffffffffff")) == .min)
        #expect(try encodedHex(Int8(-1)) == "20")
        #expect(try CBORDecoder().decode(Int8.self, from: bytes("20")) == -1)
        #expect(throws: DecodingError.self) {
            try CBORDecoder().decode(Int8.self, from: bytes("1901f4"))
        }
        #expect(throws: DecodingError.self) {
            try CBORDecoder().decode(UInt8.self, from: bytes("20"))
        }
        // -2^64 fits no Swift integer type, and says so rather than wrapping.
        #expect(throws: DecodingError.self) {
            try CBORDecoder().decode(Int64.self, from: bytes("3bffffffffffffffff"))
        }
    }

    @Test("an integer where a float belongs is refused, as ciborium refuses it")
    func integerIsNotAFloat() {
        #expect(throws: DecodingError.self) {
            try CBORDecoder().decode(Double.self, from: bytes("01"))
        }
    }

    @Test("a simple value where a boolean belongs is refused")
    func simpleValueIsNotABoolean() {
        #expect(throws: DecodingError.self) {
            try CBORDecoder().decode(Bool.self, from: bytes("f0"))
        }
    }
}

// MARK: - Floats

@Suite("float widths")
struct FloatTests {
    @Test("a float takes the narrowest width that holds it")
    func widths() throws {
        #expect(try encodedHex(Double(1.0)) == "f93c00")
        #expect(try encodedHex(Double(1.5)) == "f93e00")
        #expect(try encodedHex(Double(65504.0)) == "f97bff")
        #expect(try encodedHex(Double(100_000.0)) == "fa47c35000")
        #expect(try encodedHex(Double(0.1)) == "fb3fb999999999999a")
        #expect(try encodedHex(Float(0.1)) == "fa3dcccccd")
        #expect(try encodedHex(Double(0x1p-24)) == "f90001")
        #expect(try encodedHex(-0.0) == "f98000")
        #expect(try encodedHex(Double.infinity) == "f97c00")
        #expect(try encodedHex(Double.nan) == "f97e00")
    }

    @Test("all three widths decode to the same value")
    func widthsDecode() throws {
        let decoder = CBORDecoder()
        #expect(try decoder.decode(Double.self, from: bytes("f93c00")) == 1.0)
        #expect(try decoder.decode(Double.self, from: bytes("fa3f800000")) == 1.0)
        #expect(try decoder.decode(Double.self, from: bytes("fb3ff0000000000000")) == 1.0)
        #expect(try decoder.decode(Float.self, from: bytes("f93c00")) == 1.0)
    }

    @Test("half-precision subnormals decode exactly")
    func halfSubnormals() throws {
        let decoder = CBORDecoder()
        #expect(try decoder.decode(Double.self, from: bytes("f90001")) == 0x1p-24)
        #expect(try decoder.decode(Double.self, from: bytes("f903ff")) == 1023.0 * 0x1p-24)
        #expect(try decoder.decode(Double.self, from: bytes("f90400")) == 0x1p-14)
        #expect(try decoder.decode(Double.self, from: bytes("f98001")) == -0x1p-24)
    }

    @Test("negative zero keeps its sign through a round trip")
    func negativeZero() throws {
        let decoded = try CBORDecoder().decode(Double.self, from: bytes("f98000"))
        #expect(decoded.sign == .minus)
        #expect(try encodedHex(decoded) == "f98000")
    }

    @Test("a double that no narrower width holds keeps all 64 bits")
    func doublePrecision() throws {
        let value = 1.0e+300
        #expect(try encodedHex(value) == "fb7e37e43c8800759c")
        #expect(try CBORDecoder().decode(Double.self, from: bytes("fb7e37e43c8800759c")) == value)
    }

    @Test("a double too large for Float is refused rather than rounded to infinity")
    func floatOverflow() {
        #expect(throws: DecodingError.self) {
            try CBORDecoder().decode(Float.self, from: bytes("fb7e37e43c8800759c"))
        }
    }
}

// MARK: - Deterministic encoding

@Suite("deterministic encoding")
struct DeterminismTests {
    @Test("dictionary keys are sorted by their encoded bytes")
    func canonicalMapOrder() throws {
        // The head carries the length, so "b" and "c" sort before "aa".
        #expect(try encodedHex(["aa": 1, "b": 2, "c": 3]) == "a361620261630362616101")
    }

    @Test("an integer-keyed dictionary is keyed by integers")
    func integerKeys() throws {
        #expect(try encodedHex([2: "b", 10: "c", 1: "a"]) == "a30161610261620a6163")
        let payload = bytes("a30161610261620a6163")
        let decoded = try CBORDecoder().decode([Int: String].self, from: payload)
        #expect(decoded == [1: "a", 2: "b", 10: "c"])
    }

    @Test("a string-keyed dictionary is keyed by text even when the keys read as numbers")
    func numericStringKeys() throws {
        #expect(try encodedHex(["1": "a", "2": "b"]) == "a26131616161326162")
        let payload = bytes("a26131616161326162")
        let decoded = try CBORDecoder().decode([String: String].self, from: payload)
        #expect(decoded == ["1": "a", "2": "b"])
    }

    @Test("a set is sorted by the encoded bytes of its elements")
    func canonicalSetOrder() throws {
        #expect(try encodedHex(Set([3, 1, 2])) == "83010203")
    }

    @Test("a dictionary that reaches an array of pairs is sorted by pair")
    func canonicalPairOrder() throws {
        // A key that is neither a string nor an integer takes the
        // alternating array form, which is still sorted by key.
        #expect(try encodedHex([2.5: "b", 1.5: "a"]) == "84f93e006161f941006162")
        let decoded = try CBORDecoder().decode(
            [Double: String].self,
            from: bytes("84f93e006161f941006162")
        )
        #expect(decoded == [1.5: "a", 2.5: "b"])
    }

    @Test("two encodes of the same value agree byte for byte")
    func repeatable() throws {
        let value = [
            "gamma": [3, 1, 2],
            "alpha": [1],
            "beta": [2, 2],
        ]
        let encoder = CBOREncoder()
        let first = try encoder.encode(value)
        for _ in 0..<32 {
            #expect(try encoder.encode(value) == first)
        }
    }

    @Test("struct keys keep declaration order rather than sorting")
    func declarationOrder() throws {
        // `zebra` is declared first and stays first, though `a` sorts before it.
        struct Ordered: Codable {
            var zebra: Int
            var a: Int
        }
        #expect(try encodedHex(Ordered(zebra: 1, a: 2)) == "a2657a6562726101616102")
    }

    @Test("every integer takes the shortest head that holds it")
    func shortestIntegerHead() throws {
        #expect(try encodedHex(23) == "17")
        #expect(try encodedHex(24) == "1818")
        #expect(try encodedHex(255) == "18ff")
        #expect(try encodedHex(256) == "190100")
        #expect(try encodedHex(65535) == "19ffff")
        #expect(try encodedHex(65536) == "1a00010000")
        #expect(try encodedHex(4_294_967_295) == "1affffffff")
        #expect(try encodedHex(4_294_967_296) == "1b0000000100000000")
    }
}

// MARK: - Container mechanics

@Suite("container mechanics")
struct ContainerTests {
    @Test("a reserved super slot round-trips")
    func superContainers() throws {
        let value = WithSuper(top: 1, nested: 2)
        #expect(try encodedHex(value) == "a263746f7001657375706572a1666e657374656402")
        let decoded = try CBORDecoder().decode(
            WithSuper.self,
            from: bytes("a263746f7001657375706572a1666e657374656402")
        )
        #expect(decoded == value)
    }

    @Test("an unkeyed container nests keyed and unkeyed children")
    func nestedContainers() throws {
        struct Nest: Codable, Equatable {
            var rows: [[Int]]
            var table: [String: [Int]]
        }
        let value = Nest(rows: [[1, 2], [3]], table: ["a": [1], "b": [2]])
        let encoded = try CBOREncoder().encode(value)
        #expect(try CBORDecoder().decode(Nest.self, from: encoded) == value)
        #expect(hex(encoded) == "a264726f7773828201028103657461626c65a26161810161628102")
    }

    @Test("a nil written into a single-value container reads back as nil")
    func encodeAndDecodeNil() throws {
        let encoded = try CBOREncoder().encode(String?.none)
        #expect(hex(encoded) == "f6")
        #expect(try CBORDecoder().decode(String?.self, from: encoded) == nil)
        #expect(try CBORDecoder().decode(String?.self, from: bytes("6161")) == "a")
    }

    @Test("an array of optionals keeps its holes")
    func optionalsInAnArray() throws {
        let value: [Int?] = [1, nil, 3]
        #expect(try encodedHex(value) == "8301f603")
        #expect(try CBORDecoder().decode([Int?].self, from: bytes("8301f603")) == value)
    }

    @Test("an empty struct is an empty map")
    func emptyContainers() throws {
        struct Empty: Codable, Equatable {}
        #expect(try encodedHex(Empty()) == "a0")
        #expect(try encodedHex([Int]()) == "80")
        #expect(try encodedHex([String: Int]()) == "a0")
    }

    @Test("a value that writes nothing at all is refused")
    func nothingEncoded() {
        struct Silent: Encodable {
            func encode(to encoder: any Encoder) throws {}
        }
        #expect(throws: EncodingError.self) {
            try CBOREncoder().encode(Silent())
        }
    }

    @Test("user info reaches the value being encoded and decoded")
    func userInfo() throws {
        let key = CodingUserInfoKey(rawValue: "cbor.tests.marker")
        struct Marked: Codable, Equatable {
            var seen: String

            init(seen: String) { self.seen = seen }

            init(from decoder: any Decoder) throws {
                let container = try decoder.singleValueContainer()
                _ = try container.decode(String.self)
                seen = decoder.userInfo.keys.first?.rawValue ?? "none"
            }

            func encode(to encoder: any Encoder) throws {
                var container = encoder.singleValueContainer()
                try container.encode(encoder.userInfo.keys.first?.rawValue ?? "none")
            }
        }
        var encoder = CBOREncoder()
        encoder.userInfo = key.map { [$0: "set"] } ?? [:]
        let encoded = try encoder.encode(Marked(seen: ""))
        #expect(try CBORValue(decoding: encoded).stringValue == "cbor.tests.marker")
        var decoder = CBORDecoder()
        decoder.userInfo = key.map { [$0: "set"] } ?? [:]
        #expect(try decoder.decode(Marked.self, from: encoded).seen == "cbor.tests.marker")
    }
}

// MARK: - CBORValue

@Suite("CBORValue")
struct CBORValueTests {
    @Test("a CBORValue field carries a fragment through unchanged")
    func passthrough() throws {
        let payload = "a2646b696e64646c6f7564677061796c6f6164c1a2616101627a7a9f0102ff"
        let decoded = try CBORDecoder().decode(Envelope.self, from: bytes(payload))
        #expect(decoded.kind == "loud")
        #expect(decoded.payload == .tag(number: 1, item: decoded.payload.untagged))
        #expect(decoded.payload["a"] == .unsigned(1))
        #expect(decoded.payload["zz"] == .array([.unsigned(1), .unsigned(2)]))
        // The fragment re-encodes deterministically: the tag and the key
        // order survive, and the indefinite-length array closes.
        let reEncoded = "a2646b696e64646c6f7564677061796c6f6164c1a2616101627a7a820102"
        #expect(try encodedHex(decoded) == reEncoded)
    }

    @Test("an item can be inspected without a static type")
    func inspection() throws {
        let value = try CBORValue(decoding: bytes("a26161016162820203"))
        #expect(value["a"]?.uint64Value == 1)
        #expect(value["b"]?[1] == .unsigned(3))
        #expect(value["missing"] == nil)
        #expect(value.mapValue?.count == 2)
        #expect(try CBORValue(decoding: bytes("20")).int64Value == -1)
        #expect(try CBORValue(decoding: bytes("1bffffffffffffffff")).int64Value == nil)
        #expect(try CBORValue(decoding: bytes("1bffffffffffffffff")).uint64Value == .max)
        #expect(try CBORValue(decoding: bytes("f5")).boolValue == true)
        #expect(try CBORValue(decoding: bytes("f6")).isNull)
        #expect(try CBORValue(decoding: bytes("f7")).isUndefined)
        #expect(try CBORValue(decoding: bytes("6161")).stringValue == "a")
        #expect(try CBORValue(decoding: bytes("4101")).byteStringValue == [1])
        #expect(try CBORValue(decoding: bytes("f93c00")).doubleValue == 1.0)
    }

    @Test("equality follows the encoded form")
    func equality() {
        #expect(CBORValue.simple(21) == .bool(true))
        #expect(CBORValue.simple(22) == .null)
        #expect(CBORValue.float(.nan) == .float(.nan))
        #expect(CBORValue.float(0.0) != .float(-0.0))
        #expect(CBORValue.unsigned(0) != .negative(0))
        #expect(Set([CBORValue.simple(20), .bool(false)]).count == 1)
    }

    @Test("the four named simple values build as their own cases")
    func namedSimpleValues() {
        #expect(CBORValue.simpleValue(20) == .bool(false))
        #expect(CBORValue.simpleValue(21) == .bool(true))
        #expect(CBORValue.simpleValue(22) == .null)
        #expect(CBORValue.simpleValue(23) == .undefined)
    }

    @Test("an unnamed simple value stays simple and encodes to its byte")
    func unnamedSimpleValues() throws {
        #expect(CBORValue.simpleValue(0) == .simple(0))
        #expect(CBORValue.simpleValue(200) == .simple(200))
        #expect(hex(CBORValue.simpleValue(16).encodedBytes()) == "f0")
        #expect(hex(CBORValue.simpleValue(22).encodedBytes()) == "f6")
        #expect(try CBORValue(decoding: bytes("f8ff")) == .simpleValue(255))
    }

    @Test("an item survives a coder that is not this one")
    func foreignCoder() throws {
        struct Holder: Codable, Equatable {
            var item: CBORValue
        }
        let holder = Holder(
            item: .array([
                .unsigned(1),
                .negative(2),
                .textString("x"),
                .byteString([9]),
                .bool(true),
                .null,
                .undefined,
                .simple(200),
                .float(1.5),
                .tag(number: 7, item: .textMap([("k", .unsigned(3))])),
            ])
        )
        let json = try JSONEncoder().encode(holder)
        #expect(try JSONDecoder().decode(Holder.self, from: json) == holder)
    }

    @Test("a value can be built by hand and read by the typed decoder")
    func handBuilt() throws {
        let item = CBORValue.textMap([("a", .unsigned(7))])
        #expect(hex(item.encodedBytes()) == "a1616107")
        #expect(try CBORDecoder().decode(OneField.self, from: item) == OneField(a: 7))
    }
}

// MARK: - Malformed input

/// A row of the malformed-input table.
private struct BadInput: Sendable, CustomStringConvertible {
    let hex: String
    let reason: String

    init(_ hex: String, _ reason: String) {
        self.hex = hex
        self.reason = reason
    }

    var description: String { "\(hex) (\(reason))" }
}

private let malformedInputs: [BadInput] = [
    BadInput("", "no bytes at all"),
    BadInput("18", "a one-byte argument that is not there"),
    BadInput("19ff", "a two-byte argument cut in half"),
    BadInput("1c", "reserved additional information 28"),
    BadInput("1d", "reserved additional information 29"),
    BadInput("1e", "reserved additional information 30"),
    BadInput("1f", "an indefinite length on an unsigned integer"),
    BadInput("3f", "an indefinite length on a negative integer"),
    BadInput("df", "an indefinite length on a tag"),
    BadInput("ff", "a break that closes nothing"),
    BadInput("43010203ff", "a break after a complete item"),
    BadInput("430102", "a byte string shorter than its length"),
    BadInput("6301", "a text string shorter than its length"),
    BadInput("62c328", "a text string that is not UTF-8"),
    BadInput("83010203ff", "an array shorter than its count"),
    BadInput("a3616101", "a map shorter than its count"),
    BadInput("9a7fffffff01", "an array claiming more items than the payload holds"),
    BadInput("bbffffffffffffffff", "a map length past the addressable range"),
    BadInput("5f00ff", "an indefinite byte string holding an integer"),
    BadInput("5f7f61616161ff", "an indefinite byte string holding a text string"),
    BadInput("7f4101ff", "an indefinite text string holding a byte string"),
    BadInput("bf6161ff", "an indefinite map ending after a key"),
    BadInput("bf616101", "an indefinite map that never closes"),
    BadInput("9f0102", "an indefinite array that never closes"),
    BadInput("c1", "a tag with nothing tagged"),
    BadInput("0102", "a trailing byte after a complete item"),
]

@Suite("malformed input")
struct MalformedInputTests {
    @Test("malformed bytes raise a descriptive error", arguments: malformedInputs)
    fileprivate func refused(_ input: BadInput) {
        #expect(throws: CBORError.self) {
            try CBORValue(decoding: bytes(input.hex))
        }
    }

    @Test("the error names what went wrong and where")
    func errorDetail() {
        #expect(throws: CBORError.truncated(offset: 1, needed: 1)) {
            try CBORValue(decoding: bytes("18"))
        }
        #expect(throws: CBORError.trailingBytes(consumed: 1, remaining: 1)) {
            try CBORValue(decoding: bytes("0102"))
        }
        #expect(throws: CBORError.unexpectedBreak(offset: 0)) {
            try CBORValue(decoding: bytes("ff"))
        }
        #expect(throws: CBORError.reservedAdditionalInformation(initialByte: 0x1C, offset: 0)) {
            try CBORValue(decoding: bytes("1c"))
        }
        #expect(throws: CBORError.indefiniteLengthNotPermitted(initialByte: 0x1F, offset: 0)) {
            try CBORValue(decoding: bytes("1f"))
        }
        #expect(throws: CBORError.invalidUTF8(offset: 0)) {
            try CBORValue(decoding: bytes("62c328"))
        }
        #expect(throws: CBORError.mapValueMissing(offset: 1)) {
            try CBORValue(decoding: bytes("bf6161ff"))
        }
        #expect(throws: CBORError.lengthOutOfRange(length: .max, offset: 0)) {
            try CBORValue(decoding: bytes("bbffffffffffffffff"))
        }
    }

    @Test("every error describes itself")
    func descriptions() {
        let errors: [CBORError] = [
            .truncated(offset: 1, needed: 2),
            .reservedAdditionalInformation(initialByte: 0x1C, offset: 0),
            .indefiniteLengthNotPermitted(initialByte: 0x1F, offset: 0),
            .unexpectedBreak(offset: 3),
            .malformedIndefiniteString(offset: 4),
            .mapValueMissing(offset: 5),
            .invalidUTF8(offset: 6),
            .lengthOutOfRange(length: .max, offset: 7),
            .nestingTooDeep(offset: 8, limit: 256),
            .trailingBytes(consumed: 9, remaining: 10),
        ]
        for error in errors {
            #expect(!error.description.isEmpty)
            #expect(error.localizedDescription == error.description)
        }
    }

    @Test("nesting past the limit is refused rather than overflowing the stack")
    func nestingLimit() {
        var payload = [UInt8](repeating: 0x81, count: 400)
        payload.append(0x00)
        #expect(throws: CBORError.self) {
            try CBORValue(decoding: Data(payload))
        }
        // One level inside the limit still parses.
        var shallow = [UInt8](repeating: 0x81, count: 200)
        shallow.append(0x00)
        #expect(throws: Never.self) {
            try CBORValue(decoding: Data(shallow))
        }
    }

    @Test("a payload of the wrong shape raises a decoding error, not a parse error")
    func shapeMismatch() {
        #expect(throws: DecodingError.self) {
            try CBORDecoder().decode(OneField.self, from: bytes("83010203"))
        }
        #expect(throws: DecodingError.self) {
            try CBORDecoder().decode(OneField.self, from: bytes("a0"))
        }
        #expect(throws: DecodingError.self) {
            try CBORDecoder().decode(String.self, from: bytes("01"))
        }
    }
}

// MARK: - The JSON bridge and the diagnostic rendering

@Suite("CBORValue bridges")
struct CBORValueBridgeTests {
    @Test("a JSON-shaped item converts and converts back")
    func jsonRoundTrips() throws {
        let item = CBORValue.map([
            .init(key: .textString("flag"), value: .bool(true)),
            .init(key: .textString("count"), value: .unsigned(3)),
            .init(key: .textString("cold"), value: .negative(2)),
            .init(key: .textString("ratio"), value: .float(0.5)),
            .init(key: .textString("name"), value: .textString("post")),
            .init(key: .textString("nothing"), value: .null),
            .init(key: .textString("items"), value: .array([.unsigned(1), .unsigned(2)])),
        ])

        let object = try #require(item.jsonObject)
        #expect(JSONSerialization.isValidJSONObject(object))

        let back = try #require(CBORValue(jsonObject: object))
        // The inverse orders map entries by key, so the comparison is
        // against the sorted form rather than the written one.
        guard case .map(let entries) = back else {
            Issue.record("an object converts back to a map")
            return
        }
        let keys = entries.compactMap(\.key.stringValue)
        #expect(keys == keys.sorted())
        #expect(entries.count == 7)
        #expect(back["name"] == .textString("post"))
        #expect(back["cold"] == .negative(2))
    }

    @Test("what JSON cannot spell converts to nothing")
    func nonJsonItemsRefuse() {
        #expect(CBORValue.byteString([1, 2]).jsonObject == nil)
        #expect(CBORValue.undefined.jsonObject == nil)
        #expect(CBORValue.simple(19).jsonObject == nil)
        #expect(CBORValue.array([.byteString([1])]).jsonObject == nil)
        #expect(CBORValue.map([.init(key: .unsigned(1), value: .null)]).jsonObject == nil)
        // Past `Int64`, which JSON has no exact spelling for here.
        #expect(CBORValue.unsigned(UInt64.max).jsonObject == nil)
    }

    @Test("a tag is looked through rather than refused")
    func tagsAreLookedThrough() {
        let tagged = CBORValue.tag(number: 42, item: .textString("body"))
        #expect(tagged.jsonObject as? String == "body")
    }

    @Test("what JSONSerialization actually parses converts")
    func realJsonParses() throws {
        let source = Data(#"{"a":[1,2.5,true,null],"b":"text"}"#.utf8)
        let parsed = try JSONSerialization.jsonObject(with: source)
        let item = try #require(CBORValue(jsonObject: parsed))
        #expect(item["b"] == .textString("text"))
        #expect(item["a"]?[0] == .unsigned(1))
        #expect(item["a"]?[2] == .bool(true))
        #expect(item["a"]?[3] == .null)
    }

    @Test("an item renders in the diagnostic notation of the RFC")
    func diagnosticNotationRenders() {
        #expect("\(CBORValue.unsigned(1))" == "1")
        #expect("\(CBORValue.negative(0))" == "-1")
        #expect("\(CBORValue.byteString([0x01, 0xFF]))" == "h'01ff'")
        #expect("\(CBORValue.textString("a\"b"))" == #""a\"b""#)
        #expect("\(CBORValue.array([.unsigned(1), .null]))" == "[1, null]")
        #expect(
            "\(CBORValue.map([.init(key: .textString("k"), value: .bool(false))]))"
                == "{\"k\": false}"
        )
        #expect("\(CBORValue.tag(number: 2, item: .unsigned(3)))" == "2(3)")
        #expect("\(CBORValue.undefined)" == "undefined")
        #expect("\(CBORValue.simple(19))" == "simple(19)")
        #expect("\(CBORValue.bool(true))" == "true")
    }
}
