//! Reading the rows of `INSERT ... VALUES` statements into a table
//! instance, and writing an instance back as a canonical script.
//!
//! A script is read in script order, as `PostgreSQL` would run it. Its DDL
//! is read by the DDL reader into the schema; each `INSERT INTO table
//! [(columns)] VALUES (...), ...` adds rows to a table the statements
//! before it declared, each value a constant read under its column's type
//! (see [`values`](super::values)). A column the statement leaves out is
//! `NULL` when it has no `DEFAULT`; a column whose `DEFAULT` would be
//! evaluated is refused. `SET standard_conforming_strings` changes how the
//! string constants after it are read. A statement that would change the
//! rows already inserted (dropping their table, or adding a column to it)
//! is refused, as is any other data statement (`UPDATE`, `DELETE`, `COPY`).
//!
//! Rows are numbered from zero by table name and then in the order the
//! script inserts them, so the instance does not depend on how the script
//! interleaves its tables. Keys, foreign keys and `NOT NULL` are not
//! checked here: the shared validation and reference resolution read them
//! from the schema.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt::Write as _;

use panproto_gat::Name;
use panproto_inst::{FInstance, Row, RowId, Value};
use panproto_protocols::database::postgresql::{
    AlterAction, OtherStatements, SqlDdlError, Statement, StatementNotRead, emit_ddl_parts,
    quote_identifier, read_statements,
};
use panproto_schema::Schema;
use tree_sitter::{Node, Tree};

use super::values::{ColumnType, Element, ValueRefusal, decode, decode_numeral, encode};
use super::{Walk, child, children, escape, first_error, flatten, leaves, named, parse};

/// What a script of DDL and `INSERT` statements reads as.
#[derive(Debug)]
pub struct DumpReading {
    /// The tables the script declares, or the schema it was read against.
    pub schema: Schema,
    /// The rows the script inserts.
    pub instance: FInstance,
    /// The statements read past, in script order, when they are listed.
    pub not_read: Vec<StatementNotRead>,
}

/// Why a script's data was refused, or an instance could not be written.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum SqlDataError {
    /// The script is refused as the DDL reader refuses it, or an `INSERT`
    /// is not one the reader reads (its byte offset is the refusal's).
    #[error(transparent)]
    Script(#[from] SqlDdlError),
    /// A constant its column's type does not read exactly.
    #[error("SQL value at byte {offset} for column {column:?} of table {table:?}: {reason}")]
    Value {
        /// The byte offset of the constant.
        offset: usize,
        /// The table.
        table: String,
        /// The column.
        column: String,
        /// Why it is refused.
        reason: ValueRefusal,
    },
    /// An instance the script form cannot write.
    #[error("cannot write table {table:?}{}: {reason}", column.as_ref().map(|c| format!(" column {c:?}")).unwrap_or_default())]
    Unwritable {
        /// The table.
        table: String,
        /// The column, when the refusal is about one.
        column: Option<String>,
        /// Why it cannot be written.
        reason: WriteRefusal,
    },
    /// A schema `emit_ddl` cannot write, with its reason.
    #[error("the schema has no DDL form: {0}")]
    NoDdlForm(String),
}

/// Why an instance cannot be written against its schema.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum WriteRefusal {
    /// The schema declares no such table.
    #[error("the schema declares no such table")]
    UnknownTable,
    /// The table declares no such column.
    #[error("the table declares no such column")]
    UnknownColumn,
    /// A row has no cell for the column (an absent cell is not `NULL`).
    #[error("a row has no cell for the column")]
    MissingCell,
    /// The instance holds foreign-key pairs, which a script states only
    /// through column values.
    #[error("the instance holds foreign-key pairs")]
    ForeignKeyPairs,
    /// A labelled null, which `NULL` does not carry.
    #[error("a labelled null")]
    LabelledNull,
    /// A value its column's type does not write exactly.
    #[error("{0}")]
    Value(ValueRefusal),
}

/// Read a script of `PostgreSQL` 17 DDL and `INSERT` statements into its
/// schema and rows.
///
/// # Errors
///
/// The first refusal of the DDL in script order, a statement `others`
/// refuses, an `INSERT` the reader does not read, or else the first value
/// refused, in script order.
pub fn read_dump(src: &str, others: OtherStatements) -> Result<DumpReading, SqlDataError> {
    let tree = parse(src)?;
    let script = Script::read(src, &tree, false);
    let reading = read_statements(script.statements, others)?;
    let instance = rows(src, &reading.schema, &script.inserts)?;
    Ok(DumpReading {
        schema: reading.schema,
        instance,
        not_read: reading.not_read,
    })
}

