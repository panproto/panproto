//! RDF graph formats.
//!
//! Every RDF route reads its document into a list of [`Triple`]s and
//! [`to_graph`] turns the list into a [`GInstance`] in the RDF graph
//! encoding of [`panproto_protocols::rdf`]; [`from_graph`] reads the triples
//! back out of a graph, refusing any graph the encoding does not describe,
//! and each format's writer prints them.
//!
//! # Terms
//!
//! An IRI is held as the string it denotes, after its escapes are decoded.
//! A blank node is held as the label its document gives it: labels scope to
//! one document, so two documents that use one label denote two blank
//! nodes, and the graph of one document keeps the labels as presentation
//! that its writer prints back. A literal is held as its lexical form,
//! exactly as decoded and never canonicalised (`"01"^^xsd:integer` stays
//! `01`), with its datatype IRI and, for a language-tagged string, its
//! language tag exactly as written (no case folding). A literal written
//! with no datatype has the datatype `xsd:string`, as RDF 1.1 gives it.
//!
//! # Repeated triples
//!
//! An RDF graph is a set. A document that states one triple twice denotes
//! the graph that holds it once, and the reading holds one `triple` node for
//! it. A graph with two `triple` nodes for one triple, or two nodes for one
//! term, has no document that reads back as it, so [`from_graph`] refuses
//! it rather than writing a document that would merge them.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt;

use panproto_inst::{FieldPresence, GInstance, InstError, Node, NodeShape, Value};
use panproto_protocols::rdf::{
    BLANK, DATATYPE, EDGE_KIND, EDGES, IRI, LANGUAGE, LANGUAGE_TAG, LITERAL, OBJECT, PREDICATE,
    RDF_LANG_STRING, SUBJECT, TRIPLE,
};
use panproto_schema::{Edge, Schema};

#[cfg(feature = "tree-sitter")]
mod cst;
pub mod jsonld;
pub mod ntriples;
pub mod turtle;

/// Register the RDF graph codecs (`ntriples`, `turtle`, `jsonld`) with the
/// registry's graph table.
///
/// Each reads through a bundled tree-sitter grammar, so a codec whose
/// grammar is not compiled in is left out, and one whose grammar fails to
/// initialise is reported on stderr and left out.
#[cfg(feature = "tree-sitter")]
pub fn register_all(registry: &mut crate::ProtocolRegistry) {
    fn report(err: &crate::error::UnifiedCodecError) {
        if !matches!(err, crate::error::UnifiedCodecError::MissingGrammar { .. }) {
            eprintln!(
                "panproto-io: bundled grammar failed parser initialisation; \
                 skipping registration: {err}"
            );
        }
    }
    match ntriples::NTriplesCodec::new() {
        Ok(codec) => registry.register_graph(codec),
        Err(err) => report(&err),
    }
    match turtle::TurtleCodec::new() {
        Ok(codec) => registry.register_graph(codec),
        Err(err) => report(&err),
    }
    match jsonld::JsonLdCodec::new() {
        Ok(codec) => registry.register_graph(codec),
        Err(err) => report(&err),
    }
}

/// An RDF term.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Term {
    /// An IRI.
    Iri(String),
    /// A blank node, by the label its document gives it.
    Blank(String),
    /// A literal.
    Literal(Literal),
}

/// An RDF literal.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Literal {
    /// The lexical form.
    pub lexical: String,
    /// The datatype IRI: `rdf:langString` exactly when `language` is set.
    pub datatype: String,
    /// The language tag of a language-tagged string.
    pub language: Option<String>,
}

/// An RDF triple.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Triple {
    /// The subject: an IRI or a blank node.
    pub subject: Term,
    /// The predicate IRI.
    pub predicate: String,
    /// The object.
    pub object: Term,
}

