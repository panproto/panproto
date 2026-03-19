//! Ref-normalization for schemas.
//!
//! When a schema theory includes a `Ref` sort, edges may pass through
//! chains of ref vertices (`A -> Ref -> Ref -> B`). [`normalize`] collapses
//! such chains into direct edges (`A -> B`), removing the intermediate
//! ref vertices. The operation is idempotent.

use std::collections::HashMap;

use panproto_gat::Name;
use rustc_hash::{FxHashMap, FxHashSet};
use smallvec::SmallVec;

use crate::schema::{Edge, Schema, Vertex};

/// The vertex kind that represents a reference indirection.
const REF_KIND: &str = "ref";

/// Collapse ref-chains in a schema into direct edges.
///
/// For every path `A -> R1 -> R2 -> ... -> Rn -> B` where all
/// intermediate vertices have kind `"ref"`, this function produces a
/// single edge `A -> B`, inheriting the kind and name from the first
/// edge in the chain.
///
/// Ref vertices that become unreachable (no longer sources or targets
/// of any non-ref edge) are removed.
///
/// This operation is **idempotent**: `normalize(normalize(s)) == normalize(s)`.
#[must_use]
pub fn normalize(schema: &Schema) -> Schema {
    let ref_targets = build_ref_targets(schema);

    // If there are no ref vertices, return a clone unchanged.
    if ref_targets.is_empty() {
        return schema.clone();
    }

    let (new_edges, used_refs) = collapse_edges(schema, &ref_targets);
    rebuild_schema(schema, &new_edges, &used_refs)
}

/// Build a map from ref vertex IDs to their outgoing edge targets.
fn build_ref_targets(schema: &Schema) -> FxHashMap<Name, (Name, Edge)> {
    let mut ref_targets: FxHashMap<Name, (Name, Edge)> = FxHashMap::default();
    for (id, vertex) in &schema.vertices {
        if vertex.kind == REF_KIND {
            if let Some(edges) = schema.outgoing.get(id) {
                if let Some(edge) = edges.first() {
                    ref_targets.insert(id.clone(), (edge.tgt.clone(), edge.clone()));
                }
            }
        }
    }
    ref_targets
}

/// Follow a ref chain to find the ultimate non-ref target.
///
/// Returns `None` if the chain contains a cycle.
fn resolve_ref_chain(start: &Name, ref_targets: &FxHashMap<Name, (Name, Edge)>) -> Option<Name> {
    let mut current = start.clone();
    let mut visited = FxHashSet::default();
    loop {
        if !visited.insert(current.clone()) {
            return None;
        }
        if let Some((target, _)) = ref_targets.get(&current) {
            current.clone_from(target);
        } else {
            return Some(current);
        }
    }
}

/// Collapse edges through ref chains, returning the new edge list
/// and the set of ref vertex IDs that were consumed.
fn collapse_edges(
    schema: &Schema,
    ref_targets: &FxHashMap<Name, (Name, Edge)>,
) -> (Vec<Edge>, FxHashSet<Name>) {
    let mut new_edges: Vec<Edge> = Vec::new();
    let mut used_refs: FxHashSet<Name> = FxHashSet::default();

    for edge in schema.edges.keys() {
        let src_vertex = schema.vertices.get(&edge.src);
        // Skip edges that originate FROM a ref vertex (they are part of
        // the chain and will be collapsed into the originating edge).
        if src_vertex.is_some_and(|v| v.kind == REF_KIND) {
            continue;
        }

        // If the target is a ref vertex, resolve the chain.
        if let Some(resolved_tgt) = ref_targets
            .get(&edge.tgt)
            .and_then(|_| resolve_ref_chain(&edge.tgt, ref_targets))
        {
            // Track which ref vertices are traversed.
            let mut cursor = edge.tgt.clone();
            while let Some((next, _)) = ref_targets.get(&cursor) {
                used_refs.insert(cursor.clone());
                cursor.clone_from(next);
            }

            new_edges.push(Edge {
                src: edge.src.clone(),
                tgt: resolved_tgt,
                kind: edge.kind.clone(),
                name: edge.name.clone(),
            });
        } else {
            // Not targeting a ref — keep as-is.
            new_edges.push(edge.clone());
        }
    }

    (new_edges, used_refs)
}

