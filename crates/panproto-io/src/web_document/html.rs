//! SIMD-accelerated HTML instance codec via `tl`.
//!
//! Uses the `tl` crate for HTML parsing — the fastest HTML parser in Rust,
//! using SIMD instructions for tag scanning. Produces a `WInstance` tree
//! mirroring the HTML DOM structure.

use std::collections::HashMap;

use panproto_inst::metadata::Node;
use panproto_inst::value::{FieldPresence, Value};
use panproto_inst::{FInstance, WInstance};
use panproto_schema::{Edge, Schema};

use crate::error::{EmitInstanceError, ParseInstanceError};
use crate::traits::{InstanceEmitter, InstanceParser, NativeRepr};

/// HTML instance codec using the `tl` SIMD parser.
pub struct HtmlCodec;

impl Default for HtmlCodec {
    fn default() -> Self {
        Self
    }
}

impl HtmlCodec {
    /// Create a new HTML codec.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl InstanceParser for HtmlCodec {
    fn protocol_name(&self) -> &str {
        "html"
    }

    fn native_repr(&self) -> NativeRepr {
        NativeRepr::WType
    }

    fn parse_wtype(
        &self,
        schema: &Schema,
        input: &[u8],
    ) -> Result<WInstance, ParseInstanceError> {
        let html_str = std::str::from_utf8(input).map_err(|e| ParseInstanceError::Parse {
            protocol: "html".into(),
            message: format!("invalid UTF-8: {e}"),
        })?;

        let dom = tl::parse(html_str, tl::ParserOptions::default()).map_err(|e| {
            ParseInstanceError::Parse {
                protocol: "html".into(),
                message: format!("HTML parse error: {e}"),
            }
        })?;

        let mut nodes = HashMap::new();
        let mut arcs = Vec::new();
        let mut next_id: u32 = 0;

        // Create root document node.
        let root_id = next_id;
        next_id += 1;
        let root_anchor = find_root_vertex(schema).unwrap_or_else(|| "document".into());
        nodes.insert(
            root_id,
            Node {
                id: root_id,
                anchor: root_anchor.clone(),
                value: None,
                discriminator: None,
                extra_fields: HashMap::new(),
            },
        );

        // Walk top-level children.
        let parser = dom.parser();
        for node_handle in dom.children() {
            walk_tl_node(
                parser,
                node_handle,
                root_id,
                &root_anchor,
                schema,
                &mut nodes,
                &mut arcs,
                &mut next_id,
            );
        }

        Ok(WInstance::new(nodes, arcs, Vec::new(), root_id, root_anchor))
    }

    fn parse_functor(
        &self,
        _schema: &Schema,
        _input: &[u8],
    ) -> Result<FInstance, ParseInstanceError> {
        Err(ParseInstanceError::UnsupportedRepresentation {
            protocol: "html".into(),
            requested: NativeRepr::Functor,
            native: NativeRepr::WType,
        })
    }
}

impl InstanceEmitter for HtmlCodec {
    fn protocol_name(&self) -> &str {
        "html"
    }

    fn emit_wtype(
        &self,
        schema: &Schema,
        instance: &WInstance,
    ) -> Result<Vec<u8>, EmitInstanceError> {
        let mut output = String::new();
        emit_node(&mut output, schema, instance, instance.root);
        Ok(output.into_bytes())
    }

    fn emit_functor(
        &self,
        _schema: &Schema,
        _instance: &FInstance,
    ) -> Result<Vec<u8>, EmitInstanceError> {
        Err(EmitInstanceError::UnsupportedRepresentation {
            protocol: "html".into(),
            requested: NativeRepr::Functor,
            native: NativeRepr::WType,
        })
    }
}

