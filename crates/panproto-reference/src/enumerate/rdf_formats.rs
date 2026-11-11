//! Complete finite scopes for the rdf-formats page.

use crate::pages::rdf_formats::ntriples::{Graph, RDF_LANG_STRING, Term, Triple, XSD_STRING};

/// The subject spellings of the document scope.
pub const SUBJECTS: &[&str] = &["<http://e/s>", "_:b0", "<s>", ":s", "_:b0."];

/// The predicate spellings of the document scope.
pub const PREDICATES: &[&str] = &["<http://e/p>", "a", "<p>"];

/// The object spellings of the document scope.
pub const OBJECTS: &[&str] = &[
    "<http://e/o>",
    "_:b0",
    "_:b1",
    "\"x\"",
    "\"\"",
    "\"a\\\"b\"",
    "\"\\u00E9\"",
    "\"\\U0001F600\"",
    "\"\\uD800\"",
    "\"\\z\"",
    "'x'",
    "\"\"\"x\"\"\"",
    "\"x\"@en-US",
    "\"x\" @en",
    "\"x\"@1",
    "\"x\"^^<http://e/d>",
    "\"x\" ^^ <http://e/d>",
    "\"x\"^^<d>",
    "\"x\"^^<http://www.w3.org/2001/XMLSchema#string>",
    "1",
    "true",
    "<http://e/\\u0041>",
    "<http://e/\\u0020>",
    "<http://e/ x>",
    "[]",
    "( )",
];

/// The endings of a one-line document's triple.
pub const ENDINGS: &[&str] = &[" .", ".", " ;.", ""];

/// The lines of the two-line documents.
pub const LINES: &[&str] = &[
    "<http://e/s> <http://e/p> <http://e/o> .",
    "_:b0 <http://e/p> \"x\"@en .",
    "<http://e/s> <http://e/p> <http://e/o> .# c",
    "# a comment",
    "",
    "<http://e/s> <http://e/p> \"a\" .",
];

/// The separators of the two-line documents.
pub const SEPARATORS: &[&str] = &["\n", "\r\n", " ", "\n\n"];

/// Every document of the N-Triples scope.
///
/// Each subject, predicate, object and ending is one line ended by a line
/// feed, and each ordered pair of [`LINES`] is joined by each separator and
/// ended by a line feed.
#[must_use]
pub fn ntriples_documents() -> Vec<String> {
    let mut documents = Vec::new();
    for s in SUBJECTS {
        for p in PREDICATES {
            for o in OBJECTS {
                for ending in ENDINGS {
                    documents.push(format!("{s} {p} {o}{ending}\n"));
                }
            }
        }
    }
    for first in LINES {
        for second in LINES {
            for separator in SEPARATORS {
                documents.push(format!("{first}{separator}{second}\n"));
            }
        }
    }
    documents
}

/// The count of [`ntriples_documents`]: `5 * 3 * 26 * 4 + 6 * 6 * 4`.
#[must_use]
pub const fn ntriples_documents_count() -> usize {
    SUBJECTS.len() * PREDICATES.len() * OBJECTS.len() * ENDINGS.len()
        + LINES.len() * LINES.len() * SEPARATORS.len()
}

/// The triples of the graph scope.
///
/// Two subjects, two predicates (one of them also a subject) and six
/// objects, among them every literal form and a lexical form holding
/// characters the canonical form escapes.
#[must_use]
pub fn triples() -> Vec<Triple> {
    let a = "http://e/a".to_owned();
    let subjects = [Term::Iri(a.clone()), Term::Blank("x".into())];
    let predicates = ["http://e/p".to_owned(), a.clone()];
    let objects = [
        Term::Iri(a),
        Term::Blank("x".into()),
        Term::Literal(
            "01".into(),
            "http://www.w3.org/2001/XMLSchema#integer".into(),
            None,
        ),
        Term::Literal("v".into(), XSD_STRING.into(), None),
        Term::Literal("v".into(), RDF_LANG_STRING.into(), Some("en".into())),
        Term::Literal("a\n\u{1}\"\\\u{FFFF}\u{E9}".into(), XSD_STRING.into(), None),
    ];
    let mut out = Vec::new();
    for s in &subjects {
        for p in &predicates {
            for o in &objects {
                out.push(Triple(s.clone(), p.clone(), o.clone()));
            }
        }
    }
    out
}

/// Every list of at most two [`triples`], repeats included.
#[must_use]
pub fn triple_lists() -> Vec<Vec<Triple>> {
    super::sequences(&triples(), 2)
}

/// The count of [`triple_lists`]: `1 + 24 + 24^2`.
#[must_use]
pub fn triple_lists_count() -> u64 {
    super::sequences_count(24, 2)
}

fn next_id(graph: &Graph) -> u32 {
    graph.nodes.keys().max().map_or(0, |id| id + 1)
}

/// The single-fault variants of a graph.
///
/// Each node duplicated (a triple with copies of its out-edges), an unused
/// IRI added, each edge dropped, each node's value replaced by each value of
/// [`bad_values`] for its anchor, and an edge `datatype` added from each
/// triple to its predicate.
#[must_use]
pub fn mutations(graph: &Graph) -> Vec<Graph> {
    let mut out = Vec::new();
    for (&id, node) in &graph.nodes {
        let mut g = graph.clone();
        let copy = next_id(&g);
        g.nodes.insert(copy, node.clone());
        if node.0 == "triple" {
            let copied: Vec<_> = graph
                .edges
                .iter()
                .filter(|(src, _, _)| *src == id)
                .map(|(_, tgt, name)| (copy, *tgt, name.clone()))
                .collect();
            g.edges.extend(copied);
        }
        out.push(g);
    }
    let mut unused = graph.clone();
    unused
        .nodes
        .insert(next_id(graph), ("iri".into(), Some("http://e/u".into())));
    out.push(unused);
    for index in 0..graph.edges.len() {
        let mut g = graph.clone();
        g.edges.remove(index);
        out.push(g);
    }
    for (&id, node) in &graph.nodes {
        for value in bad_values(&node.0) {
            let mut g = graph.clone();
            g.nodes.insert(id, (node.0.clone(), value));
            out.push(g);
        }
    }
    for (src, tgt, name) in &graph.edges {
        if name == "predicate" {
            let mut g = graph.clone();
            g.edges.push((*src, *tgt, "datatype".into()));
            out.push(g);
        }
    }
    out
}

/// The faulty values tried for a node of `anchor`.
#[must_use]
pub fn bad_values(anchor: &str) -> Vec<Option<String>> {
    match anchor {
        "triple" => vec![Some("v".into())],
        "iri" => vec![
            None,
            Some("e/rel".into()),
            Some("http://e/ x".into()),
            Some(RDF_LANG_STRING.into()),
        ],
        "blank" => vec![None, Some("b.".into()), Some("-b".into())],
        "language" => vec![None, Some("1".into()), Some("en-".into())],
        _ => vec![None],
    }
}

/// The count of [`mutations`] of `graph`, from its node, edge and
/// predicate-edge counts and the size of [`bad_values`] per anchor.
#[must_use]
pub fn mutations_count(graph: &Graph) -> usize {
    let predicates = graph
        .edges
        .iter()
        .filter(|(_, _, name)| name == "predicate")
        .count();
    graph.nodes.len()
        + 1
        + graph.edges.len()
        + graph
            .nodes
            .values()
            .map(|(anchor, _)| bad_values(anchor).len())
            .sum::<usize>()
        + predicates
}
