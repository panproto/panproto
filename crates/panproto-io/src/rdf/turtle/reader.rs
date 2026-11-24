//! The Turtle reader, the `turtle` graph codec and its format-preserving
//! route.

use std::collections::{BTreeMap, HashMap, HashSet};

use panproto_gat::Name;
use panproto_inst::{FieldPresence, GInstance, Value};
use panproto_parse::languages::common::LanguageParser;
use panproto_parse::registry::AstParser;
use panproto_protocols::rdf::{RDF_LANG_STRING, XSD_STRING};
use panproto_schema::{Constraint, Schema};
use serde::{Deserialize, Serialize};

use super::super::cst::{Cursor, Tree, checked_text};
use super::super::ntriples::{decode, decode_iri_reference, push_string};
use super::super::{
    Literal, RdfReadError, RdfReadErrorKind, RdfWriteError, RdfWriteErrorKind, Term, Triple,
    from_graph, is_absolute_iri, is_blank_label, is_iri_excluded, is_language_tag,
    schema_holds_encoding, to_graph, to_graph_indexed,
};
use super::{
    InvalidBaseIri, RDF_FIRST, RDF_NIL, RDF_REST, RDF_TYPE, XSD_BOOLEAN, XSD_DECIMAL, XSD_DOUBLE,
    XSD_INTEGER, decode_local, resolve_iri, write,
};
use crate::error::{EmitInstanceError, ParseInstanceError, UnifiedCodecError};
use crate::traits::{GraphInstanceEmitter, GraphInstanceParser};
use crate::unified_codec::language_parser;

/// The protocol name the codec registers under.
const PROTOCOL: &str = "turtle";

/// How a token of the preserved document writes a term.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum TokenKind {
    /// An IRI reference, a prefixed name or the keyword `a`, which an IRI
    /// written between angle brackets replaces.
    Iri,
    /// A blank node label.
    Blank,
    /// The quoted string of a literal; the literal's tag or datatype is a
    /// token of its own.
    QuotedString,
    /// An integer, decimal, double or boolean token, whose datatype it
    /// implies.
    Bare {
        /// The datatype the token implies.
        datatype: String,
    },
    /// A language tag.
    LanguageTag,
}

/// One token of the preserved document that writes a term.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenOccurrence {
    /// The CST vertex of the token.
    pub vertex: String,
    /// How the token writes the term.
    pub token: TokenKind,
}

/// Where the preserved document writes one term.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TermOccurrences {
    /// The tokens that write it, in document order.
    pub tokens: Vec<TokenOccurrence>,
    /// Whether the document also writes it somewhere with no token of its
    /// own: the datatype of a quoted string with no datatype, of a
    /// language-tagged string or of a bare token, `rdf:first`, `rdf:rest`
    /// and `rdf:nil` of a collection, or a blank node with no label.
    pub implicit: bool,
}

/// The complement of the format-preserving Turtle route: what the graph
/// does not hold of the document it was read from.
///
/// The CST schema holds every byte of the document: directives, prefixed
/// names, abbreviations (predicate and object lists, `a`, blank node
/// property lists, collections, bare numbers and booleans), comments, white
/// space and the order of statements. The graph as read and the
/// occurrences of each of its term nodes let an edited graph be written
/// back into the same bytes with only the edited tokens rewritten.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TurtleComplement {
    /// The CST schema of the document.
    pub cst_schema: Schema,
    /// The base IRI the document was read with.
    pub base: Option<String>,
    /// The graph the document was read as.
    pub graph: GInstance,
    /// The occurrences of each term node, by node id.
    pub occurrences: BTreeMap<u32, TermOccurrences>,
}

/// A term, as the walk records its occurrences.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum TermKey {
    Iri(String),
    Blank(String),
    Literal(Literal),
    Language(String),
}

/// What reading a document yields: its triples, the occurrences of its
/// terms and its CST schema.
struct Reading {
    triples: Vec<Triple>,
    occurrences: Vec<(TermKey, Option<TokenOccurrence>)>,
    cst: Schema,
}

/// A refusal of the construct at `node`, which Turtle does not have.
fn not_in_format(node: &Cursor<'_>) -> RdfReadError {
    let construct = match node.kind {
        "graph" => "a graph block",
        other => other,
    };
    RdfReadError::new(
        node.start,
        RdfReadErrorKind::NotInFormat {
            construct: construct.into(),
        },
    )
}

