import Foundation

// MARK: - Public decoder

/// Decodes `Decodable` values from the CBOR the panproto C ABI writes.
///
/// The engine encodes every payload with `ciborium` driven by `serde`,
/// so the shapes this decoder expects are `serde`'s: a struct arrives
/// as a map with text keys, an `Option` arrives as either CBOR null or
/// the wrapped value itself, and an enum arrives externally tagged, as
/// a text string for a unit variant and as a one-entry map keyed by the
/// variant name otherwise.
///
/// Reading is deliberately tolerant, because the host is expected to
/// keep working against an engine that has learned new fields:
///
/// - Definite and indefinite lengths both parse, for maps, arrays, byte
///   strings, and text strings.
/// - Unrecognized map keys are skipped rather than refused.
/// - Half, single, and double precision floats all parse.
/// - Semantic tags are read through: a tagged item decodes as the item
///   it tags, and the bignum tags 2 and 3 additionally decode as
///   integers. A field typed as ``CBORValue`` keeps its tags.
/// - A `Data` field accepts a byte string or an array of integers, and
///   an `[UInt8]` field accepts the same two, so a Rust field spelled
///   `Vec<u8>` and one spelled with `serde_bytes` both arrive.
/// - A `UInt64` above `Int64.max` decodes without loss.
///
/// Tolerance stops at the payload boundary. A payload is exactly one
/// CBOR item, and trailing bytes are an error, which is the rule the
/// engine's own decoder applies to the same bytes.
public struct CBORDecoder: Sendable {
    /// Context handed to every `Decodable` value this decoder visits.
    public var userInfo: [CodingUserInfoKey: any Sendable] = [:]

    /// Create a decoder with no user info.
    public init() {}

    /// Decode a value of type `type` from one CBOR item.
    ///
    /// - Throws: ``CBORError`` when `data` is not a single well-formed
    ///   CBOR item, and `DecodingError` when it is but does not match
    ///   `type`.
    public func decode<T: Decodable>(_ type: T.Type, from data: Data) throws -> T {
        try decode(type, from: CBORValue(decoding: data))
    }

    /// Decode a value of type `type` from an item already parsed.
    ///
    /// This is the path for a host that inspected a payload as a
    /// ``CBORValue``, found the fragment it wanted, and now wants it as
    /// a Swift value.
    ///
    /// - Throws: `DecodingError` when `value` does not match `type`.
    public func decode<T: Decodable>(_ type: T.Type, from value: CBORValue) throws -> T {
        let decoder = CBORDecoderImpl(value: value, codingPath: [], userInfo: userInfo)
        return try decoder.unbox(type)
    }
}

// MARK: - Byte-level reader

/// Reads CBOR items out of a buffer, one index at a time.
struct CBORReader {
    /// The nesting depth past which parsing gives up.
    ///
    /// Adversarial input that nests without bound costs a diagnosable
    /// error instead of a stack overflow. The limit matches the
    /// recursion limit the engine's decoder applies to the same bytes.
    static let nestingLimit = 256

    /// The buffer being read.
    let bytes: UnsafeRawBufferPointer
    /// How far into ``bytes`` reading has got.
    private(set) var index = 0

    /// Read from the whole of `bytes`.
    init(bytes: UnsafeRawBufferPointer) {
        self.bytes = bytes
    }

    /// The argument an item's head carries.
    private enum Argument {
        /// A count, a length, or a value.
        case count(UInt64)
        /// The item runs until a break stop code.
        case indefinite
    }

    /// Fail unless every byte has been read.
    func requireExhausted() throws {
        guard index == bytes.count else {
            throw CBORError.trailingBytes(consumed: index, remaining: bytes.count - index)
        }
    }

