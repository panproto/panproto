//! The `#[wasm_bindgen]` entry points for panproto-wasm.
//!
//! Each public function takes handles (`u32`) and/or `MessagePack` byte
//! slices, performs the requested operation, and returns either a handle
//! or serialized bytes. All errors are converted to `JsError`.
//!
//! The entry points are grouped into domain submodules; this module is a
//! facade that re-exports their public surface and owns the shared
//! [`BuildOp`] type used for schema construction.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// A serializable builder operation for constructing schemas.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "op")]
enum BuildOp {
    /// Add a vertex.
    #[serde(rename = "vertex")]
    Vertex {
        /// Vertex identifier.
        id: String,
        /// Vertex kind.
        kind: String,
        /// Optional NSID.
        nsid: Option<String>,
    },
    /// Add a binary edge.
    #[serde(rename = "edge")]
    Edge {
        /// Source vertex ID.
        src: String,
        /// Target vertex ID.
        tgt: String,
        /// Edge kind.
        kind: String,
        /// Optional edge label.
        name: Option<String>,
    },
    /// Add a constraint.
    #[serde(rename = "constraint")]
    Constraint {
        /// Vertex ID.
        vertex: String,
        /// Constraint sort.
        sort: String,
        /// Constraint value.
        value: String,
    },
    /// Add a hyper-edge connecting multiple vertices via labeled positions.
    #[serde(rename = "hyper_edge")]
    HyperEdge {
        /// Hyper-edge identifier.
        id: String,
        /// Hyper-edge kind.
        kind: String,
        /// Maps label names to vertex IDs.
        signature: HashMap<String, String>,
        /// The label that identifies the parent vertex.
        parent: String,
    },
    /// Declare required edges for a vertex.
    #[serde(rename = "required")]
    Required {
        /// The vertex that owns the requirement.
        vertex: String,
        /// The edges that are required.
        edges: Vec<panproto_core::schema::Edge>,
    },
}

mod data;
mod enriched;
mod gat;
mod graph;
mod helpers;
mod instance;
mod lens;
mod registry;
mod schema;
mod vcs;

pub use data::*;
pub use enriched::*;
pub use gat::*;
pub use graph::*;
pub use instance::*;
pub use lens::*;
pub use registry::*;
pub use schema::*;
pub use vcs::*;
// A few `#[wasm_bindgen]` entry points (expression parser, query engine)
// were historically grouped with the internal helpers; re-export them
// explicitly rather than leaking every `pub(super)` helper.
pub use helpers::{eval_func_expr, execute_query, parse_expr};