/// Read the `INSERT` statements of a script that declares no schema (a
/// data-only dump) against `schema`.
///
/// # Errors
///
/// A statement that declares or changes a table, type or key, a statement
/// `others` refuses, an `INSERT` the reader does not read, or a value
/// refused, the first in script order.
pub fn read_data(
    src: &str,
    schema: &Schema,
    others: OtherStatements,
) -> Result<DumpReading, SqlDataError> {
    let tree = parse(src)?;
    let script = Script::read(src, &tree, true);
    let reading = read_statements(script.statements, others)?;
    let instance = rows(src, schema, &script.inserts)?;
    Ok(DumpReading {
        schema: schema.clone(),
        instance,
        not_read: reading.not_read,
    })
}

/// One `INSERT` statement to read once the schema is known.
struct PendingInsert<'t> {
    node: Node<'t>,
    /// Whether `standard_conforming_strings` is on at the statement.
    standard_strings: bool,
}

/// The statements of a script in order, with its `INSERT` statements set
/// aside.
struct Script<'t> {
    statements: Vec<Result<Statement, SqlDdlError>>,
    inserts: Vec<PendingInsert<'t>>,
}

impl<'t> Script<'t> {
    fn read(src: &str, tree: &'t Tree, data_only: bool) -> Self {
        let w = Walk { src };
        let root = tree.root_node();
        let error = first_error(root).map(|node| w.syntax_error(node));
        let error_at = error.as_ref().map_or(usize::MAX, SqlDdlError::offset);
        let mut script = Script {
            statements: Vec::new(),
            inserts: Vec::new(),
        };
        let mut standard_strings = true;
        let mut tables: BTreeSet<String> = BTreeSet::new();
        let mut filled: BTreeSet<String> = BTreeSet::new();
        for top in children(root) {
            if top.kind() != "toplevel_stmt" {
                continue;
            }
            if top.end_byte() > error_at || top.has_error() {
                break;
            }
            let Some(body) = child(top, "stmt").and_then(|s| children(s).into_iter().next()) else {
                continue;
            };
            let statement = match body.kind() {
                "InsertStmt" => {
                    match w.insert_table(body) {
                        Ok(table) if data_only || tables.contains(&table.text) => {
                            filled.insert(table.text);
                            script.inserts.push(PendingInsert {
                                node: body,
                                standard_strings,
                            });
                        }
                        Ok(table) => {
                            script.statements.push(Err(SqlDdlError::UnknownTable {
                                offset: table.offset,
                                name: table.text,
                            }));
                            return script;
                        }
                        Err(e) => {
                            script.statements.push(Err(e));
                            return script;
                        }
                    }
                    continue;
                }
                "VariableSetStmt" => w
                    .session_setting(body, &mut standard_strings)
                    .and_then(|()| w.statement(body)),
                _ => w
                    .statement(body)
                    .and_then(|s| track(s, data_only, &mut tables, &filled)),
            };
            let refused = statement.is_err();
            script.statements.push(statement);
            if refused {
                return script;
            }
        }
        if let Some(error) = error {
            script.statements.push(Err(error));
        }
        script
    }
}

/// Follow which tables `statement` declares and drops, refusing a change
/// to a table that holds rows, and any schema statement in a data-only
/// script.
fn track(
    statement: Statement,
    data_only: bool,
    tables: &mut BTreeSet<String>,
    filled: &BTreeSet<String>,
) -> Result<Statement, SqlDdlError> {
    let unsupported = |offset, feature: &str| SqlDdlError::Unsupported {
        offset,
        feature: feature.to_owned(),
    };
    match &statement {
        Statement::NotRead(_) => return Ok(statement),
        _ if data_only => {
            let offset = statement_offset(&statement);
            return Err(unsupported(
                offset,
                "a schema statement in a data-only script",
            ));
        }
        Statement::CreateTable { name, .. } => {
            tables.insert(name.text.clone());
        }
        Statement::DropTable { names, .. } => {
            for name in names {
                if filled.contains(&name.text) {
                    return Err(unsupported(
                        name.offset,
                        "DROP TABLE of a table holding inserted rows",
                    ));
                }
                tables.remove(&name.text);
            }
        }
        Statement::AlterTable { table, actions, .. } => {
            let adds_column = actions
                .iter()
                .any(|a| matches!(a, AlterAction::AddColumn { .. }));
            if adds_column && filled.contains(&table.text) {
                return Err(unsupported(
                    table.offset,
                    "ALTER TABLE ADD COLUMN on a table holding inserted rows",
                ));
            }
        }
        _ => {}
    }
    Ok(statement)
}