const fn syntax(offset: usize) -> RdfReadError {
    RdfReadError::new(offset, RdfReadErrorKind::Syntax)
}

/// Whether `c` is white space between Turtle terminals.
const fn is_white(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\r')
}

/// The walk over one document's tree.
struct Walk<'t, 'a> {
    text: &'a str,
    tree: &'t Tree<'a>,
    base: Option<String>,
    prefixes: HashMap<String, String>,
    used_labels: HashSet<String>,
    next_fresh: u64,
    triples: Vec<Triple>,
    occurrences: Vec<(TermKey, Option<TokenOccurrence>)>,
}

impl<'a> Walk<'_, 'a> {
    fn slice(&self, node: &Cursor<'_>) -> &'a str {
        &self.text[node.start..node.end]
    }

    /// The children of `node` in source order, comments left out.
    fn children(&self, node: &Cursor<'a>) -> Vec<Cursor<'a>> {
        self.tree
            .children(node)
            .into_iter()
            .filter(|child| child.kind != "comment")
            .collect()
    }

    fn token(&mut self, key: TermKey, node: &Cursor<'_>, token: TokenKind) {
        self.occurrences.push((
            key,
            Some(TokenOccurrence {
                vertex: node.name.to_owned(),
                token,
            }),
        ));
    }

    fn implicit(&mut self, key: TermKey) {
        self.occurrences.push((key, None));
    }

    fn fresh(&mut self) -> String {
        loop {
            let label = format!("b{}", self.next_fresh);
            self.next_fresh += 1;
            if !self.used_labels.contains(&label) {
                return label;
            }
        }
    }

    fn fresh_blank(&mut self) -> Term {
        let label = self.fresh();
        self.implicit(TermKey::Blank(label.clone()));
        Term::Blank(label)
    }

    /// The IRI an `iri_reference` denotes: its escapes decoded and, when it
    /// is relative, resolved against the base IRI.
    fn iri_reference(&self, node: &Cursor<'_>) -> Result<String, RdfReadError> {
        let text = self.slice(node);
        let reference = decode_iri_reference(&text[1..text.len() - 1], node.start + 1)?;
        if is_absolute_iri(&reference) {
            return Ok(reference);
        }
        self.base.as_deref().map_or_else(
            || {
                Err(RdfReadError::new(
                    node.start + 1,
                    RdfReadErrorKind::RelativeIri,
                ))
            },
            |base| Ok(resolve_iri(base, &reference)),
        )
    }

    /// The IRI a `prefixed_name` denotes.
    fn prefixed_name(&self, node: &Cursor<'_>) -> Result<String, RdfReadError> {
        let text = self.slice(node);
        let (prefix, local) = text.split_once(':').unwrap_or((text, ""));
        if let Some(i) = prefix.find(|c: char| is_white(c) || c == '#') {
            return Err(syntax(node.start + i));
        }
        let namespace = self.prefixes.get(prefix).ok_or_else(|| {
            RdfReadError::new(
                node.start,
                RdfReadErrorKind::UndefinedPrefix {
                    prefix: prefix.to_owned(),
                },
            )
        })?;
        Ok(format!("{namespace}{}", decode_local(local)))
    }

    /// The IRI of an `iri_reference` or `prefixed_name` term, recorded.
    fn iri(&mut self, node: &Cursor<'_>) -> Result<String, RdfReadError> {
        let iri = match node.kind {
            "iri_reference" => self.iri_reference(node)?,
            "prefixed_name" => self.prefixed_name(node)?,
            _ => return Err(not_in_format(node)),
        };
        self.token(TermKey::Iri(iri.clone()), node, TokenKind::Iri);
        Ok(iri)
    }

    fn blank_label(&mut self, node: &Cursor<'_>) -> Result<Term, RdfReadError> {
        let label = &self.slice(node)[2..];
        if !is_blank_label(label) || label.contains(':') {
            return Err(RdfReadError::new(
                node.start,
                RdfReadErrorKind::InvalidBlankLabel,
            ));
        }
        self.token(TermKey::Blank(label.to_owned()), node, TokenKind::Blank);
        Ok(Term::Blank(label.to_owned()))
    }

    fn directive(&mut self, node: &Cursor<'a>) -> Result<(), RdfReadError> {
        let [directive] = self.children(node)[..] else {
            return Err(syntax(node.start));
        };
        let parts = self.children(&directive);
        match (directive.kind, parts.as_slice()) {
            ("prefix_id" | "sparql_prefix", [namespace, iri]) if namespace.kind == "namespace" => {
                let text = self.slice(namespace);
                let prefix = &text[..text.len() - 1];
                if let Some(i) = prefix.find(|c: char| is_white(c) || c == '#') {
                    return Err(syntax(namespace.start + i));
                }
                let iri = self.iri_reference(iri)?;
                self.prefixes.insert(prefix.to_owned(), iri);
                Ok(())
            }
            ("base" | "sparql_base", [iri]) if iri.kind == "iri_reference" => {
                self.base = Some(self.iri_reference(iri)?);
                Ok(())
            }
            _ => Err(syntax(directive.start)),
        }
    }

    fn statement(&mut self, node: &Cursor<'a>) -> Result<(), RdfReadError> {
        let parts = self.children(node);
        match parts.as_slice() {
            [subject, properties] if subject.kind == "subject" => {
                let subject = self.subject(subject)?;
                self.property_list(&subject, properties)
            }
            [list] if list.kind == "blank_node_property_list" => {
                self.blank_property_list(list).map(|_| ())
            }
            [list, properties] if list.kind == "blank_node_property_list" => {
                let subject = self.blank_property_list(list)?;
                self.property_list(&subject, properties)
            }
            _ => Err(syntax(node.start)),
        }
    }

    fn subject(&mut self, node: &Cursor<'a>) -> Result<Term, RdfReadError> {
        let [term] = self.children(node)[..] else {
            return Err(syntax(node.start));
        };
        match term.kind {
            "iri_reference" | "prefixed_name" => Ok(Term::Iri(self.iri(&term)?)),
            "blank_node_label" => self.blank_label(&term),
            "anon" => Ok(self.fresh_blank()),
            "collection" => self.collection(&term),
            _ => Err(not_in_format(&term)),
        }
    }

    fn property_list(&mut self, subject: &Term, node: &Cursor<'a>) -> Result<(), RdfReadError> {
        if node.kind != "property_list" {
            return Err(not_in_format(node));
        }
        for property in self.children(node) {
            let [predicate, objects] = self.children(&property)[..] else {
                return Err(syntax(property.start));
            };
            let predicate = self.predicate(&predicate)?;
            for object in self.children(&objects) {
                let object = self.object(&object)?;
                self.triples.push(Triple {
                    subject: subject.clone(),
                    predicate: predicate.clone(),
                    object,
                });
            }
        }
        Ok(())
    }

    fn predicate(&mut self, node: &Cursor<'a>) -> Result<String, RdfReadError> {
        match self.children(node)[..] {
            [] => {
                self.token(TermKey::Iri(RDF_TYPE.to_owned()), node, TokenKind::Iri);
                Ok(RDF_TYPE.to_owned())
            }
            [iri] => self.iri(&iri),
            [_, extra, ..] => Err(syntax(extra.start)),
        }
    }

    fn object(&mut self, node: &Cursor<'a>) -> Result<Term, RdfReadError> {
        match node.kind {
            "iri_reference" | "prefixed_name" => Ok(Term::Iri(self.iri(node)?)),
            "blank_node_label" => self.blank_label(node),
            "anon" => Ok(self.fresh_blank()),
            "collection" => self.collection(node),
            "blank_node_property_list" => self.blank_property_list(node),
            "rdf_literal" => self.literal(node).map(Term::Literal),
            "integer" => Ok(self.bare(node, XSD_INTEGER)),
            "decimal" => Ok(self.bare(node, XSD_DECIMAL)),
            "double" => Ok(self.bare(node, XSD_DOUBLE)),
            "boolean_literal" => Ok(self.bare(node, XSD_BOOLEAN)),
            _ => Err(not_in_format(node)),
        }
    }

    fn blank_property_list(&mut self, node: &Cursor<'a>) -> Result<Term, RdfReadError> {
        let subject = self.fresh_blank();
        let [properties] = self.children(node)[..] else {
            return Err(syntax(node.start));
        };
        self.property_list(&subject, &properties)?;
        Ok(subject)
    }

    fn collection(&mut self, node: &Cursor<'a>) -> Result<Term, RdfReadError> {
        let elements = match self.children(node)[..] {
            [] => Vec::new(),
            [list] if list.kind == "object_collection" => self.children(&list),
            [other, ..] => return Err(not_in_format(&other)),
        };
        if elements.is_empty() {
            self.implicit(TermKey::Iri(RDF_NIL.to_owned()));
            return Ok(Term::Iri(RDF_NIL.to_owned()));
        }
        let cells: Vec<Term> = elements.iter().map(|_| self.fresh_blank()).collect();
        for (i, element) in elements.iter().enumerate() {
            let object = self.object(element)?;
            let rest = cells
                .get(i + 1)
                .cloned()
                .unwrap_or_else(|| Term::Iri(RDF_NIL.to_owned()));
            for iri in [RDF_FIRST, RDF_REST] {
                self.implicit(TermKey::Iri(iri.to_owned()));
            }
            if i + 1 == cells.len() {
                self.implicit(TermKey::Iri(RDF_NIL.to_owned()));
            }
            self.triples.push(Triple {
                subject: cells[i].clone(),
                predicate: RDF_FIRST.to_owned(),
                object,
            });
            self.triples.push(Triple {
                subject: cells[i].clone(),
                predicate: RDF_REST.to_owned(),
                object: rest,
            });
        }
        Ok(cells[0].clone())
    }

    /// The lexical form of a `string` node: its body between its quotes,
    /// escapes decoded. A short string may not hold its own quote, a line
    /// feed or a carriage return raw.
    fn string(&self, node: &Cursor<'_>) -> Result<String, RdfReadError> {
        let text = self.slice(node);
        let long = text.len() >= 6 && (text.starts_with("\"\"\"") || text.starts_with("'''"));
        let (quote, raw): (usize, &[char]) = if long {
            (3, &[])
        } else if text.starts_with('"') {
            (1, &['"', '\n', '\r'])
        } else {
            (1, &['\'', '\n', '\r'])
        };
        decode(
            &text[quote..text.len() - quote],
            node.start + quote,
            true,
            raw,
        )
    }

    fn literal(&mut self, node: &Cursor<'a>) -> Result<Literal, RdfReadError> {
        let parts = self.children(node);
        let [string, rest @ ..] = parts.as_slice() else {
            return Err(syntax(node.start));
        };
        if string.kind != "string" {
            return Err(syntax(string.start));
        }
        let lexical = self.string(string)?;
        let literal = match rest {
            [] => {
                self.implicit(TermKey::Iri(XSD_STRING.to_owned()));
                Literal {
                    lexical,
                    datatype: XSD_STRING.to_owned(),
                    language: None,
                }
            }
            [tag] if tag.kind == "lang_tag" => {
                let tag_text = &self.slice(tag)[1..];
                if !is_language_tag(tag_text) {
                    return Err(RdfReadError::new(
                        tag.start,
                        RdfReadErrorKind::InvalidLanguageTag,
                    ));
                }
                self.token(
                    TermKey::Language(tag_text.to_owned()),
                    tag,
                    TokenKind::LanguageTag,
                );
                self.implicit(TermKey::Iri(RDF_LANG_STRING.to_owned()));
                Literal {
                    lexical,
                    datatype: RDF_LANG_STRING.to_owned(),
                    language: Some(tag_text.to_owned()),
                }
            }
            [datatype] => Literal {
                lexical,
                datatype: self.iri(datatype)?,
                language: None,
            },
            [_, extra, ..] => return Err(syntax(extra.start)),
        };
        self.token(
            TermKey::Literal(literal.clone()),
            string,
            TokenKind::QuotedString,
        );
        Ok(literal)
    }

    fn bare(&mut self, node: &Cursor<'_>, datatype: &str) -> Term {
        let literal = Literal {
            lexical: self.slice(node).to_owned(),
            datatype: datatype.to_owned(),
            language: None,
        };
        self.implicit(TermKey::Iri(datatype.to_owned()));
        self.token(
            TermKey::Literal(literal.clone()),
            node,
            TokenKind::Bare {
                datatype: datatype.to_owned(),
            },
        );
        Term::Literal(literal)
    }

    /// Walk the document's statements in order.
    ///
    /// When the parser could not read the document from `recovery` on, the
    /// statements that end before it are still walked, so a fault there is
    /// reported first; then each term token that ends before `recovery`
    /// and lies in no walked statement is checked on its own (an IRI's
    /// escapes and resolution, a prefixed name's prefix, a string's
    /// escapes, a label's form), and the first it refuses is reported.
    /// Only then is the place the grammar cannot read reported.
    fn document(&mut self, root: &Cursor<'a>, recovery: Option<usize>) -> Result<(), RdfReadError> {
        let mut cursor = 0;
        for child in self.tree.children(root) {
            if let Some(offset) = recovery
                && child.end > offset
            {
                self.check_tokens(cursor, offset)?;
                return Err(syntax(offset));
            }
            if let Some(i) = self.text[cursor..child.start].find(|c: char| !is_white(c)) {
                return Err(syntax(cursor + i));
            }
            self.child(&child)?;
            cursor = child.end;
        }
        if let Some(offset) = recovery {
            self.check_tokens(cursor, offset)?;
            return Err(syntax(offset));
        }
        if let Some(i) = self.text[cursor..].find(|c: char| !is_white(c)) {
            return Err(syntax(cursor + i));
        }
        Ok(())
    }

    /// Check, in document order, each term token from `from` that ends by
    /// `to`.
    fn check_tokens(&mut self, from: usize, to: usize) -> Result<(), RdfReadError> {
        let mut tokens: Vec<Cursor<'a>> = [
            "iri_reference",
            "prefixed_name",
            "string",
            "blank_node_label",
        ]
        .iter()
        .flat_map(|kind| self.tree.all_of_kind(kind))
        .filter(|token| token.start >= from && token.end <= to)
        .collect();
        tokens.sort_by_key(|token| token.start);
        for token in &tokens {
            match token.kind {
                "iri_reference" => self.iri_reference(token).map(drop)?,
                "prefixed_name" => self.prefixed_name(token).map(drop)?,
                "string" => self.string(token).map(drop)?,
                _ => self.blank_label(token).map(drop)?,
            }
        }
        Ok(())
    }

    /// Walk one child of the document.
    fn child(&mut self, child: &Cursor<'a>) -> Result<(), RdfReadError> {
        match child.kind {
            "comment" => Ok(()),
            "directive" => self.directive(child),
            "triple" => self.statement(child),
            _ => Err(not_in_format(child)),
        }
    }
}

