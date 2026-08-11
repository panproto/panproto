import Foundation

// MARK: - Public encoder

/// Encodes `Encodable` values to the CBOR the panproto C ABI reads.
///
/// The engine decodes every payload with `ciborium` driven by `serde`,
/// so this encoder writes what `serde`'s data model writes:
///
/// - A `Codable` struct becomes a definite-length map with text keys,
///   in declaration order, matching a Rust struct.
/// - A `nil` written with `encode(_:forKey:)` becomes CBOR null and a
///   non-`nil` optional becomes the wrapped value itself, matching
///   `Option`. A synthesized `Codable` conformance instead reaches for
///   `encodeIfPresent`, which leaves the key out; the engine reads an
///   absent field for an `Option` as `None`, so both spellings arrive.
/// - An array or a tuple becomes a definite-length array.
/// - A `Dictionary` with `String` keys becomes a map with text keys; a
///   `Dictionary` with `Int` keys becomes a map with integer keys.
/// - `Data` becomes a byte string, which the engine accepts both for a
///   `serde_bytes` field and for a plain `Vec<u8>`. An `[UInt8]`
///   becomes an array of integers, which the engine also accepts for
///   both.
/// - A ``CBORValue`` is written through unchanged, so a payload can
///   carry a fragment the Swift model does not describe.
///
/// A Rust enum reaches the wire externally tagged: a unit variant is a
/// text string, and every other variant is a one-entry map keyed by the
/// variant name whose value is the payload. A Swift enum reproduces
/// that by encoding a string into a single-value container for the unit
/// case, and by opening a one-key keyed container otherwise.
///
/// Output is deterministic. Lengths are always definite, every integer
/// head is the shortest that fits, a float takes the narrowest of half,
/// single, and double precision that reproduces it exactly, and
/// collections that carry no order of their own (`Dictionary`, `Set`)
/// are sorted by the encoded bytes of their keys or elements, which is
/// the ordering RFC 8949 calls canonical. Two encodes of the same value
/// therefore agree byte for byte.
public struct CBOREncoder: Sendable {
    /// Context handed to every `Encodable` value this encoder visits.
    public var userInfo: [CodingUserInfoKey: any Sendable] = [:]

    /// Create an encoder with no user info.
    public init() {}

    /// Encode `value` as a single CBOR item.
    ///
    /// - Throws: `EncodingError` when a value declines to encode
    ///   anything at all.
    public func encode<T: Encodable>(_ value: T) throws -> Data {
        let node = try CBOREncoderImpl.box(value, at: [], userInfo: userInfo)
        guard node.kind != .empty else {
            throw EncodingError.invalidValue(
                value,
                EncodingError.Context(
                    codingPath: [],
                    debugDescription: "\(T.self) encoded nothing, so there is no CBOR item to write"
                )
            )
        }
        var writer = CBORWriter()
        writer.write(node)
        return Data(writer.bytes)
    }
}

// MARK: - Unordered collections

/// Marks a value whose map form carries no inherent key order, so that
/// the encoder sorts its keys instead of trusting the order it sees.
///
/// `Dictionary` iterates in an order that depends on the hash seed,
/// which changes between processes; sorting is what makes the encoded
/// bytes reproducible. The conformance also settles how the keys are
/// spelled, which the keys themselves cannot: the standard library
/// hands an `Int`-keyed and a `String`-keyed dictionary the same kind
/// of coding key, one carrying both a name and an index whenever the
/// name reads as a number.
protocol CBORUnorderedKeys {
    /// Whether the keys belong on the wire as integers.
    static var cborKeysAreIntegers: Bool { get }
}

extension Dictionary: CBORUnorderedKeys {
    static var cborKeysAreIntegers: Bool { Key.self == Int.self }
}

/// Marks a value whose array form carries no inherent element order, so
/// that the encoder sorts the elements by their encoded bytes.
protocol CBORUnorderedElements {}

extension Set: CBORUnorderedElements {}

// MARK: - Byte-level writer

/// Appends CBOR items to one growing buffer.
struct CBORWriter {
    /// The bytes written so far.
    private(set) var bytes: [UInt8] = []

    /// Start a writer with an empty buffer.
    init() {}

