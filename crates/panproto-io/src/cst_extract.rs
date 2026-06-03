//! CST-to-Instance extraction lens for format-preserving round-trips.
//!
//! This module implements the extraction lens that maps from a tree-sitter
//! CST Schema (the lossless format-level representation produced by
//! `panproto_parse::AstWalker`) to a domain-level `WInstance` or
//! `FInstance`, and the reverse injection that reconstructs a CST Schema
//! from a (possibly modified) instance.
//!
//! The extraction is format-specific: JSON CSTs have `pair`, `string`,
//! `object`, `array` nodes; XML CSTs have `element`, `Attribute`, `CharData`
//! nodes; etc. Each format's extraction logic lives in its own section.
//!
//! ## Lens structure
//!
//! ```text
//! CST Schema ──[extract]──→ (WInstance, CstComplement)
//!            ←──[inject]───  (WInstance, CstComplement) → updated CST Schema
//! ```
//!
//! The `CstComplement` stores the original CST Schema and a mapping from
//! `WInstance` node IDs to CST vertex names. This enables the injection
//! direction to update literal values in the CST without disturbing
//! formatting (interstitials, whitespace, indentation).

use std::collections::HashMap;

use panproto_gat::Name;
use panproto_inst::FInstance;
use panproto_inst::metadata::Node;
use panproto_inst::value::{FieldPresence, Value};
use panproto_inst::wtype::WInstance;
use panproto_schema::{Edge, Schema};
use serde::{Deserialize, Serialize};

/// The complement of the CST-to-Instance extraction lens.
///
/// Stores the full CST Schema (which is lossless: `emit_from_schema`
/// reconstructs the original bytes) and a mapping from `WInstance` node
/// IDs to the CST vertex names that carry the corresponding values.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CstComplement {
    /// The format that produced this CST (e.g., `"json"`, `"xml"`, `"yaml"`).
    pub format: String,
    /// The full CST Schema from tree-sitter parsing.
    pub cst_schema: Schema,
    /// Maps `WInstance` node IDs to the CST vertex names that hold their
    /// literal values. Used by injection to update the CST when the
    /// `WInstance` is modified.
    pub node_to_cst_value: HashMap<u32, Name>,
    /// Maps `WInstance` node IDs to the CST vertex names of the structural
    /// node (object, pair, array element) for structural reconstruction.
    pub node_to_cst_struct: HashMap<u32, Name>,
}

/// Errors from CST extraction and injection.
#[derive(Debug, thiserror::Error)]
pub enum CstExtractError {
    /// The CST Schema has an unexpected structure.
    #[error("CST structure error: {0}")]
    Structure(String),

    /// A required CST vertex was not found.
    #[error("CST vertex not found: {0}")]
    VertexNotFound(String),

    /// Domain schema mismatch.
    #[error("domain schema mismatch: {0}")]
    SchemaMismatch(String),
}

/// Accumulated state during CST extraction.
struct ExtractState {
    nodes: HashMap<u32, Node>,
    arcs: Vec<(u32, u32, Edge)>,
    next_id: u32,
    node_to_cst_value: HashMap<u32, Name>,
    node_to_cst_struct: HashMap<u32, Name>,
}

impl ExtractState {
    fn new() -> Self {
        Self {
            nodes: HashMap::new(),
            arcs: Vec::new(),
            next_id: 0,
            node_to_cst_value: HashMap::new(),
            node_to_cst_struct: HashMap::new(),
        }
    }

    const fn alloc_id(&mut self) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }
}

// ── Helpers ────────────────────────────────────────────────────────────

use panproto_inst::NodeShape;

/// Get the `literal-value` constraint from a CST vertex.
fn literal_value(cst: &Schema, vertex_id: &str) -> Option<String> {
    cst.constraints.get(vertex_id).and_then(|cs| {
        cs.iter()
            .find(|c| c.sort.as_ref() == "literal-value")
            .map(|c| c.value.clone())
    })
}

/// Get the text content of a CST vertex, descending through anonymous
/// child wrappers when the vertex itself carries no `literal-value`
/// constraint.
///
/// Tree-sitter grammars routinely model a token as a wrapper node
/// whose only purpose is to anchor a `text`-typed leaf child (TSV
/// `field → text`, JSON `string → string_content`, etc.). Callers that
/// just want "the captured bytes for this token" should not have to
/// know that detail; this helper walks the structural `child_of` chain
/// until it finds a `literal-value`, returning the empty string only
/// when no descendant carries one.
fn literal_text_deep(cst: &Schema, vertex_id: &str) -> String {
    if let Some(text) = literal_value(cst, vertex_id) {
        return text;
    }
    let mut buf = String::new();
    for child in cst_children_by_edge_kind(cst, vertex_id, "child_of") {
        if let Some(text) = literal_value(cst, child) {
            buf.push_str(&text);
        } else {
            buf.push_str(&literal_text_deep(cst, child));
        }
    }
    buf
}

/// Resolve a cell/field vertex to the descendant that actually carries the
/// `literal-value` constraint.
///
/// Tabular cells are wrapper nodes (`field → text`/`field → number`) whose
/// text lives on a single `child_of` leaf, so a write aimed at the wrapper
/// itself is a no-op. This descends through single-child wrappers to the leaf
/// holding the value, mirroring [`literal_text_deep`]'s read path so that
/// extraction and injection agree on which node owns the text. Falls back to
/// the original vertex when no descendant carries a `literal-value` or when the
/// branch forks (an ambiguous multi-leaf cell, which the simple value-replace
/// cannot target unambiguously anyway).
fn deepest_literal_vertex(cst: &Schema, vertex_id: &str) -> Name {
    if literal_value(cst, vertex_id).is_some() {
        return Name::from(vertex_id);
    }
    let children = cst_children_by_edge_kind(cst, vertex_id, "child_of");
    if children.len() == 1 {
        return deepest_literal_vertex(cst, children[0]);
    }
    Name::from(vertex_id)
}

/// Find a child of `parent` with the given edge kind in the CST.
fn cst_child_by_edge_kind<'a>(cst: &'a Schema, parent: &str, edge_kind: &str) -> Option<&'a Name> {
    cst.outgoing_edges(parent)
        .iter()
        .find(|e| *e.kind == *edge_kind)
        .map(|e| &e.tgt)
}

/// Find all children of `parent` with the given edge kind in the CST.
fn cst_children_by_edge_kind<'a>(cst: &'a Schema, parent: &str, edge_kind: &str) -> Vec<&'a Name> {
    cst.outgoing_edges(parent)
        .iter()
        .filter(|e| *e.kind == *edge_kind)
        .map(|e| &e.tgt)
        .collect()
}

/// Get the vertex kind from the CST.
fn cst_vertex_kind(cst: &Schema, vertex_id: &str) -> Option<String> {
    cst.vertex(vertex_id).map(|v| v.kind.to_string())
}

/// Get the `string_content` literal from a CST `string` vertex.
fn json_string_value(cst: &Schema, string_vertex: &str) -> Option<String> {
    let edges = cst.outgoing_edges(string_vertex);
    if edges.is_empty() {
        return None;
    }

    // Walk every child of the string vertex in source order. Tree-sitter
    // splits an escaped JSON string into a sequence of `string_content`
    // text fragments and `escape_sequence` nodes; the first non-empty
    // child carries only the prefix before the first escape, so the
    // previous "first child only" lookup truncated every string with
    // an escape in it.
    //
    // Unicode escapes (` `) are split further by the grammar:
    // `escape_sequence` captures only the first two bytes (`\u`) and
    // the four hex digits land in the next `string_content` text
    // segment. We peek-ahead in that case, decode the codepoint, and
    // skip the consumed prefix.
    let mut children: Vec<&panproto_schema::Edge> = edges.iter().collect();
    children.sort_by_key(|e| {
        cst.constraints
            .get(&e.tgt)
            .and_then(|cs| {
                cs.iter()
                    .find(|c| c.sort.as_ref() == "start-byte")
                    .and_then(|c| c.value.parse::<usize>().ok())
            })
            .unwrap_or(0)
    });

    let mut out = String::new();
    let mut saw_content = false;
    let mut hex_skip: usize = 0;
    for edge in children {
        match cst_vertex_kind(cst, &edge.tgt).as_deref() {
            Some("string_content") => {
                if let Some(text) = literal_value(cst, &edge.tgt) {
                    let body = if hex_skip > 0 && hex_skip <= text.len() {
                        let consumed = &text[..hex_skip];
                        if let Some(c) = u32::from_str_radix(consumed, 16)
                            .ok()
                            .and_then(char::from_u32)
                        {
                            out.push(c);
                        }
                        &text[hex_skip..]
                    } else {
                        text.as_str()
                    };
                    hex_skip = 0;
                    out.push_str(body);
                    saw_content = true;
                }
            }
            Some("escape_sequence") => {
                if let Some(raw) = literal_value(cst, &edge.tgt) {
                    let bytes = raw.as_bytes();
                    if bytes.len() == 2 && bytes[0] == b'\\' && bytes[1] == b'u' {
                        // Defer emission until the next string_content
                        // child contributes the four hex digits.
                        hex_skip = 4;
                    } else {
                        out.push_str(&decode_json_escape(&raw));
                    }
                    saw_content = true;
                }
            }
            _ => {}
        }
    }
    if saw_content { Some(out) } else { None }
}

