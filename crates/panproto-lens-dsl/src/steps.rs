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

/// Result of compiling a step pipeline.
#[derive(Debug)]
pub struct CompiledSteps {
    /// The schema-level protolens chain.
    pub chain: ProtolensChain,
    /// Value-level field transforms, keyed by parent vertex name.
    pub field_transforms: HashMap<Name, Vec<FieldTransform>>,
}

/// Compile a sequence of [`Step`]s into a [`ProtolensChain`] and
/// value-level [`FieldTransform`]s.
///
/// The `body_vertex` is the parent vertex ID under which fields
/// are added/removed (e.g., `"record:body"` for `ATProto` schemas).
///
/// # Errors
///
/// Returns [`LensDslError::ExprParse`] if an expression string
/// cannot be parsed.
pub fn compile_steps(steps: &[Step], body_vertex: &str) -> Result<CompiledSteps, LensDslError> {
    let mut chains: Vec<ProtolensChain> = Vec::new();
    let mut transforms: HashMap<Name, Vec<FieldTransform>> = HashMap::new();
    let body_key = Name::from(body_vertex);

    for (i, step) in steps.iter().enumerate() {
        compile_one_step(
            step,
            body_vertex,
            &body_key,
            i,
            &mut chains,
            &mut transforms,
        )?;
    }

    Ok(CompiledSteps {
        chain: combinators::pipeline(chains),
        field_transforms: transforms,
    })
}

/// Compile a single step, appending to chains and transforms.
fn compile_one_step(
    step: &Step,
    body_vertex: &str,
    body_key: &Name,
    index: usize,
    chains: &mut Vec<ProtolensChain>,
    transforms: &mut HashMap<Name, Vec<FieldTransform>>,
) -> Result<(), LensDslError> {
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
            compile_add_field(add_field, body_vertex, body_key, index, chains, transforms)?;
        }

        Step::ApplyExpr { apply_expr } => {
            compile_apply_expr(apply_expr, body_key, index, transforms)?;
        }

        Step::ComputeField { compute_field } => {
            compile_compute_field(compute_field, body_key, index, transforms)?;
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

        Step::Scoped { scoped } => {
            compile_scoped(scoped, body_vertex, index, chains, transforms)?;
        }

        Step::Pullback { pullback } => {
            compile_pullback(pullback, chains);
        }

        // Theory-level operations
        step => compile_theory_step(step, index, chains)?,
    }

    Ok(())
}

