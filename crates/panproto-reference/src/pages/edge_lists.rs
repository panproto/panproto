//! Complete reference operations assigned to `edge-lists`.
//!
//! An edge list is read from its characters: RFC 4180 records for CSV and
//! tab-separated lines for TSV, transcribed from the two standards with the
//! choices the `edge-lists` page states, then the header rules and the
//! record rules of the page. It shares no code with production, which reads
//! the text through the tree-sitter CSV and TSV grammars and the generic
//! text routes' table extraction.

use std::collections::{BTreeMap, BTreeSet};

/// The delimited text an edge list is written in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Dialect {
    /// RFC 4180 CSV.
    Csv,
    /// IANA tab-separated values.
    Tsv,
}

/// One directed edge: source name, target name, label.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Edge(pub String, pub String, pub Option<String>);

/// Why a document is refused, in the order the page states.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReadRefusal {
    /// The text is not a CSV or TSV text of the route: a quoting fault, a
    /// bare carriage return in CSV, a document whose first line is blank,
    /// or a record whose field count differs from the header's.
    Table,
    /// The document holds no record.
    NoHeader,
    /// A header field, by position, names no supported column.
    UnsupportedColumn(usize, String),
    /// A header field, by position, names a column an earlier one names.
    DuplicateColumn(usize, String),
    /// No `source` (or no `target`) column.
    MissingColumn(String),
    /// A data record, counting from 1, with an empty `source` or `target`.
    EmptyName(usize, String),
}

/// The records of a CSV text by RFC 4180: fields separated by `,`, records
/// ended by a line feed or a carriage return and line feed, the last record
/// ended by the end of the text when it has no line ending; a field is
/// either unquoted (no `,`, `"`, carriage return or line feed) or quoted
/// (between `"`, holding anything, a `"` written `""`). `None` for a text
/// that is not such a sequence of records.
fn csv_records(text: &str) -> Option<Vec<Vec<String>>> {
    let chars: Vec<char> = text.chars().collect();
    let mut records = Vec::new();
    let mut at = 0;
    while at < chars.len() {
        let mut record = Vec::new();
        loop {
            let mut field = String::new();
            if chars.get(at) == Some(&'"') {
                at += 1;
                loop {
                    match chars.get(at) {
                        None => return None,
                        Some('"') if chars.get(at + 1) == Some(&'"') => {
                            field.push('"');
                            at += 2;
                        }
                        Some('"') => {
                            at += 1;
                            break;
                        }
                        Some(&c) => {
                            field.push(c);
                            at += 1;
                        }
                    }
                }
            } else {
                while let Some(&c) = chars.get(at) {
                    if matches!(c, ',' | '\n' | '\r') {
                        break;
                    }
                    if c == '"' {
                        return None;
                    }
                    field.push(c);
                    at += 1;
                }
            }
            record.push(field);
            match chars.get(at) {
                Some(',') => at += 1,
                Some('\n') => {
                    at += 1;
                    break;
                }
                Some('\r') if chars.get(at + 1) == Some(&'\n') => {
                    at += 2;
                    break;
                }
                None => break,
                Some(_) => return None,
            }
        }
        records.push(record);
    }
    Some(records)
}

/// The lines of a TSV text, each split at its tabs: a line ends at a line
/// feed, a carriage return, or a carriage return and line feed, and the
/// last line may end at the end of the text.
fn tsv_records(text: &str) -> Vec<Vec<String>> {
    let mut lines = Vec::new();
    let mut line = String::new();
    let mut chars = text.chars().peekable();
    let mut open = false;
    while let Some(c) = chars.next() {
        match c {
            '\n' | '\r' => {
                if c == '\r' && chars.peek() == Some(&'\n') {
                    chars.next();
                }
                lines.push(std::mem::take(&mut line));
                open = false;
            }
            _ => {
                line.push(c);
                open = true;
            }
        }
    }
    if open {
        lines.push(line);
    }
    lines
        .into_iter()
        .map(|line| line.split('\t').map(str::to_owned).collect())
        .collect()
}

