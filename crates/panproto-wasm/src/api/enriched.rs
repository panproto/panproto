//! Enriched theories: expression evaluation, schema coercion/default/merger/policy, refinements.
//!
//! Split out of the monolithic api.rs into a domain module.

use panproto_core::{
    gat::{self},
    inst::{self, WInstance},
    lens::{self},
    mig::{self},
    schema::{self},
};
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

use crate::error::WasmError;
use crate::slab::{self, Resource};

use super::graph::simplify_chain;
use super::helpers::{
    classify_optic_kind, default_protocol, eval_term_recursive, extract_migration_owned,
    lookup_builtin_protocol,
};

// ---------------------------------------------------------------------------
// Phase 8: Enriched theories (expressions, schema enrichment, analysis)
// ---------------------------------------------------------------------------

/// Evaluate a GAT term with a variable environment and a theory.
///
/// The `expr_bytes` are `MessagePack`-encoded [`Term`](panproto_core::gat::Term).
/// The `env_bytes` are `MessagePack`-encoded `Vec<(String, ModelValue)>` mapping
/// variable names to their values.
/// The `config_bytes` are `MessagePack`-encoded theory handle (`u32`) specifying
/// which theory's operations to evaluate against.
///
/// Returns `MessagePack`-encoded [`ModelValue`](panproto_core::gat::ModelValue).
///
/// # Errors
///
/// Returns `JsError` if deserialization fails or evaluation encounters
/// an unbound variable or unknown operation.
#[wasm_bindgen]
pub fn eval_expr(
    expr_bytes: &[u8],
    env_bytes: &[u8],
    config_bytes: &[u8],
) -> Result<Vec<u8>, JsError> {
    let term: gat::Term =
        rmp_serde::from_slice(expr_bytes).map_err(|e| WasmError::DeserializationFailed {
            reason: format!("expression: {e}"),
        })?;

    let env: Vec<(String, gat::ModelValue)> =
        rmp_serde::from_slice(env_bytes).map_err(|e| WasmError::DeserializationFailed {
            reason: format!("environment: {e}"),
        })?;

    let theory_handle: u32 =
        rmp_serde::from_slice(config_bytes).map_err(|e| WasmError::DeserializationFailed {
            reason: format!("config: {e}"),
        })?;

    let theory = slab::with_resource(theory_handle, |r| Ok(slab::as_theory(r)?.clone()))?;

    // Build a model from the theory for evaluation. The environment provides
    // sort carrier sets, and the theory provides operation interpretations.
    let result = eval_term_recursive(&term, &env, &theory)
        .map_err(|e| WasmError::ExprEvalFailed { reason: e })?;

    rmp_serde::to_vec_named(&result).map_err(|e| -> JsError {
        WasmError::SerializationFailed {
            reason: e.to_string(),
        }
        .into()
    })
}

/// Type-check a GAT term, verifying it is well-formed against a theory.
///
/// The `expr_bytes` are `MessagePack`-encoded
/// `{ "term": Term, "theory_handle": u32, "context": Vec<(String, String)> }`
/// where context maps variable names to their sort names.
///
/// Returns `MessagePack`-encoded `{ "well_formed": bool, "output_sort": String | null, "error": String | null }`.
///
/// # Errors
///
/// Returns `JsError` if deserialization fails.
#[wasm_bindgen]
pub fn check_expr(expr_bytes: &[u8]) -> Result<Vec<u8>, JsError> {
    #[derive(Deserialize)]
    struct CheckInput {
        term: gat::Term,
        theory_handle: u32,
        context: Vec<(String, String)>,
    }

    #[derive(Serialize)]
    struct CheckOutput {
        well_formed: bool,
        output_sort: Option<String>,
        error: Option<String>,
    }

    let input: CheckInput =
        rmp_serde::from_slice(expr_bytes).map_err(|e| WasmError::DeserializationFailed {
            reason: format!("check input: {e}"),
        })?;

    let theory = slab::with_resource(input.theory_handle, |r| Ok(slab::as_theory(r)?.clone()))?;

    let mut ctx = rustc_hash::FxHashMap::default();
    for (var_name, sort_name) in &input.context {
        ctx.insert(
            std::sync::Arc::from(var_name.as_str()),
            gat::SortExpr::Name(std::sync::Arc::from(sort_name.as_str())),
        );
    }

    let result = match gat::typecheck_term(&input.term, &ctx, &theory) {
        Ok(sort) => CheckOutput {
            well_formed: true,
            output_sort: Some(sort.to_string()),
            error: None,
        },
        Err(e) => CheckOutput {
            well_formed: false,
            output_sort: None,
            error: Some(e.to_string()),
        },
    };

    rmp_serde::to_vec_named(&result).map_err(|e| -> JsError {
        WasmError::SerializationFailed {
            reason: e.to_string(),
        }
        .into()
    })
}

