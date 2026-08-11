import Foundation

// MARK: - Pairs

/// Two items the wire carries as a two-element CBOR array.
///
/// Rust reaches this shape from three directions: a tuple, a tuple
/// struct, and a map whose field carries `map_as_vec`, which turns a
/// `HashMap<K, V>` into a `Vec<(K, V)>` so that a key too structured to
/// sit in a CBOR map key still has somewhere to go. All three arrive as
/// an array of exactly two items, which is what this type reads and
/// writes.
///
/// Equality, hashing, and ordering follow the elements, so a pair is
/// usable as a dictionary key wherever both of its halves are.
public struct WirePair<Key: Codable & Sendable, Value: Codable & Sendable>: Codable, Sendable {
    /// The first item of the array.
    public var key: Key
    /// The second item of the array.
    public var value: Value

    /// Pair `key` with `value`.
    public init(_ key: Key, _ value: Value) {
        self.key = key
        self.value = value
    }

    /// Read the two items positionally.
    public init(from decoder: any Decoder) throws {
        var container = try decoder.unkeyedContainer()
        self.key = try container.decode(Key.self)
        self.value = try container.decode(Value.self)
    }

    /// Write the two items positionally.
    public func encode(to encoder: any Encoder) throws {
        var container = encoder.unkeyedContainer()
        try container.encode(key)
        try container.encode(value)
    }
}

extension WirePair: Equatable where Key: Equatable, Value: Equatable {}

extension WirePair: Hashable where Key: Hashable, Value: Hashable {}

extension WirePair: Comparable where Key: Comparable, Value: Comparable {
    /// Order pairs by their key, and by their value when the keys agree.
    public static func < (lhs: WirePair, rhs: WirePair) -> Bool {
        lhs.key == rhs.key ? lhs.value < rhs.value : lhs.key < rhs.key
    }
}

/// Three items the wire carries as a three-element CBOR array.
///
/// A Rust tuple of arity three, such as the `(parent, child, edge)`
/// arcs of an instance, encodes as a definite-length array holding the
/// three serialized items in order.
public struct WireTriple<
    First: Codable & Sendable,
    Second: Codable & Sendable,
    Third: Codable & Sendable
>: Codable, Sendable {
    /// The first item of the array.
    public var first: First
    /// The second item of the array.
    public var second: Second
    /// The third item of the array.
    public var third: Third

    /// Group `first`, `second`, and `third` in that order.
    public init(_ first: First, _ second: Second, _ third: Third) {
        self.first = first
        self.second = second
        self.third = third
    }

    /// Read the three items positionally.
    public init(from decoder: any Decoder) throws {
        var container = try decoder.unkeyedContainer()
        self.first = try container.decode(First.self)
        self.second = try container.decode(Second.self)
        self.third = try container.decode(Third.self)
    }

    /// Write the three items positionally.
    public func encode(to encoder: any Encoder) throws {
        var container = encoder.unkeyedContainer()
        try container.encode(first)
        try container.encode(second)
        try container.encode(third)
    }
}

extension WireTriple: Equatable where First: Equatable, Second: Equatable, Third: Equatable {}

extension WireTriple: Hashable where First: Hashable, Second: Hashable, Third: Hashable {}

// MARK: - Maps keyed by an integer

/// A map the wire keys with unsigned integers.
///
/// A Rust `HashMap<u32, V>` encodes as a CBOR map whose keys are major
/// type 0 items. Swift's `[UInt32: V]` does not: the standard library
/// writes a dictionary as a map only when its key is `String` or `Int`,
/// and writes every other key type as an array of alternating keys and
/// values. This wrapper carries the `UInt32` keys a host wants and
/// codes through the integer-keyed spelling the engine wrote.
public struct UInt32KeyedMap<Value: Codable & Sendable>: Codable, Sendable {
    /// The entries, keyed by the integer the wire carries.
    public var entries: [UInt32: Value]

    /// Hold `entries`.
    public init(_ entries: [UInt32: Value] = [:]) {
        self.entries = entries
    }

    /// The value filed under `key`, if there is one.
    public subscript(key: UInt32) -> Value? {
        get { entries[key] }
        set { entries[key] = newValue }
    }

    /// Read a CBOR map with integer keys.
    ///
    /// - Throws: `DecodingError.dataCorrupted` when a key falls outside
    ///   the range of a `UInt32`, which no engine payload produces and
    ///   which would otherwise be silently truncated.
    public init(from decoder: any Decoder) throws {
        let container = try decoder.singleValueContainer()
        let wide = try container.decode([Int: Value].self)
        var entries: [UInt32: Value] = [:]
        entries.reserveCapacity(wide.count)
        for (key, value) in wide {
            guard let narrow = UInt32(exactly: key) else {
                throw DecodingError.dataCorrupted(
                    DecodingError.Context(
                        codingPath: decoder.codingPath,
                        debugDescription: "map key \(key) does not fit in a UInt32"
                    )
                )
            }
            entries[narrow] = value
        }
        self.entries = entries
    }

