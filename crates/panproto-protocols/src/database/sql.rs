//! SQL protocol definition.
//!
//! SQL uses a constrained hypergraph schema theory
//! (`colimit(ThHypergraph, ThConstraint)`) and a set-valued functor
//! instance theory (`ThFunctor`).
//!
//! Tables are modeled as hyper-edges connecting column vertices.
//! Foreign keys are hyper-edges connecting source columns to target columns.

use std::collections::HashMap;
use std::hash::BuildHasher;

use panproto_gat::Theory;
use panproto_schema::{EdgeRule, Protocol, Schema, SchemaBuilder};

use super::keyword::{
    contains_keyword, find_keyword_end, name_after_keyword, starts_with_keyword,
    strip_keyword_prefix,
};
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
pub fn register_theories<S: BuildHasher>(registry: &mut HashMap<String, Theory, S>) {
    theories::register_hypergraph_functor(registry, "ThSQLSchema", "ThSQLInstance");
}

/// Parse a SQL DDL string into a [`Schema`].
///
/// Supports `CREATE TABLE`, `ALTER TABLE`, and `DROP TABLE` statements with
/// column definitions, primary keys, foreign keys, `NOT NULL`, `UNIQUE`,
/// `CHECK`, and `DEFAULT` constraints.
///
/// The script is split into statements with comments removed and with
/// string literals, quoted identifiers and dollar-quoted bodies kept
/// whole, so a `;` inside any of them ends nothing. That is what lets a
/// database dump parse: a `PostgreSQL` dump declares functions with
/// `$$`-quoted bodies, adds its keys in `ALTER TABLE ONLY t ADD
/// CONSTRAINT ...` statements, declares partitions that `INHERITS` their
/// parent's columns, and types columns with `CREATE DOMAIN`s.
///
/// # Errors
///
/// Returns [`ProtocolError`] if the DDL cannot be parsed or
/// schema construction fails.
pub fn parse_ddl(ddl: &str) -> Result<Schema, ProtocolError> {
    let proto = protocol();
    let mut builder = SchemaBuilder::new(&proto);
    let mut catalog = Catalog::default();

    let statements = split_statements(ddl);

    // First pass: identify dropped tables.
    for stmt in &statements {
        if leading_words_are(stmt, &["DROP", "TABLE"]) {
            if let Ok(name) = extract_drop_table_name(stmt) {
                catalog.dropped.insert(name);
            }
        }
    }

    for stmt in &statements {
        if leading_words_are(stmt, &["CREATE", "TABLE"]) {
            let table_name = extract_table_name(stmt)?;
            if catalog.dropped.contains(&table_name) {
                continue;
            }
            builder = parse_create_table(builder, stmt, &mut catalog)?;
        } else if leading_words_are(stmt, &["CREATE", "DOMAIN"]) {
            catalog.declare_domain(stmt);
        } else if leading_words_are(stmt, &["ALTER", "TABLE"]) {
            builder = parse_alter_table(builder, stmt, &mut catalog)?;
        }
        // DROP TABLE already handled via dropped_tables set.
    }

    let schema = builder.build()?;
    Ok(schema)
}

/// Whether the statement's leading whitespace-separated words are
/// `words`, ignoring case.
fn leading_words_are(stmt: &str, words: &[&str]) -> bool {
    let mut leading = stmt.split_whitespace();
    words
        .iter()
        .all(|w| leading.next().is_some_and(|l| l.eq_ignore_ascii_case(w)))
}

/// The first word of a definition, up to whitespace or `(`.
fn first_word(def: &str) -> &str {
    let def = def.trim_start();
    let end = def
        .find(|c: char| c.is_whitespace() || c == '(')
        .unwrap_or(def.len());
    &def[..end]
}

/// A column as declared, kept so that a table that `INHERITS` from its
/// table declares the same column.
#[derive(Clone, Debug)]
struct ColumnSpec {
    name: String,
    kind: String,
    not_null: bool,
    default: Option<String>,
}

