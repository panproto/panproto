//! Unified compilation dispatcher.
//!
//! Takes a [`LensDocument`] and produces a [`CompiledLens`] by
//! dispatching to the appropriate body-variant compiler.
//!
//! Two entry points share the same dispatch:
//!
//! - [`compile`] is schema-independent. It compiles `steps`, `rules`,
//!   `compose`, and `symmetric` bodies (all of which are
//!   schema-parametric) but rejects `auto` and `from_diff` bodies, which
//!   need a concrete source/target schema and protocol, with
//!   [`LensDslError::AutoRequiresSchemas`].
//! - [`compile_with_schemas`] additionally compiles `auto` (via
//!   [`panproto_lens::auto_generate`]) and `from_diff` (via
//!   [`panproto_lens::diff_to_protolens()`]), and verifies the compiled
//!   chain's output schema against the document's declared `target`.

use std::collections::HashMap;

use panproto_gat::Name;
use panproto_inst::FieldTransform;
use panproto_lens::{
    AutoLensConfig, DiffSpec, KindChange, ProtolensChain, Stringency, auto_generate,
    auto_generate_with_hints, combinators, diff_to_protolens,
};
use panproto_schema::{Edge, Protocol, Schema, primary_entry};

use crate::document::{AutoSpec, HintStringency, LensDocument};
use crate::error::LensDslError;

/// The compiled output of a lens document.
///
/// Contains both the schema-level [`ProtolensChain`] and the
/// value-level [`FieldTransform`]s, along with metadata from the
/// original document.
#[derive(Debug, Clone)]
pub struct CompiledLens {
    /// The lens document ID.
    pub id: String,
    /// Human-readable description, carried through from the document.
    pub description: String,
    /// Source schema NSID.
    pub source: String,
    /// Target schema NSID.
    pub target: String,
    /// The compiled protolens chain (schema-level transforms).
    ///
    /// For a `symmetric` body this holds the left (forward) leg; the
    /// full span is available on [`CompiledLens::symmetric`].
    pub chain: ProtolensChain,
    /// Value-level field transforms, keyed by parent vertex.
    pub field_transforms: HashMap<Name, Vec<FieldTransform>>,
    /// Protocol-specific extension metadata (opaque).
    pub extensions: HashMap<String, serde_json::Value>,
    /// Auto-generation spec, if the `auto` body variant was used.
    pub auto_spec: Option<AutoSpec>,
    /// The two protolens chains of a `symmetric` body (left, right).
    /// `None` for every other body variant. Assemble a concrete
    /// [`SymmetricLens`](panproto_lens::SymmetricLens) with
    /// [`SymmetricLens::from_protolens_chains`](panproto_lens::SymmetricLens::from_protolens_chains).
    pub symmetric: Option<SymmetricChains>,
    /// The document's declared invertibility, carried through so
    /// bindings can surface it. When `Some(true)`, [`compile`] has
    /// already verified that the chain contains no lossy element.
    pub invertible: Option<bool>,
}

/// The left and right protolens chains of a compiled `symmetric` body.
#[derive(Debug, Clone)]
pub struct SymmetricChains {
    /// The left leg (middle → left view).
    pub left: ProtolensChain,
    /// The right leg (middle → right view).
    pub right: ProtolensChain,
}

/// The intermediate result of compiling a document body, before
/// metadata is attached and cross-cutting checks run.
struct BodyOutput {
    chain: ProtolensChain,
    field_transforms: HashMap<Name, Vec<FieldTransform>>,
    symmetric: Option<SymmetricChains>,
}

/// Validate that exactly one body variant is present, returning its name.
///
/// # Errors
///
/// Returns [`LensDslError::NoBody`] if none is present or
/// [`LensDslError::MultipleBodies`] if more than one is.
fn validate_single_body(doc: &LensDocument) -> Result<&'static str, LensDslError> {
    let mut present = Vec::new();
    if doc.steps.is_some() {
        present.push("steps");
    }
    if doc.rules.is_some() {
        present.push("rules");
    }
    if doc.compose.is_some() {
        present.push("compose");
    }
    if doc.auto.is_some() {
        present.push("auto");
    }
    if doc.from_diff.is_some() {
        present.push("from_diff");
    }
    if doc.symmetric.is_some() {
        present.push("symmetric");
    }

    match present.as_slice() {
        [] => Err(LensDslError::NoBody { id: doc.id.clone() }),
        [only] => Ok(only),
        _ => Err(LensDslError::MultipleBodies {
            id: doc.id.clone(),
            variants: present.join(", "),
        }),
    }
}

