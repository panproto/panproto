//! Edge lists: CSV and TSV tables of directed edges.
//!
//! # The dialect
//!
//! An edge list is a delimited text with a header record, in one of two
//! dialects: [`Dialect::Csv`], RFC 4180 CSV as the CSV route reads it, and
//! [`Dialect::Tsv`], IANA `text/tab-separated-values` as the TSV route
//! reads it. The header names the columns, following the edges table of
//! Gephi's spreadsheet import: a `source` column and a `target` column are
//! required and a `label` column is optional, each name compared without
//! regard to ASCII case and in any order. Every later record is one
//! directed edge from the node its `source` field names to the node its
//! `target` field names, with its `label` field as the edge's label.
//!
//! A node is its name, the field's text exactly (nothing is trimmed or
//! folded), and an empty name is refused. Nodes have no attributes: an edge
//! list holds no node table. A record naming the same node twice is a
//! self-loop, and two records naming the same pair of nodes are two
//! parallel edges; both are kept as written, and nothing is ever merged.
//! A column other than these three (Gephi's `Type`, `Id`, `Weight` and
//! `Interval` among them), a column named twice, a missing `source` or
//! `target` column and a document without a header are refused with an
//! [`EdgeListReadError`] that names the column or record.
//!
//! # The graph
//!
//! [`to_graph`] encodes a list of edges in the edge-list graph encoding of
//! [`panproto_protocols::edgelist`]: one `node` node per distinct name and
//! one `edge` node per record, holding the label when the document has a
//! `label` column. [`from_graph`] reads the edges back out of a graph,
//! refusing every graph no edge list reads back as, and [`write`](fn@write) prints
//! them as the canonical edge list.

use std::collections::{BTreeSet, HashMap};
use std::fmt;

use panproto_inst::{FieldPresence, GInstance, InstError, Node, NodeShape, Value};
use panproto_protocols::edgelist::{EDGE, EDGE_KIND, EDGES, NODE, SOURCE, TARGET};
use panproto_schema::{Edge as SchemaEdge, Schema};

#[cfg(feature = "tree-sitter")]
mod reader;
#[cfg(feature = "tree-sitter")]
pub use reader::EdgeListCodec;

/// Register the edge-list graph codecs (`edgelist-csv`, `edgelist-tsv`)
/// with the registry's graph table.
///
/// Each reads through a bundled tree-sitter grammar, so a codec whose
/// grammar is not compiled in is left out, and one whose grammar fails to
/// initialise is reported on stderr and left out.
#[cfg(feature = "tree-sitter")]
pub fn register_all(registry: &mut crate::ProtocolRegistry) {
    for dialect in [Dialect::Csv, Dialect::Tsv] {
        match EdgeListCodec::new(dialect) {
            Ok(codec) => registry.register_graph(codec),
            Err(crate::error::UnifiedCodecError::MissingGrammar { .. }) => {}
            Err(err) => eprintln!(
                "panproto-io: bundled grammar failed parser initialisation; \
                 skipping registration: {err}"
            ),
        }
    }
}

/// The delimited text an edge list is written in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Dialect {
    /// RFC 4180 CSV, registered as `edgelist-csv`.
    Csv,
    /// IANA tab-separated values, registered as `edgelist-tsv`.
    Tsv,
}

impl Dialect {
    /// The protocol the dialect is registered under.
    #[must_use]
    pub const fn protocol(self) -> &'static str {
        match self {
            Self::Csv => "edgelist-csv",
            Self::Tsv => "edgelist-tsv",
        }
    }

    /// The field delimiter.
    #[must_use]
    pub const fn delimiter(self) -> char {
        match self {
            Self::Csv => ',',
            Self::Tsv => '\t',
        }
    }
}

/// One directed edge of an edge list.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Edge {
    /// The name of the node the edge leaves.
    pub source: String,
    /// The name of the node the edge enters.
    pub target: String,
    /// The edge's label, present exactly when the document has a `label`
    /// column.
    pub label: Option<String>,
}

