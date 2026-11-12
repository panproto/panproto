//! N-Triples (W3C Recommendation RDF 1.1 N-Triples, 25 February 2014).
//!
//! # Reading
//!
//! The reader parses the document with the tree-sitter Turtle grammar,
//! since N-Triples is a subset of Turtle, and walks the tree, refusing what
//! the grammar admits but N-Triples does not: directives, prefixed names,
//! `a`, relative IRIs, numbers and booleans, single-quoted and long strings,
//! blank node property lists, collections, predicate and object lists, a
//! triple that spans a line break, and two triples on one line. Every
//! refusal is an [`RdfReadError`] naming the byte offset of the token at
//! fault. The document denotes the set of its triples, which [`to_graph`]
//! encodes.
//!
//! Beyond the grammar of the Recommendation, an IRI whose escapes decode to
//! a character `IRIREF` excludes (a space, a control character, or one of
//! `<>"{}|^` and backquote and backslash) is refused: no IRI holds one, and
//! the canonical form could not write it back.
//!
//! # Canonical output
//!
//! [`write`](fn@write) prints a graph's triples as canonical N-Triples: one triple per
//! line in ascending code point order of the lines, each line its subject,
//! predicate and object followed by ` .` and a line feed, the terms
//! separated by one space. An IRI is written between angle brackets with no
//! escape; a blank node as `_:` and its label as read (blank nodes are not
//! relabelled, so two graphs that differ only in labels are written
//! differently); a literal with datatype `xsd:string` with no datatype, a
//! language-tagged literal with its tag as read (its case is kept), and
//! every other literal with `^^` and its datatype IRI. A string escapes
//! backspace, tab, line feed, form feed, carriage return, `"` and `\` as
//! `\b`, `\t`, `\n`, `\f`, `\r`, `\"` and `\\`, writes U+0000 to U+0007,
//! U+000B, U+000E to U+001F, U+007F, U+FFFE and U+FFFF as `\u` and four
//! uppercase hexadecimal digits, and writes every other character as
//! itself. These string rules are those of the canonical form in the RDF
//! 1.2 N-Triples Working Draft; unlike that draft, language tags are not
//! lowercased, since this reader keeps them as written.
//!
//! [`to_graph`]: super::to_graph

use std::fmt::Write as _;

use panproto_protocols::rdf::XSD_STRING;

use super::{Literal, RdfReadError, Term, Triple};

#[cfg(feature = "tree-sitter")]
pub use reader::NTriplesCodec;

