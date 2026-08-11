import Foundation
import PanprotoFFI
import PanprotoStructural

// The migration domain: the eight `pp_mig_*` entry points, and the
// builder that assembles the mapping they take.
//
// A migration crosses the boundary twice in two different forms. Going
// in it is a ``Migration``, a plain mapping between two schemas that the
// engine reads as CBOR; coming back out of ``Migration/compile(from:to:)``
// it is a slab handle holding the compiled form, which is what records
// are actually lifted through. Everything below is one of those two
// shapes.

// MARK: - Building a migration

/// Accumulates the parts of a `Migration` one entry at a time.
///
/// A mapping between two schemas is three tables: where each source
/// vertex goes, where each source edge goes, and which target edge a
/// contracted parent and child resolve to. The builder writes those
/// three and nothing else, so what it produces is a mapping and carries
/// no schema identity of its own.
///
/// ```swift
/// var builder = MigrationBuilder()
/// builder.mapVertex("app.bsky.feed.post", to: "app.bsky.feed.post")
/// builder.mapEdge(text, to: text)
/// let compiled = try await builder.build().compile(from: source, to: target)
/// ```
///
/// Survival follows from being mapped, so the mapping a fresh builder
/// starts from drops every record it reaches.
///
/// For the four edits that come up most, the mapping is already
/// written: `Migration.addingField(to:named:kind:)`,
/// `Migration.removingField(_:)`,
/// `Migration.renamingField(on:field:from:to:)`, and
/// `Migration.hoistingField(on:through:to:)` build one each, and
/// `Migration.pipeline(_:)` chains them.
public struct MigrationBuilder: Sendable {
    /// The mapping built so far.
    private var migration: Migration

    /// Start from the mapping that sends nothing anywhere.
    public init() {
        self.migration = Migration()
    }

    /// Start from `migration` and add to it.
    ///
    /// This is the way to amend a self-map from
    /// `Migration.identity(on:)` or an inverse from
    /// `Migration.inverted(from:to:)` rather than rebuild it.
    public init(extending migration: Migration) {
        self.migration = migration
    }

    /// Send the source vertex `source` to the target vertex `target`.
    ///
    /// A second entry for the same source vertex replaces the first.
    /// Compiling checks `target` against the target schema and refuses a
    /// mapping that names a vertex the target schema does not carry.
    public mutating func mapVertex(_ source: Name, to target: Name) {
        migration.vertexMap[source] = target
    }

    /// Send the source edge `source` to the target edge `target`.
    ///
    /// Compiling checks that the two edge maps agree with the vertex
    /// map: `target` has to run between the images this mapping gives
    /// `source.src` and `source.tgt`. A crossed pair is rejected before
    /// any per-record work is compiled.
    public mutating func mapEdge(_ source: Edge, to target: Edge) {
        migration.edgeMap[source] = target
    }

    /// Settle which target edge a contracted arc takes.
    ///
    /// Dropping an intermediate vertex leaves a parent and a child
    /// adjacent that were not adjacent before, and the target schema may
    /// hold several edges joining them. The two names are the parent and
    /// the child as they stand in the target schema after remapping, and
    /// `edge` is the edge the pair resolves to. Where exactly one target
    /// edge joins the pair the engine finds it unaided; where none or
    /// several do, lifting fails without an entry here.
    public mutating func resolve(from parent: Name, to child: Name, with edge: Edge) {
        migration.resolver[WirePair(parent, child)] = edge
    }

    /// The mapping built so far.
    public func build() -> Migration {
        migration
    }
}

// MARK: - The mapping

extension Migration {
    /// Check this mapping against the conditions a migration from
    /// `source` to `target` has to satisfy.
    ///
    /// A mapping that fails the conditions is not a failure of the
    /// call: the engine answers with a report whose
    /// `ExistenceReport.valid` is false and whose
    /// `ExistenceReport.errors` name the obligations that went unmet. A
    /// thrown error means the check could not run, which is a handle of
    /// the wrong slab variant, a protocol whose theories are not
    /// registered, or a payload that would not encode.
    ///
    /// Which obligations apply comes from `protocolHandle`: the engine
    /// builds a theory registry from the protocol's name and derives the
    /// conditions from the sorts its schema and instance theories carry,
    /// so a mapping between schemas of a small protocol answers to fewer
    /// of them than one between schemas of a large protocol.
    ///
    /// - Throws: ``PanprotoError/existenceCheck(_:)``.
    @PanprotoEngine
    public func checkExistence(
        against protocolHandle: ProtocolHandle,
        from source: SchemaHandle,
        to target: SchemaHandle
    ) throws(PanprotoError) -> ExistenceReport {
        let operation = "Migration.checkExistence"
        let mapping = try Payload.encode(self, .existenceCheck, operation)
        let answer = Raw.migCheckExistence(
            proto: protocolHandle.rawValue,
            src: source.rawValue,
            tgt: target.rawValue,
            mapping: mapping
        )
        try answer.status.orThrow(.existenceCheck, operation)
        return try Payload.decode(
            ExistenceReport.self, from: answer.bytes, .existenceCheck, operation)
    }