/// Substitute a variable in a GAT term.
///
/// The `expr_bytes` are `MessagePack`-encoded [`Term`](panproto_core::gat::Term).
/// `var_name` is the variable to substitute.
/// The `replacement_bytes` are `MessagePack`-encoded [`Term`](panproto_core::gat::Term).
///
/// Returns `MessagePack`-encoded [`Term`](panproto_core::gat::Term).
///
/// # Errors
///
/// Returns `JsError` if deserialization fails.
#[wasm_bindgen]
pub fn substitute_expr(
    expr_bytes: &[u8],
    var_name: &str,
    replacement_bytes: &[u8],
) -> Result<Vec<u8>, JsError> {
    let term: gat::Term =
        rmp_serde::from_slice(expr_bytes).map_err(|e| WasmError::DeserializationFailed {
            reason: format!("expression: {e}"),
        })?;

    let replacement: gat::Term =
        rmp_serde::from_slice(replacement_bytes).map_err(|e| WasmError::DeserializationFailed {
            reason: format!("replacement: {e}"),
        })?;

    let mut subst = rustc_hash::FxHashMap::default();
    subst.insert(std::sync::Arc::from(var_name), replacement);

    let result = term.substitute(&subst);

    rmp_serde::to_vec_named(&result).map_err(|e| -> JsError {
        WasmError::SerializationFailed {
            reason: e.to_string(),
        }
        .into()
    })
}

/// Add a coercion to a schema.
///
/// A coercion is encoded as a protolens step between two vertex kinds.
/// `from_kind` and `to_kind` identify the source and target vertex kinds.
/// `expr_bytes` are `MessagePack`-encoded [`Term`](panproto_core::gat::Term)
/// describing the coercion expression.
///
/// Returns a handle to a new schema with the coercion applied
/// (via a generated protolens step).
///
/// # Errors
///
/// Returns `JsError` if the schema handle is invalid, deserialization fails,
/// or coercion construction fails.
#[wasm_bindgen]
pub fn schema_add_coercion(
    schema_handle: u32,
    from_kind: &str,
    to_kind: &str,
    expr_bytes: &[u8],
) -> Result<u32, JsError> {
    let schema = slab::with_resource(schema_handle, |r| Ok(slab::as_schema(r)?.clone()))?;

    let coercion_expr: panproto_core::expr::Expr =
        rmp_serde::from_slice(expr_bytes).map_err(|e| WasmError::DeserializationFailed {
            reason: format!("coercion expression: {e}"),
        })?;

    let coercion_spec = schema::CoercionSpec {
        forward: coercion_expr,
        inverse: None,
        class: gat::CoercionClass::Opaque,
    };

    let mut new_schema = schema;
    new_schema.coercions.insert(
        (gat::Name::from(from_kind), gat::Name::from(to_kind)),
        coercion_spec,
    );

    Ok(slab::alloc(Resource::Schema(std::sync::Arc::new(
        new_schema,
    ))))
}

