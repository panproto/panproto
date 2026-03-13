//! JSX/React component props protocol definition.
//!
//! Uses Group D theory: typed graph + W-type with interfaces.

use std::collections::HashMap;
use std::hash::BuildHasher;

use panproto_gat::Theory;
use panproto_schema::{EdgeRule, Protocol, Schema, SchemaBuilder};

use crate::emit::{IndentWriter, children_by_edge, constraint_value, find_roots};
use crate::error::ProtocolError;
use crate::theories;

/// Returns the JSX/React protocol definition.
#[must_use]
pub fn protocol() -> Protocol {
    Protocol {
        name: "jsx".into(),
        schema_theory: "ThJsxSchema".into(),
        instance_theory: "ThJsxInstance".into(),
        edge_rules: edge_rules(),
        obj_kinds: vec![
            "component".into(),
            "prop".into(),
            "event-handler".into(),
            "children".into(),
            "element".into(),
            "fragment".into(),
            "context".into(),
            "hook".into(),
            "string".into(),
            "number".into(),
            "boolean".into(),
            "node".into(),
            "element-type".into(),
            "function".into(),
            "array".into(),
        ],
        constraint_sorts: vec!["required".into(), "default".into(), "generic".into()],
    }
}

/// Register the component GATs for JSX/React.
pub fn register_theories<S: BuildHasher>(registry: &mut HashMap<String, Theory, S>) {
    theories::register_typed_graph_wtype(registry, "ThJsxSchema", "ThJsxInstance");
}

/// Parse a TSX-subset text defining component props into a [`Schema`].
///
/// Expects simplified syntax:
/// ```text
/// component Button {
///   prop label: string required
///   prop onClick: function
///   prop children: node
/// }
/// ```
///
/// # Errors
///
/// Returns [`ProtocolError`] if parsing fails.
pub fn parse_jsx(input: &str) -> Result<Schema, ProtocolError> {
    let proto = protocol();
    let mut builder = SchemaBuilder::new(&proto);

    let lines: Vec<&str> = input.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let trimmed = lines[i].trim();
        if trimmed.starts_with("component ") {
            let (b, new_i) = parse_component(builder, &lines, i)?;
            builder = b;
            i = new_i;
        } else {
            i += 1;
        }
    }

    let schema = builder.build()?;
    Ok(schema)
}

/// Parse a component block.
fn parse_component(
    mut builder: SchemaBuilder,
    lines: &[&str],
    start: usize,
) -> Result<(SchemaBuilder, usize), ProtocolError> {
    let trimmed = lines[start].trim();
    let name = trimmed
        .strip_prefix("component ")
        .and_then(|s| s.split('{').next())
        .map(str::trim)
        .ok_or_else(|| ProtocolError::Parse("invalid component declaration".into()))?;

    builder = builder.vertex(name, "component", None)?;

    let mut i = start + 1;
    while i < lines.len() {
        let line = lines[i].trim();
        if line == "}" || line.starts_with('}') {
            return Ok((builder, i + 1));
        }

        if line.starts_with("prop ") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 3 {
                let prop_name = parts[1].trim_end_matches(':');
                let type_str = parts[2];
                let prop_id = format!("{name}.{prop_name}");
                let kind = jsx_type_to_kind(type_str);
                builder = builder.vertex(&prop_id, kind, None)?;
                builder = builder.edge(name, &prop_id, "prop", Some(prop_name))?;

                if parts.contains(&"required") {
                    builder = builder.constraint(&prop_id, "required", "true");
                }
                if let Some(default_pos) = parts.iter().position(|p| p.starts_with("default=")) {
                    let default_val = parts[default_pos].strip_prefix("default=").unwrap_or("");
                    builder = builder.constraint(&prop_id, "default", default_val);
                }
            }
        } else if line.starts_with("event ") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                let event_name = parts[1].trim_end_matches(':');
                let event_id = format!("{name}.{event_name}");
                builder = builder.vertex(&event_id, "event-handler", None)?;
                builder = builder.edge(name, &event_id, "prop", Some(event_name))?;
            }
        }

        i += 1;
    }

    Ok((builder, i))
}

/// Map type name to vertex kind.
fn jsx_type_to_kind(type_str: &str) -> &'static str {
    match type_str {
        "string" => "string",
        "number" => "number",
        "boolean" | "bool" => "boolean",
        "node" | "ReactNode" => "node",
        "element" | "ReactElement" => "element-type",
        "function" | "Function" => "function",
        "array" | "Array" => "array",
        _ => "prop",
    }
}

/// Map vertex kind back to type string.
fn kind_to_jsx_type(kind: &str) -> &'static str {
    match kind {
        "string" => "string",
        "number" => "number",
        "boolean" => "boolean",
        "node" => "node",
        "element-type" => "element",
        "function" => "function",
        "array" => "array",
        "event-handler" => "function",
        _ => "string",
    }
}

/// Emit a [`Schema`] as a TSX-subset text.
///
/// # Errors
///
/// Returns [`ProtocolError`] if emission fails.
pub fn emit_jsx(schema: &Schema) -> Result<String, ProtocolError> {
    let structural = &["prop", "items"];
    let roots = find_roots(schema, structural);

    let mut w = IndentWriter::new("  ");

    for root in &roots {
        if root.kind != "component" {
            continue;
        }
        w.line(&format!("component {} {{", root.id));
        w.indent();

        let props = children_by_edge(schema, &root.id, "prop");
        for (edge, prop_v) in &props {
            let prop_name = edge.name.as_deref().unwrap_or(&prop_v.id);
            let keyword = if prop_v.kind == "event-handler" {
                "event"
            } else {
                "prop"
            };
            let type_str = kind_to_jsx_type(&prop_v.kind);
            let required = if constraint_value(schema, &prop_v.id, "required") == Some("true") {
                " required"
            } else {
                ""
            };
            w.line(&format!("{keyword} {prop_name}: {type_str}{required}"));
        }

        w.dedent();
        w.line("}");
        w.blank();
    }

    Ok(w.finish())
}

fn edge_rules() -> Vec<EdgeRule> {
    vec![
        EdgeRule {
            edge_kind: "prop".into(),
            src_kinds: vec!["component".into()],
            tgt_kinds: vec![],
        },
        EdgeRule {
            edge_kind: "items".into(),
            src_kinds: vec!["component".into()],
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
        assert_eq!(p.name, "jsx");
    }

    #[test]
    fn register_theories_works() {
        let mut registry = HashMap::new();
        register_theories(&mut registry);
        assert!(registry.contains_key("ThJsxSchema"));
        assert!(registry.contains_key("ThJsxInstance"));
    }

    #[test]
    fn parse_and_emit() {
        let input = r"
component Button {
  prop label: string required
  prop onClick: function
  prop disabled: boolean
  event onHover: function
}
";
        let schema = parse_jsx(input).expect("should parse");
        assert!(schema.has_vertex("Button"));
        assert!(schema.has_vertex("Button.label"));
        assert!(schema.has_vertex("Button.onClick"));

        let emitted = emit_jsx(&schema).expect("should emit");
        assert!(emitted.contains("component Button"));
        assert!(emitted.contains("label"));
    }

    #[test]
    fn roundtrip() {
        let input = "component Card {\n  prop title: string required\n}\n";
        let s1 = parse_jsx(input).expect("parse");
        let emitted = emit_jsx(&s1).expect("emit");
        let s2 = parse_jsx(&emitted).expect("re-parse");
        assert_eq!(s1.vertex_count(), s2.vertex_count());
    }
}
