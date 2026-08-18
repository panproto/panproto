import CPanproto
import Foundation
import PanprotoStructural

// The transform half of the C ABI: migrations (`pp_mig_*`), lenses and
// protolens chains (`pp_lens_*`, `pp_protolens_*`), morphism search
// (`pp_hom_*`), the lens graph and fiber calculus (`pp_graph_*`), and
// data sets (`pp_data_*`).
//
// Three conventions hold throughout the file. First, an out-handle is
// meaningful only when the returned status is ``RawStatus/ok``; on any
// other status the handle reads back as zero and must not be freed or
// passed on. Second, every returned buffer is already copied out of
// engine storage and freed, so nothing here borrows memory the engine
// owns. Third, wherever a parameter is documented as a `Migration` or
// `MigrationWithSchemas` handle, the two differ in anchoring: a
// `MigrationWithSchemas` carries the source and target schemas it was
// compiled against, while a bare `Migration` is anchored to the minimal
// schema the engine reconstructs from the compiled payload.

// MARK: - Migration

extension Raw {
    /// Check the existence conditions for a migration mapping between
    /// two schemas.
    ///
    /// `mapping` is a CBOR-encoded `mig::Migration`; the buffer receives
    /// a CBOR-encoded `mig::ExistenceReport`. A mapping that fails the
    /// conditions still returns ``RawStatus/ok`` with `valid: false`
    /// inside the report; a non-ok status means the check could not run
    /// at all.
    ///
    /// `proto` must be a `Protocol` handle; `src` and `tgt` must be
    /// `Schema` handles.
    @inlinable
    public static func migCheckExistence(
        proto: UInt32,
        src: UInt32,
        tgt: UInt32,
        mapping: Data
    ) -> (status: RawStatus, bytes: Data) {
        withPpSlice(mapping) { mapping in
            withPpOutBuffer { out in
                pp_mig_check_existence(proto, src, tgt, mapping, out)
            }
        }
    }

    /// Compile a migration for fast per-record application.
    ///
    /// `mapping` is a CBOR-encoded `mig::Migration`; the out-handle is a
    /// fresh `MigrationWithSchemas`.
    ///
    /// `src` and `tgt` must be `Schema` handles.
    @inlinable
    public static func migCompile(
        src: UInt32,
        tgt: UInt32,
        mapping: Data
    ) -> (status: RawStatus, handle: UInt32) {
        var handle: UInt32 = 0
        let code = withPpSlice(mapping) { mapping in
            pp_mig_compile(src, tgt, mapping, &handle)
        }
        return (RawStatus(code: code), handle)
    }

    /// Compose two compiled migrations into a single migration.
    ///
    /// No payload crosses the boundary; the out-handle is a fresh
    /// `Migration`.
    ///
    /// `m1` and `m2` must each be a `Migration` or a
    /// `MigrationWithSchemas` handle.
    @inlinable
    public static func migCompose(
        m1: UInt32,
        m2: UInt32
    ) -> (status: RawStatus, handle: UInt32) {
        var handle: UInt32 = 0
        let code = pp_mig_compose(m1, m2, &handle)
        return (RawStatus(code: code), handle)
    }

    /// Run coverage analysis (a dry-run migration) over a batch of
    /// instances.
    ///
    /// `instances` is a CBOR-encoded `Vec<Instance>`; the buffer
    /// receives a CBOR-encoded report carrying `total`, `succeeded`,
    /// `failed`, `coverage_percent`, `errors`, and the source and target
    /// vertex counts.
    ///
    /// `migration` must be a `Migration` or `MigrationWithSchemas`
    /// handle; `src` and `tgt` must be `Schema` handles.
    @inlinable
    public static func migCoverage(
        migration: UInt32,
        src: UInt32,
        tgt: UInt32,
        instances: Data
    ) -> (status: RawStatus, bytes: Data) {
        withPpSlice(instances) { instances in
            withPpOutBuffer { out in
                pp_mig_coverage(migration, src, tgt, instances, out)
            }
        }
    }

    /// Invert a bijective migration.
    ///
    /// `mapping` is a CBOR-encoded `mig::Migration` and the buffer
    /// receives the CBOR-encoded inverse `mig::Migration`. A migration
    /// that is not invertible fails with ``RawStatus/operation``.
    ///
    /// `src` and `tgt` must be `Schema` handles.
    @inlinable
    public static func migInvert(
        mapping: Data,
        src: UInt32,
        tgt: UInt32
    ) -> (status: RawStatus, bytes: Data) {
        withPpSlice(mapping) { mapping in
            withPpOutBuffer { out in
                pp_mig_invert(mapping, src, tgt, out)
            }
        }
    }

