//! The N-Triples reader and the `ntriples` graph codec.

use panproto_inst::GInstance;
use panproto_parse::languages::common::LanguageParser;
use panproto_parse::registry::AstParser;
use panproto_protocols::rdf::{RDF_LANG_STRING, XSD_STRING};
use panproto_schema::Schema;

use super::super::{
    Literal, RdfReadError, RdfReadErrorKind, Term, Triple, from_graph, is_blank_label,
    is_language_tag, schema_holds_encoding, to_graph,
};
use super::{decode_iri, decode_string, write};
use crate::error::{EmitInstanceError, ParseInstanceError, UnifiedCodecError};
use crate::traits::{GraphInstanceEmitter, GraphInstanceParser};
use crate::unified_codec::language_parser;

/// The protocol name the codec registers under.
const PROTOCOL: &str = "ntriples";

/// A tree-sitter tree read out of the CST schema: each node's kind, byte
/// span and children in source order.
struct Tree<'a> {
    cst: &'a Schema,
}

/// One node of the tree.
#[derive(Clone, Copy)]
struct Cursor<'a> {
    name: &'a str,
    kind: &'a str,
    start: usize,
    end: usize,
}

impl<'a> Tree<'a> {
    fn byte(&self, name: &str, sort: &str) -> usize {
        self.cst
            .constraints
            .get(name)
            .and_then(|cs| cs.iter().find(|c| c.sort.as_ref() == sort))
            .and_then(|c| c.value.parse().ok())
            .unwrap_or(0)
    }

