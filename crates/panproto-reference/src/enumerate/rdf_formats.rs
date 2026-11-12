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

// ── Turtle ─────────────────────────────────────────────────────────────

/// The base IRIs a Turtle document of the scope is read with.
pub const TURTLE_BASES: &[Option<&str>] = &[None, Some("http://b/d/f")];

/// The directive lines that open a Turtle document of the scope.
///
/// None; an `@prefix` for the empty prefix; a SPARQL `PREFIX` and an
/// `@prefix` whose IRI is relative; and an `@base` followed by a
/// lower-case SPARQL `prefix`.
pub const TURTLE_HEADERS: &[&str] = &[
    "",
    "@prefix : <http://e/> .\n",
    "PREFIX e: <http://e/>\n@prefix : <x#> .\n",
    "@base <http://c/a/> .\nprefix : <../e/>\n",
];

/// The subject spellings of the Turtle scope.
pub const TURTLE_SUBJECTS: &[&str] = &[
    "<http://e/s>",
    "<s>",
    ":s",
    "e:s",
    "_:b0",
    "[]",
    "[ :p \"v\" ]",
    "( 1 :o )",
    "\"s\"",
];

/// The predicate spellings of the Turtle scope.
pub const TURTLE_PREDICATES: &[&str] = &["a", "<http://e/p>", ":p", "e:p"];

/// The object spellings of the Turtle scope.
pub const TURTLE_OBJECTS: &[&str] = &[
    "<http://e/o>",
    "<../o>",
    ":o",
    ":\\-x.y",
    "_:x",
    "_:b1",
    "[]",
    "[ :q :r ; :t 'u' ]",
    "( )",
    "( 1 [] ( ) )",
    "\"x\"",
    "'y'",
    "\"\"\"a\"b\"\"\"",
    "\"x\"@en-GB",
    "\"x\" @en",
    "\"x\"^^<http://e/d>",
    "\"x\"^^:d",
    "\"\\u0041\\t\"",
    "\"\\ud800\"",
    "42",
    "-1.5",
    "+1e3",
    ".5E-1",
    "true",
    "<http://e/\\u0020>",
    "\"x\"@",
];

/// The endings of a Turtle statement of the scope.
pub const TURTLE_ENDINGS: &[&str] = &[" .", " ; :q :r .", " , :o .", ""];

/// The statement lines of the two-statement Turtle documents.
pub const TURTLE_LINES: &[&str] = &[
    "_:b0 :p [] .",
    ":s :p ( _:b1 ) .",
    "@prefix : <http://f/> .",
    "BASE <http://g/>",
    "<s> :p :o .",
    "[ :p :o ] .",
    "# c",
];

/// The separators of the two-statement Turtle documents.
pub const TURTLE_SEPARATORS: &[&str] = &["\n", " "];

/// Every document of the Turtle scope, with the base it is read with.
///
/// For each base and header, each subject, predicate, object and ending as
/// one statement on the line after the header; and, with the second base
/// and header, each ordered pair of [`TURTLE_LINES`] joined by each
/// separator and ended by a line feed.
#[must_use]
pub fn turtle_documents() -> Vec<(Option<&'static str>, String)> {
    let mut documents = Vec::new();
    for base in TURTLE_BASES {
        for header in TURTLE_HEADERS {
            for s in TURTLE_SUBJECTS {
                for p in TURTLE_PREDICATES {
                    for o in TURTLE_OBJECTS {
                        for ending in TURTLE_ENDINGS {
                            documents.push((*base, format!("{header}{s} {p} {o}{ending}\n")));
                        }
                    }
                }
            }
        }
    }
    for first in TURTLE_LINES {
        for second in TURTLE_LINES {
            for separator in TURTLE_SEPARATORS {
                documents.push((
                    TURTLE_BASES[1],
                    format!("{}{first}{separator}{second}\n", TURTLE_HEADERS[1]),
                ));
            }
        }
    }
    documents
}

/// The count of [`turtle_documents`]: `2 * 4 * 9 * 4 * 26 * 4 + 7 * 7 * 2`.
#[must_use]
pub const fn turtle_documents_count() -> usize {
    TURTLE_BASES.len()
        * TURTLE_HEADERS.len()
        * TURTLE_SUBJECTS.len()
        * TURTLE_PREDICATES.len()
        * TURTLE_OBJECTS.len()
        * TURTLE_ENDINGS.len()
        + TURTLE_LINES.len() * TURTLE_LINES.len() * TURTLE_SEPARATORS.len()
}

/// The documents of the format-preserving scope: those of
/// [`turtle_documents`] with the second base and header, one statement
/// each.
#[must_use]
pub fn preserving_documents() -> Vec<String> {
    let mut documents = Vec::new();
    for s in TURTLE_SUBJECTS {
        for p in TURTLE_PREDICATES {
            for o in TURTLE_OBJECTS {
                for ending in TURTLE_ENDINGS {
                    documents.push(format!("{}{s} {p} {o}{ending}\n", TURTLE_HEADERS[1]));
                }
            }
        }
    }
    documents
}

/// The new values tried for a term of each kind in the format-preserving
/// scope.
///
/// Each kind has a fresh value and one the route may refuse or rewrite
/// differently: an IRI already in many documents, a label Turtle cannot
/// write, a lexical form that is no bare token, a second tag.
pub const EDIT_VALUES: &[(&str, &[&str])] = &[
    ("iri", &["http://e/edited", "http://e/o"]),
    ("blank", &["edited", "x:y"]),
    ("literal", &["7", "a \"q\"\n"]),
    ("language", &["de-AT", "en"]),
];

/// The base IRIs of the resolution scope.
pub const RESOLUTION_BASES: &[&str] = &[
    "http://a/b/c/d;p?q",
    "http://a",
    "http://a/",
    "g:h",
    "file:///x/y",
];

/// The pieces a reference of the resolution scope is built from.
pub const RESOLUTION_PIECES: &[&str] = &["g", ".", "..", "/", "//h", "?y", "#s", ";x", "g:"];

/// Every pair of a base of [`RESOLUTION_BASES`] and a reference made of at
/// most three [`RESOLUTION_PIECES`].
#[must_use]
pub fn resolution_pairs() -> Vec<(&'static str, String)> {
    let references = super::sequences(RESOLUTION_PIECES, 3);
    let mut out = Vec::new();
    for base in RESOLUTION_BASES {
        for pieces in &references {
            out.push((*base, pieces.concat()));
        }
    }
    out
}

/// The count of [`resolution_pairs`]: `5 * (1 + 9 + 9^2 + 9^3)`.
#[must_use]
pub fn resolution_pairs_count() -> u64 {
    5 * super::sequences_count(9, 3)
}