/// A refused edge-list document.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum EdgeListReadError {
    /// The document is not UTF-8.
    #[error("the document is not UTF-8: invalid byte sequence at byte {offset}")]
    NotUtf8 {
        /// The offset of the first byte that is not part of a valid
        /// sequence.
        offset: usize,
    },
    /// The CSV or TSV route refuses the text: a quoting fault the grammar
    /// cannot read, or a record whose field count differs from the
    /// header's.
    #[error("the delimited text is refused: {message}")]
    Table {
        /// The route's refusal.
        message: String,
    },
    /// The document holds no record, so it has no header.
    #[error("the document has no header record")]
    NoHeader,
    /// A header field names a column other than `source`, `target` and
    /// `label`.
    #[error("header column {column} is {name:?}, which is not source, target or label")]
    UnsupportedColumn {
        /// The column's position in the header, counting from 0.
        column: usize,
        /// The header field as written.
        name: String,
    },
    /// A header field names a column an earlier field names, up to ASCII
    /// case.
    #[error("header column {column} names {name:?}, which an earlier column names")]
    DuplicateColumn {
        /// The column's position in the header, counting from 0.
        column: usize,
        /// The header field as written.
        name: String,
    },
    /// The header names no `source` column, or no `target` column.
    #[error("the header names no {name} column")]
    MissingColumn {
        /// `source` or `target`.
        name: String,
    },
    /// A record's `source` or `target` field is empty.
    #[error("record {record} has an empty {column} field")]
    EmptyName {
        /// The record, counting data records from 1 (blank lines are not
        /// records).
        record: usize,
        /// `source` or `target`.
        column: String,
    },
}

/// Why a graph has no edge list.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum EdgeListWriteErrorKind {
    /// The node is anchored at neither `node` nor `edge`.
    UnknownAnchor {
        /// The anchor.
        anchor: String,
    },
    /// A `node` node holds no string, or an `edge` node holds a value that
    /// is not a string.
    Value,
    /// The node carries data the encoding has no place for: a
    /// discriminator, extra fields, annotations, a position or a shape.
    Presentation,
    /// The edge is not an edge of the encoding between its endpoints'
    /// anchors, or it carries a position.
    UnexpectedEdge {
        /// The edge's name.
        name: String,
    },
    /// The `edge` node has other than exactly one out-edge `name`.
    EdgeCount {
        /// The edge's name.
        name: String,
        /// How many the node has.
        count: usize,
    },
    /// The `node` node's name is empty, which no field can name.
    EmptyName,
    /// The name or label holds a character the dialect cannot write in a
    /// field (a tab, a line feed or a carriage return in TSV).
    UnwritableCharacter {
        /// The character.
        character: char,
    },
    /// Another `node` node holds the same name, so a document would merge
    /// the two.
    DuplicateNode {
        /// The other node.
        other: u32,
    },
    /// This `edge` node has a label and an earlier one has none, or the
    /// converse: a document either has a `label` column or has none.
    MixedLabels {
        /// The earliest `edge` node.
        first: u32,
    },
    /// The `node` node is the source or target of no edge, so no record
    /// names it and a document would drop it.
    Isolated,
}

impl fmt::Display for EdgeListWriteErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownAnchor { anchor } => write!(
                f,
                "anchor {anchor:?} is not a vertex of the edge-list graph encoding"
            ),
            Self::Value => f.write_str("the node's value does not fit its anchor"),
            Self::Presentation => {
                f.write_str("the node carries data the edge-list graph encoding has no place for")
            }
            Self::UnexpectedEdge { name } => write!(
                f,
                "edge {name:?} is not an edge of the edge-list graph encoding here"
            ),
            Self::EdgeCount { name, count } => write!(f, "the node has {count} {name:?} edges"),
            Self::EmptyName => f.write_str("the node's name is empty"),
            Self::UnwritableCharacter { character } => write!(
                f,
                "the dialect cannot write U+{:04X} in a field",
                u32::from(*character)
            ),
            Self::DuplicateNode { other } => write!(f, "node {other} holds the same name"),
            Self::MixedLabels { first } => write!(
                f,
                "edge node {first} and this one disagree on whether edges have labels"
            ),
            Self::Isolated => f.write_str("the node is the source or target of no edge"),
        }
    }
}

