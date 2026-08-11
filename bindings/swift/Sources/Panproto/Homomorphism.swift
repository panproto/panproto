import Foundation
import PanprotoFFI
import PanprotoStructural

// The homomorphism domain: searching for structure-preserving maps
// between two schemas, and pushing a theory morphism down onto the
// schemas that model the theory.
//
// Two ways of getting a migration meet here. A search asks what maps
// exist between two schemas the host already has, ranks the answers,
// and lowers one of them; the cascade starts a level up, from a morphism
// between the theories, and derives the schema-level map from it. Both
// end at a handle the migration and lens surfaces take.
//
// Every failure in this file reports ``PanprotoError/migration(_:)``:
// the search lowers through `mig::hom_search`, the cascade through
// `mig::cascade`, and both compile through `mig::compile`.

// MARK: - Searching for morphisms

extension SchemaHandle {
    /// Find every structure-preserving morphism from this schema into
    /// `target`.
    ///
    /// A morphism sends each vertex of this schema to a vertex of
    /// `target` and each edge to an edge, so that sources and targets
    /// agree. The search is a constraint problem over those assignments,
    /// and `options` is where its shape is fixed: whether the vertex map
    /// has to be injective, surjective, or both, how many answers to
    /// stop at, and which assignments to pin before the search starts.
    ///
    /// Results arrive ranked by descending `FoundMorphism.quality`, so
    /// the first is the one ``findBestMorphism(to:options:)`` would
    /// return. An empty array means no assignment satisfied the
    /// constraints, which is an ordinary answer rather than a failure.
    ///
    /// The search enumerates assignments, so its cost climbs steeply
    /// with the number of vertices on either side; a schema of a few
    /// dozen vertices is enough to make an unconstrained search long.
    /// Pinning vertices through `MorphismSearchOptions.initial`, or
    /// requiring a shape the engine can prune against, cuts the space
    /// before the enumeration starts.
    ///
    /// ```swift
    /// let candidates = try await post.findMorphisms(
    ///     to: profile,
    ///     options: MorphismSearchOptions(monic: true, maxResults: 8)
    /// )
    /// ```
    ///
    /// - Parameters:
    ///   - target: The schema to map into.
    ///   - options: The constraints the search runs under. The default
    ///     is the engine's own: no shape requirement, no result limit,
    ///     nothing pinned.
    /// - Returns: The morphisms found, best first.
    /// - Throws: ``PanprotoError/migration(_:)`` when either handle is
    ///   not a schema, or when the options will not encode.
    @PanprotoEngine
    public func findMorphisms(
        to target: SchemaHandle,
        options: MorphismSearchOptions = MorphismSearchOptions()
    ) throws(PanprotoError) -> [FoundMorphism] {
        let operation = "SchemaHandle.findMorphisms"
        let opts = try Payload.encode(options, .migration, operation)
        let result = Raw.homFindMorphisms(src: rawValue, tgt: target.rawValue, opts: opts)
        try result.status.orThrow(.migration, operation)
        return try Payload.decode(
            [FoundMorphism].self, from: result.bytes, .migration, operation)
    }

    /// Find the single highest-quality morphism from this schema into
    /// `target`.
    ///
    /// The search is the same one ``findMorphisms(to:options:)`` runs,
    /// under the same `options`; only the answer is narrowed. `nil`
    /// means the constraints admit no morphism at all.
    ///
    /// - Parameters:
    ///   - target: The schema to map into.
    ///   - options: The constraints the search runs under.
    /// - Returns: The best morphism, or `nil` when none exists.
    /// - Throws: ``PanprotoError/migration(_:)`` when either handle is
    ///   not a schema, or when the options will not encode.
    @PanprotoEngine
    public func findBestMorphism(
        to target: SchemaHandle,
        options: MorphismSearchOptions = MorphismSearchOptions()
    ) throws(PanprotoError) -> FoundMorphism? {
        let operation = "SchemaHandle.findBestMorphism"
        let opts = try Payload.encode(options, .migration, operation)
        let result = Raw.homFindBestMorphism(src: rawValue, tgt: target.rawValue, opts: opts)
        try result.status.orThrow(.migration, operation)
        return try Payload.decode(
            FoundMorphism?.self, from: result.bytes, .migration, operation)
    }
}

// MARK: - Lowering a found morphism

