import PanprotoStructural

// The built-in protocol catalogue as a stream.
//
// The catalogue arrives in two pieces. `pp_registry_list_builtin` writes
// the names, all of them at once, and `pp_registry_get_builtin` resolves
// one name to one specification. A caller that means to search the
// catalogue, rather than to name a protocol it already knows, therefore
// writes the second call once per name and pays for every specification
// whether or not it looks at it.
//
// ``BuiltinProtocolCatalogue`` is that loop, inverted. The names are
// read once and the specifications are resolved a page at a time, so a
// search that ends at the third protocol has decoded a page of
// specifications rather than the whole registry.
//
// The page is the unit of engine work rather than the element: one
// lookup is a slab-free registry read and a decode of a few kilobytes,
// which is small enough that hopping onto the engine for each one would
// be paying more for the hop than for the work.

/// One entry of the built-in protocol catalogue: the name a protocol is
/// registered under, and the specification that name resolves to.
///
/// The name is what ``ProtocolHandle/builtin(_:)`` and
/// ``ProtocolHandle/builtinSpecification(named:)`` accept, so an entry
/// that matches whatever a search was looking for can be turned into a
/// slab entry without another lookup.
public struct BuiltinProtocol: Hashable, Sendable {
    /// The catalogue name, such as `atproto` or `sql`.
    public var name: String
    /// The protocol that name resolves to.
    public var specification: ProtocolSpec

    /// Pair a catalogue name with the specification it resolves to.
    ///
    /// - Parameters:
    ///   - name: the catalogue name.
    ///   - specification: the protocol that name resolves to.
    public init(name: String, specification: ProtocolSpec) {
        self.name = name
        self.specification = specification
    }
}

/// Built-in protocols resolved to their specifications, a page at a
/// time.
///
/// This is the search surface over the catalogue that
/// ``ProtocolHandle/builtinNames()`` and
/// ``ProtocolHandle/builtinSpecification(named:)`` leave to the caller:
///
/// ```swift
/// var carryingRecords: [String] = []
/// for try await entry in ProtocolHandle.builtinCatalogue() {
///     guard entry.specification.objKinds.contains("record") else { continue }
///     carryingRecords.append(entry.name)
/// }
/// ```
///
/// Both array-returning methods remain. Reach for them where the answer
/// is one protocol, or where the names alone are the answer; reach for
/// this where the answer is whichever protocols satisfy something only
/// their specifications can say.
public struct BuiltinProtocolCatalogue: AsyncSequence, Sendable {
    /// One protocol, named and resolved.
    public typealias Element = BuiltinProtocol

    /// The names to resolve, or nil to resolve the whole catalogue.
    ///
    /// A nil listing is read from the registry on the first page, in
    /// the same engine call that resolves that page.
    public let names: [String]?

    /// How many protocols each page resolves before yielding the first
    /// of them.
    ///
    /// One page is one hop onto the engine and that many registry
    /// lookups. A value below one is read as one.
    public let pageSize: Int

    /// Stream `names`, or the whole catalogue when they are nil.
    ///
    /// - Parameters:
    ///   - names: the catalogue names to resolve, or nil for all of
    ///     them.
    ///   - pageSize: how many protocols each page resolves.
    init(names: [String]?, pageSize: Int) {
        self.names = names
        self.pageSize = Swift.max(1, pageSize)
    }

    /// Start a walk at the first name.
    ///
    /// Nothing has been resolved yet, and a nil ``names`` has not been
    /// read: the first page does both.
    ///
    /// - Returns: an iterator positioned before the first protocol.
    public func makeAsyncIterator() -> Iterator {
        Iterator(names: names, pageSize: pageSize)
    }

    /// A walk in progress: the resolved page in hand, and how far
    /// through the names it has reached.
    public struct Iterator: AsyncIteratorProtocol {
        /// One protocol, named and resolved.
        public typealias Element = BuiltinProtocol

        /// The names to resolve, once they are known.
        private var names: [String]?
        /// How many protocols a page resolves.
        private let pageSize: Int
        /// The protocols this page resolved.
        private var page: [BuiltinProtocol] = []
        /// How far into ``page`` the walk has read.
        private var position = 0
        /// How far into ``names`` the pages have reached.
        private var cursor = 0
        /// Whether every name has been resolved.
        private var isFinished = false