/// Whether `lexical` is a token of the grammar's form for `datatype`:
/// `INTEGER`, `DECIMAL`, `DOUBLE` or `BooleanLiteral` of RDF 1.1 Turtle.
fn is_bare_token(datatype: &str, lexical: &str) -> bool {
    fn digits(s: &str) -> bool {
        !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
    }
    let unsigned = lexical.strip_prefix(['+', '-']).unwrap_or(lexical);
    match datatype {
        XSD_INTEGER => digits(unsigned),
        XSD_DECIMAL => unsigned.split_once('.').is_some_and(|(whole, fraction)| {
            whole.bytes().all(|b| b.is_ascii_digit()) && digits(fraction)
        }),
        XSD_DOUBLE => unsigned
            .split_once(['e', 'E'])
            .is_some_and(|(mantissa, exponent)| {
                let exponent = exponent.strip_prefix(['+', '-']).unwrap_or(exponent);
                let mantissa_ok = match mantissa.split_once('.') {
                    Some((whole, fraction)) => {
                        (digits(whole) && fraction.bytes().all(|b| b.is_ascii_digit()))
                            || (whole.is_empty() && digits(fraction))
                    }
                    None => digits(mantissa),
                };
                mantissa_ok && digits(exponent)
            }),
        XSD_BOOLEAN => matches!(lexical, "true" | "false"),
        _ => false,
    }
}

