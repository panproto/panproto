//! Lift operations: applying compiled migrations to instances.
//!
//! `lift_wtype` and `lift_functor` delegate to the restrict
//! implementations in `panproto-inst`, passing the compiled
//! migration's precomputed tables.

use panproto_inst::{CompiledMigration, FInstance, WInstance};
use panproto_schema::Schema;

use crate::error::LiftError;

/// Apply a compiled migration to a W-type instance.
///
/// Delegates to [`panproto_inst::wtype_restrict`], which executes the
/// 5-step pipeline: anchor surviving, reachability BFS, ancestor
/// contraction, edge resolution, and fan reconstruction.
///
/// # Errors
///
/// Returns `LiftError::Restrict` if the underlying restrict operation
/// fails (e.g., edge resolution ambiguity, root pruned).
pub fn lift_wtype(
    compiled: &CompiledMigration,
    src_schema: &Schema,
    tgt_schema: &Schema,
    instance: &WInstance,
) -> Result<WInstance, LiftError> {
    let result = panproto_inst::wtype_restrict(instance, src_schema, tgt_schema, compiled)?;
    Ok(result)
}

/// Apply a compiled migration to a set-valued functor instance.
///
/// Delegates to [`panproto_inst::functor_restrict`], which performs
/// precomposition (`Delta_F`): for each table in the target, pull the
/// corresponding table from the source via the vertex remap.
///
/// # Errors
///
/// Returns `LiftError::Restrict` if the underlying restrict operation fails.
pub fn lift_functor(
    compiled: &CompiledMigration,
    instance: &FInstance,
) -> Result<FInstance, LiftError> {
    let result = panproto_inst::functor_restrict(instance, compiled)?;
    Ok(result)
}

/// Apply a compiled migration as a left Kan extension (`Sigma_F`) to a W-type instance.
///
/// Delegates to [`panproto_inst::wtype_extend`], which pushes nodes forward
/// along the migration morphism, remapping anchors and edges.
///
/// # Errors
///
/// Returns `LiftError::Restrict` if the underlying extend operation fails.
pub fn lift_wtype_sigma(
    compiled: &CompiledMigration,
    tgt_schema: &Schema,
    instance: &WInstance,
) -> Result<WInstance, LiftError> {
    let result = panproto_inst::wtype_extend(instance, tgt_schema, compiled)?;
    Ok(result)
}

/// Apply a compiled migration as a right Kan extension (`Pi_F`) to a W-type instance.
///
/// Delegates to [`panproto_inst::wtype_pi`], which computes the product
/// over fibers. Multi-element fibers produce subtree Cartesian products
/// bounded by `max_product_nodes`.
///
/// # Errors
///
/// Returns `LiftError::Restrict` if the underlying pi operation fails
/// (e.g., product size exceeded for multi-element fibers).
pub fn lift_wtype_pi(
    compiled: &CompiledMigration,
    tgt_schema: &Schema,
    instance: &WInstance,
    max_product_nodes: usize,
) -> Result<WInstance, LiftError> {
    let result = panproto_inst::wtype_pi(instance, tgt_schema, compiled, max_product_nodes)?;
    Ok(result)
}