    /// Write the head of an item: its major type and argument, using
    /// the shortest of the five encodings that holds the argument.
    mutating func writeHead(major: UInt8, argument: UInt64) {
        let base = major << 5
        switch argument {
        case ..<24:
            bytes.append(base | UInt8(argument))
        case ..<0x100:
            bytes.append(base | 24)
            bytes.append(UInt8(argument))
        case ..<0x1_0000:
            bytes.append(base | 25)
            appendBigEndian(UInt16(argument))
        case ..<0x1_0000_0000:
            bytes.append(base | 26)
            appendBigEndian(UInt32(argument))
        default:
            bytes.append(base | 27)
            appendBigEndian(argument)
        }
    }

    /// Append the big-endian bytes of `value`.
    mutating func appendBigEndian<T: FixedWidthInteger>(_ value: T) {
        withUnsafeBytes(of: value.bigEndian) { bytes.append(contentsOf: $0) }
    }

    /// Write one CBOR item.
    mutating func write(_ value: CBORValue) {
        switch value {
        case .unsigned(let magnitude):
            writeHead(major: 0, argument: magnitude)
        case .negative(let magnitude):
            writeHead(major: 1, argument: magnitude)
        case .byteString(let payload):
            writeHead(major: 2, argument: UInt64(payload.count))
            bytes.append(contentsOf: payload)
        case .textString(let text):
            writeHead(major: 3, argument: UInt64(text.utf8.count))
            bytes.append(contentsOf: text.utf8)
        case .array(let elements):
            writeHead(major: 4, argument: UInt64(elements.count))
            for element in elements { write(element) }
        case .map(let entries):
            writeHead(major: 5, argument: UInt64(entries.count))
            for entry in entries {
                write(entry.key)
                write(entry.value)
            }
        case .tag(let number, let item):
            writeHead(major: 6, argument: number)
            write(item)
        case .simple(let raw):
            if raw <= 23 {
                bytes.append(0xE0 | raw)
            } else {
                bytes.append(0xF8)
                bytes.append(raw)
            }
        case .bool(let flag):
            bytes.append(flag ? 0xF5 : 0xF4)
        case .null:
            bytes.append(0xF6)
        case .undefined:
            bytes.append(0xF7)
        case .float(let number):
            write(float: number)
        }
    }

    /// Write a float in the narrowest width that reproduces it exactly.
    mutating func write(float value: Double) {
        if let half = CBORHalf.bitPattern(of: value) {
            bytes.append(0xF9)
            appendBigEndian(half)
        } else if Double(Float(value)).bitPattern == value.bitPattern {
            bytes.append(0xFA)
            appendBigEndian(Float(value).bitPattern)
        } else {
            bytes.append(0xFB)
            appendBigEndian(value.bitPattern)
        }
    }

    /// Write a node of the tree the containers built.
    mutating func write(_ node: CBOREncodingNode) {
        switch node.kind {
        case .empty:
            // A slot a container opened and never filled reads as
            // absent, which is CBOR null on this wire.
            bytes.append(0xF6)
        case .value:
            write(node.value)
        case .array:
            write(elements: node.elements, ordering: node.ordering)
        case .map:
            write(entries: node.entries, of: node)
        }
    }

    /// Write an array node, sorting it when its order is not the
    /// value's own.
    private mutating func write(elements: [CBOREncodingNode], ordering: CBOREncodingNode.Ordering) {
        switch ordering {
        case .declared:
            writeHead(major: 4, argument: UInt64(elements.count))
            for element in elements { write(element) }
        case .canonicalElements:
            let encoded = elements.map { Self.encoded($0) }.sorted {
                $0.lexicographicallyPrecedes($1)
            }
            writeHead(major: 4, argument: UInt64(encoded.count))
            for element in encoded { bytes.append(contentsOf: element) }
        case .canonicalKeys:
            // A dictionary whose keys are neither strings nor integers
            // reaches an array of alternating keys and values.
            guard elements.count % 2 == 0 else {
                writeHead(major: 4, argument: UInt64(elements.count))
                for element in elements { write(element) }
                return
            }
            var pairs: [(key: [UInt8], value: [UInt8])] = []
            pairs.reserveCapacity(elements.count / 2)
            for index in stride(from: 0, to: elements.count, by: 2) {
                pairs.append(
                    (key: Self.encoded(elements[index]), value: Self.encoded(elements[index + 1]))
                )
            }
            pairs.sort { $0.key.lexicographicallyPrecedes($1.key) }
            writeHead(major: 4, argument: UInt64(elements.count))
            for pair in pairs {
                bytes.append(contentsOf: pair.key)
                bytes.append(contentsOf: pair.value)
            }
        }
    }