/// The text that writes `node`'s new value in place of a token of kind
/// `token`.
fn replacement(graph: &GInstance, id: u32, token: &TokenKind) -> Result<String, RdfWriteError> {
    let value = match &graph.nodes()[&id].value {
        Some(FieldPresence::Present(Value::Str(value))) => value.as_str(),
        _ => return Err(RdfWriteError::new(id, RdfWriteErrorKind::Value)),
    };
    let quoted = |out: &mut String| {
        out.push('"');
        push_string(out, value);
        out.push('"');
    };
    let mut out = String::new();
    match token {
        TokenKind::Iri => {
            out.push('<');
            out.push_str(value);
            out.push('>');
        }
        TokenKind::Blank => {
            if value.contains(':') {
                return Err(RdfWriteError::new(id, RdfWriteErrorKind::InvalidBlankLabel));
            }
            out.push_str("_:");
            out.push_str(value);
        }
        TokenKind::QuotedString => quoted(&mut out),
        TokenKind::Bare { datatype } => {
            if is_bare_token(datatype, value) {
                out.push_str(value);
            } else {
                quoted(&mut out);
                out.push_str("^^<");
                out.push_str(datatype);
                out.push('>');
            }
        }
        TokenKind::LanguageTag => {
            out.push('@');
            out.push_str(value);
        }
    }
    Ok(out)
}

