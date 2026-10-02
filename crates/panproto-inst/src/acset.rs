//! Trait for attributed C-set operations shared across all instance shapes.
//!
//! [`AcsetOps`] provides a unified interface for restrict, extend, and
//! introspection operations on the three instance shapes:
//! [`WInstance`](crate::WInstance), [`FInstance`](crate::FInstance),
//! and [`GInstance`](crate::GInstance).

use panproto_schema::{Protocol, Schema};

use crate::error::RestrictError;
use crate::functor::FInstance;
use crate::ginstance::GInstance;
use crate::restrict_boundary::BoundaryProtocols;
use crate::wtype::{CompiledMigration, WInstance};

/// Trait for attributed C-set operations shared across all instance shapes.
pub trait AcsetOps: Clone + std::fmt::Debug {
    /// Carry this instance forward along a compiled migration, keeping the
    /// fragment that survives.
    ///
    /// Structure the migration drops does not travel. For the extension that
    /// keeps everything, see [`Self::extend`]. The input is validated against
    /// `src_schema` under `protocols.src` and the output against
    /// `tgt_schema` under `protocols.tgt`, as the shape's schema-taking
    /// restrict does.
    ///
    /// # Errors
    ///
    /// Returns [`RestrictError`] if the restrict pipeline fails, and
    /// [`RestrictError::Validation`] for a boundary that does not validate.
    fn restrict(
        &self,
        src_schema: &Schema,
        tgt_schema: &Schema,
        migration: &CompiledMigration,
        protocols: BoundaryProtocols<'_>,
    ) -> Result<Self, RestrictError>;

    /// Extend (left Kan extension, `Sigma_F`) this instance along a
    /// migration, validating the output against `tgt_schema` under
    /// `tgt_protocol`.
    ///
    /// # Errors
    ///
    /// Returns [`RestrictError`] if the extend pipeline fails, and
    /// [`RestrictError::Validation`] when the output does not validate.
    fn extend(
        &self,
        tgt_schema: &Schema,
        migration: &CompiledMigration,
        tgt_protocol: &Protocol,
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
        protocols: BoundaryProtocols<'_>,
    ) -> Result<Self, RestrictError> {
        crate::wtype::wtype_restrict(self, src_schema, tgt_schema, migration, protocols)
    }

    fn extend(
        &self,
        tgt_schema: &Schema,
        migration: &CompiledMigration,
        tgt_protocol: &Protocol,
    ) -> Result<Self, RestrictError> {
        crate::wtype::wtype_extend(self, tgt_schema, migration, tgt_protocol)
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
        src_schema: &Schema,
        tgt_schema: &Schema,
        migration: &CompiledMigration,
        protocols: BoundaryProtocols<'_>,
    ) -> Result<Self, RestrictError> {
        crate::table_restrict::functor_restrict(self, src_schema, tgt_schema, migration, protocols)
    }

    fn extend(
        &self,
        tgt_schema: &Schema,
        migration: &CompiledMigration,
        tgt_protocol: &Protocol,
    ) -> Result<Self, RestrictError> {
        crate::table_restrict::functor_extend(self, tgt_schema, migration, tgt_protocol)
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
        src_schema: &Schema,
        tgt_schema: &Schema,
        migration: &CompiledMigration,
        protocols: BoundaryProtocols<'_>,
    ) -> Result<Self, RestrictError> {
        crate::graph_restrict::graph_restrict(self, src_schema, tgt_schema, migration, protocols)
    }