/// Compile an `add_field` step.
fn compile_add_field(
    add_field: &crate::document::AddFieldSpec,
    body_vertex: &str,
    body_key: &Name,
    index: usize,
    chains: &mut Vec<ProtolensChain>,
    transforms: &mut HashMap<Name, Vec<FieldTransform>>,
) -> Result<(), LensDslError> {
    let vertex_id = format!("{body_vertex}.{}", add_field.name);
    let default = json_to_value(&add_field.default, &add_field.kind);
    chains.push(combinators::add_field(
        body_vertex,
        &*vertex_id,
        &*add_field.kind,
        default,
    ));

    if let Some(expr_str) = &add_field.expr {
        let expr = parse_expr(expr_str, &format!("add_field[{index}].expr"))?;
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
    index: usize,
    transforms: &mut HashMap<Name, Vec<FieldTransform>>,
) -> Result<(), LensDslError> {
    let expr = parse_expr(&apply_expr.expr, &format!("apply_expr[{index}].expr"))?;
    let inverse = apply_expr
        .inverse
        .as_deref()
        .map(|s| parse_expr(s, &format!("apply_expr[{index}].inverse")))
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
    index: usize,
    transforms: &mut HashMap<Name, Vec<FieldTransform>>,
) -> Result<(), LensDslError> {
    let expr = parse_expr(&compute_field.expr, &format!("compute_field[{index}].expr"))?;
    let inverse = compute_field
        .inverse
        .as_deref()
        .map(|s| parse_expr(s, &format!("compute_field[{index}].inverse")))
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

/// Compile a `scoped` step (recursive).
fn compile_scoped(
    scoped: &crate::document::ScopedSpec,
    _body_vertex: &str,
    index: usize,
    chains: &mut Vec<ProtolensChain>,
    transforms: &mut HashMap<Name, Vec<FieldTransform>>,
) -> Result<(), LensDslError> {
    // Inner steps operate on the focused element, not the top-level body.
    let inner = compile_steps(&scoped.inner, &scoped.focus)?;
    let fused = inner.chain.fuse().map_err(|e| LensDslError::ExprParse {
        step_desc: format!("scoped[{index}].inner"),
        message: format!("failed to fuse inner chain: {e}"),
    })?;
    chains.push(ProtolensChain::new(vec![combinators::map_items(
        &*scoped.focus,
        fused,
    )]));

    for (k, v) in inner.field_transforms {
        transforms.entry(k).or_default().extend(v);
    }
    Ok(())
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
fn compile_theory_step(
    step: &Step,
    index: usize,
    chains: &mut Vec<ProtolensChain>,
) -> Result<(), LensDslError> {
    match step {
        Step::CoerceSort { coerce_sort } => {
            compile_coerce_sort(coerce_sort, index, chains)?;
        }
        Step::MergeSorts { merge_sorts } => {
            compile_merge_sorts(merge_sorts, index, chains)?;
        }
        Step::AddSort { add_sort } => {
            let default = json_to_value(&add_sort.default, &add_sort.kind);
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
                lhs: parse_term(&add_equation.lhs),
                rhs: parse_term(&add_equation.rhs),
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
        | Step::Scoped { .. }
        | Step::Pullback { .. } => {
            unreachable!("non-theory steps are dispatched in compile_one_step")
        }
    }
    Ok(())
}

/// Compile a `coerce_sort` step.
fn compile_coerce_sort(
    coerce_sort: &crate::document::CoerceSortSpec,
    index: usize,
    chains: &mut Vec<ProtolensChain>,
) -> Result<(), LensDslError> {
    let coercion_expr = parse_expr(&coerce_sort.expr, &format!("coerce_sort[{index}].expr"))?;
    let inverse_expr = coerce_sort
        .inverse
        .as_deref()
        .map(|s| parse_expr(s, &format!("coerce_sort[{index}].inverse")))
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
        step_desc: format!("coerce_sort[{index}]"),
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
    index: usize,
    chains: &mut Vec<ProtolensChain>,
) -> Result<(), LensDslError> {
    let merger_expr = parse_expr(&merge_sorts.expr, &format!("merge_sorts[{index}].expr"))?;

    let first_sort = Arc::from(&*merge_sorts.sort_a);
    let second_sort = Arc::from(&*merge_sorts.sort_b);
    let merged_arc = Arc::from(&*merge_sorts.merged);

    let protolens = panproto_lens::Protolens {
        name: Name::from(format!(
            "merge_{}_{}_{}",
            merge_sorts.sort_a, merge_sorts.sort_b, merge_sorts.merged
        )),
        source: TheoryEndofunctor {
            name: Arc::from("id"),
            precondition: TheoryConstraint::All(vec![
                TheoryConstraint::HasSort(Arc::clone(&first_sort)),
                TheoryConstraint::HasSort(Arc::clone(&second_sort)),
            ]),
            transform: TheoryTransform::Identity,
        },
        target: TheoryEndofunctor {
            name: Arc::from(&*format!(
                "merge_{}_{}",
                merge_sorts.sort_a, merge_sorts.sort_b
            )),
            precondition: TheoryConstraint::All(vec![
                TheoryConstraint::HasSort(first_sort.clone()),
                TheoryConstraint::HasSort(second_sort.clone()),
            ]),
            transform: TheoryTransform::MergeSorts {
                sort_a: first_sort,
                sort_b: second_sort,
                merged_name: merged_arc,
                merger_expr,
            },
        },
        complement_constructor: panproto_lens::ComplementConstructor::Composite(vec![
            panproto_lens::ComplementConstructor::DroppedSortData {
                sort: Name::from(merge_sorts.sort_a.clone()),
            },
            panproto_lens::ComplementConstructor::DroppedSortData {
                sort: Name::from(merge_sorts.sort_b.clone()),
            },
        ]),
    };
    chains.push(ProtolensChain::new(vec![protolens]));
    Ok(())
}

/// Compile a slice of [`DirectedEquationSpec`]s into a [`ProtolensChain`].
///
/// Each directed equation becomes a `directed_eq` protolens step: an
/// oriented rewrite `lhs → rhs` with a computable forward implementation
/// (and optional inverse). This is the DSL surface for the lens crate's
/// directed-equation machinery.
///
/// # Errors
///
/// Returns [`LensDslError::ExprParse`] if an implementation or inverse
/// expression cannot be parsed.
pub fn compile_directed_equations(
    equations: &[DirectedEquationSpec],
) -> Result<ProtolensChain, LensDslError> {
    let mut steps = Vec::with_capacity(equations.len());
    for (i, spec) in equations.iter().enumerate() {
        steps.push(elementary::directed_eq(directed_equation_from_spec(
            spec, i,
        )?));
    }
    Ok(ProtolensChain::new(steps))
}

/// Build a [`DirectedEquation`] from its DSL spec.
fn directed_equation_from_spec(
    spec: &DirectedEquationSpec,
    index: usize,
) -> Result<DirectedEquation, LensDslError> {
    let impl_term = parse_expr(
        &spec.impl_term,
        &format!("directed_equations[{index}].impl"),
    )?;
    let inverse = spec
        .inverse
        .as_deref()
        .map(|s| parse_expr(s, &format!("directed_equations[{index}].inverse")))
        .transpose()?;
    Ok(DirectedEquation {
        name: Arc::from(&*spec.name),
        lhs: parse_term(&spec.lhs),
        rhs: parse_term(&spec.rhs),
        impl_term,
        inverse,
        source_kind: spec.source_kind.as_deref().map(parse_value_kind),
        target_kind: spec.target_kind.as_deref().map(parse_value_kind),
        coercion_class: spec.coercion.to_coercion_class(),
    })
}

/// Parse a panproto expression string.
fn parse_expr(expr_str: &str, step_desc: &str) -> Result<panproto_expr::Expr, LensDslError> {
    let tokens = panproto_expr_parser::tokenize(expr_str).map_err(|e| LensDslError::ExprParse {
        step_desc: step_desc.to_owned(),
        message: format!("tokenization failed: {e}"),
    })?;

    panproto_expr_parser::parse(&tokens).map_err(|errors| LensDslError::ExprParse {
        step_desc: step_desc.to_owned(),
        message: errors
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("; "),
    })
}

/// Convert a JSON value to a panproto [`Value`], using the kind hint.
fn json_to_value(json: &serde_json::Value, kind: &str) -> Value {
    match json {
        serde_json::Value::Null => Value::Null,
        serde_json::Value::String(s) => Value::Str(s.clone()),
        serde_json::Value::Number(n) => n.as_i64().map_or_else(
            || n.as_f64().map_or(Value::Int(0), Value::Float),
            Value::Int,
        ),
        serde_json::Value::Bool(b) => Value::Bool(*b),
        _ => match kind {
            "integer" => Value::Int(0),
            "number" | "float" => Value::Float(0.0),
            "boolean" => Value::Bool(false),
            _ => Value::Str(String::new()),
        },
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
/// This is a simple recursive-descent parser for the term grammar:
/// ```text
/// term  ::= ident '(' term (',' term)* ')'   -- application
///          | ident                              -- variable
/// ident ::= [a-zA-Z_][a-zA-Z0-9_]*
/// ```
fn parse_term(s: &str) -> panproto_gat::Term {
    let s = s.trim();
    s.find('(').map_or_else(
        || panproto_gat::Term::Var(Arc::from(s)),
        |paren_pos| {
            let op_name = s[..paren_pos].trim();
            let inner = &s[paren_pos + 1..];
            let close = find_matching_paren(inner).unwrap_or(inner.len());
            let args_str = &inner[..close];
            let args = split_top_level_commas(args_str)
                .iter()
                .map(|a| parse_term(a))
                .collect();
            panproto_gat::Term::App {
                op: Arc::from(op_name),
                args,
            }
        },
    )
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

/// Split a string by commas at the top level (not inside parentheses).
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
    let tail = s[start..].trim();
    if !tail.is_empty() {
        parts.push(tail);
    }
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
                expr: "add count 1".to_owned(),
                inverse: Some("sub count 1".to_owned()),
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
                expr: "count".to_owned(),
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
                expr: "((((".to_owned(),
                inverse: None,
                coercion: None,
            },
        }];
        let err = compile_steps(&steps, BODY).unwrap_err();
        assert!(matches!(err, LensDslError::ExprParse { .. }));
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
            matches!(err, LensDslError::CoercionNotHonest { .. }),
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
                name: "double".to_owned(),
                lhs: "x".to_owned(),
                rhs: "double(x)".to_owned(),
                impl_term: "mul x 2".to_owned(),
                inverse: Some("mul x 2".to_owned()),
                source_kind: Some("integer".to_owned()),
                target_kind: Some("integer".to_owned()),
                coercion: CoercionKind::Iso,
            },
            DirectedEquationSpec {
                name: "stringify".to_owned(),
                lhs: "n".to_owned(),
                rhs: "s".to_owned(),
                impl_term: "int_to_str n".to_owned(),
                inverse: None,
                source_kind: None,
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
                .any(|s| s.name.as_str().contains("double")),
            "expected a directed-eq step named for `double`"
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
        assert!(matches!(err, LensDslError::ExprParse { .. }));
    }
}