/// The page's reading of an edge-list document in `dialect`.
///
/// A leading U+FEFF is skipped. A text whose first character is then a
/// line break is refused (`Table`); an empty text has no header. The text
/// is split into records ([`ReadRefusal::Table`] when it is not a CSV
/// text); the first record is the header, checked field by field (an
/// ASCII-case-insensitive `source`, `target` or `label`, each at most
/// once), then for a `source` and a `target` column. A blank line after the
/// header (a CSV record of one empty unquoted field, an empty TSV line) is
/// not a record; every other record must have the header's field count.
/// Then each record in order must have a non-empty `source`, then `target`
/// field, and is one edge, with its `label` field when the header has one.
///
/// # Errors
///
/// The first [`ReadRefusal`] in that order.
pub fn read_document(text: &str, dialect: Dialect) -> Result<Vec<Edge>, ReadRefusal> {
    let text = text.strip_prefix('\u{FEFF}').unwrap_or(text);
    if text.is_empty() {
        return Err(ReadRefusal::NoHeader);
    }
    if text.starts_with(['\n', '\r']) {
        return Err(ReadRefusal::Table);
    }
    let blank_lines: Vec<bool>;
    let records = match dialect {
        Dialect::Csv => {
            blank_lines = csv_blank_records(text);
            csv_records(text).ok_or(ReadRefusal::Table)?
        }
        Dialect::Tsv => {
            let records = tsv_records(text);
            blank_lines = records
                .iter()
                .map(|r| r.len() == 1 && r[0].is_empty())
                .collect();
            records
        }
    };
    let header = &records[0];
    let names = ["source", "target", "label"];
    let mut slots: [Option<usize>; 3] = [None; 3];
    for (column, field) in header.iter().enumerate() {
        let Some(slot) = names.iter().position(|n| n.eq_ignore_ascii_case(field)) else {
            return Err(ReadRefusal::UnsupportedColumn(column, field.clone()));
        };
        if slots[slot].is_some() {
            return Err(ReadRefusal::DuplicateColumn(column, field.clone()));
        }
        slots[slot] = Some(column);
    }
    let source = slots[0].ok_or_else(|| ReadRefusal::MissingColumn("source".into()))?;
    let target = slots[1].ok_or_else(|| ReadRefusal::MissingColumn("target".into()))?;
    let data: Vec<&Vec<String>> = records
        .iter()
        .zip(&blank_lines)
        .skip(1)
        .filter(|(_, blank)| !**blank)
        .map(|(record, _)| record)
        .collect();
    if data.iter().any(|record| record.len() != header.len()) {
        return Err(ReadRefusal::Table);
    }
    let mut edges = Vec::new();
    for (index, record) in data.iter().enumerate() {
        for (name, column) in [("source", source), ("target", target)] {
            if record[column].is_empty() {
                return Err(ReadRefusal::EmptyName(index + 1, name.into()));
            }
        }
        edges.push(Edge(
            record[source].clone(),
            record[target].clone(),
            slots[2].map(|column| record[column].clone()),
        ));
    }
    Ok(edges)
}

/// For each CSV record of `text` (as [`csv_records`] splits it, when it
/// does), whether it is a blank line: a record with no character before
/// its line ending.
fn csv_blank_records(text: &str) -> Vec<bool> {
    let chars: Vec<char> = text.chars().collect();
    let mut blanks = Vec::new();
    let mut at = 0;
    let mut start = 0;
    let mut quoted = false;
    while at < chars.len() {
        let c = chars[at];
        if c == '"' {
            quoted = !quoted;
        } else if !quoted && (c == '\n' || (c == '\r' && chars.get(at + 1) == Some(&'\n'))) {
            blanks.push(at == start);
            at += if c == '\r' { 2 } else { 1 };
            start = at;
            continue;
        }
        at += 1;
    }
    if start < chars.len() {
        blanks.push(false);
    }
    blanks
}

