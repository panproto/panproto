//! Reading `PostgreSQL` 17 DDL through the bundled tree-sitter grammar.
//!
//! The grammar is [gmr/tree-sitter-postgres](https://github.com/gmr/tree-sitter-postgres),
//! generated from `PostgreSQL`'s own `gram.y`, so the script is parsed as
//! `PostgreSQL` parses it, tokens included. This module walks the syntax
//! tree of each statement into the statement model of
//! [`panproto_protocols::database::postgresql`], and
//! [`read_statements`]
//! resolves the statements into table schemas.
//!
//! The tree mirrors `gram.y` and names no fields, so the walk identifies
//! each part of a statement by its node type and keyword children, flattens
//! the grammar's left-recursive lists, and folds the `DEFERRABLE` and
//! `INITIALLY` clauses that follow a column constraint onto it, as
//! `PostgreSQL`'s `transformConstraintAttrs` does. Every refusal names the
//! byte offset of the token it is about; a script that is not `PostgreSQL`
//! syntax is refused at the first place the parser could not read.

mod data;
mod directory;
mod resolution;
mod values;

use std::cell::RefCell;

use panproto_protocols::database::postgresql::{
    AlterAction, BaseType, ColumnConstraint, ColumnConstraintKind, ColumnDef, DdlReading,
    DomainConstraints, MAX_IDENTIFIER_BYTES, Name, OtherStatements, References, ReferentialAction,
    Scalar, SqlDdlError, Statement, StatementNotRead, TableConstraint, TableConstraintKind,
    TableElement, TypeSpec, read_statements,
};
use panproto_schema::Schema;
use tree_sitter::{Node, Parser, Tree};

pub use data::{
    DumpReading, PostgresqlCodec, SqlDataError, WriteRefusal, read_data, read_dump, write_data,
    write_dump,
};
pub use directory::{
    Dialect, MANIFEST, MANIFEST_VERSION, Manifest, TableDirectoryError, TableFile,
    load_table_directory, read_table_directory, store_table_directory, write_table_directory,
};
pub use resolution::{ForeignKeyResolution, resolve_foreign_keys};
pub use values::ValueRefusal;

/// The name the grammar is bundled under.
const GRAMMAR: &str = "postgresql";

thread_local! {
    /// The parser over the bundled grammar, made once per thread.
    static PARSER: RefCell<Option<Parser>> = const { RefCell::new(None) };
}

/// Parse `src` with the bundled `PostgreSQL` grammar.
fn parse(src: &str) -> Result<Tree, SqlDdlError> {
    let unavailable = |reason: &str| SqlDdlError::Unsupported {
        offset: 0,
        feature: format!("PostgreSQL syntax: {reason}"),
    };
    PARSER.with(|cell| {
        let mut slot = cell.borrow_mut();
        if slot.is_none() {
            let grammar = panproto_grammars::grammars()
                .into_iter()
                .find(|g| g.name == GRAMMAR)
                .ok_or_else(|| unavailable("this build did not compile the PostgreSQL grammar"))?;
            let mut parser = Parser::new();
            parser
                .set_language(&grammar.language)
                .map_err(|e| unavailable(&e.to_string()))?;
            *slot = Some(parser);
        }
        slot.as_mut()
            .and_then(|parser| parser.parse(src, None))
            .ok_or_else(|| unavailable("the parser returned no tree"))
    })
}

/// Read a `PostgreSQL` 17 DDL script into the tables it declares.
///
/// The statements read are `CREATE TABLE`, `CREATE UNIQUE INDEX`, `CREATE
/// TYPE ... AS ENUM`, `CREATE DOMAIN`, `ALTER TABLE ... ADD` and `DROP
/// TABLE`, each taking effect in script order. A statement that declares
/// nothing a table schema holds (a function, view, trigger, sequence,
/// non-unique index, grant, ownership change, comment or session setting,
/// or `COPY` and `INSERT`) is refused or listed as `others` says.
///
/// # Errors
///
/// Returns the first [`SqlDdlError`] in script order: text that is not
/// `PostgreSQL` syntax, a named feature the reader does not carry, or a
/// name that does not resolve.
pub fn read_ddl(src: &str, others: OtherStatements) -> Result<DdlReading, SqlDdlError> {
    let tree = parse(src)?;
    read_statements(statements(src, &tree), others)
}

/// Read a `PostgreSQL` 17 DDL script into a [`Schema`], refusing any
/// statement that declares something other than tables and their types.
///
/// This is [`read_ddl`] with [`OtherStatements::Refuse`].
///
/// # Errors
///
/// Returns the first refusal in script order, as [`read_ddl`] does.
pub fn parse_ddl(src: &str) -> Result<Schema, SqlDdlError> {
    Ok(read_ddl(src, OtherStatements::Refuse)?.schema)
}

/// The statements of the script, in order, ending at the first refusal.
fn statements(src: &str, tree: &Tree) -> Vec<Result<Statement, SqlDdlError>> {
    let w = Walk { src };
    let root = tree.root_node();
    let error = first_error(root).map(|node| w.syntax_error(node));
    let error_at = error.as_ref().map_or(usize::MAX, SqlDdlError::offset);
    let mut out = Vec::new();
    for top in children(root) {
        if top.kind() != "toplevel_stmt" {
            continue;
        }
        if top.end_byte() > error_at || top.has_error() {
            break;
        }
        let Some(stmt) = children(top).into_iter().find(|n| n.kind() == "stmt") else {
            continue;
        };
        let Some(body) = children(stmt).into_iter().next() else {
            continue;
        };
        let statement = w.statement(body);
        let refused = statement.is_err();
        out.push(statement);
        if refused {
            return out;
        }
    }
    if let Some(error) = error {
        out.push(Err(error));
    }
    out
}

/// The first place the parser could not read: the innermost error or
/// missing node at the earliest position.
fn first_error(node: Node<'_>) -> Option<Node<'_>> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.is_missing() {
            return Some(child);
        }
        if child.is_error() || child.has_error() {
            return first_error(child).or_else(|| child.is_error().then_some(child));
        }
    }
    None
}

fn unsupported(offset: usize, feature: impl Into<String>) -> SqlDdlError {
    SqlDdlError::Unsupported {
        offset,
        feature: feature.into(),
    }
}

fn duplicate(offset: usize, what: &'static str, name: &str) -> SqlDdlError {
    SqlDdlError::Duplicate {
        offset,
        what,
        name: name.to_owned(),
    }
}

