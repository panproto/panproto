//! Deserialize JSON-LD to RDF (JSON-LD 1.1 API Sec 8.1 to 8.3) over the
//! expanded default graph.
//!
//! The API first builds a node map, which relabels every blank node, and
//! then reads triples off it. The triples of the default graph are the same
//! set when they are read off the expanded items directly, which this
//! module does: a node object's `@type` values, properties and reverse
//! properties each give triples, and every node object nested as a value
//! gives its own. A blank node identifier the document writes keeps its
//! label when it is an N-Triples blank node label; every other blank node
//! gets a fresh label `b` and a decimal counter from 0, skipping every label
//! the document keeps.
//!
//! Triples are listed in the order the walk completes them. A node object's
//! label is allocated when the walk reaches it, then its `@type` triples
//! come first, then each property's values in document order, each value's
//! own triples before the triple that has it as object, then each reverse
//! property's values likewise, each before its reversed triple. A list
//! allocates all its cells' labels when it is reached, then gives each
//! cell's element's own triples followed by the cell's `rdf:first` and
//! `rdf:rest` triples.

use std::collections::{HashMap, HashSet};

use panproto_protocols::rdf::{RDF_LANG_STRING, XSD_STRING};

use super::expand::{Fault, Item, ListObject, NodeObject, Origin, Scalar, ValueObject};
use super::{JsonLdRefusal, boolean_literal, number_literal};
use crate::rdf::turtle::{RDF_FIRST, RDF_NIL, RDF_REST, RDF_TYPE};
use crate::rdf::{
    Literal, Term, Triple, is_absolute_iri, is_blank_label, is_iri_excluded, is_language_tag,
};

/// A term as the occurrence table keys it.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum TermKey {
    Iri(String),
    Blank(String),
    Literal(Literal),
    Language(String),
}

/// How a token writes a term.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Spelling {
    /// A JSON string spelling an IRI or a blank node identifier.
    Reference,
    /// A JSON string holding a literal's lexical form.
    Lexical,
    /// A JSON number or boolean.
    Native,
    /// A JSON string holding a language tag.
    Tag,
}

/// The triples of a document and where each term is written.
pub struct Reading {
    pub triples: Vec<Triple>,
    /// Each term's occurrences: the token that writes it, or `None` where
    /// the document writes it with no token of its own.
    pub occurrences: Vec<(TermKey, Option<(usize, Spelling)>)>,
}

type Res<T> = Result<T, Fault>;

struct Walk {
    triples: Vec<Triple>,
    occurrences: Vec<(TermKey, Option<(usize, Spelling)>)>,
    kept: HashSet<String>,
    renamed: HashMap<String, String>,
    next: u64,
}

fn refuse(refusal: JsonLdRefusal, origin: &Origin) -> Fault {
    Fault::at_origin(refusal, origin)
}

/// Refuse an IRI that holds an excluded character or is not absolute.
fn check_iri(iri: &str, origin: &Origin) -> Res<()> {
    if let Some(character) = iri.chars().find(|&c| is_iri_excluded(c)) {
        return Err(refuse(JsonLdRefusal::IriCharacter { character }, origin));
    }
    if !is_absolute_iri(iri) {
        return Err(refuse(JsonLdRefusal::RelativeIri, origin));
    }
    if iri.matches('#').count() > 1 {
        return Err(refuse(JsonLdRefusal::MalformedIri, origin));
    }
    Ok(())
}

/// Keep the label of the blank node identifier `id`, when it is an
/// N-Triples blank node label.
fn keep(out: &mut HashSet<String>, id: &str) {
    if let Some(label) = id.strip_prefix("_:")
        && is_blank_label(label)
    {
        out.insert(label.to_owned());
    }
}

/// Every blank node label the expanded items keep, in no order.
fn kept_labels(items: &[Item], out: &mut HashSet<String>) {
    for item in items {
        match item {
            Item::Node(node) => {
                if let Some((id, _)) = &node.id {
                    keep(out, id);
                }
                for (ty, _) in &node.types {
                    keep(out, ty);
                }
                for (_, _, values) in node.properties.iter().chain(&node.reverse) {
                    kept_labels(values, out);
                }
                if let Some(graph) = &node.graph {
                    kept_labels(graph, out);
                }
            }
            Item::List(list) => kept_labels(&list.items, out),
            Item::Value(_) => {}
        }
    }
}

impl Walk {
    fn record(&mut self, key: TermKey, origin: &Origin, spelling: Spelling) {
        self.occurrences
            .push((key, origin.token.map(|token| (token, spelling))));
    }

    fn implicit(&mut self, key: TermKey) {
        self.occurrences.push((key, None));
    }

    fn fresh(&mut self) -> String {
        loop {
            let label = format!("b{}", self.next);
            self.next += 1;
            if !self.kept.contains(&label) {
                return label;
            }
        }
    }

    fn fresh_blank(&mut self) -> Term {
        let label = self.fresh();
        self.implicit(TermKey::Blank(label.clone()));
        Term::Blank(label)
    }

    /// The term an expanded `@id` or `@type` value denotes.
    fn reference(&mut self, id: &str, origin: &Origin) -> Res<Term> {
        if let Some(label) = id.strip_prefix("_:") {
            if is_blank_label(label) {
                self.record(
                    TermKey::Blank(label.to_owned()),
                    origin,
                    Spelling::Reference,
                );
                return Ok(Term::Blank(label.to_owned()));
            }
            let label = if let Some(fresh) = self.renamed.get(label) {
                fresh.clone()
            } else {
                let fresh = self.fresh();
                self.renamed.insert(label.to_owned(), fresh.clone());
                fresh
            };
            self.implicit(TermKey::Blank(label.clone()));
            return Ok(Term::Blank(label));
        }
        check_iri(id, origin)?;
        self.record(TermKey::Iri(id.to_owned()), origin, Spelling::Reference);
        Ok(Term::Iri(id.to_owned()))
    }

