//! The JSON-LD reader over the tree-sitter JSON grammar, the `jsonld` graph
//! codec and its format-preserving route.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::rc::Rc;

use panproto_inst::{FieldPresence, GInstance, Value};
use panproto_parse::languages::common::LanguageParser;
use panproto_parse::registry::AstParser;
use panproto_protocols::rdf::IRI;
use panproto_schema::Schema;
use serde::{Deserialize, Serialize};

use super::expand::{Expansion, Fault};
use super::json::{Json, Kind, Member, child_pointer};
use super::to_rdf::{Reading, Spelling, TermKey, read_items};
use super::{is_json_number, number_literal, push_json_string, write};
use crate::error::{EmitInstanceError, ParseInstanceError, UnifiedCodecError};
use crate::json_pathway::{JsonStringFaultSite, json_string_fault};
use crate::rdf::cst::{Cursor, Tree};
use crate::rdf::turtle::reader::{set_token_text, structural_difference};
use crate::rdf::turtle::{
    InvalidBaseIri, TermOccurrences, TokenKind, TokenOccurrence, XSD_BOOLEAN,
};
use crate::rdf::{
    RdfReadError, RdfReadErrorKind, RdfWriteError, RdfWriteErrorKind, Triple, from_graph,
    is_absolute_iri, is_iri_excluded, schema_holds_encoding, to_graph, to_graph_indexed,
};
use crate::traits::{GraphInstanceEmitter, GraphInstanceParser};
use crate::unified_codec::language_parser;

/// The protocol name the codec registers under.
const PROTOCOL: &str = "jsonld";

/// The deepest nesting of arrays and objects the reader accepts; the top
/// value is at depth one.
pub const MAX_DEPTH: usize = 128;

const fn syntax(offset: usize) -> RdfReadError {
    RdfReadError::new(offset, RdfReadErrorKind::Syntax)
}

fn from_fault(fault: Fault) -> RdfReadError {
    RdfReadError::new(
        fault.offset,
        RdfReadErrorKind::JsonLd {
            refusal: fault.refusal,
            pointer: fault.pointer.to_string(),
        },
    )
}

/// Builds the JSON value of a document from its tree.
struct Builder<'t, 'a> {
    text: &'a str,
    tree: &'t Tree<'a>,
    /// The CST vertex of each token, by index.
    tokens: Vec<String>,
}

impl<'a> Builder<'_, 'a> {
    fn slice(&self, node: &Cursor<'_>) -> &'a str {
        &self.text[node.start..node.end]
    }

    /// The decoded text of a `string` token.
    fn string(&self, node: &Cursor<'_>) -> Result<String, RdfReadError> {
        let token = self.slice(node);
        serde_json::from_str::<String>(token).map_err(|_| {
            let offset = match json_string_fault(token).map(|fault| fault.site) {
                Some(JsonStringFaultSite::Byte(at) | JsonStringFaultSite::End(Some(at))) => at,
                _ => 0,
            };
            syntax(node.start + offset)
        })
    }

    fn token(&mut self, node: &Cursor<'_>) -> usize {
        self.tokens.push(node.name.to_owned());
        self.tokens.len() - 1
    }

    fn value(
        &mut self,
        node: &Cursor<'a>,
        pointer: Rc<str>,
        depth: usize,
    ) -> Result<Json, RdfReadError> {
        if depth > MAX_DEPTH {
            return Err(RdfReadError::new(
                node.start,
                RdfReadErrorKind::NestingLimit { limit: MAX_DEPTH },
            ));
        }
        let token = self.token(node);
        let kind = match node.kind {
            "object" => Kind::Object(self.object(node, &pointer, depth)?),
            "array" => {
                let mut items = Vec::new();
                for (i, child) in self.tree.children(node).iter().enumerate() {
                    let pointer = child_pointer(&pointer, &i.to_string());
                    items.push(self.value(child, pointer, depth + 1)?);
                }
                Kind::Array(items)
            }
            "string" => Kind::Str(self.string(node)?),
            "number" => {
                let text = self.slice(node);
                if !is_json_number(text) {
                    return Err(syntax(node.start));
                }
                Kind::Number(text.to_owned())
            }
            "true" => Kind::Bool(true),
            "false" => Kind::Bool(false),
            "null" => Kind::Null,
            _ => return Err(syntax(node.start)),
        };
        Ok(Json {
            kind,
            pointer,
            offset: node.start,
            token,
        })
    }

    fn object(
        &mut self,
        node: &Cursor<'a>,
        pointer: &str,
        depth: usize,
    ) -> Result<Vec<Member>, RdfReadError> {
        let mut members = Vec::new();
        let mut keys: HashSet<String> = HashSet::new();
        for pair in self.tree.children(node) {
            let [key, value] = self.tree.children(&pair)[..] else {
                return Err(syntax(pair.start));
            };
            if key.kind != "string" {
                return Err(syntax(key.start));
            }
            let text = self.string(&key)?;
            let child = child_pointer(pointer, &text);
            if !keys.insert(text.clone()) {
                return Err(RdfReadError::new(
                    value.start,
                    RdfReadErrorKind::JsonLd {
                        refusal: super::JsonLdRefusal::DuplicateKey { key: text },
                        pointer: child.to_string(),
                    },
                ));
            }
            let key_token = self.token(&key);
            let value = self.value(&value, child, depth + 1)?;
            members.push(Member {
                key: text,
                key_token,
                value,
            });
        }
        Ok(members)
    }
}