    /// Write a map node, sorting its entries when the keys carry no
    /// order of their own.
    private mutating func write(entries: [CBOREncodingNode.Entry], of node: CBOREncodingNode) {
        writeHead(major: 5, argument: UInt64(entries.count))
        guard node.ordering == .canonicalKeys else {
            for entry in entries {
                write(node.key(of: entry))
                write(entry.node)
            }
            return
        }
        let sorted = entries.map { entry -> (key: [UInt8], node: CBOREncodingNode) in
            var keyWriter = CBORWriter()
            keyWriter.write(node.key(of: entry))
            return (key: keyWriter.bytes, node: entry.node)
        }.sorted { $0.key.lexicographicallyPrecedes($1.key) }
        for entry in sorted {
            bytes.append(contentsOf: entry.key)
            write(entry.node)
        }
    }

    /// The bytes `node` encodes to on its own.
    private static func encoded(_ node: CBOREncodingNode) -> [UInt8] {
        var writer = CBORWriter()
        writer.write(node)
        return writer.bytes
    }
}

// MARK: - The tree the containers build

/// One position in the item being encoded.
///
/// The `Encoder` protocol hands out containers that stay live while
/// their siblings are written and lets a caller reserve a slot with
/// `superEncoder()` before filling it, so encoding builds this tree of
/// reference-typed nodes first and serializes it once the shape is
/// settled. Definite lengths need the counts anyway.
final class CBOREncodingNode {
    /// What a node holds.
    enum Kind {
        /// Nothing has been written here yet.
        case empty
        /// A complete item.
        case value
        /// A sequence of child nodes.
        case array
        /// A sequence of keyed child nodes.
        case map
    }

    /// Whether the order entries were added in is the order they are
    /// written in.
    enum Ordering {
        /// Keep the order the value produced, which is declaration
        /// order for a struct.
        case declared
        /// Sort map entries, or the pairs of an alternating array, by
        /// the encoded bytes of their keys.
        case canonicalKeys
        /// Sort array elements by their encoded bytes.
        case canonicalElements
    }

    /// How the keys of a map node reach the wire.
    enum KeySpelling {
        /// As text, which is what `serde` writes for a struct field
        /// and for a `String`-keyed map.
        case name
        /// As an integer, which is what `serde` writes for a map keyed
        /// by an integer.
        case index
    }

    /// One keyed child.
    struct Entry {
        /// The name of the key this child is filed under.
        var name: String
        /// The index of that key, for the keys that have one.
        var index: Int?
        /// The child itself.
        var node: CBOREncodingNode
    }

    /// Which of the payloads below is live.
    var kind: Kind = .empty
    /// The item, when ``kind`` is ``Kind/value``.
    var value: CBORValue = .null
    /// The children, when ``kind`` is ``Kind/array``.
    var elements: [CBOREncodingNode] = []
    /// The keyed children, when ``kind`` is ``Kind/map``.
    var entries: [Entry] = []
    /// How the children are ordered on the wire.
    var ordering: Ordering = .declared
    /// How the keys of the children reach the wire.
    var keySpelling: KeySpelling = .name

    /// An empty node.
    init() {}

    /// A node holding a complete item.
    init(value: CBORValue) {
        self.kind = .value
        self.value = value
    }

    /// Fill this node with a complete item.
    func set(_ item: CBORValue) {
        precondition(
            kind == .empty,
            "a single-value container encodes one item, and this one already holds another"
        )
        kind = .value
        value = item
    }

    /// Prepare this node to hold keyed children, or confirm that it
    /// already does.
    func openMap() {
        switch kind {
        case .empty: kind = .map
        case .map: break
        case .value, .array:
            preconditionFailure("a keyed container cannot be opened over an item already encoded")
        }
    }

    /// Prepare this node to hold children, or confirm that it already
    /// does.
    func openArray() {
        switch kind {
        case .empty: kind = .array
        case .array: break
        case .value, .map:
            preconditionFailure(
                "an unkeyed container cannot be opened over an item already encoded"
            )
        }
    }

