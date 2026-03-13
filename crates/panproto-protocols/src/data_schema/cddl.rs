//! CDDL (RFC 8610) protocol definition.
//!
//! Uses Group A theory: constrained multigraph + W-type.

use std::collections::HashMap;
use std::hash::BuildHasher;

use panproto_gat::Theory;
use panproto_schema::{EdgeRule, Protocol, Schema, SchemaBuilder};

use crate::emit::{IndentWriter, children_by_edge, find_roots};
use crate::error::ProtocolError;
use crate::theories;

/// Returns the CDDL protocol definition.
#[must_use]
pub fn protocol() -> Protocol {
    Protocol {
        name: "cddl".into(),
        schema_theory: "ThCddlSchema".into(),
        instance_theory: "ThCddlInstance".into(),
        edge_rules: edge_rules(),
        obj_kinds: vec![
            "rule".into(),
            "group".into(),
            "map".into(),
            "array".into(),
            "choice".into(),
            "uint".into(),
            "int".into(),
            "float".into(),
            "tstr".into(),
            "bstr".into(),
            "bool".into(),
            "null".into(),
            "any".into(),
            "bytes".into(),
            "text".into(),
            "tagged".into(),
        ],
        constraint_sorts: vec!["size".into(), "range".into(), "default".into()],
    }
}

/// Register the component GATs for CDDL.
pub fn register_theories<S: BuildHasher>(registry: &mut HashMap<String, Theory, S>) {
    theories::register_constrained_multigraph_wtype(registry, "ThCddlSchema", "ThCddlInstance");
}

/// Parse text-based CDDL rules into a [`Schema`].
///
/// Expects syntax like:
/// ```text
/// person = {
///   name: tstr,
///   age: uint,
/// }
/// ```
///
/// # Errors
///
/// Returns [`ProtocolError`] if parsing fails.
pub fn parse_cddl(input: &str) -> Result<Schema, ProtocolError> {
    let proto = protocol();
    let mut builder = SchemaBuilder::new(&proto);

    // Simplistic line-by-line parser for CDDL rules.
    let lines: Vec<&str> = input.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let trimmed = lines[i].trim();
        if trimmed.is_empty() || trimmed.starts_with(';') {
            i += 1;
            continue;
        }

        // Look for rule definition: name = ...
        if let Some(eq_idx) = trimmed.find(" = ") {
            let rule_name = trimmed[..eq_idx].trim();
            let rhs = trimmed[eq_idx + 3..].trim();

            if rhs.starts_with('{') {
                // Map rule.
                builder = builder.vertex(rule_name, "map", None)?;
                let (b, new_i) = parse_cddl_members(builder, &lines, i, rule_name, "prop")?;
                builder = b;
                i = new_i;
            } else if rhs.starts_with('[') {
                // Array rule.
                builder = builder.vertex(rule_name, "array", None)?;
                let (b, new_i) = parse_cddl_members(builder, &lines, i, rule_name, "items")?;
                builder = b;
                i = new_i;
            } else if rhs.contains('/') {
                // Choice rule.
                builder = builder.vertex(rule_name, "choice", None)?;
                for (vi, variant) in rhs.split('/').enumerate() {
                    let variant = variant.trim().trim_end_matches(',');
                    if !variant.is_empty() {
                        let variant_id = format!("{rule_name}:variant{vi}");
                        let kind = cddl_type_to_kind(variant);
                        builder = builder.vertex(&variant_id, kind, None)?;
                        builder = builder.edge(rule_name, &variant_id, "variant", Some(variant))?;
                    }
                }
                i += 1;
            } else {
                // Simple type alias.
                let kind = cddl_type_to_kind(rhs.trim_end_matches(','));
                builder = builder.vertex(rule_name, kind, None)?;
                i += 1;
            }
        } else {
            i += 1;
        }
    }

    let schema = builder.build()?;
    Ok(schema)
}

/// Parse map/array members inside braces or brackets.
fn parse_cddl_members(
    mut builder: SchemaBuilder,
    lines: &[&str],
    start: usize,
    parent_id: &str,
    edge_kind: &str,
) -> Result<(SchemaBuilder, usize), ProtocolError> {
    let mut i = start;
    // Skip the opening line and start looking for members.
    let first_line = lines[i].trim();
    let after_eq = first_line
        .find(" = ")
        .map_or(first_line, |idx| &first_line[idx + 3..])
        .trim();
    // If members are on the same line after '{', parse them.
    let content_after_brace = after_eq.trim_start_matches('{').trim_start_matches('[');
    if !content_after_brace.is_empty()
        && !content_after_brace.starts_with('}')
        && !content_after_brace.starts_with(']')
    {
        builder = parse_cddl_member_line(builder, content_after_brace, parent_id, edge_kind)?;
    }

    i += 1;
    while i < lines.len() {
        let line = lines[i].trim();
        if line.starts_with('}') || line.starts_with(']') {
            return Ok((builder, i + 1));
        }
        if !line.is_empty() && !line.starts_with(';') {
            builder = parse_cddl_member_line(builder, line, parent_id, edge_kind)?;
        }
        i += 1;
    }

    Ok((builder, i))
}

