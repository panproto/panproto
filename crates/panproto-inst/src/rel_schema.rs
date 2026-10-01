//! The relation-ified schema of a table schema.
//!
//! A table instance ([`FInstance`](crate::FInstance)) reads every foreign key
//! as a relation: a set of pairs that may be partial and many-valued. So it
//! is a presheaf, not on the schema's vertices and edges, but on the category
//! [`RelSchema`] built from them: one object per table vertex, and one object
//! per foreign-key edge `e: a -> b` with two arrows `src_e: e -> a` and
//! `tgt_e: e -> b`. The instance sends a table to its rows and a foreign key
//! to its pairs, with their two projections. This is the typed-graph
//! presentation of brown2022computational Section 2 (stated: typed-graph
//! categories are the C-set categories whose instances are sets and
//! relations). A foreign key the schema declares functional and total is the
//! special case where `src_e` is a bijection.
//!
//! Attribute (column) vertices are not objects: their values are the cells of
//! the rows. Which vertices are tables is read from the schema alone, never
//! from whether an instance holds rows there, so a table with no rows is
//! still a table.

use std::collections::BTreeSet;

use panproto_gat::Name;
use panproto_schema::{Edge, Schema};

/// The vertex kind that marks a vertex as a table.
pub const TABLE_KIND: &str = "table";

/// The edge kind that makes its target a column of its source.
pub const COLUMN_EDGE_KIND: &str = "prop";

/// The relation-ified schema of a [`Schema`]: its tables and its foreign keys
/// as relation objects.
///
/// A vertex is a table when the schema says so: its kind is
/// [`TABLE_KIND`], it declares a key, or a declared reference holds at it or
/// resolves to it. Otherwise a vertex is an attribute exactly when an edge
/// of kind [`COLUMN_EDGE_KIND`] reaches it (it is a column of that edge's
/// source) or any edge leaves an attribute for it (it is part of a column's
/// value, as the element vertex of an array column is), and a table when
/// neither holds. A foreign key is an edge of the
/// schema between two tables; an edge into an attribute is a column, not an
/// object. Each foreign key `e` stands for an object with the two arrows
/// `src_e` and `tgt_e` to its endpoint tables, which the edge itself names.
///
/// ```
/// use std::collections::HashMap;
///
/// use panproto_inst::RelSchema;
/// use panproto_schema::{Edge, Schema, Vertex};
///
/// let vertex = |id: &str, kind: &str| {
///     (id.into(), Vertex { id: id.into(), kind: kind.into(), nsid: None })
/// };
/// let author = Edge { src: "post".into(), tgt: "user".into(), kind: "fk".into(), name: None };
/// let title = Edge { src: "post".into(), tgt: "post.title".into(), kind: "prop".into(), name: Some("title".into()) };
/// let schema = Schema {
///     vertices: HashMap::from([
///         vertex("post", "table"),
///         vertex("user", "table"),
///         vertex("post.title", "string"),
///     ]),
///     edges: HashMap::from([(author.clone(), "fk".into()), (title, "prop".into())]),
///     ..Schema::default()
/// };
/// let rel = RelSchema::new(&schema);
/// assert!(rel.is_table("post") && rel.is_table("user"));
/// assert!(!rel.is_table("post.title"));
/// assert!(rel.relations().iter().eq([&author]));
/// ```
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RelSchema {
    /// The table objects, by vertex name.
    tables: BTreeSet<Name>,
    /// The relation objects, one per foreign-key edge.
    relations: BTreeSet<Edge>,
}

