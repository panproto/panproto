//! SQL protocol definition.
//!
//! SQL uses a constrained hypergraph schema theory
//! (`colimit(ThHypergraph, ThConstraint)`) and a set-valued functor
//! instance theory (`ThFunctor`).
//!
//! Tables are modeled as hyper-edges connecting column vertices.
//! Foreign keys are hyper-edges connecting source columns to target columns.

use std::collections::HashMap;

use panproto_gat::{Sort, Theory, colimit};
use panproto_schema::{EdgeRule, Protocol, Schema, SchemaBuilder};

use crate::error::ProtocolError;
use crate::theories;

/// Returns the SQL protocol definition.
///
/// Schema theory: `colimit(ThHypergraph, ThConstraint)`.
/// Instance theory: `ThFunctor`.
#[must_use]
pub fn protocol() -> Protocol {
    Protocol {
        name: "sql".into(),
        schema_theory: "ThSQLSchema".into(),
        instance_theory: "ThSQLInstance".into(),
        edge_rules: edge_rules(),
        obj_kinds: vec![
            "table".into(),
            "integer".into(),
            "string".into(),
            "boolean".into(),
            "number".into(),
            "bytes".into(),
            "timestamp".into(),
            "date".into(),
            "uuid".into(),
            "json".into(),
        ],
        constraint_sorts: vec![
            "NOT NULL".into(),
            "UNIQUE".into(),
            "CHECK".into(),
            "PRIMARY KEY".into(),
            "DEFAULT".into(),
            "FOREIGN KEY".into(),
        ],
        has_order: true,
        nominal_identity: true,
        ..Protocol::default()
    }
}

/// Register the component GATs for SQL with a theory registry.
///
/// Registers `ThHypergraph`, `ThConstraint`, `ThFunctor`, and the
/// composed schema/instance theories.
pub fn register_theories<S: ::std::hash::BuildHasher>(registry: &mut HashMap<String, Theory, S>) {
    let th_hypergraph = theories::th_hypergraph();
    let th_constraint = theories::th_constraint();
    let th_functor = theories::th_functor();

    registry.insert("ThHypergraph".into(), th_hypergraph.clone());
    registry.insert("ThConstraint".into(), th_constraint.clone());
    registry.insert("ThFunctor".into(), th_functor.clone());

    // Compose schema theory: colimit(ThHypergraph, ThConstraint) over shared Vertex.
    let shared_vertex = Theory::new("ThVertex", vec![Sort::simple("Vertex")], vec![], vec![]);

    if let Ok(mut schema_theory) = colimit(&th_hypergraph, &th_constraint, &shared_vertex) {
        schema_theory.name = "ThSQLSchema".into();
        registry.insert("ThSQLSchema".into(), schema_theory);
    }

    // Instance theory is just ThFunctor (no colimit needed).
    let mut inst = th_functor;
    inst.name = "ThSQLInstance".into();
    registry.insert("ThSQLInstance".into(), inst);
}

/// Parse a SQL DDL string into a [`Schema`].
///
/// Supports `CREATE TABLE`, `ALTER TABLE`, and `DROP TABLE` statements with
/// column definitions, primary keys, foreign keys, `NOT NULL`, `UNIQUE`,
/// `CHECK`, and `DEFAULT` constraints.
///
/// # Errors
///
/// Returns [`ProtocolError`] if the DDL cannot be parsed or
/// schema construction fails.
pub fn parse_ddl(ddl: &str) -> Result<Schema, ProtocolError> {
    let proto = protocol();
    let mut builder = SchemaBuilder::new(&proto);
    let mut hyper_edge_counter: usize = 0;
    let mut dropped_tables: std::collections::HashSet<String> = std::collections::HashSet::new();

    // Simple line-based DDL parser.
    let statements = split_statements(ddl);

    // First pass: identify dropped tables.
    for stmt in &statements {
        let trimmed = stmt.trim();
        let upper = trimmed.to_uppercase();
        if upper.starts_with("DROP TABLE") {
            if let Ok(name) = extract_drop_table_name(trimmed) {
                dropped_tables.insert(name);
            }
        }
    }

    // Track tables and their columns for ALTER TABLE support.
    let mut table_columns: HashMap<String, HashMap<String, String>> = HashMap::new();

    for stmt in &statements {
        let trimmed = stmt.trim();
        let upper = trimmed.to_uppercase();

        if upper.starts_with("CREATE TABLE") {
            let table_name = extract_table_name(trimmed)?;
            if dropped_tables.contains(&table_name) {
                continue;
            }
            let (new_builder, cols) =
                parse_create_table(builder, trimmed, &mut hyper_edge_counter)?;
            builder = new_builder;
            table_columns.insert(table_name, cols);
        } else if upper.starts_with("ALTER TABLE") {
            builder = parse_alter_table(builder, trimmed, &mut table_columns)?;
        }
        // DROP TABLE already handled via dropped_tables set.
    }

    let schema = builder.build()?;
    Ok(schema)
}

