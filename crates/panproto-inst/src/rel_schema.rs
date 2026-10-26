//! The attributed relation-ified schema of a table schema.
//!
//! A table instance ([`FInstance`](crate::FInstance)) reads every foreign key
//! as a relation: a set of pairs that may be partial and many-valued. So it
//! is a presheaf, not on the schema's vertices and edges, but on the entity
//! category of a [`RelSchema`]: one object per table, and one object per
//! relation `e: a -> b` with two arrows `src_e: e -> a` and `tgt_e: e -> b`,
//! with no path equations. The instance sends a table to its rows and a
//! relation to its pairs, with their two projections; this is the
//! typed-graph presentation of brown2022computational Section 2 (stated:
//! typed-graph categories are the C-set categories whose instances are sets
//! and relations). A foreign key the schema declares functional and total is
//! the special case where `src_e` is a bijection.
//!
//! The cells of the rows are the other half. Each attribute arrow `a: t -> c`
//! (an edge from a table to a value vertex) is a function from the rows of
//! `t` to one value sort, whose constants are the [`Value`](crate::Value)s
//! holding no labelled null and which has no operations and no equations: a
//! free, discrete value side, on which a labelled null is a free generator.
//! A row holds one cell per attribute arrow of its table, keyed by the
//! arrow's column name. Typing a cell by its vertex kind is the business of
//! validation, not of this schema, which is why one value sort is enough.
//!
//! Which vertices are tables is read from the protocol's two declarations,
//! [`Protocol::value_kinds`](panproto_schema::Protocol::value_kinds) and
//! [`Protocol::non_value_kinds`](panproto_schema::Protocol::non_value_kinds),
//! and from the schema's key and reference declarations, and from nothing
//! else: not from the kind of an edge, not from the name of a kind, not from
//! whether an instance holds rows there. So a table with no rows is still a
//! table.

use std::collections::{BTreeMap, BTreeSet};

use panproto_gat::Name;
use panproto_schema::{Edge, Protocol, Schema};

/// Why a schema has no attributed relation-ified form, or why given parts do
/// not form one.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum RelSchemaError {
    /// The protocol declares one vertex kind both a value kind and a
    /// non-value kind.
    #[error("vertex kind `{kind}` is declared both a value kind and a non-value kind")]
    AmbiguousKind {
        /// The least such kind.
        kind: String,
    },
    /// A vertex that no key or reference declares a table has a kind the
    /// protocol declares neither a value kind nor a non-value kind.
    #[error(
        "vertex `{vertex}` has kind `{kind}`, \
         which the protocol declares neither a value kind nor a non-value kind"
    )]
    UnclassifiedKind {
        /// The least such vertex.
        vertex: Name,
        /// Its kind.
        kind: Name,
    },
    /// An edge leads from a value vertex to a table: a declared table, or a
    /// vertex of a non-value kind. A cell cannot hold a foreign key or a
    /// record.
    #[error("edge {edge:?} leads from a value vertex to a table")]
    ValueReachesTable {
        /// The least such edge.
        edge: Edge,
    },
    /// Two attribute arrows of one table have one column name.
    #[error("table `{table}` has two attribute arrows with the column name `{column}`")]
    DuplicateColumn {
        /// The least such table.
        table: Name,
        /// The least such column of it.
        column: String,
    },
    /// A relation given to [`RelSchema::from_parts`] has an endpoint that is
    /// not among its tables.
    #[error("relation {edge:?} has an endpoint that is not a table")]
    RelationEndpointNotATable {
        /// The least such relation.
        edge: Edge,
    },
    /// A column given to [`RelSchema::from_parts`] is not an attribute arrow
    /// of its table under its own column name: its table is no table, its
    /// arrow starts elsewhere or ends at a table, or the column name is not
    /// the arrow's.
    #[error("column `{column}` of `{table}` is not an attribute arrow of that table so named")]
    ColumnEdgeMismatch {
        /// The least such table.
        table: Name,
        /// The least such column of it.
        column: String,
    },
}

/// The column name of an attribute arrow: its edge name, or the id of its
/// target when it has none.
#[must_use]
pub fn column_name(edge: &Edge) -> String {
    edge.name
        .as_ref()
        .map_or_else(|| edge.tgt.to_string(), ToString::to_string)
}

/// The columns of no table.
static NO_COLUMNS: BTreeMap<String, Edge> = BTreeMap::new();

