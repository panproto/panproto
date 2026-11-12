//! JSON-LD (W3C Recommendation JSON-LD 1.1 and JSON-LD 1.1 Processing
//! Algorithms and API, 16 July 2020).
//!
//! # Reading
//!
//! A JSON-LD document is JSON, read through the bundled tree-sitter JSON
//! grammar, and it denotes the triples the *Expansion* algorithm (API Sec
//! 5.1) and the *Deserialize JSON-LD to RDF* algorithm (API Sec 8.1) give
//! it, with the processing mode pinned to [`PROCESSING_MODE`]. The reader
//! performs no network access: every context is inline in the document.
//!
//! The reader carries this subset of the language:
//!
//! - contexts: an inline `@context` object, an array of them and `null`,
//!   wherever a node object holds one; `@base`, `@vocab`, `@language`,
//!   `@version` (which must be `1.1`); term definitions as a string, `null`
//!   or a map with `@id`, `@reverse`, `@type` (an IRI, `@id`, `@vocab` or
//!   `@none`), `@container` (`@list` or `@set`), `@language` and `@prefix`;
//!   compact IRIs, keyword aliases and the `@type` term's `@set` container;
//! - node objects with `@id`, `@type`, properties, `@reverse` and a
//!   document's top-level `@graph`; value objects with `@value`, `@type`
//!   and `@language`; `@list` (lists of lists included) and `@set`;
//! - blank node identifiers, and JSON strings, numbers and booleans as
//!   values.
//!
//! The standard drops, and so does the reader: a key that expands to no
//! IRI, blank node identifier or keyword (Expansion step 13.3); a `null`
//! value and a value object whose `@value` is `null`; a map holding only
//! `@language`. A node object with no property denotes no triple.
//!
//! Everything outside the subset is refused with
//! [`RdfReadErrorKind::JsonLd`](super::RdfReadErrorKind::JsonLd), which
//! carries a [`JsonLdRefusal`] and the JSON Pointer (RFC 6901) of the value
//! at fault, and the error's offset is that value's first byte. Where the
//! standard would drop data silently, the reader refuses instead: a
//! free-floating value (a scalar, value object or list at the top level or
//! in the top-level `@graph`), a named graph, a property that is a blank
//! node identifier, an IRI that is relative, holds a character no IRI may
//! hold or holds a second `#`, a malformed language tag, and an `@id` or
//! `@type` value that has the form of a keyword but is none. An object that
//! repeats a key is refused as well, since JSON leaves its meaning open.
//!
//! # Numbers
//!
//! A JSON number denotes the IEEE 754 binary64 value nearest its decimal
//! text (the reader refuses one that overflows). Following API Sec 8.6, a
//! value with no fractional part whose magnitude is below 10^21, not typed
//! `xsd:double`, is written as the decimal integer it is (`xsd:integer`
//! unless typed otherwise; negative zero is `0`); any other value is written
//! in the canonical form of `xsd:double` the API's snippet
//! `toExponential(15)` gives: the value rounded to 16 significant digits,
//! a tie rounded away from zero, as a mantissa with one non-zero digit
//! before the point and its trailing zeros removed (at least one digit
//! after the point), `E`, and the exponent without a plus sign or leading
//! zeros (`xsd:double` unless typed otherwise; zero is `0.0E0`).
//!
//! # Canonical output
//!
//! [`write`](fn@write) prints triples as JSON-LD in expanded form with no
//! context: a JSON array holding one node object per subject, the subjects
//! in ascending code point order of their N-Triples text, each object on
//! its own line indented by two spaces and written as `{"@id": ...}`
//! followed by its predicates in ascending code point order, each with an
//! array of its objects in ascending order of their N-Triples text. An IRI
//! object is written `{"@id": iri}`, a blank node `{"@id": "_:label"}`, a
//! literal `{"@value": lexical}` with `"@language"` for a language-tagged
//! string and `"@type"` for a datatype other than `xsd:string`. A string
//! escapes `"`, `\`, backspace, form feed, line feed, carriage return and
//! tab as `\"`, `\\`, `\b`, `\f`, `\n`, `\r` and `\t`, every other
//! character below U+0020 as `\u` and four lowercase hexadecimal digits,
//! and writes every other character as itself. An empty graph is `[]`.