    /// Read one CBOR item, and everything nested inside it.
    mutating func parseItem(depth: Int) throws -> CBORValue {
        let start = index
        guard depth <= Self.nestingLimit else {
            throw CBORError.nestingTooDeep(offset: start, limit: Self.nestingLimit)
        }

        let initial = try readByte()
        let major = initial >> 5
        let additional = initial & 0x1F

        switch major {
        case 0:
            return .unsigned(try readDefiniteArgument(additional, initial: initial, at: start))
        case 1:
            return .negative(try readDefiniteArgument(additional, initial: initial, at: start))
        case 2:
            switch try readArgument(additional, initial: initial, at: start) {
            case .count(let length):
                return .byteString(try readBytes(try checkedCount(length, bytesEach: 1, at: start)))
            case .indefinite:
                return .byteString(try readSegments(major: 2))
            }
        case 3:
            let raw: [UInt8]
            switch try readArgument(additional, initial: initial, at: start) {
            case .count(let length):
                raw = try readBytes(try checkedCount(length, bytesEach: 1, at: start))
            case .indefinite:
                raw = try readSegments(major: 3)
            }
            guard let text = String(bytes: raw, encoding: .utf8) else {
                throw CBORError.invalidUTF8(offset: start)
            }
            return .textString(text)
        case 4:
            return .array(try readElements(additional, initial: initial, at: start, depth: depth))
        case 5:
            return .map(try readEntries(additional, initial: initial, at: start, depth: depth))
        case 6:
            let number = try readDefiniteArgument(additional, initial: initial, at: start)
            return .tag(number: number, item: try parseItem(depth: depth + 1))
        default:
            return try readSimpleOrFloat(additional, initial: initial, at: start)
        }
    }

    /// Read the elements of an array item, whichever length form it
    /// uses.
    private mutating func readElements(
        _ additional: UInt8,
        initial: UInt8,
        at start: Int,
        depth: Int
    ) throws -> [CBORValue] {
        var elements: [CBORValue] = []
        switch try readArgument(additional, initial: initial, at: start) {
        case .count(let length):
            let count = try checkedCount(length, bytesEach: 1, at: start)
            elements.reserveCapacity(count)
            for _ in 0..<count {
                elements.append(try parseItem(depth: depth + 1))
            }
        case .indefinite:
            while try !consumeBreak() {
                elements.append(try parseItem(depth: depth + 1))
            }
        }
        return elements
    }

    /// Read the entries of a map item, whichever length form it uses.
    private mutating func readEntries(
        _ additional: UInt8,
        initial: UInt8,
        at start: Int,
        depth: Int
    ) throws -> [CBORValue.Entry] {
        var entries: [CBORValue.Entry] = []
        switch try readArgument(additional, initial: initial, at: start) {
        case .count(let length):
            let count = try checkedCount(length, bytesEach: 2, at: start)
            entries.reserveCapacity(count)
            for _ in 0..<count {
                let key = try parseItem(depth: depth + 1)
                entries.append(.init(key: key, value: try parseItem(depth: depth + 1)))
            }
        case .indefinite:
            while try !consumeBreak() {
                let keyStart = index
                let key = try parseItem(depth: depth + 1)
                guard try !consumeBreak() else {
                    throw CBORError.mapValueMissing(offset: keyStart)
                }
                entries.append(.init(key: key, value: try parseItem(depth: depth + 1)))
            }
        }
        return entries
    }

    /// Read a major type 7 item: a simple value, or a float in one of
    /// the three widths.
    private mutating func readSimpleOrFloat(
        _ additional: UInt8,
        initial: UInt8,
        at start: Int
    ) throws -> CBORValue {
        switch additional {
        case 0...23:
            return CBORValue.simpleValue(additional)
        case 24:
            return CBORValue.simpleValue(try readByte())
        case 25:
            let bits = UInt16(truncatingIfNeeded: try readBigEndian(byteCount: 2))
            return .float(CBORHalf.double(fromBitPattern: bits))
        case 26:
            let bits = UInt32(truncatingIfNeeded: try readBigEndian(byteCount: 4))
            return .float(Double(Float(bitPattern: bits)))
        case 27:
            return .float(Double(bitPattern: try readBigEndian(byteCount: 8)))
        case 31:
            throw CBORError.unexpectedBreak(offset: start)
        default:
            throw CBORError.reservedAdditionalInformation(initialByte: initial, offset: start)
        }
    }

