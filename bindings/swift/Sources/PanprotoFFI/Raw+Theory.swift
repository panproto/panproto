import CPanproto
import Foundation
import PanprotoStructural

// Theory-layer shims over the panproto-c ABI: the GAT domain (theories,
// theory morphisms, and models), the expression domain (surface parsing,
// functional and GAT-term evaluation, type checking, and declarative
// queries), and the enriched domain (schema coercions, defaults,
// mergers, conflict policies, and refinement subsorting).
//
// Two conventions hold for every method below, so neither is repeated on
// the individual declarations.
//
// First, an out-parameter is meaningful only when the returned status
// is ``RawStatus/ok``. On any other status the handle, buffer, count,
// or flag that comes back carries no information and the detail is
// waiting in the thread-local last-error slot.
//
// Second, every handle the engine allocates here is a slab handle the
// host owns: the theories from `gatCreateTheory` and `gatColimit`, the
// model from `gatFreeModel`, and the schemas the enriched builders
// return. Each goes back through the handle-free entry point exactly
// once. The inputs are borrowed for the duration of the call and the
// engine copies whatever it retains, so nothing here escapes.

extension Raw {

    // MARK: - GAT theories

    /// Create a GAT theory from a CBOR spec.
    ///
    /// `spec` is a CBOR-encoded `gat::Theory`. On success the returned
    /// handle names a fresh `Theory` slab entry; a spec that fails to
    /// decode yields ``RawStatus/serialization``.
    @inlinable
    public static func gatCreateTheory(spec: Data) -> (status: RawStatus, handle: UInt32) {
        var handle: UInt32 = 0
        let code = withPpSlice(spec) { spec in
            pp_gat_create_theory(spec, &handle)
        }
        return (RawStatus(code: code), handle)
    }

    /// Compute the colimit of two theories over a shared base.
    ///
    /// `t1`, `t2`, and `shared` must each be a `Theory` handle. On
    /// success the returned handle names a fresh `Theory` slab entry
    /// holding `gat::colimit_by_name(t1, t2, shared)`. A handle that is
    /// invalid or holds another resource yields
    /// ``RawStatus/invalidHandle`` or ``RawStatus/typeMismatch``; a
    /// colimit the engine cannot form yields ``RawStatus/operation``.
    @inlinable
    public static func gatColimit(
        t1: UInt32,
        t2: UInt32,
        shared: UInt32
    ) -> (status: RawStatus, handle: UInt32) {
        var handle: UInt32 = 0
        let code = pp_gat_colimit(t1, t2, shared, &handle)
        return (RawStatus(code: code), handle)
    }

    /// Serialize the theory behind a handle to CBOR.
    ///
    /// `theory` must be a `Theory` handle. On success the buffer holds
    /// the CBOR-encoded `gat::Theory` in the same shape
    /// ``gatCreateTheory(spec:)`` decodes, so a theory the engine
    /// produced (a colimit result, for instance) can be reified by the
    /// host and fed back in.
    @inlinable
    public static func gatSerializeTheory(theory: UInt32) -> (status: RawStatus, bytes: Data) {
        withPpOutBuffer { out in
            pp_gat_serialize_theory(theory, out)
        }
    }

    /// Check the validity of a theory morphism.
    ///
    /// `morphism` is a CBOR-encoded `gat::TheoryMorphism`; `domain` and
    /// `codomain` must each be a `Theory` handle. On success the buffer
    /// holds a CBOR-encoded `{ valid, error }` record. The verdict
    /// lives in that payload, so a valid and an invalid morphism both
    /// return ``RawStatus/ok``; only a malformed payload or a bad
    /// handle gives a failing status.
    @inlinable
    public static func gatCheckMorphism(
        morphism: Data,
        domain: UInt32,
        codomain: UInt32
    ) -> (status: RawStatus, bytes: Data) {
        withPpSlice(morphism) { morphism in
            withPpOutBuffer { out in
                pp_gat_check_morphism(morphism, domain, codomain, out)
            }
        }
    }

    // MARK: - GAT models