    /// Lift a JSON record through a compiled migration, returning JSON.
    ///
    /// `json` is raw JSON bytes (decoded with `serde_json`, not CBOR)
    /// and the buffer receives the migrated record as JSON bytes.
    /// `rootVertex` names the source schema vertex the JSON object maps
    /// to; an empty string auto-detects, trying the source schema's
    /// protocol name when it names a vertex, then the schema's declared
    /// primary entry.
    ///
    /// `migration` must be a `Migration` or `MigrationWithSchemas`
    /// handle.
    @inlinable
    public static func migLiftJson(
        migration: UInt32,
        json: Data,
        rootVertex: String
    ) -> (status: RawStatus, bytes: Data) {
        withPpSlices(json, rootVertex) { json, rootVertex in
            withPpOutBuffer { out in
                pp_mig_lift_json(migration, json, rootVertex, out)
            }
        }
    }

    /// Apply a compiled migration to a single W-type record.
    ///
    /// `record` is a CBOR-encoded `Instance` and the buffer receives
    /// the CBOR-encoded migrated `Instance`.
    ///
    /// `migration` must be a `Migration` or `MigrationWithSchemas`
    /// handle.
    @inlinable
    public static func migLiftRecord(
        migration: UInt32,
        record: Data
    ) -> (status: RawStatus, bytes: Data) {
        withPpSlice(record) { record in
            withPpOutBuffer { out in
                pp_mig_lift_record(migration, record, out)
            }
        }
    }

    /// Serialize a compiled migration to CBOR.
    ///
    /// The buffer receives the CBOR-encoded `inst::CompiledMigration`,
    /// meaning the compiled payload alone without its anchoring schemas.
    /// These are exactly the bytes
    /// ``graphFiberAt(instance:migration:targetAnchor:)`` and
    /// ``graphFiberDecomposition(instance:migration:)`` take as their
    /// `migration` argument.
    ///
    /// `migHandle` must be a `Migration` or `MigrationWithSchemas`
    /// handle; anything else fails with ``RawStatus/invalidHandle`` or
    /// ``RawStatus/typeMismatch``.
    @inlinable
    public static func migSerializeCompiled(
        migHandle: UInt32
    ) -> (status: RawStatus, bytes: Data) {
        withPpOutBuffer { out in
            pp_mig_serialize_compiled(migHandle, out)
        }
    }
}

// MARK: - Lens

extension Raw {
    /// Auto-generate up to `topN` ranked candidate lenses between two
    /// schemas.
    ///
    /// `stringency` is the UTF-8 tier name (`strict`, `balanced`,
    /// `lenient`, or `exploratory`; empty selects the engine default).
    /// The buffer receives a CBOR-encoded `{ candidates,
    /// coerce_proposals }` record. Each candidate carries its `chain` in
    /// the JSON shape `ProtolensChain::to_json` emits, so the host can
    /// feed it back through
    /// ``protolensFromJson(json:)`` and
    /// ``protolensInstantiate(chain:schema:)`` to obtain a runnable
    /// lens, alongside the score, coverage, quality, strategies, and
    /// per-step explanations.
    ///
    /// `schema1` and `schema2` must be `Schema` handles.
    @inlinable
    public static func lensAutoGenerateCandidates(
        schema1: UInt32,
        schema2: UInt32,
        topN: UInt32,
        stringency: String
    ) -> (status: RawStatus, bytes: Data) {
        withPpSlice(stringency) { stringency in
            withPpOutBuffer { out in
                pp_lens_auto_generate_candidates(schema1, schema2, topN, stringency, out)
            }
        }
    }

    /// Auto-generate a protolens chain between two schemas.
    ///
    /// `stringency` is the UTF-8 tier name (`strict`, `balanced`,
    /// `lenient`, or `exploratory`; empty selects the engine default).
    /// The out-handle is a fresh `ProtolensChain`.
    ///
    /// `schema1` and `schema2` must be `Schema` handles.
    @inlinable
    public static func lensAutoGenerateProtolens(
        schema1: UInt32,
        schema2: UInt32,
        stringency: String
    ) -> (status: RawStatus, handle: UInt32) {
        var handle: UInt32 = 0
        let code = withPpSlice(stringency) { stringency in
            pp_lens_auto_generate_protolens(schema1, schema2, stringency, &handle)
        }
        return (RawStatus(code: code), handle)
    }

