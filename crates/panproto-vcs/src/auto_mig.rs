//! Automatic migration derivation from schema diffs.
//!
//! Given an old schema, a new schema, and their structural diff, derives
//! a [`Migration`] that maps surviving vertices and edges via identity.
//! This handles the common cases of additions, removals, and constraint
//! changes.
//!
//! When potential renames are detected (vertices removed in old and added
//! in new), the module uses `panproto_mig::hom_search::find_best_morphism`
//! to discover a higher-quality migration that accounts for renamed elements.
//! Detected renames from [`crate::rename_detect`] are used as initial
//! assignments to guide the homomorphism search.

use panproto_gat::Name;
use std::collections::HashMap;

use panproto_check::diff::SchemaDiff;
use panproto_mig::Migration;
use panproto_mig::hom_search::{SearchOptions, find_best_morphism, morphism_to_migration};
use panproto_schema::{Edge, Schema};
use rustc_hash::FxHashSet;

use crate::rename_detect;

/// Derive a migration from a [`SchemaDiff`] between two schemas.
///
/// The derived migration uses identity mappings for all vertices and
/// edges that survive between the old and new schemas. Resolvers and
/// hyper-resolvers are left empty — if the migration requires contraction
/// resolution, the user must supply an explicit migration file.
///
/// # Algorithm
///
/// 1. **Vertex map**: For each vertex in `old` that also exists in `new`
///    (regardless of kind changes), map `id → id`.
/// 2. **Edge map**: For each edge in `old` that also exists in `new`,
///    map it to itself. For edges whose endpoints survive but the edge
///    itself changed (due to kind change), attempt to find a matching
///    edge in `new` between the same vertices with the same name.
/// 3. **Hyper-edge map**: Identity for hyper-edges present in both.
/// 4. **Label map**: Identity for labels within surviving hyper-edges
///    whose signatures still reference surviving vertices.
/// 5. **Resolver / hyper-resolver**: Empty.
#[must_use]
pub fn derive_migration(old: &Schema, new: &Schema, diff: &SchemaDiff) -> Migration {
    let removed_verts: FxHashSet<&str> = diff.removed_vertices.iter().map(String::as_str).collect();

    let removed_edges: FxHashSet<&Edge> = diff.removed_edges.iter().collect();

    // Vertex map: identity for surviving vertices.
    let vertex_map: HashMap<Name, Name> = old
        .vertices
        .keys()
        .filter(|id| !removed_verts.contains(id.as_str()))
        .map(|id| (id.clone(), id.clone()))
        .collect();

    // Edge map: identity for surviving edges, plus attempt to remap
    // edges affected by kind changes.
    let mut edge_map: HashMap<Edge, Edge> = HashMap::new();

    for edge in old.edges.keys() {
        if removed_edges.contains(edge) {
            continue;
        }
        // Both endpoints must survive.
        if removed_verts.contains(edge.src.as_str()) || removed_verts.contains(edge.tgt.as_str()) {
            continue;
        }

        if new.edges.contains_key(edge) {
            // Edge exists identically in new schema.
            edge_map.insert(edge.clone(), edge.clone());
        } else {
            // Edge was removed from new but endpoints survive — look for
            // a matching edge with the same name between the same vertices.
            if let Some(matching) =
                find_matching_edge(new, &edge.src, &edge.tgt, edge.name.as_deref())
            {
                edge_map.insert(edge.clone(), matching);
            }
        }
    }

    // Hyper-edge map: identity for surviving hyper-edges.
    let hyper_edge_map: HashMap<Name, Name> = old
        .hyper_edges
        .keys()
        .filter(|id| new.hyper_edges.contains_key(*id))
        .map(|id| (id.clone(), id.clone()))
        .collect();

    // Label map: identity for labels within surviving hyper-edges whose
    // target vertices survive.
    let mut label_map: HashMap<(Name, Name), Name> = HashMap::new();
    for (he_id, old_he) in &old.hyper_edges {
        if let Some(new_he) = new.hyper_edges.get(he_id) {
            for (label, vertex_id) in &old_he.signature {
                // Only map labels whose target vertex survives in both.
                if vertex_map.contains_key(vertex_id) {
                    if let Some(new_label) = find_label_for_vertex(new_he, vertex_id) {
                        label_map.insert((he_id.clone(), label.clone()), new_label);
                    }
                }
            }
        }
    }

    let identity_mig = Migration {
        vertex_map,
        edge_map,
        hyper_edge_map,
        label_map,
        resolver: HashMap::new(),
        hyper_resolver: HashMap::new(),
    };

    // If there are both removed and added vertices (potential renames),
    // try to find a better migration via homomorphism search.
    if !diff.removed_vertices.is_empty() && !diff.added_vertices.is_empty() {
        if let Some(enhanced) = try_hom_search_enhancement(old, new, &identity_mig) {
            return enhanced;
        }
    }

    identity_mig
}