use std::collections::BTreeMap;
use std::fmt::{self, Write as _};

use panproto_protocols::rdf::{RDF_LANG_STRING, XSD_STRING};

use super::ntriples::push_term;
use super::{Literal, Term, Triple};
use crate::rdf::turtle::{XSD_BOOLEAN, XSD_DOUBLE, XSD_INTEGER};

#[cfg(feature = "tree-sitter")]
pub(crate) mod expand;
#[cfg(feature = "tree-sitter")]
pub(crate) mod json;
#[cfg(feature = "tree-sitter")]
mod reader;
#[cfg(feature = "tree-sitter")]
pub(crate) mod to_rdf;

#[cfg(feature = "tree-sitter")]
pub use reader::{JsonLdCodec, JsonLdComplement};

/// The processing mode the reader runs in.
pub const PROCESSING_MODE: &str = "json-ld-1.1";

/// An error the JSON-LD 1.1 API names (Sec 9.4.2), by its code.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum JsonLdErrorCode {
    /// `colliding keywords`.
    CollidingKeywords,
    /// `cyclic IRI mapping`.
    CyclicIriMapping,
    /// `invalid @id value`.
    InvalidIdValue,
    /// `invalid @prefix value`.
    InvalidPrefixValue,
    /// `invalid @reverse value`.
    InvalidReverseValue,
    /// `invalid @version value`.
    InvalidVersionValue,
    /// `invalid base IRI`.
    InvalidBaseIri,
    /// `invalid container mapping`.
    InvalidContainerMapping,
    /// `invalid default language`.
    InvalidDefaultLanguage,
    /// `invalid IRI mapping`.
    InvalidIriMapping,
    /// `invalid keyword alias`.
    InvalidKeywordAlias,
    /// `invalid language mapping`.
    InvalidLanguageMapping,
    /// `invalid language-tagged string`.
    InvalidLanguageTaggedString,
    /// `invalid language-tagged value`.
    InvalidLanguageTaggedValue,
    /// `invalid local context`.
    InvalidLocalContext,
    /// `invalid reverse property`.
    InvalidReverseProperty,
    /// `invalid reverse property map`.
    InvalidReversePropertyMap,
    /// `invalid reverse property value`.
    InvalidReversePropertyValue,
    /// `invalid set or list object`.
    InvalidSetOrListObject,
    /// `invalid term definition`.
    InvalidTermDefinition,
    /// `invalid type mapping`.
    InvalidTypeMapping,
    /// `invalid type value`.
    InvalidTypeValue,
    /// `invalid typed value`.
    InvalidTypedValue,
    /// `invalid value object`.
    InvalidValueObject,
    /// `invalid value object value`.
    InvalidValueObjectValue,
    /// `invalid vocab mapping`.
    InvalidVocabMapping,
    /// `keyword redefinition`.
    KeywordRedefinition,
}

impl JsonLdErrorCode {
    /// The code as the API spells it.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::CollidingKeywords => "colliding keywords",
            Self::CyclicIriMapping => "cyclic IRI mapping",
            Self::InvalidIdValue => "invalid @id value",
            Self::InvalidPrefixValue => "invalid @prefix value",
            Self::InvalidReverseValue => "invalid @reverse value",
            Self::InvalidVersionValue => "invalid @version value",
            Self::InvalidBaseIri => "invalid base IRI",
            Self::InvalidContainerMapping => "invalid container mapping",
            Self::InvalidDefaultLanguage => "invalid default language",
            Self::InvalidIriMapping => "invalid IRI mapping",
            Self::InvalidKeywordAlias => "invalid keyword alias",
            Self::InvalidLanguageMapping => "invalid language mapping",
            Self::InvalidLanguageTaggedString => "invalid language-tagged string",
            Self::InvalidLanguageTaggedValue => "invalid language-tagged value",
            Self::InvalidLocalContext => "invalid local context",
            Self::InvalidReverseProperty => "invalid reverse property",
            Self::InvalidReversePropertyMap => "invalid reverse property map",
            Self::InvalidReversePropertyValue => "invalid reverse property value",
            Self::InvalidSetOrListObject => "invalid set or list object",
            Self::InvalidTermDefinition => "invalid term definition",
            Self::InvalidTypeMapping => "invalid type mapping",
            Self::InvalidTypeValue => "invalid type value",
            Self::InvalidTypedValue => "invalid typed value",
            Self::InvalidValueObject => "invalid value object",
            Self::InvalidValueObjectValue => "invalid value object value",
            Self::InvalidVocabMapping => "invalid vocab mapping",
            Self::KeywordRedefinition => "keyword redefinition",
        }
    }
}