/// Compile a [`LensDocument`] into a [`CompiledLens`], schema-independently.
///
/// The `body_vertex` parameter specifies the parent vertex ID under
/// which fields are added/removed (e.g., `"record:body"` for `ATProto`).
///
/// The `resolver` callback resolves lens references in `compose` bodies.
/// It receives a lens ID and returns the already-compiled lens, or
/// `None` if not found.
///
/// # Errors
///
/// Returns [`LensDslError::NoBody`] / [`LensDslError::MultipleBodies`]
/// for body-count violations, [`LensDslError::AutoRequiresSchemas`] for
/// `auto` and `from_diff` bodies (use [`compile_with_schemas`]),
/// [`LensDslError::NotInvertible`] if `invertible: true` but the chain
/// is lossy, or propagates errors from the body-specific compiler.
pub fn compile(
    doc: &LensDocument,
    body_vertex: &str,
    resolver: &dyn Fn(&str) -> Option<CompiledLens>,
) -> Result<CompiledLens, LensDslError> {
    let variant = validate_single_body(doc)?;
    let body = compile_schema_free_body(doc, variant, body_vertex, resolver)?;
    finalize(doc, body)
}

/// Compile a [`LensDocument`] with concrete schema/protocol context.
///
/// This is the schema-aware entry point. In addition to the bodies
/// [`compile`] handles, it compiles:
///
/// - `auto`: runs [`panproto_lens::auto_generate`] with the parsed
///   [`AutoSpec`] hints against `source_schema` / `target_schema`.
/// - `from_diff`: computes the structural diff of `source_schema` and
///   `target_schema` and runs [`panproto_lens::diff_to_protolens()`].
///
/// After compiling any body (except `symmetric`, which has no single
/// target), the chain is instantiated at `source_schema` and its output
/// schema's NSID is compared against the document's declared `target`;
/// a divergence yields [`LensDslError::TargetMismatch`].
///
/// # Errors
///
/// Returns any error [`compile`] can, plus
/// [`LensDslError::Generation`] if auto/diff generation or chain
/// instantiation fails, and [`LensDslError::TargetMismatch`] if the
/// produced schema's NSID does not match the declared target.
pub fn compile_with_schemas(
    doc: &LensDocument,
    body_vertex: &str,
    source_schema: &Schema,
    target_schema: &Schema,
    protocol: &Protocol,
    resolver: &dyn Fn(&str) -> Option<CompiledLens>,
) -> Result<CompiledLens, LensDslError> {
    let variant = validate_single_body(doc)?;

    let body = match variant {
        "auto" => compile_auto_body(doc, source_schema, target_schema, protocol)?,
        "from_diff" => compile_from_diff_body(doc, source_schema, target_schema)?,
        _ => compile_schema_free_body(doc, variant, body_vertex, resolver)?,
    };

    let compiled = finalize(doc, body)?;

    // Verify the compiled chain's output against the declared
    // target. A `symmetric` body has no single target schema, so the
    // check is not applicable there.
    if variant != "symmetric" {
        verify_target(&compiled, source_schema, protocol)?;
    }

    Ok(compiled)
}