/// Decode a JSON escape sequence (the raw bytes captured by
/// tree-sitter, e.g. `\n`, `\"`) to the escaped character.
///
/// `\uXXXX` escapes are NOT handled here because the JSON tree-sitter
/// grammar splits them across an `escape_sequence` (just `\u`) and a
/// trailing `string_content` text segment carrying the four hex
/// digits; [`json_string_value`] handles that join with lookahead.
/// Unrecognised forms fall back to the original bytes.
fn decode_json_escape(raw: &str) -> String {
    let bytes = raw.as_bytes();
    if bytes.len() < 2 || bytes[0] != b'\\' {
        return raw.to_owned();
    }
    match bytes[1] {
        b'"' => "\"".to_owned(),
        b'\\' => "\\".to_owned(),
        b'/' => "/".to_owned(),
        b'b' => "\u{0008}".to_owned(),
        b'f' => "\u{000C}".to_owned(),
        b'n' => "\n".to_owned(),
        b'r' => "\r".to_owned(),
        b't' => "\t".to_owned(),
        _ => raw.to_owned(),
    }
}

/// Parse a numeric string to a `Value`.
fn parse_number_value(text: &str) -> Value {
    text.parse::<i64>().map_or_else(
        |_| {
            text.parse::<f64>()
                .map_or_else(|_| Value::Str(text.to_string()), Value::Float)
        },
        Value::Int,
    )
}

/// Parse a JSON number string to a `FieldPresence`.
fn parse_json_number(text: &str) -> FieldPresence {
    FieldPresence::Present(parse_number_value(text))
}

/// Find the root vertex in the CST (the `document` node or first vertex
/// with no incoming edges).
fn find_cst_root(cst: &Schema) -> Result<String, CstExtractError> {
    for (name, v) in &cst.vertices {
        if *v.kind == *"document" {
            return Ok(name.to_string());
        }
    }
    for name in cst.vertices.keys() {
        if cst.incoming_edges(name).is_empty() {
            return Ok(name.to_string());
        }
    }
    Err(CstExtractError::Structure(
        "no root vertex found in CST".into(),
    ))
}

// ── JSON extraction ───────────────────────────────────────────────────

/// Extract a `WInstance` from a JSON CST Schema, guided by a domain schema.
///
/// # Errors
///
/// Returns `CstExtractError` if the CST structure is invalid or doesn't
/// match the domain schema.
pub fn extract_json_cst(
    cst: &Schema,
    domain_schema: &Schema,
    root_vertex: &str,
) -> Result<(WInstance, CstComplement), CstExtractError> {
    if !domain_schema.has_vertex(root_vertex) {
        return Err(CstExtractError::SchemaMismatch(format!(
            "root vertex '{root_vertex}' not found in domain schema"
        )));
    }

    let mut state = ExtractState::new();
    let doc_vertex = find_cst_root(cst)?;

    let top_value = cst_child_by_edge_kind(cst, &doc_vertex, "child_of")
        .ok_or_else(|| CstExtractError::Structure("document has no child".into()))?;

    let root_id = state.alloc_id();
    extract_json_value(
        cst,
        domain_schema,
        top_value,
        root_vertex,
        root_id,
        &mut state,
    )?;

    let complement = CstComplement {
        format: "json".into(),
        cst_schema: cst.clone(),
        node_to_cst_value: state.node_to_cst_value,
        node_to_cst_struct: state.node_to_cst_struct,
    };

    let instance = WInstance::new(
        state.nodes,
        state.arcs,
        Vec::new(),
        root_id,
        Name::from(root_vertex),
    );

    Ok((instance, complement))
}

/// Recursively extract a JSON value node from the CST into a `WInstance` node.
fn extract_json_value(
    cst: &Schema,
    domain_schema: &Schema,
    cst_vertex: &Name,
    domain_vertex: &str,
    node_id: u32,
    state: &mut ExtractState,
) -> Result<(), CstExtractError> {
    let kind = cst_vertex_kind(cst, cst_vertex)
        .ok_or_else(|| CstExtractError::VertexNotFound(cst_vertex.to_string()))?;

    state.node_to_cst_struct.insert(node_id, cst_vertex.clone());

    match kind.as_str() {
        "object" => extract_json_object(
            cst,
            domain_schema,
            cst_vertex,
            domain_vertex,
            node_id,
            state,
        ),
        "array" => extract_json_array(
            cst,
            domain_schema,
            cst_vertex,
            domain_vertex,
            node_id,
            state,
        ),
        "string" => {
            let text = json_string_value(cst, cst_vertex).unwrap_or_default();
            if let Some(sc) = cst_child_by_edge_kind(cst, cst_vertex, "child_of") {
                state.node_to_cst_value.insert(node_id, sc.clone());
            }
            let node = Node::new(node_id, domain_vertex)
                .with_value(FieldPresence::Present(Value::Str(text)));
            state.nodes.insert(node_id, node);
            Ok(())
        }
        "number" => {
            let text = literal_value(cst, cst_vertex).unwrap_or_default();
            state.node_to_cst_value.insert(node_id, cst_vertex.clone());
            let node = Node::new(node_id, domain_vertex).with_value(parse_json_number(&text));
            state.nodes.insert(node_id, node);
            Ok(())
        }
        "true" => {
            state.node_to_cst_value.insert(node_id, cst_vertex.clone());
            let node = Node::new(node_id, domain_vertex)
                .with_value(FieldPresence::Present(Value::Bool(true)));
            state.nodes.insert(node_id, node);
            Ok(())
        }
        "false" => {
            state.node_to_cst_value.insert(node_id, cst_vertex.clone());
            let node = Node::new(node_id, domain_vertex)
                .with_value(FieldPresence::Present(Value::Bool(false)));
            state.nodes.insert(node_id, node);
            Ok(())
        }
        "null" => {
            state.node_to_cst_value.insert(node_id, cst_vertex.clone());
            let node = Node::new(node_id, domain_vertex).with_value(FieldPresence::Null);
            state.nodes.insert(node_id, node);
            Ok(())
        }
        _ => {
            let text = literal_value(cst, cst_vertex).unwrap_or_default();
            let node = Node::new(node_id, domain_vertex)
                .with_value(FieldPresence::Present(Value::Str(text)));
            state.nodes.insert(node_id, node);
            Ok(())
        }
    }
}