    /// Take over what `other` holds.
    func adopt(_ other: CBOREncodingNode) {
        precondition(
            kind == .empty,
            "a single-value container encodes one item, and this one already holds another"
        )
        kind = other.kind
        value = other.value
        elements = other.elements
        entries = other.entries
        ordering = other.ordering
        keySpelling = other.keySpelling
    }

    /// The item the key of `entry` reaches the wire as.
    func key(of entry: Entry) -> CBORValue {
        if keySpelling == .index, let index = entry.index {
            return .integer(index)
        }
        return .textString(entry.name)
    }
}

// MARK: - Coding keys

/// A coding key the containers mint for positions the caller did not
/// name: the `super` slot, and the indices of an unkeyed container.
struct CBORCodingKey: CodingKey {
    var stringValue: String
    var intValue: Int?

    /// The key named `name`, with no index.
    init(name: String) {
        self.stringValue = name
        self.intValue = nil
    }

    init?(stringValue: String) {
        self.init(name: stringValue)
    }

    init?(intValue: Int) {
        self.stringValue = String(intValue)
        self.intValue = intValue
    }

    /// The key naming position `index` of an unkeyed container.
    init(index: Int) {
        self.stringValue = "Index \(index)"
        self.intValue = index
    }

    /// The key a `superEncoder()` writes under, and a `superDecoder()`
    /// reads from.
    static let superKey = CBORCodingKey(name: "super")
}

// MARK: - Encoder

/// The `Encoder` a value sees while it writes itself into one node.
final class CBOREncoderImpl: Encoder {
    /// The node this encoder fills.
    let node: CBOREncodingNode
    let codingPath: [any CodingKey]
    /// Context carried from ``CBOREncoder/userInfo``.
    let sendableUserInfo: [CodingUserInfoKey: any Sendable]

    var userInfo: [CodingUserInfoKey: Any] { sendableUserInfo }

    init(
        node: CBOREncodingNode,
        codingPath: [any CodingKey],
        userInfo: [CodingUserInfoKey: any Sendable]
    ) {
        self.node = node
        self.codingPath = codingPath
        self.sendableUserInfo = userInfo
    }

    /// Encode `value` into a node of its own.
    ///
    /// The two types the codec recognizes on sight are handled here:
    /// ``CBORValue``, which is already an item, and `Data`, which the
    /// wire spells as a byte string. Everything else writes itself
    /// through the containers, after which the node learns whether the
    /// value's order is its own.
    static func box<T: Encodable>(
        _ value: T,
        at codingPath: [any CodingKey],
        userInfo: [CodingUserInfoKey: any Sendable]
    ) throws -> CBOREncodingNode {
        if let item = value as? CBORValue {
            return CBOREncodingNode(value: item)
        }
        if let data = value as? Data {
            return CBOREncodingNode(value: .byteString([UInt8](data)))
        }
        let node = CBOREncodingNode()
        let encoder = CBOREncoderImpl(node: node, codingPath: codingPath, userInfo: userInfo)
        try value.encode(to: encoder)
        if let unordered = value as? any CBORUnorderedKeys {
            node.ordering = .canonicalKeys
            node.keySpelling = type(of: unordered).cborKeysAreIntegers ? .index : .name
        } else if value is any CBORUnorderedElements {
            node.ordering = .canonicalElements
        }
        return node
    }

    /// Encode `value` into a node of its own, carrying this encoder's
    /// context.
    func box<T: Encodable>(_ value: T, at codingPath: [any CodingKey]) throws -> CBOREncodingNode {
        try Self.box(value, at: codingPath, userInfo: sendableUserInfo)
    }

    func container<Key: CodingKey>(keyedBy type: Key.Type) -> KeyedEncodingContainer<Key> {
        node.openMap()
        return KeyedEncodingContainer(CBORKeyedEncodingContainer<Key>(encoder: self, node: node))
    }

    func unkeyedContainer() -> any UnkeyedEncodingContainer {
        node.openArray()
        return CBORUnkeyedEncodingContainer(encoder: self, node: node)
    }

    func singleValueContainer() -> any SingleValueEncodingContainer {
        CBORSingleValueEncodingContainer(encoder: self, node: node)
    }
}

// MARK: - Keyed container

/// Writes the entries of a CBOR map.
struct CBORKeyedEncodingContainer<Key: CodingKey>: KeyedEncodingContainerProtocol {
    let encoder: CBOREncoderImpl
    let node: CBOREncodingNode

    var codingPath: [any CodingKey] { encoder.codingPath }