/// Why an RDF document is refused.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum RdfReadErrorKind {
    /// The document is not UTF-8.
    NotUtf8,
    /// The grammar does not read the document.
    Syntax,
    /// A construct of a wider syntax (Turtle) that the format does not have.
    NotInFormat {
        /// The construct, as the grammar names it.
        construct: String,
    },
    /// A triple spans a line break, or two triples share a line.
    LineStructure,
    /// An IRI that is not absolute.
    RelativeIri,
    /// An IRI holding a character that no IRI may hold.
    IriCharacter {
        /// The character.
        character: char,
    },
    /// An escape sequence the format does not define.
    InvalidEscape,
    /// A numeric escape that names no Unicode scalar value.
    NotAScalarValue {
        /// The code point the escape names.
        code_point: u32,
    },
    /// A string holding a character the format requires to be escaped.
    UnescapedCharacter {
        /// The character.
        character: char,
    },
    /// A malformed language tag.
    InvalidLanguageTag,
    /// A malformed blank node label.
    InvalidBlankLabel,
    /// A raw U+0000 or U+007F, which the grammar cannot read in a string.
    UnsupportedCharacter {
        /// The character.
        character: char,
    },
    /// A prefixed name whose prefix no directive before it declares.
    UndefinedPrefix {
        /// The prefix, without its `:`.
        prefix: String,
    },
    /// A JSON-LD document the reader refuses.
    JsonLd {
        /// Why.
        refusal: jsonld::JsonLdRefusal,
        /// The JSON Pointer (RFC 6901) of the value at fault.
        pointer: String,
    },
    /// Arrays and objects nested deeper than the reader's limit.
    NestingLimit {
        /// The deepest nesting accepted.
        limit: usize,
    },
}

impl fmt::Display for RdfReadErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotUtf8 => f.write_str("the document is not UTF-8"),
            Self::Syntax => f.write_str("the grammar does not read the document here"),
            Self::NotInFormat { construct } => write!(f, "{construct} is not in the format"),
            Self::LineStructure => {
                f.write_str("each triple must be on one line, and no two on the same line")
            }
            Self::RelativeIri => f.write_str("the IRI is not absolute"),
            Self::IriCharacter { character } => {
                write!(
                    f,
                    "an IRI cannot hold the character U+{:04X}",
                    u32::from(*character)
                )
            }
            Self::InvalidEscape => f.write_str("the escape sequence is not defined"),
            Self::NotAScalarValue { code_point } => {
                write!(f, "U+{code_point:04X} is not a Unicode scalar value")
            }
            Self::UnescapedCharacter { character } => write!(
                f,
                "the string holds U+{:04X}, which must be escaped",
                u32::from(*character)
            ),
            Self::InvalidLanguageTag => f.write_str("the language tag is malformed"),
            Self::InvalidBlankLabel => f.write_str("the blank node label is malformed"),
            Self::UnsupportedCharacter { character } => write!(
                f,
                "the route cannot read the raw character U+{:04X}",
                u32::from(*character)
            ),
            Self::UndefinedPrefix { prefix } => write!(f, "the prefix {prefix:?} is not declared"),
            Self::JsonLd { refusal, pointer } => {
                write!(f, "at JSON Pointer {pointer:?}: {refusal}")
            }
            Self::NestingLimit { limit } => {
                write!(f, "arrays and objects nest deeper than {limit} levels")
            }
        }
    }
}

/// A refused RDF document: why, and the byte offset of the token at fault.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("at byte {offset}: {kind}")]
pub struct RdfReadError {
    /// The byte offset in the document.
    pub offset: usize,
    /// Why the document is refused.
    pub kind: RdfReadErrorKind,
}

impl RdfReadError {
    /// A refusal of `kind` at `offset`.
    #[must_use]
    pub const fn new(offset: usize, kind: RdfReadErrorKind) -> Self {
        Self { offset, kind }
    }
}