impl RelSchema {
    /// The relation-ified schema of `schema`, read as the type documents.
    #[must_use]
    pub fn new(schema: &Schema) -> Self {
        let declared: BTreeSet<&str> = schema
            .declarations
            .keys
            .iter()
            .filter(|(_, keys)| !keys.is_empty())
            .map(|(vertex, _)| vertex.as_ref())
            .chain(
                schema
                    .declarations
                    .references
                    .values()
                    .flat_map(|r| [r.holder.as_ref(), r.target_vertex.as_ref()]),
            )
            .collect();
        let declared_table = |id: &str| {
            schema
                .vertices
                .get(id)
                .is_some_and(|vertex| vertex.kind == TABLE_KIND)
                || declared.contains(id)
        };
        // The attributes: the targets of column edges, closed under every
        // edge out of an attribute (the element vertex of an array column,
        // the fields of a struct column), never a declared table.
        let mut attributes: BTreeSet<&str> = BTreeSet::new();
        let mut frontier: Vec<&str> = schema
            .edges
            .keys()
            .filter(|edge| edge.kind == COLUMN_EDGE_KIND)
            .map(|edge| edge.tgt.as_ref())
            .collect();
        while let Some(vertex) = frontier.pop() {
            if declared_table(vertex) || !attributes.insert(vertex) {
                continue;
            }
            frontier.extend(
                schema
                    .edges
                    .keys()
                    .filter(|edge| edge.src.as_ref() == vertex)
                    .map(|edge| edge.tgt.as_ref()),
            );
        }
        let tables: BTreeSet<Name> = schema
            .vertices
            .values()
            .filter(|vertex| !attributes.contains(vertex.id.as_ref()))
            .map(|vertex| vertex.id.clone())
            .collect();
        let relations = schema
            .edges
            .keys()
            .filter(|edge| tables.contains(&edge.src) && tables.contains(&edge.tgt))
            .cloned()
            .collect();
        Self { tables, relations }
    }

    /// The relation-ified schema with exactly the given tables and relations.
    ///
    /// # Errors
    ///
    /// Returns the first relation (ascending) one of whose endpoints is not
    /// among the tables.
    pub fn from_parts(
        tables: impl IntoIterator<Item = Name>,
        relations: impl IntoIterator<Item = Edge>,
    ) -> Result<Self, Edge> {
        let tables: BTreeSet<Name> = tables.into_iter().collect();
        let relations: BTreeSet<Edge> = relations.into_iter().collect();
        if let Some(edge) = relations
            .iter()
            .find(|edge| !tables.contains(&edge.src) || !tables.contains(&edge.tgt))
        {
            return Err(edge.clone());
        }
        Ok(Self { tables, relations })
    }

    /// The table objects, ascending.
    #[must_use]
    pub const fn tables(&self) -> &BTreeSet<Name> {
        &self.tables
    }

    /// The relation objects (foreign keys), ascending.
    #[must_use]
    pub const fn relations(&self) -> &BTreeSet<Edge> {
        &self.relations
    }

    /// Whether `vertex` is a table object.
    #[must_use]
    pub fn is_table(&self, vertex: &str) -> bool {
        self.tables.contains(vertex)
    }