/// The attributed relation-ified schema of a [`Schema`]: its tables, its
/// relations, and the columns of each table.
///
/// [`RelSchema::new`] classifies the vertices of a schema under its
/// protocol. The declared tables are the vertices with a key and the holders
/// and targets of references, whatever their kind. Every other vertex must
/// have a kind the protocol declares a value kind or a non-value kind. The
/// value vertices are the undeclared vertices of a value kind and every
/// vertex an edge reaches from one; the tables are the declared tables and
/// the vertices of a non-value kind. A relation is an edge between two
/// tables, of any edge kind. An attribute arrow is an edge from a table to a
/// value vertex, and the column it names is [`column_name`] of it. An edge
/// out of a value vertex (the element edge of a list column, the field
/// edges of a struct column) is an inner value edge: part of the structure
/// of the single value one cell holds, and no part of the entity category.
///
/// ```
/// use std::collections::{BTreeMap, BTreeSet, HashMap};
///
/// use panproto_inst::RelSchema;
/// use panproto_schema::{Edge, Protocol, ScalarKind, Schema, Vertex};
///
/// let vertex = |id: &str, kind: &str| {
///     (id.into(), Vertex { id: id.into(), kind: kind.into(), nsid: None })
/// };
/// let author = Edge { src: "post".into(), tgt: "user".into(), kind: "fk".into(), name: None };
/// let title = Edge {
///     src: "post".into(),
///     tgt: "post.title".into(),
///     kind: "prop".into(),
///     name: Some("title".into()),
/// };
/// let schema = Schema {
///     vertices: HashMap::from([
///         vertex("post", "record"),
///         vertex("user", "record"),
///         vertex("post.title", "string"),
///     ]),
///     edges: HashMap::from([(author.clone(), "fk".into()), (title.clone(), "prop".into())]),
///     ..Schema::default()
/// };
/// let protocol = Protocol {
///     value_kinds: BTreeMap::from([("string".to_owned(), ScalarKind::Str)]),
///     non_value_kinds: BTreeSet::from(["record".to_owned()]),
///     ..Protocol::default()
/// };
/// let rel = RelSchema::new(&schema, &protocol)?;
/// assert!(rel.is_table("post") && rel.is_table("user"));
/// assert!(!rel.is_table("post.title"));
/// assert!(rel.relations().iter().eq([&author]));
/// assert_eq!(rel.columns("post").get("title"), Some(&title));
/// # Ok::<(), panproto_inst::RelSchemaError>(())
/// ```
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RelSchema {
    /// The table objects.
    tables: BTreeSet<Name>,
    /// The relation objects, one per edge between two tables.
    relations: BTreeSet<Edge>,
    /// The columns of each table (every table has an entry): column name to
    /// attribute arrow.
    columns: BTreeMap<Name, BTreeMap<String, Edge>>,
    /// Each attribute arrow with its table's column name for it.
    attributes: BTreeMap<Edge, String>,
    /// The kind of each value vertex of the schema it was built from (none
    /// for one built from parts). Not an object of the entity category: the
    /// structure inside one cell.
    value_vertices: BTreeMap<Name, Name>,
    /// The inner value edges of the schema it was built from (none for one
    /// built from parts): the edges out of value vertices.
    inner_edges: BTreeSet<Edge>,
}