/// Why a graph has no document in an RDF format.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum RdfWriteErrorKind {
    /// The node is anchored at no vertex of the RDF graph encoding.
    UnknownAnchor {
        /// The anchor.
        anchor: String,
    },
    /// A term node holds no string, or a triple node holds a value.
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
    /// The node has other than exactly one out-edge `name` (at most one
    /// for a literal's `language`).
    EdgeCount {
        /// The edge's name.
        name: String,
        /// How many the node has.
        count: usize,
    },
    /// A literal with a language tag whose datatype is not
    /// `rdf:langString`, or one with that datatype and no tag.
    LanguageDatatype,
    /// Another node holds the same term.
    DuplicateTerm {
        /// The other node.
        other: u32,
    },
    /// Another triple node holds the same triple.
    DuplicateTriple {
        /// The other node.
        other: u32,
    },
    /// The node is a term of no triple, so no document can hold it.
    Unused,
    /// The IRI is not absolute.
    RelativeIri,
    /// The IRI holds a character no IRI may hold.
    IriCharacter {
        /// The character.
        character: char,
    },
    /// The label is not a blank node label.
    InvalidBlankLabel,
    /// The tag is not a language tag.
    InvalidLanguageTag,
    /// The edited graph differs from the graph a preserved document was
    /// read as in more than the values of its nodes.
    StructuralEdit {
        /// What differs.
        detail: String,
    },
    /// The node's value changed, and the preserved document writes its term
    /// with no token of its own in at least one place (a datatype the
    /// literal's form implies, `rdf:first`, `rdf:rest` and `rdf:nil` of a
    /// collection, or a blank node written without a label), so no token
    /// can be rewritten in place.
    ImplicitTerm,
    /// The node's new lexical form is not one that a JSON number or
    /// boolean in the place of the preserved token converts to.
    NativeLexical,
    /// The preserved document, with the edited tokens rewritten, reads back
    /// as another graph than the edited one.
    ReadBackDiffers,
    /// The IRI holds more than one `#`, which the JSON-LD reader refuses as
    /// RFC 3987 does, so no JSON-LD document reads back as it.
    IriFragments,
}

impl fmt::Display for RdfWriteErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownAnchor { anchor } => {
                write!(
                    f,
                    "anchor {anchor:?} is not a vertex of the RDF graph encoding"
                )
            }
            Self::Value => f.write_str("the node's value does not fit its anchor"),
            Self::Presentation => {
                f.write_str("the node carries data the RDF graph encoding has no place for")
            }
            Self::UnexpectedEdge { name } => {
                write!(
                    f,
                    "edge {name:?} is not an edge of the RDF graph encoding here"
                )
            }
            Self::EdgeCount { name, count } => write!(f, "the node has {count} {name:?} edges"),
            Self::LanguageDatatype => f.write_str(
                "a literal has a language tag exactly when its datatype is rdf:langString",
            ),
            Self::DuplicateTerm { other } => write!(f, "node {other} holds the same term"),
            Self::DuplicateTriple { other } => write!(f, "node {other} holds the same triple"),
            Self::Unused => f.write_str("the node is a term of no triple"),
            Self::RelativeIri => f.write_str("the IRI is not absolute"),
            Self::IriCharacter { character } => {
                write!(
                    f,
                    "an IRI cannot hold the character U+{:04X}",
                    u32::from(*character)
                )
            }
            Self::InvalidBlankLabel => f.write_str("the label is not a blank node label"),
            Self::InvalidLanguageTag => f.write_str("the tag is not a language tag"),
            Self::StructuralEdit { detail } => {
                write!(f, "a structural edit the preserved document cannot carry: {detail}")
            }
            Self::ImplicitTerm => f.write_str(
                "the document writes this term without a token of its own, so its value cannot change in place",
            ),
            Self::NativeLexical => f.write_str(
                "no JSON number or boolean in the token's place converts to the new lexical form",
            ),
            Self::IriFragments => f.write_str("the IRI holds more than one '#'"),
            Self::ReadBackDiffers => f.write_str(
                "the document with the edited tokens rewritten reads back as another graph",
            ),
        }
    }
}

