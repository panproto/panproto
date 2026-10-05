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

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt::Write as _;

use panproto_gat::{Name, is_interstitial_text_sort};
use panproto_inst::metadata::Node;
use panproto_inst::value::coerce::{LexClass, Lexeme, coerce};
use panproto_inst::value::{FieldPresence, Value};
use panproto_inst::wtype::WInstance;
use panproto_inst::{FInstance, ValueFidelityError, ValueSite};
use panproto_schema::{BytesText, Constraint, Edge, Protocol, ScalarKind, Schema, ValueSpec};
use serde::{Deserialize, Serialize};

use crate::error::{EmitValueError, EmitValueReason};
use crate::xml_pathway::{
    is_valid_xml_namespace_declaration, is_xml_10_text, is_xml_namespace_declaration, is_xml_qname,
    xml_qname_is_bound,
};

const XML_SCHEMA_INSTANCE_NS: &str = "http://www.w3.org/2001/XMLSchema-instance";

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
    /// Maps `WInstance` node IDs to the ordered run of CST vertices whose
    /// literal text, concatenated, is the node's value. Used by injection to
    /// update the CST when the `WInstance` is modified.
    ///
    /// A scalar occupies one vertex. A string does not: tree-sitter splits
    /// `"x\ny"` into alternating `string_content` and `escape_sequence`
    /// children, and the value lives across all of them. Injection writes the
    /// re-encoded value into the first vertex of the run and empties the rest,
    /// which is well defined only because every vertex carries the source span
    /// it covers — an emptied vertex still consumes its original bytes, so the
    /// segments it used to share the value with do not replay behind the new
    /// text.
    pub node_to_cst_value: HashMap<u32, Vec<Name>>,
    /// Maps `WInstance` node IDs to the CST vertex names of the structural
    /// node (object, pair, array element) for structural reconstruction.
    /// Every key is a node id; no key is reserved.
    pub node_to_cst_struct: HashMap<u32, Name>,
    /// Maps a tabular cell's `(row, column)` position to the CST vertices
    /// carrying its text, keyed by [`tabular_cell_key`].
    ///
    /// A tabular instance has no `WInstance` node ids to key on — its cells
    /// are addressed by position — so this is a separate map rather than a
    /// packed encoding squeezed into `node_to_cst_value`'s `u32`.
    pub cell_to_cst_value: HashMap<u64, Vec<Name>>,
    /// The domain vertex (tree root or table) selected when this complement
    /// was extracted.
    ///
    /// Every extractor sets it. A complement decoded from a carrier without
    /// this field has `None`, and preserving injection refuses it, because
    /// re-extraction has no root to check the preserved source against.
    #[serde(default)]
    pub source_vertex: Option<Name>,
}

impl CstComplement {
    /// Return the domain vertex selected when this complement was extracted,
    /// or `None` when the complement carries no provenance.
    #[must_use]
    pub const fn source_vertex(&self) -> Option<&Name> {
        self.source_vertex.as_ref()
    }
}

/// The complement key for the cell at `row`, `column`.
///
/// The two indices occupy their own halves of a `u64`, so every pair of
/// `u32` coordinates gets its own key. Packing them into a `u32` as
/// `row * 10_000 + column` instead made row 1 column 0 collide with row 0
/// column ten thousand, and overflowed outright at around four hundred
/// thousand rows — both reachable in real tabular data, and both silently
/// mis-targeting an edit.
#[must_use]
pub const fn tabular_cell_key(row: u32, column: u32) -> u64 {
    ((row as u64) << 32) | (column as u64)
}

fn unsupported_structural_edit(format: &str, detail: impl Into<String>) -> CstCodecError {
    CstCodecError::UnsupportedStructuralEdit {
        format: format.to_owned(),
        detail: detail.into(),
    }
}

fn value_maps_match(left: &HashMap<String, Value>, right: &HashMap<String, Value>) -> bool {
    left.len() == right.len()
        && left.iter().all(|(key, value)| {
            right
                .get(key)
                .is_some_and(|other| value.canonical_bytes() == other.canonical_bytes())
        })
}

fn sorted_value_keys(values: &HashMap<String, Value>) -> Vec<&str> {
    let mut keys: Vec<_> = values.keys().map(String::as_str).collect();
    keys.sort_unstable();
    keys
}

/// Return the first part of a tree instance that a fixed source CST cannot
/// represent. Values are deliberately excluded: they are the supported edit
/// surface. XML attribute values are also editable, but their names are part
/// of the fixed start-tag structure.
fn tree_structural_difference(
    current: &WInstance,
    original: &WInstance,
    xml_attribute_values_editable: bool,
) -> Option<String> {
    if current.root != original.root {
        return Some(format!(
            "root changed from node {} to node {}",
            original.root, current.root
        ));
    }
    if current.schema_root != original.schema_root {
        return Some(format!(
            "schema root changed from '{}' to '{}'",
            original.schema_root, current.schema_root
        ));
    }

    let mut current_ids: Vec<_> = current.nodes.keys().copied().collect();
    let mut original_ids: Vec<_> = original.nodes.keys().copied().collect();
    current_ids.sort_unstable();
    original_ids.sort_unstable();
    if current_ids != original_ids {
        return Some("the set of node ids changed".to_owned());
    }
    if current.arcs != original.arcs {
        return Some("ordered parent/child arcs changed".to_owned());
    }
    if current.fans != original.fans {
        return Some("hyper-edge fans changed".to_owned());
    }
    if current.parent_map != original.parent_map || current.children_map != original.children_map {
        return Some("derived parent/child topology changed".to_owned());
    }

    for node_id in original_ids {
        let Some(current_node) = current.nodes.get(&node_id) else {
            return Some(format!("node {node_id} was removed"));
        };
        let Some(original_node) = original.nodes.get(&node_id) else {
            return Some(format!("node {node_id} was added"));
        };
        if current_node.id != original_node.id
            || current_node.anchor != original_node.anchor
            || current_node.discriminator != original_node.discriminator
            || current_node.position != original_node.position
            || current_node.shape != original_node.shape
        {
            return Some(format!("node {node_id} structural metadata changed"));
        }
        let xml_empty_element_gains_value = xml_attribute_values_editable
            && original_node.value.is_none()
            && current_node.value.is_some()
            && original.children(node_id).is_empty()
            && current.children(node_id).is_empty()
            && matches!(original_node.shape, NodeShape::XmlElement { .. });
        if current_node.value.is_some() != original_node.value.is_some()
            && !xml_empty_element_gains_value
        {
            return Some(format!("node {node_id} value slot was added or removed"));
        }
        if !value_maps_match(&current_node.annotations, &original_node.annotations) {
            return Some(format!("node {node_id} annotations changed"));
        }
        if xml_attribute_values_editable {
            if sorted_value_keys(&current_node.extra_fields)
                != sorted_value_keys(&original_node.extra_fields)
            {
                return Some(format!("node {node_id} XML attribute names changed"));
            }
        } else if !value_maps_match(&current_node.extra_fields, &original_node.extra_fields) {
            return Some(format!("node {node_id} unaddressed extra fields changed"));
        }
    }
    None
}

fn validate_tree_structure(
    instance: &WInstance,
    complement: &CstComplement,
    domain_schema: &Schema,
    protocol: &Protocol,
    format: &str,
) -> Result<(), CstCodecError> {
    if complement.format != format {
        return Err(unsupported_structural_edit(
            format,
            format!(
                "complement format is '{}' rather than '{format}'",
                complement.format
            ),
        ));
    }
    let source_vertex = complement
        .source_vertex()
        .map(AsRef::as_ref)
        .ok_or_else(|| {
            unsupported_structural_edit(format, "complement has no recorded source vertex")
        })?;
    let extracted = match format {
        "json" => extract_json_cst_with_protocol(
            &complement.cst_schema,
            domain_schema,
            protocol,
            source_vertex,
        ),
        "toml" => extract_toml_cst_with_protocol(
            &complement.cst_schema,
            domain_schema,
            protocol,
            source_vertex,
        ),
        "xml" => extract_xml_cst_with_protocol(
            &complement.cst_schema,
            domain_schema,
            protocol,
            source_vertex,
        ),
        "yaml" => extract_yaml_cst_with_protocol(
            &complement.cst_schema,
            domain_schema,
            protocol,
            source_vertex,
        ),
        _ => {
            return Err(unsupported_structural_edit(
                format,
                "unknown tree complement format",
            ));
        }
    }
    .map_err(|error| {
        unsupported_structural_edit(
            format,
            format!("preserved source no longer re-extracts: {error}"),
        )
    })?;
    let (original, rebuilt_complement) = extracted;
    if complement.node_to_cst_value != rebuilt_complement.node_to_cst_value
        || complement.node_to_cst_struct != rebuilt_complement.node_to_cst_struct
        || !complement.cell_to_cst_value.is_empty()
    {
        return Err(unsupported_structural_edit(
            format,
            "complement carrier maps do not match its preserved source",
        ));
    }
    if let Some(detail) = tree_structural_difference(instance, &original, format == "xml") {
        return Err(unsupported_structural_edit(format, detail));
    }
    if format == "xml"
        && original.nodes.iter().any(|(node_id, node)| {
            if node.value.is_some()
                || instance
                    .nodes
                    .get(node_id)
                    .is_none_or(|current| current.value.is_none())
            {
                return false;
            }
            complement
                .node_to_cst_struct
                .get(node_id)
                .is_none_or(|carrier| {
                    cst_vertex_kind(&complement.cst_schema, carrier).as_deref() != Some("element")
                })
        })
    {
        return Err(unsupported_structural_edit(
            format,
            "an XML node gained a value without an element carrier",
        ));
    }
    Ok(())
}

/// Errors returned by the source-compatible CST extraction API.
///
/// Every [`CstCodecError`] has a variant of the same shape here, so a value
/// or fidelity refusal reaches callers of the schema-only functions as the
/// same structured value the protocol-aware functions return.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
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

    /// The CST nests deeper than extraction is willing to descend.
    #[error("CST nesting exceeds the {limit}-level extraction limit")]
    NestingTooDeep {
        /// The depth limit that was exceeded.
        limit: usize,
    },

    /// One or more scalar lexemes cannot be held by their declared kinds.
    #[error("value fidelity errors: {0:?}")]
    Fidelity(Vec<ValueFidelityError>),

    /// A YAML mapping or sequence carried a tag with no instance-level
    /// representation.
    #[error("unsupported YAML tag {tag:?} on {container_kind} at byte {byte_offset:?}")]
    UnsupportedTaggedContainer {
        /// The tag's source spelling.
        tag: String,
        /// `mapping` or `sequence`.
        container_kind: String,
        /// Zero-based byte offset of the tag.
        byte_offset: Option<u64>,
    },

    /// A value has no faithful representation in the output format.
    #[error(transparent)]
    Value(#[from] EmitValueError),

    /// The preserved CST has no faithful representation for a structural
    /// edit made to the extracted instance.
    #[error("{format} format-preserving emission cannot represent structural edit: {detail}")]
    UnsupportedStructuralEdit {
        /// The format whose CST cannot carry the edit.
        format: String,
        /// The first unsupported structural difference found.
        detail: String,
    },
}

/// Errors from protocol-aware CST extraction and injection.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum CstCodecError {
    /// The CST Schema has an unexpected structure.
    #[error("CST structure error: {0}")]
    Structure(String),

    /// A required CST vertex was not found.
    #[error("CST vertex not found: {0}")]
    VertexNotFound(String),

    /// Domain schema mismatch.
    #[error("domain schema mismatch: {0}")]
    SchemaMismatch(String),

    /// The CST nests deeper than extraction is willing to descend.
    ///
    /// Extraction walks the CST recursively, so unbounded nesting would
    /// exhaust the thread's stack and abort the process rather than fail.
    #[error("CST nesting exceeds the {limit}-level extraction limit")]
    NestingTooDeep {
        /// The depth limit that was exceeded.
        limit: usize,
    },

    /// One or more scalar lexemes cannot be held by their declared kinds.
    #[error("value fidelity errors: {0:?}")]
    Fidelity(Vec<ValueFidelityError>),

    /// A YAML mapping or sequence carried a tag with no instance-level
    /// representation.
    #[error("unsupported YAML tag {tag:?} on {container_kind} at byte {byte_offset:?}")]
    UnsupportedTaggedContainer {
        /// The tag's source spelling.
        tag: String,
        /// `mapping` or `sequence`.
        container_kind: String,
        /// Zero-based byte offset of the tag.
        byte_offset: Option<u64>,
    },

    /// A value has no faithful representation in the output format.
    #[error(transparent)]
    Value(#[from] EmitValueError),

    /// The preserved CST has no faithful representation for a structural
    /// edit made to the extracted instance.
    #[error("{format} format-preserving emission cannot represent structural edit: {detail}")]
    UnsupportedStructuralEdit {
        /// The format whose CST cannot carry the edit.
        format: String,
        /// The first unsupported structural difference found.
        detail: String,
    },
}

impl From<CstCodecError> for CstExtractError {
    fn from(error: CstCodecError) -> Self {
        match error {
            CstCodecError::Structure(message) => Self::Structure(message),
            CstCodecError::VertexNotFound(vertex) => Self::VertexNotFound(vertex),
            CstCodecError::SchemaMismatch(message) => Self::SchemaMismatch(message),
            CstCodecError::NestingTooDeep { limit } => Self::NestingTooDeep { limit },
            CstCodecError::Fidelity(errors) => Self::Fidelity(errors),
            CstCodecError::UnsupportedTaggedContainer {
                tag,
                container_kind,
                byte_offset,
            } => Self::UnsupportedTaggedContainer {
                tag,
                container_kind,
                byte_offset,
            },
            CstCodecError::Value(error) => Self::Value(error),
            CstCodecError::UnsupportedStructuralEdit { format, detail } => {
                Self::UnsupportedStructuralEdit { format, detail }
            }
        }
    }
}

/// How deeply CST extraction descends before it refuses to go further.
///
/// The extractors recurse over the CST, and a CST arrives from a parser fed
/// by whatever bytes a caller had. Descending without a bound turns deeply
/// nested input into a stack overflow, which aborts the process with a signal
/// no caller can catch; the bound turns it into
/// [`CstCodecError::NestingTooDeep`] instead. It sits inside the smallest
/// stack the extractors run on — an unoptimised build on a two-megabyte test
/// thread, where they reach a little over two hundred levels — and far above
/// what real documents nest to. It matches the parser's own nesting bound, so
/// a document the parser accepts is one extraction can finish. A regression
/// test extracts at exactly this depth, so a change that grows an extractor's
/// frame is caught rather than discovered in the field.
pub const MAX_EXTRACT_DEPTH: usize = 128;

/// Accumulated state during CST extraction.
struct ExtractState {
    nodes: HashMap<u32, Node>,
    arcs: Vec<(u32, u32, Edge)>,
    next_id: u32,
    node_to_cst_value: HashMap<u32, Vec<Name>>,
    node_to_cst_struct: HashMap<u32, Name>,
    /// How many recursive descents are currently on the stack.
    depth: usize,
    /// Scalar fidelity failures in source order.
    fidelity: Vec<ValueFidelityError>,
}

impl ExtractState {
    fn new() -> Self {
        Self {
            nodes: HashMap::new(),
            arcs: Vec::new(),
            next_id: 0,
            node_to_cst_value: HashMap::new(),
            node_to_cst_struct: HashMap::new(),
            depth: 0,
            fidelity: Vec::new(),
        }
    }

    const fn alloc_id(&mut self) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    /// Claim one more level of descent, refusing past [`MAX_EXTRACT_DEPTH`].
    ///
    /// Every cycle in the extractors' call graph passes through a function
    /// that opens with this and closes with [`Self::leave`], so bounding
    /// those bounds the whole descent. The pair is written out at each site
    /// rather than wrapped around a closure because a wrapper is itself a
    /// stack frame, and the frames are what the bound exists to ration.
    const fn enter(&mut self) -> Result<(), CstCodecError> {
        if self.depth >= MAX_EXTRACT_DEPTH {
            return Err(CstCodecError::NestingTooDeep {
                limit: MAX_EXTRACT_DEPTH,
            });
        }
        self.depth += 1;
        Ok(())
    }

    /// Release the level claimed by [`Self::enter`].
    ///
    /// Called on the way out of a descent that finished. A descent that
    /// failed aborts the whole extraction, so its level is never released
    /// and never needed again.
    const fn leave(&mut self) {
        self.depth -= 1;
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

/// Classify a CST `content`-child node kind for mixed-content handling.
///
/// The tree-sitter XML grammar splits an element's `content` into three
/// observable classes:
///
/// - **Text**: `CharData` plus character-/entity-reference nodes
///   (`EntityRef`, `CharRef`, `PEReference`, `_Reference`). These all belong
///   to the same logical character stream: `the state&apos;s` parses as
///   `CharData("the state") EntityRef("&apos;") CharData("s")`, which is one
///   run of text, not three siblings. Treating references as text is what
///   makes the canonical (non-complement) `emit` idempotent: re-emitting a
///   value containing `'`/`"`/`&` re-escapes it to an entity, so the re-parse
///   re-splits the run, and only a text-merging extractor maps both shapes to
///   the same node count.
/// - **Opaque**: `Comment` and the processing-instruction kinds (`PI`,
///   `StyleSheetPI`, `XmlModelPI`). CDATA is character data, so it belongs to
///   the text stream even though its source delimiters remain in the layout
///   complement.
/// - **Element**: nested `element`s.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ContentClass {
    Text,
    Element,
    Opaque,
}

fn classify_content_kind(kind: &str) -> ContentClass {
    match kind {
        "element" => ContentClass::Element,
        "CharData" | "CDSect" | "EntityRef" | "CharRef" | "PEReference" | "_Reference" => {
            ContentClass::Text
        }
        // Comment/PI/StyleSheetPI/XmlModelPI and any other
        // non-text, non-element node is opaque structural content.
        _ => ContentClass::Opaque,
    }
}

/// The ordered run of CST vertices whose text makes up a JSON string's
/// body, in source order.
///
/// Tree-sitter splits an escaped string into alternating `string_content`
/// and `escape_sequence` children, so `"x\ny"` is three vertices, not one.
/// The value lives across the whole run, which is what both extraction and
/// injection have to address.
fn json_string_segments(cst: &Schema, string_vertex: &str) -> Vec<Name> {
    let mut segments: Vec<(usize, Name)> = cst
        .outgoing_edges(string_vertex)
        .iter()
        .filter(|e| {
            matches!(
                cst_vertex_kind(cst, &e.tgt).as_deref(),
                Some("string_content" | "escape_sequence")
            )
        })
        .map(|e| {
            let start = cst
                .constraints
                .get(&e.tgt)
                .and_then(|cs| {
                    cs.iter()
                        .find(|c| c.sort.as_ref() == "start-byte")
                        .and_then(|c| c.value.parse::<usize>().ok())
                })
                .unwrap_or(0);
            (start, e.tgt.clone())
        })
        .collect();
    segments.sort_by_key(|&(start, _)| start);
    segments.into_iter().map(|(_, name)| name).collect()
}

/// Get the decoded value of a CST `string` vertex.
///
/// The segments' raw texts are concatenated first and decoded as one body.
/// Decoding segment by segment cannot see a surrogate pair, which is the
/// only way JSON can spell an astral character: the grammar splits
/// `😀` into four separate children, and neither half of the pair
/// is a character on its own.
fn json_string_value(cst: &Schema, string_vertex: &str) -> Option<String> {
    let segments = json_string_segments(cst, string_vertex);
    if segments.is_empty() {
        return (cst_vertex_kind(cst, string_vertex).as_deref() == Some("string"))
            .then(String::new);
    }
    let mut raw = String::new();
    for segment in &segments {
        if let Some(text) = literal_value(cst, segment) {
            raw.push_str(&text);
        }
    }
    Some(decode_json_string_body(&raw))
}

/// Decode the body of a JSON string (the bytes between the quotes) into the
/// text it denotes.
///
/// `\uXXXX` escapes are decoded with surrogate pairing: a high surrogate
/// followed by a low surrogate combines into the astral character the pair
/// encodes. A surrogate with no partner is not a character, so its escape is
/// kept verbatim rather than dropped — the byte-faithful emit path replays
/// the original text for an unedited value, and a caller inspecting the
/// value sees what the source actually said.
fn decode_json_string_body(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut chars = raw.char_indices().peekable();
    while let Some((_, c)) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        let Some((_, esc)) = chars.next() else {
            // A trailing backslash is not an escape; keep it.
            out.push('\\');
            continue;
        };
        match esc {
            '"' => out.push('"'),
            '\\' => out.push('\\'),
            '/' => out.push('/'),
            'b' => out.push('\u{0008}'),
            'f' => out.push('\u{000C}'),
            'n' => out.push('\n'),
            'r' => out.push('\r'),
            't' => out.push('\t'),
            'u' => decode_json_unicode_escape(raw, &mut chars, &mut out),
            other => {
                // Not a JSON escape; keep both characters as written.
                out.push('\\');
                out.push(other);
            }
        }
    }
    out
}

/// Decode one `\uXXXX` escape whose `\u` the caller has consumed, appending
/// the character it denotes to `out`.
fn decode_json_unicode_escape(
    raw: &str,
    chars: &mut std::iter::Peekable<std::str::CharIndices<'_>>,
    out: &mut String,
) {
    use std::fmt::Write as _;

    let Some(unit) = take_hex4(raw, chars) else {
        // Fewer than four hex digits follow: not an escape after all.
        out.push_str("\\u");
        return;
    };
    if let Some(c) = char::from_u32(unit) {
        out.push(c);
        return;
    }
    // A surrogate. Pair it with the next escape when that one is the
    // matching low half.
    if (0xD800..=0xDBFF).contains(&unit) {
        if let Some(low) = take_low_surrogate(raw, chars) {
            let combined = 0x1_0000 + ((unit - 0xD800) << 10) + (low - 0xDC00);
            if let Some(c) = char::from_u32(combined) {
                out.push(c);
                return;
            }
        }
    }
    let _ = write!(out, "\\u{unit:04x}");
}

/// Consume the four hex digits of a `\uXXXX` escape whose `\u` is already
/// consumed. Returns `None` — having consumed nothing — when fewer than four
/// hex digits follow.
fn take_hex4(raw: &str, chars: &mut std::iter::Peekable<std::str::CharIndices<'_>>) -> Option<u32> {
    let &(start, _) = chars.peek()?;
    let digits = raw.get(start..start + 4)?;
    if !digits.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    for _ in 0..4 {
        chars.next();
    }
    u32::from_str_radix(digits, 16).ok()
}

/// Consume a following `\uXXXX` escape when it encodes a low surrogate,
/// returning its code unit. Leaves the iterator untouched otherwise, so a
/// high surrogate followed by anything else stays unpaired.
fn take_low_surrogate(
    raw: &str,
    chars: &mut std::iter::Peekable<std::str::CharIndices<'_>>,
) -> Option<u32> {
    let &(start, _) = chars.peek()?;
    let hex = raw.get(start..start + 6)?.strip_prefix("\\u")?;
    let unit = u32::from_str_radix(hex, 16).ok()?;
    if !(0xDC00..=0xDFFF).contains(&unit) {
        return None;
    }
    for _ in 0..6 {
        chars.next();
    }
    Some(unit)
}

/// Parse one number token without a narrowing intermediate carrier.
fn parse_number_value(text: &str) -> Result<Value, ValueFidelityError> {
    crate::json_pathway::coerce_exact_lexeme(
        &Lexeme::new(text, LexClass::Number),
        &ValueSpec::new(ScalarKind::Any),
    )
}

fn cst_byte_offset(cst: &Schema, vertex: &Name) -> Option<u64> {
    cst.constraints.get(vertex).and_then(|constraints| {
        constraints
            .iter()
            .find(|constraint| constraint.sort.as_ref() == "start-byte")
            .and_then(|constraint| constraint.value.parse().ok())
    })
}

fn coerce_cst_scalar(
    cst: &Schema,
    cst_vertex: &Name,
    domain_schema: &Schema,
    protocol: &Protocol,
    domain_vertex: &str,
    lexeme: &Lexeme<'_>,
    state: &mut ExtractState,
) -> FieldPresence {
    let spec = ValueSpec::for_vertex(domain_schema, protocol, &Name::from(domain_vertex));
    match crate::json_pathway::coerce_exact_lexeme(lexeme, &spec) {
        Ok(Value::Null) => FieldPresence::Null,
        Ok(value) => FieldPresence::Present(value),
        Err(error) => {
            state.fidelity.push(error.at(ValueSite::new(
                domain_vertex,
                cst_byte_offset(cst, cst_vertex),
            )));
            FieldPresence::Present(Value::Null)
        }
    }
}

fn coerce_yaml_cst_scalar(
    cst: &Schema,
    cst_vertex: &Name,
    domain_schema: &Schema,
    protocol: &Protocol,
    domain_vertex: &str,
    lexeme: &Lexeme<'_>,
    state: &mut ExtractState,
) -> FieldPresence {
    let spec = ValueSpec::for_vertex(domain_schema, protocol, &Name::from(domain_vertex));
    match coerce_yaml_lexeme(lexeme.text, lexeme.class.clone(), &spec) {
        Ok(Value::Null) => FieldPresence::Null,
        Ok(value) => FieldPresence::Present(value),
        Err(error) => {
            state.fidelity.push(error.at(ValueSite::new(
                domain_vertex,
                cst_byte_offset(cst, cst_vertex),
            )));
            FieldPresence::Present(Value::Null)
        }
    }
}

fn coerce_yaml_lexeme(
    text: &str,
    class: LexClass,
    spec: &ValueSpec,
) -> Result<Value, ValueFidelityError> {
    let compact = text.trim_start_matches(['+', '-']).replace('_', "");
    let class = if matches!(class, LexClass::Number) && compact.starts_with("0b") {
        LexClass::String
    } else if matches!(class, LexClass::String) && is_yaml_nonfinite_scalar(text) {
        LexClass::Number
    } else {
        class
    };
    let LexClass::Tagged(tag) = &class else {
        return crate::json_pathway::coerce_exact_lexeme(&Lexeme::new(text, class), spec);
    };
    let tag_name = yaml_tag_name(tag);
    match (&spec.kind, tag_name) {
        (ScalarKind::Bytes(_) | ScalarKind::Any, "binary" | "base64") => {
            // YAML permits base64 in folded or literal blocks, where line
            // wrapping is presentation whitespace rather than encoded data.
            let compact: String = text
                .chars()
                .filter(|character| !character.is_whitespace())
                .collect();
            coerce(
                &Lexeme::new(&compact, LexClass::Tagged(tag.clone())),
                &ValueSpec::new(ScalarKind::Any),
            )
        }
        (ScalarKind::Date, "date")
        | (ScalarKind::Time, "time")
        | (ScalarKind::DateTime, "timestamp" | "datetime" | "date-time") => {
            coerce(&Lexeme::new(text, LexClass::DateTime), spec)
        }
        _ => crate::json_pathway::coerce_exact_lexeme(&Lexeme::new(text, class), spec),
    }
}