extension FoundMorphism {
    /// Lower this morphism to a compiled migration.
    ///
    /// The vertex and edge maps become a migration, which is then
    /// compiled so the handle can be applied per record. The schemas it
    /// is compiled against are reconstructed from the morphism itself:
    /// the keys of the two maps describe the source and their values the
    /// target, which is enough because a found morphism is total on the
    /// sub-schema it matched. Vertex kinds do not survive that
    /// reconstruction, and nothing in the compile path reads them.
    ///
    /// The result is the slab's `Migration`: a compiled migration
    /// carrying no anchoring schemas. Operations that need the source
    /// and target alongside the migration, `put` among them, want a
    /// ``CompiledMigrationHandle`` instead, which
    /// ``SchemaHandle/induceMigration(along:to:)`` produces.
    ///
    /// ```swift
    /// if let best = try await post.findBestMorphism(to: profile) {
    ///     let migration = try await best.migration()
    /// }
    /// ```
    ///
    /// - Returns: A fresh migration handle.
    /// - Throws: ``PanprotoError/migration(_:)`` when the morphism will
    ///   not encode, or when compiling it fails.
    @PanprotoEngine
    public func migration() throws(PanprotoError) -> MigrationHandle {
        let operation = "FoundMorphism.migration"
        let payload = try Payload.encode(self, .migration, operation)
        let result = Raw.homMorphismToMigration(morphism: payload)
        try result.status.orThrow(.migration, operation)
        return MigrationHandle(adopting: result.handle)
    }
}

// MARK: - The theory cascade

extension SchemaHandle {
    /// Push `theoryMorphism` down onto this schema, producing the schema
    /// morphism it induces.
    ///
    /// A schema is a model of its protocol's schema theory, so a
    /// morphism between theories acts on schemas: renaming a sort
    /// renames the vertices interpreting it, and renaming an operation
    /// renames the edges. The induced map is that action, recorded as a
    /// `SchemaMorphism` whose `SchemaMorphism.renames` says which
    /// theory-level renames produced it.
    ///
    /// The induced morphism is a value, not a handle. To get a migration
    /// out of it, use ``induceMigration(along:to:)``, which computes the
    /// same map and compiles it against a target schema.
    ///
    /// - Parameter theoryMorphism: The map between the two theories.
    /// - Returns: The schema morphism it induces on this schema.
    /// - Throws: ``PanprotoError/migration(_:)`` when this handle is not
    ///   a schema, or when either payload will not cross the boundary.
    @PanprotoEngine
    public func induceSchemaMorphism(
        along theoryMorphism: TheoryMorphism
    ) throws(PanprotoError) -> SchemaMorphism {
        let operation = "SchemaHandle.induceSchemaMorphism"
        let payload = try Payload.encode(theoryMorphism, .migration, operation)
        let result = Raw.homInduceSchemaMorphism(theoryMorphism: payload, src: rawValue)
        try result.status.orThrow(.migration, operation)
        return try Payload.decode(SchemaMorphism.self, from: result.bytes, .migration, operation)
    }

    /// Induce a migration from this schema into `target` along
    /// `theoryMorphism`.
    ///
    /// This is the whole cascade in one call: the theory morphism induces
    /// a schema morphism on this schema (the same one
    /// ``induceSchemaMorphism(along:)`` answers with), the morphism is
    /// turned into the pullback functor between the two schemas, and
    /// that is compiled against both.
    ///
    /// Both halves come back. `morphism` is the schema-level map, which
    /// is what to inspect when a migration moved something unexpected;
    /// `migration` is the compiled handle bundled with its source and
    /// target schemas, so it is directly usable as a lens.
    ///
    /// ```swift
    /// let induced = try await post.induceMigration(
    ///     along: renameProp,
    ///     to: renamedPost
    /// )
    /// let moved = induced.morphism.vertexMap.count
    /// ```
    ///
    /// - Parameters:
    ///   - theoryMorphism: The map between the two theories.
    ///   - target: The schema the migration lands in.
    /// - Returns: The induced schema morphism and the compiled migration
    ///   handle.
    /// - Throws: ``PanprotoError/migration(_:)`` when either handle is
    ///   not a schema, or when either payload will not cross the
    ///   boundary.
    @PanprotoEngine
    public func induceMigration(
        along theoryMorphism: TheoryMorphism,
        to target: SchemaHandle
    ) throws(PanprotoError) -> InducedMigration {
        let operation = "SchemaHandle.induceMigration"
        let payload = try Payload.encode(theoryMorphism, .migration, operation)
        let result = Raw.homInduceMigrationFromTheory(
            theoryMorphism: payload,
            src: rawValue,
            tgt: target.rawValue
        )
        try result.status.orThrow(.migration, operation)
        let morphism = try Payload.decode(
            SchemaMorphism.self, from: result.bytes, .migration, operation)
        return InducedMigration(
            morphism: morphism,
            migration: CompiledMigrationHandle(adopting: result.handle)
        )
    }
}

/// The two halves the theory cascade produces: the schema-level map, and
/// the migration compiled from it.
///
/// The map is what to inspect when a migration moved something
/// unexpected; the handle is what to run. They come back together
/// because reading the second without the first leaves no account of why
/// it does what it does.
public struct InducedMigration: Sendable {
    /// The schema morphism the theory morphism induced.
    public let morphism: SchemaMorphism
    /// The migration compiled from that morphism, carrying both schemas.
    public let migration: CompiledMigrationHandle

    /// Pair an induced morphism with the migration compiled from it.
    public init(morphism: SchemaMorphism, migration: CompiledMigrationHandle) {
        self.morphism = morphism
        self.migration = migration
    }
}
