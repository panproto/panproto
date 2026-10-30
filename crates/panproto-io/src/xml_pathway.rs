//! Shared XML pathway for schema-guided instance parsing via `quick-xml`.
//!
//! Provides a zero-copy pull-parser pathway that reads XML events and
//! builds `WInstance` trees guided by a schema. Used by ~14 protocols
//! (NAF, FoLiA, TEI, TimeML, ELAN, XSD instances, RSS/Atom, DOCX, ODF, etc.).
//!
//! The parser maps XML elements to schema vertices and XML attributes/text
//! content to node values. Element nesting follows schema edges.
//!
//! ## Prop1 waiver list
//!
//! - Emission refuses a text value with
//!   [`LimitExceeded`](crate::error::EmitValueReason::LimitExceeded)
//!   when writing it would exceed the default 64 MiB output-byte allowance.
//!
//! ## Departures from the format standard
//!
//! - A negative canonical NaN is written as `-NaN` so its sign survives an
//!   exact round trip. This spelling is a deliberate departure from XSD 1.1
//!   Part 2, Sections 3.3.4.1-3.3.4.2.

use std::collections::{BTreeMap, HashMap, HashSet};

use panproto_inst::WInstance;
use panproto_inst::metadata::Node;
use panproto_inst::value::coerce::{LexClass, Lexeme};
use panproto_inst::value::{FieldPresence, Value};
use panproto_inst::{NodeShape, ValueFidelityError, ValueSite};
use panproto_schema::value_kinds::NumeralDialect;
use panproto_schema::{Edge, Protocol, ScalarKind, Schema, ValueSpec};
use quick_xml::NsReader;
use quick_xml::events::Event;
use quick_xml::name::{NamespaceResolver, QName, ResolveResult};

use crate::error::{EmitInstanceError, EmitValueError, EmitValueReason, ParseInstanceError};
use crate::json_codec::{missing_entry_message, resolve_entry_vertex};
use crate::json_pathway::{encode_bytes, protocol_definition};

const XML_SCHEMA_INSTANCE_NS: &str = "http://www.w3.org/2001/XMLSchema-instance";
pub(crate) const XML_NS: &str = "http://www.w3.org/XML/1998/namespace";
pub(crate) const XMLNS_NS: &str = "http://www.w3.org/2000/xmlns/";
/// Maximum UTF-8 byte length of one expanded XML entity run.
pub const MAX_XML_ENTITY_EXPANSION_BYTES: usize = 1_048_576;
/// Maximum nesting depth of internal XML general entities.
pub const MAX_XML_ENTITY_DEPTH: usize = 16;
/// Maximum scale emitted for an XML Schema decimal value.
pub const MAX_XML_DECIMAL_FRACTION_DIGITS: usize = 1_048_576;
/// Keep namespace resolution bounded while admitting namespace-heavy machine
/// documents covered by the crate's linear-time regression corpus.
const MAX_XML_NAMESPACE_DECLARATIONS: usize = 4_096;

/// UTF-8 XML text together with its original source-byte offset map.
#[derive(Clone, Debug)]
pub(crate) struct DecodedXmlSource {
    pub(crate) text: String,
    /// `utf8_to_source[n]` is the source byte that produced UTF-8 byte `n`.
    /// The final entry maps the UTF-8 end boundary to the source length.
    utf8_to_source: Vec<u64>,
}

impl DecodedXmlSource {
    pub(crate) fn source_offset(&self, utf8_offset: u64) -> Option<u64> {
        usize::try_from(utf8_offset)
            .ok()
            .and_then(|offset| self.utf8_to_source.get(offset).copied())
    }
}

fn invalid_xml_encoding(_protocol: &str, byte_offset: usize) -> ParseInstanceError {
    ParseInstanceError::Fidelity(vec![ValueFidelityError {
        site: ValueSite::new("", Some(u64::try_from(byte_offset).unwrap_or(u64::MAX))),
        lexeme: String::new(),
        target: ScalarKind::Str,
        reason: panproto_inst::FidelityReason::InvalidEncoding,
    }])
}

fn xml_declared_encoding(text: &str) -> Option<String> {
    let declaration = text.strip_prefix("<?xml")?.split_once("?>")?.0;
    let encoding = declaration.find("encoding")?;
    let mut rest = declaration[encoding + "encoding".len()..].trim_start();
    rest = rest.strip_prefix('=')?.trim_start();
    let quote = rest
        .chars()
        .next()
        .filter(|quote| matches!(quote, '\'' | '"'))?;
    let value = &rest[quote.len_utf8()..];
    let end = value.find(quote)?;
    Some(value[..end].to_ascii_lowercase())
}

fn iso_8859_1_label(label: &str) -> bool {
    matches!(
        label,
        "iso-8859-1" | "iso_8859-1" | "iso_8859-1:1987" | "latin1" | "l1" | "csisolatin1"
    )
}

fn us_ascii_label(label: &str) -> bool {
    matches!(
        label,
        "us-ascii" | "ascii" | "iso-ir-6" | "ansi_x3.4-1968" | "ansi_x3.4-1986"
    )
}

fn utf8_label(label: &str) -> bool {
    matches!(label, "utf-8" | "utf8")
}

fn push_decoded_character(
    text: &mut String,
    utf8_to_source: &mut Vec<u64>,
    character: char,
    source_offset: usize,
) {
    text.push(character);
    let source_offset = u64::try_from(source_offset).unwrap_or(u64::MAX);
    utf8_to_source.extend(std::iter::repeat_n(source_offset, character.len_utf8()));
}

fn decode_utf16_source(
    input: &[u8],
    little_endian: bool,
    protocol: &str,
) -> Result<DecodedXmlSource, ParseInstanceError> {
    if input.len() % 2 != 0 {
        return Err(invalid_xml_encoding(protocol, input.len() - 1));
    }
    let mut text = String::new();
    let mut utf8_to_source = Vec::new();
    let mut offset = 2_usize;
    while offset < input.len() {
        let unit = if little_endian {
            u16::from_le_bytes([input[offset], input[offset + 1]])
        } else {
            u16::from_be_bytes([input[offset], input[offset + 1]])
        };
        if (0xD800..=0xDBFF).contains(&unit) {
            let Some(next_offset) = offset.checked_add(2).filter(|next| next + 1 < input.len())
            else {
                return Err(invalid_xml_encoding(protocol, offset));
            };
            let low = if little_endian {
                u16::from_le_bytes([input[next_offset], input[next_offset + 1]])
            } else {
                u16::from_be_bytes([input[next_offset], input[next_offset + 1]])
            };
            if !(0xDC00..=0xDFFF).contains(&low) {
                return Err(invalid_xml_encoding(protocol, next_offset));
            }
            let scalar = 0x1_0000 + ((u32::from(unit) - 0xD800) << 10) + (u32::from(low) - 0xDC00);
            let character =
                char::from_u32(scalar).ok_or_else(|| invalid_xml_encoding(protocol, offset))?;
            push_decoded_character(&mut text, &mut utf8_to_source, character, offset);
            offset += 4;
        } else if (0xDC00..=0xDFFF).contains(&unit) {
            return Err(invalid_xml_encoding(protocol, offset));
        } else {
            let character = char::from_u32(u32::from(unit))
                .ok_or_else(|| invalid_xml_encoding(protocol, offset))?;
            push_decoded_character(&mut text, &mut utf8_to_source, character, offset);
            offset += 2;
        }
    }
    utf8_to_source.push(u64::try_from(input.len()).unwrap_or(u64::MAX));
    let declared = xml_declared_encoding(&text);
    let expected = if little_endian {
        "utf-16le"
    } else {
        "utf-16be"
    };
    if declared
        .as_deref()
        .is_some_and(|label| !matches!(label, "utf-16") && label != expected)
    {
        return Err(invalid_xml_encoding(protocol, 0));
    }
    Ok(DecodedXmlSource {
        text,
        utf8_to_source,
    })
}

/// Decode the XML encodings whose semantics panproto exposes.
///
/// ISO-8859-1 and US-ASCII are deliberately decoded here rather than through
/// the WHATWG label table, where both labels alias Windows-1252.
pub(crate) fn decode_xml_source(
    input: &[u8],
    protocol: &str,
) -> Result<DecodedXmlSource, ParseInstanceError> {
    if input.starts_with(&[0xFF, 0xFE]) {
        return decode_utf16_source(input, true, protocol);
    }
    if input.starts_with(&[0xFE, 0xFF]) {
        return decode_utf16_source(input, false, protocol);
    }
    if input.starts_with(&[0x00, b'<']) || input.starts_with(&[b'<', 0x00]) {
        return Err(invalid_xml_encoding(protocol, 0));
    }

    let (payload, bom) = input
        .strip_prefix(&[0xEF, 0xBB, 0xBF])
        .map_or((input, false), |payload| (payload, true));
    let declaration_probe_len = payload
        .iter()
        .position(|byte| *byte == b'>')
        .map_or(payload.len(), |offset| offset + 1);
    let declaration_probe: String = payload[..declaration_probe_len]
        .iter()
        .map(|byte| char::from(*byte))
        .collect();
    let label = xml_declared_encoding(&declaration_probe);
    if let Some(label) = label.as_deref()
        && !utf8_label(label)
        && !iso_8859_1_label(label)
        && !us_ascii_label(label)
    {
        return Err(ParseInstanceError::UnsupportedFeature {
            protocol: protocol.to_owned(),
            feature: format!("XML encoding {label:?}"),
            byte_offset: Some(u64::from(bom) * 3),
        });
    }

    let source_base = if bom { 3 } else { 0 };
    let mut text = String::new();
    let mut utf8_to_source = Vec::new();
    if label.as_deref().is_some_and(iso_8859_1_label) {
        for (relative, byte) in payload.iter().copied().enumerate() {
            push_decoded_character(
                &mut text,
                &mut utf8_to_source,
                char::from(byte),
                source_base + relative,
            );
        }
    } else if label.as_deref().is_some_and(us_ascii_label) {
        for (relative, byte) in payload.iter().copied().enumerate() {
            if byte > 0x7F {
                return Err(invalid_xml_encoding(protocol, source_base + relative));
            }
            push_decoded_character(
                &mut text,
                &mut utf8_to_source,
                char::from(byte),
                source_base + relative,
            );
        }
    } else {
        let decoded = std::str::from_utf8(payload)
            .map_err(|error| invalid_xml_encoding(protocol, source_base + error.valid_up_to()))?;
        text.push_str(decoded);
        utf8_to_source.extend(
            (source_base..source_base + payload.len())
                .map(|offset| u64::try_from(offset).unwrap_or(u64::MAX)),
        );
    }
    utf8_to_source.push(u64::try_from(input.len()).unwrap_or(u64::MAX));
    Ok(DecodedXmlSource {
        text,
        utf8_to_source,
    })
}

/// Why bounded XML entity expansion failed.
#[derive(
    Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize, thiserror::Error,
)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum XmlEntityError {
    /// A named entity has no internal declaration.
    #[error("XML entity {0:?} is undeclared")]
    Undeclared(String),
    /// The internal entity declaration graph contains a cycle.
    #[error("XML entity {0:?} is recursive")]
    Recursive(String),
    /// Recursive expansion exceeded [`MAX_XML_ENTITY_DEPTH`].
    #[error("XML entity nesting exceeds {MAX_XML_ENTITY_DEPTH} levels")]
    Depth,
    /// Expanded UTF-8 text exceeded [`MAX_XML_ENTITY_EXPANSION_BYTES`].
    #[error("XML entity expansion exceeds {MAX_XML_ENTITY_EXPANSION_BYTES} bytes")]
    Bytes,
    /// A reference is incomplete, has an invalid name, or denotes an invalid
    /// XML 1.0 character.
    #[error("malformed XML entity or character reference")]
    Malformed,
    /// An external DTD subset was declared. Panproto never reads external
    /// resources while parsing XML.
    #[error("external XML DTD subsets are unsupported")]
    ExternalSubsetUnsupported,
    /// An external parsed general entity was declared.
    #[error("external XML entity {0:?} is unsupported")]
    ExternalEntityUnsupported(String),
    /// A parameter entity declaration or reference occurred in the internal
    /// subset.
    #[error("XML parameter entity construct {0:?} is unsupported")]
    ParameterEntityUnsupported(String),
}

/// Whether every scalar is permitted by XML 1.0's `Char` production.
pub(crate) fn is_xml_10_text(text: &str) -> bool {
    text.chars().all(|character| {
        matches!(
            character as u32,
            0x9 | 0xA | 0xD | 0x20..=0xD7FF | 0xE000..=0xFFFD | 0x0001_0000..=0x0010_FFFF
        )
    })
}

const fn is_xml_name_start(character: char) -> bool {
    matches!(
        character as u32,
        0x41..=0x5A
            | 0x5F
            | 0x61..=0x7A
            | 0xC0..=0xD6
            | 0xD8..=0xF6
            | 0xF8..=0x2FF
            | 0x370..=0x37D
            | 0x37F..=0x1FFF
            | 0x200C..=0x200D
            | 0x2070..=0x218F
            | 0x2C00..=0x2FEF
            | 0x3001..=0xD7FF
            | 0xF900..=0xFDCF
            | 0xFDF0..=0xFFFD
            | 0x10000..=0xEFFFF
    )
}

const fn is_xml_name_char(character: char) -> bool {
    is_xml_name_start(character)
        || matches!(
            character as u32,
            0x2D | 0x2E | 0x30..=0x39 | 0xB7 | 0x300..=0x36F | 0x203F..=0x2040
        )
}

const fn is_xml_entity_name_start(character: char) -> bool {
    character == ':' || is_xml_name_start(character)
}

const fn is_xml_entity_name_char(character: char) -> bool {
    character == ':' || is_xml_name_char(character)
}

fn is_xml_entity_name(name: &str) -> bool {
    let mut characters = name.chars();
    characters.next().is_some_and(is_xml_entity_name_start)
        && characters.all(is_xml_entity_name_char)
}

fn append_xml_entity_output(output: &mut String, text: &str) -> Result<(), XmlEntityError> {
    let expanded_len = output
        .len()
        .checked_add(text.len())
        .ok_or(XmlEntityError::Bytes)?;
    if expanded_len > MAX_XML_ENTITY_EXPANSION_BYTES {
        return Err(XmlEntityError::Bytes);
    }
    output.push_str(text);
    Ok(())
}

/// Resolve the body of an XML 1.0 `CharRef` (`'&#' [0-9]+ ';' | '&#x'
/// [0-9a-fA-F]+ ';'`). Only ASCII digits of the reference's radix are read:
/// `u32::from_str_radix` would also take a leading `+`, which the grammar
/// does not admit.
fn xml_character_reference(name: &str) -> Option<Result<char, XmlEntityError>> {
    let digits = name.strip_prefix('#')?;
    let (radix, digits) = digits
        .strip_prefix('x')
        .map_or((10, digits), |hexadecimal| (16, hexadecimal));
    let well_formed = !digits.is_empty()
        && digits.bytes().all(|byte| {
            if radix == 16 {
                byte.is_ascii_hexdigit()
            } else {
                byte.is_ascii_digit()
            }
        });
    if !well_formed {
        return Some(Err(XmlEntityError::Malformed));
    }
    let Ok(code_point) = u32::from_str_radix(digits, radix) else {
        return Some(Err(XmlEntityError::Malformed));
    };
    Some(
        char::from_u32(code_point)
            .filter(|character| {
                let mut encoded = [0_u8; 4];
                is_xml_10_text(character.encode_utf8(&mut encoded))
            })
            .ok_or(XmlEntityError::Malformed),
    )
}

fn expand_xml_entity_fragment(
    text: &str,
    declarations: &BTreeMap<String, String>,
    stack: &mut Vec<String>,
    output: &mut String,
) -> Result<(), XmlEntityError> {
    if stack.len() > MAX_XML_ENTITY_DEPTH {
        return Err(XmlEntityError::Depth);
    }
    let mut unexpanded = text;
    while let Some(reference_start) = unexpanded.find('&') {
        append_xml_entity_output(output, &unexpanded[..reference_start])?;
        let after_ampersand = &unexpanded[reference_start + 1..];
        let reference_end = after_ampersand.find(';').ok_or(XmlEntityError::Malformed)?;
        let name = &after_ampersand[..reference_end];
        if let Some(predefined) = quick_xml::escape::resolve_predefined_entity(name) {
            append_xml_entity_output(output, predefined)?;
        } else if let Some(character) = xml_character_reference(name) {
            let mut encoded = [0_u8; 4];
            append_xml_entity_output(output, character?.encode_utf8(&mut encoded))?;
        } else {
            if !is_xml_entity_name(name) {
                return Err(XmlEntityError::Malformed);
            }
            let replacement = declarations
                .get(name)
                .ok_or_else(|| XmlEntityError::Undeclared(name.to_owned()))?;
            if stack.iter().any(|active| active == name) {
                return Err(XmlEntityError::Recursive(name.to_owned()));
            }
            stack.push(name.to_owned());
            expand_xml_entity_fragment(replacement, declarations, stack, output)?;
            stack.pop();
        }
        unexpanded = &after_ampersand[reference_end + 1..];
    }
    append_xml_entity_output(output, unexpanded)
}

/// Expand XML character references, predefined entities, and internal general
/// entities under fixed depth and output-byte bounds.
///
/// The declaration map contains raw replacement text for internal general
/// entities. External entities are intentionally absent from this interface.
///
/// # Errors
///
/// Returns [`XmlEntityError`] for malformed, undeclared, recursive, or
/// over-budget input.
pub fn expand_xml_entities(
    text: &str,
    declarations: &BTreeMap<String, String>,
) -> Result<String, XmlEntityError> {
    let mut output = String::new();
    expand_xml_entity_fragment(text, declarations, &mut Vec::new(), &mut output)?;
    Ok(output)
}

#[derive(Clone, Debug)]
struct PreparedXmlSource {
    text: String,
    /// UTF-8 prepared-source byte to UTF-8 decoded-source byte.
    utf8_to_decoded: Vec<u64>,
    /// DTD parsed from the decoded source, including exact declaration provenance.
    dtd: XmlDtd,
    /// Entity replacement bytes already charged while preparing element content.
    entity_expansion_bytes: usize,
}

impl PreparedXmlSource {
    fn decoded_offset(&self, prepared_offset: u64) -> Option<u64> {
        usize::try_from(prepared_offset)
            .ok()
            .and_then(|offset| self.utf8_to_decoded.get(offset).copied())
    }
}

fn xml_doctype_span(text: &str) -> Result<Option<(usize, usize)>, XmlEntityError> {
    let Some(start) = text.find("<!DOCTYPE") else {
        return Ok(None);
    };
    let mut quote = None;
    let mut subset_depth = 0_u32;
    for (relative, character) in text[start + "<!DOCTYPE".len()..].char_indices() {
        match (quote, character) {
            (None, '\'' | '"') => quote = Some(character),
            (Some(active), current) if active == current => quote = None,
            (None, '[') => subset_depth = subset_depth.saturating_add(1),
            (None, ']') => subset_depth = subset_depth.saturating_sub(1),
            (None, '>') if subset_depth == 0 => {
                let end = start + "<!DOCTYPE".len() + relative + character.len_utf8();
                return Ok(Some((start, end)));
            }
            _ => {}
        }
    }
    Err(XmlEntityError::Malformed)
}

fn preparse_xml_doctype(text: &str) -> Result<(XmlDtd, Option<(usize, usize)>), XmlEntityError> {
    let Some((start, end)) = xml_doctype_span(text)? else {
        return Ok((XmlDtd::default(), None));
    };
    let declaration = text[start + "<!DOCTYPE".len()..end - 1].trim();
    let mut dtd = parse_xml_doctype(declaration, Some(u64::try_from(start).unwrap_or(u64::MAX)))?;
    dtd.source = Some(text[start..end].to_owned());
    Ok((dtd, Some((start, end))))
}

fn expand_xml_entity_source_fragment(
    text: &str,
    declarations: &BTreeMap<String, String>,
    stack: &mut Vec<String>,
    output: &mut String,
) -> Result<(), XmlEntityError> {
    if stack.len() > MAX_XML_ENTITY_DEPTH {
        return Err(XmlEntityError::Depth);
    }
    let mut rest = text;
    while let Some(ampersand) = rest.find('&') {
        output.push_str(&rest[..ampersand]);
        let reference = &rest[ampersand + 1..];
        let semicolon = reference.find(';').ok_or(XmlEntityError::Malformed)?;
        let name = &reference[..semicolon];
        if quick_xml::escape::resolve_predefined_entity(name).is_some()
            || xml_character_reference(name).is_some()
        {
            output.push('&');
            output.push_str(name);
            output.push(';');
        } else {
            if !is_xml_entity_name(name) {
                return Err(XmlEntityError::Malformed);
            }
            let replacement = declarations
                .get(name)
                .ok_or_else(|| XmlEntityError::Undeclared(name.to_owned()))?;
            if stack.iter().any(|active| active == name) {
                return Err(XmlEntityError::Recursive(name.to_owned()));
            }
            stack.push(name.to_owned());
            expand_xml_entity_source_fragment(replacement, declarations, stack, output)?;
            stack.pop();
        }
        rest = &reference[semicolon + 1..];
    }
    output.push_str(rest);
    Ok(())
}

fn copy_prepared_span(
    source: &str,
    range: std::ops::Range<usize>,
    output: &mut String,
    offsets: &mut Vec<u64>,
) {
    output.push_str(&source[range.clone()]);
    offsets.extend(range.map(|offset| u64::try_from(offset).unwrap_or(u64::MAX)));
}