/// What the parse knows of the declarations read so far.
#[derive(Default)]
struct Catalog {
    /// Per table: column name to column vertex id.
    columns: HashMap<String, HashMap<String, String>>,
    /// Per table: its columns in declaration order.
    specs: HashMap<String, Vec<ColumnSpec>>,
    /// `CREATE DOMAIN` names, lowercased, to the vertex kind of the
    /// domain's base type.
    domains: HashMap<String, String>,
    /// Tables a `DROP TABLE` removes.
    dropped: std::collections::HashSet<String>,
    /// The number of table hyper-edges declared, which names the next.
    hyper_edges: usize,
}

impl Catalog {
    /// The declared table a written name refers to: the name itself, or
    /// for a schema-qualified name (`public.actor`) the unqualified table
    /// it names when only that is declared.
    fn table_named(&self, written: &str) -> String {
        if self.columns.contains_key(written) {
            return written.to_owned();
        }
        let last = unqualified(written);
        if self.columns.contains_key(last) {
            last.to_owned()
        } else {
            written.to_owned()
        }
    }

    /// The vertex kind of a column type, through a declared domain.
    fn column_kind(&self, sql_type: &str) -> String {
        let base = sql_type.split('(').next().unwrap_or(sql_type);
        self.domains
            .get(&unqualified(base).to_ascii_lowercase())
            .cloned()
            .unwrap_or_else(|| sql_type_to_kind(sql_type))
    }

    /// Record `CREATE DOMAIN name [AS] base_type ...`.
    fn declare_domain(&mut self, stmt: &str) {
        let mut words = stmt.split_whitespace().skip(2);
        let Some(name) = words.next() else {
            return;
        };
        let base = match words.next() {
            Some(w) if w.eq_ignore_ascii_case("AS") => words.next(),
            other => other,
        };
        if let Some(base) = base {
            let kind = self.column_kind(base);
            let name = unqualified(name).trim_matches('"').to_ascii_lowercase();
            self.domains.insert(name, kind);
        }
    }
}

/// The last component of a possibly schema-qualified name, unquoted.
fn unqualified(name: &str) -> &str {
    name.rsplit('.')
        .next()
        .unwrap_or(name)
        .trim_matches('"')
        .trim_matches('`')
}

/// Whether `b` can continue an identifier, which a dollar-quote tag may
/// not follow.
const fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'$' || b >= 0x80
}

/// The dollar-quote opening tag (`$$`, `$body$`) at byte `i`, if one
/// starts there.
fn dollar_tag(ddl: &str, i: usize) -> Option<&str> {
    let bytes = ddl.as_bytes();
    if bytes[i] != b'$' || (i > 0 && is_ident_byte(bytes[i - 1])) {
        return None;
    }
    let mut j = i + 1;
    if bytes.get(j).is_some_and(u8::is_ascii_digit) {
        return None;
    }
    while bytes
        .get(j)
        .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_')
    {
        j += 1;
    }
    (bytes.get(j) == Some(&b'$')).then(|| &ddl[i..=j])
}

/// The byte just past the literal or quoted identifier opening at `i`
/// with the quote `q`. A doubled quote is an escaped one, and an
/// `E'...'` string also escapes with a backslash.
fn skip_quoted(bytes: &[u8], i: usize, q: u8) -> usize {
    let backslash = q == b'\''
        && i > 0
        && matches!(bytes[i - 1], b'E' | b'e')
        && (i < 2 || !is_ident_byte(bytes[i - 2]));
    let mut j = i + 1;
    while j < bytes.len() {
        if backslash && bytes[j] == b'\\' {
            j += 2;
        } else if bytes[j] == q {
            if bytes.get(j + 1) == Some(&q) {
                j += 2;
            } else {
                return j + 1;
            }
        } else {
            j += 1;
        }
    }
    bytes.len()
}