/// The byte offset of the name a statement declares.
fn statement_offset(statement: &Statement) -> usize {
    match statement {
        Statement::CreateTable { name, .. }
        | Statement::CreateDomain { name, .. }
        | Statement::CreateEnum { name, .. } => name.offset,
        Statement::CreateUniqueIndex { table, .. } | Statement::AlterTable { table, .. } => {
            table.offset
        }
        Statement::DropTable { names, .. } => names.first().map_or(0, |name| name.offset),
        Statement::NotRead(n) => n.offset,
    }
}

/// A constant of a `VALUES` row, before its column's type reads it.
enum Constant<'t> {
    Null,
    Bool(bool),
    /// An integer or float literal, negated when `negative`.
    Numeral {
        token: Node<'t>,
        negative: bool,
    },
    /// A string constant's decoded text.
    Text(String),
}

/// One `INSERT` statement: its table, its target columns as written, and
/// its rows of constants.
struct InsertRows<'t> {
    table: super::Name,
    columns: Option<Vec<super::Name>>,
    rows: Vec<Vec<(Node<'t>, Constant<'t>)>>,
}

impl Walk<'_> {
    /// The table an `INSERT` names.
    fn insert_table(&self, node: Node<'_>) -> Result<super::Name, SqlDdlError> {
        let target =
            child(node, "insert_target").ok_or_else(|| self.syntax(node, "an INSERT target"))?;
        let name =
            child(target, "qualified_name").ok_or_else(|| self.syntax(target, "a table name"))?;
        self.object_name(name)
    }

    /// A `SET` the data reader follows: `standard_conforming_strings`, and
    /// `client_encoding`, which must name UTF-8.
    fn session_setting(&self, node: Node<'_>, standard: &mut bool) -> Result<(), SqlDdlError> {
        let Some(generic) = child(node, "set_rest")
            .and_then(|r| child(r, "set_rest_more"))
            .and_then(|r| child(r, "generic_set"))
        else {
            return Ok(());
        };
        let Some(var) = child(generic, "var_name") else {
            return Ok(());
        };
        let variable = self.text(var).to_ascii_lowercase();
        let values: Vec<Node<'_>> = child(generic, "var_list")
            .map(|l| flatten(l))
            .unwrap_or_default();
        let value = match values.as_slice() {
            [one] => {
                let leaf = leaves(*one);
                match leaf.as_slice() {
                    [l] if l.kind().ends_with("string_literal") => self.string_value(*l, true),
                    [l] => Some(self.text(*l).to_ascii_lowercase()),
                    _ => None,
                }
            }
            _ => None,
        };
        let at = generic.start_byte();
        let refuse = |feature: String| SqlDdlError::Unsupported {
            offset: at,
            feature,
        };
        match variable.as_str() {
            "standard_conforming_strings" => {
                match value.as_deref().map(str::to_ascii_lowercase).as_deref() {
                    Some("on" | "true" | "1" | "yes") => *standard = true,
                    Some("off" | "false" | "0" | "no") => *standard = false,
                    _ => {
                        return Err(refuse(
                            "a standard_conforming_strings value other than on or off".to_owned(),
                        ));
                    }
                }
            }
            "client_encoding" => {
                let utf8 = value.as_deref().is_some_and(|v| {
                    matches!(
                        v.to_ascii_lowercase().replace(['-', '_'], "").as_str(),
                        "utf8" | "unicode"
                    )
                });
                if !utf8 {
                    return Err(refuse("a client_encoding other than UTF8".to_owned()));
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// The table, target columns and rows of an `INSERT`.
    fn insert_rows<'t>(
        &self,
        node: Node<'t>,
        standard: bool,
    ) -> Result<InsertRows<'t>, SqlDdlError> {
        let unsupported = |at: Node<'_>, feature: &str| SqlDdlError::Unsupported {
            offset: at.start_byte(),
            feature: feature.to_owned(),
        };
        for c in named(node) {
            if c.start_byte() == c.end_byte() {
                continue;
            }
            match c.kind() {
                "opt_with_clause" | "with_clause" => return Err(unsupported(c, "WITH")),
                "opt_on_conflict" => return Err(unsupported(c, "ON CONFLICT")),
                "returning_clause" => return Err(unsupported(c, "RETURNING")),
                _ => {}
            }
        }
        let table = self.insert_table(node)?;
        let rest = child(node, "insert_rest").ok_or_else(|| self.syntax(node, "VALUES"))?;
        if let Some(kw) = child(rest, "kw_overriding") {
            return Err(unsupported(kw, "OVERRIDING ... VALUE"));
        }
        if let Some(kw) = child(rest, "kw_default") {
            return Err(unsupported(kw, "DEFAULT VALUES"));
        }
        let columns = match child(rest, "insert_column_list") {
            Some(list) => Some(
                flatten(list)
                    .into_iter()
                    .map(|item| match named(item).as_slice() {
                        [col] => self.identifier(*col),
                        [_, more, ..] => Err(unsupported(
                            *more,
                            "a subscript or field of a target column",
                        )),
                        [] => Err(self.syntax(item, "a column name")),
                    })
                    .collect::<Result<Vec<_>, _>>()?,
            ),
            None => None,
        };
        let select = child(rest, "SelectStmt").ok_or_else(|| self.syntax(rest, "VALUES"))?;
        let values = child(select, "select_no_parens")
            .filter(|s| named(*s).len() == 1)
            .and_then(|s| child(s, "simple_select"))
            .filter(|s| named(*s).len() == 1)
            .and_then(|s| child(s, "values_clause"))
            .ok_or_else(|| unsupported(select, "INSERT ... SELECT"))?;
        let mut rows = Vec::new();
        for list in flatten(values)
            .into_iter()
            .filter(|n| n.kind() == "expr_list")
        {
            let mut row = Vec::new();
            for expr in flatten(list) {
                row.push((expr, self.constant(expr, standard)?));
            }
            rows.push(row);
        }
        Ok(InsertRows {
            table,
            columns,
            rows,
        })
    }

    /// The constant an expression of a `VALUES` row is.
    fn constant<'t>(&self, node: Node<'t>, standard: bool) -> Result<Constant<'t>, SqlDdlError> {
        let not_constant = |at: Node<'_>, feature: &str| SqlDdlError::Unsupported {
            offset: at.start_byte(),
            feature: feature.to_owned(),
        };
        let mut current = node;
        let mut sign: Option<bool> = None;
        loop {
            let cs = children(current);
            match current.kind() {
                "a_expr" | "a_expr_prec" | "c_expr" => match cs.as_slice() {
                    [only] if only.kind() == "kw_default" => {
                        return Err(not_constant(*only, "DEFAULT in VALUES"));
                    }
                    [only] if only.is_named() => current = *only,
                    [open, inner, close]
                        if open.kind() == "(" && close.kind() == ")" && sign.is_none() =>
                    {
                        current = *inner;
                    }
                    [op, inner] if matches!(op.kind(), "-" | "+") && sign.is_none() => {
                        sign = Some(op.kind() == "-");
                        current = *inner;
                    }
                    _ if cs.iter().any(|c| c.kind() == "::") => {
                        return Err(not_constant(current, "a type cast"));
                    }
                    _ => return Err(not_constant(current, "an expression other than a constant")),
                },
                "AexprConst" => {
                    let first = cs
                        .first()
                        .ok_or_else(|| self.syntax(current, "a constant"))?;
                    let constant = match first.kind() {
                        "Iconst" | "float_literal" => {
                            let token = leaves(*first).into_iter().next().unwrap_or(*first);
                            return Ok(Constant::Numeral {
                                token,
                                negative: sign == Some(true),
                            });
                        }
                        "Sconst" => {
                            let leaf = leaves(*first).into_iter().next().unwrap_or(*first);
                            Constant::Text(self.string_constant(leaf, standard)?)
                        }
                        "kw_true" => Constant::Bool(true),
                        "kw_false" => Constant::Bool(false),
                        "kw_null" => Constant::Null,
                        "bit_string_literal" | "hex_string_literal" => {
                            return Err(not_constant(*first, "a bit-string constant"));
                        }
                        _ => return Err(not_constant(current, "a typed constant")),
                    };
                    if sign.is_some() {
                        return Err(not_constant(node, "an expression other than a constant"));
                    }
                    return Ok(constant);
                }
                _ => return Err(not_constant(current, "an expression other than a constant")),
            }
        }
    }

    /// The text of a string constant token: a standard string (`'...'`),
    /// an escape string (`E'...'`) or a dollar-quoted string, with its
    /// continuation segments (Sec 4.1.2.1 to 4.1.2.4).
    fn string_constant(&self, leaf: Node<'_>, standard: bool) -> Result<String, SqlDdlError> {
        let at = leaf.start_byte();
        let raw = self.text(leaf);
        if !standard && leaf.kind() == "string_literal" && backslash_before_quote(raw) {
            return Err(SqlDdlError::Unsupported {
                offset: at,
                feature: "a backslash before a quote under standard_conforming_strings off"
                    .to_owned(),
            });
        }
        self.string_value(leaf, standard)
            .ok_or_else(|| SqlDdlError::Syntax {
                offset: at,
                expected: "a string constant whose escapes decode to UTF-8 text without NUL",
                found: raw.to_owned(),
            })
    }

    /// The decoded text of a string constant leaf, `None` when it does not
    /// decode to UTF-8 text without NUL.
    fn string_value(&self, leaf: Node<'_>, standard: bool) -> Option<String> {
        let raw = self.text(leaf);
        let value = match leaf.kind() {
            "dollar_quoted_string" => {
                let tag_end = raw[1..].find('$')? + 2;
                let tag = &raw[..tag_end];
                raw.strip_prefix(tag)?
                    .strip_suffix(tag)?
                    .as_bytes()
                    .to_vec()
            }
            "escape_string_literal" => decode_segments(&raw[1..], true)?,
            "string_literal" => decode_segments(raw, !standard)?,
            _ => return None,
        };
        if value.contains(&0) {
            return None;
        }
        String::from_utf8(value).ok()
    }
}