    /// Write a CBOR map with integer keys, ordered by key.
    public func encode(to encoder: any Encoder) throws {
        var wide: [Int: Value] = [:]
        wide.reserveCapacity(entries.count)
        for (key, value) in entries { wide[Int(key)] = value }
        var container = encoder.singleValueContainer()
        try container.encode(wide)
    }
}

extension UInt32KeyedMap: Equatable where Value: Equatable {}

extension UInt32KeyedMap: Hashable where Value: Hashable {}

extension UInt32KeyedMap: ExpressibleByDictionaryLiteral {
    /// Build a map from `elements`, the last occurrence of a repeated
    /// key winning, which is how the engine's own collect behaves.
    public init(dictionaryLiteral elements: (UInt32, Value)...) {
        self.init([UInt32: Value](elements, uniquingKeysWith: { _, latest in latest }))
    }
}

// MARK: - Maps carried as pair arrays

/// Conversions between a Swift dictionary and the array of pairs the
/// wire carries it as.
///
/// A Rust map whose key is not a string reaches the wire through
/// `map_as_vec`, which serializes `map.iter().collect::<Vec<_>>()`: an
/// array of two-element arrays. Reading is the inverse collect, so a
/// repeated key keeps its last occurrence.
///
/// The engine writes those pairs in hash order, which varies from run
/// to run. These helpers impose an order on the way out, so a value
/// encodes to the same bytes every time without changing what it
/// denotes.
///
/// The order is the CBOR encoding of the key, compared byte by byte.
/// That reaches every key shape the wire carries, including the ones no
/// `Comparable` conformance covers, such as an edge or a name paired
/// with a list of names, so one rule settles every pair array in the
/// module.
public enum WireMap {
    /// The entries of `map` as pairs, ordered by the encoded key.
    ///
    /// - Throws: `EncodingError` when a key declines to encode.
    public static func pairs<Key, Value>(
        of map: [Key: Value]
    ) throws -> [WirePair<Key, Value>]
    where Key: Codable & Hashable & Sendable, Value: Codable & Sendable {
        let encoder = CBOREncoder()
        var ranked: [(bytes: Data, pair: WirePair<Key, Value>)] = []
        ranked.reserveCapacity(map.count)
        for (key, value) in map {
            ranked.append((try encoder.encode(key), WirePair(key, value)))
        }
        ranked.sort { $0.bytes.lexicographicallyPrecedes($1.bytes) }
        return ranked.map(\.pair)
    }

    /// The map `pairs` denotes, the last occurrence of a repeated key
    /// winning.
    public static func dictionary<Key, Value>(
        from pairs: [WirePair<Key, Value>]
    ) -> [Key: Value] where Key: Codable & Hashable & Sendable, Value: Codable & Sendable {
        [Key: Value](pairs.map { ($0.key, $0.value) }, uniquingKeysWith: { _, latest in latest })
    }

    /// `entries` ordered by the encoded key, which is what a map built
    /// as ``CBORValue`` entries needs to encode reproducibly.
    public static func entries(_ entries: [CBORValue.Entry]) -> [CBORValue.Entry] {
        entries.sorted { $0.key.encodedBytes().lexicographicallyPrecedes($1.key.encodedBytes()) }
    }
}

// MARK: - Variant keys

/// A coding key that names any wire key.
///
/// An externally tagged Rust enum arrives as a one-entry map whose key
/// is the variant name, and the payload under it is a map whose keys are
/// the field names. Neither set of names is fixed at compile time from
/// the container's point of view, so both are addressed through this.
struct VariantKey: CodingKey {
    /// The name this key spells.
    var stringValue: String
    /// Nothing: a variant key is never an array index.
    var intValue: Int? { nil }

    /// A key spelling `stringValue`.
    init(_ stringValue: String) {
        self.stringValue = stringValue
    }

    /// A key spelling `stringValue`, which always succeeds.
    init?(stringValue: String) {
        self.init(stringValue)
    }

    /// Nothing: a variant key is never an array index.
    init?(intValue: Int) {
        nil
    }
}

extension KeyedEncodingContainer where Key == VariantKey {
    /// Open the field map of the variant `tag` names.
    mutating func fields(
        _ tag: some RawRepresentable<String>
    ) -> KeyedEncodingContainer<VariantKey> {
        nestedContainer(keyedBy: VariantKey.self, forKey: VariantKey(tag.rawValue))
    }
}
