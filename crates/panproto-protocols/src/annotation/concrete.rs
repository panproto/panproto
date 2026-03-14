//! Concrete (JHU HLTCOE) annotation format protocol definition.
//!
//! Uses Group A theory: constrained multigraph + W-type.
//!
//! Vertex kinds map to the principal Concrete Thrift structs:
//!
//! | Kind | Concrete struct |
//! |------|----------------|
//! | `communication` | `Communication` |
//! | `section` | `Section` |
//! | `sentence` | `Sentence` |
//! | `tokenization` | `Tokenization` |
//! | `token` | `Token` |
//! | `token-tagging` | `TokenTagging` |
//! | `dependency-parse` | `DependencyParse` |
//! | `constituent` | `Constituent` (node in a `Parse` tree) |
//! | `parse` | `Parse` (the parse container; children are `constituent`) |
//! | `entity-mention` | `EntityMention` |
//! | `entity-mention-set` | `EntityMentionSet` |
//! | `entity` | `Entity` |
//! | `entity-set` | `EntitySet` (coreference clusters) |
//! | `situation-mention` | `SituationMention` |
//! | `situation-mention-set` | `SituationMentionSet` |
//! | `situation` | `Situation` |
//! | `situation-set` | `SituationSet` |
//! | `document-tag` | `CommunicationTagging` |
//! | `span` | `TextSpan` / `AudioSpan` (scalar anchor) |
//! | `string` | string-valued leaf |
//! | `integer` | integer-valued leaf |

use std::collections::HashMap;
use std::hash::BuildHasher;

use panproto_gat::Theory;
use panproto_schema::{EdgeRule, Protocol, Schema, SchemaBuilder};

use crate::emit::{children_by_edge, find_roots, vertex_constraints};
use crate::error::ProtocolError;
use crate::theories;

/// Returns the Concrete protocol definition.
#[must_use]
pub fn protocol() -> Protocol {
    Protocol {
        name: "concrete".into(),
        schema_theory: "ThConcreteSchema".into(),
        instance_theory: "ThConcreteInstance".into(),
        edge_rules: edge_rules(),
        obj_kinds: vec![
            "communication".into(),
            "section".into(),
            "sentence".into(),
            "tokenization".into(),
            "token".into(),
            "token-tagging".into(),
            "dependency-parse".into(),
            "constituent".into(),
            "parse".into(),
            "entity-mention".into(),
            "entity-mention-set".into(),
            "entity".into(),
            "entity-set".into(),
            "situation-mention".into(),
            "situation-mention-set".into(),
            "situation".into(),
            "situation-set".into(),
            "document-tag".into(),
            "span".into(),
            "string".into(),
            "integer".into(),
        ],
        constraint_sorts: vec![
            "uuid".into(),
            "kind".into(),
            "text".into(),
            "tag".into(),
            "confidence".into(),
            "tool".into(),
            "kbest-index".into(),
            "timestamp".into(),
            "role".into(),
            "subkind".into(),
        ],
        has_order: true,
        ..Protocol::default()
    }
}

/// Register the component GATs for Concrete.
pub fn register_theories<S: BuildHasher>(registry: &mut HashMap<String, Theory, S>) {
    theories::register_constrained_multigraph_wtype(
        registry,
        "ThConcreteSchema",
        "ThConcreteInstance",
    );
}

