//! SQL protocol definition.
//!
//! SQL uses a constrained hypergraph schema theory
//! (`colimit(ThHypergraph, ThConstraint)`) and a set-valued functor
//! instance theory (`ThFunctor`).
//!
//! Tables are modeled as hyper-edges connecting column vertices.
//! A foreign key is a `FOREIGN KEY` constraint on the referencing column
//! whose value is the referenced column's vertex id.

use std::collections::HashMap;
use std::hash::BuildHasher;

use panproto_gat::Theory;
use panproto_schema::{EdgeRule, Protocol, Schema, SchemaBuilder};

use super::keyword::{
    contains_keyword, find_keyword_end, starts_with_keyword, strip_keyword_prefix,
};
use crate::error::ProtocolError;
use crate::theories;

/// Returns the SQL protocol definition.
///
/// Schema theory: `colimit(ThHypergraph, ThConstraint)`.
/// Instance theory: `ThFunctor`.
#[must_use]
pub fn protocol() -> Protocol {
    let mut protocol = Protocol {
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
            "array".into(),
        ],
        constraint_sorts: vec![
            "NOT NULL".into(),
            "UNIQUE".into(),
            "CHECK".into(),
            "PRIMARY KEY".into(),
            "DEFAULT".into(),
            "FOREIGN KEY".into(),
            "ENUM".into(),
        ],
        has_order: true,
        nominal_identity: true,
        ..Default::default()
    };
    crate::semantics::attach(&mut protocol);
    protocol
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
/// parent's columns, and types columns with `CREATE DOMAIN`s and `CREATE
/// TYPE ... AS ENUM`s.
///
/// Statements take effect in script order: a `DROP TABLE` removes the
/// table declared before it, so the dump idiom `DROP TABLE IF EXISTS t;
/// CREATE TABLE t (...)` declares `t`.
///
/// A column of an array type (`text[]`, `integer ARRAY`) is an `array`
/// vertex with an `items` edge to its element, one level per dimension.
/// A column of an enum type is a `string` whose element carries an
/// `ENUM` constraint listing the labels as written. A `FOREIGN KEY`
/// constraint's value is the column it references, `table.column`, with
/// a reference that lists no columns resolved to the referenced table's
/// primary key.
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
    let dropped = dropped_creates(&statements);

    for (index, stmt) in statements.iter().enumerate() {
        if leading_words_are(stmt, &["CREATE", "TABLE"]) {
            if dropped.contains(&index) {
                continue;
            }
            builder = parse_create_table(builder, stmt, &mut catalog)?;
        } else if leading_words_are(stmt, &["CREATE", "DOMAIN"]) {
            catalog.declare_domain(stmt);
        } else if leading_words_are(stmt, &["CREATE", "TYPE"]) {
            catalog.declare_enum(stmt);
        } else if leading_words_are(stmt, &["ALTER", "TABLE"]) {
            builder = parse_alter_table(builder, stmt, &mut catalog)?;
        }
    }

    builder = catalog.record_foreign_keys(builder);
    let schema = builder.build()?;
    Ok(schema)
}

/// The indices of the `CREATE TABLE` statements a later `DROP TABLE`
/// removes: each `DROP TABLE` removes the latest declaration of each
/// table it names that no earlier `DROP TABLE` removed.
///
/// Names compare as [`name_key`]s, so an unquoted or bracket-quoted
/// name matches in any case. A name that matches no declaration exactly
/// matches one that differs only in qualification: `t` drops
/// `public.t`, and `public.t` drops `t`, but `a.t` does not drop `b.t`.
fn dropped_creates(statements: &[String]) -> std::collections::HashSet<usize> {
    let mut live: Vec<(Vec<String>, usize)> = Vec::new();
    let mut dropped = std::collections::HashSet::new();
    for (index, stmt) in statements.iter().enumerate() {
        if leading_words_are(stmt, &["CREATE", "TABLE"]) {
            if let Some((parts, _)) = created_table_text(stmt).and_then(read_name_parts) {
                let key = name_key(parts);
                live.retain(|(declared, _)| *declared != key);
                live.push((key, index));
            }
        } else if leading_words_are(stmt, &["DROP", "TABLE"]) {
            for parts in extract_drop_table_names(stmt) {
                let key = name_key(parts);
                let at = live
                    .iter()
                    .rposition(|(declared, _)| *declared == key)
                    .or_else(|| {
                        live.iter().rposition(|(declared, _)| {
                            same_table_up_to_qualification(declared, &key)
                        })
                    });
                if let Some(at) = at {
                    dropped.insert(live.remove(at).1);
                }
            }
        }
    }
    dropped
}

/// A name as [`read_name_parts`] reads it, as a key two spellings of one
/// name share: each case-insensitive part is folded to lowercase and
/// each case-sensitive part is kept as written.
fn name_key(parts: Vec<(String, bool)>) -> Vec<String> {
    parts
        .into_iter()
        .map(|(part, case_sensitive)| {
            if case_sensitive {
                part
            } else {
                part.to_ascii_lowercase()
            }
        })
        .collect()
}