/// Whether a backslash run of odd length stands right before a quote in
/// the token (where `PostgreSQL` with standard strings off would read an
/// escaped quote, and the grammar does not).
fn backslash_before_quote(raw: &str) -> bool {
    let bytes = raw.as_bytes();
    let mut run = 0;
    for b in bytes {
        match b {
            b'\\' => run += 1,
            b'\'' if run % 2 == 1 => return true,
            _ => run = 0,
        }
    }
    false
}

/// The bytes of the quoted segments of a string token, `''` read as one
/// quote and, when `escapes`, backslash escapes decoded.
fn decode_segments(raw: &str, escapes: bool) -> Option<Vec<u8>> {
    let bytes = raw.as_bytes();
    let mut value = Vec::new();
    let mut i = 0;
    loop {
        if bytes.get(i) != Some(&b'\'') {
            return None;
        }
        i += 1;
        loop {
            match *bytes.get(i)? {
                b'\'' if bytes.get(i + 1) == Some(&b'\'') => {
                    value.push(b'\'');
                    i += 2;
                }
                b'\'' => {
                    i += 1;
                    break;
                }
                b'\\' if escapes => i = escape(bytes, i, &mut value)?,
                b => {
                    value.push(b);
                    i += 1;
                }
            }
        }
        // White space and comments, with a newline, separate a
        // continuation segment.
        while i < bytes.len() && bytes[i] != b'\'' {
            if bytes[i] == b'-' && bytes.get(i + 1) == Some(&b'-') {
                while i < bytes.len() && !matches!(bytes[i], b'\n' | b'\r') {
                    i += 1;
                }
            } else {
                i += 1;
            }
        }
        if i >= bytes.len() {
            return Some(value);
        }
    }
}