/// Split a DDL script into statements at each `;` outside a literal,
/// quoted identifier, dollar-quoted body or comment.
///
/// Comments are dropped (each leaves a space, so the words it separated
/// stay separate), so every statement begins with its first keyword. The
/// data rows a `COPY ... FROM stdin;` statement is followed by, up to the
/// `\.` line that ends them, are not statements and are skipped.
fn split_statements(ddl: &str) -> Vec<String> {
    let bytes = ddl.as_bytes();
    let mut statements = Vec::new();
    let mut current = String::new();
    let mut run = 0;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'-' if bytes.get(i + 1) == Some(&b'-') => {
                current.push_str(&ddl[run..i]);
                current.push(' ');
                i = ddl[i..].find('\n').map_or(bytes.len(), |n| i + n);
                run = i;
            }
            b'/' if bytes.get(i + 1) == Some(&b'*') => {
                current.push_str(&ddl[run..i]);
                current.push(' ');
                // Block comments nest in PostgreSQL.
                let mut depth = 0usize;
                while i < bytes.len() {
                    if bytes[i] == b'/' && bytes.get(i + 1) == Some(&b'*') {
                        depth += 1;
                        i += 2;
                    } else if bytes[i] == b'*' && bytes.get(i + 1) == Some(&b'/') {
                        depth -= 1;
                        i += 2;
                        if depth == 0 {
                            break;
                        }
                    } else {
                        i += 1;
                    }
                }
                run = i;
            }
            q @ (b'\'' | b'"' | b'`') => i = skip_quoted(bytes, i, q),
            b'$' => {
                if let Some(tag) = dollar_tag(ddl, i) {
                    let body = i + tag.len();
                    i = ddl[body..]
                        .find(tag)
                        .map_or(bytes.len(), |end| body + end + tag.len());
                } else {
                    i += 1;
                }
            }
            b';' => {
                current.push_str(&ddl[run..i]);
                i += 1;
                let stmt = std::mem::take(&mut current).trim().to_owned();
                if leading_words_are(&stmt, &["COPY"]) && contains_keyword(&stmt, "FROM stdin") {
                    i = skip_copy_data(ddl, i);
                }
                if !stmt.is_empty() {
                    statements.push(stmt);
                }
                run = i;
            }
            _ => i += 1,
        }
    }
    current.push_str(&ddl[run..]);
    let last = current.trim();
    if !last.is_empty() {
        statements.push(last.to_owned());
    }
    statements
}

/// The byte just past the `\.` line that ends the rows of a
/// `COPY ... FROM stdin` statement ending at byte `i`.
fn skip_copy_data(ddl: &str, i: usize) -> usize {
    // The rows start on the line after the statement.
    let mut at = ddl[i..].find('\n').map_or(ddl.len(), |n| i + n + 1);
    while at < ddl.len() {
        let end = ddl[at..].find('\n').map_or(ddl.len(), |n| at + n);
        let line = &ddl[at..end];
        at = (end + 1).min(ddl.len());
        if line.trim_end() == "\\." {
            break;
        }
    }
    at
}