/// Parse a JSON-based Concrete schema into a [`Schema`].
///
/// # Errors
///
/// Returns [`ProtocolError`] if parsing fails.
pub fn parse_concrete_schema(json: &serde_json::Value) -> Result<Schema, ProtocolError> {
    let proto = protocol();
    let mut builder = SchemaBuilder::new(&proto);

    let types = json
        .get("types")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| ProtocolError::MissingField("types".into()))?;

    for (name, def) in types {
        let kind = def
            .get("kind")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("communication");
        builder = builder.vertex(name, kind, None)?;

        if let Some(fields) = def.get("fields").and_then(serde_json::Value::as_object) {
            for (field_name, field_def) in fields {
                let field_id = format!("{name}.{field_name}");
                let field_kind = field_def
                    .get("type")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("string");
                builder = builder.vertex(&field_id, field_kind, None)?;
                builder = builder.edge(name, &field_id, "contains", Some(field_name))?;

                if let Some(uuid) = field_def.get("uuid").and_then(serde_json::Value::as_str) {
                    builder = builder.constraint(&field_id, "uuid", uuid);
                }
                if let Some(tag) = field_def.get("tag").and_then(serde_json::Value::as_str) {
                    builder = builder.constraint(&field_id, "tag", tag);
                }
                if let Some(tool) = field_def.get("tool").and_then(serde_json::Value::as_str) {
                    builder = builder.constraint(&field_id, "tool", tool);
                }
                if let Some(ts) = field_def
                    .get("timestamp")
                    .and_then(serde_json::Value::as_str)
                {
                    builder = builder.constraint(&field_id, "timestamp", ts);
                }
                if let Some(role) = field_def.get("role").and_then(serde_json::Value::as_str) {
                    builder = builder.constraint(&field_id, "role", role);
                }
                if let Some(sk) = field_def.get("subkind").and_then(serde_json::Value::as_str) {
                    builder = builder.constraint(&field_id, "subkind", sk);
                }
            }
        }

        if let Some(items) = def.get("items").and_then(serde_json::Value::as_array) {
            for (i, item) in items.iter().enumerate() {
                if let Some(item_kind) = item.as_str() {
                    let item_id = format!("{name}:item{i}");
                    builder = builder.vertex(&item_id, item_kind, None)?;
                    builder = builder.edge(name, &item_id, "contains", Some(item_kind))?;
                }
            }
        }

        // Structural reference edges encoded in the JSON as arrays of IDs.
        // "token_refs": ["other-vertex-id", ...]  → token-ref edges
        // "span_refs":  ["other-vertex-id", ...]  → span-ref edges
        // "parse_children": ["constituent-id", ...] → parse-child edges
        // "graph_deps": ["dep-parse-id", ...]     → graph-dep edges
        // Deferred to a second pass (see below).
    }

    // Second pass: structural reference edges whose targets must already exist.
    for (name, def) in types {
        if let Some(refs) = def.get("token_refs").and_then(serde_json::Value::as_array) {
            for r in refs {
                if let Some(tgt) = r.as_str() {
                    builder = builder.edge(name, tgt, "token-ref", None)?;
                }
            }
        }
        if let Some(refs) = def.get("span_refs").and_then(serde_json::Value::as_array) {
            for r in refs {
                if let Some(tgt) = r.as_str() {
                    builder = builder.edge(name, tgt, "span-ref", None)?;
                }
            }
        }
        if let Some(refs) = def
            .get("parse_children")
            .and_then(serde_json::Value::as_array)
        {
            for r in refs {
                if let Some(tgt) = r.as_str() {
                    builder = builder.edge(name, tgt, "parse-child", None)?;
                }
            }
        }
        if let Some(refs) = def.get("graph_deps").and_then(serde_json::Value::as_array) {
            for r in refs {
                if let Some(tgt) = r.as_str() {
                    builder = builder.edge(name, tgt, "graph-dep", None)?;
                }
            }
        }
    }

    let schema = builder.build()?;
    Ok(schema)
}