    /// Migrate a model's carrier through a theory morphism.
    ///
    /// `model` is a CBOR-encoded sort-interpretation map
    /// (`HashMap<String, Vec<ModelValue>>`); `morphism` is a
    /// CBOR-encoded `gat::TheoryMorphism`. On success the buffer holds
    /// the reindexed sort interpretations in that same CBOR shape: for
    /// each `(domain_sort, codomain_sort)` pair in the morphism's
    /// `sort_map`, the codomain sort's carrier is copied to the domain
    /// sort name.
    ///
    /// Only sort interpretations cross the boundary. A `gat::Model`
    /// also carries operation interpretations as closures, which cannot
    /// be serialized; reindexing those is the host's job once the sorts
    /// have moved.
    @inlinable
    public static func gatMigrateModel(
        model: Data,
        morphism: Data
    ) -> (status: RawStatus, bytes: Data) {
        withPpSlices(model, morphism) { model, morphism in
            withPpOutBuffer { out in
                pp_gat_migrate_model(model, morphism, out)
            }
        }
    }

    /// Construct a bounded approximation of the free (initial) model of
    /// a theory.
    ///
    /// `theory` must be a `Theory` handle. `config` is an optional
    /// CBOR-encoded `{ max_depth, max_terms_per_sort }` record; an
    /// empty `Data` selects the engine defaults, and either field may
    /// be omitted to default just that bound. On success the returned
    /// handle names a fresh `Model` slab entry holding the constructed
    /// model.
    ///
    /// The name mirrors the engine's `gat::free_model`: this allocates
    /// a model rather than releasing one. A model stays behind its
    /// handle because its operation interpretations are closures that
    /// cannot cross the ABI, so the two ways to read it are
    /// ``gatEvalInModel(model:opName:args:)`` for its operations and
    /// ``gatModelSortInterp(model:)`` for its carrier. The handle is
    /// released like any other, through the handle-free entry point.
    ///
    /// A malformed config yields ``RawStatus/serialization``, a bad
    /// theory handle yields ``RawStatus/invalidHandle`` or
    /// ``RawStatus/typeMismatch``, and a construction the engine cannot
    /// finish (a cyclic sort dependency, or an exceeded term bound)
    /// yields ``RawStatus/operation``.
    @inlinable
    public static func gatFreeModel(
        theory: UInt32,
        config: Data
    ) -> (status: RawStatus, handle: UInt32) {
        var handle: UInt32 = 0
        let code = withPpSlice(config) { config in
            pp_gat_free_model(theory, config, &handle)
        }
        return (RawStatus(code: code), handle)
    }

    /// Check a model against a theory, returning equation violations.
    ///
    /// `model` must be a `Model` handle and `theory` a `Theory` handle.
    /// On success the buffer holds a CBOR-encoded `Vec<String>` of
    /// violation descriptions, empty when the model satisfies every
    /// equation. A satisfied and a violated model both return
    /// ``RawStatus/ok``; the verdict lives in the payload. Checking
    /// that itself fails (a missing carrier set, or an assignment count
    /// past the engine bound) yields ``RawStatus/operation``.
    @inlinable
    public static func gatCheckModel(
        model: UInt32,
        theory: UInt32
    ) -> (status: RawStatus, bytes: Data) {
        withPpOutBuffer { out in
            pp_gat_check_model(model, theory, out)
        }
    }

    /// Evaluate an operation in a model and return the resulting value.
    ///
    /// `model` must be a `Model` handle; `opName` is the operation
    /// name; `args` is a CBOR-encoded `Vec<ModelValue>`. On success the
    /// buffer holds the CBOR-encoded `gat::ModelValue` the operation
    /// produced. The interpretation is a closure held in the model and
    /// runs in-process, so only its inputs and its output cross the
    /// boundary.
    ///
    /// A malformed argument payload yields ``RawStatus/serialization``,
    /// a bad model handle yields ``RawStatus/invalidHandle`` or
    /// ``RawStatus/typeMismatch``, and an operation absent from the
    /// model or an interpretation that fails yields
    /// ``RawStatus/operation``.
    @inlinable
    public static func gatEvalInModel(
        model: UInt32,
        opName: String,
        args: Data
    ) -> (status: RawStatus, bytes: Data) {
        withPpSlices(opName, args) { opName, args in
            withPpOutBuffer { out in
                pp_gat_eval_in_model(model, opName, args, out)
            }
        }
    }

    /// Emit a model's full carrier: its sort-interpretation map.
    ///
    /// `model` must be a `Model` handle. On success the buffer holds
    /// the CBOR-encoded `HashMap<String, Vec<ModelValue>>` of the
    /// model's `sort_interp`, each sort name mapped to its carrier set.
    /// This is the extractable half of a model, the operation
    /// interpretations staying in-process.
    @inlinable
    public static func gatModelSortInterp(model: UInt32) -> (status: RawStatus, bytes: Data) {
        withPpOutBuffer { out in
            pp_gat_model_sort_interp(model, out)
        }
    }

