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
use panproto_inst::{BoundaryProtocols, FieldTransform, RestrictPlan};
use panproto_lens::{
    AutoLensConfig, DiffSpec, KindChange, LensPath, ProtolensChain, Stringency, auto_generate,
    auto_generate_with_hints, combinators, diff_to_protolens,
};
use panproto_schema::{Edge, Protocol, Schema, primary_entry};

use crate::compose::Resolve;
use crate::document::{AutoSpec, HintStringency, LensDocument};
use crate::error::LensDslError;
use crate::path::DocumentPath;

pub use panproto_dsl_eval::SourceFormat;

/// The path of a top-level member of the document.
fn member(key: &str) -> DocumentPath {
    DocumentPath::root().key(key)
}

/// Attach the source text and the byte range of the refused element to a
/// refusal that names a document path, when `source` is JSON text in which
/// the path names a value; any other refusal is returned as it is.
fn spanned(error: LensDslError, source: &str, format: SourceFormat) -> LensDslError {
    let Some(path) = error.path().cloned() else {
        return error;
    };
    match panproto_dsl_eval::source::locate(source, format, &path) {
        Some((offset, length)) => LensDslError::Spanned {
            path: Some(path),
            src: source.to_owned(),
            span: miette::SourceSpan::new(offset.into(), length),
            error: Box::new(error),
        },
        None => error,
    }
}

/// Read a lens document from its source text.
///
/// JSON and YAML text is parsed to a value, and Nickel text evaluated to
/// one (with no import path beyond the bundled contract library), and the
/// value is read by [`crate::shape::read_document`].
///
/// # Errors
///
/// A JSON syntax error is [`LensDslError::Spanned`] around a
/// [`LensDslError::Json`], its span the one byte at which the JSON reader
/// stopped (empty at the end of the text). A YAML syntax error is
/// [`LensDslError::Yaml`]. A Nickel evaluation error is
/// [`LensDslError::NickelEvalSpanned`] when Nickel's diagnostic locates it
/// in `source` and [`LensDslError::NickelEval`] otherwise. A value the
/// grammar refuses is [`LensDslError::Shape`], wrapped in
/// [`LensDslError::Spanned`] for JSON text.
pub fn read_source(source: &str, format: SourceFormat) -> Result<LensDocument, LensDslError> {
    let value = match format {
        SourceFormat::Json => serde_json::from_str::<serde_json::Value>(source).map_err(|e| {
            let (offset, length) = panproto_dsl_eval::source::json_syntax_span(source, &e);
            LensDslError::Spanned {
                path: None,
                src: source.to_owned(),
                span: miette::SourceSpan::new(offset.into(), length),
                error: Box::new(LensDslError::Json(e)),
            }
        })?,
        SourceFormat::Yaml => crate::eval::yaml_value(source)?,
        SourceFormat::Nickel => crate::eval::nickel_value(source, &[])?,
    };
    crate::shape::read_document(&value).map_err(|e| spanned(e, source, format))
}

/// Read a lens document from its source text and compile it with
/// [`compile`], locating every refusal in `source`.
///
/// # Errors
///
/// The errors of [`read_source`] and of [`compile`]; for JSON text a
/// compile refusal is wrapped in [`LensDslError::Spanned`] whose span is the
/// byte range of the refused element's value as written.
pub fn compile_source(
    source: &str,
    format: SourceFormat,
    body_vertex: &str,
    resolver: &dyn Fn(&str) -> Option<CompiledLens>,
) -> Result<CompiledLens, LensDslError> {
    let doc = read_source(source, format)?;
    compile_with_source(&doc, source, format, body_vertex, resolver)
}

