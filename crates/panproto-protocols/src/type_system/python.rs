//! Python Pydantic/dataclass type system protocol definition.
//!
//! Uses typed graph + W-type with interfaces (Group D).
//! Schema: `colimit(ThGraph, ThConstraint, ThMulti, ThInterface)`.
//! Instance: `ThWType`.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::manual_strip,
    clippy::option_if_let_else,
    clippy::map_unwrap_or,
    clippy::unnecessary_wraps,
    clippy::items_after_statements,
    clippy::redundant_closure_for_method_calls,
    clippy::match_same_arms,
    clippy::if_same_then_else,
    clippy::single_char_pattern,
    clippy::needless_pass_by_value
)]

use std::collections::HashMap;
use std::hash::BuildHasher;

use panproto_gat::Theory;
use panproto_schema::{EdgeRule, Protocol, Schema, SchemaBuilder};

use crate::emit::{IndentWriter, children_by_edge, constraint_value, find_roots, resolve_type};
use crate::error::ProtocolError;
use crate::theories;

/// Returns the Python Pydantic protocol definition.
#[must_use]
pub fn protocol() -> Protocol {
    Protocol {
        name: "python".into(),
        schema_theory: "ThPythonSchema".into(),
        instance_theory: "ThPythonInstance".into(),
        edge_rules: edge_rules(),
        obj_kinds: vec![
            "model".into(),
            "field".into(),
            "enum".into(),
            "validator".into(),
            "str".into(),
            "int".into(),
            "float".into(),
            "bool".into(),
            "bytes".into(),
            "date".into(),
            "datetime".into(),
            "list".into(),
            "dict".into(),
            "set".into(),
            "tuple".into(),
            "optional".into(),
            "union".into(),
            "any".into(),
        ],
        constraint_sorts: vec![
            "optional".into(),
            "default".into(),
            "ge".into(),
            "le".into(),
            "min_length".into(),
            "max_length".into(),
            "pattern".into(),
            "alias".into(),
        ],
    }
}

/// Register the component GATs for Python with a theory registry.
pub fn register_theories<S: BuildHasher>(registry: &mut HashMap<String, Theory, S>) {
    theories::register_typed_graph_wtype(registry, "ThPythonSchema", "ThPythonInstance");
}

/// Parse Pydantic model definitions into a [`Schema`].
///
/// # Errors
///
/// Returns [`ProtocolError`] if parsing or schema construction fails.
#[allow(clippy::too_many_lines)]
pub fn parse_pydantic(input: &str) -> Result<Schema, ProtocolError> {
    let proto = protocol();
    let lines: Vec<&str> = input.lines().collect();
    let mut i = 0;
    let mut builder = SchemaBuilder::new(&proto);
    let mut vertex_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut deferred: Vec<PyFieldDeferred> = Vec::new();

    while i < lines.len() {
        let trimmed = lines[i].trim();

        if trimmed.starts_with("class ") && trimmed.contains('(') {
            let rest = &trimmed["class ".len()..];
            let name = rest
                .split(|c: char| c == '(' || c.is_whitespace())
                .next()
                .unwrap_or("")
                .trim();
            let base = rest
                .split('(')
                .nth(1)
                .and_then(|s| s.split(')').next())
                .unwrap_or("")
                .trim();

            if name.is_empty() {
                i += 1;
            } else {
                if base.contains("Enum") || base.contains("enum") {
                    builder = builder.vertex(name, "enum", None)?;
                    vertex_ids.insert(name.to_owned());
                    i += 1;
                    let (b, new_i) =
                        parse_py_enum_members(builder, &lines, i, name, &mut vertex_ids)?;
                    builder = b;
                    i = new_i;
                } else {
                    builder = builder.vertex(name, "model", None)?;
                    vertex_ids.insert(name.to_owned());
                    i += 1;
                    let (b, new_i, new_deferred) =
                        parse_py_fields(builder, &lines, i, name, &mut vertex_ids)?;
                    builder = b;
                    deferred.extend(new_deferred);
                    i = new_i;
                }
            }
        } else {
            i += 1;
        }
    }

    for d in deferred {
        if vertex_ids.contains(&d.type_name) {
            builder = builder.edge(&d.field_id, &d.type_name, "type-of", None)?;
        }
    }

    let schema = builder.build()?;
    Ok(schema)
}