impl fmt::Display for JsonLdErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code())
    }
}

/// Why the JSON-LD reader refuses a document.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum JsonLdRefusal {
    /// An error the API defines.
    Error {
        /// The error code.
        code: JsonLdErrorCode,
    },
    /// A feature outside the subset the reader carries.
    Unsupported {
        /// The feature: a keyword, `remote context`, `scoped context` or
        /// `@container` followed by the container keyword.
        feature: String,
    },
    /// An object member whose key an earlier member of the same object has.
    DuplicateKey {
        /// The key.
        key: String,
    },
    /// A scalar, value object or list at the top level or in the top-level
    /// `@graph`, which the standard drops.
    FreeFloating,
    /// A graph object other than the document's top-level `@graph`: a named
    /// graph, which the RDF graph encoding does not carry.
    NamedGraph,
    /// A property that is a blank node identifier, which an RDF triple
    /// cannot hold.
    BlankPredicate,
    /// An `@id` or `@type` value of the form `@` and letters that is not a
    /// keyword, which the standard ignores.
    ReservedKeyword {
        /// The value.
        value: String,
    },
    /// An IRI that is not absolute.
    RelativeIri,
    /// An IRI holding a character no IRI may hold.
    IriCharacter {
        /// The character.
        character: char,
    },
    /// An IRI holding more than one `#`, which RFC 3987 does not admit.
    MalformedIri,
    /// A language tag that is not an RDF `LANGTAG`.
    InvalidLanguageTag,
    /// A JSON number whose nearest binary64 value is infinite.
    NumberOutOfRange,
}

impl fmt::Display for JsonLdRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Error { code } => write!(f, "JSON-LD error: {code}"),
            Self::Unsupported { feature } => {
                write!(
                    f,
                    "{feature} is outside the JSON-LD subset the reader carries"
                )
            }
            Self::DuplicateKey { key } => write!(f, "the key {key:?} is repeated"),
            Self::FreeFloating => f.write_str("a free-floating value, which the standard drops"),
            Self::NamedGraph => {
                f.write_str("a named graph, which the RDF graph encoding does not carry")
            }
            Self::BlankPredicate => f.write_str("a property that is a blank node identifier"),
            Self::ReservedKeyword { value } => {
                write!(f, "{value:?} has the form of a keyword but is none")
            }
            Self::RelativeIri => f.write_str("the IRI is not absolute"),
            Self::IriCharacter { character } => write!(
                f,
                "an IRI cannot hold the character U+{:04X}",
                u32::from(*character)
            ),
            Self::MalformedIri => f.write_str("the IRI holds more than one '#'"),
            Self::InvalidLanguageTag => f.write_str("the language tag is malformed"),
            Self::NumberOutOfRange => f.write_str("the number overflows binary64"),
        }
    }
}

/// Whether `token` is a JSON number (RFC 8259 Sec 6): an optional minus,
/// an integer part with no leading zero, an optional fraction and an
/// optional exponent.
#[must_use]
pub fn is_json_number(token: &str) -> bool {
    fn digits(s: &str) -> usize {
        s.bytes().take_while(u8::is_ascii_digit).count()
    }
    let rest = token.strip_prefix('-').unwrap_or(token);
    let int = digits(rest);
    if int == 0 || (int > 1 && rest.starts_with('0')) {
        return false;
    }
    let mut rest = &rest[int..];
    if let Some(after) = rest.strip_prefix('.') {
        let n = digits(after);
        if n == 0 {
            return false;
        }
        rest = &after[n..];
    }
    if let Some(after) = rest.strip_prefix(['e', 'E']) {
        let after = after.strip_prefix(['+', '-']).unwrap_or(after);
        let n = digits(after);
        if n == 0 {
            return false;
        }
        rest = &after[n..];
    }
    rest.is_empty()
}