    /// Check the `GetPut` lens law on a test instance.
    ///
    /// `instance` is a CBOR-encoded `Instance` and the buffer receives
    /// a CBOR-encoded `LawCheckResult`. A law violation is reported
    /// inside that result, not as a failing status.
    ///
    /// `migration` must be a `Migration` or `MigrationWithSchemas`
    /// handle.
    @inlinable
    public static func lensCheckGetPut(
        migration: UInt32,
        instance: Data
    ) -> (status: RawStatus, bytes: Data) {
        withPpSlice(instance) { instance in
            withPpOutBuffer { out in
                pp_lens_check_get_put(migration, instance, out)
            }
        }
    }

    /// Check both the `GetPut` and `PutGet` lens laws on a test
    /// instance.
    ///
    /// `instance` is a CBOR-encoded `Instance` and the buffer receives
    /// a CBOR-encoded `LawCheckResult`. A law violation is reported
    /// inside that result, not as a failing status.
    ///
    /// `migration` must be a `Migration` or `MigrationWithSchemas`
    /// handle.
    @inlinable
    public static func lensCheckLaws(
        migration: UInt32,
        instance: Data
    ) -> (status: RawStatus, bytes: Data) {
        withPpSlice(instance) { instance in
            withPpOutBuffer { out in
                pp_lens_check_laws(migration, instance, out)
            }
        }
    }

    /// Check the `PutGet` lens law on a test instance.
    ///
    /// `instance` is a CBOR-encoded `Instance` and the buffer receives
    /// a CBOR-encoded `LawCheckResult`. A law violation is reported
    /// inside that result, not as a failing status.
    ///
    /// `migration` must be a `Migration` or `MigrationWithSchemas`
    /// handle.
    @inlinable
    public static func lensCheckPutGet(
        migration: UInt32,
        instance: Data
    ) -> (status: RawStatus, bytes: Data) {
        withPpSlice(instance) { instance in
            withPpOutBuffer { out in
                pp_lens_check_put_get(migration, instance, out)
            }
        }
    }

    /// Compile a lens DSL document into a protolens chain.
    ///
    /// `source` is the UTF-8 DSL document, `format` is `json` or `yaml`
    /// (`yml` is accepted for the latter), and `bodyVertex` is the
    /// parent vertex id field-level steps attach to. The out-handle is a
    /// fresh `ProtolensChain`.
    ///
    /// Nickel (`ncl`) is not a supported format here, matching the WASM
    /// boundary: Nickel evaluation needs a filesystem for its contract
    /// imports, so callers precompile Nickel to JSON on the host.
    @inlinable
    public static func lensCompileDocument(
        source: String,
        format: String,
        bodyVertex: String
    ) -> (status: RawStatus, handle: UInt32) {
        var handle: UInt32 = 0
        let code = withPpSlices(source, format, bodyVertex) { source, format, bodyVertex in
            pp_lens_compile_document(source, format, bodyVertex, &handle)
        }
        return (RawStatus(code: code), handle)
    }

    /// Compile a lens DSL document, resolving `compose` named references
    /// against a bundle of sibling documents.
    ///
    /// `source`, `format`, and `bodyVertex` carry the same meaning they
    /// do in ``lensCompileDocument(source:format:bodyVertex:)``.
    /// `refs` is a CBOR-encoded map from each referenced lens `id` to
    /// that lens's document source, written in the same `format` as
    /// `source`; every `ref` entry inside a `compose` body is resolved
    /// against this map and parsed with the same evaluator. The
    /// out-handle is a fresh `ProtolensChain`.
    ///
    /// Nickel (`ncl`) is not a supported format here either.
    @inlinable
    public static func lensCompileDocumentWithRefs(
        source: String,
        format: String,
        bodyVertex: String,
        refs: Data
    ) -> (status: RawStatus, handle: UInt32) {
        var handle: UInt32 = 0
        let code = withPpSlices(source, format, bodyVertex, refs) {
            source, format, bodyVertex, refs in
            pp_lens_compile_document_with_refs(source, format, bodyVertex, refs, &handle)
        }
        return (RawStatus(code: code), handle)
    }

    /// Compose two lenses sequentially.
    ///
    /// No payload crosses the boundary; the out-handle is a fresh
    /// `MigrationWithSchemas`.
    ///
    /// `l1` and `l2` must each be a `Migration` or a
    /// `MigrationWithSchemas` handle.
    @inlinable
    public static func lensCompose(
        l1: UInt32,
        l2: UInt32
    ) -> (status: RawStatus, handle: UInt32) {
        var handle: UInt32 = 0
        let code = pp_lens_compose(l1, l2, &handle)
        return (RawStatus(code: code), handle)
    }

