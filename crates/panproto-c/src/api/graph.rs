//! Fiber decomposition, internal hom, and lens-graph routing.
//!
//! Frozen-signature scaffold. Every entry point currently returns
//! [`PpStatus::Operation`]; the engine-wiring pass fills in the bodies
//! against `panproto_core::{inst, lens}`.

use safer_ffi::prelude::*;

use crate::error::FfiError;
use crate::panic::guard;

/// Compute the fiber of a compiled migration at a target anchor.
///
/// `instance` and `migration` are CBOR-encoded `WInstance` and
/// `CompiledMigration`; `target_anchor` is the UTF-8 anchor name. On
/// success, `out` receives a CBOR-encoded `Vec<u32>` of source node
/// IDs. Will call `inst::fiber_at_anchor`.
///
/// Stub: returns [`PpStatus::Operation`](crate::error::PpStatus::Operation)
/// until implemented in the engine-wiring pass.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_graph_fiber_at(
    instance: c_slice::Ref<'_, u8>,
    migration: c_slice::Ref<'_, u8>,
    target_anchor: c_slice::Ref<'_, u8>,
    out: &mut repr_c::Vec<u8>,
) -> i32 {
    let _ = (instance, migration, target_anchor, out);
    guard(|| {
        Err(FfiError::Operation(
            "unimplemented: pp_graph_fiber_at".into(),
        ))
    })
}

/// Compute fibers for all target anchors at once.
///
/// `instance` and `migration` are CBOR-encoded `WInstance` and
/// `CompiledMigration`. On success, `out` receives a CBOR-encoded
/// `HashMap<String, Vec<u32>>` partitioning the source nodes. Will call
/// `inst::fiber_decomposition`.
///
/// Stub: returns [`PpStatus::Operation`](crate::error::PpStatus::Operation)
/// until implemented in the engine-wiring pass.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_graph_fiber_decomposition(
    instance: c_slice::Ref<'_, u8>,
    migration: c_slice::Ref<'_, u8>,
    out: &mut repr_c::Vec<u8>,
) -> i32 {
    let _ = (instance, migration, out);
    guard(|| {
        Err(FfiError::Operation(
            "unimplemented: pp_graph_fiber_decomposition".into(),
        ))
    })
}

/// Construct the internal hom schema `[S, T]`.
///
/// `source_schema` and `target_schema` are CBOR-encoded `Schema`
/// values. On success, `out` receives the CBOR-encoded hom `Schema`.
/// Will call `inst::hom_schema`.
///
/// Stub: returns [`PpStatus::Operation`](crate::error::PpStatus::Operation)
/// until implemented in the engine-wiring pass.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_graph_poly_hom(
    source_schema: c_slice::Ref<'_, u8>,
    target_schema: c_slice::Ref<'_, u8>,
    out: &mut repr_c::Vec<u8>,
) -> i32 {
    let _ = (source_schema, target_schema, out);
    guard(|| {
        Err(FfiError::Operation(
            "unimplemented: pp_graph_poly_hom".into(),
        ))
    })
}

/// Find the cheapest conversion path between two schemas in a lens graph.
///
/// `graph` is a CBOR-encoded `Vec<GraphEdge>` (each with `source`,
/// `target`, and a CBOR-encoded `ProtolensChain`); `source_schema` and
/// `target_schema` are UTF-8 schema names. On success, `out` receives a
/// CBOR-encoded `{ cost, steps }` record. Will call
/// `LensGraph::preferred_path`.
///
/// Stub: returns [`PpStatus::Operation`](crate::error::PpStatus::Operation)
/// until implemented in the engine-wiring pass.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_graph_preferred_path(
    graph: c_slice::Ref<'_, u8>,
    source_schema: c_slice::Ref<'_, u8>,
    target_schema: c_slice::Ref<'_, u8>,
    out: &mut repr_c::Vec<u8>,
) -> i32 {
    let _ = (graph, source_schema, target_schema, out);
    guard(|| {
        Err(FfiError::Operation(
            "unimplemented: pp_graph_preferred_path".into(),
        ))
    })
}

/// Compute the shortest distance between two schemas in a lens graph.
///
/// `graph` is a CBOR-encoded `Vec<GraphEdge>`; `source_schema` and
/// `target_schema` are UTF-8 schema names. On success, `out_distance`
/// receives the distance (`f64::INFINITY` when unreachable). Will call
/// `LensGraph::distance`.
///
/// Stub: returns [`PpStatus::Operation`](crate::error::PpStatus::Operation)
/// until implemented in the engine-wiring pass.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_graph_conversion_distance(
    graph: c_slice::Ref<'_, u8>,
    source_schema: c_slice::Ref<'_, u8>,
    target_schema: c_slice::Ref<'_, u8>,
    out_distance: &mut f64,
) -> i32 {
    let _ = (graph, source_schema, target_schema, out_distance);
    guard(|| {
        Err(FfiError::Operation(
            "unimplemented: pp_graph_conversion_distance".into(),
        ))
    })
}