/// Statements that declare nothing a table schema holds, by their leading
/// keywords. They are listed or refused, never read.
const OTHER_STATEMENTS: &[(&[&str], &str)] = &[
    (&["set"], "SET"),
    (&["reset"], "RESET"),
    (&["select"], "SELECT"),
    (&["grant"], "GRANT"),
    (&["revoke"], "REVOKE"),
    (&["comment", "on"], "COMMENT ON"),
    (&["begin"], "BEGIN"),
    (&["commit"], "COMMIT"),
    (&["start", "transaction"], "START TRANSACTION"),
    (&["copy"], "COPY"),
    (&["insert"], "INSERT"),
    (&["create", "index"], "CREATE INDEX"),
    (&["create", "view"], "CREATE VIEW"),
    (&["create", "or", "replace", "view"], "CREATE VIEW"),
    (
        &["create", "materialized", "view"],
        "CREATE MATERIALIZED VIEW",
    ),
    (&["create", "function"], "CREATE FUNCTION"),
    (&["create", "or", "replace", "function"], "CREATE FUNCTION"),
    (&["create", "procedure"], "CREATE PROCEDURE"),
    (
        &["create", "or", "replace", "procedure"],
        "CREATE PROCEDURE",
    ),
    (&["create", "trigger"], "CREATE TRIGGER"),
    (&["create", "or", "replace", "trigger"], "CREATE TRIGGER"),
    (&["create", "rule"], "CREATE RULE"),
    (&["create", "or", "replace", "rule"], "CREATE RULE"),
    (&["create", "aggregate"], "CREATE AGGREGATE"),
    (
        &["create", "or", "replace", "aggregate"],
        "CREATE AGGREGATE",
    ),
    (&["create", "sequence"], "CREATE SEQUENCE"),
    (&["create", "extension"], "CREATE EXTENSION"),
    (&["create", "language"], "CREATE LANGUAGE"),
    (&["create", "procedural", "language"], "CREATE LANGUAGE"),
    (&["create", "or", "replace", "language"], "CREATE LANGUAGE"),
    (
        &["create", "or", "replace", "procedural", "language"],
        "CREATE LANGUAGE",
    ),
    (&["create", "schema"], "CREATE SCHEMA"),
    (&["create", "type"], "CREATE TYPE"),
    (&["alter", "function"], "ALTER FUNCTION"),
    (&["alter", "procedure"], "ALTER PROCEDURE"),
    (&["alter", "aggregate"], "ALTER AGGREGATE"),
    (&["alter", "sequence"], "ALTER SEQUENCE"),
    (&["alter", "view"], "ALTER VIEW"),
    (&["alter", "schema"], "ALTER SCHEMA"),
    (&["alter", "language"], "ALTER LANGUAGE"),
    (&["alter", "procedural", "language"], "ALTER LANGUAGE"),
];

/// Built-in `PostgreSQL` 17 types (Chapter 8) the reader does not carry.
const UNSUPPORTED_TYPES: &[&str] = &[
    "money",
    "interval",
    "timetz",
    "xml",
    "inet",
    "cidr",
    "macaddr",
    "macaddr8",
    "point",
    "line",
    "lseg",
    "box",
    "path",
    "polygon",
    "circle",
    "bit",
    "varbit",
    "tsvector",
    "tsquery",
    "pg_lsn",
    "pg_snapshot",
    "txid_snapshot",
    "int4range",
    "int8range",
    "numrange",
    "tsrange",
    "tstzrange",
    "daterange",
    "int4multirange",
    "int8multirange",
    "nummultirange",
    "tsmultirange",
    "tstzmultirange",
    "datemultirange",
    "oid",
    "regclass",
    "regtype",
    "regproc",
    "name",
    "char",
    "jsonpath",
    "record",
];

/// The type names a quoted identifier may spell: the catalog names of the
/// built-in types the reader carries.
const CATALOG_TYPE_NAMES: &[&str] = &[
    "int2",
    "int4",
    "int8",
    "numeric",
    "float4",
    "float8",
    "text",
    "varchar",
    "bpchar",
    "bool",
    "bytea",
    "date",
    "time",
    "timestamp",
    "timestamptz",
    "uuid",
    "json",
    "jsonb",
];

/// A type modifier: its value when it is an integer constant, and the
/// byte offset of its expression.
type Modifier = (Option<u32>, usize);

/// A constraint attribute the reader accepts: the defaults, which declare
/// nothing a schema holds.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Deferral {
    NotDeferrable,
    InitiallyImmediate,
    Enforced,
}

/// A walk over the syntax tree of `src`.
struct Walk<'s> {
    src: &'s str,
}