/// Expand internal general entities in character content before event parsing.
///
/// XML replacement text is parsed as XML content, so an entity whose value is
/// `<child/>` contributes an element rather than the six literal characters.
/// Attribute references are deliberately left for
/// [`normalize_xml_attribute_value`], where XML's attribute-specific whitespace
/// and `<` rules apply.
// This must remain one source-ordered scan: splitting tag, quote, opaque-region,
// and content transitions would obscure the shared entity-byte budget and
// decoded-offset map that the scan constructs.
#[allow(clippy::too_many_lines)]
fn prepare_xml_content_entities(
    decoded: &str,
    protocol: &str,
) -> Result<PreparedXmlSource, ParseInstanceError> {
    let (dtd, doctype_span) = preparse_xml_doctype(decoded).map_err(|source| {
        let offset = xml_doctype_error_offset(decoded.as_bytes(), Some(0), &source);
        xml_entity_error(protocol, source, offset)
    })?;
    if dtd.entities.is_empty() {
        return Ok(PreparedXmlSource {
            text: decoded.to_owned(),
            utf8_to_decoded: (0..=decoded.len())
                .map(|offset| u64::try_from(offset).unwrap_or(u64::MAX))
                .collect(),
            dtd,
            entity_expansion_bytes: 0,
        });
    }

    let mut output = String::with_capacity(decoded.len());
    let mut offsets = Vec::with_capacity(decoded.len() + 1);
    let mut cursor = 0_usize;
    let mut in_tag = false;
    let mut quote = None;
    let mut expanded_bytes = 0_usize;
    while cursor < decoded.len() {
        if let Some((start, end)) = doctype_span
            && cursor == start
        {
            copy_prepared_span(decoded, start..end, &mut output, &mut offsets);
            cursor = end;
            continue;
        }
        let rest = &decoded[cursor..];
        let opaque_end = if rest.starts_with("<!--") {
            rest.find("-->").map(|end| cursor + end + 3)
        } else if rest.starts_with("<![CDATA[") {
            rest.find("]]>").map(|end| cursor + end + 3)
        } else if rest.starts_with("<?") {
            rest.find("?>").map(|end| cursor + end + 2)
        } else {
            None
        };
        if let Some(end) = opaque_end {
            copy_prepared_span(decoded, cursor..end, &mut output, &mut offsets);
            cursor = end;
            continue;
        }

        let character = rest.chars().next().ok_or_else(|| {
            xml_entity_error(protocol, XmlEntityError::Malformed, Some(cursor as u64))
        })?;
        match (in_tag, quote, character) {
            (false, _, '<') => in_tag = true,
            (true, None, '>') => in_tag = false,
            (true, None, '\'' | '"') => quote = Some(character),
            (true, Some(active), current) if active == current => quote = None,
            (true, Some(_), '&') => {
                let Some(semicolon) = rest.find(';') else {
                    return Err(xml_entity_error(
                        protocol,
                        XmlEntityError::Malformed,
                        Some(u64::try_from(cursor).unwrap_or(u64::MAX)),
                    ));
                };
                let name = &rest[1..semicolon];
                if quick_xml::escape::resolve_predefined_entity(name).is_none()
                    && xml_character_reference(name).is_none()
                {
                    let replacement = dtd.entities.get(name).ok_or_else(|| {
                        xml_entity_error(
                            protocol,
                            XmlEntityError::Undeclared(name.to_owned()),
                            Some(u64::try_from(cursor).unwrap_or(u64::MAX)),
                        )
                    })?;
                    let mut semantic = String::new();
                    expand_xml_entity_fragment(
                        replacement,
                        &dtd.entities,
                        &mut vec![name.to_owned()],
                        &mut semantic,
                    )
                    .map_err(|source| {
                        let offset = if matches!(source, XmlEntityError::Undeclared(_)) {
                            xml_doctype_error_offset(decoded.as_bytes(), dtd.byte_offset, &source)
                                .or_else(|| u64::try_from(cursor).ok())
                        } else {
                            Some(u64::try_from(cursor).unwrap_or(u64::MAX))
                        };
                        xml_entity_error(protocol, source, offset)
                    })?;
                    charge_xml_entity_expansion(&mut expanded_bytes, semantic.len()).map_err(
                        |source| {
                            xml_entity_error(
                                protocol,
                                source,
                                Some(u64::try_from(cursor).unwrap_or(u64::MAX)),
                            )
                        },
                    )?;
                }
                let end = cursor + semicolon + 1;
                copy_prepared_span(decoded, cursor..end, &mut output, &mut offsets);
                cursor = end;
                continue;
            }
            (false, _, '&') => {
                let Some(semicolon) = rest.find(';') else {
                    return Err(xml_entity_error(
                        protocol,
                        XmlEntityError::Malformed,
                        Some(u64::try_from(cursor).unwrap_or(u64::MAX)),
                    ));
                };
                let name = &rest[1..semicolon];
                if quick_xml::escape::resolve_predefined_entity(name).is_none()
                    && xml_character_reference(name).is_none()
                {
                    let replacement = dtd.entities.get(name).ok_or_else(|| {
                        xml_entity_error(
                            protocol,
                            XmlEntityError::Undeclared(name.to_owned()),
                            Some(u64::try_from(cursor).unwrap_or(u64::MAX)),
                        )
                    })?;
                    let mut semantic = String::new();
                    expand_xml_entity_fragment(
                        replacement,
                        &dtd.entities,
                        &mut vec![name.to_owned()],
                        &mut semantic,
                    )
                    .map_err(|source| {
                        let offset = if matches!(
                            source,
                            XmlEntityError::Depth
                                | XmlEntityError::Bytes
                                | XmlEntityError::Recursive(_)
                        ) {
                            Some(u64::try_from(cursor).unwrap_or(u64::MAX))
                        } else {
                            xml_doctype_error_offset(decoded.as_bytes(), dtd.byte_offset, &source)
                                .or_else(|| u64::try_from(cursor).ok())
                        };
                        xml_entity_error(protocol, source, offset)
                    })?;
                    charge_xml_entity_expansion(&mut expanded_bytes, semantic.len()).map_err(
                        |source| {
                            xml_entity_error(
                                protocol,
                                source,
                                Some(u64::try_from(cursor).unwrap_or(u64::MAX)),
                            )
                        },
                    )?;
                    let mut expanded_source = String::new();
                    expand_xml_entity_source_fragment(
                        replacement,
                        &dtd.entities,
                        &mut vec![name.to_owned()],
                        &mut expanded_source,
                    )
                    .map_err(|source| {
                        let offset = if matches!(
                            source,
                            XmlEntityError::Depth
                                | XmlEntityError::Bytes
                                | XmlEntityError::Recursive(_)
                        ) {
                            Some(u64::try_from(cursor).unwrap_or(u64::MAX))
                        } else {
                            xml_doctype_error_offset(decoded.as_bytes(), dtd.byte_offset, &source)
                                .or_else(|| u64::try_from(cursor).ok())
                        };
                        xml_entity_error(protocol, source, offset)
                    })?;
                    output.push_str(&expanded_source);
                    offsets.extend(std::iter::repeat_n(
                        u64::try_from(cursor).unwrap_or(u64::MAX),
                        expanded_source.len(),
                    ));
                    cursor += semicolon + 1;
                    continue;
                }
            }
            _ => {}
        }
        let end = cursor + character.len_utf8();
        copy_prepared_span(decoded, cursor..end, &mut output, &mut offsets);
        cursor = end;
    }
    offsets.push(u64::try_from(decoded.len()).unwrap_or(u64::MAX));
    Ok(PreparedXmlSource {
        text: output,
        utf8_to_decoded: offsets,
        dtd,
        entity_expansion_bytes: expanded_bytes,
    })
}

#[derive(Clone, Debug, Default)]
struct XmlDtd {
    root_name: Option<String>,
    entities: BTreeMap<String, String>,
    attributes: BTreeMap<String, BTreeMap<String, XmlAttributeDeclaration>>,
    byte_offset: Option<u64>,
    /// Exact decoded declaration spelling, beginning with `<!DOCTYPE`.
    source: Option<String>,
    /// For each UTF-8 byte boundary in `source`, the original source-byte offset.
    source_byte_offsets: Vec<u64>,
}

#[derive(Clone, Debug)]
struct XmlAttributeDeclaration {
    non_cdata: bool,
    default: XmlAttributeDefault,
}

#[derive(Clone, Debug)]
enum XmlAttributeDefault {
    Required,
    Implied,
    Value(String),
    Fixed(String),
}

struct XmlDtdCursor<'a> {
    source: &'a str,
    offset: usize,
}

impl<'a> XmlDtdCursor<'a> {
    const fn new(source: &'a str) -> Self {
        Self { source, offset: 0 }
    }

    fn rest(&self) -> &'a str {
        &self.source[self.offset..]
    }

    fn is_empty(&self) -> bool {
        self.rest().is_empty()
    }

    fn skip_whitespace(&mut self) {
        while let Some(character) = self.rest().chars().next() {
            if !matches!(character, ' ' | '\t' | '\r' | '\n') {
                break;
            }
            self.offset += character.len_utf8();
        }
    }

    fn consume(&mut self, prefix: &str) -> bool {
        if self.rest().starts_with(prefix) {
            self.offset += prefix.len();
            true
        } else {
            false
        }
    }

    fn name(&mut self) -> Result<String, XmlEntityError> {
        let start = self.offset;
        let mut characters = self.rest().char_indices();
        let Some((_, first)) = characters.next() else {
            return Err(XmlEntityError::Malformed);
        };
        if !is_xml_entity_name_start(first) {
            return Err(XmlEntityError::Malformed);
        }
        self.offset += first.len_utf8();
        for (relative, character) in characters {
            if !is_xml_entity_name_char(character) {
                self.offset = start + relative;
                break;
            }
            self.offset = start + relative + character.len_utf8();
        }
        Ok(self.source[start..self.offset].to_owned())
    }

    fn quoted(&mut self) -> Result<String, XmlEntityError> {
        let quote = self
            .rest()
            .chars()
            .next()
            .filter(|character| matches!(character, '\'' | '"'))
            .ok_or(XmlEntityError::Malformed)?;
        self.offset += 1;
        let start = self.offset;
        let end = self.rest().find(quote).ok_or(XmlEntityError::Malformed)?;
        self.offset += end;
        let value = self.source[start..self.offset].to_owned();
        self.offset += 1;
        Ok(value)
    }

    fn token(&mut self) -> Result<String, XmlEntityError> {
        let start = self.offset;
        while let Some(character) = self.rest().chars().next() {
            if matches!(character, ' ' | '\t' | '\r' | '\n') {
                break;
            }
            self.offset += character.len_utf8();
        }
        if start == self.offset {
            Err(XmlEntityError::Malformed)
        } else {
            Ok(self.source[start..self.offset].to_owned())
        }
    }

    fn parenthesized(&mut self) -> Result<(), XmlEntityError> {
        if !self.consume("(") {
            return Err(XmlEntityError::Malformed);
        }
        let mut depth = 1_u32;
        while let Some(character) = self.rest().chars().next() {
            self.offset += character.len_utf8();
            match character {
                '(' => depth = depth.checked_add(1).ok_or(XmlEntityError::Malformed)?,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        return Ok(());
                    }
                }
                _ => {}
            }
        }
        Err(XmlEntityError::Malformed)
    }
}

fn find_unquoted(text: &str, needle: char) -> Option<usize> {
    let mut quote = None;
    for (offset, character) in text.char_indices() {
        match (quote, character) {
            (None, '\'' | '"') => quote = Some(character),
            (Some(active), current) if active == current => quote = None,
            (None, current) if current == needle => return Some(offset),
            _ => {}
        }
    }
    None
}

fn parameter_entity_reference(text: &str) -> Option<String> {
    let mut quote = None;
    let mut cursor = 0;
    while cursor < text.len() {
        let character = text[cursor..].chars().next()?;
        match (quote, character) {
            (None, '\'' | '"') => quote = Some(character),
            (Some(active), current) if active == current => quote = None,
            (None, '%') => {
                let after_percent = cursor + 1;
                let mut candidate = XmlDtdCursor::new(&text[after_percent..]);
                candidate.skip_whitespace();
                if let Ok(name) = candidate.name() {
                    candidate.skip_whitespace();
                    if candidate.consume(";") {
                        return Some(name);
                    }
                }
            }
            _ => {}
        }
        cursor += character.len_utf8();
    }
    None
}

fn declaration_end(text: &str) -> Result<usize, XmlEntityError> {
    find_unquoted(text, '>').ok_or(XmlEntityError::Malformed)
}

fn parse_xml_entity_declaration(
    declaration: &str,
    entities: &mut BTreeMap<String, String>,
) -> Result<(), XmlEntityError> {
    let mut cursor = XmlDtdCursor::new(declaration);
    cursor.skip_whitespace();
    if cursor.consume("%") {
        cursor.skip_whitespace();
        let name = cursor.name().unwrap_or_else(|_| "%".to_owned());
        return Err(XmlEntityError::ParameterEntityUnsupported(name));
    }
    let name = cursor.name()?;
    cursor.skip_whitespace();
    if cursor.rest().starts_with("SYSTEM") || cursor.rest().starts_with("PUBLIC") {
        return Err(XmlEntityError::ExternalEntityUnsupported(name));
    }
    let value = cursor.quoted()?;
    cursor.skip_whitespace();
    if !cursor.is_empty() {
        return Err(XmlEntityError::Malformed);
    }
    if let Some(parameter) = parameter_entity_reference(&value) {
        return Err(XmlEntityError::ParameterEntityUnsupported(parameter));
    }
    entities.entry(name).or_insert(value);
    Ok(())
}

fn parse_xml_attribute_type(cursor: &mut XmlDtdCursor<'_>) -> Result<bool, XmlEntityError> {
    if cursor.rest().starts_with('(') {
        cursor.parenthesized()?;
        return Ok(true);
    }
    let attribute_type = cursor.token()?;
    if attribute_type == "NOTATION" {
        cursor.skip_whitespace();
        cursor.parenthesized()?;
    }
    Ok(attribute_type != "CDATA")
}

fn parse_xml_attribute_default(
    cursor: &mut XmlDtdCursor<'_>,
) -> Result<XmlAttributeDefault, XmlEntityError> {
    if cursor.consume("#REQUIRED") {
        Ok(XmlAttributeDefault::Required)
    } else if cursor.consume("#IMPLIED") {
        Ok(XmlAttributeDefault::Implied)
    } else if cursor.consume("#FIXED") {
        cursor.skip_whitespace();
        Ok(XmlAttributeDefault::Fixed(cursor.quoted()?))
    } else {
        Ok(XmlAttributeDefault::Value(cursor.quoted()?))
    }
}

fn parse_xml_attlist_declaration(
    declaration: &str,
    attributes: &mut BTreeMap<String, BTreeMap<String, XmlAttributeDeclaration>>,
) -> Result<(), XmlEntityError> {
    if let Some(parameter) = parameter_entity_reference(declaration) {
        return Err(XmlEntityError::ParameterEntityUnsupported(parameter));
    }
    let mut cursor = XmlDtdCursor::new(declaration);
    cursor.skip_whitespace();
    let element = cursor.name()?;
    loop {
        cursor.skip_whitespace();
        if cursor.is_empty() {
            return Ok(());
        }
        let attribute = cursor.name()?;
        cursor.skip_whitespace();
        let non_cdata = parse_xml_attribute_type(&mut cursor)?;
        cursor.skip_whitespace();
        let default = parse_xml_attribute_default(&mut cursor)?;
        attributes
            .entry(element.clone())
            .or_default()
            .entry(attribute)
            .or_insert(XmlAttributeDeclaration { non_cdata, default });
    }
}

fn parse_xml_internal_subset(subset: &str, dtd: &mut XmlDtd) -> Result<(), XmlEntityError> {
    let mut cursor = XmlDtdCursor::new(subset);
    loop {
        cursor.skip_whitespace();
        if cursor.is_empty() {
            return Ok(());
        }
        if cursor.rest().starts_with("<!--") {
            let end = cursor.rest().find("-->").ok_or(XmlEntityError::Malformed)?;
            cursor.offset += end + 3;
            continue;
        }
        if cursor.rest().starts_with("<?") {
            let end = cursor.rest().find("?>").ok_or(XmlEntityError::Malformed)?;
            cursor.offset += end + 2;
            continue;
        }
        if cursor.consume("%") {
            cursor.skip_whitespace();
            let name = cursor.name().unwrap_or_else(|_| "%".to_owned());
            return Err(XmlEntityError::ParameterEntityUnsupported(name));
        }
        let kind = if cursor.consume("<!ENTITY") {
            "ENTITY"
        } else if cursor.consume("<!ATTLIST") {
            "ATTLIST"
        } else if cursor.consume("<!ELEMENT") {
            "ELEMENT"
        } else if cursor.consume("<!NOTATION") {
            "NOTATION"
        } else {
            return Err(XmlEntityError::Malformed);
        };
        let end = declaration_end(cursor.rest())?;
        let declaration = &cursor.rest()[..end];
        match kind {
            "ENTITY" => parse_xml_entity_declaration(declaration, &mut dtd.entities)?,
            "ATTLIST" => parse_xml_attlist_declaration(declaration, &mut dtd.attributes)?,
            _ => {
                if let Some(parameter) = parameter_entity_reference(declaration) {
                    return Err(XmlEntityError::ParameterEntityUnsupported(parameter));
                }
            }
        }
        cursor.offset += end + 1;
    }
}

fn parse_xml_doctype(text: &str, byte_offset: Option<u64>) -> Result<XmlDtd, XmlEntityError> {
    let text = text.trim();
    let (preamble, subset) = if let Some(start) = find_unquoted(text, '[') {
        let end = text.rfind(']').ok_or(XmlEntityError::Malformed)?;
        if !text[end + 1..].trim().is_empty() {
            return Err(XmlEntityError::Malformed);
        }
        (&text[..start], Some(&text[start + 1..end]))
    } else {
        (text, None)
    };
    let mut cursor = XmlDtdCursor::new(preamble);
    cursor.skip_whitespace();
    let root_name = cursor.name()?;
    cursor.skip_whitespace();
    if !cursor.is_empty() {
        if cursor.rest().starts_with("SYSTEM") || cursor.rest().starts_with("PUBLIC") {
            return Err(XmlEntityError::ExternalSubsetUnsupported);
        }
        return Err(XmlEntityError::Malformed);
    }
    let mut dtd = XmlDtd {
        root_name: Some(root_name),
        byte_offset,
        ..XmlDtd::default()
    };
    if let Some(subset) = subset {
        parse_xml_internal_subset(subset, &mut dtd)?;
    }
    Ok(dtd)
}

/// Construct the prefix-independent identity of an XML name after namespace
/// resolution.
#[must_use]
pub fn xml_expanded_name(namespace_uri: &str, local_name: &str) -> (String, String) {
    (namespace_uri.to_owned(), local_name.to_owned())
}

/// The spec under which XML text of `vertex` is read and written: the
/// vertex's spec in the XML Schema numeral dialect, whose special doubles are
/// `INF`, `-INF` and `NaN`.
fn xml_value_spec(schema: &Schema, protocol: &Protocol, vertex: &panproto_gat::Name) -> ValueSpec {
    ValueSpec {
        numeral: Some(NumeralDialect::Xsd),
        ..ValueSpec::for_vertex(schema, protocol, vertex)
    }
}

/// The XML Schema spelling of a binary64 value: the shared finite printer,
/// `INF` and `-INF`, `NaN`, and `-NaN` for the negative canonical NaN. A NaN
/// with any other payload has no spelling.
fn xml_float_lexeme(value: f64) -> Option<String> {
    if value.is_finite() {
        crate::json_pathway::finite_float_lexeme(value, true)
    } else if value.to_bits() == f64::INFINITY.to_bits() {
        Some("INF".to_owned())
    } else if value.to_bits() == f64::NEG_INFINITY.to_bits() {
        Some("-INF".to_owned())
    } else if value.to_bits() == panproto_inst::value::CANONICAL_NAN.to_bits() {
        Some("NaN".to_owned())
    } else if value.to_bits() == panproto_inst::value::NEGATIVE_CANONICAL_NAN.to_bits() {
        Some("-NaN".to_owned())
    } else {
        None
    }
}

fn xml_clark_name(namespace_uri: &str, local_name: &str) -> String {
    format!("{{{namespace_uri}}}{local_name}")
}

/// The namespace name under which an attribute named `xmlns` or `xmlns:*`
/// is bound by Namespaces in XML 1.0 Sec 3.
const XMLNS_NAMESPACE: &str = "http://www.w3.org/2000/xmlns/";

/// Offsets, relative to the first byte after `<`, of each attribute name
/// token of a start tag whose attribute syntax quick-xml has accepted, in
/// source order. `source` is the tag's bytes after `<`, its element name
/// first.
fn xml_attribute_name_offsets(source: &[u8]) -> Vec<usize> {
    let space = |byte: u8| matches!(byte, b' ' | b'\t' | b'\r' | b'\n');
    let mut offsets = Vec::new();
    let mut index = source
        .iter()
        .position(|byte| space(*byte))
        .unwrap_or(source.len());
    loop {
        while source.get(index).copied().is_some_and(space) {
            index += 1;
        }
        if index >= source.len() || source[index] == b'/' {
            return offsets;
        }
        offsets.push(index);
        while source
            .get(index)
            .is_some_and(|byte| !space(*byte) && *byte != b'=')
        {
            index += 1;
        }
        while source
            .get(index)
            .is_some_and(|byte| space(*byte) || *byte == b'=')
        {
            index += 1;
        }
        let Some(&quote) = source.get(index) else {
            return offsets;
        };
        index += 1;
        while source.get(index).is_some_and(|byte| *byte != quote) {
            index += 1;
        }
        index += 1;
    }
}

fn parse_xml_clark_name(name: &str) -> Option<(&str, &str)> {
    let rest = name.strip_prefix('{')?;
    let (namespace_uri, local_name) = rest.split_once('}')?;
    (!namespace_uri.is_empty() && is_xml_ncname(local_name)).then_some((namespace_uri, local_name))
}

fn is_xml_ncname(name: &str) -> bool {
    let mut characters = name.chars();
    characters.next().is_some_and(is_xml_name_start) && characters.all(is_xml_name_char)
}

/// Whether `name` is a namespace-aware XML name with at most one colon.
pub(crate) fn is_xml_qname(name: &str) -> bool {
    let mut parts = name.split(':');
    let Some(first) = parts.next() else {
        return false;
    };
    is_xml_ncname(first) && parts.next().is_none_or(is_xml_ncname) && parts.next().is_none()
}

pub(crate) fn xml_qname_prefix(name: &str) -> Option<&str> {
    name.split_once(':').map(|(prefix, _)| prefix)
}

fn expanded_xml_qname(
    name: &str,
    namespaces: &HashMap<String, String>,
    use_default_namespace: bool,
) -> Option<String> {
    let (namespace_uri, local_name) = if let Some((prefix, local_name)) = name.split_once(':') {
        let namespace_uri = if prefix == "xml" {
            XML_NS
        } else {
            namespaces.get(prefix)?.as_str()
        };
        (namespace_uri, local_name)
    } else if use_default_namespace {
        (namespaces.get("")?.as_str(), name)
    } else {
        return None;
    };
    (!namespace_uri.is_empty()).then(|| xml_clark_name(namespace_uri, local_name))
}