    // MARK: - Expressions

    /// Parse expression source text into a `panproto-expr` AST.
    ///
    /// `source` is the expression source. On success the buffer holds
    /// the CBOR-encoded `panproto_core::expr::Expr`. Source the engine
    /// cannot tokenize or parse yields ``RawStatus/operation``.
    @inlinable
    public static func exprParse(source: String) -> (status: RawStatus, bytes: Data) {
        withPpSlice(source) { source in
            withPpOutBuffer { out in
                pp_expr_parse(source, out)
            }
        }
    }

    /// Evaluate a functional expression against an environment.
    ///
    /// `expr` is a CBOR-encoded `panproto_core::expr::Expr`; `env` is a
    /// CBOR-encoded `Vec<(String, panproto_core::expr::Literal)>`. On
    /// success the buffer holds the CBOR-encoded
    /// `panproto_core::expr::Literal` result. Evaluation runs under the
    /// default `EvalConfig` step and depth limits; exceeding them, like
    /// any other evaluation failure, yields ``RawStatus/operation``.
    @inlinable
    public static func exprEvalFunc(
        expr: Data,
        env: Data
    ) -> (status: RawStatus, bytes: Data) {
        withPpSlices(expr, env) { expr, env in
            withPpOutBuffer { out in
                pp_expr_eval_func(expr, env, out)
            }
        }
    }

    /// Evaluate a GAT term against a theory and a variable environment.
    ///
    /// `expr` is a CBOR-encoded `gat::Term`; `env` is a CBOR-encoded
    /// `Vec<(String, gat::ModelValue)>`; `theory` must be a slab
    /// `Theory`. On success the buffer holds the CBOR-encoded
    /// `gat::ModelValue` result. Variables resolve against the
    /// environment, applications evaluate their arguments and consult
    /// the theory's operation table, nullary constants reduce to their
    /// name as a string, and any other application produces a
    /// structured `{ op, args, output_sort }` map.
    @inlinable
    public static func exprEvalGat(
        expr: Data,
        env: Data,
        theory: UInt32
    ) -> (status: RawStatus, bytes: Data) {
        withPpSlices(expr, env) { expr, env in
            withPpOutBuffer { out in
                pp_expr_eval_gat(expr, env, theory, out)
            }
        }
    }

    /// Type-check a GAT term against a theory and a typing context.
    ///
    /// `expr` is a CBOR-encoded `gat::Term`; `theory` must be a slab
    /// `Theory`; `context` is a CBOR-encoded `Vec<(String, String)>`
    /// mapping variable names to sort names. On success the buffer
    /// holds a CBOR-encoded `{ well_formed, output_sort, error }`
    /// record. The result encodes well-formedness, so a well-formed and
    /// an ill-formed term both return ``RawStatus/ok``; only a
    /// malformed payload or a bad handle gives a failing status.
    @inlinable
    public static func exprCheck(
        expr: Data,
        theory: UInt32,
        context: Data
    ) -> (status: RawStatus, bytes: Data) {
        withPpSlices(expr, context) { expr, context in
            withPpOutBuffer { out in
                pp_expr_check(expr, theory, context, out)
            }
        }
    }

    /// Execute a declarative query against a W-type instance.
    ///
    /// `query` is a CBOR-encoded `inst::InstanceQuery`; `instance` is a
    /// CBOR-encoded `Instance`; `schemaHandle` must be a `Schema`
    /// handle. On success the buffer holds a CBOR-encoded list of match
    /// records, each a map with `node_id`, `anchor`, `value`, and
    /// `fields`.
    @inlinable
    public static func queryExecute(
        query: Data,
        instance: Data,
        schemaHandle: UInt32
    ) -> (status: RawStatus, bytes: Data) {
        withPpSlices(query, instance) { query, instance in
            withPpOutBuffer { out in
                pp_query_execute(query, instance, schemaHandle, out)
            }
        }
    }

    // MARK: - Enriched schemas

    /// Add a coercion between two vertex kinds to a schema.
    ///
    /// `schemaHandle` must be a `Schema` handle; `fromKind` and
    /// `toKind` are the source and target vertex kind names; `expr` is
    /// a CBOR-encoded `panproto_core::expr::Expr` coercion expression.
    /// On success the returned handle names a fresh `Schema` slab entry
    /// carrying the coercion, installed as an opaque coercion with no
    /// inverse and keyed by the `(fromKind, toKind)` pair. The input
    /// schema is left untouched and its handle stays valid.
    @inlinable
    public static func schemaAddCoercion(
        schemaHandle: UInt32,
        fromKind: String,
        toKind: String,
        expr: Data
    ) -> (status: RawStatus, handle: UInt32) {
        var handle: UInt32 = 0
        let code = withPpSlices(fromKind, toKind, expr) { fromKind, toKind, expr in
            pp_schema_add_coercion(schemaHandle, fromKind, toKind, expr, &handle)
        }
        return (RawStatus(code: code), handle)
    }