impl<'s> Walk<'s> {
    fn text(&self, node: Node<'_>) -> &'s str {
        &self.src[node.start_byte()..node.end_byte()]
    }

    /// The refusal of text the grammar does not read, at `node`.
    fn syntax_error(&self, node: Node<'_>) -> SqlDdlError {
        // The place is the first token the node covers or, for a token the
        // parser found missing, the token that follows it.
        let offset = leaves(node)
            .into_iter()
            .find(|leaf| leaf.start_byte() < leaf.end_byte())
            .map_or_else(
                || {
                    let start = node.start_byte();
                    let rest = &self.src[start..];
                    start + (rest.len() - rest.trim_start().len())
                },
                |leaf| leaf.start_byte(),
            );
        let found = self.src[offset..]
            .split_whitespace()
            .next()
            .map_or_else(|| "end of input".to_owned(), str::to_owned);
        // Another dialect's quoted identifier is not PostgreSQL syntax;
        // the refusal names the dialect.
        if found.starts_with('`') {
            return unsupported(offset, "MySQL backquoted identifier");
        }
        let bracketed = self.src[offset..]
            .strip_prefix('[')
            .and_then(|rest| rest.split_once(']'))
            .is_some_and(|(name, _)| {
                !name.is_empty()
                    && !name.contains(|c: char| c.is_whitespace() || ",()[".contains(c))
            });
        if bracketed {
            return unsupported(offset, "T-SQL bracket-quoted identifier");
        }
        SqlDdlError::Syntax {
            offset,
            expected: "PostgreSQL 17 syntax",
            found,
        }
    }

    fn syntax(&self, node: Node<'_>, expected: &'static str) -> SqlDdlError {
        SqlDdlError::Syntax {
            offset: node.start_byte(),
            expected,
            found: self.text(node).to_owned(),
        }
    }

    // ── Names and strings ───────────────────────────────────────────────

    /// The identifier `node` spells (a `ColId`, `ColLabel`, `name` and the
    /// like, one token): an unquoted one folded to lowercase, a quoted one
    /// kept whole.
    fn identifier(&self, node: Node<'_>) -> Result<Name, SqlDdlError> {
        let leaves = leaves(node);
        let [leaf] = leaves.as_slice() else {
            return Err(self.syntax(node, "a name"));
        };
        let raw = self.text(*leaf);
        let offset = leaf.start_byte();
        let text = if leaf.kind() == "quoted_identifier" {
            raw.strip_prefix('"')
                .and_then(|r| r.strip_suffix('"'))
                .ok_or_else(|| self.syntax(*leaf, "a quoted identifier"))?
                .replace("\"\"", "\"")
        } else {
            raw.to_ascii_lowercase()
        };
        if text.len() > MAX_IDENTIFIER_BYTES {
            return Err(unsupported(offset, "identifier longer than 63 bytes"));
        }
        if text.contains(['.', ':']) {
            return Err(unsupported(offset, "identifier containing `.` or `:`"));
        }
        Ok(Name { text, offset })
    }

    /// The parts of a dotted name: a `qualified_name`, `any_name` or a
    /// `GenericType`'s name with its `attrs`.
    fn name_parts(&self, node: Node<'_>) -> Result<Vec<Name>, SqlDdlError> {
        let mut parts = Vec::new();
        for c in named(node) {
            match c.kind() {
                "ColId" | "type_function_name" | "ColLabel" | "attr_name" => {
                    parts.push(self.identifier(c)?);
                }
                "indirection" | "attrs" | "indirection_el" => parts.extend(self.name_parts(c)?),
                "opt_type_modifiers" => {}
                _ => return Err(self.syntax(c, "a name")),
            }
        }
        Ok(parts)
    }

    /// An object in the `public` schema: `name` or `public.name`.
    fn object_name(&self, node: Node<'_>) -> Result<Name, SqlDdlError> {
        let mut parts = self.name_parts(node)?;
        match parts.len() {
            1 => Ok(parts.remove(0)),
            2 => {
                if parts[0].text != "public" {
                    return Err(unsupported(parts[0].offset, "a schema other than public"));
                }
                Ok(parts.remove(1))
            }
            _ => Err(unsupported(
                node.start_byte(),
                "a name qualified by a database",
            )),
        }
    }

    /// The table a `relation_expr` names, with or without `ONLY` and `*`.
    fn relation(&self, node: Node<'_>) -> Result<Name, SqlDdlError> {
        let mut current = node;
        loop {
            if current.kind() == "qualified_name" {
                return self.object_name(current);
            }
            let next = named(current)
                .into_iter()
                .find(|c| matches!(c.kind(), "qualified_name" | "extended_relation_expr"));
            match next {
                Some(next) => current = next,
                None => return Err(self.syntax(current, "a table name")),
            }
        }
    }

    /// The column names of a `columnList` (or the list inside an
    /// `opt_column_list`).
    fn column_list(&self, node: Node<'_>) -> Result<Vec<Name>, SqlDdlError> {
        let list = if node.kind() == "columnList" {
            node
        } else {
            child(node, "columnList").ok_or_else(|| self.syntax(node, "a column list"))?
        };
        flatten(list)
            .into_iter()
            .map(|elem| match child(elem, "ColId") {
                Some(col) if named(elem).len() == 1 => self.identifier(col),
                _ => Err(unsupported(elem.start_byte(), "PERIOD")),
            })
            .collect()
    }

    /// The value of a string constant (`Sconst`): standard and `E` strings
    /// are decoded; `None` for a form whose value is not read.
    fn string(&self, node: Node<'_>) -> Option<String> {
        let mut value = Vec::new();
        for leaf in leaves(node) {
            let raw = self.text(leaf).as_bytes();
            let (body, escapes) = match leaf.kind() {
                "string_literal" => (raw, false),
                "escape_string_literal" => (raw.get(1..)?, true),
                _ => return None,
            };
            let inner = body.strip_prefix(b"'")?.strip_suffix(b"'")?;
            let mut j = 0;
            while j < inner.len() {
                match inner[j] {
                    b'\'' => {
                        value.push(b'\'');
                        j += 2;
                    }
                    b'\\' if escapes => j = escape(inner, j, &mut value)?,
                    b => {
                        value.push(b);
                        j += 1;
                    }
                }
            }
        }
        String::from_utf8(value).ok()
    }

    // ── Statements ──────────────────────────────────────────────────────

    fn statement(&self, node: Node<'_>) -> Result<Statement, SqlDdlError> {
        let words = leading_words(node);
        let at = |w: &[&str]| words.len() >= w.len() && words.iter().zip(w).all(|(a, b)| a == b);
        let offset = node.start_byte();
        if at(&["create", "table"]) {
            return self.create_table(node);
        }
        if at(&["create", "unique", "index"]) && node.kind() == "IndexStmt" {
            return self.create_unique_index(node);
        }
        if at(&["create", "domain"]) && node.kind() == "CreateDomainStmt" {
            return self.create_domain(node);
        }
        if at(&["create", "type"]) && node.kind() == "DefineStmt" && has(node, "kw_enum") {
            return self.create_enum(node);
        }
        if at(&["alter", "table"]) && node.kind() == "AlterTableStmt" {
            return self.alter_table(node);
        }
        if at(&["drop", "table"]) && node.kind() == "DropStmt" {
            return self.drop_table(node);
        }
        if words.first().is_some_and(|w| w == "create")
            && words.get(1).is_some_and(|w| {
                matches!(
                    w.as_str(),
                    "temp" | "temporary" | "unlogged" | "global" | "local"
                )
            })
        {
            return Err(unsupported(offset, "CREATE TEMPORARY or UNLOGGED TABLE"));
        }
        if node.kind() == "AlterOwnerStmt" && (at(&["alter", "type"]) || at(&["alter", "domain"])) {
            return Ok(not_read("ALTER OWNER TO", offset));
        }
        let class = OTHER_STATEMENTS
            .iter()
            .filter(|(w, _)| at(w))
            .max_by_key(|(w, _)| w.len())
            .map(|(_, class)| *class);
        if let Some(class) = class {
            // A statement that sets the schema search path changes how the
            // names after it resolve, unless it sets it to `public` or empty.
            if matches!(class, "SET" | "SELECT") && self.sets_another_search_path(node) {
                return Err(unsupported(offset, "a search_path other than public"));
            }
            return Ok(not_read(class, offset));
        }
        let words: Vec<String> = leaves(node)
            .into_iter()
            .take(3)
            .take_while(|l| l.kind().starts_with("kw_") || l.kind() == "identifier")
            .map(|l| self.text(l).to_ascii_uppercase())
            .collect();
        Err(unsupported(
            offset,
            format!("statement {}", words.join(" ")),
        ))
    }

    /// Whether this `SET search_path ...` or `SELECT
    /// pg_catalog.set_config('search_path', ...)` names a schema other
    /// than `public` and `pg_catalog`.
    fn sets_another_search_path(&self, node: Node<'_>) -> bool {
        const WORDS: &[&str] = &[
            "search_path",
            "to",
            "public",
            "pg_catalog",
            "set_config",
            "false",
            "true",
            "session",
            "local",
        ];
        let leaves = leaves(node);
        let word = |leaf: Node<'_>| -> Option<String> {
            match leaf.kind() {
                "identifier" => Some(self.text(leaf).to_ascii_lowercase()),
                "quoted_identifier" => self.identifier(leaf).ok().map(|n| n.text),
                k if k.starts_with("kw_") => Some(self.text(leaf).to_ascii_lowercase()),
                _ => None,
            }
        };
        let strings: Vec<String> = leaves
            .iter()
            .filter(|l| l.kind().ends_with("string_literal"))
            .filter_map(|l| self.string(*l))
            .collect();
        let mentions = leaves
            .iter()
            .any(|l| word(*l).is_some_and(|w| w == "search_path"))
            || strings.iter().any(|s| s == "search_path");
        if !mentions {
            return false;
        }
        let other_word = leaves
            .iter()
            .skip(1)
            .filter_map(|l| word(*l))
            .any(|w| !WORDS.contains(&w.as_str()));
        let other_string = strings.iter().any(|s| {
            s.split(',')
                .map(str::trim)
                .any(|p| !["", "search_path", "public", "pg_catalog"].contains(&p))
        });
        other_word || other_string
    }

    fn create_table(&self, node: Node<'_>) -> Result<Statement, SqlDdlError> {
        if node.kind() == "CreateAsStmt" {
            let at = child(node, "kw_as").unwrap_or(node);
            return Err(unsupported(at.start_byte(), "CREATE TABLE AS"));
        }
        if node.kind() != "CreateStmt" {
            return Err(self.syntax(node, "CREATE TABLE"));
        }
        let if_not_exists = has(node, "kw_exists");
        let name_node =
            child(node, "qualified_name").ok_or_else(|| self.syntax(node, "a table name"))?;
        let name = self.object_name(name_node)?;
        if let Some(of) = child(node, "kw_of") {
            return Err(unsupported(of.start_byte(), "typed table (OF type)"));
        }
        if let Some(partition) = child(node, "kw_partition") {
            return Err(unsupported(partition.start_byte(), "PARTITION OF"));
        }
        let mut elements = Vec::new();
        if let Some(list) =
            child(node, "OptTableElementList").and_then(|l| child(l, "TableElementList"))
        {
            for element in flatten(list) {
                elements.push(self.table_element(element)?);
            }
        }
        for clause in named(node) {
            if clause.start_byte() == clause.end_byte() {
                continue;
            }
            let feature = match clause.kind() {
                "OptInherit" => "INHERITS",
                "OptPartitionSpec" | "PartitionSpec" => "PARTITION BY",
                "table_access_method_clause" => "USING (table access method)",
                "OptWith" => "table storage parameters",
                "OnCommitOption" => "ON COMMIT",
                "OptTableSpace" => "TABLESPACE",
                _ => continue,
            };
            return Err(unsupported(clause.start_byte(), feature));
        }
        Ok(Statement::CreateTable {
            if_not_exists,
            name,
            elements,
        })
    }