/// Parse a single member line like `name: tstr,`.
fn parse_cddl_member_line(
    builder: SchemaBuilder,
    line: &str,
    parent_id: &str,
    edge_kind: &str,
) -> Result<SchemaBuilder, ProtocolError> {
    let line = line
        .trim()
        .trim_end_matches(',')
        .trim_end_matches('}')
        .trim_end_matches(']')
        .trim();
    if line.is_empty() {
        return Ok(builder);
    }

    // Handle "? key: type" for optional, "key: type" for required.
    let line = line.trim_start_matches("? ").trim_start_matches('?');

    if let Some(colon_idx) = line.find(':') {
        let member_name = line[..colon_idx].trim().trim_matches('"');
        let type_str = line[colon_idx + 1..].trim();
        let member_id = format!("{parent_id}.{member_name}");
        let kind = cddl_type_to_kind(type_str);
        let b = builder.vertex(&member_id, kind, None)?;
        let b = b.edge(parent_id, &member_id, edge_kind, Some(member_name))?;
        return Ok(b);
    }

    Ok(builder)
}

/// Map CDDL type to vertex kind.
fn cddl_type_to_kind(type_str: &str) -> &'static str {
    match type_str.trim() {
        "uint" => "uint",
        "int" | "nint" => "int",
        "float" | "float16" | "float32" | "float64" => "float",
        "tstr" | "text" => "tstr",
        "bstr" | "bytes" => "bstr",
        "bool" => "bool",
        "null" | "nil" => "null",
        "any" => "any",
        _ => "rule",
    }
}

/// Map vertex kind to CDDL type.
fn kind_to_cddl_type(kind: &str) -> &'static str {
    match kind {
        "uint" => "uint",
        "int" => "int",
        "float" => "float",
        "tstr" | "text" => "tstr",
        "bstr" | "bytes" => "bstr",
        "bool" => "bool",
        "null" => "null",
        "any" => "any",
        _ => "any",
    }
}

/// Emit a [`Schema`] as CDDL text.
///
/// # Errors
///
/// Returns [`ProtocolError`] if emission fails.
pub fn emit_cddl(schema: &Schema) -> Result<String, ProtocolError> {
    let structural = &["prop", "items", "variant"];
    let roots = find_roots(schema, structural);

    let mut w = IndentWriter::new("  ");

    for root in &roots {
        match root.kind.as_str() {
            "map" => {
                w.line(&format!("{} = {{", root.id));
                w.indent();
                let props = children_by_edge(schema, &root.id, "prop");
                for (edge, child) in &props {
                    let name = edge.name.as_deref().unwrap_or(&child.id);
                    let type_str = kind_to_cddl_type(&child.kind);
                    w.line(&format!("{name}: {type_str},"));
                }
                w.dedent();
                w.line("}");
                w.blank();
            }
            "array" => {
                w.line(&format!("{} = [", root.id));
                w.indent();
                let items = children_by_edge(schema, &root.id, "items");
                for (_, child) in &items {
                    let type_str = kind_to_cddl_type(&child.kind);
                    w.line(&format!("{type_str},"));
                }
                w.dedent();
                w.line("]");
                w.blank();
            }
            "choice" => {
                let variants = children_by_edge(schema, &root.id, "variant");
                let variant_strs: Vec<&str> = variants
                    .iter()
                    .map(|(_, child)| kind_to_cddl_type(&child.kind))
                    .collect();
                w.line(&format!("{} = {}", root.id, variant_strs.join(" / ")));
                w.blank();
            }
            _ => {
                let type_str = kind_to_cddl_type(&root.kind);
                w.line(&format!("{} = {type_str}", root.id));
                w.blank();
            }
        }
    }

    Ok(w.finish())
}

fn edge_rules() -> Vec<EdgeRule> {
    vec![
        EdgeRule {
            edge_kind: "prop".into(),
            src_kinds: vec!["map".into(), "group".into(), "rule".into()],
            tgt_kinds: vec![],
        },
        EdgeRule {
            edge_kind: "items".into(),
            src_kinds: vec!["array".into()],
            tgt_kinds: vec![],
        },
        EdgeRule {
            edge_kind: "variant".into(),
            src_kinds: vec!["choice".into()],
            tgt_kinds: vec![],
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
        assert_eq!(p.name, "cddl");
    }

    #[test]
    fn register_theories_works() {
        let mut registry = HashMap::new();
        register_theories(&mut registry);
        assert!(registry.contains_key("ThCddlSchema"));
        assert!(registry.contains_key("ThCddlInstance"));
    }

    #[test]
    fn parse_simple() {
        let input = r"
person = {
  name: tstr,
  age: uint,
}
";
        let schema = parse_cddl(input).expect("should parse");
        assert!(schema.has_vertex("person"));
        assert!(schema.has_vertex("person.name"));
        assert!(schema.has_vertex("person.age"));
    }

    #[test]
    fn parse_choice() {
        let input = "value = tstr / uint / bool\n";
        let schema = parse_cddl(input).expect("should parse");
        assert!(schema.has_vertex("value"));
    }

    #[test]
    fn roundtrip() {
        let input = "mytype = tstr\n";
        let s1 = parse_cddl(input).expect("parse");
        let emitted = emit_cddl(&s1).expect("emit");
        let s2 = parse_cddl(&emitted).expect("re-parse");
        assert_eq!(s1.vertex_count(), s2.vertex_count());
    }
}