/// Refuse `graph` when it differs from `original` in more than node values:
/// a node added or removed, an anchor changed, or an edge added, removed,
/// renamed or moved.
pub(in crate::rdf) fn structural_difference(
    graph: &GInstance,
    original: &GInstance,
) -> Result<(), RdfWriteError> {
    let mut ids: Vec<u32> = graph
        .nodes()
        .keys()
        .chain(original.nodes().keys())
        .copied()
        .collect();
    ids.sort_unstable();
    ids.dedup();
    for id in ids {
        let detail = match (graph.nodes().get(&id), original.nodes().get(&id)) {
            (Some(_), None) => "the node was added",
            (None, Some(_)) => "the node was removed",
            (Some(now), Some(then)) if now.anchor != then.anchor => "the node's anchor changed",
            _ => continue,
        };
        return Err(RdfWriteError::new(
            id,
            RdfWriteErrorKind::StructuralEdit {
                detail: detail.to_owned(),
            },
        ));
    }
    let now: BTreeMap<_, _> = graph.edges().iter().collect();
    let then: BTreeMap<_, _> = original.edges().iter().collect();
    let mut edge_ids: Vec<_> = now.keys().chain(then.keys()).copied().collect();
    edge_ids.sort_unstable();
    edge_ids.dedup();
    for edge_id in edge_ids {
        match (now.get(edge_id), then.get(edge_id)) {
            (Some(a), Some(b)) if a == b => {}
            (Some(edge), _) | (None, Some(edge)) => {
                return Err(RdfWriteError::new(
                    edge.src,
                    RdfWriteErrorKind::StructuralEdit {
                        detail: format!("edge {edge_id} was added, removed or changed"),
                    },
                ));
            }
            (None, None) => {}
        }
    }
    Ok(())
}