    /// Compile this mapping against `source` and `target` into the form
    /// records are lifted through.
    ///
    /// Compiling settles up front everything a per-record application
    /// needs: which target vertices and edges survive, the two remapping
    /// tables, the resolver, and the value-level assignments the source
    /// schema's coercions imply. It also rejects a mapping that is not a
    /// morphism, meaning one carrying an edge that does not run between
    /// the images of its own endpoints.
    ///
    /// The handle keeps `source` and `target` with it, which is what
    /// lets ``MigrationCarrying/lift(_:)`` take a record and no schemas.
    ///
    /// - Throws: ``PanprotoError/migration(_:)``.
    @PanprotoEngine
    public func compile(
        from source: SchemaHandle,
        to target: SchemaHandle
    ) throws(PanprotoError) -> CompiledMigrationHandle {
        let operation = "Migration.compile"
        let mapping = try Payload.encode(self, .migration, operation)
        let answer = Raw.migCompile(
            src: source.rawValue,
            tgt: target.rawValue,
            mapping: mapping
        )
        try answer.status.orThrow(.migration, operation)
        return CompiledMigrationHandle(adopting: answer.handle)
    }

    /// The mapping that undoes this one.
    ///
    /// Inversion needs a bijection. The vertex, edge, and hyper-edge maps
    /// each have to be injective and to cover every element of `target`,
    /// so a mapping that merges two vertices, or that leaves any part of
    /// the target without a preimage, has no inverse and this throws. The
    /// inverse comes back without expression resolvers and without schema
    /// identifiers: the engine rebuilds it from the structural tables
    /// alone.
    ///
    /// - Throws: ``PanprotoError/migration(_:)``.
    @PanprotoEngine
    public func inverted(
        from source: SchemaHandle,
        to target: SchemaHandle
    ) throws(PanprotoError) -> Migration {
        let operation = "Migration.inverted"
        let mapping = try Payload.encode(self, .migration, operation)
        let answer = Raw.migInvert(
            mapping: mapping,
            src: source.rawValue,
            tgt: target.rawValue
        )
        try answer.status.orThrow(.migration, operation)
        return try Payload.decode(Migration.self, from: answer.bytes, .migration, operation)
    }
}

// MARK: - Handles holding a compiled migration

/// A slab handle the migration entry points accept.
///
/// Two slab variants hold a compiled migration, and they differ in what
/// they are anchored to. ``CompiledMigrationHandle`` keeps the source and
/// target schemas it was compiled against. ``MigrationHandle`` keeps the
/// compiled payload alone, and the engine reconstructs a minimal schema
/// from its surviving vertex and edge sets whenever an operation needs
/// one; the vertices come back with kind `unknown` and no nsid, which is
/// enough to lift a record through and not enough to validate one
/// against.
///
/// The protocol has no requirements of its own. It exists so the five
/// operations that accept either variant are written once and reach
/// both.
public protocol MigrationCarrying: PanprotoHandle {}

/// A compiled migration anchored to the schemas it was compiled against
/// accepts every migration operation.
extension CompiledMigrationHandle: MigrationCarrying {}

/// A migration anchored to nothing accepts every migration operation,
/// against the minimal schemas the engine reconstructs for it.
extension MigrationHandle: MigrationCarrying {}

extension MigrationCarrying {
    /// The compiled payload this handle holds.
    ///
    /// This is the payload alone: the schemas a
    /// ``CompiledMigrationHandle`` is anchored to do not travel with it.
    /// These are also the bytes the lens graph's fiber calculus consumes,
    /// so a caller reaching for a fiber decomposition encodes this value
    /// and hands the result over.
    ///
    /// - Throws: ``PanprotoError/migration(_:)``.
    @PanprotoEngine
    public func compiledMigration() throws(PanprotoError) -> CompiledMigration {
        let operation = "\(Self.self).compiledMigration"
        let answer = Raw.migSerializeCompiled(migHandle: rawValue)
        try answer.status.orThrow(.migration, operation)
        return try Payload.decode(CompiledMigration.self, from: answer.bytes, .migration, operation)
    }

