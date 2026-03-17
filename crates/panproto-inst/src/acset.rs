//! Trait for attributed C-set operations shared across all instance shapes.
//!
//! [`AcsetOps`] provides a unified interface for restrict, extend, and
//! introspection operations on the three instance shapes:
//! [`WInstance`](crate::WInstance), [`FInstance`](crate::FInstance),
//! and [`GInstance`](crate::GInstance).

use panproto_schema::Schema;

use crate::error::RestrictError;
use crate::functor::FInstance;
use crate::ginstance::GInstance;
use crate::wtype::{CompiledMigration, WInstance};

/// Trait for attributed C-set operations shared across all instance shapes.
pub trait AcsetOps: Clone + std::fmt::Debug {
    /// Restrict this instance along a compiled migration.
    ///
    /// Corresponds to `Delta_F` (precomposition / pullback).
    ///
    /// # Errors
    ///
    /// Returns [`RestrictError`] if the restrict pipeline fails.
    fn restrict(
        &self,
        src_schema: &Schema,
        tgt_schema: &Schema,
        migration: &CompiledMigration,
    ) -> Result<Self, RestrictError>;

    /// Extend (left Kan extension, `Sigma_F`) this instance along a migration.
    ///
    /// # Errors
    ///
    /// Returns [`RestrictError`] if the extend pipeline fails.
    fn extend(
        &self,
        tgt_schema: &Schema,
        migration: &CompiledMigration,
    ) -> Result<Self, RestrictError>;

    /// Returns the number of top-level elements (nodes, rows, or vertices).
    fn element_count(&self) -> usize;

    /// Returns the shape name as a static string.
    fn shape_name(&self) -> &'static str;
}

impl AcsetOps for WInstance {
    fn restrict(
        &self,
        src_schema: &Schema,
        tgt_schema: &Schema,
        migration: &CompiledMigration,
    ) -> Result<Self, RestrictError> {
        crate::wtype::wtype_restrict(self, src_schema, tgt_schema, migration)
    }

    fn extend(
        &self,
        tgt_schema: &Schema,
        migration: &CompiledMigration,
    ) -> Result<Self, RestrictError> {
        crate::wtype::wtype_extend(self, tgt_schema, migration)
    }

    fn element_count(&self) -> usize {
        self.node_count()
    }

    fn shape_name(&self) -> &'static str {
        "wtype"
    }
}

impl AcsetOps for FInstance {
    fn restrict(
        &self,
        _src_schema: &Schema,
        _tgt_schema: &Schema,
        migration: &CompiledMigration,
    ) -> Result<Self, RestrictError> {
        crate::functor::functor_restrict(self, migration)
    }

    fn extend(
        &self,
        _tgt_schema: &Schema,
        migration: &CompiledMigration,
    ) -> Result<Self, RestrictError> {
        crate::functor::functor_extend(self, migration)
    }

    fn element_count(&self) -> usize {
        self.table_count()
    }

    fn shape_name(&self) -> &'static str {
        "functor"
    }
}

impl AcsetOps for GInstance {
    fn restrict(
        &self,
        _src_schema: &Schema,
        _tgt_schema: &Schema,
        migration: &CompiledMigration,
    ) -> Result<Self, RestrictError> {
        crate::ginstance::graph_restrict(self, migration)
    }

    fn extend(
        &self,
        _tgt_schema: &Schema,
        migration: &CompiledMigration,
    ) -> Result<Self, RestrictError> {
        crate::ginstance::graph_extend(self, migration)
    }

    fn element_count(&self) -> usize {
        self.node_count()
    }