    /// Read the chunks of an indefinite-length string and concatenate
    /// them.
    private mutating func readSegments(major: UInt8) throws -> [UInt8] {
        var accumulated: [UInt8] = []
        while true {
            let chunkStart = index
            let initial = try readByte()
            if initial == 0xFF { return accumulated }
            guard initial >> 5 == major else {
                throw CBORError.malformedIndefiniteString(offset: chunkStart)
            }
            guard
                case .count(let length) = try readArgument(
                    initial & 0x1F,
                    initial: initial,
                    at: chunkStart
                )
            else {
                throw CBORError.malformedIndefiniteString(offset: chunkStart)
            }
            let count = try checkedCount(length, bytesEach: 1, at: chunkStart)
            accumulated.append(contentsOf: try readBytes(count))
        }
    }

    /// Read the argument of an item's head.
    private mutating func readArgument(
        _ additional: UInt8,
        initial: UInt8,
        at start: Int
    ) throws -> Argument {
        switch additional {
        case 0...23: .count(UInt64(additional))
        case 24: .count(UInt64(try readByte()))
        case 25: .count(try readBigEndian(byteCount: 2))
        case 26: .count(try readBigEndian(byteCount: 4))
        case 27: .count(try readBigEndian(byteCount: 8))
        case 31: .indefinite
        default:
            throw CBORError.reservedAdditionalInformation(initialByte: initial, offset: start)
        }
    }

    /// Read the argument of an item whose major type has no
    /// indefinite-length form.
    private mutating func readDefiniteArgument(
        _ additional: UInt8,
        initial: UInt8,
        at start: Int
    ) throws -> UInt64 {
        guard case .count(let value) = try readArgument(additional, initial: initial, at: start)
        else {
            throw CBORError.indefiniteLengthNotPermitted(initialByte: initial, offset: start)
        }
        return value
    }

    /// Consume a break stop code if one is next.
    private mutating func consumeBreak() throws -> Bool {
        guard index < bytes.count else {
            throw CBORError.truncated(offset: index, needed: 1)
        }
        guard bytes[index] == 0xFF else { return false }
        index += 1
        return true
    }

    /// Read one byte.
    private mutating func readByte() throws -> UInt8 {
        guard index < bytes.count else {
            throw CBORError.truncated(offset: index, needed: 1)
        }
        defer { index += 1 }
        return bytes[index]
    }

    /// Read `byteCount` bytes as a big-endian integer.
    private mutating func readBigEndian(byteCount: Int) throws -> UInt64 {
        guard byteCount <= bytes.count - index else {
            throw CBORError.truncated(offset: index, needed: byteCount - (bytes.count - index))
        }
        var value: UInt64 = 0
        for offset in index..<(index + byteCount) {
            value = value << 8 | UInt64(bytes[offset])
        }
        index += byteCount
        return value
    }

    /// Read `count` bytes.
    private mutating func readBytes(_ count: Int) throws -> [UInt8] {
        guard count <= bytes.count - index else {
            throw CBORError.truncated(offset: index, needed: count - (bytes.count - index))
        }
        defer { index += count }
        return [UInt8](bytes[index..<(index + count)])
    }

    /// Turn a declared length into a count this platform can hold, and
    /// refuse one the remaining input cannot possibly satisfy.
    ///
    /// A definite-length head is free to claim more items than the
    /// payload holds, and every item costs at least `bytesEach` bytes,
    /// so the claim can be checked before anything is allocated.
    private func checkedCount(_ length: UInt64, bytesEach: Int, at offset: Int) throws -> Int {
        guard let count = Int(exactly: length) else {
            throw CBORError.lengthOutOfRange(length: length, offset: offset)
        }
        let available = bytes.count - index
        guard count <= available / bytesEach else {
            let required = length.multipliedReportingOverflow(by: UInt64(bytesEach))
            let needed =
                required.overflow ? Int.max : Int(clamping: required.partialValue) - available
            throw CBORError.truncated(offset: index, needed: needed)
        }
        return count
    }
}

// MARK: - Decoder

/// The `Decoder` a value sees while it reads itself out of one item.
final class CBORDecoderImpl: Decoder {
    /// The item this decoder reads.
    let value: CBORValue
    let codingPath: [any CodingKey]
    /// Context carried from ``CBORDecoder/userInfo``.
    let sendableUserInfo: [CodingUserInfoKey: any Sendable]

    var userInfo: [CodingUserInfoKey: Any] { sendableUserInfo }

    init(
        value: CBORValue,
        codingPath: [any CodingKey],
        userInfo: [CodingUserInfoKey: any Sendable]
    ) {
        self.value = value
        self.codingPath = codingPath
        self.sendableUserInfo = userInfo
    }