/// Split DDL text into individual statements by semicolons.
fn split_statements(ddl: &str) -> Vec<String> {
    ddl.split(';')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Parse a single CREATE TABLE statement.
///
/// Returns the updated builder and a map of column names to vertex IDs.
fn parse_create_table(
    mut builder: SchemaBuilder,
    stmt: &str,
    hyper_edge_counter: &mut usize,
) -> Result<(SchemaBuilder, HashMap<String, String>), ProtocolError> {
    // Extract table name.
    let table_name = extract_table_name(stmt)?;

    // Create a vertex for the table.
    builder = builder.vertex(&table_name, "table", None)?;

    // Extract column block (content between outer parentheses).
    let columns_block = extract_parenthesized(stmt)?;

    // Parse each column definition.
    let column_defs = split_column_defs(&columns_block);

    let mut sig = HashMap::new();

    for col_def in &column_defs {
        let trimmed = col_def.trim();
        if trimmed.is_empty() {
            continue;
        }

        let upper = trimmed.to_uppercase();

        // Handle table-level constraints.
        if upper.starts_with("PRIMARY KEY") {
            // PRIMARY KEY(col1, col2)
            if let Some(cols) = extract_constraint_columns(trimmed) {
                let constraint_val = cols.join(",");
                builder = builder.constraint(&table_name, "PRIMARY KEY", &constraint_val);
            }
            continue;
        }
        if upper.starts_with("FOREIGN KEY") {
            // FOREIGN KEY(col) REFERENCES other_table(col)
            builder = parse_table_foreign_key(builder, trimmed, &table_name, &sig);
            continue;
        }
        if upper.starts_with("UNIQUE") {
            if let Some(cols) = extract_constraint_columns(trimmed) {
                let constraint_val = cols.join(",");
                builder = builder.constraint(&table_name, "UNIQUE", &constraint_val);
            }
            continue;
        }
        if upper.starts_with("CHECK") {
            // Extract the expression inside parentheses.
            if let Ok(expr) = extract_parenthesized(trimmed) {
                builder = builder.constraint(&table_name, "CHECK", &expr);
            }
            continue;
        }
        if upper.starts_with("CONSTRAINT") {
            // Named constraint: CONSTRAINT name PRIMARY KEY(...) / FOREIGN KEY(...) / etc.
            // Parse the inner constraint type.
            if upper.contains("PRIMARY KEY") {
                if let Some(cols) = extract_constraint_columns(trimmed) {
                    let constraint_val = cols.join(",");
                    builder = builder.constraint(&table_name, "PRIMARY KEY", &constraint_val);
                }
            } else if upper.contains("FOREIGN KEY") {
                builder = parse_table_foreign_key(builder, trimmed, &table_name, &sig);
            } else if upper.contains("UNIQUE") {
                if let Some(cols) = extract_constraint_columns(trimmed) {
                    let constraint_val = cols.join(",");
                    builder = builder.constraint(&table_name, "UNIQUE", &constraint_val);
                }
            } else if upper.contains("CHECK") {
                if let Ok(expr) = extract_parenthesized(trimmed) {
                    builder = builder.constraint(&table_name, "CHECK", &expr);
                }
            }
            continue;
        }

        // Parse column: name type [constraints...]
        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        if parts.len() < 2 {
            continue;
        }

        let col_name = parts[0].trim_matches('"').trim_matches('`');
        let col_type = parts[1];
        let col_id = format!("{table_name}.{col_name}");

        // Determine vertex kind from SQL type.
        let kind = sql_type_to_kind(col_type);
        builder = builder.vertex(&col_id, &kind, None)?;

        // Parse inline constraints.
        let rest = parts[2..].join(" ").to_uppercase();
        if rest.contains("NOT NULL") {
            builder = builder.constraint(&col_id, "NOT NULL", "true");
        }
        if rest.contains("PRIMARY KEY") {
            builder = builder.constraint(&col_id, "PRIMARY KEY", "true");
        }
        if rest.contains("UNIQUE") {
            builder = builder.constraint(&col_id, "UNIQUE", "true");
        }
        if let Some(default_val) = extract_default(&rest) {
            builder = builder.constraint(&col_id, "DEFAULT", &default_val);
        }

        // Handle inline REFERENCES.
        if let Some(ref_idx) = rest.find("REFERENCES") {
            let ref_rest = &rest[ref_idx + "REFERENCES".len()..].trim().to_string();
            let ref_table = ref_rest
                .split(|c: char| c == '(' || c.is_whitespace())
                .next()
                .unwrap_or("")
                .trim();
            if !ref_table.is_empty() {
                builder =
                    builder.constraint(&col_id, "FOREIGN KEY", &format!("{ref_table}.{col_name}"));
            }
        }

        // Add a prop edge from table to column.
        builder = builder.edge(&table_name, &col_id, "prop", Some(col_name))?;

        sig.insert(col_name.to_string(), col_id);
    }

    // Create a hyper-edge for the table (connecting all columns).
    if !sig.is_empty() {
        let he_id = format!("he_{hyper_edge_counter}");
        *hyper_edge_counter += 1;
        builder = builder.hyper_edge(&he_id, "table", sig.clone(), &table_name)?;
    }

    Ok((builder, sig))
}

/// Parse a table-level FOREIGN KEY constraint.
fn parse_table_foreign_key(
    mut builder: SchemaBuilder,
    constraint_str: &str,
    table_name: &str,
    sig: &HashMap<String, String>,
) -> SchemaBuilder {
    let upper = constraint_str.to_uppercase();

    // Extract the column(s) in the FOREIGN KEY clause.
    let fk_cols = extract_constraint_columns_at(&upper, "FOREIGN KEY");

    // Extract REFERENCES target.
    if let Some(ref_idx) = upper.find("REFERENCES") {
        let ref_rest = &constraint_str[ref_idx + "REFERENCES".len()..]
            .trim()
            .to_string();
        let ref_table = ref_rest
            .split(|c: char| c == '(' || c.is_whitespace())
            .next()
            .unwrap_or("")
            .trim()
            .to_string();

        if !ref_table.is_empty() {
            if let Some(fk_cols) = fk_cols {
                for col in &fk_cols {
                    let col_lower = col.to_lowercase();
                    if let Some(col_id) = sig.get(&col_lower) {
                        builder = builder.constraint(
                            col_id,
                            "FOREIGN KEY",
                            &format!("{ref_table}.{col_lower}"),
                        );
                    } else {
                        // Column may not exist yet; add constraint to table.
                        builder = builder.constraint(
                            table_name,
                            "FOREIGN KEY",
                            &format!("{col_lower}->{ref_table}"),
                        );
                    }
                }
            }
        }
    }

    builder
}

/// Parse an ALTER TABLE statement.
fn parse_alter_table(
    mut builder: SchemaBuilder,
    stmt: &str,
    table_columns: &mut HashMap<String, HashMap<String, String>>,
) -> Result<SchemaBuilder, ProtocolError> {
    let upper = stmt.to_uppercase();

    // Extract table name after ALTER TABLE.
    let after_alter = upper
        .find("ALTER TABLE")
        .map(|i| i + "ALTER TABLE".len())
        .ok_or_else(|| ProtocolError::Parse("no ALTER TABLE keyword found".into()))?;

    let remainder = stmt[after_alter..].trim();
    let table_end = remainder
        .find(|c: char| c.is_whitespace())
        .unwrap_or(remainder.len());
    let table_name = remainder[..table_end]
        .trim()
        .trim_matches('"')
        .trim_matches('`')
        .to_string();

    let after_table = remainder[table_end..].trim();
    let after_table_upper = after_table.to_uppercase();

    if after_table_upper.starts_with("ADD COLUMN") || after_table_upper.starts_with("ADD ") {
        // ADD [COLUMN] name type [constraints...]
        let col_def = if after_table_upper.starts_with("ADD COLUMN") {
            after_table["ADD COLUMN".len()..].trim()
        } else {
            after_table["ADD".len()..].trim()
        };

        let parts: Vec<&str> = col_def.split_whitespace().collect();
        if parts.len() >= 2 {
            let col_name = parts[0].trim_matches('"').trim_matches('`');
            let col_type = parts[1];
            let col_id = format!("{table_name}.{col_name}");
            let kind = sql_type_to_kind(col_type);
            builder = builder.vertex(&col_id, &kind, None)?;
            builder = builder.edge(&table_name, &col_id, "prop", Some(col_name))?;

            let rest = parts[2..].join(" ").to_uppercase();
            if rest.contains("NOT NULL") {
                builder = builder.constraint(&col_id, "NOT NULL", "true");
            }

            if let Some(cols) = table_columns.get_mut(&table_name) {
                cols.insert(col_name.to_string(), col_id);
            }
        }
    } else if after_table_upper.starts_with("DROP COLUMN") || after_table_upper.starts_with("DROP ")
    {
        // DROP [COLUMN] name - we acknowledge but the column vertex remains.
        // Full removal would require schema diffing, which is out of scope.
    } else if after_table_upper.starts_with("MODIFY")
        || after_table_upper.starts_with("ALTER COLUMN")
    {
        // MODIFY/ALTER COLUMN name type - acknowledged but column vertex already exists.
        // Constraints could be updated, but column identity doesn't change.
    }

    Ok(builder)
}

/// Extract the table name from a CREATE TABLE statement.
fn extract_table_name(stmt: &str) -> Result<String, ProtocolError> {
    // "CREATE TABLE [IF NOT EXISTS] name (...)"
    let upper = stmt.to_uppercase();
    let start = if upper.contains("IF NOT EXISTS") {
        upper
            .find("IF NOT EXISTS")
            .map(|i| i + "IF NOT EXISTS".len())
    } else {
        upper.find("TABLE").map(|i| i + "TABLE".len())
    };

    let start = start.ok_or_else(|| ProtocolError::Parse("no TABLE keyword found".into()))?;
    let remainder = stmt[start..].trim();
    let name_end = remainder
        .find(|c: char| c == '(' || c.is_whitespace())
        .unwrap_or(remainder.len());

    let name = remainder[..name_end]
        .trim()
        .trim_matches('"')
        .trim_matches('`')
        .to_string();

    if name.is_empty() {
        return Err(ProtocolError::Parse("empty table name".into()));
    }

    Ok(name)
}

/// Extract the table name from a DROP TABLE statement.
fn extract_drop_table_name(stmt: &str) -> Result<String, ProtocolError> {
    let upper = stmt.to_uppercase();
    let start = if upper.contains("IF EXISTS") {
        upper.find("IF EXISTS").map(|i| i + "IF EXISTS".len())
    } else {
        upper.find("TABLE").map(|i| i + "TABLE".len())
    };

    let start = start.ok_or_else(|| ProtocolError::Parse("no TABLE keyword found".into()))?;
    let remainder = stmt[start..].trim();
    let name_end = remainder
        .find(|c: char| c.is_whitespace() || c == ';')
        .unwrap_or(remainder.len());

    let name = remainder[..name_end]
        .trim()
        .trim_matches('"')
        .trim_matches('`')
        .to_string();

    if name.is_empty() {
        return Err(ProtocolError::Parse("empty table name".into()));
    }

    Ok(name)
}

/// Extract the parenthesized block from a statement.
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

/// Split column definitions by commas, respecting nested parentheses.
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

/// Map a SQL type name to a vertex kind.
fn sql_type_to_kind(sql_type: &str) -> String {
    let upper = sql_type.to_uppercase();
    if upper.starts_with("INT")
        || upper.starts_with("BIGINT")
        || upper.starts_with("SMALLINT")
        || upper.starts_with("TINYINT")
        || upper.starts_with("SERIAL")
    {
        "integer".into()
    } else if upper.starts_with("VARCHAR") || upper.starts_with("TEXT") || upper.starts_with("CHAR")
    {
        "string".into()
    } else if upper.starts_with("BOOL") {
        "boolean".into()
    } else if upper.starts_with("FLOAT")
        || upper.starts_with("DOUBLE")
        || upper.starts_with("DECIMAL")
        || upper.starts_with("NUMERIC")
        || upper.starts_with("REAL")
    {
        "number".into()
    } else if upper.starts_with("BYTEA") || upper.starts_with("BLOB") {
        "bytes".into()
    } else if upper.starts_with("TIMESTAMP") {
        "timestamp".into()
    } else if upper.starts_with("DATE") {
        "date".into()
    } else if upper.starts_with("UUID") {
        "uuid".into()
    } else if upper.starts_with("JSON") || upper.starts_with("JSONB") {
        "json".into()
    } else {
        "string".into()
    }
}

/// Extract a DEFAULT value from a constraint string.
fn extract_default(constraint_str: &str) -> Option<String> {
    let idx = constraint_str.find("DEFAULT")?;
    let rest = constraint_str[idx + "DEFAULT".len()..].trim();
    // Take the first token as the default value.
    let end = rest
        .find(|c: char| c.is_whitespace() || c == ',')
        .unwrap_or(rest.len());
    let val = rest[..end].trim().to_string();
    if val.is_empty() { None } else { Some(val) }
}

/// Extract column names from a constraint like `PRIMARY KEY(col1, col2)`.
fn extract_constraint_columns(constraint_str: &str) -> Option<Vec<String>> {
    let open = constraint_str.find('(')?;
    let close = constraint_str[open..].find(')')? + open;
    let inner = &constraint_str[open + 1..close];
    let cols: Vec<String> = inner
        .split(',')
        .map(|s| s.trim().trim_matches('"').trim_matches('`').to_string())
        .filter(|s| !s.is_empty())
        .collect();
    if cols.is_empty() { None } else { Some(cols) }
}

/// Extract column names from a constraint starting at a specific keyword.
fn extract_constraint_columns_at(upper_str: &str, keyword: &str) -> Option<Vec<String>> {
    let idx = upper_str.find(keyword)?;
    let after = &upper_str[idx + keyword.len()..];
    let open = after.find('(')?;
    let close = after[open..].find(')')? + open;
    let inner = &after[open + 1..close];
    let cols: Vec<String> = inner
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    if cols.is_empty() { None } else { Some(cols) }
}

/// Map a vertex kind back to a SQL type name.
fn kind_to_sql_type(kind: &str) -> &'static str {
    match kind {
        "integer" => "INTEGER",
        "boolean" => "BOOLEAN",
        "number" => "FLOAT",
        "bytes" => "BYTEA",
        "timestamp" => "TIMESTAMP",
        "date" => "DATE",
        "uuid" => "UUID",
        "json" => "JSONB",
        _ => "TEXT",
    }
}