    fn shape_name(&self) -> &'static str {
        "graph"
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};

    use panproto_gat::Name;
    use panproto_schema::Edge;

    use super::*;
    use crate::metadata::Node;
    use crate::value::{FieldPresence, Value};

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    fn make_empty_schema() -> Schema {
        Schema {
            protocol: "test".into(),
            vertices: HashMap::new(),
            edges: HashMap::new(),
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

    fn make_test_schema(vertices: &[&str], edges: &[Edge]) -> Schema {
        use smallvec::smallvec;
        let mut between = HashMap::new();
        for edge in edges {
            between
                .entry((Name::from(&*edge.src), Name::from(&*edge.tgt)))
                .or_insert_with(|| smallvec![])
                .push(edge.clone());
        }
        Schema {
            protocol: "test".into(),
            vertices: vertices
                .iter()
                .map(|&v| {
                    (
                        Name::from(v),
                        panproto_schema::Vertex {
                            id: Name::from(v),
                            kind: Name::from("object"),
                            nsid: None,
                        },
                    )
                })
                .collect(),
            edges: HashMap::new(),
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
            between,
        }
    }

    fn identity_migration(verts: &[&str], edges: &[Edge]) -> CompiledMigration {
        CompiledMigration {
            surviving_verts: verts.iter().map(|&v| Name::from(v)).collect(),
            surviving_edges: edges.iter().cloned().collect(),
            vertex_remap: HashMap::new(),
            edge_remap: HashMap::new(),
            resolver: HashMap::new(),
            hyper_resolver: HashMap::new(),
        }
    }

    fn three_node_winstance() -> WInstance {
        let mut nodes = HashMap::new();
        nodes.insert(0, Node::new(0, "post:body"));
        nodes.insert(
            1,
            Node::new(1, "post:body.text")
                .with_value(FieldPresence::Present(Value::Str("hello".into()))),
        );
        nodes.insert(
            2,
            Node::new(2, "post:body.createdAt")
                .with_value(FieldPresence::Present(Value::Str("2024-01-01".into()))),
        );
        let arcs = vec![
            (
                0,
                1,
                Edge {
                    src: "post:body".into(),
                    tgt: "post:body.text".into(),
                    kind: "prop".into(),
                    name: Some("text".into()),
                },
            ),
            (
                0,
                2,
                Edge {
                    src: "post:body".into(),
                    tgt: "post:body.createdAt".into(),
                    kind: "prop".into(),
                    name: Some("createdAt".into()),
                },
            ),
        ];
        WInstance::new(nodes, arcs, vec![], 0, Name::from("post:body"))
    }

    fn two_node_finstance() -> FInstance {
        let mut row = HashMap::new();
        row.insert("name".to_string(), Value::Str("Alice".into()));
        FInstance::new().with_table("users", vec![row])
    }

    fn two_node_ginstance() -> (GInstance, Edge) {
        let edge = Edge {
            src: "person".into(),
            tgt: "person".into(),
            kind: "knows".into(),
            name: None,
        };
        let g = GInstance::new()
            .with_node(Node::new(0, "person"))
            .with_node(Node::new(1, "person"))
            .with_edge(0, 1, edge.clone())
            .with_value(0, Value::Str("Alice".into()))
            .with_value(1, Value::Str("Bob".into()));
        (g, edge)
    }

    // -----------------------------------------------------------------------
    // shape_name tests
    // -----------------------------------------------------------------------

    #[test]
    fn winstance_shape_name() {
        let w = three_node_winstance();
        assert_eq!(AcsetOps::shape_name(&w), "wtype");
    }

    #[test]
    fn finstance_shape_name() {
        let f = two_node_finstance();
        assert_eq!(AcsetOps::shape_name(&f), "functor");
    }

    #[test]
    fn ginstance_shape_name() {
        let (g, _) = two_node_ginstance();
        assert_eq!(AcsetOps::shape_name(&g), "graph");
    }

    // -----------------------------------------------------------------------
    // element_count tests
    // -----------------------------------------------------------------------

    #[test]
    fn winstance_element_count() {
        let w = three_node_winstance();
        assert_eq!(AcsetOps::element_count(&w), 3);
    }

    #[test]
    fn finstance_element_count() {
        let f = two_node_finstance();
        assert_eq!(AcsetOps::element_count(&f), 1);
    }

    #[test]
    fn ginstance_element_count() {
        let (g, _) = two_node_ginstance();
        assert_eq!(AcsetOps::element_count(&g), 2);
    }

    // -----------------------------------------------------------------------
    // restrict through trait matches direct function call
    // -----------------------------------------------------------------------

    #[test]
    fn winstance_restrict_via_trait() -> Result<(), Box<dyn std::error::Error>> {
        let w = three_node_winstance();
        let edge_text = Edge {
            src: "post:body".into(),
            tgt: "post:body.text".into(),
            kind: "prop".into(),
            name: Some("text".into()),
        };
        let tgt_schema = make_test_schema(&["post:body", "post:body.text"], &[edge_text]);
        let src_schema = make_empty_schema();
        let migration = CompiledMigration {
            surviving_verts: HashSet::from([Name::from("post:body"), Name::from("post:body.text")]),
            surviving_edges: HashSet::new(),
            vertex_remap: HashMap::new(),
            edge_remap: HashMap::new(),
            resolver: HashMap::new(),
            hyper_resolver: HashMap::new(),
        };

        let via_trait = AcsetOps::restrict(&w, &src_schema, &tgt_schema, &migration)?;
        let via_fn = crate::wtype::wtype_restrict(&w, &src_schema, &tgt_schema, &migration)?;
        assert_eq!(via_trait.node_count(), via_fn.node_count());
        assert_eq!(via_trait.arc_count(), via_fn.arc_count());
        Ok(())
    }

    #[test]
    fn finstance_restrict_via_trait() -> Result<(), Box<dyn std::error::Error>> {
        let f = two_node_finstance();
        let schema = make_empty_schema();
        let migration = identity_migration(&["users"], &[]);

        let via_trait = AcsetOps::restrict(&f, &schema, &schema, &migration)?;
        let via_fn = crate::functor::functor_restrict(&f, &migration)?;
        assert_eq!(via_trait.table_count(), via_fn.table_count());
        Ok(())
    }

    #[test]
    fn ginstance_restrict_via_trait() -> Result<(), Box<dyn std::error::Error>> {
        let (g, _edge) = two_node_ginstance();
        let schema = make_empty_schema();
        let migration = CompiledMigration {
            surviving_verts: HashSet::from([Name::from("person_new")]),
            surviving_edges: HashSet::new(),
            vertex_remap: HashMap::from([("person".into(), "person_new".into())]),
            edge_remap: HashMap::new(),
            resolver: HashMap::new(),
            hyper_resolver: HashMap::new(),
        };

        let via_trait = AcsetOps::restrict(&g, &schema, &schema, &migration)?;
        let via_fn = crate::ginstance::graph_restrict(&g, &migration)?;
        assert_eq!(via_trait.node_count(), via_fn.node_count());
        assert_eq!(via_trait.edge_count(), via_fn.edge_count());
        Ok(())
    }

    // -----------------------------------------------------------------------
    // graph_extend tests
    // -----------------------------------------------------------------------

    #[test]
    fn graph_extend_identity_preserves_instance() -> Result<(), Box<dyn std::error::Error>> {
        let (g, edge) = two_node_ginstance();
        let schema = make_empty_schema();
        let migration = identity_migration(&["person"], &[edge]);

        let result = AcsetOps::extend(&g, &schema, &migration)?;
        assert_eq!(result.node_count(), 2);
        assert_eq!(result.edge_count(), 1);
        assert_eq!(result.values.len(), 2);
        assert_eq!(result.values[&0], Value::Str("Alice".into()));
        assert_eq!(result.values[&1], Value::Str("Bob".into()));
        Ok(())
    }

    #[test]
    fn graph_extend_vertex_remap_updates_anchors() -> Result<(), Box<dyn std::error::Error>> {
        let (g, edge) = two_node_ginstance();
        let schema = make_empty_schema();
        let new_edge = Edge {
            src: "human".into(),
            tgt: "human".into(),
            kind: "knows".into(),
            name: None,
        };
        let migration = CompiledMigration {
            surviving_verts: HashSet::from([Name::from("human")]),
            surviving_edges: HashSet::new(),
            vertex_remap: HashMap::from([("person".into(), "human".into())]),
            edge_remap: HashMap::from([(edge, new_edge.clone())]),
            resolver: HashMap::new(),
            hyper_resolver: HashMap::new(),
        };

        let result = AcsetOps::extend(&g, &schema, &migration)?;
        assert_eq!(result.node_count(), 2);
        assert_eq!(result.nodes[&0].anchor, Name::from("human"));
        assert_eq!(result.nodes[&1].anchor, Name::from("human"));
        assert_eq!(result.edge_count(), 1);
        assert_eq!(result.edges[0].2, new_edge);
        // Values preserved
        assert_eq!(result.values[&0], Value::Str("Alice".into()));
        Ok(())
    }
}