    /// Decode `type` from this decoder's item.
    ///
    /// The two types the codec recognizes on sight are handled here:
    /// ``CBORValue``, which is the item itself, tags and all, and
    /// `Data`, which reads from a byte string or from an array of byte
    /// values. Everything else reads itself through the containers.
    func unbox<T: Decodable>(_ type: T.Type) throws -> T {
        if T.self == CBORValue.self, let item = value as? T {
            return item
        }
        if T.self == Data.self, let data = try CBORUnboxing.data(value, at: codingPath) as? T {
            return data
        }
        return try T(from: self)
    }

    func container<Key: CodingKey>(
        keyedBy type: Key.Type
    ) throws -> KeyedDecodingContainer<Key> {
        guard case .map(let entries) = value.untagged else {
            throw DecodingError.typeMismatch(
                [String: CBORValue].self,
                DecodingError.Context(
                    codingPath: codingPath,
                    debugDescription: "expected a CBOR map, found \(CBORUnboxing.describe(value))"
                )
            )
        }
        return KeyedDecodingContainer(
            CBORKeyedDecodingContainer<Key>(decoder: self, entries: entries)
        )
    }

    func unkeyedContainer() throws -> any UnkeyedDecodingContainer {
        switch value.untagged {
        case .array(let elements):
            return CBORUnkeyedDecodingContainer(decoder: self, source: .items(elements))
        case .byteString(let payload):
            // A Rust `Vec<u8>` written with `serde_bytes` arrives as a
            // byte string; read it as the sequence of bytes it is.
            return CBORUnkeyedDecodingContainer(decoder: self, source: .bytes(payload))
        default:
            throw DecodingError.typeMismatch(
                [CBORValue].self,
                DecodingError.Context(
                    codingPath: codingPath,
                    debugDescription: "expected a CBOR array, found \(CBORUnboxing.describe(value))"
                )
            )
        }
    }

    func singleValueContainer() throws -> any SingleValueDecodingContainer {
        CBORSingleValueDecodingContainer(decoder: self)
    }

    /// A decoder over `item`, one step further down `key`.
    func child(_ item: CBORValue, at key: any CodingKey) -> CBORDecoderImpl {
        CBORDecoderImpl(
            value: item,
            codingPath: codingPath + [key],
            userInfo: sendableUserInfo
        )
    }
}

// MARK: - Keyed container

/// Reads the entries of a CBOR map.
struct CBORKeyedDecodingContainer<Key: CodingKey>: KeyedDecodingContainerProtocol {
    let decoder: CBORDecoderImpl
    /// The entries in wire order, which is what ``allKeys`` reports.
    let entries: [CBORValue.Entry]
    /// The entries a `CodingKey` can name, first occurrence winning.
    private let index: [CBORMapKey: CBORValue]

    var codingPath: [any CodingKey] { decoder.codingPath }

    init(decoder: CBORDecoderImpl, entries: [CBORValue.Entry]) {
        self.decoder = decoder
        self.entries = entries
        var index: [CBORMapKey: CBORValue] = [:]
        index.reserveCapacity(entries.count)
        for entry in entries {
            guard let key = CBORMapKey(entry.key), index[key] == nil else { continue }
            index[key] = entry.value
        }
        self.index = index
    }

    var allKeys: [Key] {
        entries.compactMap { CBORMapKey($0.key)?.codingKey(as: Key.self) }
    }

    func contains(_ key: Key) -> Bool {
        item(for: key) != nil
    }

    /// The value filed under `key`.
    ///
    /// The name is tried first, because that is how `serde` writes a
    /// struct field and a `String`-keyed map. A key that also names an
    /// index then tries the integer spelling, which is how `serde`
    /// writes a map keyed by an integer; so does a name that reads as a
    /// number, since the standard library hands the keys of a
    /// `String`-keyed dictionary no index of their own.
    private func item(for key: Key) -> CBORValue? {
        if let found = index[.text(key.stringValue)] { return found }
        if let integer = key.intValue, let found = index[.integer(Int64(integer))] {
            return found
        }
        if let integer = Int64(key.stringValue), let found = index[.integer(integer)] {
            return found
        }
        return nil
    }