/// Emit a [`Schema`] as a JSON Concrete schema.
///
/// # Errors
///
/// Returns [`ProtocolError`] if emission fails.
pub fn emit_concrete_schema(schema: &Schema) -> Result<serde_json::Value, ProtocolError> {
    // Only `contains` forms the nesting hierarchy that makes a vertex a
    // non-root.  The reference edges (token-ref, span-ref, parse-child,
    // graph-dep) are cross-references; their targets must still appear as
    // top-level entries in the emitted JSON so they can be re-parsed.
    let structural = &["contains"];
    let roots = find_roots(schema, structural);

    let mut types = serde_json::Map::new();
    for root in &roots {
        let mut obj = serde_json::Map::new();
        obj.insert("kind".into(), serde_json::json!(root.kind));

        let props = children_by_edge(schema, &root.id, "contains");
        // Exclude item vertices (IDs of the form "{parent}:item{n}") from fields.
        let field_props: Vec<_> = props
            .iter()
            .filter(|(_, child)| !child.id.contains(":item"))
            .collect();
        if !field_props.is_empty() {
            let mut fields = serde_json::Map::new();
            for (edge, child) in &field_props {
                let name = edge.name.as_deref().unwrap_or(&child.id);
                let mut field = serde_json::Map::new();
                field.insert("type".into(), serde_json::json!(child.kind));
                for c in vertex_constraints(schema, &child.id) {
                    field.insert(c.sort.clone(), serde_json::json!(c.value));
                }
                fields.insert(name.to_string(), serde_json::Value::Object(field));
            }
            obj.insert("fields".into(), serde_json::Value::Object(fields));
        }

        let items = children_by_edge(schema, &root.id, "contains");
        let item_arr: Vec<serde_json::Value> = items
            .iter()
            .filter_map(|(e, child)| {
                // Item vertices have IDs of the form "{parent}:item{n}" and their
                // edge name is the vertex kind, not a field name.  Emit only those.
                if child.id.contains(":item") {
                    e.name.as_deref().map(|n| serde_json::json!(n))
                } else {
                    None
                }
            })
            .collect();
        if !item_arr.is_empty() {
            obj.insert("items".into(), serde_json::Value::Array(item_arr));
        }

        // Emit structural reference edges as ID arrays.
        emit_ref_edges(schema, &root.id, "token-ref", "token_refs", &mut obj);
        emit_ref_edges(schema, &root.id, "span-ref", "span_refs", &mut obj);
        emit_ref_edges(schema, &root.id, "parse-child", "parse_children", &mut obj);
        emit_ref_edges(schema, &root.id, "graph-dep", "graph_deps", &mut obj);

        types.insert(root.id.clone(), serde_json::Value::Object(obj));
    }

    Ok(serde_json::json!({ "types": types }))
}

/// Collect outgoing edges of `edge_kind` from `vertex_id` and emit their
/// target IDs as a JSON array under `key` in `obj` (only when non-empty).
fn emit_ref_edges(
    schema: &Schema,
    vertex_id: &str,
    edge_kind: &str,
    key: &str,
    obj: &mut serde_json::Map<String, serde_json::Value>,
) {
    let refs: Vec<serde_json::Value> = schema
        .outgoing_edges(vertex_id)
        .iter()
        .filter(|e| e.kind == edge_kind)
        .map(|e| serde_json::json!(e.tgt))
        .collect();
    if !refs.is_empty() {
        obj.insert(key.into(), serde_json::Value::Array(refs));
    }
}