/// Extract a JSON object from CST, matching keys to domain schema edges.
///
/// When the domain schema has no outgoing edges (open schema), all pairs
/// become child nodes with synthesized edges, matching legacy codec behavior.
fn extract_json_object(
    cst: &Schema,
    domain_schema: &Schema,
    cst_vertex: &Name,
    domain_vertex: &str,
    node_id: u32,
    state: &mut ExtractState,
) -> Result<(), CstExtractError> {
    let mut node = Node::new(node_id, domain_vertex);

    // Check for $type discriminator
    if let Some(disc_value) = find_json_pair_value(cst, cst_vertex, "$type") {
        if cst_vertex_kind(cst, &disc_value).as_deref() == Some("string") {
            if let Some(text) = json_string_value(cst, &disc_value) {
                node.discriminator = Some(Name::from(text.as_str()));
            }
        }
    }

    let domain_edges: Vec<Edge> = domain_schema.outgoing_edges(domain_vertex).to_vec();

    if domain_edges.is_empty() {
        // Open schema: extract ALL pairs as child nodes with synthesized edges.
        let pairs = cst_children_by_edge_kind(cst, cst_vertex, "child_of");
        for pair_name in pairs {
            if cst_vertex_kind(cst, pair_name).as_deref() != Some("pair") {
                continue;
            }
            if let Some(key) = extract_pair_key(cst, pair_name) {
                if key == "$type" {
                    continue;
                }
                if let Some(value_vertex) = cst_child_by_edge_kind(cst, pair_name, "value") {
                    let child_anchor = format!("{domain_vertex}:{key}");
                    let child_id = state.alloc_id();
                    extract_json_value_open(cst, value_vertex, &child_anchor, child_id, state)?;
                    let synth_edge = Edge {
                        src: Name::from(domain_vertex),
                        tgt: Name::from(child_anchor.as_str()),
                        kind: "prop".into(),
                        name: Some(Name::from(key.as_str())),
                    };
                    state.arcs.push((node_id, child_id, synth_edge));
                }
            }
        }
    } else {
        // Schema-guided extraction
        let mut handled_keys = std::collections::HashSet::new();

        for domain_edge in &domain_edges {
            let field_name = domain_edge.name.as_deref().unwrap_or(&domain_edge.tgt);
            handled_keys.insert(field_name.to_string());

            if let Some(value_cst_vertex) = find_json_pair_value(cst, cst_vertex, field_name) {
                let child_id = state.alloc_id();
                extract_json_value(
                    cst,
                    domain_schema,
                    &value_cst_vertex,
                    &domain_edge.tgt,
                    child_id,
                    state,
                )?;
                state.arcs.push((node_id, child_id, domain_edge.clone()));
            }
        }

        // Preserve unhandled pairs as extra_fields
        let pairs = cst_children_by_edge_kind(cst, cst_vertex, "child_of");
        for pair_name in pairs {
            if cst_vertex_kind(cst, pair_name).as_deref() != Some("pair") {
                continue;
            }
            if let Some(key) = extract_pair_key(cst, pair_name) {
                if key == "$type" || handled_keys.contains(&key) {
                    continue;
                }
                if let Some(value_vertex) = cst_child_by_edge_kind(cst, pair_name, "value") {
                    let val = extract_json_generic_value(cst, value_vertex);
                    node.extra_fields.insert(key, val);
                }
            }
        }
    }

    state.nodes.insert(node_id, node);
    Ok(())
}

/// Extract a JSON value without schema guidance (for open schemas).
fn extract_json_value_open(
    cst: &Schema,
    cst_vertex: &Name,
    anchor: &str,
    node_id: u32,
    state: &mut ExtractState,
) -> Result<(), CstExtractError> {
    let kind = cst_vertex_kind(cst, cst_vertex).unwrap_or_default();
    state.node_to_cst_struct.insert(node_id, cst_vertex.clone());

    match kind.as_str() {
        "object" => {
            let node = Node::new(node_id, anchor);
            let pairs = cst_children_by_edge_kind(cst, cst_vertex, "child_of");
            for pair_name in pairs {
                if cst_vertex_kind(cst, pair_name).as_deref() != Some("pair") {
                    continue;
                }
                if let Some(key) = extract_pair_key(cst, pair_name) {
                    if let Some(value_vertex) = cst_child_by_edge_kind(cst, pair_name, "value") {
                        let child_anchor = format!("{anchor}:{key}");
                        let child_id = state.alloc_id();
                        extract_json_value_open(cst, value_vertex, &child_anchor, child_id, state)?;
                        let synth_edge = Edge {
                            src: Name::from(anchor),
                            tgt: Name::from(child_anchor.as_str()),
                            kind: "prop".into(),
                            name: Some(Name::from(key.as_str())),
                        };
                        state.arcs.push((node_id, child_id, synth_edge));
                    }
                }
            }
            state.nodes.insert(node_id, node);
            Ok(())
        }
        "array" => {
            let mut node = Node::new(node_id, anchor);
            node.shape = NodeShape::List;
            state.nodes.insert(node_id, node);
            let children = cst_children_by_edge_kind(cst, cst_vertex, "child_of");
            for child_name in &children {
                let child_anchor = format!("{anchor}:items");
                let child_id = state.alloc_id();
                extract_json_value_open(cst, child_name, &child_anchor, child_id, state)?;
                let synth_edge = Edge {
                    src: Name::from(anchor),
                    tgt: Name::from(child_anchor.as_str()),
                    kind: "item".into(),
                    name: Some("item".into()),
                };
                state.arcs.push((node_id, child_id, synth_edge));
            }
            Ok(())
        }
        "string" => {
            let text = json_string_value(cst, cst_vertex).unwrap_or_default();
            if let Some(sc) = cst_child_by_edge_kind(cst, cst_vertex, "child_of") {
                state.node_to_cst_value.insert(node_id, sc.clone());
            }
            let node =
                Node::new(node_id, anchor).with_value(FieldPresence::Present(Value::Str(text)));
            state.nodes.insert(node_id, node);
            Ok(())
        }
        "number" => {
            state.node_to_cst_value.insert(node_id, cst_vertex.clone());
            let text = literal_value(cst, cst_vertex).unwrap_or_default();
            let node = Node::new(node_id, anchor).with_value(parse_json_number(&text));
            state.nodes.insert(node_id, node);
            Ok(())
        }
        "true" => {
            state.node_to_cst_value.insert(node_id, cst_vertex.clone());
            let node =
                Node::new(node_id, anchor).with_value(FieldPresence::Present(Value::Bool(true)));
            state.nodes.insert(node_id, node);
            Ok(())
        }
        "false" => {
            state.node_to_cst_value.insert(node_id, cst_vertex.clone());
            let node =
                Node::new(node_id, anchor).with_value(FieldPresence::Present(Value::Bool(false)));
            state.nodes.insert(node_id, node);
            Ok(())
        }
        "null" => {
            state.node_to_cst_value.insert(node_id, cst_vertex.clone());
            let node = Node::new(node_id, anchor).with_value(FieldPresence::Null);
            state.nodes.insert(node_id, node);
            Ok(())
        }
        _ => {
            state.node_to_cst_value.insert(node_id, cst_vertex.clone());
            let text = literal_value(cst, cst_vertex).unwrap_or_default();
            let node =
                Node::new(node_id, anchor).with_value(FieldPresence::Present(Value::Str(text)));
            state.nodes.insert(node_id, node);
            Ok(())
        }
    }
}

/// Extract a JSON array from CST.
fn extract_json_array(
    cst: &Schema,
    domain_schema: &Schema,
    cst_vertex: &Name,
    domain_vertex: &str,
    node_id: u32,
    state: &mut ExtractState,
) -> Result<(), CstExtractError> {
    let mut node = Node::new(node_id, domain_vertex);
    node.shape = NodeShape::List;
    state.nodes.insert(node_id, node);

    let domain_edges: Vec<Edge> = domain_schema.outgoing_edges(domain_vertex).to_vec();
    let item_edge = domain_edges
        .iter()
        .find(|e| *e.kind == *"item" || e.name.as_deref() == Some("item"));

    let children = cst_children_by_edge_kind(cst, cst_vertex, "child_of");
    if let Some(edge) = item_edge {
        // Domain schema declares the item slot: walk children with the
        // declared item-target kind so type-aware routing kicks in.
        for child_name in children {
            let child_id = state.alloc_id();
            extract_json_value(cst, domain_schema, child_name, &edge.tgt, child_id, state)?;
            state.arcs.push((node_id, child_id, edge.clone()));
        }
    } else {
        // Open / partial schema: synthesise an `item` edge per child
        // and recurse through the open extractor. Without this branch
        // a top-level JSON array (or any array reaching a domain
        // vertex with no declared item edge) silently parsed to an
        // empty list because no children were collected.
        for child_name in &children {
            let child_anchor = format!("{domain_vertex}:items");
            let child_id = state.alloc_id();
            extract_json_value_open(cst, child_name, &child_anchor, child_id, state)?;
            let synth_edge = Edge {
                src: Name::from(domain_vertex),
                tgt: Name::from(child_anchor.as_str()),
                kind: "item".into(),
                name: Some("item".into()),
            };
            state.arcs.push((node_id, child_id, synth_edge));
        }
    }

    Ok(())
}

