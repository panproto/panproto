//! Cassandra CQL protocol definition.
//!
//! Cassandra uses a constrained hypergraph schema theory
//! (`colimit(ThHypergraph, ThConstraint)`) and a set-valued functor
//! instance theory (`ThFunctor`).

use std::collections::HashMap;
use std::fmt::Write as _;
use std::hash::BuildHasher;

use panproto_gat::Theory;
use panproto_schema::{EdgeRule, Protocol, Schema, SchemaBuilder};

use crate::emit::{children_by_edge, constraint_value, vertex_constraints};
use crate::error::ProtocolError;
use crate::theories;

/// Returns the Cassandra CQL protocol definition.
#[must_use]
pub fn protocol() -> Protocol {
    Protocol {
        name: "cassandra".into(),
        schema_theory: "ThCassandraSchema".into(),
        instance_theory: "ThCassandraInstance".into(),
        edge_rules: edge_rules(),
        obj_kinds: vec![
            "table".into(),
            "column".into(),
            "udt".into(),
            "udt-field".into(),
            "ascii".into(),
            "bigint".into(),
            "blob".into(),
            "boolean".into(),
            "counter".into(),
            "date".into(),
            "decimal".into(),
            "double".into(),
            "float".into(),
            "inet".into(),
            "int".into(),
            "smallint".into(),
            "text".into(),
            "time".into(),
            "timestamp".into(),
            "timeuuid".into(),
            "tinyint".into(),
            "uuid".into(),
            "varchar".into(),
            "varint".into(),
            "list".into(),
            "set".into(),
            "map".into(),
            "frozen".into(),
            "tuple".into(),
        ],
        constraint_sorts: vec![
            "PRIMARY KEY".into(),
            "CLUSTERING ORDER".into(),
            "STATIC".into(),
            "NOT NULL".into(),
        ],
    }
}

/// Register the component GATs for Cassandra with a theory registry.
pub fn register_theories<S: BuildHasher>(registry: &mut HashMap<String, Theory, S>) {
    theories::register_hypergraph_functor(registry, "ThCassandraSchema", "ThCassandraInstance");
}

/// Parse a CQL DDL string into a [`Schema`].
///
/// Supports `CREATE TABLE` and `CREATE TYPE` statements with
/// Cassandra-specific types and constraints.
///
/// # Errors
///
/// Returns [`ProtocolError`] if the CQL cannot be parsed.
pub fn parse_cql(input: &str) -> Result<Schema, ProtocolError> {
    let proto = protocol();
    let mut builder = SchemaBuilder::new(&proto);
    let mut he_counter: usize = 0;

    let statements = split_statements(input);

    for stmt in &statements {
        let trimmed = stmt.trim();
        let upper = trimmed.to_uppercase();

        if upper.starts_with("CREATE TABLE") {
            builder = parse_create_table(builder, trimmed, &mut he_counter)?;
        } else if upper.starts_with("CREATE TYPE") {
            builder = parse_create_type(builder, trimmed)?;
        }
    }

    let schema = builder.build()?;
    Ok(schema)
}