    fn iri(&mut self, iri: &str) -> String {
        self.implicit(TermKey::Iri(iri.to_owned()));
        iri.to_owned()
    }

    fn push(&mut self, subject: Term, predicate: String, object: Term) {
        self.triples.push(Triple {
            subject,
            predicate,
            object,
        });
    }

    /// The predicate an expanded property denotes.
    fn predicate(&mut self, iri: &str, origin: &Origin) -> Res<String> {
        if iri.starts_with("_:") {
            return Err(refuse(JsonLdRefusal::BlankPredicate, origin));
        }
        check_iri(iri, origin)?;
        self.record(TermKey::Iri(iri.to_owned()), origin, Spelling::Reference);
        Ok(iri.to_owned())
    }

    /// Give the triples of `node` and return its term.
    fn node(&mut self, node: &NodeObject) -> Res<Term> {
        if node.graph.is_some() {
            return Err(refuse(JsonLdRefusal::NamedGraph, &node.origin));
        }
        let subject = match &node.id {
            Some((id, origin)) => self.reference(id, origin)?,
            None => self.fresh_blank(),
        };
        for (ty, origin) in &node.types {
            let object = self.reference(ty, origin)?;
            let predicate = self.iri(RDF_TYPE);
            self.push(subject.clone(), predicate, object);
        }
        for (iri, origin, values) in &node.properties {
            let predicate = self.predicate(iri, origin)?;
            for value in values {
                let object = self.object(value)?;
                self.push(subject.clone(), predicate.clone(), object);
            }
        }
        for (iri, origin, values) in &node.reverse {
            let predicate = self.predicate(iri, origin)?;
            for value in values {
                let reversed = self.object(value)?;
                self.push(reversed, predicate.clone(), subject.clone());
            }
        }
        Ok(subject)
    }

    /// Object to RDF Conversion (API Sec 8.2).
    fn object(&mut self, item: &Item) -> Res<Term> {
        match item {
            Item::Node(node) => self.node(node),
            Item::Value(value) => self.literal(value).map(Term::Literal),
            Item::List(list) => self.list(list),
        }
    }

    /// List to RDF Conversion (API Sec 8.3).
    fn list(&mut self, list: &ListObject) -> Res<Term> {
        if list.items.is_empty() {
            return Ok(Term::Iri(self.iri(RDF_NIL)));
        }
        let cells: Vec<Term> = list.items.iter().map(|_| self.fresh_blank()).collect();
        for (i, item) in list.items.iter().enumerate() {
            let object = self.object(item)?;
            let first = self.iri(RDF_FIRST);
            self.push(cells[i].clone(), first, object);
            let rest = cells
                .get(i + 1)
                .map_or_else(|| Term::Iri(self.iri(RDF_NIL)), Clone::clone);
            let predicate = self.iri(RDF_REST);
            self.push(cells[i].clone(), predicate, rest);
        }
        Ok(cells[0].clone())
    }

    /// The literal of a value object.
    fn literal(&mut self, value: &ValueObject) -> Res<Literal> {
        let datatype = match &value.datatype {
            Some((iri, origin)) => {
                check_iri(iri, origin)?;
                Some(iri.as_str())
            }
            None => None,
        };
        let (mut literal, spelling) = match &value.value {
            Scalar::Str(text) => (
                Literal {
                    lexical: text.clone(),
                    datatype: datatype
                        .unwrap_or_else(|| {
                            if value.language.is_some() {
                                RDF_LANG_STRING
                            } else {
                                XSD_STRING
                            }
                        })
                        .to_owned(),
                    language: None,
                },
                Spelling::Lexical,
            ),
            Scalar::Number(token) => (
                number_literal(token, datatype)
                    .ok_or_else(|| refuse(JsonLdRefusal::NumberOutOfRange, &value.value_origin))?,
                Spelling::Native,
            ),
            Scalar::Bool(b) => (boolean_literal(*b, datatype), Spelling::Native),
        };
        match &value.datatype {
            Some((iri, origin)) => {
                self.record(TermKey::Iri(iri.clone()), origin, Spelling::Reference);
            }
            None => self.implicit(TermKey::Iri(literal.datatype.clone())),
        }
        if let Some((tag, origin)) = &value.language {
            if !is_language_tag(tag) {
                return Err(refuse(JsonLdRefusal::InvalidLanguageTag, origin));
            }
            self.record(TermKey::Language(tag.clone()), origin, Spelling::Tag);
            literal.language = Some(tag.clone());
        }
        self.record(
            TermKey::Literal(literal.clone()),
            &value.value_origin,
            spelling,
        );
        Ok(literal)
    }
}

/// The triples of the expanded default graph `items`.
pub fn read_items(items: &[Item]) -> Res<Reading> {
    let mut kept = HashSet::new();
    kept_labels(items, &mut kept);
    let mut walk = Walk {
        triples: Vec::new(),
        occurrences: Vec::new(),
        kept,
        renamed: HashMap::new(),
        next: 0,
    };
    for item in items {
        match item {
            Item::Node(node) => {
                walk.node(node)?;
            }
            Item::Value(value) => {
                return Err(refuse(JsonLdRefusal::FreeFloating, &value.origin));
            }
            Item::List(list) => {
                return Err(refuse(JsonLdRefusal::FreeFloating, &list.origin));
            }
        }
    }
    Ok(Reading {
        triples: walk.triples,
        occurrences: walk.occurrences,
    })
}
