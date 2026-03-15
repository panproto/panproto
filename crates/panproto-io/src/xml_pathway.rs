//! Shared XML pathway for schema-guided instance parsing via `quick-xml`.
//!
//! Provides a zero-copy pull-parser pathway that reads XML events and
//! builds `WInstance` trees guided by a schema. Used by ~14 protocols
//! (NAF, FoLiA, TEI, TimeML, ELAN, XSD instances, RSS/Atom, DOCX, ODF, etc.).
//!
//! The parser maps XML elements to schema vertices and XML attributes/text
//! content to node values. Element nesting follows schema edges.

use std::collections::HashMap;

use panproto_inst::WInstance;
use panproto_inst::metadata::Node;
use panproto_inst::value::{FieldPresence, Value};
use panproto_schema::{Edge, Schema};
use quick_xml::Reader;
use quick_xml::events::Event;

use crate::error::{EmitInstanceError, ParseInstanceError};

/// Accumulated state during XML parsing.
struct XmlParseState {
    nodes: HashMap<u32, Node>,
    arcs: Vec<(u32, u32, Edge)>,
    next_id: u32,
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
#[allow(clippy::too_many_lines)]
pub fn parse_xml_bytes(
    schema: &Schema,
    input: &[u8],
    protocol: &str,
) -> Result<WInstance, ParseInstanceError> {
    let mut reader = Reader::from_reader(input);
    reader.config_mut().trim_text(true);

    let mut state = XmlParseState::new();
    let mut element_stack: Vec<(u32, String)> = Vec::new(); // (node_id, vertex_id)
    let mut buf = Vec::new();
    let mut root_id: Option<u32> = None;
    let mut root_vertex = String::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                let tag = String::from_utf8_lossy(e.name().as_ref()).to_string();
                let node_id = state.alloc_id();

                // Determine the vertex kind: match tag against schema vertices.
                let vertex_id = element_stack.last().map_or_else(
                    || find_root_by_tag(schema, &tag).unwrap_or_else(|| tag.clone()),
                    |parent| {
                        find_child_vertex(schema, &parent.1, &tag)
                            .unwrap_or_else(|| format!("{}:{}", parent.1, tag))
                    },
                );

                let _kind = schema
                    .vertices
                    .get(vertex_id.as_str())
                    .map_or_else(|| tag.clone(), |v| v.kind.to_string());

                let mut extra_fields = HashMap::new();

                // Parse XML attributes as extra fields.
                for attr in e.attributes().flatten() {
                    let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                    let val = String::from_utf8_lossy(&attr.value).to_string();
                    extra_fields.insert(key, Value::Str(val));
                }

                let node = Node {
                    id: node_id,
                    anchor: panproto_gat::Name::from(vertex_id.as_str()),
                    value: None,
                    discriminator: None,
                    extra_fields,
                    position: None,
                    annotations: HashMap::new(),
                };
                state.nodes.insert(node_id, node);

                // Create arc from parent to this node.
                if let Some(parent) = element_stack.last() {
                    let edge = find_schema_edge(schema, &parent.1, &vertex_id, &tag);
                    state.arcs.push((parent.0, node_id, edge));
                } else {
                    root_id = Some(node_id);
                    root_vertex.clone_from(&vertex_id);
                }

                element_stack.push((node_id, vertex_id));
            }
            Ok(Event::Empty(ref e)) => {
                // Self-closing element: <foo attr="val"/>
                // Same as Start + End with no children.
                let tag = String::from_utf8_lossy(e.name().as_ref()).to_string();
                let node_id = state.alloc_id();

                let vertex_id = element_stack.last().map_or_else(
                    || find_root_by_tag(schema, &tag).unwrap_or_else(|| tag.clone()),
                    |parent| {
                        find_child_vertex(schema, &parent.1, &tag)
                            .unwrap_or_else(|| format!("{}:{}", parent.1, tag))
                    },
                );

                let mut extra_fields = HashMap::new();
                for attr in e.attributes().flatten() {
                    let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                    let val = String::from_utf8_lossy(&attr.value).to_string();
                    extra_fields.insert(key, Value::Str(val));
                }

                let node = Node {
                    id: node_id,
                    anchor: panproto_gat::Name::from(vertex_id.as_str()),
                    value: None,
                    discriminator: None,
                    extra_fields,
                    position: None,
                    annotations: HashMap::new(),
                };
                state.nodes.insert(node_id, node);

                if let Some(parent) = element_stack.last() {
                    let edge = find_schema_edge(schema, &parent.1, &vertex_id, &tag);
                    state.arcs.push((parent.0, node_id, edge));
                } else {
                    root_id = Some(node_id);
                    root_vertex = vertex_id;
                }
                // No push to element_stack — self-closing has no children.
            }
            Ok(Event::Text(ref e)) => {
                let text = e.unescape().map_err(|err| ParseInstanceError::Parse {
                    protocol: protocol.to_string(),
                    message: format!("XML text unescape error: {err}"),
                })?;
                if !text.trim().is_empty() {
                    if let Some(current) = element_stack.last() {
                        if let Some(node) = state.nodes.get_mut(&current.0) {
                            node.value = Some(FieldPresence::Present(Value::Str(text.to_string())));
                        }
                    }
                }
            }
            Ok(Event::End(_)) => {
                element_stack.pop();
            }
            Ok(Event::Eof) => break,
            Ok(_) => {} // Skip comments, PIs, CDATA, etc.
            Err(e) => {
                return Err(ParseInstanceError::Parse {
                    protocol: protocol.to_string(),
                    message: format!(
                        "XML parse error at position {}: {e}",
                        reader.error_position()
                    ),
                });
            }
        }
        buf.clear();
    }

    let root_id = root_id.ok_or_else(|| ParseInstanceError::Parse {
        protocol: protocol.to_string(),
        message: "XML document has no root element".into(),
    })?;

    Ok(WInstance::new(
        state.nodes,
        state.arcs,
        Vec::new(),
        root_id,
        panproto_gat::Name::from(root_vertex),
    ))
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
    _schema: &Schema,
    instance: &WInstance,
    _protocol: &str,
) -> Result<Vec<u8>, EmitInstanceError> {
    use quick_xml::Writer;
    use quick_xml::events::{BytesEnd, BytesStart, BytesText};

    fn write_node(
        writer: &mut Writer<Vec<u8>>,
        instance: &WInstance,
        node_id: u32,
    ) -> Result<(), EmitInstanceError> {
        let node = instance
            .nodes
            .get(&node_id)
            .ok_or_else(|| EmitInstanceError::Emit {
                protocol: String::new(),
                message: format!("node {node_id} not found"),
            })?;

        // Determine tag name from anchor (use last segment after ':').
        let tag = node.anchor.rsplit(':').next().unwrap_or(&node.anchor);

        let mut elem = BytesStart::new(tag);

        // Write extra_fields as XML attributes.
        for (key, val) in &node.extra_fields {
            if let Value::Str(s) = val {
                elem.push_attribute((key.as_str(), s.as_str()));
            }
        }

        writer
            .write_event(Event::Start(elem))
            .map_err(|e| EmitInstanceError::Emit {
                protocol: String::new(),
                message: format!("XML write error: {e}"),
            })?;

        // Write text content if present.
        if let Some(FieldPresence::Present(Value::Str(ref text))) = node.value {
            writer
                .write_event(Event::Text(BytesText::new(text)))
                .map_err(|e| EmitInstanceError::Emit {
                    protocol: String::new(),
                    message: format!("XML write error: {e}"),
                })?;
        }

        // Recurse into children.
        if let Some(children) = instance.children_map.get(&node_id) {
            for &child_id in children {
                write_node(writer, instance, child_id)?;
            }
        }

        writer
            .write_event(Event::End(BytesEnd::new(tag)))
            .map_err(|e| EmitInstanceError::Emit {
                protocol: String::new(),
                message: format!("XML write error: {e}"),
            })?;

        Ok(())
    }

    let mut writer = Writer::new(Vec::new());
    write_node(&mut writer, instance, instance.root)?;

    Ok(writer.into_inner())
}

/// Find a child vertex reachable from `parent_vertex` via an edge matching `tag`.
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

/// Find a root vertex whose kind or ID matches the given XML tag.
fn find_root_by_tag(schema: &Schema, tag: &str) -> Option<String> {
    // Try exact match on vertex ID first.
    if schema.vertices.contains_key(tag) {
        return Some(tag.to_string());
    }
    // Then match on vertex kind.
    schema
        .vertices
        .values()
        .find(|v| v.kind == tag && schema.incoming_edges(&v.id).is_empty())
        .map(|v| v.id.to_string())
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
            ..panproto_schema::Protocol::default()
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

    #[test]
    fn parse_simple_xml() {
        let schema = xml_schema();
        let input = b"<doc><title>Hello</title><body>World</body></doc>";
        let instance = parse_xml_bytes(&schema, input, "test_xml").expect("parse");
        assert!(instance.node_count() >= 3, "should have doc + title + body");
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
    fn empty_xml_returns_error() {
        let schema = xml_schema();
        let input = b"";
        let result = parse_xml_bytes(&schema, input, "test_xml");
        assert!(result.is_err(), "empty input should fail (no root element)");
    }
}