/// Emit a [`Schema`] as SQL DDL `CREATE TABLE` statements.
///
/// Reconstructs table definitions from the schema graph, including
/// column types and constraints (`NOT NULL`, `PRIMARY KEY`, `UNIQUE`,
/// `DEFAULT`).
///
/// # Errors
///
/// Returns [`ProtocolError::Emit`] if the schema cannot be serialized.
pub fn emit_ddl(schema: &Schema) -> Result<String, ProtocolError> {
    use std::fmt::Write;

    use crate::emit::{children_by_edge, vertex_constraints};

    let mut output = String::new();

    // Find all table vertices.
    let mut tables: Vec<&panproto_schema::Vertex> = schema
        .vertices
        .values()
        .filter(|v| v.kind == "table")
        .collect();
    tables.sort_by(|a, b| a.id.cmp(&b.id));

    for table in &tables {
        let _ = writeln!(output, "CREATE TABLE {} (", table.id);

        let columns = children_by_edge(schema, &table.id, "prop");
        let col_count = columns.len();
        for (i, (edge, col_vertex)) in columns.iter().enumerate() {
            let col_name = edge.name.as_deref().unwrap_or(&col_vertex.id);
            let sql_type = kind_to_sql_type(&col_vertex.kind);

            let mut constraints_str = String::new();
            let constraints = vertex_constraints(schema, &col_vertex.id);
            for c in &constraints {
                match c.sort.as_str() {
                    "PRIMARY KEY" if c.value == "true" => {
                        constraints_str.push_str(" PRIMARY KEY");
                    }
                    "NOT NULL" if c.value == "true" => {
                        constraints_str.push_str(" NOT NULL");
                    }
                    "UNIQUE" if c.value == "true" => {
                        constraints_str.push_str(" UNIQUE");
                    }
                    "DEFAULT" => {
                        let _ = write!(constraints_str, " DEFAULT {}", c.value);
                    }
                    _ => {}
                }
            }

            let comma = if i + 1 < col_count { "," } else { "" };
            let _ = writeln!(output, "  {col_name} {sql_type}{constraints_str}{comma}");
        }

        output.push_str(");\n\n");
    }

    Ok(output)
}