fn is_yaml_nonfinite_scalar(text: &str) -> bool {
    let unsigned = text.trim_start_matches(['-', '+']);
    unsigned
        .strip_prefix('.')
        .is_some_and(|value| value.eq_ignore_ascii_case("nan") || value.eq_ignore_ascii_case("inf"))
}

fn yaml_tag_name(tag: &Name) -> &str {
    tag.as_ref()
        .trim_start_matches('!')
        .strip_prefix("tag:yaml.org,2002:")
        .unwrap_or_else(|| tag.as_ref().trim_start_matches('!'))
}

fn finish_fidelity(state: &mut ExtractState) -> Result<(), CstCodecError> {
    if state.fidelity.is_empty() {
        return Ok(());
    }
    state
        .fidelity
        .sort_by_key(|error| error.site.byte_offset.unwrap_or(u64::MAX));
    Err(CstCodecError::Fidelity(std::mem::take(&mut state.fidelity)))
}

/// Find the root vertex in the CST (the `document` node or first vertex
/// with no incoming edges).
fn find_cst_root(cst: &Schema) -> Result<String, CstCodecError> {
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
    Err(CstCodecError::Structure(
        "no root vertex found in CST".into(),
    ))
}

fn validate_cst_has_no_recovery_markers(cst: &Schema, format: &str) -> Result<(), CstCodecError> {
    let count = cst
        .vertices
        .iter()
        .filter(|(name, vertex)| {
            let constraints = cst.constraints.get(*name);
            let explicitly_missing = constraints.is_some_and(|constraints| {
                constraints
                    .iter()
                    .any(|constraint| constraint.sort.as_ref() == "missing")
            });
            let span = constraints.and_then(|constraints| {
                let byte = |sort: &str| {
                    constraints
                        .iter()
                        .find(|constraint| constraint.sort.as_ref() == sort)?
                        .value
                        .parse::<usize>()
                        .ok()
                };
                Some((byte("start-byte")?, byte("end-byte")?))
            });
            let zero_width_named_recovery = span.is_some_and(|(start, end)| start == end)
                && !cst.incoming_edges(name).is_empty()
                && !matches!(
                    (format, vertex.kind.as_ref()),
                    ("YAML", "stream" | "document") | ("tabular", "field" | "text")
                );
            *vertex.kind == *"ERROR" || explicitly_missing || zero_width_named_recovery
        })
        .count();
    if count == 0 {
        Ok(())
    } else {
        Err(CstCodecError::Structure(format!(
            "{format} CST contains {count} parser recovery marker(s)"
        )))
    }
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
    extract_json_cst_with_protocol(cst, domain_schema, &Protocol::default(), root_vertex)
        .map_err(Into::into)
}