impl RelSchema {
    /// The attributed relation-ified schema of `schema` under `protocol`,
    /// read as the type documents.
    ///
    /// # Errors
    ///
    /// In this order: [`RelSchemaError::AmbiguousKind`] for the least kind
    /// the protocol declares both ways; [`RelSchemaError::UnclassifiedKind`]
    /// for the least vertex, not declared a table by a key or reference,
    /// whose kind the protocol declares neither way;
    /// [`RelSchemaError::ValueReachesTable`] for the least edge from a value
    /// vertex to a declared table or a vertex of a non-value kind; and
    /// [`RelSchemaError::DuplicateColumn`] for the least table, and column,
    /// that two attribute arrows name.
    pub fn new(schema: &Schema, protocol: &Protocol) -> Result<Self, RelSchemaError> {
        let non_value = &protocol.non_value_kinds;
        if let Some(kind) = protocol
            .value_kinds
            .keys()
            .find(|kind| non_value.contains(*kind))
        {
            return Err(RelSchemaError::AmbiguousKind { kind: kind.clone() });
        }
        let is_value_kind = |kind: &Name| protocol.value_kinds.contains_key(kind.as_ref());
        let is_non_value_kind = |kind: &Name| non_value.contains(kind.as_ref());

        let declared: BTreeSet<&Name> = schema
            .declarations
            .keys
            .iter()
            .filter(|(_, keys)| !keys.is_empty())
            .map(|(vertex, _)| vertex)
            .chain(
                schema
                    .declarations
                    .references
                    .values()
                    .flat_map(|r| [&r.holder, &r.target_vertex]),
            )
            .filter(|vertex| schema.vertices.contains_key(*vertex))
            .collect();

        let vertices: BTreeMap<&Name, &Name> = schema
            .vertices
            .iter()
            .map(|(id, vertex)| (id, &vertex.kind))
            .collect();
        if let Some((vertex, kind)) = vertices.iter().find(|(vertex, kind)| {
            !declared.contains(*vertex) && !is_value_kind(kind) && !is_non_value_kind(kind)
        }) {
            return Err(RelSchemaError::UnclassifiedKind {
                vertex: (*vertex).clone(),
                kind: (*kind).clone(),
            });
        }

        let edges: BTreeSet<&Edge> = schema.edges.keys().collect();
        let seeds = vertices
            .iter()
            .filter(|(vertex, kind)| !declared.contains(*vertex) && is_value_kind(kind))
            .map(|(vertex, _)| *vertex);
        let is_table = |vertex: &Name| {
            declared.contains(vertex) || vertices.get(vertex).is_some_and(|k| is_non_value_kind(k))
        };
        let values = closure_under(&edges, &vertices, seeds, is_table);
        if let Some(edge) = edges
            .iter()
            .find(|edge| values.contains(&edge.src) && is_table(&edge.tgt))
        {
            return Err(RelSchemaError::ValueReachesTable {
                edge: (*edge).clone(),
            });
        }

        let tables: BTreeSet<Name> = vertices
            .keys()
            .filter(|vertex| is_table(vertex))
            .map(|vertex| (*vertex).clone())
            .collect();
        let relations: BTreeSet<Edge> = edges
            .iter()
            .filter(|edge| tables.contains(&edge.src) && tables.contains(&edge.tgt))
            .map(|edge| (*edge).clone())
            .collect();
        let columns = columns_of(&edges, &tables, &values)?;
        let value_vertices = values
            .iter()
            .filter_map(|vertex| Some(((*vertex).clone(), (*vertices.get(vertex)?).clone())))
            .collect();
        let inner_edges = edges
            .iter()
            .filter(|edge| values.contains(&edge.src))
            .map(|edge| (*edge).clone())
            .collect();
        Ok(Self::assemble(
            tables,
            relations,
            columns,
            value_vertices,
            inner_edges,
        ))
    }

    /// The relation-ified schema with exactly the given tables, relations
    /// and columns (each table's column name to its attribute arrow). A table
    /// given no columns has none. It records no value vertices and no inner
    /// value edges.
    ///
    /// # Errors
    ///
    /// [`RelSchemaError::RelationEndpointNotATable`] for the least relation
    /// with an endpoint that is not a table, then
    /// [`RelSchemaError::ColumnEdgeMismatch`] for the least table, and
    /// column, whose entry is not an attribute arrow of that table under its
    /// own column name: the table is not a table, the arrow starts at
    /// another vertex or ends at a table, or its column name differs.
    pub fn from_parts(
        tables: impl IntoIterator<Item = Name>,
        relations: impl IntoIterator<Item = Edge>,
        columns: BTreeMap<Name, BTreeMap<String, Edge>>,
    ) -> Result<Self, RelSchemaError> {
        let tables: BTreeSet<Name> = tables.into_iter().collect();
        let relations: BTreeSet<Edge> = relations.into_iter().collect();
        if let Some(edge) = relations
            .iter()
            .find(|edge| !tables.contains(&edge.src) || !tables.contains(&edge.tgt))
        {
            return Err(RelSchemaError::RelationEndpointNotATable { edge: edge.clone() });
        }
        for (table, table_columns) in &columns {
            for (column, edge) in table_columns {
                if !tables.contains(table)
                    || edge.src != *table
                    || tables.contains(&edge.tgt)
                    || column_name(edge) != *column
                {
                    return Err(RelSchemaError::ColumnEdgeMismatch {
                        table: table.clone(),
                        column: column.clone(),
                    });
                }
            }
        }
        let mut all: BTreeMap<Name, BTreeMap<String, Edge>> = tables
            .iter()
            .map(|table| (table.clone(), BTreeMap::new()))
            .collect();
        all.extend(columns);
        Ok(Self::assemble(
            tables,
            relations,
            all,
            BTreeMap::new(),
            BTreeSet::new(),
        ))
    }

