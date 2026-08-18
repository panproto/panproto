import Foundation
import Panproto
import PanprotoFFI
import PanprotoStructural
import Testing

/// The homomorphism surface driven against real schemas.
///
/// Two routes to a migration meet in this domain. The search asks which
/// structure-preserving maps exist between two schemas and ranks them;
/// the cascade starts from a morphism between the theories those schemas
/// model and derives the schema-level map. Both are exercised here on
/// schemas built through the ATProto protocol, and the search is also
/// pointed at two unrelated lexicon schemas to pin down what an answer
/// of "none" looks like.
@Suite("Homomorphism search and the theory cascade")
struct HomomorphismTests {
    // MARK: - Searching

    @Test("a search finds the map that drops a property, and ranks it")
    func searchFindsTheDroppedProperty() async throws {
        try await PanprotoEngine.run {
            let schemas = try PostSchemaPair()
            defer { schemas.release() }

            let found = try schemas.source.findMorphisms(to: schemas.target)
            #expect(!found.isEmpty, "the target is the source minus one property")

            // Ranked by descending quality, which is the order the engine
            // promises and the reason the first result is the best one.
            for pair in zip(found, found.dropFirst()) {
                #expect(pair.0.quality >= pair.1.quality)
            }
            for morphism in found {
                #expect((0.0...1.0).contains(morphism.quality))
            }

            // The record and the object it carries survive as themselves,
            // and every source vertex is accounted for.
            let best = try #require(found.first)
            #expect(best.vertexMap[PostSchemaPair.record] == PostSchemaPair.record)
            #expect(best.vertexMap[PostSchemaPair.object] == PostSchemaPair.object)
            #expect(best.vertexMap.count == 4)
            #expect(best.edgeMap.count == 3)
            // The property the target does not have is where the two
            // schemas differ, so it cannot land on itself.
            #expect(best.vertexMap[PostSchemaPair.createdAt] != PostSchemaPair.createdAt)
        }
    }

    @Test("the best morphism is the first the full search returns")
    func bestIsTheFirstResult() async throws {
        try await PanprotoEngine.run {
            let schemas = try PostSchemaPair()
            defer { schemas.release() }

            let found = try schemas.source.findMorphisms(to: schemas.target)
            let best = try schemas.source.findBestMorphism(to: schemas.target)
            #expect(best == found.first)
        }
    }

    @Test("a shape requirement no map can meet answers with nothing")
    func shapeRequirementsPrune() async throws {
        try await PanprotoEngine.run {
            let schemas = try PostSchemaPair()
            defer { schemas.release() }

            // The source has four vertices and the target three, so no
            // vertex map into the target can be injective, and none can
            // be a bijection.
            let monic = try schemas.source.findMorphisms(
                to: schemas.target,
                options: MorphismSearchOptions(monic: true)
            )
            #expect(monic.isEmpty)

            let iso = try schemas.source.findBestMorphism(
                to: schemas.target,
                options: MorphismSearchOptions(iso: true)
            )
            #expect(iso == nil)

            // Dropping the requirement puts the answer back.
            #expect(!(try schemas.source.findMorphisms(to: schemas.target).isEmpty))
        }
    }

    @Test("a result limit caps the search, and pinned vertices are honored")
    func optionsSteerTheSearch() async throws {
        try await PanprotoEngine.run {
            let schemas = try PostSchemaPair()
            defer { schemas.release() }

            let capped = try schemas.source.findMorphisms(
                to: schemas.target,
                options: MorphismSearchOptions(maxResults: 1)
            )
            #expect(capped.count <= 1)

            let anchored = try schemas.source.findMorphisms(
                to: schemas.target,
                options: MorphismSearchOptions(
                    hardPins: [PostSchemaPair.record: PostSchemaPair.record]
                )
            )
            #expect(!anchored.isEmpty)
            for morphism in anchored {
                #expect(morphism.vertexMap[PostSchemaPair.record] == PostSchemaPair.record)
            }
        }
    }

    @Test("two unrelated lexicon schemas admit no morphism at all")
    func unrelatedLexiconsFindNothing() async throws {
        try await PanprotoEngine.run {
            let post = try lexiconSchema("schema-bsky-post")
            let profile = try lexiconSchema("schema-bsky-profile")
            defer {
                post.release()
                profile.release()
            }

            // A post and a profile share no structure the search can
            // preserve, and the two entry points have to agree about
            // that: an empty ranking and no best result are the same
            // answer written twice.
            #expect(try post.findMorphisms(to: profile).isEmpty)
            #expect(try post.findBestMorphism(to: profile) == nil)
        }
    }

    // MARK: - Spans

