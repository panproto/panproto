//! Microsoft Bond protocol definition.
//!
//! Bond uses a simple constrained graph schema theory
//! (`colimit(ThSimpleGraph, ThConstraint)` + `ThFlat`).
//!
//! Vertex kinds: struct, field, enum, enum-value, service, function, alias,
//!               string, wstring, bool, int8, int16, int32, int64, uint8,
//!               uint16, uint32, uint64, float, double, blob, list, set,
//!               map, nullable.
//! Edge kinds: field-of, type-of, variant-of.

use std::collections::HashMap;
use std::hash::BuildHasher;

use panproto_gat::Theory;
use panproto_schema::{EdgeRule, Protocol, Schema, SchemaBuilder};

use crate::emit::{IndentWriter, children_by_edge, constraint_value, find_roots};
use crate::error::ProtocolError;
use crate::theories;

/// Returns the Bond protocol definition.
#[must_use]
pub fn protocol() -> Protocol {
    Protocol {
        name: "bond".into(),
        schema_theory: "ThBondSchema".into(),
        instance_theory: "ThBondInstance".into(),
        edge_rules: edge_rules(),
        obj_kinds: vec![
            "struct".into(),
            "field".into(),
            "enum".into(),
            "enum-value".into(),
            "service".into(),
            "function".into(),
            "alias".into(),
            "string".into(),
            "wstring".into(),
            "bool".into(),
            "int8".into(),
            "int16".into(),
            "int32".into(),
            "int64".into(),
            "uint8".into(),
            "uint16".into(),
            "uint32".into(),
            "uint64".into(),
            "float".into(),
            "double".into(),
            "blob".into(),
            "list".into(),
            "set".into(),
            "map".into(),
            "nullable".into(),
        ],
        constraint_sorts: vec![
            "field_id".into(),
            "required".into(),
            "optional".into(),
            "default".into(),
        ],
    }
}

/// Register the component GATs for Bond with a theory registry.
pub fn register_theories<S: BuildHasher>(registry: &mut HashMap<String, Theory, S>) {
    theories::register_simple_graph_flat(registry, "ThBondSchema", "ThBondInstance");
}

/// Intermediate representation of a parsed field for two-pass resolution.
struct FieldInfo {
    field_id: String,
    type_name: String,
}

/// Parse a Bond schema file into a [`Schema`].
///
/// # Errors
///
/// Returns [`ProtocolError`] if the Bond file cannot be parsed.
pub fn parse_bond(input: &str) -> Result<Schema, ProtocolError> {
    let proto_def = protocol();
    let mut builder = SchemaBuilder::new(&proto_def);
    let mut vertex_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut field_infos: Vec<FieldInfo> = Vec::new();

    let lines: Vec<&str> = input.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let trimmed = lines[i].trim();

        if trimmed.starts_with("namespace")
            || trimmed.starts_with("import")
            || trimmed.starts_with("//")
            || trimmed.is_empty()
        {
            i += 1;
        } else if trimmed.starts_with("struct ") {
            let (new_i, new_fields) = parse_struct(&mut builder, &lines, i, &mut vertex_ids)?;
            field_infos.extend(new_fields);
            i = new_i;
        } else if trimmed.starts_with("enum ") {
            i = parse_enum(&mut builder, &lines, i, &mut vertex_ids)?;
        } else if trimmed.starts_with("service ") {
            let (new_i, new_fields) = parse_service(&mut builder, &lines, i, &mut vertex_ids)?;
            field_infos.extend(new_fields);
            i = new_i;
        } else {
            i += 1;
        }
    }

    // Pass 2: Resolve type-of edges.
    for info in &field_infos {
        if vertex_ids.contains(&info.type_name) {
            builder = builder.edge(&info.field_id, &info.type_name, "type-of", None)?;
        }
    }

    let schema = builder.build()?;
    Ok(schema)
}