/// Attempt to find a better migration via homomorphism search with
/// rename detection providing initial assignments.
///
/// Returns `Some(migration)` if a higher-quality migration is found,
/// `None` otherwise.
fn try_hom_search_enhancement(
    old: &Schema,
    new: &Schema,
    identity_mig: &Migration,
) -> Option<Migration> {
    // Use detected renames as initial assignments for the search.
    let renames = rename_detect::detect_vertex_renames(old, new, 0.3);
    let mut initial: HashMap<Name, Name> = HashMap::new();
    for detected in &renames {
        initial.insert(
            Name::from(detected.rename.old.as_ref()),
            Name::from(detected.rename.new.as_ref()),
        );
    }

    let opts = SearchOptions {
        initial,
        ..SearchOptions::default()
    };

    let best = find_best_morphism(old, new, &opts)?;

    // Only use the morphism-based migration if it maps more vertices
    // than the identity-based one.
    if best.vertex_map.len() > identity_mig.vertex_map.len() {
        let mut hom_mig = morphism_to_migration(&best);
        // Preserve hyper-edge and label maps from the identity migration
        // since the homomorphism search does not cover those.
        hom_mig
            .hyper_edge_map
            .clone_from(&identity_mig.hyper_edge_map);
        hom_mig.label_map.clone_from(&identity_mig.label_map);
        Some(hom_mig)
    } else {
        None
    }
}

/// Find an edge in `schema` between `src` and `tgt` with the given `name`.
fn find_matching_edge(schema: &Schema, src: &str, tgt: &str, name: Option<&str>) -> Option<Edge> {
    schema
        .edges
        .keys()
        .find(|e| e.src == src && e.tgt == tgt && e.name.as_deref() == name)
        .cloned()
}

/// Find a label in a hyper-edge that points to the given vertex.
fn find_label_for_vertex(he: &panproto_schema::HyperEdge, vertex_id: &str) -> Option<Name> {
    he.signature
        .iter()
        .find(|(_, v)| **v == *vertex_id)
        .map(|(label, _)| label.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use panproto_check::diff::diff;
    use panproto_schema::Vertex;

    fn make_schema(vertices: &[(&str, &str)], edges: &[Edge]) -> Schema {
        let mut vert_map = HashMap::new();
        let mut edge_map = HashMap::new();

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
            outgoing: HashMap::new(),
            incoming: HashMap::new(),
            between: HashMap::new(),
        }
    }

    #[test]
    fn derive_identity_for_unchanged() {
        let s = make_schema(&[("a", "object"), ("b", "string")], &[]);
        let d = diff(&s, &s);
        let m = derive_migration(&s, &s, &d);
        assert_eq!(m.vertex_map.len(), 2);
        assert_eq!(m.vertex_map["a"], "a");
        assert_eq!(m.vertex_map["b"], "b");
    }

    #[test]
    fn derive_drops_removed_vertices() {
        let old = make_schema(&[("a", "object"), ("b", "string")], &[]);
        let new = make_schema(&[("a", "object")], &[]);
        let d = diff(&old, &new);
        let m = derive_migration(&old, &new, &d);
        assert_eq!(m.vertex_map.len(), 1);
        assert!(m.vertex_map.contains_key("a"));
        assert!(!m.vertex_map.contains_key("b"));
    }

    #[test]
    fn derive_keeps_edges_with_surviving_endpoints() {
        let edge = Edge {
            src: "a".into(),
            tgt: "b".into(),
            kind: "prop".into(),
            name: Some("x".into()),
        };
        let old = make_schema(
            &[("a", "object"), ("b", "string")],
            std::slice::from_ref(&edge),
        );
        let new = make_schema(
            &[("a", "object"), ("b", "string")],
            std::slice::from_ref(&edge),
        );
        let d = diff(&old, &new);
        let m = derive_migration(&old, &new, &d);
        assert_eq!(m.edge_map.len(), 1);
    }

    #[test]
    fn derive_drops_edges_with_removed_endpoints() {
        let edge = Edge {
            src: "a".into(),
            tgt: "b".into(),
            kind: "prop".into(),
            name: None,
        };
        let old = make_schema(&[("a", "object"), ("b", "string")], &[edge]);
        let new = make_schema(&[("a", "object")], &[]);
        let d = diff(&old, &new);
        let m = derive_migration(&old, &new, &d);
        assert!(m.edge_map.is_empty());
    }

    #[test]
    fn derive_handles_addition() {
        let old = make_schema(&[("a", "object")], &[]);
        let new = make_schema(&[("a", "object"), ("b", "string")], &[]);
        let d = diff(&old, &new);
        let m = derive_migration(&old, &new, &d);
        // Only 'a' exists in old, so only 'a' is mapped.
        assert_eq!(m.vertex_map.len(), 1);
        assert!(m.vertex_map.contains_key("a"));
    }
}