    /// Bidirectional get: extract a view and a complement from a record.
    ///
    /// `record` is a CBOR-encoded `Instance`. The buffer receives a
    /// CBOR map with the keys `view` and `complement`, each holding a
    /// CBOR byte string that wraps one self-contained item: the
    /// projected `Instance` and its `Complement`. In that complement,
    /// the tuple-keyed `contraction_choices` and `arc_edges` fields are
    /// written as lists of `[[k0, k1], edge]` pairs rather than as CBOR
    /// maps.
    ///
    /// `migration` must be a `Migration` or `MigrationWithSchemas`
    /// handle.
    @inlinable
    public static func lensGetRecord(
        migration: UInt32,
        record: Data
    ) -> (status: RawStatus, bytes: Data) {
        withPpSlice(record) { record in
            withPpOutBuffer { out in
                pp_lens_get_record(migration, record, out)
            }
        }
    }

    /// Bidirectional put: restore a record from a view and a complement.
    ///
    /// `view` is a CBOR-encoded `Instance` and `complement` is a
    /// CBOR-encoded `Complement`, accepted either in the list-of-pairs
    /// shape ``lensGetRecord(migration:record:)`` emits or with
    /// `contraction_choices` and `arc_edges` left as CBOR maps. The
    /// buffer receives the CBOR-encoded restored `Instance`.
    ///
    /// `migration` must be a `Migration` or `MigrationWithSchemas`
    /// handle.
    @inlinable
    public static func lensPutRecord(
        migration: UInt32,
        view: Data,
        complement: Data
    ) -> (status: RawStatus, bytes: Data) {
        withPpSlices(view, complement) { view, complement in
            withPpOutBuffer { out in
                pp_lens_put_record(migration, view, complement, out)
            }
        }
    }

    /// Auto-generate a symmetric lens from two schemas.
    ///
    /// No payload crosses the boundary; the out-handle is a fresh
    /// `SymmetricLensHandle`.
    ///
    /// `schema1` and `schema2` must be `Schema` handles.
    @inlinable
    public static func lensSymmetricFromSchemas(
        schema1: UInt32,
        schema2: UInt32
    ) -> (status: RawStatus, handle: UInt32) {
        var handle: UInt32 = 0
        let code = pp_lens_symmetric_from_schemas(schema1, schema2, &handle)
        return (RawStatus(code: code), handle)
    }

    /// Sync data through a symmetric lens.
    ///
    /// `view` is a CBOR-encoded `Instance` and `complement` is a
    /// CBOR-encoded `Complement` in either accepted shape;
    /// `direction` is `0` for left-to-right and `1` for right-to-left.
    /// The buffer receives the CBOR-encoded synced `Instance`.
    ///
    /// `symLens` must be a `SymmetricLensHandle` handle.
    @inlinable
    public static func lensSymmetricSync(
        symLens: UInt32,
        view: Data,
        complement: Data,
        direction: UInt8
    ) -> (status: RawStatus, bytes: Data) {
        withPpSlices(view, complement) { view, complement in
            withPpOutBuffer { out in
                pp_lens_symmetric_sync(symLens, view, complement, direction, out)
            }
        }
    }

    /// Serialize a protolens chain to JSON.
    ///
    /// The buffer receives a JSON array with one summary object per
    /// step, carrying `name`, `source_endofunctor`,
    /// `target_endofunctor`, and `lossless`. This is a description of
    /// the chain, not its serialized form: the bytes
    /// ``protolensFromJson(json:)`` reads back are the full
    /// `ProtolensChain` encoding, which this summary does not carry.
    ///
    /// `chain` must be a `ProtolensChain` handle.
    @inlinable
    public static func protolensChainToJson(
        chain: UInt32
    ) -> (status: RawStatus, bytes: Data) {
        withPpOutBuffer { out in
            pp_protolens_chain_to_json(chain, out)
        }
    }

    /// Get the complement spec a protolens chain induces at a schema.
    ///
    /// The buffer receives a CBOR-encoded `lens::ComplementSpec`.
    ///
    /// `chain` must be a `ProtolensChain` handle and `schema` must be a
    /// `Schema` handle.
    @inlinable
    public static func protolensComplementSpec(
        chain: UInt32,
        schema: UInt32
    ) -> (status: RawStatus, bytes: Data) {
        withPpOutBuffer { out in
            pp_protolens_complement_spec(chain, schema, out)
        }
    }