    fn table_element(&self, node: Node<'_>) -> Result<TableElement, SqlDdlError> {
        let inner = named(node)
            .into_iter()
            .next()
            .ok_or_else(|| self.syntax(node, "a table element"))?;
        match inner.kind() {
            "columnDef" => Ok(TableElement::Column(self.column_def(inner)?)),
            "TableConstraint" => Ok(TableElement::Constraint(self.table_constraint(inner)?)),
            "TableLikeClause" => Err(unsupported(inner.start_byte(), "LIKE")),
            _ => Err(self.syntax(inner, "a column or table constraint")),
        }
    }

    fn column_def(&self, node: Node<'_>) -> Result<ColumnDef, SqlDdlError> {
        let name_node = child(node, "ColId").ok_or_else(|| self.syntax(node, "a column name"))?;
        let name = self.identifier(name_node)?;
        let typename = child(node, "Typename").ok_or_else(|| self.syntax(node, "a column type"))?;
        let ty = self.type_spec(typename)?;
        if matches!(ty.base, BaseType::Serial(_)) && ty.dims > 0 {
            return Err(unsupported(ty.offset, "an array of a serial type"));
        }
        for c in named(node) {
            if matches!(c.kind(), "opt_column_storage" | "opt_column_compression")
                && c.start_byte() < c.end_byte()
            {
                return Err(unsupported(c.start_byte(), "column storage options"));
            }
            if c.kind() == "create_generic_options" && c.start_byte() < c.end_byte() {
                return Err(unsupported(c.start_byte(), "column options"));
            }
        }
        let constraints = match child(node, "ColQualList") {
            Some(list) => self.column_constraints(list, &name, &ty)?,
            None => Vec::new(),
        };
        Ok(ColumnDef {
            name,
            ty,
            constraints,
        })
    }

    /// The constraints of a column's `ColQualList`, with the deferrability
    /// clauses after a constraint folded onto it.
    fn column_constraints(
        &self,
        list: Node<'_>,
        column: &Name,
        ty: &TypeSpec,
    ) -> Result<Vec<ColumnConstraint>, SqlDdlError> {
        let mut out: Vec<ColumnConstraint> = Vec::new();
        let mut null_declared = false;
        let mut default_declared = matches!(ty.base, BaseType::Serial(_));
        let mut deferrals: Vec<Deferral> = Vec::new();
        for item in flatten(list) {
            let children = named(item);
            if let Some(attr) = children.iter().find(|c| c.kind() == "ConstraintAttr") {
                let deferral = deferral(*attr)?;
                let takes_attrs = out.last().is_some_and(|c| {
                    matches!(
                        c.kind,
                        ColumnConstraintKind::Unique
                            | ColumnConstraintKind::PrimaryKey
                            | ColumnConstraintKind::References(_)
                    )
                });
                if !takes_attrs {
                    return Err(self.syntax(*attr, "a key or foreign key before the clause"));
                }
                if deferrals.contains(&deferral) {
                    return Err(self.syntax(*attr, "one clause of each kind"));
                }
                deferrals.push(deferral);
                continue;
            }
            deferrals.clear();
            if has(item, "kw_collate") {
                let collation =
                    child(item, "any_name").ok_or_else(|| self.syntax(item, "a collation"))?;
                self.collation(collation)?;
                continue;
            }
            let constraint_name = match child(item, "name") {
                Some(n) if has(item, "kw_constraint") => Some(self.identifier(n)?),
                _ => None,
            };
            let elem = child(item, "ColConstraintElem")
                .ok_or_else(|| self.syntax(item, "a column constraint"))?;
            let at = elem.start_byte();
            let kind = self.column_constraint(elem)?;
            match &kind {
                ColumnConstraintKind::NotNull | ColumnConstraintKind::Null => {
                    if null_declared {
                        return Err(duplicate(at, "NULL declaration", &column.text));
                    }
                    null_declared = true;
                }
                ColumnConstraintKind::Default(_) => {
                    if default_declared {
                        return Err(duplicate(at, "DEFAULT", &column.text));
                    }
                    default_declared = true;
                }
                _ => {}
            }
            out.push(ColumnConstraint {
                name: constraint_name,
                kind,
                offset: at,
            });
        }
        Ok(out)
    }

    /// One `ColConstraintElem`.
    fn column_constraint(&self, elem: Node<'_>) -> Result<ColumnConstraintKind, SqlDdlError> {
        let at = elem.start_byte();
        let words = keywords(elem);
        let first = words.first().map_or("", String::as_str);
        match first {
            "not" if words.get(1).is_some_and(|w| w == "null") => {
                if let Some(rest) = child(elem, "opt_no_inherit")
                    && rest.start_byte() < rest.end_byte()
                {
                    return Err(unsupported(rest.start_byte(), "NO INHERIT"));
                }
                Ok(ColumnConstraintKind::NotNull)
            }
            "null" => Ok(ColumnConstraintKind::Null),
            "default" => {
                let expr = child(elem, "b_expr")
                    .ok_or_else(|| self.syntax(elem, "a DEFAULT expression"))?;
                Ok(ColumnConstraintKind::Default(self.text(expr).to_owned()))
            }
            "check" => {
                let expr =
                    child(elem, "a_expr").ok_or_else(|| self.syntax(elem, "a CHECK expression"))?;
                if let Some(rest) = child(elem, "opt_no_inherit")
                    && rest.start_byte() < rest.end_byte()
                {
                    return Err(unsupported(rest.start_byte(), "NO INHERIT"));
                }
                Ok(ColumnConstraintKind::Check(self.text(expr).to_owned()))
            }
            "unique" => {
                key_options(elem)?;
                Ok(ColumnConstraintKind::Unique)
            }
            "primary" => {
                key_options(elem)?;
                Ok(ColumnConstraintKind::PrimaryKey)
            }
            "references" => Ok(ColumnConstraintKind::References(self.references(elem)?)),
            "generated" => Err(unsupported(
                at,
                if has(elem, "kw_identity") {
                    "identity column"
                } else {
                    "generated column"
                },
            )),
            _ => Err(unsupported(
                at,
                format!("column constraint {}", first.to_ascii_uppercase()),
            )),
        }
    }

    /// The clauses of a table constraint's `ConstraintAttributeSpec`.
    fn constraint_attributes(&self, spec: Node<'_>) -> Result<(), SqlDdlError> {
        let mut seen: Vec<Deferral> = Vec::new();
        for elem in flatten(spec) {
            let deferral = deferral(elem)?;
            if seen.contains(&deferral) {
                return Err(self.syntax(elem, "one clause of each kind"));
            }
            seen.push(deferral);
        }
        Ok(())
    }

    /// The `COLLATE` name: `"default"`, `"C"` and `"POSIX"` compare by
    /// bytes, as keys do; any other collation is refused.
    fn collation(&self, node: Node<'_>) -> Result<(), SqlDdlError> {
        let at = node.start_byte();
        let parts = self.name_parts(node)?;
        let name = match parts.as_slice() {
            [name] => name,
            [schema, name] if schema.text == "pg_catalog" => name,
            _ => return Err(unsupported(at, "a collation outside pg_catalog")),
        };
        if matches!(name.text.as_str(), "default" | "C" | "POSIX") {
            Ok(())
        } else {
            Err(unsupported(at, format!("COLLATE {}", name.text)))
        }
    }