/// Parse a struct declaration.
fn parse_struct(
    builder: &mut SchemaBuilder,
    lines: &[&str],
    start: usize,
    vertex_ids: &mut std::collections::HashSet<String>,
) -> Result<(usize, Vec<FieldInfo>), ProtocolError> {
    let trimmed = lines[start].trim();
    let name = trimmed
        .strip_prefix("struct ")
        .and_then(|s| s.split('{').next())
        .map(str::trim)
        .ok_or_else(|| ProtocolError::Parse("invalid struct declaration".into()))?;

    let taken = std::mem::replace(builder, SchemaBuilder::new(&protocol()));
    let mut b = taken.vertex(name, "struct", None)?;
    vertex_ids.insert(name.to_string());

    let mut field_infos = Vec::new();
    let mut i = start + 1;
    while i < lines.len() {
        let line = lines[i].trim();
        if line == "}" || line.starts_with('}') {
            *builder = b;
            return Ok((i + 1, field_infos));
        }

        if !line.is_empty() && !line.starts_with("//") {
            // Parse field: N: [required|optional] Type field_name [= default];
            let clean = line.trim_end_matches(';').trim();
            let parts: Vec<&str> = clean.split_whitespace().collect();

            if parts.len() >= 3 {
                let first = parts[0].trim_end_matches(':');

                // Determine field_id, requiredness, type, and name.
                let (fid, requiredness, type_name, field_name, default) =
                    if parts[1] == "required" || parts[1] == "optional" {
                        if parts.len() >= 4 {
                            let default = parts
                                .iter()
                                .position(|p| *p == "=")
                                .and_then(|eq_pos| parts.get(eq_pos + 1).copied());
                            (first, Some(parts[1]), parts[2], parts[3], default)
                        } else {
                            i += 1;
                            continue;
                        }
                    } else {
                        let field_name_part = parts[2].split('=').next().unwrap_or(parts[2]).trim();
                        let default = parts
                            .iter()
                            .position(|p| *p == "=")
                            .and_then(|eq_pos| parts.get(eq_pos + 1).copied());
                        (first, None, parts[1], field_name_part, default)
                    };

                let field_id = format!("{name}.{field_name}");
                b = b.vertex(&field_id, "field", None)?;
                vertex_ids.insert(field_id.clone());
                b = b.edge(name, &field_id, "field-of", Some(field_name))?;

                b = b.constraint(&field_id, "field_id", fid);
                if let Some(req) = requiredness {
                    b = b.constraint(&field_id, req, "true");
                }
                if let Some(d) = default {
                    b = b.constraint(&field_id, "default", d);
                }

                field_infos.push(FieldInfo {
                    field_id,
                    type_name: type_name.to_string(),
                });
            }
        }

        i += 1;
    }

    *builder = b;
    Ok((i, field_infos))
}

/// Parse an enum declaration.
fn parse_enum(
    builder: &mut SchemaBuilder,
    lines: &[&str],
    start: usize,
    vertex_ids: &mut std::collections::HashSet<String>,
) -> Result<usize, ProtocolError> {
    let trimmed = lines[start].trim();
    let name = trimmed
        .strip_prefix("enum ")
        .and_then(|s| s.split('{').next())
        .map(str::trim)
        .ok_or_else(|| ProtocolError::Parse("invalid enum declaration".into()))?;

    let taken = std::mem::replace(builder, SchemaBuilder::new(&protocol()));
    let mut b = taken.vertex(name, "enum", None)?;
    vertex_ids.insert(name.to_string());

    let mut i = start + 1;
    while i < lines.len() {
        let line = lines[i].trim();
        if line == "}" || line.starts_with('}') {
            *builder = b;
            return Ok(i + 1);
        }

        if !line.is_empty() && !line.starts_with("//") {
            let clean = line.trim_end_matches([',', ';']);
            if let Some((val_name, _)) = clean.split_once('=') {
                let val_name = val_name.trim();
                let val_id = format!("{name}.{val_name}");
                b = b.vertex(&val_id, "enum-value", None)?;
                vertex_ids.insert(val_id.clone());
                b = b.edge(name, &val_id, "variant-of", Some(val_name))?;
            } else {
                let val_name = clean.trim();
                if !val_name.is_empty() {
                    let val_id = format!("{name}.{val_name}");
                    b = b.vertex(&val_id, "enum-value", None)?;
                    vertex_ids.insert(val_id.clone());
                    b = b.edge(name, &val_id, "variant-of", Some(val_name))?;
                }
            }
        }

        i += 1;
    }

    *builder = b;
    Ok(i)
}

/// Parse a service declaration.
fn parse_service(
    builder: &mut SchemaBuilder,
    lines: &[&str],
    start: usize,
    vertex_ids: &mut std::collections::HashSet<String>,
) -> Result<(usize, Vec<FieldInfo>), ProtocolError> {
    let trimmed = lines[start].trim();
    let name = trimmed
        .strip_prefix("service ")
        .and_then(|s| s.split('{').next())
        .map(str::trim)
        .ok_or_else(|| ProtocolError::Parse("invalid service declaration".into()))?;

    let taken = std::mem::replace(builder, SchemaBuilder::new(&protocol()));
    let mut b = taken.vertex(name, "service", None)?;
    vertex_ids.insert(name.to_string());

    let field_infos = Vec::new();
    let mut i = start + 1;
    while i < lines.len() {
        let line = lines[i].trim();
        if line == "}" || line.starts_with('}') {
            *builder = b;
            return Ok((i + 1, field_infos));
        }

        if !line.is_empty() && !line.starts_with("//") {
            // Parse: ReturnType function_name(args);
            let clean = line.trim_end_matches(';').trim();
            if let Some(paren_pos) = clean.find('(') {
                let before_paren = &clean[..paren_pos];
                let parts: Vec<&str> = before_paren.split_whitespace().collect();
                if parts.len() >= 2 {
                    let func_name = parts[parts.len() - 1];
                    let func_id = format!("{name}.{func_name}");
                    b = b.vertex(&func_id, "function", None)?;
                    vertex_ids.insert(func_id.clone());
                    b = b.edge(name, &func_id, "field-of", Some(func_name))?;
                }
            }
        }

        i += 1;
    }

    *builder = b;
    Ok((i, field_infos))
}