pub(crate) fn xml_qname_is_bound(name: &str, namespaces: &HashMap<String, String>) -> bool {
    xml_qname_prefix(name).is_none_or(|prefix| {
        prefix == "xml"
            || namespaces
                .get(prefix)
                .is_some_and(|namespace| !namespace.is_empty())
    })
}

pub(crate) fn is_xml_namespace_declaration(name: &str) -> bool {
    name == "xmlns" || xml_qname_prefix(name) == Some("xmlns")
}

pub(crate) fn is_valid_xml_namespace_declaration(name: &str, uri: &str) -> bool {
    if name == "xmlns" {
        return uri != XML_NS && uri != XMLNS_NS;
    }
    let Some(prefix) = name.strip_prefix("xmlns:") else {
        return false;
    };
    !prefix.is_empty()
        && prefix != "xmlns"
        && !uri.is_empty()
        && uri != XMLNS_NS
        && ((prefix == "xml" && uri == XML_NS) || (prefix != "xml" && uri != XML_NS))
}

/// Accumulated state during XML parsing.
struct XmlParseState {
    nodes: HashMap<u32, Node>,
    arcs: Vec<(u32, u32, Edge)>,
    next_id: u32,
}

#[derive(Default)]
struct XmlDocumentRoot {
    id: Option<u32>,
    vertex: String,
}

impl XmlParseState {
    fn new() -> Self {
        Self {
            nodes: HashMap::new(),
            arcs: Vec::new(),
            next_id: 0,
        }
    }

    const fn alloc_id(&mut self) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }
}

/// Parse XML bytes into a `WInstance` using `quick-xml`.
///
/// The parser processes XML events (start/end element, text, attributes)
/// and maps them to schema vertices and edges. Element tag names are
/// matched against vertex kinds and edge names in the schema.
///
/// # Errors
///
/// Returns [`ParseInstanceError::Parse`] if the XML is malformed, including
/// an undeclared or recursive entity; [`ParseInstanceError::LimitExceeded`]
/// if bounded entity expansion exceeds its depth or byte allowance;
/// [`ParseInstanceError::UnsupportedFeature`] for external DTD resources or
/// parameter entities; or [`ParseInstanceError::SchemaMismatch`] if the
/// document does not match the schema structure.
// The event loop is the XML parser state machine. Keeping its transitions
// together makes the element stack, text run, DTD, and source-offset updates
// auditable in event order.
#[allow(clippy::too_many_lines)]
pub fn parse_xml_bytes(
    schema: &Schema,
    input: &[u8],
    protocol: &str,
) -> Result<WInstance, ParseInstanceError> {
    let decoded = decode_xml_source(input, protocol)?;
    let prepared = prepare_xml_content_entities(&decoded.text, protocol)?;
    let mut reader = configured_xml_reader(prepared.text.as_bytes());

    let mut state = XmlParseState::new();
    let mut element_stack: Vec<(u32, String)> = Vec::new(); // (node_id, vertex_id)
    let mut buf = Vec::new();
    let mut root = XmlDocumentRoot::default();
    let protocol_definition = protocol_definition(protocol);
    let mut fidelity = Vec::new();
    let mut dtd = prepared.dtd.clone();
    if let (Some(source), Some(decoded_start)) = (&dtd.source, dtd.byte_offset) {
        dtd.source_byte_offsets = (0..=source.len())
            .filter_map(|relative| {
                decoded_start
                    .checked_add(u64::try_from(relative).ok()?)
                    .and_then(|offset| decoded.source_offset(offset))
            })
            .collect();
    }
    let mut saw_doctype = false;
    let mut entity_expansion_bytes = prepared.entity_expansion_bytes;
    // One element's character data can arrive as several events: a run of
    // text, an entity or character reference, a CDATA section, then more
    // text. They are accumulated here and written to the node when the
    // element closes, so `a &amp; b` is one value rather than whichever
    // fragment happened to arrive last.
    let mut text_buf = String::new();
    let mut text_byte_offset = None;

    loop {
        let event_offset_utf8 = reader.buffer_position();
        let event_offset_decoded = prepared.decoded_offset(event_offset_utf8);
        let event_offset = event_offset_decoded.and_then(|offset| decoded.source_offset(offset));
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                if element_stack.is_empty() {
                    flush_document_text(&mut text_buf, protocol)?;
                } else {
                    flush_text(
                        &mut text_buf,
                        &element_stack,
                        &mut state,
                        schema,
                        &protocol_definition,
                        &mut fidelity,
                        true,
                        &mut text_byte_offset,
                    )?;
                }
                let (node_id, vertex_id) = ingest_xml_element(
                    e,
                    reader.resolver(),
                    schema,
                    &mut state,
                    &element_stack,
                    &mut root,
                    protocol,
                    &dtd,
                    &mut entity_expansion_bytes,
                    event_offset,
                )?;
                element_stack.push((node_id, vertex_id));
            }
            Ok(Event::Empty(ref e)) => {
                // Self-closing element: emit the node (same as Start) but
                // don't push onto the element stack since it has no children.
                if element_stack.is_empty() {
                    flush_document_text(&mut text_buf, protocol)?;
                } else {
                    flush_text(
                        &mut text_buf,
                        &element_stack,
                        &mut state,
                        schema,
                        &protocol_definition,
                        &mut fidelity,
                        true,
                        &mut text_byte_offset,
                    )?;
                }
                let (node_id, vertex_id) = ingest_xml_element(
                    e,
                    reader.resolver(),
                    schema,
                    &mut state,
                    &element_stack,
                    &mut root,
                    protocol,
                    &dtd,
                    &mut entity_expansion_bytes,
                    event_offset,
                )?;
                materialize_declared_empty_xml_scalar(
                    &mut state,
                    schema,
                    &protocol_definition,
                    node_id,
                    &vertex_id,
                    event_offset,
                    &mut fidelity,
                );
            }
            Ok(Event::Text(ref e)) => {
                if text_buf.is_empty() {
                    text_byte_offset = event_offset;
                }
                append_xml_text(e, &mut text_buf, protocol)?;
            }
            Ok(Event::GeneralRef(ref e)) => {
                // A reference is its own event: the reader no longer
                // resolves it into the surrounding text. A character
                // reference resolves numerically; a named one resolves
                // only if it is XML's own, since resolving a
                // document-defined entity is what the billion-laughs
                // class of attack turns on. An unresolvable name is
                // written back literally rather than dropped, so no
                // character disappears from a value.
                if text_buf.is_empty() {
                    text_byte_offset = event_offset;
                }
                append_xml_reference(
                    e,
                    &mut text_buf,
                    protocol,
                    &dtd.entities,
                    &mut entity_expansion_bytes,
                    event_offset,
                )?;
            }
            Ok(Event::CData(ref e)) => {
                // CDATA is character data too, and was previously
                // dropped by the catch-all arm, so `<![CDATA[x]]>` read
                // back as an element with no value at all.
                if text_buf.is_empty() {
                    text_byte_offset = event_offset;
                }
                append_xml_cdata(e, &mut text_buf, protocol)?;
            }
            Ok(Event::End(_)) => {
                let had_character_data = !text_buf.is_empty();
                flush_text(
                    &mut text_buf,
                    &element_stack,
                    &mut state,
                    schema,
                    &protocol_definition,
                    &mut fidelity,
                    false,
                    &mut text_byte_offset,
                )?;
                if !had_character_data && let Some((node_id, vertex_id)) = element_stack.last() {
                    materialize_declared_empty_xml_scalar(
                        &mut state,
                        schema,
                        &protocol_definition,
                        *node_id,
                        vertex_id,
                        event_offset,
                        &mut fidelity,
                    );
                }
                element_stack.pop();
            }
            Ok(Event::DocType(ref event)) => {
                if saw_doctype || root.id.is_some() {
                    return Err(ParseInstanceError::Parse {
                        protocol: protocol.to_owned(),
                        message:
                            "XML document has an invalid or misplaced document type declaration"
                                .to_owned(),
                    });
                }
                let declaration =
                    event
                        .xml10_content()
                        .map_err(|error| ParseInstanceError::Parse {
                            protocol: protocol.to_owned(),
                            message: format!("XML document type decode error: {error}"),
                        })?;
                parse_xml_doctype(&declaration, event_offset).map_err(|source| {
                    let decoded_offset = xml_doctype_error_offset(
                        decoded.text.as_bytes(),
                        event_offset_decoded,
                        &source,
                    );
                    let offset = decoded_offset.and_then(|offset| decoded.source_offset(offset));
                    xml_entity_error(protocol, source, offset)
                })?;
                saw_doctype = true;
            }
            Ok(Event::Eof) => {
                if element_stack.is_empty() {
                    flush_document_text(&mut text_buf, protocol)?;
                }
                break;
            }
            Ok(_) => {} // Skip comments, PIs, and the XML declaration.
            Err(e) => {
                let position = decoded
                    .source_offset(
                        prepared
                            .decoded_offset(reader.error_position())
                            .unwrap_or_else(|| reader.error_position()),
                    )
                    .unwrap_or_else(|| reader.error_position());
                return Err(xml_parse_error(protocol, position, &e));
            }
        }
        buf.clear();
    }

    finish_xml_parse(state, root, fidelity, protocol)
}

fn finish_xml_parse(
    state: XmlParseState,
    root: XmlDocumentRoot,
    fidelity: Vec<ValueFidelityError>,
    protocol: &str,
) -> Result<WInstance, ParseInstanceError> {
    let root_id = root.id.ok_or_else(|| ParseInstanceError::Parse {
        protocol: protocol.to_owned(),
        message: "XML document has no root element".into(),
    })?;
    if !fidelity.is_empty() {
        return Err(ParseInstanceError::Fidelity(fidelity));
    }
    Ok(WInstance::new(
        state.nodes,
        state.arcs,
        Vec::new(),
        root_id,
        panproto_gat::Name::from(root.vertex),
    ))
}

fn materialize_declared_empty_xml_scalar(
    state: &mut XmlParseState,
    schema: &Schema,
    protocol: &Protocol,
    node_id: u32,
    vertex: &str,
    byte_offset: Option<u64>,
    fidelity: &mut Vec<ValueFidelityError>,
) {
    let vertex = panproto_gat::Name::from(vertex);
    let spec = xml_value_spec(schema, protocol, &vertex);
    if matches!(spec.kind, ScalarKind::Any)
        || state
            .nodes
            .get(&node_id)
            .is_none_or(|node| node.value.is_some())
        || state.arcs.iter().any(|(parent, _, _)| *parent == node_id)
    {
        return;
    }
    let Some(node) = state.nodes.get_mut(&node_id) else {
        return;
    };
    match crate::json_pathway::coerce_exact_lexeme(&Lexeme::new("", LexClass::String), &spec) {
        Ok(Value::Null) => node.value = Some(FieldPresence::Null),
        Ok(value) => node.value = Some(FieldPresence::Present(value)),
        Err(error) => fidelity.push(error.at(ValueSite::new(node.anchor.clone(), byte_offset))),
    }
}

fn configured_xml_reader(input: &[u8]) -> NsReader<&[u8]> {
    let mut reader = NsReader::from_reader(input);
    reader
        .resolver_mut()
        .set_max_declarations_per_element(MAX_XML_NAMESPACE_DECLARATIONS);
    // Trimming individual events loses spaces around entity references. Text
    // is instead assembled and normalized once by `flush_text`.
    reader.config_mut().trim_text(false);
    reader
}

fn flush_document_text(text: &mut String, protocol: &str) -> Result<(), ParseInstanceError> {
    if !text.trim().is_empty() {
        return Err(ParseInstanceError::Parse {
            protocol: protocol.to_owned(),
            message: "XML character data outside the document element".into(),
        });
    }
    text.clear();
    Ok(())
}

fn xml_parse_error(protocol: &str, position: u64, error: &quick_xml::Error) -> ParseInstanceError {
    ParseInstanceError::Parse {
        protocol: protocol.to_owned(),
        message: format!("XML parse error at position {position}: {error}"),
    }
}

fn find_xml_source_offset(input: &[u8], start: Option<u64>, needle: &[u8]) -> Option<u64> {
    let start = usize::try_from(start?).ok()?;
    let relative = input
        .get(start..)?
        .windows(needle.len())
        .position(|part| part == needle)?;
    u64::try_from(start.checked_add(relative)?).ok()
}

fn xml_doctype_error_offset(
    input: &[u8],
    doctype_offset: Option<u64>,
    source: &XmlEntityError,
) -> Option<u64> {
    let needle = match source {
        XmlEntityError::ExternalSubsetUnsupported => {
            let system = find_xml_source_offset(input, doctype_offset, b"SYSTEM");
            let public = find_xml_source_offset(input, doctype_offset, b"PUBLIC");
            return match (system, public) {
                (Some(left), Some(right)) => Some(left.min(right)),
                (Some(offset), None) | (None, Some(offset)) => Some(offset),
                (None, None) => doctype_offset,
            };
        }
        XmlEntityError::ExternalEntityUnsupported(name) => format!("<!ENTITY {name}").into_bytes(),
        XmlEntityError::ParameterEntityUnsupported(name) => {
            let declaration = find_xml_source_offset(input, doctype_offset, b"<!ENTITY %");
            let reference =
                find_xml_source_offset(input, doctype_offset, format!("%{name};").as_bytes());
            return match (declaration, reference) {
                (Some(left), Some(right)) => Some(left.min(right)),
                (Some(offset), None) | (None, Some(offset)) => Some(offset),
                (None, None) => doctype_offset,
            };
        }
        XmlEntityError::Undeclared(name) | XmlEntityError::Recursive(name) => {
            return find_xml_source_offset(input, doctype_offset, format!("&{name};").as_bytes())
                .or(doctype_offset);
        }
        XmlEntityError::Depth | XmlEntityError::Bytes | XmlEntityError::Malformed => {
            return find_xml_source_offset(input, doctype_offset, b"&").or(doctype_offset);
        }
    };
    find_xml_source_offset(input, doctype_offset, &needle).or(doctype_offset)
}

fn xml_stored_doctype_error_offset(dtd: &XmlDtd, source: &XmlEntityError) -> Option<u64> {
    let relative = dtd
        .source
        .as_ref()
        .and_then(|declaration| xml_doctype_error_offset(declaration.as_bytes(), Some(0), source));
    if let Some(offset) = relative
        .and_then(|relative| usize::try_from(relative).ok())
        .and_then(|relative| dtd.source_byte_offsets.get(relative).copied())
    {
        return Some(offset);
    }
    dtd.byte_offset.and_then(|base| {
        relative
            .and_then(|relative| base.checked_add(relative))
            .or(Some(base))
    })
}

fn xml_attribute_error_offset(
    raw_value: &str,
    value_offset: Option<u64>,
    source: &XmlEntityError,
) -> Option<u64> {
    let relative = match source {
        XmlEntityError::Undeclared(name) | XmlEntityError::Recursive(name) => {
            raw_value.find(&format!("&{name};"))
        }
        XmlEntityError::Depth | XmlEntityError::Bytes | XmlEntityError::Malformed => {
            raw_value.find('&')
        }
        _ => None,
    };
    value_offset.and_then(|offset| {
        relative
            .and_then(|relative| u64::try_from(relative).ok())
            .and_then(|relative| offset.checked_add(relative))
            .or(Some(offset))
    })
}

fn xml_entity_error(
    protocol: &str,
    source: XmlEntityError,
    byte_offset: Option<u64>,
) -> ParseInstanceError {
    match source {
        XmlEntityError::Depth => ParseInstanceError::LimitExceeded {
            protocol: protocol.to_owned(),
            resource: "XML entity nesting depth".to_owned(),
            bound: u64::try_from(MAX_XML_ENTITY_DEPTH).unwrap_or(u64::MAX),
            byte_offset,
        },
        XmlEntityError::Bytes => ParseInstanceError::LimitExceeded {
            protocol: protocol.to_owned(),
            resource: "XML entity expansion bytes".to_owned(),
            bound: u64::try_from(MAX_XML_ENTITY_EXPANSION_BYTES).unwrap_or(u64::MAX),
            byte_offset,
        },
        XmlEntityError::ExternalSubsetUnsupported => ParseInstanceError::UnsupportedFeature {
            protocol: protocol.to_owned(),
            feature: "external XML DTD subset".to_owned(),
            byte_offset,
        },
        XmlEntityError::ExternalEntityUnsupported(name) => ParseInstanceError::UnsupportedFeature {
            protocol: protocol.to_owned(),
            feature: format!("external XML entity {name:?}"),
            byte_offset,
        },
        XmlEntityError::ParameterEntityUnsupported(name) => {
            ParseInstanceError::UnsupportedFeature {
                protocol: protocol.to_owned(),
                feature: format!("XML parameter entity construct {name:?}"),
                byte_offset,
            }
        }
        XmlEntityError::Undeclared(name) => ParseInstanceError::Parse {
            protocol: protocol.to_owned(),
            message: format!("XML entity {name:?} is undeclared at byte {byte_offset:?}"),
        },
        XmlEntityError::Recursive(name) => ParseInstanceError::Parse {
            protocol: protocol.to_owned(),
            message: format!("XML entity {name:?} is recursive at byte {byte_offset:?}"),
        },
        XmlEntityError::Malformed => ParseInstanceError::Parse {
            protocol: protocol.to_owned(),
            message: format!("malformed XML entity or character reference at byte {byte_offset:?}"),
        },
    }
}

fn charge_xml_entity_expansion(
    expanded_bytes: &mut usize,
    additional: usize,
) -> Result<(), XmlEntityError> {
    *expanded_bytes = expanded_bytes
        .checked_add(additional)
        .ok_or(XmlEntityError::Bytes)?;
    if *expanded_bytes > MAX_XML_ENTITY_EXPANSION_BYTES {
        Err(XmlEntityError::Bytes)
    } else {
        Ok(())
    }
}

fn append_xml_text(
    event: &quick_xml::events::BytesText<'_>,
    text: &mut String,
    protocol: &str,
) -> Result<(), ParseInstanceError> {
    let decoded = event
        .xml10_content()
        .map_err(|error| ParseInstanceError::Parse {
            protocol: protocol.to_owned(),
            message: format!("XML text decode error: {error}"),
        })?;
    validate_parsed_xml_text(&decoded, protocol)?;
    text.push_str(&decoded);
    Ok(())
}

fn append_xml_cdata(
    event: &quick_xml::events::BytesCData<'_>,
    text: &mut String,
    protocol: &str,
) -> Result<(), ParseInstanceError> {
    let decoded = event
        .xml10_content()
        .map_err(|error| ParseInstanceError::Parse {
            protocol: protocol.to_owned(),
            message: format!("XML CDATA decode error: {error}"),
        })?;
    validate_parsed_xml_text(&decoded, protocol)?;
    text.push_str(&decoded);
    Ok(())
}

fn append_xml_reference(
    reference: &quick_xml::events::BytesRef<'_>,
    text: &mut String,
    protocol: &str,
    declarations: &BTreeMap<String, String>,
    expanded_bytes: &mut usize,
    byte_offset: Option<u64>,
) -> Result<(), ParseInstanceError> {
    let resolved = reference
        .resolve_char_ref()
        .map_err(|error| ParseInstanceError::Parse {
            protocol: protocol.to_owned(),
            message: format!("XML character reference error: {error}"),
        })?;
    if let Some(character) = resolved {
        let mut encoded = [0_u8; 4];
        if !is_xml_10_text(character.encode_utf8(&mut encoded)) {
            return Err(ParseInstanceError::Parse {
                protocol: protocol.to_owned(),
                message: "XML character reference resolves to a forbidden XML 1.0 character"
                    .to_owned(),
            });
        }
        text.push(character);
        return Ok(());
    }
    let name = reference
        .decode()
        .map_err(|error| ParseInstanceError::Parse {
            protocol: protocol.to_owned(),
            message: format!("XML entity reference decode error: {error}"),
        })?;
    if let Some(value) = quick_xml::escape::resolve_predefined_entity(&name) {
        text.push_str(value);
    } else {
        let expanded = expand_xml_entities(&format!("&{name};"), declarations)
            .map_err(|source| xml_entity_error(protocol, source, byte_offset))?;
        charge_xml_entity_expansion(expanded_bytes, expanded.len())
            .map_err(|source| xml_entity_error(protocol, source, byte_offset))?;
        validate_parsed_xml_text(&expanded, protocol)?;
        text.push_str(&expanded);
    }
    Ok(())
}

fn validate_parsed_xml_text(text: &str, protocol: &str) -> Result<(), ParseInstanceError> {
    if is_xml_10_text(text) {
        Ok(())
    } else {
        Err(ParseInstanceError::Parse {
            protocol: protocol.to_owned(),
            message: "XML input contains a forbidden XML 1.0 character".to_owned(),
        })
    }
}

fn append_xml_attribute_literal(
    output: &mut String,
    text: &str,
    bounded: bool,
) -> Result<(), XmlEntityError> {
    let mut characters = text.chars().peekable();
    while let Some(character) = characters.next() {
        if character == '<' {
            return Err(XmlEntityError::Malformed);
        }
        let normalized = match character {
            '\r' => {
                if characters.peek() == Some(&'\n') {
                    characters.next();
                }
                ' '
            }
            '\t' | '\n' => ' ',
            _ => character,
        };
        let mut encoded = [0_u8; 4];
        let normalized = normalized.encode_utf8(&mut encoded);
        if bounded {
            append_xml_entity_output(output, normalized)?;
        } else {
            output.push_str(normalized);
        }
    }
    Ok(())
}