    /// The value filed under `key`, or a `keyNotFound` failure.
    private func require(_ key: Key) throws -> CBORValue {
        guard let found = item(for: key) else {
            throw DecodingError.keyNotFound(
                key,
                DecodingError.Context(
                    codingPath: codingPath,
                    debugDescription: "no entry keyed by \(key.stringValue)"
                )
            )
        }
        return found
    }

    /// The coding path a value filed under `key` sits at.
    private func path(_ key: Key) -> [any CodingKey] { codingPath + [key] }

    func decodeNil(forKey key: Key) throws -> Bool {
        let item = try require(key)
        return item.isNull || item.isUndefined
    }

    func decode(_ type: Bool.Type, forKey key: Key) throws -> Bool {
        try CBORUnboxing.bool(try require(key), at: path(key))
    }

    func decode(_ type: String.Type, forKey key: Key) throws -> String {
        try CBORUnboxing.string(try require(key), at: path(key))
    }

    func decode(_ type: Double.Type, forKey key: Key) throws -> Double {
        try CBORUnboxing.float(type, from: try require(key), at: path(key))
    }

    func decode(_ type: Float.Type, forKey key: Key) throws -> Float {
        try CBORUnboxing.float(type, from: try require(key), at: path(key))
    }

    func decode(_ type: Int.Type, forKey key: Key) throws -> Int {
        try CBORUnboxing.integer(type, from: try require(key), at: path(key))
    }

    func decode(_ type: Int8.Type, forKey key: Key) throws -> Int8 {
        try CBORUnboxing.integer(type, from: try require(key), at: path(key))
    }

    func decode(_ type: Int16.Type, forKey key: Key) throws -> Int16 {
        try CBORUnboxing.integer(type, from: try require(key), at: path(key))
    }

    func decode(_ type: Int32.Type, forKey key: Key) throws -> Int32 {
        try CBORUnboxing.integer(type, from: try require(key), at: path(key))
    }

    func decode(_ type: Int64.Type, forKey key: Key) throws -> Int64 {
        try CBORUnboxing.integer(type, from: try require(key), at: path(key))
    }

    func decode(_ type: UInt.Type, forKey key: Key) throws -> UInt {
        try CBORUnboxing.integer(type, from: try require(key), at: path(key))
    }

    func decode(_ type: UInt8.Type, forKey key: Key) throws -> UInt8 {
        try CBORUnboxing.integer(type, from: try require(key), at: path(key))
    }

    func decode(_ type: UInt16.Type, forKey key: Key) throws -> UInt16 {
        try CBORUnboxing.integer(type, from: try require(key), at: path(key))
    }

    func decode(_ type: UInt32.Type, forKey key: Key) throws -> UInt32 {
        try CBORUnboxing.integer(type, from: try require(key), at: path(key))
    }

    func decode(_ type: UInt64.Type, forKey key: Key) throws -> UInt64 {
        try CBORUnboxing.integer(type, from: try require(key), at: path(key))
    }

    func decode<T: Decodable>(_ type: T.Type, forKey key: Key) throws -> T {
        try decoder.child(try require(key), at: key).unbox(type)
    }

    func nestedContainer<NestedKey: CodingKey>(
        keyedBy type: NestedKey.Type,
        forKey key: Key
    ) throws -> KeyedDecodingContainer<NestedKey> {
        try decoder.child(try require(key), at: key).container(keyedBy: type)
    }

    func nestedUnkeyedContainer(forKey key: Key) throws -> any UnkeyedDecodingContainer {
        try decoder.child(try require(key), at: key).unkeyedContainer()
    }

    func superDecoder() throws -> any Decoder {
        let key = CBORCodingKey.superKey
        let item = index[.text(key.stringValue)] ?? .null
        return CBORDecoderImpl(
            value: item,
            codingPath: codingPath + [key],
            userInfo: decoder.sendableUserInfo
        )
    }

    func superDecoder(forKey key: Key) throws -> any Decoder {
        decoder.child(try require(key), at: key)
    }
}

// MARK: - Unkeyed container

/// Reads the elements of a CBOR array, or the bytes of a byte string
/// standing in for one.
struct CBORUnkeyedDecodingContainer: UnkeyedDecodingContainer {
    /// What the container walks.
    enum Source {
        /// The elements of an array item.
        case items([CBORValue])
        /// The bytes of a byte string, each read as an integer.
        case bytes([UInt8])
    }