/// Emit a [`Schema`] as CQL `CREATE TABLE` statements.
///
/// # Errors
///
/// Returns [`ProtocolError::Emit`] if the schema cannot be serialized.
pub fn emit_cql(schema: &Schema) -> Result<String, ProtocolError> {
    let mut output = String::new();

    // Emit UDTs first.
    let mut udts: Vec<_> = schema
        .vertices
        .values()
        .filter(|v| v.kind == "udt")
        .collect();
    udts.sort_by(|a, b| a.id.cmp(&b.id));

    for udt in &udts {
        let _ = writeln!(output, "CREATE TYPE {} (", udt.id);
        let fields = children_by_edge(schema, &udt.id, "prop");
        let field_count = fields.len();
        for (i, (edge, field_vertex)) in fields.iter().enumerate() {
            let field_name = edge.name.as_deref().unwrap_or(&field_vertex.id);
            let cql_type = kind_to_cql_type(&field_vertex.kind);
            let comma = if i + 1 < field_count { "," } else { "" };
            let _ = writeln!(output, "  {field_name} {cql_type}{comma}");
        }
        output.push_str(");\n\n");
    }

    // Emit tables.
    let mut tables: Vec<_> = schema
        .vertices
        .values()
        .filter(|v| v.kind == "table")
        .collect();
    tables.sort_by(|a, b| a.id.cmp(&b.id));

    for table in &tables {
        let _ = writeln!(output, "CREATE TABLE {} (", table.id);
        let columns = children_by_edge(schema, &table.id, "prop");
        let col_count = columns.len();

        // Collect primary key info from table-level constraint.
        let pk_val = constraint_value(schema, &table.id, "PRIMARY KEY");

        for (i, (edge, col_vertex)) in columns.iter().enumerate() {
            let col_name = edge.name.as_deref().unwrap_or(&col_vertex.id);
            let cql_type = kind_to_cql_type(&col_vertex.kind);

            let mut constraints_str = String::new();
            let constraints = vertex_constraints(schema, &col_vertex.id);
            for c in &constraints {
                match c.sort.as_str() {
                    "STATIC" if c.value == "true" => constraints_str.push_str(" STATIC"),
                    "NOT NULL" if c.value == "true" => constraints_str.push_str(" NOT NULL"),
                    _ => {}
                }
            }

            let needs_comma = i + 1 < col_count || pk_val.is_some();
            let comma = if needs_comma { "," } else { "" };
            let _ = writeln!(output, "  {col_name} {cql_type}{constraints_str}{comma}");
        }

        if let Some(pk) = pk_val {
            let _ = writeln!(output, "  PRIMARY KEY ({pk})");
        }

        output.push_str(");\n\n");
    }

    Ok(output)
}