    /// Build the schema and its index of attribute arrows.
    fn assemble(
        tables: BTreeSet<Name>,
        relations: BTreeSet<Edge>,
        columns: BTreeMap<Name, BTreeMap<String, Edge>>,
        value_vertices: BTreeMap<Name, Name>,
        inner_edges: BTreeSet<Edge>,
    ) -> Self {
        let attributes = columns
            .values()
            .flat_map(|table_columns| {
                table_columns
                    .iter()
                    .map(|(column, edge)| (edge.clone(), column.clone()))
            })
            .collect();
        Self {
            tables,
            relations,
            columns,
            attributes,
            value_vertices,
            inner_edges,
        }
    }

    /// The table objects, ascending.
    #[must_use]
    pub const fn tables(&self) -> &BTreeSet<Name> {
        &self.tables
    }

    /// The relation objects, ascending.
    #[must_use]
    pub const fn relations(&self) -> &BTreeSet<Edge> {
        &self.relations
    }

    /// The columns of every table: each table (all of them) with its column
    /// names and their attribute arrows, ascending.
    #[must_use]
    pub const fn all_columns(&self) -> &BTreeMap<Name, BTreeMap<String, Edge>> {
        &self.columns
    }

    /// The columns of `table`, by column name; none when it is no table.
    #[must_use]
    pub fn columns(&self, table: &str) -> &BTreeMap<String, Edge> {
        self.columns.get(table).unwrap_or(&NO_COLUMNS)
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

    /// The column name of `edge` when it is an attribute arrow (its source is
    /// the table it is a column of).
    #[must_use]
    pub fn attribute_column(&self, edge: &Edge) -> Option<&str> {
        self.attributes.get(edge).map(String::as_str)
    }

    /// Whether `edge` is an inner value edge: an edge out of a value vertex.
    #[must_use]
    pub fn is_inner_edge(&self, edge: &Edge) -> bool {
        self.inner_edges.contains(edge)
    }

    /// The inner value edges, ascending.
    #[must_use]
    pub const fn inner_edges(&self) -> &BTreeSet<Edge> {
        &self.inner_edges
    }

    /// The kind of a value vertex, or `None` when `vertex` is none.
    #[must_use]
    pub fn value_kind(&self, vertex: &str) -> Option<&Name> {
        self.value_vertices.get(vertex)
    }
}

/// The columns of each table: every edge from a table to a value vertex,
/// under its column name.
///
/// # Errors
///
/// [`RelSchemaError::DuplicateColumn`] for the least table, and column, that
/// two attribute arrows name.
fn columns_of(
    edges: &BTreeSet<&Edge>,
    tables: &BTreeSet<Name>,
    values: &BTreeSet<&Name>,
) -> Result<BTreeMap<Name, BTreeMap<String, Edge>>, RelSchemaError> {
    let mut columns: BTreeMap<Name, BTreeMap<String, Edge>> = tables
        .iter()
        .map(|table| (table.clone(), BTreeMap::new()))
        .collect();
    let mut duplicates = BTreeSet::new();
    for edge in edges
        .iter()
        .filter(|edge| tables.contains(&edge.src) && values.contains(&edge.tgt))
    {
        let column = column_name(edge);
        match columns.entry(edge.src.clone()).or_default().entry(column) {
            std::collections::btree_map::Entry::Occupied(taken) => {
                duplicates.insert((edge.src.clone(), taken.key().clone()));
            }
            std::collections::btree_map::Entry::Vacant(free) => {
                free.insert((*edge).clone());
            }
        }
    }
    if let Some((table, column)) = duplicates.into_iter().next() {
        return Err(RelSchemaError::DuplicateColumn { table, column });
    }
    Ok(columns)
}

/// The least set of vertices of `vertices` holding `seeds` and closed under
/// `edges` into vertices that are not tables: the value vertices of a
/// schema. A table the closure reaches is not walked through and not held,
/// so the edges into tables out of the set are exactly the edges by which a
/// value reaches a table.
fn closure_under<'a>(
    edges: &BTreeSet<&'a Edge>,
    vertices: &BTreeMap<&'a Name, &'a Name>,
    seeds: impl Iterator<Item = &'a Name>,
    is_table: impl Fn(&Name) -> bool,
) -> BTreeSet<&'a Name> {
    let mut out_edges: BTreeMap<&Name, Vec<&Edge>> = BTreeMap::new();
    for edge in edges {
        out_edges.entry(&edge.src).or_default().push(edge);
    }
    let mut values: BTreeSet<&Name> = BTreeSet::new();
    let mut frontier: Vec<&Name> = seeds.collect();
    while let Some(vertex) = frontier.pop() {
        if !vertices.contains_key(vertex) || is_table(vertex) || !values.insert(vertex) {
            continue;
        }
        for edge in out_edges.get(vertex).into_iter().flatten() {
            frontier.push(&edge.tgt);
        }
    }
    values
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use panproto_schema::{
        Cardinality, Key, KeyPart, KeyRole, KeyScope, OnUnresolved, RefSource, Reference,
        ResolveMode, ScalarKind, Vertex,
    };

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

    fn schema(vertices: &[(&str, &str)], edges: &[Edge]) -> Schema {
        Schema {
            vertices: vertices.iter().map(|(id, kind)| vertex(id, kind)).collect(),
            edges: edges.iter().map(|e| (e.clone(), e.kind.clone())).collect(),
            ..Schema::default()
        }
    }

    /// A protocol declaring the given value kinds and non-value kinds.
    fn protocol(value: &[&str], non_value: &[&str]) -> Protocol {
        Protocol {
            value_kinds: value
                .iter()
                .map(|k| ((*k).to_owned(), ScalarKind::Any))
                .collect(),
            non_value_kinds: non_value.iter().map(|k| (*k).to_owned()).collect(),
            ..Protocol::default()
        }
    }

    fn primary_key(schema: &mut Schema, vertex: &str) {
        schema.declarations.keys.insert(
            vertex.into(),
            vec![Key {
                id: "pk".into(),
                parts: vec![KeyPart::Path(Vec::new())],
                role: KeyRole::Primary,
                scope: KeyScope::Global,
            }],
        );
    }

    /// A SQL-shaped schema: tables `post` and `user` with columns, and a
    /// foreign key between them.
    fn sql_like() -> (Schema, Protocol) {
        (
            schema(
                &[
                    ("post", "record"),
                    ("user", "record"),
                    ("post.title", "string"),
                    ("user.name", "string"),
                ],
                &[
                    edge("post", "post.title", "prop", "title"),
                    edge("user", "user.name", "prop", "name"),
                    edge("post", "user", "foreign-key", "author"),
                ],
            ),
            protocol(&["string"], &["record"]),
        )
    }

    fn names(set: &BTreeSet<Name>) -> Vec<&str> {
        set.iter().map(AsRef::as_ref).collect()
    }

    #[test]
    fn value_closure_walks_each_non_table_vertex_once() {
        let a = Name::from("a");
        let b = Name::from("b");
        let c = Name::from("c");
        let table = Name::from("table");
        let kind = Name::from("value");
        let vertices = BTreeMap::from([(&a, &kind), (&b, &kind), (&c, &kind), (&table, &kind)]);
        let a_to_b = edge("a", "b", "inner", "ab");
        let b_to_c = edge("b", "c", "inner", "bc");
        let c_to_b = edge("c", "b", "inner", "cb");
        let b_to_table = edge("b", "table", "inner", "bt");
        let c_to_missing = edge("c", "missing", "inner", "cm");
        let edges = BTreeSet::from([&a_to_b, &b_to_c, &c_to_b, &b_to_table, &c_to_missing]);

        assert_eq!(
            closure_under(&edges, &vertices, std::iter::once(&a), |vertex| {
                vertex == &table
            }),
            BTreeSet::from([&a, &b, &c])
        );
    }

    #[test]
    fn rel_schema_reads_tables_from_the_non_value_kinds() -> Result<(), RelSchemaError> {
        let (schema, protocol) = sql_like();
        let rel = RelSchema::new(&schema, &protocol)?;
        assert_eq!(names(rel.tables()), ["post", "user"]);
        assert!(
            rel.relations()
                .iter()
                .eq([&edge("post", "user", "foreign-key", "author")])
        );
        assert_eq!(
            rel.columns("post").get("title"),
            Some(&edge("post", "post.title", "prop", "title"))
        );
        assert_eq!(rel.columns("user").len(), 1);
        assert!(rel.columns("post.title").is_empty());
        assert_eq!(
            rel.attribute_column(&edge("user", "user.name", "prop", "name")),
            Some("name")
        );
        Ok(())
    }

    #[test]
    fn an_empty_table_is_still_a_table() -> Result<(), Box<dyn std::error::Error>> {
        let (schema, protocol) = sql_like();
        let rel = RelSchema::new(&schema, &protocol)?;
        // No instance is consulted: a table with no rows and one with rows
        // are both tables, and a column is never one.
        let rows = FInstance::new().with_table("post", [HashMap::new()])?;
        assert_eq!(rows.row_count("user"), 0);
        assert!(rel.is_table("user") && rel.is_table("post"));
        assert!(!rel.is_table("post.title"));
        assert_eq!(RelSchema::new(&schema, &protocol)?, rel);
        Ok(())
    }

    /// A vertex of a value kind is an attribute whatever edge reaches it, or
    /// none: one no edge reaches is the column of no table, an array column
    /// and its element are one cell's value, and an edge out of a value
    /// vertex is an inner value edge, never a relation.
    #[test]
    fn a_vertex_of_a_value_kind_is_an_attribute() -> Result<(), RelSchemaError> {
        let items = Edge {
            src: "post.tags".into(),
            tgt: "post.tags:items".into(),
            kind: "items".into(),
            name: None,
        };
        let s = schema(
            &[
                ("post", "record"),
                ("post.tags", "array"),
                ("post.tags:items", "string"),
                ("lone", "integer"),
            ],
            &[edge("post", "post.tags", "prop", "tags"), items.clone()],
        );
        let rel = RelSchema::new(&s, &protocol(&["array", "string", "integer"], &["record"]))?;
        assert_eq!(names(rel.tables()), ["post"]);
        assert!(rel.relations().is_empty());
        assert_eq!(rel.columns("post").keys().collect::<Vec<_>>(), ["tags"]);
        assert!(rel.is_inner_edge(&items));
        assert_eq!(
            rel.value_kind("post.tags:items"),
            Some(&Name::from("string"))
        );
        assert_eq!(rel.value_kind("lone"), Some(&Name::from("integer")));
        Ok(())
    }

    /// A vertex with a key, and the holder and target of a reference, are
    /// tables whatever their kind, even a value kind.
    #[test]
    fn a_declared_vertex_is_a_table_whatever_its_kind() -> Result<(), RelSchemaError> {
        let mut s = schema(
            &[
                ("post", "record"),
                ("tag", "string"),
                ("a", "string"),
                ("b", "mystery"),
            ],
            &[
                edge("post", "tag", "prop", "tag"),
                edge("a", "b", "ref", "r"),
            ],
        );
        primary_key(&mut s, "tag");
        s.declarations.references.insert(
            "r".into(),
            Reference {
                id: "r".into(),
                holder: "a".into(),
                source: RefSource::KeyValue { parts: Vec::new() },
                target_vertex: "b".into(),
                target_key: None,
                edge: None,
                cardinality: Cardinality::One,
                nullable: false,
                on_unresolved: OnUnresolved::Error,
                mode: ResolveMode::AddEdge,
            },
        );
        let rel = RelSchema::new(&s, &protocol(&["string"], &["record"]))?;
        assert_eq!(names(rel.tables()), ["a", "b", "post", "tag"]);
        assert!(rel.is_relation(&edge("post", "tag", "prop", "tag")));
        assert!(rel.is_relation(&edge("a", "b", "ref", "r")));
        Ok(())
    }

    /// A tag kind the protocol declares a value kind (CoNLL-U's `upos-tag`)
    /// is an attribute of the word it hangs from.
    #[test]
    fn a_declared_tag_kind_is_an_attribute() -> Result<(), RelSchemaError> {
        let s = schema(
            &[("w", "word"), ("w.upos", "upos-tag"), ("v", "word")],
            &[
                edge("w", "w.upos", "upos", "upos"),
                edge("w", "v", "dep", "nsubj"),
            ],
        );
        let rel = RelSchema::new(&s, &protocol(&["upos-tag"], &["word"]))?;
        assert_eq!(names(rel.tables()), ["v", "w"]);
        assert_eq!(rel.columns("w").keys().collect::<Vec<_>>(), ["upos"]);
        assert!(rel.is_relation(&edge("w", "v", "dep", "nsubj")));
        Ok(())
    }

    /// The edge kind never decides a column: a `prop` edge into a vertex of a
    /// non-value kind is a relation, and an edge of any other kind into a
    /// value vertex is an attribute arrow.
    #[test]
    fn the_edge_kind_never_decides_a_column() -> Result<(), RelSchemaError> {
        let into_meta = edge("post", "meta", "prop", "meta");
        let s = schema(
            &[
                ("post", "object"),
                ("meta", "object"),
                ("meta.id", "integer"),
            ],
            &[into_meta.clone(), edge("meta", "meta.id", "anything", "id")],
        );
        let rel = RelSchema::new(&s, &protocol(&["integer"], &["object"]))?;
        assert!(rel.is_table("meta"));
        assert!(rel.is_relation(&into_meta));
        assert_eq!(rel.columns("meta").keys().collect::<Vec<_>>(), ["id"]);
        Ok(())
    }

    /// The kind name `table` is not special: a protocol that declares it a
    /// value kind makes its vertices attributes, and one that declares it
    /// neither way leaves it unclassified.
    #[test]
    fn the_kind_name_table_is_not_special() -> Result<(), RelSchemaError> {
        let s = schema(
            &[("r", "record"), ("t", "table")],
            &[edge("r", "t", "prop", "t")],
        );
        let rel = RelSchema::new(&s, &protocol(&["table"], &["record"]))?;
        assert_eq!(names(rel.tables()), ["r"]);
        assert_eq!(rel.columns("r").keys().collect::<Vec<_>>(), ["t"]);
        assert_eq!(
            RelSchema::new(&s, &protocol(&[], &["record"])),
            Err(RelSchemaError::UnclassifiedKind {
                vertex: "t".into(),
                kind: "table".into()
            })
        );
        Ok(())
    }

    /// A leaf of a non-value kind (UCCA's `terminal`) stays a table, so its
    /// rows and the foreign keys into it are not cells.
    #[test]
    fn a_leaf_of_a_non_value_kind_is_a_table() -> Result<(), RelSchemaError> {
        let into_terminal = edge("node", "terminal", "edge", "e");
        let s = schema(
            &[("node", "node"), ("terminal", "terminal")],
            std::slice::from_ref(&into_terminal),
        );
        let rel = RelSchema::new(&s, &protocol(&[], &["node", "terminal"]))?;
        assert!(rel.is_table("terminal"));
        assert!(rel.is_relation(&into_terminal));
        Ok(())
    }

    #[test]
    fn an_unclassified_kind_is_refused() {
        let s = schema(&[("b", "mystery"), ("a", "record"), ("c", "enigma")], &[]);
        assert_eq!(
            RelSchema::new(&s, &protocol(&["string"], &["record"])),
            Err(RelSchemaError::UnclassifiedKind {
                vertex: "b".into(),
                kind: "mystery".into()
            })
        );
    }

    #[test]
    fn a_kind_declared_both_ways_is_refused() {
        // Refused before any vertex is looked at, the least kind first.
        let s = schema(&[("a", "unknown")], &[]);
        assert_eq!(
            RelSchema::new(&s, &protocol(&["x", "y", "z"], &["z", "y"])),
            Err(RelSchemaError::AmbiguousKind { kind: "y".into() })
        );
    }

    /// An edge out of a value vertex into a declared table is refused: a cell
    /// cannot hold a foreign key. The least such edge is named.
    #[test]
    fn a_value_reaching_a_table_is_refused() {
        let mut s = schema(
            &[
                ("post", "record"),
                ("post.tags", "array"),
                ("post.tags:items", "string"),
                ("user", "string"),
            ],
            &[
                edge("post", "post.tags", "prop", "tags"),
                edge("post.tags", "post.tags:items", "items", "items"),
                edge("post.tags:items", "user", "fk", "owner"),
                edge("post.tags", "user", "fk", "owner"),
            ],
        );
        primary_key(&mut s, "user");
        assert_eq!(
            RelSchema::new(&s, &protocol(&["array", "string"], &["record"])),
            Err(RelSchemaError::ValueReachesTable {
                edge: edge("post.tags", "user", "fk", "owner")
            })
        );
    }

    /// The refusal names the edge out of the value vertex, not a relation
    /// between two tables that the value reaches through it: here `b -> a`
    /// is the lesser edge, but `b` is a table, not a value vertex.
    #[test]
    fn a_value_reaching_a_table_names_the_edge_out_of_the_value() {
        let s = schema(
            &[("a", "record"), ("b", "record"), ("v", "string")],
            &[edge("v", "b", "x", "x"), edge("b", "a", "fk", "fk")],
        );
        assert_eq!(
            RelSchema::new(&s, &protocol(&["string"], &["record"])),
            Err(RelSchemaError::ValueReachesTable {
                edge: edge("v", "b", "x", "x")
            })
        );
    }

    #[test]
    fn a_value_reaching_a_non_value_kind_is_refused() {
        let s = schema(
            &[
                ("post", "record"),
                ("post.meta", "json"),
                ("inner", "record"),
            ],
            &[
                edge("post", "post.meta", "prop", "meta"),
                edge("post.meta", "inner", "field", "inner"),
            ],
        );
        assert_eq!(
            RelSchema::new(&s, &protocol(&["json"], &["record"])),
            Err(RelSchemaError::ValueReachesTable {
                edge: edge("post.meta", "inner", "field", "inner")
            })
        );
    }

    /// Two attribute arrows of one table with one column name are refused:
    /// by edge name, or by target id when an edge has no name.
    #[test]
    fn two_attribute_arrows_with_one_column_name_are_refused() {
        let unnamed = Edge {
            src: "w".into(),
            tgt: "x".into(),
            kind: "k".into(),
            name: None,
        };
        let s = schema(
            &[("w", "word"), ("v", "word"), ("x", "tag"), ("y", "tag")],
            &[
                edge("w", "y", "a", "root"),
                edge("w", "x", "b", "root"),
                edge("v", "x", "c", "x"),
                unnamed,
                edge("v", "y", "d", "x"),
            ],
        );
        assert_eq!(
            RelSchema::new(&s, &protocol(&["tag"], &["word"])),
            Err(RelSchemaError::DuplicateColumn {
                table: "v".into(),
                column: "x".into()
            })
        );
    }

    /// The errors are reported in the order the classification runs.
    #[test]
    fn errors_come_in_the_order_of_the_classification() {
        let s = schema(
            &[("w", "word"), ("x", "tag"), ("t", "other")],
            &[edge("w", "x", "a", "c"), edge("w", "x", "b", "c")],
        );
        // Unclassified before the duplicate column.
        assert!(matches!(
            RelSchema::new(&s, &protocol(&["tag"], &["word"])),
            Err(RelSchemaError::UnclassifiedKind { .. })
        ));
        // Ambiguous before unclassified.
        assert!(matches!(
            RelSchema::new(&s, &protocol(&["tag", "word"], &["word"])),
            Err(RelSchemaError::AmbiguousKind { .. })
        ));
    }

    #[test]
    fn from_parts_refuses_a_relation_off_its_tables() -> Result<(), RelSchemaError> {
        let e = edge("a", "b", "fk", "e");
        assert_eq!(
            RelSchema::from_parts([Name::from("a")], [e.clone()], BTreeMap::new()),
            Err(RelSchemaError::RelationEndpointNotATable { edge: e.clone() })
        );
        let column = edge("a", "a.x", "prop", "x");
        let ok = RelSchema::from_parts(
            [Name::from("a"), Name::from("b")],
            [e],
            BTreeMap::from([(
                Name::from("a"),
                BTreeMap::from([("x".to_owned(), column.clone())]),
            )]),
        )?;
        assert_eq!(ok.columns("a").get("x"), Some(&column));
        assert!(ok.columns("b").is_empty());
        for (table, name, arrow) in [
            ("a", "y", column.clone()),
            ("b", "x", column),
            ("c", "x", edge("c", "c.x", "prop", "x")),
            ("a", "b", edge("a", "b", "prop", "b")),
        ] {
            assert_eq!(
                RelSchema::from_parts(
                    [Name::from("a"), Name::from("b")],
                    [],
                    BTreeMap::from([(
                        Name::from(table),
                        BTreeMap::from([(name.to_owned(), arrow)])
                    )]),
                ),
                Err(RelSchemaError::ColumnEdgeMismatch {
                    table: table.into(),
                    column: name.to_owned()
                })
            );
        }
        Ok(())
    }
}