    let decoder: CBORDecoderImpl
    let source: Source
    var currentIndex = 0

    var codingPath: [any CodingKey] { decoder.codingPath }

    var count: Int? {
        switch source {
        case .items(let items): items.count
        case .bytes(let payload): payload.count
        }
    }

    var isAtEnd: Bool { currentIndex >= (count ?? 0) }

    /// The key naming the position about to be read.
    private var currentKey: CBORCodingKey { CBORCodingKey(index: currentIndex) }

    /// The element at ``currentIndex``, without consuming it.
    private func peek(_ type: Any.Type) throws -> CBORValue {
        guard !isAtEnd else {
            throw DecodingError.valueNotFound(
                type,
                DecodingError.Context(
                    codingPath: codingPath + [currentKey],
                    debugDescription: "the array holds \(count ?? 0) element(s)"
                )
            )
        }
        switch source {
        case .items(let items): return items[currentIndex]
        case .bytes(let payload): return .unsigned(UInt64(payload[currentIndex]))
        }
    }

    /// The element at ``currentIndex``, consuming it.
    private mutating func next(_ type: Any.Type) throws -> CBORValue {
        let item = try peek(type)
        currentIndex += 1
        return item
    }

    mutating func decodeNil() throws -> Bool {
        let item = try peek(CBORValue.self)
        guard item.isNull || item.isUndefined else { return false }
        currentIndex += 1
        return true
    }

    mutating func decode(_ type: Bool.Type) throws -> Bool {
        try CBORUnboxing.bool(try next(type), at: codingPath + [currentKey])
    }

    mutating func decode(_ type: String.Type) throws -> String {
        try CBORUnboxing.string(try next(type), at: codingPath + [currentKey])
    }

    mutating func decode(_ type: Double.Type) throws -> Double {
        try CBORUnboxing.float(type, from: try next(type), at: codingPath + [currentKey])
    }

    mutating func decode(_ type: Float.Type) throws -> Float {
        try CBORUnboxing.float(type, from: try next(type), at: codingPath + [currentKey])
    }

    mutating func decode(_ type: Int.Type) throws -> Int {
        try CBORUnboxing.integer(type, from: try next(type), at: codingPath + [currentKey])
    }

    mutating func decode(_ type: Int8.Type) throws -> Int8 {
        try CBORUnboxing.integer(type, from: try next(type), at: codingPath + [currentKey])
    }

    mutating func decode(_ type: Int16.Type) throws -> Int16 {
        try CBORUnboxing.integer(type, from: try next(type), at: codingPath + [currentKey])
    }

    mutating func decode(_ type: Int32.Type) throws -> Int32 {
        try CBORUnboxing.integer(type, from: try next(type), at: codingPath + [currentKey])
    }

    mutating func decode(_ type: Int64.Type) throws -> Int64 {
        try CBORUnboxing.integer(type, from: try next(type), at: codingPath + [currentKey])
    }

    mutating func decode(_ type: UInt.Type) throws -> UInt {
        try CBORUnboxing.integer(type, from: try next(type), at: codingPath + [currentKey])
    }

    mutating func decode(_ type: UInt8.Type) throws -> UInt8 {
        try CBORUnboxing.integer(type, from: try next(type), at: codingPath + [currentKey])
    }

    mutating func decode(_ type: UInt16.Type) throws -> UInt16 {
        try CBORUnboxing.integer(type, from: try next(type), at: codingPath + [currentKey])
    }

    mutating func decode(_ type: UInt32.Type) throws -> UInt32 {
        try CBORUnboxing.integer(type, from: try next(type), at: codingPath + [currentKey])
    }

    mutating func decode(_ type: UInt64.Type) throws -> UInt64 {
        try CBORUnboxing.integer(type, from: try next(type), at: codingPath + [currentKey])
    }

    mutating func decode<T: Decodable>(_ type: T.Type) throws -> T {
        let key = currentKey
        return try decoder.child(try next(type), at: key).unbox(type)
    }

    mutating func nestedContainer<NestedKey: CodingKey>(
        keyedBy type: NestedKey.Type
    ) throws -> KeyedDecodingContainer<NestedKey> {
        let key = currentKey
        return try decoder.child(try next(CBORValue.self), at: key).container(keyedBy: type)
    }

