//! Compilation of step pipelines to `ProtolensChain` and `FieldTransform`s.
//!
//! Each [`Step`] variant maps to one or more panproto combinator or
//! elementary protolens calls. Schema-level steps produce `Protolens`
//! instances collected into a `ProtolensChain`; value-level steps
//! produce `FieldTransform`s keyed by the body vertex.

use std::collections::HashMap;
use std::sync::Arc;

use panproto_gat::{
    CoercionClass, DirectedEquation, Equation, Name, TheoryConstraint, TheoryEndofunctor,
    TheoryMorphism, TheoryTransform, ValueKind,
};
use panproto_inst::FieldTransform;
use panproto_inst::value::Value;
use panproto_lens::{ProtolensChain, coercion_laws, combinators, elementary};

/// Free-variable name under which a `coerce_sort` step binds the coerced
/// value in its forward and inverse expressions. Matches the binding the
/// instance-level coercion uses when it applies the expressions.
const COERCION_VAR: &str = "v";

use crate::document::{CoercionKind, DirectedEquationSpec, Step};
use crate::error::LensDslError;
use crate::path::DocumentPath;

/// The path of member `key` of the element a refusal is relative to.
fn at(key: &str) -> DocumentPath {
    DocumentPath::root().key(key)
}

/// The member name that selects `step`'s variant in the document grammar.
#[must_use]
pub const fn variant_name(step: &Step) -> &'static str {
    match step {
        Step::RemoveField { .. } => "remove_field",
        Step::RenameField { .. } => "rename_field",
        Step::AddField { .. } => "add_field",
        Step::ApplyExpr { .. } => "apply_expr",
        Step::ComputeField { .. } => "compute_field",
        Step::HoistField { .. } => "hoist_field",
        Step::NestField { .. } => "nest_field",
        Step::Scoped { .. } => "scoped",
        Step::Pullback { .. } => "pullback",
        Step::CoerceSort { .. } => "coerce_sort",
        Step::MergeSorts { .. } => "merge_sorts",
        Step::AddSort { .. } => "add_sort",
        Step::DropSort { .. } => "drop_sort",
        Step::RenameSort { .. } => "rename_sort",
        Step::AddOp { .. } => "add_op",
        Step::DropOp { .. } => "drop_op",
        Step::RenameOp { .. } => "rename_op",
        Step::AddEquation { .. } => "add_equation",
        Step::DropEquation { .. } => "drop_equation",
    }
}

/// Result of compiling a step pipeline.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct CompiledStage {
    /// Schema-level operations performed at this point in the pipeline.
    pub chain: ProtolensChain,
    /// Value-level operations performed by the same concrete migration.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub field_transforms: HashMap<Name, Vec<FieldTransform>>,
}

/// Result of compiling an ordered step pipeline.
#[derive(Debug)]
pub struct CompiledSteps {
    /// The schema-level protolens chain.
    pub chain: ProtolensChain,
    /// Value-level field transforms, keyed by parent vertex name.
    pub field_transforms: HashMap<Name, Vec<FieldTransform>>,
    /// Ordered execution stages. Unlike the compatibility fields above, this
    /// preserves where value-level transforms occur relative to structural
    /// changes.
    pub stages: Vec<CompiledStage>,
}

/// Compile a sequence of [`Step`]s into a [`ProtolensChain`] and
/// value-level [`FieldTransform`]s.
///
/// The `body_vertex` is the parent vertex ID under which fields
/// are added/removed (e.g., `"record:body"` for `ATProto` schemas).
/// Each step is compiled on its own, and the result is the concatenation
/// of the steps' stages in list order.
///
/// # Errors
///
/// The first step's refusal, located relative to the list: a refusal of
/// step `i` names a path starting `/i/<variant>`.
pub fn compile_steps(steps: &[Step], body_vertex: &str) -> Result<CompiledSteps, LensDslError> {
    let mut parts = Vec::with_capacity(steps.len());
    for (i, step) in steps.iter().enumerate() {
        parts.push(
            compile_step(step, body_vertex)
                .map_err(|e| e.within(&DocumentPath::root().index(i)))?,
        );
    }
    Ok(concat(parts))
}

/// Concatenate compiled parts: the stages in order, the flat chain as their
/// pipeline, and the flat transform summary keyed by anchor in part order.
#[must_use]
pub fn concat(parts: Vec<CompiledSteps>) -> CompiledSteps {
    let mut chains = Vec::with_capacity(parts.len());
    let mut transforms: HashMap<Name, Vec<FieldTransform>> = HashMap::new();
    let mut stages = Vec::new();
    for part in parts {
        chains.push(part.chain);
        for stage in &part.stages {
            for (anchor, entries) in &stage.field_transforms {
                transforms
                    .entry(anchor.clone())
                    .or_default()
                    .extend(entries.iter().cloned());
            }
        }
        stages.extend(part.stages);
    }
    CompiledSteps {
        chain: combinators::pipeline(chains),
        field_transforms: transforms,
        stages,
    }
}