/// A column of a table: its name, its vertex and its type, in position
/// order.
pub(crate) struct Column {
    pub name: String,
    pub ty: ColumnType,
    pub has_default: bool,
}

/// The columns of `table` in `schema`, in position order.
pub(crate) fn columns(schema: &Schema, table: &str) -> Option<Vec<Column>> {
    if schema.vertices.get(table).is_none_or(|v| v.kind != "table") {
        return None;
    }
    let mut edges: Vec<(u32, &panproto_schema::Edge)> = schema
        .outgoing_edges(table)
        .iter()
        .filter(|e| e.kind == "prop")
        .map(|e| (schema.orderings.get(e).copied().unwrap_or(u32::MAX), e))
        .collect();
    edges.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.name.cmp(&b.1.name)));
    edges
        .into_iter()
        .map(|(_, e)| {
            let name = e.name.as_deref()?.to_owned();
            let ty = ColumnType::of(schema, &e.tgt)?;
            let has_default = schema
                .constraints
                .get(&e.tgt)
                .is_some_and(|cs| cs.iter().any(|c| c.sort == "DEFAULT"));
            Some(Column {
                name,
                ty,
                has_default,
            })
        })
        .collect()
}

/// Number the rows of each table, tables in name order and rows in the
/// order given, from zero.
pub(crate) fn number_rows(by_table: BTreeMap<String, Vec<Row>>) -> FInstance {
    let mut instance = FInstance::new();
    let mut next = 0u32;
    for (table, table_rows) in by_table {
        for row in table_rows {
            // Ids are fresh and ascending, so insertion cannot collide.
            if instance
                .insert_row(Name::from(table.as_str()), RowId(next), row)
                .is_ok()
            {
                next += 1;
            }
        }
    }
    instance
}

