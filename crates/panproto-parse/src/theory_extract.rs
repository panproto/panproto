//! Automated theory extraction from tree-sitter grammar metadata.
//!
//! Tree-sitter grammars are theory presentations: each grammar's `node-types.json`
//! file is structurally isomorphic to a GAT. This module extracts panproto theories
//! directly from grammar metadata, ensuring the theory is always in sync with the
//! parser.
//!
//! ## Mapping
//!
//! | `node-types.json` | panproto GAT |
//! |---|---|
//! | Named node type | Sort (vertex kind) |
//! | Field (`required: true`) | Operation (mandatory edge kind) |
//! | Field (`required: false`) | Operation (optional edge kind) |
//! | Field (`multiple: true`) | Operation (ordered edge kind) |
//! | Supertype with subtypes | Abstract sort with subtype inclusions |

use panproto_gat::{Operation, Sort, Theory};
use rustc_hash::FxHashSet;

use crate::error::ParseError;

// ─── node-types.json schema ───────────────────────────────────────────────

/// A node type entry from tree-sitter's `node-types.json`.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct NodeType {
    /// The node type name (e.g. `"function_declaration"`).
    #[serde(rename = "type")]
    pub node_type: String,
    /// Whether this is a named grammar rule (true) or anonymous token (false).
    pub named: bool,
    /// Named fields and their specifications.
    #[serde(default)]
    pub fields: serde_json::Map<String, serde_json::Value>,
    /// Unnamed children specification.
    #[serde(default)]
    pub children: Option<ChildSpec>,
    /// For supertype nodes, the concrete subtypes.
    #[serde(default)]
    pub subtypes: Option<Vec<SubtypeRef>>,
}

/// Specification for unnamed children of a node type.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ChildSpec {
    /// Whether multiple children are allowed.
    pub multiple: bool,
    /// Whether at least one child is required.
    pub required: bool,
    /// The allowed child node types.
    pub types: Vec<SubtypeRef>,
}

/// A reference to a node type (used in field types and subtype arrays).
#[derive(Debug, Clone, serde::Deserialize)]
pub struct SubtypeRef {
    /// The node type name.
    #[serde(rename = "type")]
    pub node_type: String,
    /// Whether this is a named node type.
    pub named: bool,
}

/// A parsed field specification from `node-types.json`.
#[derive(Debug, Clone)]
pub struct FieldSpec {
    /// The field name (e.g. `"body"`, `"condition"`, `"parameters"`).
    pub name: String,
    /// Whether this field is required.
    pub required: bool,
    /// Whether this field can contain multiple children.
    pub multiple: bool,
    /// The allowed child node types.
    pub types: Vec<SubtypeRef>,
}

/// Metadata about an extracted theory, capturing information beyond the GAT itself.
#[derive(Debug, Clone)]
pub struct ExtractedTheoryMeta {
    /// The GAT extracted from the grammar.
    pub theory: Theory,
    /// Named node types that are supertypes (abstract sorts).
    pub supertypes: FxHashSet<String>,
    /// Mapping from supertype name to its concrete subtypes.
    pub subtype_map: Vec<(String, Vec<String>)>,
    /// Fields that are optional (for ThPartial composition).
    pub optional_fields: FxHashSet<String>,
    /// Fields that are ordered (for ThOrder composition).
    pub ordered_fields: FxHashSet<String>,
    /// All named node types (vertex kinds for the protocol).
    pub vertex_kinds: Vec<String>,
    /// All field names (edge kinds for the protocol).
    pub edge_kinds: Vec<String>,
}

// ─── extraction from node-types.json ──────────────────────────────────────

/// Parse tree-sitter's `node-types.json` bytes into a vector of [`NodeType`] entries.
///
/// # Errors
///
/// Returns [`ParseError::NodeTypesJson`] if JSON deserialization fails.
pub fn parse_node_types(json: &[u8]) -> Result<Vec<NodeType>, ParseError> {
    serde_json::from_slice(json).map_err(|e| ParseError::NodeTypesJson { source: e })
}