/// Compile one step to its stages: one stage for every step except
/// `scoped`, which gives one per inner stage, and none for a step whose
/// chain and transforms are both empty.
///
/// # Errors
///
/// The step's refusal, located relative to the step object: its path
/// starts with the step's variant name.
pub fn compile_step(step: &Step, body_vertex: &str) -> Result<CompiledSteps, LensDslError> {
    let variant = DocumentPath::root().key(variant_name(step));
    if let Step::Scoped { scoped } = step {
        return compile_scoped(scoped).map_err(|e| e.within(&variant));
    }
    let mut step_chains = Vec::new();
    let mut step_transforms = HashMap::new();
    compile_one_step(step, body_vertex, &mut step_chains, &mut step_transforms)
        .map_err(|e| e.within(&variant))?;
    let chain = combinators::pipeline(step_chains);
    let stages = if chain.steps.is_empty() && step_transforms.is_empty() {
        Vec::new()
    } else {
        vec![CompiledStage {
            chain: chain.clone(),
            field_transforms: step_transforms.clone(),
        }]
    };
    Ok(CompiledSteps {
        chain,
        field_transforms: step_transforms,
        stages,
    })
}

/// Compile a single step, appending to chains and transforms.
fn compile_one_step(
    step: &Step,
    body_vertex: &str,
    chains: &mut Vec<ProtolensChain>,
    transforms: &mut HashMap<Name, Vec<FieldTransform>>,
) -> Result<(), LensDslError> {
    let body_key = &Name::from(body_vertex);
    match step {
        // -- High-level field combinators --
        Step::RemoveField { remove_field } => {
            let vertex_id = format!("{body_vertex}.{remove_field}");
            chains.push(combinators::remove_field(vertex_id));
        }

        Step::RenameField { rename_field } => {
            let field_vertex = format!("{body_vertex}.{}", rename_field.old);
            chains.push(combinators::rename_field(
                body_vertex,
                &*field_vertex,
                &*rename_field.old,
                &*rename_field.new,
            ));
        }

        Step::AddField { add_field } => {
            compile_add_field(add_field, body_vertex, body_key, chains, transforms)?;
        }

        Step::ApplyExpr { apply_expr } => {
            compile_apply_expr(apply_expr, body_key, transforms)?;
        }

        Step::ComputeField { compute_field } => {
            compile_compute_field(compute_field, body_key, transforms)?;
        }

        // -- Structural combinators --
        Step::HoistField { hoist_field } => {
            chains.push(combinators::hoist_field(
                &*hoist_field.parent,
                &*hoist_field.intermediate,
                &*hoist_field.child,
            ));
        }

        Step::NestField { nest_field } => {
            // Empty `old_edge_name` maps to `None` (original edge unlabeled).
            let old_edge_name = if nest_field.old_edge_name.is_empty() {
                None
            } else {
                Some(Name::from(nest_field.old_edge_name.as_str()))
            };
            // Empty new-edge labels default to the intermediate/child
            // vertex ids respectively, preserving the prior convention
            // for callers that don't distinguish vertex id from edge label.
            let parent_to_intermediate: &str = if nest_field.parent_to_intermediate.is_empty() {
                &nest_field.intermediate
            } else {
                &nest_field.parent_to_intermediate
            };
            let intermediate_to_child: &str = if nest_field.intermediate_to_child.is_empty() {
                &nest_field.child
            } else {
                &nest_field.intermediate_to_child
            };
            chains.push(combinators::nest_field(
                &*nest_field.parent,
                &*nest_field.child,
                &*nest_field.intermediate,
                &*nest_field.intermediate_kind,
                &*nest_field.edge_kind,
                old_edge_name,
                parent_to_intermediate,
                intermediate_to_child,
            ));
        }

        Step::Pullback { pullback } => {
            compile_pullback(pullback, chains);
        }

        // Theory-level operations
        step => compile_theory_step(step, chains)?,
    }

    Ok(())
}

/// Compile an `add_field` step.
fn compile_add_field(
    add_field: &crate::document::AddFieldSpec,
    body_vertex: &str,
    body_key: &Name,
    chains: &mut Vec<ProtolensChain>,
    transforms: &mut HashMap<Name, Vec<FieldTransform>>,
) -> Result<(), LensDslError> {
    let vertex_id = format!("{body_vertex}.{}", add_field.name);
    let default =
        json_to_value(&add_field.default, &add_field.kind).map_err(|e| e.within(&at("default")))?;
    chains.push(combinators::add_field_with_label(
        body_vertex,
        &*vertex_id,
        &*add_field.name,
        &*add_field.kind,
        default,
    ));

    if let Some(expr_str) = &add_field.expr {
        let expr = parse_expr(expr_str).map_err(|e| e.within(&at("expr")))?;
        transforms
            .entry(body_key.clone())
            .or_default()
            .push(FieldTransform::ComputeField {
                target_key: add_field.name.clone(),
                expr,
                inverse: None,
                coercion_class: CoercionClass::Projection,
            });
    }
    Ok(())
}

/// Compile an `apply_expr` step.
fn compile_apply_expr(
    apply_expr: &crate::document::ApplyExprSpec,
    body_key: &Name,
    transforms: &mut HashMap<Name, Vec<FieldTransform>>,
) -> Result<(), LensDslError> {
    let expr = resolve_expr(&apply_expr.expr).map_err(|e| e.within(&at("expr")))?;
    let inverse = apply_expr
        .inverse
        .as_ref()
        .map(|s| resolve_expr(s).map_err(|e| e.within(&at("inverse"))))
        .transpose()?;
    let class = apply_expr
        .coercion
        .unwrap_or(CoercionKind::Projection)
        .to_coercion_class();

    transforms
        .entry(body_key.clone())
        .or_default()
        .push(FieldTransform::ApplyExpr {
            key: apply_expr.field.clone(),
            expr,
            inverse,
            coercion_class: class,
        });
    Ok(())
}

