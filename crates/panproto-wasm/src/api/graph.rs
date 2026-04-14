//! Fiber, hom, and graph operations (functions 73-77).
//!
//! Split out of the monolithic api.rs into a domain module.

use panproto_core::{
    inst::{self, CompiledMigration, WInstance},
    lens::{self},
    schema::Schema,
};
use std::collections::HashMap;

use serde::Deserialize;
use wasm_bindgen::prelude::*;

use crate::error::WasmError;

// ---------------------------------------------------------------------------
// Fiber, hom, and graph operations (73-77)
// ---------------------------------------------------------------------------

/// Compute the fiber of a compiled migration at a specific target anchor.
///
/// Given a source instance and a migration, returns the IDs of all source
/// nodes whose remapped anchor equals the given `target_anchor`.
///
/// Both `instance_bytes` and `migration_bytes` are `MessagePack`-encoded.
/// Returns `MessagePack`-encoded `Vec<u32>`.
///
/// # Errors
///
/// Returns `JsError` if deserialization or serialization fails.
#[wasm_bindgen]
pub fn fiber_at(
    instance_bytes: &[u8],
    migration_bytes: &[u8],
    target_anchor: &str,
) -> Result<Vec<u8>, JsError> {
    let instance: WInstance =
        rmp_serde::from_slice(instance_bytes).map_err(|e| WasmError::DeserializationFailed {
            reason: format!("instance: {e}"),
        })?;

    let migration: CompiledMigration =
        rmp_serde::from_slice(migration_bytes).map_err(|e| WasmError::DeserializationFailed {
            reason: format!("migration: {e}"),
        })?;

    let name = panproto_core::gat::Name::from(target_anchor);
    let result = inst::fiber_at_anchor(&migration, &instance, &name);

    rmp_serde::to_vec(&result).map_err(|e| -> JsError {
        WasmError::SerializationFailed {
            reason: e.to_string(),
        }
        .into()
    })
}

/// Compute fibers for all target anchors simultaneously.
///
/// Returns a map from target anchor name to source node IDs. Every source
/// node appears in exactly one fiber (the fibers partition the source).
///
/// Both `instance_bytes` and `migration_bytes` are `MessagePack`-encoded.
/// Returns `MessagePack`-encoded `HashMap<String, Vec<u32>>`.
///
/// # Errors
///
/// Returns `JsError` if deserialization or serialization fails.
#[wasm_bindgen]
pub fn fiber_decomposition_wasm(
    instance_bytes: &[u8],
    migration_bytes: &[u8],
) -> Result<Vec<u8>, JsError> {
    let instance: WInstance =
        rmp_serde::from_slice(instance_bytes).map_err(|e| WasmError::DeserializationFailed {
            reason: format!("instance: {e}"),
        })?;

    let migration: CompiledMigration =
        rmp_serde::from_slice(migration_bytes).map_err(|e| WasmError::DeserializationFailed {
            reason: format!("migration: {e}"),
        })?;

    let fibers = inst::fiber_decomposition(&migration, &instance);

    let result: HashMap<String, Vec<u32>> = fibers
        .into_iter()
        .map(|(name, ids)| (name.to_string(), ids))
        .collect();

    rmp_serde::to_vec(&result).map_err(|e| -> JsError {
        WasmError::SerializationFailed {
            reason: e.to_string(),
        }
        .into()
    })
}

/// Construct the internal hom schema `[S, T]`.
///
/// For each source vertex in `S`, the hom schema contains choice vertices
/// and backward vertices encoding all possible structure-preserving maps
/// from `S` to `T`.
///
/// Both `source_schema_bytes` and `target_schema_bytes` are
/// `MessagePack`-encoded [`Schema`](panproto_core::schema::Schema).
/// Returns `MessagePack`-encoded `Schema`.
///
/// # Errors
///
/// Returns `JsError` if deserialization or serialization fails.
#[wasm_bindgen]
pub fn poly_hom(
    source_schema_bytes: &[u8],
    target_schema_bytes: &[u8],
) -> Result<Vec<u8>, JsError> {
    let source: Schema = rmp_serde::from_slice(source_schema_bytes).map_err(|e| {
        WasmError::DeserializationFailed {
            reason: format!("source schema: {e}"),
        }
    })?;

    let target: Schema = rmp_serde::from_slice(target_schema_bytes).map_err(|e| {
        WasmError::DeserializationFailed {
            reason: format!("target schema: {e}"),
        }
    })?;

    let hom = inst::hom_schema(&source, &target);

    rmp_serde::to_vec(&hom).map_err(|e| -> JsError {
        WasmError::SerializationFailed {
            reason: e.to_string(),
        }
        .into()
    })
}

/// A serializable edge in a lens graph.
#[derive(Deserialize)]
struct GraphEdge {
    /// Source schema name.
    source: String,
    /// Target schema name.
    target: String,
    /// `MessagePack`-encoded `ProtolensChain`.
    chain: Vec<u8>,
}