/// What reading a document yields.
struct Read {
    reading: Reading,
    tokens: Vec<String>,
    cst: Schema,
}

/// The format-preserving complement of a JSON-LD document: what the graph
/// does not hold of it.
///
/// The CST schema holds every byte of the document: its contexts, compact
/// IRIs, terms, key order, white space and the shape of its node objects.
/// The graph as read and the occurrences of each of its term nodes let an
/// edited graph be written back into the same bytes with only the edited
/// tokens rewritten.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct JsonLdComplement {
    /// The CST schema of the document.
    pub cst_schema: Schema,
    /// The base IRI the document was read with.
    pub base: Option<String>,
    /// The graph the document was read as.
    pub graph: GInstance,
    /// The occurrences of each term node, by node id.
    pub occurrences: BTreeMap<u32, TermOccurrences>,
}

/// The `jsonld` graph codec: reads JSON-LD into the RDF graph encoding,
/// writes a graph as canonical JSON-LD, and reads and writes through a
/// format-preserving route.
pub struct JsonLdCodec {
    parser: LanguageParser,
    base: Option<String>,
}

impl JsonLdCodec {
    /// A codec over the bundled tree-sitter JSON grammar, with no base IRI:
    /// a relative IRI the document leaves unresolved is refused.
    ///
    /// # Errors
    ///
    /// Returns [`UnifiedCodecError`] if the JSON grammar is not compiled in
    /// or fails to initialise.
    pub fn new() -> Result<Self, UnifiedCodecError> {
        Ok(Self {
            parser: language_parser("json")?,
            base: None,
        })
    }

    /// This codec, reading with `base` as the document's base IRI (its
    /// retrieval IRI, the API's `documentUrl`).
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

    /// The base IRI a document is read with, if any.
    #[must_use]
    pub fn base(&self) -> Option<&str> {
        self.base.as_deref()
    }

    fn json(&self, input: &[u8]) -> Result<(Json, Vec<String>, Schema), RdfReadError> {
        let text = std::str::from_utf8(input)
            .map_err(|e| RdfReadError::new(e.valid_up_to(), RdfReadErrorKind::NotUtf8))?;
        let cst = self
            .parser
            .parse(input, "input.jsonld")
            .map_err(|_| syntax(0))?;
        let tree = Tree { cst: &cst };
        if let Some(offset) = tree.first_recovery() {
            return Err(syntax(offset));
        }
        if let Some(comment) = tree.all_of_kind("comment").first() {
            return Err(RdfReadError::new(
                comment.start,
                RdfReadErrorKind::NotInFormat {
                    construct: "a comment".into(),
                },
            ));
        }
        let strings: Vec<(usize, usize)> = tree
            .all_of_kind("string")
            .iter()
            .map(|s| (s.start, s.end))
            .collect();
        if let Some((offset, _)) = text.char_indices().find(|&(i, c)| {
            c.is_whitespace()
                && !matches!(c, ' ' | '\t' | '\n' | '\r')
                && !strings.iter().any(|&(s, e)| s <= i && i < e)
        }) {
            return Err(syntax(offset));
        }
        let Some(root) = tree.root() else {
            return Err(syntax(0));
        };
        let values = tree.children(&root);
        let top = match values.as_slice() {
            [top] => *top,
            [] => return Err(syntax(text.len())),
            [_, second, ..] => return Err(syntax(second.start)),
        };
        let mut builder = Builder {
            text,
            tree: &tree,
            tokens: Vec::new(),
        };
        let json = builder.value(&top, Rc::from(""), 1)?;
        let tokens = builder.tokens;
        Ok((json, tokens, cst.clone()))
    }