/// Read the rows of the pending inserts against `schema`.
fn rows(
    src: &str,
    schema: &Schema,
    inserts: &[PendingInsert<'_>],
) -> Result<FInstance, SqlDataError> {
    let w = Walk { src };
    let mut by_table: BTreeMap<String, Vec<Row>> = BTreeMap::new();
    for insert in inserts {
        let statement = w.insert_rows(insert.node, insert.standard_strings)?;
        let table = &statement.table;
        let table_columns =
            columns(schema, &table.text).ok_or_else(|| SqlDdlError::UnknownTable {
                offset: table.offset,
                name: table.text.clone(),
            })?;
        let targets: Vec<usize> = match &statement.columns {
            Some(names) => {
                let mut seen = BTreeSet::new();
                names
                    .iter()
                    .map(|n| {
                        if !seen.insert(n.text.clone()) {
                            return Err(SqlDdlError::Duplicate {
                                offset: n.offset,
                                what: "column",
                                name: n.text.clone(),
                            });
                        }
                        table_columns
                            .iter()
                            .position(|c| c.name == n.text)
                            .ok_or_else(|| SqlDdlError::UnknownColumn {
                                offset: n.offset,
                                table: table.text.clone(),
                                column: n.text.clone(),
                            })
                    })
                    .collect::<Result<_, _>>()?
            }
            None => (0..table_columns.len()).collect(),
        };
        for row_constants in statement.rows {
            let given = row_constants.len();
            let first = row_constants.first().map_or(insert.node, |(n, _)| *n);
            if given > targets.len() {
                return Err(SqlDdlError::Syntax {
                    offset: first.start_byte(),
                    expected: "at most one value per target column",
                    found: format!("{given} values"),
                }
                .into());
            }
            if statement.columns.is_some() && given < targets.len() {
                return Err(SqlDdlError::Syntax {
                    offset: first.start_byte(),
                    expected: "one value per target column",
                    found: format!("{given} values"),
                }
                .into());
            }
            let mut row: Row = HashMap::new();
            for ((node, constant), &index) in row_constants.into_iter().zip(&targets) {
                let column = &table_columns[index];
                let value = read_constant(&w, &constant, &column.ty).map_err(|reason| {
                    SqlDataError::Value {
                        offset: node.start_byte(),
                        table: table.text.clone(),
                        column: column.name.clone(),
                        reason,
                    }
                })?;
                row.insert(column.name.clone(), value);
            }
            for column in &table_columns {
                if row.contains_key(&column.name) {
                    continue;
                }
                if column.has_default {
                    return Err(SqlDdlError::Unsupported {
                        offset: first.start_byte(),
                        feature: format!("column {} left to its DEFAULT", column.name),
                    }
                    .into());
                }
                row.insert(column.name.clone(), Value::Null);
            }
            by_table.entry(table.text.clone()).or_default().push(row);
        }
    }
    Ok(number_rows(by_table))
}

fn read_constant(
    w: &Walk<'_>,
    constant: &Constant<'_>,
    ty: &ColumnType,
) -> Result<Value, ValueRefusal> {
    match constant {
        Constant::Null => Ok(Value::Null),
        Constant::Bool(b) if ty.dims == 0 && ty.element == Element::Bool => Ok(Value::Bool(*b)),
        Constant::Bool(_) => Err(ValueRefusal::Unsupported(
            "a boolean constant for a column of another type",
        )),
        Constant::Numeral { token, negative } => decode_numeral(w.text(*token), *negative, ty),
        Constant::Text(text) => decode(text, ty),
    }
}

/// The rows of `instance` grouped by table in name order, each table's
/// rows in id order, checked against `schema`.
pub(crate) fn instance_rows<'i>(
    schema: &Schema,
    instance: &'i FInstance,
) -> Result<Vec<(String, Vec<Column>, Vec<&'i Row>)>, SqlDataError> {
    let unwritable = |table: &str, column: Option<&str>, reason| SqlDataError::Unwritable {
        table: table.to_owned(),
        column: column.map(str::to_owned),
        reason,
    };
    if instance.pair_count() > 0 {
        return Err(unwritable("", None, WriteRefusal::ForeignKeyPairs));
    }
    let mut tables: Vec<&Name> = instance.table_names();
    tables.sort();
    let mut out = Vec::new();
    for table in tables {
        let table_columns = columns(schema, table)
            .ok_or_else(|| unwritable(table, None, WriteRefusal::UnknownTable))?;
        let table_rows: Vec<&Row> = instance.rows(table).into_iter().map(|(_, r)| r).collect();
        for row in &table_rows {
            let mut cells: Vec<&String> = row.keys().collect();
            cells.sort();
            if let Some(extra) = cells
                .iter()
                .find(|c| !table_columns.iter().any(|col| &col.name == **c))
            {
                return Err(unwritable(table, Some(extra), WriteRefusal::UnknownColumn));
            }
            for column in &table_columns {
                match row.get(&column.name) {
                    None => {
                        return Err(unwritable(
                            table,
                            Some(&column.name),
                            WriteRefusal::MissingCell,
                        ));
                    }
                    Some(value) if contains_labelled_null(value) => {
                        return Err(unwritable(
                            table,
                            Some(&column.name),
                            WriteRefusal::LabelledNull,
                        ));
                    }
                    Some(_) => {}
                }
            }
        }
        if !table_rows.is_empty() {
            out.push((table.to_string(), table_columns, table_rows));
        }
    }
    Ok(out)
}

