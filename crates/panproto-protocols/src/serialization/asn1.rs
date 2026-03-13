//! ASN.1 protocol definition.
//!
//! ASN.1 uses a simple constrained graph schema theory
//! (`colimit(ThSimpleGraph, ThConstraint)` + `ThFlat`).
//!
//! Vertex kinds: module, type-assignment, sequence, set, choice, enumerated,
//!               integer, boolean, octet-string, bit-string, ia5-string,
//!               utf8-string, object-id, null, real, sequence-of, set-of, field.
//! Edge kinds: field-of, type-of, variant-of.

use std::collections::HashMap;
use std::hash::BuildHasher;

use panproto_gat::Theory;
use panproto_schema::{EdgeRule, Protocol, Schema, SchemaBuilder};

use crate::emit::{IndentWriter, children_by_edge, constraint_value, find_roots};
use crate::error::ProtocolError;
use crate::theories;

/// Returns the ASN.1 protocol definition.
#[must_use]
pub fn protocol() -> Protocol {
    Protocol {
        name: "asn1".into(),
        schema_theory: "ThAsn1Schema".into(),
        instance_theory: "ThAsn1Instance".into(),
        edge_rules: edge_rules(),
        obj_kinds: vec![
            "module".into(),
            "type-assignment".into(),
            "sequence".into(),
            "set".into(),
            "choice".into(),
            "enumerated".into(),
            "integer".into(),
            "boolean".into(),
            "octet-string".into(),
            "bit-string".into(),
            "ia5-string".into(),
            "utf8-string".into(),
            "object-id".into(),
            "null".into(),
            "real".into(),
            "sequence-of".into(),
            "set-of".into(),
            "field".into(),
        ],
        constraint_sorts: vec![
            "size".into(),
            "range".into(),
            "default".into(),
            "optional".into(),
            "tag".into(),
        ],
    }
}

/// Register the component GATs for ASN.1 with a theory registry.
pub fn register_theories<S: BuildHasher>(registry: &mut HashMap<String, Theory, S>) {
    theories::register_simple_graph_flat(registry, "ThAsn1Schema", "ThAsn1Instance");
}

/// Intermediate representation of a parsed field for two-pass resolution.
struct FieldInfo {
    field_id: String,
    type_name: String,
}