    /// The `REFERENCES` clause in `node` (a column constraint or a table
    /// `FOREIGN KEY`).
    fn references(&self, node: Node<'_>) -> Result<References, SqlDdlError> {
        let target_node =
            child(node, "qualified_name").ok_or_else(|| self.syntax(node, "a referenced table"))?;
        let target = self.object_name(target_node)?;
        let target_columns = match child(node, "opt_column_list")
            .or_else(|| child(node, "opt_column_and_period_list"))
        {
            Some(list) if list.start_byte() < list.end_byte() => Some(self.column_list(list)?),
            _ => None,
        };
        let mut match_full = false;
        if let Some(m) = child(node, "key_match")
            && m.start_byte() < m.end_byte()
        {
            let words = keywords(m);
            match words.get(1).map(String::as_str) {
                Some("full") => match_full = true,
                Some("simple") => {}
                _ => return Err(unsupported(m.start_byte(), "MATCH PARTIAL")),
            }
        }
        let mut on_delete = ReferentialAction::NoAction;
        let mut on_update = ReferentialAction::NoAction;
        if let Some(actions) = child(node, "key_actions") {
            for clause in named(actions) {
                let action = child(clause, "key_action")
                    .ok_or_else(|| self.syntax(clause, "a referential action"))?;
                let value = self.referential_action(action)?;
                if has(clause, "kw_delete") {
                    on_delete = value;
                } else {
                    on_update = value;
                }
            }
        }
        if let Some(spec) = child(node, "ConstraintAttributeSpec") {
            self.constraint_attributes(spec)?;
        }
        Ok(References {
            target,
            target_columns,
            match_full,
            on_delete,
            on_update,
        })
    }

    fn referential_action(&self, node: Node<'_>) -> Result<ReferentialAction, SqlDdlError> {
        if let Some(list) = child(node, "opt_column_list")
            && list.start_byte() < list.end_byte()
        {
            return Err(unsupported(
                list.start_byte(),
                "a column list on SET NULL or SET DEFAULT",
            ));
        }
        let words = keywords(node);
        let words: Vec<&str> = words.iter().map(String::as_str).collect();
        match words.as_slice() {
            ["no", "action"] => Ok(ReferentialAction::NoAction),
            ["restrict"] => Ok(ReferentialAction::Restrict),
            ["cascade"] => Ok(ReferentialAction::Cascade),
            ["set", "null"] => Ok(ReferentialAction::SetNull),
            ["set", "default"] => Ok(ReferentialAction::SetDefault),
            _ => Err(self.syntax(node, "a referential action")),
        }
    }

    fn table_constraint(&self, node: Node<'_>) -> Result<TableConstraint, SqlDdlError> {
        let name = match child(node, "name") {
            Some(n) => Some(self.identifier(n)?),
            None => None,
        };
        let elem =
            child(node, "ConstraintElem").ok_or_else(|| self.syntax(node, "a table constraint"))?;
        let offset = elem.start_byte();
        let words = keywords(elem);
        let kind = match words.first().map(String::as_str) {
            Some("check") => {
                let expr =
                    child(elem, "a_expr").ok_or_else(|| self.syntax(elem, "a CHECK expression"))?;
                if let Some(spec) = child(elem, "ConstraintAttributeSpec") {
                    self.constraint_attributes(spec)?;
                }
                TableConstraintKind::Check(self.text(expr).to_owned())
            }
            Some(word @ ("unique" | "primary")) => {
                key_options(elem)?;
                let list = child(elem, "columnList")
                    .ok_or_else(|| unsupported(offset, "index parameters"))?;
                let columns = self.column_list(list)?;
                if let Some(spec) = child(elem, "ConstraintAttributeSpec") {
                    self.constraint_attributes(spec)?;
                }
                if word == "unique" {
                    TableConstraintKind::Unique(columns)
                } else {
                    TableConstraintKind::PrimaryKey(columns)
                }
            }
            Some("foreign") => {
                let list =
                    child(elem, "columnList").ok_or_else(|| self.syntax(elem, "a column list"))?;
                let columns = self.column_list(list)?;
                TableConstraintKind::ForeignKey(columns, self.references(elem)?)
            }
            Some("exclude") => return Err(unsupported(offset, "EXCLUDE")),
            Some("not") => return Err(unsupported(offset, "a table NOT NULL constraint")),
            _ => return Err(self.syntax(elem, "a table constraint")),
        };
        Ok(TableConstraint { name, kind, offset })
    }

    fn create_unique_index(&self, node: Node<'_>) -> Result<Statement, SqlDdlError> {
        let if_not_exists = has(node, "kw_exists");
        let name = match child(node, "name").or_else(|| child(node, "opt_single_name")) {
            Some(n) if n.start_byte() < n.end_byte() => Some(self.identifier(n)?),
            _ => None,
        };
        let relation =
            child(node, "relation_expr").ok_or_else(|| self.syntax(node, "the indexed table"))?;
        let table = self.relation(relation)?;
        let mut columns = Vec::new();
        for c in named(node) {
            if c.start_byte() == c.end_byte() {
                continue;
            }
            match c.kind() {
                "access_method_clause" => {
                    let method =
                        child(c, "name").ok_or_else(|| self.syntax(c, "an index method"))?;
                    let method_name = self.identifier(method)?;
                    if method_name.text != "btree" {
                        return Err(unsupported(
                            method_name.offset,
                            format!("index method {}", method_name.text),
                        ));
                    }
                }
                "index_params" => {
                    for elem in flatten(c) {
                        let parts = named(elem);
                        match parts.as_slice() {
                            [col] if col.kind() == "ColId" => columns.push(self.identifier(*col)?),
                            [col, next, ..] if col.kind() == "ColId" => {
                                return Err(unsupported(
                                    next.start_byte(),
                                    "an index column with an operator class, collation or ordering",
                                ));
                            }
                            _ => {
                                return Err(unsupported(
                                    elem.start_byte(),
                                    "an index on an expression",
                                ));
                            }
                        }
                    }
                }
                "opt_unique_null_treatment" => {
                    if has(c, "kw_not") {
                        return Err(unsupported(c.start_byte(), "UNIQUE NULLS NOT DISTINCT"));
                    }
                }
                "where_clause" => return Err(unsupported(c.start_byte(), "a partial index")),
                "opt_include" => return Err(unsupported(c.start_byte(), "INCLUDE")),
                "opt_reloptions" | "OptTableSpace" => {
                    return Err(unsupported(c.start_byte(), "index parameters"));
                }
                _ => {}
            }
        }
        Ok(Statement::CreateUniqueIndex {
            if_not_exists,
            name,
            table,
            columns,
        })
    }