/// A graph refused by an RDF writer: why, and the node at fault.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("node {node}: {kind}")]
pub struct RdfWriteError {
    /// The node at fault (the source node, for an edge).
    pub node: u32,
    /// Why the graph is refused.
    pub kind: RdfWriteErrorKind,
}

impl RdfWriteError {
    pub(crate) const fn new(node: u32, kind: RdfWriteErrorKind) -> Self {
        Self { node, kind }
    }
}

// ── Lexical checks shared by the readers and the writers ───────────────

/// Whether `c` may not appear in an IRI, raw or escaped: the characters
/// RDF 1.1 N-Triples and Turtle exclude from `IRIREF`, which RFC 3987 does
/// not allow in an IRI either.
#[must_use]
pub const fn is_iri_excluded(c: char) -> bool {
    matches!(
        c,
        '\u{0}'..='\u{20}' | '<' | '>' | '"' | '{' | '}' | '|' | '^' | '`' | '\\'
    )
}

/// Whether `iri` is absolute: it begins with a scheme and a colon
/// (RFC 3987 Sec 2.2, `scheme ":"`).
#[must_use]
pub fn is_absolute_iri(iri: &str) -> bool {
    let Some((scheme, _)) = iri.split_once(':') else {
        return false;
    };
    let mut chars = scheme.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
}

/// `PN_CHARS_BASE` of RDF 1.1 N-Triples and Turtle.
const fn is_pn_chars_base(c: char) -> bool {
    matches!(c,
        'A'..='Z' | 'a'..='z'
        | '\u{C0}'..='\u{D6}' | '\u{D8}'..='\u{F6}' | '\u{F8}'..='\u{2FF}'
        | '\u{370}'..='\u{37D}' | '\u{37F}'..='\u{1FFF}' | '\u{200C}'..='\u{200D}'
        | '\u{2070}'..='\u{218F}' | '\u{2C00}'..='\u{2FEF}' | '\u{3001}'..='\u{D7FF}'
        | '\u{F900}'..='\u{FDCF}' | '\u{FDF0}'..='\u{FFFD}' | '\u{10000}'..='\u{EFFFF}')
}

/// `PN_CHARS_U` of RDF 1.1 N-Triples and Turtle (with `:`, as N-Triples has it).
const fn is_pn_chars_u(c: char) -> bool {
    is_pn_chars_base(c) || c == '_' || c == ':'
}

/// `PN_CHARS` of RDF 1.1 N-Triples and Turtle.
const fn is_pn_chars(c: char) -> bool {
    is_pn_chars_u(c)
        || c == '-'
        || c.is_ascii_digit()
        || matches!(c, '\u{B7}' | '\u{300}'..='\u{36F}' | '\u{203F}'..='\u{2040}')
}

/// Whether `label` is the label part of an N-Triples `BLANK_NODE_LABEL`:
/// `(PN_CHARS_U | [0-9]) ((PN_CHARS | '.')* PN_CHARS)?`.
#[must_use]
pub fn is_blank_label(label: &str) -> bool {
    let mut chars = label.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (is_pn_chars_u(first) || first.is_ascii_digit())
        && label.chars().skip(1).all(|c| is_pn_chars(c) || c == '.')
        && !label.ends_with('.')
}

/// Whether `tag` is an N-Triples `LANGTAG` without its `@`:
/// `[a-zA-Z]+ ('-' [a-zA-Z0-9]+)*`.
#[must_use]
pub fn is_language_tag(tag: &str) -> bool {
    let mut parts = tag.split('-');
    parts
        .next()
        .is_some_and(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_alphabetic()))
        && parts.all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_alphanumeric()))
}

// ── Triples to graph ───────────────────────────────────────────────────

fn encoding_edge(src: &str, tgt: &str, name: &str) -> Edge {
    Edge {
        src: src.into(),
        tgt: tgt.into(),
        kind: EDGE_KIND.into(),
        name: Some(name.into()),
    }
}