/// Apply a compiled migration as a right Kan extension (`Pi_F`) to a functor instance.
///
/// Delegates to [`panproto_inst::functor_pi`], which computes Cartesian
/// products over fibers.
///
/// # Errors
///
/// Returns `LiftError::Restrict` if the underlying pi operation fails
/// (e.g., product size exceeded).
pub fn lift_functor_pi(
    compiled: &CompiledMigration,
    instance: &FInstance,
    max_product_size: usize,
) -> Result<FInstance, LiftError> {
    let result = panproto_inst::functor_pi(instance, compiled, max_product_size)?;
    Ok(result)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::collections::{HashMap, HashSet};

    use super::*;
    use panproto_inst::value::FieldPresence;
    use panproto_inst::{Node, Value};
    use panproto_schema::{Edge, Vertex};

    fn test_schema(vertices: &[(&str, &str)], edges: &[Edge]) -> Schema {
        let mut vert_map = HashMap::new();
        let mut edge_map = HashMap::new();
        let mut outgoing: HashMap<panproto_gat::Name, smallvec::SmallVec<Edge, 4>> = HashMap::new();
        let mut incoming: HashMap<panproto_gat::Name, smallvec::SmallVec<Edge, 4>> = HashMap::new();
        let mut between: HashMap<
            (panproto_gat::Name, panproto_gat::Name),
            smallvec::SmallVec<Edge, 2>,
        > = HashMap::new();

        for (id, kind) in vertices {
            vert_map.insert(
                panproto_gat::Name::from(*id),
                Vertex {
                    id: panproto_gat::Name::from(*id),
                    kind: panproto_gat::Name::from(*kind),
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
    fn identity_migration_preserves_all_nodes() {
        // Test 1: identity migration preserves all nodes.
        let edge_text = Edge {
            src: "body".into(),
            tgt: "body.text".into(),
            kind: "prop".into(),
            name: Some("text".into()),
        };
        let edge_time = Edge {
            src: "body".into(),
            tgt: "body.createdAt".into(),
            kind: "prop".into(),
            name: Some("createdAt".into()),
        };

        let schema = test_schema(
            &[
                ("body", "object"),
                ("body.text", "string"),
                ("body.createdAt", "string"),
            ],
            &[edge_text.clone(), edge_time.clone()],
        );

        let mut nodes = HashMap::new();
        nodes.insert(0, Node::new(0, "body"));
        nodes.insert(
            1,
            Node::new(1, "body.text")
                .with_value(FieldPresence::Present(Value::Str("hello".into()))),
        );
        nodes.insert(
            2,
            Node::new(2, "body.createdAt")
                .with_value(FieldPresence::Present(Value::Str("2024-01-01".into()))),
        );

        let arcs = vec![(0, 1, edge_text.clone()), (0, 2, edge_time.clone())];
        let instance = WInstance::new(nodes, arcs, vec![], 0, panproto_gat::Name::from("body"));

        // Identity compiled migration
        let compiled = CompiledMigration {
            surviving_verts: HashSet::from([
                "body".into(),
                "body.text".into(),
                "body.createdAt".into(),
            ]),
            surviving_edges: HashSet::from([edge_text, edge_time]),
            vertex_remap: HashMap::new(),
            edge_remap: HashMap::new(),
            resolver: HashMap::new(),
            hyper_resolver: HashMap::new(),
        };

        let result = lift_wtype(&compiled, &schema, &schema, &instance);
        assert!(result.is_ok(), "identity lift should succeed");
        let lifted = result.unwrap_or_else(|_| panic!("lift should succeed"));
        assert_eq!(
            lifted.node_count(),
            instance.node_count(),
            "identity migration should preserve all nodes"
        );
        assert_eq!(
            lifted.arc_count(),
            instance.arc_count(),
            "identity migration should preserve all arcs"
        );
    }

    #[test]
    fn projection_drops_vertices() {
        // Test 2: projection - drop vertices, verify surviving set.
        let edge_text = Edge {
            src: "body".into(),
            tgt: "body.text".into(),
            kind: "prop".into(),
            name: Some("text".into()),
        };
        let edge_time = Edge {
            src: "body".into(),
            tgt: "body.createdAt".into(),
            kind: "prop".into(),
            name: Some("createdAt".into()),
        };

        let schema = test_schema(
            &[("body", "object"), ("body.text", "string")],
            std::slice::from_ref(&edge_text),
        );

        let mut nodes = HashMap::new();
        nodes.insert(0, Node::new(0, "body"));
        nodes.insert(
            1,
            Node::new(1, "body.text")
                .with_value(FieldPresence::Present(Value::Str("hello".into()))),
        );
        nodes.insert(
            2,
            Node::new(2, "body.createdAt")
                .with_value(FieldPresence::Present(Value::Str("2024-01-01".into()))),
        );

        let arcs = vec![(0, 1, edge_text.clone()), (0, 2, edge_time)];
        let instance = WInstance::new(nodes, arcs, vec![], 0, panproto_gat::Name::from("body"));

        // Migration that drops body.createdAt
        let compiled = CompiledMigration {
            surviving_verts: HashSet::from(["body".into(), "body.text".into()]),
            surviving_edges: HashSet::from([edge_text]),
            vertex_remap: HashMap::new(),
            edge_remap: HashMap::new(),
            resolver: HashMap::new(),
            hyper_resolver: HashMap::new(),
        };

        let result = lift_wtype(&compiled, &schema, &schema, &instance);
        assert!(result.is_ok(), "projection lift should succeed");
        let lifted = result.unwrap_or_else(|_| panic!("lift should succeed"));
        assert_eq!(lifted.node_count(), 2, "should have 2 surviving nodes");
        assert!(lifted.nodes.contains_key(&0), "root should survive");
        assert!(lifted.nodes.contains_key(&1), "text node should survive");
        assert!(
            !lifted.nodes.contains_key(&2),
            "createdAt node should be dropped"
        );
    }

    #[test]
    fn recursive_projection_via_wtype_restrict() {
        // Test 3: Recursive schema with nested children. Drop intermediate
        // vertices and verify the reachability filter prunes unreachable
        // subtrees via wtype_restrict.
        //
        // Schema: root -> container -> leaf1
        //                           -> leaf2
        //         root -> leaf3
        //
        // Migration drops "container", so leaf1 and leaf2 become
        // unreachable (no surviving ancestor path from root).
        let edge_root_container = Edge {
            src: "root".into(),
            tgt: "container".into(),
            kind: "prop".into(),
            name: Some("items".into()),
        };
        let edge_container_leaf1 = Edge {
            src: "container".into(),
            tgt: "leaf1".into(),
            kind: "prop".into(),
            name: Some("a".into()),
        };
        let edge_container_leaf2 = Edge {
            src: "container".into(),
            tgt: "leaf2".into(),
            kind: "prop".into(),
            name: Some("b".into()),
        };
        let edge_root_leaf3 = Edge {
            src: "root".into(),
            tgt: "leaf3".into(),
            kind: "prop".into(),
            name: Some("direct".into()),
        };

        let schema = test_schema(
            &[("root", "object"), ("leaf3", "string")],
            std::slice::from_ref(&edge_root_leaf3),
        );

        let mut nodes = HashMap::new();
        nodes.insert(0, Node::new(0, "root"));
        nodes.insert(1, Node::new(1, "container"));
        nodes.insert(
            2,
            Node::new(2, "leaf1").with_value(FieldPresence::Present(Value::Str("val1".into()))),
        );
        nodes.insert(
            3,
            Node::new(3, "leaf2").with_value(FieldPresence::Present(Value::Str("val2".into()))),
        );
        nodes.insert(
            4,
            Node::new(4, "leaf3").with_value(FieldPresence::Present(Value::Str("val3".into()))),
        );

        let arcs = vec![
            (0, 1, edge_root_container),
            (1, 2, edge_container_leaf1),
            (1, 3, edge_container_leaf2),
            (0, 4, edge_root_leaf3.clone()),
        ];
        let instance = WInstance::new(nodes, arcs, vec![], 0, panproto_gat::Name::from("root"));

        // Migration: drop "container", keep root and leaf3 only.
        let compiled = CompiledMigration {
            surviving_verts: HashSet::from(["root".into(), "leaf3".into()]),
            surviving_edges: HashSet::from([edge_root_leaf3]),
            vertex_remap: HashMap::new(),
            edge_remap: HashMap::new(),
            resolver: HashMap::new(),
            hyper_resolver: HashMap::new(),
        };

        let result = lift_wtype(&compiled, &schema, &schema, &instance);
        assert!(result.is_ok(), "recursive projection should succeed");
        let lifted = result.unwrap_or_else(|_| panic!("lift should succeed"));

        // Only root (0) and leaf3 (4) should survive; container (1),
        // leaf1 (2), and leaf2 (3) are all pruned.
        assert_eq!(
            lifted.node_count(),
            2,
            "should have 2 surviving nodes (root + leaf3)"
        );
        assert!(lifted.nodes.contains_key(&0), "root should survive");
        assert!(lifted.nodes.contains_key(&4), "leaf3 should survive");
        assert!(!lifted.nodes.contains_key(&1), "container should be pruned");
        assert!(
            !lifted.nodes.contains_key(&2),
            "leaf1 should be pruned (unreachable)"
        );
        assert!(
            !lifted.nodes.contains_key(&3),
            "leaf2 should be pruned (unreachable)"
        );
    }
}