/// A graph refused by an edge-list writer: why, and the node at fault.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("node {node}: {kind}")]
pub struct EdgeListWriteError {
    /// The node at fault (the source node, for an edge).
    pub node: u32,
    /// Why the graph is refused.
    pub kind: EdgeListWriteErrorKind,
}

impl EdgeListWriteError {
    const fn new(node: u32, kind: EdgeListWriteErrorKind) -> Self {
        Self { node, kind }
    }
}

// ── Edges to graph ─────────────────────────────────────────────────────

fn encoding_edge(name: &str) -> SchemaEdge {
    SchemaEdge {
        src: EDGE.into(),
        tgt: NODE.into(),
        kind: EDGE_KIND.into(),
        name: Some(name.into()),
    }
}

fn string_node(id: u32, anchor: &str, value: &str) -> Node {
    Node::new(id, anchor).with_value(FieldPresence::Present(Value::Str(value.to_owned())))
}

/// The graph of `edges` in the edge-list graph encoding.
///
/// Nodes are numbered from 0 in record order: for each edge, its `edge`
/// node, then its source's `node` node and its target's when the name is
/// new. Each `edge` node's `source` edge is added before its `target`
/// edge, and edges are numbered in the order they are added.
///
/// # Errors
///
/// Returns [`InstError::NodeIdsExhausted`] or
/// [`InstError::EdgeIdsExhausted`] when the edges need more nodes or edges
/// than a graph can number.
pub fn to_graph(edges: &[Edge]) -> Result<GInstance, InstError> {
    let mut graph = GInstance::new();
    let mut names: HashMap<&str, u32> = HashMap::new();
    let mut next: u32 = 0;
    let mut fresh = || -> Result<u32, InstError> {
        let id = next;
        next = id.checked_add(1).ok_or(InstError::NodeIdsExhausted)?;
        Ok(id)
    };
    for edge in edges {
        let id = fresh()?;
        let mut node = Node::new(id, EDGE);
        if let Some(label) = &edge.label {
            node = node.with_value(FieldPresence::Present(Value::Str(label.clone())));
        }
        graph.insert_node(node)?;
        for (name, end) in [(SOURCE, &edge.source), (TARGET, &edge.target)] {
            let target = if let Some(&known) = names.get(end.as_str()) {
                known
            } else {
                let new = fresh()?;
                graph.insert_node(string_node(new, NODE, end))?;
                names.insert(end, new);
                new
            };
            graph.add_edge(id, target, encoding_edge(name))?;
        }
    }
    Ok(graph)
}

/// Whether `schema` holds the edge-list graph encoding: each of its
/// vertices with its kind and each of its edges.
#[must_use]
pub fn schema_holds_encoding(schema: &Schema) -> bool {
    [EDGE, NODE].iter().all(|kind| {
        schema
            .vertices
            .get(*kind)
            .is_some_and(|vertex| vertex.kind.as_ref() == *kind)
    }) && EDGES
        .iter()
        .all(|(_, _, name)| schema.edges.contains_key(&encoding_edge(name)))
}

// ── Graph to edges ─────────────────────────────────────────────────────

/// The character of `text` that `dialect` cannot write in a field.
fn unwritable(text: &str, dialect: Dialect) -> Option<char> {
    match dialect {
        Dialect::Csv => None,
        Dialect::Tsv => text.chars().find(|c| matches!(c, '\t' | '\n' | '\r')),
    }
}