fn expand_xml_attribute_entity_fragment(
    text: &str,
    declarations: &BTreeMap<String, String>,
    stack: &mut Vec<String>,
    output: &mut String,
) -> Result<(), XmlEntityError> {
    if stack.len() > MAX_XML_ENTITY_DEPTH {
        return Err(XmlEntityError::Depth);
    }
    let mut unexpanded = text;
    while let Some(reference_start) = unexpanded.find('&') {
        append_xml_attribute_literal(output, &unexpanded[..reference_start], true)?;
        let after_ampersand = &unexpanded[reference_start + 1..];
        let reference_end = after_ampersand.find(';').ok_or(XmlEntityError::Malformed)?;
        let name = &after_ampersand[..reference_end];
        if let Some(predefined) = quick_xml::escape::resolve_predefined_entity(name) {
            append_xml_entity_output(output, predefined)?;
        } else if let Some(character) = xml_character_reference(name) {
            let mut encoded = [0_u8; 4];
            append_xml_entity_output(output, character?.encode_utf8(&mut encoded))?;
        } else {
            if !is_xml_entity_name(name) {
                return Err(XmlEntityError::Malformed);
            }
            let replacement = declarations
                .get(name)
                .ok_or_else(|| XmlEntityError::Undeclared(name.to_owned()))?;
            if stack.iter().any(|active| active == name) {
                return Err(XmlEntityError::Recursive(name.to_owned()));
            }
            stack.push(name.to_owned());
            expand_xml_attribute_entity_fragment(replacement, declarations, stack, output)?;
            stack.pop();
        }
        unexpanded = &after_ampersand[reference_end + 1..];
    }
    append_xml_attribute_literal(output, unexpanded, true)
}

fn normalize_xml_attribute_value(
    raw: &str,
    non_cdata: bool,
    declarations: &BTreeMap<String, String>,
    expanded_bytes: &mut usize,
) -> Result<String, XmlEntityError> {
    let mut normalized = String::with_capacity(raw.len());
    let mut unexpanded = raw;
    while let Some(reference_start) = unexpanded.find('&') {
        append_xml_attribute_literal(&mut normalized, &unexpanded[..reference_start], false)?;
        let after_ampersand = &unexpanded[reference_start + 1..];
        let reference_end = after_ampersand.find(';').ok_or(XmlEntityError::Malformed)?;
        let name = &after_ampersand[..reference_end];
        if let Some(predefined) = quick_xml::escape::resolve_predefined_entity(name) {
            normalized.push_str(predefined);
        } else if let Some(character) = xml_character_reference(name) {
            normalized.push(character?);
        } else {
            if !is_xml_entity_name(name) {
                return Err(XmlEntityError::Malformed);
            }
            let replacement = declarations
                .get(name)
                .ok_or_else(|| XmlEntityError::Undeclared(name.to_owned()))?;
            let mut replacement_text = String::new();
            expand_xml_attribute_entity_fragment(
                replacement,
                declarations,
                &mut vec![name.to_owned()],
                &mut replacement_text,
            )?;
            charge_xml_entity_expansion(expanded_bytes, replacement_text.len())?;
            normalized.push_str(&replacement_text);
        }
        unexpanded = &after_ampersand[reference_end + 1..];
    }
    append_xml_attribute_literal(&mut normalized, unexpanded, false)?;
    if non_cdata {
        Ok(normalized
            .split(' ')
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join(" "))
    } else {
        Ok(normalized)
    }
}

/// Write accumulated character data to the innermost open element, and
/// clear the buffer.
///
/// Every character-data run is preserved. In mixed content, whitespace
/// between or after child elements can be semantically significant, so it is
/// represented by the same ordered text-segment nodes as non-whitespace text.
/// A leaf element's text remains its direct value.
// These arguments are the complete state transition for one accumulated text
// run; hiding part of it in an implicit parser context would make mutation of
// fidelity and source-offset state less apparent.
#[allow(clippy::too_many_arguments)]
fn flush_text(
    text_buf: &mut String,
    element_stack: &[(u32, String)],
    state: &mut XmlParseState,
    schema: &Schema,
    protocol: &Protocol,
    fidelity: &mut Vec<ValueFidelityError>,
    before_child: bool,
    text_byte_offset: &mut Option<u64>,
) -> Result<(), ParseInstanceError> {
    if !text_buf.is_empty() {
        if let Some(current) = element_stack.last() {
            let has_child_element = state.arcs.iter().any(|(parent, _, _)| *parent == current.0);
            let is_null = state
                .nodes
                .get(&current.0)
                .is_some_and(|node| matches!(node.value, Some(FieldPresence::Null)));
            if is_null {
                return Err(ParseInstanceError::Parse {
                    protocol: protocol.name.clone(),
                    message: "an xsi:nil element cannot contain character data".into(),
                });
            }
            if before_child || has_child_element {
                let segment_anchor = format!("{}:text", current.1);
                let segment_id = state.alloc_id();
                let mut segment = Node::new(segment_id, segment_anchor.as_str());
                segment.value = Some(FieldPresence::Present(Value::Str(text_buf.clone())));
                segment.shape = NodeShape::XmlTextSegment;
                state.nodes.insert(segment_id, segment);
                state.arcs.push((
                    current.0,
                    segment_id,
                    Edge {
                        src: current.1.as_str().into(),
                        tgt: segment_anchor.as_str().into(),
                        kind: "prop".into(),
                        name: Some("$xml_text".into()),
                    },
                ));
            } else if let Some(node) = state.nodes.get_mut(&current.0) {
                let spec = xml_value_spec(schema, protocol, &node.anchor);
                match crate::json_pathway::coerce_exact_lexeme(
                    &Lexeme::new(text_buf, LexClass::String),
                    &spec,
                ) {
                    Ok(Value::Null) => node.value = Some(FieldPresence::Null),
                    Ok(value) => node.value = Some(FieldPresence::Present(value)),
                    Err(error) => {
                        fidelity
                            .push(error.at(ValueSite::new(node.anchor.clone(), *text_byte_offset)));
                    }
                }
            }
        }
    }
    text_buf.clear();
    *text_byte_offset = None;
    Ok(())
}

/// Emit a `WInstance` to XML bytes.
///
/// Walks the instance tree from root, emitting XML start/end elements
/// for each node and text content for leaf values.
///
/// # Errors
///
/// Returns [`EmitInstanceError::Emit`] if serialization fails.
pub fn emit_xml_bytes(
    schema: &Schema,
    instance: &WInstance,
    protocol: &str,
) -> Result<Vec<u8>, EmitInstanceError> {
    crate::json_pathway::validate_canonical_tree_structure(
        schema,
        instance,
        protocol,
        "XML",
        crate::json_pathway::TreeReader::Canonical,
    )?;
    let mut writer = quick_xml::Writer::new(Vec::new());
    let protocol_definition = protocol_definition(protocol);
    let context = XmlEmitContext {
        schema,
        protocol_definition: &protocol_definition,
        instance,
        protocol,
    };
    write_xml_node(&mut writer, &context, instance.root, &HashMap::new())?;
    Ok(writer.into_inner())
}

struct XmlEmitContext<'a> {
    schema: &'a Schema,
    protocol_definition: &'a Protocol,
    instance: &'a WInstance,
    protocol: &'a str,
}

fn has_emittable_child(instance: &WInstance, node_id: u32) -> bool {
    instance.children_map.get(&node_id).is_some_and(|children| {
        children.iter().any(|child_id| {
            instance
                .node(*child_id)
                .is_some_and(|child| !matches!(child.value, Some(FieldPresence::Absent)))
        })
    })
}

fn write_xml_text_segment(
    writer: &mut quick_xml::Writer<Vec<u8>>,
    context: &XmlEmitContext<'_>,
    node_id: u32,
    node: &Node,
) -> Result<(), EmitInstanceError> {
    use quick_xml::events::BytesText;

    let has_children = context
        .instance
        .children_map
        .get(&node_id)
        .is_some_and(|children| !children.is_empty());
    if has_children || node.discriminator.is_some() || !node.extra_fields.is_empty() {
        return Err(EmitInstanceError::Emit {
            protocol: context.protocol.to_owned(),
            message: format!("XML text segment {node_id} carries element structure"),
        });
    }
    let site = ValueSite::new(node.anchor.clone(), None);
    let text = match &node.value {
        Some(FieldPresence::Present(Value::Str(text))) if !text.is_empty() => text,
        Some(FieldPresence::Present(Value::Str(_))) => {
            return Err(
                EmitValueError::new(site, "str", "xml", EmitValueReason::NotRepresentable).into(),
            );
        }
        Some(FieldPresence::Present(value)) => {
            return Err(EmitValueError::new(
                site,
                value.type_name(),
                "xml",
                EmitValueReason::NotRepresentable,
            )
            .into());
        }
        Some(FieldPresence::Null) => {
            return Err(EmitValueError::new(
                site,
                "null",
                "xml",
                EmitValueReason::NullNotRepresentable,
            )
            .into());
        }
        Some(FieldPresence::Absent) | None => {
            return Err(EmitValueError::new(
                site,
                "absent",
                "xml",
                EmitValueReason::AbsentNotRepresentable,
            )
            .into());
        }
    };
    if !is_xml_10_text(text) {
        return Err(
            EmitValueError::new(site, "str", "xml", EmitValueReason::NotRepresentable).into(),
        );
    }
    let escaped = escape_xml_source_text(text, false);
    write_xml_event(
        writer,
        Event::Text(BytesText::from_escaped(escaped)),
        context.protocol,
    )
}

fn validate_xml_element_value_structure(
    context: &XmlEmitContext<'_>,
    node_id: u32,
    node: &Node,
    site: &ValueSite,
) -> Result<(), EmitInstanceError> {
    if node.discriminator.is_some() {
        return Err(EmitInstanceError::Emit {
            protocol: context.protocol.to_owned(),
            message: format!("XML element node {node_id} carries an unsupported discriminator"),
        });
    }
    if matches!(node.value, Some(FieldPresence::Present(Value::Null)))
        || (matches!(node.value, Some(FieldPresence::Null))
            && has_emittable_child(context.instance, node_id))
    {
        return Err(EmitValueError::new(
            site.clone(),
            "null",
            "xml",
            EmitValueReason::NotRepresentable,
        )
        .into());
    }
    if matches!(node.value, Some(FieldPresence::Present(_)))
        && has_emittable_child(context.instance, node_id)
    {
        return Err(EmitInstanceError::Emit {
            protocol: context.protocol.to_owned(),
            message: format!("XML scalar node {node_id} also carries child elements"),
        });
    }
    Ok(())
}

fn write_xml_scalar_value(
    writer: &mut quick_xml::Writer<Vec<u8>>,
    context: &XmlEmitContext<'_>,
    node_id: u32,
    node: &Node,
    site: &ValueSite,
) -> Result<(), EmitInstanceError> {
    use quick_xml::events::BytesText;

    match &node.value {
        Some(FieldPresence::Present(value)) if !matches!(value, Value::Null) => {
            let spec = xml_value_spec(context.schema, context.protocol_definition, &node.anchor);
            let text = xml_value_text(value, Some(&spec), site, false)?;
            let escaped = escape_xml_source_text(&text, false);
            write_xml_event(
                writer,
                Event::Text(BytesText::from_escaped(escaped)),
                context.protocol,
            )
        }
        Some(FieldPresence::Absent) if node_id == context.instance.root => {
            Err(EmitValueError::new(
                site.clone(),
                "absent",
                "xml",
                EmitValueReason::AbsentNotRepresentable,
            )
            .into())
        }
        _ => Ok(()),
    }
}

fn write_xml_children(
    writer: &mut quick_xml::Writer<Vec<u8>>,
    context: &XmlEmitContext<'_>,
    node_id: u32,
    namespaces: &HashMap<String, String>,
) -> Result<(), EmitInstanceError> {
    let Some(children) = context.instance.children_map.get(&node_id) else {
        return Ok(());
    };
    for &child_id in children {
        if let Some(child) = context.instance.node(child_id)
            && matches!(child.value, Some(FieldPresence::Absent))
        {
            let has_grandchildren = context
                .instance
                .children_map
                .get(&child_id)
                .is_some_and(|grandchildren| !grandchildren.is_empty());
            let has_unrepresentable_structure = has_grandchildren
                || child.discriminator.is_some()
                || !child.extra_fields.is_empty()
                || !matches!(
                    &child.shape,
                    NodeShape::Plain | NodeShape::XmlElement { .. }
                );
            if has_unrepresentable_structure {
                return Err(EmitInstanceError::Emit {
                    protocol: context.protocol.to_owned(),
                    message: format!(
                        "absent XML child node {child_id} carries structure that omission would discard"
                    ),
                });
            }
            continue;
        }
        write_xml_node(writer, context, child_id, namespaces)?;
    }
    Ok(())
}

fn write_xml_node(
    writer: &mut quick_xml::Writer<Vec<u8>>,
    context: &XmlEmitContext<'_>,
    node_id: u32,
    in_scope_namespaces: &HashMap<String, String>,
) -> Result<(), EmitInstanceError> {
    use quick_xml::events::{BytesEnd, BytesStart};

    let node = context
        .instance
        .nodes
        .get(&node_id)
        .ok_or_else(|| EmitInstanceError::Emit {
            protocol: context.protocol.to_owned(),
            message: format!("node {node_id} not found"),
        })?;
    if !matches!(
        &node.shape,
        NodeShape::XmlElement { .. } | NodeShape::XmlTextSegment
    ) {
        return Err(EmitInstanceError::Emit {
            protocol: context.protocol.to_owned(),
            message: format!("XML node {node_id} carries an incompatible structural shape"),
        });
    }
    if node.is_xml_text_segment() {
        return write_xml_text_segment(writer, context, node_id, node);
    }

    let anchor_tag = node.anchor.rsplit(':').next().unwrap_or(&node.anchor);
    let tag = node
        .xml_tag()
        .map_or_else(|| anchor_tag.to_owned(), ToString::to_string);
    let site = ValueSite::new(node.anchor.clone(), None);
    if !is_xml_qname(&tag) {
        return Err(EmitInstanceError::Emit {
            protocol: context.protocol.to_owned(),
            message: format!("XML element node {node_id} has invalid QName {tag:?}"),
        });
    }
    validate_xml_element_value_structure(context, node_id, node, &site)?;
    let rendered = xml_attributes(node, &site, in_scope_namespaces, context.protocol)?;
    if !xml_qname_is_bound(&tag, &rendered.namespaces) {
        return Err(EmitInstanceError::Emit {
            protocol: context.protocol.to_owned(),
            message: format!("XML element QName {tag:?} uses an unbound namespace prefix"),
        });
    }
    let expanded_tag = expanded_xml_qname(&tag, &rendered.namespaces, true);
    validate_xml_emission_route(context, node_id, node, &tag, expanded_tag.as_deref())?;
    let mut element = BytesStart::new(&tag);
    for (key, value) in &rendered.values {
        element.push_attribute(quick_xml::events::attributes::Attribute {
            key: quick_xml::name::QName(key.as_bytes()),
            value: std::borrow::Cow::Owned(escape_xml_source_text(value, true).into_bytes()),
        });
    }
    write_xml_event(writer, Event::Start(element), context.protocol)?;

    write_xml_scalar_value(writer, context, node_id, node, &site)?;
    write_xml_children(writer, context, node_id, &rendered.namespaces)?;
    write_xml_event(
        writer,
        Event::End(BytesEnd::new(tag.as_str())),
        context.protocol,
    )
}

fn validate_xml_emission_route(
    context: &XmlEmitContext<'_>,
    node_id: u32,
    node: &Node,
    tag: &str,
    expanded_tag: Option<&str>,
) -> Result<(), EmitInstanceError> {
    let route_error = || EmitInstanceError::Emit {
        protocol: context.protocol.to_owned(),
        message: format!(
            "XML tag {tag:?} would not reparse to node {node_id} at schema anchor '{}'",
            node.anchor
        ),
    };
    if node_id == context.instance.root {
        if find_root_by_tag(context.schema, tag, expanded_tag).as_deref()
            == Some(node.anchor.as_ref())
        {
            return Ok(());
        }
        return Err(route_error());
    }

    let Some(parent_id) = context.instance.parent_map.get(&node_id) else {
        return Err(route_error());
    };
    let Some(parent) = context.instance.node(*parent_id) else {
        return Err(route_error());
    };
    if let Some(selected_anchor) =
        find_child_vertex(context.schema, &parent.anchor, tag, expanded_tag)
    {
        if selected_anchor != node.anchor.as_ref() {
            return Err(route_error());
        }
        let Some((_, _, emitted_edge)) = context
            .instance
            .arcs
            .iter()
            .find(|(source, target, _)| source == parent_id && *target == node_id)
        else {
            return Err(route_error());
        };
        if find_schema_edge(
            context.schema,
            &parent.anchor,
            &node.anchor,
            tag,
            expanded_tag,
        ) != *emitted_edge
        {
            return Err(route_error());
        }
    } else {
        let expected_anchor = format!("{}:{tag}", parent.anchor);
        if node.anchor.as_ref() != expected_anchor {
            return Err(route_error());
        }
    }
    Ok(())
}

fn write_xml_event(
    writer: &mut quick_xml::Writer<Vec<u8>>,
    event: Event<'_>,
    protocol: &str,
) -> Result<(), EmitInstanceError> {
    writer
        .write_event(event)
        .map_err(|error| EmitInstanceError::Emit {
            protocol: protocol.to_owned(),
            message: format!("XML write error: {error}"),
        })
}

fn escape_xml_source_text(text: &str, attribute: bool) -> String {
    let escaped = quick_xml::escape::escape(text);
    let mut output = String::with_capacity(escaped.len());
    for character in escaped.chars() {
        match character {
            '\r' => output.push_str("&#xD;"),
            '\n' if attribute => output.push_str("&#xA;"),
            '\t' if attribute => output.push_str("&#x9;"),
            other => output.push(other),
        }
    }
    output
}

struct XmlAttributes {
    values: BTreeMap<String, String>,
    namespaces: HashMap<String, String>,
}

fn xml_attributes(
    node: &Node,
    site: &ValueSite,
    in_scope_namespaces: &HashMap<String, String>,
    protocol: &str,
) -> Result<XmlAttributes, EmitInstanceError> {
    let mut attributes = BTreeMap::new();
    let mut namespaces = in_scope_namespaces.clone();
    let ordered: BTreeMap<&str, &Value> = node
        .extra_fields
        .iter()
        .map(|(key, value)| (key.as_str(), value))
        .collect();
    render_xml_namespace_attributes(&ordered, &mut attributes, &mut namespaces, site, protocol)?;
    for (&key, &value) in ordered
        .iter()
        .filter(|(key, _)| !is_xml_namespace_declaration(key))
    {
        let output_key = if let Some((namespace_uri, local_name)) = parse_xml_clark_name(key) {
            let prefix = if namespace_uri == XML_NS {
                Some("xml")
            } else {
                namespaces
                    .iter()
                    .filter(|(prefix, uri)| !prefix.is_empty() && uri.as_str() == namespace_uri)
                    .map(|(prefix, _)| prefix.as_str())
                    .min()
            };
            let Some(prefix) = prefix else {
                return Err(EmitValueError::new(
                    site.clone(),
                    value.type_name(),
                    "xml",
                    EmitValueReason::NotRepresentable,
                )
                .into());
            };
            format!("{prefix}:{local_name}")
        } else {
            if !is_xml_qname(key) {
                return Err(EmitInstanceError::Emit {
                    protocol: protocol.to_owned(),
                    message: format!("XML attribute has invalid QName or Clark name {key:?}"),
                });
            }
            key.to_owned()
        };
        let rendered = xml_value_text(value, None, site, true)?;
        if attributes.insert(output_key.clone(), rendered).is_some() {
            return Err(EmitInstanceError::Emit {
                protocol: protocol.to_owned(),
                message: format!(
                    "XML attributes map to duplicate written qualified name {output_key:?}"
                ),
            });
        }
    }
    let null = matches!(node.value, Some(FieldPresence::Null));
    let existing_nil = validate_xml_nil_attributes(&attributes, &namespaces, null, site)?;
    if null && existing_nil.is_none() {
        let prefix = xml_schema_instance_prefix(&namespaces);
        if namespaces
            .get(&prefix)
            .is_none_or(|uri| uri != XML_SCHEMA_INSTANCE_NS)
        {
            namespaces.insert(prefix.clone(), XML_SCHEMA_INSTANCE_NS.to_owned());
            attributes.insert(format!("xmlns:{prefix}"), XML_SCHEMA_INSTANCE_NS.to_owned());
        }
        attributes.insert(format!("{prefix}:nil"), "true".to_owned());
    }
    validate_xml_attribute_bindings(&attributes, &namespaces, protocol)?;
    Ok(XmlAttributes {
        values: attributes,
        namespaces,
    })
}

fn render_xml_namespace_attributes(
    ordered: &BTreeMap<&str, &Value>,
    attributes: &mut BTreeMap<String, String>,
    namespaces: &mut HashMap<String, String>,
    site: &ValueSite,
    protocol: &str,
) -> Result<(), EmitInstanceError> {
    for (&key, &value) in ordered
        .iter()
        .filter(|(key, _)| is_xml_namespace_declaration(key))
    {
        if !is_xml_qname(key) {
            return Err(EmitInstanceError::Emit {
                protocol: protocol.to_owned(),
                message: format!("XML attribute has invalid QName {key:?}"),
            });
        }
        let rendered = xml_value_text(value, None, site, true)?;
        if key == "xmlns" {
            if !is_valid_xml_namespace_declaration(key, &rendered) {
                return Err(EmitInstanceError::Emit {
                    protocol: protocol.to_owned(),
                    message: format!("invalid XML namespace declaration {key:?}"),
                });
            }
            namespaces.insert(String::new(), rendered.clone());
        } else if let Some(prefix) = key.strip_prefix("xmlns:") {
            if !is_valid_xml_namespace_declaration(key, &rendered) {
                return Err(EmitInstanceError::Emit {
                    protocol: protocol.to_owned(),
                    message: format!("invalid XML namespace declaration {key:?}"),
                });
            }
            namespaces.insert(prefix.to_owned(), rendered.clone());
        }
        attributes.insert(key.to_owned(), rendered);
    }
    Ok(())
}

fn validate_xml_attribute_bindings(
    attributes: &BTreeMap<String, String>,
    namespaces: &HashMap<String, String>,
    protocol: &str,
) -> Result<(), EmitInstanceError> {
    for key in attributes.keys() {
        if !is_xml_namespace_declaration(key) && !xml_qname_is_bound(key, namespaces) {
            return Err(EmitInstanceError::Emit {
                protocol: protocol.to_owned(),
                message: format!("XML attribute QName {key:?} uses an unbound namespace prefix"),
            });
        }
    }
    Ok(())
}