/// Extract a JSON CST using the selected protocol's value constraints.
///
/// # Errors
///
/// Returns `CstCodecError` if the CST structure is invalid, a value violates
/// the protocol constraints, or the CST does not match the domain schema.
pub fn extract_json_cst_with_protocol(
    cst: &Schema,
    domain_schema: &Schema,
    protocol: &Protocol,
    root_vertex: &str,
) -> Result<(WInstance, CstComplement), CstCodecError> {
    validate_cst_has_no_recovery_markers(cst, "JSON")?;
    validate_json_cst_string_tokens(cst)?;
    if !domain_schema.has_vertex(root_vertex) {
        return Err(CstCodecError::SchemaMismatch(format!(
            "root vertex '{root_vertex}' not found in domain schema"
        )));
    }

    let mut state = ExtractState::new();
    let doc_vertex = find_cst_root(cst)?;

    let top_value = cst_child_by_edge_kind(cst, &doc_vertex, "child_of")
        .ok_or_else(|| CstCodecError::Structure("document has no child".into()))?;

    let root_id = state.alloc_id();
    extract_json_value(
        cst,
        domain_schema,
        protocol,
        top_value,
        root_vertex,
        root_id,
        &mut state,
    )?;

    finish_fidelity(&mut state)?;
    let complement = CstComplement {
        format: "json".into(),
        cst_schema: cst.clone(),
        node_to_cst_value: state.node_to_cst_value,
        node_to_cst_struct: state.node_to_cst_struct,
        cell_to_cst_value: HashMap::new(),
        source_vertex: Some(Name::from(root_vertex)),
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

fn find_unique_yaml_document(cst: &Schema) -> Result<String, CstCodecError> {
    let documents: Vec<_> = cst
        .vertices
        .iter()
        .filter(|(_, vertex)| *vertex.kind == *"document")
        .map(|(name, _)| name.to_string())
        .collect();
    match documents.as_slice() {
        [document] => Ok(document.clone()),
        [] => Err(CstCodecError::Structure(
            "YAML stream has no document".into(),
        )),
        _ => Err(CstCodecError::Structure(format!(
            "YAML stream has {} documents; exactly one is required",
            documents.len()
        ))),
    }
}

/// Recursively extract a JSON value node from the CST into a `WInstance`
/// node, one level deeper than the caller and no deeper than
/// [`MAX_EXTRACT_DEPTH`].
fn extract_json_value(
    cst: &Schema,
    domain_schema: &Schema,
    protocol: &Protocol,
    cst_vertex: &Name,
    domain_vertex: &str,
    node_id: u32,
    state: &mut ExtractState,
) -> Result<(), CstCodecError> {
    state.enter()?;
    let kind = cst_vertex_kind(cst, cst_vertex)
        .ok_or_else(|| CstCodecError::VertexNotFound(cst_vertex.to_string()))?;

    state.node_to_cst_struct.insert(node_id, cst_vertex.clone());

    let extracted = match kind.as_str() {
        "object" => extract_json_object(
            cst,
            domain_schema,
            protocol,
            cst_vertex,
            domain_vertex,
            node_id,
            state,
        ),
        "array" => extract_json_array(
            cst,
            domain_schema,
            protocol,
            cst_vertex,
            domain_vertex,
            node_id,
            state,
        ),
        "string" | "number" | "true" | "false" => {
            let (text, class) = match kind.as_str() {
                "string" => (
                    json_string_value(cst, cst_vertex).unwrap_or_default(),
                    LexClass::String,
                ),
                "number" => (
                    literal_value(cst, cst_vertex).unwrap_or_default(),
                    LexClass::Number,
                ),
                boolean => (boolean.to_owned(), LexClass::Bool),
            };
            state
                .node_to_cst_value
                .insert(node_id, vec![cst_vertex.clone()]);
            let presence = coerce_cst_scalar(
                cst,
                cst_vertex,
                domain_schema,
                protocol,
                domain_vertex,
                &Lexeme::new(&text, class),
                state,
            );
            let node = Node::new(node_id, domain_vertex).with_value(presence);
            state.nodes.insert(node_id, node);
            Ok(())
        }
        "null" => {
            state
                .node_to_cst_value
                .insert(node_id, vec![cst_vertex.clone()]);
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
    };
    state.leave();
    extracted
}

/// Extract a JSON object from CST, matching keys to domain schema edges.
///
/// When the domain schema has no outgoing edges (open schema), all pairs
/// become child nodes with synthesized edges, matching legacy codec behavior.
fn extract_json_object(
    cst: &Schema,
    domain_schema: &Schema,
    protocol: &Protocol,
    cst_vertex: &Name,
    domain_vertex: &str,
    node_id: u32,
    state: &mut ExtractState,
) -> Result<(), CstCodecError> {
    ensure_unique_json_object_keys(cst, cst_vertex)?;
    let mut node = Node::new(node_id, domain_vertex);

    // Check for $type discriminator
    if let Some(disc_value) = find_json_pair_value(cst, cst_vertex, "$type") {
        if cst_vertex_kind(cst, &disc_value).as_deref() == Some("string") {
            if let Some(text) = json_string_value(cst, &disc_value) {
                node.discriminator = Some(Name::from(text.as_str()));
            }
        }
    }
    let has_string_discriminator = node.discriminator.is_some();

    let domain_edges: Vec<Edge> = domain_schema.outgoing_edges(domain_vertex).to_vec();

    if domain_edges.is_empty() {
        // Open schema: extract ALL pairs as child nodes with synthesized edges.
        let pairs = cst_children_by_edge_kind(cst, cst_vertex, "child_of");
        for pair_name in pairs {
            if cst_vertex_kind(cst, pair_name).as_deref() != Some("pair") {
                continue;
            }
            if let Some(key) = extract_pair_key(cst, pair_name) {
                if key == "$type" && has_string_discriminator {
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
                    protocol,
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
                if (key == "$type" && has_string_discriminator) || handled_keys.contains(&key) {
                    continue;
                }
                if let Some(value_vertex) = cst_child_by_edge_kind(cst, pair_name, "value") {
                    let val = extract_json_generic_value(cst, value_vertex, domain_vertex, state)?;
                    node.extra_fields.insert(key, val);
                }
            }
        }
    }

    state.nodes.insert(node_id, node);
    Ok(())
}

/// Extract a JSON object without schema guidance, synthesising one `prop`
/// edge per pair.
fn extract_json_object_open(
    cst: &Schema,
    cst_vertex: &Name,
    anchor: &str,
    node_id: u32,
    state: &mut ExtractState,
) -> Result<(), CstCodecError> {
    ensure_unique_json_object_keys(cst, cst_vertex)?;
    let mut node = Node::new(node_id, anchor);
    if let Some(discriminator) = find_json_pair_value(cst, cst_vertex, "$type")
        && cst_vertex_kind(cst, &discriminator).as_deref() == Some("string")
        && let Some(text) = json_string_value(cst, &discriminator)
    {
        node.discriminator = Some(Name::from(text.as_str()));
    }
    let has_string_discriminator = node.discriminator.is_some();
    let pairs = cst_children_by_edge_kind(cst, cst_vertex, "child_of");
    for pair_name in pairs {
        if cst_vertex_kind(cst, pair_name).as_deref() != Some("pair") {
            continue;
        }
        let Some(key) = extract_pair_key(cst, pair_name) else {
            continue;
        };
        if key == "$type" && has_string_discriminator {
            continue;
        }
        let Some(value_vertex) = cst_child_by_edge_kind(cst, pair_name, "value") else {
            continue;
        };
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
    state.nodes.insert(node_id, node);
    Ok(())
}

/// Extract a JSON array without schema guidance, synthesising one `item`
/// edge per element.
fn extract_json_array_open(
    cst: &Schema,
    cst_vertex: &Name,
    anchor: &str,
    node_id: u32,
    state: &mut ExtractState,
) -> Result<(), CstCodecError> {
    let mut node = Node::new(node_id, anchor);
    node.shape = NodeShape::List;
    state.nodes.insert(node_id, node);
    for child_name in &cst_children_by_edge_kind(cst, cst_vertex, "child_of") {
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

/// Extract a JSON value without schema guidance (for open schemas), one
/// level deeper than the caller and no deeper than [`MAX_EXTRACT_DEPTH`].
fn extract_json_value_open(
    cst: &Schema,
    cst_vertex: &Name,
    anchor: &str,
    node_id: u32,
    state: &mut ExtractState,
) -> Result<(), CstCodecError> {
    state.enter()?;
    let kind = cst_vertex_kind(cst, cst_vertex).unwrap_or_default();
    state.node_to_cst_struct.insert(node_id, cst_vertex.clone());

    let extracted = match kind.as_str() {
        "object" => extract_json_object_open(cst, cst_vertex, anchor, node_id, state),
        "array" => extract_json_array_open(cst, cst_vertex, anchor, node_id, state),
        "string" => {
            let text = json_string_value(cst, cst_vertex).unwrap_or_default();
            state
                .node_to_cst_value
                .insert(node_id, vec![cst_vertex.clone()]);
            let node =
                Node::new(node_id, anchor).with_value(FieldPresence::Present(Value::Str(text)));
            state.nodes.insert(node_id, node);
            Ok(())
        }
        "number" => {
            state
                .node_to_cst_value
                .insert(node_id, vec![cst_vertex.clone()]);
            let text = literal_value(cst, cst_vertex).unwrap_or_default();
            let value = match parse_number_value(&text) {
                Ok(value) => value,
                Err(error) => {
                    state
                        .fidelity
                        .push(error.at(ValueSite::new(anchor, cst_byte_offset(cst, cst_vertex))));
                    Value::Null
                }
            };
            let node = Node::new(node_id, anchor).with_value(FieldPresence::Present(value));
            state.nodes.insert(node_id, node);
            Ok(())
        }
        "true" => {
            state
                .node_to_cst_value
                .insert(node_id, vec![cst_vertex.clone()]);
            let node =
                Node::new(node_id, anchor).with_value(FieldPresence::Present(Value::Bool(true)));
            state.nodes.insert(node_id, node);
            Ok(())
        }
        "false" => {
            state
                .node_to_cst_value
                .insert(node_id, vec![cst_vertex.clone()]);
            let node =
                Node::new(node_id, anchor).with_value(FieldPresence::Present(Value::Bool(false)));
            state.nodes.insert(node_id, node);
            Ok(())
        }
        "null" => {
            state
                .node_to_cst_value
                .insert(node_id, vec![cst_vertex.clone()]);
            let node = Node::new(node_id, anchor).with_value(FieldPresence::Null);
            state.nodes.insert(node_id, node);
            Ok(())
        }
        _ => {
            state
                .node_to_cst_value
                .insert(node_id, vec![cst_vertex.clone()]);
            let text = literal_value(cst, cst_vertex).unwrap_or_default();
            let node =
                Node::new(node_id, anchor).with_value(FieldPresence::Present(Value::Str(text)));
            state.nodes.insert(node_id, node);
            Ok(())
        }
    };
    state.leave();
    extracted
}

/// Extract a JSON array from CST.
fn extract_json_array(
    cst: &Schema,
    domain_schema: &Schema,
    protocol: &Protocol,
    cst_vertex: &Name,
    domain_vertex: &str,
    node_id: u32,
    state: &mut ExtractState,
) -> Result<(), CstCodecError> {
    let mut node = Node::new(node_id, domain_vertex);
    node.shape = NodeShape::List;
    state.nodes.insert(node_id, node);

    let domain_edges: Vec<Edge> = domain_schema.outgoing_edges(domain_vertex).to_vec();
    let item_edge = domain_edges.iter().find(|edge| edge.name.is_none());

    let children = cst_children_by_edge_kind(cst, cst_vertex, "child_of");
    if let Some(edge) = item_edge {
        // Domain schema declares the item slot: walk children with the
        // declared item-target kind so type-aware routing kicks in.
        for child_name in children {
            let child_id = state.alloc_id();
            extract_json_value(
                cst,
                domain_schema,
                protocol,
                child_name,
                &edge.tgt,
                child_id,
                state,
            )?;
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

fn ensure_unique_json_object_keys(cst: &Schema, object_vertex: &Name) -> Result<(), CstCodecError> {
    let mut seen = HashSet::new();
    for pair_name in cst_children_by_edge_kind(cst, object_vertex, "child_of") {
        if cst_vertex_kind(cst, pair_name).as_deref() != Some("pair") {
            continue;
        }
        let key = extract_pair_key(cst, pair_name).ok_or_else(|| {
            CstCodecError::Structure(format!("JSON object member at {pair_name} has no key"))
        })?;
        if !seen.insert(key) {
            let offset = cst_byte_offset(cst, pair_name)
                .map_or_else(|| "unknown".to_owned(), |value| value.to_string());
            return Err(CstCodecError::Structure(format!(
                "duplicate JSON object key at byte {offset}"
            )));
        }
    }
    Ok(())
}

/// Extract the key string from a JSON pair vertex.
fn extract_pair_key(cst: &Schema, pair_vertex: &Name) -> Option<String> {
    let key_vertex = cst_child_by_edge_kind(cst, pair_vertex, "key")?;
    json_string_value(cst, key_vertex)
}

/// Extract a generic JSON value from CST as a panproto `Value` (for `extra_fields`).
fn extract_json_generic_value(
    cst: &Schema,
    cst_vertex: &Name,
    domain_vertex: &str,
    state: &mut ExtractState,
) -> Result<Value, CstCodecError> {
    state.enter()?;
    let Some(kind) = cst_vertex_kind(cst, cst_vertex) else {
        state.leave();
        return Ok(Value::Null);
    };

    let value = match kind.as_str() {
        "string" => Value::Str(json_string_value(cst, cst_vertex).unwrap_or_default()),
        "number" => {
            let text = literal_value(cst, cst_vertex).unwrap_or_default();
            match parse_number_value(&text) {
                Ok(value) => value,
                Err(error) => {
                    state.fidelity.push(error.at(ValueSite::new(
                        domain_vertex,
                        cst_byte_offset(cst, cst_vertex),
                    )));
                    Value::Null
                }
            }
        }
        "true" => Value::Bool(true),
        "false" => Value::Bool(false),
        "null" => Value::Null,
        "object" => {
            ensure_unique_json_object_keys(cst, cst_vertex)?;
            let mut fields = HashMap::new();
            let pairs = cst_children_by_edge_kind(cst, cst_vertex, "child_of");
            for pair_name in pairs {
                if cst_vertex_kind(cst, pair_name).as_deref() != Some("pair") {
                    continue;
                }
                if let Some(key) = extract_pair_key(cst, pair_name) {
                    if let Some(val_vertex) = cst_child_by_edge_kind(cst, pair_name, "value") {
                        fields.insert(
                            key,
                            extract_json_generic_value(cst, val_vertex, domain_vertex, state)?,
                        );
                    }
                }
            }
            Value::Unknown(fields)
        }
        "array" => {
            let children = cst_children_by_edge_kind(cst, cst_vertex, "child_of");
            let items = children
                .iter()
                .map(|child_name| extract_json_generic_value(cst, child_name, domain_vertex, state))
                .collect::<Result<Vec<_>, _>>()?;
            Value::List(items)
        }
        _ => Value::Str(literal_value(cst, cst_vertex).unwrap_or_default()),
    };
    state.leave();
    Ok(value)
}

// ── JSON injection ────────────────────────────────────────────────────

/// Inject a (possibly modified) `WInstance` back into a JSON CST Schema.
///
/// This source-compatible entry point uses an open protocol. Call
/// [`inject_json_cst_with_protocol`] when protocol value constraints matter.
///
/// # Errors
///
/// Returns `CstExtractError` if the complement is invalid.
pub fn inject_json_cst(
    instance: &WInstance,
    complement: &CstComplement,
    domain_schema: &Schema,
) -> Result<Schema, CstExtractError> {
    inject_json_cst_with_protocol(instance, complement, domain_schema, &Protocol::default())
        .map_err(Into::into)
}

/// Inject a JSON CST using the selected protocol's value constraints.
///
/// Updates the `literal-value` and `interstitial-*` constraints in the
/// CST Schema to reflect changes in the `WInstance`. The result can be
/// emitted via `emit_from_schema` to produce formatted bytes.
///
/// # Errors
///
/// Returns `CstCodecError` if the complement is invalid.
pub fn inject_json_cst_with_protocol(
    instance: &WInstance,
    complement: &CstComplement,
    domain_schema: &Schema,
    protocol: &Protocol,
) -> Result<Schema, CstCodecError> {
    validate_tree_structure(instance, complement, domain_schema, protocol, "json")?;
    let mut cst = complement.cst_schema.clone();

    for (&node_id, node) in &instance.nodes {
        if let Some(ref presence) = node.value {
            let Some(segments) = complement.node_to_cst_value.get(&node_id) else {
                continue;
            };
            let Some(head) = segments.first() else {
                continue;
            };
            let spec = ValueSpec::for_vertex(domain_schema, protocol, &node.anchor);
            // Every scalar is compared under the same declared kind used by
            // extraction. This preserves an untouched exact number, encoded
            // byte string, temporal string, or escaped string byte-for-byte.
            let struct_vertex = complement.node_to_cst_struct.get(&node_id).unwrap_or(head);
            if json_scalar_unchanged(&cst, struct_vertex, presence, &spec) {
                continue;
            }
            let site = ValueSite::new(node.anchor.clone(), cst_byte_offset(&cst, struct_vertex));
            let new_text = field_presence_to_json_text(presence, &spec, &site)?;
            let type_member_parent = instance
                .arcs
                .iter()
                .find(|(_, child, edge)| *child == node_id && edge.name.as_deref() == Some("$type"))
                .map(|(parent, _, _)| *parent);
            let parent_has_discriminator = type_member_parent.is_some_and(|parent| {
                instance
                    .node(parent)
                    .is_some_and(|node| node.discriminator.is_some())
            });
            if type_member_parent.is_some()
                && (parent_has_discriminator || new_text.starts_with('"'))
            {
                return Err(unsupported_structural_edit(
                    "json",
                    "a $type data member cannot be rewritten when its token would change discriminator presence or value",
                ));
            }
            set_toml_token_text(&mut cst, head, &new_text);
            for tail in &segments[1..] {
                set_toml_token_text(&mut cst, tail, "");
            }
        }
    }

    Ok(cst)
}

/// Whether the JSON scalar leaf at `cst_vertex` still decodes to the same
/// instance `presence`, in which case its original bytes should be preserved
/// verbatim rather than re-serialized from the (lossy) parsed `Value`.
///
/// This guards numeric tokens whose lexical form is not recoverable from the
/// `Value` (`1e10`, `-0`, `3.14159E-5`, integers past f64 precision) and the
/// boolean / `null` keywords. String tokens are decoded from the structural
/// vertex because their CST may split escapes into child vertices.
fn json_scalar_unchanged(
    cst: &Schema,
    cst_vertex: &str,
    presence: &FieldPresence,
    spec: &ValueSpec,
) -> bool {
    let kind = cst_vertex_kind(cst, cst_vertex).unwrap_or_default();
    let parsed = match kind.as_str() {
        "string" => json_string_value(cst, cst_vertex)
            .and_then(|text| coerce(&Lexeme::new(&text, LexClass::String), spec).ok()),
        "number" => literal_value(cst, cst_vertex).and_then(|text| {
            crate::json_pathway::coerce_exact_lexeme(&Lexeme::new(&text, LexClass::Number), spec)
                .ok()
        }),
        "true" => coerce(&Lexeme::new("true", LexClass::Bool), spec).ok(),
        "false" => coerce(&Lexeme::new("false", LexClass::Bool), spec).ok(),
        "null" => Some(Value::Null),
        _ => None,
    };
    parsed.is_some_and(|value| presence_matches_value(presence, &value))
}

fn presence_matches_value(presence: &FieldPresence, value: &Value) -> bool {
    match presence {
        FieldPresence::Present(expected) => scalar_values_match(expected, value),
        FieldPresence::Null => *value == Value::Null,
        FieldPresence::Absent => false,
    }
}

fn scalar_values_match(expected: &Value, actual: &Value) -> bool {
    match (expected, actual) {
        (Value::Float(expected), Value::Float(actual)) => expected.to_bits() == actual.to_bits(),
        _ => expected == actual,
    }
}

/// Convert a `FieldPresence` to the JSON text representation.
///
/// Used by [`inject_json_cst`] to overwrite `literal-value` constraints
/// on a CST leaf when the instance has changed. The produced text must
/// match the wire format of a JSON token bit-for-bit so the
/// format-preserving emitter is byte-identity for unchanged instances:
///
/// - `Str(s)` is encoded as a complete JSON string token.
/// - `Float(f)` always renders with a decimal point, so `6.0` round-trips
///   as `"6.0"` rather than `"6"` (the default `f64::Display`).
/// - `Null` renders as the literal token `null`; `Present(Null)` is refused
///   because JSON cannot preserve that distinct presence state.
fn field_presence_to_json_text(
    presence: &FieldPresence,
    spec: &ValueSpec,
    site: &ValueSite,
) -> Result<String, CstCodecError> {
    match presence {
        FieldPresence::Present(value) => json_present_value_text(value, spec, site),
        FieldPresence::Null => Ok("null".to_owned()),
        FieldPresence::Absent => Err(EmitValueError::new(
            site.clone(),
            "absent",
            "json",
            EmitValueReason::AbsentNotRepresentable,
        )
        .into()),
    }
}

fn json_present_value_text(
    value: &Value,
    spec: &ValueSpec,
    site: &ValueSite,
) -> Result<String, CstCodecError> {
    if matches!(value, Value::Null) {
        return Err(EmitValueError::new(
            site.clone(),
            "null",
            "json",
            EmitValueReason::NotRepresentable,
        )
        .into());
    }
    if matches!(value, Value::Float(number) if !number.is_finite()) {
        return Err(
            EmitValueError::new(site.clone(), "float", "json", EmitValueReason::NonFinite).into(),
        );
    }
    if !crate::json_pathway::json_value_roundtrips(value, spec) {
        return Err(EmitValueError::new(
            site.clone(),
            value.type_name(),
            "json",
            EmitValueReason::NotRepresentable,
        )
        .into());
    }
    match value {
        Value::Str(text) | Value::Token(text) => {
            Ok(format!("\"{}\"", json_encode_string_content(text)))
        }
        Value::Int(number) => Ok(number.to_string()),
        Value::Float(number) => Ok(json_format_float(*number)),
        Value::Bool(value) => Ok(value.to_string()),
        Value::Bytes(bytes) => json_bytes_text(bytes, spec, site),
        Value::Decimal(number) => Ok(number.to_string()),
        Value::BigInt(number) => Ok(number.to_string()),
        Value::Temporal(value) => Ok(format!(
            "\"{}\"",
            json_encode_string_content(&value.to_string())
        )),
        other => Err(EmitValueError::new(
            site.clone(),
            other.type_name(),
            "json",
            EmitValueReason::NotRepresentable,
        )
        .into()),
    }
}

fn json_bytes_text(
    bytes: &[u8],
    spec: &ValueSpec,
    site: &ValueSite,
) -> Result<String, CstCodecError> {
    let ScalarKind::Bytes(encoding) = &spec.kind else {
        return Err(EmitValueError::new(
            site.clone(),
            "bytes",
            "json",
            EmitValueReason::NotRepresentable,
        )
        .into());
    };
    let encoded = crate::json_pathway::encode_bytes(bytes, encoding).ok_or_else(|| {
        EmitValueError::new(
            site.clone(),
            "bytes",
            "json",
            EmitValueReason::NotRepresentable,
        )
    })?;
    Ok(format!("\"{}\"", json_encode_string_content(&encoded)))
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
    debug_assert!(f.is_finite(), "callers reject non-finite JSON floats");
    let s = f.to_string();
    if s.contains('.') || s.contains('e') || s.contains('E') {
        s
    } else {
        format!("{s}.0")
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
            } else if is_interstitial_text_sort(c.sort.as_ref()) {
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

/// Write `new_text` across the run of CST vertices that carried a value.
///
/// The whole value goes into the run's first vertex and every later vertex
/// is emptied. Replay decides coverage from each vertex's recorded source
/// span, so an emptied vertex still consumes the bytes it used to occupy:
/// the segments that used to spell the rest of the value neither replay
/// behind the new text nor leave a hole for the following token to slide
/// into. Writing only the first vertex — which is what a single-vertex
/// mapping amounts to — instead appends the old tail to the new value, so
/// editing `"x\ny"` produced `"x\ny-perturbed\ny"`.
fn set_value_run(cst: &mut Schema, run: &[Name], new_text: &str) {
    let mut segments = run.iter();
    let Some(head) = segments.next() else {
        return;
    };
    update_literal_value(cst, head, new_text);
    for tail in segments {
        update_literal_value(cst, tail, "");
    }
}

/// Replace an XML character-data run with one escaped text token.
///
/// The run may have originated as ordinary character data, references, or a
/// CDATA section. Replacing its complete first span with escaped character
/// data preserves the semantic text without depending on the original CST
/// node kind's internal token layout.
fn set_xml_text_run(cst: &mut Schema, run: &[Name], new_text: &str) {
    let mut segments = run.iter();
    let Some(head) = segments.next() else {
        return;
    };
    if cst_vertex_kind(cst, head).as_deref() == Some("CDSect")
        && !new_text.contains("]]>")
        && !new_text.contains('\r')
    {
        set_toml_token_text(cst, head, &format!("<![CDATA[{new_text}]]>"));
    } else {
        let escaped = escape_xml_edit_text(new_text, false);
        set_toml_token_text(cst, head, &escaped);
    }
    for tail in segments {
        set_toml_token_text(cst, tail, "");
    }
}

// ── TOML extraction ───────────────────────────────────────────────────

/// The CST kinds that hold key/value entries: the document itself, a
/// `[table]` header's body, a `[[table array]]` element's body, and an inline
/// `{ ... }` table.
const TOML_TABLE_KINDS: [&str; 4] = ["document", "table", "table_array_element", "inline_table"];

/// The CST kinds that spell a key.
const TOML_KEY_KINDS: [&str; 3] = ["bare_key", "quoted_key", "dotted_key"];

fn validate_json_cst_string_tokens(cst: &Schema) -> Result<(), CstCodecError> {
    for (name, vertex) in &cst.vertices {
        if vertex.kind.as_ref() != "string" {
            continue;
        }
        let token = cst_source_text(cst, name);
        serde_json::from_str::<String>(&token).map_err(|error| {
            let (reason, stop) =
                crate::json_pathway::json_string_token_error(token.as_bytes(), &error);
            let start = cst_byte_offset(cst, name);
            CstCodecError::Structure(match (start, stop) {
                (Some(start), Some(stop)) => format!(
                    "invalid JSON string token at byte {start}: {reason} at byte {}",
                    start.saturating_add(u64::try_from(stop).unwrap_or(u64::MAX))
                ),
                (Some(start), None) => {
                    format!("invalid JSON string token at byte {start}: {reason}")
                }
                (None, Some(stop)) => format!(
                    "invalid JSON string token at an unrecorded byte: {reason} at byte {stop} \
                     of the token"
                ),
                (None, None) => {
                    format!("invalid JSON string token at an unrecorded byte: {reason}")
                }
            })
        })?;
    }
    Ok(())
}

fn validate_toml_cst_string_tokens(cst: &Schema) -> Result<(), CstCodecError> {
    for (name, vertex) in &cst.vertices {
        if !matches!(vertex.kind.as_ref(), "string" | "quoted_key") {
            continue;
        }
        let token = cst_source_text(cst, name);
        if !token.starts_with('"') {
            continue;
        }
        let synthetic = format!("value = {token}");
        toml::from_str::<toml::Table>(&synthetic).map_err(|error| {
            CstCodecError::Structure(format!(
                "invalid TOML basic string token at byte {:?}: {error}",
                cst_byte_offset(cst, name)
            ))
        })?;
    }
    Ok(())
}

/// Extract a `WInstance` from a TOML CST Schema, guided by a domain schema.
///
/// This source-compatible entry point uses an open protocol. Call
/// [`extract_toml_cst_with_protocol`] when protocol value constraints matter.
///
/// # Errors
///
/// Returns `CstExtractError` if the CST structure is invalid, if it does not
/// match the domain schema, or if it nests past [`MAX_EXTRACT_DEPTH`].
pub fn extract_toml_cst(
    cst: &Schema,
    domain_schema: &Schema,
    root_vertex: &str,
) -> Result<(WInstance, CstComplement), CstExtractError> {
    extract_toml_cst_with_protocol(cst, domain_schema, &Protocol::default(), root_vertex)
        .map_err(Into::into)
}

/// Extract a TOML CST using the selected protocol's value constraints.
///
/// TOML's tree differs from JSON's in three ways that matter here. The
/// document *is* the top-level table rather than wrapping a single value; a
/// `[table]` header and its pairs are one CST node whose first child is the
/// key; and repeated `[[table array]]` headers with the same key are the
/// elements of one list.
///
/// # Errors
///
/// Returns `CstCodecError` if the CST structure is invalid, if it does not
/// match the domain schema, or if it nests past [`MAX_EXTRACT_DEPTH`].
pub fn extract_toml_cst_with_protocol(
    cst: &Schema,
    domain_schema: &Schema,
    protocol: &Protocol,
    root_vertex: &str,
) -> Result<(WInstance, CstComplement), CstCodecError> {
    validate_cst_has_no_recovery_markers(cst, "TOML")?;
    validate_toml_cst_string_tokens(cst)?;
    if cst
        .vertices
        .values()
        .any(|vertex| *vertex.kind == *"dotted_key")
    {
        return Err(CstCodecError::Structure(
            "TOML dotted keys require nested-table semantics and are not supported".into(),
        ));
    }
    if !domain_schema.has_vertex(root_vertex) {
        return Err(CstCodecError::SchemaMismatch(format!(
            "root vertex '{root_vertex}' not found in domain schema"
        )));
    }

    let mut state = ExtractState::new();
    let doc_vertex = Name::from(find_cst_root(cst)?.as_str());

    let root_id = state.alloc_id();
    extract_toml_table(
        cst,
        domain_schema,
        protocol,
        &doc_vertex,
        root_vertex,
        root_id,
        &mut state,
    )?;

    finish_fidelity(&mut state)?;
    let complement = CstComplement {
        format: "toml".into(),
        cst_schema: cst.clone(),
        node_to_cst_value: state.node_to_cst_value,
        node_to_cst_struct: state.node_to_cst_struct,
        cell_to_cst_value: HashMap::new(),
        source_vertex: Some(Name::from(root_vertex)),
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

/// One key/value entry of a TOML table.
struct TomlEntry<'a> {
    /// The entry's key, as written.
    key: String,
    /// The CST vertex carrying the entry's value: a scalar, an `array`, an
    /// `inline_table`, or a `table` / `table_array_element` body.
    value: &'a Name,
}

/// The key text of a `pair`, `table`, or `table_array_element` node.
fn toml_entry_key(cst: &Schema, vertex: &Name) -> Option<String> {
    cst_children_by_edge_kind(cst, vertex, "child_of")
        .into_iter()
        .find(|child| {
            cst_vertex_kind(cst, child).is_some_and(|k| TOML_KEY_KINDS.contains(&k.as_str()))
        })
        .map(|key_vertex| toml_key_text(cst, key_vertex))
}

/// The text a key node denotes, with a quoted key's quotes removed.
fn toml_key_text(cst: &Schema, key_vertex: &Name) -> String {
    toml_unquote(&cst_source_text(cst, key_vertex))
}

/// Reassemble the source text a CST vertex spanned.
///
/// A leaf carries its whole text in `literal-value`. A vertex with children
/// carries the runs between them as `interstitial-N` constraints and leaves
/// the rest to those children, so its text is the two interleaved back into
/// source order. TOML needs this where JSON does not: an escape inside a
/// basic string is its own child, but the unescaped runs around it are not,
/// so a string with an escape has no `literal-value` of its own and its plain
/// text lives entirely in its interstitials.
fn cst_source_text(cst: &Schema, vertex: &str) -> String {
    if let Some(text) = literal_value(cst, vertex) {
        return text;
    }

    let mut fragments: Vec<(usize, String)> = Vec::new();
    if let Some(constraints) = cst.constraints.get(vertex) {
        let by_sort: HashMap<&str, &str> = constraints
            .iter()
            .map(|c| (c.sort.as_ref(), c.value.as_str()))
            .collect();
        for c in constraints {
            let sort = c.sort.as_ref();
            if !is_interstitial_text_sort(sort) {
                continue;
            }
            let Some(start) = by_sort
                .get(format!("{sort}-start-byte").as_str())
                .and_then(|v| v.parse::<usize>().ok())
            else {
                continue;
            };
            fragments.push((start, c.value.clone()));
        }
    }
    for edge in cst.outgoing_edges(vertex) {
        let child = &edge.tgt;
        let start = cst
            .constraints
            .get(child)
            .and_then(|cs| {
                cs.iter()
                    .find(|c| c.sort.as_ref() == "start-byte")
                    .and_then(|c| c.value.parse::<usize>().ok())
            })
            .unwrap_or(usize::MAX);
        fragments.push((start, cst_source_text(cst, child)));
    }

    fragments.sort_by_key(|(start, _)| *start);
    fragments.into_iter().map(|(_, text)| text).collect()
}

/// Give the CST vertex at `vertex` a `literal-value` spanning its whole
/// recorded span, replacing whatever text it used to hold.
///
/// A vertex's recorded span is the widest fragment starting where it starts,
/// so replay writes this text and skips the interstitials and children nested
/// inside it. That is what lets one call replace a whole TOML token, escapes
/// and all, rather than one of the pieces it was split into.
fn set_toml_token_text(cst: &mut Schema, vertex: &Name, new_text: &str) {
    let constraints = cst.constraints.entry(vertex.clone()).or_default();
    if let Some(existing) = constraints
        .iter_mut()
        .find(|c| c.sort.as_ref() == "literal-value")
    {
        new_text.clone_into(&mut existing.value);
        return;
    }
    constraints.push(Constraint {
        sort: Name::from("literal-value"),
        value: new_text.to_owned(),
    });
}

/// Strip a TOML string's delimiters and decode its escapes.
///
/// Basic strings (`"…"`) take escapes; literal strings (`'…'`) do not, and
/// multi-line forms use tripled delimiters.
fn toml_unquote(raw: &str) -> String {
    for delim in ["\"\"\"", "'''"] {
        if let Some(inner) = raw.strip_prefix(delim).and_then(|r| r.strip_suffix(delim)) {
            let inner = inner
                .strip_prefix("\r\n")
                .or_else(|| inner.strip_prefix('\n'))
                .unwrap_or(inner);
            return if delim == "\"\"\"" {
                toml_decode_escapes(inner)
            } else {
                inner.to_owned()
            };
        }
    }
    if let Some(inner) = raw.strip_prefix('"').and_then(|r| r.strip_suffix('"')) {
        return toml_decode_escapes(inner);
    }
    if let Some(inner) = raw.strip_prefix('\'').and_then(|r| r.strip_suffix('\'')) {
        return inner.to_owned();
    }
    raw.to_owned()
}

/// Decode the escape sequences a TOML basic string admits.
fn toml_decode_escapes(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some('b') => out.push('\u{0008}'),
            Some('f') => out.push('\u{000C}'),
            Some('e') => out.push('\u{001B}'),
            Some('"') => out.push('"'),
            Some('\n') => {
                while chars
                    .next_if(|next| matches!(next, ' ' | '\t' | '\n' | '\r'))
                    .is_some()
                {}
            }
            Some('\r') if chars.next_if_eq(&'\n').is_some() => {
                while chars
                    .next_if(|next| matches!(next, ' ' | '\t' | '\n' | '\r'))
                    .is_some()
                {}
            }
            // An escaped backslash, and a backslash with nothing after it,
            // both denote one backslash.
            Some('\\') | None => out.push('\\'),
            Some(esc @ ('u' | 'U')) => {
                let width = if esc == 'u' { 4 } else { 8 };
                let digits: String = chars.clone().take(width).collect();
                match u32::from_str_radix(&digits, 16)
                    .ok()
                    .and_then(char::from_u32)
                {
                    Some(decoded) if digits.len() == width => {
                        for _ in 0..width {
                            chars.next();
                        }
                        out.push(decoded);
                    }
                    // Not a well-formed escape: keep the source text so the
                    // value still round-trips to the bytes it came from.
                    _ => {
                        out.push('\\');
                        out.push(esc);
                    }
                }
            }
            Some('x') => {
                let digits: String = chars.clone().take(2).collect();
                match u8::from_str_radix(&digits, 16) {
                    Ok(decoded) if digits.len() == 2 => {
                        chars.next();
                        chars.next();
                        out.push(char::from(decoded));
                    }
                    _ => out.push_str("\\x"),
                }
            }
            // Not an escape this grammar defines: keep the backslash and
            // whatever followed it, so the value still round-trips to the
            // bytes it came from.
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
        }
    }
    out
}

/// Encode `s` as the body of a TOML basic string, delimiters excluded.
fn toml_encode_string_content(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{0008}' => out.push_str("\\b"),
            '\u{000C}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 || c as u32 == 0x7F => {
                let _ = write!(out, "\\u{:04X}", c as u32);
            }
            c => out.push(c),
        }
    }
    out
}

/// The entries of a TOML table-like node, in source order.
///
/// A `pair` contributes its key and value directly. A nested `table` or
/// `table_array_element` contributes its own key and itself as the value, so
/// the recursion descends into it exactly as it does into an inline table.
fn toml_entries<'a>(cst: &'a Schema, container: &Name) -> Vec<TomlEntry<'a>> {
    let mut entries = Vec::new();
    for child in cst_children_by_edge_kind(cst, container, "child_of") {
        let Some(kind) = cst_vertex_kind(cst, child) else {
            continue;
        };
        match kind.as_str() {
            "pair" => {
                let Some(key) = toml_entry_key(cst, child) else {
                    continue;
                };
                let Some(value) = toml_pair_value(cst, child) else {
                    continue;
                };
                entries.push(TomlEntry { key, value });
            }
            "table" | "table_array_element" => {
                let Some(key) = toml_entry_key(cst, child) else {
                    continue;
                };
                entries.push(TomlEntry { key, value: child });
            }
            _ => {}
        }
    }
    entries
}

/// The value vertex of a `pair`: its first child that is not the key.
fn toml_pair_value<'a>(cst: &'a Schema, pair: &Name) -> Option<&'a Name> {
    cst_children_by_edge_kind(cst, pair, "child_of")
        .into_iter()
        .find(|child| {
            cst_vertex_kind(cst, child).is_some_and(|k| !TOML_KEY_KINDS.contains(&k.as_str()))
        })
}

/// Group the entries of a table by key, preserving first-appearance order.
///
/// Repeated `[[table array]]` headers share a key and are the elements of one
/// list; every other key appears once.
fn toml_grouped_entries<'a>(
    cst: &'a Schema,
    container: &Name,
) -> Result<Vec<(String, Vec<&'a Name>)>, CstCodecError> {
    let mut grouped: Vec<(String, Vec<&Name>)> = Vec::new();
    for entry in toml_entries(cst, container) {
        if let Some(slot) = grouped.iter_mut().find(|(key, _)| *key == entry.key) {
            slot.1.push(entry.value);
        } else {
            grouped.push((entry.key, vec![entry.value]));
        }
    }
    for (_, values) in &grouped {
        if values.len() > 1
            && !values
                .iter()
                .all(|value| is_toml_table_array_element(cst, value))
        {
            let offset = cst_byte_offset(cst, values[1])
                .map_or_else(|| "unknown".to_owned(), |value| value.to_string());
            return Err(CstCodecError::Structure(format!(
                "duplicate TOML key at byte {offset}"
            )));
        }
    }
    Ok(grouped)
}

/// Whether a CST vertex is a `[[table array]]` element.
fn is_toml_table_array_element(cst: &Schema, vertex: &Name) -> bool {
    cst_vertex_kind(cst, vertex).as_deref() == Some("table_array_element")
}

/// Extract a TOML table-like node, matching keys to domain schema edges.
///
/// With no outgoing edges on the domain vertex (the open-schema case) every
/// entry becomes a child node under a synthesised `prop` edge, exactly as the
/// JSON extractor does.
fn extract_toml_table(
    cst: &Schema,
    domain_schema: &Schema,
    protocol: &Protocol,
    cst_vertex: &Name,
    domain_vertex: &str,
    node_id: u32,
    state: &mut ExtractState,
) -> Result<(), CstCodecError> {
    state.enter()?;
    let mut node = Node::new(node_id, domain_vertex);
    state.node_to_cst_struct.insert(node_id, cst_vertex.clone());

    let domain_edges: Vec<Edge> = domain_schema.outgoing_edges(domain_vertex).to_vec();
    let grouped = toml_grouped_entries(cst, cst_vertex)?;

    if domain_edges.is_empty() {
        state.nodes.insert(node_id, node);
        for (key, values) in grouped {
            let child_anchor = format!("{domain_vertex}:{key}");
            let child_id = state.alloc_id();
            extract_toml_entry_open(cst, &values, &child_anchor, child_id, state)?;
            let synth_edge = Edge {
                src: Name::from(domain_vertex),
                tgt: Name::from(child_anchor.as_str()),
                kind: "prop".into(),
                name: Some(Name::from(key.as_str())),
            };
            state.arcs.push((node_id, child_id, synth_edge));
        }
        state.leave();
        return Ok(());
    }

    let mut handled_keys = std::collections::HashSet::new();
    for domain_edge in &domain_edges {
        let field_name = domain_edge.name.as_deref().unwrap_or(&domain_edge.tgt);
        handled_keys.insert(field_name.to_owned());
        let Some((_, values)) = grouped.iter().find(|(key, _)| key == field_name) else {
            continue;
        };
        let child_id = state.alloc_id();
        extract_toml_entry(
            cst,
            domain_schema,
            protocol,
            values,
            &domain_edge.tgt,
            child_id,
            state,
        )?;
        state.arcs.push((node_id, child_id, domain_edge.clone()));
    }

    // Keys the domain schema does not declare survive as extra fields, the
    // same way unhandled JSON pairs do.
    for (key, values) in &grouped {
        if handled_keys.contains(key) {
            continue;
        }
        if let [single] = values.as_slice() {
            node.extra_fields.insert(
                key.clone(),
                toml_generic_value(cst, single, domain_vertex, state)?,
            );
        } else {
            node.extra_fields.insert(
                key.clone(),
                Value::List(
                    values
                        .iter()
                        .map(|value| toml_generic_value(cst, value, domain_vertex, state))
                        .collect::<Result<Vec<_>, _>>()?,
                ),
            );
        }
    }

    state.nodes.insert(node_id, node);
    state.leave();
    Ok(())
}

/// Extract the value(s) a single key carries, schema-guided.
///
/// Several `[[table array]]` elements under one key form a list; a single
/// value is extracted directly.
fn extract_toml_entry(
    cst: &Schema,
    domain_schema: &Schema,
    protocol: &Protocol,
    values: &[&Name],
    domain_vertex: &str,
    node_id: u32,
    state: &mut ExtractState,
) -> Result<(), CstCodecError> {
    if let [single] = values {
        if !is_toml_table_array_element(cst, single) {
            return extract_toml_value(
                cst,
                domain_schema,
                protocol,
                single,
                domain_vertex,
                node_id,
                state,
            );
        }
    }

    state.enter()?;
    let mut node = Node::new(node_id, domain_vertex);
    node.shape = NodeShape::List;
    state.nodes.insert(node_id, node);

    let domain_edges: Vec<Edge> = domain_schema.outgoing_edges(domain_vertex).to_vec();
    let item_edge = domain_edges.iter().find(|edge| edge.name.is_none());

    for element in values {
        let child_id = state.alloc_id();
        if let Some(edge) = item_edge {
            extract_toml_value(
                cst,
                domain_schema,
                protocol,
                element,
                &edge.tgt,
                child_id,
                state,
            )?;
            state.arcs.push((node_id, child_id, edge.clone()));
        } else {
            let child_anchor = format!("{domain_vertex}:items");
            extract_toml_value_open(cst, element, &child_anchor, child_id, state)?;
            state.arcs.push((
                node_id,
                child_id,
                Edge {
                    src: Name::from(domain_vertex),
                    tgt: Name::from(child_anchor.as_str()),
                    kind: "item".into(),
                    name: Some("item".into()),
                },
            ));
        }
    }

    state.leave();
    Ok(())
}

/// The open-schema twin of [`extract_toml_entry`].
fn extract_toml_entry_open(
    cst: &Schema,
    values: &[&Name],
    anchor: &str,
    node_id: u32,
    state: &mut ExtractState,
) -> Result<(), CstCodecError> {
    if let [single] = values {
        if !is_toml_table_array_element(cst, single) {
            return extract_toml_value_open(cst, single, anchor, node_id, state);
        }
    }

    state.enter()?;
    let mut node = Node::new(node_id, anchor);
    node.shape = NodeShape::List;
    state.nodes.insert(node_id, node);

    for element in values {
        let child_anchor = format!("{anchor}:items");
        let child_id = state.alloc_id();
        extract_toml_value_open(cst, element, &child_anchor, child_id, state)?;
        state.arcs.push((
            node_id,
            child_id,
            Edge {
                src: Name::from(anchor),
                tgt: Name::from(child_anchor.as_str()),
                kind: "item".into(),
                name: Some("item".into()),
            },
        ));
    }

    state.leave();
    Ok(())
}

/// Extract a TOML value node, schema-guided.
fn extract_toml_value(
    cst: &Schema,
    domain_schema: &Schema,
    protocol: &Protocol,
    cst_vertex: &Name,
    domain_vertex: &str,
    node_id: u32,
    state: &mut ExtractState,
) -> Result<(), CstCodecError> {
    state.enter()?;
    let kind = cst_vertex_kind(cst, cst_vertex)
        .ok_or_else(|| CstCodecError::VertexNotFound(cst_vertex.to_string()))?;
    state.node_to_cst_struct.insert(node_id, cst_vertex.clone());

    let extracted = if TOML_TABLE_KINDS.contains(&kind.as_str()) {
        extract_toml_table(
            cst,
            domain_schema,
            protocol,
            cst_vertex,
            domain_vertex,
            node_id,
            state,
        )
    } else if kind == "array" {
        extract_toml_array(
            cst,
            domain_schema,
            protocol,
            cst_vertex,
            domain_vertex,
            node_id,
            state,
        )
    } else {
        let (text, class) = toml_scalar_lexeme(cst, cst_vertex);
        let presence = coerce_cst_scalar(
            cst,
            cst_vertex,
            domain_schema,
            protocol,
            domain_vertex,
            &Lexeme::new(&text, class),
            state,
        );
        let node = Node::new(node_id, domain_vertex).with_value(presence);
        // The whole token, not the deepest leaf under it: a basic string with
        // one escape has exactly one child, and addressing that child would
        // rewrite the escape and leave the text around it in place.
        state
            .node_to_cst_value
            .insert(node_id, vec![cst_vertex.clone()]);
        state.nodes.insert(node_id, node);
        Ok(())
    };

    state.leave();
    extracted
}

/// Extract a TOML value node without schema guidance.
fn extract_toml_value_open(
    cst: &Schema,
    cst_vertex: &Name,
    anchor: &str,
    node_id: u32,
    state: &mut ExtractState,
) -> Result<(), CstCodecError> {
    state.enter()?;
    let kind = cst_vertex_kind(cst, cst_vertex).unwrap_or_default();
    state.node_to_cst_struct.insert(node_id, cst_vertex.clone());

    let extracted = if TOML_TABLE_KINDS.contains(&kind.as_str()) {
        extract_toml_table_open(cst, cst_vertex, anchor, node_id, state)
    } else if kind == "array" {
        extract_toml_array_open(cst, cst_vertex, anchor, node_id, state)
    } else {
        let value = match toml_scalar_value(cst, cst_vertex) {
            Ok(value) => value,
            Err(error) => {
                state
                    .fidelity
                    .push(error.at(ValueSite::new(anchor, cst_byte_offset(cst, cst_vertex))));
                Value::Null
            }
        };
        let node = Node::new(node_id, anchor).with_value(FieldPresence::Present(value));
        // The whole token, for the reason given in `extract_toml_value`.
        state
            .node_to_cst_value
            .insert(node_id, vec![cst_vertex.clone()]);
        state.nodes.insert(node_id, node);
        Ok(())
    };

    state.leave();
    extracted
}

/// Extract a TOML table without schema guidance, synthesising one `prop`
/// edge per key.
fn extract_toml_table_open(
    cst: &Schema,
    cst_vertex: &Name,
    anchor: &str,
    node_id: u32,
    state: &mut ExtractState,
) -> Result<(), CstCodecError> {
    let node = Node::new(node_id, anchor);
    state.nodes.insert(node_id, node);

    for (key, values) in toml_grouped_entries(cst, cst_vertex)? {
        let child_anchor = format!("{anchor}:{key}");
        let child_id = state.alloc_id();
        extract_toml_entry_open(cst, &values, &child_anchor, child_id, state)?;
        state.arcs.push((
            node_id,
            child_id,
            Edge {
                src: Name::from(anchor),
                tgt: Name::from(child_anchor.as_str()),
                kind: "prop".into(),
                name: Some(Name::from(key.as_str())),
            },
        ));
    }

    Ok(())
}

/// Extract a TOML `[…]` array, schema-guided.
fn extract_toml_array(
    cst: &Schema,
    domain_schema: &Schema,
    protocol: &Protocol,
    cst_vertex: &Name,
    domain_vertex: &str,
    node_id: u32,
    state: &mut ExtractState,
) -> Result<(), CstCodecError> {
    let mut node = Node::new(node_id, domain_vertex);
    node.shape = NodeShape::List;
    state.nodes.insert(node_id, node);

    let domain_edges: Vec<Edge> = domain_schema.outgoing_edges(domain_vertex).to_vec();
    let item_edge = domain_edges.iter().find(|edge| edge.name.is_none());

    for child in toml_array_elements(cst, cst_vertex) {
        let child_id = state.alloc_id();
        if let Some(edge) = item_edge {
            extract_toml_value(
                cst,
                domain_schema,
                protocol,
                child,
                &edge.tgt,
                child_id,
                state,
            )?;
            state.arcs.push((node_id, child_id, edge.clone()));
        } else {
            let child_anchor = format!("{domain_vertex}:items");
            extract_toml_value_open(cst, child, &child_anchor, child_id, state)?;
            state.arcs.push((
                node_id,
                child_id,
                Edge {
                    src: Name::from(domain_vertex),
                    tgt: Name::from(child_anchor.as_str()),
                    kind: "item".into(),
                    name: Some("item".into()),
                },
            ));
        }
    }

    Ok(())
}

/// Extract a TOML `[…]` array without schema guidance.
fn extract_toml_array_open(
    cst: &Schema,
    cst_vertex: &Name,
    anchor: &str,
    node_id: u32,
    state: &mut ExtractState,
) -> Result<(), CstCodecError> {
    let mut node = Node::new(node_id, anchor);
    node.shape = NodeShape::List;
    state.nodes.insert(node_id, node);

    for child in toml_array_elements(cst, cst_vertex) {
        let child_anchor = format!("{anchor}:items");
        let child_id = state.alloc_id();
        extract_toml_value_open(cst, child, &child_anchor, child_id, state)?;
        state.arcs.push((
            node_id,
            child_id,
            Edge {
                src: Name::from(anchor),
                tgt: Name::from(child_anchor.as_str()),
                kind: "item".into(),
                name: Some("item".into()),
            },
        ));
    }

    Ok(())
}

/// The value children of a TOML array, comments and punctuation excluded.
fn toml_array_elements<'a>(cst: &'a Schema, array: &Name) -> Vec<&'a Name> {
    cst_children_by_edge_kind(cst, array, "child_of")
        .into_iter()
        .filter(|child| cst_vertex_kind(cst, child).as_deref() != Some("comment"))
        .collect()
}

fn toml_scalar_lexeme(cst: &Schema, cst_vertex: &Name) -> (String, LexClass) {
    let kind = cst_vertex_kind(cst, cst_vertex).unwrap_or_default();
    let text = cst_source_text(cst, cst_vertex);
    match kind.as_str() {
        "string" => (toml_unquote(&text), LexClass::String),
        "boolean" => (text, LexClass::Bool),
        "integer" => (
            normalize_radix_integer(&text, true).unwrap_or_else(|| text.replace('_', "")),
            LexClass::Number,
        ),
        "float" => (text.replace('_', ""), LexClass::Number),
        "local_date" | "local_time" | "local_date_time" | "offset_date_time" => {
            (text, LexClass::DateTime)
        }
        _ => (text, LexClass::String),
    }
}

/// Convert a format-recognised radix integer to an exact decimal spelling.
/// The digit accumulator is decimal and unbounded, so no machine integer is
/// used as an intermediate carrier.
fn normalize_radix_integer(text: &str, allow_binary: bool) -> Option<String> {
    let compact = text.replace('_', "");
    let (negative, unsigned) = compact
        .strip_prefix('-')
        .map_or((false, compact.as_str()), |digits| (true, digits));
    let unsigned = unsigned.strip_prefix('+').unwrap_or(unsigned);
    let (radix, digits) = if let Some(digits) = unsigned.strip_prefix("0x") {
        (16_u32, digits)
    } else if let Some(digits) = unsigned.strip_prefix("0o") {
        (8_u32, digits)
    } else if allow_binary {
        (2_u32, unsigned.strip_prefix("0b")?)
    } else {
        return None;
    };
    if digits.is_empty() {
        return None;
    }

    let mut decimal = vec![0_u8];
    for character in digits.chars() {
        let digit = character.to_digit(radix)?;
        let mut carry = digit;
        for place in &mut decimal {
            let value = u32::from(*place) * radix + carry;
            *place = (value % 10) as u8;
            carry = value / 10;
        }
        while carry > 0 {
            decimal.push((carry % 10) as u8);
            carry /= 10;
        }
    }
    while decimal.len() > 1 && decimal.last() == Some(&0) {
        decimal.pop();
    }
    let mut result = String::with_capacity(decimal.len() + usize::from(negative));
    if negative && decimal.iter().any(|digit| *digit != 0) {
        result.push('-');
    }
    result.extend(decimal.iter().rev().map(|digit| char::from(b'0' + *digit)));
    Some(result)
}

/// The exact value a TOML scalar leaf denotes under an open schema.
fn toml_scalar_value(cst: &Schema, cst_vertex: &Name) -> Result<Value, ValueFidelityError> {
    let (text, class) = toml_scalar_lexeme(cst, cst_vertex);
    crate::json_pathway::coerce_exact_lexeme(
        &Lexeme::new(&text, class),
        &ValueSpec::new(ScalarKind::Any),
    )
}

/// The `Value` a TOML node denotes, for the extra-fields path that carries
/// undeclared keys without giving them instance nodes.
fn toml_generic_value(
    cst: &Schema,
    cst_vertex: &Name,
    domain_vertex: &str,
    state: &mut ExtractState,
) -> Result<Value, CstCodecError> {
    state.enter()?;
    let kind = cst_vertex_kind(cst, cst_vertex).unwrap_or_default();
    let value = if TOML_TABLE_KINDS.contains(&kind.as_str()) {
        let mut fields = HashMap::new();
        for (key, values) in toml_grouped_entries(cst, cst_vertex)? {
            let value = if let [single] = values.as_slice() {
                toml_generic_value(cst, single, domain_vertex, state)?
            } else {
                Value::List(
                    values
                        .iter()
                        .map(|value| toml_generic_value(cst, value, domain_vertex, state))
                        .collect::<Result<Vec<_>, _>>()?,
                )
            };
            fields.insert(key, value);
        }
        Value::Unknown(fields)
    } else if kind == "array" {
        Value::List(
            toml_array_elements(cst, cst_vertex)
                .into_iter()
                .map(|child| toml_generic_value(cst, child, domain_vertex, state))
                .collect::<Result<Vec<_>, _>>()?,
        )
    } else {
        match toml_scalar_value(cst, cst_vertex) {
            Ok(value) => value,
            Err(error) => {
                state.fidelity.push(error.at(ValueSite::new(
                    domain_vertex,
                    cst_byte_offset(cst, cst_vertex),
                )));
                Value::Null
            }
        }
    };
    state.leave();
    Ok(value)
}

/// Inject a modified `WInstance` back into a TOML CST Schema.
///
/// This source-compatible entry point uses an open protocol. Call
/// [`inject_toml_cst_with_protocol`] when protocol value constraints matter.
///
/// # Errors
///
/// Returns `CstExtractError` if the complement is invalid.
pub fn inject_toml_cst(
    instance: &WInstance,
    complement: &CstComplement,
    domain_schema: &Schema,
) -> Result<Schema, CstExtractError> {
    inject_toml_cst_with_protocol(instance, complement, domain_schema, &Protocol::default())
        .map_err(Into::into)
}

/// Inject a TOML CST using the selected protocol's value constraints.
///
/// # Errors
///
/// Returns `CstCodecError` if the complement is invalid.
pub fn inject_toml_cst_with_protocol(
    instance: &WInstance,
    complement: &CstComplement,
    domain_schema: &Schema,
    protocol: &Protocol,
) -> Result<Schema, CstCodecError> {
    validate_tree_structure(instance, complement, domain_schema, protocol, "toml")?;
    let mut cst = complement.cst_schema.clone();

    for (&node_id, node) in &instance.nodes {
        let Some(presence) = node.value.as_ref() else {
            continue;
        };
        let Some(segments) = complement.node_to_cst_value.get(&node_id) else {
            continue;
        };
        let Some(head) = segments.first() else {
            continue;
        };
        // A scalar's spelling is not recoverable from the parsed value: an
        // integer written `1_000`, a string written literally rather than
        // escaped, and a date of any form all re-render differently while
        // denoting the same thing. When the leaf's original text still
        // decodes to the instance's value, nothing changed, so the original
        // bytes stay.
        let spec = ValueSpec::for_vertex(domain_schema, protocol, &node.anchor);
        if toml_scalar_unchanged(&cst, head, presence, &spec) {
            continue;
        }
        let site = ValueSite::new(node.anchor.clone(), cst_byte_offset(&cst, head));
        let new_text = toml_presence_text(&cst, head, presence, &spec, &site)?;
        set_toml_token_text(&mut cst, head, &new_text);
        for tail in segments.iter().skip(1) {
            set_toml_token_text(&mut cst, tail, "");
        }
    }

    Ok(cst)
}

/// Whether the TOML leaf at `cst_vertex` still denotes `presence`.
fn toml_scalar_unchanged(
    cst: &Schema,
    cst_vertex: &Name,
    presence: &FieldPresence,
    spec: &ValueSpec,
) -> bool {
    let (text, class) = toml_scalar_lexeme(cst, cst_vertex);
    crate::json_pathway::coerce_exact_lexeme(&Lexeme::new(&text, class), spec)
        .is_ok_and(|value| presence_matches_value(presence, &value))
}

/// Render `presence` as the TOML token that replaces the leaf at
/// `cst_vertex`.
///
/// The leaf carries a whole token, delimiters included, so a string is
/// rendered with its quotes. TOML has no null or absent literal, so either
/// presence state is refused rather than collapsed into an empty string.
fn toml_presence_text(
    cst: &Schema,
    cst_vertex: &Name,
    presence: &FieldPresence,
    spec: &ValueSpec,
    site: &ValueSite,
) -> Result<String, CstCodecError> {
    if matches!(
        presence,
        FieldPresence::Present(Value::Null) | FieldPresence::Null
    ) {
        return Err(EmitValueError::new(
            site.clone(),
            "null",
            "toml",
            EmitValueReason::NullNotRepresentable,
        )
        .into());
    }
    let text = match presence {
        FieldPresence::Present(Value::Str(s) | Value::Token(s)) => {
            let literal_form = cst_source_text(cst, cst_vertex).starts_with('\'');
            if literal_form && !s.contains('\'') && !s.contains('\n') {
                format!("'{s}'")
            } else {
                format!("\"{}\"", toml_encode_string_content(s))
            }
        }
        FieldPresence::Present(Value::Int(i)) => i.to_string(),
        FieldPresence::Present(Value::Float(f)) => crate::toml_pathway::toml_float_lexeme(*f)
            .ok_or_else(|| {
                EmitValueError::new(
                    site.clone(),
                    "float",
                    "toml",
                    EmitValueReason::NotRepresentable,
                )
            })?,
        FieldPresence::Present(Value::Bool(b)) => b.to_string(),
        FieldPresence::Present(Value::Bytes(bytes)) => {
            let ScalarKind::Bytes(encoding) = &spec.kind else {
                return Err(EmitValueError::new(
                    site.clone(),
                    "bytes",
                    "toml",
                    EmitValueReason::NotRepresentable,
                )
                .into());
            };
            let encoded = crate::json_pathway::encode_bytes(bytes, encoding).ok_or_else(|| {
                EmitValueError::new(
                    site.clone(),
                    "bytes",
                    "toml",
                    EmitValueReason::NotRepresentable,
                )
            })?;
            format!("\"{}\"", toml_encode_string_content(&encoded))
        }
        FieldPresence::Present(Value::Temporal(value)) => value.to_string(),
        FieldPresence::Present(Value::Null) | FieldPresence::Null => {
            return Err(EmitValueError::new(
                site.clone(),
                "null",
                "toml",
                EmitValueReason::NullNotRepresentable,
            )
            .into());
        }
        FieldPresence::Absent => {
            return Err(EmitValueError::new(
                site.clone(),
                "absent",
                "toml",
                EmitValueReason::AbsentNotRepresentable,
            )
            .into());
        }
        FieldPresence::Present(other) => {
            return Err(EmitValueError::new(
                site.clone(),
                other.type_name(),
                "toml",
                EmitValueReason::NotRepresentable,
            )
            .into());
        }
    };
    if let FieldPresence::Present(value) = presence {
        if !crate::toml_pathway::toml_scalar_roundtrips(value, spec) {
            return Err(EmitValueError::new(
                site.clone(),
                value.type_name(),
                "toml",
                EmitValueReason::NotRepresentable,
            )
            .into());
        }
    }
    Ok(text)
}

// ── XML extraction ────────────────────────────────────────────────────

/// Extract a `WInstance` from an XML CST Schema, guided by a domain schema.
///
/// This source-compatible entry point uses an open protocol. Call
/// [`extract_xml_cst_with_protocol`] when protocol value constraints matter.
///
/// # Errors
///
/// Returns `CstExtractError` if the CST is invalid.
pub fn extract_xml_cst(
    cst: &Schema,
    domain_schema: &Schema,
    root_vertex: &str,
) -> Result<(WInstance, CstComplement), CstExtractError> {
    extract_xml_cst_with_protocol(cst, domain_schema, &Protocol::default(), root_vertex)
        .map_err(Into::into)
}

/// Extract an XML CST using the selected protocol's value constraints.
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
/// Returns `CstCodecError` if the CST is invalid.
pub fn extract_xml_cst_with_protocol(
    cst: &Schema,
    domain_schema: &Schema,
    protocol: &Protocol,
    root_vertex: &str,
) -> Result<(WInstance, CstComplement), CstCodecError> {
    validate_cst_has_no_recovery_markers(cst, "XML")?;
    if !domain_schema.has_vertex(root_vertex) {
        return Err(CstCodecError::SchemaMismatch(format!(
            "root vertex '{root_vertex}' not found in domain schema"
        )));
    }

    let mut state = ExtractState::new();
    let doc_vertex = find_cst_root(cst)?;

    let root_element = find_first_element_child(cst, &doc_vertex)
        .ok_or_else(|| CstCodecError::Structure("no element child in XML document".into()))?;

    let root_id = state.alloc_id();
    extract_xml_element(
        cst,
        (domain_schema, protocol),
        &root_element,
        root_vertex,
        root_id,
        &mut state,
        &HashMap::new(),
    )?;

    finish_fidelity(&mut state)?;
    let complement = CstComplement {
        format: "xml".into(),
        cst_schema: cst.clone(),
        node_to_cst_value: state.node_to_cst_value,
        node_to_cst_struct: state.node_to_cst_struct,
        cell_to_cst_value: HashMap::new(),
        source_vertex: Some(Name::from(root_vertex)),
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

/// Recursively extract an XML element from the CST into a `WInstance`
/// node, one level deeper than the caller and no deeper than
/// [`MAX_EXTRACT_DEPTH`].
#[allow(clippy::too_many_lines)]
fn extract_xml_element(
    cst: &Schema,
    domain: (&Schema, &Protocol),
    cst_vertex: &Name,
    domain_vertex: &str,
    node_id: u32,
    state: &mut ExtractState,
    parent_namespaces: &HashMap<String, String>,
) -> Result<(), CstCodecError> {
    let (domain_schema, protocol) = domain;
    state.enter()?;
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
        extract_xml_attributes(cst, stag, &mut node)?;
    }
    let mut namespaces = parent_namespaces.clone();
    for (key, value) in &node.extra_fields {
        let Value::Str(value) = value else { continue };
        if key == "xmlns" {
            if !is_valid_xml_namespace_declaration(key, value) {
                return Err(CstCodecError::Structure(format!(
                    "invalid XML namespace declaration {key:?}"
                )));
            }
            namespaces.insert(String::new(), value.clone());
        } else if let Some(prefix) = key.strip_prefix("xmlns:") {
            if !is_valid_xml_namespace_declaration(key, value) {
                return Err(CstCodecError::Structure(format!(
                    "invalid XML namespace declaration {key:?}"
                )));
            }
            namespaces.insert(prefix.to_owned(), value.clone());
        }
    }
    validate_xml_node_qnames(&node, &namespaces).map_err(CstCodecError::Structure)?;
    let mut xsi_nil = false;
    for (key, value) in &node.extra_fields {
        let semantic = key.rsplit_once(':').is_some_and(|(prefix, local)| {
            local == "nil"
                && namespaces
                    .get(prefix)
                    .is_some_and(|uri| uri == XML_SCHEMA_INSTANCE_NS)
        });
        if let (true, Value::Str(text)) = (semantic, value) {
            xsi_nil |= crate::xml_pathway::xsd_boolean(text).ok_or_else(|| {
                CstCodecError::Fidelity(vec![crate::xml_pathway::invalid_xsi_nil(
                    node.anchor.as_ref(),
                    text,
                )])
            })?;
        }
    }
    if xsi_nil {
        node.value = Some(FieldPresence::Null);
    }

    if let Some(ref content) = content_vertex {
        let content_children = ordered_content_children(cst, content);
        let has_elements = content_children
            .iter()
            .any(|(kind, _)| classify_content_kind(kind) == ContentClass::Element);
        // Opaque siblings (comments / processing-instructions) cannot
        // be folded into a single `node.value`: that collapse would overwrite
        // the first `CharData` leaf with the concatenated CharData text and
        // drop the opaque nodes, breaking the byte-faithful round-trip of such
        // content. When any opaque sibling is present, take the structural
        // path (and let the preserving emit replay the CST verbatim).
        // Character/entity references (`EntityRef`, `CharRef`, ...) are *not*
        // opaque: they are part of the text run, so they never force the
        // structural path on their own.
        let has_opaque = content_children
            .iter()
            .any(|(kind, _)| classify_content_kind(kind) == ContentClass::Opaque);
        let has_text = content_children
            .iter()
            .any(|(kind, _)| classify_content_kind(kind) == ContentClass::Text);
        let text_children: Vec<Name> = content_children
            .iter()
            .filter(|(kind, _)| classify_content_kind(kind) == ContentClass::Text)
            .map(|(_, name)| name.clone())
            .collect();
        let nil_text = xml_text_run_value(cst, &text_children)?.unwrap_or_default();
        if xsi_nil && (has_elements || !nil_text.is_empty()) {
            return Err(CstCodecError::Structure(
                "an xsi:nil XML element cannot contain character data or child elements".into(),
            ));
        }
        let domain_edges: Vec<Edge> = domain_schema.outgoing_edges(domain_vertex).to_vec();

        if !has_elements && !has_opaque {
            // Pure text content (possibly with entity references, which are
            // part of the same text run): collapse to node.value. This is the
            // leaf-element fast path: a round-trip just stores the captured
            // text. Crucially, collapsing here keeps the canonical emit
            // idempotent: re-emitting a value with `'`/`"`/`&` re-escapes it
            // to an entity, the re-parse re-splits the run into
            // `CharData EntityRef CharData`, and this branch folds it back to
            // exactly one node.
            if let Some(text) = xml_text_run_value(cst, &text_children)? {
                if !xsi_nil {
                    node.value = Some(coerce_cst_scalar(
                        cst,
                        content,
                        domain_schema,
                        protocol,
                        domain_vertex,
                        &Lexeme::new(&text, LexClass::String),
                        state,
                    ));
                    // The whole source-ordered run is the editable carrier.
                    // Injection first checks semantic equality, preserving an
                    // unchanged entity spelling byte-for-byte; after an edit,
                    // it writes escaped text into the first span and empties
                    // the remaining spans.
                    state.node_to_cst_value.insert(node_id, text_children);
                }
            }
        } else if has_text || has_opaque {
            // Mixed content: walk content in source order. Consecutive
            // text-class nodes (CharData + entity references) are coalesced
            // into a single text-segment leaf so an entity-split run
            // (`x&amp;y` -> `CharData EntityRef CharData`) does not multiply
            // the node count; each `element` becomes a regular child; opaque
            // siblings (comments/PIs) are preserved as structural-only
            // segments so the preserving emit replays them from the CST.
            emit_mixed_content(
                cst,
                domain,
                domain_vertex,
                node_id,
                &content_children,
                state,
                &namespaces,
            )?;
            state.nodes.insert(node_id, node);
            state.leave();
            return Ok(());
        }

        let child_elements: Vec<Name> = content_children
            .iter()
            .filter(|(kind, _)| kind == "element")
            .map(|(_, name)| name.clone())
            .collect();

        // Assign every source child exactly once. A named schema edge wins,
        // then the first item edge acts as the collection fallback. Unknown
        // children get a synthesized open-schema anchor instead of vanishing
        // merely because the parent schema describes some other child.
        for child_name in &child_elements {
            let tag = extract_xml_tag_name(cst, child_name).unwrap_or_else(|| "child".into());
            let domain_edge = domain_edges
                .iter()
                .find(|edge| {
                    let field_name = edge.name.as_deref().unwrap_or(&edge.tgt);
                    let is_item = *edge.kind == *"item" || edge.name.as_deref() == Some("item");
                    !is_item && field_name == tag
                })
                .or_else(|| {
                    domain_edges
                        .iter()
                        .find(|edge| *edge.kind == *"item" || edge.name.as_deref() == Some("item"))
                });
            let child_anchor = domain_edge.map_or_else(
                || Name::from(format!("{domain_vertex}:{tag}")),
                |edge| edge.tgt.clone(),
            );
            let child_id = state.alloc_id();
            extract_xml_element(
                cst,
                domain,
                child_name,
                &child_anchor,
                child_id,
                state,
                &namespaces,
            )?;
            let edge = domain_edge.cloned().unwrap_or_else(|| Edge {
                src: Name::from(domain_vertex),
                tgt: child_anchor,
                kind: "prop".into(),
                name: Some(Name::from(tag.as_str())),
            });
            state.arcs.push((node_id, child_id, edge));
        }
    }

    state.nodes.insert(node_id, node);
    state.leave();
    Ok(())
}

/// Walk an element's mixed `content` children in source order, emitting:
///
/// - one coalesced `XmlTextSegment` leaf per maximal run of text-class nodes
///   (`CharData` + entity references), so an entity-split text run does not
///   inflate the node count and the canonical emit stays idempotent;
/// - one regular child element per `element`;
/// - one structural-only segment per opaque sibling (comment /
///   processing-instruction). The opaque segment carries no value and is
///   mapped to its CST vertex so the preserving emit replays it verbatim.
fn emit_mixed_content(
    cst: &Schema,
    domain: (&Schema, &Protocol),
    domain_vertex: &str,
    node_id: u32,
    content_children: &[(String, Name)],
    state: &mut ExtractState,
    namespaces: &HashMap<String, String>,
) -> Result<(), CstCodecError> {
    let mut text_run: Vec<&Name> = Vec::new();

    let flush_text =
        |state: &mut ExtractState, run: &mut Vec<&Name>| -> Result<(), CstCodecError> {
            if run.is_empty() {
                return Ok(());
            }
            let segments: Vec<Name> = run.iter().map(|vertex| (*vertex).clone()).collect();
            let text = xml_text_run_value(cst, &segments)?.unwrap_or_default();
            run.clear();
            let seg_anchor = format!("{domain_vertex}:text");
            let seg_id = state.alloc_id();
            let mut seg_node = Node::new(seg_id, seg_anchor.as_str());
            seg_node.value = Some(FieldPresence::Present(Value::Str(text)));
            seg_node.shape = NodeShape::XmlTextSegment;
            state.nodes.insert(seg_id, seg_node);
            state.node_to_cst_value.insert(seg_id, segments);
            let edge = Edge {
                src: Name::from(domain_vertex),
                tgt: Name::from(seg_anchor.as_str()),
                kind: "prop".into(),
                name: Some(Name::from("$xml_text")),
            };
            state.arcs.push((node_id, seg_id, edge));
            Ok(())
        };

    for (kind, child_vertex) in content_children {
        match classify_content_kind(kind) {
            ContentClass::Text => text_run.push(child_vertex),
            ContentClass::Element => {
                flush_text(state, &mut text_run)?;
                let tag = extract_xml_tag_name(cst, child_vertex).unwrap_or_else(|| "child".into());
                let domain_edges = domain.0.outgoing_edges(domain_vertex);
                let domain_edge = domain_edges
                    .iter()
                    .find(|edge| {
                        let field_name = edge.name.as_deref().unwrap_or(&edge.tgt);
                        let is_item = *edge.kind == *"item" || edge.name.as_deref() == Some("item");
                        !is_item && field_name == tag
                    })
                    .or_else(|| {
                        domain_edges.iter().find(|edge| {
                            *edge.kind == *"item" || edge.name.as_deref() == Some("item")
                        })
                    });
                let child_anchor = domain_edge.map_or_else(
                    || Name::from(format!("{domain_vertex}:{tag}")),
                    |edge| edge.tgt.clone(),
                );
                let child_id = state.alloc_id();
                extract_xml_element(
                    cst,
                    domain,
                    child_vertex,
                    &child_anchor,
                    child_id,
                    state,
                    namespaces,
                )?;
                let edge = domain_edge.cloned().unwrap_or_else(|| Edge {
                    src: Name::from(domain_vertex),
                    tgt: child_anchor,
                    kind: "prop".into(),
                    name: Some(Name::from(tag.as_str())),
                });
                state.arcs.push((node_id, child_id, edge));
            }
            ContentClass::Opaque => {
                flush_text(state, &mut text_run)?;
                let seg_anchor = format!("{domain_vertex}:opaque");
                let seg_id = state.alloc_id();
                let mut seg_node = Node::new(seg_id, seg_anchor.as_str());
                seg_node.shape = NodeShape::XmlTextSegment;
                state.nodes.insert(seg_id, seg_node);
                state
                    .node_to_cst_struct
                    .insert(seg_id, child_vertex.clone());
                let edge = Edge {
                    src: Name::from(domain_vertex),
                    tgt: Name::from(seg_anchor.as_str()),
                    kind: "prop".into(),
                    name: Some(Name::from("$xml_opaque")),
                };
                state.arcs.push((node_id, seg_id, edge));
            }
        }
    }
    flush_text(state, &mut text_run)?;
    Ok(())
}

fn extract_xml_attributes(
    cst: &Schema,
    stag_vertex: &Name,
    node: &mut Node,
) -> Result<(), CstCodecError> {
    let mut seen = HashSet::new();
    for edge in cst.outgoing_edges(stag_vertex) {
        if cst_vertex_kind(cst, &edge.tgt).as_deref() == Some("Attribute") {
            if let (Some(attr_name), Some(attr_value)) = (
                extract_xml_attr_name(cst, &edge.tgt),
                extract_xml_attr_value(cst, &edge.tgt)?,
            ) {
                if !seen.insert(attr_name.clone()) {
                    return Err(CstCodecError::Structure(format!(
                        "duplicate XML attribute at {}",
                        cst_byte_offset(cst, &edge.tgt).map_or_else(
                            || "unknown byte".to_owned(),
                            |offset| format!("byte {offset}"),
                        )
                    )));
                }
                node.extra_fields.insert(attr_name, Value::Str(attr_value));
            }
        }
    }
    Ok(())
}

fn extract_xml_attr_name(cst: &Schema, attr_vertex: &Name) -> Option<String> {
    for edge in cst.outgoing_edges(attr_vertex) {
        if cst_vertex_kind(cst, &edge.tgt).as_deref() == Some("Name") {
            return literal_value(cst, &edge.tgt);
        }
    }
    None
}

fn extract_xml_attr_value(
    cst: &Schema,
    attr_vertex: &Name,
) -> Result<Option<String>, CstCodecError> {
    for edge in cst.outgoing_edges(attr_vertex) {
        if cst_vertex_kind(cst, &edge.tgt).as_deref() == Some("AttValue") {
            let raw = cst_source_text(cst, &edge.tgt);
            let trimmed = raw
                .strip_prefix('"')
                .and_then(|s| s.strip_suffix('"'))
                .or_else(|| raw.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')))
                .unwrap_or(&raw);
            let mut normalized = String::with_capacity(trimmed.len());
            let mut characters = trimmed.chars().peekable();
            while let Some(character) = characters.next() {
                match character {
                    '\r' => {
                        characters.next_if_eq(&'\n');
                        normalized.push(' ');
                    }
                    '\t' | '\n' => normalized.push(' '),
                    other => normalized.push(other),
                }
            }
            return decode_xml_text(&normalized).map(Some);
        }
    }
    Ok(None)
}

fn xml_text_run_value(cst: &Schema, segments: &[Name]) -> Result<Option<String>, CstCodecError> {
    if segments.is_empty() {
        Ok(None)
    } else {
        let parts = segments
            .iter()
            .map(|vertex| {
                let raw = cst_source_text(cst, vertex);
                if cst_vertex_kind(cst, vertex).as_deref() == Some("CDSect") {
                    let text = raw
                        .strip_prefix("<![CDATA[")
                        .and_then(|body| body.strip_suffix("]]>"))
                        .unwrap_or(&raw);
                    let text = normalize_xml_literal_eols(text);
                    if is_xml_10_text(&text) {
                        Ok(text)
                    } else {
                        Err(CstCodecError::Structure(
                            "XML CST contains a forbidden XML 1.0 character".into(),
                        ))
                    }
                } else {
                    decode_xml_text(&normalize_xml_literal_eols(&raw))
                }
            })
            .collect::<Result<Vec<_>, CstCodecError>>()?;
        Ok(Some(parts.concat()))
    }
}

fn normalize_xml_literal_eols(text: &str) -> String {
    let mut normalized = String::with_capacity(text.len());
    let mut characters = text.chars().peekable();
    while let Some(character) = characters.next() {
        if character == '\r' {
            characters.next_if_eq(&'\n');
            normalized.push('\n');
        } else {
            normalized.push(character);
        }
    }
    normalized
}

/// Decode XML character and predefined entity references without resolving
/// document-defined entities. Unknown named entities remain literal text,
/// matching the non-CST XML pathway and avoiding entity-expansion attacks.
fn decode_xml_text(raw: &str) -> Result<String, CstCodecError> {
    let mut decoded = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(amp) = rest.find('&') {
        decoded.push_str(&rest[..amp]);
        let reference = &rest[amp..];
        let Some(semi) = reference.find(';') else {
            decoded.push_str(reference);
            return if is_xml_10_text(&decoded) {
                Ok(decoded)
            } else {
                Err(CstCodecError::Structure(
                    "XML CST contains a forbidden XML 1.0 character".into(),
                ))
            };
        };
        let name = &reference[1..semi];
        let replacement = match (name.strip_prefix("#x"), name.strip_prefix('#')) {
            (Some(hex), _) => Some(
                u32::from_str_radix(hex, 16)
                    .ok()
                    .and_then(char::from_u32)
                    .filter(|character| {
                        let mut encoded = [0_u8; 4];
                        is_xml_10_text(character.encode_utf8(&mut encoded))
                    })
                    .ok_or_else(|| {
                        CstCodecError::Structure(
                            "XML CST contains an invalid numeric character reference".into(),
                        )
                    })?,
            ),
            (None, Some(decimal)) => Some(
                decimal
                    .parse::<u32>()
                    .ok()
                    .and_then(char::from_u32)
                    .filter(|character| {
                        let mut encoded = [0_u8; 4];
                        is_xml_10_text(character.encode_utf8(&mut encoded))
                    })
                    .ok_or_else(|| {
                        CstCodecError::Structure(
                            "XML CST contains an invalid numeric character reference".into(),
                        )
                    })?,
            ),
            (None, None) => quick_xml::escape::resolve_predefined_entity(name).and_then(|value| {
                let mut characters = value.chars();
                let first = characters.next()?;
                characters.next().is_none().then_some(first)
            }),
        };
        if let Some(character) = replacement {
            decoded.push(character);
        } else {
            decoded.push_str(&reference[..=semi]);
        }
        rest = &reference[semi + 1..];
    }
    decoded.push_str(rest);
    if is_xml_10_text(&decoded) {
        Ok(decoded)
    } else {
        Err(CstCodecError::Structure(
            "XML CST contains a forbidden XML 1.0 character".into(),
        ))
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
/// This source-compatible entry point uses an open protocol. Call
/// [`inject_xml_cst_with_protocol`] when protocol value constraints matter.
///
/// # Errors
///
/// Returns `CstExtractError` if the complement is invalid.
pub fn inject_xml_cst(
    instance: &WInstance,
    complement: &CstComplement,
    domain_schema: &Schema,
) -> Result<Schema, CstExtractError> {
    inject_xml_cst_with_protocol(instance, complement, domain_schema, &Protocol::default())
        .map_err(Into::into)
}

/// Inject an XML CST using the selected protocol's value constraints.
///
/// Updates `CharData` literal values for text content and `AttValue`
/// constraints for attributes stored in `node.extra_fields`.
///
/// # Errors
///
/// Returns `CstCodecError` if the complement is invalid.
pub fn inject_xml_cst_with_protocol(
    instance: &WInstance,
    complement: &CstComplement,
    domain_schema: &Schema,
    protocol: &Protocol,
) -> Result<Schema, CstCodecError> {
    validate_tree_structure(instance, complement, domain_schema, protocol, "xml")?;
    let mut cst = complement.cst_schema.clone();

    for (&node_id, node) in &instance.nodes {
        let mut replaced_whole_element = false;
        let site = ValueSite::new(node.anchor.clone(), None);
        let namespaces = xml_instance_namespaces(instance, node_id);
        validate_xml_node_qnames(node, &namespaces)
            .map_err(|detail| unsupported_structural_edit("xml", detail))?;
        let existing_nil = validate_xml_nil_fields(node, &namespaces, &site)?;
        // Update text content via node_to_cst_value mapping
        if let Some(ref presence) = node.value {
            if node.is_xml_text_segment() && matches!(presence, FieldPresence::Null) {
                return Err(EmitValueError::new(
                    site.clone(),
                    "null",
                    "xml",
                    EmitValueReason::NullNotRepresentable,
                )
                .into());
            }
            if matches!(presence, FieldPresence::Present(Value::Null)) {
                return Err(EmitValueError::new(
                    site.clone(),
                    "null",
                    "xml",
                    EmitValueReason::NotRepresentable,
                )
                .into());
            }
            let spec = ValueSpec::for_vertex(domain_schema, protocol, &node.anchor);
            if let Some(segments) = complement.node_to_cst_value.get(&node_id) {
                if !xml_scalar_unchanged(&cst, segments, presence, &spec) {
                    let new_text = field_presence_to_text(presence, &spec, &site)?;
                    validate_xml_edit_text(&new_text, presence, &site)?;
                    set_xml_text_run(&mut cst, segments, &new_text);
                }
            } else if !matches!(presence, FieldPresence::Null) {
                if !instance.children(node_id).is_empty() {
                    return Err(unsupported_structural_edit(
                        "xml",
                        format!(
                            "node {node_id} has no text carrier and has child elements, so its value cannot be replaced"
                        ),
                    ));
                }
                let cst_struct = complement.node_to_cst_struct.get(&node_id).ok_or_else(|| {
                    unsupported_structural_edit(
                        "xml",
                        format!("node {node_id} has no recorded element carrier"),
                    )
                })?;
                if !node.extra_fields.is_empty() {
                    inject_xml_attributes(&mut cst, cst_struct, &node.extra_fields, &site)?;
                }
                // A self-closing `xsi:nil` element has no character-data
                // carrier. When it is edited to a concrete scalar, replace
                // the element's complete source span with a start tag, the
                // encoded value, and an end tag. Updating only `xsi:nil`
                // would otherwise reparse as an empty value and lose the
                // edit. The parent-span literal masks the old empty-tag CST
                // children during replay.
                let replacement =
                    xml_scalar_element_text(&cst, cst_struct, node, presence, &spec, &site)?;
                set_toml_token_text(&mut cst, cst_struct, &replacement);
                replaced_whole_element = true;
            }
        }

        // Update attributes: find the STag in the CST structural node
        // and update AttValue constraints for modified extra_fields.
        if replaced_whole_element {
            continue;
        }
        if !node.extra_fields.is_empty() {
            if let Some(cst_struct) = complement.node_to_cst_struct.get(&node_id) {
                let null = matches!(node.value, Some(FieldPresence::Null));
                inject_xml_attributes(&mut cst, cst_struct, &node.extra_fields, &site)?;
                if null && existing_nil.is_none() {
                    insert_xml_nil_attribute(&mut cst, cst_struct, &namespaces)?;
                }
            }
        } else if matches!(node.value, Some(FieldPresence::Null))
            && let Some(cst_struct) = complement.node_to_cst_struct.get(&node_id)
        {
            insert_xml_nil_attribute(&mut cst, cst_struct, &namespaces)?;
        }
    }

    Ok(cst)
}

fn validate_xml_node_qnames(
    node: &Node,
    namespaces: &HashMap<String, String>,
) -> Result<(), String> {
    if let Some(tag) = node.xml_tag() {
        if !is_xml_qname(tag) {
            return Err(format!("element has invalid QName {tag:?}"));
        }
        if !xml_qname_is_bound(tag, namespaces) {
            return Err(format!(
                "element QName {tag:?} uses an unbound or reserved namespace prefix"
            ));
        }
    }
    for (key, value) in &node.extra_fields {
        if !is_xml_qname(key) {
            return Err(format!("attribute has invalid QName {key:?}"));
        }
        if is_xml_namespace_declaration(key) {
            let Value::Str(uri) = value else {
                return Err(format!("namespace declaration {key:?} is not a string"));
            };
            if !is_valid_xml_namespace_declaration(key, uri) {
                return Err(format!("invalid XML namespace declaration {key:?}"));
            }
        } else if !xml_qname_is_bound(key, namespaces) {
            return Err(format!(
                "attribute QName {key:?} uses an unbound or reserved namespace prefix"
            ));
        }
    }
    Ok(())
}

fn validate_xml_nil_fields(
    node: &Node,
    namespaces: &HashMap<String, String>,
    site: &ValueSite,
) -> Result<Option<String>, CstCodecError> {
    let nil_like: Vec<_> = node
        .extra_fields
        .keys()
        .filter(|key| {
            key.rsplit_once(':')
                .is_some_and(|(_, local)| local == "nil")
        })
        .collect();
    let semantic: Vec<_> = nil_like
        .iter()
        .filter(|key| {
            key.rsplit_once(':').is_some_and(|(prefix, _)| {
                namespaces
                    .get(prefix)
                    .is_some_and(|uri| uri == XML_SCHEMA_INSTANCE_NS)
            })
        })
        .collect();
    let null = matches!(node.value, Some(FieldPresence::Null));
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
    let value = node.extra_fields.get(key.as_str());
    let consistent = matches!(
        value,
        Some(Value::Str(text)) if crate::xml_pathway::xsd_boolean(text) == Some(null)
    );
    if !consistent {
        return Err(EmitValueError::new(
            site.clone(),
            value.map_or("str", Value::type_name),
            "xml",
            EmitValueReason::NotRepresentable,
        )
        .into());
    }
    Ok(Some(key.as_str().to_owned()))
}

fn xml_scalar_element_text(
    cst: &Schema,
    cst_vertex: &Name,
    node: &Node,
    presence: &FieldPresence,
    spec: &ValueSpec,
    site: &ValueSite,
) -> Result<String, CstCodecError> {
    let tag = node
        .xml_tag()
        .map(ToString::to_string)
        .or_else(|| extract_xml_tag_name(cst, cst_vertex))
        .ok_or_else(|| {
            CstCodecError::Structure(format!(
                "XML element {cst_vertex} has no tag name for scalar replacement"
            ))
        })?;
    if !is_xml_qname(&tag) {
        return Err(unsupported_structural_edit(
            "xml",
            format!("element has invalid QName {tag:?}"),
        ));
    }
    let text = field_presence_to_text(presence, spec, site)?;
    validate_xml_edit_text(&text, presence, site)?;
    let text = escape_xml_edit_text(&text, false);
    let source = cst_source_text(cst, cst_vertex);
    if let Some(start_tag) = source.strip_suffix("/>") {
        return Ok(format!("{start_tag}>{text}</{tag}>"));
    }
    let Some(end_tag_offset) = source.rfind("</") else {
        return Err(unsupported_structural_edit(
            "xml",
            format!("element {cst_vertex} has no exact insertion point for character data"),
        ));
    };
    Ok(format!(
        "{}{text}{}",
        &source[..end_tag_offset],
        &source[end_tag_offset..]
    ))
}

fn validate_xml_edit_text(
    text: &str,
    presence: &FieldPresence,
    site: &ValueSite,
) -> Result<(), CstCodecError> {
    let value_type = match presence {
        FieldPresence::Present(value) => value.type_name(),
        FieldPresence::Null => "null",
        FieldPresence::Absent => "absent",
    };
    validate_xml_edit_value(text, value_type, site)
}

fn validate_xml_edit_value(
    text: &str,
    value_type: &str,
    site: &ValueSite,
) -> Result<(), CstCodecError> {
    if is_xml_10_text(text) {
        Ok(())
    } else {
        Err(EmitValueError::new(
            site.clone(),
            value_type,
            "xml",
            EmitValueReason::NotRepresentable,
        )
        .into())
    }
}

fn escape_xml_edit_text(text: &str, attribute: bool) -> String {
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

/// Whether an XML text run still denotes the instance value. This equality
/// guard preserves the source spelling of unchanged references such as
/// `&amp;` and `&#38;`; an actual edit is still spliced across the full run.
fn xml_scalar_unchanged(
    cst: &Schema,
    segments: &[Name],
    presence: &FieldPresence,
    spec: &ValueSpec,
) -> bool {
    let Ok(Some(text)) = xml_text_run_value(cst, segments) else {
        return false;
    };
    let original =
        match crate::json_pathway::coerce_exact_lexeme(&Lexeme::new(&text, LexClass::String), spec)
        {
            Ok(Value::Null) => FieldPresence::Null,
            Ok(value) => FieldPresence::Present(value),
            Err(_) => return false,
        };
    match original {
        FieldPresence::Present(value) => presence_matches_value(presence, &value),
        FieldPresence::Null => matches!(presence, FieldPresence::Null),
        FieldPresence::Absent => matches!(presence, FieldPresence::Absent),
    }
}

/// Update XML attribute values in the CST from node `extra_fields`.
fn inject_xml_attributes(
    cst: &mut Schema,
    element_vertex: &Name,
    extra_fields: &HashMap<String, Value>,
    site: &ValueSite,
) -> Result<(), CstCodecError> {
    let rendered: BTreeMap<&str, String> = extra_fields
        .iter()
        .map(|(key, value)| {
            if !is_xml_qname(key) {
                return Err(unsupported_structural_edit(
                    "xml",
                    format!("attribute has invalid QName {key:?}"),
                ));
            }
            let Value::Str(text) = value else {
                return Err(EmitValueError::new(
                    site.clone(),
                    value.type_name(),
                    "xml",
                    EmitValueReason::NotRepresentable,
                )
                .into());
            };
            validate_xml_edit_value(text, value.type_name(), site)?;
            Ok((key.as_str(), text.clone()))
        })
        .collect::<Result<_, CstCodecError>>()?;
    // Find the STag child of this element
    let stag = cst
        .outgoing_edges(element_vertex)
        .iter()
        .find(|e| {
            cst.vertex(&e.tgt)
                .is_some_and(|v| *v.kind == *"STag" || *v.kind == *"EmptyElemTag")
        })
        .map(|e| e.tgt.clone());

    let Some(stag_name) = stag else { return Ok(()) };

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
            if let Some(text) = rendered.get(name.as_str()) {
                // When the attribute value is unchanged, leave the original
                // AttValue token untouched: it carries lexical details the
                // re-serialization discards (single- vs double-quote style,
                // and entity-encoded characters such as `&lt;`/`&amp;`).
                if extract_xml_attr_value(cst, attr_vertex)
                    .ok()
                    .flatten()
                    .as_deref()
                    == Some(text.as_str())
                {
                    continue;
                }
                // Find and update the AttValue child
                let attvalue_vertex = cst
                    .outgoing_edges(attr_vertex)
                    .iter()
                    .find(|e| cst.vertex(&e.tgt).is_some_and(|v| *v.kind == *"AttValue"))
                    .map(|e| e.tgt.clone());
                if let Some(av) = attvalue_vertex {
                    // The AttValue literal includes quotes; update the inner text
                    let quoted = format!("\"{}\"", escape_xml_edit_text(text, true));
                    set_toml_token_text(&mut *cst, &av, &quoted);
                }
            }
        }
    }
    Ok(())
}

fn xml_instance_namespaces(instance: &WInstance, node_id: u32) -> HashMap<String, String> {
    let mut lineage = vec![node_id];
    let mut current = node_id;
    while let Some((parent, _, _)) = instance.arcs.iter().find(|(_, child, _)| *child == current) {
        lineage.push(*parent);
        current = *parent;
    }
    lineage.reverse();

    let mut namespaces = HashMap::new();
    for id in lineage {
        let Some(node) = instance.node(id) else {
            continue;
        };
        for (key, value) in &node.extra_fields {
            let Value::Str(uri) = value else { continue };
            if key == "xmlns" {
                namespaces.insert(String::new(), uri.clone());
            } else if let Some(prefix) = key.strip_prefix("xmlns:") {
                namespaces.insert(prefix.to_owned(), uri.clone());
            }
        }
    }
    namespaces
}

fn insert_xml_nil_attribute(
    cst: &mut Schema,
    element_vertex: &Name,
    namespaces: &HashMap<String, String>,
) -> Result<(), CstCodecError> {
    let stag = cst
        .outgoing_edges(element_vertex)
        .iter()
        .find(|edge| {
            cst.vertex(&edge.tgt)
                .is_some_and(|vertex| matches!(vertex.kind.as_ref(), "STag" | "EmptyElemTag"))
        })
        .map(|edge| edge.tgt.clone())
        .ok_or_else(|| {
            CstCodecError::Structure(format!(
                "XML element {element_vertex:?} has no start tag for xsi:nil"
            ))
        })?;
    let raw = cst_source_text(cst, &stag);
    let insertion = raw
        .rfind("/>")
        .or_else(|| raw.rfind('>'))
        .ok_or_else(|| CstCodecError::Structure("XML start tag has no closing delimiter".into()))?;
    let existing_prefix = namespaces
        .iter()
        .filter(|(prefix, uri)| !prefix.is_empty() && uri.as_str() == XML_SCHEMA_INSTANCE_NS)
        .map(|(prefix, _)| prefix.clone())
        .min();
    let (prefix, declare) = existing_prefix.map_or_else(
        || {
            let mut suffix = 0_u32;
            let prefix = loop {
                let candidate = if suffix == 0 {
                    "xsi".to_owned()
                } else {
                    format!("xsi{suffix}")
                };
                if !namespaces.contains_key(&candidate) {
                    break candidate;
                }
                suffix += 1;
            };
            (prefix, true)
        },
        |prefix| (prefix, false),
    );
    let addition = if declare {
        format!(" xmlns:{prefix}=\"{XML_SCHEMA_INSTANCE_NS}\" {prefix}:nil=\"true\"")
    } else {
        format!(" {prefix}:nil=\"true\"")
    };
    let mut updated = raw;
    updated.insert_str(insertion, &addition);
    set_toml_token_text(cst, &stag, &updated);
    Ok(())
}

/// Convert a `FieldPresence` to plain text (for XML text content).
fn field_presence_to_text(
    presence: &FieldPresence,
    spec: &ValueSpec,
    site: &ValueSite,
) -> Result<String, CstCodecError> {
    if matches!(presence, FieldPresence::Present(Value::Null)) {
        return Err(EmitValueError::new(
            site.clone(),
            "null",
            "xml",
            EmitValueReason::NotRepresentable,
        )
        .into());
    }
    let text = match presence {
        FieldPresence::Present(value)
            if !crate::json_pathway::plain_text_value_roundtrips(value, spec) =>
        {
            return Err(EmitValueError::new(
                site.clone(),
                value.type_name(),
                "xml",
                EmitValueReason::NotRepresentable,
            )
            .into());
        }
        FieldPresence::Present(Value::Str(s) | Value::Token(s)) => s.clone(),
        FieldPresence::Present(Value::Int(i)) => i.to_string(),
        FieldPresence::Present(value @ Value::Float(number)) => {
            crate::json_pathway::plain_text_float_lexeme(*number).ok_or_else(|| {
                EmitValueError::new(
                    site.clone(),
                    value.type_name(),
                    "xml",
                    EmitValueReason::NonFinite,
                )
            })?
        }
        FieldPresence::Present(Value::Bool(b)) => b.to_string(),
        FieldPresence::Present(Value::Bytes(bytes)) => {
            let ScalarKind::Bytes(encoding) = &spec.kind else {
                return Err(EmitValueError::new(
                    site.clone(),
                    "bytes",
                    "xml",
                    EmitValueReason::NotRepresentable,
                )
                .into());
            };
            crate::json_pathway::encode_bytes(bytes, encoding).ok_or_else(|| {
                EmitValueError::new(
                    site.clone(),
                    "bytes",
                    "xml",
                    EmitValueReason::NotRepresentable,
                )
            })?
        }
        FieldPresence::Present(Value::Decimal(value)) => value.to_string(),
        FieldPresence::Present(Value::BigInt(value)) => value.to_string(),
        FieldPresence::Present(Value::Temporal(value)) => value.to_string(),
        FieldPresence::Null | FieldPresence::Present(Value::Null) => String::new(),
        FieldPresence::Absent => {
            return Err(EmitValueError::new(
                site.clone(),
                "absent",
                "xml",
                EmitValueReason::AbsentNotRepresentable,
            )
            .into());
        }
        FieldPresence::Present(other) => {
            return Err(EmitValueError::new(
                site.clone(),
                other.type_name(),
                "xml",
                EmitValueReason::NotRepresentable,
            )
            .into());
        }
    };
    if let FieldPresence::Present(value) = presence
        && text.is_empty()
    {
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

// ── YAML extraction ───────────────────────────────────────────────────

/// Extract a `WInstance` from a YAML CST Schema, guided by a domain schema.
///
/// This source-compatible entry point uses an open protocol. Call
/// [`extract_yaml_cst_with_protocol`] when protocol value constraints matter.
///
/// # Errors
///
/// Returns `CstExtractError` if the CST is invalid.
pub fn extract_yaml_cst(
    cst: &Schema,
    domain_schema: &Schema,
    root_vertex: &str,
) -> Result<(WInstance, CstComplement), CstExtractError> {
    extract_yaml_cst_with_protocol(cst, domain_schema, &Protocol::default(), root_vertex)
        .map_err(Into::into)
}

/// Extract a YAML CST using the selected protocol's value constraints.
///
/// # Errors
///
/// Returns `CstCodecError` if the CST is invalid.
pub fn extract_yaml_cst_with_protocol(
    cst: &Schema,
    domain_schema: &Schema,
    protocol: &Protocol,
    root_vertex: &str,
) -> Result<(WInstance, CstComplement), CstCodecError> {
    validate_cst_has_no_recovery_markers(cst, "YAML")?;
    if cst.vertices.values().any(|vertex| *vertex.kind == *"alias") {
        return Err(CstCodecError::Structure(
            "YAML aliases require anchor resolution and are not supported".into(),
        ));
    }
    if !domain_schema.has_vertex(root_vertex) {
        return Err(CstCodecError::SchemaMismatch(format!(
            "root vertex '{root_vertex}' not found in domain schema"
        )));
    }

    let mut state = ExtractState::new();
    let doc_vertex = find_unique_yaml_document(cst)?;

    let top_value = find_yaml_top_value(cst, &doc_vertex)
        .ok_or_else(|| CstCodecError::Structure("no value node in YAML document".into()))?;

    let root_id = state.alloc_id();
    extract_yaml_value(
        cst,
        domain_schema,
        protocol,
        &top_value,
        root_vertex,
        root_id,
        &mut state,
    )?;

    finish_fidelity(&mut state)?;
    let complement = CstComplement {
        format: "yaml".into(),
        cst_schema: cst.clone(),
        node_to_cst_value: state.node_to_cst_value,
        node_to_cst_struct: state.node_to_cst_struct,
        cell_to_cst_value: HashMap::new(),
        source_vertex: Some(Name::from(root_vertex)),
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
            "flow_node"
            | "block_node"
            | "block_mapping"
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

/// Descend through the `flow_node` / `block_node` wrappers the YAML grammar
/// interposes between a container and its content.
///
/// Every mapping value, sequence element, and document body reaches the CST
/// wrapped in one of these, so a dispatch on the wrapper's own kind sees
/// neither a mapping nor a sequence nor a scalar and falls through to the
/// scalar branch with no text.
fn unwrap_yaml_node(cst: &Schema, vertex: &Name) -> Name {
    let mut current = vertex.clone();
    for _ in 0..YAML_WRAPPER_DEPTH {
        match cst_vertex_kind(cst, &current).as_deref() {
            Some("flow_node" | "block_node") => {
                let children = cst_children_by_edge_kind(cst, &current, "child_of");
                let semantic = children.iter().find(|child| {
                    cst_vertex_kind(cst, child)
                        .as_deref()
                        .is_some_and(is_yaml_value_kind)
                });
                match semantic {
                    Some(child) => current = (**child).clone(),
                    None => return current,
                }
            }
            _ => return current,
        }
    }
    current
}

fn reject_tagged_yaml_container(cst: &Schema, vertex: &Name) -> Result<(), CstCodecError> {
    let mut current = vertex.clone();
    for _ in 0..YAML_WRAPPER_DEPTH {
        if !matches!(
            cst_vertex_kind(cst, &current).as_deref(),
            Some("flow_node" | "block_node")
        ) {
            return Ok(());
        }
        let children = cst_children_by_edge_kind(cst, &current, "child_of");
        let semantic = children.iter().find(|child| {
            cst_vertex_kind(cst, child)
                .as_deref()
                .is_some_and(is_yaml_value_kind)
        });
        let Some(semantic) = semantic else {
            return Ok(());
        };
        let semantic_kind = cst_vertex_kind(cst, semantic).unwrap_or_default();
        let container_kind = match semantic_kind.as_str() {
            "block_mapping" | "flow_mapping" => Some("mapping"),
            "block_sequence" | "flow_sequence" => Some("sequence"),
            _ => None,
        };
        if let Some(container_kind) = container_kind
            && let Some(tag_vertex) = children
                .iter()
                .find(|child| cst_source_text(cst, child).trim_start().starts_with('!'))
        {
            return Err(CstCodecError::UnsupportedTaggedContainer {
                tag: cst_source_text(cst, tag_vertex).trim().to_owned(),
                container_kind: container_kind.to_owned(),
                byte_offset: cst_byte_offset(cst, tag_vertex),
            });
        }
        current = (**semantic).clone();
    }
    Ok(())
}

/// Choose the existing literal that covers the complete scalar expression.
///
/// YAML wraps scalar leaves in `flow_node` / `block_node` vertices. Adding a
/// second literal to such a wrapper creates two equal-span, equal-rank replay
/// fragments (the new wrapper literal and the old child literal), so whichever
/// hash-map entry happens to be visited first wins. Updating the already
/// captured full-span literal removes that nondeterminism. Tagged expressions
/// without one fall back to the wrapper: its newly added literal is then wider
/// than every descendant fragment and deterministically masks them.
fn yaml_value_carrier(cst: &Schema, source: &Name) -> Name {
    let source_span = cst_recorded_span(cst, source);
    let mut pending = vec![source.clone()];
    let mut seen = HashSet::new();
    while let Some(vertex) = pending.pop() {
        if !seen.insert(vertex.clone()) {
            continue;
        }
        if literal_value(cst, &vertex).is_some()
            && source_span.is_some()
            && cst_recorded_span(cst, &vertex) == source_span
        {
            return vertex;
        }
        let mut children = cst_children_by_edge_kind(cst, &vertex, "child_of");
        children.reverse();
        pending.extend(children.into_iter().cloned());
    }
    source.clone()
}

fn cst_recorded_span(cst: &Schema, vertex: &str) -> Option<(usize, usize)> {
    let constraints = cst.constraints.get(vertex)?;
    let byte = |sort: &str| {
        constraints
            .iter()
            .find(|constraint| constraint.sort.as_ref() == sort)?
            .value
            .parse::<usize>()
            .ok()
    };
    Some((byte("start-byte")?, byte("end-byte")?))
}

/// How many `flow_node` / `block_node` wrappers [`unwrap_yaml_node`] will
/// descend through before giving up. The grammar nests at most a couple, so
/// this only bounds a cycle in a malformed CST.
const YAML_WRAPPER_DEPTH: usize = 8;

fn is_yaml_value_kind(kind: &str) -> bool {
    matches!(
        kind,
        "flow_node"
            | "block_node"
            | "block_mapping"
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
            | "null_scalar"
    )
}

/// Extract a YAML value without schema guidance, for a domain vertex that
/// declares no outgoing edges.
///
/// Mirrors [`extract_json_value_open`]: containers become nodes with one
/// synthesised edge per child, so the shape of an open-schema YAML document
/// survives into the instance instead of collapsing to a bare root.
/// One level deeper than the caller, and no deeper than
/// [`MAX_EXTRACT_DEPTH`].
fn extract_yaml_value_open(
    cst: &Schema,
    cst_vertex: &Name,
    anchor: &str,
    node_id: u32,
    state: &mut ExtractState,
) -> Result<(), CstCodecError> {
    reject_tagged_yaml_container(cst, cst_vertex)?;
    state.enter()?;
    let source_vertex = cst_vertex.clone();
    let semantic_vertex = unwrap_yaml_node(cst, cst_vertex);
    let cst_vertex = &semantic_vertex;
    let kind = cst_vertex_kind(cst, cst_vertex).unwrap_or_default();
    state.node_to_cst_struct.insert(node_id, cst_vertex.clone());

    let extracted = match kind.as_str() {
        "block_mapping" | "flow_mapping" => {
            extract_yaml_mapping_open(cst, cst_vertex, anchor, node_id, state)
        }
        "block_sequence" | "flow_sequence" => {
            extract_yaml_sequence_open(cst, cst_vertex, anchor, node_id, state)
        }
        _ => {
            let (text, class) = yaml_scalar_lexeme(cst, &source_vertex, cst_vertex);
            let carrier = yaml_value_carrier(cst, &source_vertex);
            state.node_to_cst_value.insert(node_id, vec![carrier]);
            let presence = match coerce_yaml_lexeme(&text, class, &ValueSpec::new(ScalarKind::Any))
            {
                Ok(Value::Null) => FieldPresence::Null,
                Ok(value) => FieldPresence::Present(value),
                Err(error) => {
                    state
                        .fidelity
                        .push(error.at(ValueSite::new(anchor, cst_byte_offset(cst, cst_vertex))));
                    FieldPresence::Present(Value::Null)
                }
            };
            let node = Node::new(node_id, anchor).with_value(presence);
            state.nodes.insert(node_id, node);
            Ok(())
        }
    };
    state.leave();
    extracted
}

/// Extract a YAML mapping with no schema guidance, synthesising one `prop`
/// edge per pair.
fn extract_yaml_mapping_open(
    cst: &Schema,
    cst_vertex: &Name,
    anchor: &str,
    node_id: u32,
    state: &mut ExtractState,
) -> Result<(), CstCodecError> {
    state.nodes.insert(node_id, Node::new(node_id, anchor));
    for (pair_name, key) in decoded_yaml_mapping_pairs(cst, cst_vertex)? {
        let child_anchor = format!("{anchor}:{key}");
        let child_id = state.alloc_id();
        if let Some(value_vertex) = find_yaml_pair_value(cst, &pair_name) {
            extract_yaml_value_open(cst, &value_vertex, &child_anchor, child_id, state)?;
        } else {
            insert_empty_yaml_pair_node(&pair_name, &child_anchor, child_id, state);
        }
        let synth_edge = Edge {
            src: Name::from(anchor),
            tgt: Name::from(child_anchor.as_str()),
            kind: "prop".into(),
            name: Some(Name::from(key.as_str())),
        };
        state.arcs.push((node_id, child_id, synth_edge));
    }
    Ok(())
}

/// Extract a YAML sequence with no schema guidance, synthesising one `item`
/// edge per element.
fn extract_yaml_sequence_open(
    cst: &Schema,
    cst_vertex: &Name,
    anchor: &str,
    node_id: u32,
    state: &mut ExtractState,
) -> Result<(), CstCodecError> {
    let mut node = Node::new(node_id, anchor);
    node.shape = NodeShape::List;
    state.nodes.insert(node_id, node);
    for item_name in &cst_children_by_edge_kind(cst, cst_vertex, "child_of") {
        let child_anchor = format!("{anchor}:items");
        let child_id = state.alloc_id();
        if let Some(value_vertex) = find_yaml_sequence_item_value(cst, item_name) {
            extract_yaml_value_open(cst, &value_vertex, &child_anchor, child_id, state)?;
        } else {
            insert_empty_yaml_sequence_node(item_name, &child_anchor, child_id, state);
        }
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

fn insert_empty_yaml_sequence_node(
    item: &Name,
    anchor: &str,
    node_id: u32,
    state: &mut ExtractState,
) {
    state.nodes.insert(
        node_id,
        Node::new(node_id, anchor).with_value(FieldPresence::Null),
    );
    state.node_to_cst_struct.insert(node_id, item.clone());
    state.node_to_cst_value.insert(node_id, vec![item.clone()]);
}

/// Decode one YAML scalar while retaining an exact numeric lexeme.
///
/// Quoted and block scalars are semantic strings, not their source tokens.
/// Explicit tags similarly classify the decoded payload. Numbers are the one
/// exception: their source token remains the carrier so the YAML library's
/// bounded integer and binary-float representation cannot round or reject an
/// exact numeral before [`coerce`] sees it.
fn yaml_scalar_lexeme(
    cst: &Schema,
    source_vertex: &Name,
    scalar_vertex: &Name,
) -> (String, LexClass) {
    let raw = cst_source_text(cst, source_vertex);
    let fallback = literal_text_deep(cst, scalar_vertex);
    let class = yaml_scalar_class(cst, scalar_vertex).unwrap_or(LexClass::Null);
    yaml_source_lexeme(&raw, &fallback, class)
}

fn yaml_source_lexeme(raw: &str, fallback: &str, class: LexClass) -> (String, LexClass) {
    let compact_fallback = fallback.trim_start_matches(['+', '-']).replace('_', "");
    let compact_raw = raw.trim().trim_start_matches(['+', '-']).replace('_', "");
    if class == LexClass::Number
        && (compact_fallback.starts_with("0b") || compact_raw.starts_with("0b"))
    {
        return (raw.trim().to_owned(), LexClass::String);
    }
    if let Some((tag, payload)) = yaml_explicit_tag(raw) {
        let text = yaml_serde::from_str::<yaml_serde::Value>(payload).map_or_else(
            |_| normalize_yaml_number(payload, &class),
            |value| yaml_scalar_value_text(value, payload, &class),
        );
        return (text, LexClass::Tagged(Name::from(tag)));
    }
    let Ok(value) = yaml_serde::from_str::<yaml_serde::Value>(raw) else {
        return (normalize_yaml_number(fallback, &class), class);
    };
    yaml_value_lexeme(value, raw, fallback, class)
}

fn yaml_value_lexeme(
    value: yaml_serde::Value,
    raw: &str,
    fallback: &str,
    fallback_class: LexClass,
) -> (String, LexClass) {
    match value {
        yaml_serde::Value::Null => ("null".to_owned(), LexClass::Null),
        yaml_serde::Value::Bool(value) => (value.to_string(), LexClass::Bool),
        yaml_serde::Value::Number(_) => (
            normalize_yaml_number(fallback, &LexClass::Number),
            LexClass::Number,
        ),
        yaml_serde::Value::String(value) if fallback_class == LexClass::Number => {
            let compact = fallback.trim_start_matches(['+', '-']).replace('_', "");
            if compact.starts_with("0b") {
                (value, LexClass::String)
            } else {
                (
                    normalize_yaml_number(fallback, &fallback_class),
                    fallback_class,
                )
            }
        }
        yaml_serde::Value::String(value) => (value, LexClass::String),
        yaml_serde::Value::Tagged(tagged) => {
            let tag = tagged.tag.to_string();
            let tag = tag.strip_prefix('!').unwrap_or(&tag);
            let payload = yaml_tag_payload(raw);
            let text = yaml_scalar_value_text(tagged.value, payload, &fallback_class);
            (text, LexClass::Tagged(Name::from(tag)))
        }
        yaml_serde::Value::Sequence(_) | yaml_serde::Value::Mapping(_) => (
            normalize_yaml_number(fallback, &fallback_class),
            fallback_class,
        ),
    }
}

fn yaml_scalar_value_text(value: yaml_serde::Value, source: &str, class: &LexClass) -> String {
    match value {
        yaml_serde::Value::Null => "null".to_owned(),
        yaml_serde::Value::Bool(value) => value.to_string(),
        yaml_serde::Value::Number(_) => normalize_yaml_number(source, &LexClass::Number),
        yaml_serde::Value::String(value) => value,
        yaml_serde::Value::Tagged(tagged) => yaml_scalar_value_text(tagged.value, source, class),
        yaml_serde::Value::Sequence(_) | yaml_serde::Value::Mapping(_) => {
            normalize_yaml_number(source, class)
        }
    }
}

fn normalize_yaml_number(text: &str, class: &LexClass) -> String {
    if *class == LexClass::Number {
        normalize_radix_integer(text, false)
            .unwrap_or_else(|| text.chars().filter(|character| *character != '_').collect())
    } else {
        text.to_owned()
    }
}

fn yaml_explicit_tag(raw: &str) -> Option<(&str, &str)> {
    let raw = raw.trim_start();
    if !raw.starts_with('!') {
        return None;
    }
    if let Some(after) = raw.strip_prefix("!<") {
        let end = after.find('>')?;
        return Some((&after[..end], after[end + 1..].trim_start()));
    }
    let split = raw.find(char::is_whitespace).unwrap_or(raw.len());
    let tag = &raw[..split];
    let payload = raw[split..].trim_start();
    (!payload.is_empty()).then_some((tag, payload))
}

fn yaml_tag_payload(raw: &str) -> &str {
    let raw = raw.trim_start();
    if let Some(after) = raw.strip_prefix("!<")
        && let Some(end) = after.find('>')
    {
        return after[end + 1..].trim_start();
    }
    raw.find(char::is_whitespace)
        .map_or(raw, |split| raw[split..].trim_start())
}

/// Recursively extract a YAML value from the CST into a `WInstance` node,
/// one level deeper than the caller and no deeper than
/// [`MAX_EXTRACT_DEPTH`].
fn extract_yaml_value(
    cst: &Schema,
    domain_schema: &Schema,
    protocol: &Protocol,
    cst_vertex: &Name,
    domain_vertex: &str,
    node_id: u32,
    state: &mut ExtractState,
) -> Result<(), CstCodecError> {
    reject_tagged_yaml_container(cst, cst_vertex)?;
    state.enter()?;
    let source_vertex = cst_vertex.clone();
    let semantic_vertex = unwrap_yaml_node(cst, cst_vertex);
    let cst_vertex = &semantic_vertex;
    let kind = cst_vertex_kind(cst, cst_vertex).unwrap_or_default();
    state.node_to_cst_struct.insert(node_id, cst_vertex.clone());

    let extracted = match kind.as_str() {
        "block_mapping" | "flow_mapping" => extract_yaml_mapping(
            cst,
            domain_schema,
            protocol,
            cst_vertex,
            domain_vertex,
            node_id,
            state,
        ),
        "block_sequence" | "flow_sequence" => extract_yaml_sequence(
            cst,
            domain_schema,
            protocol,
            cst_vertex,
            domain_vertex,
            node_id,
            state,
        ),
        _ => {
            let (text, class) = yaml_scalar_lexeme(cst, &source_vertex, cst_vertex);
            let carrier = yaml_value_carrier(cst, &source_vertex);
            state.node_to_cst_value.insert(node_id, vec![carrier]);
            let presence = coerce_yaml_cst_scalar(
                cst,
                cst_vertex,
                domain_schema,
                protocol,
                domain_vertex,
                &Lexeme::new(&text, class),
                state,
            );
            let node = Node::new(node_id, domain_vertex).with_value(presence);
            state.nodes.insert(node_id, node);
            Ok(())
        }
    };
    state.leave();
    extracted
}

fn extract_yaml_mapping(
    cst: &Schema,
    domain_schema: &Schema,
    protocol: &Protocol,
    cst_vertex: &Name,
    domain_vertex: &str,
    node_id: u32,
    state: &mut ExtractState,
) -> Result<(), CstCodecError> {
    let domain_edges: Vec<Edge> = domain_schema.outgoing_edges(domain_vertex).to_vec();

    if domain_edges.is_empty() {
        // Open schema: every pair becomes a child node under a synthesised
        // `prop` edge. Folding the pairs into `extra_fields` instead left the
        // mapping with no child nodes at all, so nothing downstream could
        // address — let alone edit — a value in an open-schema document.
        return extract_yaml_mapping_open(cst, cst_vertex, domain_vertex, node_id, state);
    }

    let mut node = Node::new(node_id, domain_vertex);
    let mut handled_keys = std::collections::HashSet::new();
    let decoded_pairs = decoded_yaml_mapping_pairs(cst, cst_vertex)?;

    for domain_edge in &domain_edges {
        let field_name = domain_edge.name.as_deref().unwrap_or(&domain_edge.tgt);
        handled_keys.insert(field_name.to_string());

        for (pair_name, key) in &decoded_pairs {
            if key == field_name {
                let child_id = state.alloc_id();
                if let Some(value_vertex) = find_yaml_pair_value(cst, pair_name) {
                    extract_yaml_value(
                        cst,
                        domain_schema,
                        protocol,
                        &value_vertex,
                        &domain_edge.tgt,
                        child_id,
                        state,
                    )?;
                } else {
                    insert_empty_yaml_pair_node(pair_name, &domain_edge.tgt, child_id, state);
                }
                state.arcs.push((node_id, child_id, domain_edge.clone()));
            }
        }
    }

    for (pair_name, key_text) in decoded_pairs {
        if handled_keys.contains(&key_text) {
            continue;
        }
        if let Some(value_vertex) = find_yaml_pair_value(cst, &pair_name) {
            let val = extract_yaml_generic_value(cst, &value_vertex, domain_vertex, state)?;
            node.extra_fields.insert(key_text, val);
        } else {
            node.extra_fields.insert(key_text, Value::Null);
        }
    }

    state.nodes.insert(node_id, node);
    Ok(())
}

fn insert_empty_yaml_pair_node(pair: &Name, anchor: &str, node_id: u32, state: &mut ExtractState) {
    state.nodes.insert(
        node_id,
        Node::new(node_id, anchor).with_value(FieldPresence::Null),
    );
    state.node_to_cst_struct.insert(node_id, pair.clone());
    state.node_to_cst_value.insert(node_id, vec![pair.clone()]);
}

fn extract_yaml_sequence(
    cst: &Schema,
    domain_schema: &Schema,
    protocol: &Protocol,
    cst_vertex: &Name,
    domain_vertex: &str,
    node_id: u32,
    state: &mut ExtractState,
) -> Result<(), CstCodecError> {
    let mut node = Node::new(node_id, domain_vertex);
    node.shape = NodeShape::List;
    state.nodes.insert(node_id, node);

    let domain_edges: Vec<Edge> = domain_schema.outgoing_edges(domain_vertex).to_vec();
    let item_edge = domain_edges.iter().find(|edge| edge.name.is_none());

    let Some(edge) = item_edge else {
        // Open / partial schema: synthesise an `item` edge per element and
        // recurse through the open extractor. Without this the sequence
        // extracted as an empty list, because no children were collected at
        // all.
        return extract_yaml_sequence_open(cst, cst_vertex, domain_vertex, node_id, state);
    };

    for item_name in cst_children_by_edge_kind(cst, cst_vertex, "child_of") {
        let child_id = state.alloc_id();
        if let Some(value_vertex) = find_yaml_sequence_item_value(cst, item_name) {
            extract_yaml_value(
                cst,
                domain_schema,
                protocol,
                &value_vertex,
                &edge.tgt,
                child_id,
                state,
            )?;
        } else {
            insert_empty_yaml_sequence_node(item_name, &edge.tgt, child_id, state);
        }
        state.arcs.push((node_id, child_id, edge.clone()));
    }

    Ok(())
}

fn extract_yaml_pair_key(cst: &Schema, pair_vertex: &Name) -> Result<String, CstCodecError> {
    let key_vertex = cst_child_by_edge_kind(cst, pair_vertex, "key").ok_or_else(|| {
        CstCodecError::Structure(format!("YAML mapping pair {pair_vertex} has no key"))
    })?;
    let scalar_vertex = unwrap_yaml_node(cst, key_vertex);
    let kind = cst_vertex_kind(cst, &scalar_vertex).unwrap_or_default();
    if !matches!(
        kind.as_str(),
        "plain_scalar"
            | "double_quote_scalar"
            | "single_quote_scalar"
            | "block_scalar"
            | "integer_scalar"
            | "float_scalar"
            | "boolean_scalar"
            | "null_scalar"
    ) {
        return Err(CstCodecError::Structure(format!(
            "YAML mapping key at {pair_vertex} is not a scalar"
        )));
    }
    let raw = cst_source_text(cst, key_vertex);
    let parsed = yaml_serde::from_str::<yaml_serde::Value>(&raw).map_err(|error| {
        CstCodecError::Structure(format!(
            "YAML mapping key at {pair_vertex} is invalid: {error}"
        ))
    })?;
    if matches!(
        parsed,
        yaml_serde::Value::Sequence(_) | yaml_serde::Value::Mapping(_)
    ) {
        return Err(CstCodecError::Structure(format!(
            "YAML mapping key at {pair_vertex} is not a scalar"
        )));
    }
    Ok(yaml_scalar_lexeme(cst, key_vertex, &scalar_vertex).0)
}

fn decoded_yaml_mapping_pairs(
    cst: &Schema,
    mapping_vertex: &Name,
) -> Result<Vec<(Name, String)>, CstCodecError> {
    let mut seen = HashSet::new();
    cst_children_by_edge_kind(cst, mapping_vertex, "child_of")
        .iter()
        .filter(|pair| {
            matches!(
                cst_vertex_kind(cst, pair).as_deref(),
                Some("block_mapping_pair" | "flow_pair")
            )
        })
        .map(|pair| {
            let key = extract_yaml_pair_key(cst, pair)?;
            if !seen.insert(key.clone()) {
                let offset = cst_byte_offset(cst, pair)
                    .map_or_else(|| "unknown".to_owned(), |value| value.to_string());
                return Err(CstCodecError::Structure(format!(
                    "duplicate YAML mapping key at byte {offset}"
                )));
            }
            Ok(((*pair).clone(), key))
        })
        .collect()
}

fn find_yaml_pair_value(cst: &Schema, pair_vertex: &Name) -> Option<Name> {
    cst_child_by_edge_kind(cst, pair_vertex, "value").cloned()
}

fn find_yaml_sequence_item_value(cst: &Schema, item_vertex: &Name) -> Option<Name> {
    for edge in cst.outgoing_edges(item_vertex) {
        let inner = unwrap_yaml_node(cst, &edge.tgt);
        let kind = cst_vertex_kind(cst, &inner).unwrap_or_default();
        if is_yaml_value_kind(&kind) {
            return Some(edge.tgt.clone());
        }
    }
    None
}

fn yaml_scalar_class(cst: &Schema, cst_vertex: &Name) -> Option<LexClass> {
    let leaf = deepest_literal_vertex(cst, cst_vertex);
    let kind = cst_vertex_kind(cst, &leaf).unwrap_or_default();
    match kind.as_str() {
        "null_scalar" => None,
        "boolean_scalar" => Some(LexClass::Bool),
        "integer_scalar" | "float_scalar" => Some(LexClass::Number),
        _ => Some(LexClass::String),
    }
}

#[cfg(test)]
fn parse_yaml_scalar_lexeme(text: &str, class: LexClass) -> FieldPresence {
    coerce_yaml_lexeme(text, class, &ValueSpec::new(ScalarKind::Any)).map_or_else(
        |_| FieldPresence::Present(Value::Str(text.to_owned())),
        |value| {
            if value == Value::Null {
                FieldPresence::Null
            } else {
                FieldPresence::Present(value)
            }
        },
    )
}

fn extract_yaml_generic_value(
    cst: &Schema,
    cst_vertex: &Name,
    domain_vertex: &str,
    state: &mut ExtractState,
) -> Result<Value, CstCodecError> {
    reject_tagged_yaml_container(cst, cst_vertex)?;
    state.enter()?;
    let source_vertex = cst_vertex;
    let semantic_vertex = unwrap_yaml_node(cst, cst_vertex);
    let kind = cst_vertex_kind(cst, &semantic_vertex).unwrap_or_default();
    let cst_vertex = &semantic_vertex;

    let value = match kind.as_str() {
        "block_mapping" | "flow_mapping" => {
            let mut fields = HashMap::new();
            for (pair_name, key) in decoded_yaml_mapping_pairs(cst, cst_vertex)? {
                let value = match find_yaml_pair_value(cst, &pair_name) {
                    Some(val_vertex) => {
                        extract_yaml_generic_value(cst, &val_vertex, domain_vertex, state)?
                    }
                    None => Value::Null,
                };
                fields.insert(key, value);
            }
            Value::Unknown(fields)
        }
        "block_sequence" | "flow_sequence" => {
            let items = cst_children_by_edge_kind(cst, cst_vertex, "child_of");
            Value::List(
                items
                    .iter()
                    .map(|item_name| {
                        find_yaml_sequence_item_value(cst, item_name).map_or_else(
                            || Ok(Value::Null),
                            |val_vertex| {
                                extract_yaml_generic_value(cst, &val_vertex, domain_vertex, state)
                            },
                        )
                    })
                    .collect::<Result<Vec<_>, _>>()?,
            )
        }
        _ => {
            let (text, class) = yaml_scalar_lexeme(cst, source_vertex, cst_vertex);
            match coerce_yaml_lexeme(&text, class, &ValueSpec::new(ScalarKind::Any)) {
                Ok(value) => value,
                Err(error) => {
                    state.fidelity.push(error.at(ValueSite::new(
                        domain_vertex,
                        cst_byte_offset(cst, cst_vertex),
                    )));
                    Value::Null
                }
            }
        }
    };
    state.leave();
    Ok(value)
}

/// Inject a (possibly modified) `WInstance` back into a YAML CST Schema.
///
/// This source-compatible entry point uses an open protocol. Call
/// [`inject_yaml_cst_with_protocol`] when protocol value constraints matter.
///
/// # Errors
///
/// Returns `CstExtractError` if the complement is invalid.
pub fn inject_yaml_cst(
    instance: &WInstance,
    complement: &CstComplement,
    domain_schema: &Schema,
) -> Result<Schema, CstExtractError> {
    inject_yaml_cst_with_protocol(instance, complement, domain_schema, &Protocol::default())
        .map_err(Into::into)
}

/// Inject a YAML CST using the selected protocol's value constraints.
///
/// Updates scalar `literal-value` constraints in YAML-specific CST nodes
/// (`plain_scalar`, `double_quote_scalar`, `single_quote_scalar`,
/// `integer_scalar`, `float_scalar`, etc.).
///
/// # Errors
///
/// Returns `CstCodecError` if the complement is invalid.
pub fn inject_yaml_cst_with_protocol(
    instance: &WInstance,
    complement: &CstComplement,
    domain_schema: &Schema,
    protocol: &Protocol,
) -> Result<Schema, CstCodecError> {
    validate_tree_structure(instance, complement, domain_schema, protocol, "yaml")?;
    let mut cst = complement.cst_schema.clone();

    for (&node_id, node) in &instance.nodes {
        let Some(ref presence) = node.value else {
            continue;
        };
        let Some(segments) = complement.node_to_cst_value.get(&node_id) else {
            continue;
        };
        let Some(head) = segments.first() else {
            continue;
        };
        if is_empty_yaml_pair_carrier(&cst, head) {
            if matches!(presence, FieldPresence::Null) {
                continue;
            }
            let spec = ValueSpec::for_vertex(domain_schema, protocol, &node.anchor);
            let site = ValueSite::new(node.anchor.clone(), cst_byte_offset(&cst, head));
            let new_text = field_presence_to_yaml_text(presence, &spec, &site)?;
            let source = cst_source_text(&cst, head);
            let Some(colon) = yaml_mapping_value_colon(&source) else {
                return Err(unsupported_structural_edit(
                    "yaml",
                    "empty mapping pair has no value colon carrier",
                ));
            };
            let replacement = format!("{} {new_text}{}", &source[..=colon], &source[colon + 1..]);
            set_toml_token_text(&mut cst, head, &replacement);
            continue;
        }
        if is_empty_yaml_sequence_item_carrier(&cst, head) {
            if matches!(presence, FieldPresence::Null) {
                continue;
            }
            let spec = ValueSpec::for_vertex(domain_schema, protocol, &node.anchor);
            let site = ValueSite::new(node.anchor.clone(), cst_byte_offset(&cst, head));
            let new_text = field_presence_to_yaml_text(presence, &spec, &site)?;
            let source = cst_source_text(&cst, head);
            let Some(dash) = source.find('-') else {
                return Err(unsupported_structural_edit(
                    "yaml",
                    "empty sequence item has no dash carrier",
                ));
            };
            let replacement = format!("{} {new_text}{}", &source[..=dash], &source[dash + 1..]);
            set_toml_token_text(&mut cst, head, &replacement);
            continue;
        }
        // A YAML scalar's lexical form does not survive a round-trip through
        // the parsed `Value`: `yes` and `true` are the same boolean, `1.0`
        // and `1` the same number, and a quoted scalar carries quotes the
        // value does not. When the CST's own text still parses to the
        // instance's value nothing has changed, so leave the bytes alone
        // rather than re-serialising a form the source never used.
        let spec = ValueSpec::for_vertex(domain_schema, protocol, &node.anchor);
        let scalar = unwrap_yaml_node(&cst, head);
        let (old_text, old_class) = yaml_scalar_lexeme(&cst, head, &scalar);
        if coerce_yaml_lexeme(&old_text, old_class, &spec)
            .is_ok_and(|value| presence_matches_value(presence, &value))
        {
            continue;
        }
        let site = ValueSite::new(node.anchor.clone(), cst_byte_offset(&cst, head));
        let new_text = field_presence_to_yaml_text(presence, &spec, &site)?;
        set_toml_token_text(&mut cst, head, &new_text);
        for tail in &segments[1..] {
            set_toml_token_text(&mut cst, tail, "");
        }
    }

    Ok(cst)
}

fn yaml_mapping_value_colon(source: &str) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut quote = None;
    let mut escaped = false;
    for (index, byte) in bytes.iter().copied().enumerate() {
        if escaped {
            escaped = false;
            continue;
        }
        match quote {
            Some(b'"') if byte == b'\\' => escaped = true,
            Some(active) if byte == active => quote = None,
            None if matches!(byte, b'"' | b'\'') => quote = Some(byte),
            None if byte == b':' => {
                let next = bytes.get(index + 1).copied();
                if next.is_none_or(|next| next.is_ascii_whitespace() || next == b'#') {
                    return Some(index);
                }
            }
            Some(_) | None => {}
        }
    }
    None
}

fn is_empty_yaml_pair_carrier(cst: &Schema, vertex: &Name) -> bool {
    matches!(
        cst_vertex_kind(cst, vertex).as_deref(),
        Some("block_mapping_pair" | "flow_pair")
    ) && find_yaml_pair_value(cst, vertex).is_none()
}

fn is_empty_yaml_sequence_item_carrier(cst: &Schema, vertex: &Name) -> bool {
    cst_vertex_kind(cst, vertex).as_deref() == Some("block_sequence_item")
        && find_yaml_sequence_item_value(cst, vertex).is_none()
}

/// Convert a `FieldPresence` to YAML scalar text.
fn field_presence_to_yaml_text(
    presence: &FieldPresence,
    spec: &ValueSpec,
    site: &ValueSite,
) -> Result<String, CstCodecError> {
    if matches!(presence, FieldPresence::Present(Value::Null)) {
        return Err(EmitValueError::new(
            site.clone(),
            "null",
            "yaml",
            EmitValueReason::NotRepresentable,
        )
        .into());
    }
    let text = match presence {
        FieldPresence::Present(Value::Str(s) | Value::Token(s)) => serde_json::to_string(s)
            .map_err(|error| {
                CstCodecError::Structure(format!("YAML string encoding failed: {error}"))
            })?,
        FieldPresence::Present(Value::Int(i)) => i.to_string(),
        FieldPresence::Present(Value::Float(f)) if f.to_bits() == f64::NAN.to_bits() => {
            ".nan".to_owned()
        }
        FieldPresence::Present(Value::Float(f)) if f.to_bits() == (-f64::NAN).to_bits() => {
            "-.nan".to_owned()
        }
        FieldPresence::Present(Value::Float(f)) if f.is_nan() => ".nan".to_owned(),
        FieldPresence::Present(Value::Float(f)) if *f == f64::INFINITY => ".inf".to_owned(),
        FieldPresence::Present(Value::Float(f)) if *f == f64::NEG_INFINITY => "-.inf".to_owned(),
        FieldPresence::Present(Value::Float(f)) => json_format_float(*f),
        FieldPresence::Present(Value::Bool(true)) => "true".to_owned(),
        FieldPresence::Present(Value::Bool(false)) => "false".to_owned(),
        FieldPresence::Present(Value::Bytes(bytes)) => yaml_bytes_text(bytes, spec, site)?,
        FieldPresence::Present(Value::Decimal(value)) => {
            if spec.kind == ScalarKind::Any {
                format!("!!decimal {value}")
            } else {
                value.to_string()
            }
        }
        FieldPresence::Present(Value::BigInt(value)) => {
            if spec.kind == ScalarKind::Any {
                format!("!!int {value}")
            } else {
                value.to_string()
            }
        }
        FieldPresence::Present(Value::Temporal(value)) => {
            let quoted = serde_json::to_string(&value.to_string()).map_err(|error| {
                CstCodecError::Structure(format!("YAML timestamp encoding failed: {error}"))
            })?;
            format!("!!timestamp {quoted}")
        }
        FieldPresence::Null | FieldPresence::Present(Value::Null) => "null".to_owned(),
        FieldPresence::Absent => {
            return Err(EmitValueError::new(
                site.clone(),
                "absent",
                "yaml",
                EmitValueReason::AbsentNotRepresentable,
            )
            .into());
        }
        FieldPresence::Present(other) => {
            return Err(EmitValueError::new(
                site.clone(),
                other.type_name(),
                "yaml",
                EmitValueReason::NotRepresentable,
            )
            .into());
        }
    };
    if let FieldPresence::Present(value) = presence {
        let fallback_class = match value {
            Value::Bool(_) => LexClass::Bool,
            Value::Int(_) | Value::Float(_) | Value::Decimal(_) | Value::BigInt(_) => {
                LexClass::Number
            }
            Value::Null => LexClass::Null,
            _ => LexClass::String,
        };
        let (lexeme, class) = yaml_source_lexeme(&text, &text, fallback_class);
        if !coerce_yaml_lexeme(&lexeme, class, spec)
            .is_ok_and(|actual| scalar_values_match(value, &actual))
        {
            return Err(EmitValueError::new(
                site.clone(),
                value.type_name(),
                "yaml",
                EmitValueReason::NotRepresentable,
            )
            .into());
        }
    }
    Ok(text)
}

fn yaml_bytes_text(
    bytes: &[u8],
    spec: &ValueSpec,
    site: &ValueSite,
) -> Result<String, CstCodecError> {
    let encoding = match &spec.kind {
        ScalarKind::Bytes(encoding) => encoding.clone(),
        ScalarKind::Any => BytesText::Base64,
        _ => {
            return Err(EmitValueError::new(
                site.clone(),
                "bytes",
                "yaml",
                EmitValueReason::NotRepresentable,
            )
            .into());
        }
    };
    let encoded = crate::json_pathway::encode_bytes(bytes, &encoding).ok_or_else(|| {
        EmitValueError::new(
            site.clone(),
            "bytes",
            "yaml",
            EmitValueReason::NotRepresentable,
        )
    })?;
    let quoted = serde_json::to_string(&encoded)
        .map_err(|error| CstCodecError::Structure(format!("YAML byte encoding failed: {error}")))?;
    Ok(if encoding == BytesText::Base64 {
        format!("!!binary {quoted}")
    } else {
        quoted
    })
}

// ── Tabular extraction ────────────────────────────────────────────────

/// Extract an `FInstance` from a CSV/TSV CST Schema.
///
/// This source-compatible entry point uses an open protocol. Call
/// [`extract_tabular_cst_with_protocol`] when protocol value constraints matter.
///
/// # Errors
///
/// Returns `CstExtractError` if the CST is invalid.
pub fn extract_tabular_cst(
    cst: &Schema,
    domain_schema: &Schema,
    table_vertex: &str,
) -> Result<(FInstance, CstComplement), CstExtractError> {
    extract_tabular_cst_with_protocol(cst, domain_schema, &Protocol::default(), table_vertex)
        .map_err(Into::into)
}

/// Extract a tabular CST using the selected protocol's value constraints.
///
/// The first row is treated as headers (column names).
/// The complement records the CST vertex for each cell so that
/// `inject_tabular_cst` can update cell values.
///
/// # Errors
///
/// Returns `CstCodecError` if the CST is invalid.
pub fn extract_tabular_cst_with_protocol(
    cst: &Schema,
    domain_schema: &Schema,
    protocol: &Protocol,
    table_vertex: &str,
) -> Result<(FInstance, CstComplement), CstCodecError> {
    validate_cst_has_no_recovery_markers(cst, "tabular")?;
    let doc_vertex = find_cst_root(cst)?;
    let row_vertices = cst_children_by_edge_kind(cst, &doc_vertex, "child_of");

    if row_vertices.is_empty() {
        let complement = CstComplement {
            format: "tabular".into(),
            cst_schema: cst.clone(),
            node_to_cst_value: HashMap::new(),
            node_to_cst_struct: HashMap::new(),
            cell_to_cst_value: HashMap::new(),
            source_vertex: Some(Name::from(table_vertex)),
        };
        return Ok((FInstance::new(), complement));
    }

    let header_fields = cst_children_by_edge_kind(cst, row_vertices[0], "child_of");
    let headers: Vec<String> = header_fields
        .iter()
        .map(|f| decode_tabular_field(&literal_text_deep(cst, f)))
        .collect();
    let mut distinct_headers = HashSet::with_capacity(headers.len());
    if let Some(duplicate) = headers
        .iter()
        .find(|header| !distinct_headers.insert(header.as_str()))
    {
        return Err(CstCodecError::Structure(format!(
            "duplicate tabular header name '{duplicate}'"
        )));
    }

    // Track the CST vertex behind each cell, keyed by its (row, column)
    // position, since a tabular instance has no node ids to key on.
    let mut cell_to_cst: HashMap<u64, Vec<Name>> = HashMap::new();
    let mut rows = Vec::new();
    let mut fidelity = Vec::new();

    for (row_idx, row_name) in row_vertices[1..].iter().enumerate() {
        let fields = cst_children_by_edge_kind(cst, row_name, "child_of");
        if fields.len() > headers.len() {
            return Err(CstCodecError::Structure(format!(
                "data row {} at byte {:?} has {} fields but {} header fields are declared",
                row_idx + 1,
                cst_byte_offset(cst, row_name),
                fields.len(),
                headers.len()
            )));
        }
        let mut row = HashMap::new();
        for (col_idx, (field_name, col)) in fields.iter().zip(&headers).enumerate() {
            let col = col.clone();
            let raw = literal_text_deep(cst, field_name);
            if raw != "_" {
                let text = decode_tabular_field(&raw);
                let (vertex, spec) = tabular_cell_spec(domain_schema, protocol, table_vertex, &col);
                match crate::json_pathway::coerce_exact_lexeme(
                    &Lexeme::new(&text, LexClass::String),
                    &spec,
                ) {
                    Ok(value) => {
                        row.insert(col, value);
                    }
                    Err(error) => fidelity
                        .push(error.at(ValueSite::new(vertex, cst_byte_offset(cst, field_name)))),
                }
            }
            let (Ok(row_u32), Ok(col_u32)) = (u32::try_from(row_idx), u32::try_from(col_idx))
            else {
                continue;
            };
            // Map to the leaf that owns the text, not the wrapper field, so
            // injection can actually rewrite the value.
            cell_to_cst.insert(
                tabular_cell_key(row_u32, col_u32),
                vec![deepest_literal_vertex(cst, field_name)],
            );
        }
        rows.push(row);
    }

    if !fidelity.is_empty() {
        return Err(CstCodecError::Fidelity(fidelity));
    }

    let instance = FInstance::new()
        .with_table(table_vertex, rows)
        .map_err(|e| CstCodecError::Structure(e.to_string()))?;
    let complement = CstComplement {
        format: "tabular".into(),
        cst_schema: cst.clone(),
        node_to_cst_value: HashMap::new(),
        node_to_cst_struct: HashMap::new(),
        cell_to_cst_value: cell_to_cst,
        source_vertex: Some(Name::from(table_vertex)),
    };

    Ok((instance, complement))
}

fn tabular_source_vertex<'a>(
    complement: &'a CstComplement,
    format: &str,
) -> Result<&'a str, CstCodecError> {
    let source_vertex = complement
        .source_vertex()
        .map(AsRef::as_ref)
        .ok_or_else(|| {
            unsupported_structural_edit(format, "complement has no recorded source table")
        })?;
    if !complement.node_to_cst_value.is_empty() || !complement.node_to_cst_struct.is_empty() {
        return Err(unsupported_structural_edit(
            format,
            "tabular complement contains tree carrier maps",
        ));
    }
    Ok(source_vertex)
}

fn validate_tabular_structure(
    instance: &FInstance,
    complement: &CstComplement,
    domain_schema: &Schema,
    protocol: &Protocol,
    format: &str,
) -> Result<(), CstCodecError> {
    let source_vertex = tabular_source_vertex(complement, format)?;
    let (original, rebuilt_complement) = extract_tabular_cst_with_protocol(
        &complement.cst_schema,
        domain_schema,
        protocol,
        source_vertex,
    )
    .map_err(|error| {
        unsupported_structural_edit(
            format,
            format!("preserved source no longer re-extracts: {error}"),
        )
    })?;
    if complement.cell_to_cst_value != rebuilt_complement.cell_to_cst_value {
        return Err(unsupported_structural_edit(
            format,
            "complement cell carriers do not match its preserved source",
        ));
    }
    if instance.table_names() != original.table_names() {
        return Err(unsupported_structural_edit(
            format,
            "the set of non-empty tables changed",
        ));
    }
    if instance.pair_count() != original.pair_count() {
        return Err(unsupported_structural_edit(
            format,
            "foreign-key pairs changed",
        ));
    }

    let current_rows = instance.rows(source_vertex);
    let original_rows = original.rows(source_vertex);
    let current_ids: Vec<_> = current_rows.iter().map(|(id, _)| *id).collect();
    let original_ids: Vec<_> = original_rows.iter().map(|(id, _)| *id).collect();
    if current_ids != original_ids {
        return Err(unsupported_structural_edit(
            format,
            "the ordered row ids changed",
        ));
    }

    let doc_vertex = find_cst_root(&complement.cst_schema)?;
    let row_vertices = cst_children_by_edge_kind(&complement.cst_schema, &doc_vertex, "child_of");
    let headers = row_vertices.first().map_or_else(Vec::new, |header_row| {
        cst_children_by_edge_kind(&complement.cst_schema, header_row, "child_of")
            .iter()
            .map(|field| decode_tabular_field(&literal_text_deep(&complement.cst_schema, field)))
            .collect::<Vec<_>>()
    });
    for (row_index, ((_, current), (_, source))) in
        current_rows.iter().zip(&original_rows).enumerate()
    {
        let mut keys: Vec<_> = current.keys().chain(source.keys()).collect();
        keys.sort_unstable();
        keys.dedup();
        for key in keys {
            let addressable =
                headers
                    .iter()
                    .position(|header| header == key)
                    .is_some_and(|column| {
                        u32::try_from(row_index)
                            .ok()
                            .zip(u32::try_from(column).ok())
                            .is_some_and(|(row, column)| {
                                complement
                                    .cell_to_cst_value
                                    .contains_key(&tabular_cell_key(row, column))
                            })
                    });
            if addressable {
                continue;
            }
            let unchanged = current.get(key).zip(source.get(key)).is_some_and(
                |(current_value, source_value)| {
                    current_value.canonical_bytes() == source_value.canonical_bytes()
                },
            );
            if !unchanged {
                return Err(unsupported_structural_edit(
                    format,
                    format!(
                        "row {} changed unaddressed column '{key}'",
                        current_ids[row_index].0
                    ),
                ));
            }
        }
    }
    Ok(())
}

/// Inject an `FInstance` back into a tabular CST Schema.
///
/// This source-compatible entry point uses an open protocol. Call
/// [`inject_tabular_cst_with_protocol`] when protocol value constraints matter.
///
/// # Errors
///
/// Returns `CstExtractError` if the complement is invalid.
pub fn inject_tabular_cst(
    instance: &FInstance,
    complement: &CstComplement,
    domain_schema: &Schema,
) -> Result<Schema, CstExtractError> {
    inject_tabular_cst_with_protocol(instance, complement, domain_schema, &Protocol::default())
        .map_err(Into::into)
}

/// Inject a tabular CST using the selected protocol's value constraints.
///
/// Walks the `FInstance` table rows and updates the corresponding CST
/// field vertices' `literal-value` constraints using the complement's
/// cell-to-vertex mapping.
///
/// # Errors
///
/// Returns `CstCodecError` if the complement is invalid.
pub fn inject_tabular_cst_with_protocol(
    instance: &FInstance,
    complement: &CstComplement,
    domain_schema: &Schema,
    protocol: &Protocol,
) -> Result<Schema, CstCodecError> {
    let (format, delimiter) = match complement.format.as_str() {
        "csv" => ("csv", Some(b',')),
        "tsv" => ("tsv", Some(b'\t')),
        _ => ("tabular", None),
    };
    validate_tabular_structure(instance, complement, domain_schema, protocol, format)?;
    let mut cst = complement.cst_schema.clone();

    // Find the table (there should be exactly one in the FInstance).
    for (table_name, table) in instance.tables() {
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
            .map(|f| decode_tabular_field(&literal_text_deep(&cst, f)))
            .collect();

        // Rows were numbered in file order, so the rows in ascending id are
        // the file's data rows in order.
        for (row_idx, (_, row)) in table.rows().into_iter().enumerate() {
            for (col_idx, col_name) in headers.iter().enumerate() {
                let (Ok(row_u32), Ok(col_u32)) = (u32::try_from(row_idx), u32::try_from(col_idx))
                else {
                    continue;
                };
                let cell_key = tabular_cell_key(row_u32, col_u32);
                if let Some(segments) = complement.cell_to_cst_value.get(&cell_key) {
                    if let Some(value) = row.get(col_name) {
                        let (vertex, spec) =
                            tabular_cell_spec(domain_schema, protocol, table_name, col_name);
                        let original_raw: String = segments
                            .iter()
                            .map(|segment| cst_source_text(&cst, segment))
                            .collect();
                        let original = decode_tabular_field(&original_raw);
                        if original_raw != "_"
                            && crate::json_pathway::coerce_exact_lexeme(
                                &Lexeme::new(&original, LexClass::String),
                                &spec,
                            )
                            .is_ok_and(|parsed| scalar_values_match(value, &parsed))
                        {
                            continue;
                        }
                        let site = ValueSite::new(vertex, None);
                        let text = tabular_cst_value_text(value, &spec, &site, format, delimiter)?;
                        if headers.len() == 1 && text.is_empty() {
                            return Err(EmitValueError::new(
                                site,
                                value.type_name(),
                                format,
                                EmitValueReason::NotRepresentable,
                            )
                            .into());
                        }
                        set_value_run(&mut cst, segments, &text);
                    } else {
                        let original: String = segments
                            .iter()
                            .map(|segment| cst_source_text(&cst, segment))
                            .collect();
                        if original != "_" {
                            set_value_run(&mut cst, segments, "_");
                        }
                    }
                }
            }
        }
    }

    Ok(cst)
}

fn tabular_cell_spec(
    schema: &Schema,
    protocol: &Protocol,
    table_vertex: &str,
    column: &str,
) -> (Name, ValueSpec) {
    let target = schema
        .outgoing_edges(table_vertex)
        .iter()
        .find(|edge| edge.name.as_deref() == Some(column) || edge.tgt.as_ref() == column)
        .map_or_else(|| Name::from(column), |edge| edge.tgt.clone());
    let spec = if schema.vertices.contains_key(&target) {
        ValueSpec::for_vertex(schema, protocol, &target)
    } else {
        ValueSpec::new(ScalarKind::Any)
    };
    (target, spec)
}

/// Decode the quoted-field alternative shared by the CSV and TSV grammars.
/// The CST carrier keeps this raw token for replay; the instance receives its
/// semantic text.
fn decode_tabular_field(raw: &str) -> String {
    raw.strip_prefix('"')
        .and_then(|inner| inner.strip_suffix('"'))
        .map_or_else(|| raw.to_owned(), |inner| inner.replace("\"\"", "\""))
}

fn tabular_cst_value_text(
    value: &Value,
    spec: &ValueSpec,
    site: &ValueSite,
    format: &str,
    delimiter: Option<u8>,
) -> Result<String, CstCodecError> {
    if !matches!(value, Value::Null)
        && !crate::json_pathway::plain_text_value_roundtrips(value, spec)
    {
        return Err(EmitValueError::new(
            site.clone(),
            value.type_name(),
            format,
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
                    format,
                    EmitValueReason::NonFinite,
                )
            })?
        }
        Value::Bool(value) => value.to_string(),
        Value::Bytes(bytes) => {
            let ScalarKind::Bytes(encoding) = &spec.kind else {
                return Err(EmitValueError::new(
                    site.clone(),
                    "bytes",
                    format,
                    EmitValueReason::NotRepresentable,
                )
                .into());
            };
            crate::json_pathway::encode_bytes(bytes, encoding).ok_or_else(|| {
                EmitValueError::new(
                    site.clone(),
                    "bytes",
                    format,
                    EmitValueReason::NotRepresentable,
                )
            })?
        }
        Value::Decimal(value) => value.to_string(),
        Value::BigInt(value) => value.to_string(),
        Value::Temporal(value) => value.to_string(),
        Value::Null => {
            return Err(EmitValueError::new(
                site.clone(),
                value.type_name(),
                format,
                EmitValueReason::NullNotRepresentable,
            )
            .into());
        }
        other => {
            return Err(EmitValueError::new(
                site.clone(),
                other.type_name(),
                format,
                EmitValueReason::NotRepresentable,
            )
            .into());
        }
    };
    let contains_delimiter = delimiter.map_or_else(
        || text.contains([',', '\t']),
        |delimiter| text.as_bytes().contains(&delimiter),
    );
    if text == "_" || text.contains(['\r', '\n', '"']) || contains_delimiter {
        return Err(EmitValueError::new(
            site.clone(),
            value.type_name(),
            format,
            EmitValueReason::NotRepresentable,
        )
        .into());
    }
    Ok(text)
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
    use panproto_schema::SchemaBuilder;

    fn test_site() -> ValueSite {
        ValueSite::new("leaf", Some(17))
    }

    fn nested_container_cst(kind: &str, levels: usize) -> Option<Schema> {
        let protocol = Protocol::default();
        let mut builder = SchemaBuilder::new(&protocol)
            .vertex("c0", kind, None)
            .ok()?;
        for level in 1..levels {
            let parent = format!("c{}", level - 1);
            let child = format!("c{level}");
            builder = builder
                .vertex(&child, kind, None)
                .ok()?
                .edge(&parent, &child, "child_of", None)
                .ok()?;
        }
        builder.build().ok()
    }

    fn json_string_token_cst(token: &str, start_byte: Option<&str>) -> Option<Schema> {
        let protocol = Protocol::default();
        let builder = SchemaBuilder::new(&protocol)
            .vertex("s", "string", None)
            .ok()?
            .constraint("s", "literal-value", token);
        match start_byte {
            Some(start) => builder.constraint("s", "start-byte", start),
            None => builder,
        }
        .build()
        .ok()
    }

    #[test]
    fn json_string_token_errors_name_document_offsets() {
        let message = |token: &str, start: Option<&str>| {
            let Some(cst) = json_string_token_cst(token, start) else {
                panic!("valid JSON string token CST fixture");
            };
            match validate_json_cst_string_tokens(&cst) {
                Err(CstCodecError::Structure(message)) => message,
                other => panic!("{token} must be refused, got {other:?}"),
            }
        };
        // The hex escape ends at the closing quote, seven bytes into the
        // token; the message places that offending byte in the document.
        assert_eq!(
            message(r#""\ud800""#, Some("11")),
            "invalid JSON string token at byte 11: unexpected end of hex escape at byte 18"
        );
        // Without a recorded start the offending byte is given within the
        // token, and no `Option` debug text appears.
        assert_eq!(
            message(r#""\ud800""#, None),
            "invalid JSON string token at an unrecorded byte: unexpected end of hex escape \
             at byte 7 of the token"
        );
        // A raw line feed (serde_json's line 2, column 0) is byte 2 of the
        // token, the line feed itself.
        assert_eq!(
            message("\"a\nb\"", Some("11")),
            "invalid JSON string token at byte 11: control character (\\u0000-\\u001F) found \
             while parsing a string at byte 13"
        );
    }

    #[test]
    fn undeclared_generic_values_obey_the_extraction_depth_bound() {
        let Some(json) = nested_container_cst("array", MAX_EXTRACT_DEPTH * 2) else {
            panic!("valid nested JSON CST fixture");
        };
        let mut state = ExtractState::new();
        assert!(matches!(
            extract_json_generic_value(&json, &Name::from("c0"), "root", &mut state),
            Err(CstCodecError::NestingTooDeep { .. })
        ));

        let Some(toml) = nested_container_cst("array", MAX_EXTRACT_DEPTH * 2) else {
            panic!("valid nested TOML CST fixture");
        };
        let mut state = ExtractState::new();
        assert!(matches!(
            toml_generic_value(&toml, &Name::from("c0"), "root", &mut state),
            Err(CstCodecError::NestingTooDeep { .. })
        ));

        let Some(yaml) = nested_container_cst("flow_sequence", MAX_EXTRACT_DEPTH * 4) else {
            panic!("valid nested YAML CST fixture");
        };
        let mut state = ExtractState::new();
        assert!(matches!(
            extract_yaml_generic_value(&yaml, &Name::from("c0"), "root", &mut state),
            Err(CstCodecError::NestingTooDeep { .. })
        ));
    }

    fn emitted(result: Result<String, CstCodecError>) -> String {
        match result {
            Ok(text) => text,
            Err(error) => panic!("expected emitted scalar text, got {error}"),
        }
    }

    fn value_error_reason(result: Result<String, CstCodecError>) -> EmitValueReason {
        match result {
            Err(CstCodecError::Value(error)) => error.reason,
            Err(other) => panic!("expected a structured value error, got {other}"),
            Ok(text) => panic!("expected a structured value error, got {text}"),
        }
    }

    fn yaml_presence(source: &str, fallback: &str, class: LexClass) -> FieldPresence {
        let (text, class) = yaml_source_lexeme(source, fallback, class);
        parse_yaml_scalar_lexeme(&text, class)
    }

    fn structural_fixture() -> WInstance {
        let mut nodes = HashMap::new();
        nodes.insert(0, Node::new(0, "root"));
        nodes.insert(
            1,
            Node::new(1, "left").with_value(FieldPresence::Present(Value::Str("a".into()))),
        );
        nodes.insert(
            2,
            Node::new(2, "right").with_value(FieldPresence::Present(Value::Str("b".into()))),
        );
        WInstance::new(
            nodes,
            vec![
                (
                    0,
                    1,
                    Edge {
                        src: "root".into(),
                        tgt: "left".into(),
                        kind: "prop".into(),
                        name: Some("left".into()),
                    },
                ),
                (
                    0,
                    2,
                    Edge {
                        src: "root".into(),
                        tgt: "right".into(),
                        kind: "prop".into(),
                        name: Some("right".into()),
                    },
                ),
            ],
            Vec::new(),
            0,
            "root".into(),
        )
    }

    #[test]
    fn fixed_tree_csts_accept_value_edits_but_refuse_structural_edits() {
        let original = structural_fixture();

        let mut value_edit = original.clone();
        let Some(value_node) = value_edit.nodes.get_mut(&1) else {
            panic!("value node fixture");
        };
        value_node.value = Some(FieldPresence::Present(Value::Str("changed".into())));
        assert!(tree_structural_difference(&value_edit, &original, false).is_none());

        let mut added = original.clone();
        added.nodes.insert(3, Node::new(3, "added"));
        assert!(tree_structural_difference(&added, &original, false).is_some());

        let mut removed = original.clone();
        removed.nodes.remove(&2);
        assert!(tree_structural_difference(&removed, &original, false).is_some());

        let mut reordered = original.clone();
        reordered.arcs.reverse();
        assert!(tree_structural_difference(&reordered, &original, false).is_some());

        let mut reparented_arcs = original.arcs.clone();
        reparented_arcs[1].0 = 1;
        let reparented = WInstance::new(
            original.nodes.clone(),
            reparented_arcs,
            Vec::new(),
            0,
            "root".into(),
        );
        assert!(tree_structural_difference(&reparented, &original, false).is_some());
    }

    #[test]
    fn fixed_tree_csts_refuse_unaddressed_metadata_edits() {
        let mut original = structural_fixture();
        let Some(original_root) = original.nodes.get_mut(&0) else {
            panic!("root fixture");
        };
        original_root
            .extra_fields
            .insert("kept".into(), Value::Str("old".into()));

        let mut changed = original.clone();
        let Some(changed_root) = changed.nodes.get_mut(&0) else {
            panic!("root fixture");
        };
        changed_root
            .extra_fields
            .insert("kept".into(), Value::Str("new".into()));
        assert!(tree_structural_difference(&changed, &original, false).is_some());
        assert!(tree_structural_difference(&changed, &original, true).is_none());

        let Some(changed_root) = changed.nodes.get_mut(&0) else {
            panic!("root fixture");
        };
        changed_root
            .extra_fields
            .insert("added".into(), Value::Str("new".into()));
        assert!(tree_structural_difference(&changed, &original, true).is_some());
    }

    #[test]
    fn childless_xml_elements_may_gain_a_value_slot() {
        let original = WInstance::new(
            HashMap::from([(
                0,
                Node::new(0, "root").with_shape(NodeShape::XmlElement { tag: "doc".into() }),
            )]),
            Vec::new(),
            Vec::new(),
            0,
            "root".into(),
        );
        let mut changed = original.clone();
        let Some(changed_root) = changed.nodes.get_mut(&0) else {
            panic!("root fixture");
        };
        changed_root.value = Some(FieldPresence::Present(Value::Str("text".into())));
        assert!(tree_structural_difference(&changed, &original, true).is_none());
        assert!(tree_structural_difference(&changed, &original, false).is_some());

        let opaque = WInstance::new(
            HashMap::from([(
                0,
                Node::new(0, "root").with_shape(NodeShape::XmlTextSegment),
            )]),
            Vec::new(),
            Vec::new(),
            0,
            "root".into(),
        );
        let mut changed_opaque = opaque.clone();
        let Some(opaque_root) = changed_opaque.nodes.get_mut(&0) else {
            panic!("opaque root fixture");
        };
        opaque_root.value = Some(FieldPresence::Present(Value::Str("text".into())));
        assert!(tree_structural_difference(&changed_opaque, &opaque, true).is_some());
    }

    #[test]
    fn yaml_quoted_block_and_tagged_scalars_decode_semantically() {
        assert_eq!(
            yaml_presence(r#""line\nfeed""#, r"line\nfeed", LexClass::String),
            FieldPresence::Present(Value::Str("line\nfeed".to_owned()))
        );
        assert_eq!(
            yaml_presence("|-\n  first\n  second\n", "first\nsecond", LexClass::String,),
            FieldPresence::Present(Value::Str("first\nsecond".to_owned()))
        );
        assert_eq!(
            yaml_presence(r#"!!binary "AAEC/w==""#, "AAEC/w==", LexClass::String,),
            FieldPresence::Present(Value::Bytes(vec![0x00, 0x01, 0x02, 0xff]))
        );
        assert_eq!(
            yaml_presence(
                "!!binary |\n  AAEC\n  /w==\n",
                "AAEC\n/w==\n",
                LexClass::String,
            ),
            FieldPresence::Present(Value::Bytes(vec![0x00, 0x01, 0x02, 0xff]))
        );
        let (binary_text, binary_class) =
            yaml_source_lexeme(r#"!!binary "AAEC/w==""#, "AAEC/w==", LexClass::String);
        let Ok(binary_value) = coerce_yaml_lexeme(
            &binary_text,
            binary_class,
            &ValueSpec::new(ScalarKind::Bytes(panproto_schema::BytesText::Hex)),
        ) else {
            panic!("the explicit YAML encoding must override the schema's text encoding");
        };
        assert_eq!(binary_value, Value::Bytes(vec![0x00, 0x01, 0x02, 0xff]));
        assert!(matches!(
            yaml_presence(
                r#"!!timestamp "2026-09-28T12:34:56.789+05:30""#,
                "2026-09-28T12:34:56.789+05:30",
                LexClass::String,
            ),
            FieldPresence::Present(Value::Temporal(value))
                if value.to_string() == "2026-09-28T12:34:56.789+05:30"
        ));
        assert_eq!(
            emitted(field_presence_to_yaml_text(
                &FieldPresence::Present(Value::Bytes(vec![0x00, 0x01, 0x02, 0xff])),
                &ValueSpec::new(ScalarKind::Any),
                &test_site(),
            )),
            "!!binary \"AAEC/w==\""
        );
        assert_eq!(
            emitted(field_presence_to_yaml_text(
                &FieldPresence::Present(Value::Bytes(vec![0x00, 0x01, 0x02, 0xff])),
                &ValueSpec::new(ScalarKind::Bytes(panproto_schema::BytesText::Hex)),
                &test_site(),
            )),
            "\"000102ff\""
        );
    }

    #[test]
    fn yaml_exact_number_lexemes_bypass_the_yaml_number_carrier() {
        assert!(matches!(
            yaml_presence(
                "123_456_789_012_345_678_901_234_567_890",
                "123_456_789_012_345_678_901_234_567_890",
                LexClass::Number,
            ),
            FieldPresence::Present(Value::BigInt(value))
                if value.to_string() == "123456789012345678901234567890"
        ));
        assert!(matches!(
            yaml_presence("1e400", "1e400", LexClass::Number),
            FieldPresence::Present(Value::Decimal(value)) if value.to_string() == "1e400"
        ));
    }

    #[test]
    fn format_kind_grammar_names() {
        assert_eq!(FormatKind::Json.grammar_name(), "json");
        assert_eq!(FormatKind::Xml.grammar_name(), "xml");
        assert_eq!(FormatKind::Yaml.grammar_name(), "yaml");
        assert_eq!(FormatKind::Csv.grammar_name(), "csv");
    }

    /// A surrogate pair is the only way a JSON source can spell an astral
    /// character. Decoding each `\uXXXX` on its own yields two lone
    /// surrogates, neither of which is a character, so the emoji used to
    /// disappear from the extracted value entirely.
    #[test]
    fn surrogate_pairs_decode_to_the_astral_character() {
        assert_eq!(decode_json_string_body(r"x\ud83d\ude00y"), "x\u{1F600}y");
        assert_eq!(decode_json_string_body(r"\ud83d\ude00"), "\u{1F600}");
        assert_eq!(
            decode_json_string_body(r"a\ud83dz\ude00b"),
            "a\\ud83dz\\ude00b",
            "surrogates separated by other text are not a pair"
        );
    }

    /// A surrogate with no partner denotes no character at all. Keeping its
    /// escape text is what lets the caller see what the source said, where
    /// dropping it silently loses bytes.
    #[test]
    fn a_lone_surrogate_keeps_its_escape_text() {
        assert_eq!(decode_json_string_body(r"x\ud83dy"), r"x\ud83dy");
        assert_eq!(decode_json_string_body(r"x\ude00y"), r"x\ude00y");
    }

    #[test]
    fn simple_escapes_and_bmp_escapes_decode() {
        assert_eq!(decode_json_string_body(r"x\ny"), "x\ny");
        assert_eq!(decode_json_string_body(r#"a\tb\\c\"d"#), "a\tb\\c\"d");
        assert_eq!(decode_json_string_body(r"caf\u00e9"), "caf\u{e9}");
        assert_eq!(decode_json_string_body("plain"), "plain");
    }

    #[test]
    fn toml_multiline_basic_string_continuations_trim_layout() {
        assert_eq!(
            toml_unquote("\"\"\"first\\\n    second\\\r\n\n\tthird\"\"\""),
            "firstsecondthird"
        );
        assert_eq!(toml_unquote("\"\"\"\r\ntext\"\"\""), "text");
        assert_eq!(
            toml_unquote(r#""escape: \e; latin: \xE9""#),
            "escape: \u{1b}; latin: é"
        );
        assert_eq!(toml_unquote(r#""reserved: \q""#), r"reserved: \q");
    }

    #[test]
    fn unchanged_nan_uses_its_ieee_representation() {
        let positive = Value::Float(f64::NAN);
        let negative = Value::Float(-f64::NAN);
        assert!(presence_matches_value(
            &FieldPresence::Present(positive.clone()),
            &positive
        ));
        assert!(!presence_matches_value(
            &FieldPresence::Present(positive),
            &negative
        ));
    }

    /// A backslash that starts nothing the grammar recognises is text, not
    /// an escape, and must survive rather than be swallowed.
    #[test]
    fn unrecognised_escapes_survive_verbatim() {
        assert_eq!(decode_json_string_body(r"a\qb"), r"a\qb");
        assert_eq!(decode_json_string_body(r"trailing\"), "trailing\\");
        assert_eq!(decode_json_string_body(r"short\u01"), r"short\u01");
    }

    #[test]
    fn xml_text_decodes_safe_references_without_dropping_unknown_entities() {
        let Ok(safe_references) = decode_xml_text("Alice &amp; Bob &#38; &#x1f600; &project;")
        else {
            panic!("legal references must decode");
        };
        assert_eq!(safe_references, "Alice & Bob & 😀 &project;");
        let Ok(unterminated_reference) = decode_xml_text("unterminated &amp") else {
            panic!("an unterminated reference must remain literal");
        };
        assert_eq!(unterminated_reference, "unterminated &amp");
        assert!(decode_xml_text("invalid &#xD800;").is_err());
        assert!(decode_xml_text("invalid &#x110000;").is_err());
        assert!(decode_xml_text("forbidden &#1;").is_err());
    }

    fn assert_native_byte_encodings() {
        let bytes = FieldPresence::Present(Value::Bytes(vec![0x00, 0x01, 0x02, 0xff]));
        let bytes_spec = ValueSpec::new(ScalarKind::Bytes(BytesText::Hex));
        assert_eq!(
            emitted(field_presence_to_json_text(
                &bytes,
                &bytes_spec,
                &test_site()
            )),
            "\"000102ff\""
        );
        assert_eq!(
            emitted(field_presence_to_yaml_text(
                &bytes,
                &bytes_spec,
                &test_site()
            )),
            "\"000102ff\""
        );
        assert_eq!(
            coerce_yaml_lexeme("000102ff", LexClass::String, &bytes_spec),
            Ok(Value::Bytes(vec![0x00, 0x01, 0x02, 0xff]))
        );
        assert_eq!(
            emitted(field_presence_to_yaml_text(
                &bytes,
                &ValueSpec::new(ScalarKind::Bytes(BytesText::Base64)),
                &test_site(),
            )),
            "!!binary \"AAEC/w==\""
        );
        assert_eq!(
            emitted(field_presence_to_yaml_text(
                &bytes,
                &ValueSpec::new(ScalarKind::Bytes(BytesText::Base64Url)),
                &test_site(),
            )),
            "\"AAEC_w==\""
        );
        assert_eq!(
            value_error_reason(field_presence_to_yaml_text(
                &bytes,
                &ValueSpec::new(ScalarKind::Bytes(BytesText::Raw)),
                &test_site(),
            )),
            EmitValueReason::NotRepresentable
        );
        assert_eq!(
            emitted(field_presence_to_json_text(
                &FieldPresence::Present(Value::Bytes(b"quote\"\\\n\0".to_vec())),
                &ValueSpec::new(ScalarKind::Bytes(BytesText::Raw)),
                &test_site(),
            )),
            "\"quote\\\"\\\\\\n\\u0000\""
        );
        let Some(bytes_value) = bytes.as_value() else {
            panic!("present bytes fixture");
        };
        assert_eq!(
            emitted(tabular_cst_value_text(
                bytes_value,
                &bytes_spec,
                &test_site(),
                "csv",
                Some(b','),
            )),
            "000102ff"
        );
    }

    #[test]
    fn codecs_write_exact_values_in_native_form() {
        assert_eq!(
            emitted(field_presence_to_json_text(
                &FieldPresence::Null,
                &ValueSpec::new(ScalarKind::Any),
                &test_site(),
            )),
            "null"
        );
        let Ok(decimal) =
            panproto_inst::value::DecimalValue::parse("12345678901234567890.123456789")
        else {
            panic!("valid decimal fixture");
        };
        let decimal = FieldPresence::Present(Value::Decimal(decimal));
        let decimal_spec = ValueSpec::new(ScalarKind::Decimal);
        assert_eq!(
            emitted(field_presence_to_json_text(
                &decimal,
                &decimal_spec,
                &test_site()
            )),
            "12345678901234567890.123456789"
        );
        assert_eq!(
            emitted(field_presence_to_yaml_text(
                &decimal,
                &decimal_spec,
                &test_site()
            )),
            "12345678901234567890.123456789"
        );
        assert_eq!(
            emitted(field_presence_to_text(
                &decimal,
                &decimal_spec,
                &test_site()
            )),
            "12345678901234567890.123456789"
        );

        assert_native_byte_encodings();
    }

    fn assert_tabular_value_refusals(any: &ValueSpec) {
        assert_eq!(
            value_error_reason(tabular_cst_value_text(
                &Value::Null,
                any,
                &test_site(),
                "csv",
                Some(b','),
            )),
            EmitValueReason::NullNotRepresentable
        );

        for (text, format, delimiter) in [
            ("_", "csv", b','),
            ("left,right", "csv", b','),
            ("left\tright", "tsv", b'\t'),
            ("left\nright", "csv", b','),
        ] {
            assert_eq!(
                value_error_reason(tabular_cst_value_text(
                    &Value::Str(text.into()),
                    any,
                    &test_site(),
                    format,
                    Some(delimiter),
                )),
                EmitValueReason::NotRepresentable
            );
        }

        let raw_bytes = ValueSpec::new(ScalarKind::Bytes(BytesText::Raw));
        match tabular_cst_value_text(
            &Value::Bytes(vec![0xff]),
            &raw_bytes,
            &test_site(),
            "csv",
            Some(b','),
        ) {
            Err(CstCodecError::Value(error)) => {
                assert_eq!(error.format, "csv");
                assert_eq!(error.reason, EmitValueReason::NotRepresentable);
            }
            Err(other) => panic!("expected a structured CSV value error, got {other}"),
            Ok(text) => panic!("expected a structured CSV value error, got {text}"),
        }

        for text in ["left,right", "left\tright"] {
            assert_eq!(
                value_error_reason(tabular_cst_value_text(
                    &Value::Str(text.to_owned()),
                    &ValueSpec::new(ScalarKind::Str),
                    &test_site(),
                    "tabular",
                    None,
                )),
                EmitValueReason::NotRepresentable
            );
        }
    }

    #[test]
    fn emit_returns_typed_value_errors_instead_of_debug_text() {
        let any = ValueSpec::new(ScalarKind::Any);
        assert_eq!(
            value_error_reason(field_presence_to_json_text(
                &FieldPresence::Present(Value::Float(f64::INFINITY)),
                &any,
                &test_site(),
            )),
            EmitValueReason::NonFinite
        );

        let Ok(short_decimal) = panproto_inst::value::DecimalValue::parse("0.1") else {
            panic!("valid decimal fixture");
        };
        let short_decimal = FieldPresence::Present(Value::Decimal(short_decimal));
        assert_eq!(
            value_error_reason(field_presence_to_json_text(
                &short_decimal,
                &any,
                &test_site(),
            )),
            EmitValueReason::NotRepresentable
        );
        assert_eq!(
            emitted(field_presence_to_yaml_text(
                &short_decimal,
                &any,
                &test_site(),
            )),
            "!!decimal 0.1"
        );

        for presence in [
            FieldPresence::Present(Value::Null),
            FieldPresence::Present(Value::Float(f64::from_bits(
                f64::NAN.to_bits().wrapping_add(1),
            ))),
        ] {
            assert_eq!(
                value_error_reason(field_presence_to_yaml_text(&presence, &any, &test_site(),)),
                EmitValueReason::NotRepresentable
            );
        }

        let cst = Schema::default();
        let leaf = Name::from("leaf");
        for presence in [FieldPresence::Null, FieldPresence::Present(Value::Null)] {
            assert_eq!(
                value_error_reason(toml_presence_text(
                    &cst,
                    &leaf,
                    &presence,
                    &any,
                    &test_site(),
                )),
                EmitValueReason::NullNotRepresentable
            );
        }

        assert_eq!(
            value_error_reason(field_presence_to_yaml_text(
                &FieldPresence::Absent,
                &any,
                &test_site(),
            )),
            EmitValueReason::AbsentNotRepresentable
        );

        assert_tabular_value_refusals(&any);
    }

    /// The carrier maps of a complement as they were serialized before
    /// provenance had a field of its own, with a structural node whose id is
    /// `u32::MAX` and no recorded source vertex.
    fn legacy_carrier_json(format: &str, struct_vertex: &str) -> serde_json::Value {
        let Ok(cst_schema) = serde_json::to_value(Schema::default()) else {
            panic!("serialize CST schema");
        };
        serde_json::json!({
            "format": format,
            "cst_schema": cst_schema,
            "node_to_cst_value": {},
            "node_to_cst_struct": { u32::MAX.to_string(): struct_vertex },
            "cell_to_cst_value": {},
        })
    }

    #[test]
    fn a_structural_node_at_the_last_id_is_not_provenance() {
        let Ok(tree) =
            serde_json::from_value::<CstComplement>(legacy_carrier_json("json", "object_1"))
        else {
            panic!("decode tree carrier");
        };
        assert_eq!(tree.source_vertex(), None);
        assert_eq!(
            tree.node_to_cst_struct.get(&u32::MAX).map(AsRef::as_ref),
            Some("object_1")
        );
        let instance = WInstance::new(
            HashMap::new(),
            Vec::new(),
            Vec::new(),
            0,
            Name::from("object_1"),
        );
        let Err(refusal) = inject_json_cst(&instance, &tree, &Schema::default()) else {
            panic!("a complement without provenance is refused");
        };
        let refusal = refusal.to_string();
        assert!(
            refusal.contains("complement has no recorded source vertex"),
            "{refusal}"
        );

        let Ok(table) = serde_json::from_value::<CstComplement>(legacy_carrier_json("tsv", "rows"))
        else {
            panic!("decode table carrier");
        };
        assert_eq!(table.source_vertex(), None);
        let Err(refusal) = inject_tabular_cst(&FInstance::new(), &table, &Schema::default()) else {
            panic!("a tabular complement without provenance is refused");
        };
        let refusal = refusal.to_string();
        assert!(
            refusal.contains("complement has no recorded source table"),
            "{refusal}"
        );
    }

    #[test]
    fn a_legacy_msgpack_complement_decodes_without_provenance() {
        let mut node_to_cst_struct = HashMap::new();
        node_to_cst_struct.insert(u32::MAX, Name::from("object_1"));
        let legacy = (
            "json",
            Schema::default(),
            HashMap::<u32, Vec<Name>>::new(),
            node_to_cst_struct,
            HashMap::<u64, Vec<Name>>::new(),
        );
        let Ok(bytes) = rmp_serde::to_vec(&legacy) else {
            panic!("encode legacy carrier");
        };
        let Ok(decoded) = rmp_serde::from_slice::<CstComplement>(&bytes) else {
            panic!("decode legacy carrier");
        };
        assert_eq!(decoded.source_vertex(), None);
        assert_eq!(decoded.node_to_cst_struct.len(), 1);
    }

    #[test]
    fn source_compatible_errors_keep_structured_codec_errors() {
        let value = EmitValueError::new(
            ValueSite::new("x", None),
            "float",
            "json",
            EmitValueReason::NonFinite,
        );
        assert!(matches!(
            CstExtractError::from(CstCodecError::Value(value.clone())),
            CstExtractError::Value(ref carried) if *carried == value
        ));

        let fidelity = vec![ValueFidelityError {
            site: ValueSite::new("x", Some(5)),
            lexeme: "1e2147483648".to_owned(),
            target: ScalarKind::Any,
            reason: panproto_inst::FidelityReason::Overflow,
        }];
        assert!(matches!(
            CstExtractError::from(CstCodecError::Fidelity(fidelity.clone())),
            CstExtractError::Fidelity(ref carried) if *carried == fidelity
        ));

        assert!(matches!(
            CstExtractError::from(CstCodecError::UnsupportedTaggedContainer {
                tag: "!set".to_owned(),
                container_kind: "mapping".to_owned(),
                byte_offset: Some(3),
            }),
            CstExtractError::UnsupportedTaggedContainer {
                ref tag,
                ref container_kind,
                byte_offset: Some(3),
            } if tag == "!set" && container_kind == "mapping"
        ));
        assert!(matches!(
            CstExtractError::from(CstCodecError::UnsupportedStructuralEdit {
                format: "json".to_owned(),
                detail: "a new key".to_owned(),
            }),
            CstExtractError::UnsupportedStructuralEdit { ref format, ref detail }
                if format == "json" && detail == "a new key"
        ));
    }

    #[test]
    fn cst_json_zero_numerals_keep_their_sign_whatever_their_exponent() {
        for (numeral, expected) in [
            ("0e99999999999", 0.0_f64),
            ("0.0e-99999999999", 0.0),
            ("-0.0", -0.0),
            ("-0e99999999999", -0.0),
        ] {
            let Ok(Value::Float(actual)) = parse_number_value(numeral) else {
                panic!(
                    "{numeral} reads as a float: {:?}",
                    parse_number_value(numeral)
                );
            };
            assert_eq!(actual.to_bits(), expected.to_bits(), "{numeral}");
        }
    }
}