/// The node's string value, or `None` when it has none.
fn node_text(node: &Node) -> Result<Option<&str>, EdgeListWriteError> {
    match &node.value {
        None => Ok(None),
        Some(FieldPresence::Present(Value::Str(text))) => Ok(Some(text)),
        Some(_) => Err(EdgeListWriteError::new(
            node.id,
            EdgeListWriteErrorKind::Value,
        )),
    }
}

/// Check one node and its out-edges; for an `edge` node, return its
/// source and target nodes.
fn check_node(
    graph: &GInstance,
    id: u32,
    dialect: Dialect,
) -> Result<Option<(u32, u32)>, EdgeListWriteError> {
    let node = &graph.nodes()[&id];
    let anchor = node.anchor.as_ref();
    let fail = |kind| Err(EdgeListWriteError::new(id, kind));
    if anchor != EDGE && anchor != NODE {
        return fail(EdgeListWriteErrorKind::UnknownAnchor {
            anchor: anchor.to_owned(),
        });
    }
    if node.discriminator.is_some()
        || !node.extra_fields.is_empty()
        || !node.annotations.is_empty()
        || node.position.is_some()
        || node.shape != NodeShape::Plain
    {
        return fail(EdgeListWriteErrorKind::Presentation);
    }
    let text = node_text(node)?;
    match (anchor, text) {
        (NODE, None) => return fail(EdgeListWriteErrorKind::Value),
        (NODE, Some("")) => return fail(EdgeListWriteErrorKind::EmptyName),
        _ => {}
    }
    if let Some(character) = text.and_then(|t| unwritable(t, dialect)) {
        return fail(EdgeListWriteErrorKind::UnwritableCharacter { character });
    }
    let mut ends: [Vec<u32>; 2] = [Vec::new(), Vec::new()];
    for edge_id in graph.out_edges(id) {
        let edge = &graph.edges()[edge_id];
        let name = edge.edge.name.as_deref().unwrap_or("");
        let slot = [SOURCE, TARGET].iter().position(|n| *n == name);
        let target_anchor = graph.nodes()[&edge.tgt].anchor.as_ref();
        match slot {
            Some(slot)
                if anchor == EDGE
                    && target_anchor == NODE
                    && edge.position.is_none()
                    && edge.edge == encoding_edge(name) =>
            {
                ends[slot].push(edge.tgt);
            }
            _ => {
                return fail(EdgeListWriteErrorKind::UnexpectedEdge {
                    name: name.to_owned(),
                });
            }
        }
    }
    if anchor == NODE {
        return Ok(None);
    }
    for (name, found) in [SOURCE, TARGET].iter().zip(&ends) {
        if found.len() != 1 {
            return fail(EdgeListWriteErrorKind::EdgeCount {
                name: (*name).to_owned(),
                count: found.len(),
            });
        }
    }
    Ok(Some((ends[0][0], ends[1][0])))
}