    @Test("a schema spans onto itself, which makes the span a total morphism")
    func selfSpanIsTotal() async throws {
        try await PanprotoEngine.run {
            let schemas = try PostSchemaPair()
            defer { schemas.release() }

            let span = try schemas.source.findSpan(
                to: schemas.source,
                in: schemas.protocolHandle
            )

            #expect(span.isTotal)
            #expect(span.apexCoverage == 1.0)
            #expect(span.provenOptimal)
            #expect(span.qualityLo <= span.quality)
            #expect(span.quality <= span.qualityHi)
            #expect(span.apex.vertices.count == 4)
            // The left leg is an inclusion, so it is the identity on the
            // apex and the right leg carries the whole identification.
            #expect(span.left.vertexMap[PostSchemaPair.record] == PostSchemaPair.record)
            #expect(span.right.vertexMap[PostSchemaPair.record] == PostSchemaPair.record)

            let total = try #require(span.asTotalMorphism)
            #expect(total.vertexMap == span.right.vertexMap)
            #expect(total.quality == span.quality)
        }
    }

    @Test("the span answers for a pair that admits no total morphism")
    func spanAnswersWhereTheMorphismSearchCannot() async throws {
        try await PanprotoEngine.run {
            let schemas = try PostSchemaPair()
            defer { schemas.release() }
            let post = try lexiconSchema("schema-bsky-post")
            let profile = try lexiconSchema("schema-bsky-profile")
            defer {
                post.release()
                profile.release()
            }

            // The total-morphism search has nothing to say about this
            // pair: no assignment covers the whole post lexicon.
            #expect(try post.findBestMorphism(to: profile) == nil)

            // The span says what they do share, and never refuses.
            let span = try post.findSpan(to: profile, in: schemas.protocolHandle)
            #expect(!span.isTotal)
            #expect(span.asTotalMorphism == nil)
            #expect((0.0..<1.0).contains(span.apexCoverage))
            #expect(span.apex.vertices.count == span.right.vertexMap.count)
            // The left leg is an inclusion, so it maps exactly the apex.
            #expect(span.left.vertexMap.count == span.apex.vertices.count)
        }
    }

    @Test("a span reads back as the identification list a pushout takes")
    func spanReadsBackAsAnOverlap() async throws {
        try await PanprotoEngine.run {
            let schemas = try PostSchemaPair()
            defer { schemas.release() }

            let span = try schemas.source.findSpan(
                to: schemas.target,
                in: schemas.protocolHandle
            )
            let overlap = try span.overlap()

            #expect(overlap.vertexPairs.count == span.right.vertexMap.count)
            #expect(overlap.edgePairs.count == span.right.edgeMap.count)
            for pair in overlap.vertexPairs {
                #expect(span.right.vertexMap[pair.key] == pair.value)
            }
            // Sorted by key, so one span always yields the same list.
            #expect(overlap.vertexPairs.map(\.key) == overlap.vertexPairs.map(\.key).sorted())
        }
    }

    @Test("constraints keep a source vertex out of the apex")
    func constraintsShrinkTheApex() async throws {
        try await PanprotoEngine.run {
            let schemas = try PostSchemaPair()
            defer { schemas.release() }

            let full = try schemas.source.findSpan(
                to: schemas.source,
                in: schemas.protocolHandle
            )
            #expect(full.isTotal)

            let restricted = try schemas.source.findSpan(
                to: schemas.source,
                in: schemas.protocolHandle,
                constraints: MorphismDomainConstraints(
                    excludedSources: [PostSchemaPair.createdAt]
                )
            )
            #expect(!restricted.isTotal)
            #expect(restricted.right.vertexMap[PostSchemaPair.createdAt] == nil)
            #expect(restricted.apexCoverage < full.apexCoverage)
        }
    }

    @Test("a weight vector the engine refuses is reported, not ignored")
    func unusableWeightsAreRefused() async throws {
        try await PanprotoEngine.run {
            let schemas = try PostSchemaPair()
            defer { schemas.release() }

            do {
                _ = try schemas.source.findSpan(
                    to: schemas.target,
                    in: schemas.protocolHandle,
                    constraints: MorphismDomainConstraints(
                        scoringWeights: MorphismCostWeights(
                            name: 0, edge: 0, prop: 0, degree: 0, anchor: 0
                        )
                    )
                )
                Issue.record("an all-zero weight vector was accepted")
            } catch let error as PanprotoError {
                #expect(error.domain == .migration)
                #expect(error.detail.operation == "SchemaHandle.findSpan")
                #expect(error.detail.status == .operation)
            }
        }
    }

    // MARK: - Lowering

    @Test("a found morphism lowers to a migration handle")
    func foundMorphismLowersToAMigration() async throws {
        try await PanprotoEngine.run {
            let schemas = try PostSchemaPair()
            defer { schemas.release() }

            let best = try #require(try schemas.source.findBestMorphism(to: schemas.target))
            let lowered = try best.migration()
            defer { lowered.release() }

            // The handle names a live slab entry of the migration
            // variant, which is what makes it usable by the migration
            // surface. It is the bare variant: a lowered morphism
            // carries no anchoring schemas.
            #expect(try slabVariant(of: lowered) == MigrationHandle.slabVariant)
            let serialized = Raw.migSerializeCompiled(migHandle: lowered.rawValue)
            #expect(serialized.status == .ok)
            #expect(!serialized.bytes.isEmpty)
        }
    }