fn string_node(id: u32, anchor: &str, value: &str) -> Node {
    Node::new(id, anchor).with_value(FieldPresence::Present(Value::Str(value.to_owned())))
}

/// Builds the graph of a list of triples, one node per distinct term and
/// one per distinct triple.
struct GraphBuilder {
    graph: GInstance,
    next: u32,
    iris: HashMap<String, u32>,
    blanks: HashMap<String, u32>,
    literals: HashMap<Literal, u32>,
    languages: HashMap<String, u32>,
    triples: HashMap<Triple, u32>,
}

impl GraphBuilder {
    fn new() -> Self {
        Self {
            graph: GInstance::new(),
            next: 0,
            iris: HashMap::new(),
            blanks: HashMap::new(),
            literals: HashMap::new(),
            languages: HashMap::new(),
            triples: HashMap::new(),
        }
    }

    fn insert(&mut self, node: Node) -> Result<u32, InstError> {
        let id = node.id;
        self.graph.insert_node(node)?;
        Ok(id)
    }

    fn fresh(&mut self) -> Result<u32, InstError> {
        let id = self.next;
        self.next = id.checked_add(1).ok_or(InstError::NodeIdsExhausted)?;
        Ok(id)
    }

    fn link(&mut self, src: u32, tgt: u32, edge: Edge) -> Result<(), InstError> {
        self.graph.add_edge(src, tgt, edge).map(|_| ())
    }

    fn table(&mut self, anchor: &str) -> &mut HashMap<String, u32> {
        match anchor {
            IRI => &mut self.iris,
            BLANK => &mut self.blanks,
            _ => &mut self.languages,
        }
    }

    fn keyed(&mut self, anchor: &'static str, value: &str) -> Result<u32, InstError> {
        if let Some(&id) = self.table(anchor).get(value) {
            return Ok(id);
        }
        let id = self.fresh()?;
        self.insert(string_node(id, anchor, value))?;
        self.table(anchor).insert(value.to_owned(), id);
        Ok(id)
    }

    fn term(&mut self, term: &Term) -> Result<(u32, &'static str), InstError> {
        match term {
            Term::Iri(iri) => Ok((self.keyed(IRI, iri)?, IRI)),
            Term::Blank(label) => Ok((self.keyed(BLANK, label)?, BLANK)),
            Term::Literal(literal) => {
                if let Some(&id) = self.literals.get(literal) {
                    return Ok((id, LITERAL));
                }
                let id = self.fresh()?;
                self.insert(string_node(id, LITERAL, &literal.lexical))?;
                self.literals.insert(literal.clone(), id);
                let datatype = self.keyed(IRI, &literal.datatype)?;
                self.link(id, datatype, encoding_edge(LITERAL, IRI, DATATYPE))?;
                if let Some(tag) = &literal.language {
                    let language = self.keyed(LANGUAGE, tag)?;
                    self.link(id, language, encoding_edge(LITERAL, LANGUAGE, LANGUAGE_TAG))?;
                }
                Ok((id, LITERAL))
            }
        }
    }

    fn triple(&mut self, triple: &Triple) -> Result<(), InstError> {
        if self.triples.contains_key(triple) {
            return Ok(());
        }
        let id = self.fresh()?;
        self.insert(Node::new(id, TRIPLE))?;
        self.triples.insert(triple.clone(), id);
        let (subject, subject_anchor) = self.term(&triple.subject)?;
        self.link(id, subject, encoding_edge(TRIPLE, subject_anchor, SUBJECT))?;
        let predicate = self.keyed(IRI, &triple.predicate)?;
        self.link(id, predicate, encoding_edge(TRIPLE, IRI, PREDICATE))?;
        let (object, object_anchor) = self.term(&triple.object)?;
        self.link(id, object, encoding_edge(TRIPLE, object_anchor, OBJECT))
    }
}