/// Extract a panproto [`Theory`] from tree-sitter's `node-types.json` content.
///
/// The returned [`ExtractedTheoryMeta`] includes the GAT plus metadata about
/// supertypes, optional fields, and ordered fields needed for protocol definition
/// and colimit composition.
///
/// # Errors
///
/// Returns [`ParseError`] if JSON parsing fails or the grammar has structural
/// issues preventing theory extraction.
pub fn extract_theory_from_node_types(
    theory_name: &str,
    json: &[u8],
) -> Result<ExtractedTheoryMeta, ParseError> {
    let node_types = parse_node_types(json)?;
    extract_theory_from_entries(theory_name, &node_types)
}

/// Extract a theory from already-parsed [`NodeType`] entries.
///
/// # Errors
///
/// Returns [`ParseError::TheoryExtraction`] if the grammar structure is invalid.
pub fn extract_theory_from_entries(
    theory_name: &str,
    node_types: &[NodeType],
) -> Result<ExtractedTheoryMeta, ParseError> {
    let mut sorts: Vec<Sort> = Vec::new();
    let mut ops: Vec<Operation> = Vec::new();
    let mut supertypes = FxHashSet::default();
    let mut subtype_map: Vec<(String, Vec<String>)> = Vec::new();
    let mut optional_fields = FxHashSet::default();
    let mut ordered_fields = FxHashSet::default();
    let mut vertex_kinds: Vec<String> = Vec::new();
    let mut edge_kind_set = FxHashSet::default();
    let mut seen_sorts = FxHashSet::default();

    // Always include Vertex and Edge as base sorts (shared with ThGraph via colimit).
    sorts.push(Sort::simple("Vertex"));
    sorts.push(Sort::simple("Edge"));
    seen_sorts.insert("Vertex".to_owned());
    seen_sorts.insert("Edge".to_owned());

    for entry in node_types {
        // Skip anonymous tokens (punctuation, keywords).
        if !entry.named {
            continue;
        }

        let sort_name = &entry.node_type;

        // Supertype nodes define abstract sorts with subtype inclusions.
        if let Some(ref subtypes) = entry.subtypes {
            supertypes.insert(sort_name.clone());
            let concrete: Vec<String> = subtypes
                .iter()
                .filter(|s| s.named)
                .map(|s| s.node_type.clone())
                .collect();
            subtype_map.push((sort_name.clone(), concrete));

            // Register the supertype as a sort if not already present.
            if seen_sorts.insert(sort_name.clone()) {
                sorts.push(Sort::simple(sort_name.as_str()));
                vertex_kinds.push(sort_name.clone());
            }
            continue;
        }

        // Regular named node type: create a sort and operations for its fields.
        if seen_sorts.insert(sort_name.clone()) {
            sorts.push(Sort::simple(sort_name.as_str()));
            vertex_kinds.push(sort_name.clone());
        }

        // Process fields: each field becomes an operation (edge kind).
        for (field_name, field_value) in &entry.fields {
            let spec = parse_field_spec(field_name, field_value)?;

            // Track optional and ordered fields for later composition.
            if !spec.required {
                optional_fields.insert(field_name.clone());
            }
            if spec.multiple {
                ordered_fields.insert(field_name.clone());
            }

            // Create an operation for this field if not already registered.
            // The operation represents an edge kind: parent_sort --field_name--> child_sort.
            // Since tree-sitter fields can accept multiple child types, we model
            // the operation as mapping from the parent sort to Vertex (the abstract base).
            if edge_kind_set.insert(field_name.clone()) {
                ops.push(Operation::unary(
                    field_name.as_str(),
                    "parent",
                    "Vertex",
                    "Vertex",
                ));
            }
        }

        // Process unnamed children (if present).
        if let Some(ref children) = entry.children {
            if children.multiple {
                ordered_fields.insert("children".to_owned());
            }
            // Unnamed children use a generic "child_of" edge.
            if edge_kind_set.insert("child_of".to_owned()) {
                ops.push(Operation::unary("child_of", "parent", "Vertex", "Vertex"));
            }
        }
    }

    let edge_kinds: Vec<String> = edge_kind_set.into_iter().collect();

    let theory = Theory::new(theory_name, sorts, ops, vec![]);

    Ok(ExtractedTheoryMeta {
        theory,
        supertypes,
        subtype_map,
        optional_fields,
        ordered_fields,
        vertex_kinds,
        edge_kinds,
    })
}