/// Find the value vertex of a JSON pair with the given key name.
fn find_json_pair_value(cst: &Schema, object_vertex: &Name, key_name: &str) -> Option<Name> {
    let pairs = cst_children_by_edge_kind(cst, object_vertex, "child_of");
    for pair_name in pairs {
        if cst_vertex_kind(cst, pair_name).as_deref() != Some("pair") {
            continue;
        }
        if extract_pair_key(cst, pair_name).as_deref() == Some(key_name) {
            return cst_child_by_edge_kind(cst, pair_name, "value").cloned();
        }
    }
    None
}

/// Extract the key string from a JSON pair vertex.
fn extract_pair_key(cst: &Schema, pair_vertex: &Name) -> Option<String> {
    let key_vertex = cst_child_by_edge_kind(cst, pair_vertex, "key")?;
    json_string_value(cst, key_vertex)
}

/// Extract a generic JSON value from CST as a panproto `Value` (for `extra_fields`).
fn extract_json_generic_value(cst: &Schema, cst_vertex: &Name) -> Value {
    let Some(kind) = cst_vertex_kind(cst, cst_vertex) else {
        return Value::Null;
    };

    match kind.as_str() {
        "string" => Value::Str(json_string_value(cst, cst_vertex).unwrap_or_default()),
        "number" => parse_number_value(&literal_value(cst, cst_vertex).unwrap_or_default()),
        "true" => Value::Bool(true),
        "false" => Value::Bool(false),
        "null" => Value::Null,
        "object" => {
            let mut fields = HashMap::new();
            let pairs = cst_children_by_edge_kind(cst, cst_vertex, "child_of");
            for pair_name in pairs {
                if cst_vertex_kind(cst, pair_name).as_deref() != Some("pair") {
                    continue;
                }
                if let Some(key) = extract_pair_key(cst, pair_name) {
                    if let Some(val_vertex) = cst_child_by_edge_kind(cst, pair_name, "value") {
                        fields.insert(key, extract_json_generic_value(cst, val_vertex));
                    }
                }
            }
            Value::Unknown(fields)
        }
        "array" => {
            let children = cst_children_by_edge_kind(cst, cst_vertex, "child_of");
            let items: Vec<Value> = children
                .iter()
                .map(|child_name| extract_json_generic_value(cst, child_name))
                .collect();
            Value::List(items)
        }
        _ => Value::Str(literal_value(cst, cst_vertex).unwrap_or_default()),
    }
}

// ── JSON injection ────────────────────────────────────────────────────

/// Inject a (possibly modified) `WInstance` back into a CST Schema.
///
/// Updates the `literal-value` and `interstitial-*` constraints in the
/// CST Schema to reflect changes in the `WInstance`. The result can be
/// emitted via `emit_from_schema` to produce formatted bytes.
///
/// # Errors
///
/// Returns `CstExtractError` if the complement is invalid.
pub fn inject_json_cst(
    instance: &WInstance,
    complement: &CstComplement,
    _domain_schema: &Schema,
) -> Result<Schema, CstExtractError> {
    let mut cst = complement.cst_schema.clone();

    for (&node_id, node) in &instance.nodes {
        if let Some(ref presence) = node.value {
            if let Some(cst_vertex) = complement.node_to_cst_value.get(&node_id) {
                // For strings the CST may store the value across multiple
                // child segments (text + escape_sequence + ...). The
                // tracked `node_to_cst_value` points at the first
                // segment only; updating it with the fully decoded text
                // would either truncate (when the new text is shorter
                // than the original token) or lose escape style (when
                // it is the same value but written ` ` vs
                // literally). When the parsed string still matches the
                // instance value, skip the inject so the original
                // bytes survive untouched.
                if let Value::Str(s) = match presence {
                    FieldPresence::Present(v) => v,
                    _ => &Value::Null,
                } {
                    if let Some(struct_vertex) = complement.node_to_cst_struct.get(&node_id) {
                        if let Some(original) = json_string_value(&cst, struct_vertex) {
                            if &original == s {
                                continue;
                            }
                        }
                    }
                }
                let new_text = field_presence_to_json_text(presence);
                update_literal_value(&mut cst, cst_vertex, &new_text);
            }
        }
    }

    Ok(cst)
}

/// Convert a `FieldPresence` to the JSON text representation.
///
/// Used by [`inject_json_cst`] to overwrite `literal-value` constraints
/// on a CST leaf when the instance has changed. The produced text must
/// match the wire format of a JSON token bit-for-bit so the
/// format-preserving emitter is byte-identity for unchanged instances:
///
/// - `Str(s)` is JSON-encoded (escapes restored, no surrounding quotes
///   because tree-sitter's `string_content` constraint covers only the
///   inner bytes).
/// - `Float(f)` always renders with a decimal point, so `6.0` round-trips
///   as `"6.0"` rather than `"6"` (the default `f64::Display`).
/// - `Null` / `Present(Null)` render as the literal token `null`.
fn field_presence_to_json_text(presence: &FieldPresence) -> String {
    match presence {
        FieldPresence::Present(Value::Str(s)) => json_encode_string_content(s),
        FieldPresence::Present(Value::Int(i)) => i.to_string(),
        FieldPresence::Present(Value::Float(f)) => json_format_float(*f),
        FieldPresence::Present(Value::Bool(b)) => b.to_string(),
        FieldPresence::Null | FieldPresence::Present(Value::Null) => "null".into(),
        FieldPresence::Absent => String::new(),
        FieldPresence::Present(other) => format!("{other:?}"),
    }
}

/// Encode a Rust string as the JSON `string_content` body (between the
/// surrounding quotes, with `"`, `\`, control characters, and forward
/// slash escapes restored). Mirrors the inverse of `decode_json_escape`
/// + the lookahead that joins `\u` with its four-hex tail.
fn json_encode_string_content(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{0008}' => out.push_str("\\b"),
            '\u{000C}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                use std::fmt::Write as _;
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out
}

/// Format an `f64` as a JSON numeric token that always carries either a
/// decimal point or an exponent. `format!("{f}")` collapses
/// integer-valued floats to `"6"`, which round-trips through the
/// JSON parser as an integer rather than a float; appending `.0` keeps
/// the original lexical category.
fn json_format_float(f: f64) -> String {
    if f.is_finite() {
        let s = f.to_string();
        if s.contains('.') || s.contains('e') || s.contains('E') {
            s
        } else {
            format!("{s}.0")
        }
    } else {
        // JSON has no syntax for NaN/Infinity; fall back to `null` so
        // the inject does not produce unparseable bytes.
        "null".into()
    }
}

/// Update the `literal-value` and co-occurring interstitial constraints
/// on a CST vertex.
///
/// The `AstWalker` stores both `literal-value` and `interstitial-N` on
/// leaf nodes. When they carry the same text (which happens for identifiers,
/// numbers, keywords, and bare text), both must be updated together.
/// We first read the old `literal-value`, then update both constraints
/// only when the interstitial was previously equal to the old literal.
fn update_literal_value(cst: &mut Schema, vertex: &Name, new_text: &str) {
    if let Some(constraints) = cst.constraints.get_mut(vertex) {
        // First pass: find the old literal-value.
        let old_literal = constraints
            .iter()
            .find(|c| c.sort.as_ref() == "literal-value")
            .map(|c| c.value.clone());

        // Second pass: update literal-value and matching interstitials.
        for c in constraints.iter_mut() {
            if c.sort.as_ref() == "literal-value" {
                c.value = new_text.to_string();
            } else if c.sort.starts_with("interstitial-") && !c.sort.ends_with("-start-byte") {
                // Update the interstitial only if it previously matched the
                // old literal value exactly. This preserves punctuation and
                // whitespace interstitials while updating text interstitials.
                if let Some(ref old) = old_literal {
                    if c.value == *old {
                        c.value = new_text.to_string();
                    }
                }
            }
        }
    }
}

// ── XML extraction ────────────────────────────────────────────────────