    /// File `child` under `key`.
    private func append(_ child: CBOREncodingNode, forKey key: Key) {
        node.entries.append(
            CBOREncodingNode.Entry(name: key.stringValue, index: key.intValue, node: child)
        )
    }

    /// File a complete item under `key`.
    private func append(_ item: CBORValue, forKey key: Key) {
        append(CBOREncodingNode(value: item), forKey: key)
    }

    func encodeNil(forKey key: Key) throws { append(.null, forKey: key) }
    func encode(_ value: Bool, forKey key: Key) throws { append(.bool(value), forKey: key) }
    func encode(_ value: String, forKey key: Key) throws { append(.textString(value), forKey: key) }
    func encode(_ value: Double, forKey key: Key) throws { append(.float(value), forKey: key) }
    func encode(_ value: Float, forKey key: Key) throws {
        append(.float(Double(value)), forKey: key)
    }
    func encode(_ value: Int, forKey key: Key) throws { append(.integer(value), forKey: key) }
    func encode(_ value: Int8, forKey key: Key) throws { append(.integer(value), forKey: key) }
    func encode(_ value: Int16, forKey key: Key) throws { append(.integer(value), forKey: key) }
    func encode(_ value: Int32, forKey key: Key) throws { append(.integer(value), forKey: key) }
    func encode(_ value: Int64, forKey key: Key) throws { append(.integer(value), forKey: key) }
    func encode(_ value: UInt, forKey key: Key) throws { append(.integer(value), forKey: key) }
    func encode(_ value: UInt8, forKey key: Key) throws { append(.integer(value), forKey: key) }
    func encode(_ value: UInt16, forKey key: Key) throws { append(.integer(value), forKey: key) }
    func encode(_ value: UInt32, forKey key: Key) throws { append(.integer(value), forKey: key) }
    func encode(_ value: UInt64, forKey key: Key) throws { append(.integer(value), forKey: key) }

    func encode<T: Encodable>(_ value: T, forKey key: Key) throws {
        append(try encoder.box(value, at: codingPath + [key]), forKey: key)
    }

    func nestedContainer<NestedKey: CodingKey>(
        keyedBy keyType: NestedKey.Type,
        forKey key: Key
    ) -> KeyedEncodingContainer<NestedKey> {
        let child = CBOREncodingNode()
        child.openMap()
        append(child, forKey: key)
        let nested = CBOREncoderImpl(
            node: child,
            codingPath: codingPath + [key],
            userInfo: encoder.sendableUserInfo
        )
        return KeyedEncodingContainer(
            CBORKeyedEncodingContainer<NestedKey>(encoder: nested, node: child)
        )
    }

    func nestedUnkeyedContainer(forKey key: Key) -> any UnkeyedEncodingContainer {
        let child = CBOREncodingNode()
        child.openArray()
        append(child, forKey: key)
        let nested = CBOREncoderImpl(
            node: child,
            codingPath: codingPath + [key],
            userInfo: encoder.sendableUserInfo
        )
        return CBORUnkeyedEncodingContainer(encoder: nested, node: child)
    }

    func superEncoder() -> any Encoder {
        let child = CBOREncodingNode()
        let key = CBORCodingKey.superKey
        node.entries.append(
            CBOREncodingNode.Entry(name: key.stringValue, index: nil, node: child)
        )
        return CBOREncoderImpl(
            node: child,
            codingPath: codingPath + [key],
            userInfo: encoder.sendableUserInfo
        )
    }

    func superEncoder(forKey key: Key) -> any Encoder {
        let child = CBOREncodingNode()
        append(child, forKey: key)
        return CBOREncoderImpl(
            node: child,
            codingPath: codingPath + [key],
            userInfo: encoder.sendableUserInfo
        )
    }
}

// MARK: - Unkeyed container

/// Writes the elements of a CBOR array.
struct CBORUnkeyedEncodingContainer: UnkeyedEncodingContainer {
    let encoder: CBOREncoderImpl
    let node: CBOREncodingNode

    var codingPath: [any CodingKey] { encoder.codingPath }
    var count: Int { node.elements.count }

    /// The key naming the position about to be written.
    private var nextKey: CBORCodingKey { CBORCodingKey(index: node.elements.count) }

    /// Append a complete item.
    private func append(_ item: CBORValue) {
        node.elements.append(CBOREncodingNode(value: item))
    }