/// Compile a schema-independent body (steps, rules, compose, symmetric).
///
/// `auto` and `from_diff` are rejected here with
/// [`LensDslError::AutoRequiresSchemas`].
fn compile_schema_free_body(
    doc: &LensDocument,
    variant: &str,
    body_vertex: &str,
    resolver: &dyn Fn(&str) -> Option<CompiledLens>,
) -> Result<BodyOutput, LensDslError> {
    match variant {
        "steps" => {
            let steps = doc.steps.as_deref().unwrap_or_default();
            let compiled = crate::steps::compile_steps(steps, body_vertex)?;
            Ok(BodyOutput {
                chain: compiled.chain,
                field_transforms: compiled.field_transforms,
                symmetric: None,
            })
        }
        "rules" => {
            let rules = doc.rules.as_deref().unwrap_or_default();
            let compiled = crate::rules::compile_rules(rules, doc.passthrough, body_vertex)?;
            Ok(BodyOutput {
                chain: compiled.chain,
                field_transforms: compiled.field_transforms,
                symmetric: None,
            })
        }
        "compose" => {
            let compose = doc
                .compose
                .as_ref()
                .ok_or_else(|| LensDslError::NoBody { id: doc.id.clone() })?;
            let compiled = crate::compose::compile_compose(compose, body_vertex, resolver)?;
            Ok(BodyOutput {
                chain: compiled.chain,
                field_transforms: compiled.field_transforms,
                symmetric: None,
            })
        }
        "symmetric" => {
            let spec = doc
                .symmetric
                .as_ref()
                .ok_or_else(|| LensDslError::NoBody { id: doc.id.clone() })?;
            let focus = if spec.focus.is_empty() {
                body_vertex
            } else {
                spec.focus.as_str()
            };
            let left = crate::steps::compile_steps(&spec.left, focus)?;
            let right = crate::steps::compile_steps(&spec.right, focus)?;
            let mut field_transforms = left.field_transforms;
            for (k, v) in right.field_transforms {
                field_transforms.entry(k).or_default().extend(v);
            }
            Ok(BodyOutput {
                chain: left.chain.clone(),
                field_transforms,
                symmetric: Some(SymmetricChains {
                    left: left.chain,
                    right: right.chain,
                }),
            })
        }
        // `auto` and `from_diff` require schema context.
        _ => Err(LensDslError::AutoRequiresSchemas { id: doc.id.clone() }),
    }
}

/// Compile an `auto` body via [`panproto_lens::auto_generate`].
fn compile_auto_body(
    doc: &LensDocument,
    source_schema: &Schema,
    target_schema: &Schema,
    protocol: &Protocol,
) -> Result<BodyOutput, LensDslError> {
    let spec = doc
        .auto
        .as_ref()
        .ok_or_else(|| LensDslError::NoBody { id: doc.id.clone() })?;

    let mut config = AutoLensConfig::default();
    if let Some(enable_overlap) = spec.enable_overlap {
        config.try_overlap = enable_overlap;
    }
    if let Some(max_results) = spec.max_search_depth {
        config.search_opts.max_results = max_results.max(1);
    }

    let result = if let Some(hints) = &spec.hints {
        if let Some(stringency) = hints.stringency {
            config.stringency = hint_stringency_to_engine(stringency);
        }
        for cluster in &hints.alias_clusters {
            config.alias_dict.add_cluster(cluster);
        }
        config.try_overlap = true;

        let parts = panproto_lens::hint::HintParts {
            anchors: hints.anchors.clone(),
            scope_pairs: hints.scope_pairs(),
            excluded_targets: hints.excluded_target_names(),
            excluded_sources: hints.excluded_source_names(),
            scoring_weights: hints.scoring_weights(),
            name_similarity_threshold: hints.name_similarity_threshold(),
        };
        let (derived, domain_constraints) =
            panproto_lens::hint::resolve_hints(&parts, source_schema, target_schema);

        auto_generate_with_hints(
            source_schema,
            target_schema,
            protocol,
            &config,
            &derived,
            &domain_constraints,
            spec.quality_threshold,
        )
        .map_err(|e| LensDslError::Generation {
            id: doc.id.clone(),
            message: e.to_string(),
        })?
    } else {
        auto_generate(source_schema, target_schema, protocol, &config).map_err(|e| {
            LensDslError::Generation {
                id: doc.id.clone(),
                message: e.to_string(),
            }
        })?
    };

    Ok(BodyOutput {
        chain: result.chain,
        field_transforms: HashMap::new(),
        symmetric: None,
    })
}