/// The canonical `xsd:double` form of a finite `value`: the API's
/// `toExponential(15)` with trailing zeros of the mantissa removed.
fn double_lexical(value: f64) -> String {
    if value == 0.0 {
        return "0.0E0".to_owned();
    }
    // Rust prints the exact decimal expansion of a binary64 value when the
    // precision is at least its length (at most 767 significant digits).
    let exact = format!("{:.800e}", value.abs());
    let (mantissa, exponent) = exact.split_once('e').unwrap_or((&exact, "0"));
    let mut exponent: i32 = exponent.parse().unwrap_or(0);
    let mut digits: Vec<u8> = mantissa
        .bytes()
        .filter(u8::is_ascii_digit)
        .map(|b| b - b'0')
        .collect();
    let round_up = digits.get(16).is_some_and(|&d| d >= 5);
    digits.truncate(16);
    digits.resize(16, 0);
    if round_up {
        let mut i = 15;
        loop {
            if digits[i] == 9 {
                digits[i] = 0;
                if i == 0 {
                    digits.insert(0, 1);
                    digits.truncate(16);
                    exponent += 1;
                    break;
                }
                i -= 1;
            } else {
                digits[i] += 1;
                break;
            }
        }
    }
    while digits.len() > 2 && digits.last() == Some(&0) {
        digits.pop();
    }
    let mut out = String::new();
    if value < 0.0 {
        out.push('-');
    }
    out.push(char::from(b'0' + digits[0]));
    out.push('.');
    for &d in &digits[1..] {
        out.push(char::from(b'0' + d));
    }
    let _ = write!(out, "E{exponent}");
    out
}

/// The literal a JSON number denotes, typed `datatype` when it is given
/// (the module documentation states the rule), or `None` when `token` is
/// not a JSON number or overflows binary64.
#[must_use]
pub fn number_literal(token: &str, datatype: Option<&str>) -> Option<Literal> {
    if !is_json_number(token) {
        return None;
    }
    let value: f64 = token.parse().ok()?;
    if !value.is_finite() {
        return None;
    }
    let (lexical, default) =
        if value.fract() != 0.0 || value.abs() >= 1e21 || datatype == Some(XSD_DOUBLE) {
            (double_lexical(value), XSD_DOUBLE)
        } else if value == 0.0 {
            ("0".to_owned(), XSD_INTEGER)
        } else {
            (format!("{value:.0}"), XSD_INTEGER)
        };
    Some(Literal {
        lexical,
        datatype: datatype.unwrap_or(default).to_owned(),
        language: None,
    })
}

/// The literal a JSON boolean denotes, typed `datatype` when it is given.
#[must_use]
pub fn boolean_literal(value: bool, datatype: Option<&str>) -> Literal {
    Literal {
        lexical: if value { "true" } else { "false" }.to_owned(),
        datatype: datatype.unwrap_or(XSD_BOOLEAN).to_owned(),
        language: None,
    }
}

/// Append `text` as a JSON string, quotes included, with the escapes of
/// the canonical form.
pub fn push_json_string(out: &mut String, text: &str) {
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{8}' => out.push_str("\\b"),
            '\u{C}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{0}'..='\u{1F}' => {
                let _ = write!(out, "\\u{:04x}", u32::from(c));
            }
            _ => out.push(c),
        }
    }
    out.push('"');
}

/// The N-Triples text of `term`, the sort key of the canonical form.
fn term_text(term: &Term) -> String {
    let mut out = String::new();
    push_term(&mut out, term);
    out
}