/// Append `text` as the body of an N-Triples string literal.
pub(super) fn push_string(out: &mut String, text: &str) {
    for c in text.chars() {
        match c {
            '\u{8}' => out.push_str("\\b"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\u{C}' => out.push_str("\\f"),
            '\r' => out.push_str("\\r"),
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{0}'..='\u{7}'
            | '\u{B}'
            | '\u{E}'..='\u{1F}'
            | '\u{7F}'
            | '\u{FFFE}'
            | '\u{FFFF}' => {
                let _ = write!(out, "\\u{:04X}", u32::from(c));
            }
            _ => out.push(c),
        }
    }
}

pub(super) fn push_term(out: &mut String, term: &Term) {
    match term {
        Term::Iri(iri) => {
            out.push('<');
            out.push_str(iri);
            out.push('>');
        }
        Term::Blank(label) => {
            out.push_str("_:");
            out.push_str(label);
        }
        Term::Literal(Literal {
            lexical,
            datatype,
            language,
        }) => {
            out.push('"');
            push_string(out, lexical);
            out.push('"');
            if let Some(tag) = language {
                out.push('@');
                out.push_str(tag);
            } else if datatype != XSD_STRING {
                out.push_str("^^<");
                out.push_str(datatype);
                out.push('>');
            }
        }
    }
}

/// The canonical N-Triples line of `triple`, with its line feed.
#[must_use]
pub fn line(triple: &Triple) -> String {
    let mut out = String::new();
    push_term(&mut out, &triple.subject);
    out.push_str(" <");
    out.push_str(&triple.predicate);
    out.push_str("> ");
    push_term(&mut out, &triple.object);
    out.push_str(" .\n");
    out
}

/// The canonical N-Triples document of `triples`: their lines in ascending
/// code point order, each written once.
#[must_use]
pub fn write(triples: &[Triple]) -> String {
    let mut lines: Vec<String> = triples.iter().map(line).collect();
    lines.sort_unstable();
    lines.dedup();
    lines.concat()
}

/// Decode the escapes of a string literal's body (between its quotes),
/// which starts at byte `offset` of the document.
///
/// # Errors
///
/// Returns [`RdfReadError`] at the escape at fault for an undefined escape
/// or a numeric escape naming no scalar value, and at a raw character a
/// `STRING_LITERAL_QUOTE` excludes (`"`, `\`, line feed, carriage return).
pub fn decode_string(body: &str, offset: usize) -> Result<String, RdfReadError> {
    decode(body, offset, true, &['"', '\n', '\r'])
}

/// Decode the escapes of an IRI's body (between its angle brackets), which
/// starts at byte `offset` of the document, and check the IRI.
///
/// # Errors
///
/// Returns [`RdfReadError`] for an escape other than a numeric one, a
/// numeric escape naming no scalar value, a character no IRI holds (raw or
/// escaped), and an IRI that is not absolute.
pub fn decode_iri(body: &str, offset: usize) -> Result<String, RdfReadError> {
    let iri = decode_iri_reference(body, offset)?;
    if !super::is_absolute_iri(&iri) {
        return Err(RdfReadError::new(
            offset,
            super::RdfReadErrorKind::RelativeIri,
        ));
    }
    Ok(iri)
}

/// Decode the escapes of an IRI reference's body (between its angle
/// brackets), which starts at byte `offset` of the document, without
/// requiring it to be absolute.
///
/// # Errors
///
/// Returns [`RdfReadError`] for an escape other than a numeric one, a
/// numeric escape naming no scalar value, and a character no IRI holds (raw
/// or escaped).
pub(super) fn decode_iri_reference(body: &str, offset: usize) -> Result<String, RdfReadError> {
    let iri = decode(body, offset, false, &[])?;
    if let Some(character) = iri.chars().find(|&c| super::is_iri_excluded(c)) {
        let at = body
            .char_indices()
            .find(|&(_, c)| c == character || c == '\\')
            .map_or(offset, |(i, _)| offset + i);
        return Err(RdfReadError::new(
            at,
            super::RdfReadErrorKind::IriCharacter { character },
        ));
    }
    Ok(iri)
}

/// Decode the escapes of `body`, which starts at byte `offset`: `UCHAR`
/// always, and `ECHAR` when `string` is set; a raw character of `raw` is
/// refused.
pub(super) fn decode(
    body: &str,
    offset: usize,
    string: bool,
    raw: &[char],
) -> Result<String, RdfReadError> {
    use super::RdfReadErrorKind::{InvalidEscape, NotAScalarValue, UnescapedCharacter};
    let mut out = String::with_capacity(body.len());
    let mut chars = body.char_indices();
    while let Some((i, c)) = chars.next() {
        if c != '\\' {
            if raw.contains(&c) {
                return Err(RdfReadError::new(
                    offset + i,
                    UnescapedCharacter { character: c },
                ));
            }
            out.push(c);
            continue;
        }
        let at = offset + i;
        let escape = chars
            .next()
            .ok_or_else(|| RdfReadError::new(at, InvalidEscape))?
            .1;
        let digits = match escape {
            'u' => 4,
            'U' => 8,
            _ if string => {
                out.push(match escape {
                    't' => '\t',
                    'b' => '\u{8}',
                    'n' => '\n',
                    'r' => '\r',
                    'f' => '\u{C}',
                    '"' => '"',
                    '\'' => '\'',
                    '\\' => '\\',
                    _ => return Err(RdfReadError::new(at, InvalidEscape)),
                });
                continue;
            }
            _ => return Err(RdfReadError::new(at, InvalidEscape)),
        };
        let mut code_point: u32 = 0;
        for _ in 0..digits {
            let digit = chars
                .next()
                .and_then(|(_, d)| d.to_digit(16))
                .ok_or_else(|| RdfReadError::new(at, InvalidEscape))?;
            code_point = code_point * 16 + digit;
        }
        out.push(
            char::from_u32(code_point)
                .ok_or_else(|| RdfReadError::new(at, NotAScalarValue { code_point }))?,
        );
    }
    Ok(out)
}

#[cfg(feature = "tree-sitter")]
mod reader;