// ── The graph encoding ─────────────────────────────────────────────────

/// A graph in the edge-list graph encoding: nodes by id with their anchor
/// and value, and edges in id order as source, target and name.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Graph {
    /// Each node's anchor and string value.
    pub nodes: BTreeMap<u32, (String, Option<String>)>,
    /// Each edge as source, target and name, in id order.
    pub edges: Vec<(u32, u32, String)>,
}

/// The page's encoding of a list of edges: for each edge in order an
/// `edge` node holding its label, then the `node` node of its source and of
/// its target when the name has none yet, each new node taking the next id
/// from 0; the edge node's `source` edge, then its `target` edge.
#[must_use]
pub fn encode(edges: &[Edge]) -> Graph {
    let mut graph = Graph::default();
    let mut names: BTreeMap<String, u32> = BTreeMap::new();
    let mut next = 0_u32;
    for Edge(source, target, label) in edges {
        let id = next;
        next += 1;
        graph.nodes.insert(id, ("edge".into(), label.clone()));
        for (name, end) in [("source", source), ("target", target)] {
            let node = if let Some(&node) = names.get(end) {
                node
            } else {
                let node = next;
                next += 1;
                graph.nodes.insert(node, ("node".into(), Some(end.clone())));
                names.insert(end.clone(), node);
                node
            };
            graph.edges.push((id, node, name.into()));
        }
    }
    graph
}

// ── Canonical writing ──────────────────────────────────────────────────

/// Why a graph has no edge list, at the node named beside it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WriteRefusal {
    /// The anchor is neither `node` nor `edge`.
    UnknownAnchor,
    /// A `node` node without a value.
    Value,
    /// An edge other than `source` or `target` from an `edge` node to a
    /// `node` node.
    UnexpectedEdge,
    /// An `edge` node without exactly one out-edge of the name.
    EdgeCount(String),
    /// A `node` node whose name is empty.
    EmptyName,
    /// A character the dialect cannot write in a field.
    UnwritableCharacter(char),
    /// An earlier `node` node holds the same name.
    DuplicateNode(u32),
    /// The `edge` node disagrees with the first on having a label.
    MixedLabels(u32),
    /// No edge reaches the `node` node.
    Isolated,
}

/// The canonical field text of `field` in `dialect`.
fn field_text(field: &str, dialect: Dialect) -> String {
    if dialect == Dialect::Csv
        && (field.is_empty() || field.chars().any(|c| matches!(c, ',' | '"' | '\r' | '\n')))
    {
        format!("\"{}\"", field.replace('"', "\"\""))
    } else {
        field.to_owned()
    }
}

/// The first character of `text` that `dialect` cannot write in a field.
fn unwritable(text: &str, dialect: Dialect) -> Option<char> {
    match dialect {
        Dialect::Csv => None,
        Dialect::Tsv => text.chars().find(|c| matches!(c, '\t' | '\n' | '\r')),
    }
}

/// The page's canonical edge list of `edges`: the header `source`,
/// `target` and, when the first edge has a label, `label`; each edge one
/// record in order; fields joined by the delimiter, every record ended by
/// a line feed, a CSV field quoted when empty or holding `,`, `"`, a
/// carriage return or a line feed. `None` when the edges disagree on
/// having a label, a name is empty, or a TSV field holds a tab or a line
/// break.
#[must_use]
pub fn canonical_text(edges: &[Edge], dialect: Dialect) -> Option<String> {
    let delimiter = match dialect {
        Dialect::Csv => ",",
        Dialect::Tsv => "\t",
    };
    let labelled = edges.first().is_some_and(|edge| edge.2.is_some());
    let header: &[&str] = if labelled {
        &["source", "target", "label"]
    } else {
        &["source", "target"]
    };
    let mut lines = vec![header.join(delimiter)];
    for Edge(source, target, label) in edges {
        if label.is_some() != labelled || source.is_empty() || target.is_empty() {
            return None;
        }
        let mut fields = vec![source.as_str(), target.as_str()];
        fields.extend(label.as_deref());
        if fields.iter().any(|f| unwritable(f, dialect).is_some()) {
            return None;
        }
        let texts: Vec<String> = fields.iter().map(|f| field_text(f, dialect)).collect();
        lines.push(texts.join(delimiter));
    }
    Some(lines.iter().map(|line| format!("{line}\n")).collect())
}

