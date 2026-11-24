//! Turtle (W3C Recommendation RDF 1.1 Turtle, 25 February 2014).
//!
//! # Reading
//!
//! The reader parses the document with the tree-sitter Turtle grammar and
//! walks the tree in document order. Directives update the base IRI and the
//! prefix table from the point they appear; a relative IRI reference is
//! resolved against the base IRI in force by [`resolve_iri`] (RFC 3986 Sec
//! 5.2), and a prefixed name is its prefix's IRI followed by its local
//! name with the local name's `\` escapes removed (a `%` escape is kept as
//! written). A document read with no base IRI refuses every relative
//! reference with `RelativeIri`.
//!
//! A blank node written with a label keeps the label. Each blank node the
//! document writes without one (`[]`, a blank node property list, and each
//! cell of a collection) gets a fresh label: `b` followed by a decimal
//! counter starting at 0 and incremented on every allocation, skipping any
//! label the document writes, so no fresh label names a labelled node.
//! Labels are allocated in document order: a blank node property list's at
//! its `[`, and a collection's cells all at its `(`, first to last.
//!
//! Triples are listed in the order the walk completes them: an object's
//! own triples (those of a blank node property list or a collection)
//! before the triple that has it as object, a subject's own triples before
//! the subject's first predicate, and a collection's triples cell by cell,
//! each element's own triples then the cell's `rdf:first` and `rdf:rest`
//! triples. An integer, decimal, double or boolean keeps its token as its
//! lexical form, with the datatype `xsd:integer`, `xsd:decimal`,
//! `xsd:double` or `xsd:boolean`.
//!
//! Beyond the grammar, the reader refuses an undefined prefix
//! (`UndefinedPrefix`), a graph block of `TriG` (`NotInFormat`), an IRI whose
//! escapes decode to a character `IRIREF` excludes, and a numeric escape
//! that names a surrogate; and, as the N-Triples reader does, a raw U+0000
//! or U+007F, which the grammar cannot read inside a string.
//!
//! # Canonical output
//!
//! [`write`](fn@write) prints triples as canonical Turtle: no directives; the subjects
//! in ascending code point order of their N-Triples text, each written once
//! and followed by its predicates in ascending order, separated by ` ;`,
//! a line feed and four spaces; each predicate followed by its objects in
//! ascending order, separated by `, `; the subject's block ended by ` .`
//! and a line feed. Terms are written as canonical N-Triples writes them,
//! so canonical Turtle reads back as the same triples with or without a
//! base IRI.

use std::collections::BTreeMap;

use super::ntriples::push_term;
use super::{Term, Triple};

#[cfg(feature = "tree-sitter")]
pub use reader::{TermOccurrences, TokenKind, TokenOccurrence, TurtleCodec, TurtleComplement};

/// `rdf:type`, which the keyword `a` names.
pub const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
/// `rdf:first`.
pub const RDF_FIRST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#first";
/// `rdf:rest`.
pub const RDF_REST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest";
/// `rdf:nil`, the empty collection.
pub const RDF_NIL: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#nil";
/// `xsd:integer`, the datatype of an integer token.
pub const XSD_INTEGER: &str = "http://www.w3.org/2001/XMLSchema#integer";
/// `xsd:decimal`, the datatype of a decimal token.
pub const XSD_DECIMAL: &str = "http://www.w3.org/2001/XMLSchema#decimal";
/// `xsd:double`, the datatype of a double token.
pub const XSD_DOUBLE: &str = "http://www.w3.org/2001/XMLSchema#double";
/// `xsd:boolean`, the datatype of `true` and `false`.
pub const XSD_BOOLEAN: &str = "http://www.w3.org/2001/XMLSchema#boolean";

/// A base IRI that is not an absolute IRI.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("the base IRI {base:?} is not an absolute IRI")]
pub struct InvalidBaseIri {
    /// The base IRI given.
    pub base: String,
}

/// The five components of an IRI reference (RFC 3986 Sec 3), split as the
/// regular expression of RFC 3986 Appendix B splits them, except that a
/// scheme must have the form `scheme` of Sec 3.1 requires.
struct Components<'a> {
    scheme: Option<&'a str>,
    authority: Option<&'a str>,
    path: &'a str,
    query: Option<&'a str>,
    fragment: Option<&'a str>,
}

fn components(reference: &str) -> Components<'_> {
    let (rest, fragment) = match reference.split_once('#') {
        Some((rest, fragment)) => (rest, Some(fragment)),
        None => (reference, None),
    };
    let (rest, query) = match rest.split_once('?') {
        Some((rest, query)) => (rest, Some(query)),
        None => (rest, None),
    };
    // A scheme and its colon at the start; `?` and `#` are already split
    // off.
    let (scheme, rest) = rest
        .split_once(':')
        .filter(|_| super::is_absolute_iri(rest))
        .map_or((None, rest), |(scheme, after)| (Some(scheme), after));
    let (authority, path) = rest.strip_prefix("//").map_or((None, rest), |after| {
        let end = after.find('/').unwrap_or(after.len());
        (Some(&after[..end]), &after[end..])
    });
    Components {
        scheme,
        authority,
        path,
        query,
        fragment,
    }
}