    fn create_domain(&self, node: Node<'_>) -> Result<Statement, SqlDdlError> {
        let name_node =
            child(node, "any_name").ok_or_else(|| self.syntax(node, "a domain name"))?;
        let name = self.object_name(name_node)?;
        let typename =
            child(node, "Typename").ok_or_else(|| self.syntax(node, "the domain's type"))?;
        let ty = self.type_spec(typename)?;
        let mut constraints = DomainConstraints {
            not_null: false,
            default: None,
            checks: Vec::new(),
        };
        let mut null_declared = false;
        if let Some(list) = child(node, "ColQualList") {
            for item in flatten(list) {
                let at = item.start_byte();
                if has(item, "kw_collate") {
                    return Err(unsupported(at, "COLLATE"));
                }
                let constraint_name = match child(item, "name") {
                    Some(n) => Some(self.identifier(n)?),
                    None => None,
                };
                let Some(elem) = child(item, "ColConstraintElem") else {
                    return Err(self.syntax(item, "a domain constraint"));
                };
                let at = elem.start_byte();
                match self.column_constraint(elem)? {
                    ColumnConstraintKind::NotNull | ColumnConstraintKind::Null => {
                        if null_declared {
                            return Err(duplicate(at, "NULL declaration", "NULL"));
                        }
                        null_declared = true;
                        constraints.not_null =
                            keywords(elem).first().map(String::as_str) == Some("not");
                    }
                    ColumnConstraintKind::Check(expr) => {
                        constraints.checks.push((constraint_name, expr));
                    }
                    ColumnConstraintKind::Default(expr) if constraint_name.is_none() => {
                        if constraints.default.is_some() {
                            return Err(duplicate(at, "DEFAULT", "DEFAULT"));
                        }
                        constraints.default = Some(expr);
                    }
                    _ => return Err(self.syntax(elem, "a domain constraint")),
                }
            }
        }
        Ok(Statement::CreateDomain {
            name,
            ty,
            constraints,
        })
    }

    fn create_enum(&self, node: Node<'_>) -> Result<Statement, SqlDdlError> {
        let name_node = child(node, "any_name").ok_or_else(|| self.syntax(node, "a type name"))?;
        let name = self.object_name(name_node)?;
        let mut labels: Vec<String> = Vec::new();
        let list = child(node, "opt_enum_val_list").and_then(|l| child(l, "enum_val_list"));
        if let Some(list) = list {
            for label in flatten(list) {
                let at = label.start_byte();
                let value = self
                    .string(label)
                    .filter(|v| v.len() <= MAX_IDENTIFIER_BYTES)
                    .ok_or_else(|| {
                        self.syntax(label, "an enum label: a string of at most 63 bytes")
                    })?;
                if labels.contains(&value) {
                    return Err(duplicate(at, "enum label", &value));
                }
                labels.push(value);
            }
        }
        Ok(Statement::CreateEnum { name, labels })
    }

    fn alter_table(&self, node: Node<'_>) -> Result<Statement, SqlDdlError> {
        let offset = node.start_byte();
        let if_exists = has(node, "kw_exists");
        let relation = child(node, "relation_expr")
            .ok_or_else(|| unsupported(offset, "ALTER TABLE of more than one table"))?;
        let table = self.relation(relation)?;
        let commands = child(node, "alter_table_cmds")
            .map(|c| flatten(c))
            .ok_or_else(|| unsupported(offset, "ALTER TABLE without an action"))?;
        if let [command] = commands.as_slice()
            && keywords(*command).first().is_some_and(|w| w == "owner")
        {
            return Ok(not_read("ALTER TABLE OWNER TO", offset));
        }
        // Switching triggers off or on, as a data-only dump does around its
        // rows, declares nothing a table schema holds.
        let trigger_switches = commands.iter().all(|c| {
            let words = keywords(*c);
            matches!(
                words.first().map(String::as_str),
                Some("enable" | "disable")
            ) && words.iter().any(|w| w == "trigger")
        });
        if trigger_switches {
            return Ok(not_read("ALTER TABLE ENABLE or DISABLE TRIGGER", offset));
        }
        let mut actions = Vec::new();
        for command in commands {
            let words = keywords(command);
            if words.first().is_none_or(|w| w != "add") {
                let named: Vec<String> = leaves(command)
                    .into_iter()
                    .take(2)
                    .take_while(|l| l.kind().starts_with("kw_"))
                    .map(|l| self.text(l).to_ascii_uppercase())
                    .collect();
                return Err(unsupported(
                    command.start_byte(),
                    format!("ALTER TABLE {}", named.join(" ")),
                ));
            }
            if let Some(constraint) = child(command, "TableConstraint") {
                actions.push(AlterAction::AddConstraint(
                    self.table_constraint(constraint)?,
                ));
            } else if let Some(column) = child(command, "columnDef") {
                actions.push(AlterAction::AddColumn {
                    if_not_exists: has(command, "kw_exists"),
                    column: self.column_def(column)?,
                });
            } else {
                return Err(self.syntax(command, "a column or table constraint"));
            }
        }
        Ok(Statement::AlterTable {
            if_exists,
            table,
            actions,
        })
    }

    fn drop_table(&self, node: Node<'_>) -> Result<Statement, SqlDdlError> {
        let if_exists = has(node, "kw_exists");
        let list =
            child(node, "any_name_list").ok_or_else(|| self.syntax(node, "the tables to drop"))?;
        let names = flatten(list)
            .into_iter()
            .map(|n| self.object_name(n))
            .collect::<Result<Vec<_>, _>>()?;
        if let Some(behavior) = child(node, "opt_drop_behavior")
            && has(behavior, "kw_cascade")
        {
            return Err(unsupported(behavior.start_byte(), "DROP TABLE CASCADE"));
        }
        Ok(Statement::DropTable { if_exists, names })
    }

    // ── Types ───────────────────────────────────────────────────────────

    /// A `Typename`: its element type and array dimensions.
    fn type_spec(&self, node: Node<'_>) -> Result<TypeSpec, SqlDdlError> {
        let offset = node.start_byte();
        if has(node, "kw_setof") {
            return Err(unsupported(offset, "SETOF"));
        }
        let simple = child(node, "SimpleTypename").ok_or_else(|| self.syntax(node, "a type"))?;
        let base = self.simple_type(simple)?;
        let dims = if has(node, "kw_array") {
            1
        } else {
            child(node, "opt_array_bounds").map_or(0, |bounds| {
                leaves(bounds)
                    .into_iter()
                    .filter(|l| self.text(*l) == "[")
                    .count()
            })
        };
        Ok(TypeSpec { base, dims, offset })
    }

    fn simple_type(&self, node: Node<'_>) -> Result<BaseType, SqlDdlError> {
        let at = node.start_byte();
        let inner = named(node)
            .into_iter()
            .next()
            .ok_or_else(|| self.syntax(node, "a type"))?;
        let plain = |s: Scalar| Ok(BaseType::Builtin(s));
        match inner.kind() {
            "Numeric" => {
                let words = keywords(inner);
                let modifiers = self.modifiers(inner);
                match words.first().map(String::as_str) {
                    Some("int" | "integer") => plain(Scalar::Int(32)),
                    Some("smallint") => plain(Scalar::Int(16)),
                    Some("bigint") => plain(Scalar::Int(64)),
                    Some("real") => plain(Scalar::Real),
                    Some("double") => plain(Scalar::Double),
                    Some("boolean") => plain(Scalar::Boolean),
                    Some("float") => self.float(inner),
                    Some("numeric" | "decimal" | "dec") => {
                        Ok(BaseType::Builtin(self.numeric(&modifiers?)?))
                    }
                    _ => Err(self.syntax(inner, "a numeric type")),
                }
            }
            "Character" => self.character(inner),
            "ConstDatetime" => {
                let words = keywords(inner);
                let zone = child(inner, "opt_timezone")
                    .map(|z| keywords(z))
                    .unwrap_or_default();
                if has(inner, "Iconst") {
                    let paren = children(inner)
                        .into_iter()
                        .find(|c| self.text(*c) == "(")
                        .map_or(at, |c| c.start_byte());
                    return Err(unsupported(paren, "fractional-second precision"));
                }
                let with_zone = zone.first().is_some_and(|w| w == "with");
                match words.first().map(String::as_str) {
                    Some("time") if with_zone => Err(unsupported(at, "type time with time zone")),
                    Some("time") => plain(Scalar::Time),
                    Some("timestamp") => plain(Scalar::Timestamp(with_zone)),
                    _ => Err(self.syntax(inner, "a date-time type")),
                }
            }
            "JsonType" => plain(Scalar::Json),
            "ConstInterval" => Err(unsupported(at, "type interval")),
            "Bit" => Err(unsupported(at, "type bit")),
            "GenericType" => self.generic_type(inner),
            _ => Err(unsupported(at, format!("type {}", self.text(inner)))),
        }
    }