/// The graph of `triples` in the RDF graph encoding.
///
/// Nodes are numbered from 0 in the order the triples first mention them:
/// for each triple not seen before, its `triple` node, then its subject, its
/// predicate and its object, a new literal followed by its datatype and its
/// language tag. A term or triple seen before reuses its node, so a repeated
/// triple adds nothing. Edges are numbered in the order they are added.
///
/// # Errors
///
/// Returns [`InstError::NodeIdsExhausted`] or
/// [`InstError::EdgeIdsExhausted`] when the triples need more nodes or edges
/// than a graph can number.
pub fn to_graph<'a>(triples: impl IntoIterator<Item = &'a Triple>) -> Result<GInstance, InstError> {
    let mut builder = GraphBuilder::new();
    for triple in triples {
        builder.triple(triple)?;
    }
    Ok(builder.graph)
}

/// The node of each term of a graph [`to_graph_indexed`] built.
#[cfg(feature = "tree-sitter")]
pub(crate) struct TermIndex {
    pub(crate) iris: HashMap<String, u32>,
    pub(crate) blanks: HashMap<String, u32>,
    pub(crate) literals: HashMap<Literal, u32>,
    pub(crate) languages: HashMap<String, u32>,
}

/// [`to_graph`], with the node of each term.
#[cfg(feature = "tree-sitter")]
pub(crate) fn to_graph_indexed<'a>(
    triples: impl IntoIterator<Item = &'a Triple>,
) -> Result<(GInstance, TermIndex), InstError> {
    let mut builder = GraphBuilder::new();
    for triple in triples {
        builder.triple(triple)?;
    }
    Ok((
        builder.graph,
        TermIndex {
            iris: builder.iris,
            blanks: builder.blanks,
            literals: builder.literals,
            languages: builder.languages,
        },
    ))
}

/// Whether `schema` holds the RDF graph encoding: each of its vertices with
/// its kind and each of its edges.
#[must_use]
pub fn schema_holds_encoding(schema: &Schema) -> bool {
    [TRIPLE, IRI, BLANK, LITERAL, LANGUAGE].iter().all(|kind| {
        schema
            .vertices
            .get(*kind)
            .is_some_and(|vertex| vertex.kind.as_ref() == *kind)
    }) && EDGES
        .iter()
        .all(|(src, tgt, name)| schema.edges.contains_key(&encoding_edge(src, tgt, name)))
}

// ── Graph to triples ───────────────────────────────────────────────────

fn node_string(node: &Node) -> Result<&str, RdfWriteError> {
    match &node.value {
        Some(FieldPresence::Present(Value::Str(text))) => Ok(text),
        _ => Err(RdfWriteError::new(node.id, RdfWriteErrorKind::Value)),
    }
}

fn check_iri(id: u32, iri: &str) -> Result<(), RdfWriteError> {
    if let Some(character) = iri.chars().find(|&c| is_iri_excluded(c)) {
        return Err(RdfWriteError::new(
            id,
            RdfWriteErrorKind::IriCharacter { character },
        ));
    }
    if !is_absolute_iri(iri) {
        return Err(RdfWriteError::new(id, RdfWriteErrorKind::RelativeIri));
    }
    Ok(())
}

/// The single out-edge `name` of `node`, refusing any other count.
fn single(node: u32, edges: &BTreeMap<&str, Vec<u32>>, name: &str) -> Result<u32, RdfWriteError> {
    match edges.get(name).map(Vec::as_slice) {
        Some([target]) => Ok(*target),
        other => {
            let count = other.map_or(0, <[u32]>::len);
            Err(RdfWriteError::new(
                node,
                RdfWriteErrorKind::EdgeCount {
                    name: name.to_owned(),
                    count,
                },
            ))
        }
    }
}

