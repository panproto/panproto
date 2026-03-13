//! Neo4j graph database protocol definition.
//!
//! Uses Group E theory: constrained multigraph + W-type + metadata.

use std::collections::HashMap;
use std::hash::BuildHasher;

use panproto_gat::Theory;
use panproto_schema::{EdgeRule, Protocol, Schema, SchemaBuilder};

use crate::emit::{IndentWriter, children_by_edge, constraint_value, find_roots};
use crate::error::ProtocolError;
use crate::theories;

/// Returns the Neo4j protocol definition.
#[must_use]
pub fn protocol() -> Protocol {
    Protocol {
        name: "neo4j".into(),
        schema_theory: "ThNeo4jSchema".into(),
        instance_theory: "ThNeo4jInstance".into(),
        edge_rules: edge_rules(),
        obj_kinds: vec![
            "node-label".into(),
            "relationship-type".into(),
            "property".into(),
            "string".into(),
            "integer".into(),
            "float".into(),
            "boolean".into(),
            "date".into(),
            "datetime".into(),
            "point".into(),
            "duration".into(),
            "list".into(),
        ],
        constraint_sorts: vec![
            "unique".into(),
            "exists".into(),
            "node-key".into(),
            "type".into(),
        ],
    }
}

/// Register the component GATs for Neo4j.
pub fn register_theories<S: BuildHasher>(registry: &mut HashMap<String, Theory, S>) {
    theories::register_multigraph_wtype_meta(registry, "ThNeo4jSchema", "ThNeo4jInstance");
}

/// Parse Cypher constraint/index DDL into a [`Schema`].
///
/// Expects Cypher DDL statements like:
/// ```text
/// CREATE CONSTRAINT ON (p:Person) ASSERT p.name IS UNIQUE;
/// CREATE (n:Person {name: STRING, age: INTEGER});
/// ```
///
/// # Errors
///
/// Returns [`ProtocolError`] if parsing fails.
pub fn parse_cypher_schema(input: &str) -> Result<Schema, ProtocolError> {
    let proto = protocol();
    let mut builder = SchemaBuilder::new(&proto);
    let mut labels_seen: std::collections::HashSet<String> = std::collections::HashSet::new();

    for line in input.lines() {
        let trimmed = line.trim().trim_end_matches(';');
        if trimmed.is_empty() || trimmed.starts_with("//") {
            continue;
        }

        if trimmed.starts_with("CREATE CONSTRAINT") {
            builder = parse_cypher_constraint(builder, trimmed, &mut labels_seen)?;
        } else if trimmed.starts_with("CREATE") {
            builder = parse_cypher_create(builder, trimmed, &mut labels_seen)?;
        }
    }

    let schema = builder.build()?;
    Ok(schema)
}

/// Parse a CREATE statement defining nodes/relationships with properties.
fn parse_cypher_create(
    mut builder: SchemaBuilder,
    line: &str,
    labels_seen: &mut std::collections::HashSet<String>,
) -> Result<SchemaBuilder, ProtocolError> {
    // Match patterns like (n:Label {prop: TYPE, ...}) or -[:REL_TYPE {prop: TYPE}]->
    let mut rest = line.strip_prefix("CREATE").unwrap_or(line).trim();

    while !rest.is_empty() {
        if let Some(paren_start) = rest.find('(') {
            if let Some(paren_end) = rest[paren_start..].find(')') {
                let inside = &rest[paren_start + 1..paren_start + paren_end];
                builder = parse_node_pattern(builder, inside, labels_seen)?;
                rest = &rest[paren_start + paren_end + 1..];
                continue;
            }
        }
        if let Some(bracket_start) = rest.find('[') {
            if let Some(bracket_end) = rest[bracket_start..].find(']') {
                let inside = &rest[bracket_start + 1..bracket_start + bracket_end];
                builder = parse_rel_pattern(builder, inside, labels_seen)?;
                rest = &rest[bracket_start + bracket_end + 1..];
                continue;
            }
        }
        break;
    }

    Ok(builder)
}

/// Parse a node pattern like `n:Person {name: STRING, age: INTEGER}`.
fn parse_node_pattern(
    mut builder: SchemaBuilder,
    pattern: &str,
    labels_seen: &mut std::collections::HashSet<String>,
) -> Result<SchemaBuilder, ProtocolError> {
    let (label_part, props_part) = if let Some(brace_start) = pattern.find('{') {
        let brace_end = pattern.rfind('}').unwrap_or(pattern.len());
        (
            &pattern[..brace_start],
            Some(&pattern[brace_start + 1..brace_end]),
        )
    } else {
        (pattern, None)
    };

    // Extract label after ':'.
    let label = label_part.split(':').nth(1).map_or("", str::trim).trim();
    if label.is_empty() {
        return Ok(builder);
    }

    if !labels_seen.contains(label) {
        builder = builder.vertex(label, "node-label", None)?;
        labels_seen.insert(label.to_string());
    }

    if let Some(props) = props_part {
        for prop in props.split(',') {
            let prop = prop.trim();
            if let Some((name, type_str)) = prop.split_once(':') {
                let name = name.trim();
                let type_str = type_str.trim();
                let prop_id = format!("{label}.{name}");
                let kind = cypher_type_to_kind(type_str);
                if !labels_seen.contains(&prop_id) {
                    builder = builder.vertex(&prop_id, kind, None)?;
                    builder = builder.edge(label, &prop_id, "prop", Some(name))?;
                    labels_seen.insert(prop_id);
                }
            }
        }
    }

    Ok(builder)
}