/// Add a default value expression to a schema vertex.
///
/// The `expr_bytes` are `MessagePack`-encoded
/// [`Value`](panproto_core::inst::value::Value) for the default.
///
/// Returns a handle to a new schema with the default applied. Internally,
/// this builds a protolens `add_sort` step with the specified default value.
///
/// # Errors
///
/// Returns `JsError` if the schema handle is invalid, deserialization fails,
/// or the default cannot be applied.
#[wasm_bindgen]
pub fn schema_add_default(
    schema_handle: u32,
    vertex_name: &str,
    expr_bytes: &[u8],
) -> Result<u32, JsError> {
    let schema = slab::with_resource(schema_handle, |r| Ok(slab::as_schema(r)?.clone()))?;

    let default_value: inst::value::Value =
        rmp_serde::from_slice(expr_bytes).map_err(|e| WasmError::DeserializationFailed {
            reason: format!("default value: {e}"),
        })?;

    // If the vertex already exists, we store the default as a constraint annotation.
    // If it doesn't exist, we add it via a protolens add_sort step with the default.
    if schema.has_vertex(vertex_name) {
        // Vertex exists: mutate in place.
        let mut new_schema = schema;
        let constraint = panproto_core::schema::Constraint {
            sort: "default".into(),
            value: format!("{default_value:?}"),
        };
        new_schema
            .constraints
            .entry(gat::Name::from(vertex_name))
            .or_default()
            .push(constraint);
        Ok(slab::alloc(Resource::Schema(std::sync::Arc::new(
            new_schema,
        ))))
    } else {
        // Vertex doesn't exist: add it with a default via protolens.
        let vertex_kind = gat::Name::from("string");
        let protolens = lens::protolens::elementary::add_sort(
            gat::Name::from(vertex_name),
            vertex_kind,
            default_value,
        );

        let protocol = lookup_builtin_protocol(&schema.protocol)
            .unwrap_or_else(|| default_protocol(&schema.protocol));

        let lens_obj = protolens
            .instantiate(&schema, &protocol)
            .map_err(|e| -> JsError {
                WasmError::EnrichmentFailed {
                    reason: format!("add default for '{vertex_name}': {e}"),
                }
                .into()
            })?;

        Ok(slab::alloc(Resource::Schema(std::sync::Arc::new(
            lens_obj.tgt_schema,
        ))))
    }
}

/// Add a merger expression to a schema vertex.
///
/// A merger defines how to combine two values for the same vertex during
/// a merge operation. The `expr_bytes` are `MessagePack`-encoded
/// `{ "strategy": String, "args": Vec<String> }`.
///
/// Returns a handle to a new schema with the merger annotation added.
///
/// # Errors
///
/// Returns `JsError` if the schema handle is invalid, deserialization fails,
/// or the vertex does not exist.
#[wasm_bindgen]
pub fn schema_add_merger(
    schema_handle: u32,
    vertex_name: &str,
    expr_bytes: &[u8],
) -> Result<u32, JsError> {
    #[derive(Deserialize)]
    struct MergerSpec {
        strategy: String,
        #[serde(default)]
        args: Vec<String>,
    }

    let schema = slab::with_resource(schema_handle, |r| Ok(slab::as_schema(r)?.clone()))?;

    let merger: MergerSpec =
        rmp_serde::from_slice(expr_bytes).map_err(|e| WasmError::DeserializationFailed {
            reason: format!("merger spec: {e}"),
        })?;

    if !schema.has_vertex(vertex_name) {
        return Err(WasmError::EnrichmentFailed {
            reason: format!("vertex '{vertex_name}' not found in schema"),
        }
        .into());
    }

    let mut new_schema = schema;
    let constraint_value = if merger.args.is_empty() {
        merger.strategy
    } else {
        format!("{}({})", merger.strategy, merger.args.join(", "))
    };
    let constraint = panproto_core::schema::Constraint {
        sort: "merger".into(),
        value: constraint_value,
    };
    new_schema
        .constraints
        .entry(gat::Name::from(vertex_name))
        .or_default()
        .push(constraint);

    Ok(slab::alloc(Resource::Schema(std::sync::Arc::new(
        new_schema,
    ))))
}