/// The triples of a graph in the RDF graph encoding, in ascending order.
///
/// Nodes are checked in ascending id order, and the first fault found is
/// returned: for each node, its anchor, its presentation data and its value
/// (and an IRI, label or tag's form), then its out-edges (each an edge of the
/// encoding between its endpoints' anchors, with no position, and the
/// counts each anchor requires) and the literal's datatype rule; then,
/// again in id order, each term or triple another node of a smaller id
/// already holds; then each node that is a term of no triple.
///
/// # Errors
///
/// Returns [`RdfWriteError`] naming the node at fault when the graph is not
/// the graph of any list of triples.
pub fn from_graph(graph: &GInstance) -> Result<Vec<Triple>, RdfWriteError> {
    let mut ids: Vec<u32> = graph.nodes().keys().copied().collect();
    ids.sort_unstable();
    let mut out: HashMap<u32, OutEdges<'_>> = HashMap::new();
    for &id in &ids {
        out.insert(id, check_node(graph, id)?);
    }
    let view = GraphView { graph, out };
    let triples = view.distinct_triples(&ids)?;
    view.check_used(&ids)?;
    Ok(triples)
}

/// A node's out-edges by name.
type OutEdges<'a> = BTreeMap<&'a str, Vec<u32>>;

/// Check one node and its out-edges, and return the out-edges by name.
fn check_node(graph: &GInstance, id: u32) -> Result<OutEdges<'_>, RdfWriteError> {
    let node = &graph.nodes()[&id];
    let anchor = node.anchor.as_ref();
    if ![TRIPLE, IRI, BLANK, LITERAL, LANGUAGE].contains(&anchor) {
        return Err(RdfWriteError::new(
            id,
            RdfWriteErrorKind::UnknownAnchor {
                anchor: anchor.to_owned(),
            },
        ));
    }
    if node.discriminator.is_some()
        || !node.extra_fields.is_empty()
        || !node.annotations.is_empty()
        || node.position.is_some()
        || node.shape != NodeShape::Plain
    {
        return Err(RdfWriteError::new(id, RdfWriteErrorKind::Presentation));
    }
    match anchor {
        TRIPLE if node.value.is_some() => {
            return Err(RdfWriteError::new(id, RdfWriteErrorKind::Value));
        }
        TRIPLE => {}
        IRI => check_iri(id, node_string(node)?)?,
        BLANK if !is_blank_label(node_string(node)?) => {
            return Err(RdfWriteError::new(id, RdfWriteErrorKind::InvalidBlankLabel));
        }
        LANGUAGE if !is_language_tag(node_string(node)?) => {
            return Err(RdfWriteError::new(
                id,
                RdfWriteErrorKind::InvalidLanguageTag,
            ));
        }
        _ => {
            node_string(node)?;
        }
    }
    let mut edges: OutEdges<'_> = BTreeMap::new();
    for edge_id in graph.out_edges(id) {
        let edge = &graph.edges()[edge_id];
        let name = edge.edge.name.as_deref().unwrap_or("");
        let target_anchor = graph.nodes()[&edge.tgt].anchor.as_ref();
        if edge.position.is_some()
            || edge.edge != encoding_edge(anchor, target_anchor, name)
            || !EDGES
                .iter()
                .any(|(s, t, n)| *s == anchor && *t == target_anchor && *n == name)
        {
            return Err(RdfWriteError::new(
                id,
                RdfWriteErrorKind::UnexpectedEdge {
                    name: name.to_owned(),
                },
            ));
        }
        edges.entry(name).or_default().push(edge.tgt);
    }
    match anchor {
        TRIPLE => {
            for name in [SUBJECT, PREDICATE, OBJECT] {
                single(id, &edges, name)?;
            }
        }
        LITERAL => {
            let datatype = single(id, &edges, DATATYPE)?;
            let tags = edges.get(LANGUAGE_TAG).map_or(0, Vec::len);
            if tags > 1 {
                return Err(RdfWriteError::new(
                    id,
                    RdfWriteErrorKind::EdgeCount {
                        name: LANGUAGE_TAG.to_owned(),
                        count: tags,
                    },
                ));
            }
            let lang_string = matches!(
                &graph.nodes()[&datatype].value,
                Some(FieldPresence::Present(Value::Str(iri))) if iri == RDF_LANG_STRING
            );
            if lang_string != (tags == 1) {
                return Err(RdfWriteError::new(id, RdfWriteErrorKind::LanguageDatatype));
            }
        }
        _ => {}
    }
    Ok(edges)
}