/// Compile a `from_diff` body via [`panproto_lens::diff_to_protolens()`].
///
/// The [`DiffSpec`] is computed as the structural difference between
/// `source_schema` (old) and `target_schema` (new).
fn compile_from_diff_body(
    doc: &LensDocument,
    source_schema: &Schema,
    target_schema: &Schema,
) -> Result<BodyOutput, LensDslError> {
    // `from_diff` is a schema-aware body; its presence is validated by
    // the caller, but reject a stray schema-free call defensively.
    if doc.from_diff.is_none() {
        return Err(LensDslError::AutoRequiresSchemas { id: doc.id.clone() });
    }
    let diff = structural_diff(source_schema, target_schema);
    let chain = diff_to_protolens(&diff, source_schema, target_schema).map_err(|e| {
        LensDslError::Generation {
            id: doc.id.clone(),
            message: e.to_string(),
        }
    })?;
    Ok(BodyOutput {
        chain,
        field_transforms: HashMap::new(),
        symmetric: None,
    })
}

/// Attach document metadata, append directed-equation steps, and run the
/// invertibility check.
fn finalize(doc: &LensDocument, body: BodyOutput) -> Result<CompiledLens, LensDslError> {
    let mut chain = body.chain;

    // Directed-equation modifier: append oriented rewrites to the chain.
    if let Some(equations) = &doc.directed_equations {
        if !equations.is_empty() {
            let eq_chain = crate::steps::compile_directed_equations(equations)?;
            chain = combinators::pipeline(vec![chain, eq_chain]);
        }
    }

    let compiled = CompiledLens {
        id: doc.id.clone(),
        description: doc.description.clone(),
        source: doc.source.clone(),
        target: doc.target.clone(),
        chain,
        field_transforms: body.field_transforms,
        extensions: doc.extensions.clone(),
        auto_spec: doc.auto.clone(),
        symmetric: body.symmetric,
        invertible: doc.invertible,
    };

    // Honor `invertible: true` by rejecting a lossy chain.
    if doc.invertible == Some(true) {
        if let Some(element) = first_lossy_element(&compiled) {
            return Err(LensDslError::NotInvertible {
                id: doc.id.clone(),
                element,
            });
        }
    }

    Ok(compiled)
}

/// Return a description of the first lossy element (chain step or field
/// transform) in `compiled`, or `None` if every element is lossless.
///
/// A chain step is lossless per [`Protolens::is_lossless`]; a field
/// transform is lossless when its coercion class is `Iso`.
fn first_lossy_element(compiled: &CompiledLens) -> Option<String> {
    for step in &compiled.chain.steps {
        if !step.is_lossless() {
            return Some(format!("step `{}`", step.name));
        }
    }
    for (vertex, transforms) in &compiled.field_transforms {
        for transform in transforms {
            if !transform.coercion_class().is_lossless() {
                return Some(format!(
                    "field transform on `{vertex}` (class {:?})",
                    transform.coercion_class()
                ));
            }
        }
    }
    None
}

/// Verify the compiled chain's output schema NSID against the declared
/// target.
///
/// Instantiates `compiled.chain` at `source_schema` and compares the
/// NSID of the resulting schema's primary entry vertex against
/// `compiled.target`. The check is skipped when the declared target is
/// empty or the produced schema declares no entry NSID (unverifiable).
fn verify_target(
    compiled: &CompiledLens,
    source_schema: &Schema,
    protocol: &Protocol,
) -> Result<(), LensDslError> {
    let declared = compiled.target.trim();
    if declared.is_empty() {
        return Ok(());
    }

    let lens = compiled
        .chain
        .instantiate(source_schema, protocol)
        .map_err(|e| LensDslError::Generation {
            id: compiled.id.clone(),
            message: format!("cannot instantiate chain at source schema to verify target: {e}"),
        })?;

    // The produced schema's identity is the NSID of its primary entry.
    // When absent, we cannot verify and do not fail.
    if let Some(actual) = schema_target_nsid(&lens.tgt_schema) {
        if actual != declared {
            return Err(LensDslError::TargetMismatch {
                id: compiled.id.clone(),
                declared: declared.to_owned(),
                actual,
            });
        }
    }
    Ok(())
}