    /// Whether `edge` is a relation object.
    #[must_use]
    pub fn is_relation(&self, edge: &Edge) -> bool {
        self.relations.contains(edge)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use panproto_schema::{Key, KeyPart, KeyRole, KeyScope, Vertex};

    use super::*;
    use crate::FInstance;

    fn vertex(id: &str, kind: &str) -> (Name, Vertex) {
        (
            id.into(),
            Vertex {
                id: id.into(),
                kind: kind.into(),
                nsid: None,
            },
        )
    }

    fn edge(src: &str, tgt: &str, kind: &str, name: &str) -> Edge {
        Edge {
            src: src.into(),
            tgt: tgt.into(),
            kind: kind.into(),
            name: Some(name.into()),
        }
    }

    /// A SQL-shaped schema: tables `post` and `user` with columns, a foreign
    /// key between them, and `tag`, a vertex reached by a column edge that
    /// declares a key and so is a table.
    fn sql_like() -> Schema {
        let mut schema = Schema {
            vertices: HashMap::from([
                vertex("post", "table"),
                vertex("user", "table"),
                vertex("post.title", "string"),
                vertex("user.name", "string"),
                vertex("tag", "string"),
                vertex("audit", "object"),
            ]),
            edges: [
                edge("post", "post.title", "prop", "title"),
                edge("user", "user.name", "prop", "name"),
                edge("post", "user", "foreign-key", "author"),
                edge("post", "tag", "prop", "tag"),
                edge("post", "post.title", "fk", "odd"),
            ]
            .into_iter()
            .map(|e| {
                let kind = e.kind.clone();
                (e, kind)
            })
            .collect(),
            ..Schema::default()
        };
        schema.declarations.keys.insert(
            "tag".into(),
            vec![Key {
                id: "pk".into(),
                parts: vec![KeyPart::Path(Vec::new())],
                role: KeyRole::Primary,
                scope: KeyScope::Global,
            }],
        );
        schema
    }

    #[test]
    fn rel_schema_reads_table_kinds_from_the_schema() {
        let rel = RelSchema::new(&sql_like());
        let tables: Vec<&str> = rel.tables().iter().map(AsRef::as_ref).collect();
        assert_eq!(tables, ["audit", "post", "tag", "user"]);
        let relations: Vec<&Edge> = rel.relations().iter().collect();
        assert_eq!(
            relations,
            [
                &edge("post", "tag", "prop", "tag"),
                &edge("post", "user", "foreign-key", "author"),
            ],
            "edges between tables are relations; edges into columns are cells"
        );
    }

    #[test]
    fn an_empty_table_is_still_a_table() -> Result<(), crate::error::InstError> {
        let schema = sql_like();
        let empty = RelSchema::new(&schema);
        // No instance is consulted: a table with no rows and one with rows
        // are both tables, and a column is never one.
        let rows = FInstance::new()
            .with_table("post", [HashMap::new()])?;
        assert_eq!(rows.row_count("user"), 0);
        assert!(empty.is_table("user"));
        assert!(empty.is_table("post"));
        assert!(!empty.is_table("post.title"));
        assert_eq!(RelSchema::new(&schema), empty);
        Ok(())
    }

    /// The shape the SQL protocol gives an array column `post.tags TEXT[]`:
    /// the column vertex of kind `array` reached by a column edge, and its
    /// element vertex reached from it by an `items` edge. Both are parts of
    /// a cell, not tables, and so is a field below a struct column.
    #[test]
    fn a_vertex_below_a_column_is_an_attribute() {
        let mut schema = sql_like();
        schema.vertices.extend([
            vertex("post.tags", "array"),
            vertex("post.tags:items", "string"),
        ]);
        for e in [
            edge("post", "post.tags", "prop", "tags"),
            Edge {
                src: "post.tags".into(),
                tgt: "post.tags:items".into(),
                kind: "items".into(),
                name: None,
            },
            // An edge from a cell into a declared table leaves it a table.
            edge("post.tags:items", "user", "fk", "owner"),
        ] {
            let kind = e.kind.clone();
            schema.edges.insert(e, kind);
        }
        let rel = RelSchema::new(&schema);
        assert!(!rel.is_table("post.tags"));
        assert!(
            !rel.is_table("post.tags:items"),
            "an array column's element vertex is part of the cell: {:?}",
            rel.tables()
        );
        assert!(rel.is_table("user"));
        let tables: Vec<&str> = rel.tables().iter().map(AsRef::as_ref).collect();
        assert_eq!(tables, ["audit", "post", "tag", "user"]);
        assert!(
            rel.relations().iter().all(|e| e.src != "post.tags:items"),
            "an edge out of a cell is no foreign key"
        );
    }

    #[test]
    fn from_parts_refuses_a_relation_off_its_tables() {
        let e = edge("a", "b", "fk", "e");
        assert_eq!(
            RelSchema::from_parts([Name::from("a")], [e.clone()]),
            Err(e)
        );
    }
}