/// Compile a document read from `source`, locating every refusal in it.
///
/// # Errors
///
/// The errors of [`compile`]; for JSON text a refusal that names a
/// document path is wrapped in [`LensDslError::Spanned`] whose span is the
/// byte range of that element's value in `source`.
pub fn compile_with_source(
    doc: &LensDocument,
    source: &str,
    format: SourceFormat,
    body_vertex: &str,
    resolver: &dyn Fn(&str) -> Option<CompiledLens>,
) -> Result<CompiledLens, LensDslError> {
    compile(doc, body_vertex, resolver).map_err(|e| spanned(e, source, format))
}

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
    /// Ordered concrete-migration stages.
    ///
    /// This is authoritative for instantiation. `chain` and
    /// `field_transforms` remain as compatibility summaries, but cannot by
    /// themselves represent a value transform between two structural steps.
    pub stages: Vec<crate::steps::CompiledStage>,
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
    stages: Vec<crate::steps::CompiledStage>,
    symmetric: Option<SymmetricChains>,
}

impl CompiledLens {
    /// Instantiate the ordered structural and value-level stages.
    ///
    /// # Errors
    ///
    /// Returns a lens error when a stage cannot be instantiated, a value
    /// transform writes an undeclared target attribute, or adjacent concrete
    /// lenses cannot be composed.
    pub fn instantiate(
        &self,
        schema: &Schema,
        protocol: &Protocol,
    ) -> Result<LensPath, panproto_lens::LensError> {
        let mut composed: Option<LensPath> = None;
        for stage in &self.stages {
            let running_schema = composed
                .as_ref()
                .map_or(schema, panproto_lens::LensPath::tgt_schema);
            let mut stage_lens = stage.chain.instantiate(running_schema, protocol)?;
            for (anchor, transforms) in &stage.field_transforms {
                stage_lens
                    .compiled
                    .field_transforms
                    .entry(anchor.clone())
                    .or_default()
                    .extend(transforms.iter().cloned());
            }
            RestrictPlan::compile_with_protocols(
                &stage_lens.src_schema,
                &stage_lens.tgt_schema,
                &stage_lens.compiled,
                BoundaryProtocols {
                    src: protocol,
                    tgt: protocol,
                },
            )?;
            let stage_path = LensPath::single(stage_lens);
            composed = Some(match composed {
                Some(accumulated) => accumulated.then(&stage_path)?,
                None => stage_path,
            });
        }
        Ok(composed.unwrap_or_else(|| LensPath::identity(schema)))
    }
}