    // MARK: - The cascade

    @Test("a theory rename induces the same rename on every edge it names")
    func theoryRenameInducesASchemaMorphism() async throws {
        try await PanprotoEngine.run {
            let schemas = try PostSchemaPair()
            defer { schemas.release() }

            let induced = try schemas.source.induceSchemaMorphism(along: renameProp)

            // Vertices are untouched by an operation rename, and every
            // one of them is carried across.
            #expect(induced.vertexMap.count == 4)
            for (from, to) in induced.vertexMap {
                #expect(from == to)
            }

            // Every `prop` edge becomes a `field` edge, and the
            // record-schema edge, which the morphism does not name, does
            // not move.
            let renamed = induced.edgeMap.filter { $0.key.kind == "prop" }
            #expect(renamed.count == 2)
            #expect(renamed.allSatisfy { $0.value.kind == "field" })
            #expect(
                induced.edgeMap.contains {
                    $0.key.kind == "record-schema" && $0.value.kind == "record-schema"
                }
            )
            #expect(!induced.renames.isEmpty)
        }
    }

    @Test("the cascade answers with both the morphism and a compiled migration")
    func cascadeReturnsBothHalves() async throws {
        try await PanprotoEngine.run {
            let schemas = try PostSchemaPair()
            defer { schemas.release() }

            let induced = try schemas.source.induceMigration(
                along: renameProp,
                to: schemas.target
            )
            defer { induced.migration.release() }

            // The morphism half is the one `induceSchemaMorphism`
            // computes on its own, so the two have to agree.
            let alone = try schemas.source.induceSchemaMorphism(along: renameProp)
            #expect(induced.morphism.vertexMap == alone.vertexMap)
            #expect(induced.morphism.edgeMap == alone.edgeMap)

            // The migration half is the variant that carries its
            // anchoring schemas, which is what separates it from the
            // handle a lowered search result produces, and it is
            // compiled, so it serializes.
            #expect(
                try slabVariant(of: induced.migration)
                    == CompiledMigrationHandle.slabVariant
            )
            let serialized = Raw.migSerializeCompiled(migHandle: induced.migration.rawValue)
            #expect(serialized.status == .ok)
            #expect(!serialized.bytes.isEmpty)
        }
    }

    // MARK: - Failures

    @Test("a search against a released schema reports the migration domain")
    func searchOnAReleasedHandleFails() async throws {
        try await PanprotoEngine.run {
            let schemas = try PostSchemaPair()
            defer { schemas.release() }

            let doomed = try lexiconSchema("schema-bsky-profile")
            let index = doomed.rawValue
            doomed.release()

            do {
                _ = try schemas.source.findMorphisms(to: doomed)
                Issue.record("a released schema was searched against")
            } catch let error as PanprotoError {
                #expect(error.domain == .migration)
                #expect(error.detail.operation == "SchemaHandle.findMorphisms")
                #expect(error.detail.status == .invalidHandle)
                #expect(error.detail.fault == .invalidHandle(handle: index))
            }
        }
    }

    @Test("a cascade against a schema handle that is a protocol reports the mismatch")
    func cascadeOnTheWrongVariantFails() async throws {
        try await PanprotoEngine.run {
            // A protocol slab entry reached through a `SchemaHandle` is
            // the mistake the handle types exist to prevent, so it has
            // to be built deliberately. The protocol is defined a second
            // time and adopted only as a schema, which leaves this
            // handle the sole owner of that entry. Adopting a live
            // handle's index instead would put two owners on one entry,
            // and the second free would land on whatever the slab had
            // since handed that index to: the slab is process-global,
            // first-fit, and shared with every suite running in
            // parallel.
            let builtin = Raw.registryGetBuiltin(name: "atproto")
            try #require(builtin.status == .ok, "the engine defines no atproto protocol")
            let defined = Raw.protocolDefine(spec: builtin.bytes)
            try #require(defined.status == .ok, "the atproto protocol would not define")
            let miscast = SchemaHandle(adopting: defined.handle)
            defer { miscast.release() }

            do {
                _ = try miscast.induceSchemaMorphism(along: renameProp)
                Issue.record("a protocol handle induced a schema morphism")
            } catch let error as PanprotoError {
                #expect(error.domain == .migration)
                #expect(error.detail.operation == "SchemaHandle.induceSchemaMorphism")
                #expect(error.detail.status == .typeMismatch)
                #expect(error.detail.fault == .typeMismatch(expected: "Schema", actual: "Protocol"))
            }
        }
    }
}

/// A theory morphism renaming the `prop` operation to `field`.
///
/// Nothing else moves: the sort map is empty, so every vertex stays
/// where it is, and only the edges interpreting `prop` are rewritten.
private let renameProp = TheoryMorphism(
    name: "rename_prop",
    domain: "atproto",
    codomain: "atproto",
    opMap: ["prop": .op("field")]
)
