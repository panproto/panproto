//! Compilation of step pipelines to `ProtolensChain` and `FieldTransform`s.
//!
//! Each [`Step`] variant maps to one or more panproto combinator or
//! elementary protolens calls. Schema-level steps produce `Protolens`
//! instances collected into a `ProtolensChain`; value-level steps
//! produce `FieldTransform`s keyed by the body vertex.

use std::collections::HashMap;
use std::sync::Arc;

use panproto_gat::{
    CoercionClass, Equation, Name, TheoryConstraint, TheoryEndofunctor, TheoryMorphism,
    TheoryTransform, ValueKind,
};
use panproto_inst::FieldTransform;
use panproto_inst::value::Value;
use panproto_lens::{ProtolensChain, combinators, elementary};

use crate::document::{CoercionKind, Step};
use crate::error::LensDslError;

/// Result of compiling a step pipeline.
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
            chains.push(combinators::nest_field(
                &*nest_field.parent,
                &*nest_field.child,
                &*nest_field.intermediate,
                &*nest_field.intermediate_kind,
                &*nest_field.edge_kind,
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
            .collect(),
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