/// Whether two name keys name one table when one of them is
/// unqualified: their last parts agree.
fn same_table_up_to_qualification(a: &[String], b: &[String]) -> bool {
    (a.len() == 1 || b.len() == 1) && a.last() == b.last()
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

/// The (possibly schema-qualified) identifier at the start of `text`,
/// after leading whitespace, and the text after it.
///
/// Each dot-separated part is a double-quoted, backquoted or (as SQLite
/// and T-SQL write them) bracket-quoted identifier, which may hold any
/// character (a doubled closing quote is an escaped one), or an unquoted
/// run up to whitespace or one of `(),;.`. The parts are returned
/// unquoted and joined by `.`.
fn read_name(text: &str) -> Option<(String, &str)> {
    read_name_parts(text).map(|(parts, after)| {
        let parts: Vec<String> = parts.into_iter().map(|(part, _)| part).collect();
        (parts.join("."), after)
    })
}

/// The parts of the (possibly schema-qualified) identifier at the start
/// of `text`, each unquoted with whether it compares case-sensitively,
/// and the text after it. See [`read_name`].
///
/// A double-quoted or backquoted part is case-sensitive, as `PostgreSQL`
/// and `MySQL` read it. An unquoted part is not, and neither is a
/// bracket-quoted one: SQLite and T-SQL, the dialects that bracket
/// identifiers, compare them without regard to case.
fn read_name_parts(text: &str) -> Option<(Vec<(String, bool)>, &str)> {
    let mut rest = text.trim_start();
    let mut parts: Vec<(String, bool)> = Vec::new();
    loop {
        let bytes = rest.as_bytes();
        let (part, after) = match bytes.first() {
            Some(b'[') => {
                let end = skip_bracketed(bytes, 0)?;
                ((rest[1..end - 1].replace("]]", "]"), false), &rest[end..])
            }
            Some(&q @ (b'"' | b'`')) => {
                let end = skip_quoted(bytes, 0, q);
                if end < 2 || bytes[end - 1] != q {
                    return None;
                }
                let doubled = if q == b'"' { "\"\"" } else { "``" };
                let single = if q == b'"' { "\"" } else { "`" };
                (
                    (rest[1..end - 1].replace(doubled, single), true),
                    &rest[end..],
                )
            }
            Some(_) => {
                let end = rest
                    .find(|c: char| c.is_whitespace() || "(),;.".contains(c))
                    .unwrap_or(rest.len());
                if end == 0 {
                    return None;
                }
                ((rest[..end].to_owned(), false), &rest[end..])
            }
            None => return None,
        };
        parts.push(part);
        match after.strip_prefix('.') {
            Some(next) => rest = next,
            None => return Some((parts, after)),
        }
    }
}

/// The byte just past the `]` that closes the bracket-quoted identifier
/// opening at byte `open`, where `]]` is an escaped `]`; `None` when it
/// is not closed.
fn skip_bracketed(bytes: &[u8], open: usize) -> Option<usize> {
    let mut j = open + 1;
    while j < bytes.len() {
        if bytes[j] == b']' {
            if bytes.get(j + 1) == Some(&b']') {
                j += 2;
            } else {
                return Some(j + 1);
            }
        } else {
            j += 1;
        }
    }
    None
}

/// The byte just past the bracket-quoted identifier that the `[` at byte
/// `i` opens, or `None` when that `[` opens an array or a subscript (or
/// is not closed).
///
/// A `[` opens an array when `in_array` (it is inside another array's
/// brackets, as the inner rows of `ARRAY[[1, 2], [3, 4]]` are), when it
/// follows the word `ARRAY`, and when it is written against the text
/// before it (`INT[]`, `VARCHAR(5)[]`, `x[1]`). Otherwise it opens an
/// identifier, which the scanners pass over whole, so a quote,
/// parenthesis, comma or `;` inside it (`[a"b]`, `[e(f]`) is part of
/// the name.
fn bracket_identifier_end(bytes: &[u8], i: usize, in_array: bool) -> Option<usize> {
    if in_array {
        return None;
    }
    let before = bytes[..i].trim_ascii_end();
    if let Some(&last) = before.last()
        && before.len() == i
        && (is_ident_byte(last) || matches!(last, b']' | b')' | b'['))
    {
        return None;
    }
    let word = before
        .iter()
        .rposition(|b| !is_ident_byte(*b))
        .map_or(before, |p| &before[p + 1..]);
    if word.eq_ignore_ascii_case(b"ARRAY") {
        return None;
    }
    skip_bracketed(bytes, i)
}

/// The identifiers of a parenthesized, comma-separated column list such
/// as `("a", b)`, given the text inside the parentheses.
fn identifier_list(inner: &str) -> Vec<String> {
    split_column_defs(inner)
        .iter()
        .filter_map(|item| read_name(item).map(|(name, _)| name))
        .collect()
}

/// A column type as the parse resolves it.
#[derive(Clone, Debug)]
struct ColumnType {
    /// The vertex kind of the element.
    kind: String,
    /// For an enum type, its labels, each as a standard SQL string
    /// literal (`'it''s'`) however the script spelled it, comma-separated.
    /// A comma inside a label is inside its quotes, so a quote-aware
    /// split recovers the labels.
    labels: Option<String>,
    /// The number of array dimensions around the element.
    dims: usize,
}

/// A column as declared, kept so that a table that `INHERITS` from its
/// table declares the same column.
#[derive(Clone, Debug)]
struct ColumnSpec {
    name: String,
    ty: ColumnType,
    not_null: bool,
    default: Option<String>,
}

/// A `FOREIGN KEY` read from the script, recorded once every table and
/// primary key is known.
struct ForeignKey {
    /// The declaring table.
    table: String,
    /// The referencing column as written.
    column: String,
    /// The referencing column's vertex, when the table declares it.
    column_id: Option<String>,
    /// The referenced table as written.
    ref_table: String,
    /// The referenced column as written, or `None` for the referenced
    /// table's primary key.
    ref_column: Option<String>,
    /// The column's position in the constraint's column list, which
    /// picks the primary-key column it references.
    position: usize,
}

/// What the parse knows of the declarations read so far.
#[derive(Default)]
struct Catalog {
    /// Per table: column name to column vertex id.
    columns: HashMap<String, HashMap<String, String>>,
    /// Per table: its columns in declaration order.
    specs: HashMap<String, Vec<ColumnSpec>>,
    /// Per table: its primary-key columns.
    primary_keys: HashMap<String, Vec<String>>,
    /// `CREATE DOMAIN` and `CREATE TYPE ... AS ENUM` names, lowercased
    /// and unqualified, to the type they declare.
    types: HashMap<String, ColumnType>,
    /// The foreign keys read so far.
    foreign_keys: Vec<ForeignKey>,
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

    /// The type of a column declared with the words `words` after its
    /// name: its first word names the element type (through a declared
    /// domain or enum), and among the words of the type (up to the first
    /// constraint, clause or literal) each array bound suffix (`[]`,
    /// `[3]`) and an `ARRAY` keyword add dimensions.
    ///
    /// A bracketed word that is not an array bound is a bracket-quoted
    /// identifier (`[int]`), which names the type.
    fn column_type(&self, words: &[&str]) -> ColumnType {
        let Some((first, rest)) = words.split_first() else {
            return ColumnType {
                kind: "string".into(),
                labels: None,
                dims: 0,
            };
        };
        let mut bounds = 0;
        let mut keyword = false;
        let type_words = (0..rest.len())
            .find(|&i| ends_column_type(rest[i], rest.get(i + 1).copied()))
            .unwrap_or(rest.len());
        for word in std::iter::once(*first).chain(rest[..type_words].iter().copied()) {
            let (head, count) = array_bounds(word);
            bounds += count;
            keyword |= head.eq_ignore_ascii_case("ARRAY");
        }
        let dims = if keyword { bounds.max(1) } else { bounds };

        let (head, _) = array_bounds(first);
        let written = if head.starts_with(['[', '"', '`']) {
            read_name(head).map_or_else(|| head.to_owned(), |(name, _)| name)
        } else {
            head.split('(').next().unwrap_or(head).to_owned()
        };
        let mut ty = self
            .types
            .get(&unqualified(&written).to_ascii_lowercase())
            .cloned()
            .unwrap_or_else(|| ColumnType {
                kind: sql_type_to_kind(unqualified(&written)),
                labels: None,
                dims: 0,
            });
        ty.dims += dims;
        ty
    }

    /// Record `CREATE DOMAIN name [AS] data_type ...`.
    fn declare_domain(&mut self, stmt: &str) {
        let Some(after) = find_keyword_end(stmt, "DOMAIN") else {
            return;
        };
        let Some((name, rest)) = read_name(&stmt[after..]) else {
            return;
        };
        let words: Vec<&str> = rest.split_whitespace().collect();
        let words = match words.split_first() {
            Some((first, tail)) if first.eq_ignore_ascii_case("AS") => tail,
            _ => &words[..],
        };
        if !words.is_empty() {
            let ty = self.column_type(words);
            self.types
                .insert(unqualified(&name).to_ascii_lowercase(), ty);
        }
    }

    /// Record `CREATE TYPE name AS ENUM ('label', ...)`. Other type
    /// declarations (composite, range, base) declare no column type the
    /// parser reads, so a column of one keeps its fallback kind.
    fn declare_enum(&mut self, stmt: &str) {
        let Some(after) = find_keyword_end(stmt, "TYPE") else {
            return;
        };
        let Some((name, rest)) = read_name(&stmt[after..]) else {
            return;
        };
        let words: Vec<&str> = rest.split_whitespace().take(2).collect();
        let is_enum = words.len() == 2
            && words[0].eq_ignore_ascii_case("AS")
            && first_word(words[1]).eq_ignore_ascii_case("ENUM");
        if !is_enum {
            return;
        }
        let Some(open) = rest.find('(') else {
            return;
        };
        let Some(close) = matching_paren(rest, open) else {
            return;
        };
        let labels: Vec<String> = split_column_defs(&rest[open + 1..close])
            .iter()
            .map(|written| {
                sql_string_value(written)
                    .map_or_else(|| written.clone(), |v| sql_string_literal(&v))
            })
            .collect();
        self.types.insert(
            unqualified(&name).to_ascii_lowercase(),
            ColumnType {
                kind: "string".into(),
                labels: Some(labels.join(",")),
                dims: 0,
            },
        );
    }

    /// Add the `FOREIGN KEY` constraint of every foreign key read.
    ///
    /// The value is the referenced column, `table.column`, spelled as the
    /// referenced table declares it when it does. A reference with no
    /// column list refers to the referenced table's primary key; when that
    /// is not declared, the value is the referenced table alone. A
    /// constraint on a column its table does not declare is recorded on
    /// the table, as `column->reference`.
    fn record_foreign_keys(&self, mut builder: SchemaBuilder) -> SchemaBuilder {
        for fk in &self.foreign_keys {
            let table = self.table_named(&fk.ref_table);
            let ref_column = fk.ref_column.clone().or_else(|| {
                self.primary_keys
                    .get(&table)
                    .and_then(|pk| pk.get(fk.position))
                    .cloned()
            });
            let target = match ref_column {
                Some(col) => self
                    .columns
                    .get(&table)
                    .and_then(|sig| lookup_column(sig, &col))
                    .cloned()
                    .unwrap_or_else(|| format!("{table}.{col}")),
                None => table,
            };
            builder = match &fk.column_id {
                Some(col_id) => builder.constraint(col_id, "FOREIGN KEY", &target),
                None => builder.constraint(
                    &fk.table,
                    "FOREIGN KEY",
                    &format!("{}->{target}", fk.column),
                ),
            };
        }
        builder
    }
}

/// Whether `word` opens a column constraint, which ends the column's
/// type.
fn is_constraint_word(word: &str) -> bool {
    [
        "NOT",
        "NULL",
        "DEFAULT",
        "PRIMARY",
        "UNIQUE",
        "CHECK",
        "REFERENCES",
        "CONSTRAINT",
        "COLLATE",
        "GENERATED",
        "AUTO_INCREMENT",
        "AUTOINCREMENT",
    ]
    .iter()
    .any(|k| first_word(word).eq_ignore_ascii_case(k))
}

/// The value of the SQL string literal `written`: `'...'` with `''` for
/// a quote, or `E'...'` which also takes backslash escapes. `None` for
/// anything else.
fn sql_string_value(written: &str) -> Option<String> {
    let written = written.trim();
    let (backslash, quoted) = match written.strip_prefix(['E', 'e']) {
        Some(rest) if rest.starts_with('\'') => (true, rest),
        _ => (false, written),
    };
    let body = quoted.strip_prefix('\'')?.strip_suffix('\'')?;
    let mut value = String::with_capacity(body.len());
    let mut chars = body.chars();
    while let Some(c) = chars.next() {
        match c {
            '\'' => {
                // Inside the literal a quote is only ever doubled.
                if chars.next() != Some('\'') {
                    return None;
                }
                value.push('\'');
            }
            '\\' if backslash => match chars.next()? {
                'b' => value.push('\u{8}'),
                'f' => value.push('\u{c}'),
                'n' => value.push('\n'),
                'r' => value.push('\r'),
                't' => value.push('\t'),
                other => value.push(other),
            },
            other => value.push(other),
        }
    }
    Some(value)
}

/// `value` as a standard SQL string literal, `'...'` with `''` for a
/// quote.
fn sql_string_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

/// Whether `word`, after a column's first type word, begins a clause
/// that follows the type (`COMMENT`, `CHARSET`, `ON UPDATE`, `AS (...)`,
/// ...). `CHARACTER`, which is also a word of a type (`NATIONAL
/// CHARACTER VARYING`), begins one only as `CHARACTER SET`, which
/// [`ends_column_type`] reads with the word after it.
fn is_type_clause_word(word: &str) -> bool {
    [
        "COMMENT",
        "CHARSET",
        "ON",
        "AS",
        "VISIBLE",
        "INVISIBLE",
        "STORED",
        "VIRTUAL",
        "IDENTITY",
        "SRID",
        "COLUMN_FORMAT",
        "STORAGE",
        "COMPRESSION",
    ]
    .iter()
    .any(|k| first_word(word).eq_ignore_ascii_case(k))
}

/// Whether `word`, after a column's first type word and followed by
/// `next`, is past the type: a constraint, a string literal, a clause
/// word, or the `CHARACTER` of `CHARACTER SET`.
fn ends_column_type(word: &str, next: Option<&str>) -> bool {
    is_constraint_word(word)
        || is_type_clause_word(word)
        || first_word(word).eq_ignore_ascii_case("CHARACTER")
            && next.is_some_and(|n| first_word(n).eq_ignore_ascii_case("SET"))
        || word.starts_with('\'')
        || word.starts_with(['E', 'e']) && word[1..].starts_with('\'')
}

/// A type word without its trailing array bounds (`[]`, `[3]`), and the
/// number of them. A bracketed suffix holding anything but digits is
/// not a bound, so a bracket-quoted identifier (`[int]`) has none.
fn array_bounds(word: &str) -> (&str, usize) {
    let mut head = word;
    let mut count = 0;
    while let Some(inner) = head.strip_suffix(']') {
        let Some(open) = inner.rfind('[') else {
            break;
        };
        let bound = inner[open + 1..].trim();
        if !bound.bytes().all(|b| b.is_ascii_digit()) {
            break;
        }
        head = &inner[..open];
        count += 1;
    }
    (head, count)
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
    let mut arrays = 0usize;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'[' => {
                if let Some(end) = bracket_identifier_end(bytes, i, arrays > 0) {
                    i = end;
                } else {
                    arrays += 1;
                    i += 1;
                }
            }
            b']' => {
                arrays = arrays.saturating_sub(1);
                i += 1;
            }
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
                arrays = 0;
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
    let (table_name, after_name) = extract_table_name(stmt)?;

    // Create a vertex for the table.
    builder = builder.vertex(&table_name, "table", None)?;

    // The column block, and what follows it.
    let (columns_block, tail) = split_table_body(after_name)?;

    let mut sig = HashMap::new();
    let mut specs: Vec<ColumnSpec> = Vec::new();

    for parent in inherited_tables(tail) {
        let parent = catalog.table_named(&parent);
        for spec in catalog.specs.get(&parent).cloned().unwrap_or_default() {
            if sig.contains_key(&spec.name) {
                continue;
            }
            let col_id = format!("{table_name}.{}", spec.name);
            builder = column_vertices(builder, &col_id, &spec.ty)?;
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

    // Column constraints refer to the columns declared before them, and
    // table constraints may come first, so the columns are read first.
    let defs = split_column_defs(&columns_block);
    let mut constraints = Vec::new();
    for col_def in &defs {
        let trimmed = col_def.trim();
        if trimmed.is_empty() {
            continue;
        }
        if is_table_constraint(trimmed) {
            constraints.push(trimmed);
            continue;
        }
        let (new_builder, spec) = declare_column(builder, &table_name, trimmed, catalog)?;
        builder = new_builder;
        if let Some(spec) = spec {
            sig.insert(spec.name.clone(), format!("{table_name}.{}", spec.name));
            specs.push(spec);
        }
    }
    for def in constraints {
        builder = apply_table_constraint(builder, def, &table_name, &sig, catalog);
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
    catalog: &mut Catalog,
) -> SchemaBuilder {
    // A named constraint, `CONSTRAINT name <constraint>`, is recorded as
    // its constraint.
    if contains_keyword(def, "PRIMARY KEY") {
        if let Some(cols) = extract_constraint_columns(def) {
            builder = builder.constraint(table_name, "PRIMARY KEY", &cols.join(","));
            catalog.primary_keys.insert(table_name.to_owned(), cols);
        }
    } else if contains_keyword(def, "FOREIGN KEY") {
        read_table_foreign_key(def, table_name, sig, catalog);
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

/// Declare the vertex `col_id` of a column of type `ty`: for an array,
/// an `array` vertex per dimension, each with an `items` edge to the
/// next, around the element; the element carries an enum's `ENUM`
/// constraint.
fn column_vertices(
    mut builder: SchemaBuilder,
    col_id: &str,
    ty: &ColumnType,
) -> Result<SchemaBuilder, ProtocolError> {
    let mut levels = vec![col_id.to_owned()];
    for d in 0..ty.dims {
        let item = format!("{}:items", levels[d]);
        levels.push(item);
    }
    for (d, vertex) in levels.iter().enumerate() {
        let kind = if d < ty.dims { "array" } else { &ty.kind };
        builder = builder.vertex(vertex, kind, None)?;
    }
    for pair in levels.windows(2) {
        builder = builder.edge(&pair[0], &pair[1], "items", None)?;
    }
    if let (Some(labels), Some(element)) = (&ty.labels, levels.last()) {
        builder = builder.constraint(element, "ENUM", labels);
    }
    Ok(builder)
}

/// Declare the column `def` (`name type [constraints...]`) of
/// `table_name`: its vertex, its inline constraints and the `prop` edge
/// from the table. A definition with no type declares nothing.
fn declare_column(
    mut builder: SchemaBuilder,
    table_name: &str,
    def: &str,
    catalog: &mut Catalog,
) -> Result<(SchemaBuilder, Option<ColumnSpec>), ProtocolError> {
    let Some((col_name, after_name)) = read_name(def) else {
        return Ok((builder, None));
    };
    let parts: Vec<&str> = after_name.split_whitespace().collect();
    if parts.is_empty() {
        return Ok((builder, None));
    }

    let col_id = format!("{table_name}.{col_name}");
    let ty = catalog.column_type(&parts);
    builder = column_vertices(builder, &col_id, &ty)?;

    // Parse inline constraints.
    let rest = parts[1..].join(" ");
    let not_null = contains_keyword(&rest, "NOT NULL");
    if not_null {
        builder = builder.constraint(&col_id, "NOT NULL", "true");
    }
    if contains_keyword(&rest, "PRIMARY KEY") {
        builder = builder.constraint(&col_id, "PRIMARY KEY", "true");
        catalog
            .primary_keys
            .insert(table_name.to_owned(), vec![col_name.clone()]);
    }
    if contains_keyword(&rest, "UNIQUE") {
        builder = builder.constraint(&col_id, "UNIQUE", "true");
    }
    let default = extract_default(&rest);
    if let Some(default_val) = &default {
        builder = builder.constraint(&col_id, "DEFAULT", default_val);
    }

    // Handle inline REFERENCES.
    if let Some((ref_table, ref_columns)) = references(&rest) {
        catalog.foreign_keys.push(ForeignKey {
            table: table_name.to_owned(),
            column: col_name.clone(),
            column_id: Some(col_id.clone()),
            ref_table,
            ref_column: ref_columns.into_iter().next(),
            position: 0,
        });
    }

    // Add a prop edge from table to column.
    builder = builder.edge(table_name, &col_id, "prop", Some(&col_name))?;

    Ok((
        builder,
        Some(ColumnSpec {
            name: col_name,
            ty,
            not_null,
            default,
        }),
    ))
}

/// The table and the columns a `REFERENCES table [(col, ...)]` clause in
/// `text` names; the column list is empty when the clause has none.
fn references(text: &str) -> Option<(String, Vec<String>)> {
    let at = find_keyword_end(text, "REFERENCES")?;
    let (table, rest) = read_name(&text[at..])?;
    let rest = rest.trim_start();
    let columns = if rest.starts_with('(') {
        matching_paren(rest, 0)
            .map(|close| identifier_list(&rest[1..close]))
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    Some((table, columns))
}

/// The tables an `INHERITS (a, b)` clause after a table body names.
fn inherited_tables(tail: &str) -> Vec<String> {
    let Some(at) = find_keyword_end(tail, "INHERITS") else {
        return Vec::new();
    };
    let rest = tail[at..].trim_start();
    if !rest.starts_with('(') {
        return Vec::new();
    }
    matching_paren(rest, 0)
        .map(|close| identifier_list(&rest[1..close]))
        .unwrap_or_default()
}

/// The column block of a `CREATE TABLE` (between the parenthesis that
/// opens it and the one that closes it) and the text after it, given
/// the text after the table's name.
fn split_table_body(after_name: &str) -> Result<(String, &str), ProtocolError> {
    let open = after_name
        .find('(')
        .ok_or_else(|| ProtocolError::Parse("no opening parenthesis".into()))?;
    let close = matching_paren(after_name, open)
        .ok_or_else(|| ProtocolError::Parse("no closing parenthesis".into()))?;
    Ok((
        after_name[open + 1..close].to_string(),
        &after_name[close + 1..],
    ))
}

/// The byte of the `)` that closes the `(` at byte `open`, skipping
/// quoted text and bracket-quoted identifiers.
fn matching_paren(text: &str, open: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut depth = 0usize;
    let mut arrays = 0usize;
    let mut i = open;
    while i < bytes.len() {
        match bytes[i] {
            b'[' => match bracket_identifier_end(bytes, i, arrays > 0) {
                Some(end) => {
                    i = end;
                    continue;
                }
                None => arrays += 1,
            },
            b']' => arrays = arrays.saturating_sub(1),
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

/// Read a table-level `FOREIGN KEY (cols) REFERENCES table [(cols)]`
/// constraint into the catalog, pairing the columns by position.
fn read_table_foreign_key(
    constraint_str: &str,
    table_name: &str,
    sig: &HashMap<String, String>,
    catalog: &mut Catalog,
) {
    let Some(fk_cols) = extract_constraint_columns_at(constraint_str, "FOREIGN KEY") else {
        return;
    };
    let Some((ref_table, ref_columns)) = references(constraint_str) else {
        return;
    };
    for (position, col) in fk_cols.iter().enumerate() {
        catalog.foreign_keys.push(ForeignKey {
            table: table_name.to_owned(),
            column: col.clone(),
            column_id: lookup_column(sig, col).cloned(),
            ref_table: ref_table.clone(),
            ref_column: ref_columns.get(position).cloned(),
            position,
        });
    }
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
/// a table that is not declared at that point in the script (a dropped
/// table, a sequence or a view).
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
    let Some((written, after_name)) = read_name(remainder) else {
        return Ok(builder);
    };
    let table_name = catalog.table_named(&written);
    let actions = after_name.trim_start().trim_start_matches('*');

    if !catalog.columns.contains_key(&table_name) {
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
            builder = apply_table_constraint(builder, added, &table_name, &sig, catalog);
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

/// Extract the table name from a CREATE TABLE statement, and the text
/// after it.
fn extract_table_name(stmt: &str) -> Result<(String, &str), ProtocolError> {
    let rest =
        created_table_text(stmt).ok_or_else(|| ProtocolError::Parse("no TABLE keyword".into()))?;
    read_name(rest).ok_or_else(|| ProtocolError::Parse("no table name after TABLE keyword".into()))
}

/// The text of a `CREATE TABLE [IF NOT EXISTS] name (...)` statement
/// from its table name on.
fn created_table_text(stmt: &str) -> Option<&str> {
    let after = find_keyword_end(stmt, "TABLE")?;
    let rest = stmt[after..].trim_start();
    Some(
        strip_keyword_prefix(rest, "IF NOT EXISTS")
            .filter(|stripped| stripped.starts_with(char::is_whitespace))
            .unwrap_or(rest),
    )
}

/// The table names a `DROP TABLE [IF EXISTS] a [, b ...]` statement
/// names, each as [`read_name_parts`] reads it.
fn extract_drop_table_names(stmt: &str) -> Vec<Vec<(String, bool)>> {
    let Some(after) = find_keyword_end(stmt, "TABLE") else {
        return Vec::new();
    };
    let mut rest = stmt[after..].trim_start();
    if let Some(stripped) = strip_keyword_prefix(rest, "IF EXISTS") {
        if stripped.starts_with(char::is_whitespace) {
            rest = stripped;
        }
    }
    let mut names = Vec::new();
    while let Some((name, after_name)) = read_name_parts(rest) {
        names.push(name);
        match after_name.trim_start().strip_prefix(',') {
            Some(next) => rest = next,
            None => break,
        }
    }
    names
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

/// Split column definitions by commas, respecting nested parentheses,
/// brackets (an `ARRAY[1, 2]` literal), bracket-quoted identifiers and
/// quoted text.
fn split_column_defs(block: &str) -> Vec<String> {
    let bytes = block.as_bytes();
    let mut defs = Vec::new();
    let mut start = 0;
    let mut depth = 0usize;
    let mut arrays = 0usize;
    let mut i = 0;

    while i < bytes.len() {
        match bytes[i] {
            b'[' => match bracket_identifier_end(bytes, i, arrays > 0) {
                Some(end) => {
                    i = end;
                    continue;
                }
                None => arrays += 1,
            },
            b']' => arrays = arrays.saturating_sub(1),
            b'(' => depth += 1,
            b')' => depth = depth.saturating_sub(1),
            q @ (b'\'' | b'"' | b'`') => {
                i = skip_quoted(bytes, i, q);
                continue;
            }
            b',' if depth == 0 && arrays == 0 => {
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

/// Extract a DEFAULT value from a constraint string: the expression
/// after `DEFAULT` up to the first whitespace or comma outside
/// parentheses, brackets and quoted text, so `ARRAY[1, 2]`,
/// `nextval('s')` and `'a b'` are read whole.
fn extract_default(constraint_str: &str) -> Option<String> {
    let idx = find_keyword_end(constraint_str, "DEFAULT")?;
    let rest = constraint_str[idx..].trim();
    let bytes = rest.as_bytes();
    let mut depth = 0usize;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'(' | b'[' => depth += 1,
            b')' | b']' => depth = depth.saturating_sub(1),
            q @ (b'\'' | b'"' | b'`') => {
                i = skip_quoted(bytes, i, q);
                continue;
            }
            b',' if depth == 0 => break,
            b if depth == 0 && b.is_ascii_whitespace() => break,
            _ => {}
        }
        i += 1;
    }
    let val = rest[..i.min(rest.len())].trim().to_string();
    if val.is_empty() { None } else { Some(val) }
}

/// Extract column names from a constraint like `PRIMARY KEY(col1, col2)`.
fn extract_constraint_columns(constraint_str: &str) -> Option<Vec<String>> {
    let open = constraint_str.find('(')?;
    let close = matching_paren(constraint_str, open)?;
    let cols = identifier_list(&constraint_str[open + 1..close]);
    if cols.is_empty() { None } else { Some(cols) }
}

/// Extract column names from a constraint starting at a specific keyword.
fn extract_constraint_columns_at(stmt: &str, keyword: &str) -> Option<Vec<String>> {
    let idx = find_keyword_end(stmt, keyword)?;
    extract_constraint_columns(&stmt[idx..])
}

/// `name` as it is written in DDL: as is when it is a plain identifier
/// (a letter or `_`, then letters, digits, `_` and `$`), and otherwise
/// double-quoted, with `""` for a `"`, so a re-parse reads the same
/// name.
fn quoted_identifier(name: &str) -> std::borrow::Cow<'_, str> {
    let plain = name
        .bytes()
        .next()
        .is_some_and(|b| b.is_ascii_alphabetic() || b == b'_' || b >= 0x80)
        && name.bytes().all(is_ident_byte);
    if plain {
        std::borrow::Cow::Borrowed(name)
    } else {
        std::borrow::Cow::Owned(format!("\"{}\"", name.replace('"', "\"\"")))
    }
}

/// A (possibly schema-qualified) table name as it is written in DDL:
/// each `.`-separated part as [`quoted_identifier`] writes it, so
/// `public.t` stays qualified and `my table` is `"my table"`.
fn quoted_name(name: &str) -> String {
    name.split('.')
        .map(quoted_identifier)
        .collect::<Vec<_>>()
        .join(".")
}

/// The name of the enum type emit declares for column `column` of
/// `table`: `<table>_<column>_enum`, with each byte that cannot continue
/// an identifier (a space, a `.` of a qualified table name) written as
/// `_`, and a leading `_` before a digit, so the name is a plain
/// identifier the column's type reads as one word.
fn enum_type_stem(table: &str, column: &str) -> String {
    let stem: String = format!("{table}_{column}_enum")
        .chars()
        .map(|c| {
            if c.is_ascii() && !is_ident_byte(c as u8) {
                '_'
            } else {
                c
            }
        })
        .collect();
    if stem.starts_with(|c: char| c.is_ascii_digit()) {
        format!("_{stem}")
    } else {
        stem
    }
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
/// column types (an array column as its element type with a `[]` per
/// dimension) and constraints (`NOT NULL`, `PRIMARY KEY`, `UNIQUE`,
/// `DEFAULT`). A column whose element carries an `ENUM` constraint is of
/// an enum type, `CREATE TYPE <table>_<column>_enum AS ENUM (...)`,
/// declared before its table, so a re-parse reads the same labels.
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

    // The enum types declared so far, by name, with their labels.
    let mut enum_types: HashMap<String, String> = HashMap::new();

    for table in &tables {
        let mut body = String::new();
        let _ = writeln!(body, "CREATE TABLE {} (", quoted_name(&table.id));

        let columns = children_by_edge(schema, &table.id, "prop");
        let col_count = columns.len();
        for (i, (edge, col_vertex)) in columns.iter().enumerate() {
            let col_name = edge.name.as_deref().unwrap_or(&col_vertex.id);

            // An array column is written as its element's type with a
            // `[]` per dimension.
            let mut element = *col_vertex;
            let mut dims = String::new();
            while element.kind == "array" {
                let Some((_, item)) = children_by_edge(schema, &element.id, "items")
                    .first()
                    .copied()
                else {
                    break;
                };
                element = item;
                dims.push_str("[]");
            }
            // An enum's labels, which constrain the element, are written
            // as an enum type declared before the table, which the
            // element is of.
            let labels = vertex_constraints(schema, &element.id)
                .into_iter()
                .find(|c| c.sort == "ENUM")
                .map(|c| c.value.clone());
            let element_type = match labels {
                Some(labels) => {
                    let stem = enum_type_stem(&table.id, col_name);
                    let mut name = stem.clone();
                    let mut n = 1;
                    while enum_types
                        .get(&name)
                        .is_some_and(|declared| *declared != labels)
                    {
                        n += 1;
                        name = format!("{stem}{n}");
                    }
                    if !enum_types.contains_key(&name) {
                        let _ = writeln!(output, "CREATE TYPE {name} AS ENUM ({labels});\n");
                        enum_types.insert(name.clone(), labels);
                    }
                    name
                }
                None => kind_to_sql_type(&element.kind).to_owned(),
            };
            let sql_type = format!("{element_type}{dims}");

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
            let col_name = quoted_identifier(col_name);
            let _ = writeln!(body, "  {col_name} {sql_type}{constraints_str}{comma}");
        }

        body.push_str(");\n\n");
        output.push_str(&body);
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
        EdgeRule {
            edge_kind: "items".into(),
            src_kinds: vec!["array".into()],
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

    #[test]
    fn emit_ddl_keeps_array_and_enum_columns() {
        let ddl = r"
            CREATE TYPE mood AS ENUM ('sad', 'ok');
            CREATE TABLE t (
                m mood,
                labels TEXT[],
                grid INTEGER[][]
            );
        ";
        let schema1 = parse_ddl(ddl).expect("first parse should succeed");
        let emitted = emit_ddl(&schema1).expect("emit should succeed");
        assert!(emitted.contains("labels TEXT[]"), "{emitted}");
        assert!(emitted.contains("grid INTEGER[][]"), "{emitted}");
        assert!(
            emitted.contains("CREATE TYPE t_m_enum AS ENUM ('sad','ok');"),
            "{emitted}"
        );
        assert!(emitted.contains("m t_m_enum"), "{emitted}");
        let schema2 = parse_ddl(&emitted).expect("re-parse should succeed");
        for v in ["t.m", "t.labels", "t.labels:items", "t.grid:items:items"] {
            assert_eq!(schema1.vertices[v].kind, schema2.vertices[v].kind, "{v}");
            assert_eq!(
                schema1.constraints.get(v),
                schema2.constraints.get(v),
                "{v}"
            );
        }
    }

    #[test]
    fn a_foreign_key_declared_before_its_table_resolves_to_its_primary_key() {
        let ddl = r"
            CREATE TABLE child (parent_ref INTEGER REFERENCES parent);
            CREATE TABLE parent (code TEXT, PRIMARY KEY (code));
        ";
        let schema = parse_ddl(ddl).expect("should parse");
        let fk = schema.constraints["child.parent_ref"]
            .iter()
            .find(|c| c.sort == "FOREIGN KEY")
            .expect("foreign key constraint");
        assert_eq!(fk.value, "parent.code");
    }
}