/// Add a conflict policy to a schema vertex.
///
/// A conflict policy controls how conflicts are resolved during merge.
/// The `expr_bytes` are `MessagePack`-encoded `{ "policy": String }`.
///
/// Returns a handle to a new schema with the policy annotation added.
///
/// # Errors
///
/// Returns `JsError` if the schema handle is invalid, deserialization fails,
/// or the vertex does not exist.
#[wasm_bindgen]
pub fn schema_add_policy(
    schema_handle: u32,
    vertex_name: &str,
    expr_bytes: &[u8],
) -> Result<u32, JsError> {
    #[derive(Deserialize)]
    struct PolicySpec {
        policy: String,
    }

    let schema = slab::with_resource(schema_handle, |r| Ok(slab::as_schema(r)?.clone()))?;

    let policy: PolicySpec =
        rmp_serde::from_slice(expr_bytes).map_err(|e| WasmError::DeserializationFailed {
            reason: format!("policy spec: {e}"),
        })?;

    if !schema.has_vertex(vertex_name) {
        return Err(WasmError::EnrichmentFailed {
            reason: format!("vertex '{vertex_name}' not found in schema"),
        }
        .into());
    }

    let mut new_schema = schema;
    let constraint = panproto_core::schema::Constraint {
        sort: "conflict_policy".into(),
        value: policy.policy,
    };
    new_schema
        .constraints
        .entry(gat::Name::from(vertex_name))
        .or_default()
        .push(constraint);

    Ok(slab::alloc(Resource::Schema(std::sync::Arc::new(
        new_schema,
    ))))
}

/// Run coverage analysis (dry-run migration).
///
/// Tests how many instances from the source schema can be successfully
/// migrated to the target schema using the compiled migration.
///
/// The `instances_bytes` are `MessagePack`-encoded `Vec<WInstance>`.
/// Returns `MessagePack`-encoded coverage report with counts and details.
///
/// # Errors
///
/// Returns `JsError` if handles are invalid, deserialization fails, or
/// analysis fails.
#[wasm_bindgen]
pub fn migration_coverage(
    compiled_handle: u32,
    instances_bytes: &[u8],
    src_schema_handle: u32,
    tgt_schema_handle: u32,
) -> Result<Vec<u8>, JsError> {
    let (compiled, _src_from_handle, _tgt_from_handle) =
        slab::with_resource(compiled_handle, extract_migration_owned)?;

    let (src_schema, tgt_schema) =
        slab::with_two_resources(src_schema_handle, tgt_schema_handle, |r1, r2| {
            let s1 = slab::as_schema(r1)?;
            let s2 = slab::as_schema(r2)?;
            Ok((s1.clone(), s2.clone()))
        })?;

    let instances: Vec<WInstance> =
        rmp_serde::from_slice(instances_bytes).map_err(|e| WasmError::DeserializationFailed {
            reason: format!("instances: {e}"),
        })?;

    let total = instances.len();
    let mut succeeded = 0u64;
    let mut failed = 0u64;
    let mut errors: Vec<String> = Vec::new();

    for (i, instance) in instances.iter().enumerate() {
        match mig::lift_wtype(&compiled, &src_schema, &tgt_schema, instance) {
            Ok(_) => succeeded += 1,
            Err(e) => {
                failed += 1;
                if errors.len() < 20 {
                    errors.push(format!("record {i}: {e}"));
                }
            }
        }
    }

    #[allow(clippy::cast_precision_loss)]
    let coverage_pct = if total > 0 {
        (succeeded as f64 / total as f64) * 100.0
    } else {
        100.0
    };

    let report = serde_json::json!({
        "total": total,
        "succeeded": succeeded,
        "failed": failed,
        "coverage_percent": coverage_pct,
        "errors": errors,
        "src_vertices": src_schema.vertex_count(),
        "tgt_vertices": tgt_schema.vertex_count(),
    });

    rmp_serde::to_vec_named(&report).map_err(|e| -> JsError {
        WasmError::SerializationFailed {
            reason: e.to_string(),
        }
        .into()
    })
}

