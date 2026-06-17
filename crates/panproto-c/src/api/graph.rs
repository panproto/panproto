//! Fiber decomposition, internal hom, and lens-graph routing.
//!
//! Ported from `panproto_wasm::api::graph`, narrowed to the five entry
//! points the C ABI exposes. The WASM `WasmError`/`JsError` pair becomes
//! [`FfiError`], `rmp_serde` becomes [`crate::canonical`] (CBOR via
//! ciborium). Unlike the migration and lens domains, every graph op takes
//! its instance, migration, schema, and lens-graph inputs as CBOR values
//! (not slab handles): the source instance and compiled migration cross
//! the boundary as bytes, the hom inputs and output are CBOR `Schema`
//! values, and the lens graph arrives as a CBOR `Vec<GraphEdge>` whose
//! `chain` field is itself a CBOR-encoded [`lens::ProtolensChain`].

use std::collections::HashMap;

use panproto_core::{
    inst::{self, CompiledMigration, WInstance},
    lens,
    schema::Schema,
};
use safer_ffi::prelude::*;
use serde::{Deserialize, Serialize};

use crate::error::{FfiError, PpStatus};
use crate::panic::guard;

/// A serializable edge in a lens graph.
///
/// Mirrors the shadow type in `panproto_wasm::api::graph`: each edge names
/// a `source` and `target` schema and carries a `chain` of CBOR-encoded
/// [`lens::ProtolensChain`] bytes. The outer `Vec<GraphEdge>` is decoded
/// from one CBOR payload; each `chain` is then decoded individually.
#[derive(Deserialize)]
struct GraphEdge {
    /// Source schema name.
    source: String,
    /// Target schema name.
    target: String,
    /// CBOR-encoded [`lens::ProtolensChain`].
    chain: Vec<u8>,
}

/// The result of a preferred-path query: total cost plus the schema-name
/// trace of the protolens steps along the cheapest route.
#[derive(Serialize)]
struct PathResult {
    /// Total accumulated edge cost along the shortest path.
    cost: f64,
    /// Names of the protolens steps composing the path, in order.
    steps: Vec<String>,
}

/// Build a [`lens::LensGraph`] from CBOR-encoded edges and compute its
/// all-pairs shortest-path matrices.
fn build_lens_graph(graph_bytes: &[u8]) -> Result<lens::LensGraph, FfiError> {
    let edges: Vec<GraphEdge> = crate::canonical::decode(graph_bytes)?;

    let mut graph = lens::LensGraph::new();

    for edge in edges {
        let chain: lens::ProtolensChain = crate::canonical::decode(&edge.chain).map_err(|e| {
            FfiError::Serialization(format!("chain for {}->{}: {e}", edge.source, edge.target))
        })?;
        let src = panproto_core::gat::Name::from(edge.source.as_str());
        let tgt = panproto_core::gat::Name::from(edge.target.as_str());
        graph.add_lens(&src, &tgt, chain);
    }

    graph.compute_distances();
    Ok(graph)
}

/// Compute the fiber of a compiled migration at a target anchor.
///
/// `instance` and `migration` are CBOR-encoded [`WInstance`] and
/// [`CompiledMigration`];
/// `target_anchor` is the UTF-8 anchor name. On success, `out` receives a
/// CBOR-encoded `Vec<u32>` of source node IDs whose remapped anchor equals
/// `target_anchor`. Calls `inst::fiber_at_anchor`.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_graph_fiber_at(
    instance: c_slice::Ref<'_, u8>,
    migration: c_slice::Ref<'_, u8>,
    target_anchor: c_slice::Ref<'_, u8>,
    out: &mut repr_c::Vec<u8>,
) -> i32 {
    guard(|| {
        let instance: WInstance = crate::canonical::decode(instance.as_slice())?;
        let migration: CompiledMigration = crate::canonical::decode(migration.as_slice())?;

        let anchor = std::str::from_utf8(target_anchor.as_slice())
            .map_err(|e| FfiError::Serialization(format!("target_anchor not UTF-8: {e}")))?;
        let name = panproto_core::gat::Name::from(anchor);

        let result = inst::fiber_at_anchor(&migration, &instance, &name);

        let bytes = crate::canonical::encode(&result)?;
        *out = bytes.into();
        Ok(PpStatus::Ok)
    })
}