/// Extract a `WInstance` from an XML CST Schema, guided by a domain schema.
///
/// The tree-sitter-xml grammar produces:
/// - `document` → `element` (root edge)
/// - `element` → `STag` + `content` + `ETag` (`child_of` edges)
/// - `STag` → `Name` + `Attribute` × N (`child_of` edges)
/// - `Attribute` → `Name` + `AttValue` (`child_of` edges)
/// - `content` → `CharData` + `element` × N (`child_of` edges)
///
/// # Errors
///
/// Returns `CstExtractError` if the CST is invalid.
pub fn extract_xml_cst(
    cst: &Schema,
    domain_schema: &Schema,
    root_vertex: &str,
) -> Result<(WInstance, CstComplement), CstExtractError> {
    if !domain_schema.has_vertex(root_vertex) {
        return Err(CstExtractError::SchemaMismatch(format!(
            "root vertex '{root_vertex}' not found in domain schema"
        )));
    }

    let mut state = ExtractState::new();
    let doc_vertex = find_cst_root(cst)?;

    let root_element = find_first_element_child(cst, &doc_vertex)
        .ok_or_else(|| CstExtractError::Structure("no element child in XML document".into()))?;

    let root_id = state.alloc_id();
    extract_xml_element(
        cst,
        domain_schema,
        &root_element,
        root_vertex,
        root_id,
        &mut state,
    )?;

    let complement = CstComplement {
        format: "xml".into(),
        cst_schema: cst.clone(),
        node_to_cst_value: state.node_to_cst_value,
        node_to_cst_struct: state.node_to_cst_struct,
    };

    let instance = WInstance::new(
        state.nodes,
        state.arcs,
        Vec::new(),
        root_id,
        Name::from(root_vertex),
    );

    Ok((instance, complement))
}

fn find_first_element_child(cst: &Schema, parent: &str) -> Option<Name> {
    for edge in cst.outgoing_edges(parent) {
        if cst_vertex_kind(cst, &edge.tgt).as_deref() == Some("element") {
            return Some(edge.tgt.clone());
        }
    }
    None
}

#[allow(clippy::too_many_lines)]
fn extract_xml_element(
    cst: &Schema,
    domain_schema: &Schema,
    cst_vertex: &Name,
    domain_vertex: &str,
    node_id: u32,
    state: &mut ExtractState,
) -> Result<(), CstExtractError> {
    let mut node = Node::new(node_id, domain_vertex);
    state.node_to_cst_struct.insert(node_id, cst_vertex.clone());

    let mut stag_vertex = None;
    let mut content_vertex = None;

    for edge in cst.outgoing_edges(cst_vertex) {
        match cst_vertex_kind(cst, &edge.tgt).as_deref() {
            Some("STag" | "EmptyElemTag") => stag_vertex = Some(edge.tgt.clone()),
            Some("content") => content_vertex = Some(edge.tgt.clone()),
            _ => {}
        }
    }

    if let Some(tag) = extract_xml_tag_name(cst, cst_vertex) {
        node.shape = NodeShape::XmlElement {
            tag: panproto_gat::Name::from(tag.as_str()),
        };
    }

    if let Some(ref stag) = stag_vertex {
        extract_xml_attributes(cst, stag, &mut node);
    }

    if let Some(ref content) = content_vertex {
        let content_children = ordered_content_children(cst, content);
        let has_elements = content_children.iter().any(|(kind, _)| kind == "element");
        let has_nonempty_text = content_children.iter().any(|(kind, name)| {
            kind == "CharData"
                && !literal_value(cst, name)
                    .unwrap_or_default()
                    .trim()
                    .is_empty()
        });
        let domain_edges: Vec<Edge> = domain_schema.outgoing_edges(domain_vertex).to_vec();

        if !has_elements {
            // Pure text content: collapse to node.value (existing
            // semantics; preserves the leaf-element fast path where a
            // round-trip just stores the captured text).
            if let Some(text) = extract_xml_text_content(cst, content) {
                if !text.trim().is_empty() {
                    node.value = Some(FieldPresence::Present(Value::Str(text)));
                    for edge in cst.outgoing_edges(content) {
                        if cst_vertex_kind(cst, &edge.tgt).as_deref() == Some("CharData") {
                            state.node_to_cst_value.insert(node_id, edge.tgt.clone());
                            break;
                        }
                    }
                }
            }
        } else if has_nonempty_text {
            // Mixed content: walk content in source order, emitting a
            // text-segment leaf for each non-blank CharData and a
            // regular element child for each `element`. Preserves the
            // original interleaving so `<p>x<em>y</em>z</p>` round-trips
            // through emit without losing the trailing text run.
            for (kind, child_vertex) in &content_children {
                if kind == "CharData" {
                    let text = literal_value(cst, child_vertex).unwrap_or_default();
                    if text.trim().is_empty() {
                        continue;
                    }
                    let seg_anchor = format!("{domain_vertex}:text");
                    let seg_id = state.alloc_id();
                    let mut seg_node = Node::new(seg_id, seg_anchor.as_str());
                    seg_node.value = Some(FieldPresence::Present(Value::Str(text)));
                    seg_node.shape = NodeShape::XmlTextSegment;
                    state.nodes.insert(seg_id, seg_node);
                    state.node_to_cst_value.insert(seg_id, child_vertex.clone());
                    let edge = Edge {
                        src: Name::from(domain_vertex),
                        tgt: Name::from(seg_anchor.as_str()),
                        kind: "prop".into(),
                        name: Some(Name::from("$xml_text")),
                    };
                    state.arcs.push((node_id, seg_id, edge));
                } else if kind == "element" {
                    let tag =
                        extract_xml_tag_name(cst, child_vertex).unwrap_or_else(|| "child".into());
                    let child_anchor = format!("{domain_vertex}:{tag}");
                    let child_id = state.alloc_id();
                    extract_xml_element(
                        cst,
                        domain_schema,
                        child_vertex,
                        &child_anchor,
                        child_id,
                        state,
                    )?;
                    let synth_edge = Edge {
                        src: Name::from(domain_vertex),
                        tgt: Name::from(child_anchor.as_str()),
                        kind: "prop".into(),
                        name: Some(Name::from(tag.as_str())),
                    };
                    state.arcs.push((node_id, child_id, synth_edge));
                }
            }
            state.nodes.insert(node_id, node);
            return Ok(());
        }

        let child_elements: Vec<Name> = content_children
            .iter()
            .filter(|(kind, _)| kind == "element")
            .map(|(_, name)| name.clone())
            .collect();

        if domain_edges.is_empty() {
            // Open schema: extract ALL child elements (text-only, so
            // ordering relative to text segments is moot).
            for child_name in &child_elements {
                let tag = extract_xml_tag_name(cst, child_name).unwrap_or_else(|| "child".into());
                let child_anchor = format!("{domain_vertex}:{tag}");
                let child_id = state.alloc_id();
                extract_xml_element(
                    cst,
                    domain_schema,
                    child_name,
                    &child_anchor,
                    child_id,
                    state,
                )?;
                let synth_edge = Edge {
                    src: Name::from(domain_vertex),
                    tgt: Name::from(child_anchor.as_str()),
                    kind: "prop".into(),
                    name: Some(Name::from(tag.as_str())),
                };
                state.arcs.push((node_id, child_id, synth_edge));
            }
        } else {
            for domain_edge in &domain_edges {
                let field_name = domain_edge.name.as_deref().unwrap_or(&domain_edge.tgt);
                let is_item =
                    *domain_edge.kind == *"item" || domain_edge.name.as_deref() == Some("item");

                if is_item {
                    for child_name in &child_elements {
                        let child_id = state.alloc_id();
                        extract_xml_element(
                            cst,
                            domain_schema,
                            child_name,
                            &domain_edge.tgt,
                            child_id,
                            state,
                        )?;
                        state.arcs.push((node_id, child_id, domain_edge.clone()));
                    }
                } else {
                    for child_name in &child_elements {
                        if extract_xml_tag_name(cst, child_name).as_deref() == Some(field_name) {
                            let child_id = state.alloc_id();
                            extract_xml_element(
                                cst,
                                domain_schema,
                                child_name,
                                &domain_edge.tgt,
                                child_id,
                                state,
                            )?;
                            state.arcs.push((node_id, child_id, domain_edge.clone()));
                        }
                    }
                }
            }
        }
    }

    state.nodes.insert(node_id, node);
    Ok(())
}

fn extract_xml_attributes(cst: &Schema, stag_vertex: &Name, node: &mut Node) {
    for edge in cst.outgoing_edges(stag_vertex) {
        if cst_vertex_kind(cst, &edge.tgt).as_deref() == Some("Attribute") {
            if let (Some(attr_name), Some(attr_value)) = (
                extract_xml_attr_name(cst, &edge.tgt),
                extract_xml_attr_value(cst, &edge.tgt),
            ) {
                node.extra_fields.insert(attr_name, Value::Str(attr_value));
            }
        }
    }
}