/// Classify a protolens chain's optic kind.
///
/// Analyzes the complement constructors in the chain to determine the
/// overall optic classification:
/// - `"iso"`: all steps are lossless (complement is empty)
/// - `"lens"`: some steps have `AddedElement` complements (data is added)
/// - `"prism"`: some steps have `DroppedSortData` or `DroppedOpData` (data is removed)
/// - `"affine"`: mix of added and dropped data
/// - `"traversal"`: composite complements spanning multiple elements
///
/// # Errors
///
/// Returns `JsError` if the handle is invalid.
#[wasm_bindgen]
pub fn protolens_optic_kind(chain_handle: u32) -> Result<String, JsError> {
    let chain = slab::with_resource(chain_handle, |r| Ok(slab::as_protolens_chain(r)?.clone()))?;

    let kind = classify_optic_kind(&chain);
    Ok(kind.to_string())
}

/// Simplify a protolens chain symbolically.
///
/// Eliminates redundant steps (e.g., an `add_sort` followed by a
/// `drop_sort` of the same element, or consecutive renames that can
/// be fused). Returns a handle to the simplified chain.
///
/// # Errors
///
/// Returns `JsError` if the handle is invalid.
#[wasm_bindgen]
pub fn protolens_simplify(chain_handle: u32) -> Result<u32, JsError> {
    let chain = slab::with_resource(chain_handle, |r| Ok(slab::as_protolens_chain(r)?.clone()))?;

    let simplified = simplify_chain(&chain);
    Ok(slab::alloc(Resource::ProtolensChain(Box::new(simplified))))
}

/// Check refinement subsort relationship.
///
/// Given a base sort and two sets of constraints (encoded as `MessagePack`
/// `Vec<(String, String)>` of `(sort, value)` pairs), determines whether
/// the refinement type defined by `constraints_a` is a subsort of the
/// refinement defined by `constraints_b`.
///
/// Returns `true` if every constraint in `constraints_b` is also
/// satisfied by `constraints_a` (i.e., `A` refines at least as much as `B`).
///
/// # Errors
///
/// Returns `JsError` if deserialization fails.
#[wasm_bindgen]
pub fn refinement_subsort(
    base_sort: &str,
    sub_constraints: &[u8],
    super_constraints: &[u8],
) -> Result<bool, JsError> {
    let refined: Vec<(String, String)> =
        rmp_serde::from_slice(sub_constraints).map_err(|e| WasmError::DeserializationFailed {
            reason: format!("sub_constraints: {e}"),
        })?;

    let target: Vec<(String, String)> =
        rmp_serde::from_slice(super_constraints).map_err(|e| WasmError::DeserializationFailed {
            reason: format!("super_constraints: {e}"),
        })?;

    // The refined type is a subsort if it has at least all the constraints
    // of the target. Both refinements share the same base sort.
    let _ = base_sort;

    let refined_set: std::collections::HashSet<(&str, &str)> = refined
        .iter()
        .map(|(s, v)| (s.as_str(), v.as_str()))
        .collect();

    let is_subsort = target
        .iter()
        .all(|(s, v)| refined_set.contains(&(s.as_str(), v.as_str())));

    Ok(is_subsort)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::api::test_support;

    #[test]
    fn schema_add_default_on_existing_vertex() {
        let schema_h = test_support::schema_handle(&test_support::target_schema());
        let value = panproto_core::inst::value::Value::Int(1);
        let value_bytes = rmp_serde::to_vec_named(&value).unwrap();
        let new_h = schema_add_default(schema_h, "post", &value_bytes).unwrap();
        assert_ne!(new_h, u32::MAX, "should allocate an updated schema handle");
    }
}