    /// Compose two protolens chains.
    ///
    /// No payload crosses the boundary; the out-handle is a fresh
    /// `ProtolensChain` holding the concatenated steps.
    ///
    /// `chain1` and `chain2` must be `ProtolensChain` handles.
    @inlinable
    public static func protolensCompose(
        chain1: UInt32,
        chain2: UInt32
    ) -> (status: RawStatus, handle: UInt32) {
        var handle: UInt32 = 0
        let code = pp_protolens_compose(chain1, chain2, &handle)
        return (RawStatus(code: code), handle)
    }

    /// Build a protolens chain from a diff spec.
    ///
    /// `diff` is a CBOR-encoded `lens::DiffSpec`; the out-handle is a
    /// fresh `ProtolensChain`.
    ///
    /// `schema1` and `schema2` must be `Schema` handles.
    @inlinable
    public static func protolensFromDiff(
        diff: Data,
        schema1: UInt32,
        schema2: UInt32
    ) -> (status: RawStatus, handle: UInt32) {
        var handle: UInt32 = 0
        let code = withPpSlice(diff) { diff in
            pp_protolens_from_diff(diff, schema1, schema2, &handle)
        }
        return (RawStatus(code: code), handle)
    }

    /// Deserialize a protolens chain from JSON.
    ///
    /// `json` is raw JSON bytes holding a whole `ProtolensChain` in the
    /// shape `ProtolensChain::to_json` emits, which is the shape each
    /// `chain` field inside the
    /// ``lensAutoGenerateCandidates(schema1:schema2:topN:stringency:)``
    /// report carries. The out-handle is a fresh `ProtolensChain`.
    @inlinable
    public static func protolensFromJson(
        json: Data
    ) -> (status: RawStatus, handle: UInt32) {
        var handle: UInt32 = 0
        let code = withPpSlice(json) { json in
            pp_protolens_from_json(json, &handle)
        }
        return (RawStatus(code: code), handle)
    }

    /// Fuse a protolens chain into a single composite step.
    ///
    /// No payload crosses the boundary; the out-handle is a fresh
    /// `ProtolensChain` holding the fused step.
    ///
    /// `chain` must be a `ProtolensChain` handle.
    @inlinable
    public static func protolensFuse(
        chain: UInt32
    ) -> (status: RawStatus, handle: UInt32) {
        var handle: UInt32 = 0
        let code = pp_protolens_fuse(chain, &handle)
        return (RawStatus(code: code), handle)
    }

    /// Instantiate a protolens chain at a specific schema.
    ///
    /// No payload crosses the boundary; the out-handle is a fresh
    /// `MigrationWithSchemas` carrying the runnable lens.
    ///
    /// `chain` must be a `ProtolensChain` handle and `schema` must be a
    /// `Schema` handle.
    @inlinable
    public static func protolensInstantiate(
        chain: UInt32,
        schema: UInt32
    ) -> (status: RawStatus, handle: UInt32) {
        var handle: UInt32 = 0
        let code = pp_protolens_instantiate(chain, schema, &handle)
        return (RawStatus(code: code), handle)
    }
}

// MARK: - Morphism search

extension Raw {
    /// Find the single best-quality morphism between two schemas.
    ///
    /// `opts` is a CBOR-encoded `MorphismSearchOptions` mirroring
    /// `mig::hom_search::SearchOptions`; the buffer receives a
    /// CBOR-encoded `Option<FoundMorphism>`, which is CBOR `null`
    /// when the search finds nothing.
    ///
    /// `src` and `tgt` must be `Schema` handles.
    @inlinable
    public static func homFindBestMorphism(
        src: UInt32,
        tgt: UInt32,
        opts: Data
    ) -> (status: RawStatus, bytes: Data) {
        withPpSlice(opts) { opts in
            withPpOutBuffer { out in
                pp_hom_find_best_morphism(src, tgt, opts, out)
            }
        }
    }

    /// Find structure-preserving morphisms between two schemas.
    ///
    /// `opts` is a CBOR-encoded `MorphismSearchOptions` mirroring
    /// `mig::hom_search::SearchOptions`; the buffer receives a
    /// CBOR-encoded `Vec<FoundMorphism>`, each entry carrying
    /// `vertex_map`, `edge_map`, and `quality`.
    ///
    /// The entries are the morphisms attaining the optimum, capped by
    /// `max_results`, so they all carry the same quality and an empty
    /// array means no total morphism exists.
    ///
    /// `src` and `tgt` must be `Schema` handles.
    @inlinable
    public static func homFindMorphisms(
        src: UInt32,
        tgt: UInt32,
        opts: Data
    ) -> (status: RawStatus, bytes: Data) {
        withPpSlice(opts) { opts in
            withPpOutBuffer { out in
                pp_hom_find_morphisms(src, tgt, opts, out)
            }
        }
    }