/// Compute fibers for all target anchors at once.
///
/// `instance` and `migration` are CBOR-encoded `WInstance` and
/// `CompiledMigration`. On success, `out` receives a CBOR-encoded
/// `HashMap<String, Vec<u32>>` partitioning the source nodes (every
/// source node appears in exactly one fiber). Calls
/// `inst::fiber_decomposition`.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_graph_fiber_decomposition(
    instance: c_slice::Ref<'_, u8>,
    migration: c_slice::Ref<'_, u8>,
    out: &mut repr_c::Vec<u8>,
) -> i32 {
    guard(|| {
        let instance: WInstance = crate::canonical::decode(instance.as_slice())?;
        let migration: CompiledMigration = crate::canonical::decode(migration.as_slice())?;

        let fibers = inst::fiber_decomposition(&migration, &instance);

        let result: HashMap<String, Vec<u32>> = fibers
            .into_iter()
            .map(|(name, ids)| (name.to_string(), ids))
            .collect();

        let bytes = crate::canonical::encode(&result)?;
        *out = bytes.into();
        Ok(PpStatus::Ok)
    })
}

/// Construct the internal hom schema `[S, T]`.
///
/// `source_schema` and `target_schema` are CBOR-encoded
/// [`Schema`](panproto_core::schema::Schema) values. For each source
/// vertex in `S`, the hom schema contains choice and backward vertices
/// encoding all structure-preserving maps from `S` to `T`. On success,
/// `out` receives the CBOR-encoded hom `Schema`. Calls `inst::hom_schema`.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_graph_poly_hom(
    source_schema: c_slice::Ref<'_, u8>,
    target_schema: c_slice::Ref<'_, u8>,
    out: &mut repr_c::Vec<u8>,
) -> i32 {
    guard(|| {
        let source: Schema = crate::canonical::decode(source_schema.as_slice())?;
        let target: Schema = crate::canonical::decode(target_schema.as_slice())?;

        let hom = inst::hom_schema(&source, &target);

        let bytes = crate::canonical::encode(&hom)?;
        *out = bytes.into();
        Ok(PpStatus::Ok)
    })
}

/// Find the cheapest conversion path between two schemas in a lens graph.
///
/// `graph` is a CBOR-encoded `Vec<GraphEdge>` (each with `source`,
/// `target`, and a CBOR-encoded `ProtolensChain`); `source_schema` and
/// `target_schema` are UTF-8 schema names. On success, `out` receives a
/// CBOR-encoded `{ cost, steps }` record giving the total cost and the
/// protolens step names along the shortest path. Calls
/// `LensGraph::preferred_path`; returns [`PpStatus::Operation`] when no
/// path exists.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_graph_preferred_path(
    graph: c_slice::Ref<'_, u8>,
    source_schema: c_slice::Ref<'_, u8>,
    target_schema: c_slice::Ref<'_, u8>,
    out: &mut repr_c::Vec<u8>,
) -> i32 {
    guard(|| {
        let graph = build_lens_graph(graph.as_slice())?;

        let src_name = std::str::from_utf8(source_schema.as_slice())
            .map_err(|e| FfiError::Serialization(format!("source_schema not UTF-8: {e}")))?;
        let tgt_name = std::str::from_utf8(target_schema.as_slice())
            .map_err(|e| FfiError::Serialization(format!("target_schema not UTF-8: {e}")))?;

        let src = panproto_core::gat::Name::from(src_name);
        let tgt = panproto_core::gat::Name::from(tgt_name);

        let (cost, chain) = graph.preferred_path(&src, &tgt).ok_or_else(|| {
            FfiError::Operation(format!("no conversion path from {src_name} to {tgt_name}"))
        })?;

        let steps: Vec<String> = chain.steps.iter().map(|s| s.name.to_string()).collect();
        let result = PathResult { cost, steps };

        let bytes = crate::canonical::encode(&result)?;
        *out = bytes.into();
        Ok(PpStatus::Ok)
    })
}