/// Compile a `compute_field` step.
fn compile_compute_field(
    compute_field: &crate::document::ComputeFieldSpec,
    body_key: &Name,
    transforms: &mut HashMap<Name, Vec<FieldTransform>>,
) -> Result<(), LensDslError> {
    let expr = resolve_expr(&compute_field.expr).map_err(|e| e.within(&at("expr")))?;
    let inverse = compute_field
        .inverse
        .as_ref()
        .map(|s| resolve_expr(s).map_err(|e| e.within(&at("inverse"))))
        .transpose()?;
    let class = compute_field
        .coercion
        .unwrap_or(CoercionKind::Projection)
        .to_coercion_class();

    transforms
        .entry(body_key.clone())
        .or_default()
        .push(FieldTransform::ComputeField {
            target_key: compute_field.target.clone(),
            expr,
            inverse,
            coercion_class: class,
        });
    Ok(())
}

/// Compile a `scoped` step: each inner step is compiled at the focus
/// vertex, and each of its stages becomes one stage whose chain is the
/// stage's fused chain mapped over the focus (`map_items`), keeping the
/// stage's transforms.
///
/// # Errors
///
/// An inner step's refusal at `/inner/k`, and [`LensDslError::Fuse`] at
/// `/inner/k` when a stage of inner step `k` does not fuse.
fn compile_scoped(scoped: &crate::document::ScopedSpec) -> Result<CompiledSteps, LensDslError> {
    let mut parts = Vec::with_capacity(scoped.inner.len());
    for (k, step) in scoped.inner.iter().enumerate() {
        let located = |e: LensDslError| e.within(&at("inner").index(k));
        let inner = compile_step(step, &scoped.focus).map_err(located)?;
        let mut stages = Vec::with_capacity(inner.stages.len());
        for stage in inner.stages {
            let chain = if stage.chain.steps.is_empty() {
                ProtolensChain::new(Vec::new())
            } else {
                let fused = stage
                    .chain
                    .fuse()
                    .map_err(|e| located(LensDslError::Fuse { error: Box::new(e) }))?;
                ProtolensChain::new(vec![combinators::map_items(&*scoped.focus, fused)])
            };
            stages.push(CompiledStage {
                chain,
                field_transforms: stage.field_transforms,
            });
        }
        parts.push(CompiledSteps {
            chain: combinators::pipeline(stages.iter().map(|s| s.chain.clone()).collect()),
            field_transforms: HashMap::new(),
            stages,
        });
    }
    Ok(concat(parts))
}

/// Compile a `pullback` step.
fn compile_pullback(pullback: &crate::document::PullbackSpec, chains: &mut Vec<ProtolensChain>) {
    let morphism = TheoryMorphism::new(
        Arc::from(&*pullback.name),
        Arc::from(&*pullback.domain),
        Arc::from(&*pullback.codomain),
        pullback
            .sort_map
            .iter()
            .map(|(k, v)| (Arc::from(&**k), Arc::from(&**v)))
            .collect(),
        pullback
            .op_map
            .iter()
            .map(|(k, v)| (Arc::from(&**k), Arc::from(&**v)))
            .collect::<std::collections::HashMap<Arc<str>, Arc<str>>>(),
    );
    chains.push(ProtolensChain::new(vec![elementary::pullback(morphism)]));
}

/// Compile theory-level steps: coerce, merge, sort/op/equation operations.
fn compile_theory_step(step: &Step, chains: &mut Vec<ProtolensChain>) -> Result<(), LensDslError> {
    match step {
        Step::CoerceSort { coerce_sort } => {
            compile_coerce_sort(coerce_sort, chains)?;
        }
        Step::MergeSorts { merge_sorts } => {
            compile_merge_sorts(merge_sorts, chains)?;
        }
        Step::AddSort { add_sort } => {
            let default = json_to_value(&add_sort.default, &add_sort.kind)
                .map_err(|e| e.within(&at("default")))?;
            chains.push(ProtolensChain::new(vec![elementary::add_sort(
                &*add_sort.name,
                &*add_sort.kind,
                default,
            )]));
        }
        Step::DropSort { drop_sort } => {
            chains.push(ProtolensChain::new(vec![elementary::drop_sort(
                &**drop_sort,
            )]));
        }
        Step::RenameSort { rename_sort } => {
            chains.push(ProtolensChain::new(vec![elementary::rename_sort(
                &*rename_sort.old,
                &*rename_sort.new,
            )]));
        }
        Step::AddOp { add_op } => {
            chains.push(ProtolensChain::new(vec![elementary::add_op(
                &*add_op.name,
                &*add_op.src,
                &*add_op.tgt,
                &*add_op.kind,
            )]));
        }
        Step::DropOp { drop_op } => {
            chains.push(ProtolensChain::new(vec![elementary::drop_op(&**drop_op)]));
        }
        Step::RenameOp { rename_op } => {
            chains.push(ProtolensChain::new(vec![elementary::rename_op(
                &*rename_op.old,
                &*rename_op.new,
            )]));
        }
        Step::AddEquation { add_equation } => {
            let eq = Equation {
                name: Arc::from(&*add_equation.name),
                lhs: parse_term(&add_equation.lhs).map_err(|e| e.within(&at("lhs")))?,
                rhs: parse_term(&add_equation.rhs).map_err(|e| e.within(&at("rhs")))?,
            };
            chains.push(ProtolensChain::new(vec![elementary::add_equation(eq)]));
        }
        Step::DropEquation { drop_equation } => {
            chains.push(ProtolensChain::new(vec![elementary::drop_equation(
                &**drop_equation,
            )]));
        }
        // All field/value/structural steps are handled in compile_one_step.
        // If we reach here, compile_one_step has a bug.
        Step::RemoveField { .. }
        | Step::RenameField { .. }
        | Step::AddField { .. }
        | Step::ApplyExpr { .. }
        | Step::ComputeField { .. }
        | Step::HoistField { .. }
        | Step::NestField { .. }
        | Step::Pullback { .. } => {
            unreachable!("non-theory steps are dispatched in compile_one_step")
        }
        Step::Scoped { .. } => unreachable!("scoped steps are expanded by compile_steps"),
    }
    Ok(())
}