/// Recursively walk a `tl` DOM node and populate the instance.
#[allow(clippy::too_many_arguments)]
fn walk_tl_node(
    parser: &tl::Parser<'_>,
    node_handle: &tl::NodeHandle,
    parent_id: u32,
    parent_anchor: &str,
    schema: &Schema,
    nodes: &mut HashMap<u32, Node>,
    arcs: &mut Vec<(u32, u32, Edge)>,
    next_id: &mut u32,
) {
    let node = match node_handle.get(parser) {
        Some(n) => n,
        None => return,
    };

    match node {
        tl::Node::Tag(tag) => {
            let tag_name = tag.name().as_utf8_str().to_lowercase();
            let node_id = *next_id;
            *next_id += 1;

            let anchor = find_child_anchor(schema, parent_anchor, &tag_name)
                .unwrap_or_else(|| format!("{parent_anchor}:{tag_name}"));

            let kind = schema
                .vertices
                .get(&anchor)
                .map_or("element", |v| v.kind.as_str())
                .to_string();

            // Collect HTML attributes as extra_fields.
            let mut extra_fields = HashMap::new();
            for (key, value) in tag.attributes().iter() {
                let k = key.to_string();
                let v = value.map_or_else(String::new, |cow| cow.to_string());
                extra_fields.insert(k, Value::Str(v));
            }

            nodes.insert(
                node_id,
                Node {
                    id: node_id,
                    anchor: anchor.clone(),
                    value: None,
                    discriminator: Some(kind),
                    extra_fields,
                },
            );

            let edge = Edge {
                src: parent_anchor.to_string(),
                tgt: anchor.clone(),
                kind: "contains".to_string(),
                name: Some(tag_name),
            };
            arcs.push((parent_id, node_id, edge));

            // Recurse into children.
            let children = tag.children();
            for child in children.top().iter() {
                walk_tl_node(parser, child, node_id, &anchor, schema, nodes, arcs, next_id);
            }
        }
        tl::Node::Raw(text) => {
            let content = text.as_utf8_str().trim().to_string();
            if !content.is_empty() {
                let node_id = *next_id;
                *next_id += 1;

                let anchor = format!("{parent_anchor}:text");
                nodes.insert(
                    node_id,
                    Node {
                        id: node_id,
                        anchor: anchor.clone(),
                        value: Some(FieldPresence::Present(Value::Str(content))),
                        discriminator: None,
                        extra_fields: HashMap::new(),
                    },
                );

                let edge = Edge {
                    src: parent_anchor.to_string(),
                    tgt: anchor,
                    kind: "contains".to_string(),
                    name: Some("text".to_string()),
                };
                arcs.push((parent_id, node_id, edge));
            }
        }
        tl::Node::Comment(_) => {} // Skip comments.
    }
}

fn find_root_vertex(schema: &Schema) -> Option<String> {
    schema
        .vertices
        .values()
        .find(|v| schema.incoming_edges(&v.id).is_empty())
        .map(|v| v.id.clone())
}

fn find_child_anchor(schema: &Schema, parent: &str, tag: &str) -> Option<String> {
    let edges = schema.outgoing_edges(parent);
    for edge in edges {
        if edge.name.as_deref() == Some(tag) {
            return Some(edge.tgt.clone());
        }
    }
    None
}

fn emit_node(output: &mut String, schema: &Schema, instance: &WInstance, node_id: u32) {
    let Some(node) = instance.nodes.get(&node_id) else {
        return;
    };

    // If this is a text node, emit its value directly.
    if let Some(FieldPresence::Present(Value::Str(ref text))) = node.value {
        output.push_str(text);
        return;
    }

    // Determine tag name from anchor.
    let tag = node.anchor.rsplit(':').next().unwrap_or(&node.anchor);

    // Skip document-level wrapper in output.
    let is_root = node_id == instance.root;

    if !is_root {
        output.push('<');
        output.push_str(tag);

        // Emit extra_fields as attributes.
        for (key, val) in &node.extra_fields {
            if let Value::Str(s) = val {
                output.push(' ');
                output.push_str(key);
                output.push_str("=\"");
                output.push_str(s);
                output.push('"');
            }
        }

        output.push('>');
    }

    // Emit children.
    if let Some(children) = instance.children_map.get(&node_id) {
        for &child_id in children {
            emit_node(output, schema, instance, child_id);
        }
    }

    if !is_root {
        output.push_str("</");
        output.push_str(tag);
        output.push('>');
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use panproto_schema::SchemaBuilder;

    fn html_schema() -> Schema {
        let proto = panproto_schema::Protocol {
            name: "html".into(),
            schema_theory: "ThHtmlSchema".into(),
            instance_theory: "ThHtmlInstance".into(),
            edge_rules: vec![],
            obj_kinds: vec!["document".into(), "element".into(), "text".into()],
            constraint_sorts: vec![],
        };
        SchemaBuilder::new(&proto)
            .vertex("doc", "document", None)
            .unwrap()
            .build()
            .unwrap()
    }

    #[test]
    fn parse_simple_html() {
        let codec = HtmlCodec::new();
        let schema = html_schema();
        let input = b"<div><p>Hello</p></div>";
        let instance = codec.parse_wtype(&schema, input).expect("parse");
        assert!(instance.node_count() >= 3, "doc + div + p + text");
    }

    #[test]
    fn roundtrip_html() {
        let codec = HtmlCodec::new();
        let schema = html_schema();
        let input = b"<div><p>Hello</p></div>";
        let instance = codec.parse_wtype(&schema, input).expect("parse");
        let emitted = codec.emit_wtype(&schema, &instance).expect("emit");
        let instance2 = codec.parse_wtype(&schema, &emitted).expect("re-parse");
        assert_eq!(instance.node_count(), instance2.node_count());
    }
}
