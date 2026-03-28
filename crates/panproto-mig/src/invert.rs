//! Invertibility check and inverse construction.
//!
//! A migration is invertible if and only if:
//! - The vertex map is bijective
//! - The edge map is bijective
//! - No vertices or edges are dropped
//!
//! The inverse swaps source and target in all maps.

use std::collections::HashMap;

use panproto_gat::Name;
use panproto_schema::{Edge, Schema};
use rustc_hash::FxHashSet;

use crate::error::InvertError;
use crate::migration::Migration;

/// Check invertibility and return the inverse migration if possible.
///
/// A migration is invertible iff its vertex map and edge map are both
/// bijective and no schema elements are dropped. The inverse simply
/// reverses the direction of all mappings.
///
/// # Errors
///
/// Returns `InvertError` describing why the migration is not invertible:
/// - `NotBijective` if the vertex map maps distinct sources to the same target
/// - `EdgeNotBijective` if the edge map is not injective
/// - `DroppedVertices` if target vertices are not in the image
/// - `DroppedEdges` if target edges are not in the image
#[allow(clippy::too_many_lines)]
pub fn invert(
    migration: &Migration,
    _src: &Schema,
    tgt: &Schema,
) -> Result<Migration, InvertError> {
    // Check vertex map bijectivity.
    let mut seen_targets: FxHashSet<Name> = FxHashSet::default();
    for tgt_v in migration.vertex_map.values() {
        if !seen_targets.insert(tgt_v.clone()) {
            return Err(InvertError::NotBijective {
                detail: format!("target vertex {tgt_v} has multiple preimages"),
            });
        }
    }

    // Check surjectivity: every target vertex must be in the image.
    let dropped: Vec<String> = tgt
        .vertices
        .keys()
        .filter(|v| !seen_targets.contains(*v))
        .map(std::string::ToString::to_string)
        .collect();
    if !dropped.is_empty() {
        return Err(InvertError::DroppedVertices { dropped });
    }

    // Check edge map bijectivity.
    let mut seen_edge_targets: FxHashSet<Edge> = FxHashSet::default();
    for tgt_e in migration.edge_map.values() {
        if !seen_edge_targets.insert(tgt_e.clone()) {
            return Err(InvertError::EdgeNotBijective {
                detail: format!(
                    "target edge {} -> {} ({}) has multiple preimages",
                    tgt_e.src, tgt_e.tgt, tgt_e.kind
                ),
            });
        }
    }

    // Check edge surjectivity.
    let has_unmapped_edges = tgt.edges.keys().any(|e| !seen_edge_targets.contains(e));
    if has_unmapped_edges {
        return Err(InvertError::DroppedEdges);
    }

    // Check hyper-edge map bijectivity (injectivity).
    let mut seen_hyper_targets: FxHashSet<Name> = FxHashSet::default();
    for tgt_he in migration.hyper_edge_map.values() {
        if !seen_hyper_targets.insert(tgt_he.clone()) {
            return Err(InvertError::HyperEdgeNotBijective {
                detail: format!("target hyper-edge {tgt_he} has multiple preimages"),
            });
        }
    }

    // Check hyper-edge surjectivity: every target hyper-edge must be in the image.
    let dropped_hyper: Vec<String> = tgt
        .hyper_edges
        .keys()
        .filter(|he| !seen_hyper_targets.contains(*he))
        .map(std::string::ToString::to_string)
        .collect();
    if !dropped_hyper.is_empty() {
        return Err(InvertError::DroppedHyperEdges {
            dropped: dropped_hyper,
        });
    }

    // Build the inverse migration by swapping keys and values.
    let inv_vertex_map: HashMap<Name, Name> = migration
        .vertex_map
        .iter()
        .map(|(s, t)| (t.clone(), s.clone()))
        .collect();

    let inv_edge_map: HashMap<Edge, Edge> = migration
        .edge_map
        .iter()
        .map(|(s, t)| (t.clone(), s.clone()))
        .collect();

    let inv_hyper_edge_map: HashMap<Name, Name> = migration
        .hyper_edge_map
        .iter()
        .map(|(s, t)| (t.clone(), s.clone()))
        .collect();

    // Invert label map: swap (he_id, label) direction.
    let mut inv_label_map = HashMap::new();
    for ((he_id, old_label), new_label) in &migration.label_map {
        if let Some(inv_he_id) = migration.hyper_edge_map.get(he_id) {
            inv_label_map.insert((inv_he_id.clone(), new_label.clone()), old_label.clone());
        }
    }

    // Invert resolver: swap (src, tgt) -> edge to (inv_src, inv_tgt) -> inv_edge.
    let mut inv_resolver = HashMap::new();
    for ((src, tgt), edge) in &migration.resolver {
        let inv_src = inv_vertex_map
            .get(migration.vertex_map.get(src).unwrap_or(src))
            .cloned()
            .unwrap_or_else(|| {
                migration
                    .vertex_map
                    .iter()
                    .find(|(_, v)| *v == src)
                    .map_or_else(|| src.clone(), |(k, _)| k.clone())
            });
        let inv_tgt = inv_vertex_map
            .get(migration.vertex_map.get(tgt).unwrap_or(tgt))
            .cloned()
            .unwrap_or_else(|| {
                migration
                    .vertex_map
                    .iter()
                    .find(|(_, v)| *v == tgt)
                    .map_or_else(|| tgt.clone(), |(k, _)| k.clone())
            });
        let inv_edge = inv_edge_map.get(edge).cloned().unwrap_or_else(|| Edge {
            src: inv_src.clone(),
            tgt: inv_tgt.clone(),
            kind: edge.kind.clone(),
            name: edge.name.clone(),
        });
        inv_resolver.insert((inv_src, inv_tgt), inv_edge);
    }

    // Invert hyper_resolver: swap keys and values.
    let mut inv_hyper_resolver = HashMap::new();
    for ((he_id, labels), (tgt_he_id, label_remap)) in &migration.hyper_resolver {
        let inv_he_id = inv_hyper_edge_map
            .get(tgt_he_id)
            .cloned()
            .unwrap_or_else(|| tgt_he_id.clone());
        // Invert label_remap
        let inv_label_remap: HashMap<Name, Name> = label_remap
            .iter()
            .map(|(k, v)| (v.clone(), k.clone()))
            .collect();
        // Remap labels through the forward mapping to get the target-side labels
        let inv_labels: Vec<Name> = labels
            .iter()
            .map(|l| {
                migration
                    .vertex_map
                    .get(l)
                    .cloned()
                    .unwrap_or_else(|| l.clone())
            })
            .collect();
        inv_hyper_resolver.insert((inv_he_id, inv_labels), (he_id.clone(), inv_label_remap));
    }

    Ok(Migration {
        vertex_map: inv_vertex_map,
        edge_map: inv_edge_map,
        hyper_edge_map: inv_hyper_edge_map,
        label_map: inv_label_map,
        resolver: inv_resolver,
        hyper_resolver: inv_hyper_resolver,
        expr_resolvers: HashMap::new(),
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use panproto_schema::Vertex;

    fn test_schema(vertices: &[(&str, &str)], edges: &[Edge]) -> Schema {
        let mut vert_map = HashMap::new();
        let mut edge_map = HashMap::new();
        let mut outgoing: HashMap<Name, smallvec::SmallVec<Edge, 4>> = HashMap::new();
        let mut incoming: HashMap<Name, smallvec::SmallVec<Edge, 4>> = HashMap::new();
        let mut between: HashMap<(Name, Name), smallvec::SmallVec<Edge, 2>> = HashMap::new();

        for (id, kind) in vertices {
            vert_map.insert(
                Name::from(*id),
                Vertex {
                    id: Name::from(*id),
                    kind: Name::from(*kind),
                    nsid: None,
                },
            );
        }

        for edge in edges {
            edge_map.insert(edge.clone(), edge.kind.clone());
            outgoing
                .entry(edge.src.clone())
                .or_default()
                .push(edge.clone());
            incoming
                .entry(edge.tgt.clone())
                .or_default()
                .push(edge.clone());
            between
                .entry((edge.src.clone(), edge.tgt.clone()))
                .or_default()
                .push(edge.clone());
        }

        Schema {
            protocol: "test".into(),
            vertices: vert_map,
            edges: edge_map,
            hyper_edges: HashMap::new(),
            constraints: HashMap::new(),
            required: HashMap::new(),
            nsids: HashMap::new(),
            variants: HashMap::new(),
            orderings: HashMap::new(),
            recursion_points: HashMap::new(),
            spans: HashMap::new(),
            usage_modes: HashMap::new(),
            nominal: HashMap::new(),
            coercions: HashMap::new(),
            mergers: HashMap::new(),
            defaults: HashMap::new(),
            policies: HashMap::new(),
            outgoing,
            incoming,
            between,
        }
    }

    #[test]
    fn bijective_migration_inverts() {
        let edge = Edge {
            src: "a".into(),
            tgt: "b".into(),
            kind: "prop".into(),
            name: Some("x".into()),
        };
        let inv_edge = Edge {
            src: "A".into(),
            tgt: "B".into(),
            kind: "prop".into(),
            name: Some("x".into()),
        };

        let src = test_schema(
            &[("a", "object"), ("b", "string")],
            std::slice::from_ref(&edge),
        );
        let tgt = test_schema(
            &[("A", "object"), ("B", "string")],
            std::slice::from_ref(&inv_edge),
        );

        let mig = Migration {
            vertex_map: HashMap::from([
                (Name::from("a"), Name::from("A")),
                (Name::from("b"), Name::from("B")),
            ]),
            edge_map: HashMap::from([(edge.clone(), inv_edge.clone())]),
            hyper_edge_map: HashMap::new(),
            label_map: HashMap::new(),
            resolver: HashMap::new(),
            hyper_resolver: HashMap::new(),
            expr_resolvers: HashMap::new(),
        };

        let result = invert(&mig, &src, &tgt);
        assert!(result.is_ok(), "bijective migration should invert");
        let inv = result.unwrap_or_else(|_| panic!("invert should succeed"));
        assert_eq!(inv.vertex_map.get("A"), Some(&Name::from("a")));
        assert_eq!(inv.vertex_map.get("B"), Some(&Name::from("b")));
        assert_eq!(inv.edge_map.get(&inv_edge), Some(&edge));
    }

    #[test]
    fn non_bijective_does_not_invert() {
        let edge = Edge {
            src: "a".into(),
            tgt: "b".into(),
            kind: "prop".into(),
            name: Some("x".into()),
        };

        let src = test_schema(
            &[("a", "object"), ("b", "string"), ("c", "string")],
            std::slice::from_ref(&edge),
        );

        // Non-injective: two source vertices map to the same target.
        let mig_non_inj = Migration {
            vertex_map: HashMap::from([
                (Name::from("a"), Name::from("a")),
                (Name::from("b"), Name::from("a")),
            ]),
            edge_map: HashMap::new(),
            hyper_edge_map: HashMap::new(),
            label_map: HashMap::new(),
            resolver: HashMap::new(),
            hyper_resolver: HashMap::new(),
            expr_resolvers: HashMap::new(),
        };

        let tgt2 = test_schema(&[("a", "object")], &[]);
        let result = invert(&mig_non_inj, &src, &tgt2);
        assert!(result.is_err(), "non-bijective migration should not invert");
    }
}
