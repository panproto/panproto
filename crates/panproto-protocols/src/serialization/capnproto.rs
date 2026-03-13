//! Cap'n Proto protocol definition.
//!
//! Cap'n Proto uses a simple constrained graph schema theory
//! (`colimit(ThSimpleGraph, ThConstraint)` + `ThFlat`).
//!
//! Vertex kinds: struct, field, enum, enumerant, interface, method, group,
//!               union, void, bool, int8, int16, int32, int64, uint8, uint16,
//!               uint32, uint64, float32, float64, text, data, list.
//! Edge kinds: field-of, type-of, variant-of.

use std::collections::HashMap;
use std::hash::BuildHasher;

use panproto_gat::Theory;
use panproto_schema::{EdgeRule, Protocol, Schema, SchemaBuilder};

use crate::emit::{IndentWriter, children_by_edge, constraint_value, find_roots};
use crate::error::ProtocolError;
use crate::theories;

/// Returns the Cap'n Proto protocol definition.
#[must_use]
pub fn protocol() -> Protocol {
    Protocol {
        name: "capnproto".into(),
        schema_theory: "ThCapnProtoSchema".into(),
        instance_theory: "ThCapnProtoInstance".into(),
        edge_rules: edge_rules(),
        obj_kinds: vec![
            "struct".into(),
            "field".into(),
            "enum".into(),
            "enumerant".into(),
            "interface".into(),
            "method".into(),
            "group".into(),
            "union".into(),
            "void".into(),
            "bool".into(),
            "int8".into(),
            "int16".into(),
            "int32".into(),
            "int64".into(),
            "uint8".into(),
            "uint16".into(),
            "uint32".into(),
            "uint64".into(),
            "float32".into(),
            "float64".into(),
            "text".into(),
            "data".into(),
            "list".into(),
        ],
        constraint_sorts: vec!["field_id".into(), "default".into()],
    }
}

/// Register the component GATs for Cap'n Proto with a theory registry.
pub fn register_theories<S: BuildHasher>(registry: &mut HashMap<String, Theory, S>) {
    theories::register_simple_graph_flat(registry, "ThCapnProtoSchema", "ThCapnProtoInstance");
}

/// Intermediate representation of a parsed field for two-pass resolution.
struct FieldInfo {
    field_id: String,
    type_name: String,
}