/// Parse a single CREATE TABLE statement, recording its columns in the
/// catalog.
///
/// A table that `INHERITS` from declared tables first declares their
/// columns, with their `NOT NULL` and `DEFAULT` constraints, as
/// `PostgreSQL` does; key constraints are not inherited.
fn parse_create_table(
    mut builder: SchemaBuilder,
    stmt: &str,
    catalog: &mut Catalog,
) -> Result<SchemaBuilder, ProtocolError> {
    // Extract table name.
    let table_name = extract_table_name(stmt)?;

    // Create a vertex for the table.
    builder = builder.vertex(&table_name, "table", None)?;

    // The column block, and what follows it.
    let (columns_block, tail) = split_table_body(stmt)?;

    let mut sig = HashMap::new();
    let mut specs: Vec<ColumnSpec> = Vec::new();

    for parent in inherited_tables(tail) {
        let parent = catalog.table_named(parent);
        for spec in catalog.specs.get(&parent).cloned().unwrap_or_default() {
            if sig.contains_key(&spec.name) {
                continue;
            }
            let col_id = format!("{table_name}.{}", spec.name);
            builder = builder.vertex(&col_id, &spec.kind, None)?;
            if spec.not_null {
                builder = builder.constraint(&col_id, "NOT NULL", "true");
            }
            if let Some(default) = &spec.default {
                builder = builder.constraint(&col_id, "DEFAULT", default);
            }
            builder = builder.edge(&table_name, &col_id, "prop", Some(&spec.name))?;
            sig.insert(spec.name.clone(), col_id);
            specs.push(spec);
        }
    }

    for col_def in &split_column_defs(&columns_block) {
        let trimmed = col_def.trim();
        if trimmed.is_empty() {
            continue;
        }
        if is_table_constraint(trimmed) {
            builder = apply_table_constraint(builder, trimmed, &table_name, &sig);
            continue;
        }
        let (new_builder, spec) = declare_column(builder, &table_name, trimmed, catalog)?;
        builder = new_builder;
        if let Some(spec) = spec {
            sig.insert(spec.name.clone(), format!("{table_name}.{}", spec.name));
            specs.push(spec);
        }
    }

    // Create a hyper-edge for the table (connecting all columns).
    if !sig.is_empty() {
        let he_id = format!("he_{}", catalog.hyper_edges);
        catalog.hyper_edges += 1;
        builder = builder.hyper_edge(&he_id, "table", sig.clone(), &table_name)?;
    }

    catalog.columns.insert(table_name.clone(), sig);
    catalog.specs.insert(table_name, specs);
    Ok(builder)
}

/// Whether a table-body entry (or the body of an `ADD`) is a table
/// constraint rather than a column: it opens with `PRIMARY KEY`,
/// `FOREIGN KEY`, `UNIQUE`, `CHECK`, `EXCLUDE` or `CONSTRAINT name`.
///
/// The whole first word is compared, so a column whose name merely
/// starts with one of them (`unique_code`, `check_in`) is a column.
fn is_table_constraint(def: &str) -> bool {
    let word = first_word(def);
    ["CONSTRAINT", "UNIQUE", "CHECK", "EXCLUDE"]
        .iter()
        .any(|k| word.eq_ignore_ascii_case(k))
        || starts_with_keyword(def, "PRIMARY KEY")
        || starts_with_keyword(def, "FOREIGN KEY")
}

/// Record a table constraint of `table_name`, whose columns are `sig`.
fn apply_table_constraint(
    mut builder: SchemaBuilder,
    def: &str,
    table_name: &str,
    sig: &HashMap<String, String>,
) -> SchemaBuilder {
    // A named constraint, `CONSTRAINT name <constraint>`, is recorded as
    // its constraint.
    if contains_keyword(def, "PRIMARY KEY") {
        if let Some(cols) = extract_constraint_columns(def) {
            builder = builder.constraint(table_name, "PRIMARY KEY", &cols.join(","));
        }
    } else if contains_keyword(def, "FOREIGN KEY") {
        builder = parse_table_foreign_key(builder, def, table_name, sig);
    } else if contains_keyword(def, "UNIQUE") {
        if let Some(cols) = extract_constraint_columns(def) {
            builder = builder.constraint(table_name, "UNIQUE", &cols.join(","));
        }
    } else if contains_keyword(def, "CHECK") {
        if let Ok(expr) = extract_parenthesized(def) {
            builder = builder.constraint(table_name, "CHECK", &expr);
        }
    }
    builder
}