    /// The modifiers of a type: each an integer constant, or refused.
    fn modifiers(&self, node: Node<'_>) -> Result<Vec<Modifier>, SqlDdlError> {
        let Some(mods) = child(node, "opt_type_modifiers") else {
            return Ok(Vec::new());
        };
        let Some(list) = child(mods, "expr_list") else {
            return Ok(Vec::new());
        };
        flatten(list)
            .into_iter()
            .map(|expr| {
                let text = self.text(expr).trim();
                if text.starts_with('-') {
                    return Ok((None, expr.start_byte()));
                }
                text.parse::<u32>()
                    .map(|n| (Some(n), expr.start_byte()))
                    .map_err(|_| self.syntax(expr, "an integer"))
            })
            .collect()
    }

    /// The parenthesis opening a type's modifiers, which a type with none
    /// is refused at.
    fn no_modifier(&self, node: Node<'_>) -> Result<(), SqlDdlError> {
        match child(node, "opt_type_modifiers") {
            Some(m) if m.start_byte() < m.end_byte() => Err(self.syntax(m, "no type modifier")),
            _ => Ok(()),
        }
    }

    /// `numeric` with modifiers `(p)` or `(p, s)`.
    fn numeric(&self, modifiers: &[Modifier]) -> Result<Scalar, SqlDdlError> {
        match modifiers {
            [] => Ok(Scalar::Numeric(None)),
            [(precision, p_at), rest @ ..] if rest.len() <= 1 => {
                let precision = precision
                    .filter(|p| (1..=1000).contains(p))
                    .ok_or_else(|| SqlDdlError::Syntax {
                        offset: *p_at,
                        expected: "a numeric precision from 1 to 1000",
                        found: self.token_at(*p_at),
                    })?;
                let scale = match rest.first() {
                    None => 0,
                    Some((None, s_at)) => {
                        return Err(unsupported(*s_at, "numeric with a negative scale"));
                    }
                    Some((Some(scale), s_at)) => {
                        if *scale > precision {
                            return Err(unsupported(
                                *s_at,
                                "numeric with a scale above its precision",
                            ));
                        }
                        *scale
                    }
                };
                Ok(Scalar::Numeric(Some((precision, scale))))
            }
            [.., (_, at)] => Err(SqlDdlError::Syntax {
                offset: *at,
                expected: "at most two numeric modifiers",
                found: self.token_at(*at),
            }),
        }
    }

    /// The token text at `offset`.
    fn token_at(&self, offset: usize) -> String {
        self.src[offset..]
            .split(|c: char| c.is_whitespace() || c == ',' || c == ')')
            .next()
            .unwrap_or_default()
            .to_owned()
    }

    /// `float` with an optional precision: up to 24 bits is `real`, up to
    /// 53 is `double precision`.
    fn float(&self, node: Node<'_>) -> Result<BaseType, SqlDdlError> {
        let Some(precision) = child(node, "opt_float").and_then(|f| child(f, "Iconst")) else {
            return Ok(BaseType::Builtin(Scalar::Double));
        };
        match self.text(precision).parse::<u32>() {
            Ok(1..=24) => Ok(BaseType::Builtin(Scalar::Real)),
            Ok(25..=53) => Ok(BaseType::Builtin(Scalar::Double)),
            _ => Err(self.syntax(precision, "a float precision from 1 to 53")),
        }
    }

    /// A length modifier from 1 to 10485760.
    fn length(&self, modifier: Option<Modifier>) -> Result<Option<u32>, SqlDdlError> {
        match modifier {
            None => Ok(None),
            Some((Some(n @ 1..=10_485_760), _)) => Ok(Some(n)),
            Some((_, at)) => Err(SqlDdlError::Syntax {
                offset: at,
                expected: "a length from 1 to 10485760",
                found: self.token_at(at),
            }),
        }
    }

    /// `CHARACTER [VARYING] [(n)]` and its other spellings.
    fn character(&self, node: Node<'_>) -> Result<BaseType, SqlDdlError> {
        let inner = named(node)
            .into_iter()
            .next()
            .ok_or_else(|| self.syntax(node, "a character type"))?;
        let spelling =
            child(inner, "character").ok_or_else(|| self.syntax(inner, "a character type"))?;
        let words = keywords(spelling);
        let varying = words.iter().any(|w| w == "varchar" || w == "varying")
            || child(spelling, "opt_varying").is_some_and(|v| has(v, "kw_varying"));
        let length = match child(inner, "Iconst") {
            Some(n) => self.length(Some((self.text(n).parse().ok(), n.start_byte())))?,
            None => None,
        };
        Ok(BaseType::Builtin(if varying {
            Scalar::Varchar(length)
        } else {
            Scalar::Character(length.unwrap_or(1))
        }))
    }

    /// A type named by an identifier: a built-in type the grammar does not
    /// spell with keywords, or a type the script declares.
    fn generic_type(&self, node: Node<'_>) -> Result<BaseType, SqlDdlError> {
        let at = node.start_byte();
        let parts = self.name_parts(node)?;
        let quoted = leaves(node).iter().any(|l| {
            l.kind() == "quoted_identifier"
                && child(node, "opt_type_modifiers").is_none_or(|m| l.start_byte() < m.start_byte())
        });
        let name = match parts.as_slice() {
            [name] => name.clone(),
            [schema, name] if matches!(schema.text.as_str(), "public" | "pg_catalog") => {
                name.clone()
            }
            [_, _] => return Err(unsupported(at, "a schema other than public")),
            _ => return Err(unsupported(at, "a name qualified by a database")),
        };
        if quoted && !CATALOG_TYPE_NAMES.contains(&name.text.as_str()) {
            return Ok(BaseType::Named(name));
        }
        let plain = |s: Scalar| -> Result<BaseType, SqlDdlError> {
            self.no_modifier(node)?;
            Ok(BaseType::Builtin(s))
        };
        match name.text.as_str() {
            "smallint" | "int2" => plain(Scalar::Int(16)),
            "integer" | "int" | "int4" => plain(Scalar::Int(32)),
            "bigint" | "int8" => plain(Scalar::Int(64)),
            "smallserial" | "serial2" => self.no_modifier(node).map(|()| BaseType::Serial(16)),
            "serial" | "serial4" => self.no_modifier(node).map(|()| BaseType::Serial(32)),
            "bigserial" | "serial8" => self.no_modifier(node).map(|()| BaseType::Serial(64)),
            "real" | "float4" => plain(Scalar::Real),
            "float8" => plain(Scalar::Double),
            "numeric" | "decimal" => Ok(BaseType::Builtin(self.numeric(&self.modifiers(node)?)?)),
            "text" => plain(Scalar::Text),
            "varchar" => {
                let modifiers = self.modifiers(node)?;
                Ok(BaseType::Builtin(Scalar::Varchar(
                    self.length(modifiers.first().copied())?,
                )))
            }
            "bpchar" => {
                let modifiers = self.modifiers(node)?;
                self.length(modifiers.first().copied())?
                    .map(|n| BaseType::Builtin(Scalar::Character(n)))
                    .ok_or_else(|| unsupported(at, "type bpchar without a length"))
            }
            "boolean" | "bool" => plain(Scalar::Boolean),
            "bytea" => plain(Scalar::Bytea),
            "date" => plain(Scalar::Date),
            "time" => match child(node, "opt_type_modifiers") {
                Some(m) if m.start_byte() < m.end_byte() => {
                    Err(unsupported(m.start_byte(), "fractional-second precision"))
                }
                _ => Ok(BaseType::Builtin(Scalar::Time)),
            },
            "timestamp" | "timestamptz" => match child(node, "opt_type_modifiers") {
                Some(m) if m.start_byte() < m.end_byte() => {
                    Err(unsupported(m.start_byte(), "fractional-second precision"))
                }
                _ => Ok(BaseType::Builtin(Scalar::Timestamp(
                    name.text == "timestamptz",
                ))),
            },
            "uuid" => plain(Scalar::Uuid),
            "json" => plain(Scalar::Json),
            "jsonb" => plain(Scalar::Jsonb),
            other if UNSUPPORTED_TYPES.contains(&other) => {
                Err(unsupported(at, format!("type {other}")))
            }
            _ => Ok(BaseType::Named(name)),
        }
    }
}