    fn read_document(&self, input: &[u8]) -> Result<Read, RdfReadError> {
        let (json, tokens, cst) = self.json(input)?;
        let items = Expansion::document(&json, self.base.clone()).map_err(from_fault)?;
        let reading = read_items(&items).map_err(from_fault)?;
        Ok(Read {
            reading,
            tokens,
            cst,
        })
    }

    /// The triples of a JSON-LD document, in the order the walk completes
    /// them, a repeated triple listed each time it is stated.
    ///
    /// # Errors
    ///
    /// Returns [`RdfReadError`] at the first fault: invalid UTF-8, a place
    /// the JSON grammar cannot read, a comment, white space JSON does not
    /// have, a malformed string or number token, nesting deeper than
    /// [`MAX_DEPTH`], and then the first JSON-LD refusal
    /// ([`RdfReadErrorKind::JsonLd`]) in processing order.
    pub fn read(&self, input: &[u8]) -> Result<Vec<Triple>, RdfReadError> {
        self.read_document(input).map(|read| read.reading.triples)
    }

    /// Read a JSON-LD document into the RDF graph encoding, with the
    /// complement that writes it back byte for byte.
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
    ) -> Result<(GInstance, JsonLdComplement), ParseInstanceError> {
        check_schema(schema).map_err(|message| ParseInstanceError::SchemaMismatch {
            protocol: PROTOCOL.into(),
            message,
        })?;
        let read = self
            .read_document(input)
            .map_err(|error| ParseInstanceError::Rdf {
                protocol: PROTOCOL.into(),
                error,
            })?;
        let (graph, index) =
            to_graph_indexed(&read.reading.triples).map_err(|e| ParseInstanceError::Parse {
                protocol: PROTOCOL.into(),
                message: e.to_string(),
            })?;
        let mut occurrences: BTreeMap<u32, TermOccurrences> = BTreeMap::new();
        for (key, token) in read.reading.occurrences {
            let (node, blank) = match &key {
                TermKey::Iri(iri) => (index.iris.get(iri), false),
                TermKey::Blank(label) => (index.blanks.get(label), true),
                TermKey::Literal(literal) => (index.literals.get(literal), false),
                TermKey::Language(tag) => (index.languages.get(tag), false),
            };
            let Some(&node) = node else { continue };
            let entry = occurrences.entry(node).or_default();
            match token {
                None => entry.implicit = true,
                Some((token, spelling)) => entry.tokens.push(TokenOccurrence {
                    vertex: read.tokens[token].clone(),
                    token: match (spelling, &key) {
                        (Spelling::Reference, _) if blank => TokenKind::Blank,
                        (Spelling::Reference, _) => TokenKind::Iri,
                        (Spelling::Lexical, _) => TokenKind::QuotedString,
                        (Spelling::Native, TermKey::Literal(literal)) => TokenKind::Bare {
                            datatype: literal.datatype.clone(),
                        },
                        (Spelling::Native | Spelling::Tag, _) => TokenKind::LanguageTag,
                    },
                }),
            }
        }
        Ok((
            graph.clone(),
            JsonLdComplement {
                cst_schema: read.cst,
                base: self.base.clone(),
                graph,
                occurrences,
            },
        ))
    }

    /// Write `graph` back into the document `complement` preserves.
    ///
    /// An unedited graph is written as the document's bytes. An edit to the
    /// value of a term node rewrites each token that writes the term and
    /// nothing else: an IRI as a JSON string holding the absolute IRI, a
    /// blank node as a JSON string holding `_:` and its label, a lexical
    /// form or a language tag as a JSON string holding it, and a lexical
    /// form written as a JSON number or boolean as the bare token that
    /// converts to it. The rewritten document is read again, and the edit is
    /// written only when it reads back as the edited graph.
    ///
    /// # Errors
    ///
    /// Returns [`EmitInstanceError::Rdf`] carrying an [`RdfWriteError`]
    /// that names the node at fault: `StructuralEdit` for a graph that
    /// differs from the one read in more than node values, any refusal of
    /// [`from_graph`] for a graph no document reads back as, `ImplicitTerm`
    /// for a changed term the document writes without a token in some
    /// place, `NativeLexical` for a lexical form no JSON number or boolean
    /// in that place converts to, and `ReadBackDiffers` for an edit whose
    /// rewritten document reads back as another graph. Returns
    /// [`EmitInstanceError::Emit`] when the CST cannot be emitted.
    pub fn emit_graph_preserving(
        &self,
        schema: &Schema,
        graph: &GInstance,
        complement: &JsonLdComplement,
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
        let edited = from_graph(graph).map_err(refuse)?;
        let mut cst = complement.cst_schema.clone();
        let mut ids: Vec<u32> = graph.nodes().keys().copied().collect();
        ids.sort_unstable();
        let mut first_changed = None;
        for id in ids {
            if graph.nodes()[&id].value == complement.graph.nodes()[&id].value {
                continue;
            }
            first_changed.get_or_insert(id);
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
        let bytes = self
            .parser
            .emit(&cst)
            .map_err(|e| EmitInstanceError::Emit {
                protocol: PROTOCOL.into(),
                message: e.to_string(),
            })?;
        if let Some(id) = first_changed {
            let expected: BTreeSet<Triple> = edited.into_iter().collect();
            let reads_back = self
                .read(&bytes)
                .is_ok_and(|triples| triples.into_iter().collect::<BTreeSet<_>>() == expected);
            if !reads_back {
                return Err(refuse(RdfWriteError::new(
                    id,
                    RdfWriteErrorKind::ReadBackDiffers,
                )));
            }
        }
        Ok(bytes)
    }
}

