//! Automatic overlap discovery between two schemas.
//!
//! Uses the homomorphism search from [`crate::hom_search`] to find
//! the largest shared sub-schema, then packages the result as a
//! [`SchemaOverlap`] suitable for [`panproto_schema::schema_pushout`].

use panproto_schema::{Edge, Schema, SchemaOverlap};

use crate::hom_search::{FoundMorphism, SearchOptions, find_best_morphism};

/// Automatically discover the largest shared sub-schema between two schemas.
///
/// Uses homomorphism search to find the best injective morphism in
/// both directions and returns whichever direction produces more
/// matched pairs.
#[must_use]
pub fn discover_overlap(left: &Schema, right: &Schema) -> SchemaOverlap {
    let opts = SearchOptions {
        monic: true,
        ..SearchOptions::default()
    };

    let forward = find_best_morphism(left, right, &opts);
    let reverse = find_best_morphism(right, left, &opts);

    let forward_size = forward
        .as_ref()
        .map_or(0, |m| m.vertex_map.len() + m.edge_map.len());
    let reverse_size = reverse
        .as_ref()
        .map_or(0, |m| m.vertex_map.len() + m.edge_map.len());

    if forward_size == 0 && reverse_size == 0 {
        return SchemaOverlap::default();
    }

    if forward_size >= reverse_size {
        overlap_from_morphism_forward(forward.as_ref())
    } else {
        // Reverse direction: morphism goes right→left, so we swap
        // the pairs to maintain (left, right) convention.
        overlap_from_morphism_reverse(reverse.as_ref())
    }
}

/// Build an overlap from a forward morphism `left → right`.
fn overlap_from_morphism_forward(morphism: Option<&FoundMorphism>) -> SchemaOverlap {
    let Some(m) = morphism else {
        return SchemaOverlap::default();
    };

    let vertex_pairs = m
        .vertex_map
        .iter()
        .map(|(left_id, right_id)| (left_id.clone(), right_id.clone()))
        .collect();

    let edge_pairs: Vec<(Edge, Edge)> = m
        .edge_map
        .iter()
        .map(|(left_e, right_e)| (left_e.clone(), right_e.clone()))
        .collect();

    SchemaOverlap {
        vertex_pairs,
        edge_pairs,
    }
}

/// Build an overlap from a reverse morphism `right → left`.
///
/// Swaps pairs so the convention is `(left_id, right_id)`.
fn overlap_from_morphism_reverse(morphism: Option<&FoundMorphism>) -> SchemaOverlap {
    let Some(m) = morphism else {
        return SchemaOverlap::default();
    };

    let vertex_pairs = m
        .vertex_map
        .iter()
        .map(|(right_id, left_id)| (left_id.clone(), right_id.clone()))
        .collect();

    let edge_pairs: Vec<(Edge, Edge)> = m
        .edge_map
        .iter()
        .map(|(right_e, left_e)| (left_e.clone(), right_e.clone()))
        .collect();

    SchemaOverlap {
        vertex_pairs,
        edge_pairs,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use panproto_schema::{Protocol, SchemaBuilder};

    fn test_protocol() -> Protocol {
        Protocol {
            name: "test".into(),
            schema_theory: "ThTest".into(),
            instance_theory: "ThWType".into(),
            edge_rules: vec![],
            obj_kinds: vec!["object".into(), "string".into(), "integer".into()],
            constraint_sorts: vec![],
            ..Protocol::default()
        }
    }

    fn build_schema(vertices: &[(&str, &str)], edges: &[(&str, &str, &str, &str)]) -> Schema {
        let proto = test_protocol();
        let mut builder = SchemaBuilder::new(&proto);
        for (id, kind) in vertices {
            builder = builder.vertex(id, kind, None::<&str>).unwrap();
        }
        for (src, tgt, kind, name) in edges {
            builder = builder.edge(src, tgt, kind, Some(*name)).unwrap();
        }
        builder.build().unwrap()
    }

    #[test]
    fn overlap_of_identical_schemas_has_all_vertices() {
        let s = build_schema(
            &[("root", "object"), ("root.x", "string")],
            &[("root", "root.x", "prop", "x")],
        );
        let overlap = discover_overlap(&s, &s);

        assert_eq!(
            overlap.vertex_pairs.len(),
            s.vertex_count(),
            "all vertices should be paired"
        );
        assert_eq!(
            overlap.edge_pairs.len(),
            s.edge_count(),
            "all edges should be paired"
        );
    }

    #[test]
    fn overlap_of_disjoint_schemas_is_empty() {
        let left = build_schema(
            &[("a", "object"), ("a.x", "string")],
            &[("a", "a.x", "prop", "x")],
        );
        // Right uses only `integer` vertices, incompatible kinds.
        let right = build_schema(
            &[("b", "integer"), ("c", "integer")],
            &[("b", "c", "prop", "y")],
        );
        let overlap = discover_overlap(&left, &right);

        assert!(
            overlap.vertex_pairs.is_empty(),
            "disjoint schemas should have no vertex overlap"
        );
        assert!(
            overlap.edge_pairs.is_empty(),
            "disjoint schemas should have no edge overlap"
        );
    }

    #[test]
    fn overlap_finds_shared_subgraph() {
        // Both schemas share an `object → string` sub-graph.
        let left = build_schema(
            &[
                ("root", "object"),
                ("root.x", "string"),
                ("root.extra", "integer"),
            ],
            &[
                ("root", "root.x", "prop", "x"),
                ("root", "root.extra", "prop", "extra"),
            ],
        );
        let right = build_schema(
            &[("root", "object"), ("root.x", "string")],
            &[("root", "root.x", "prop", "x")],
        );

        let overlap = discover_overlap(&left, &right);

        // Should find at least the 2-vertex subgraph that matches.
        assert!(
            overlap.vertex_pairs.len() >= 2,
            "should find at least the shared sub-graph vertices, got {}",
            overlap.vertex_pairs.len()
        );
        assert!(
            !overlap.edge_pairs.is_empty(),
            "should find at least one shared edge"
        );
    }
}