fn validate_xml_nil_attributes(
    attributes: &BTreeMap<String, String>,
    namespaces: &HashMap<String, String>,
    null: bool,
    site: &ValueSite,
) -> Result<Option<String>, EmitInstanceError> {
    let nil_like: Vec<_> = attributes
        .keys()
        .filter(|key| {
            key.rsplit_once(':')
                .is_some_and(|(_, local)| local == "nil")
        })
        .collect();
    let semantic: Vec<_> = nil_like
        .iter()
        .filter(|key| is_xml_nil_attribute(key, namespaces))
        .collect();
    if semantic.len() > 1 || (null && semantic.is_empty() && !nil_like.is_empty()) {
        return Err(EmitValueError::new(
            site.clone(),
            "str",
            "xml",
            EmitValueReason::NotRepresentable,
        )
        .into());
    }
    let Some(key) = semantic.first() else {
        return Ok(None);
    };
    let consistent = attributes
        .get(key.as_str())
        .and_then(|value| xsd_boolean(value))
        == Some(null);
    if !consistent {
        return Err(EmitValueError::new(
            site.clone(),
            "str",
            "xml",
            EmitValueReason::NotRepresentable,
        )
        .into());
    }
    Ok(Some(key.as_str().to_owned()))
}

/// The truth value of an `xs:boolean` lexeme, or `None` outside its lexical
/// space.
///
/// `xs:boolean` collapses whitespace before it is read, so leading and
/// trailing spaces, tabs, line feeds and carriage returns are dropped; the
/// lexical space is then exactly `true`, `false`, `1` and `0`, with the two
/// words in lower case. Whitespace inside a lexeme makes it invalid, since
/// collapsing would leave a space that no literal contains.
pub(crate) fn xsd_boolean(text: &str) -> Option<bool> {
    match text.trim_matches([' ', '\t', '\n', '\r']) {
        "true" | "1" => Some(true),
        "false" | "0" => Some(false),
        _ => None,
    }
}

/// The typed refusal of an `xsi:nil` value outside the `xs:boolean` lexical
/// space, sited at the element's schema vertex.
pub(crate) fn invalid_xsi_nil(vertex: &str, lexeme: &str) -> ValueFidelityError {
    ValueFidelityError {
        site: ValueSite::new(vertex, None),
        lexeme: lexeme.to_owned(),
        target: ScalarKind::Bool,
        reason: panproto_inst::FidelityReason::InvalidEncoding,
    }
}

fn is_xml_nil_attribute(key: &str, namespaces: &HashMap<String, String>) -> bool {
    key.rsplit_once(':').is_some_and(|(prefix, local)| {
        local == "nil"
            && namespaces
                .get(prefix)
                .is_some_and(|uri| uri == XML_SCHEMA_INSTANCE_NS)
    })
}

fn xml_schema_instance_prefix(namespaces: &HashMap<String, String>) -> String {
    namespaces
        .iter()
        .filter(|(prefix, uri)| !prefix.is_empty() && uri.as_str() == XML_SCHEMA_INSTANCE_NS)
        .map(|(prefix, _)| prefix.clone())
        .min()
        .unwrap_or_else(|| {
            let mut suffix = 0_u32;
            loop {
                let candidate = if suffix == 0 {
                    "xsi".to_owned()
                } else {
                    format!("xsi{suffix}")
                };
                if !namespaces.contains_key(&candidate) {
                    break candidate;
                }
                suffix += 1;
            }
        })
}

fn xml_value_text(
    value: &Value,
    spec: Option<&ValueSpec>,
    site: &ValueSite,
    attribute: bool,
) -> Result<String, EmitInstanceError> {
    // XML Schema's decimal has no infinity or NaN, so no spelling reads back
    // as the same special `Decimal`.
    if matches!(value, Value::Decimal(number) if !number.is_finite()) {
        return Err(EmitValueError::new(
            site.clone(),
            value.type_name(),
            "xml",
            EmitValueReason::NonFinite,
        )
        .into());
    }
    let bigint_text = if !attribute && let Value::BigInt(number) = value {
        Some(
            crate::json_pathway::bigint_lexeme(number, false).map_err(|limit| {
                EmitValueError::new(
                    site.clone(),
                    "bigint",
                    "xml",
                    EmitValueReason::LimitExceeded(limit),
                )
            })?,
        )
    } else {
        None
    };
    if !xml_value_is_representable(value, spec, attribute, bigint_text.as_deref()) {
        return Err(EmitValueError::new(
            site.clone(),
            value.type_name(),
            "xml",
            EmitValueReason::NotRepresentable,
        )
        .into());
    }
    let text = match value {
        Value::Str(text) | Value::Token(text) => text.clone(),
        Value::Int(number) => number.to_string(),
        Value::Float(number) => xml_float_lexeme(*number).ok_or_else(|| {
            EmitValueError::new(
                site.clone(),
                value.type_name(),
                "xml",
                EmitValueReason::NotRepresentable,
            )
        })?,
        Value::Bool(value) => value.to_string(),
        Value::Bytes(bytes) => xml_bytes_text(bytes, spec, site)?,
        Value::Decimal(number) => xml_decimal_lexeme(number).ok_or_else(|| {
            EmitValueError::new(
                site.clone(),
                value.type_name(),
                "xml",
                EmitValueReason::NotRepresentable,
            )
        })?,
        Value::BigInt(_) => bigint_text.ok_or_else(|| {
            EmitValueError::new(
                site.clone(),
                "bigint",
                "xml",
                EmitValueReason::NotRepresentable,
            )
        })?,
        Value::Temporal(value) => value.to_string(),
        Value::Null if attribute => {
            return Err(EmitValueError::new(
                site.clone(),
                value.type_name(),
                "xml",
                EmitValueReason::NullNotRepresentable,
            )
            .into());
        }
        Value::Null => String::new(),
        other => {
            return Err(EmitValueError::new(
                site.clone(),
                other.type_name(),
                "xml",
                EmitValueReason::NotRepresentable,
            )
            .into());
        }
    };
    validate_xml_value_text(value, text, attribute, site)
}

fn validate_xml_value_text(
    value: &Value,
    text: String,
    attribute: bool,
    site: &ValueSite,
) -> Result<String, EmitInstanceError> {
    if !is_xml_10_text(&text) {
        return Err(EmitValueError::new(
            site.clone(),
            value.type_name(),
            "xml",
            EmitValueReason::NotRepresentable,
        )
        .into());
    }
    // An empty element reads as `Str("")` under a text kind, so the empty
    // string is written as one; no other value has empty element text.
    if !attribute && text.is_empty() && !matches!(value, Value::Str(_)) {
        return Err(EmitValueError::new(
            site.clone(),
            value.type_name(),
            "xml",
            EmitValueReason::NotRepresentable,
        )
        .into());
    }
    Ok(text)
}

fn xml_value_is_representable(
    value: &Value,
    spec: Option<&ValueSpec>,
    attribute: bool,
    bigint_text: Option<&str>,
) -> bool {
    if attribute {
        matches!(value, Value::Str(_))
    } else if let Some(text) = bigint_text {
        spec.is_some_and(|spec| crate::json_pathway::plain_text_reparses_as(value, text, spec))
    } else if let Value::Float(number) = value {
        xml_float_lexeme(*number).is_some_and(|text| {
            spec.is_some_and(|spec| crate::json_pathway::plain_text_reparses_as(value, &text, spec))
        })
    } else if let Value::Decimal(number) = value {
        xml_decimal_lexeme(number).is_some_and(|text| {
            spec.is_some_and(|spec| crate::json_pathway::plain_text_reparses_as(value, &text, spec))
        })
    } else {
        spec.is_some_and(|spec| crate::json_pathway::plain_text_value_roundtrips(value, spec))
    }
}

fn xml_decimal_lexeme(number: &panproto_inst::value::DecimalValue) -> Option<String> {
    if !number.is_finite() || (!number.exponent.is_negative() && !number.exponent.is_zero()) {
        return None;
    }
    let scale = if number.exponent.is_zero() {
        0
    } else {
        number
            .exponent
            .negated()
            .to_i64()
            .and_then(|scale| usize::try_from(scale).ok())?
    };
    if scale > MAX_XML_DECIMAL_FRACTION_DIGITS {
        return None;
    }
    let unsigned_len = if scale == 0 {
        number.digits.len()
    } else if scale < number.digits.len() {
        number.digits.len().checked_add(1)?
    } else {
        scale.checked_add(2)?
    };
    let output_len = unsigned_len.checked_add(usize::from(number.negative))?;
    if u64::try_from(output_len).ok()? > crate::json_pathway::DEFAULT_OUTPUT_BYTES {
        return None;
    }

    let mut output = String::with_capacity(output_len);
    if number.negative {
        output.push('-');
    }
    if scale == 0 {
        output.push_str(&number.digits);
    } else if scale < number.digits.len() {
        let split = number.digits.len() - scale;
        output.push_str(&number.digits[..split]);
        output.push('.');
        output.push_str(&number.digits[split..]);
    } else {
        output.push_str("0.");
        output.extend(std::iter::repeat_n('0', scale - number.digits.len()));
        output.push_str(&number.digits);
    }
    Some(output)
}

fn xml_bytes_text(
    bytes: &[u8],
    spec: Option<&ValueSpec>,
    site: &ValueSite,
) -> Result<String, EmitInstanceError> {
    let encoding = spec.and_then(|spec| match &spec.kind {
        ScalarKind::Bytes(encoding) => Some(encoding),
        _ => None,
    });
    let Some(encoding) = encoding else {
        return Err(EmitValueError::new(
            site.clone(),
            "bytes",
            "xml",
            EmitValueReason::NotRepresentable,
        )
        .into());
    };
    encode_bytes(bytes, encoding).ok_or_else(|| {
        EmitValueError::new(
            site.clone(),
            "bytes",
            "xml",
            EmitValueReason::NotRepresentable,
        )
        .into()
    })
}

/// Find a child vertex reachable from `parent_vertex` via an edge matching `tag`.
/// Ingest one XML element (either `Event::Start` or `Event::Empty`),
/// creating a fresh `Node` in `state`, linking it to the current parent
/// (if any), and returning `(node_id, vertex_id)` for the caller to
/// push onto its element stack if needed.
// Element ingestion is one transactional parser transition: route selection,
// DTD-aware attributes, node insertion, and parent/root linkage must share the
// same stack and expansion-budget snapshot.
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
fn ingest_xml_element(
    e: &quick_xml::events::BytesStart<'_>,
    namespaces: &NamespaceResolver,
    schema: &Schema,
    state: &mut XmlParseState,
    element_stack: &[(u32, String)],
    root: &mut XmlDocumentRoot,
    protocol: &str,
    dtd: &XmlDtd,
    entity_expansion_bytes: &mut usize,
    byte_offset: Option<u64>,
) -> Result<(u32, String), ParseInstanceError> {
    if element_stack.is_empty() && root.id.is_some() {
        return Err(ParseInstanceError::Parse {
            protocol: protocol.to_owned(),
            message: "XML document has more than one root element".into(),
        });
    }
    let tag = e
        .decoder()
        .decode(e.name().as_ref())
        .map_err(|error| ParseInstanceError::Parse {
            protocol: protocol.to_owned(),
            message: format!("XML element name decode error: {error}"),
        })?
        .into_owned();
    if element_stack.is_empty()
        && dtd
            .root_name
            .as_deref()
            .is_some_and(|root_name| root_name != tag)
    {
        return Err(ParseInstanceError::Parse {
            protocol: protocol.to_owned(),
            message: format!(
                "XML document element {tag:?} does not match its declared document type"
            ),
        });
    }
    let (namespace, local_name) = namespaces.resolve_element(e.name());
    if !is_xml_qname(&tag)
        || xml_qname_prefix(&tag) == Some("xmlns")
        || matches!(namespace, ResolveResult::Unknown(_))
    {
        return Err(ParseInstanceError::Parse {
            protocol: protocol.to_owned(),
            message: format!("XML element {tag:?} uses an invalid or unbound namespace prefix"),
        });
    }
    let local_name = e
        .decoder()
        .decode(local_name.as_ref())
        .map_err(|error| ParseInstanceError::Parse {
            protocol: protocol.to_owned(),
            message: format!("XML element local-name decode error: {error}"),
        })?
        .into_owned();
    let expanded_tag = match namespace {
        ResolveResult::Bound(namespace) => {
            let namespace = e.decoder().decode(namespace.as_ref()).map_err(|error| {
                ParseInstanceError::Parse {
                    protocol: protocol.to_owned(),
                    message: format!("XML namespace URI decode error: {error}"),
                }
            })?;
            Some(xml_clark_name(&namespace, &local_name))
        }
        ResolveResult::Unbound => None,
        ResolveResult::Unknown(_) => unreachable!("unknown prefixes were refused above"),
    };
    if element_stack.last().is_some_and(|(parent, _)| {
        state
            .nodes
            .get(parent)
            .is_some_and(|node| matches!(node.value, Some(FieldPresence::Null)))
    }) {
        return Err(ParseInstanceError::Parse {
            protocol: protocol.to_owned(),
            message: "an xsi:nil element cannot contain a child element".into(),
        });
    }
    let node_id = state.alloc_id();

    let vertex_id = match element_stack.last() {
        None => find_root_by_tag(schema, &tag, expanded_tag.as_deref()).ok_or_else(|| {
            ParseInstanceError::SchemaMismatch {
                protocol: protocol.to_owned(),
                message: missing_entry_message(None, schema),
            }
        })?,
        Some(parent) => find_child_vertex(schema, &parent.1, &tag, expanded_tag.as_deref())
            .unwrap_or_else(|| format!("{}:{}", parent.1, tag)),
    };
    let (extra_fields, nil) = parse_xml_element_attributes(
        e,
        namespaces,
        &vertex_id,
        &tag,
        protocol,
        dtd,
        entity_expansion_bytes,
        byte_offset,
    )?;

    let node = Node {
        id: node_id,
        anchor: panproto_gat::Name::from(vertex_id.as_str()),
        value: nil.then_some(FieldPresence::Null),
        discriminator: None,
        extra_fields,
        position: None,
        shape: NodeShape::XmlElement {
            tag: tag.as_str().into(),
        },
        annotations: HashMap::new(),
    };
    state.nodes.insert(node_id, node);

    if let Some(parent) = element_stack.last() {
        let edge = find_schema_edge(schema, &parent.1, &vertex_id, &tag, expanded_tag.as_deref());
        state.arcs.push((parent.0, node_id, edge));
    } else {
        root.id = Some(node_id);
        root.vertex.clone_from(&vertex_id);
    }

    Ok((node_id, vertex_id))
}

// Explicit and defaulted attributes deliberately share one namespace view,
// duplicate-name set, xsi:nil state, and entity-byte budget. Keeping both
// passes together makes the DTD defaulting transition auditable.
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
fn parse_xml_element_attributes(
    element: &quick_xml::events::BytesStart<'_>,
    namespaces: &NamespaceResolver,
    vertex: &str,
    element_name: &str,
    protocol: &str,
    dtd: &XmlDtd,
    entity_expansion_bytes: &mut usize,
    element_byte_offset: Option<u64>,
) -> Result<(HashMap<String, Value>, bool), ParseInstanceError> {
    let mut extra_fields = HashMap::new();
    let mut nil = false;
    let mut explicit_attributes = BTreeMap::new();
    let declarations = dtd.attributes.get(element_name);
    let element_source: &[u8] = element.as_ref();
    let name_offsets = xml_attribute_name_offsets(element_source);
    let mut identities = HashSet::new();
    let mut attribute_search_start = 0_usize;
    // Duplicates are decided below by expanded name, which also covers equal
    // raw names, so quick-xml's raw-name check would only report the same
    // refusal untyped.
    let mut attributes = element.attributes();
    attributes.with_checks(false);
    for (ordinal, attribute) in attributes.enumerate() {
        let attribute = attribute.map_err(|error| ParseInstanceError::Parse {
            protocol: protocol.to_owned(),
            message: format!("XML attribute error: {error}"),
        })?;
        let key = element
            .decoder()
            .decode(attribute.key.as_ref())
            .map_err(|error| ParseInstanceError::Parse {
                protocol: protocol.to_owned(),
                message: format!("XML attribute name decode error: {error}"),
            })?
            .into_owned();
        let raw_value = element
            .decoder()
            .decode(attribute.value.as_ref())
            .map_err(|error| ParseInstanceError::Parse {
                protocol: protocol.to_owned(),
                message: format!("XML attribute value error: {error}"),
            })?
            .into_owned();
        let raw_bytes = attribute.value.as_ref();
        let raw_relative_offset = if raw_bytes.is_empty() {
            None
        } else {
            element_source
                .get(attribute_search_start..)
                .and_then(|remaining| {
                    remaining
                        .windows(raw_bytes.len())
                        .position(|part| part == raw_bytes)
                })
                .and_then(|relative| attribute_search_start.checked_add(relative))
        };
        if let Some(relative) = raw_relative_offset {
            attribute_search_start = relative.saturating_add(raw_bytes.len());
        }
        let raw_byte_offset = element_byte_offset.and_then(|offset| {
            raw_relative_offset
                .and_then(|relative| u64::try_from(relative).ok())
                .and_then(|relative| offset.checked_add(1)?.checked_add(relative))
        });
        let non_cdata = declarations
            .and_then(|declarations| declarations.get(&key))
            .is_some_and(|declaration| declaration.non_cdata);
        // Explicit general-entity references were charged in decoded source
        // order while preparing content. Normalize them here without charging
        // a second time; defaulted declaration values below still charge the
        // shared document budget when they are supplied.
        let mut already_charged = 0_usize;
        let value = normalize_xml_attribute_value(
            &raw_value,
            non_cdata,
            &dtd.entities,
            &mut already_charged,
        )
        .map_err(|source| {
            let offset = xml_attribute_error_offset(&raw_value, raw_byte_offset, &source);
            xml_entity_error(protocol, source, offset)
        })?;
        validate_parsed_xml_text(&value, protocol)?;
        if is_xml_namespace_declaration(&key) && !is_valid_xml_namespace_declaration(&key, &value) {
            return Err(ParseInstanceError::Parse {
                protocol: protocol.to_owned(),
                message: format!("invalid XML namespace declaration {key:?}"),
            });
        }
        let (namespace, local_name) = namespaces.resolve_attribute(attribute.key);
        match &namespace {
            ResolveResult::Bound(namespace)
                if namespace.as_ref() == XML_SCHEMA_INSTANCE_NS.as_bytes()
                    && local_name.as_ref() == b"nil" =>
            {
                nil = xsd_boolean(&value).ok_or_else(|| {
                    ParseInstanceError::Fidelity(vec![invalid_xsi_nil(vertex, &value)])
                })?;
            }
            ResolveResult::Unknown(prefix) => {
                return Err(ParseInstanceError::Parse {
                    protocol: protocol.to_owned(),
                    message: format!(
                        "XML attribute {key:?} uses unbound namespace prefix {:?}",
                        String::from_utf8_lossy(prefix)
                    ),
                });
            }
            ResolveResult::Unbound | ResolveResult::Bound(_) => {}
        }
        let (identity, semantic_key) = if is_xml_namespace_declaration(&key) {
            (xml_expanded_name(XMLNS_NAMESPACE, &key), key.clone())
        } else if let ResolveResult::Bound(namespace) = &namespace {
            let namespace = element
                .decoder()
                .decode(namespace.as_ref())
                .map_err(|error| ParseInstanceError::Parse {
                    protocol: protocol.to_owned(),
                    message: format!("XML attribute namespace URI decode error: {error}"),
                })?;
            let local_name = element
                .decoder()
                .decode(local_name.as_ref())
                .map_err(|error| ParseInstanceError::Parse {
                    protocol: protocol.to_owned(),
                    message: format!("XML attribute local-name decode error: {error}"),
                })?;
            (
                xml_expanded_name(&namespace, &local_name),
                xml_clark_name(&namespace, &local_name),
            )
        } else {
            (xml_expanded_name("", &key), key.clone())
        };
        if !identities.insert(identity) {
            return Err(ParseInstanceError::DuplicateMember {
                protocol: protocol.to_owned(),
                name: semantic_key,
                byte_offset: element_byte_offset.and_then(|offset| {
                    let relative = u64::try_from(*name_offsets.get(ordinal)?).ok()?;
                    offset.checked_add(1)?.checked_add(relative)
                }),
            });
        }
        extra_fields.insert(semantic_key, Value::Str(value.clone()));
        explicit_attributes.insert(key, value);
    }
    if let Some(declarations) = declarations {
        for (key, declaration) in declarations {
            if let Some(explicit) = explicit_attributes.get(key) {
                if let XmlAttributeDefault::Fixed(value) = &declaration.default {
                    let fixed = normalize_xml_attribute_value(
                        value,
                        declaration.non_cdata,
                        &dtd.entities,
                        entity_expansion_bytes,
                    )
                    .map_err(|source| {
                        let offset = xml_stored_doctype_error_offset(dtd, &source);
                        xml_entity_error(protocol, source, offset)
                    })?;
                    if fixed != *explicit {
                        return Err(ParseInstanceError::Parse {
                            protocol: protocol.to_owned(),
                            message: format!(
                                "XML #FIXED attribute {key:?} on element {element_name:?} has a different value"
                            ),
                        });
                    }
                }
                continue;
            }
            let normalized_default = match &declaration.default {
                XmlAttributeDefault::Required | XmlAttributeDefault::Implied => None,
                XmlAttributeDefault::Value(value) | XmlAttributeDefault::Fixed(value) => Some(
                    normalize_xml_attribute_value(
                        value,
                        declaration.non_cdata,
                        &dtd.entities,
                        entity_expansion_bytes,
                    )
                    .map_err(|source| {
                        let offset = xml_stored_doctype_error_offset(dtd, &source);
                        xml_entity_error(protocol, source, offset)
                    })?,
                ),
            };
            let Some(value) = normalized_default else {
                continue;
            };
            validate_parsed_xml_text(&value, protocol)?;
            if is_xml_namespace_declaration(key) && !is_valid_xml_namespace_declaration(key, &value)
            {
                return Err(ParseInstanceError::Parse {
                    protocol: protocol.to_owned(),
                    message: format!("invalid XML namespace declaration {key:?}"),
                });
            }
            let (namespace, local_name) = namespaces.resolve_attribute(QName(key.as_bytes()));
            let semantic_key = if is_xml_namespace_declaration(key) {
                key.clone()
            } else if let ResolveResult::Bound(namespace) = &namespace {
                let namespace = element
                    .decoder()
                    .decode(namespace.as_ref())
                    .map_err(|error| ParseInstanceError::Parse {
                        protocol: protocol.to_owned(),
                        message: format!("XML attribute namespace URI decode error: {error}"),
                    })?;
                let local_name =
                    element
                        .decoder()
                        .decode(local_name.as_ref())
                        .map_err(|error| ParseInstanceError::Parse {
                            protocol: protocol.to_owned(),
                            message: format!("XML attribute local-name decode error: {error}"),
                        })?;
                xml_clark_name(&namespace, &local_name)
            } else if let ResolveResult::Unknown(prefix) = &namespace {
                return Err(ParseInstanceError::Parse {
                    protocol: protocol.to_owned(),
                    message: format!(
                        "XML defaulted attribute {key:?} uses unbound namespace prefix {:?}",
                        String::from_utf8_lossy(prefix)
                    ),
                });
            } else {
                key.clone()
            };
            if let ResolveResult::Bound(namespace) = &namespace
                && namespace.as_ref() == XML_SCHEMA_INSTANCE_NS.as_bytes()
                && local_name.as_ref() == b"nil"
            {
                nil = xsd_boolean(&value).ok_or_else(|| {
                    ParseInstanceError::Fidelity(vec![invalid_xsi_nil(vertex, &value)])
                })?;
            }
            // A supplied default has no name token in the start tag, so the
            // refusal carries no offset.
            if extra_fields
                .insert(semantic_key.clone(), Value::Str(value))
                .is_some()
            {
                return Err(ParseInstanceError::DuplicateMember {
                    protocol: protocol.to_owned(),
                    name: semantic_key,
                    byte_offset: None,
                });
            }
        }
    }
    Ok((extra_fields, nil))
}