/// Compile a `coerce_sort` step.
fn compile_coerce_sort(
    coerce_sort: &crate::document::CoerceSortSpec,
    chains: &mut Vec<ProtolensChain>,
) -> Result<(), LensDslError> {
    let coercion_expr = parse_expr(&coerce_sort.expr).map_err(|e| e.within(&at("expr")))?;
    let inverse_expr = coerce_sort
        .inverse
        .as_deref()
        .map(|s| parse_expr(s).map_err(|e| e.within(&at("inverse"))))
        .transpose()?;
    let target_kind = parse_value_kind(&coerce_sort.target_kind);
    let class = coerce_sort.coercion.to_coercion_class();

    // Reject a dishonest declaration at compile time: run the declared
    // class's round-trip laws against sampled inputs of the source kind.
    // A class that fails on the samples cannot round-trip, so the step is
    // refused here rather than built and silently accepted. The check is
    // evidence, not proof: it exercises the declared laws on the drawn
    // samples only.
    let source_kind = coerce_sort
        .source_kind
        .as_deref()
        .map_or(ValueKind::Any, parse_value_kind);
    let registry = coercion_laws::CoercionSampleRegistry::with_defaults();
    coercion_laws::check_coercion_honesty(
        &coercion_expr,
        inverse_expr.as_ref(),
        class,
        source_kind,
        COERCION_VAR,
        &registry,
    )
    .map_err(|e| LensDslError::CoercionNotHonest {
        message: e.to_string(),
    })?;

    let sort_arc = Arc::from(&*coerce_sort.sort);
    let protolens = panproto_lens::Protolens {
        name: Name::from(format!("coerce_sort_{}", coerce_sort.sort)),
        source: TheoryEndofunctor {
            name: Arc::from("id"),
            precondition: TheoryConstraint::HasSort(Arc::clone(&sort_arc)),
            transform: TheoryTransform::Identity,
        },
        target: TheoryEndofunctor {
            name: Arc::from(&*format!("coerce_{}", coerce_sort.sort)),
            precondition: TheoryConstraint::HasSort(Arc::clone(&sort_arc)),
            transform: TheoryTransform::CoerceSort {
                sort_name: sort_arc,
                target_kind,
                coercion_expr,
                inverse_expr,
                coercion_class: class,
            },
        },
        complement_constructor: panproto_lens::ComplementConstructor::CoercedSortData {
            sort: Name::from(coerce_sort.sort.clone()),
            class,
        },
    };
    chains.push(ProtolensChain::new(vec![protolens]));
    Ok(())
}

/// Compile a `merge_sorts` step.
fn compile_merge_sorts(
    merge_sorts: &crate::document::MergeSortsSpec,
    chains: &mut Vec<ProtolensChain>,
) -> Result<(), LensDslError> {
    let merger_expr = parse_expr(&merge_sorts.expr).map_err(|e| e.within(&at("expr")))?;
    let protolens = elementary::merge_sorts(
        &*merge_sorts.sort_a,
        &*merge_sorts.sort_b,
        &*merge_sorts.merged,
        merger_expr,
    );
    chains.push(ProtolensChain::new(vec![protolens]));
    Ok(())
}

/// Compile a slice of [`DirectedEquationSpec`]s into a [`ProtolensChain`].
///
/// Each directed equation becomes a `directed_eq` protolens step: an
/// oriented rewrite `lhs → rhs` with a computable forward implementation
/// (and optional inverse). Its declared coercion class is checked against
/// the built-in sample registry, each sample bound to the left-hand side
/// when that side is a variable and to `x` otherwise.
///
/// # Errors
///
/// Refusals located relative to the list: [`LensDslError::ExprParse`] at
/// `/i/impl` or `/i/inverse`, [`LensDslError::TermParse`] at `/i/lhs` or
/// `/i/rhs`, and [`LensDslError::CoercionNotHonest`] at `/i` when the
/// declared class fails on the samples.
pub fn compile_directed_equations(
    equations: &[DirectedEquationSpec],
) -> Result<ProtolensChain, LensDslError> {
    let registry = coercion_laws::CoercionSampleRegistry::with_defaults();
    let mut steps = Vec::with_capacity(equations.len());
    for (i, spec) in equations.iter().enumerate() {
        let here = DocumentPath::root().index(i);
        let equation = directed_equation_from_spec(spec).map_err(|e| e.within(&here))?;
        let var = match &equation.lhs {
            panproto_gat::Term::Var(name) => name.to_string(),
            _ => "x".to_owned(),
        };
        let violations =
            coercion_laws::check_directed_equation_with_registry(&equation, &registry, &var);
        if !violations.is_empty() {
            return Err(LensDslError::CoercionNotHonest {
                message: format!(
                    "directed equation `{}` declares {:?}, which the sampled inputs falsify \
                     (evidence, not proof): {violations:?}",
                    spec.name, spec.coercion
                ),
            }
            .within(&here));
        }
        steps.push(elementary::directed_eq(equation));
    }
    Ok(ProtolensChain::new(steps))
}