/// Declare the column `def` (`name type [constraints...]`) of
/// `table_name`: its vertex, its inline constraints and the `prop` edge
/// from the table. A definition with no type declares nothing.
fn declare_column(
    mut builder: SchemaBuilder,
    table_name: &str,
    def: &str,
    catalog: &Catalog,
) -> Result<(SchemaBuilder, Option<ColumnSpec>), ProtocolError> {
    let parts: Vec<&str> = def.split_whitespace().collect();
    if parts.len() < 2 {
        return Ok((builder, None));
    }

    let col_name = parts[0].trim_matches('"').trim_matches('`');
    let col_type = parts[1];
    let col_id = format!("{table_name}.{col_name}");

    // Determine vertex kind from SQL type.
    let kind = catalog.column_kind(col_type);
    builder = builder.vertex(&col_id, &kind, None)?;

    // Parse inline constraints.
    let rest = parts[2..].join(" ");
    let not_null = contains_keyword(&rest, "NOT NULL");
    if not_null {
        builder = builder.constraint(&col_id, "NOT NULL", "true");
    }
    if contains_keyword(&rest, "PRIMARY KEY") {
        builder = builder.constraint(&col_id, "PRIMARY KEY", "true");
    }
    if contains_keyword(&rest, "UNIQUE") {
        builder = builder.constraint(&col_id, "UNIQUE", "true");
    }
    let default = extract_default(&rest);
    if let Some(default_val) = &default {
        builder = builder.constraint(&col_id, "DEFAULT", default_val);
    }

    // Handle inline REFERENCES.
    if let Some(ref_start) = find_keyword_end(&rest, "REFERENCES") {
        let ref_table = rest[ref_start..]
            .trim()
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
    builder = builder.edge(table_name, &col_id, "prop", Some(col_name))?;

    Ok((
        builder,
        Some(ColumnSpec {
            name: col_name.to_owned(),
            kind,
            not_null,
            default,
        }),
    ))
}

/// The tables an `INHERITS (a, b)` clause after a table body names.
fn inherited_tables(tail: &str) -> Vec<&str> {
    let Some(at) = find_keyword_end(tail, "INHERITS") else {
        return Vec::new();
    };
    let rest = tail[at..].trim_start();
    let Some(inner) = rest.strip_prefix('(') else {
        return Vec::new();
    };
    let inner = inner.split(')').next().unwrap_or(inner);
    inner
        .split(',')
        .map(|t| t.trim().trim_matches('"').trim_matches('`'))
        .filter(|t| !t.is_empty())
        .collect()
}

/// The column block of a `CREATE TABLE` (between the parenthesis that
/// opens it and the one that closes it) and the text after it.
fn split_table_body(stmt: &str) -> Result<(String, &str), ProtocolError> {
    let open = stmt
        .find('(')
        .ok_or_else(|| ProtocolError::Parse("no opening parenthesis".into()))?;
    let close = matching_paren(stmt, open)
        .ok_or_else(|| ProtocolError::Parse("no closing parenthesis".into()))?;
    Ok((stmt[open + 1..close].to_string(), &stmt[close + 1..]))
}

/// The byte of the `)` that closes the `(` at byte `open`, skipping
/// quoted text.
fn matching_paren(text: &str, open: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut depth = 0usize;
    let mut i = open;
    while i < bytes.len() {
        match bytes[i] {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            q @ (b'\'' | b'"' | b'`') => {
                i = skip_quoted(bytes, i, q);
                continue;
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// Parse a table-level FOREIGN KEY constraint.
fn parse_table_foreign_key(
    mut builder: SchemaBuilder,
    constraint_str: &str,
    table_name: &str,
    sig: &HashMap<String, String>,
) -> SchemaBuilder {
    // Extract the column(s) in the FOREIGN KEY clause.
    let fk_cols = extract_constraint_columns_at(constraint_str, "FOREIGN KEY");

    // Extract REFERENCES target.
    if let Some(ref_start) = find_keyword_end(constraint_str, "REFERENCES") {
        let ref_table = constraint_str[ref_start..]
            .trim()
            .split(|c: char| c == '(' || c.is_whitespace())
            .next()
            .unwrap_or("")
            .trim();

        if !ref_table.is_empty() {
            if let Some(fk_cols) = fk_cols {
                for col in &fk_cols {
                    if let Some(col_id) = lookup_column(sig, col) {
                        builder = builder.constraint(
                            col_id,
                            "FOREIGN KEY",
                            &format!("{ref_table}.{col}"),
                        );
                    } else {
                        // Column may not exist yet; add constraint to table.
                        builder = builder.constraint(
                            table_name,
                            "FOREIGN KEY",
                            &format!("{col}->{ref_table}"),
                        );
                    }
                }
            }
        }
    }

    builder
}

/// Resolve a column name against a table's column signature.
///
/// An unquoted SQL identifier is case-insensitive, so an exact match is
/// tried first and an ASCII case-insensitive match second. Ties among
/// case variants resolve to the lexicographically smallest key so the
/// result does not depend on hash iteration order.
fn lookup_column<'a, S: BuildHasher>(
    sig: &'a HashMap<String, String, S>,
    col: &str,
) -> Option<&'a String> {
    sig.get(col).or_else(|| {
        sig.iter()
            .filter(|(k, _)| k.eq_ignore_ascii_case(col))
            .min_by(|a, b| a.0.cmp(b.0))
            .map(|(_, v)| v)
    })
}

/// Parse an ALTER TABLE statement.
///
/// `ALTER TABLE [IF EXISTS] [ONLY] name [*] action [, action ...]`. An
/// `ADD [CONSTRAINT name] <table constraint>` records the constraint as
/// a table body would, and an `ADD [COLUMN] [IF NOT EXISTS] <column>`
/// declares the column. Other actions (`OWNER TO`, `DROP COLUMN`,
/// `ALTER COLUMN`, ...) leave the schema as it is, as does any action on
/// a dropped table or on a relation that is not a declared table (a
/// sequence or a view).
fn parse_alter_table(
    mut builder: SchemaBuilder,
    stmt: &str,
    catalog: &mut Catalog,
) -> Result<SchemaBuilder, ProtocolError> {
    let after_alter = find_keyword_end(stmt, "ALTER TABLE")
        .ok_or_else(|| ProtocolError::Parse("no ALTER TABLE keyword found".into()))?;

    let mut remainder = stmt[after_alter..].trim_start();
    for guard in ["IF EXISTS", "ONLY"] {
        if let Some(rest) = strip_keyword_prefix(remainder, guard) {
            if rest.starts_with(char::is_whitespace) {
                remainder = rest.trim_start();
            }
        }
    }
    let table_end = remainder
        .find(char::is_whitespace)
        .unwrap_or(remainder.len());
    let written = remainder[..table_end].trim_end_matches('*');
    let table_name = catalog.table_named(written.trim_matches('"').trim_matches('`'));
    let actions = remainder[table_end..].trim_start().trim_start_matches('*');

    if catalog.dropped.contains(&table_name) || !catalog.columns.contains_key(&table_name) {
        return Ok(builder);
    }

    for action in split_column_defs(actions) {
        let Some(added) = strip_keyword_prefix(action.trim(), "ADD")
            .filter(|rest| rest.starts_with(char::is_whitespace))
            .map(str::trim_start)
        else {
            continue;
        };
        if is_table_constraint(added) {
            let sig = catalog
                .columns
                .get(&table_name)
                .cloned()
                .unwrap_or_default();
            builder = apply_table_constraint(builder, added, &table_name, &sig);
            continue;
        }
        let mut col_def = added;
        for guard in ["COLUMN", "IF NOT EXISTS"] {
            if let Some(rest) = strip_keyword_prefix(col_def, guard) {
                if rest.starts_with(char::is_whitespace) {
                    col_def = rest.trim_start();
                }
            }
        }
        let (new_builder, spec) = declare_column(builder, &table_name, col_def, catalog)?;
        builder = new_builder;
        if let Some(spec) = spec {
            let col_id = format!("{table_name}.{}", spec.name);
            if let Some(cols) = catalog.columns.get_mut(&table_name) {
                cols.insert(spec.name.clone(), col_id);
            }
            if let Some(specs) = catalog.specs.get_mut(&table_name) {
                specs.push(spec);
            }
        }
    }

    Ok(builder)
}

/// Extract the table name from a CREATE TABLE statement.
fn extract_table_name(stmt: &str) -> Result<String, ProtocolError> {
    // "CREATE TABLE [IF NOT EXISTS] name (...)"
    name_after_keyword(stmt, "TABLE", &["IF NOT EXISTS"])
        .map(ToOwned::to_owned)
        .ok_or_else(|| ProtocolError::Parse("no table name after TABLE keyword".into()))
}

/// Extract the table name from a DROP TABLE statement.
fn extract_drop_table_name(stmt: &str) -> Result<String, ProtocolError> {
    name_after_keyword(stmt, "TABLE", &["IF EXISTS"])
        .map(ToOwned::to_owned)
        .ok_or_else(|| ProtocolError::Parse("no table name after TABLE keyword".into()))
}

/// Extract the parenthesized block from a statement: from its first `(`
/// to its last `)`.
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

/// Split column definitions by commas, respecting nested parentheses
/// and quoted text.
fn split_column_defs(block: &str) -> Vec<String> {
    let bytes = block.as_bytes();
    let mut defs = Vec::new();
    let mut start = 0;
    let mut depth = 0usize;
    let mut i = 0;

    while i < bytes.len() {
        match bytes[i] {
            b'(' => depth += 1,
            b')' => depth = depth.saturating_sub(1),
            q @ (b'\'' | b'"' | b'`') => {
                i = skip_quoted(bytes, i, q);
                continue;
            }
            b',' if depth == 0 => {
                defs.push(block[start..i].trim().to_string());
                start = i + 1;
            }
            _ => {}
        }
        i += 1;
    }
    let last = block[start..].trim();
    if !last.is_empty() {
        defs.push(last.to_string());
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
    let idx = find_keyword_end(constraint_str, "DEFAULT")?;
    let rest = constraint_str[idx..].trim();
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
fn extract_constraint_columns_at(stmt: &str, keyword: &str) -> Option<Vec<String>> {
    let idx = find_keyword_end(stmt, keyword)?;
    let after = &stmt[idx..];
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
    fn parse_non_ascii_named_constraint() {
        let ddl = "CREATE TABLE t (a INT, CONSTRAINT \u{250}\u{250} FOREIGN KEY(a) REFERENCES \u{250}x(a));";
        let schema = parse_ddl(ddl).expect("should parse");
        let cs = schema.constraints.get("t.a").expect("column constraints");
        let fk = cs
            .iter()
            .find(|c| c.sort == "FOREIGN KEY")
            .expect("foreign key constraint");
        assert_eq!(fk.value, "\u{250}x.a");
    }

    #[test]
    fn parse_table_name_ignores_guard_phrase_in_a_literal() {
        let ddl = "CREATE TABLE users (id INTEGER, note TEXT DEFAULT 'IF NOT EXISTS');";
        let schema = parse_ddl(ddl).expect("should parse");
        assert!(schema.has_vertex("users"));
        assert!(schema.has_vertex("users.note"));
    }

    #[test]
    fn parse_lowercase_ddl() {
        let ddl = "create table users (id integer primary key, name text not null);";
        let schema = parse_ddl(ddl).expect("should parse");
        assert!(schema.has_vertex("users.name"));
        let cs = schema.constraints.get("users.name").expect("constraints");
        assert!(cs.iter().any(|c| c.sort == "NOT NULL"));
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
