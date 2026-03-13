//! HCL/Terraform protocol definition.
//!
//! Uses Group C theory: simple graph + flat.

use std::collections::HashMap;
use std::hash::BuildHasher;

use panproto_gat::Theory;
use panproto_schema::{EdgeRule, Protocol, Schema, SchemaBuilder};

use crate::emit::{IndentWriter, children_by_edge, constraint_value, find_roots};
use crate::error::ProtocolError;
use crate::theories;

/// Returns the HCL protocol definition.
#[must_use]
pub fn protocol() -> Protocol {
    Protocol {
        name: "hcl".into(),
        schema_theory: "ThHclSchema".into(),
        instance_theory: "ThHclInstance".into(),
        edge_rules: edge_rules(),
        obj_kinds: vec![
            "resource".into(),
            "data".into(),
            "variable".into(),
            "output".into(),
            "block".into(),
            "attribute".into(),
            "string".into(),
            "number".into(),
            "bool".into(),
            "list".into(),
            "map".into(),
            "set".into(),
            "object".into(),
            "tuple".into(),
        ],
        constraint_sorts: vec!["default".into(), "description".into(), "type".into()],
    }
}

/// Register the component GATs for HCL.
pub fn register_theories<S: BuildHasher>(registry: &mut HashMap<String, Theory, S>) {
    theories::register_simple_graph_flat(registry, "ThHclSchema", "ThHclInstance");
}

/// Parse HCL block syntax into a [`Schema`].
///
/// Expects simplified syntax:
/// ```text
/// resource "aws_instance" "example" {
///   ami           = string
///   instance_type = string
/// }
/// variable "region" {
///   type    = string
///   default = "us-east-1"
/// }
/// ```
///
/// # Errors
///
/// Returns [`ProtocolError`] if parsing fails.
pub fn parse_hcl(input: &str) -> Result<Schema, ProtocolError> {
    let proto = protocol();
    let mut builder = SchemaBuilder::new(&proto);

    let lines: Vec<&str> = input.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let trimmed = lines[i].trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with("//") {
            i += 1;
            continue;
        }

        let block_kind = if trimmed.starts_with("resource ") {
            Some("resource")
        } else if trimmed.starts_with("data ") {
            Some("data")
        } else if trimmed.starts_with("variable ") {
            Some("variable")
        } else if trimmed.starts_with("output ") {
            Some("output")
        } else {
            None
        };

        if let Some(kind) = block_kind {
            let (b, new_i) = parse_hcl_block(builder, &lines, i, kind)?;
            builder = b;
            i = new_i;
        } else {
            i += 1;
        }
    }

    let schema = builder.build()?;
    Ok(schema)
}

/// Parse a single HCL block.
fn parse_hcl_block(
    mut builder: SchemaBuilder,
    lines: &[&str],
    start: usize,
    kind: &str,
) -> Result<(SchemaBuilder, usize), ProtocolError> {
    let trimmed = lines[start].trim();

    // Extract quoted labels from the block header (e.g., "aws_instance" "web").
    let labels: Vec<&str> = trimmed
        .split('"')
        .enumerate()
        .filter_map(|(i, s)| {
            // Odd-indexed segments are inside quotes.
            if i % 2 == 1 && !s.is_empty() {
                Some(s)
            } else {
                None
            }
        })
        .collect();

    let block_id = if labels.len() >= 2 {
        format!("{}.{}", labels[0], labels[1])
    } else if let Some(label) = labels.first() {
        (*label).to_string()
    } else {
        kind.to_string()
    };

    builder = builder.vertex(&block_id, kind, None)?;

    let has_brace = trimmed.contains('{');
    if !has_brace {
        return Ok((builder, start + 1));
    }

    let mut i = start + 1;
    while i < lines.len() {
        let line = lines[i].trim();
        if line == "}" || line.starts_with('}') {
            return Ok((builder, i + 1));
        }

        if !line.is_empty() && !line.starts_with('#') && !line.starts_with("//") {
            // Parse attribute: name = type_or_value
            if let Some(eq_idx) = line.find('=') {
                let attr_name = line[..eq_idx].trim();
                let attr_value = line[eq_idx + 1..].trim();
                let attr_id = format!("{block_id}.{attr_name}");

                // Special handling for known meta-attributes.
                match attr_name {
                    "default" => {
                        builder = builder.constraint(&block_id, "default", attr_value);
                    }
                    "description" => {
                        let val = attr_value.trim_matches('"');
                        builder = builder.constraint(&block_id, "description", val);
                    }
                    "type" if kind == "variable" => {
                        builder = builder.constraint(&block_id, "type", attr_value);
                    }
                    _ => {
                        let attr_kind = hcl_type_to_kind(attr_value);
                        builder = builder.vertex(&attr_id, attr_kind, None)?;
                        builder = builder.edge(&block_id, &attr_id, "prop", Some(attr_name))?;
                    }
                }
            }
        }

        i += 1;
    }

    Ok((builder, i))
}