/// Build a [`DirectedEquation`] from its DSL spec.
fn directed_equation_from_spec(
    spec: &DirectedEquationSpec,
) -> Result<DirectedEquation, LensDslError> {
    let impl_term = parse_expr(&spec.impl_term).map_err(|e| e.within(&at("impl")))?;
    let inverse = spec
        .inverse
        .as_deref()
        .map(|s| parse_expr(s).map_err(|e| e.within(&at("inverse"))))
        .transpose()?;
    Ok(DirectedEquation {
        name: Arc::from(&*spec.name),
        lhs: parse_term(&spec.lhs).map_err(|e| e.within(&at("lhs")))?,
        rhs: parse_term(&spec.rhs).map_err(|e| e.within(&at("rhs")))?,
        impl_term,
        inverse,
        source_kind: spec.source_kind.as_deref().map(parse_value_kind),
        target_kind: spec.target_kind.as_deref().map(parse_value_kind),
        coercion_class: spec.coercion.to_coercion_class(),
    })
}

/// Resolve the expression a step carries.
///
/// Surface syntax is parsed; an expression the rules compiler already
/// built is taken as it stands.
fn resolve_expr(source: &crate::document::ExprSource) -> Result<panproto_expr::Expr, LensDslError> {
    match source {
        crate::document::ExprSource::Source(text) => parse_expr(text),
        crate::document::ExprSource::Built(expr) => Ok((**expr).clone()),
    }
}

/// Parse a panproto expression string.
fn parse_expr(expr_str: &str) -> Result<panproto_expr::Expr, LensDslError> {
    let tokens = panproto_expr_parser::tokenize(expr_str).map_err(|e| LensDslError::ExprParse {
        message: format!("tokenization failed: {e}"),
    })?;

    panproto_expr_parser::parse(&tokens).map_err(|errors| LensDslError::ExprParse {
        message: errors
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("; "),
    })
}

/// Convert a JSON default into a panproto [`Value`].
///
/// Every JSON shape has a `Value` that carries it: an array becomes
/// [`Value::List`] and an object becomes [`Value::Unknown`], the
/// list-object and record constructors the instance algebra provides.
/// The declared `kind` is then checked against what the default
/// actually is, so a field declared `integer` with a string default is
/// reported rather than quietly given a different default from the one
/// written.
///
/// # Errors
///
/// Returns [`LensDslError::DefaultValue`] for an integer too large for
/// the engine's `i64`, and for a default whose shape contradicts the
/// declared kind.
fn json_to_value(json: &serde_json::Value, kind: &str) -> Result<Value, LensDslError> {
    let value = json_shape(json)?;
    check_default_kind(&value, kind)?;
    Ok(value)
}

/// Carry a JSON value into the instance algebra without narrowing it.
fn json_shape(json: &serde_json::Value) -> Result<Value, LensDslError> {
    Ok(match json {
        serde_json::Value::Null => Value::Null,
        serde_json::Value::String(s) => Value::Str(s.clone()),
        serde_json::Value::Bool(b) => Value::Bool(*b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::Int(i)
            } else if let Some(f) = n.as_f64().filter(|_| n.as_u64().is_none()) {
                Value::Float(f)
            } else {
                // A whole number past `i64::MAX`. Values are carried as
                // `i64`, so widening it to a float here would hand back a
                // default that is not the one the document names.
                return Err(LensDslError::DefaultValue {
                    message: format!("integer default {n} does not fit in a 64-bit signed integer"),
                });
            }
        }
        serde_json::Value::Array(items) => Value::List(
            items
                .iter()
                .map(json_shape)
                .collect::<Result<Vec<_>, _>>()?,
        ),
        serde_json::Value::Object(fields) => Value::Unknown(
            fields
                .iter()
                .map(|(key, val)| Ok((key.clone(), json_shape(val)?)))
                .collect::<Result<std::collections::HashMap<_, _>, LensDslError>>()?,
        ),
    })
}

/// Check a default against the kind the document declares for it.
///
/// A null default is accepted everywhere: it says the field has no
/// default rather than naming one of the wrong type. An integer is
/// accepted where a float is declared, since a whole number is a
/// perfectly good float default. Kinds outside the known vocabulary
/// constrain nothing.
fn check_default_kind(value: &Value, kind: &str) -> Result<(), LensDslError> {
    if matches!(value, Value::Null) {
        return Ok(());
    }
    let ok = match parse_value_kind(kind) {
        ValueKind::Bool => matches!(value, Value::Bool(_)),
        ValueKind::Int => matches!(value, Value::Int(_)),
        ValueKind::Float => matches!(value, Value::Float(_) | Value::Int(_)),
        ValueKind::Str => matches!(value, Value::Str(_)),
        ValueKind::Bytes => matches!(value, Value::Bytes(_) | Value::Str(_)),
        ValueKind::Token => matches!(value, Value::Token(_) | Value::Str(_)),
        ValueKind::Null => false,
        _ => true,
    };
    if ok {
        Ok(())
    } else {
        Err(LensDslError::DefaultValue {
            message: format!("default {value:?} does not match the declared kind {kind:?}"),
        })
    }
}