/// Parse a `.capnp` file into a [`Schema`].
///
/// # Errors
///
/// Returns [`ProtocolError`] if the Cap'n Proto file cannot be parsed.
pub fn parse_capnp(input: &str) -> Result<Schema, ProtocolError> {
    let proto_def = protocol();
    let mut builder = SchemaBuilder::new(&proto_def);
    let mut vertex_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut field_infos: Vec<FieldInfo> = Vec::new();

    let lines: Vec<&str> = input.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let trimmed = lines[i].trim();

        if trimmed.starts_with('#')
            || trimmed.starts_with("using")
            || trimmed.starts_with('@')
            || trimmed.is_empty()
        {
            i += 1;
        } else if trimmed.starts_with("struct ") {
            let (new_i, new_fields) = parse_struct(&mut builder, &lines, i, "", &mut vertex_ids)?;
            field_infos.extend(new_fields);
            i = new_i;
        } else if trimmed.starts_with("enum ") {
            i = parse_enum(&mut builder, &lines, i, "", &mut vertex_ids)?;
        } else if trimmed.starts_with("interface ") {
            let (new_i, new_fields) = parse_interface(&mut builder, &lines, i, &mut vertex_ids)?;
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
    prefix: &str,
    vertex_ids: &mut std::collections::HashSet<String>,
) -> Result<(usize, Vec<FieldInfo>), ProtocolError> {
    let trimmed = lines[start].trim();
    let name = trimmed
        .strip_prefix("struct ")
        .and_then(|s| s.split('{').next())
        .map(str::trim)
        .ok_or_else(|| ProtocolError::Parse("invalid struct declaration".into()))?;

    let struct_id = if prefix.is_empty() {
        name.to_string()
    } else {
        format!("{prefix}.{name}")
    };

    let taken = std::mem::replace(builder, SchemaBuilder::new(&protocol()));
    let mut b = taken.vertex(&struct_id, "struct", None)?;
    vertex_ids.insert(struct_id.clone());

    let mut field_infos = Vec::new();
    let mut i = start + 1;
    while i < lines.len() {
        let line = lines[i].trim();
        if line == "}" || line.starts_with('}') {
            *builder = b;
            return Ok((i + 1, field_infos));
        }

        if line.starts_with("struct ") {
            *builder = b;
            let (new_i, new_fields) = parse_struct(builder, lines, i, &struct_id, vertex_ids)?;
            b = std::mem::replace(builder, SchemaBuilder::new(&protocol()));
            field_infos.extend(new_fields);
            i = new_i;
            continue;
        }

        if line.starts_with("enum ") {
            *builder = b;
            let new_i = parse_enum(builder, lines, i, &struct_id, vertex_ids)?;
            b = std::mem::replace(builder, SchemaBuilder::new(&protocol()));
            i = new_i;
            continue;
        }

        // Parse field: name @N :Type [= default];
        if !line.is_empty() && !line.starts_with("//") && !line.starts_with('#') {
            if let Some(parsed) = parse_capnp_field(line) {
                let field_id = format!("{struct_id}.{}", parsed.name);
                b = b.vertex(&field_id, "field", None)?;
                vertex_ids.insert(field_id.clone());
                b = b.edge(&struct_id, &field_id, "field-of", Some(parsed.name))?;
                b = b.constraint(&field_id, "field_id", &parsed.ordinal.to_string());

                if let Some(default) = parsed.default {
                    b = b.constraint(&field_id, "default", default);
                }

                let type_name = parsed.type_name.to_string();
                field_infos.push(FieldInfo {
                    field_id,
                    type_name,
                });
            }
        }

        i += 1;
    }

    *builder = b;
    Ok((i, field_infos))
}

/// Parsed Cap'n Proto field info.
struct CapnpField<'a> {
    name: &'a str,
    ordinal: u32,
    type_name: &'a str,
    default: Option<&'a str>,
}

/// Parse a single Cap'n Proto field line.
fn parse_capnp_field(line: &str) -> Option<CapnpField<'_>> {
    let clean = line.trim_end_matches(';').trim();
    // name @N :Type [= default]
    let at_pos = clean.find('@')?;
    let name = clean[..at_pos].trim();
    let after_at = &clean[at_pos + 1..];

    // Extract ordinal number.
    let colon_pos = after_at.find(':')?;
    let ordinal_str = after_at[..colon_pos].trim();
    let ordinal: u32 = ordinal_str.parse().ok()?;

    let type_and_default = after_at[colon_pos + 1..].trim();

    // Check for default value.
    if let Some((type_part, default_part)) = type_and_default.split_once('=') {
        Some(CapnpField {
            name,
            ordinal,
            type_name: type_part.trim(),
            default: Some(default_part.trim()),
        })
    } else {
        Some(CapnpField {
            name,
            ordinal,
            type_name: type_and_default,
            default: None,
        })
    }
}