/// Map HCL type to vertex kind.
fn hcl_type_to_kind(type_str: &str) -> &'static str {
    match type_str.trim().trim_matches('"') {
        "string" => "string",
        "number" => "number",
        "bool" => "bool",
        "list" => "list",
        "map" => "map",
        "set" => "set",
        "object" => "object",
        "tuple" => "tuple",
        _ => "attribute",
    }
}

/// Map vertex kind to HCL type.
fn kind_to_hcl_type(kind: &str) -> &'static str {
    match kind {
        "string" => "string",
        "number" => "number",
        "bool" => "bool",
        "list" => "list",
        "map" => "map",
        "set" => "set",
        "object" => "object",
        "tuple" => "tuple",
        _ => "string",
    }
}

/// Emit a [`Schema`] as HCL text.
///
/// # Errors
///
/// Returns [`ProtocolError`] if emission fails.
pub fn emit_hcl(schema: &Schema) -> Result<String, ProtocolError> {
    let structural = &["prop"];
    let roots = find_roots(schema, structural);

    let mut w = IndentWriter::new("  ");

    for root in &roots {
        let parts: Vec<&str> = root.id.splitn(2, '.').collect();
        if parts.len() == 2 {
            w.line(&format!("{} \"{}\" {{", root.kind, parts[1]));
        } else {
            w.line(&format!("{} \"{}\" {{", root.kind, root.id));
        }
        w.indent();

        if let Some(desc) = constraint_value(schema, &root.id, "description") {
            w.line(&format!("description = \"{desc}\""));
        }
        if let Some(t) = constraint_value(schema, &root.id, "type") {
            w.line(&format!("type    = {t}"));
        }
        if let Some(d) = constraint_value(schema, &root.id, "default") {
            w.line(&format!("default = {d}"));
        }

        let attrs = children_by_edge(schema, &root.id, "prop");
        for (edge, child) in &attrs {
            let name = edge.name.as_deref().unwrap_or(&child.id);
            let type_str = kind_to_hcl_type(&child.kind);
            w.line(&format!("{name} = {type_str}"));
        }

        w.dedent();
        w.line("}");
        w.blank();
    }

    Ok(w.finish())
}

fn edge_rules() -> Vec<EdgeRule> {
    vec![EdgeRule {
        edge_kind: "prop".into(),
        src_kinds: vec![
            "resource".into(),
            "data".into(),
            "variable".into(),
            "output".into(),
            "block".into(),
        ],
        tgt_kinds: vec![],
    }]
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn protocol_def() {
        let p = protocol();
        assert_eq!(p.name, "hcl");
    }

    #[test]
    fn register_theories_works() {
        let mut registry = HashMap::new();
        register_theories(&mut registry);
        assert!(registry.contains_key("ThHclSchema"));
    }

    #[test]
    fn parse_and_emit() {
        let input = r#"
resource "aws_instance" "web" {
  ami           = string
  instance_type = string
}

variable "region" {
  type    = string
  default = "us-east-1"
}
"#;
        let schema = parse_hcl(input).expect("should parse");
        assert!(schema.has_vertex("aws_instance.web"));
        assert!(schema.has_vertex("aws_instance.web.ami"));

        let emitted = emit_hcl(&schema).expect("should emit");
        assert!(emitted.contains("ami"));
    }

    #[test]
    fn roundtrip() {
        let input = "resource \"test\" \"r\" {\n  name = string\n}\n";
        let s1 = parse_hcl(input).expect("parse");
        let emitted = emit_hcl(&s1).expect("emit");
        let s2 = parse_hcl(&emitted).expect("re-parse");
        assert_eq!(s1.vertex_count(), s2.vertex_count());
    }
}