    fn cursor(&self, name: &'a str) -> Cursor<'a> {
        Cursor {
            name,
            kind: self.cst.vertices.get(name).map_or("", |v| v.kind.as_ref()),
            start: self.byte(name, "start-byte"),
            end: self.byte(name, "end-byte"),
        }
    }

    fn children(&self, parent: &Cursor<'a>) -> Vec<Cursor<'a>> {
        let mut children: Vec<Cursor<'a>> = self
            .cst
            .outgoing_edges(parent.name)
            .iter()
            .map(|edge| self.cursor(edge.tgt.as_ref()))
            .collect();
        children.sort_by_key(|child| (child.start, child.end));
        children
    }

    /// The only child of `parent`, or a refusal naming what is there instead.
    fn only_child(&self, parent: &Cursor<'a>) -> Result<Cursor<'a>, RdfReadError> {
        match self.children(parent).as_slice() {
            [child] => Ok(*child),
            [_, second, ..] => Err(not_in_format(second)),
            [] => Err(RdfReadError::new(
                parent.start,
                RdfReadErrorKind::NotInFormat {
                    construct: "the keyword `a`".into(),
                },
            )),
        }
    }

    /// The first parser recovery marker, by offset: an `ERROR` node, a node
    /// the parser inserted as missing, or an empty node other than the
    /// document.
    fn first_recovery(&self) -> Option<usize> {
        let markers: Vec<(usize, usize)> = self
            .cst
            .vertices
            .iter()
            .filter(|(name, vertex)| {
                let missing = self
                    .cst
                    .constraints
                    .get(*name)
                    .is_some_and(|cs| cs.iter().any(|c| c.sort.as_ref() == "missing"));
                let cursor = self.cursor(name.as_ref());
                vertex.kind.as_ref() == "ERROR"
                    || missing
                    || (cursor.start == cursor.end && vertex.kind.as_ref() != "document")
            })
            .map(|(name, _)| {
                let cursor = self.cursor(name.as_ref());
                (cursor.start, cursor.end)
            })
            .collect();
        // The outermost marker spans the whole region the parser could not
        // read; the innermost one nested in it is where reading failed.
        let mut current = *markers
            .iter()
            .min_by_key(|(start, end)| (*start, std::cmp::Reverse(*end)))?;
        while let Some(inner) = markers
            .iter()
            .filter(|&&m| m != current && m.0 >= current.0 && m.1 <= current.1)
            .min_by_key(|(start, end)| (*start, std::cmp::Reverse(*end)))
        {
            current = *inner;
        }
        Some(current.0)
    }
}

/// A refusal of the construct at `node`, which N-Triples does not have.
fn not_in_format(node: &Cursor<'_>) -> RdfReadError {
    let construct = match node.kind {
        "directive" | "prefix_id" | "base" | "sparql_prefix" | "sparql_base" => "a directive",
        "prefixed_name" => "a prefixed name",
        "blank_node_property_list" | "anon" => "a blank node property list",
        "collection" | "object_collection" => "a collection",
        "integer" | "decimal" | "double" => "a numeric literal",
        "boolean_literal" => "a boolean literal",
        "property" => "a predicate list",
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

/// Refuse any byte of `text[from..to]` other than a space or a tab.
fn blanks_only(text: &str, from: usize, to: usize) -> Result<(), RdfReadError> {
    text[from..to]
        .find(|c| c != ' ' && c != '\t')
        .map_or(Ok(()), |i| {
            let kind = if matches!(text.as_bytes()[from + i], b'\n' | b'\r') {
                RdfReadErrorKind::LineStructure
            } else {
                RdfReadErrorKind::Syntax
            };
            Err(RdfReadError::new(from + i, kind))
        })
}

/// The reader over one document.
struct Reader<'a> {
    text: &'a str,
    tree: Tree<'a>,
}

impl<'a> Reader<'a> {
    fn slice(&self, node: &Cursor<'_>) -> &'a str {
        &self.text[node.start..node.end]
    }

    fn iri(&self, node: &Cursor<'_>) -> Result<String, RdfReadError> {
        if node.kind != "iri_reference" {
            return Err(not_in_format(node));
        }
        let text = self.slice(node);
        decode_iri(&text[1..text.len() - 1], node.start + 1)
    }

    fn blank(&self, node: &Cursor<'_>) -> Result<String, RdfReadError> {
        let label = &self.slice(node)[2..];
        if is_blank_label(label) {
            Ok(label.to_owned())
        } else {
            Err(RdfReadError::new(
                node.start,
                RdfReadErrorKind::InvalidBlankLabel,
            ))
        }
    }

    fn subject(&self, node: &Cursor<'_>) -> Result<Term, RdfReadError> {
        match node.kind {
            "iri_reference" => Ok(Term::Iri(self.iri(node)?)),
            "blank_node_label" => Ok(Term::Blank(self.blank(node)?)),
            _ => Err(not_in_format(node)),
        }
    }

    fn literal(&self, node: &Cursor<'_>) -> Result<Literal, RdfReadError> {
        let parts = self.tree.children(node);
        let [string, rest @ ..] = parts.as_slice() else {
            return Err(RdfReadError::new(node.start, RdfReadErrorKind::Syntax));
        };
        let text = self.slice(string);
        if text.starts_with('\'') {
            return Err(RdfReadError::new(
                string.start,
                RdfReadErrorKind::NotInFormat {
                    construct: "a single-quoted string".into(),
                },
            ));
        }
        if text.len() > 2 && text.starts_with("\"\"") {
            return Err(RdfReadError::new(
                string.start,
                RdfReadErrorKind::NotInFormat {
                    construct: "a long string".into(),
                },
            ));
        }
        let lexical = decode_string(&text[1..text.len() - 1], string.start + 1)?;
        let (datatype, language) = match rest {
            [] => (XSD_STRING.to_owned(), None),
            [tag] if tag.kind == "lang_tag" => {
                let tag_text = &self.slice(tag)[1..];
                if !is_language_tag(tag_text) {
                    return Err(RdfReadError::new(
                        tag.start,
                        RdfReadErrorKind::InvalidLanguageTag,
                    ));
                }
                blanks_only(self.text, string.end, tag.start)?;
                (RDF_LANG_STRING.to_owned(), Some(tag_text.to_owned()))
            }
            [datatype] => {
                let between = &self.text[string.end..datatype.start];
                let marker = between.find("^^").unwrap_or(0);
                blanks_only(self.text, string.end, string.end + marker)?;
                blanks_only(self.text, string.end + marker + 2, datatype.start)?;
                (self.iri(datatype)?, None)
            }
            [_, extra, ..] => return Err(not_in_format(extra)),
        };
        Ok(Literal {
            lexical,
            datatype,
            language,
        })
    }

    fn object(&self, node: &Cursor<'_>) -> Result<Term, RdfReadError> {
        match node.kind {
            "rdf_literal" => Ok(Term::Literal(self.literal(node)?)),
            _ => self.subject(node),
        }
    }

    /// The triple at `node`, checking that only blanks separate its terms
    /// and that it ends in `.`.
    fn triple(&self, node: &Cursor<'_>) -> Result<Triple, RdfReadError> {
        let parts = self.tree.children(node);
        let [subject, properties] = parts.as_slice() else {
            return Err(parts.iter().find(|p| p.kind != "subject").map_or_else(
                || RdfReadError::new(node.start, RdfReadErrorKind::Syntax),
                not_in_format,
            ));
        };
        if subject.kind != "subject" {
            return Err(not_in_format(subject));
        }
        let subject_term = self.tree.only_child(subject)?;
        let property = self.tree.only_child(properties)?;
        let [predicate, objects] = self.tree.children(&property)[..] else {
            return Err(RdfReadError::new(property.start, RdfReadErrorKind::Syntax));
        };
        let predicate_term = self.tree.only_child(&predicate)?;
        let object_term = self.tree.only_child(&objects)?;

        blanks_only(self.text, subject_term.end, predicate_term.start)?;
        blanks_only(self.text, predicate_term.end, object_term.start)?;
        if !self.text[..node.end].ends_with('.') {
            return Err(RdfReadError::new(node.end, RdfReadErrorKind::Syntax));
        }
        blanks_only(self.text, object_term.end, node.end - 1)?;

        Ok(Triple {
            subject: self.subject(&subject_term)?,
            predicate: self.iri(&predicate_term)?,
            object: self.object(&object_term)?,
        })
    }

    fn document(&self, root: &Cursor<'_>) -> Result<Vec<Triple>, RdfReadError> {
        let mut triples = Vec::new();
        let mut cursor = 0;
        let mut previous_triple_end: Option<usize> = None;
        for child in self.tree.children(root) {
            if let Some(i) = self.text[cursor..child.start]
                .find(|c: char| !matches!(c, ' ' | '\t' | '\n' | '\r'))
            {
                return Err(RdfReadError::new(cursor + i, RdfReadErrorKind::Syntax));
            }
            match child.kind {
                "comment" => {}
                "triple" => {
                    if let Some(end) = previous_triple_end
                        && !self.text[end..child.start].contains(['\n', '\r'])
                    {
                        return Err(RdfReadError::new(
                            child.start,
                            RdfReadErrorKind::LineStructure,
                        ));
                    }
                    triples.push(self.triple(&child)?);
                    previous_triple_end = Some(child.end);
                }
                _ => return Err(not_in_format(&child)),
            }
            cursor = child.end;
        }
        if let Some(i) = self.text[cursor..].find(|c: char| !matches!(c, ' ' | '\t' | '\n' | '\r'))
        {
            return Err(RdfReadError::new(cursor + i, RdfReadErrorKind::Syntax));
        }
        Ok(triples)
    }
}

/// The `ntriples` graph codec: reads N-Triples into the RDF graph encoding
/// and writes a graph in that encoding as canonical N-Triples.
pub struct NTriplesCodec {
    parser: LanguageParser,
}

impl NTriplesCodec {
    /// A codec over the bundled tree-sitter Turtle grammar.
    ///
    /// # Errors
    ///
    /// Returns [`UnifiedCodecError`] if the Turtle grammar is not compiled
    /// in or fails to initialise.
    pub fn new() -> Result<Self, UnifiedCodecError> {
        Ok(Self {
            parser: language_parser("turtle")?,
        })
    }

    /// The triples of an N-Triples document, in document order, a repeated
    /// triple listed each time it appears.
    ///
    /// # Errors
    ///
    /// Returns [`RdfReadError`] naming the byte offset of the first fault
    /// the reader meets: invalid UTF-8, then a raw U+0000 or U+007F (which
    /// the grammar cannot read in a string), then the first parser recovery
    /// marker, then the first fault in document order.
    pub fn read(&self, input: &[u8]) -> Result<Vec<Triple>, RdfReadError> {
        let text = std::str::from_utf8(input)
            .map_err(|e| RdfReadError::new(e.valid_up_to(), RdfReadErrorKind::NotUtf8))?;
        if let Some((offset, character)) = text
            .char_indices()
            .find(|&(_, c)| matches!(c, '\u{0}' | '\u{7F}'))
        {
            return Err(RdfReadError::new(
                offset,
                RdfReadErrorKind::UnsupportedCharacter { character },
            ));
        }
        let cst = self
            .parser
            .parse(input, "input.nt")
            .map_err(|_| RdfReadError::new(0, RdfReadErrorKind::Syntax))?;
        let tree = Tree { cst: &cst };
        if let Some(offset) = tree.first_recovery() {
            return Err(RdfReadError::new(offset, RdfReadErrorKind::Syntax));
        }
        let root = cst
            .vertices
            .iter()
            .find(|(_, v)| v.kind.as_ref() == "document")
            .map(|(name, _)| tree.cursor(name.as_ref()));
        let Some(root) = root else {
            return if text.trim_matches([' ', '\t', '\n', '\r']).is_empty() {
                Ok(Vec::new())
            } else {
                Err(RdfReadError::new(0, RdfReadErrorKind::Syntax))
            };
        };
        Reader { text, tree }.document(&root)
    }
}

fn schema_mismatch() -> String {
    "the schema is not the RDF graph encoding".into()
}

impl GraphInstanceParser for NTriplesCodec {
    fn protocol_name(&self) -> &'static str {
        PROTOCOL
    }

    fn parse_graph(&self, schema: &Schema, input: &[u8]) -> Result<GInstance, ParseInstanceError> {
        if !schema_holds_encoding(schema) {
            return Err(ParseInstanceError::SchemaMismatch {
                protocol: PROTOCOL.into(),
                message: schema_mismatch(),
            });
        }
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

impl GraphInstanceEmitter for NTriplesCodec {
    fn protocol_name(&self) -> &'static str {
        PROTOCOL
    }

    fn emit_graph(
        &self,
        schema: &Schema,
        instance: &GInstance,
    ) -> Result<Vec<u8>, EmitInstanceError> {
        if !schema_holds_encoding(schema) {
            return Err(EmitInstanceError::Emit {
                protocol: PROTOCOL.into(),
                message: schema_mismatch(),
            });
        }
        let triples = from_graph(instance).map_err(|error| EmitInstanceError::Rdf {
            protocol: PROTOCOL.into(),
            error,
        })?;
        Ok(write(&triples).into_bytes())
    }
}
