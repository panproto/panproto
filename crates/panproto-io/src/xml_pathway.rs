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

use std::collections::{BTreeMap, HashMap};

use panproto_inst::WInstance;
use panproto_inst::metadata::Node;
use panproto_inst::value::coerce::{LexClass, Lexeme};
use panproto_inst::value::{FieldPresence, Value};
use panproto_inst::{NodeShape, ValueFidelityError, ValueSite};
use panproto_schema::{Edge, Protocol, ScalarKind, Schema, ValueSpec};
use quick_xml::events::Event;
use quick_xml::name::{NamespaceResolver, ResolveResult};
use quick_xml::{NsReader, XmlVersion};

use crate::error::{EmitInstanceError, EmitValueError, EmitValueReason, ParseInstanceError};
use crate::json_codec::{missing_entry_message, resolve_entry_vertex};
use crate::json_pathway::{encode_bytes, protocol_definition};

const XML_SCHEMA_INSTANCE_NS: &str = "http://www.w3.org/2001/XMLSchema-instance";
pub(crate) const XML_NS: &str = "http://www.w3.org/XML/1998/namespace";
pub(crate) const XMLNS_NS: &str = "http://www.w3.org/2000/xmlns/";
/// Keep namespace resolution bounded while admitting namespace-heavy machine
/// documents covered by the crate's linear-time regression corpus.
const MAX_XML_NAMESPACE_DECLARATIONS: usize = 4_096;

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
/// Returns [`ParseInstanceError::Parse`] if the XML is malformed or
/// doesn't match the schema structure.
pub fn parse_xml_bytes(
    schema: &Schema,
    input: &[u8],
    protocol: &str,
) -> Result<WInstance, ParseInstanceError> {
    let mut reader = configured_xml_reader(input);

    let mut state = XmlParseState::new();
    let mut element_stack: Vec<(u32, String)> = Vec::new(); // (node_id, vertex_id)
    let mut buf = Vec::new();
    let mut root = XmlDocumentRoot::default();
    let protocol_definition = protocol_definition(protocol);
    let mut fidelity = Vec::new();
    // One element's character data can arrive as several events: a run of
    // text, an entity or character reference, a CDATA section, then more
    // text. They are accumulated here and written to the node when the
    // element closes, so `a &amp; b` is one value rather than whichever
    // fragment happened to arrive last.
    let mut text_buf = String::new();

    loop {
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
                    )?;
                }
                ingest_xml_element(
                    e,
                    reader.resolver(),
                    schema,
                    &mut state,
                    &element_stack,
                    &mut root,
                    protocol,
                )?;
            }
            Ok(Event::Text(ref e)) => {
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
                append_xml_reference(e, &mut text_buf, protocol)?;
            }
            Ok(Event::CData(ref e)) => {
                // CDATA is character data too, and was previously
                // dropped by the catch-all arm, so `<![CDATA[x]]>` read
                // back as an element with no value at all.
                append_xml_cdata(e, &mut text_buf, protocol)?;
            }
            Ok(Event::End(_)) => {
                flush_text(
                    &mut text_buf,
                    &element_stack,
                    &mut state,
                    schema,
                    &protocol_definition,
                    &mut fidelity,
                    false,
                )?;
                element_stack.pop();
            }
            Ok(Event::Eof) => {
                if element_stack.is_empty() {
                    flush_document_text(&mut text_buf, protocol)?;
                }
                break;
            }
            Ok(_) => {} // Skip comments, PIs, the declaration, the DTD.
            Err(e) => {
                return Err(xml_parse_error(protocol, reader.error_position(), &e));
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
        text.push('&');
        text.push_str(&name);
        text.push(';');
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

/// Write accumulated character data to the innermost open element, and
/// clear the buffer.
///
/// Every character-data run is preserved. In mixed content, whitespace
/// between or after child elements can be semantically significant, so it is
/// represented by the same ordered text-segment nodes as non-whitespace text.
/// A leaf element's text remains its direct value.
fn flush_text(
    text_buf: &mut String,
    element_stack: &[(u32, String)],
    state: &mut XmlParseState,
    schema: &Schema,
    protocol: &Protocol,
    fidelity: &mut Vec<ValueFidelityError>,
    before_child: bool,
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
                let spec = ValueSpec::for_vertex(schema, protocol, &node.anchor);
                match crate::json_pathway::coerce_exact_lexeme(
                    &Lexeme::new(text_buf, LexClass::String),
                    &spec,
                ) {
                    Ok(Value::Null) => node.value = Some(FieldPresence::Null),
                    Ok(value) => node.value = Some(FieldPresence::Present(value)),
                    Err(error) => {
                        fidelity.push(error.at(ValueSite::new(node.anchor.clone(), None)));
                    }
                }
            }
        }
    }
    text_buf.clear();
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
    crate::json_pathway::validate_canonical_tree_structure(schema, instance, protocol, "XML")?;
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
            let spec =
                ValueSpec::for_vertex(context.schema, context.protocol_definition, &node.anchor);
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
    validate_xml_emission_route(context, node_id, node, &tag)?;
    validate_xml_element_value_structure(context, node_id, node, &site)?;
    let rendered = xml_attributes(node, &site, in_scope_namespaces, context.protocol)?;
    if !xml_qname_is_bound(&tag, &rendered.namespaces) {
        return Err(EmitInstanceError::Emit {
            protocol: context.protocol.to_owned(),
            message: format!("XML element QName {tag:?} uses an unbound namespace prefix"),
        });
    }
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
) -> Result<(), EmitInstanceError> {
    let route_error = || EmitInstanceError::Emit {
        protocol: context.protocol.to_owned(),
        message: format!(
            "XML tag {tag:?} would not reparse to node {node_id} at schema anchor '{}'",
            node.anchor
        ),
    };
    if node_id == context.instance.root {
        if find_root_by_tag(context.schema, tag).as_deref() == Some(node.anchor.as_ref()) {
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
    if let Some(selected_anchor) = find_child_vertex(context.schema, &parent.anchor, tag) {
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
        if find_schema_edge(context.schema, &parent.anchor, &node.anchor, tag) != *emitted_edge {
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
    for (key, value) in ordered {
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
    for key in attributes.keys() {
        if !is_xml_namespace_declaration(key) && !xml_qname_is_bound(key, &namespaces) {
            return Err(EmitInstanceError::Emit {
                protocol: protocol.to_owned(),
                message: format!("XML attribute QName {key:?} uses an unbound namespace prefix"),
            });
        }
    }
    Ok(XmlAttributes {
        values: attributes,
        namespaces,
    })
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
    let representable = if attribute {
        matches!(value, Value::Str(_))
    } else if let Some(text) = &bigint_text {
        spec.is_some_and(|spec| crate::json_pathway::plain_text_reparses_as(value, text, spec))
    } else {
        spec.is_some_and(|spec| crate::json_pathway::plain_text_value_roundtrips(value, spec))
    };
    if !representable {
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
        Value::Float(number) => {
            crate::json_pathway::plain_text_float_lexeme(*number).ok_or_else(|| {
                EmitValueError::new(
                    site.clone(),
                    value.type_name(),
                    "xml",
                    EmitValueReason::NonFinite,
                )
            })?
        }
        Value::Bool(value) => value.to_string(),
        Value::Bytes(bytes) => xml_bytes_text(bytes, spec, site)?,
        Value::Decimal(number) => number.to_string(),
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
    if !is_xml_10_text(&text) {
        return Err(EmitValueError::new(
            site.clone(),
            value.type_name(),
            "xml",
            EmitValueReason::NotRepresentable,
        )
        .into());
    }
    if !attribute && text.is_empty() {
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
fn ingest_xml_element(
    e: &quick_xml::events::BytesStart<'_>,
    namespaces: &NamespaceResolver,
    schema: &Schema,
    state: &mut XmlParseState,
    element_stack: &[(u32, String)],
    root: &mut XmlDocumentRoot,
    protocol: &str,
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
    if !is_xml_qname(&tag)
        || xml_qname_prefix(&tag) == Some("xmlns")
        || matches!(
            namespaces.resolve_element(e.name()).0,
            ResolveResult::Unknown(_)
        )
    {
        return Err(ParseInstanceError::Parse {
            protocol: protocol.to_owned(),
            message: format!("XML element {tag:?} uses an invalid or unbound namespace prefix"),
        });
    }
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
        None => {
            find_root_by_tag(schema, &tag).ok_or_else(|| ParseInstanceError::SchemaMismatch {
                protocol: protocol.to_owned(),
                message: missing_entry_message(None, schema),
            })?
        }
        Some(parent) => find_child_vertex(schema, &parent.1, &tag)
            .unwrap_or_else(|| format!("{}:{}", parent.1, tag)),
    };
    let (extra_fields, nil) = parse_xml_element_attributes(e, namespaces, &vertex_id, protocol)?;

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
        let edge = find_schema_edge(schema, &parent.1, &vertex_id, &tag);
        state.arcs.push((parent.0, node_id, edge));
    } else {
        root.id = Some(node_id);
        root.vertex.clone_from(&vertex_id);
    }

    Ok((node_id, vertex_id))
}

fn parse_xml_element_attributes(
    element: &quick_xml::events::BytesStart<'_>,
    namespaces: &NamespaceResolver,
    vertex: &str,
    protocol: &str,
) -> Result<(HashMap<String, Value>, bool), ParseInstanceError> {
    let mut extra_fields = HashMap::new();
    let mut nil = false;
    for attribute in element.attributes() {
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
        let value = attribute
            .decoded_and_normalized_value(XmlVersion::Implicit1_0, element.decoder())
            .map_err(|error| ParseInstanceError::Parse {
                protocol: protocol.to_owned(),
                message: format!("XML attribute value error: {error}"),
            })?
            .into_owned();
        validate_parsed_xml_text(&value, protocol)?;
        if is_xml_namespace_declaration(&key) && !is_valid_xml_namespace_declaration(&key, &value) {
            return Err(ParseInstanceError::Parse {
                protocol: protocol.to_owned(),
                message: format!("invalid XML namespace declaration {key:?}"),
            });
        }
        let (namespace, local_name) = namespaces.resolve_attribute(attribute.key);
        match namespace {
            ResolveResult::Bound(namespace)
                if namespace.into_inner() == XML_SCHEMA_INSTANCE_NS.as_bytes()
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
                        String::from_utf8_lossy(&prefix)
                    ),
                });
            }
            ResolveResult::Unbound | ResolveResult::Bound(_) => {}
        }
        extra_fields.insert(key, Value::Str(value));
    }
    Ok((extra_fields, nil))
}

fn find_child_vertex(schema: &Schema, parent_vertex: &str, tag: &str) -> Option<String> {
    let edges = schema.outgoing_edges(parent_vertex);
    // First try matching edge name.
    for edge in edges {
        if edge.name.as_deref() == Some(tag) {
            return Some(edge.tgt.to_string());
        }
    }
    // Then try matching target vertex kind.
    for edge in edges {
        if let Some(v) = schema.vertices.get(&edge.tgt) {
            if v.kind == tag {
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
fn find_root_by_tag(schema: &Schema, tag: &str) -> Option<String> {
    // Try exact match on vertex ID first.
    if schema.vertices.contains_key(tag) {
        return Some(tag.to_string());
    }
    // Then match on vertex kind.
    schema
        .vertices
        .values()
        .filter(|v| v.kind == tag && schema.incoming_edges(&v.id).is_empty())
        .min_by(|left, right| left.id.cmp(&right.id))
        .map(|v| v.id.to_string())
        .or_else(|| resolve_entry_vertex(None, schema).map(ToString::to_string))
}

/// Find or construct a schema edge between parent and child vertices.
fn find_schema_edge(schema: &Schema, parent: &str, child: &str, tag: &str) -> Edge {
    let edges = schema.edges_between(parent, child);
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
        assert!(matches!(result, Err(ParseInstanceError::Parse { .. })));
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