struct PyFieldDeferred {
    field_id: String,
    type_name: String,
}

fn parse_py_fields(
    mut builder: SchemaBuilder,
    lines: &[&str],
    start: usize,
    parent: &str,
    vertex_ids: &mut std::collections::HashSet<String>,
) -> Result<(SchemaBuilder, usize, Vec<PyFieldDeferred>), ProtocolError> {
    let mut i = start;
    let mut deferred = Vec::new();
    let base_indent = lines
        .get(start)
        .map(|l| l.len() - l.trim_start().len())
        .unwrap_or(4);

    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim();
        if trimmed.is_empty() {
            i += 1;
            continue;
        }
        let indent = line.len() - line.trim_start().len();
        if indent < base_indent && !trimmed.is_empty() {
            break;
        }

        if trimmed.starts_with('#') || trimmed.starts_with("class ") {
            break;
        }

        if let Some(colon_idx) = trimmed.find(':') {
            let field_name = trimmed[..colon_idx].trim();
            if !field_name.is_empty()
                && !field_name.contains(' ')
                && field_name != "pass"
                && !field_name.starts_with('@')
                && !field_name.starts_with("def ")
            {
                let field_id = format!("{parent}.{field_name}");
                builder = builder.vertex(&field_id, "field", None)?;
                vertex_ids.insert(field_id.clone());
                builder = builder.edge(parent, &field_id, "field-of", Some(field_name))?;

                let type_and_default = trimmed[colon_idx + 1..].trim();
                let type_expr = type_and_default.split('=').next().unwrap_or("").trim();
                let base_type = extract_py_base_type(type_expr);

                if type_expr.starts_with("Optional") {
                    builder = builder.constraint(&field_id, "optional", "true");
                }

                if let Some(eq_idx) = type_and_default.find('=') {
                    let default_val = type_and_default[eq_idx + 1..].trim();
                    if !default_val.is_empty() && !default_val.starts_with("Field(") {
                        builder = builder.constraint(&field_id, "default", default_val);
                    }
                    if default_val.starts_with("Field(") {
                        let field_args = default_val
                            .trim_start_matches("Field(")
                            .trim_end_matches(')');
                        for arg in field_args.split(',') {
                            let arg = arg.trim();
                            if let Some((key, val)) = arg.split_once('=') {
                                let key = key.trim();
                                let val = val.trim();
                                match key {
                                    "ge" | "le" | "min_length" | "max_length" | "pattern"
                                    | "alias" | "default" => {
                                        builder = builder.constraint(&field_id, key, val);
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                }

                if !base_type.is_empty() {
                    deferred.push(PyFieldDeferred {
                        field_id,
                        type_name: base_type.to_owned(),
                    });
                }
            }
        }
        i += 1;
    }
    Ok((builder, i, deferred))
}

fn parse_py_enum_members(
    mut builder: SchemaBuilder,
    lines: &[&str],
    start: usize,
    parent: &str,
    vertex_ids: &mut std::collections::HashSet<String>,
) -> Result<(SchemaBuilder, usize), ProtocolError> {
    let mut i = start;
    let base_indent = lines
        .get(start)
        .map(|l| l.len() - l.trim_start().len())
        .unwrap_or(4);

    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim();
        if trimmed.is_empty() {
            i += 1;
            continue;
        }
        let indent = line.len() - line.trim_start().len();
        if indent < base_indent && !trimmed.is_empty() {
            break;
        }
        if trimmed.starts_with("class ") {
            break;
        }

        if let Some(eq_idx) = trimmed.find('=') {
            let member_name = trimmed[..eq_idx].trim();
            if !member_name.is_empty() && member_name != "pass" {
                let member_id = format!("{parent}.{member_name}");
                builder = builder.vertex(&member_id, "validator", None)?;
                vertex_ids.insert(member_id.clone());
                builder = builder.edge(parent, &member_id, "field-of", Some(member_name))?;
            }
        }
        i += 1;
    }
    Ok((builder, i))
}

fn extract_py_base_type(type_expr: &str) -> &str {
    let s = type_expr.trim();
    let s = if let Some(inner) = s.strip_prefix("Optional[") {
        inner.trim_end_matches(']')
    } else if let Some(inner) = s.strip_prefix("List[") {
        inner.trim_end_matches(']')
    } else {
        s
    };
    let s = s.split('[').next().unwrap_or(s);
    let s = s.split('|').next().unwrap_or(s);
    s.trim()
}

/// Emit a [`Schema`] as Pydantic model definitions.
///
/// # Errors
///
/// Returns [`ProtocolError::Emit`] if the schema cannot be serialized.
pub fn emit_pydantic(schema: &Schema) -> Result<String, ProtocolError> {
    let structural = &["field-of", "type-of", "implements"];
    let roots = find_roots(schema, structural);
    let mut w = IndentWriter::new("    ");

    for root in &roots {
        match root.kind.as_str() {
            "model" => emit_py_model(schema, root, &mut w)?,
            "enum" => emit_py_enum(schema, root, &mut w)?,
            _ => {}
        }
    }
    Ok(w.finish())
}

fn emit_py_model(
    schema: &Schema,
    vertex: &panproto_schema::Vertex,
    w: &mut IndentWriter,
) -> Result<(), ProtocolError> {
    w.line(&format!("class {}(BaseModel):", vertex.id));
    w.indent();
    let fields = children_by_edge(schema, &vertex.id, "field-of");
    if fields.is_empty() {
        w.line("pass");
    } else {
        for (edge, field_vertex) in &fields {
            let field_name = edge.name.as_deref().unwrap_or(&field_vertex.id);
            let type_name = resolve_type(schema, &field_vertex.id)
                .map(|v| v.id.clone())
                .unwrap_or_else(|| "Any".to_string());
            let is_optional =
                constraint_value(schema, &field_vertex.id, "optional").is_some_and(|v| v == "true");
            let ty = if is_optional {
                format!("Optional[{type_name}]")
            } else {
                type_name
            };
            let default = constraint_value(schema, &field_vertex.id, "default");
            let suffix = match default {
                Some(val) => format!(" = {val}"),
                None => String::new(),
            };
            w.line(&format!("{field_name}: {ty}{suffix}"));
        }
    }
    w.dedent();
    w.blank();
    Ok(())
}

fn emit_py_enum(
    schema: &Schema,
    vertex: &panproto_schema::Vertex,
    w: &mut IndentWriter,
) -> Result<(), ProtocolError> {
    w.line(&format!("class {}(Enum):", vertex.id));
    w.indent();
    let members = children_by_edge(schema, &vertex.id, "field-of");
    if members.is_empty() {
        w.line("pass");
    } else {
        for (edge, _) in &members {
            let name = edge.name.as_deref().unwrap_or("UNKNOWN");
            w.line(&format!("{name} = auto()"));
        }
    }
    w.dedent();
    w.blank();
    Ok(())
}

fn edge_rules() -> Vec<EdgeRule> {
    vec![
        EdgeRule {
            edge_kind: "field-of".into(),
            src_kinds: vec!["model".into(), "enum".into()],
            tgt_kinds: vec!["field".into(), "validator".into()],
        },
        EdgeRule {
            edge_kind: "type-of".into(),
            src_kinds: vec!["field".into()],
            tgt_kinds: vec![],
        },
        EdgeRule {
            edge_kind: "implements".into(),
            src_kinds: vec!["model".into()],
            tgt_kinds: vec![],
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
        assert_eq!(p.name, "python");
        assert_eq!(p.schema_theory, "ThPythonSchema");
    }

    #[test]
    fn register_theories_works() {
        let mut registry = HashMap::new();
        register_theories(&mut registry);
        assert!(registry.contains_key("ThPythonSchema"));
        assert!(registry.contains_key("ThPythonInstance"));
    }

    #[test]
    fn parse_minimal() {
        let input = r"
class User(BaseModel):
    name: str
    age: int
";
        let schema = parse_pydantic(input).expect("should parse");
        assert!(schema.has_vertex("User"));
        assert!(schema.has_vertex("User.name"));
        assert!(schema.has_vertex("User.age"));
    }

    #[test]
    fn roundtrip() {
        let input = r"
class User(BaseModel):
    name: str
    age: int

class Color(Enum):
    RED = auto()
    GREEN = auto()
";
        let schema = parse_pydantic(input).expect("should parse");
        let output = emit_pydantic(&schema).expect("should emit");
        assert!(output.contains("class User(BaseModel):"));
        assert!(output.contains("class Color(Enum):"));
    }
}