fn contains_labelled_null(value: &Value) -> bool {
    match value {
        Value::LabeledNull(_) => true,
        Value::List(items) => items.iter().any(contains_labelled_null),
        _ => false,
    }
}

/// The SQL constant that reads back as `value` in a column of type `ty`.
fn literal(value: &Value, ty: &ColumnType) -> Result<String, ValueRefusal> {
    if *value == Value::Null {
        return Ok("NULL".to_owned());
    }
    let text = encode(value, ty)?;
    if ty.dims == 0 {
        match (&ty.element, value) {
            (Element::Bool, Value::Bool(b)) => {
                return Ok(if *b { "TRUE" } else { "FALSE" }.to_owned());
            }
            (Element::Int(_), _) => return Ok(text),
            (Element::Numeric(_), Value::Decimal(d)) if d.is_finite() => return Ok(text),
            _ => {}
        }
    }
    Ok(format!("'{}'", text.replace('\'', "''")))
}

/// Write the rows of `instance` as canonical `INSERT` statements.
///
/// One statement per table holding rows, tables in name order, its target
/// columns in position order and its rows in id order: `NULL`, `TRUE` and
/// `FALSE`, an integer or a finite `numeric` unquoted, and every other
/// value as a standard string constant holding its output text.
///
/// # Errors
///
/// [`SqlDataError::Unwritable`] for a table or column the schema does not
/// declare, a row without a cell for a column, foreign-key pairs, a
/// labelled null, or a value its column's type does not write exactly.
pub fn write_data(schema: &Schema, instance: &FInstance) -> Result<String, SqlDataError> {
    let mut out = String::new();
    for (table, table_columns, table_rows) in instance_rows(schema, instance)? {
        let names: Vec<String> = table_columns
            .iter()
            .map(|c| quote_identifier(&c.name))
            .collect();
        let _ = write!(
            out,
            "INSERT INTO {} ({}) VALUES\n",
            quote_identifier(&table),
            names.join(", ")
        );
        for (i, row) in table_rows.iter().enumerate() {
            let mut values = Vec::with_capacity(table_columns.len());
            for column in &table_columns {
                let value = row.get(&column.name).unwrap_or(&Value::Null);
                values.push(literal(value, &column.ty).map_err(|reason| {
                    SqlDataError::Unwritable {
                        table: table.clone(),
                        column: Some(column.name.clone()),
                        reason: WriteRefusal::Value(reason),
                    }
                })?);
            }
            let end = if i + 1 == table_rows.len() { ";" } else { "," };
            let _ = writeln!(out, "    ({}){end}", values.join(", "));
        }
    }
    Ok(out)
}