fn extract_xml_attr_name(cst: &Schema, attr_vertex: &Name) -> Option<String> {
    for edge in cst.outgoing_edges(attr_vertex) {
        if cst_vertex_kind(cst, &edge.tgt).as_deref() == Some("Name") {
            return literal_value(cst, &edge.tgt);
        }
    }
    None
}

fn extract_xml_attr_value(cst: &Schema, attr_vertex: &Name) -> Option<String> {
    for edge in cst.outgoing_edges(attr_vertex) {
        if cst_vertex_kind(cst, &edge.tgt).as_deref() == Some("AttValue") {
            let raw = literal_value(cst, &edge.tgt)?;
            let trimmed = raw
                .strip_prefix('"')
                .and_then(|s| s.strip_suffix('"'))
                .or_else(|| raw.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')))
                .unwrap_or(&raw);
            return Some(trimmed.to_string());
        }
    }
    None
}

fn extract_xml_text_content(cst: &Schema, content_vertex: &Name) -> Option<String> {
    let mut texts = Vec::new();
    for edge in cst.outgoing_edges(content_vertex) {
        if cst_vertex_kind(cst, &edge.tgt).as_deref() == Some("CharData") {
            if let Some(text) = literal_value(cst, &edge.tgt) {
                texts.push(text);
            }
        }
    }
    if texts.is_empty() {
        None
    } else {
        Some(texts.join(""))
    }
}

fn extract_xml_tag_name(cst: &Schema, element_vertex: &Name) -> Option<String> {
    for edge in cst.outgoing_edges(element_vertex) {
        let kind = cst_vertex_kind(cst, &edge.tgt).unwrap_or_default();
        if kind == "STag" || kind == "EmptyElemTag" {
            for inner in cst.outgoing_edges(&edge.tgt) {
                if cst_vertex_kind(cst, &inner.tgt).as_deref() == Some("Name") {
                    return literal_value(cst, &inner.tgt);
                }
            }
        }
    }
    None
}

/// Walk an XML `content` vertex's children in source order, returning
/// `(kind, vertex_id)` pairs for `CharData` text runs and `element`
/// children. Used by [`extract_xml_element`] to preserve the
/// interleaving order of mixed XML content.
fn ordered_content_children(cst: &Schema, content_vertex: &Name) -> Vec<(String, Name)> {
    let mut entries: Vec<(usize, String, Name)> = cst
        .outgoing_edges(content_vertex)
        .iter()
        .filter_map(|e| {
            let kind = cst_vertex_kind(cst, &e.tgt)?;
            let pos = cst
                .constraints
                .get(&e.tgt)
                .and_then(|cs| {
                    cs.iter()
                        .find(|c| c.sort.as_ref() == "start-byte")
                        .and_then(|c| c.value.parse::<usize>().ok())
                })
                .unwrap_or(0);
            Some((pos, kind, e.tgt.clone()))
        })
        .collect();
    entries.sort_by_key(|(pos, _, _)| *pos);
    entries
        .into_iter()
        .map(|(_, kind, name)| (kind, name))
        .collect()
}

/// Inject a (possibly modified) `WInstance` back into an XML CST Schema.
///
/// Updates `CharData` literal values for text content and `AttValue`
/// constraints for attributes stored in `node.extra_fields`.
///
/// # Errors
///
/// Returns `CstExtractError` if the complement is invalid.
pub fn inject_xml_cst(
    instance: &WInstance,
    complement: &CstComplement,
    _domain_schema: &Schema,
) -> Result<Schema, CstExtractError> {
    let mut cst = complement.cst_schema.clone();

    for (&node_id, node) in &instance.nodes {
        // Update text content via node_to_cst_value mapping
        if let Some(ref presence) = node.value {
            if let Some(cst_vertex) = complement.node_to_cst_value.get(&node_id) {
                let new_text = field_presence_to_text(presence);
                update_literal_value(&mut cst, cst_vertex, &new_text);
            }
        }

        // Update attributes: find the STag in the CST structural node
        // and update AttValue constraints for modified extra_fields.
        if !node.extra_fields.is_empty() {
            if let Some(cst_struct) = complement.node_to_cst_struct.get(&node_id) {
                inject_xml_attributes(&mut cst, cst_struct, &node.extra_fields);
            }
        }
    }

    Ok(cst)
}

/// Update XML attribute values in the CST from node `extra_fields`.
fn inject_xml_attributes(
    cst: &mut Schema,
    element_vertex: &Name,
    extra_fields: &HashMap<String, Value>,
) {
    // Find the STag child of this element
    let stag = cst
        .outgoing_edges(element_vertex)
        .iter()
        .find(|e| {
            cst.vertex(&e.tgt)
                .is_some_and(|v| *v.kind == *"STag" || *v.kind == *"EmptyElemTag")
        })
        .map(|e| e.tgt.clone());

    let Some(stag_name) = stag else { return };

    // For each Attribute child of the STag, check if its name matches
    // an extra_field and update the AttValue if so.
    let attr_edges: Vec<_> = cst
        .outgoing_edges(&stag_name)
        .iter()
        .filter(|e| cst.vertex(&e.tgt).is_some_and(|v| *v.kind == *"Attribute"))
        .map(|e| e.tgt.clone())
        .collect();

    for attr_vertex in &attr_edges {
        let attr_name = extract_xml_attr_name(cst, attr_vertex);
        if let Some(ref name) = attr_name {
            if let Some(new_value) = extra_fields.get(name) {
                let text = match new_value {
                    Value::Str(s) => s.clone(),
                    other => format!("{other:?}"),
                };
                // Find and update the AttValue child
                let attvalue_vertex = cst
                    .outgoing_edges(attr_vertex)
                    .iter()
                    .find(|e| cst.vertex(&e.tgt).is_some_and(|v| *v.kind == *"AttValue"))
                    .map(|e| e.tgt.clone());
                if let Some(av) = attvalue_vertex {
                    // The AttValue literal includes quotes; update the inner text
                    let quoted = format!("\"{text}\"");
                    update_literal_value(&mut *cst, &av, &quoted);
                }
            }
        }
    }
}

/// Convert a `FieldPresence` to plain text (for XML text content).
fn field_presence_to_text(presence: &FieldPresence) -> String {
    match presence {
        FieldPresence::Present(Value::Str(s)) => s.clone(),
        FieldPresence::Present(Value::Int(i)) => i.to_string(),
        FieldPresence::Present(Value::Float(f)) => f.to_string(),
        FieldPresence::Present(Value::Bool(b)) => b.to_string(),
        FieldPresence::Null | FieldPresence::Present(Value::Null) | FieldPresence::Absent => {
            String::new()
        }
        FieldPresence::Present(other) => format!("{other:?}"),
    }
}

// ── YAML extraction ───────────────────────────────────────────────────

/// Extract a `WInstance` from a YAML CST Schema, guided by a domain schema.
///
/// # Errors
///
/// Returns `CstExtractError` if the CST is invalid.
pub fn extract_yaml_cst(
    cst: &Schema,
    domain_schema: &Schema,
    root_vertex: &str,
) -> Result<(WInstance, CstComplement), CstExtractError> {
    if !domain_schema.has_vertex(root_vertex) {
        return Err(CstExtractError::SchemaMismatch(format!(
            "root vertex '{root_vertex}' not found in domain schema"
        )));
    }

    let mut state = ExtractState::new();
    let doc_vertex = find_cst_root(cst)?;

    let top_value = find_yaml_top_value(cst, &doc_vertex)
        .ok_or_else(|| CstExtractError::Structure("no value node in YAML document".into()))?;

    let root_id = state.alloc_id();
    extract_yaml_value(
        cst,
        domain_schema,
        &top_value,
        root_vertex,
        root_id,
        &mut state,
    )?;

    let complement = CstComplement {
        format: "yaml".into(),
        cst_schema: cst.clone(),
        node_to_cst_value: state.node_to_cst_value,
        node_to_cst_struct: state.node_to_cst_struct,
    };

    let instance = WInstance::new(
        state.nodes,
        state.arcs,
        Vec::new(),
        root_id,
        Name::from(root_vertex),
    );

    Ok((instance, complement))
}

fn find_yaml_top_value(cst: &Schema, root: &str) -> Option<Name> {
    let mut current = Name::from(root);
    for _ in 0..5 {
        let kind = cst_vertex_kind(cst, &current)?;
        match kind.as_str() {
            "block_mapping"
            | "flow_mapping"
            | "block_sequence"
            | "flow_sequence"
            | "plain_scalar"
            | "double_quote_scalar"
            | "single_quote_scalar"
            | "block_scalar"
            | "integer_scalar"
            | "float_scalar"
            | "boolean_scalar"
            | "null_scalar" => return Some(current),
            _ => {
                current = cst_child_by_edge_kind(cst, &current, "child_of")?.clone();
            }
        }
    }
    None
}

fn extract_yaml_value(
    cst: &Schema,
    domain_schema: &Schema,
    cst_vertex: &Name,
    domain_vertex: &str,
    node_id: u32,
    state: &mut ExtractState,
) -> Result<(), CstExtractError> {
    let kind = cst_vertex_kind(cst, cst_vertex).unwrap_or_default();
    state.node_to_cst_struct.insert(node_id, cst_vertex.clone());

    match kind.as_str() {
        "block_mapping" | "flow_mapping" => extract_yaml_mapping(
            cst,
            domain_schema,
            cst_vertex,
            domain_vertex,
            node_id,
            state,
        ),
        "block_sequence" | "flow_sequence" => extract_yaml_sequence(
            cst,
            domain_schema,
            cst_vertex,
            domain_vertex,
            node_id,
            state,
        ),
        _ => {
            let text = literal_value(cst, cst_vertex)
                .or_else(|| {
                    cst.outgoing_edges(cst_vertex)
                        .iter()
                        .find_map(|e| literal_value(cst, &e.tgt))
                })
                .unwrap_or_default();
            state.node_to_cst_value.insert(node_id, cst_vertex.clone());
            let node = Node::new(node_id, domain_vertex).with_value(parse_yaml_scalar(&text));
            state.nodes.insert(node_id, node);
            Ok(())
        }
    }
}

fn extract_yaml_mapping(
    cst: &Schema,
    domain_schema: &Schema,
    cst_vertex: &Name,
    domain_vertex: &str,
    node_id: u32,
    state: &mut ExtractState,
) -> Result<(), CstExtractError> {
    let mut node = Node::new(node_id, domain_vertex);
    let domain_edges: Vec<Edge> = domain_schema.outgoing_edges(domain_vertex).to_vec();
    let mut handled_keys = std::collections::HashSet::new();
    let pairs = cst_children_by_edge_kind(cst, cst_vertex, "child_of");

    for domain_edge in &domain_edges {
        let field_name = domain_edge.name.as_deref().unwrap_or(&domain_edge.tgt);
        handled_keys.insert(field_name.to_string());

        for pair_name in &pairs {
            if extract_yaml_pair_key(cst, pair_name).as_deref() == Some(field_name) {
                if let Some(value_vertex) = find_yaml_pair_value(cst, pair_name) {
                    let child_id = state.alloc_id();
                    extract_yaml_value(
                        cst,
                        domain_schema,
                        &value_vertex,
                        &domain_edge.tgt,
                        child_id,
                        state,
                    )?;
                    state.arcs.push((node_id, child_id, domain_edge.clone()));
                }
            }
        }
    }

    for pair_name in &pairs {
        if let Some(key_text) = extract_yaml_pair_key(cst, pair_name) {
            if handled_keys.contains(&key_text) {
                continue;
            }
            if let Some(value_vertex) = find_yaml_pair_value(cst, pair_name) {
                let val = extract_yaml_generic_value(cst, &value_vertex);
                node.extra_fields.insert(key_text, val);
            }
        }
    }

    state.nodes.insert(node_id, node);
    Ok(())
}

fn extract_yaml_sequence(
    cst: &Schema,
    domain_schema: &Schema,
    cst_vertex: &Name,
    domain_vertex: &str,
    node_id: u32,
    state: &mut ExtractState,
) -> Result<(), CstExtractError> {
    let mut node = Node::new(node_id, domain_vertex);
    node.shape = NodeShape::List;
    state.nodes.insert(node_id, node);

    let domain_edges: Vec<Edge> = domain_schema.outgoing_edges(domain_vertex).to_vec();
    let item_edge = domain_edges
        .iter()
        .find(|e| *e.kind == *"item" || e.name.as_deref() == Some("item"));

    if let Some(edge) = item_edge {
        let items = cst_children_by_edge_kind(cst, cst_vertex, "child_of");
        for item_name in items {
            let value_vertex =
                find_yaml_sequence_item_value(cst, item_name).unwrap_or_else(|| item_name.clone());
            let child_id = state.alloc_id();
            extract_yaml_value(
                cst,
                domain_schema,
                &value_vertex,
                &edge.tgt,
                child_id,
                state,
            )?;
            state.arcs.push((node_id, child_id, edge.clone()));
        }
    }

    Ok(())
}

fn extract_yaml_pair_key(cst: &Schema, pair_vertex: &Name) -> Option<String> {
    let key_vertex = cst_child_by_edge_kind(cst, pair_vertex, "key")?;
    literal_value(cst, key_vertex).or_else(|| {
        cst.outgoing_edges(key_vertex)
            .iter()
            .find_map(|e| literal_value(cst, &e.tgt))
    })
}

fn find_yaml_pair_value(cst: &Schema, pair_vertex: &Name) -> Option<Name> {
    cst_child_by_edge_kind(cst, pair_vertex, "value").cloned()
}

fn find_yaml_sequence_item_value(cst: &Schema, item_vertex: &Name) -> Option<Name> {
    for edge in cst.outgoing_edges(item_vertex) {
        let kind = cst_vertex_kind(cst, &edge.tgt).unwrap_or_default();
        match kind.as_str() {
            "block_mapping"
            | "flow_mapping"
            | "block_sequence"
            | "flow_sequence"
            | "plain_scalar"
            | "double_quote_scalar"
            | "single_quote_scalar"
            | "block_scalar"
            | "integer_scalar"
            | "float_scalar" => return Some(edge.tgt.clone()),
            _ => {}
        }
    }
    None
}

fn parse_yaml_scalar(text: &str) -> FieldPresence {
    match text {
        "true" | "True" | "TRUE" | "yes" | "Yes" | "YES" | "on" | "On" | "ON" => {
            FieldPresence::Present(Value::Bool(true))
        }
        "false" | "False" | "FALSE" | "no" | "No" | "NO" | "off" | "Off" | "OFF" => {
            FieldPresence::Present(Value::Bool(false))
        }
        "null" | "Null" | "NULL" | "~" | "" => FieldPresence::Null,
        _ => FieldPresence::Present(parse_number_value(text)),
    }
}

fn extract_yaml_generic_value(cst: &Schema, cst_vertex: &Name) -> Value {
    let kind = cst_vertex_kind(cst, cst_vertex).unwrap_or_default();

    match kind.as_str() {
        "block_mapping" | "flow_mapping" => {
            let mut fields = HashMap::new();
            let pairs = cst_children_by_edge_kind(cst, cst_vertex, "child_of");
            for pair_name in pairs {
                if let Some(key) = extract_yaml_pair_key(cst, pair_name) {
                    if let Some(val_vertex) = find_yaml_pair_value(cst, pair_name) {
                        fields.insert(key, extract_yaml_generic_value(cst, &val_vertex));
                    }
                }
            }
            Value::Unknown(fields)
        }
        "block_sequence" | "flow_sequence" => {
            let mut fields = HashMap::new();
            let items = cst_children_by_edge_kind(cst, cst_vertex, "child_of");
            for (i, item_name) in items.iter().enumerate() {
                let val_vertex = find_yaml_sequence_item_value(cst, item_name)
                    .unwrap_or_else(|| (*item_name).clone());
                fields.insert(i.to_string(), extract_yaml_generic_value(cst, &val_vertex));
            }
            Value::Unknown(fields)
        }
        _ => {
            let text = literal_value(cst, cst_vertex)
                .or_else(|| {
                    cst.outgoing_edges(cst_vertex)
                        .iter()
                        .find_map(|e| literal_value(cst, &e.tgt))
                })
                .unwrap_or_default();
            match parse_yaml_scalar(&text) {
                FieldPresence::Present(v) => v,
                FieldPresence::Null | FieldPresence::Absent => Value::Null,
            }
        }
    }
}

/// Inject a (possibly modified) `WInstance` back into a YAML CST Schema.
///
/// Updates scalar `literal-value` constraints in YAML-specific CST nodes
/// (`plain_scalar`, `double_quote_scalar`, `single_quote_scalar`,
/// `integer_scalar`, `float_scalar`, etc.).
///
/// # Errors
///
/// Returns `CstExtractError` if the complement is invalid.
pub fn inject_yaml_cst(
    instance: &WInstance,
    complement: &CstComplement,
    _domain_schema: &Schema,
) -> Result<Schema, CstExtractError> {
    let mut cst = complement.cst_schema.clone();

    for (&node_id, node) in &instance.nodes {
        if let Some(ref presence) = node.value {
            if let Some(cst_vertex) = complement.node_to_cst_value.get(&node_id) {
                let new_text = field_presence_to_yaml_text(presence);
                update_literal_value(&mut cst, cst_vertex, &new_text);
                // YAML scalars may have the literal in a child node too;
                // check and update children.
                let child_vertices: Vec<_> = cst
                    .outgoing_edges(cst_vertex)
                    .iter()
                    .map(|e| e.tgt.clone())
                    .collect();
                for child in &child_vertices {
                    update_literal_value(&mut cst, child, &new_text);
                }
            }
        }
    }

    Ok(cst)
}

/// Convert a `FieldPresence` to YAML scalar text.
fn field_presence_to_yaml_text(presence: &FieldPresence) -> String {
    match presence {
        FieldPresence::Present(Value::Str(s)) => s.clone(),
        FieldPresence::Present(Value::Int(i)) => i.to_string(),
        FieldPresence::Present(Value::Float(f)) => f.to_string(),
        FieldPresence::Present(Value::Bool(true)) => "true".to_string(),
        FieldPresence::Present(Value::Bool(false)) => "false".to_string(),
        FieldPresence::Null | FieldPresence::Present(Value::Null) => "null".to_string(),
        FieldPresence::Absent => "~".to_string(),
        FieldPresence::Present(other) => format!("{other:?}"),
    }
}

// ── Tabular extraction ────────────────────────────────────────────────

/// Extract an `FInstance` from a CSV/TSV CST Schema.
///
/// The first row is treated as headers (column names).
/// The complement records the CST vertex for each cell so that
/// `inject_tabular_cst` can update cell values.
///
/// # Errors
///
/// Returns `CstExtractError` if the CST is invalid.
pub fn extract_tabular_cst(
    cst: &Schema,
    _domain_schema: &Schema,
    table_vertex: &str,
) -> Result<(FInstance, CstComplement), CstExtractError> {
    let doc_vertex = find_cst_root(cst)?;
    let row_vertices = cst_children_by_edge_kind(cst, &doc_vertex, "child_of");

    if row_vertices.is_empty() {
        let complement = CstComplement {
            format: "tabular".into(),
            cst_schema: cst.clone(),
            node_to_cst_value: HashMap::new(),
            node_to_cst_struct: HashMap::new(),
        };
        return Ok((FInstance::new(), complement));
    }

    let header_fields = cst_children_by_edge_kind(cst, row_vertices[0], "child_of");
    let headers: Vec<String> = header_fields
        .iter()
        .map(|f| literal_text_deep(cst, f))
        .collect();

    // Track CST vertex for each cell: keyed by (row_index, col_index)
    // encoded as a u32 node ID = row_index * 10000 + col_index.
    // This is stored in node_to_cst_value for injection.
    let mut cell_to_cst: HashMap<u32, Name> = HashMap::new();
    let mut rows = Vec::new();

    for (row_idx, row_name) in row_vertices[1..].iter().enumerate() {
        let fields = cst_children_by_edge_kind(cst, row_name, "child_of");
        let mut row = HashMap::new();
        for (col_idx, field_name) in fields.iter().enumerate() {
            let col = headers
                .get(col_idx)
                .cloned()
                .unwrap_or_else(|| col_idx.to_string());
            let text = literal_text_deep(cst, field_name);
            row.insert(col, Value::Str(text));
            // Encode (row_idx, col_idx) as a u32 key for the complement mapping.
            #[allow(clippy::cast_possible_truncation)]
            let cell_key = (row_idx as u32) * 10_000 + (col_idx as u32);
            // Map to the leaf that owns the text, not the wrapper field, so
            // injection can actually rewrite the value.
            cell_to_cst.insert(cell_key, deepest_literal_vertex(cst, field_name));
        }
        rows.push(row);
    }

    let instance = FInstance::new().with_table(table_vertex, rows);

    let complement = CstComplement {
        format: "tabular".into(),
        cst_schema: cst.clone(),
        node_to_cst_value: cell_to_cst,
        node_to_cst_struct: HashMap::new(),
    };

    Ok((instance, complement))
}

/// Inject an `FInstance` back into a tabular CST Schema.
///
/// Walks the `FInstance` table rows and updates the corresponding CST
/// field vertices' `literal-value` constraints using the complement's
/// cell-to-vertex mapping.
///
/// # Errors
///
/// Returns `CstExtractError` if the complement is invalid.
pub fn inject_tabular_cst(
    instance: &FInstance,
    complement: &CstComplement,
    _domain_schema: &Schema,
) -> Result<Schema, CstExtractError> {
    let mut cst = complement.cst_schema.clone();

    // Find the table (there should be exactly one in the FInstance).
    for rows in instance.tables.values() {
        // Reconstruct headers from the complement's CST.
        let doc_vertex = find_cst_root(&cst)?;
        let row_vertices = cst_children_by_edge_kind(&cst, &doc_vertex, "child_of");
        if row_vertices.is_empty() {
            continue;
        }
        let header_fields = cst_children_by_edge_kind(&cst, row_vertices[0], "child_of");
        // Match `extract_tabular_cst`: a header cell is usually a wrapper node
        // whose text lives in a `child_of` leaf, so read it the same deep way
        // the row keys were built from. Using the shallow `literal_value` here
        // yielded empty header names, so every cell lookup missed and edits
        // were silently dropped.
        let headers: Vec<String> = header_fields
            .iter()
            .map(|f| literal_text_deep(&cst, f))
            .collect();

        for (row_idx, row) in rows.iter().enumerate() {
            for (col_idx, col_name) in headers.iter().enumerate() {
                if let Some(value) = row.get(col_name) {
                    let text = match value {
                        Value::Str(s) => s.clone(),
                        Value::Int(i) => i.to_string(),
                        Value::Float(f) => f.to_string(),
                        Value::Bool(b) => b.to_string(),
                        Value::Null => String::new(),
                        other => format!("{other:?}"),
                    };
                    #[allow(clippy::cast_possible_truncation)]
                    let cell_key = (row_idx as u32) * 10_000 + (col_idx as u32);
                    if let Some(cst_vertex) = complement.node_to_cst_value.get(&cell_key) {
                        update_literal_value(&mut cst, cst_vertex, &text);
                    }
                }
            }
        }
    }

    Ok(cst)
}

// ── Format dispatch ───────────────────────────────────────────────────

/// The format kind determines which extraction logic to use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FormatKind {
    /// JSON format (tree-sitter-json grammar).
    Json,
    /// XML format (tree-sitter-xml grammar).
    Xml,
    /// YAML format (tree-sitter-yaml grammar).
    Yaml,
    /// TOML format (tree-sitter-toml grammar).
    Toml,
    /// CSV format (tree-sitter-csv grammar).
    Csv,
    /// TSV format (tree-sitter-tsv grammar).
    Tsv,
}

impl FormatKind {
    /// The tree-sitter grammar name for this format.
    #[must_use]
    pub const fn grammar_name(self) -> &'static str {
        match self {
            Self::Json => "json",
            Self::Xml => "xml",
            Self::Yaml => "yaml",
            Self::Toml => "toml",
            Self::Csv => "csv",
            Self::Tsv => "tsv",
        }
    }

    /// File extensions for this format.
    #[must_use]
    pub const fn extensions(self) -> &'static [&'static str] {
        match self {
            Self::Json => &["json"],
            Self::Xml => &["xml"],
            Self::Yaml => &["yaml", "yml"],
            Self::Toml => &["toml"],
            Self::Csv => &["csv"],
            Self::Tsv => &["tsv"],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_kind_grammar_names() {
        assert_eq!(FormatKind::Json.grammar_name(), "json");
        assert_eq!(FormatKind::Xml.grammar_name(), "xml");
        assert_eq!(FormatKind::Yaml.grammar_name(), "yaml");
        assert_eq!(FormatKind::Csv.grammar_name(), "csv");
    }
}
