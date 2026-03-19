//! Migration specification type.
//!
//! A [`Migration`] describes a mapping between two schemas: how vertices,
//! edges, hyper-edges, and labels in the source correspond to elements
//! in the target. Resolvers handle ambiguous cases where ancestor
//! contraction produces multiple candidate edges.

use std::collections::HashMap;

use panproto_gat::Name;
use panproto_schema::Edge;
use serde::{Deserialize, Serialize};

/// A migration specification: maps between two schemas.
///
/// The vertex and edge maps define the core graph morphism. The resolver
/// and hyper-resolver handle contraction ambiguities that arise when
/// intermediate vertices are dropped.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Migration {
    /// Maps source vertex IDs to target vertex IDs.
    pub vertex_map: HashMap<Name, Name>,
    /// Maps source edges to target edges.
    #[serde(with = "panproto_schema::serde_helpers::map_as_vec")]
    pub edge_map: HashMap<Edge, Edge>,
    /// Maps source hyper-edge IDs to target hyper-edge IDs.
    pub hyper_edge_map: HashMap<Name, Name>,
    /// Maps (hyper-edge ID, label) pairs to new labels.
    #[serde(with = "panproto_schema::serde_helpers::map_as_vec")]
    pub label_map: HashMap<(Name, Name), Name>,
    /// Binary contraction resolver: `(src_vertex, tgt_vertex)` -> resolved edge.
    #[serde(with = "panproto_schema::serde_helpers::map_as_vec")]
    pub resolver: HashMap<(Name, Name), Edge>,
    /// Hyper-edge contraction resolver: maps `(hyper_edge_id, labels)` to
    /// `(target_hyper_edge_id, label_remap)`.
    #[allow(clippy::type_complexity)]
    #[serde(with = "panproto_schema::serde_helpers::map_as_vec")]
    pub hyper_resolver: HashMap<(Name, Vec<Name>), (Name, HashMap<Name, Name>)>,
    /// Expression-based resolvers for enriched migrations.
    #[serde(default, with = "panproto_schema::serde_helpers::map_as_vec_default")]
    pub expr_resolvers: HashMap<(Name, Name), panproto_expr::Expr>,
}

impl Migration {
    /// Create an identity migration for the given schema vertex and edge sets.
    ///
    /// Every vertex maps to itself and every edge maps to itself.
    #[must_use]
    pub fn identity(vertices: &[Name], edges: &[Edge]) -> Self {
        let vertex_map: HashMap<Name, Name> =
            vertices.iter().map(|v| (v.clone(), v.clone())).collect();
        let edge_map: HashMap<Edge, Edge> = edges.iter().map(|e| (e.clone(), e.clone())).collect();
        Self {
            vertex_map,
            edge_map,
            hyper_edge_map: HashMap::new(),
            label_map: HashMap::new(),
            resolver: HashMap::new(),
            hyper_resolver: HashMap::new(),
            expr_resolvers: HashMap::new(),
        }
    }

    /// Create an empty migration (no mappings).
    #[must_use]
    pub fn empty() -> Self {
        Self {
            vertex_map: HashMap::new(),
            edge_map: HashMap::new(),
            hyper_edge_map: HashMap::new(),
            label_map: HashMap::new(),
            resolver: HashMap::new(),
            hyper_resolver: HashMap::new(),
            expr_resolvers: HashMap::new(),
        }
    }
}