/// Append the canonical node reference or value object of `term`.
fn push_object(out: &mut String, term: &Term) {
    match term {
        Term::Iri(iri) => {
            out.push_str("{\"@id\": ");
            push_json_string(out, iri);
            out.push('}');
        }
        Term::Blank(label) => {
            out.push_str("{\"@id\": ");
            push_json_string(out, &format!("_:{label}"));
            out.push('}');
        }
        Term::Literal(literal) => {
            out.push_str("{\"@value\": ");
            push_json_string(out, &literal.lexical);
            if let Some(tag) = &literal.language {
                out.push_str(", \"@language\": ");
                push_json_string(out, tag);
            } else if literal.datatype != XSD_STRING && literal.datatype != RDF_LANG_STRING {
                out.push_str(", \"@type\": ");
                push_json_string(out, &literal.datatype);
            }
            out.push('}');
        }
    }
}

/// A subject's objects by predicate, each keyed by its N-Triples text.
type Predicates<'t> = BTreeMap<&'t str, BTreeMap<String, &'t Term>>;

/// The canonical JSON-LD document of `triples`, each written once.
#[must_use]
pub fn write(triples: &[Triple]) -> String {
    let mut subjects: BTreeMap<String, (&Term, Predicates<'_>)> = BTreeMap::new();
    for triple in triples {
        subjects
            .entry(term_text(&triple.subject))
            .or_insert_with(|| (&triple.subject, BTreeMap::new()))
            .1
            .entry(triple.predicate.as_str())
            .or_default()
            .insert(term_text(&triple.object), &triple.object);
    }
    if subjects.is_empty() {
        return "[]\n".to_owned();
    }
    let mut out = String::from("[\n");
    for (i, (subject, predicates)) in subjects.values().enumerate() {
        out.push_str(if i == 0 {
            "  {\"@id\": "
        } else {
            ",\n  {\"@id\": "
        });
        match subject {
            Term::Blank(label) => push_json_string(&mut out, &format!("_:{label}")),
            Term::Iri(iri) => push_json_string(&mut out, iri),
            Term::Literal(literal) => push_json_string(&mut out, &literal.lexical),
        }
        for (predicate, objects) in predicates {
            out.push_str(", ");
            push_json_string(&mut out, predicate);
            out.push_str(": [");
            for (j, object) in objects.values().enumerate() {
                if j > 0 {
                    out.push_str(", ");
                }
                push_object(&mut out, object);
            }
            out.push(']');
        }
        out.push('}');
    }
    out.push_str("\n]\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lexical(token: &str) -> String {
        number_literal(token, None)
            .map(|l| l.lexical)
            .unwrap_or_default()
    }

    #[test]
    fn integers_below_ten_to_the_twenty_one_are_written_as_integers() {
        assert_eq!(lexical("0"), "0");
        assert_eq!(lexical("-0"), "0");
        assert_eq!(lexical("1.0"), "1");
        assert_eq!(lexical("-42"), "-42");
        assert_eq!(lexical("9007199254740993"), "9007199254740992");
        assert_eq!(lexical("999999999999999999999"), "1.0E21");
        assert_eq!(lexical("999999999999999868928"), "999999999999999868928");
        assert_eq!(lexical("1e21"), "1.0E21");
        assert_eq!(lexical("1e20"), "100000000000000000000");
    }

    #[test]
    fn doubles_round_to_sixteen_digits_with_ties_away_from_zero() {
        assert_eq!(lexical("0.1"), "1.0E-1");
        assert_eq!(lexical("1.5"), "1.5E0");
        assert_eq!(lexical("-1.5e-7"), "-1.5E-7");
        assert_eq!(lexical("5e-324"), "4.940656458412465E-324");
        assert_eq!(lexical("1.0000152587890625"), "1.000015258789063E0");
        assert_eq!(lexical("1.7976931348623157e308"), "1.797693134862316E308");
        assert_eq!(lexical("0.9999999999999999"), "9.999999999999999E-1");
        assert_eq!(
            number_literal("5", Some(XSD_DOUBLE)).map(|l| l.lexical),
            Some("5.0E0".to_owned())
        );
        assert_eq!(number_literal("1e400", None), None);
        assert_eq!(number_literal("01", None), None);
    }
}