/// Give the CST vertex `vertex` a `literal-value` holding `text`, so that
/// emission writes `text` over the whole span the vertex covers.
pub(in crate::rdf) fn set_token_text(cst: &mut Schema, vertex: &str, text: &str) {
    let constraints = cst.constraints.entry(Name::from(vertex)).or_default();
    if let Some(existing) = constraints
        .iter_mut()
        .find(|c| c.sort.as_ref() == "literal-value")
    {
        text.clone_into(&mut existing.value);
        return;
    }
    constraints.push(Constraint {
        sort: Name::from("literal-value"),
        value: text.to_owned(),
    });
}

/// The `turtle` graph codec: reads Turtle into the RDF graph encoding,
/// writes a graph as canonical Turtle, and reads and writes through a
/// format-preserving route.
pub struct TurtleCodec {
    parser: LanguageParser,
    base: Option<String>,
}

impl TurtleCodec {
    /// A codec over the bundled tree-sitter Turtle grammar, with no base
    /// IRI: a document that holds a relative IRI reference is refused.
    ///
    /// # Errors
    ///
    /// Returns [`UnifiedCodecError`] if the Turtle grammar is not compiled
    /// in or fails to initialise.
    pub fn new() -> Result<Self, UnifiedCodecError> {
        Ok(Self {
            parser: language_parser("turtle")?,
            base: None,
        })
    }