        /// Start a walk over `names`, resolving `pageSize` at a time.
        ///
        /// - Parameters:
        ///   - names: the catalogue names to resolve, or nil for all of
        ///     them.
        ///   - pageSize: how many protocols each page resolves.
        init(names: [String]?, pageSize: Int) {
            self.names = names
            self.pageSize = Swift.max(1, pageSize)
        }

        /// The next protocol, resolving a page when the one in hand runs
        /// out.
        ///
        /// Cancellation is observed here, between protocols, and never
        /// inside the call that resolves a page: the engine has no
        /// cancellation channel, so a page that has started is a page
        /// that finishes.
        ///
        /// The clause is untyped because a cancelled walk fails as
        /// `CancellationError`, which is not an engine failure.
        /// Everything raised on the engine's behalf is a
        /// ``PanprotoError/io(_:)``, the same error
        /// ``ProtocolHandle/builtinSpecification(named:)`` raises.
        ///
        /// - Returns: the next protocol, or nil once every name is
        ///   resolved.
        /// - Throws: `CancellationError` when the task is cancelled, and
        ///   ``PanprotoError/io(_:)`` when the catalogue will not list
        ///   or a name will not resolve.
        public mutating func next() async throws -> BuiltinProtocol? {
            try Task.checkCancellation()
            while position == page.count {
                guard !isFinished else { return nil }
                try await readPage()
            }
            let entry = page[position]
            position += 1
            return entry
        }

        /// Resolve the next `pageSize` names, listing the catalogue
        /// first where the names are not yet known.
        ///
        /// - Throws: ``PanprotoError/io(_:)`` when the catalogue will
        ///   not list, or when a name resolves to nothing.
        private mutating func readPage() async throws {
            let known = names
            let size = pageSize
            let start = cursor
            let resolved = try await PanprotoEngine.run {
                () throws(PanprotoError) -> (names: [String], page: [BuiltinProtocol]) in
                let all: [String]
                if let known {
                    all = known
                } else {
                    all = try ProtocolHandle.builtinNames()
                }
                let batch = all.dropFirst(start).prefix(size)
                var page: [BuiltinProtocol] = []
                page.reserveCapacity(batch.count)
                for name in batch {
                    page.append(
                        BuiltinProtocol(
                            name: name,
                            specification: try ProtocolHandle.builtinSpecification(named: name)
                        )
                    )
                }
                return (all, page)
            }

            names = resolved.names
            page = resolved.page
            position = 0
            cursor += resolved.page.count
            if cursor >= resolved.names.count { isFinished = true }
        }
    }
}

// MARK: - Reaching the catalogue as a stream

extension ProtocolHandle {
    /// Every built-in protocol, resolved to its specification a page at
    /// a time.
    ///
    /// The catalogue spans annotation formats, API description
    /// languages, configuration schemas, data schemas, databases,
    /// serialization formats, and web document formats, and it is the
    /// one ``builtinNames()`` lists. Streaming it is what makes a search
    /// over the specifications cost the protocols it looked at rather
    /// than the whole registry:
    ///
    /// ```swift
    /// var ordered: [String] = []
    /// for try await entry in ProtocolHandle.builtinCatalogue() {
    ///     if entry.specification.hasOrder { ordered.append(entry.name) }
    /// }
    /// ```
    ///
    /// - Parameter pageSize: how many protocols each page resolves
    ///   before yielding the first of them. A value below one is read as
    ///   one.
    /// - Returns: the catalogue, in the engine's own order.
    public nonisolated static func builtinCatalogue(
        pageSize: Int = 8
    ) -> BuiltinProtocolCatalogue {
        BuiltinProtocolCatalogue(names: nil, pageSize: pageSize)
    }

    /// The named built-in protocols, resolved a page at a time.
    ///
    /// This is ``builtinCatalogue(pageSize:)`` over a listing the caller
    /// supplies rather than the whole registry, which is what a host
    /// reading protocol names out of a manifest wants: the names it did
    /// not reach are never looked up, so a name the catalogue does not
    /// carry fails only once the walk arrives at it.
    ///
    /// - Parameters:
    ///   - names: the catalogue names to resolve, in the order to
    ///     resolve them.
    ///   - pageSize: how many protocols each page resolves before
    ///     yielding the first of them. A value below one is read as one.
    /// - Returns: the named protocols, in the order given.
    public nonisolated static func builtinSpecifications(
        named names: [String],
        pageSize: Int = 8
    ) -> BuiltinProtocolCatalogue {
        BuiltinProtocolCatalogue(names: names, pageSize: pageSize)
    }
}