/// The JSON text that writes `node`'s new value in place of a token of
/// kind `token`.
fn replacement(graph: &GInstance, id: u32, token: &TokenKind) -> Result<String, RdfWriteError> {
    let value = match &graph.nodes()[&id].value {
        Some(FieldPresence::Present(Value::Str(value))) => value.as_str(),
        _ => return Err(RdfWriteError::new(id, RdfWriteErrorKind::Value)),
    };
    let mut out = String::new();
    match token {
        TokenKind::Blank => push_json_string(&mut out, &format!("_:{value}")),
        TokenKind::Bare { datatype } => {
            let fits = if datatype == XSD_BOOLEAN || matches!(value, "true" | "false") {
                matches!(value, "true" | "false")
            } else {
                number_literal(value, Some(datatype)).is_some_and(|l| l.lexical == value)
            };
            if !fits {
                return Err(RdfWriteError::new(id, RdfWriteErrorKind::NativeLexical));
            }
            out.push_str(value);
        }
        _ => push_json_string(&mut out, value),
    }
    Ok(out)
}

fn check_schema(schema: &Schema) -> Result<(), String> {
    if schema_holds_encoding(schema) {
        Ok(())
    } else {
        Err("the schema is not the RDF graph encoding".into())
    }
}

impl GraphInstanceParser for JsonLdCodec {
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

impl GraphInstanceEmitter for JsonLdCodec {
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
        let refuse = |error| EmitInstanceError::Rdf {
            protocol: PROTOCOL.into(),
            error,
        };
        let triples = from_graph(instance).map_err(refuse)?;
        let mut ids: Vec<u32> = instance.nodes().keys().copied().collect();
        ids.sort_unstable();
        for id in ids {
            let node = &instance.nodes()[&id];
            if node.anchor.as_ref() == IRI
                && matches!(&node.value, Some(FieldPresence::Present(Value::Str(iri))) if iri.matches('#').count() > 1)
            {
                return Err(refuse(RdfWriteError::new(
                    id,
                    RdfWriteErrorKind::IriFragments,
                )));
            }
        }
        Ok(write(&triples).into_bytes())
    }
}