/// The edges of a graph in the edge-list graph encoding, in ascending order
/// of their `edge` nodes' ids.
///
/// Nodes are checked in ascending id order, and the first fault found is
/// returned: for each node, its anchor, its presentation data, its value
/// (a `node` node's name present and not empty) and the characters the
/// dialect cannot write, then its out-edges (each a `source` or `target`
/// edge of the encoding from an `edge` node, with no position, and one of
/// each); then, again in id order, each `node` node whose name a node of a
/// smaller id already holds; then each `edge` node that disagrees with the
/// first on having a label; then each `node` node no edge reaches.
///
/// # Errors
///
/// Returns [`EdgeListWriteError`] naming the node at fault when no edge
/// list in `dialect` reads back as the graph.
pub fn from_graph(graph: &GInstance, dialect: Dialect) -> Result<Vec<Edge>, EdgeListWriteError> {
    let mut ids: Vec<u32> = graph.nodes().keys().copied().collect();
    ids.sort_unstable();
    let mut ends: Vec<(u32, (u32, u32))> = Vec::new();
    for &id in &ids {
        if let Some(pair) = check_node(graph, id, dialect)? {
            ends.push((id, pair));
        }
    }
    let text = |id: u32| match &graph.nodes()[&id].value {
        Some(FieldPresence::Present(Value::Str(text))) => Some(text.clone()),
        _ => None,
    };
    let mut names: HashMap<String, u32> = HashMap::new();
    for &id in &ids {
        if graph.nodes()[&id].anchor.as_ref() == NODE {
            let name = text(id).unwrap_or_default();
            if let Some(&other) = names.get(&name) {
                return Err(EdgeListWriteError::new(
                    id,
                    EdgeListWriteErrorKind::DuplicateNode { other },
                ));
            }
            names.insert(name, id);
        }
    }
    if let Some(&(first, _)) = ends.first() {
        let labelled = text(first).is_some();
        if let Some(&(id, _)) = ends.iter().find(|(id, _)| text(*id).is_some() != labelled) {
            return Err(EdgeListWriteError::new(
                id,
                EdgeListWriteErrorKind::MixedLabels { first },
            ));
        }
    }
    let used: BTreeSet<u32> = ends.iter().flat_map(|(_, (s, t))| [*s, *t]).collect();
    if let Some(&id) = ids
        .iter()
        .find(|id| graph.nodes()[id].anchor.as_ref() == NODE && !used.contains(id))
    {
        return Err(EdgeListWriteError::new(
            id,
            EdgeListWriteErrorKind::Isolated,
        ));
    }
    Ok(ends
        .iter()
        .map(|&(id, (source, target))| Edge {
            source: text(source).unwrap_or_default(),
            target: text(target).unwrap_or_default(),
            label: text(id),
        })
        .collect())
}

// ── Canonical text ─────────────────────────────────────────────────────

/// Append `field` as a field of `dialect`: in CSV, between quotes with each
/// quote doubled when it is empty or holds `,`, `"`, a carriage return or
/// a line feed, and as itself otherwise; in TSV, as itself.
fn push_field(out: &mut String, field: &str, dialect: Dialect) {
    let quote =
        dialect == Dialect::Csv && (field.is_empty() || field.contains([',', '"', '\r', '\n']));
    if quote {
        out.push('"');
        out.push_str(&field.replace('"', "\"\""));
        out.push('"');
    } else {
        out.push_str(field);
    }
}

/// The canonical edge list of `edges` in `dialect`.
///
/// The header is `source` and `target`, followed by `label` when the
/// edges have labels, and each edge is one record in the order given; the
/// fields are separated by the dialect's delimiter and every record,
/// the header included, ends in a line feed. A CSV field is quoted exactly
/// when it is empty or holds `,`, `"`, a carriage return or a line feed.
///
/// Returns `None` when no document in `dialect` reads back as `edges`:
/// some edges have labels and others none, a name is empty, or (in TSV) a
/// name or label holds a tab, a line feed or a carriage return.
#[must_use]
pub fn write(edges: &[Edge], dialect: Dialect) -> Option<String> {
    let labelled = edges.first().is_some_and(|edge| edge.label.is_some());
    let delimiter = dialect.delimiter();
    let mut out = String::new();
    for (i, column) in ["source", "target", "label"]
        .iter()
        .take(if labelled { 3 } else { 2 })
        .enumerate()
    {
        if i > 0 {
            out.push(delimiter);
        }
        out.push_str(column);
    }
    out.push('\n');
    for edge in edges {
        if edge.label.is_some() != labelled || edge.source.is_empty() || edge.target.is_empty() {
            return None;
        }
        let fields = [Some(&edge.source), Some(&edge.target), edge.label.as_ref()];
        for (i, field) in fields.into_iter().flatten().enumerate() {
            if unwritable(field, dialect).is_some() {
                return None;
            }
            if i > 0 {
                out.push(delimiter);
            }
            push_field(&mut out, field, dialect);
        }
        out.push('\n');
    }
    Some(out)
}