fn find_child_vertex(
    schema: &Schema,
    parent_vertex: &str,
    tag: &str,
    expanded_tag: Option<&str>,
) -> Option<String> {
    let edges = schema.outgoing_edges(parent_vertex);
    // Clark notation takes precedence when the schema opts into namespace
    // identity. A raw QName retains the historical prefix-sensitive route.
    if let Some(expanded_tag) = expanded_tag {
        for edge in edges {
            if edge.name.as_deref() == Some(expanded_tag) {
                return Some(edge.tgt.to_string());
            }
        }
    }
    for edge in edges {
        if edge.name.as_deref() == Some(tag) {
            return Some(edge.tgt.to_string());
        }
    }
    // Then try matching target vertex kind.
    for edge in schema.outgoing_edges(parent_vertex) {
        if let Some(v) = schema.vertices.get(&edge.tgt) {
            if expanded_tag.is_some_and(|expanded| v.kind == expanded) || v.kind == tag {
                return Some(edge.tgt.to_string());
            }
        }
    }
    None
}

/// Find the schema vertex that anchors a document element with `tag`.
///
/// A vertex whose ID is the tag comes first, then the least root vertex whose
/// kind is the tag, then the schema's primary entry. The result is always a
/// vertex of `schema`; `None` means the schema has no usable entry.
fn find_root_by_tag(schema: &Schema, tag: &str, expanded_tag: Option<&str>) -> Option<String> {
    // Try exact match on vertex ID first.
    if let Some(vertex) = expanded_tag.and_then(|expanded| schema.vertices.get(expanded)) {
        return Some(vertex.id.to_string());
    }
    if let Some(vertex) = schema.vertices.get(tag) {
        return Some(vertex.id.to_string());
    }
    // Then match on vertex kind.
    schema
        .vertices
        .values()
        .filter(|v| {
            (expanded_tag.is_some_and(|expanded| v.kind == expanded) || v.kind == tag)
                && schema.incoming_edges(&v.id).is_empty()
        })
        .min_by(|left, right| left.id.cmp(&right.id))
        .map(|v| v.id.to_string())
        .or_else(|| resolve_entry_vertex(None, schema).map(ToString::to_string))
}