/// The NSID that identifies a schema for target comparison: the NSID
/// mapped to its primary entry vertex, if any.
fn schema_target_nsid(schema: &Schema) -> Option<String> {
    let entry = primary_entry(schema)?;
    schema.nsids.get(entry).map(ToString::to_string)
}

/// Compute the structural diff of two schemas: added/removed vertices,
/// vertex kind changes, and added/removed edges.
fn structural_diff(old_schema: &Schema, new_schema: &Schema) -> DiffSpec {
    let added_vertices = new_schema
        .vertices
        .keys()
        .filter(|id| !old_schema.vertices.contains_key(*id))
        .map(ToString::to_string)
        .collect();
    let removed_vertices = old_schema
        .vertices
        .keys()
        .filter(|id| !new_schema.vertices.contains_key(*id))
        .map(ToString::to_string)
        .collect();

    let kind_changes = old_schema
        .vertices
        .iter()
        .filter_map(|(id, old_vertex)| {
            let new_vertex = new_schema.vertices.get(id)?;
            (old_vertex.kind != new_vertex.kind).then(|| KindChange {
                vertex_id: id.to_string(),
                old_kind: old_vertex.kind.to_string(),
                new_kind: new_vertex.kind.to_string(),
            })
        })
        .collect();

    let added_edges = new_schema
        .edges
        .keys()
        .filter(|edge| !old_schema.edges.contains_key(*edge))
        .cloned()
        .collect::<Vec<Edge>>();
    let removed_edges = old_schema
        .edges
        .keys()
        .filter(|edge| !new_schema.edges.contains_key(*edge))
        .cloned()
        .collect::<Vec<Edge>>();

    DiffSpec {
        added_vertices,
        removed_vertices,
        kind_changes,
        added_edges,
        removed_edges,
    }
}