    /// Find the maximum span between two schemas.
    ///
    /// `opts` is a CBOR-encoded `MorphismSearchOptions` and
    /// `constraints` a CBOR-encoded `MorphismDomainConstraints`; an
    /// empty CBOR map is a valid payload for either. The buffer receives
    /// a CBOR-encoded `SchemaSpan`.
    ///
    /// The search is total: two schemas with nothing in common answer
    /// with an empty apex rather than with a failing status.
    ///
    /// `src` and `tgt` must be `Schema` handles and `protocol` a
    /// `Protocol` handle: the apex is a schema, a schema is well formed
    /// only against a protocol, and inducing the apex re-validates it,
    /// so the protocol cannot be read off the source, which stores only
    /// its name.
    @inlinable
    public static func homFindSpan(
        src: UInt32,
        tgt: UInt32,
        protocol protocolHandle: UInt32,
        opts: Data,
        constraints: Data
    ) -> (status: RawStatus, bytes: Data) {
        withPpSlices(opts, constraints) { opts, constraints in
            withPpOutBuffer { out in
                pp_hom_find_span(src, tgt, protocolHandle, opts, constraints, out)
            }
        }
    }

    /// Read a span's apex as the identification list a pushout takes.
    ///
    /// `span` is a CBOR-encoded `SchemaSpan`, as ``homFindSpan(src:tgt:protocol:opts:constraints:)``
    /// wrote it; the buffer receives a CBOR-encoded `SchemaOverlap`
    /// whose two pair arrays are sorted by key.
    @inlinable
    public static func homSpanToOverlap(
        span: Data
    ) -> (status: RawStatus, bytes: Data) {
        withPpSlice(span) { span in
            withPpOutBuffer { out in
                pp_hom_span_to_overlap(span, out)
            }
        }
    }

    /// Induce a migration from a theory morphism and a pair of schemas.
    ///
    /// `theoryMorphism` is a CBOR-encoded `gat::TheoryMorphism`. The
    /// buffer receives the CBOR-encoded induced
    /// `schema::SchemaMorphism` and the out-handle is a fresh
    /// `MigrationWithSchemas`: the compiled `Delta_F` pullback bundled
    /// with its anchoring schemas.
    ///
    /// `src` and `tgt` must be `Schema` handles.
    @inlinable
    public static func homInduceMigrationFromTheory(
        theoryMorphism: Data,
        src: UInt32,
        tgt: UInt32
    ) -> (status: RawStatus, handle: UInt32, bytes: Data) {
        var handle: UInt32 = 0
        let result = withPpSlice(theoryMorphism) { theoryMorphism in
            withPpOutBuffer { out in
                pp_hom_induce_migration_from_theory(theoryMorphism, src, tgt, out, &handle)
            }
        }
        return (result.status, handle, result.bytes)
    }

    /// Induce a schema morphism from a theory morphism and a source
    /// schema.
    ///
    /// `theoryMorphism` is a CBOR-encoded `gat::TheoryMorphism`; the
    /// buffer receives a CBOR-encoded `schema::SchemaMorphism`.
    ///
    /// `src` must be a `Schema` handle.
    @inlinable
    public static func homInduceSchemaMorphism(
        theoryMorphism: Data,
        src: UInt32
    ) -> (status: RawStatus, bytes: Data) {
        withPpSlice(theoryMorphism) { theoryMorphism in
            withPpOutBuffer { out in
                pp_hom_induce_schema_morphism(theoryMorphism, src, out)
            }
        }
    }

    /// Convert a found morphism into a compiled migration.
    ///
    /// `morphism` is a CBOR-encoded `FoundMorphism`, lowered to a
    /// `mig::Migration` and then compiled against the minimal schemas
    /// its surviving vertex and edge sets imply. The out-handle is a
    /// fresh `Migration`.
    @inlinable
    public static func homMorphismToMigration(
        morphism: Data
    ) -> (status: RawStatus, handle: UInt32) {
        var handle: UInt32 = 0
        let code = withPpSlice(morphism) { morphism in
            pp_hom_morphism_to_migration(morphism, &handle)
        }
        return (RawStatus(code: code), handle)
    }
}

// MARK: - Lens graph and fibers