/// Compute the shortest distance between two schemas in a lens graph.
///
/// `graph` is a CBOR-encoded `Vec<GraphEdge>`; `source_schema` and
/// `target_schema` are UTF-8 schema names. On success, `out_distance`
/// receives the distance ([`f64::INFINITY`] when unreachable, the schemas
/// are unknown, or no path exists). Calls `LensGraph::distance`.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_graph_conversion_distance(
    graph: c_slice::Ref<'_, u8>,
    source_schema: c_slice::Ref<'_, u8>,
    target_schema: c_slice::Ref<'_, u8>,
    out_distance: &mut f64,
) -> i32 {
    guard(|| {
        let graph = build_lens_graph(graph.as_slice())?;

        let src_name = std::str::from_utf8(source_schema.as_slice())
            .map_err(|e| FfiError::Serialization(format!("source_schema not UTF-8: {e}")))?;
        let tgt_name = std::str::from_utf8(target_schema.as_slice())
            .map_err(|e| FfiError::Serialization(format!("target_schema not UTF-8: {e}")))?;

        let src = panproto_core::gat::Name::from(src_name);
        let tgt = panproto_core::gat::Name::from(tgt_name);

        *out_distance = graph.distance(&src, &tgt);
        Ok(PpStatus::Ok)
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use std::collections::HashMap;

    use panproto_core::gat::Name;
    use panproto_core::inst::metadata::Node;
    use panproto_core::lens::{ProtolensChain, elementary};
    use panproto_core::schema::{Schema, SchemaBuilder};

    use super::*;
    use crate::canonical::{decode, encode};

    /// A serializable edge in CBOR-encoder shape, mirroring the
    /// [`GraphEdge`] decoder the entry points consume.
    #[derive(Serialize)]
    struct EncodedEdge {
        source: String,
        target: String,
        chain: Vec<u8>,
    }

    fn slice(bytes: &[u8]) -> c_slice::Box<u8> {
        bytes.to_vec().into_boxed_slice().into()
    }

    /// A two-vertex schema: a `record` vertex with a `text` string property.
    fn simple_schema(record: &str) -> Schema {
        let proto = crate::api::helpers::default_protocol("test");
        SchemaBuilder::new(&proto)
            .vertex(record, "record", None)
            .unwrap()
            .vertex("text", "string", None)
            .unwrap()
            .edge(record, "text", "prop", Some("text"))
            .unwrap()
            .build()
            .unwrap()
    }

    /// A migration that remaps both `post` and `text` source anchors onto
    /// a single `note` target anchor, so the fiber over `note` is the
    /// whole instance.
    fn collapse_migration() -> CompiledMigration {
        let mut vertex_remap = HashMap::new();
        vertex_remap.insert(Name::from("post"), Name::from("note"));
        vertex_remap.insert(Name::from("text"), Name::from("note"));
        CompiledMigration {
            vertex_remap,
            ..CompiledMigration::default()
        }
    }

    /// A migration that maps each source anchor to a distinct target.
    fn split_migration() -> CompiledMigration {
        let mut vertex_remap = HashMap::new();
        vertex_remap.insert(Name::from("post"), Name::from("note"));
        vertex_remap.insert(Name::from("text"), Name::from("body"));
        CompiledMigration {
            vertex_remap,
            ..CompiledMigration::default()
        }
    }

    /// A two-node instance: a `post` root with a single `text` child.
    fn two_node_instance() -> WInstance {
        let mut nodes = HashMap::new();
        nodes.insert(0, Node::new(0, "post"));
        nodes.insert(1, Node::new(1, "text"));
        WInstance::new(nodes, vec![], vec![], 0, Name::from("post"))
    }

    #[test]
    fn fiber_at_collects_all_remapped_nodes() {
        let instance = encode(&two_node_instance()).unwrap();
        let migration = encode(&collapse_migration()).unwrap();
        let anchor = b"note";

        let mut out: repr_c::Vec<u8> = Vec::new().into();
        let status = pp_graph_fiber_at(
            slice(&instance).as_ref(),
            slice(&migration).as_ref(),
            slice(anchor).as_ref(),
            &mut out,
        );
        assert_eq!(status, PpStatus::Ok as i32);

        let mut ids: Vec<u32> = decode(&out).unwrap();
        ids.sort_unstable();
        assert_eq!(ids, vec![0, 1]);
    }

    #[test]
    fn fiber_at_empty_for_unknown_anchor() {
        let instance = encode(&two_node_instance()).unwrap();
        let migration = encode(&split_migration()).unwrap();

        let mut out: repr_c::Vec<u8> = Vec::new().into();
        let status = pp_graph_fiber_at(
            slice(&instance).as_ref(),
            slice(&migration).as_ref(),
            slice(b"nonexistent").as_ref(),
            &mut out,
        );
        assert_eq!(status, PpStatus::Ok as i32);

        let ids: Vec<u32> = decode(&out).unwrap();
        assert!(ids.is_empty());
    }

    #[test]
    fn fiber_decomposition_partitions_source() {
        let instance = encode(&two_node_instance()).unwrap();
        let migration = encode(&split_migration()).unwrap();

        let mut out: repr_c::Vec<u8> = Vec::new().into();
        let status = pp_graph_fiber_decomposition(
            slice(&instance).as_ref(),
            slice(&migration).as_ref(),
            &mut out,
        );
        assert_eq!(status, PpStatus::Ok as i32);

        let fibers: HashMap<String, Vec<u32>> = decode(&out).unwrap();
        assert_eq!(fibers.get("note"), Some(&vec![0]));
        assert_eq!(fibers.get("body"), Some(&vec![1]));
        // Every source node lands in exactly one fiber.
        let total: usize = fibers.values().map(Vec::len).sum();
        assert_eq!(total, 2);
    }

    #[test]
    fn fiber_decomposition_collapse_unions() {
        let instance = encode(&two_node_instance()).unwrap();
        let migration = encode(&collapse_migration()).unwrap();

        let mut out: repr_c::Vec<u8> = Vec::new().into();
        let status = pp_graph_fiber_decomposition(
            slice(&instance).as_ref(),
            slice(&migration).as_ref(),
            &mut out,
        );
        assert_eq!(status, PpStatus::Ok as i32);

        let fibers: HashMap<String, Vec<u32>> = decode(&out).unwrap();
        let mut note = fibers.get("note").cloned().unwrap();
        note.sort_unstable();
        assert_eq!(note, vec![0, 1]);
    }

    #[test]
    fn poly_hom_round_trips_to_a_schema() {
        let source = encode(&simple_schema("post")).unwrap();
        let target = encode(&simple_schema("note")).unwrap();

        let mut out: repr_c::Vec<u8> = Vec::new().into();
        let status = pp_graph_poly_hom(slice(&source).as_ref(), slice(&target).as_ref(), &mut out);
        assert_eq!(status, PpStatus::Ok as i32);

        // The output decodes as a Schema, and the hom of non-empty schemas
        // is non-empty.
        let hom: Schema = decode(&out).unwrap();
        assert!(!hom.vertices.is_empty());
    }

    /// Build a CBOR `Vec<GraphEdge>` over `a -> b -> c`, each edge carrying
    /// a single-step `drop_sort` chain (positive, finite cost).
    fn three_node_graph() -> Vec<u8> {
        let step_chain =
            encode(&ProtolensChain::new(vec![elementary::drop_sort("dropped")])).unwrap();

        let edges = vec![
            EncodedEdge {
                source: "a".into(),
                target: "b".into(),
                chain: step_chain.clone(),
            },
            EncodedEdge {
                source: "b".into(),
                target: "c".into(),
                chain: step_chain,
            },
        ];
        encode(&edges).unwrap()
    }

    #[test]
    fn preferred_path_traverses_two_hops() {
        let graph = three_node_graph();

        let mut out: repr_c::Vec<u8> = Vec::new().into();
        let status = pp_graph_preferred_path(
            slice(&graph).as_ref(),
            slice(b"a").as_ref(),
            slice(b"c").as_ref(),
            &mut out,
        );
        assert_eq!(status, PpStatus::Ok as i32);

        // The result is a CBOR map with `cost` and `steps` keys.
        let result: HashMap<String, ciborium::Value> = decode(&out).unwrap();
        assert!(result.contains_key("cost"));
        assert!(result.contains_key("steps"));
    }

    #[test]
    fn preferred_path_reports_unreachable_as_operation() {
        let graph = three_node_graph();

        let mut out: repr_c::Vec<u8> = Vec::new().into();
        let status = pp_graph_preferred_path(
            slice(&graph).as_ref(),
            slice(b"c").as_ref(),
            slice(b"a").as_ref(),
            &mut out,
        );
        assert_eq!(status, PpStatus::Operation as i32);
    }

    #[test]
    fn conversion_distance_is_finite_for_reachable() {
        let graph = three_node_graph();

        let mut dist = 0.0;
        let status = pp_graph_conversion_distance(
            slice(&graph).as_ref(),
            slice(b"a").as_ref(),
            slice(b"c").as_ref(),
            &mut dist,
        );
        assert_eq!(status, PpStatus::Ok as i32);
        assert!(dist.is_finite());
        assert!(dist >= 0.0);
    }

    #[test]
    fn conversion_distance_is_infinite_for_unreachable() {
        let graph = three_node_graph();

        let mut dist = 0.0;
        let status = pp_graph_conversion_distance(
            slice(&graph).as_ref(),
            slice(b"c").as_ref(),
            slice(b"a").as_ref(),
            &mut dist,
        );
        assert_eq!(status, PpStatus::Ok as i32);
        assert!(dist.is_infinite());
    }

    #[test]
    fn conversion_distance_unknown_schema_is_infinite() {
        let graph = three_node_graph();

        let mut dist = 0.0;
        let status = pp_graph_conversion_distance(
            slice(&graph).as_ref(),
            slice(b"zzz").as_ref(),
            slice(b"c").as_ref(),
            &mut dist,
        );
        assert_eq!(status, PpStatus::Ok as i32);
        assert!(dist.is_infinite());
    }

    #[test]
    fn fiber_at_rejects_garbage_instance() {
        let migration = encode(&collapse_migration()).unwrap();
        let bad: &[u8] = &[0xFF, 0xFE, 0xFD];

        let mut out: repr_c::Vec<u8> = Vec::new().into();
        let status = pp_graph_fiber_at(
            slice(bad).as_ref(),
            slice(&migration).as_ref(),
            slice(b"note").as_ref(),
            &mut out,
        );
        assert_eq!(status, PpStatus::Serialization as i32);
    }
}