    fn extend(
        &self,
        tgt_schema: &Schema,
        migration: &CompiledMigration,
        tgt_protocol: &Protocol,
    ) -> Result<Self, RestrictError> {
        crate::graph_restrict::graph_extend(self, tgt_schema, migration, tgt_protocol)
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
    use std::collections::{BTreeMap, BTreeSet, HashMap};

    use panproto_gat::Name;
    use panproto_schema::{Edge, ScalarKind, SchemaBuilder};

    use super::*;
    use crate::metadata::Node;
    use crate::value::{FieldPresence, Value};

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    /// A protocol that declares `record` a table kind and `string` a value
    /// kind.
    fn protocol() -> Protocol {
        Protocol {
            value_kinds: BTreeMap::from([("string".to_owned(), ScalarKind::Str)]),
            non_value_kinds: BTreeSet::from(["record".to_owned()]),
            ..Protocol::default()
        }
    }

    type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

    fn schema(vertices: &[(&str, &str)], edges: &[&Edge]) -> TestResult<Schema> {
        let protocol = Protocol::default();
        let mut builder = SchemaBuilder::new(&protocol);
        for (id, kind) in vertices {
            builder = builder.vertex(id, kind, None)?;
        }
        for edge in edges {
            builder = builder.edge(&edge.src, &edge.tgt, &edge.kind, edge.name.as_deref())?;
        }
        Ok(builder.build()?)
    }

    fn prop(src: &str, tgt: &str, name: &str) -> Edge {
        Edge {
            src: src.into(),
            tgt: tgt.into(),
            kind: "prop".into(),
            name: Some(name.into()),
        }
    }

    fn knows(vertex: &str) -> Edge {
        Edge {
            src: vertex.into(),
            tgt: vertex.into(),
            kind: "knows".into(),
            name: None,
        }
    }

    fn keep(verts: &[&str]) -> CompiledMigration {
        CompiledMigration {
            surviving_verts: verts.iter().map(|&v| Name::from(v)).collect(),
            ..CompiledMigration::default()
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
            (0, 1, prop("post:body", "post:body.text", "text")),
            (0, 2, prop("post:body", "post:body.createdAt", "createdAt")),
        ];
        WInstance::new(nodes, arcs, vec![], 0, Name::from("post:body"))
    }

    fn two_node_finstance() -> FInstance {
        let mut row = HashMap::new();
        row.insert("name".to_string(), Value::Str("Alice".into()));
        let mut f = FInstance::new();
        f.place_row(Name::from("users"), crate::RowId(0), row);
        f
    }

    fn alice() -> FieldPresence {
        FieldPresence::Present(Value::Str("Alice".into()))
    }

    fn bob() -> FieldPresence {
        FieldPresence::Present(Value::Str("Bob".into()))
    }

    fn two_node_ginstance() -> GInstance {
        GInstance::new()
            .with_node(Node::new(0, "person"))
            .with_node(Node::new(1, "person"))
            .with_edge(0, 1, knows("person"))
            .with_value(0, Value::Str("Alice".into()))
            .with_value(1, Value::Str("Bob".into()))
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
        let g = two_node_ginstance();
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
        let g = two_node_ginstance();
        assert_eq!(AcsetOps::element_count(&g), 2);
    }

    // -----------------------------------------------------------------------
    // restrict and extend through the trait match the direct functions
    // -----------------------------------------------------------------------

    #[test]
    fn winstance_restrict_via_trait() -> TestResult {
        let w = three_node_winstance();
        let text = prop("post:body", "post:body.text", "text");
        let created = prop("post:body", "post:body.createdAt", "createdAt");
        let src_schema = schema(
            &[
                ("post:body", "object"),
                ("post:body.text", "string"),
                ("post:body.createdAt", "string"),
            ],
            &[&text, &created],
        )?;
        let tgt_schema = schema(
            &[("post:body", "object"), ("post:body.text", "string")],
            &[&text],
        )?;
        let migration = keep(&["post:body", "post:body.text"]);
        let protocol = Protocol::default();
        let protocols = BoundaryProtocols {
            src: &protocol,
            tgt: &protocol,
        };

        let via_trait = AcsetOps::restrict(&w, &src_schema, &tgt_schema, &migration, protocols)?;
        let via_fn =
            crate::wtype::wtype_restrict(&w, &src_schema, &tgt_schema, &migration, protocols)?;
        assert_eq!(via_trait.node_count(), 2);
        assert_eq!(via_trait.node_count(), via_fn.node_count());
        assert_eq!(via_trait.arcs, via_fn.arcs);
        Ok(())
    }

    #[test]
    fn finstance_restrict_via_trait() -> TestResult {
        let f = two_node_finstance();
        let name = prop("users", "users.name", "name");
        let schema = schema(&[("users", "record"), ("users.name", "string")], &[&name])?;
        let mut migration = keep(&["users", "users.name"]);
        migration.surviving_edges.insert(name);
        let protocol = protocol();
        let protocols = BoundaryProtocols {
            src: &protocol,
            tgt: &protocol,
        };

        let via_trait = AcsetOps::restrict(&f, &schema, &schema, &migration, protocols)?;
        let via_fn =
            crate::table_restrict::functor_restrict(&f, &schema, &schema, &migration, protocols)?;
        assert_eq!(via_trait, via_fn);
        assert_eq!(via_trait, f, "the identity keeps the table");
        let extended = AcsetOps::extend(&f, &schema, &migration, &protocol)?;
        assert_eq!(
            extended,
            crate::table_restrict::functor_extend(&f, &schema, &migration, &protocol)?
        );
        Ok(())
    }

    #[test]
    fn ginstance_restrict_via_trait() -> TestResult {
        let g = two_node_ginstance();
        let src = schema(&[("person", "object")], &[&knows("person")])?;
        let tgt = schema(&[("person_new", "object")], &[&knows("person_new")])?;
        let mut migration = keep(&["person_new"]);
        migration
            .vertex_remap
            .insert("person".into(), "person_new".into());
        let protocol = Protocol::default();
        let protocols = BoundaryProtocols {
            src: &protocol,
            tgt: &protocol,
        };

        let via_trait = AcsetOps::restrict(&g, &src, &tgt, &migration, protocols)?;
        let via_fn = crate::graph_restrict::graph_restrict(&g, &src, &tgt, &migration, protocols)?;
        assert!(GInstance::identical(&via_trait, &via_fn));
        assert_eq!(via_trait.node_count(), 2);
        assert_eq!(
            via_trait.edge_count(),
            1,
            "the knows edge carries its own label"
        );
        Ok(())
    }

    // -----------------------------------------------------------------------
    // graph_extend tests
    // -----------------------------------------------------------------------

    #[test]
    fn graph_extend_identity_preserves_instance() -> TestResult {
        let g = two_node_ginstance();
        let schema = schema(&[("person", "object")], &[&knows("person")])?;
        let mut migration = keep(&["person"]);
        migration.surviving_edges.insert(knows("person"));

        let result = AcsetOps::extend(&g, &schema, &migration, &Protocol::default())?;
        assert_eq!(result.node_count(), 2);
        assert_eq!(result.edge_count(), 1);
        assert_eq!(result.node(0).and_then(|n| n.value.clone()), Some(alice()));
        assert_eq!(result.node(1).and_then(|n| n.value.clone()), Some(bob()));
        Ok(())
    }

    #[test]
    fn graph_extend_vertex_remap_updates_anchors() -> TestResult {
        let g = two_node_ginstance();
        let schema = schema(&[("human", "object")], &[&knows("human")])?;
        let mut migration = keep(&["human"]);
        migration
            .vertex_remap
            .insert("person".into(), "human".into());
        migration.edge_remap.insert(knows("person"), knows("human"));

        let result = AcsetOps::extend(&g, &schema, &migration, &Protocol::default())?;
        assert_eq!(result.node_count(), 2);
        assert_eq!(
            result.node(0).map(|n| &n.anchor),
            Some(&Name::from("human"))
        );
        assert_eq!(
            result.node(1).map(|n| &n.anchor),
            Some(&Name::from("human"))
        );
        assert_eq!(result.edge_count(), 1);
        assert_eq!(
            result.edge(crate::ginstance::EdgeId(0)).map(|e| &e.edge),
            Some(&knows("human"))
        );
        // Values preserved
        assert_eq!(result.node(0).and_then(|n| n.value.clone()), Some(alice()));
        Ok(())
    }
}