/// Well-formedness rules for SQL edges.
fn edge_rules() -> Vec<EdgeRule> {
    vec![
        EdgeRule {
            edge_kind: "prop".into(),
            src_kinds: vec!["table".into()],
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
        assert_eq!(p.name, "sql");
        assert_eq!(p.schema_theory, "ThSQLSchema");
        assert_eq!(p.instance_theory, "ThSQLInstance");
        assert!(p.find_edge_rule("prop").is_some());
    }

    #[test]
    fn register_theories_adds_correct_theories() {
        let mut registry = HashMap::new();
        register_theories(&mut registry);

        assert!(registry.contains_key("ThHypergraph"));
        assert!(registry.contains_key("ThConstraint"));
        assert!(registry.contains_key("ThFunctor"));
        assert!(registry.contains_key("ThSQLSchema"));
        assert!(registry.contains_key("ThSQLInstance"));

        let schema_t = &registry["ThSQLSchema"];
        assert!(schema_t.find_sort("Vertex").is_some());
        assert!(schema_t.find_sort("HyperEdge").is_some());
        assert!(schema_t.find_sort("Constraint").is_some());
    }

    #[test]
    fn parse_simple_create_table() {
        let ddl = r"
            CREATE TABLE users (
                id INTEGER PRIMARY KEY NOT NULL,
                name VARCHAR(255) NOT NULL,
                email TEXT UNIQUE,
                active BOOLEAN DEFAULT true
            );
        ";

        let schema = parse_ddl(ddl);
        assert!(schema.is_ok(), "parse_ddl should succeed: {schema:?}");
        let schema = schema.ok();
        let schema = schema.as_ref();

        assert!(schema.is_some_and(|s| s.has_vertex("users")));
        assert!(schema.is_some_and(|s| s.has_vertex("users.id")));
        assert!(schema.is_some_and(|s| s.has_vertex("users.name")));
        assert!(schema.is_some_and(|s| s.has_vertex("users.email")));
        assert!(schema.is_some_and(|s| s.has_vertex("users.active")));
    }

    #[test]
    fn parse_multiple_tables() {
        let ddl = r"
            CREATE TABLE posts (
                id INTEGER PRIMARY KEY,
                title TEXT NOT NULL,
                author_id INTEGER
            );
            CREATE TABLE comments (
                id INTEGER PRIMARY KEY,
                body TEXT,
                post_id INTEGER
            );
        ";

        let schema = parse_ddl(ddl);
        assert!(schema.is_ok(), "parse_ddl should succeed: {schema:?}");
        let schema = schema.ok();
        let schema = schema.as_ref();

        assert!(schema.is_some_and(|s| s.has_vertex("posts")));
        assert!(schema.is_some_and(|s| s.has_vertex("comments")));
        assert!(schema.is_some_and(|s| s.has_vertex("posts.title")));
        assert!(schema.is_some_and(|s| s.has_vertex("comments.body")));
    }

    #[test]
    fn parse_empty_ddl() {
        let result = parse_ddl("");
        // Empty DDL produces no vertices, which SchemaBuilder rejects.
        assert!(result.is_err(), "empty DDL should fail with EmptySchema");
    }

    #[test]
    fn parse_timestamp_and_uuid_types() {
        let ddl = r"
            CREATE TABLE events (
                id UUID PRIMARY KEY,
                created_at TIMESTAMP NOT NULL,
                event_date DATE,
                payload JSONB
            );
        ";
        let schema = parse_ddl(ddl).expect("should parse");
        assert_eq!(schema.vertices.get("events.id").unwrap().kind, "uuid");
        assert_eq!(
            schema.vertices.get("events.created_at").unwrap().kind,
            "timestamp"
        );
        assert_eq!(
            schema.vertices.get("events.event_date").unwrap().kind,
            "date"
        );
        assert_eq!(schema.vertices.get("events.payload").unwrap().kind, "json");
    }

    #[test]
    fn parse_float_double_types() {
        let ddl = r"
            CREATE TABLE measurements (
                temp FLOAT,
                pressure DOUBLE
            );
        ";
        let schema = parse_ddl(ddl).expect("should parse");
        assert_eq!(
            schema.vertices.get("measurements.temp").unwrap().kind,
            "number"
        );
        assert_eq!(
            schema.vertices.get("measurements.pressure").unwrap().kind,
            "number"
        );
    }

    #[test]
    fn parse_drop_table() {
        let ddl = r"
            CREATE TABLE temp (id INTEGER);
            DROP TABLE temp;
        ";
        let result = parse_ddl(ddl);
        // The table was dropped, so no vertices should be created.
        assert!(result.is_err(), "dropped table should produce empty schema");
    }

    #[test]
    fn parse_table_level_primary_key() {
        let ddl = r"
            CREATE TABLE orders (
                order_id INTEGER NOT NULL,
                product_id INTEGER NOT NULL,
                PRIMARY KEY(order_id, product_id)
            );
        ";
        let schema = parse_ddl(ddl).expect("should parse");
        let constraints = schema.constraints.get("orders");
        assert!(constraints.is_some());
        assert!(constraints.unwrap().iter().any(|c| c.sort == "PRIMARY KEY"));
    }

    #[test]
    fn emit_ddl_roundtrip() {
        let ddl = r"
            CREATE TABLE users (
                id INTEGER PRIMARY KEY NOT NULL,
                name TEXT NOT NULL,
                active BOOLEAN DEFAULT true
            );
        ";

        let schema1 = parse_ddl(ddl).expect("first parse should succeed");
        let emitted = emit_ddl(&schema1).expect("emit should succeed");
        let schema2 = parse_ddl(&emitted).expect("re-parse should succeed");

        assert_eq!(
            schema1.vertex_count(),
            schema2.vertex_count(),
            "vertex counts should match after round-trip"
        );
        assert_eq!(
            schema1.edge_count(),
            schema2.edge_count(),
            "edge counts should match after round-trip"
        );
    }

    #[test]
    fn parse_alter_table_add_column() {
        let ddl = r"
            CREATE TABLE users (
                id INTEGER PRIMARY KEY
            );
            ALTER TABLE users ADD COLUMN name TEXT NOT NULL;
        ";
        let schema = parse_ddl(ddl).expect("should parse");
        assert!(schema.has_vertex("users.name"));
    }
}