/// Build a [`LensGraph`] from serialized edges.
fn build_lens_graph(graph_bytes: &[u8]) -> Result<lens::LensGraph, WasmError> {
    let edges: Vec<GraphEdge> =
        rmp_serde::from_slice(graph_bytes).map_err(|e| WasmError::DeserializationFailed {
            reason: format!("graph edges: {e}"),
        })?;

    let mut graph = lens::LensGraph::new();

    for edge in edges {
        let chain: lens::ProtolensChain =
            rmp_serde::from_slice(&edge.chain).map_err(|e| WasmError::DeserializationFailed {
                reason: format!("chain for {}->{}: {e}", edge.source, edge.target),
            })?;
        let src = panproto_core::gat::Name::from(edge.source.as_str());
        let tgt = panproto_core::gat::Name::from(edge.target.as_str());
        graph.add_lens(&src, &tgt, chain);
    }

    graph.compute_distances();
    Ok(graph)
}

/// Find the cheapest conversion path between two schemas in a lens graph.
///
/// The `graph_bytes` are `MessagePack`-encoded `Vec<GraphEdge>`, where
/// each edge has `source`, `target`, and `chain` (a `MessagePack`-encoded
/// `ProtolensChain`).
///
/// Returns `MessagePack`-encoded `{ cost: f64, steps: Vec<String> }` with
/// the total cost and the schema names along the shortest path. Returns an
/// error if no path exists.
///
/// # Errors
///
/// Returns `JsError` if deserialization fails or no path exists.
#[wasm_bindgen]
pub fn preferred_conversion_path(
    graph_bytes: &[u8],
    source_schema: &str,
    target_schema: &str,
) -> Result<Vec<u8>, JsError> {
    let graph = build_lens_graph(graph_bytes)?;

    let src = panproto_core::gat::Name::from(source_schema);
    let tgt = panproto_core::gat::Name::from(target_schema);

    let (cost, chain) = graph.preferred_path(&src, &tgt).ok_or_else(|| {
        JsError::new(&format!(
            "no conversion path from {source_schema} to {target_schema}"
        ))
    })?;

    let step_names: Vec<String> = chain.steps.iter().map(|s| s.name.to_string()).collect();

    let result = serde_json::json!({
        "cost": cost,
        "steps": step_names,
    });

    rmp_serde::to_vec(&result).map_err(|e| -> JsError {
        WasmError::SerializationFailed {
            reason: e.to_string(),
        }
        .into()
    })
}

/// Compute the shortest distance between two schemas in a lens graph.
///
/// The `graph_bytes` format is the same as for
/// [`preferred_conversion_path`]. Returns [`f64::INFINITY`] if no path
/// exists.
///
/// # Errors
///
/// Returns `JsError` if deserialization fails.
#[wasm_bindgen]
pub fn conversion_distance(
    graph_bytes: &[u8],
    source_schema: &str,
    target_schema: &str,
) -> Result<f64, JsError> {
    let graph = build_lens_graph(graph_bytes)?;

    let src = panproto_core::gat::Name::from(source_schema);
    let tgt = panproto_core::gat::Name::from(target_schema);

    Ok(graph.distance(&src, &tgt))
}

/// Simplify a protolens chain by eliminating redundant steps.
///
/// Removes pairs where an `add_sort(X)` is immediately followed by
/// `drop_sort(X)` (or vice versa), and fuses consecutive renames.
pub(super) fn simplify_chain(chain: &lens::ProtolensChain) -> lens::ProtolensChain {
    let mut steps = chain.steps.clone();
    let mut changed = true;

    while changed {
        changed = false;
        let mut i = 0;
        while i + 1 < steps.len() {
            let a_name = steps[i].name.to_string();
            let b_name = steps[i + 1].name.to_string();

            // Detect add+drop or drop+add cancellations.
            let cancel = (a_name.starts_with("add_sort_")
                && b_name.starts_with("drop_sort_")
                && a_name.strip_prefix("add_sort_") == b_name.strip_prefix("drop_sort_"))
                || (a_name.starts_with("drop_sort_")
                    && b_name.starts_with("add_sort_")
                    && a_name.strip_prefix("drop_sort_") == b_name.strip_prefix("add_sort_"))
                || (a_name.starts_with("add_op_")
                    && b_name.starts_with("drop_op_")
                    && a_name.strip_prefix("add_op_") == b_name.strip_prefix("drop_op_"))
                || (a_name.starts_with("drop_op_")
                    && b_name.starts_with("add_op_")
                    && a_name.strip_prefix("drop_op_") == b_name.strip_prefix("add_op_"));

            if cancel {
                steps.remove(i + 1);
                steps.remove(i);
                changed = true;
            } else {
                i += 1;
            }
        }
    }

    lens::ProtolensChain::new(steps)
}