/// Parse a relationship pattern like `:KNOWS {since: DATE}`.
fn parse_rel_pattern(
    mut builder: SchemaBuilder,
    pattern: &str,
    labels_seen: &mut std::collections::HashSet<String>,
) -> Result<SchemaBuilder, ProtocolError> {
    let (label_part, props_part) = if let Some(brace_start) = pattern.find('{') {
        let brace_end = pattern.rfind('}').unwrap_or(pattern.len());
        (
            &pattern[..brace_start],
            Some(&pattern[brace_start + 1..brace_end]),
        )
    } else {
        (pattern, None)
    };

    let rel_type = label_part.split(':').nth(1).map_or("", str::trim).trim();
    if rel_type.is_empty() {
        return Ok(builder);
    }

    if !labels_seen.contains(rel_type) {
        builder = builder.vertex(rel_type, "relationship-type", None)?;
        labels_seen.insert(rel_type.to_string());
    }

    if let Some(props) = props_part {
        for prop in props.split(',') {
            let prop = prop.trim();
            if let Some((name, type_str)) = prop.split_once(':') {
                let name = name.trim();
                let type_str = type_str.trim();
                let prop_id = format!("{rel_type}.{name}");
                let kind = cypher_type_to_kind(type_str);
                if !labels_seen.contains(&prop_id) {
                    builder = builder.vertex(&prop_id, kind, None)?;
                    builder = builder.edge(rel_type, &prop_id, "prop", Some(name))?;
                    labels_seen.insert(prop_id);
                }
            }
        }
    }

    Ok(builder)
}

/// Parse a CREATE CONSTRAINT statement.
fn parse_cypher_constraint(
    mut builder: SchemaBuilder,
    line: &str,
    labels_seen: &mut std::collections::HashSet<String>,
) -> Result<SchemaBuilder, ProtocolError> {
    // Extract label from (n:Label).
    if let Some(paren_start) = line.find('(') {
        if let Some(paren_end) = line[paren_start..].find(')') {
            let inside = &line[paren_start + 1..paren_start + paren_end];
            let label = inside.split(':').nth(1).map_or("", str::trim).trim();
            if !label.is_empty() {
                if !labels_seen.contains(label) {
                    builder = builder.vertex(label, "node-label", None)?;
                    labels_seen.insert(label.to_string());
                }

                // Determine constraint type.
                let upper = line.to_uppercase();
                if upper.contains("IS UNIQUE") {
                    // Extract property name.
                    if let Some(prop_name) = extract_constraint_property(line, label) {
                        let prop_id = format!("{label}.{prop_name}");
                        if !labels_seen.contains(&prop_id) {
                            builder = builder.vertex(&prop_id, "property", None)?;
                            builder = builder.edge(label, &prop_id, "prop", Some(&prop_name))?;
                            labels_seen.insert(prop_id.clone());
                        }
                        builder = builder.constraint(&prop_id, "unique", "true");
                    }
                } else if upper.contains("EXISTS") {
                    if let Some(prop_name) = extract_constraint_property(line, label) {
                        let prop_id = format!("{label}.{prop_name}");
                        if !labels_seen.contains(&prop_id) {
                            builder = builder.vertex(&prop_id, "property", None)?;
                            builder = builder.edge(label, &prop_id, "prop", Some(&prop_name))?;
                            labels_seen.insert(prop_id.clone());
                        }
                        builder = builder.constraint(&prop_id, "exists", "true");
                    }
                }
            }
        }
    }

    Ok(builder)
}

/// Extract property name from ASSERT clause.
fn extract_constraint_property(line: &str, _label: &str) -> Option<String> {
    // Look for "ASSERT x.propname" pattern.
    let upper = line.to_uppercase();
    if let Some(assert_idx) = upper.find("ASSERT") {
        let after = &line[assert_idx + "ASSERT".len()..].trim();
        if let Some(dot_idx) = after.find('.') {
            let after_dot = &after[dot_idx + 1..];
            let prop_name = after_dot.split_whitespace().next().unwrap_or("").trim();
            if !prop_name.is_empty() {
                return Some(prop_name.to_string());
            }
        }
    }
    None
}