/// Rebuild the schema from the collapsed edges, removing consumed ref vertices.
fn rebuild_schema(schema: &Schema, new_edges: &[Edge], used_refs: &FxHashSet<Name>) -> Schema {
    let new_vertices: HashMap<Name, Vertex> = schema
        .vertices
        .iter()
        .filter(|(id, v)| v.kind != REF_KIND || !used_refs.contains(*id))
        .map(|(id, v)| (id.clone(), v.clone()))
        .collect();

    let mut edge_map = HashMap::with_capacity(new_edges.len());
    let mut outgoing: HashMap<Name, SmallVec<Edge, 4>> = HashMap::new();
    let mut incoming: HashMap<Name, SmallVec<Edge, 4>> = HashMap::new();
    let mut between: HashMap<(Name, Name), SmallVec<Edge, 2>> = HashMap::new();

    for edge in new_edges {
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

    let new_constraints = schema
        .constraints
        .iter()
        .filter(|(id, _)| new_vertices.contains_key(*id))
        .map(|(id, c)| (id.clone(), c.clone()))
        .collect();

    let new_required = schema
        .required
        .iter()
        .filter(|(id, _)| new_vertices.contains_key(*id))
        .map(|(id, r)| (id.clone(), r.clone()))
        .collect();

    let new_nsids = schema
        .nsids
        .iter()
        .filter(|(id, _)| new_vertices.contains_key(*id))
        .map(|(id, n)| (id.clone(), n.clone()))
        .collect();

    let new_hyper_edges = schema
        .hyper_edges
        .iter()
        .filter(|(_, he)| he.signature.values().all(|v| new_vertices.contains_key(v)))
        .map(|(id, he)| (id.clone(), he.clone()))
        .collect();

    let new_mergers = schema
        .mergers
        .iter()
        .filter(|(id, _)| new_vertices.contains_key(*id))
        .map(|(id, e)| (id.clone(), e.clone()))
        .collect();

    let new_defaults = schema
        .defaults
        .iter()
        .filter(|(id, _)| new_vertices.contains_key(*id))
        .map(|(id, e)| (id.clone(), e.clone()))
        .collect();

    Schema {
        protocol: schema.protocol.clone(),
        vertices: new_vertices,
        edges: edge_map,
        hyper_edges: new_hyper_edges,
        constraints: new_constraints,
        required: new_required,
        nsids: new_nsids,
        variants: schema.variants.clone(),
        orderings: schema.orderings.clone(),
        recursion_points: schema.recursion_points.clone(),
        spans: schema.spans.clone(),
        usage_modes: schema.usage_modes.clone(),
        nominal: schema.nominal.clone(),
        coercions: schema.coercions.clone(),
        mergers: new_mergers,
        defaults: new_defaults,
        policies: schema.policies.clone(),
        outgoing,
        incoming,
        between,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::builder::SchemaBuilder;
    use crate::protocol::{EdgeRule, Protocol};

    /// Build a protocol that allows ref vertices and prop edges between anything.
    fn ref_protocol() -> Protocol {
        Protocol {
            name: "test-ref".to_owned(),
            schema_theory: "ThTestSchema".to_owned(),
            instance_theory: "ThWType".to_owned(),
            edge_rules: vec![EdgeRule {
                edge_kind: "prop".to_owned(),
                src_kinds: vec![],
                tgt_kinds: vec![],
            }],
            obj_kinds: vec!["object".to_owned(), "string".to_owned(), "ref".to_owned()],
            constraint_sorts: vec![],
            ..Protocol::default()
        }
    }

    #[test]
    fn collapse_single_ref() {
        let proto = ref_protocol();
        let schema = SchemaBuilder::new(&proto)
            .vertex("A", "object", None)
            .expect("A")
            .vertex("R", "ref", None)
            .expect("R")
            .vertex("B", "string", None)
            .expect("B")
            .edge("A", "R", "prop", Some("x"))
            .expect("A->R")
            .edge("R", "B", "prop", None)
            .expect("R->B")
            .build()
            .expect("build");

        let normalized = normalize(&schema);

        // The ref vertex R should be removed.
        assert!(
            !normalized.has_vertex("R"),
            "ref vertex R should be removed"
        );
        assert_eq!(normalized.vertex_count(), 2, "should have A and B");

        // There should be a direct edge A -> B.
        let ab = normalized.edges_between("A", "B");
        assert_eq!(ab.len(), 1, "should have one edge A -> B");
        assert_eq!(ab[0].kind, "prop");
        assert_eq!(ab[0].name.as_deref(), Some("x"));
    }

    #[test]
    fn collapse_double_ref_chain() {
        let proto = ref_protocol();
        let schema = SchemaBuilder::new(&proto)
            .vertex("A", "object", None)
            .expect("A")
            .vertex("R1", "ref", None)
            .expect("R1")
            .vertex("R2", "ref", None)
            .expect("R2")
            .vertex("B", "string", None)
            .expect("B")
            .edge("A", "R1", "prop", Some("link"))
            .expect("A->R1")
            .edge("R1", "R2", "prop", None)
            .expect("R1->R2")
            .edge("R2", "B", "prop", None)
            .expect("R2->B")
            .build()
            .expect("build");

        let normalized = normalize(&schema);

        // Both ref vertices should be removed.
        assert!(!normalized.has_vertex("R1"));
        assert!(!normalized.has_vertex("R2"));
        assert_eq!(normalized.vertex_count(), 2);

        // Direct edge A -> B.
        let ab = normalized.edges_between("A", "B");
        assert_eq!(ab.len(), 1);
        assert_eq!(ab[0].name.as_deref(), Some("link"));
    }

    #[test]
    fn normalize_is_idempotent() {
        let proto = ref_protocol();
        let schema = SchemaBuilder::new(&proto)
            .vertex("A", "object", None)
            .expect("A")
            .vertex("R1", "ref", None)
            .expect("R1")
            .vertex("R2", "ref", None)
            .expect("R2")
            .vertex("B", "string", None)
            .expect("B")
            .edge("A", "R1", "prop", Some("link"))
            .expect("A->R1")
            .edge("R1", "R2", "prop", None)
            .expect("R1->R2")
            .edge("R2", "B", "prop", None)
            .expect("R2->B")
            .build()
            .expect("build");

        let once = normalize(&schema);
        let twice = normalize(&once);

        assert_eq!(once.vertex_count(), twice.vertex_count());
        assert_eq!(once.edge_count(), twice.edge_count());

        // Verify same vertices.
        for id in once.vertices.keys() {
            assert!(
                twice.vertices.contains_key(id),
                "vertex {id} missing after second normalize"
            );
        }

        // Verify same edges.
        for edge in once.edges.keys() {
            assert!(
                twice.edges.contains_key(edge),
                "edge {edge:?} missing after second normalize"
            );
        }
    }

    #[test]
    fn no_refs_is_noop() {
        let proto = ref_protocol();
        let schema = SchemaBuilder::new(&proto)
            .vertex("A", "object", None)
            .expect("A")
            .vertex("B", "string", None)
            .expect("B")
            .edge("A", "B", "prop", Some("name"))
            .expect("edge")
            .build()
            .expect("build");

        let normalized = normalize(&schema);
        assert_eq!(normalized.vertex_count(), schema.vertex_count());
        assert_eq!(normalized.edge_count(), schema.edge_count());
    }
}