/// Parse a value kind string to [`ValueKind`].
fn parse_value_kind(s: &str) -> ValueKind {
    match s {
        "boolean" | "bool" => ValueKind::Bool,
        "integer" | "int" => ValueKind::Int,
        "float" | "number" => ValueKind::Float,
        "string" | "str" => ValueKind::Str,
        "bytes" => ValueKind::Bytes,
        "token" => ValueKind::Token,
        "null" => ValueKind::Null,
        _ => ValueKind::Any,
    }
}

/// Parse a term string into a GAT [`Term`](panproto_gat::Term).
///
/// Supports two forms:
/// - Variable: `x`, `my_var`
/// - Application: `op(arg1, arg2, ...)` with recursive arguments
///
/// This is a recursive-descent parser for the term grammar:
/// ```text
/// term  ::= ident '(' (term (',' term)*)? ')'   -- application
///          | ident                              -- variable
/// ident ::= [a-zA-Z_][a-zA-Z0-9_]*
/// ```
///
/// Anything outside that grammar is rejected. Accepting it instead
/// would file the whole malformed string away as a variable name, or
/// close an unclosed application at the end of the input, and the
/// equation built from it would silently mean something the document
/// never said.
///
/// # Errors
///
/// Returns [`LensDslError::TermParse`] for an empty term, an identifier
/// that is not one, an application whose parenthesis is never closed,
/// and trailing text after a complete term.
fn parse_term(s: &str) -> Result<panproto_gat::Term, LensDslError> {
    let trimmed = s.trim();
    let malformed = |message: String| LensDslError::TermParse {
        term: s.to_owned(),
        message,
    };

    let Some(paren_pos) = trimmed.find('(') else {
        check_identifier(trimmed, &malformed)?;
        return Ok(panproto_gat::Term::Var(Arc::from(trimmed)));
    };

    let op_name = trimmed[..paren_pos].trim();
    check_identifier(op_name, &malformed)?;

    let inner = &trimmed[paren_pos + 1..];
    let Some(close) = find_matching_paren(inner) else {
        return Err(malformed(format!(
            "the argument list opened after `{op_name}` is never closed"
        )));
    };
    let trailing = inner[close + 1..].trim();
    if !trailing.is_empty() {
        return Err(malformed(format!(
            "trailing text after the term: {trailing:?}"
        )));
    }

    let args_str = &inner[..close];
    let args = if args_str.trim().is_empty() {
        Vec::new()
    } else {
        split_top_level_commas(args_str)
            .iter()
            .map(|a| parse_term(a))
            .collect::<Result<Vec<_>, _>>()?
    };
    Ok(panproto_gat::Term::App {
        op: Arc::from(op_name),
        args,
    })
}

/// Check that `name` is an identifier the term grammar admits.
fn check_identifier(
    name: &str,
    malformed: &impl Fn(String) -> LensDslError,
) -> Result<(), LensDslError> {
    let mut chars = name.chars();
    match chars.next() {
        None => Err(malformed("an empty term names nothing".to_owned())),
        Some(first) if !(first.is_ascii_alphabetic() || first == '_') => Err(malformed(format!(
            "{name:?} is not an identifier: it starts with {first:?}"
        ))),
        Some(_) if !chars.all(|c| c.is_ascii_alphanumeric() || c == '_') => Err(malformed(
            format!("{name:?} is not an identifier: it holds a character outside [A-Za-z0-9_]"),
        )),
        Some(_) => Ok(()),
    }
}