/// Map Cypher type to vertex kind.
fn cypher_type_to_kind(type_str: &str) -> &'static str {
    match type_str.to_uppercase().as_str() {
        "STRING" => "string",
        "INTEGER" | "INT" | "LONG" => "integer",
        "FLOAT" | "DOUBLE" => "float",
        "BOOLEAN" | "BOOL" => "boolean",
        "DATE" => "date",
        "DATETIME" | "LOCALDATETIME" | "ZONEDDATETIME" => "datetime",
        "POINT" => "point",
        "DURATION" => "duration",
        "LIST" => "list",
        _ => "property",
    }
}

/// Map vertex kind to Cypher type.
fn kind_to_cypher_type(kind: &str) -> &'static str {
    match kind {
        "string" => "STRING",
        "integer" => "INTEGER",
        "float" => "FLOAT",
        "boolean" => "BOOLEAN",
        "date" => "DATE",
        "datetime" => "DATETIME",
        "point" => "POINT",
        "duration" => "DURATION",
        "list" => "LIST",
        _ => "STRING",
    }
}

/// Emit a [`Schema`] as Cypher DDL statements.
///
/// # Errors
///
/// Returns [`ProtocolError`] if emission fails.
pub fn emit_cypher_schema(schema: &Schema) -> Result<String, ProtocolError> {
    let structural = &["prop", "ref"];
    let roots = find_roots(schema, structural);

    let mut w = IndentWriter::new("  ");

    for root in &roots {
        match root.kind.as_str() {
            "node-label" => {
                let props = children_by_edge(schema, &root.id, "prop");
                if props.is_empty() {
                    w.line(&format!("CREATE (n:{});", root.id));
                } else {
                    let prop_strs: Vec<String> = props
                        .iter()
                        .map(|(edge, child)| {
                            let name = edge.name.as_deref().unwrap_or(&child.id);
                            let type_str = kind_to_cypher_type(&child.kind);
                            format!("{name}: {type_str}")
                        })
                        .collect();
                    w.line(&format!(
                        "CREATE (n:{} {{{}}});",
                        root.id,
                        prop_strs.join(", ")
                    ));
                }

                // Emit constraints.
                for (edge, child) in &props {
                    let name = edge.name.as_deref().unwrap_or(&child.id);
                    if constraint_value(schema, &child.id, "unique") == Some("true") {
                        w.line(&format!(
                            "CREATE CONSTRAINT ON (n:{}) ASSERT n.{} IS UNIQUE;",
                            root.id, name
                        ));
                    }
                    if constraint_value(schema, &child.id, "exists") == Some("true") {
                        w.line(&format!(
                            "CREATE CONSTRAINT ON (n:{}) ASSERT EXISTS(n.{});",
                            root.id, name
                        ));
                    }
                }
            }
            "relationship-type" => {
                let props = children_by_edge(schema, &root.id, "prop");
                if props.is_empty() {
                    w.line(&format!("// Relationship type: {}", root.id));
                } else {
                    let prop_strs: Vec<String> = props
                        .iter()
                        .map(|(edge, child)| {
                            let name = edge.name.as_deref().unwrap_or(&child.id);
                            let type_str = kind_to_cypher_type(&child.kind);
                            format!("{name}: {type_str}")
                        })
                        .collect();
                    w.line(&format!(
                        "// Relationship type: {} {{{}}}",
                        root.id,
                        prop_strs.join(", ")
                    ));
                }
            }
            _ => {}
        }
    }

    Ok(w.finish())
}

fn edge_rules() -> Vec<EdgeRule> {
    vec![
        EdgeRule {
            edge_kind: "prop".into(),
            src_kinds: vec!["node-label".into(), "relationship-type".into()],
            tgt_kinds: vec![],
        },
        EdgeRule {
            edge_kind: "ref".into(),
            src_kinds: vec!["relationship-type".into()],
            tgt_kinds: vec!["node-label".into()],
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
        assert_eq!(p.name, "neo4j");
    }

    #[test]
    fn register_theories_works() {
        let mut registry = HashMap::new();
        register_theories(&mut registry);
        assert!(registry.contains_key("ThNeo4jSchema"));
        assert!(registry.contains_key("ThNeo4jInstance"));
    }

    #[test]
    fn parse_cypher() {
        let cypher = r"
CREATE (n:Person {name: STRING, age: INTEGER});
CREATE CONSTRAINT ON (p:Person) ASSERT p.name IS UNIQUE;
";
        let schema = parse_cypher_schema(cypher).expect("should parse");
        assert!(schema.has_vertex("Person"));
        assert!(schema.has_vertex("Person.name"));
        assert!(schema.has_vertex("Person.age"));
    }

    #[test]
    fn emit_cypher() {
        let cypher = "CREATE (n:User {email: STRING});\n";
        let schema = parse_cypher_schema(cypher).expect("parse");
        let emitted = emit_cypher_schema(&schema).expect("emit");
        assert!(emitted.contains("User"));
        assert!(emitted.contains("email"));
    }
}
