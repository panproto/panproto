//! Complete finite scopes for the edge-lists page.

use crate::pages::edge_lists::{Dialect, Edge, Graph};

/// The CSV header spellings of the document scope.
pub const CSV_HEADERS: &[&str] = &[
    "source,target",
    "Target,SOURCE",
    "source,target,label",
    "label,Source,target",
    "source",
    "source,target,weight",
    "source,target,Source",
    "source,,target",
    "\"source\",\"tar,get\"",
    "\"sou\"rce,target",
];

/// The CSV record spellings of the document scope.
pub const CSV_RECORDS: &[&str] = &[
    "a,b",
    "a,a",
    "b,a,x",
    "a,b,",
    "\"a,1\",\"b\"\"c\"",
    ",b",
    "a,\"\"",
    "",
    "a\"b,c",
    "\"a\"b,c",
    "\"ab,c",
    "a,b,c,d",
    " a , b ",
    "\"x\ny\",\"\"",
    "x\"a,b\"y,c",
    "1\"2\"3,b",
];

/// The TSV header spellings of the document scope.
pub const TSV_HEADERS: &[&str] = &[
    "source\ttarget",
    "Target\tSOURCE",
    "source\ttarget\tlabel",
    "label\tSource\ttarget",
    "source",
    "source\ttarget\tweight",
    "source\ttarget\tSource",
    "source\t\ttarget",
    "\"source\"\ttarget",
];

/// The TSV record spellings of the document scope.
pub const TSV_RECORDS: &[&str] = &[
    "a\tb",
    "a\ta",
    "b\ta\tx",
    "a\tb\t",
    "\"a\"\t\"b",
    "\tb",
    "a\t",
    "",
    "a\tb\tc\td",
    " a \t b ",
    "a",
];

/// The line endings of the document scope: for CSV, line feed and
/// carriage return with line feed; for TSV, a carriage return alone too.
#[must_use]
pub const fn line_endings(dialect: Dialect) -> &'static [&'static str] {
    match dialect {
        Dialect::Csv => &["\n", "\r\n"],
        Dialect::Tsv => &["\n", "\r\n", "\r"],
    }
}

/// The prefixes a document of the scope starts with: none, a byte-order
/// mark, and a blank first line.
pub const PREFIXES: &[&str] = &["", "\u{FEFF}", "\n"];

/// Every document of the scope in `dialect`.
///
/// For each prefix, header and line ending, the header alone and followed
/// by each record and each ordered pair of records, each line ended by the
/// line ending, and each such document also without its final line
/// ending; and the empty document.
#[must_use]
pub fn documents(dialect: Dialect) -> Vec<String> {
    let (headers, records) = match dialect {
        Dialect::Csv => (CSV_HEADERS, CSV_RECORDS),
        Dialect::Tsv => (TSV_HEADERS, TSV_RECORDS),
    };
    let mut bodies: Vec<Vec<&str>> = vec![Vec::new()];
    for first in records {
        bodies.push(vec![first]);
        for second in records {
            bodies.push(vec![first, second]);
        }
    }
    let mut out = vec![String::new()];
    for prefix in PREFIXES {
        for header in headers {
            for ending in line_endings(dialect) {
                for body in &bodies {
                    let mut text = format!("{prefix}{header}{ending}");
                    for record in body {
                        text.push_str(record);
                        text.push_str(ending);
                    }
                    let trimmed = text[..text.len() - ending.len()].to_owned();
                    out.push(text);
                    out.push(trimmed);
                }
            }
        }
    }
    out
}

/// The number of documents of [`documents`] in `dialect`.
#[must_use]
pub const fn documents_count(dialect: Dialect) -> usize {
    let (headers, records) = match dialect {
        Dialect::Csv => (CSV_HEADERS.len(), CSV_RECORDS.len()),
        Dialect::Tsv => (TSV_HEADERS.len(), TSV_RECORDS.len()),
    };
    let bodies = 1 + records + records * records;
    1 + PREFIXES.len() * headers * line_endings(dialect).len() * bodies * 2
}

/// The edges every edge list of the write scope draws on: unlabelled,
/// labelled, a self-loop, parallel edges, names and labels the canonical
/// form quotes, and fields holding a tab or a line break.
#[must_use]
pub fn edge_pool() -> Vec<Edge> {
    let edge = |s: &str, t: &str, l: Option<&str>| Edge(s.into(), t.into(), l.map(Into::into));
    vec![
        edge("a", "b", None),
        edge("a", "a", None),
        edge("x,y", "a", None),
        edge("a", "b", Some("l")),
        edge("b", "a", Some("")),
        edge("a", "a", Some("q\"r")),
        edge("t\tu", "a", None),
        edge("a", "b", Some("m\nn")),
    ]
}

/// Every list of at most two edges of [`edge_pool`], in order.
#[must_use]
pub fn edge_lists() -> Vec<Vec<Edge>> {
    let pool = edge_pool();
    let mut out = vec![Vec::new()];
    for first in &pool {
        out.push(vec![first.clone()]);
        for second in &pool {
            out.push(vec![first.clone(), second.clone()]);
        }
    }
    out
}

fn next_id(graph: &Graph) -> u32 {
    graph.nodes.keys().next_back().map_or(0, |id| id + 1)
}

/// `graph` and its single-fault variants.
///
/// The variants are each node duplicated under a new
/// id (an `edge` node with copies of its out-edges), an isolated node and a
/// node of an unknown anchor added, each edge dropped, each `node` node's
/// value removed and emptied, each `edge` node's label removed or added, and
/// a `source` edge added from each `node` node to itself.
#[must_use]
pub fn mutations(graph: &Graph) -> Vec<Graph> {
    let mut out = vec![graph.clone()];
    for (&id, node) in &graph.nodes {
        let mut g = graph.clone();
        let copy = next_id(&g);
        g.nodes.insert(copy, node.clone());
        let copied: Vec<_> = graph
            .edges
            .iter()
            .filter(|(src, _, _)| *src == id)
            .map(|(_, tgt, name)| (copy, *tgt, name.clone()))
            .collect();
        g.edges.extend(copied);
        out.push(g);
    }
    for added in [("node", Some("z")), ("vertex", Some("v"))] {
        let mut g = graph.clone();
        g.nodes
            .insert(next_id(graph), (added.0.into(), added.1.map(Into::into)));
        out.push(g);
    }
    for index in 0..graph.edges.len() {
        let mut g = graph.clone();
        g.edges.remove(index);
        out.push(g);
    }
    for (&id, (anchor, value)) in &graph.nodes {
        let faults: Vec<Option<String>> = if anchor == "node" {
            vec![None, Some(String::new())]
        } else if value.is_some() {
            vec![None]
        } else {
            vec![Some("w".into())]
        };
        for fault in faults {
            let mut g = graph.clone();
            g.nodes.insert(id, (anchor.clone(), fault));
            out.push(g);
        }
        if anchor == "node" {
            let mut g = graph.clone();
            g.edges.push((id, id, "source".into()));
            out.push(g);
        }
    }
    out
}

/// The number of graphs [`mutations`] gives for `graph`.
#[must_use]
pub fn mutations_count(graph: &Graph) -> usize {
    let nodes = graph
        .nodes
        .values()
        .filter(|(anchor, _)| anchor == "node")
        .count();
    let edges = graph.nodes.len() - nodes;
    1 + graph.nodes.len() + 2 + graph.edges.len() + 3 * nodes + edges
}