    /// This codec, reading with `base` as the base IRI a document starts
    /// with (its retrieval IRI, by RFC 3986 Sec 5.1.3).
    ///
    /// # Errors
    ///
    /// Returns [`InvalidBaseIri`] when `base` is not an absolute IRI or
    /// holds a character no IRI holds.
    pub fn with_base(mut self, base: impl Into<String>) -> Result<Self, InvalidBaseIri> {
        let base = base.into();
        if !is_absolute_iri(&base) || base.chars().any(is_iri_excluded) {
            return Err(InvalidBaseIri { base });
        }
        self.base = Some(base);
        Ok(self)
    }

    /// The base IRI a document starts with, if any.
    #[must_use]
    pub fn base(&self) -> Option<&str> {
        self.base.as_deref()
    }

    fn reading(&self, input: &[u8]) -> Result<Reading, RdfReadError> {
        let text = checked_text(input)?;
        let cst = self
            .parser
            .parse(input, "input.ttl")
            .map_err(|_| syntax(0))?;
        let tree = Tree { cst: &cst };
        let recovery = tree.first_marker();
        let Some(root) = tree.root() else {
            let fault = recovery.or_else(|| text.find(|c: char| !is_white(c)));
            return fault.map_or_else(
                || {
                    Ok(Reading {
                        triples: Vec::new(),
                        occurrences: Vec::new(),
                        cst: cst.clone(),
                    })
                },
                |offset| Err(syntax(offset)),
            );
        };
        let used_labels = tree
            .all_of_kind("blank_node_label")
            .iter()
            .map(|node| text[node.start + 2..node.end].to_owned())
            .collect();
        let mut walk = Walk {
            text,
            tree: &tree,
            base: self.base.clone(),
            prefixes: HashMap::new(),
            used_labels,
            next_fresh: 0,
            triples: Vec::new(),
            occurrences: Vec::new(),
        };
        walk.document(&root, recovery)?;
        let (triples, occurrences) = (walk.triples, walk.occurrences);
        Ok(Reading {
            triples,
            occurrences,
            cst,
        })
    }

    /// The triples of a Turtle document, in the order the walk completes
    /// them, a repeated triple listed each time it is stated.
    ///
    /// # Errors
    ///
    /// Returns [`RdfReadError`] naming the byte offset of the first fault
    /// the reader meets: invalid UTF-8, then a raw U+0000 or U+007F, then
    /// the first fault in document order, a place the grammar cannot read
    /// included.
    pub fn read(&self, input: &[u8]) -> Result<Vec<Triple>, RdfReadError> {
        self.reading(input).map(|reading| reading.triples)
    }

    /// Read a Turtle document into the RDF graph encoding, with the
    /// complement that writes it back byte for byte.
    ///
    /// The graph is the one [`parse_graph`](GraphInstanceParser::parse_graph)
    /// reads.
    ///
    /// # Errors
    ///
    /// Returns [`ParseInstanceError::SchemaMismatch`] when `schema` is not
    /// the RDF graph encoding, and [`ParseInstanceError::Rdf`] when the
    /// reader refuses the document.
    pub fn parse_graph_preserving(
        &self,
        schema: &Schema,
        input: &[u8],
    ) -> Result<(GInstance, TurtleComplement), ParseInstanceError> {
        check_schema(schema).map_err(|message| ParseInstanceError::SchemaMismatch {
            protocol: PROTOCOL.into(),
            message,
        })?;
        let reading = self
            .reading(input)
            .map_err(|error| ParseInstanceError::Rdf {
                protocol: PROTOCOL.into(),
                error,
            })?;
        let (graph, index) =
            to_graph_indexed(&reading.triples).map_err(|e| ParseInstanceError::Parse {
                protocol: PROTOCOL.into(),
                message: e.to_string(),
            })?;
        let mut occurrences: BTreeMap<u32, TermOccurrences> = BTreeMap::new();
        for (key, token) in reading.occurrences {
            let node = match &key {
                TermKey::Iri(iri) => index.iris.get(iri),
                TermKey::Blank(label) => index.blanks.get(label),
                TermKey::Literal(literal) => index.literals.get(literal),
                TermKey::Language(tag) => index.languages.get(tag),
            };
            if let Some(&node) = node {
                let entry = occurrences.entry(node).or_default();
                match token {
                    Some(token) => entry.tokens.push(token),
                    None => entry.implicit = true,
                }
            }
        }
        Ok((
            graph.clone(),
            TurtleComplement {
                cst_schema: reading.cst,
                base: self.base.clone(),
                graph,
                occurrences,
            },
        ))
    }