/// Write a schema and its rows as one canonical script: the schema's types
/// and tables as [`emit_ddl`](panproto_protocols::database::postgresql::emit_ddl)
/// writes them, then the rows as [`write_data`] writes them, then the
/// foreign keys, so the script loads into `PostgreSQL` whatever order the
/// tables reference each other in. Reading the script gives the schema
/// and the instance again, its rows numbered as the reader numbers them.
///
/// # Errors
///
/// [`SqlDataError::NoDdlForm`] when the schema has no DDL form, or
/// [`SqlDataError::Unwritable`] as [`write_data`] refuses.
pub fn write_dump(schema: &Schema, instance: &FInstance) -> Result<String, SqlDataError> {
    let (tables, foreign_keys) =
        emit_ddl_parts(schema).map_err(|e| SqlDataError::NoDdlForm(e.to_string()))?;
    let data = write_data(schema, instance)?;
    let mut out = tables;
    if !data.is_empty() {
        out.push_str(&data);
        out.push('\n');
    }
    out.push_str(&foreign_keys);
    Ok(out)
}

/// The table codec of the `postgresql` protocol: a script of DDL and
/// `INSERT` statements, read and written as [`read_dump`] and
/// [`write_dump`] do.
///
/// A script that declares tables must declare exactly the schema it is
/// read against (their canonical DDL texts are equal); a script that
/// declares none is a data-only script, read against the schema as
/// [`read_data`] reads it. Statements that declare nothing a table schema
/// holds are read past.
#[derive(Clone, Copy, Debug, Default)]
pub struct PostgresqlCodec;

const PROTOCOL: &str = "postgresql";

impl crate::traits::TableInstanceParser for PostgresqlCodec {
    fn protocol_name(&self) -> &str {
        PROTOCOL
    }

    fn parse_table(
        &self,
        schema: &Schema,
        input: &[u8],
    ) -> Result<FInstance, crate::error::ParseInstanceError> {
        use crate::error::ParseInstanceError;
        let parse_error = |message: String| ParseInstanceError::Parse {
            protocol: PROTOCOL.to_owned(),
            message,
        };
        let src = std::str::from_utf8(input)
            .map_err(|e| parse_error(format!("not UTF-8 at byte {}", e.valid_up_to())))?;
        let refused = |error: SqlDataError| match error {
            SqlDataError::Script(SqlDdlError::Unsupported { offset, feature }) => {
                ParseInstanceError::UnsupportedFeature {
                    protocol: PROTOCOL.to_owned(),
                    feature,
                    byte_offset: u64::try_from(offset).ok(),
                }
            }
            other => parse_error(other.to_string()),
        };
        let reading = read_dump(src, OtherStatements::List).map_err(refused)?;
        let declares_tables = reading.schema.vertices.values().any(|v| v.kind == "table");
        if !declares_tables {
            return Ok(read_data(src, schema, OtherStatements::List)
                .map_err(refused)?
                .instance);
        }
        let ddl = |s: &Schema| panproto_protocols::database::postgresql::emit_ddl(s).ok();
        if ddl(&reading.schema) != ddl(schema) || ddl(schema).is_none() {
            return Err(ParseInstanceError::SchemaMismatch {
                protocol: PROTOCOL.to_owned(),
                message: "the script declares a schema other than the one given".to_owned(),
            });
        }
        Ok(reading.instance)
    }
}

impl crate::traits::TableInstanceEmitter for PostgresqlCodec {
    fn protocol_name(&self) -> &str {
        PROTOCOL
    }

    fn emit_table(
        &self,
        schema: &Schema,
        instance: &FInstance,
    ) -> Result<Vec<u8>, crate::error::EmitInstanceError> {
        write_dump(schema, instance)
            .map(String::into_bytes)
            .map_err(|e| crate::error::EmitInstanceError::Emit {
                protocol: PROTOCOL.to_owned(),
                message: e.to_string(),
            })
    }
}