    mutating func nestedUnkeyedContainer() throws -> any UnkeyedDecodingContainer {
        let key = currentKey
        return try decoder.child(try next(CBORValue.self), at: key).unkeyedContainer()
    }

    mutating func superDecoder() throws -> any Decoder {
        let key = currentKey
        return decoder.child(try next(CBORValue.self), at: key)
    }
}

// MARK: - Single value container

/// Reads one CBOR item.
struct CBORSingleValueDecodingContainer: SingleValueDecodingContainer {
    let decoder: CBORDecoderImpl

    var codingPath: [any CodingKey] { decoder.codingPath }

    /// The item being read.
    private var value: CBORValue { decoder.value }

    func decodeNil() -> Bool {
        value.isNull || value.isUndefined
    }

    func decode(_ type: Bool.Type) throws -> Bool {
        try CBORUnboxing.bool(value, at: codingPath)
    }

    func decode(_ type: String.Type) throws -> String {
        try CBORUnboxing.string(value, at: codingPath)
    }

    func decode(_ type: Double.Type) throws -> Double {
        try CBORUnboxing.float(type, from: value, at: codingPath)
    }

    func decode(_ type: Float.Type) throws -> Float {
        try CBORUnboxing.float(type, from: value, at: codingPath)
    }

    func decode(_ type: Int.Type) throws -> Int {
        try CBORUnboxing.integer(type, from: value, at: codingPath)
    }

    func decode(_ type: Int8.Type) throws -> Int8 {
        try CBORUnboxing.integer(type, from: value, at: codingPath)
    }

    func decode(_ type: Int16.Type) throws -> Int16 {
        try CBORUnboxing.integer(type, from: value, at: codingPath)
    }

    func decode(_ type: Int32.Type) throws -> Int32 {
        try CBORUnboxing.integer(type, from: value, at: codingPath)
    }

    func decode(_ type: Int64.Type) throws -> Int64 {
        try CBORUnboxing.integer(type, from: value, at: codingPath)
    }

    func decode(_ type: UInt.Type) throws -> UInt {
        try CBORUnboxing.integer(type, from: value, at: codingPath)
    }

    func decode(_ type: UInt8.Type) throws -> UInt8 {
        try CBORUnboxing.integer(type, from: value, at: codingPath)
    }

    func decode(_ type: UInt16.Type) throws -> UInt16 {
        try CBORUnboxing.integer(type, from: value, at: codingPath)
    }

    func decode(_ type: UInt32.Type) throws -> UInt32 {
        try CBORUnboxing.integer(type, from: value, at: codingPath)
    }

    func decode(_ type: UInt64.Type) throws -> UInt64 {
        try CBORUnboxing.integer(type, from: value, at: codingPath)
    }

    func decode<T: Decodable>(_ type: T.Type) throws -> T {
        try decoder.unbox(type)
    }
}

// MARK: - Items as Swift scalars

/// Turns CBOR items into the Swift scalars the containers hand back.
enum CBORUnboxing {
    /// A short phrase naming what an item is, for a failure message.
    static func describe(_ value: CBORValue) -> String {
        switch value {
        case .unsigned: "an unsigned integer"
        case .negative: "a negative integer"
        case .byteString: "a byte string"
        case .textString: "a text string"
        case .array: "an array"
        case .map: "a map"
        case .tag(let number, _): "an item tagged \(number)"
        case .simple(let raw): "simple value \(raw)"
        case .bool: "a boolean"
        case .null: "null"
        case .undefined: "undefined"
        case .float: "a float"
        }
    }

    /// A `typeMismatch` failure naming what was wanted and what arrived.
    private static func mismatch(
        _ type: Any.Type,
        _ wanted: String,
        _ value: CBORValue,
        _ path: [any CodingKey]
    ) -> DecodingError {
        DecodingError.typeMismatch(
            type,
            DecodingError.Context(
                codingPath: path,
                debugDescription: "expected \(wanted), found \(describe(value))"
            )
        )
    }