/// A graph whose nodes have each passed [`check_node`].
struct GraphView<'a> {
    graph: &'a GInstance,
    out: HashMap<u32, OutEdges<'a>>,
}

impl GraphView<'_> {
    fn string(&self, id: u32) -> Result<String, RdfWriteError> {
        node_string(&self.graph.nodes()[&id]).map(str::to_owned)
    }

    fn term(&self, id: u32) -> Result<Term, RdfWriteError> {
        match self.graph.nodes()[&id].anchor.as_ref() {
            IRI => Ok(Term::Iri(self.string(id)?)),
            BLANK => Ok(Term::Blank(self.string(id)?)),
            _ => {
                let edges = &self.out[&id];
                let language = match edges.get(LANGUAGE_TAG).map(Vec::as_slice) {
                    Some([tag]) => Some(self.string(*tag)?),
                    _ => None,
                };
                Ok(Term::Literal(Literal {
                    lexical: self.string(id)?,
                    datatype: self.string(edges[DATATYPE][0])?,
                    language,
                }))
            }
        }
    }

    /// The triples, sorted, refusing a node whose term or triple a node of
    /// a smaller id already holds.
    fn distinct_triples(&self, ids: &[u32]) -> Result<Vec<Triple>, RdfWriteError> {
        let mut seen_terms: HashMap<(&str, Term), u32> = HashMap::new();
        let mut seen_tags: HashMap<String, u32> = HashMap::new();
        let mut seen_triples: HashMap<Triple, u32> = HashMap::new();
        let mut triples = Vec::new();
        for &id in ids {
            let anchor = self.graph.nodes()[&id].anchor.as_ref();
            let other = match anchor {
                TRIPLE => {
                    let edges = &self.out[&id];
                    let triple = Triple {
                        subject: self.term(edges[SUBJECT][0])?,
                        predicate: self.string(edges[PREDICATE][0])?,
                        object: self.term(edges[OBJECT][0])?,
                    };
                    let other = seen_triples.get(&triple).copied();
                    if other.is_none() {
                        seen_triples.insert(triple.clone(), id);
                        triples.push(triple);
                    }
                    other.map(|other| RdfWriteErrorKind::DuplicateTriple { other })
                }
                LANGUAGE => {
                    let tag = self.string(id)?;
                    let other = seen_tags.get(&tag).copied();
                    seen_tags.entry(tag).or_insert(id);
                    other.map(|other| RdfWriteErrorKind::DuplicateTerm { other })
                }
                _ => {
                    let key = (anchor, self.term(id)?);
                    let other = seen_terms.get(&key).copied();
                    seen_terms.entry(key).or_insert(id);
                    other.map(|other| RdfWriteErrorKind::DuplicateTerm { other })
                }
            };
            if let Some(kind) = other {
                return Err(RdfWriteError::new(id, kind));
            }
        }
        triples.sort();
        Ok(triples)
    }

    /// Refuse the first node, by id, that no triple reaches.
    fn check_used(&self, ids: &[u32]) -> Result<(), RdfWriteError> {
        let mut used: BTreeSet<u32> = BTreeSet::new();
        for &id in ids {
            if self.graph.nodes()[&id].anchor.as_ref() == TRIPLE {
                used.insert(id);
                for &target in self.out[&id].values().flatten() {
                    used.insert(target);
                    used.extend(self.out[&target].values().flatten().copied());
                }
            }
        }
        ids.iter()
            .find(|id| !used.contains(id))
            .map_or(Ok(()), |&unused| {
                Err(RdfWriteError::new(unused, RdfWriteErrorKind::Unused))
            })
    }
}