/// `remove_dot_segments` of RFC 3986 Sec 5.2.4.
fn remove_dot_segments(path: &str) -> String {
    let mut input = path;
    let mut output = String::with_capacity(path.len());
    while !input.is_empty() {
        if let Some(rest) = input.strip_prefix("../") {
            input = rest;
        } else if let Some(rest) = input.strip_prefix("./") {
            input = rest;
        } else if input.starts_with("/./") {
            input = &input[2..];
        } else if input == "/." {
            input = "/";
        } else if input.starts_with("/../") || input == "/.." {
            input = if input.len() == 3 { "/" } else { &input[3..] };
            let cut = output.rfind('/').unwrap_or(0);
            output.truncate(cut);
        } else if input == "." || input == ".." {
            input = "";
        } else {
            let start = usize::from(input.starts_with('/'));
            let end = input[start..].find('/').map_or(input.len(), |i| i + start);
            output.push_str(&input[..end]);
            input = &input[end..];
        }
    }
    output
}

/// Resolve the IRI reference `reference` against the absolute IRI `base`.
///
/// Resolution is that of RFC 3986 Sec 5.2.2, with the merge of Sec 5.2.3,
/// the dot-segment removal of Sec 5.2.4 and the recomposition of Sec 5.3.
/// The strict parser is used: a reference with a scheme is never read as
/// relative.
#[must_use]
pub fn resolve_iri(base: &str, reference: &str) -> String {
    let r = components(reference);
    let b = components(base);
    let (scheme, authority, path, query): (Option<&str>, Option<&str>, String, Option<&str>) =
        if r.scheme.is_some() {
            (r.scheme, r.authority, remove_dot_segments(r.path), r.query)
        } else if r.authority.is_some() {
            (b.scheme, r.authority, remove_dot_segments(r.path), r.query)
        } else if r.path.is_empty() {
            (
                b.scheme,
                b.authority,
                b.path.to_owned(),
                r.query.or(b.query),
            )
        } else if r.path.starts_with('/') {
            (b.scheme, b.authority, remove_dot_segments(r.path), r.query)
        } else {
            let merged = if b.authority.is_some() && b.path.is_empty() {
                format!("/{}", r.path)
            } else {
                let keep = b.path.rfind('/').map_or(0, |i| i + 1);
                format!("{}{}", &b.path[..keep], r.path)
            };
            (b.scheme, b.authority, remove_dot_segments(&merged), r.query)
        };
    let mut out = String::with_capacity(base.len() + reference.len());
    if let Some(scheme) = scheme {
        out.push_str(scheme);
        out.push(':');
    }
    if let Some(authority) = authority {
        out.push_str("//");
        out.push_str(authority);
    }
    out.push_str(&path);
    if let Some(query) = query {
        out.push('?');
        out.push_str(query);
    }
    if let Some(fragment) = r.fragment {
        out.push('#');
        out.push_str(fragment);
    }
    out
}

/// The text of `term` as canonical N-Triples writes it.
fn term_text(term: &Term) -> String {
    let mut out = String::new();
    push_term(&mut out, term);
    out
}

/// The canonical Turtle document of `triples`, each written once.
#[must_use]
pub fn write(triples: &[Triple]) -> String {
    let mut subjects: BTreeMap<String, BTreeMap<String, Vec<String>>> = BTreeMap::new();
    for triple in triples {
        let objects = subjects
            .entry(term_text(&triple.subject))
            .or_default()
            .entry(format!("<{}>", triple.predicate))
            .or_default();
        let object = term_text(&triple.object);
        if !objects.contains(&object) {
            objects.push(object);
        }
    }
    let mut out = String::new();
    for (subject, predicates) in subjects {
        out.push_str(&subject);
        for (i, (predicate, mut objects)) in predicates.into_iter().enumerate() {
            out.push_str(if i == 0 { " " } else { " ;\n    " });
            out.push_str(&predicate);
            objects.sort_unstable();
            for (j, object) in objects.iter().enumerate() {
                out.push_str(if j == 0 { " " } else { ", " });
                out.push_str(object);
            }
        }
        out.push_str(" .\n");
    }
    out
}

/// Decode a `PN_LOCAL`: each `PN_LOCAL_ESC` (`\` and one of
/// `_~.-!$&'()*+,;=/?#@%`) is replaced by its character, and every other
/// character, a `%` escape included, is kept.
#[must_use]
pub fn decode_local(local: &str) -> String {
    let mut out = String::with_capacity(local.len());
    let mut chars = local.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(escaped) = chars.next() {
                out.push(escaped);
            }
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(feature = "tree-sitter")]
pub(super) mod reader;
