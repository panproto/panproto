//! The edge-list graph formats.
//!
//! An edge list is a table with one record per directed edge: the node the
//! edge leaves, the node it enters and, optionally, a label. Its columns
//! follow the edges table of [Gephi's spreadsheet import](https://docs.gephi.org/desktop/User_Manual/Import/CSV_Format/):
//! a header record names a `Source` and a `Target` column (the names
//! compared without regard to ASCII case) and may name a `Label` column.
//! Two dialects are registered, one per delimited text format the
//! generic text routes read: `edgelist-csv` over RFC 4180 CSV and
//! `edgelist-tsv` over IANA tab-separated values.
//!
//! Every edge-list protocol reads its documents into one graph encoding,
//! the *edge-list graph encoding*, fixed by
//! [`graph_schema`](crate::edgelist::graph_schema):
//!
//! - one `node` node per distinct node name, holding the name as a string;
//! - one `edge` node per record, holding the record's label as a string
//!   when the document has a `Label` column and no value otherwise, with a
//!   `source` edge and a `target` edge to `node` nodes.
//!
//! An edge is a node rather than an instance edge so that it carries its
//! label, and so that two records naming the same pair of nodes (parallel
//! edges) stay two edges with their own labels and their own place in the
//! order of the records.

use panproto_schema::{EdgeRule, Protocol, Schema, SchemaBuilder, SchemaError};

/// The CSV edge-list protocol.
pub mod csv;
/// The TSV edge-list protocol.
pub mod tsv;

/// The vertex of node nodes.
pub const NODE: &str = "node";
/// The vertex of edge nodes.
pub const EDGE: &str = "edge";

/// The edge from an edge node to the node it leaves.
pub const SOURCE: &str = "source";
/// The edge from an edge node to the node it enters.
pub const TARGET: &str = "target";

/// The kind of every edge of the edge-list graph encoding.
pub const EDGE_KIND: &str = "prop";

/// The schema edges of the edge-list graph encoding, as
/// `(source, target, name)`.
pub const EDGES: &[(&str, &str, &str)] = &[(EDGE, NODE, SOURCE), (EDGE, NODE, TARGET)];

/// The vertex kinds of the edge-list graph encoding.
#[must_use]
pub fn obj_kinds() -> Vec<String> {
    vec![EDGE.to_owned(), NODE.to_owned()]
}

/// The edge rule of the edge-list graph encoding: a `prop` edge leaves an
/// edge node and enters a node.
#[must_use]
pub fn edge_rules() -> Vec<EdgeRule> {
    vec![EdgeRule {
        edge_kind: EDGE_KIND.into(),
        src_kinds: vec![EDGE.into()],
        tgt_kinds: vec![NODE.into()],
    }]
}

/// The schema of the edge-list graph encoding under `protocol`: the two
/// vertices, each named for its kind, and the edges of [`EDGES`].
///
/// # Errors
///
/// Returns [`SchemaError`] when `protocol` does not admit the encoding's
/// vertex kinds and edges.
pub fn graph_schema(protocol: &Protocol) -> Result<Schema, SchemaError> {
    let mut builder = SchemaBuilder::new(protocol);
    for kind in [EDGE, NODE] {
        builder = builder.vertex(kind, kind, None)?;
    }
    for (src, tgt, name) in EDGES {
        builder = builder.edge(src, tgt, EDGE_KIND, Some(name))?;
    }
    builder.build()
}