/// Find the position of the closing ')' that matches the opening '('.
/// The input starts immediately after the opening '('.
fn find_matching_paren(s: &str) -> Option<usize> {
    let mut depth = 1u32;
    for (i, ch) in s.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

/// Split a non-blank argument list by commas at the top level (not inside
/// parentheses), each argument trimmed.
fn split_top_level_commas(s: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0u32;
    let mut start = 0;
    for (i, ch) in s.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                parts.push(s[start..i].trim());
                start = i + 1;
            }
            _ => {}
        }
    }
    // The text after the last comma is an argument too, so a trailing comma
    // leaves an empty one, which the term grammar refuses.
    parts.push(s[start..].trim());
    parts
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::document::{
        AddFieldSpec, AddSortSpec, ApplyExprSpec, CoerceSortSpec, ComputeFieldSpec,
        DirectedEquationSpec, RenameSpec,
    };

    const BODY: &str = "record:body";

    #[test]
    fn a_malformed_equation_term_is_refused() {
        // Each of these parsed before as if it meant something: the
        // first two became a variable whose name is the whole string,
        // the third closed its own argument list at end of input, and
        // the fourth dropped everything after the closing paren.
        for bad in ["1 + 2", "a b", "f(a, b", "f(a))"] {
            let steps = vec![Step::AddEquation {
                add_equation: crate::document::EquationSpec {
                    name: "law".to_owned(),
                    lhs: bad.to_owned(),
                    rhs: "x".to_owned(),
                },
            }];
            match compile_steps(&steps, BODY)
                .as_ref()
                .map_err(LensDslError::unlocated)
            {
                Err(LensDslError::TermParse { term, .. }) => assert_eq!(term, bad),
                other => panic!("term {bad:?} should be refused, got {other:?}"),
            }
        }

        // The grammar the doc comment states still parses.
        for good in ["x", "my_var", "f(a, g(b))", "f()"] {
            parse_term(good).unwrap_or_else(|e| panic!("term {good:?} should parse: {e}"));
        }
    }

    #[test]
    fn an_argument_list_with_an_empty_argument_is_refused() {
        // The text after the last comma is an argument, so `f(a,)` has an
        // empty second argument and is no term.
        for bad in ["f(a,)", "f(a, ,b)", "g(f(x,))"] {
            assert!(
                matches!(parse_term(bad), Err(LensDslError::TermParse { .. })),
                "term {bad:?} should be refused"
            );
        }
        assert_eq!(
            parse_term("f(a, b)").ok(),
            Some(panproto_gat::Term::App {
                op: Arc::from("f"),
                args: vec![
                    panproto_gat::Term::Var(Arc::from("a")),
                    panproto_gat::Term::Var(Arc::from("b")),
                ],
            })
        );
    }

    #[test]
    fn a_structured_default_is_carried_rather_than_flattened() {
        let steps = vec![Step::AddField {
            add_field: AddFieldSpec {
                name: "tags".to_owned(),
                kind: "array".to_owned(),
                default: serde_json::json!(["a", 1, {"k": true}]),
                expr: None,
            },
        }];
        // Compiling must succeed and must not have turned the array
        // into an empty string on the way through.
        compile_steps(&steps, BODY).unwrap();

        let carried = json_to_value(&serde_json::json!(["a", 1, {"k": true}]), "array").unwrap();
        let Value::List(items) = &carried else {
            panic!("an array default must become a list, got {carried:?}");
        };
        assert_eq!(items.len(), 3, "every element must survive: {items:?}");
        assert!(
            matches!(&items[2], Value::Unknown(fields) if fields.get("k") == Some(&Value::Bool(true))),
            "a nested object must survive as a record, got {:?}",
            items[2],
        );
    }

    #[test]
    fn a_default_that_contradicts_its_kind_is_refused() {
        let Err(err) = json_to_value(&serde_json::json!("nope"), "integer") else {
            panic!("a string default for an integer field must be refused");
        };
        assert!(matches!(err, LensDslError::DefaultValue { .. }), "{err:?}");

        // An integer past i64 cannot be carried, and widening it to a
        // float would change the value the document names.
        let huge: serde_json::Value = serde_json::from_str("18446744073709551615").unwrap();
        let Err(err) = json_to_value(&huge, "integer") else {
            panic!("an out-of-range integer default must be refused");
        };
        assert!(matches!(err, LensDslError::DefaultValue { .. }), "{err:?}");

        // A whole number is still a fine float default.
        json_to_value(&serde_json::json!(3), "number").unwrap();
    }

    #[test]
    fn rename_field_step_compiles_to_chain() {
        let steps = vec![Step::RenameField {
            rename_field: RenameSpec {
                old: "title".to_owned(),
                new: "heading".to_owned(),
            },
        }];
        let compiled = compile_steps(&steps, BODY).unwrap();
        assert!(
            !compiled.chain.steps.is_empty(),
            "rename_field should emit at least one protolens step"
        );
        assert!(compiled.field_transforms.is_empty());
    }

    #[test]
    fn remove_field_step_compiles_to_chain() {
        let steps = vec![Step::RemoveField {
            remove_field: "obsolete".to_owned(),
        }];
        let compiled = compile_steps(&steps, BODY).unwrap();
        assert!(!compiled.chain.steps.is_empty());
    }

    #[test]
    fn add_field_with_expr_emits_field_transform() {
        let steps = vec![Step::AddField {
            add_field: AddFieldSpec {
                name: "slug".to_owned(),
                kind: "string".to_owned(),
                default: serde_json::Value::String(String::new()),
                expr: Some("title".to_owned()),
            },
        }];
        let compiled = compile_steps(&steps, BODY).unwrap();
        // add_field emits a schema-level chain step ...
        assert!(!compiled.chain.steps.is_empty());
        // ... and, because `expr` is present, a value-level transform
        // keyed by the body vertex.
        let key = Name::from(BODY);
        let transforms = compiled.field_transforms.get(&key).unwrap();
        assert!(
            transforms
                .iter()
                .any(|t| matches!(t, FieldTransform::ComputeField { target_key, .. } if target_key == "slug")),
            "expected a ComputeField transform for `slug`"
        );
    }

    #[test]
    fn apply_expr_step_emits_field_transform_with_coercion() {
        let steps = vec![Step::ApplyExpr {
            apply_expr: ApplyExprSpec {
                field: "count".to_owned(),
                expr: "add count 1".into(),
                inverse: Some("sub count 1".into()),
                coercion: Some(CoercionKind::Iso),
            },
        }];
        let compiled = compile_steps(&steps, BODY).unwrap();
        let key = Name::from(BODY);
        let transforms = compiled.field_transforms.get(&key).unwrap();
        assert!(transforms.iter().any(|t| matches!(
            t,
            FieldTransform::ApplyExpr { key, coercion_class, inverse: Some(_), .. }
                if key == "count" && coercion_class.is_lossless()
        )));
    }

    #[test]
    fn compute_field_defaults_to_projection() {
        let steps = vec![Step::ComputeField {
            compute_field: ComputeFieldSpec {
                target: "derived".to_owned(),
                expr: "count".into(),
                inverse: None,
                coercion: None,
            },
        }];
        let compiled = compile_steps(&steps, BODY).unwrap();
        let key = Name::from(BODY);
        let transforms = compiled.field_transforms.get(&key).unwrap();
        assert!(transforms.iter().any(|t| matches!(
            t,
            FieldTransform::ComputeField { target_key, coercion_class, .. }
                if target_key == "derived" && *coercion_class == CoercionClass::Projection
        )));
    }

    #[test]
    fn add_sort_and_drop_sort_are_theory_steps() {
        let steps = vec![
            Step::AddSort {
                add_sort: AddSortSpec {
                    name: "flag".to_owned(),
                    kind: "boolean".to_owned(),
                    default: serde_json::Value::Bool(false),
                },
            },
            Step::DropSort {
                drop_sort: "legacy".to_owned(),
            },
        ];
        let compiled = compile_steps(&steps, BODY).unwrap();
        assert_eq!(
            compiled.chain.steps.len(),
            2,
            "add_sort + drop_sort should emit two chain steps"
        );
    }

    #[test]
    fn invalid_expr_reports_expr_parse_error() {
        let steps = vec![Step::ApplyExpr {
            apply_expr: ApplyExprSpec {
                field: "x".to_owned(),
                expr: "((((".into(),
                inverse: None,
                coercion: None,
            },
        }];
        let err = compile_steps(&steps, BODY).unwrap_err();
        assert!(matches!(err.unlocated(), LensDslError::ExprParse { .. }));
    }

    #[test]
    fn honest_coerce_sort_compiles() {
        // `not v` is a boolean involution: `not (not b) == b` for both
        // sample booleans, so the declared `Iso` class is honest and the
        // step compiles to a chain.
        let steps = vec![Step::CoerceSort {
            coerce_sort: CoerceSortSpec {
                sort: "flag".to_owned(),
                source_kind: Some("boolean".to_owned()),
                target_kind: "boolean".to_owned(),
                expr: "not v".to_owned(),
                inverse: Some("not v".to_owned()),
                coercion: CoercionKind::Iso,
            },
        }];
        let compiled = compile_steps(&steps, BODY).unwrap();
        assert!(
            !compiled.chain.steps.is_empty(),
            "an honest coerce_sort should emit a chain step"
        );
    }

    #[test]
    fn dishonest_coerce_sort_is_rejected() {
        // `upper v` forward with an identity inverse cannot be an `Iso`:
        // `upper` is not invertible, so `v = inverse(upper(v))` fails on
        // any string with lowercase content. The declared class is
        // dishonest and the step is refused at compile time.
        let steps = vec![Step::CoerceSort {
            coerce_sort: CoerceSortSpec {
                sort: "name".to_owned(),
                source_kind: Some("string".to_owned()),
                target_kind: "string".to_owned(),
                expr: "upper v".to_owned(),
                inverse: Some("v".to_owned()),
                coercion: CoercionKind::Iso,
            },
        }];
        let err = compile_steps(&steps, BODY).unwrap_err();
        assert!(
            matches!(err.unlocated(), LensDslError::CoercionNotHonest { .. }),
            "dishonest coercion must be rejected, got {err:?}"
        );
        // The diagnostic carries the evidence-not-proof caveat.
        assert!(
            err.to_string().contains("evidence, not proof"),
            "error text should carry the evidence-not-proof caveat: {err}"
        );
    }

    #[test]
    fn directed_equations_compile_to_chain_steps() {
        let equations = vec![
            DirectedEquationSpec {
                name: "negate_iso".to_owned(),
                lhs: "x".to_owned(),
                rhs: "negate(x)".to_owned(),
                impl_term: "not x".to_owned(),
                inverse: Some("not x".to_owned()),
                source_kind: Some("boolean".to_owned()),
                target_kind: Some("boolean".to_owned()),
                coercion: CoercionKind::Iso,
            },
            DirectedEquationSpec {
                name: "stringify".to_owned(),
                lhs: "n".to_owned(),
                rhs: "s".to_owned(),
                impl_term: "int_to_str n".to_owned(),
                inverse: None,
                source_kind: Some("integer".to_owned()),
                target_kind: None,
                coercion: CoercionKind::Projection,
            },
        ];
        let chain = compile_directed_equations(&equations).unwrap();
        assert_eq!(chain.steps.len(), 2);
        assert!(
            chain
                .steps
                .iter()
                .any(|s| s.name.as_str().contains("negate_iso")),
            "expected a directed-eq step named for `negate_iso`"
        );
    }

    #[test]
    fn directed_equation_with_bad_impl_errors() {
        let equations = vec![DirectedEquationSpec {
            name: "broken".to_owned(),
            lhs: "x".to_owned(),
            rhs: "y".to_owned(),
            impl_term: "((((".to_owned(),
            inverse: None,
            source_kind: None,
            target_kind: None,
            coercion: CoercionKind::Iso,
        }];
        let err = compile_directed_equations(&equations).unwrap_err();
        assert!(matches!(err.unlocated(), LensDslError::ExprParse { .. }));
    }
}