extension Raw {
    /// Compute the shortest distance between two schemas in a lens
    /// graph.
    ///
    /// `graph` is a CBOR-encoded `Vec<GraphEdge>`; `sourceSchema` and
    /// `targetSchema` are UTF-8 schema names. The distance is
    /// `Double.infinity` when the schemas are unknown to the graph or no
    /// path connects them.
    @inlinable
    public static func graphConversionDistance(
        graph: Data,
        sourceSchema: String,
        targetSchema: String
    ) -> (status: RawStatus, distance: Double) {
        var distance = 0.0
        let code = withPpSlices(graph, sourceSchema, targetSchema) {
            graph, sourceSchema, targetSchema in
            pp_graph_conversion_distance(graph, sourceSchema, targetSchema, &distance)
        }
        return (RawStatus(code: code), distance)
    }

    /// Compute the fiber of a compiled migration at one target anchor.
    ///
    /// `instance` is a CBOR-encoded `Instance` and `migration` is a
    /// CBOR-encoded `CompiledMigration`, the byte form
    /// ``migSerializeCompiled(migHandle:)`` produces;
    /// `targetAnchor` is the UTF-8 anchor name. The buffer receives a
    /// CBOR-encoded `Vec<u32>` of the source node ids whose remapped
    /// anchor equals `targetAnchor`.
    @inlinable
    public static func graphFiberAt(
        instance: Data,
        migration: Data,
        targetAnchor: String
    ) -> (status: RawStatus, bytes: Data) {
        withPpSlices(instance, migration, targetAnchor) { instance, migration, targetAnchor in
            withPpOutBuffer { out in
                pp_graph_fiber_at(instance, migration, targetAnchor, out)
            }
        }
    }

    /// Compute the fibers of a compiled migration for every target
    /// anchor at once.
    ///
    /// `instance` is a CBOR-encoded `Instance` and `migration` is a
    /// CBOR-encoded `CompiledMigration`. The buffer receives a
    /// CBOR-encoded map from anchor name to `Vec<u32>` partitioning the
    /// source nodes, so every source node appears in exactly one fiber.
    @inlinable
    public static func graphFiberDecomposition(
        instance: Data,
        migration: Data
    ) -> (status: RawStatus, bytes: Data) {
        withPpSlices(instance, migration) { instance, migration in
            withPpOutBuffer { out in
                pp_graph_fiber_decomposition(instance, migration, out)
            }
        }
    }

    /// Construct the internal hom schema `[S, T]`.
    ///
    /// `sourceSchema` and `targetSchema` are CBOR-encoded `Schema`
    /// values, not handles. The buffer receives the CBOR-encoded hom
    /// `Schema`, which holds, for each source vertex in `S`, the choice
    /// and backward vertices encoding all structure-preserving maps from
    /// `S` to `T`.
    @inlinable
    public static func graphPolyHom(
        sourceSchema: Data,
        targetSchema: Data
    ) -> (status: RawStatus, bytes: Data) {
        withPpSlices(sourceSchema, targetSchema) { sourceSchema, targetSchema in
            withPpOutBuffer { out in
                pp_graph_poly_hom(sourceSchema, targetSchema, out)
            }
        }
    }

    /// Find the cheapest conversion path between two schemas in a lens
    /// graph.
    ///
    /// `graph` is a CBOR-encoded `Vec<GraphEdge>`, each edge carrying
    /// `source`, `target`, and a CBOR-encoded `ProtolensChain`;
    /// `sourceSchema` and `targetSchema` are UTF-8 schema names. The
    /// buffer receives a CBOR-encoded `{ cost, steps }` record giving
    /// the total cost and the protolens step names along the shortest
    /// path. When no path exists the call fails with
    /// ``RawStatus/operation``.
    @inlinable
    public static func graphPreferredPath(
        graph: Data,
        sourceSchema: String,
        targetSchema: String
    ) -> (status: RawStatus, bytes: Data) {
        withPpSlices(graph, sourceSchema, targetSchema) { graph, sourceSchema, targetSchema in
            withPpOutBuffer { out in
                pp_graph_preferred_path(graph, sourceSchema, targetSchema, out)
            }
        }
    }
}

// MARK: - Data sets

extension Raw {
    /// Check whether a data set's schema matches a given schema.
    ///
    /// The buffer receives a CBOR-encoded record of `stale`,
    /// `data_schema_id`, and `target_schema_id`: the data set is stale
    /// when the schema id it stores differs from the hash of the
    /// supplied schema.
    ///
    /// `datasetHandle` must be a `DataSet` handle and `schemaHandle`
    /// must be a `Schema` handle.
    @inlinable
    public static func dataCheckStaleness(
        datasetHandle: UInt32,
        schemaHandle: UInt32
    ) -> (status: RawStatus, bytes: Data) {
        withPpOutBuffer { out in
            pp_data_check_staleness(datasetHandle, schemaHandle, out)
        }
    }