/// Validate that exactly one body variant is present, returning its name.
///
/// # Errors
///
/// [`LensDslError::Shape`] at the root with
/// [`ShapeProblem::NoBody`](crate::shape::ShapeProblem::NoBody) or
/// [`ShapeProblem::SeveralBodies`](crate::shape::ShapeProblem::SeveralBodies).
fn validate_single_body(doc: &LensDocument) -> Result<&'static str, LensDslError> {
    let present: Vec<&'static str> = [
        ("steps", doc.steps.is_some()),
        ("rules", doc.rules.is_some()),
        ("compose", doc.compose.is_some()),
        ("auto", doc.auto.is_some()),
        ("from_diff", doc.from_diff.is_some()),
        ("symmetric", doc.symmetric.is_some()),
    ]
    .into_iter()
    .filter_map(|(key, here)| here.then_some(key))
    .collect();
    match present.as_slice() {
        [] => Err(LensDslError::Shape {
            path: DocumentPath::root(),
            problem: crate::shape::ShapeProblem::NoBody,
        }),
        [only] => Ok(only),
        several => Err(LensDslError::Shape {
            path: DocumentPath::root(),
            problem: crate::shape::ShapeProblem::SeveralBodies {
                keys: several.iter().map(|key| (*key).to_owned()).collect(),
            },
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
/// Every refusal names the document path of the element it refuses:
/// [`LensDslError::Shape`] at the root for a body-count violation,
/// [`LensDslError::AutoRequiresSchemas`] at `/auto` or `/from_diff` (use
/// [`compile_with_schemas`]), [`LensDslError::NotInvertible`] at
/// `/invertible` when `invertible: true` but the lens is lossy, and the
/// body compiler's refusals under the body key.
pub fn compile(
    doc: &LensDocument,
    body_vertex: &str,
    resolver: &dyn Fn(&str) -> Option<CompiledLens>,
) -> Result<CompiledLens, LensDslError> {
    compile_with(doc, body_vertex, &|id: &str| resolver(id).map(Ok))
}

/// [`compile`] with a resolver that reports a referenced lens's refusal.
///
/// # Errors
///
/// As [`compile`], with the resolver's refusal at `/compose/lenses/i/ref`.
pub fn compile_with(
    doc: &LensDocument,
    body_vertex: &str,
    resolver: &Resolve<'_>,
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
/// instantiation fails, [`LensDslError::PlanCompilation`] when a stage's
/// migration has no executable plan, and [`LensDslError::TargetMismatch`]
/// if the produced schema's NSID does not match the declared target.
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
        "auto" => compile_auto_body(doc, source_schema, target_schema, protocol)
            .map_err(|e| e.within(&member("auto")))?,
        "from_diff" => compile_from_diff_body(doc, source_schema, target_schema)
            .map_err(|e| e.within(&member("from_diff")))?,
        _ => compile_schema_free_body(doc, variant, body_vertex, &|id: &str| resolver(id).map(Ok))?,
    };

    let compiled = finalize(doc, body)?;

    // Verify the compiled chain's output against the declared
    // target. A `symmetric` body has no single target schema, so the
    // check is not applicable there.
    if variant != "symmetric" {
        verify_target(&compiled, source_schema, protocol, variant)?;
    }

    Ok(compiled)
}

/// Compile a schema-independent body (steps, rules, compose, symmetric),
/// locating every refusal under the body key.
///
/// `auto` and `from_diff` are refused here with
/// [`LensDslError::AutoRequiresSchemas`].
fn compile_schema_free_body(
    doc: &LensDocument,
    variant: &'static str,
    body_vertex: &str,
    resolver: &Resolve<'_>,
) -> Result<BodyOutput, LensDslError> {
    let located = |e: LensDslError| e.within(&member(variant));
    let compiled = match variant {
        "steps" => {
            crate::steps::compile_steps(doc.steps.as_deref().unwrap_or_default(), body_vertex)
                .map_err(located)?
        }
        "rules" => crate::rules::compile_rules(
            doc.rules.as_deref().unwrap_or_default(),
            doc.passthrough,
            body_vertex,
        )
        .map_err(located)?,
        "compose" => match &doc.compose {
            Some(compose) => crate::compose::compile_compose_with(compose, body_vertex, resolver)
                .map_err(located)?,
            None => crate::steps::concat(Vec::new()),
        },
        "symmetric" => return compile_symmetric(doc, body_vertex).map_err(located),
        // `auto` and `from_diff` require schema context.
        _ => {
            return Err(
                LensDslError::AutoRequiresSchemas { id: doc.id.clone() }.within(&member(variant))
            );
        }
    };
    Ok(BodyOutput {
        chain: compiled.chain,
        field_transforms: compiled.field_transforms,
        stages: compiled.stages,
        symmetric: None,
    })
}

/// Compile a `symmetric` body: each leg's steps at the focus vertex (the
/// body vertex when `focus` is empty). The compiled lens's `chain` and
/// `stages` are the left leg's.
///
/// # Errors
///
/// A leg step's refusal at `/left/i` or `/right/i`, and
/// [`LensDslError::ValueStepInSymmetricLeg`] there for a step whose
/// compilation carries a value-level transform.
fn compile_symmetric(doc: &LensDocument, body_vertex: &str) -> Result<BodyOutput, LensDslError> {
    let Some(spec) = &doc.symmetric else {
        return Ok(BodyOutput {
            chain: ProtolensChain::new(Vec::new()),
            field_transforms: HashMap::new(),
            stages: Vec::new(),
            symmetric: None,
        });
    };
    let focus = if spec.focus.is_empty() {
        body_vertex
    } else {
        spec.focus.as_str()
    };
    let leg = |name: &str, steps: &[crate::document::Step]| {
        let mut parts = Vec::with_capacity(steps.len());
        for (i, step) in steps.iter().enumerate() {
            let here = DocumentPath::root().key(name).index(i);
            let part = crate::steps::compile_step(step, focus).map_err(|e| e.within(&here))?;
            if !part.field_transforms.is_empty() {
                return Err(LensDslError::ValueStepInSymmetricLeg.within(&here));
            }
            parts.push(part);
        }
        Ok(crate::steps::concat(parts))
    };
    let left = leg("left", &spec.left)?;
    let right = leg("right", &spec.right)?;
    Ok(BodyOutput {
        chain: left.chain.clone(),
        field_transforms: HashMap::new(),
        stages: left.stages,
        symmetric: Some(SymmetricChains {
            left: left.chain,
            right: right.chain,
        }),
    })
}

/// Compile an `auto` body via [`panproto_lens::auto_generate`].
fn compile_auto_body(
    doc: &LensDocument,
    source_schema: &Schema,
    target_schema: &Schema,
    protocol: &Protocol,
) -> Result<BodyOutput, LensDslError> {
    let spec = doc.auto.clone().unwrap_or_default();
    let spec = &spec;

    let mut config = AutoLensConfig::default();
    if let Some(enable_overlap) = spec.enable_overlap {
        config.try_overlap = enable_overlap;
    }
    if let Some(max_results) = spec.max_results {
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

    let field_transforms = result.lens.compiled.field_transforms;
    let chain = result.chain;
    Ok(BodyOutput {
        stages: vec![crate::steps::CompiledStage {
            chain: chain.clone(),
            field_transforms: field_transforms.clone(),
        }],
        chain,
        field_transforms,
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
    let diff = structural_diff(source_schema, target_schema);
    let chain = diff_to_protolens(&diff, source_schema, target_schema).map_err(|e| {
        LensDslError::Generation {
            id: doc.id.clone(),
            message: e.to_string(),
        }
    })?;
    Ok(BodyOutput {
        stages: vec![crate::steps::CompiledStage {
            chain: chain.clone(),
            field_transforms: HashMap::new(),
        }],
        chain,
        field_transforms: HashMap::new(),
        symmetric: None,
    })
}

/// Attach document metadata, append directed-equation steps, and run the
/// invertibility check.
fn finalize(doc: &LensDocument, body: BodyOutput) -> Result<CompiledLens, LensDslError> {
    let mut chain = body.chain;
    let mut stages = body.stages;

    // Directed-equation modifier: append oriented rewrites to the chain.
    if let Some(equations) = &doc.directed_equations
        && !equations.is_empty()
    {
        let eq_chain = crate::steps::compile_directed_equations(equations)
            .map_err(|e| e.within(&member("directed_equations")))?;
        chain = combinators::pipeline(vec![chain, eq_chain.clone()]);
        stages.push(crate::steps::CompiledStage {
            chain: eq_chain,
            field_transforms: HashMap::new(),
        });
    }

    let compiled = CompiledLens {
        id: doc.id.clone(),
        description: doc.description.clone(),
        source: doc.source.clone(),
        target: doc.target.clone(),
        chain,
        field_transforms: body.field_transforms,
        stages,
        extensions: doc.extensions.clone(),
        auto_spec: doc.auto.clone(),
        symmetric: body.symmetric,
        invertible: doc.invertible,
    };

    // Honor `invertible: true` by rejecting a lossy chain.
    if doc.invertible == Some(true)
        && let Some(element) = first_lossy_element(&compiled)
    {
        return Err(LensDslError::NotInvertible {
            id: doc.id.clone(),
            element,
        }
        .within(&member("invertible")));
    }

    Ok(compiled)
}

/// Return a description of the first lossy element of `compiled`, or
/// `None` if every element is lossless.
///
/// Stages are visited in order; within a stage, its chain steps in order
/// and then its field transforms, anchors in name order. A chain step is
/// lossless per [`Protolens::is_lossless`](panproto_lens::Protolens::is_lossless);
/// a field transform is lossless when its coercion class is.
fn first_lossy_element(compiled: &CompiledLens) -> Option<String> {
    for stage in &compiled.stages {
        if let Some(step) = stage.chain.steps.iter().find(|step| !step.is_lossless()) {
            return Some(format!("step `{}`", step.name));
        }
        let mut anchors: Vec<&Name> = stage.field_transforms.keys().collect();
        anchors.sort();
        for vertex in anchors {
            for transform in stage.field_transforms.get(vertex).into_iter().flatten() {
                if !transform.coercion_class().is_lossless() {
                    return Some(format!(
                        "field transform on `{vertex}` (class {:?})",
                        transform.coercion_class()
                    ));
                }
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
/// `compiled.target`. Plan compilation is always checked. The NSID check
/// is skipped when the declared target is empty or the produced schema
/// declares no entry NSID (unverifiable).
fn verify_target(
    compiled: &CompiledLens,
    source_schema: &Schema,
    protocol: &Protocol,
    variant: &str,
) -> Result<(), LensDslError> {
    let lens = compiled
        .instantiate(source_schema, protocol)
        .map_err(|error| match error {
            panproto_lens::LensError::Restrict(source) => LensDslError::PlanCompilation {
                id: compiled.id.clone(),
                source,
            },
            other => LensDslError::Generation {
                id: compiled.id.clone(),
                message: format!(
                    "cannot instantiate chain at source schema to verify target: {other}"
                ),
            },
        })
        .map_err(|e| e.within(&member(variant)))?;

    let declared = compiled.target.trim();
    if declared.is_empty() {
        return Ok(());
    }

    // The produced schema's identity is the NSID of its primary entry.
    // When absent, we cannot verify and do not fail.
    if let Some(actual) = schema_target_nsid(lens.tgt_schema())
        && actual != declared
    {
        return Err(LensDslError::TargetMismatch {
            id: compiled.id.clone(),
            declared: declared.to_owned(),
            actual,
        }
        .within(&member("target")));
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
            ..Default::default()
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

    /// Source whose operation kind and JSON label are both `text`.
    fn generated_transform_source_schema() -> Schema {
        let proto = test_protocol();
        SchemaBuilder::new(&proto)
            .entry("post")
            .vertex("post", "record", Some("app.test.post"))
            .unwrap()
            .vertex("post.text", "string", None)
            .unwrap()
            .edge("post", "post.text", "text", Some("text"))
            .unwrap()
            .build()
            .unwrap()
    }

    /// Same vertices as [`generated_transform_source_schema`], without its edge.
    fn generated_transform_target_schema() -> Schema {
        let proto = test_protocol();
        SchemaBuilder::new(&proto)
            .entry("post")
            .vertex("post", "record", Some("app.test.post"))
            .unwrap()
            .vertex("post.text", "string", None)
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
        let err = eval_json(r#"{ "id": "x", "source": "s", "target": "t" }"#).unwrap_err();
        assert!(matches!(
            err,
            LensDslError::Shape {
                problem: crate::shape::ShapeProblem::NoBody,
                ..
            }
        ));
    }

    #[test]
    fn multiple_bodies_are_rejected() {
        let err = eval_json(
            r#"{ "id": "x", "source": "s", "target": "t",
                 "steps": [{ "remove_field": "a" }],
                 "rules": [{ "pattern": { "name": "a" } }] }"#,
        )
        .unwrap_err();
        assert!(matches!(
            err,
            LensDslError::Shape {
                problem: crate::shape::ShapeProblem::SeveralBodies { .. },
                ..
            }
        ));
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

    #[test]
    fn instantiation_refuses_an_undeclared_computed_output() {
        let document = doc(r#"{
            "id": "computed", "source": "app.test.post", "target": "",
            "steps": [{"compute_field": {"target": "derived", "expr": "text"}}]
        }"#);
        let compiled = compile(&document, "post", &null_resolver).unwrap();
        let result = compiled.instantiate(&source_schema(), &test_protocol());
        assert!(matches!(result,
            Err(panproto_lens::LensError::Restrict(
                panproto_inst::RestrictError::UndeclaredTransformField { source_vertex, target_vertex, field }
            )) if source_vertex == "post" && target_vertex == "post" && field == "derived"
        ));
    }

    #[test]
    fn schema_aware_compilation_checks_outputs_even_without_a_target_nsid() {
        let schema = source_schema();
        let protocol = test_protocol();
        let document = doc(r#"{
            "id": "computed", "source": "app.test.post", "target": "",
            "steps": [{"compute_field": {"target": "derived", "expr": "text"}}]
        }"#);
        assert!(matches!(
            compile_with_schemas(&document, "post", &schema, &schema, &protocol, &null_resolver)
                .as_ref()
                .map_err(LensDslError::unlocated),
            Err(LensDslError::PlanCompilation { id, source:
                panproto_inst::RestrictError::UndeclaredTransformField { field, .. }
            }) if id == "computed" && field == "derived"
        ));
    }

    #[test]
    fn instantiation_accepts_a_declared_computed_output() {
        let schema = source_schema();
        let protocol = test_protocol();
        let document = doc(r#"{
            "id": "computed", "source": "app.test.post", "target": "",
            "steps": [{"compute_field": {"target": "subtitle", "expr": "text"}}]
        }"#);
        let compiled = compile_with_schemas(
            &document,
            "post",
            &schema,
            &schema,
            &protocol,
            &null_resolver,
        )
        .unwrap();
        assert!(compiled.instantiate(&schema, &protocol).is_ok());
    }

    // -- auto body ------------------------------------------------

    #[test]
    fn auto_body_without_schemas_errors() {
        let d = doc(r#"{ "id": "a", "source": "s", "target": "t", "auto": {} }"#);
        let err = compile(&d, "post", &null_resolver).unwrap_err();
        assert!(matches!(
            err.unlocated(),
            LensDslError::AutoRequiresSchemas { .. }
        ));
        assert_eq!(
            err.path().map(ToString::to_string).as_deref(),
            Some("/auto")
        );
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
        assert_eq!(
            serde_json::to_value(&compiled.field_transforms).unwrap(),
            serde_json::to_value(&direct.lens.compiled.field_transforms).unwrap(),
            "DSL auto must retain the generated value-level transforms"
        );
        assert_eq!(compiled.stages.len(), 1);
    }

    #[test]
    fn auto_body_retains_generated_field_transforms() {
        let src = generated_transform_source_schema();
        let tgt = generated_transform_target_schema();
        let proto = test_protocol();
        let d = doc(
            r#"{ "id": "a", "source": "app.test.post", "target": "app.test.post",
                 "auto": { "hints": { "stringency": "exploratory" } } }"#,
        );

        let compiled =
            compile_with_schemas(&d, "post", &src, &tgt, &proto, &null_resolver).unwrap();
        let direct = auto_generate(
            &src,
            &tgt,
            &proto,
            &AutoLensConfig {
                stringency: Stringency::Exploratory,
                ..AutoLensConfig::default()
            },
        )
        .unwrap();

        assert!(
            !direct.lens.compiled.field_transforms.is_empty(),
            "drop-op fixture must produce a generated value-level transform"
        );
        assert_eq!(
            serde_json::to_value(&compiled.field_transforms).unwrap(),
            serde_json::to_value(&direct.lens.compiled.field_transforms).unwrap(),
            "DSL auto must retain the generated value-level transforms"
        );
        assert_eq!(
            serde_json::to_value(&compiled.stages[0].field_transforms).unwrap(),
            serde_json::to_value(&direct.lens.compiled.field_transforms).unwrap(),
            "ordered auto stage must carry the generated transforms"
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
        assert_eq!(
            err.path().map(ToString::to_string).as_deref(),
            Some("/target")
        );
        match err.into_unlocated() {
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
        assert!(matches!(
            err.unlocated(),
            LensDslError::NotInvertible { .. }
        ));
    }

    // -- from_diff ----------------------------------------

    #[test]
    fn from_diff_without_schemas_errors() {
        let d = doc(r#"{ "id": "d", "source": "s", "target": "", "from_diff": {} }"#);
        let err = compile(&d, "post", &null_resolver).unwrap_err();
        assert!(matches!(
            err.unlocated(),
            LensDslError::AutoRequiresSchemas { .. }
        ));
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
                    { "name": "e", "lhs": "a", "rhs": "b", "impl": "int_to_str a",
                      "source_kind": "integer", "coercion": "projection" }
                ]
            }"#);
        let compiled = compile(&d, "post", &null_resolver).unwrap();
        // One body step (add_sort) + one directed-eq step.
        assert_eq!(compiled.chain.steps.len(), 2);
    }
}