/// Map a hint-DSL stringency tier to the engine's [`Stringency`].
const fn hint_stringency_to_engine(s: HintStringency) -> Stringency {
    match s {
        HintStringency::Strict => Stringency::Strict,
        HintStringency::Balanced => Stringency::Balanced,
        HintStringency::Lenient => Stringency::Lenient,
        HintStringency::Exploratory => Stringency::Exploratory,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use panproto_schema::{Protocol, Schema, SchemaBuilder};

    use super::*;
    use crate::eval::eval_json;

    fn null_resolver(_: &str) -> Option<CompiledLens> {
        None
    }

    /// An open protocol with the object kinds our fixtures use.
    fn test_protocol() -> Protocol {
        Protocol {
            name: "test".into(),
            schema_theory: "ThGraph".into(),
            instance_theory: "ThWType".into(),
            edge_rules: vec![],
            obj_kinds: vec![
                "object".into(),
                "string".into(),
                "record".into(),
                "boolean".into(),
                "integer".into(),
            ],
            constraint_sorts: vec![],
            ..Protocol::default()
        }
    }

    /// `post` record with `text` and `subtitle` string fields; entry
    /// `post` carries the NSID `app.test.post`.
    fn source_schema() -> Schema {
        let proto = test_protocol();
        SchemaBuilder::new(&proto)
            .entry("post")
            .vertex("post", "record", Some("app.test.post"))
            .unwrap()
            .vertex("post.text", "string", None)
            .unwrap()
            .vertex("post.subtitle", "string", None)
            .unwrap()
            .edge("post", "post.text", "prop", Some("text"))
            .unwrap()
            .edge("post", "post.subtitle", "prop", Some("subtitle"))
            .unwrap()
            .build()
            .unwrap()
    }

    /// `post` record with only the `text` field; the morphism from
    /// [`source_schema`] drops `subtitle`.
    fn target_schema() -> Schema {
        let proto = test_protocol();
        SchemaBuilder::new(&proto)
            .entry("post")
            .vertex("post", "record", Some("app.test.post"))
            .unwrap()
            .vertex("post.text", "string", None)
            .unwrap()
            .edge("post", "post.text", "prop", Some("text"))
            .unwrap()
            .build()
            .unwrap()
    }

    fn doc(json: &str) -> LensDocument {
        eval_json(json).unwrap()
    }

    // -- body-count validation ------------------------------------

    #[test]
    fn no_body_is_rejected() {
        let d = doc(r#"{ "id": "x", "source": "s", "target": "t" }"#);
        let err = compile(&d, "post", &null_resolver).unwrap_err();
        assert!(matches!(err, LensDslError::NoBody { .. }));
    }

    #[test]
    fn multiple_bodies_are_rejected() {
        let d = doc(r#"{ "id": "x", "source": "s", "target": "t",
                 "steps": [{ "remove_field": "a" }],
                 "rules": [{ "pattern": { "name": "a" } }] }"#);
        let err = compile(&d, "post", &null_resolver).unwrap_err();
        assert!(matches!(err, LensDslError::MultipleBodies { .. }));
    }

    // -- metadata survives compilation ----------------------------

    #[test]
    fn field_transforms_description_and_extensions_survive() {
        let d = doc(r#"{
                "id": "meta",
                "description": "carries metadata",
                "source": "s",
                "target": "",
                "steps": [
                    { "apply_expr": { "field": "n", "expr": "add n 1" } }
                ],
                "extensions": { "vendor": { "flag": true } }
            }"#);
        let compiled = compile(&d, "post", &null_resolver).unwrap();
        assert_eq!(compiled.description, "carries metadata");
        assert!(compiled.extensions.contains_key("vendor"));
        let key = Name::from("post");
        assert!(
            compiled
                .field_transforms
                .get(&key)
                .is_some_and(|t| !t.is_empty()),
            "apply_expr should survive as a field transform"
        );
    }

    // -- auto body ------------------------------------------------

    #[test]
    fn auto_body_without_schemas_errors() {
        let d = doc(r#"{ "id": "a", "source": "s", "target": "t", "auto": {} }"#);
        let err = compile(&d, "post", &null_resolver).unwrap_err();
        assert!(matches!(err, LensDslError::AutoRequiresSchemas { .. }));
    }

    #[test]
    fn auto_body_with_schemas_matches_direct_auto_generate() {
        let src = source_schema();
        let tgt = target_schema();
        let proto = test_protocol();

        let d = doc(
            r#"{ "id": "a", "source": "app.test.post", "target": "app.test.post", "auto": {} }"#,
        );
        let compiled =
            compile_with_schemas(&d, "post", &src, &tgt, &proto, &null_resolver).unwrap();

        // The DSL auto path must route to the engine and yield the same
        // chain as calling `auto_generate` directly with the default
        // config (the empty `AutoSpec` selects the defaults).
        let direct = auto_generate(&src, &tgt, &proto, &AutoLensConfig::default()).unwrap();
        assert_eq!(
            compiled.chain.to_json().unwrap(),
            direct.chain.to_json().unwrap(),
            "DSL auto chain must match direct auto_generate output"
        );
    }

    #[test]
    fn auto_body_with_hints_routes_through_engine() {
        // A hinted auto body must anchor `post -> post` and still produce
        // a chain the engine accepts (non-panicking end-to-end path).
        let src = source_schema();
        let tgt = target_schema();
        let proto = test_protocol();
        let d = doc(r#"{
                "id": "a", "source": "app.test.post", "target": "app.test.post",
                "auto": {
                    "enable_overlap": true,
                    "hints": { "anchors": { "post": "post" }, "stringency": "lenient" }
                }
            }"#);
        let compiled = compile_with_schemas(&d, "post", &src, &tgt, &proto, &null_resolver);
        assert!(compiled.is_ok(), "hinted auto should compile: {compiled:?}");
    }

    // -- target verification --------------------------------------

    #[test]
    fn target_match_passes() {
        let src = source_schema();
        let tgt = target_schema();
        let proto = test_protocol();
        let d = doc(
            r#"{ "id": "m", "source": "app.test.post", "target": "app.test.post",
                 "steps": [{ "add_field": { "name": "extra", "kind": "string" } }] }"#,
        );
        let compiled = compile_with_schemas(&d, "post", &src, &tgt, &proto, &null_resolver);
        assert!(
            compiled.is_ok(),
            "matching target should compile: {compiled:?}"
        );
    }

    #[test]
    fn target_mismatch_errors() {
        let src = source_schema();
        let tgt = target_schema();
        let proto = test_protocol();
        let d = doc(
            r#"{ "id": "m", "source": "app.test.post", "target": "app.test.WRONG",
                 "steps": [{ "add_field": { "name": "extra", "kind": "string" } }] }"#,
        );
        let err = compile_with_schemas(&d, "post", &src, &tgt, &proto, &null_resolver).unwrap_err();
        match err {
            LensDslError::TargetMismatch {
                declared, actual, ..
            } => {
                assert_eq!(declared, "app.test.WRONG");
                assert_eq!(actual, "app.test.post");
            }
            other => panic!("expected TargetMismatch, got {other:?}"),
        }
    }

    // -- invertible check -----------------------------------------

    #[test]
    fn invertible_true_passes_for_lossless_chain() {
        let d = doc(
            r#"{ "id": "inv", "source": "s", "target": "", "invertible": true,
                 "steps": [{ "rename_sort": { "old": "a", "new": "b" } }] }"#,
        );
        let compiled = compile(&d, "post", &null_resolver).unwrap();
        assert_eq!(compiled.invertible, Some(true));
    }

    #[test]
    fn invertible_true_fails_for_lossy_chain() {
        let d = doc(
            r#"{ "id": "inv", "source": "s", "target": "", "invertible": true,
                 "steps": [{ "remove_field": "gone" }] }"#,
        );
        let err = compile(&d, "post", &null_resolver).unwrap_err();
        assert!(matches!(err, LensDslError::NotInvertible { .. }));
    }

    // -- from_diff ----------------------------------------

    #[test]
    fn from_diff_without_schemas_errors() {
        let d = doc(r#"{ "id": "d", "source": "s", "target": "", "from_diff": {} }"#);
        let err = compile(&d, "post", &null_resolver).unwrap_err();
        assert!(matches!(err, LensDslError::AutoRequiresSchemas { .. }));
    }

    #[test]
    fn from_diff_with_schemas_generates_drop_chain() {
        let src = source_schema();
        let tgt = target_schema();
        let proto = test_protocol();
        let d = doc(r#"{ "id": "d", "source": "app.test.post", "target": "", "from_diff": {} }"#);
        let compiled =
            compile_with_schemas(&d, "post", &src, &tgt, &proto, &null_resolver).unwrap();
        assert!(
            !compiled.chain.steps.is_empty(),
            "dropping subtitle should yield a non-empty diff chain"
        );
    }

    // -- symmetric ----------------------------------------

    #[test]
    fn symmetric_body_compiles_both_legs() {
        let d = doc(r#"{
                "id": "sym", "source": "s", "target": "",
                "symmetric": {
                    "left": [{ "remove_field": "l" }],
                    "right": [{ "remove_field": "r" }]
                }
            }"#);
        let compiled = compile(&d, "post", &null_resolver).unwrap();
        let sym = compiled.symmetric.as_ref().unwrap();
        assert!(!sym.left.steps.is_empty());
        assert!(!sym.right.steps.is_empty());
        // `chain` mirrors the left (forward) leg so downstream consumers
        // reading `.chain` never see a silent no-op.
        assert_eq!(compiled.chain.steps.len(), sym.left.steps.len());
    }

    // -- directed equations -------------------------------

    #[test]
    fn directed_equations_append_to_body_chain() {
        let d = doc(r#"{
                "id": "de", "source": "s", "target": "",
                "steps": [{ "add_sort": { "name": "x", "kind": "string" } }],
                "directed_equations": [
                    { "name": "e", "lhs": "a", "rhs": "b", "impl": "int_to_str a" }
                ]
            }"#);
        let compiled = compile(&d, "post", &null_resolver).unwrap();
        // One body step (add_sort) + one directed-eq step.
        assert_eq!(compiled.chain.steps.len(), 2);
    }
}