fn edge_rules() -> Vec<EdgeRule> {
    vec![
        // Containment: structural nesting following the Concrete hierarchy.
        // Any vertex kind can own fields (empty src_kinds = unrestricted).
        // Concrete uses fields on every struct type.
        EdgeRule {
            edge_kind: "contains".into(),
            src_kinds: vec![],
            tgt_kinds: vec![],
        },
        // token-ref: cross-layer reference from an annotation layer to a Token.
        // Sources: dependency-parse (DependencyParse.dependencyList govs/deps),
        // token-tagging (TaggedToken.tokenIndex), parse (Parse.constituentList
        // start/ending indices), entity-mention/situation-mention (via
        // TokenRefSequence).
        EdgeRule {
            edge_kind: "token-ref".into(),
            src_kinds: vec![
                "dependency-parse".into(),
                "token-tagging".into(),
                "parse".into(),
                "entity-mention".into(),
                "situation-mention".into(),
            ],
            tgt_kinds: vec!["token".into()],
        },
        // span-ref: reference from an annotation to a scalar span anchor.
        // Sources: entity-mention, situation-mention, token-tagging, parse,
        // section, sentence, token (textSpan / audioSpan fields).
        EdgeRule {
            edge_kind: "span-ref".into(),
            src_kinds: vec![
                "entity-mention".into(),
                "situation-mention".into(),
                "token-tagging".into(),
                "parse".into(),
                "section".into(),
                "sentence".into(),
                "token".into(),
            ],
            tgt_kinds: vec!["span".into()],
        },
        // parse-child: constituent → constituent tree edge inside a Parse.
        // (Constituent.childList holds indices into Parse.constituentList.)
        EdgeRule {
            edge_kind: "parse-child".into(),
            src_kinds: vec!["constituent".into()],
            tgt_kinds: vec!["constituent".into()],
        },
        // graph-dep: directed dependency arc inside a DependencyParse.
        // (Dependency.gov → Dependency.dep; both are token indices.)
        EdgeRule {
            edge_kind: "graph-dep".into(),
            src_kinds: vec!["dependency-parse".into()],
            tgt_kinds: vec!["dependency-parse".into()],
        },
    ]
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn protocol_def() {
        let p = protocol();
        assert_eq!(p.name, "concrete");
        assert_eq!(p.schema_theory, "ThConcreteSchema");
        assert_eq!(p.instance_theory, "ThConcreteInstance");
    }

    #[test]
    fn all_vertex_kinds_present() {
        let p = protocol();
        for kind in &[
            "communication",
            "section",
            "sentence",
            "tokenization",
            "token",
            "token-tagging",
            "dependency-parse",
            "constituent",
            "parse",
            "entity-mention",
            "entity-mention-set",
            "entity",
            "entity-set",
            "situation-mention",
            "situation-mention-set",
            "situation",
            "situation-set",
            "document-tag",
            "span",
            "string",
            "integer",
        ] {
            assert!(
                p.obj_kinds.contains(&kind.to_string()),
                "missing obj_kind: {kind}"
            );
        }
    }

    #[test]
    fn all_constraint_sorts_present() {
        let p = protocol();
        for sort in &[
            "uuid",
            "kind",
            "text",
            "tag",
            "confidence",
            "tool",
            "kbest-index",
            "timestamp",
            "role",
            "subkind",
        ] {
            assert!(
                p.constraint_sorts.contains(&sort.to_string()),
                "missing constraint sort: {sort}"
            );
        }
    }

    #[test]
    fn edge_rules_present() {
        let p = protocol();
        for rule_kind in &[
            "contains",
            "token-ref",
            "span-ref",
            "parse-child",
            "graph-dep",
        ] {
            assert!(
                p.find_edge_rule(rule_kind).is_some(),
                "missing edge rule: {rule_kind}"
            );
        }
    }

    #[test]
    fn parse_child_is_constituent_to_constituent() {
        let p = protocol();
        let rule = p.find_edge_rule("parse-child").unwrap();
        assert_eq!(
            rule.src_kinds,
            vec!["constituent".to_string()],
            "parse-child src must be constituent"
        );
        assert_eq!(
            rule.tgt_kinds,
            vec!["constituent".to_string()],
            "parse-child tgt must be constituent"
        );
    }

    #[test]
    fn token_ref_sources_include_tagging_and_parse() {
        let p = protocol();
        let rule = p.find_edge_rule("token-ref").unwrap();
        assert!(
            rule.src_kinds.contains(&"token-tagging".to_string()),
            "token-ref must allow token-tagging sources"
        );
        assert!(
            rule.src_kinds.contains(&"parse".to_string()),
            "token-ref must allow parse sources"
        );
        assert!(
            rule.src_kinds.contains(&"dependency-parse".to_string()),
            "token-ref must allow dependency-parse sources"
        );
    }

    #[test]
    fn span_ref_sources_include_tagging_and_parse() {
        let p = protocol();
        let rule = p.find_edge_rule("span-ref").unwrap();
        assert!(
            rule.src_kinds.contains(&"token-tagging".to_string()),
            "span-ref must allow token-tagging sources"
        );
        assert!(
            rule.src_kinds.contains(&"parse".to_string()),
            "span-ref must allow parse sources"
        );
    }

    #[test]
    fn graph_dep_is_dependency_parse_to_dependency_parse() {
        let p = protocol();
        let rule = p.find_edge_rule("graph-dep").unwrap();
        assert_eq!(
            rule.src_kinds,
            vec!["dependency-parse".to_string()],
            "graph-dep src must be dependency-parse"
        );
        assert_eq!(
            rule.tgt_kinds,
            vec!["dependency-parse".to_string()],
            "graph-dep tgt must be dependency-parse"
        );
    }

    #[test]
    fn register_theories_works() {
        let mut registry = HashMap::new();
        register_theories(&mut registry);
        assert!(registry.contains_key("ThConcreteSchema"));
        assert!(registry.contains_key("ThConcreteInstance"));
    }

    #[test]
    fn parse_and_emit() {
        let json = serde_json::json!({
            "types": {
                "comm1": {
                    "kind": "communication",
                    "fields": {
                        "text": {"type": "string", "uuid": "abc-123"}
                    },
                    "items": ["section"]
                }
            }
        });
        let schema = parse_concrete_schema(&json).expect("should parse");
        assert!(schema.has_vertex("comm1"));
        let emitted = emit_concrete_schema(&schema).expect("emit");
        let s2 = parse_concrete_schema(&emitted).expect("re-parse");
        assert_eq!(schema.vertex_count(), s2.vertex_count());
    }

    #[test]
    fn parse_and_emit_with_all_constraint_sorts() {
        let json = serde_json::json!({
            "types": {
                "sm1": {
                    "kind": "situation-mention",
                    "fields": {
                        "kind_field": {
                            "type": "string",
                            "tag": "EVENT",
                            "confidence": "0.95",
                            "tool": "my-tool",
                            "timestamp": "1234567890",
                            "role": "trigger",
                            "subkind": "motion"
                        }
                    }
                }
            }
        });
        let schema = parse_concrete_schema(&json).expect("should parse");
        assert!(schema.has_vertex("sm1"));
        assert!(schema.has_vertex("sm1.kind_field"));

        let emitted = emit_concrete_schema(&schema).expect("emit");
        let s2 = parse_concrete_schema(&emitted).expect("re-parse");
        assert_eq!(schema.vertex_count(), s2.vertex_count());
    }

    #[test]
    fn parse_child_edge_constituent_to_constituent() {
        // Rebuild with parse-child edges via the second-pass JSON encoding.
        let json2 = serde_json::json!({
            "types": {
                "root_c": {
                    "kind": "constituent",
                    "parse_children": ["child_c", "another_c"]
                },
                "child_c": {"kind": "constituent"},
                "another_c": {"kind": "constituent"}
            }
        });
        let schema = parse_concrete_schema(&json2).expect("parse with parse-child edges");
        let edges = schema.outgoing_edges("root_c");
        let parse_child_count = edges.iter().filter(|e| e.kind == "parse-child").count();
        assert_eq!(parse_child_count, 2, "expected 2 parse-child edges");
        assert!(schema.has_vertex("child_c"));
        assert!(schema.has_vertex("another_c"));
    }

    #[test]
    fn graph_dep_edge_roundtrip() {
        let json = serde_json::json!({
            "types": {
                "dp1": {
                    "kind": "dependency-parse",
                    "graph_deps": ["dp2"]
                },
                "dp2": {"kind": "dependency-parse"}
            }
        });
        let schema = parse_concrete_schema(&json).expect("parse with graph-dep edge");
        let edges = schema.outgoing_edges("dp1");
        assert!(
            edges.iter().any(|e| e.kind == "graph-dep"),
            "expected graph-dep edge from dp1"
        );

        let emitted = emit_concrete_schema(&schema).expect("emit");
        let s2 = parse_concrete_schema(&emitted).expect("re-parse");
        assert_eq!(schema.vertex_count(), s2.vertex_count());
        assert_eq!(schema.edge_count(), s2.edge_count());
    }

    #[test]
    fn entity_set_and_situation_set_kinds() {
        let json = serde_json::json!({
            "types": {
                "es1": {"kind": "entity-set"},
                "e1":  {"kind": "entity"},
                "ss1": {"kind": "situation-set"},
                "s1":  {"kind": "situation"},
                "dt1": {"kind": "document-tag"}
            }
        });
        let schema = parse_concrete_schema(&json).expect("should parse");
        assert_eq!(schema.vertices["es1"].kind, "entity-set");
        assert_eq!(schema.vertices["e1"].kind, "entity");
        assert_eq!(schema.vertices["ss1"].kind, "situation-set");
        assert_eq!(schema.vertices["s1"].kind, "situation");
        assert_eq!(schema.vertices["dt1"].kind, "document-tag");
    }
}