/// Emit a Bond schema file from a [`Schema`].
///
/// # Errors
///
/// Returns [`ProtocolError`] if the schema cannot be serialized.
pub fn emit_bond(schema: &Schema) -> Result<String, ProtocolError> {
    let mut w = IndentWriter::new("  ");
    let roots = find_roots(schema, &["field-of", "variant-of", "type-of"]);

    for root in &roots {
        match root.kind.as_str() {
            "struct" => {
                w.line(&format!("struct {} {{", root.id));
                w.indent();

                let fields = children_by_edge(schema, &root.id, "field-of");
                for (edge, field_vertex) in &fields {
                    let name = edge.name.as_deref().unwrap_or(&field_vertex.id);
                    let fid = constraint_value(schema, &field_vertex.id, "field_id").unwrap_or("0");
                    let req = if constraint_value(schema, &field_vertex.id, "required").is_some() {
                        "required "
                    } else if constraint_value(schema, &field_vertex.id, "optional").is_some() {
                        "optional "
                    } else {
                        ""
                    };

                    let type_children = children_by_edge(schema, &field_vertex.id, "type-of");
                    let type_name = type_children
                        .first()
                        .map_or("string", |(_, tv)| tv.kind.as_str());

                    let default_part = constraint_value(schema, &field_vertex.id, "default")
                        .map_or_else(String::new, |d| format!(" = {d}"));

                    w.line(&format!("{fid}: {req}{type_name} {name}{default_part};"));
                }

                w.dedent();
                w.line("}");
                w.blank();
            }
            "enum" => {
                w.line(&format!("enum {} {{", root.id));
                w.indent();

                let variants = children_by_edge(schema, &root.id, "variant-of");
                for (idx, (edge, _)) in variants.iter().enumerate() {
                    let name = edge.name.as_deref().unwrap_or("UNKNOWN");
                    w.line(&format!("{name} = {idx},"));
                }

                w.dedent();
                w.line("}");
                w.blank();
            }
            "service" => {
                w.line(&format!("service {} {{", root.id));
                w.indent();

                let funcs = children_by_edge(schema, &root.id, "field-of");
                for (edge, func_vertex) in &funcs {
                    let name = edge.name.as_deref().unwrap_or(&func_vertex.id);
                    w.line(&format!("void {name}();"));
                }

                w.dedent();
                w.line("}");
                w.blank();
            }
            _ => {}
        }
    }

    Ok(w.finish())
}

/// Well-formedness rules for Bond edges.
fn edge_rules() -> Vec<EdgeRule> {
    let all_types: Vec<String> = vec![
        "field", "struct", "enum", "string", "wstring", "bool", "int8", "int16", "int32", "int64",
        "uint8", "uint16", "uint32", "uint64", "float", "double", "blob", "list", "set", "map",
        "nullable",
    ]
    .into_iter()
    .map(Into::into)
    .collect();

    vec![
        EdgeRule {
            edge_kind: "field-of".into(),
            src_kinds: vec!["struct".into(), "service".into()],
            tgt_kinds: vec!["field".into(), "function".into()],
        },
        EdgeRule {
            edge_kind: "type-of".into(),
            src_kinds: vec!["field".into(), "function".into()],
            tgt_kinds: all_types,
        },
        EdgeRule {
            edge_kind: "variant-of".into(),
            src_kinds: vec!["enum".into()],
            tgt_kinds: vec!["enum-value".into()],
        },
    ]
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn protocol_creates_valid_definition() {
        let p = protocol();
        assert_eq!(p.name, "bond");
        assert_eq!(p.schema_theory, "ThBondSchema");
        assert_eq!(p.instance_theory, "ThBondInstance");
    }

    #[test]
    fn register_theories_works() {
        let mut registry = HashMap::new();
        register_theories(&mut registry);
        assert!(registry.contains_key("ThBondSchema"));
        assert!(registry.contains_key("ThBondInstance"));
    }

    #[test]
    fn parse_minimal() {
        let input = r"
struct Person {
  0: required string name;
  1: optional int32 age;
}
";
        let schema = parse_bond(input).expect("should parse");
        assert!(schema.has_vertex("Person"));
        assert!(schema.has_vertex("Person.name"));
        assert!(schema.has_vertex("Person.age"));
    }

    #[test]
    fn emit_minimal() {
        let input = r"
struct Person {
  0: required string name;
  1: optional int32 age;
}
";
        let schema = parse_bond(input).expect("should parse");
        let emitted = emit_bond(&schema).expect("should emit");
        assert!(emitted.contains("struct Person"));
        assert!(emitted.contains("name"));
        assert!(emitted.contains("age"));
    }

    #[test]
    fn roundtrip() {
        let input = r"
struct Event {
  0: required string id;
  1: optional int32 count;
}
";
        let schema1 = parse_bond(input).expect("parse 1");
        let emitted = emit_bond(&schema1).expect("emit");
        let schema2 = parse_bond(&emitted).expect("parse 2");

        assert_eq!(schema1.vertex_count(), schema2.vertex_count());
        assert!(schema2.has_vertex("Event"));
        assert!(schema2.has_vertex("Event.id"));
        assert!(schema2.has_vertex("Event.count"));
    }
}