fn split_statements(input: &str) -> Vec<String> {
    input
        .split(';')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

fn parse_create_table(
    mut builder: SchemaBuilder,
    stmt: &str,
    he_counter: &mut usize,
) -> Result<SchemaBuilder, ProtocolError> {
    let table_name = extract_name_after_keyword(stmt, "TABLE")?;
    builder = builder.vertex(&table_name, "table", None)?;

    let columns_block = extract_parenthesized(stmt)?;
    let column_defs = split_column_defs(&columns_block);
    let mut sig = HashMap::new();

    for col_def in &column_defs {
        let trimmed = col_def.trim();
        if trimmed.is_empty() {
            continue;
        }
        let upper = trimmed.to_uppercase();

        if upper.starts_with("PRIMARY KEY") {
            // Table-level PRIMARY KEY
            if let Ok(inner) = extract_parenthesized(trimmed) {
                builder = builder.constraint(&table_name, "PRIMARY KEY", inner.trim());
            }
            continue;
        }

        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        if parts.len() < 2 {
            continue;
        }

        let col_name = parts[0].trim_matches('"');
        let col_type = parts[1];
        let col_id = format!("{table_name}.{col_name}");
        let kind = cql_type_to_kind(col_type);
        builder = builder.vertex(&col_id, &kind, None)?;

        let rest = parts[2..].join(" ").to_uppercase();
        if rest.contains("STATIC") {
            builder = builder.constraint(&col_id, "STATIC", "true");
        }
        if rest.contains("NOT NULL") {
            builder = builder.constraint(&col_id, "NOT NULL", "true");
        }
        if rest.contains("PRIMARY KEY") {
            builder = builder.constraint(&col_id, "PRIMARY KEY", "true");
        }

        builder = builder.edge(&table_name, &col_id, "prop", Some(col_name))?;
        sig.insert(col_name.to_string(), col_id);
    }

    if !sig.is_empty() {
        let he_id = format!("he_{he_counter}");
        *he_counter += 1;
        builder = builder.hyper_edge(&he_id, "table", sig, &table_name)?;
    }

    Ok(builder)
}

fn parse_create_type(
    mut builder: SchemaBuilder,
    stmt: &str,
) -> Result<SchemaBuilder, ProtocolError> {
    let type_name = extract_name_after_keyword(stmt, "TYPE")?;
    builder = builder.vertex(&type_name, "udt", None)?;

    let fields_block = extract_parenthesized(stmt)?;
    let field_defs = split_column_defs(&fields_block);

    for field_def in &field_defs {
        let trimmed = field_def.trim();
        if trimmed.is_empty() {
            continue;
        }
        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        if parts.len() < 2 {
            continue;
        }
        let field_name = parts[0].trim_matches('"');
        let field_type = parts[1];
        let field_id = format!("{type_name}.{field_name}");
        let kind = cql_type_to_kind(field_type);
        builder = builder.vertex(&field_id, &kind, None)?;
        builder = builder.edge(&type_name, &field_id, "prop", Some(field_name))?;
    }

    Ok(builder)
}

fn extract_name_after_keyword(stmt: &str, keyword: &str) -> Result<String, ProtocolError> {
    let upper = stmt.to_uppercase();
    let start = if upper.contains("IF NOT EXISTS") {
        upper
            .find("IF NOT EXISTS")
            .map(|i| i + "IF NOT EXISTS".len())
    } else {
        upper.find(keyword).map(|i| i + keyword.len())
    };
    let start = start.ok_or_else(|| ProtocolError::Parse(format!("no {keyword} keyword found")))?;
    let remainder = stmt[start..].trim();
    let name_end = remainder
        .find(|c: char| c == '(' || c.is_whitespace())
        .unwrap_or(remainder.len());
    let name = remainder[..name_end].trim().trim_matches('"').to_string();
    if name.is_empty() {
        return Err(ProtocolError::Parse(format!("empty {keyword} name")));
    }
    Ok(name)
}

fn extract_parenthesized(stmt: &str) -> Result<String, ProtocolError> {
    let open = stmt
        .find('(')
        .ok_or_else(|| ProtocolError::Parse("no opening parenthesis".into()))?;
    let close = stmt
        .rfind(')')
        .ok_or_else(|| ProtocolError::Parse("no closing parenthesis".into()))?;
    if close <= open {
        return Err(ProtocolError::Parse("mismatched parentheses".into()));
    }
    Ok(stmt[open + 1..close].to_string())
}

fn split_column_defs(block: &str) -> Vec<String> {
    let mut defs = Vec::new();
    let mut current = String::new();
    let mut depth = 0;
    for ch in block.chars() {
        match ch {
            '(' => {
                depth += 1;
                current.push(ch);
            }
            ')' => {
                depth -= 1;
                current.push(ch);
            }
            ',' if depth == 0 => {
                defs.push(current.trim().to_string());
                current.clear();
            }
            _ => current.push(ch),
        }
    }
    if !current.trim().is_empty() {
        defs.push(current.trim().to_string());
    }
    defs
}

fn cql_type_to_kind(cql_type: &str) -> String {
    let upper = cql_type.to_uppercase();
    let base = upper.split('<').next().unwrap_or(&upper);
    match base.trim() {
        "ASCII" => "ascii",
        "BIGINT" => "bigint",
        "BLOB" => "blob",
        "BOOLEAN" => "boolean",
        "COUNTER" => "counter",
        "DATE" => "date",
        "DECIMAL" => "decimal",
        "DOUBLE" => "double",
        "FLOAT" => "float",
        "INET" => "inet",
        "INT" => "int",
        "SMALLINT" => "smallint",
        "TEXT" => "text",
        "TIME" => "time",
        "TIMESTAMP" => "timestamp",
        "TIMEUUID" => "timeuuid",
        "TINYINT" => "tinyint",
        "UUID" => "uuid",
        "VARCHAR" => "varchar",
        "VARINT" => "varint",
        "LIST" => "list",
        "SET" => "set",
        "MAP" => "map",
        "FROZEN" => "frozen",
        "TUPLE" => "tuple",
        _ => "text",
    }
    .into()
}

fn kind_to_cql_type(kind: &str) -> &'static str {
    match kind {
        "ascii" => "ascii",
        "bigint" => "bigint",
        "blob" => "blob",
        "boolean" => "boolean",
        "counter" => "counter",
        "date" => "date",
        "decimal" => "decimal",
        "double" => "double",
        "float" => "float",
        "inet" => "inet",
        "int" => "int",
        "smallint" => "smallint",
        "time" => "time",
        "timestamp" => "timestamp",
        "timeuuid" => "timeuuid",
        "tinyint" => "tinyint",
        "uuid" => "uuid",
        "varchar" => "varchar",
        "varint" => "varint",
        "list" => "list",
        "set" => "set",
        "map" => "map",
        "frozen" => "frozen",
        "tuple" => "tuple",
        _ => "text",
    }
}

