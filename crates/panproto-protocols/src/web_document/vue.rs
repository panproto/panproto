//! Vue component definition protocol.
//!
//! Uses Group D theory: typed graph + W-type with interfaces.

use std::collections::HashMap;
use std::hash::BuildHasher;

use panproto_gat::Theory;
use panproto_schema::{EdgeRule, Protocol, Schema, SchemaBuilder};

use crate::emit::{IndentWriter, children_by_edge, constraint_value, find_roots};
use crate::error::ProtocolError;
use crate::theories;

/// Returns the Vue protocol definition.
#[must_use]
pub fn protocol() -> Protocol {
    Protocol {
        name: "vue".into(),
        schema_theory: "ThVueSchema".into(),
        instance_theory: "ThVueInstance".into(),
        edge_rules: edge_rules(),
        obj_kinds: vec![
            "component".into(),
            "prop".into(),
            "emit-event".into(),
            "slot".into(),
            "computed".into(),
            "ref".into(),
            "string".into(),
            "number".into(),
            "boolean".into(),
            "array".into(),
            "object".into(),
            "function".into(),
        ],
        constraint_sorts: vec!["required".into(), "default".into()],
    }
}

/// Register the component GATs for Vue.
pub fn register_theories<S: BuildHasher>(registry: &mut HashMap<String, Theory, S>) {
    theories::register_typed_graph_wtype(registry, "ThVueSchema", "ThVueInstance");
}

/// Parse a text-based Vue component definition into a [`Schema`].
///
/// Expects simplified syntax:
/// ```text
/// component MyComponent {
///   prop title: string required
///   emit update: object
///   slot default
/// }
/// ```
///
/// # Errors
///
/// Returns [`ProtocolError`] if parsing fails.
pub fn parse_vue(input: &str) -> Result<Schema, ProtocolError> {
    let proto = protocol();
    let mut builder = SchemaBuilder::new(&proto);

    let lines: Vec<&str> = input.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let trimmed = lines[i].trim();
        if trimmed.starts_with("component ") {
            let (b, new_i) = parse_vue_component(builder, &lines, i)?;
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
fn parse_vue_component(
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

        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 2 {
            match parts[0] {
                "prop" => {
                    let prop_name = parts[1].trim_end_matches(':');
                    let type_str = parts.get(2).copied().unwrap_or("string");
                    let prop_id = format!("{name}.{prop_name}");
                    let kind = vue_type_to_kind(type_str);
                    builder = builder.vertex(&prop_id, kind, None)?;
                    builder = builder.edge(name, &prop_id, "prop", Some(prop_name))?;
                    if parts.contains(&"required") {
                        builder = builder.constraint(&prop_id, "required", "true");
                    }
                }
                "emit" => {
                    let event_name = parts[1].trim_end_matches(':');
                    let event_id = format!("{name}.{event_name}");
                    builder = builder.vertex(&event_id, "emit-event", None)?;
                    builder = builder.edge(name, &event_id, "prop", Some(event_name))?;
                }
                "slot" => {
                    let slot_name = parts[1];
                    let slot_id = format!("{name}:slot:{slot_name}");
                    builder = builder.vertex(&slot_id, "slot", None)?;
                    builder = builder.edge(name, &slot_id, "prop", Some(slot_name))?;
                }
                "computed" => {
                    let comp_name = parts[1].trim_end_matches(':');
                    let comp_id = format!("{name}.{comp_name}");
                    let kind = parts.get(2).copied().map_or("string", vue_type_to_kind);
                    builder = builder.vertex(&comp_id, kind, None)?;
                    builder = builder.edge(name, &comp_id, "prop", Some(comp_name))?;
                }
                _ => {}
            }
        }

        i += 1;
    }

    Ok((builder, i))
}

/// Map type name to vertex kind.
fn vue_type_to_kind(type_str: &str) -> &'static str {
    match type_str {
        "string" | "String" => "string",
        "number" | "Number" => "number",
        "boolean" | "Boolean" => "boolean",
        "array" | "Array" => "array",
        "object" | "Object" => "object",
        "function" | "Function" => "function",
        _ => "prop",
    }
}

/// Map vertex kind to type string.
fn kind_to_vue_type(kind: &str) -> &'static str {
    match kind {
        "string" => "string",
        "number" => "number",
        "boolean" => "boolean",
        "array" => "array",
        "object" => "object",
        "function" => "function",
        _ => "string",
    }
}

/// Emit a [`Schema`] as a text-based Vue component definition.
///
/// # Errors
///
/// Returns [`ProtocolError`] if emission fails.
pub fn emit_vue(schema: &Schema) -> Result<String, ProtocolError> {
    let structural = &["prop"];
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
            match prop_v.kind.as_str() {
                "emit-event" => {
                    w.line(&format!("emit {prop_name}"));
                }
                "slot" => {
                    w.line(&format!("slot {prop_name}"));
                }
                _ => {
                    let type_str = kind_to_vue_type(&prop_v.kind);
                    let required =
                        if constraint_value(schema, &prop_v.id, "required") == Some("true") {
                            " required"
                        } else {
                            ""
                        };
                    w.line(&format!("prop {prop_name}: {type_str}{required}"));
                }
            }
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
        src_kinds: vec!["component".into()],
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
        assert_eq!(p.name, "vue");
    }

    #[test]
    fn register_theories_works() {
        let mut registry = HashMap::new();
        register_theories(&mut registry);
        assert!(registry.contains_key("ThVueSchema"));
        assert!(registry.contains_key("ThVueInstance"));
    }

    #[test]
    fn parse_and_emit() {
        let input = r"
component MyComponent {
  prop title: string required
  prop count: number
  emit update
  slot default
}
";
        let schema = parse_vue(input).expect("should parse");
        assert!(schema.has_vertex("MyComponent"));
        assert!(schema.has_vertex("MyComponent.title"));

        let emitted = emit_vue(&schema).expect("should emit");
        assert!(emitted.contains("component MyComponent"));
    }

    #[test]
    fn roundtrip() {
        let input = "component App {\n  prop name: string required\n}\n";
        let s1 = parse_vue(input).expect("parse");
        let emitted = emit_vue(&s1).expect("emit");
        let s2 = parse_vue(&emitted).expect("re-parse");
        assert_eq!(s1.vertex_count(), s2.vertex_count());
    }
}