    func encodeNil() throws { append(.null) }
    func encode(_ value: Bool) throws { append(.bool(value)) }
    func encode(_ value: String) throws { append(.textString(value)) }
    func encode(_ value: Double) throws { append(.float(value)) }
    func encode(_ value: Float) throws { append(.float(Double(value))) }
    func encode(_ value: Int) throws { append(.integer(value)) }
    func encode(_ value: Int8) throws { append(.integer(value)) }
    func encode(_ value: Int16) throws { append(.integer(value)) }
    func encode(_ value: Int32) throws { append(.integer(value)) }
    func encode(_ value: Int64) throws { append(.integer(value)) }
    func encode(_ value: UInt) throws { append(.integer(value)) }
    func encode(_ value: UInt8) throws { append(.integer(value)) }
    func encode(_ value: UInt16) throws { append(.integer(value)) }
    func encode(_ value: UInt32) throws { append(.integer(value)) }
    func encode(_ value: UInt64) throws { append(.integer(value)) }

    func encode<T: Encodable>(_ value: T) throws {
        node.elements.append(try encoder.box(value, at: codingPath + [nextKey]))
    }

    func nestedContainer<NestedKey: CodingKey>(
        keyedBy keyType: NestedKey.Type
    ) -> KeyedEncodingContainer<NestedKey> {
        let key = nextKey
        let child = CBOREncodingNode()
        child.openMap()
        node.elements.append(child)
        let nested = CBOREncoderImpl(
            node: child,
            codingPath: codingPath + [key],
            userInfo: encoder.sendableUserInfo
        )
        return KeyedEncodingContainer(
            CBORKeyedEncodingContainer<NestedKey>(encoder: nested, node: child)
        )
    }

    func nestedUnkeyedContainer() -> any UnkeyedEncodingContainer {
        let key = nextKey
        let child = CBOREncodingNode()
        child.openArray()
        node.elements.append(child)
        let nested = CBOREncoderImpl(
            node: child,
            codingPath: codingPath + [key],
            userInfo: encoder.sendableUserInfo
        )
        return CBORUnkeyedEncodingContainer(encoder: nested, node: child)
    }

    func superEncoder() -> any Encoder {
        let key = nextKey
        let child = CBOREncodingNode()
        node.elements.append(child)
        return CBOREncoderImpl(
            node: child,
            codingPath: codingPath + [key],
            userInfo: encoder.sendableUserInfo
        )
    }
}

// MARK: - Single value container

/// Writes one CBOR item.
struct CBORSingleValueEncodingContainer: SingleValueEncodingContainer {
    let encoder: CBOREncoderImpl
    let node: CBOREncodingNode

    var codingPath: [any CodingKey] { encoder.codingPath }

    func encodeNil() throws { node.set(.null) }
    func encode(_ value: Bool) throws { node.set(.bool(value)) }
    func encode(_ value: String) throws { node.set(.textString(value)) }
    func encode(_ value: Double) throws { node.set(.float(value)) }
    func encode(_ value: Float) throws { node.set(.float(Double(value))) }
    func encode(_ value: Int) throws { node.set(.integer(value)) }
    func encode(_ value: Int8) throws { node.set(.integer(value)) }
    func encode(_ value: Int16) throws { node.set(.integer(value)) }
    func encode(_ value: Int32) throws { node.set(.integer(value)) }
    func encode(_ value: Int64) throws { node.set(.integer(value)) }
    func encode(_ value: UInt) throws { node.set(.integer(value)) }
    func encode(_ value: UInt8) throws { node.set(.integer(value)) }
    func encode(_ value: UInt16) throws { node.set(.integer(value)) }
    func encode(_ value: UInt32) throws { node.set(.integer(value)) }
    func encode(_ value: UInt64) throws { node.set(.integer(value)) }

    func encode<T: Encodable>(_ value: T) throws {
        node.adopt(try encoder.box(value, at: codingPath))
    }
}

// MARK: - Integers as items

extension CBORValue {
    /// The item `value` encodes to: major type 0 when it is
    /// non-negative, major type 1 holding `-1 - value` when it is not.
    static func integer<T: FixedWidthInteger>(_ value: T) -> CBORValue {
        value < 0
            ? .negative(UInt64(truncatingIfNeeded: ~value))
            : .unsigned(UInt64(truncatingIfNeeded: value))
    }
}