/// The children of `node` in order, comments left out.
fn children(node: Node<'_>) -> Vec<Node<'_>> {
    let mut cursor = node.walk();
    node.children(&mut cursor)
        .filter(|c| c.kind() != "comment")
        .collect()
}

/// The named children of `node` in order, comments left out.
fn named(node: Node<'_>) -> Vec<Node<'_>> {
    children(node).into_iter().filter(Node::is_named).collect()
}

/// The first child of `node` of kind `kind`.
fn child<'t>(node: Node<'t>, kind: &str) -> Option<Node<'t>> {
    children(node).into_iter().find(|c| c.kind() == kind)
}

/// Whether `node` has a child of kind `kind`.
fn has(node: Node<'_>, kind: &str) -> bool {
    child(node, kind).is_some()
}

/// The keyword children of `node`, lowercase, in order.
fn keywords(node: Node<'_>) -> Vec<String> {
    children(node)
        .into_iter()
        .filter_map(|c| c.kind().strip_prefix("kw_").map(str::to_owned))
        .collect()
}

/// The elements of a left-recursive list `node` of kind `list`: its
/// named children other than the nested lists, in order.
fn flatten(node: Node<'_>) -> Vec<Node<'_>> {
    let list = node.kind();
    let mut out = Vec::new();
    for c in named(node) {
        if c.kind() == list {
            out.extend(flatten(c));
        } else {
            out.push(c);
        }
    }
    out
}

/// The leaves under `node`, in order.
fn leaves(node: Node<'_>) -> Vec<Node<'_>> {
    let children = children(node);
    if children.is_empty() {
        return vec![node];
    }
    children.into_iter().flat_map(|c| leaves(c)).collect()
}

/// The leading keywords of a statement, lowercase.
fn leading_words(node: Node<'_>) -> Vec<String> {
    leaves(node)
        .into_iter()
        .take(6)
        .map_while(|leaf| leaf.kind().strip_prefix("kw_").map(str::to_owned))
        .collect()
}

/// Refuse the options of a key the reader does not carry: `NULLS NOT
/// DISTINCT`, `INCLUDE`, `WITH (...)`, `USING INDEX ...` and `WITHOUT
/// OVERLAPS`.
fn key_options(node: Node<'_>) -> Result<(), SqlDdlError> {
    for c in children(node) {
        if c.start_byte() == c.end_byte() {
            continue;
        }
        match c.kind() {
            "opt_unique_null_treatment" => {
                if has(c, "kw_not") {
                    return Err(unsupported(c.start_byte(), "UNIQUE NULLS NOT DISTINCT"));
                }
            }
            "opt_c_include" | "opt_definition" | "OptConsTableSpace" | "ExistingIndex" => {
                return Err(unsupported(c.start_byte(), "index parameters"));
            }
            "opt_without_overlaps" | "kw_without" => {
                return Err(unsupported(c.start_byte(), "WITHOUT OVERLAPS"));
            }
            _ => {}
        }
    }
    Ok(())
}

/// A `NOT DEFERRABLE` or `INITIALLY IMMEDIATE` clause; the others are
/// refused.
fn deferral(node: Node<'_>) -> Result<Deferral, SqlDdlError> {
    let words = keywords(node);
    let words: Vec<&str> = words.iter().map(String::as_str).collect();
    match words.as_slice() {
        ["not", "deferrable"] => Ok(Deferral::NotDeferrable),
        ["initially", "immediate"] => Ok(Deferral::InitiallyImmediate),
        ["deferrable"] | ["initially", "deferred"] => {
            Err(unsupported(node.start_byte(), "a deferrable constraint"))
        }
        ["not", "valid"] => Err(unsupported(node.start_byte(), "NOT VALID")),
        ["no", "inherit"] => Err(unsupported(node.start_byte(), "NO INHERIT")),
        ["enforced"] => Ok(Deferral::Enforced),
        _ => Err(unsupported(
            node.start_byte(),
            format!("constraint attribute {}", words.join(" ").to_uppercase()),
        )),
    }
}

const fn not_read(statement: &'static str, offset: usize) -> Statement {
    Statement::NotRead(StatementNotRead { statement, offset })
}

/// Decode the backslash escape at `j` of an `E` string's body (Sec
/// 4.1.2.2, table 4.1) into `value`; the index after it.
fn escape(bytes: &[u8], j: usize, value: &mut Vec<u8>) -> Option<usize> {
    let c = *bytes.get(j + 1)?;
    let radix_run = |from: usize, radix: u32, max: usize| {
        let mut n = 0u32;
        let mut k = from;
        while k < from + max && bytes.get(k).is_some_and(|b| char::from(*b).is_digit(radix)) {
            n = n * radix + char::from(bytes[k]).to_digit(radix)?;
            k += 1;
        }
        (k > from).then_some((n, k))
    };
    let simple = |b: u8, value: &mut Vec<u8>| {
        value.push(b);
        Some(j + 2)
    };
    match c {
        b'b' => simple(0x08, value),
        b'f' => simple(0x0c, value),
        b'n' => simple(b'\n', value),
        b'r' => simple(b'\r', value),
        b't' => simple(b'\t', value),
        b'0'..=b'7' => {
            let (n, k) = radix_run(j + 1, 8, 3)?;
            value.push(u8::try_from(n).ok()?);
            Some(k)
        }
        b'x' => {
            let (n, k) = radix_run(j + 2, 16, 2)?;
            value.push(u8::try_from(n).ok()?);
            Some(k)
        }
        b'u' | b'U' => {
            let width = if c == b'u' { 4 } else { 8 };
            let (n, k) = radix_run(j + 2, 16, width)?;
            if k - (j + 2) != width {
                return None;
            }
            let ch = char::from_u32(n)?;
            value.extend_from_slice(ch.encode_utf8(&mut [0; 4]).as_bytes());
            Some(k)
        }
        _ => {
            // Any other character is taken literally, a multi-byte one whole.
            let ch = std::str::from_utf8(&bytes[j + 1..]).ok()?.chars().next()?;
            let width = ch.len_utf8();
            value.extend_from_slice(&bytes[j + 1..j + 1 + width]);
            Some(j + 1 + width)
        }
    }
}