    /// Write `graph` back into the document `complement` preserves.
    ///
    /// An unedited graph is written as the document's bytes. An edit to the
    /// value of a term node (an IRI, a blank node label, a literal's lexical
    /// form, a language tag) rewrites each token that writes the term and
    /// nothing else: an IRI as an absolute IRI between angle brackets, a
    /// label as `_:` and the label, a quoted string as a double-quoted
    /// string with the canonical N-Triples escapes, a bare number or boolean
    /// as the bare token when the new lexical form is a token of the same
    /// form and otherwise as a quoted string with its datatype, and a tag
    /// as `@` and the tag.
    ///
    /// # Errors
    ///
    /// Returns [`EmitInstanceError::Rdf`] carrying an [`RdfWriteError`]
    /// that names the node at fault: `StructuralEdit` for a graph that
    /// differs from the one read in more than node values, any refusal of
    /// [`from_graph`] for a graph no document reads back as, `ImplicitTerm`
    /// for a changed term the document writes without a token in some
    /// place, and `InvalidBlankLabel` for a label Turtle cannot write.
    /// Returns [`EmitInstanceError::Emit`] when the CST cannot be emitted.
    pub fn emit_graph_preserving(
        &self,
        schema: &Schema,
        graph: &GInstance,
        complement: &TurtleComplement,
    ) -> Result<Vec<u8>, EmitInstanceError> {
        check_schema(schema).map_err(|message| EmitInstanceError::Emit {
            protocol: PROTOCOL.into(),
            message,
        })?;
        let refuse = |error| EmitInstanceError::Rdf {
            protocol: PROTOCOL.into(),
            error,
        };
        structural_difference(graph, &complement.graph).map_err(refuse)?;
        from_graph(graph).map_err(refuse)?;
        let mut cst = complement.cst_schema.clone();
        let mut ids: Vec<u32> = graph.nodes().keys().copied().collect();
        ids.sort_unstable();
        for id in ids {
            if graph.nodes()[&id].value == complement.graph.nodes()[&id].value {
                continue;
            }
            let Some(occurrences) = complement.occurrences.get(&id) else {
                continue;
            };
            if occurrences.implicit {
                return Err(refuse(RdfWriteError::new(
                    id,
                    RdfWriteErrorKind::ImplicitTerm,
                )));
            }
            for occurrence in &occurrences.tokens {
                let text = replacement(graph, id, &occurrence.token).map_err(refuse)?;
                set_token_text(&mut cst, &occurrence.vertex, &text);
            }
        }
        self.parser.emit(&cst).map_err(|e| EmitInstanceError::Emit {
            protocol: PROTOCOL.into(),
            message: e.to_string(),
        })
    }
}

fn check_schema(schema: &Schema) -> Result<(), String> {
    if schema_holds_encoding(schema) {
        Ok(())
    } else {
        Err("the schema is not the RDF graph encoding".into())
    }
}

impl GraphInstanceParser for TurtleCodec {
    fn protocol_name(&self) -> &'static str {
        PROTOCOL
    }

    fn parse_graph(&self, schema: &Schema, input: &[u8]) -> Result<GInstance, ParseInstanceError> {
        check_schema(schema).map_err(|message| ParseInstanceError::SchemaMismatch {
            protocol: PROTOCOL.into(),
            message,
        })?;
        let triples = self.read(input).map_err(|error| ParseInstanceError::Rdf {
            protocol: PROTOCOL.into(),
            error,
        })?;
        to_graph(&triples).map_err(|e| ParseInstanceError::Parse {
            protocol: PROTOCOL.into(),
            message: e.to_string(),
        })
    }
}

impl GraphInstanceEmitter for TurtleCodec {
    fn protocol_name(&self) -> &'static str {
        PROTOCOL
    }

    fn emit_graph(
        &self,
        schema: &Schema,
        instance: &GInstance,
    ) -> Result<Vec<u8>, EmitInstanceError> {
        check_schema(schema).map_err(|message| EmitInstanceError::Emit {
            protocol: PROTOCOL.into(),
            message,
        })?;
        let triples = from_graph(instance).map_err(|error| EmitInstanceError::Rdf {
            protocol: PROTOCOL.into(),
            error,
        })?;
        Ok(write(&triples).into_bytes())
    }
}