    /// Carry one record through this migration.
    ///
    /// A node survives when its anchor is remapped, or when its anchor
    /// already names a vertex that survives; everything else is pruned.
    /// An arc whose endpoints both survive is re-resolved against the
    /// target schema, through the resolver where one applies and through
    /// the unique joining edge otherwise. A lift that prunes the root
    /// fails rather than answering with an empty record.
    ///
    /// - Throws: ``PanprotoError/migration(_:)``.
    @PanprotoEngine
    public func lift(_ record: Instance) throws(PanprotoError) -> Instance {
        let operation = "\(Self.self).lift(_:)"
        let payload = try Payload.encode(record, .migration, operation)
        let answer = Raw.migLiftRecord(migration: rawValue, record: payload)
        try answer.status.orThrow(.migration, operation)
        return try Payload.decode(Instance.self, from: answer.bytes, .migration, operation)
    }

    /// Carry one JSON record through this migration, answering with
    /// JSON.
    ///
    /// The bytes are JSON on both sides rather than CBOR, which is why
    /// this takes and returns `Data`: the engine parses them against the
    /// source schema, lifts the instance that comes out, and writes the
    /// result against the target schema.
    ///
    /// `rootVertex` names the source vertex the top-level object is
    /// anchored to. Leaving it nil auto-detects, first trying a vertex
    /// named after the source schema's protocol and then the schema's
    /// declared primary entry. A bare ``MigrationHandle`` declares
    /// neither, so a lift through one needs the vertex named.
    ///
    /// - Throws: ``PanprotoError/migration(_:)``.
    @PanprotoEngine
    public func lift(json: Data, rootVertex: Name? = nil) throws(PanprotoError) -> Data {
        let operation = "\(Self.self).lift(json:)"
        let answer = Raw.migLiftJson(
            migration: rawValue,
            json: json,
            rootVertex: rootVertex ?? ""
        )
        try answer.status.orThrow(.migration, operation)
        return answer.bytes
    }

    /// Lift `instances` without keeping the results, and report how many
    /// arrived.
    ///
    /// This is the dry run: it says what share of a data set the
    /// migration carries before anything is committed to it. The report
    /// keeps the first twenty failures, each naming the position of the
    /// record that raised it, and counts the vertices of both schemas so
    /// a low share can be read against how much structure was in play.
    /// An empty batch counts as full coverage.
    ///
    /// `source` and `target` are supplied here rather than taken from
    /// the handle, so a bare ``MigrationHandle`` runs against real
    /// schemas instead of the reconstructed minimal ones.
    ///
    /// - Throws: ``PanprotoError/migration(_:)``.
    @PanprotoEngine
    public func coverage(
        over instances: [Instance],
        from source: SchemaHandle,
        to target: SchemaHandle
    ) throws(PanprotoError) -> CoverageReport {
        let operation = "\(Self.self).coverage"
        let payload = try Payload.encode(instances, .migration, operation)
        let answer = Raw.migCoverage(
            migration: rawValue,
            src: source.rawValue,
            tgt: target.rawValue,
            instances: payload
        )
        try answer.status.orThrow(.migration, operation)
        return try Payload.decode(CoverageReport.self, from: answer.bytes, .migration, operation)
    }

    /// Run this migration and then `next`, as one migration.
    ///
    /// Composition is drop-on-miss. A vertex this migration sends to some
    /// intermediate vertex keeps its place in the composite only where
    /// `next` maps that intermediate vertex onward, or lists it among the
    /// vertices that survive `next`; otherwise the composite sends that
    /// vertex nowhere, which is to say it drops it. Edges compose the
    /// same way.
    ///
    /// Drop-on-miss is also why there is no `Migration` value standing
    /// as a unit here. The only identity on the right is the self-map
    /// of the schema in the middle, and a self-map is a fact about one
    /// schema rather than a value that can be written down once: the
    /// empty mapping, the one candidate needing no schema, maps nothing
    /// and so drops everything composed into it.
    /// `Migration.identity(on:)` builds the self-map where a schema is
    /// at hand. A monoid-shaped protocol would need the unit that does
    /// not exist, so `Migration` conforms to none, and the `+` on the
    /// value type is a semigroup operation rather than a monoid one.
    ///
    /// The composite is structural. The value-level work either side
    /// carries, meaning field transforms, conditional survival, term
    /// assignments, and expansion paths, is not composed and does not
    /// appear in the result; a composite of two migrations that coerce
    /// values moves the structure and leaves the coercions behind.
    ///
    /// The result is a bare ``MigrationHandle``, anchored to no
    /// schemas. Composing the two *specifications* instead, which keeps
    /// a `Migration` to check, invert, or compile, is
    /// `Migration.composed(with:)`.
    ///
    /// - Throws: ``PanprotoError/migration(_:)``.
    @PanprotoEngine
    public func composed(
        with next: some MigrationCarrying
    ) throws(PanprotoError) -> MigrationHandle {
        let operation = "\(Self.self).composed"
        let answer = Raw.migCompose(m1: rawValue, m2: next.rawValue)
        try answer.status.orThrow(.migration, operation)
        return MigrationHandle(adopting: answer.handle)
    }
}