fn edge_rules() -> Vec<EdgeRule> {
    vec![
        EdgeRule {
            edge_kind: "prop".into(),
            src_kinds: vec!["table".into(), "udt".into()],
            tgt_kinds: vec![],
        },
        EdgeRule {
            edge_kind: "foreign-key".into(),
            src_kinds: vec![],
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
        assert_eq!(p.name, "cassandra");
        assert_eq!(p.schema_theory, "ThCassandraSchema");
        assert_eq!(p.instance_theory, "ThCassandraInstance");
        assert!(p.find_edge_rule("prop").is_some());
    }

    #[test]
    fn register_theories_adds_correct_theories() {
        let mut registry = HashMap::new();
        register_theories(&mut registry);
        assert!(registry.contains_key("ThCassandraSchema"));
        assert!(registry.contains_key("ThCassandraInstance"));
    }

    #[test]
    fn parse_simple_create_table() {
        let cql = r"
            CREATE TABLE users (
                user_id uuid,
                name text,
                email text,
                age int,
                PRIMARY KEY (user_id)
            );
        ";
        let schema = parse_cql(cql).expect("should parse");
        assert!(schema.has_vertex("users"));
        assert!(schema.has_vertex("users.user_id"));
        assert!(schema.has_vertex("users.name"));
        assert_eq!(schema.vertices.get("users.user_id").unwrap().kind, "uuid");
        assert_eq!(schema.vertices.get("users.age").unwrap().kind, "int");
    }

    #[test]
    fn parse_create_type() {
        let cql = r"
            CREATE TYPE address (
                street text,
                city text,
                zip int
            );
        ";
        let schema = parse_cql(cql).expect("should parse");
        assert!(schema.has_vertex("address"));
        assert!(schema.has_vertex("address.street"));
        assert_eq!(schema.vertices.get("address").unwrap().kind, "udt");
    }

    #[test]
    fn parse_static_column() {
        let cql = r"
            CREATE TABLE sensor_data (
                sensor_id uuid,
                ts timestamp,
                location text STATIC,
                reading double,
                PRIMARY KEY (sensor_id, ts)
            );
        ";
        let schema = parse_cql(cql).expect("should parse");
        let constraints = schema.constraints.get("sensor_data.location");
        assert!(constraints.is_some());
        assert!(
            constraints
                .unwrap()
                .iter()
                .any(|c| c.sort == "STATIC" && c.value == "true")
        );
    }

    #[test]
    fn emit_roundtrip() {
        let cql = r"
            CREATE TABLE events (
                id uuid,
                name text,
                ts timestamp,
                PRIMARY KEY (id)
            );
        ";
        let schema1 = parse_cql(cql).expect("first parse");
        let emitted = emit_cql(&schema1).expect("emit");
        let schema2 = parse_cql(&emitted).expect("re-parse");
        assert_eq!(schema1.vertex_count(), schema2.vertex_count());
    }

    #[test]
    fn parse_empty_cql_fails() {
        let result = parse_cql("");
        assert!(result.is_err());
    }
}