/// Find or construct a schema edge between parent and child vertices.
fn find_schema_edge(
    schema: &Schema,
    parent: &str,
    child: &str,
    tag: &str,
    expanded_tag: Option<&str>,
) -> Edge {
    let edges = schema.edges_between(parent, child);
    if let Some(edge) = expanded_tag.and_then(|expanded| {
        edges
            .iter()
            .find(|edge| edge.name.as_deref() == Some(expanded))
    }) {
        return edge.clone();
    }
    if let Some(edge) = edges.iter().find(|edge| edge.name.as_deref() == Some(tag)) {
        return edge.clone();
    }
    if let Some(edge) = edges.first() {
        return edge.clone();
    }
    // Fallback: construct a synthetic edge.
    Edge {
        src: parent.into(),
        tgt: child.into(),
        kind: "contains".into(),
        name: Some(tag.to_string().into()),
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use panproto_schema::SchemaBuilder;

    #[test]
    fn bounded_xml_entity_expansion_covers_success_and_typed_refusals() {
        let declarations = BTreeMap::from([
            ("a".to_owned(), "A &amp; B".to_owned()),
            ("b".to_owned(), "&a; / &#x41; / &#65;".to_owned()),
        ]);
        assert_eq!(
            expand_xml_entities("<&b;>", &declarations),
            Ok("<A & B / A / A>".to_owned())
        );
        assert_eq!(
            expand_xml_entities("&missing;", &declarations),
            Err(XmlEntityError::Undeclared("missing".to_owned()))
        );
        assert_eq!(
            expand_xml_entities("&#0;", &declarations),
            Err(XmlEntityError::Malformed)
        );
        assert_eq!(
            expand_xml_entities("&unfinished", &declarations),
            Err(XmlEntityError::Malformed)
        );

        let recursive = BTreeMap::from([
            ("a".to_owned(), "&b;".to_owned()),
            ("b".to_owned(), "&a;".to_owned()),
        ]);
        assert_eq!(
            expand_xml_entities("&a;", &recursive),
            Err(XmlEntityError::Recursive("a".to_owned()))
        );
    }

    #[test]
    fn bounded_xml_entity_expansion_enforces_depth_and_byte_limits() {
        let mut nested = BTreeMap::new();
        for depth in 0..=MAX_XML_ENTITY_DEPTH {
            nested.insert(format!("e{depth}"), format!("&e{};", depth + 1));
        }
        nested.insert(format!("e{}", MAX_XML_ENTITY_DEPTH + 1), "x".to_owned());
        assert_eq!(
            expand_xml_entities("&e0;", &nested),
            Err(XmlEntityError::Depth)
        );

        let over_budget = BTreeMap::from([(
            "large".to_owned(),
            "x".repeat(MAX_XML_ENTITY_EXPANSION_BYTES + 1),
        )]);
        assert_eq!(
            expand_xml_entities("&large;", &over_budget),
            Err(XmlEntityError::Bytes)
        );
    }

    #[test]
    fn xml_decimal_emission_uses_only_bounded_xsd_positional_grammar() {
        use panproto_inst::value::DecimalValue;

        for (source, expected) in [
            ("0", "0"),
            ("-0.00", "-0.00"),
            ("12.30", "12.30"),
            (".5", "0.5"),
            ("1e-3", "0.001"),
        ] {
            let decimal = DecimalValue::parse(source).expect("decimal fixture");
            assert_eq!(xml_decimal_lexeme(&decimal).as_deref(), Some(expected));
        }
        for source in ["0e1", "1e1", "1e-70000000", "Infinity", "NaN"] {
            let decimal = DecimalValue::parse(source).expect("decimal fixture");
            assert_eq!(xml_decimal_lexeme(&decimal), None, "{source}");
        }

        let value = Value::Decimal(DecimalValue::parse("0e1").expect("decimal fixture"));
        let spec = ValueSpec::new(ScalarKind::Decimal);
        let error = xml_value_text(&value, Some(&spec), &ValueSite::new("doc", None), false)
            .expect_err("positive-exponent decimal is outside XSD decimal grammar");
        assert!(matches!(
            error,
            EmitInstanceError::Value(EmitValueError {
                reason: EmitValueReason::NotRepresentable,
                ..
            })
        ));

        let boundary = DecimalValue::new(
            false,
            "1",
            -i64::try_from(MAX_XML_DECIMAL_FRACTION_DIGITS).unwrap_or(i64::MAX),
        )
        .expect("boundary decimal");
        let boundary_text = xml_decimal_lexeme(&boundary).expect("boundary is admitted");
        assert_eq!(boundary_text.len(), MAX_XML_DECIMAL_FRACTION_DIGITS + 2);
        assert!(boundary_text.starts_with("0."));
        assert!(boundary_text.ends_with('1'));

        let outside = DecimalValue::new(
            false,
            "1",
            -i64::try_from(MAX_XML_DECIMAL_FRACTION_DIGITS + 1).unwrap_or(i64::MAX),
        )
        .expect("outside decimal");
        assert_eq!(xml_decimal_lexeme(&outside), None);
    }

    #[test]
    fn xml_route_expands_internal_entities_in_content_and_attributes() {
        let schema = xml_schema();
        let input = br#"<!DOCTYPE doc [<!ENTITY e "x">]><doc a="&e;">&e;</doc>"#;
        let instance = parse_xml_bytes(&schema, input, "test_xml").expect("parse entities");
        let root = instance.node(instance.root).expect("root");
        assert_eq!(
            root.value,
            Some(FieldPresence::Present(Value::Str("x".into())))
        );
        assert_eq!(root.extra_fields.get("a"), Some(&Value::Str("x".into())));
    }

    #[test]
    fn xml_entity_replacement_markup_is_content_but_not_attribute_syntax() {
        let schema = xml_schema();
        let content = br#"<!DOCTYPE doc [<!ENTITY e "<body>z</body>">]><doc>&e;</doc>"#;
        let instance = parse_xml_bytes(&schema, content, "test_xml").expect("content markup");
        let body = instance
            .nodes
            .values()
            .find(|node| node.anchor.as_ref() == "doc:body")
            .expect("entity replacement contributes an element");
        assert_eq!(
            body.value,
            Some(FieldPresence::Present(Value::Str("z".to_owned())))
        );

        let attribute = br#"<!DOCTYPE doc [<!ENTITY e "<body/>">]><doc a="&e;"/>"#;
        assert!(matches!(
            parse_xml_bytes(&schema, attribute, "test_xml"),
            Err(ParseInstanceError::Parse { protocol, message })
                if protocol == "test_xml" && message.contains("malformed XML entity")
        ));
    }

    #[test]
    fn declared_empty_xml_text_leaf_is_present_not_absent() {
        let protocol = panproto_protocols::database::cassandra::protocol();
        let schema = SchemaBuilder::new(&protocol)
            .vertex("root", "table", None)
            .expect("root")
            .vertex("value", "text", None)
            .expect("text value")
            .edge("root", "value", "prop", Some("value"))
            .expect("value edge")
            .build()
            .expect("text schema");
        let instance = parse_xml_bytes(&schema, b"<root><value></value></root>", "cassandra")
            .expect("empty text leaf");
        let value = instance
            .nodes
            .values()
            .find(|node| node.anchor.as_ref() == "value")
            .expect("present value node");
        assert_eq!(
            value.value,
            Some(FieldPresence::Present(Value::Str(String::new())))
        );
    }

    #[test]
    fn xml_floats_use_the_xsd_numeral_dialect() {
        let protocol = panproto_protocols::database::cassandra::protocol();
        let schema = SchemaBuilder::new(&protocol)
            .vertex("root", "table", None)
            .expect("root")
            .vertex("value", "double", None)
            .expect("double value")
            .edge("root", "value", "prop", Some("value"))
            .expect("value edge")
            .build()
            .expect("double schema");
        for (value, spelling) in [
            (f64::INFINITY, "INF"),
            (f64::NEG_INFINITY, "-INF"),
            (f64::NAN, "NaN"),
            (-f64::NAN, "-NaN"),
        ] {
            let mut instance = WInstance::new(
                HashMap::from([
                    (0, Node::new(0, "root")),
                    (
                        1,
                        Node::new(1, "value")
                            .with_value(FieldPresence::Present(Value::Float(value))),
                    ),
                ]),
                vec![(
                    0,
                    1,
                    Edge {
                        src: "root".into(),
                        tgt: "value".into(),
                        kind: "prop".into(),
                        name: Some("value".into()),
                    },
                )],
                Vec::new(),
                0,
                "root".into(),
            );
            for node in instance.nodes.values_mut() {
                node.shape = NodeShape::XmlElement {
                    tag: node.anchor.clone(),
                };
            }
            let emitted = emit_xml_bytes(&schema, &instance, "cassandra").expect("XSD spelling");
            let text = String::from_utf8(emitted.clone()).expect("UTF-8");
            assert!(
                text.contains(&format!("<value>{spelling}</value>")),
                "{text}"
            );
            let reparsed = parse_xml_bytes(&schema, &emitted, "cassandra").expect("reparse");
            let read = reparsed
                .nodes
                .values()
                .find(|node| node.anchor.as_ref() == "value")
                .and_then(|node| node.value.clone());
            assert!(
                matches!(read, Some(FieldPresence::Present(Value::Float(actual))) if actual.to_bits() == value.to_bits()),
                "{spelling}: {read:?}"
            );
        }
        for lexeme in ["Infinity", "inf", "nan", "+NaN"] {
            let input = format!("<root><value>{lexeme}</value></root>");
            assert!(
                matches!(
                    parse_xml_bytes(&schema, input.as_bytes(), "cassandra"),
                    Err(ParseInstanceError::Fidelity(_))
                ),
                "{lexeme} is not an XSD double"
            );
        }
    }

    #[test]
    fn an_empty_string_value_round_trips_through_an_empty_element() {
        let protocol = panproto_protocols::database::cassandra::protocol();
        let schema = SchemaBuilder::new(&protocol)
            .vertex("root", "table", None)
            .expect("root")
            .vertex("value", "text", None)
            .expect("text value")
            .edge("root", "value", "prop", Some("value"))
            .expect("value edge")
            .build()
            .expect("text schema");
        let source = b"<root><value></value></root>";
        let instance = parse_xml_bytes(&schema, source, "cassandra").expect("empty text leaf");
        let emitted = emit_xml_bytes(&schema, &instance, "cassandra")
            .expect("the value an empty element reads as is writable");
        let reparsed = parse_xml_bytes(&schema, &emitted, "cassandra").expect("reparse");
        let value = reparsed
            .nodes
            .values()
            .find(|node| node.anchor.as_ref() == "value")
            .expect("present value node");
        assert_eq!(
            value.value,
            Some(FieldPresence::Present(Value::Str(String::new())))
        );
    }

    fn utf16_xml(text: &str, little_endian: bool) -> Vec<u8> {
        let mut bytes = if little_endian {
            vec![0xFF, 0xFE]
        } else {
            vec![0xFE, 0xFF]
        };
        for unit in text.encode_utf16() {
            if little_endian {
                bytes.extend_from_slice(&unit.to_le_bytes());
            } else {
                bytes.extend_from_slice(&unit.to_be_bytes());
            }
        }
        bytes
    }

    #[test]
    fn canonical_xml_decodes_required_encodings_without_windows_1252_aliases() {
        let schema = xml_schema();
        let utf16_text = "<?xml version=\"1.0\" encoding=\"UTF-16\"?><doc><body>x</body></doc>";
        for little_endian in [true, false] {
            let instance =
                parse_xml_bytes(&schema, &utf16_xml(utf16_text, little_endian), "test_xml")
                    .expect("UTF-16 XML");
            assert!(instance.nodes.values().any(|node| {
                node.anchor.as_ref() == "doc:body"
                    && node.value == Some(FieldPresence::Present(Value::Str("x".to_owned())))
            }));
            let emitted = emit_xml_bytes(&schema, &instance, "test_xml").expect("UTF-8 emit");
            assert!(!emitted.starts_with(&[0xEF, 0xBB, 0xBF]));
            let emitted = std::str::from_utf8(&emitted).expect("canonical UTF-8");
            assert!(!emitted.contains("<?xml"));
            assert!(!emitted.contains("encoding="));
        }

        let mut latin1 = b"<?xml version=\"1.0\" encoding=\"ISO-8859-1\"?><doc><body>".to_vec();
        latin1.push(0x80);
        latin1.extend_from_slice(b"</body></doc>");
        let instance = parse_xml_bytes(&schema, &latin1, "test_xml").expect("ISO-8859-1");
        assert!(instance.nodes.values().any(|node| {
            node.anchor.as_ref() == "doc:body"
                && node.value == Some(FieldPresence::Present(Value::Str("\u{80}".to_owned())))
        }));
    }

    #[test]
    fn canonical_xml_encoding_refusals_use_typed_source_byte_offsets() {
        let schema = xml_schema();
        let mut ascii = b"<?xml version=\"1.0\" encoding=\"US-ASCII\"?><doc><body>".to_vec();
        let bad_offset = u64::try_from(ascii.len()).expect("offset");
        ascii.push(0x80);
        ascii.extend_from_slice(b"</body></doc>");
        assert!(matches!(
            parse_xml_bytes(&schema, &ascii, "test_xml"),
            Err(ParseInstanceError::Fidelity(errors))
                if matches!(errors.as_slice(), [error]
                    if error.reason == panproto_inst::FidelityReason::InvalidEncoding
                        && error.site.byte_offset == Some(bad_offset))
        ));

        let unknown = b"<?xml version=\"1.0\" encoding=\"x-unknown\"?><doc><body>x</body></doc>";
        assert!(matches!(
            parse_xml_bytes(&schema, unknown, "test_xml"),
            Err(ParseInstanceError::UnsupportedFeature {
                protocol,
                feature,
                ..
            }) if protocol == "test_xml" && feature.contains("x-unknown")
        ));
    }

    #[test]
    fn utf16_value_fidelity_offsets_count_original_source_bytes() {
        let protocol = panproto_protocols::database::cassandra::protocol();
        let schema = SchemaBuilder::new(&protocol)
            .vertex("root", "table", None)
            .expect("root")
            .vertex("value", "decimal", None)
            .expect("decimal value")
            .edge("root", "value", "prop", Some("value"))
            .expect("value edge")
            .build()
            .expect("decimal schema");
        let text = "<?xml version=\"1.0\" encoding=\"UTF-16\"?><root><value>x</value></root>";
        let marker = "<value>";
        let value_start = text.find(marker).expect("value element") + marker.len();
        let preceding_code_units = text[..value_start].encode_utf16().count();
        let expected =
            2_u64 + 2 * u64::try_from(preceding_code_units).expect("UTF-16 source byte offset");
        let input = utf16_xml(text, true);
        let result = parse_xml_bytes(&schema, &input, "cassandra");
        assert!(
            matches!(
                &result,
                Err(ParseInstanceError::Fidelity(errors))
                    if matches!(errors.as_slice(), [error]
                        if error.site.byte_offset == Some(expected))
            ),
            "expected source byte offset {expected}, got {result:?}"
        );
    }

    #[test]
    fn xml_route_supplies_defaults_and_normalizes_declared_attribute_types() {
        let schema = xml_schema();
        let input = br#"<!DOCTYPE doc [
            <!ENTITY z " z ">
            <!ATTLIST doc b CDATA "&z;" c NMTOKENS #IMPLIED f NMTOKEN #FIXED " fixed " n CDATA "&#10;">
        ]><doc c="  p   q " f="fixed" tab="&#9;" line="a
b"/>"#;
        let instance = parse_xml_bytes(&schema, input, "test_xml").expect("parse defaults");
        let root = instance.node(instance.root).expect("root");
        assert_eq!(root.extra_fields.get("b"), Some(&Value::Str(" z ".into())));
        assert_eq!(root.extra_fields.get("c"), Some(&Value::Str("p q".into())));
        assert_eq!(
            root.extra_fields.get("f"),
            Some(&Value::Str("fixed".into()))
        );
        assert_eq!(root.extra_fields.get("n"), Some(&Value::Str("\n".into())));
        assert_eq!(root.extra_fields.get("tab"), Some(&Value::Str("\t".into())));
        assert_eq!(
            root.extra_fields.get("line"),
            Some(&Value::Str("a b".into()))
        );

        let absent_fixed = br#"<!DOCTYPE doc [<!ATTLIST doc f NMTOKEN #FIXED " fixed ">]><doc/>"#;
        let instance =
            parse_xml_bytes(&schema, absent_fixed, "test_xml").expect("supply fixed default");
        assert_eq!(
            instance
                .node(instance.root)
                .expect("root")
                .extra_fields
                .get("f"),
            Some(&Value::Str("fixed".into()))
        );
    }

    #[test]
    fn xml_route_rejects_a_fixed_attribute_mismatch() {
        let schema = xml_schema();
        let input = br#"<!DOCTYPE doc [<!ATTLIST doc f NMTOKEN #FIXED "fixed">]><doc f="other"/>"#;
        assert!(matches!(
            parse_xml_bytes(&schema, input, "test_xml"),
            Err(ParseInstanceError::Parse { message, .. })
                if message.contains("#FIXED") && message.contains('f')
        ));
    }

    #[test]
    fn xml_default_entity_error_points_at_the_nested_reference() {
        let schema = xml_schema();
        let input =
            br#"<!DOCTYPE doc [<!ENTITY a "&missing;"><!ATTLIST doc x CDATA "&a;">]><doc/>"#;
        let expected = input
            .windows(b"&missing;".len())
            .position(|window| window == b"&missing;")
            .and_then(|offset| u64::try_from(offset).ok());
        assert!(matches!(
            parse_xml_bytes(&schema, input, "test_xml"),
            Err(ParseInstanceError::Parse { protocol, message })
                if protocol == "test_xml"
                    && message == format!(
                        "XML entity \"missing\" is undeclared at byte {expected:?}"
                    )
        ));
    }

    #[test]
    fn xml_route_rejects_undeclared_and_recursive_entities_by_name() {
        let schema = xml_schema();
        assert!(matches!(
            parse_xml_bytes(&schema, b"<doc>&u;</doc>", "test_xml"),
            Err(ParseInstanceError::Parse { protocol, message })
                if protocol == "test_xml"
                    && message == "XML entity \"u\" is undeclared at byte Some(5)"
        ));
        let recursive = br#"<!DOCTYPE doc [
            <!ENTITY a "&b;">
            <!ENTITY b "&a;">
        ]><doc>&a;</doc>"#;
        let reference_offset = recursive
            .windows(b"<doc>&a;".len())
            .position(|part| part == b"<doc>&a;")
            .and_then(|offset| u64::try_from(offset + b"<doc>".len()).ok());
        let error = parse_xml_bytes(&schema, recursive, "test_xml").expect_err("recursive");
        assert!(matches!(
            error,
            ParseInstanceError::Parse { protocol, message }
                if protocol == "test_xml"
                    && message == format!(
                        "XML entity \"a\" is recursive at byte {reference_offset:?}"
                    )
        ));
    }

    #[test]
    fn xml_route_enforces_entity_depth_and_document_byte_budgets() {
        let schema = xml_schema();
        let mut depth_subset = String::new();
        for depth in 0..=MAX_XML_ENTITY_DEPTH {
            depth_subset.push_str(&format!("<!ENTITY e{depth} \"&e{};\">", depth + 1));
        }
        depth_subset.push_str(&format!("<!ENTITY e{} \"x\">", MAX_XML_ENTITY_DEPTH + 1));
        let depth_document = format!("<!DOCTYPE doc [{depth_subset}]><doc>&e0;</doc>");
        let depth_offset = u64::try_from(depth_document.rfind("&e0;").expect("reference"));
        assert!(matches!(
            parse_xml_bytes(&schema, depth_document.as_bytes(), "test_xml"),
            Err(ParseInstanceError::LimitExceeded {
                protocol,
                resource,
                bound,
                byte_offset,
            }) if protocol == "test_xml"
                && resource == "XML entity nesting depth"
                && bound == u64::try_from(MAX_XML_ENTITY_DEPTH).unwrap_or(u64::MAX)
                && byte_offset == depth_offset.ok()
        ));

        let mut bytes_subset = "<!ENTITY e0 \"x\">".to_owned();
        for depth in 1..=10 {
            let references = format!("&e{};", depth - 1).repeat(10);
            bytes_subset.push_str(&format!("<!ENTITY e{depth} \"{references}\">"));
        }
        let bytes_document = format!("<!DOCTYPE doc [{bytes_subset}]><doc>&e10;</doc>");
        let bytes_offset = u64::try_from(bytes_document.rfind("&e10;").expect("reference"));
        assert!(matches!(
            parse_xml_bytes(&schema, bytes_document.as_bytes(), "test_xml"),
            Err(ParseInstanceError::LimitExceeded {
                protocol,
                resource,
                bound,
                byte_offset,
            }) if protocol == "test_xml"
                && resource == "XML entity expansion bytes"
                && bound == u64::try_from(MAX_XML_ENTITY_EXPANSION_BYTES).unwrap_or(u64::MAX)
                && byte_offset == bytes_offset.ok()
        ));

        let replacement = "x".repeat(MAX_XML_ENTITY_EXPANSION_BYTES / 2 + 1);
        let cumulative_document =
            format!("<!DOCTYPE doc [<!ENTITY e \"{replacement}\">]><doc>&e;&e;</doc>");
        let cumulative_offset =
            u64::try_from(cumulative_document.rfind("&e;").expect("second reference"));
        assert!(matches!(
            parse_xml_bytes(&schema, cumulative_document.as_bytes(), "test_xml"),
            Err(ParseInstanceError::LimitExceeded {
                protocol,
                resource,
                bound,
                byte_offset,
            }) if protocol == "test_xml"
                && resource == "XML entity expansion bytes"
                && bound == u64::try_from(MAX_XML_ENTITY_EXPANSION_BYTES).unwrap_or(u64::MAX)
                && byte_offset == cumulative_offset.ok()
        ));

        let combined =
            format!("<!DOCTYPE doc [<!ENTITY e \"{replacement}\">]><doc a=\"&e;\">&e;</doc>");
        let combined_offset = u64::try_from(combined.rfind("&e;").expect("content reference"));
        assert!(matches!(
            parse_xml_bytes(&schema, combined.as_bytes(), "test_xml"),
            Err(ParseInstanceError::LimitExceeded {
                protocol,
                resource,
                bound,
                byte_offset,
            }) if protocol == "test_xml"
                && resource == "XML entity expansion bytes"
                && bound == u64::try_from(MAX_XML_ENTITY_EXPANSION_BYTES).unwrap_or(u64::MAX)
                && byte_offset == combined_offset.ok()
        ));
    }

    #[test]
    fn xml_route_refuses_every_external_or_parameter_entity_construct() {
        let schema = xml_schema();
        let external_subset = br#"<!DOCTYPE doc SYSTEM "d.dtd"><doc/>"#;
        let external_subset_offset = external_subset
            .windows(b"SYSTEM".len())
            .position(|part| part == b"SYSTEM")
            .and_then(|offset| u64::try_from(offset).ok());
        assert!(matches!(
            parse_xml_bytes(&schema, external_subset, "test_xml"),
            Err(ParseInstanceError::UnsupportedFeature {
                protocol,
                feature,
                byte_offset,
            }) if protocol == "test_xml"
                && feature == "external XML DTD subset"
                && byte_offset == external_subset_offset
        ));
        let external_entity = br#"<!DOCTYPE doc [<!ENTITY e SYSTEM "e.ent">]><doc/>"#;
        let external_entity_offset = external_entity
            .windows(b"<!ENTITY e".len())
            .position(|part| part == b"<!ENTITY e")
            .and_then(|offset| u64::try_from(offset).ok());
        assert!(matches!(
            parse_xml_bytes(&schema, external_entity, "test_xml"),
            Err(ParseInstanceError::UnsupportedFeature {
                protocol,
                feature,
                byte_offset,
            }) if protocol == "test_xml"
                && feature == "external XML entity \"e\""
                && byte_offset == external_entity_offset
        ));
        for (input, needle) in [
            (
                br#"<!DOCTYPE doc [<!ENTITY % p "x">]><doc/>"#.as_slice(),
                b"<!ENTITY %".as_slice(),
            ),
            (
                br#"<!DOCTYPE doc [%p;]><doc/>"#.as_slice(),
                b"%p;".as_slice(),
            ),
        ] {
            let expected_offset = input
                .windows(needle.len())
                .position(|part| part == needle)
                .and_then(|offset| u64::try_from(offset).ok());
            assert!(matches!(
                parse_xml_bytes(&schema, input, "test_xml"),
                Err(ParseInstanceError::UnsupportedFeature {
                    protocol,
                    feature,
                    byte_offset,
                }) if protocol == "test_xml"
                    && feature == "XML parameter entity construct \"p\""
                    && byte_offset == expected_offset
            ));
        }
    }

    #[test]
    fn expanded_xml_name_uses_namespace_uri_and_local_name() {
        assert_eq!(
            xml_expanded_name("urn:example", "value"),
            ("urn:example".to_owned(), "value".to_owned())
        );
        assert_eq!(
            xml_expanded_name("", "value"),
            (String::new(), "value".to_owned())
        );
    }

    #[test]
    fn clark_schema_names_match_elements_and_attributes_independent_of_prefix() {
        let protocol = Protocol {
            name: "test_xml".into(),
            obj_kinds: vec!["document".into(), "element".into()],
            ..Protocol::default()
        };
        let schema = SchemaBuilder::new(&protocol)
            .vertex("doc", "document", None)
            .expect("root")
            .vertex("doc:value", "element", None)
            .expect("child")
            .edge("doc", "doc:value", "contains", Some("{urn:test}value"))
            .expect("edge")
            .build()
            .expect("schema");
        let input = br#"<doc xmlns:p="urn:test"><p:value p:note="x"/></doc>"#;
        let instance = parse_xml_bytes(&schema, input, "test_xml").expect("parse");
        let value = instance
            .nodes
            .values()
            .find(|node| node.anchor.as_ref() == "doc:value")
            .expect("Clark-routed child");
        assert_eq!(
            value.extra_fields.get("{urn:test}note"),
            Some(&Value::Str("x".to_owned()))
        );

        let emitted = emit_xml_bytes(&schema, &instance, "test_xml").expect("emit");
        let reparsed = parse_xml_bytes(&schema, &emitted, "test_xml").expect("reparse");
        assert_eq!(reparsed.node_count(), instance.node_count());
    }

    #[test]
    fn equal_expanded_attribute_names_are_refused_across_prefixes() {
        let schema = xml_schema();
        let input = br#"<doc xmlns:p="urn:test" xmlns:q="urn:test" p:x="1" q:x="2"/>"#;
        assert!(matches!(
            parse_xml_bytes(&schema, input, "test_xml"),
            Err(ParseInstanceError::DuplicateMember { protocol, name, byte_offset: Some(51) })
                if protocol == "test_xml" && name == "{urn:test}x"
        ));
        let input = br#"<a xmlns:p="u" xmlns:q="u" p:x="1" q:x="2"/>"#;
        let schema = SchemaBuilder::new(&Protocol {
            name: "test_xml".into(),
            obj_kinds: vec!["document".into()],
            ..Protocol::default()
        })
        .vertex("a", "document", None)
        .expect("root")
        .build()
        .expect("schema");
        assert!(matches!(
            parse_xml_bytes(&schema, input, "test_xml"),
            Err(ParseInstanceError::DuplicateMember { protocol, name, byte_offset: Some(35) })
                if protocol == "test_xml" && name == "{u}x"
        ));
    }

    fn xml_schema() -> Schema {
        let proto = panproto_schema::Protocol {
            name: "test_xml".into(),
            schema_theory: "ThTestSchema".into(),
            instance_theory: "ThTestInstance".into(),
            edge_rules: vec![],
            obj_kinds: vec!["document".into(), "element".into(), "text".into()],
            constraint_sorts: vec![],
            ..Default::default()
        };
        SchemaBuilder::new(&proto)
            .vertex("doc", "document", None)
            .expect("v")
            .vertex("doc:title", "element", None)
            .expect("v")
            .vertex("doc:body", "element", None)
            .expect("v")
            .edge("doc", "doc:title", "contains", Some("title"))
            .expect("e")
            .edge("doc", "doc:body", "contains", Some("body"))
            .expect("e")
            .build()
            .expect("build")
    }

    fn xml_element(id: u32, anchor: &str, tag: &str) -> Node {
        let mut node = Node::new(id, anchor);
        node.shape = NodeShape::XmlElement { tag: tag.into() };
        node
    }

    #[test]
    fn parse_simple_xml() {
        let schema = xml_schema();
        let input = b"<doc><title>Hello</title><body>World</body></doc>";
        let instance = parse_xml_bytes(&schema, input, "test_xml").expect("parse");
        assert!(instance.node_count() >= 3, "should have doc + title + body");
    }

    #[test]
    fn document_boundary_refuses_character_data_and_multiple_roots() {
        let schema = xml_schema();
        for input in [
            b"junk<doc/>".as_slice(),
            b"<doc/>junk".as_slice(),
            b"<doc/><doc/>".as_slice(),
        ] {
            assert!(matches!(
                parse_xml_bytes(&schema, input, "test_xml"),
                Err(ParseInstanceError::Parse { .. })
            ));
        }
        parse_xml_bytes(&schema, b" \n<doc/>\t", "test_xml")
            .expect("document whitespace is permitted outside the root element");
        for input in [
            b"<doc>&#1;</doc>".as_slice(),
            b"<doc note=\"&#1;\"/>".as_slice(),
            b"<p:doc/>".as_slice(),
            b"<doc p:note=\"x\"/>".as_slice(),
            b"<doc xmlns:xml=\"urn:evil\"/>".as_slice(),
            b"<xmlns:doc/>".as_slice(),
        ] {
            assert!(matches!(
                parse_xml_bytes(&schema, input, "test_xml"),
                Err(ParseInstanceError::Parse { .. })
            ));
        }
    }

    #[test]
    fn roundtrip_xml() {
        let schema = xml_schema();
        let input = b"<doc><title>Hello</title><body>World</body></doc>";
        let instance = parse_xml_bytes(&schema, input, "test_xml").expect("parse");
        let emitted = emit_xml_bytes(&schema, &instance, "test_xml").expect("emit");
        let instance2 = parse_xml_bytes(&schema, &emitted, "test_xml").expect("re-parse");
        assert_eq!(instance.node_count(), instance2.node_count());
    }

    #[test]
    fn open_xml_roots_use_the_schema_primary_entry() {
        let protocol = Protocol {
            name: "test_xml".into(),
            obj_kinds: vec!["object".into()],
            ..Protocol::default()
        };
        let schema = SchemaBuilder::new(&protocol)
            .vertex("root", "object", None)
            .expect("root")
            .build()
            .expect("open schema");
        let input = b"<unknown><child>value</child></unknown>";

        let instance = parse_xml_bytes(&schema, input, "test_xml").expect("parse open XML");
        assert_eq!(instance.schema_root.as_ref(), "root");
        assert_eq!(
            instance
                .node(instance.root)
                .expect("root node")
                .anchor
                .as_ref(),
            "root"
        );

        let emitted = emit_xml_bytes(&schema, &instance, "test_xml").expect("emit open XML");
        let reparsed = parse_xml_bytes(&schema, &emitted, "test_xml").expect("reparse open XML");
        assert_eq!(reparsed.schema_root, instance.schema_root);
        assert_eq!(reparsed.node_count(), instance.node_count());
    }

    #[test]
    fn roundtrip_uses_the_parsed_edge_name_as_the_element_tag() {
        let protocol = Protocol {
            name: "test_xml".into(),
            obj_kinds: vec!["document".into(), "element".into()],
            ..Protocol::default()
        };
        let schema = SchemaBuilder::new(&protocol)
            .vertex("doc", "document", None)
            .expect("root")
            .vertex("decimal_field", "element", None)
            .expect("field")
            .edge("doc", "decimal_field", "contains", Some("b"))
            .expect("named edge")
            .build()
            .expect("schema");
        let input = b"<doc><b>0.1</b></doc>";

        let instance = parse_xml_bytes(&schema, input, "test_xml").expect("parse named edge");
        assert_eq!(
            emit_xml_bytes(&schema, &instance, "test_xml").expect("emit named edge"),
            input
        );
    }

    #[test]
    fn xsi_nil_is_null_and_empty_differs_from_absent() {
        let schema = xml_schema();
        let input = br#"<doc xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"><title xsi:nil="true"/></doc>"#;
        let instance = parse_xml_bytes(&schema, input, "test_xml").expect("parse");

        let title = instance
            .nodes
            .values()
            .find(|node| node.anchor.as_str() == "doc:title")
            .expect("the nil title element is present");
        assert!(matches!(title.value, Some(FieldPresence::Null)));
        assert!(
            instance
                .nodes
                .values()
                .all(|node| node.anchor.as_str() != "doc:body"),
            "an absent body has no node"
        );

        let empty = parse_xml_bytes(&schema, b"<doc><body/></doc>", "test_xml")
            .expect("parse empty element");
        let body = empty
            .nodes
            .values()
            .find(|node| node.anchor.as_str() == "doc:body")
            .expect("an empty body still has a node");
        assert!(body.value.is_none(), "empty content is not explicit null");
    }

    #[test]
    fn xsi_nil_rejects_character_data_and_child_elements() {
        let schema = xml_schema();
        let prefix = br#"<doc xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance">"#;
        for invalid in [
            b"<title xsi:nil=\"true\">payload</title></doc>".as_slice(),
            b"<title xsi:nil=\"true\"> </title></doc>".as_slice(),
            b"<title xsi:nil=\"true\"><nested/></title></doc>".as_slice(),
        ] {
            let mut input = prefix.to_vec();
            input.extend_from_slice(invalid);
            assert!(
                matches!(
                    parse_xml_bytes(&schema, &input, "test_xml"),
                    Err(ParseInstanceError::Parse { .. })
                ),
                "nilled content must be rejected: {}",
                String::from_utf8_lossy(&input)
            );
        }

        let mut empty_cdata = prefix.to_vec();
        empty_cdata.extend_from_slice(b"<title xsi:nil=\"true\"><![CDATA[]]></title></doc>");
        parse_xml_bytes(&schema, &empty_cdata, "test_xml")
            .expect("empty CDATA carries no characters");
    }

    #[test]
    fn xsi_nil_uses_the_namespace_uri_not_a_literal_prefix() {
        let schema = xml_schema();
        let alias = br#"<doc xmlns:n="http://www.w3.org/2001/XMLSchema-instance"><title n:nil="true"/></doc>"#;
        let instance = parse_xml_bytes(&schema, alias, "test_xml").expect("parse alias");
        let title = instance
            .nodes
            .values()
            .find(|node| node.anchor.as_str() == "doc:title")
            .expect("title");
        assert!(matches!(title.value, Some(FieldPresence::Null)));

        let wrong = br#"<doc xmlns:xsi="urn:not-xsi"><title xsi:nil="true"/></doc>"#;
        let instance = parse_xml_bytes(&schema, wrong, "test_xml").expect("parse wrong URI");
        let title = instance
            .nodes
            .values()
            .find(|node| node.anchor.as_str() == "doc:title")
            .expect("title");
        assert!(!matches!(title.value, Some(FieldPresence::Null)));
    }

    #[test]
    fn xsi_nil_reads_the_xsd_boolean_lexical_space() {
        let schema = xml_schema();
        let document = |nil: &str| {
            format!(
                r#"<doc xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"><title xsi:nil="{nil}"/></doc>"#
            )
        };
        let title_value = |instance: &WInstance| {
            instance
                .nodes
                .values()
                .find(|node| node.anchor.as_str() == "doc:title")
                .map(|node| node.value.clone())
        };
        // xs:boolean collapses whitespace, so surrounding spaces and the
        // character references for tab, line feed and carriage return are
        // part of the same four literals.
        for (nil, null) in [
            ("true", true),
            ("1", true),
            (" true ", true),
            ("&#9;1&#10;", true),
            ("&#13;true", true),
            ("false", false),
            ("0", false),
            (" 0 ", false),
            ("false&#10;", false),
        ] {
            let input = document(nil);
            let instance = parse_xml_bytes(&schema, input.as_bytes(), "test_xml")
                .unwrap_or_else(|error| panic!("xsi:nil={nil:?} parses: {error}"));
            let expected = null.then_some(FieldPresence::Null);
            assert_eq!(title_value(&instance), Some(expected.clone()), "{nil}");
            let emitted = emit_xml_bytes(&schema, &instance, "test_xml")
                .unwrap_or_else(|error| panic!("xsi:nil={nil:?} re-emits: {error}"));
            let reparsed = parse_xml_bytes(&schema, &emitted, "test_xml").expect("reparse");
            assert_eq!(
                title_value(&reparsed),
                Some(expected),
                "{nil} after re-emission"
            );
        }
        // Anything else is not an xs:boolean and is refused when read.
        for nil in ["TRUE", "False", "yes", "", " ", "t rue", "01", "+1"] {
            let input = document(nil);
            let result = parse_xml_bytes(&schema, input.as_bytes(), "test_xml");
            assert!(
                matches!(
                    &result,
                    Err(ParseInstanceError::Fidelity(errors))
                        if errors.len() == 1
                            && errors[0].lexeme == nil
                            && errors[0].target == ScalarKind::Bool
                            && errors[0].reason == panproto_inst::FidelityReason::InvalidEncoding
                            && errors[0].site.vertex.as_ref() == "doc:title"
                ),
                "xsi:nil={nil:?} must be a typed refusal: {result:?}"
            );
        }
    }

    #[test]
    fn xml_attributes_are_unescaped_once() {
        let schema = xml_schema();
        let input = br#"<doc><title note="a &amp; b">text</title></doc>"#;
        let instance = parse_xml_bytes(&schema, input, "test_xml").expect("parse");
        let title = instance
            .nodes
            .values()
            .find(|node| node.anchor.as_str() == "doc:title")
            .expect("title");
        assert_eq!(
            title.extra_fields.get("note"),
            Some(&Value::Str("a & b".into()))
        );

        let emitted = emit_xml_bytes(&schema, &instance, "test_xml").expect("emit");
        let text = std::str::from_utf8(&emitted).expect("UTF-8");
        assert!(text.contains("note=\"a &amp; b\""));
        assert!(!text.contains("&amp;amp;"));
    }

    #[test]
    fn malformed_or_duplicate_attributes_are_not_dropped() {
        let schema = xml_schema();
        let result = parse_xml_bytes(&schema, br#"<doc a="1" a="2"/>"#, "test_xml");
        assert!(matches!(
            result,
            Err(ParseInstanceError::DuplicateMember {
                ref protocol,
                ref name,
                byte_offset: Some(11),
            }) if protocol == "test_xml" && name == "a"
        ));
        let result = parse_xml_bytes(&schema, br#"<doc a="1" b="2" a="3"/>"#, "test_xml");
        assert!(matches!(
            result,
            Err(ParseInstanceError::DuplicateMember {
                ref name,
                byte_offset: Some(17),
                ..
            }) if name == "a"
        ));
        let result = parse_xml_bytes(&schema, br#"<doc a="1" a ='2'/>"#, "test_xml");
        assert!(matches!(
            result,
            Err(ParseInstanceError::DuplicateMember {
                ref name,
                byte_offset: Some(11),
                ..
            }) if name == "a"
        ));
        let result = parse_xml_bytes(&schema, br#"<doc a="" a=""/>"#, "test_xml");
        assert!(matches!(
            result,
            Err(ParseInstanceError::DuplicateMember {
                ref name,
                byte_offset: Some(10),
                ..
            }) if name == "a"
        ));
    }

    #[test]
    fn character_references_admit_only_unsigned_digits() {
        let empty = BTreeMap::new();
        for reference in [
            "&#+65;", "&#-65;", "&#x+41;", "&#x-41;", "&# 65;", "&#x 41;", "&#X41;", "&#65 ;",
            "&#٦٥;",
        ] {
            assert_eq!(
                expand_xml_entities(reference, &empty),
                Err(XmlEntityError::Malformed),
                "{reference}"
            );
            let schema = xml_schema();
            for input in [
                format!("<doc><title>{reference}</title></doc>"),
                format!("<doc t=\"{reference}\"/>"),
                format!(
                    "<!DOCTYPE doc [<!ENTITY e \"{reference}\">]><doc><title>&e;</title></doc>"
                ),
            ] {
                assert!(
                    parse_xml_bytes(&schema, input.as_bytes(), "test_xml").is_err(),
                    "{input}"
                );
            }
        }
        for (reference, expected) in [
            ("&#65;", "A"),
            ("&#x41;", "A"),
            ("&#0065;", "A"),
            ("&#x0041;", "A"),
        ] {
            assert_eq!(
                expand_xml_entities(reference, &empty),
                Ok(expected.to_owned())
            );
        }
    }

    #[test]
    fn leaf_character_data_keeps_significant_whitespace() {
        let schema = xml_schema();
        let input = b"<doc><title>  Hello  </title></doc>";
        let instance = parse_xml_bytes(&schema, input, "test_xml").expect("parse");
        let title = instance
            .nodes
            .values()
            .find(|node| node.anchor.as_str() == "doc:title")
            .expect("title");
        assert!(matches!(
            title.value,
            Some(FieldPresence::Present(Value::Str(ref text))) if text == "  Hello  "
        ));
    }

    #[test]
    fn mixed_character_data_keeps_every_ordered_text_run() {
        let schema = xml_schema();
        let input = b"<doc>before<title>x</title>after</doc>";
        let instance = parse_xml_bytes(&schema, input, "test_xml").expect("parse mixed content");
        let root = instance.node(instance.root).expect("root");
        assert!(root.value.is_none());

        let children = instance
            .children_map
            .get(&instance.root)
            .expect("mixed-content children")
            .clone();
        assert_eq!(children.len(), 3);
        assert!(matches!(
            instance.node(children[0]),
            Some(node)
                if node.shape == NodeShape::XmlTextSegment
                    && node.value == Some(FieldPresence::Present(Value::Str("before".into())))
        ));
        assert_eq!(
            instance.node(children[1]).expect("element").anchor,
            "doc:title"
        );
        assert!(matches!(
            instance.node(children[2]),
            Some(node)
                if node.shape == NodeShape::XmlTextSegment
                    && node.value == Some(FieldPresence::Present(Value::Str("after".into())))
        ));

        assert_eq!(
            emit_xml_bytes(&schema, &instance, "test_xml").expect("emit mixed content"),
            input
        );

        let mut changed_type = instance;
        changed_type
            .nodes
            .get_mut(&children[0])
            .expect("first text segment")
            .value = Some(FieldPresence::Present(Value::Decimal(
            panproto_inst::value::DecimalValue::parse("1.25").expect("decimal fixture"),
        )));
        assert!(matches!(
            emit_xml_bytes(&schema, &changed_type, "test_xml"),
            Err(EmitInstanceError::Value(EmitValueError {
                reason: EmitValueReason::NotRepresentable,
                ..
            }))
        ));
    }

    #[test]
    fn mixed_content_preserves_between_child_and_trailing_whitespace() {
        let schema = xml_schema();
        let input = b"<doc><title>x</title> <body>y</body> </doc>";
        let instance = parse_xml_bytes(&schema, input, "test_xml").expect("parse mixed spaces");
        let text_segments: Vec<_> = instance
            .children(instance.root)
            .iter()
            .filter_map(|id| instance.node(*id))
            .filter(|node| node.shape == NodeShape::XmlTextSegment)
            .map(|node| node.value.as_ref())
            .collect();
        assert_eq!(
            text_segments,
            vec![
                Some(&FieldPresence::Present(Value::Str(" ".into()))),
                Some(&FieldPresence::Present(Value::Str(" ".into()))),
            ]
        );
        assert_eq!(
            emit_xml_bytes(&schema, &instance, "test_xml").expect("emit mixed spaces"),
            input
        );
    }

    #[test]
    fn whitespace_only_leaf_is_a_value_not_layout() {
        let schema = xml_schema();
        let input = b"<doc><title>   \t</title></doc>";
        let instance = parse_xml_bytes(&schema, input, "test_xml").expect("parse");
        let title = instance
            .nodes
            .values()
            .find(|node| node.anchor.as_str() == "doc:title")
            .expect("title");
        assert!(matches!(
            title.value,
            Some(FieldPresence::Present(Value::Str(ref text))) if text == "   \t"
        ));

        let emitted = emit_xml_bytes(&schema, &instance, "test_xml").expect("emit");
        let reparsed = parse_xml_bytes(&schema, &emitted, "test_xml").expect("reparse");
        let reparsed_title = reparsed
            .nodes
            .values()
            .find(|node| node.anchor.as_str() == "doc:title")
            .expect("reparsed title");
        assert_eq!(reparsed_title.value, title.value);
    }

    #[test]
    fn canonical_xml_refuses_conflicting_nil_markers_and_namespace_rebinding() {
        let schema = xml_schema();
        let input = br#"<doc xmlns:q="http://www.w3.org/2001/XMLSchema-instance"><title q:nil="false"/></doc>"#;
        let mut instance = parse_xml_bytes(&schema, input, "test_xml").expect("parse");
        let title = instance
            .nodes
            .values_mut()
            .find(|node| node.anchor.as_str() == "doc:title")
            .expect("title");
        title.value = Some(FieldPresence::Null);
        assert!(matches!(
            emit_xml_bytes(&schema, &instance, "test_xml"),
            Err(EmitInstanceError::Value(EmitValueError {
                reason: EmitValueReason::NotRepresentable,
                ref format,
                ref value_type,
                ..
            })) if format == "xml" && value_type == "str"
        ));

        let input = br#"<doc xmlns:q="http://www.w3.org/2001/XMLSchema-instance"><title q:nil="true"/></doc>"#;
        let mut instance = parse_xml_bytes(&schema, input, "test_xml").expect("parse null");
        instance
            .nodes
            .get_mut(&instance.root)
            .expect("root")
            .extra_fields
            .insert("xmlns:q".into(), Value::Str("urn:rebound".into()));
        assert!(matches!(
            emit_xml_bytes(&schema, &instance, "test_xml"),
            Err(EmitInstanceError::Value(EmitValueError {
                reason: EmitValueReason::NotRepresentable,
                ref format,
                ref value_type,
                ..
            })) if format == "xml" && value_type == "str"
        ));
    }

    #[test]
    fn canonical_xml_refuses_illegal_characters_and_qnames() {
        let schema = xml_schema();
        let input = br#"<doc note="kept"><title>text</title></doc>"#;

        let mut illegal_text = parse_xml_bytes(&schema, input, "test_xml").expect("parse text");
        illegal_text
            .nodes
            .values_mut()
            .find(|node| node.anchor.as_str() == "doc:title")
            .expect("title")
            .value = Some(FieldPresence::Present(Value::Str("bad\0text".into())));
        assert!(matches!(
            emit_xml_bytes(&schema, &illegal_text, "test_xml"),
            Err(EmitInstanceError::Value(EmitValueError {
                reason: EmitValueReason::NotRepresentable,
                ..
            }))
        ));

        let mut illegal_attribute =
            parse_xml_bytes(&schema, input, "test_xml").expect("parse attribute");
        illegal_attribute
            .nodes
            .get_mut(&illegal_attribute.root)
            .expect("root")
            .extra_fields
            .insert("note".into(), Value::Str("bad\0attribute".into()));
        assert!(matches!(
            emit_xml_bytes(&schema, &illegal_attribute, "test_xml"),
            Err(EmitInstanceError::Value(EmitValueError {
                reason: EmitValueReason::NotRepresentable,
                ..
            }))
        ));

        let mut illegal_name = parse_xml_bytes(&schema, input, "test_xml").expect("parse name");
        illegal_name
            .nodes
            .get_mut(&illegal_name.root)
            .expect("root")
            .shape = NodeShape::XmlElement {
            tag: "bad name".into(),
        };
        assert!(matches!(
            emit_xml_bytes(&schema, &illegal_name, "test_xml"),
            Err(EmitInstanceError::Emit { message, .. }) if message.contains("invalid QName")
        ));

        let mut unbound_attribute =
            parse_xml_bytes(&schema, input, "test_xml").expect("parse unbound attribute");
        unbound_attribute
            .nodes
            .get_mut(&unbound_attribute.root)
            .expect("root")
            .extra_fields
            .insert("p:note".into(), Value::Str("value".into()));
        assert!(matches!(
            emit_xml_bytes(&schema, &unbound_attribute, "test_xml"),
            Err(EmitInstanceError::Emit { message, .. })
                if message.contains("unbound namespace prefix")
        ));

        let mut illegal_segment = parse_xml_bytes(
            &schema,
            b"<doc>before<title>inside</title></doc>",
            "test_xml",
        )
        .expect("parse mixed content");
        illegal_segment
            .nodes
            .values_mut()
            .find(|node| node.is_xml_text_segment())
            .expect("text segment")
            .value = Some(FieldPresence::Present(Value::Str("bad\0segment".into())));
        assert!(matches!(
            emit_xml_bytes(&schema, &illegal_segment, "test_xml"),
            Err(EmitInstanceError::Value(EmitValueError {
                reason: EmitValueReason::NotRepresentable,
                ..
            }))
        ));
    }

    #[test]
    fn canonical_xml_root_and_child_route_edits_follow_parser_selection() {
        let schema = xml_schema();
        let input = br#"<doc note="kept"><title>text</title></doc>"#;
        let mut rerouted_root = parse_xml_bytes(&schema, input, "test_xml").expect("parse root");
        rerouted_root
            .nodes
            .get_mut(&rerouted_root.root)
            .expect("root")
            .shape = NodeShape::XmlElement {
            tag: "other".into(),
        };
        let rerouted =
            emit_xml_bytes(&schema, &rerouted_root, "test_xml").expect("emit rerouted root");
        let reparsed =
            parse_xml_bytes(&schema, &rerouted, "test_xml").expect("reparse rerouted root");
        assert_eq!(reparsed.schema_root, rerouted_root.schema_root);
        assert_eq!(
            reparsed
                .node(reparsed.root)
                .expect("reparsed root")
                .xml_tag()
                .map(AsRef::as_ref),
            Some("other")
        );

        let mut rerouted_child = parse_xml_bytes(&schema, input, "test_xml").expect("parse child");
        rerouted_child
            .nodes
            .values_mut()
            .find(|node| node.anchor.as_ref() == "doc:title")
            .expect("title")
            .shape = NodeShape::XmlElement { tag: "body".into() };
        assert!(matches!(
            emit_xml_bytes(&schema, &rerouted_child, "test_xml"),
            Err(EmitInstanceError::Emit { message, .. }) if message.contains("would not reparse")
        ));
    }

    #[test]
    fn xml_emit_preserves_null_and_omits_absent_children() {
        let schema = xml_schema();
        let mut instance = parse_xml_bytes(
            &schema,
            b"<doc><title>replace</title><body>omit</body></doc>",
            "test_xml",
        )
        .expect("parse");
        let title = instance
            .nodes
            .values_mut()
            .find(|node| node.anchor.as_str() == "doc:title")
            .expect("title");
        title.value = Some(FieldPresence::Null);
        let body = instance
            .nodes
            .values_mut()
            .find(|node| node.anchor.as_str() == "doc:body")
            .expect("body");
        body.value = Some(FieldPresence::Absent);

        let emitted = emit_xml_bytes(&schema, &instance, "test_xml").expect("emit");
        let text = std::str::from_utf8(&emitted).expect("UTF-8 XML");
        assert!(text.contains("xsi:nil=\"true\""));
        assert!(!text.contains("<body"));

        let reparsed = parse_xml_bytes(&schema, &emitted, "test_xml").expect("reparse");
        let title = reparsed
            .nodes
            .values()
            .find(|node| node.anchor.as_str() == "doc:title")
            .expect("title after reparse");
        assert!(matches!(title.value, Some(FieldPresence::Null)));
        assert!(
            reparsed
                .nodes
                .values()
                .all(|node| node.anchor.as_str() != "doc:body")
        );
    }

    #[test]
    fn xml_refuses_absent_children_with_retained_structure() {
        let schema = xml_schema();
        let mut instance = parse_xml_bytes(&schema, b"<doc><title>child</title></doc>", "test_xml")
            .expect("parse");
        let title = instance
            .nodes
            .values_mut()
            .find(|node| node.anchor.as_str() == "doc:title")
            .expect("title");
        title.value = Some(FieldPresence::Absent);
        title
            .extra_fields
            .insert("lost".into(), Value::Str("field".into()));

        assert!(matches!(
            emit_xml_bytes(&schema, &instance, "test_xml"),
            Err(EmitInstanceError::Emit { message, .. })
                if message.contains("absent XML child")
        ));
    }

    #[test]
    fn canonical_xml_refuses_children_under_a_null_parent() {
        let schema = xml_schema();
        let mut instance = parse_xml_bytes(&schema, b"<doc><title>child</title></doc>", "test_xml")
            .expect("parse parent and child");
        instance.nodes.get_mut(&instance.root).expect("root").value = Some(FieldPresence::Null);

        assert!(matches!(
            emit_xml_bytes(&schema, &instance, "test_xml"),
            Err(EmitInstanceError::Value(EmitValueError {
                reason: EmitValueReason::NotRepresentable,
                ..
            }))
        ));
    }

    #[test]
    fn canonical_xml_refuses_scalar_values_with_child_elements() {
        let schema = xml_schema();
        let mut instance = parse_xml_bytes(&schema, b"<doc><title>child</title></doc>", "test_xml")
            .expect("parse");
        instance.nodes.get_mut(&instance.root).expect("root").value =
            Some(FieldPresence::Present(Value::Str("prefix".into())));

        assert!(matches!(
            emit_xml_bytes(&schema, &instance, "test_xml"),
            Err(EmitInstanceError::Emit { message, .. })
                if message.contains("scalar node") && message.contains("child elements")
        ));

        let root = instance.nodes.get_mut(&instance.root).expect("root");
        root.value = None;
        root.shape = NodeShape::List;
        assert!(matches!(
            emit_xml_bytes(&schema, &instance, "test_xml"),
            Err(EmitInstanceError::Emit { message, .. })
                if message.contains("does not use the XML element shape")
        ));

        let root = instance.nodes.get_mut(&instance.root).expect("root");
        root.shape = NodeShape::XmlElement { tag: "doc".into() };
        root.discriminator = Some("kind".into());
        assert!(matches!(
            emit_xml_bytes(&schema, &instance, "test_xml"),
            Err(EmitInstanceError::Emit { message, .. })
                if message.contains("unsupported discriminator")
        ));
    }

    #[test]
    fn canonical_xml_text_segments_refuse_element_structure() {
        let schema = xml_schema();
        let mut instance = parse_xml_bytes(
            &schema,
            b"<doc>before<title>child</title></doc>",
            "test_xml",
        )
        .expect("parse mixed content");
        let text_id = *instance
            .children(instance.root)
            .iter()
            .find(|id| instance.node(**id).is_some_and(Node::is_xml_text_segment))
            .expect("text segment");
        instance
            .nodes
            .get_mut(&text_id)
            .expect("text segment")
            .extra_fields
            .insert("lost".into(), Value::Str("metadata".into()));

        assert!(matches!(
            emit_xml_bytes(&schema, &instance, "test_xml"),
            Err(EmitInstanceError::Emit { message, .. })
                if message.contains("text segment") && message.contains("element structure")
        ));
    }

    #[test]
    fn canonical_xml_refuses_text_segments_that_would_collapse() {
        let schema = xml_schema();
        let mut first = Node::new(1, "doc:text");
        first.shape = NodeShape::XmlTextSegment;
        first.value = Some(FieldPresence::Present(Value::Str("left".into())));
        let text_edge = Edge {
            src: "doc".into(),
            tgt: "doc:text".into(),
            kind: "prop".into(),
            name: Some("$xml_text".into()),
        };
        let lone = WInstance::new(
            HashMap::from([(0, xml_element(0, "doc", "doc")), (1, first.clone())]),
            vec![(0, 1, text_edge.clone())],
            Vec::new(),
            0,
            "doc".into(),
        );
        assert!(matches!(
            emit_xml_bytes(&schema, &lone, "test_xml"),
            Err(EmitInstanceError::Emit { message, .. })
                if message.contains("require an emitted element sibling")
        ));

        let mut second = Node::new(2, "doc:text");
        second.shape = NodeShape::XmlTextSegment;
        second.value = Some(FieldPresence::Present(Value::Str("right".into())));
        let title_edge = schema.outgoing_edges("doc")[0].clone();
        let adjacent = WInstance::new(
            HashMap::from([
                (0, xml_element(0, "doc", "doc")),
                (1, first),
                (2, second),
                (
                    3,
                    xml_element(3, "doc:title", "title")
                        .with_value(FieldPresence::Present(Value::Str("child".into()))),
                ),
            ]),
            vec![
                (0, 1, text_edge.clone()),
                (0, 2, text_edge),
                (0, 3, title_edge),
            ],
            Vec::new(),
            0,
            "doc".into(),
        );
        assert!(matches!(
            emit_xml_bytes(&schema, &adjacent, "test_xml"),
            Err(EmitInstanceError::Emit { message, .. })
                if message.contains("adjacent XML text segments")
        ));
    }

    #[test]
    fn canonical_xml_requires_element_shapes() {
        let schema = xml_schema();
        let plain_root = WInstance::new(
            HashMap::from([(0, Node::new(0, "doc"))]),
            Vec::new(),
            Vec::new(),
            0,
            "doc".into(),
        );
        assert!(matches!(
            emit_xml_bytes(&schema, &plain_root, "test_xml"),
            Err(EmitInstanceError::Emit { message, .. })
                if message.contains("does not use the XML element shape")
        ));

        let title_edge = schema.outgoing_edges("doc")[0].clone();
        let plain_child = WInstance::new(
            HashMap::from([
                (0, xml_element(0, "doc", "doc")),
                (
                    1,
                    Node::new(1, "doc:title")
                        .with_value(FieldPresence::Present(Value::Str("child".into()))),
                ),
            ]),
            vec![(0, 1, title_edge)],
            Vec::new(),
            0,
            "doc".into(),
        );
        assert!(matches!(
            emit_xml_bytes(&schema, &plain_child, "test_xml"),
            Err(EmitInstanceError::Emit { message, .. })
                if message.contains("does not use the XML element shape")
        ));
    }

    #[test]
    fn canonical_xml_open_route_ignores_isolated_vertex_name_collisions() {
        let protocol = Protocol {
            name: "test_xml".into(),
            obj_kinds: vec!["document".into(), "element".into()],
            ..Protocol::default()
        };
        let schema = SchemaBuilder::new(&protocol)
            .vertex("doc", "document", None)
            .expect("root")
            .vertex("doc:x", "element", None)
            .expect("isolated colliding vertex")
            .build()
            .expect("schema");
        let instance = WInstance::new(
            HashMap::from([
                (0, xml_element(0, "doc", "doc")),
                (1, xml_element(1, "doc:x", "x")),
            ]),
            vec![(
                0,
                1,
                Edge {
                    src: "doc".into(),
                    tgt: "doc:x".into(),
                    kind: "contains".into(),
                    name: Some("x".into()),
                },
            )],
            Vec::new(),
            0,
            "doc".into(),
        );

        let emitted = emit_xml_bytes(&schema, &instance, "test_xml")
            .expect("emit exact open XML route despite isolated vertex collision");
        let reparsed = parse_xml_bytes(&schema, &emitted, "test_xml").expect("reparse XML");
        assert_eq!(reparsed.node_count(), instance.node_count());
        for (id, expected) in &instance.nodes {
            let actual = reparsed.node(*id).expect("reparsed node");
            assert_eq!(actual.anchor, expected.anchor);
            assert_eq!(actual.value, expected.value);
            assert_eq!(actual.shape, expected.shape);
        }
        assert_eq!(reparsed.arcs, instance.arcs);
    }

    #[test]
    fn canonical_xml_open_children_cannot_shadow_declared_routes() {
        let protocol = Protocol {
            name: "test_xml".into(),
            obj_kinds: vec!["document".into(), "element".into()],
            ..Protocol::default()
        };
        let schema = SchemaBuilder::new(&protocol)
            .vertex("doc", "document", None)
            .expect("root")
            .vertex("declared", "element", None)
            .expect("declared child")
            .edge("doc", "declared", "contains", Some("child"))
            .expect("declared child route")
            .build()
            .expect("schema");
        let mut child = Node::new(1, "doc:child");
        child.shape = NodeShape::XmlElement {
            tag: "child".into(),
        };
        let instance = WInstance::new(
            HashMap::from([(0, xml_element(0, "doc", "doc")), (1, child)]),
            vec![(
                0,
                1,
                Edge {
                    src: "doc".into(),
                    tgt: "doc:child".into(),
                    kind: "contains".into(),
                    name: Some("child".into()),
                },
            )],
            Vec::new(),
            0,
            "doc".into(),
        );

        assert!(matches!(
            emit_xml_bytes(&schema, &instance, "test_xml"),
            Err(EmitInstanceError::Emit { message, .. })
                if message.contains("not declared")
        ));
    }

    #[test]
    fn empty_xml_returns_error() {
        let schema = xml_schema();
        let input = b"";
        let result = parse_xml_bytes(&schema, input, "test_xml");
        assert!(result.is_err(), "empty input should fail (no root element)");
    }

    #[test]
    fn emit_error_carries_protocol_name() {
        // Emit on an instance whose root id is missing produces an
        // EmitInstanceError::Emit; its `protocol` field must carry
        // the caller-supplied name (regression test for the previous
        // behaviour of leaving `protocol: String::new()` everywhere).
        use panproto_inst::WInstance;
        use panproto_inst::metadata::Node;

        let nodes = std::collections::HashMap::from([(0, Node::new(0, "doc"))]);
        // Root id 999 doesn't exist in `nodes`, so emit hits the
        // `node not found` arm.
        let instance = WInstance::new(
            nodes,
            Vec::new(),
            Vec::new(),
            999,
            panproto_gat::Name::from("doc"),
        );
        let schema = xml_schema();

        let err = emit_xml_bytes(&schema, &instance, "my-protocol").expect_err("emit must fail");
        match err {
            EmitInstanceError::Emit { protocol, .. } => {
                assert_eq!(
                    protocol, "my-protocol",
                    "emit error must carry the caller-supplied protocol name"
                );
            }
            other => panic!("expected Emit variant, got {other:?}"),
        }
    }

    #[test]
    fn xml_roots_never_anchor_outside_the_schema() {
        let protocol = Protocol {
            name: "test_xml".into(),
            obj_kinds: vec!["object".into()],
            ..Protocol::default()
        };
        let mut stale = SchemaBuilder::new(&protocol)
            .vertex("a", "object", None)
            .expect("a")
            .build()
            .expect("schema");
        stale.entries = vec!["ghost".into()];

        let result = parse_xml_bytes(&stale, b"<b>t</b>", "test_xml");
        assert!(
            matches!(
                result,
                Err(ParseInstanceError::SchemaMismatch { ref protocol, ref message })
                    if protocol == "test_xml"
                        && message == "declared entry vertex 'ghost' is not a vertex of the schema"
            ),
            "{result:?}"
        );

        let empty = parse_xml_bytes(&Schema::default(), b"<b>t</b>", "test_xml");
        assert!(
            matches!(
                empty,
                Err(ParseInstanceError::SchemaMismatch { ref message, .. })
                    if message == "schema has no entry vertex"
            ),
            "{empty:?}"
        );
    }
}