/// Parse an enum declaration.
fn parse_enum(
    builder: &mut SchemaBuilder,
    lines: &[&str],
    start: usize,
    prefix: &str,
    vertex_ids: &mut std::collections::HashSet<String>,
) -> Result<usize, ProtocolError> {
    let trimmed = lines[start].trim();
    let name = trimmed
        .strip_prefix("enum ")
        .and_then(|s| s.split('{').next())
        .map(str::trim)
        .ok_or_else(|| ProtocolError::Parse("invalid enum declaration".into()))?;

    let enum_id = if prefix.is_empty() {
        name.to_string()
    } else {
        format!("{prefix}.{name}")
    };

    let taken = std::mem::replace(builder, SchemaBuilder::new(&protocol()));
    let mut b = taken.vertex(&enum_id, "enum", None)?;
    vertex_ids.insert(enum_id.clone());

    let mut i = start + 1;
    while i < lines.len() {
        let line = lines[i].trim();
        if line == "}" || line.starts_with('}') {
            *builder = b;
            return Ok(i + 1);
        }

        // Parse: name @N;
        if !line.is_empty() && !line.starts_with("//") {
            let clean = line.trim_end_matches(';').trim();
            if let Some(at_pos) = clean.find('@') {
                let val_name = clean[..at_pos].trim();
                let val_id = format!("{enum_id}.{val_name}");
                b = b.vertex(&val_id, "enumerant", None)?;
                vertex_ids.insert(val_id.clone());
                b = b.edge(&enum_id, &val_id, "variant-of", Some(val_name))?;
            }
        }

        i += 1;
    }

    *builder = b;
    Ok(i)
}

/// Parse an interface declaration.
fn parse_interface(
    builder: &mut SchemaBuilder,
    lines: &[&str],
    start: usize,
    vertex_ids: &mut std::collections::HashSet<String>,
) -> Result<(usize, Vec<FieldInfo>), ProtocolError> {
    let trimmed = lines[start].trim();
    let name = trimmed
        .strip_prefix("interface ")
        .and_then(|s| s.split('{').next())
        .map(str::trim)
        .ok_or_else(|| ProtocolError::Parse("invalid interface declaration".into()))?;

    let taken = std::mem::replace(builder, SchemaBuilder::new(&protocol()));
    let mut b = taken.vertex(name, "interface", None)?;
    vertex_ids.insert(name.to_string());

    let field_infos = Vec::new();
    let mut i = start + 1;
    while i < lines.len() {
        let line = lines[i].trim();
        if line == "}" || line.starts_with('}') {
            *builder = b;
            return Ok((i + 1, field_infos));
        }

        // Parse: name @N (args) -> (ReturnType);
        if !line.is_empty() && !line.starts_with("//") {
            if let Some(at_pos) = line.find('@') {
                let method_name = line[..at_pos].trim();
                let method_id = format!("{name}.{method_name}");
                b = b.vertex(&method_id, "method", None)?;
                vertex_ids.insert(method_id.clone());
                b = b.edge(name, &method_id, "field-of", Some(method_name))?;

                // Extract ordinal.
                let after_at = &line[at_pos + 1..];
                if let Some(paren_pos) = after_at.find('(') {
                    let ordinal_str = after_at[..paren_pos].trim();
                    if let Ok(ordinal) = ordinal_str.parse::<u32>() {
                        b = b.constraint(&method_id, "field_id", &ordinal.to_string());
                    }
                }
            }
        }

        i += 1;
    }

    *builder = b;
    Ok((i, field_infos))
}