    /// Retrieve a data set as CBOR-encoded instances.
    ///
    /// The buffer receives a CBOR-encoded `Vec<Instance>`. The stored
    /// payload is decoded and re-encoded on the way out, so a corrupt
    /// carrier surfaces as ``RawStatus/serialization`` rather than as
    /// opaque bytes.
    ///
    /// `datasetHandle` must be a `DataSet` handle.
    @inlinable
    public static func dataGetDataset(
        datasetHandle: UInt32
    ) -> (status: RawStatus, bytes: Data) {
        withPpOutBuffer { out in
            pp_data_get_dataset(datasetHandle, out)
        }
    }

    /// Round-trip a forward-migration complement carrier.
    ///
    /// `complement` is the CBOR-encoded `Vec<Complement>` a forward
    /// migration produced, with the tuple-keyed fields in the CBOR map
    /// shape `ciborium` writes; the buffer receives those complements
    /// re-encoded. Decoding and re-encoding is the point: a malformed
    /// carrier surfaces as ``RawStatus/serialization`` here rather than
    /// during a later backward migration.
    @inlinable
    public static func dataGetMigrationComplement(
        complement: Data
    ) -> (status: RawStatus, bytes: Data) {
        withPpSlice(complement) { complement in
            withPpOutBuffer { out in
                pp_data_get_migration_complement(complement, out)
            }
        }
    }

    /// Migrate a data set backward using a stored complement.
    ///
    /// `complement` is the CBOR-encoded `Vec<Complement>` the forward
    /// migration produced. The engine auto-generates the lens and
    /// applies `lens::put` per record, pairing each migrated view with
    /// its complement; the out-handle is a fresh `DataSet` re-anchored
    /// to the source schema.
    ///
    /// `datasetHandle` must be the migrated (forward) `DataSet` handle;
    /// `srcSchema` and `tgtSchema` must be `Schema` handles, the same
    /// pair in the same order the forward migration used.
    @inlinable
    public static func dataMigrateBackward(
        datasetHandle: UInt32,
        complement: Data,
        srcSchema: UInt32,
        tgtSchema: UInt32
    ) -> (status: RawStatus, handle: UInt32) {
        var handle: UInt32 = 0
        let code = withPpSlice(complement) { complement in
            pp_data_migrate_backward(datasetHandle, complement, srcSchema, tgtSchema, &handle)
        }
        return (RawStatus(code: code), handle)
    }

    /// Migrate a data set forward between two schemas.
    ///
    /// The engine auto-generates a lens via `lens::auto_generate`,
    /// applies `lens::get` per record, and stores two fresh `DataSet`
    /// handles: `data` holds the migrated instances and is hashed
    /// against the target schema, and `complement` keeps the source
    /// schema id with the CBOR-encoded `Vec<Complement>` in its `data`
    /// field, ready for
    /// ``dataMigrateBackward(datasetHandle:complement:srcSchema:tgtSchema:)``.
    ///
    /// `datasetHandle` must be a `DataSet` handle; `srcSchema` and
    /// `tgtSchema` must be `Schema` handles.
    @inlinable
    public static func dataMigrateForward(
        datasetHandle: UInt32,
        srcSchema: UInt32,
        tgtSchema: UInt32
    ) -> (status: RawStatus, data: UInt32, complement: UInt32) {
        var data: UInt32 = 0
        var complement: UInt32 = 0
        let code = pp_data_migrate_forward(
            datasetHandle,
            srcSchema,
            tgtSchema,
            &data,
            &complement
        )
        return (RawStatus(code: code), data, complement)
    }

    /// Store a data set from JSON, binding it to a schema.
    ///
    /// `dataJson` is raw JSON bytes holding an array of records; a bare
    /// object counts as a one-element array. Each record is parsed
    /// against the schema's inferred root vertex, the instances are
    /// CBOR-encoded into a fresh data set carrier, and the schema is
    /// hashed to stamp the carrier's schema id. The out-handle is a
    /// fresh `DataSet`.
    ///
    /// `schemaHandle` must be a `Schema` handle.
    @inlinable
    public static func dataStoreDataset(
        schemaHandle: UInt32,
        dataJson: Data
    ) -> (status: RawStatus, handle: UInt32) {
        var handle: UInt32 = 0
        let code = withPpSlice(dataJson) { dataJson in
            pp_data_store_dataset(schemaHandle, dataJson, &handle)
        }
        return (RawStatus(code: code), handle)
    }
}