/// The page's writer: checks each node in id order (its anchor; a `node`
/// node's value present and not empty; a character the dialect cannot
/// write; each out-edge in edge order, which must be a `source` or
/// `target` edge from an `edge` node to a `node` node; an `edge` node's
/// one `source` and one `target`), then in id order each `node` node whose
/// name an earlier one holds, then each `edge` node that disagrees with the
/// first on having a label, then each `node` node no edge reaches, and
/// otherwise writes the canonical text of the edges in `edge` node order.
///
/// # Errors
///
/// The first refusal, with the node at fault.
pub fn write_graph(graph: &Graph, dialect: Dialect) -> Result<String, (u32, WriteRefusal)> {
    let mut ends: BTreeMap<u32, (u32, u32)> = BTreeMap::new();
    for (&id, (anchor, value)) in &graph.nodes {
        if anchor != "node" && anchor != "edge" {
            return Err((id, WriteRefusal::UnknownAnchor));
        }
        if anchor == "node" {
            match value.as_deref() {
                None => return Err((id, WriteRefusal::Value)),
                Some("") => return Err((id, WriteRefusal::EmptyName)),
                Some(_) => {}
            }
        }
        if let Some(c) = value.as_deref().and_then(|v| unwritable(v, dialect)) {
            return Err((id, WriteRefusal::UnwritableCharacter(c)));
        }
        let mut found: BTreeMap<&str, Vec<u32>> = BTreeMap::new();
        for (src, tgt, name) in &graph.edges {
            if *src != id {
                continue;
            }
            let fits = anchor == "edge"
                && graph.nodes[tgt].0 == "node"
                && (name == "source" || name == "target");
            if !fits {
                return Err((id, WriteRefusal::UnexpectedEdge));
            }
            found.entry(name).or_default().push(*tgt);
        }
        if anchor == "edge" {
            for name in ["source", "target"] {
                if found.get(name).map_or(0, Vec::len) != 1 {
                    return Err((id, WriteRefusal::EdgeCount(name.into())));
                }
            }
            ends.insert(id, (found["source"][0], found["target"][0]));
        }
    }
    let mut names: BTreeMap<&str, u32> = BTreeMap::new();
    for (&id, (anchor, value)) in &graph.nodes {
        if anchor == "node" {
            let name = value.as_deref().unwrap_or_default();
            if let Some(&other) = names.get(name) {
                return Err((id, WriteRefusal::DuplicateNode(other)));
            }
            names.insert(name, id);
        }
    }
    if let Some(&first) = ends.keys().next() {
        let labelled = graph.nodes[&first].1.is_some();
        if let Some(&id) = ends
            .keys()
            .find(|id| graph.nodes[*id].1.is_some() != labelled)
        {
            return Err((id, WriteRefusal::MixedLabels(first)));
        }
    }
    let used: BTreeSet<u32> = ends.values().flat_map(|(s, t)| [*s, *t]).collect();
    if let Some((&id, _)) = graph
        .nodes
        .iter()
        .find(|(id, (anchor, _))| anchor == "node" && !used.contains(id))
    {
        return Err((id, WriteRefusal::Isolated));
    }
    let value = |id: u32| graph.nodes[&id].1.clone().unwrap_or_default();
    let edges: Vec<Edge> = ends
        .iter()
        .map(|(&id, &(s, t))| Edge(value(s), value(t), graph.nodes[&id].1.clone()))
        .collect();
    Ok(canonical_text(&edges, dialect).unwrap_or_default())
}