/// Extract a theory at runtime from a tree-sitter [`Language`] object.
///
/// This uses the Language introspection API (`node_kind_count`, `field_count`,
/// `node_kind_is_named`, `supertypes()`) rather than parsing `node-types.json`.
///
/// Supertype information is available via the runtime API. However, field
/// optionality (`required`) and multiplicity (`multiple`) are NOT exposed
/// by the Language runtime API. For full metadata, use
/// [`extract_theory_from_node_types`] with the `NODE_TYPES` constant
/// embedded in each grammar crate.
///
/// # Errors
///
/// Returns [`ParseError::TheoryExtraction`] if introspection fails.
pub fn extract_theory_from_language(
    theory_name: &str,
    language: &tree_sitter::Language,
) -> Result<ExtractedTheoryMeta, ParseError> {
    let mut sorts: Vec<Sort> = Vec::new();
    let mut ops: Vec<Operation> = Vec::new();
    let mut vertex_kinds: Vec<String> = Vec::new();
    let mut edge_kind_set = FxHashSet::default();
    let mut seen_sorts = FxHashSet::default();
    // Base sorts.
    sorts.push(Sort::simple("Vertex"));
    sorts.push(Sort::simple("Edge"));
    seen_sorts.insert("Vertex".to_owned());
    seen_sorts.insert("Edge".to_owned());

    // Enumerate all named node types as sorts.
    let node_count = language.node_kind_count();
    for id in 0..node_count {
        let id_u16 = id as u16;
        if language.node_kind_is_named(id_u16) {
            if let Some(name) = language.node_kind_for_id(id_u16) {
                // Skip internal hidden nodes (prefixed with _).
                if name.starts_with('_') {
                    continue;
                }

                if seen_sorts.insert(name.to_owned()) {
                    sorts.push(Sort::simple(name));
                    vertex_kinds.push(name.to_owned());
                }
            }
        }
    }

    // Enumerate all field names as operations (edge kinds).
    let field_count = language.field_count();
    for id in 1..=field_count {
        if let Some(name) = language.field_name_for_id(id as u16) {
            if edge_kind_set.insert(name.to_owned()) {
                ops.push(Operation::unary(name, "parent", "Vertex", "Vertex"));
            }
        }
    }

    let edge_kinds: Vec<String> = edge_kind_set.into_iter().collect();

    let theory = Theory::new(theory_name, sorts, ops, vec![]);

    // Note: optional_fields, ordered_fields, supertypes, and subtype_map
    // cannot be fully determined from the tree-sitter 0.24 Language runtime API.
    // For full metadata, use extract_theory_from_node_types() with the NODE_TYPES
    // constant from the grammar crate.
    Ok(ExtractedTheoryMeta {
        theory,
        supertypes: FxHashSet::default(),
        subtype_map: Vec::new(),
        optional_fields: FxHashSet::default(),
        ordered_fields: FxHashSet::default(),
        vertex_kinds,
        edge_kinds,
    })
}

// ─── helpers ──────────────────────────────────────────────────────────────