/// Emit a `.capnp` file from a [`Schema`].
///
/// # Errors
///
/// Returns [`ProtocolError`] if the schema cannot be serialized.
pub fn emit_capnp(schema: &Schema) -> Result<String, ProtocolError> {
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

                    let type_children = children_by_edge(schema, &field_vertex.id, "type-of");
                    let type_name = type_children
                        .first()
                        .map_or("Text", |(_, tv)| tv.kind.as_str());

                    let capnp_type = rust_to_capnp_type(type_name);

                    let default_part = constraint_value(schema, &field_vertex.id, "default")
                        .map_or_else(String::new, |d| format!(" = {d}"));

                    w.line(&format!("{name} @{fid} :{capnp_type}{default_part};"));
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
                    let name = edge.name.as_deref().unwrap_or("unknown");
                    w.line(&format!("{name} @{idx};"));
                }

                w.dedent();
                w.line("}");
                w.blank();
            }
            "interface" => {
                w.line(&format!("interface {} {{", root.id));
                w.indent();

                let methods = children_by_edge(schema, &root.id, "field-of");
                for (edge, method_vertex) in &methods {
                    let name = edge.name.as_deref().unwrap_or(&method_vertex.id);
                    let fid =
                        constraint_value(schema, &method_vertex.id, "field_id").unwrap_or("0");
                    w.line(&format!("{name} @{fid} () -> ();"));
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

/// Map vertex kind to Cap'n Proto type name for emitting.
fn rust_to_capnp_type(kind: &str) -> &str {
    match kind {
        "text" | "string" => "Text",
        "data" => "Data",
        "bool" | "boolean" => "Bool",
        "int8" => "Int8",
        "int16" => "Int16",
        "int32" => "Int32",
        "int64" => "Int64",
        "uint8" => "UInt8",
        "uint16" => "UInt16",
        "uint32" => "UInt32",
        "uint64" => "UInt64",
        "float32" => "Float32",
        "float64" => "Float64",
        "void" => "Void",
        other => other,
    }
}

/// Well-formedness rules for Cap'n Proto edges.
fn edge_rules() -> Vec<EdgeRule> {
    let all_types: Vec<String> = vec![
        "field", "struct", "enum", "group", "union", "void", "bool", "int8", "int16", "int32",
        "int64", "uint8", "uint16", "uint32", "uint64", "float32", "float64", "text", "data",
        "list",
    ]
    .into_iter()
    .map(Into::into)
    .collect();

    vec![
        EdgeRule {
            edge_kind: "field-of".into(),
            src_kinds: vec![
                "struct".into(),
                "group".into(),
                "union".into(),
                "interface".into(),
            ],
            tgt_kinds: vec!["field".into(), "method".into()],
        },
        EdgeRule {
            edge_kind: "type-of".into(),
            src_kinds: vec!["field".into(), "method".into()],
            tgt_kinds: all_types,
        },
        EdgeRule {
            edge_kind: "variant-of".into(),
            src_kinds: vec!["enum".into()],
            tgt_kinds: vec!["enumerant".into()],
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
        assert_eq!(p.name, "capnproto");
        assert_eq!(p.schema_theory, "ThCapnProtoSchema");
        assert_eq!(p.instance_theory, "ThCapnProtoInstance");
    }

    #[test]
    fn register_theories_works() {
        let mut registry = HashMap::new();
        register_theories(&mut registry);
        assert!(registry.contains_key("ThCapnProtoSchema"));
        assert!(registry.contains_key("ThCapnProtoInstance"));
    }

    #[test]
    fn parse_minimal() {
        let input = r"
struct Person {
  name @0 :Text;
  age @1 :UInt32;
}
";
        let schema = parse_capnp(input).expect("should parse");
        assert!(schema.has_vertex("Person"));
        assert!(schema.has_vertex("Person.name"));
        assert!(schema.has_vertex("Person.age"));
    }

    #[test]
    fn emit_minimal() {
        let input = r"
struct Person {
  name @0 :Text;
  age @1 :UInt32;
}
";
        let schema = parse_capnp(input).expect("should parse");
        let emitted = emit_capnp(&schema).expect("should emit");
        assert!(emitted.contains("struct Person"));
        assert!(emitted.contains("name"));
        assert!(emitted.contains("age"));
    }

    #[test]
    fn roundtrip() {
        let input = r"
struct Event {
  id @0 :Text;
  count @1 :UInt32;
}
";
        let schema1 = parse_capnp(input).expect("parse 1");
        let emitted = emit_capnp(&schema1).expect("emit");
        let schema2 = parse_capnp(&emitted).expect("parse 2");

        assert_eq!(schema1.vertex_count(), schema2.vertex_count());
        assert!(schema2.has_vertex("Event"));
        assert!(schema2.has_vertex("Event.id"));
        assert!(schema2.has_vertex("Event.count"));
    }
}