    /// The boolean an item carries.
    static func bool(_ value: CBORValue, at path: [any CodingKey]) throws -> Bool {
        guard case .bool(let flag) = value.untagged else {
            throw mismatch(Bool.self, "a CBOR boolean", value, path)
        }
        return flag
    }

    /// The text an item carries.
    static func string(_ value: CBORValue, at path: [any CodingKey]) throws -> String {
        guard case .textString(let text) = value.untagged else {
            throw mismatch(String.self, "a CBOR text string", value, path)
        }
        return text
    }

    /// The bytes an item carries, from a byte string or from an array
    /// of byte-sized integers.
    static func data(_ value: CBORValue, at path: [any CodingKey]) throws -> Data {
        switch value.untagged {
        case .byteString(let payload):
            return Data(payload)
        case .array(let elements):
            var payload = [UInt8]()
            payload.reserveCapacity(elements.count)
            for element in elements {
                guard case .unsigned(let magnitude) = element.untagged,
                    let byte = UInt8(exactly: magnitude)
                else {
                    throw mismatch(Data.self, "an array of byte values", element, path)
                }
                payload.append(byte)
            }
            return Data(payload)
        default:
            throw mismatch(Data.self, "a CBOR byte string", value, path)
        }
    }

    /// The floating-point value an item carries.
    static func float<T: BinaryFloatingPoint>(
        _ type: T.Type,
        from value: CBORValue,
        at path: [any CodingKey]
    ) throws -> T {
        guard case .float(let number) = value.untagged else {
            throw mismatch(type, "a CBOR float", value, path)
        }
        let narrowed = T(number)
        guard narrowed.isFinite || !number.isFinite else {
            throw DecodingError.dataCorrupted(
                DecodingError.Context(
                    codingPath: path,
                    debugDescription: "CBOR float \(number) does not fit in \(type)"
                )
            )
        }
        return narrowed
    }

    /// The integer an item carries, as the sign and magnitude the wire
    /// stores.
    ///
    /// A negative integer stores `n` and denotes `-1 - n`, so the pair
    /// reaches `-2^64` and `2^64 - 1` alike. The bignum tags 2 and 3
    /// are read here as well, for the magnitudes that fit.
    private static func parts(of value: CBORValue) -> (negative: Bool, magnitude: UInt64)? {
        var current = value
        while case .tag(let number, let item) = current {
            if number == 2 || number == 3, case .byteString(let payload) = item {
                guard let magnitude = bigEndianMagnitude(payload) else { return nil }
                return (negative: number == 3, magnitude: magnitude)
            }
            current = item
        }
        switch current {
        case .unsigned(let magnitude): return (false, magnitude)
        case .negative(let magnitude): return (true, magnitude)
        default: return nil
        }
    }

    /// The value of a big-endian bignum payload, when it fits in 64
    /// bits.
    private static func bigEndianMagnitude(_ payload: [UInt8]) -> UInt64? {
        var magnitude: UInt64 = 0
        var seen = 0
        for byte in payload {
            if seen == 0 && byte == 0 { continue }
            guard seen < 8 else { return nil }
            magnitude = magnitude << 8 | UInt64(byte)
            seen += 1
        }
        return magnitude
    }

    /// The integer an item carries, narrowed to `type`.
    static func integer<T: FixedWidthInteger>(
        _ type: T.Type,
        from value: CBORValue,
        at path: [any CodingKey]
    ) throws -> T {
        guard let parts = parts(of: value) else {
            throw mismatch(type, "a CBOR integer", value, path)
        }
        if parts.negative {
            guard parts.magnitude <= UInt64(Int64.max),
                let narrowed = T(exactly: Int64(bitPattern: ~parts.magnitude))
            else {
                throw outOfRange("-1 - \(parts.magnitude)", type, path)
            }
            return narrowed
        }
        guard let narrowed = T(exactly: parts.magnitude) else {
            throw outOfRange("\(parts.magnitude)", type, path)
        }
        return narrowed
    }

    /// A `dataCorrupted` failure for an integer that does not narrow.
    private static func outOfRange(
        _ rendered: String,
        _ type: Any.Type,
        _ path: [any CodingKey]
    ) -> DecodingError {
        DecodingError.dataCorrupted(
            DecodingError.Context(
                codingPath: path,
                debugDescription: "CBOR integer \(rendered) does not fit in \(type)"
            )
        )
    }
}