/// Parse a field specification from the JSON value in node-types.json.
fn parse_field_spec(
    name: &str,
    value: &serde_json::Value,
) -> Result<FieldSpec, ParseError> {
    let obj = value.as_object().ok_or_else(|| ParseError::TheoryExtraction {
        reason: format!("field '{name}' is not an object"),
    })?;

    let required = obj
        .get("required")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);

    let multiple = obj
        .get("multiple")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);

    let types: Vec<SubtypeRef> = obj
        .get("types")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();

    Ok(FieldSpec {
        name: name.to_owned(),
        required,
        multiple,
        types,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn extract_minimal_grammar() {
        let json = br#"[
            {
                "type": "program",
                "named": true,
                "fields": {},
                "children": {
                    "multiple": true,
                    "required": false,
                    "types": [{"type": "statement", "named": true}]
                }
            },
            {
                "type": "statement",
                "named": true,
                "fields": {
                    "body": {
                        "multiple": false,
                        "required": true,
                        "types": [{"type": "expression", "named": true}]
                    }
                }
            },
            {
                "type": "expression",
                "named": true,
                "fields": {}
            },
            {
                "type": ";",
                "named": false
            }
        ]"#;

        let meta = extract_theory_from_node_types("ThTest", json).unwrap();

        // Should have Vertex, Edge (base) + program, statement, expression = 5 sorts.
        assert_eq!(meta.theory.sorts.len(), 5);

        // Should have body + child_of = 2 operations.
        assert_eq!(meta.theory.ops.len(), 2);

        // "program", "statement", "expression" as vertex kinds.
        assert_eq!(meta.vertex_kinds.len(), 3);
        assert!(meta.vertex_kinds.contains(&"program".to_owned()));
        assert!(meta.vertex_kinds.contains(&"statement".to_owned()));
        assert!(meta.vertex_kinds.contains(&"expression".to_owned()));

        // "body" and "child_of" as edge kinds.
        assert_eq!(meta.edge_kinds.len(), 2);

        // "children" on program is ordered (multiple=true).
        assert!(meta.ordered_fields.contains("children"));
    }

    #[test]
    fn extract_supertype() {
        let json = br#"[
            {
                "type": "_expression",
                "named": true,
                "subtypes": [
                    {"type": "binary_expression", "named": true},
                    {"type": "call_expression", "named": true}
                ]
            },
            {
                "type": "binary_expression",
                "named": true,
                "fields": {
                    "left": {
                        "multiple": false,
                        "required": true,
                        "types": [{"type": "_expression", "named": true}]
                    },
                    "right": {
                        "multiple": false,
                        "required": true,
                        "types": [{"type": "_expression", "named": true}]
                    }
                }
            },
            {
                "type": "call_expression",
                "named": true,
                "fields": {
                    "function": {
                        "multiple": false,
                        "required": true,
                        "types": [{"type": "_expression", "named": true}]
                    },
                    "arguments": {
                        "multiple": true,
                        "required": true,
                        "types": [{"type": "_expression", "named": true}]
                    }
                }
            }
        ]"#;

        let meta = extract_theory_from_node_types("ThExprTest", json).unwrap();

        // _expression is a supertype.
        assert!(meta.supertypes.contains("_expression"));

        // subtype_map: _expression → [binary_expression, call_expression]
        assert_eq!(meta.subtype_map.len(), 1);
        let (st, subs) = &meta.subtype_map[0];
        assert_eq!(st, "_expression");
        assert_eq!(subs.len(), 2);

        // "arguments" is ordered.
        assert!(meta.ordered_fields.contains("arguments"));

        // Operations: left, right, function, arguments = 4 edge kinds.
        assert_eq!(meta.edge_kinds.len(), 4);
    }

    #[test]
    fn anonymous_tokens_skipped() {
        let json = br#"[
            {"type": "identifier", "named": true, "fields": {}},
            {"type": "(", "named": false},
            {"type": ")", "named": false}
        ]"#;

        let meta = extract_theory_from_node_types("ThAnon", json).unwrap();

        // Only "identifier" + base sorts (Vertex, Edge) = 3.
        assert_eq!(meta.theory.sorts.len(), 3);
        assert_eq!(meta.vertex_kinds.len(), 1);
    }
}