    /// Add a default value to a schema vertex.
    ///
    /// `schemaHandle` must be a `Schema` handle; `vertexName` is the
    /// vertex name; `expr` is a CBOR-encoded
    /// `panproto_core::inst::value::Value`. The value is recorded as a
    /// `default` constraint annotation on the vertex, the annotation's
    /// text being the debug rendering of the decoded value. On success
    /// the returned handle names a fresh `Schema` slab entry carrying
    /// that annotation, the input schema being left untouched.
    @inlinable
    public static func schemaAddDefault(
        schemaHandle: UInt32,
        vertexName: String,
        expr: Data
    ) -> (status: RawStatus, handle: UInt32) {
        var handle: UInt32 = 0
        let code = withPpSlices(vertexName, expr) { vertexName, expr in
            pp_schema_add_default(schemaHandle, vertexName, expr, &handle)
        }
        return (RawStatus(code: code), handle)
    }

    /// Add a merger annotation to a schema vertex.
    ///
    /// `schemaHandle` must be a `Schema` handle; `vertexName` is the
    /// vertex name; `spec` is a CBOR-encoded `{ strategy, args }`
    /// record, where `args` defaults to empty. The merger is recorded
    /// as a `merger` constraint annotation reading `strategy` when
    /// `args` is empty and `strategy(a, b)` when it is not. On success
    /// the returned handle is a fresh slab `Schema` carrying that
    /// annotation, the input schema being left untouched. A vertex name
    /// absent from the schema yields ``RawStatus/operation``.
    @inlinable
    public static func schemaAddMerger(
        schemaHandle: UInt32,
        vertexName: String,
        spec: Data
    ) -> (status: RawStatus, handle: UInt32) {
        var handle: UInt32 = 0
        let code = withPpSlices(vertexName, spec) { vertexName, spec in
            pp_schema_add_merger(schemaHandle, vertexName, spec, &handle)
        }
        return (RawStatus(code: code), handle)
    }

    /// Add a conflict policy annotation to a schema vertex.
    ///
    /// `schemaHandle` must be a `Schema` handle; `vertexName` is the
    /// vertex name; `spec` is a CBOR-encoded `{ policy }` record. The
    /// policy is recorded as a `conflict_policy` constraint annotation.
    /// On success the returned handle names a fresh `Schema` slab entry
    /// carrying that annotation, the input schema being left untouched.
    /// A vertex name absent from the schema yields
    /// ``RawStatus/operation``.
    @inlinable
    public static func schemaAddPolicy(
        schemaHandle: UInt32,
        vertexName: String,
        spec: Data
    ) -> (status: RawStatus, handle: UInt32) {
        var handle: UInt32 = 0
        let code = withPpSlices(vertexName, spec) { vertexName, spec in
            pp_schema_add_policy(schemaHandle, vertexName, spec, &handle)
        }
        return (RawStatus(code: code), handle)
    }

    // MARK: - Refinement

    /// Decide a refinement subsort relationship between two constraint
    /// sets.
    ///
    /// `baseSort` is the shared base sort name that both refinements
    /// are taken over; `subConstraints` and `superConstraints` are
    /// CBOR-encoded `Vec<(String, String)>` of `(sort, value)` pairs.
    /// On success the returned value is `1` when the sub-refinement
    /// refines at least as much as the super-refinement, meaning it
    /// carries every constraint the super-refinement does, and `0`
    /// otherwise. The decision is that constraint-set comparison;
    /// `baseSort` names the carrier both refinements sit over and is
    /// validated as UTF-8.
    @inlinable
    public static func enrichedRefinementSubsort(
        baseSort: String,
        subConstraints: Data,
        superConstraints: Data
    ) -> (status: RawStatus, isSubsort: UInt32) {
        var isSubsort: UInt32 = 0
        let code = withPpSlices(baseSort, subConstraints, superConstraints) {
            baseSort, subConstraints, superConstraints in
            pp_enriched_refinement_subsort(baseSort, subConstraints, superConstraints, &isSubsort)
        }
        return (RawStatus(code: code), isSubsort)
    }
}