/// Parse an ASN.1 module into a [`Schema`].
///
/// # Errors
///
/// Returns [`ProtocolError`] if the ASN.1 input cannot be parsed.
pub fn parse_asn1(input: &str) -> Result<Schema, ProtocolError> {
    let proto_def = protocol();
    let mut builder = SchemaBuilder::new(&proto_def);
    let mut vertex_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut field_infos: Vec<FieldInfo> = Vec::new();

    let lines: Vec<&str> = input.lines().collect();
    let mut i = 0;

    // Look for module definition.
    while i < lines.len() {
        let trimmed = lines[i].trim();

        if trimmed.contains("DEFINITIONS") && trimmed.contains("::=") {
            // Module header: ModuleName DEFINITIONS ::= BEGIN
            let module_name = trimmed.split_whitespace().next().unwrap_or("Module");
            builder = builder.vertex(module_name, "module", None)?;
            vertex_ids.insert(module_name.to_string());
            i += 1;

            // Skip to BEGIN.
            while i < lines.len() {
                let t = lines[i].trim();
                if t == "BEGIN" || t.ends_with("BEGIN") {
                    i += 1;
                    break;
                }
                i += 1;
            }

            // Parse type assignments until END.
            while i < lines.len() {
                let t = lines[i].trim();
                if t == "END" {
                    i += 1;
                    break;
                }

                // TypeName ::= TypeDefinition
                if t.contains("::=") {
                    let (new_i, new_fields) = parse_type_assignment(
                        &mut builder,
                        &lines,
                        i,
                        module_name,
                        &mut vertex_ids,
                    )?;
                    field_infos.extend(new_fields);
                    i = new_i;
                } else {
                    i += 1;
                }
            }
        } else if trimmed.contains("::=") {
            // Standalone type assignment (no module wrapper).
            let (new_i, new_fields) =
                parse_type_assignment(&mut builder, &lines, i, "", &mut vertex_ids)?;
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

/// Parse a type assignment: `TypeName ::= SEQUENCE { ... }`.
fn parse_type_assignment(
    builder: &mut SchemaBuilder,
    lines: &[&str],
    start: usize,
    _module_name: &str,
    vertex_ids: &mut std::collections::HashSet<String>,
) -> Result<(usize, Vec<FieldInfo>), ProtocolError> {
    let trimmed = lines[start].trim();
    let parts: Vec<&str> = trimmed.splitn(2, "::=").collect();
    if parts.len() < 2 {
        return Ok((start + 1, Vec::new()));
    }

    let type_name = parts[0].trim();
    let rhs = parts[1].trim();

    let mut field_infos = Vec::new();

    // Determine the kind from the RHS.
    let kind = asn1_type_kind(rhs);

    let taken = std::mem::replace(builder, SchemaBuilder::new(&protocol()));
    let mut b = taken.vertex(type_name, kind, None)?;
    vertex_ids.insert(type_name.to_string());

    // If SEQUENCE, SET, or CHOICE, parse fields inside { }.
    if rhs.contains('{') || (kind == "sequence" || kind == "set" || kind == "choice") {
        let mut i = start;
        // Find opening brace.
        while i < lines.len() && !lines[i].contains('{') {
            i += 1;
        }
        i += 1; // Skip the line with {.

        while i < lines.len() {
            let line = lines[i].trim();
            if line.starts_with('}') || line == "}" {
                *builder = b;
                return Ok((i + 1, field_infos));
            }

            if !line.is_empty() && !line.starts_with("--") && line != "..." {
                // Parse field: fieldName Type [OPTIONAL] [DEFAULT value],
                let clean = line.trim_end_matches(',').trim();
                let parts: Vec<&str> = clean.split_whitespace().collect();

                if parts.len() >= 2 {
                    let field_name = parts[0];
                    let field_type_str = parts[1];

                    let field_id = format!("{type_name}.{field_name}");
                    let edge_kind = if kind == "choice" {
                        "variant-of"
                    } else {
                        "field-of"
                    };
                    b = b.vertex(&field_id, "field", None)?;
                    vertex_ids.insert(field_id.clone());
                    b = b.edge(type_name, &field_id, edge_kind, Some(field_name))?;

                    // Check for OPTIONAL.
                    if parts.iter().any(|p| p.eq_ignore_ascii_case("OPTIONAL")) {
                        b = b.constraint(&field_id, "optional", "true");
                    }

                    // Check for DEFAULT.
                    if let Some(pos) = parts.iter().position(|p| p.eq_ignore_ascii_case("DEFAULT"))
                    {
                        if let Some(val) = parts.get(pos + 1) {
                            b = b.constraint(&field_id, "default", val);
                        }
                    }

                    let resolved_kind = asn1_type_kind(field_type_str);
                    field_infos.push(FieldInfo {
                        field_id,
                        type_name: if resolved_kind == "type-assignment" {
                            field_type_str.to_string()
                        } else {
                            resolved_kind.to_string()
                        },
                    });
                }
            }

            i += 1;
        }

        *builder = b;
        return Ok((lines.len(), field_infos));
    }

    // Check for ENUMERATED.
    if kind == "enumerated" && rhs.contains('{') {
        // Already handled above.
    }

    *builder = b;
    Ok((start + 1, field_infos))
}

/// Map ASN.1 type keywords to vertex kinds.
fn asn1_type_kind(type_str: &str) -> &'static str {
    let upper = type_str.trim().to_uppercase();
    let first_word = upper.split_whitespace().next().unwrap_or("");
    match first_word {
        "SEQUENCE" => {
            if upper.contains("OF") {
                "sequence-of"
            } else {
                "sequence"
            }
        }
        "SET" => {
            if upper.contains("OF") {
                "set-of"
            } else {
                "set"
            }
        }
        "CHOICE" => "choice",
        "ENUMERATED" => "enumerated",
        "INTEGER" => "integer",
        "BOOLEAN" => "boolean",
        "OCTET" => "octet-string",
        "BIT" => "bit-string",
        "IA5STRING" => "ia5-string",
        "UTF8STRING" => "utf8-string",
        "OBJECT" => "object-id",
        "NULL" => "null",
        "REAL" => "real",
        _ => "type-assignment",
    }
}

/// Emit an ASN.1 module from a [`Schema`].
///
/// # Errors
///
/// Returns [`ProtocolError`] if the schema cannot be serialized.
pub fn emit_asn1(schema: &Schema) -> Result<String, ProtocolError> {
    let mut w = IndentWriter::new("  ");
    let roots = find_roots(schema, &["field-of", "variant-of", "type-of"]);

    // Check if there is a module root.
    let module_roots: Vec<_> = roots.iter().filter(|r| r.kind == "module").collect();
    let type_roots: Vec<_> = roots.iter().filter(|r| r.kind != "module").collect();

    if let Some(module) = module_roots.first() {
        w.line(&format!("{} DEFINITIONS ::= BEGIN", module.id));
        w.blank();
    }

    for root in &type_roots {
        emit_type_def(schema, &root.id, &mut w)?;
    }

    if !module_roots.is_empty() {
        w.line("END");
    }

    Ok(w.finish())
}

/// Emit a single type definition.
fn emit_type_def(
    schema: &Schema,
    vertex_id: &str,
    w: &mut IndentWriter,
) -> Result<(), ProtocolError> {
    let vertex = schema
        .vertices
        .get(vertex_id)
        .ok_or_else(|| ProtocolError::Emit(format!("vertex not found: {vertex_id}")))?;

    let asn1_type = kind_to_asn1(&vertex.kind);

    match vertex.kind.as_str() {
        "sequence" | "set" | "choice" => {
            let edge_kind = if vertex.kind == "choice" {
                "variant-of"
            } else {
                "field-of"
            };
            let fields = children_by_edge(schema, vertex_id, edge_kind);

            w.line(&format!("{vertex_id} ::= {asn1_type} {{"));
            w.indent();

            for (idx, (edge, field_vertex)) in fields.iter().enumerate() {
                let name = edge.name.as_deref().unwrap_or(&field_vertex.id);
                let type_children = children_by_edge(schema, &field_vertex.id, "type-of");
                let field_type = type_children
                    .first()
                    .map_or("OCTET STRING", |(_, tv)| kind_to_asn1(&tv.kind));

                let optional = if constraint_value(schema, &field_vertex.id, "optional").is_some() {
                    " OPTIONAL"
                } else {
                    ""
                };

                let comma = if idx < fields.len() - 1 { "," } else { "" };
                w.line(&format!("{name} {field_type}{optional}{comma}"));
            }

            w.dedent();
            w.line("}");
            w.blank();
        }
        _ => {
            w.line(&format!("{vertex_id} ::= {asn1_type}"));
            w.blank();
        }
    }

    Ok(())
}

/// Map vertex kind to ASN.1 type keyword.
fn kind_to_asn1(kind: &str) -> &str {
    match kind {
        "sequence" => "SEQUENCE",
        "set" => "SET",
        "choice" => "CHOICE",
        "enumerated" => "ENUMERATED",
        "integer" => "INTEGER",
        "boolean" => "BOOLEAN",
        "octet-string" => "OCTET STRING",
        "bit-string" => "BIT STRING",
        "ia5-string" => "IA5String",
        "utf8-string" => "UTF8String",
        "object-id" => "OBJECT IDENTIFIER",
        "null" => "NULL",
        "real" => "REAL",
        "sequence-of" => "SEQUENCE OF",
        "set-of" => "SET OF",
        other => other,
    }
}

/// Well-formedness rules for ASN.1 edges.
fn edge_rules() -> Vec<EdgeRule> {
    let all_types: Vec<String> = vec![
        "field",
        "type-assignment",
        "sequence",
        "set",
        "choice",
        "enumerated",
        "integer",
        "boolean",
        "octet-string",
        "bit-string",
        "ia5-string",
        "utf8-string",
        "object-id",
        "null",
        "real",
        "sequence-of",
        "set-of",
    ]
    .into_iter()
    .map(Into::into)
    .collect();

    vec![
        EdgeRule {
            edge_kind: "field-of".into(),
            src_kinds: vec!["sequence".into(), "set".into(), "module".into()],
            tgt_kinds: vec!["field".into(), "type-assignment".into()],
        },
        EdgeRule {
            edge_kind: "type-of".into(),
            src_kinds: vec!["field".into(), "type-assignment".into()],
            tgt_kinds: all_types,
        },
        EdgeRule {
            edge_kind: "variant-of".into(),
            src_kinds: vec!["choice".into(), "enumerated".into()],
            tgt_kinds: vec!["field".into()],
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
        assert_eq!(p.name, "asn1");
        assert_eq!(p.schema_theory, "ThAsn1Schema");
        assert_eq!(p.instance_theory, "ThAsn1Instance");
    }

    #[test]
    fn register_theories_works() {
        let mut registry = HashMap::new();
        register_theories(&mut registry);
        assert!(registry.contains_key("ThAsn1Schema"));
        assert!(registry.contains_key("ThAsn1Instance"));
    }

    #[test]
    fn parse_minimal() {
        let input = r"
Person ::= SEQUENCE {
  name UTF8String,
  age INTEGER
}
";
        let schema = parse_asn1(input).expect("should parse");
        assert!(schema.has_vertex("Person"));
        assert!(schema.has_vertex("Person.name"));
        assert!(schema.has_vertex("Person.age"));
    }

    #[test]
    fn emit_minimal() {
        let input = r"
Person ::= SEQUENCE {
  name UTF8String,
  age INTEGER
}
";
        let schema = parse_asn1(input).expect("should parse");
        let emitted = emit_asn1(&schema).expect("should emit");
        assert!(emitted.contains("Person ::= SEQUENCE"));
        assert!(emitted.contains("name"));
        assert!(emitted.contains("age"));
    }

    #[test]
    fn roundtrip() {
        let input = r"
Event ::= SEQUENCE {
  id UTF8String,
  count INTEGER
}
";
        let schema1 = parse_asn1(input).expect("parse 1");
        let emitted = emit_asn1(&schema1).expect("emit");
        let schema2 = parse_asn1(&emitted).expect("parse 2");

        assert_eq!(schema1.vertex_count(), schema2.vertex_count());
        assert!(schema2.has_vertex("Event"));
        assert!(schema2.has_vertex("Event.id"));
        assert!(schema2.has_vertex("Event.count"));
    }
}
