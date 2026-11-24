//! Table directories: a schema file of `PostgreSQL` DDL and one file of
//! rows per table, named by a versioned manifest.
//!
//! A directory is a map from file names to bytes. Its manifest,
//! `manifest.json`, is required; nothing about the directory is inferred
//! from file names. It names the schema file, the dialect of the table
//! files, and the file of each table:
//!
//! ```json
//! {
//!   "format": "panproto-table-directory",
//!   "version": 1,
//!   "protocol": "postgresql",
//!   "schema": "schema.sql",
//!   "dialect": "csv",
//!   "tables": [{ "table": "actor", "file": "actor.csv" }]
//! }
//! ```
//!
//! The schema file is read by the DDL reader and may hold no rows. A table
//! the manifest lists no file for has no rows. Each table file starts with
//! a header line naming every column of its table once, in any order, and
//! has one line per row. The two dialects are those of `PostgreSQL` 17's
//! `COPY ... TO` and `COPY ... FROM` with `HEADER`:
//!
//! - `csv` (`FORMAT csv`): fields separated by `,`, a field in `"` quotes
//!   with `""` for a quote and any line break inside, records ended by
//!   `\n` or `\r\n`. An unquoted empty field is `NULL`, a quoted one the
//!   empty string. This is RFC 4180 with `NULL` added: RFC 4180 has no
//!   `NULL`, and a file written to it reads its unquoted empty fields as
//!   `NULL` here. A bare `\r`, a quote inside an unquoted field, and an
//!   unquoted `\.` alone on a line (`PostgreSQL`'s end-of-data marker) are
//!   refused.
//! - `text` (`FORMAT text`): fields separated by a tab, lines ended by
//!   `\n` or `\r\n`, `\N` for `NULL`, and backslash escapes: `\b`, `\f`,
//!   `\n`, `\r`, `\t`, `\v`, `\\`, one to three octal digits, `x` and one
//!   or two hex digits, and a backslash before any other character for
//!   that character. A line `\.` is refused.
//!
//! Each field is the input text of its column's type, read as an `INSERT`
//! constant's text is (see [`values`](super::values)). Rows are numbered
//! as a script's are: by table name, then in file order.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::Path;

use panproto_inst::{FInstance, Row, Value};
use panproto_protocols::database::postgresql::{OtherStatements, emit_ddl};
use panproto_schema::Schema;
use serde::{Deserialize, Serialize};

use super::data::{Column, SqlDataError, WriteRefusal, columns, instance_rows, number_rows};
use super::read_dump;
use super::values::{ValueRefusal, decode, encode};
use crate::tabular_pathway::{csv_records, split_fields, split_lines};

/// The name of a table directory's manifest.
pub const MANIFEST: &str = "manifest.json";

/// The format name a manifest declares.
const FORMAT: &str = "panproto-table-directory";

/// The manifest version this reader reads and this writer writes.
pub const MANIFEST_VERSION: u32 = 1;

/// The dialect of a table directory's table files.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Dialect {
    /// `PostgreSQL`'s `COPY ... (FORMAT csv, HEADER)`.
    Csv,
    /// `PostgreSQL`'s `COPY ... (FORMAT text, HEADER)`.
    Text,
}

impl Dialect {
    const fn extension(self) -> &'static str {
        match self {
            Self::Csv => "csv",
            Self::Text => "tsv",
        }
    }
}

/// One table's file, as the manifest lists it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TableFile {
    /// The table.
    pub table: String,
    /// The file holding its rows.
    pub file: String,
}

/// A table directory's manifest.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    /// Always `panproto-table-directory`.
    pub format: String,
    /// The manifest version, [`MANIFEST_VERSION`].
    pub version: u32,
    /// The protocol of the schema file, `postgresql`.
    pub protocol: String,
    /// The schema file.
    pub schema: String,
    /// The dialect of the table files.
    pub dialect: Dialect,
    /// The table files, by table.
    pub tables: Vec<TableFile>,
}

/// Why a table directory is refused.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum TableDirectoryError {
    /// The directory has no `manifest.json`.
    #[error("the table directory has no manifest.json")]
    NoManifest,
    /// The manifest is not a manifest of this format and version.
    #[error("manifest.json: {0}")]
    Manifest(String),
    /// A file the manifest names is not in the directory.
    #[error("the table directory has no file {0:?}")]
    MissingFile(String),
    /// A file name that is not a plain name in the directory.
    #[error("{0:?} is not a file name of the directory")]
    FileName(String),
    /// The manifest lists a table twice, a file twice, or a table the
    /// schema does not declare.
    #[error("manifest.json lists {what} {name:?} {reason}")]
    Listing {
        /// `table` or `file`.
        what: &'static str,
        /// The name.
        name: String,
        /// Why it is refused.
        reason: &'static str,
    },
    /// The schema file is refused, or holds rows.
    #[error("schema file {file:?}: {error}")]
    Schema {
        /// The schema file.
        file: String,
        /// Its refusal.
        error: SqlDataError,
    },
    /// A table file is not of its dialect.
    #[error("{file:?} at byte {offset}: {reason}")]
    Record {
        /// The table file.
        file: String,
        /// The byte offset of the line or field.
        offset: usize,
        /// What is wrong.
        reason: String,
    },
    /// A header that does not name every column of its table once.
    #[error("{file:?}: the header {reason} {column:?}")]
    Header {
        /// The table file.
        file: String,
        /// The column.
        column: String,
        /// `names no column`, `names twice` or `leaves out`.
        reason: &'static str,
    },
    /// A field its column's type does not read.
    #[error("{file:?} at byte {offset}, column {column:?}: {reason}")]
    Value {
        /// The table file.
        file: String,
        /// The byte offset of the field.
        offset: usize,
        /// The column.
        column: String,
        /// Why it is refused.
        reason: ValueRefusal,
    },
    /// An instance the directory cannot write.
    #[error(transparent)]
    Unwritable(SqlDataError),
    /// The directory could not be read from or written to disk.
    #[error("table directory I/O: {0}")]
    Io(String),
}

fn record(file: &str, offset: usize, reason: impl Into<String>) -> TableDirectoryError {
    TableDirectoryError::Record {
        file: file.to_owned(),
        offset,
        reason: reason.into(),
    }
}

/// Whether `name` is a plain file name: not empty, no path separator, not
/// `.` or `..`.
fn plain_file_name(name: &str) -> bool {
    !name.is_empty() && !name.contains(['/', '\\', '\0']) && name != "." && name != ".."
}

/// Read a table directory into its schema and rows.
///
/// # Errors
///
/// A [`TableDirectoryError`]: no manifest or one of another format or
/// version; a file it names missing or not a plain name; a table listed
/// twice or not declared; a schema file refused or holding rows; a table
/// file that is not of its dialect; a header that does not name every
/// column once; or a field its column's type does not read.
pub fn read_table_directory(
    files: &BTreeMap<String, Vec<u8>>,
) -> Result<(Schema, FInstance), TableDirectoryError> {
    let manifest_bytes = files.get(MANIFEST).ok_or(TableDirectoryError::NoManifest)?;
    let manifest: Manifest = serde_json::from_slice(manifest_bytes)
        .map_err(|e| TableDirectoryError::Manifest(e.to_string()))?;
    if manifest.format != FORMAT {
        return Err(TableDirectoryError::Manifest(format!(
            "format {:?} is not {FORMAT:?}",
            manifest.format
        )));
    }
    if manifest.version != MANIFEST_VERSION {
        return Err(TableDirectoryError::Manifest(format!(
            "version {} is not {MANIFEST_VERSION}",
            manifest.version
        )));
    }
    if manifest.protocol != "postgresql" {
        return Err(TableDirectoryError::Manifest(format!(
            "protocol {:?} is not \"postgresql\"",
            manifest.protocol
        )));
    }
    let file = |name: &str| -> Result<&str, TableDirectoryError> {
        if !plain_file_name(name) || name == MANIFEST {
            return Err(TableDirectoryError::FileName(name.to_owned()));
        }
        let bytes = files
            .get(name)
            .ok_or_else(|| TableDirectoryError::MissingFile(name.to_owned()))?;
        std::str::from_utf8(bytes).map_err(|e| record(name, e.valid_up_to(), "not UTF-8"))
    };
    let schema_text = file(&manifest.schema)?;
    let schema_error = |error| TableDirectoryError::Schema {
        file: manifest.schema.clone(),
        error,
    };
    let reading = read_dump(schema_text, OtherStatements::List).map_err(schema_error)?;
    if reading.instance.total_row_count() > 0 {
        return Err(schema_error(SqlDataError::Script(
            panproto_protocols::database::postgresql::SqlDdlError::Unsupported {
                offset: 0,
                feature: "rows in the schema file of a table directory".to_owned(),
            },
        )));
    }
    let schema = reading.schema;
    let mut tables = BTreeSet::new();
    let mut names = BTreeSet::new();
    let mut by_table: BTreeMap<String, Vec<Row>> = BTreeMap::new();
    for entry in &manifest.tables {
        let listing = |what, name: &str, reason| TableDirectoryError::Listing {
            what,
            name: name.to_owned(),
            reason,
        };
        if !tables.insert(entry.table.clone()) {
            return Err(listing("table", &entry.table, "twice"));
        }
        if !names.insert(entry.file.clone()) || entry.file == manifest.schema {
            return Err(listing("file", &entry.file, "twice"));
        }
        let table_columns = columns(&schema, &entry.table)
            .ok_or_else(|| listing("table", &entry.table, "that the schema does not declare"))?;
        let text = file(&entry.file)?;
        let rows = match manifest.dialect {
            Dialect::Csv => csv_rows(&entry.file, text, &table_columns)?,
            Dialect::Text => text_rows(&entry.file, text, &table_columns)?,
        };
        if !rows.is_empty() {
            by_table.insert(entry.table.clone(), rows);
        }
    }
    Ok((schema, number_rows(by_table)))
}

/// The column of each header field, checked to name every column once.
fn header(
    file: &str,
    names: &[String],
    table_columns: &[Column],
) -> Result<Vec<usize>, TableDirectoryError> {
    let header_error = |column: &str, reason| TableDirectoryError::Header {
        file: file.to_owned(),
        column: column.to_owned(),
        reason,
    };
    let mut seen = BTreeSet::new();
    let mut out = Vec::with_capacity(names.len());
    for name in names {
        let index = table_columns
            .iter()
            .position(|c| &c.name == name)
            .ok_or_else(|| header_error(name, "names no column"))?;
        if !seen.insert(index) {
            return Err(header_error(name, "names twice"));
        }
        out.push(index);
    }
    if let Some(missing) = table_columns
        .iter()
        .enumerate()
        .find(|(i, _)| !seen.contains(i))
    {
        return Err(header_error(&missing.1.name, "leaves out"));
    }
    Ok(out)
}

/// One row from its fields, each `None` for `NULL`, in header order.
fn row(
    file: &str,
    fields: Vec<(Option<String>, usize)>,
    order: &[usize],
    table_columns: &[Column],
    line_offset: usize,
) -> Result<Row, TableDirectoryError> {
    if fields.len() != order.len() {
        return Err(record(
            file,
            line_offset,
            format!(
                "{} fields where the header has {}",
                fields.len(),
                order.len()
            ),
        ));
    }
    let mut out = Row::new();
    for ((field, offset), &index) in fields.into_iter().zip(order) {
        let column = &table_columns[index];
        let value = match field {
            None => Value::Null,
            Some(text) => {
                decode(&text, &column.ty).map_err(|reason| TableDirectoryError::Value {
                    file: file.to_owned(),
                    offset,
                    column: column.name.clone(),
                    reason,
                })?
            }
        };
        out.insert(column.name.clone(), value);
    }
    Ok(out)
}

fn utf8(file: &str, bytes: Vec<u8>, offset: usize) -> Result<String, TableDirectoryError> {
    String::from_utf8(bytes).map_err(|_| record(file, offset, "not UTF-8"))
}

/// The rows of a `csv` table file.
fn csv_rows(
    file: &str,
    text: &str,
    table_columns: &[Column],
) -> Result<Vec<Row>, TableDirectoryError> {
    let records =
        csv_records(text.as_bytes(), "postgresql").map_err(|e| record(file, 0, e.to_string()))?;
    let mut records = records.into_iter();
    let Some(first) = records.next() else {
        return Err(record(file, 0, "no header line"));
    };
    let names = first
        .into_iter()
        .map(|cell| utf8(file, cell.bytes, cell.offset))
        .collect::<Result<Vec<_>, _>>()?;
    let order = header(file, &names, table_columns)?;
    let mut rows = Vec::new();
    for fields in records {
        let line_offset = fields.first().map_or(text.len(), |c| c.offset);
        if let [only] = fields.as_slice()
            && !only.quoted
            && only.bytes == b"\\."
        {
            return Err(record(file, line_offset, "the end-of-data marker \\."));
        }
        let fields = fields
            .into_iter()
            .map(|cell| {
                let value = if cell.bytes.is_empty() && !cell.quoted {
                    None
                } else {
                    Some(utf8(file, cell.bytes, cell.offset)?)
                };
                Ok((value, cell.offset))
            })
            .collect::<Result<Vec<_>, TableDirectoryError>>()?;
        rows.push(row(file, fields, &order, table_columns, line_offset)?);
    }
    Ok(rows)
}

/// The value of a `text` field, `None` for `\N`.
fn text_field(
    file: &str,
    field: &[u8],
    offset: usize,
) -> Result<Option<String>, TableDirectoryError> {
    if field == b"\\N" {
        return Ok(None);
    }
    let mut out = Vec::with_capacity(field.len());
    let mut i = 0;
    while i < field.len() {
        if field[i] != b'\\' {
            if field[i] == b'\r' {
                return Err(record(file, offset + i, "a carriage return inside a line"));
            }
            out.push(field[i]);
            i += 1;
            continue;
        }
        let Some(&c) = field.get(i + 1) else {
            return Err(record(
                file,
                offset + i,
                "a backslash at the end of a field",
            ));
        };
        let simple = match c {
            b'b' => Some(0x08),
            b'f' => Some(0x0c),
            b'n' => Some(b'\n'),
            b'r' => Some(b'\r'),
            b't' => Some(b'\t'),
            b'v' => Some(0x0b),
            _ => None,
        };
        if let Some(b) = simple {
            out.push(b);
            i += 2;
        } else if (b'0'..=b'7').contains(&c) {
            let digits = field[i + 1..]
                .iter()
                .take(3)
                .take_while(|d| (b'0'..=b'7').contains(*d))
                .count();
            let mut n = 0u32;
            for d in &field[i + 1..i + 1 + digits] {
                n = n * 8 + u32::from(d - b'0');
            }
            out.push(u8::try_from(n & 0xff).unwrap_or(0));
            i += 1 + digits;
        } else if c == b'x' && field.get(i + 2).is_some_and(u8::is_ascii_hexdigit) {
            let digits = field[i + 2..]
                .iter()
                .take(2)
                .take_while(|d| d.is_ascii_hexdigit())
                .count();
            let hex = std::str::from_utf8(&field[i + 2..i + 2 + digits]).unwrap_or("0");
            out.push(u8::from_str_radix(hex, 16).unwrap_or(0));
            i += 2 + digits;
        } else {
            out.push(c);
            i += 2;
        }
    }
    if out.contains(&0) {
        return Err(record(file, offset, "a NUL byte"));
    }
    utf8(file, out, offset).map(Some)
}

/// The rows of a `text` table file.
fn text_rows(
    file: &str,
    text: &str,
    table_columns: &[Column],
) -> Result<Vec<Row>, TableDirectoryError> {
    let base = text.as_ptr().addr();
    let offset_of = |slice: &[u8]| slice.as_ptr().addr().saturating_sub(base);
    let lines = split_lines(text.as_bytes());
    let mut lines = lines.into_iter();
    let Some(first) = lines.next() else {
        return Err(record(file, 0, "no header line"));
    };
    let names = split_fields(first, b'\t')
        .into_iter()
        .map(|f| {
            text_field(file, f, offset_of(f))?
                .ok_or_else(|| record(file, offset_of(f), "a NULL column name"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let order = header(file, &names, table_columns)?;
    let mut rows = Vec::new();
    for line in lines {
        let line_offset = offset_of(line);
        if line == b"\\." {
            return Err(record(file, line_offset, "the end-of-data marker \\."));
        }
        let fields = split_fields(line, b'\t')
            .into_iter()
            .map(|f| Ok((text_field(file, f, offset_of(f))?, offset_of(f))))
            .collect::<Result<Vec<_>, TableDirectoryError>>()?;
        rows.push(row(file, fields, &order, table_columns, line_offset)?);
    }
    Ok(rows)
}

/// The file name of `table`'s rows: every byte outside `a`-`z`, `0`-`9`,
/// `_` and `-` written as `%` and two uppercase hex digits, so distinct
/// tables have distinct names on every file system, then the dialect's
/// extension.
fn table_file_name(table: &str, dialect: Dialect) -> String {
    let mut out = String::with_capacity(table.len() + 4);
    for b in table.bytes() {
        if b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-' {
            out.push(char::from(b));
        } else {
            let _ = write!(out, "%{b:02X}");
        }
    }
    out.push('.');
    out.push_str(dialect.extension());
    out
}

/// A `csv` field: unquoted unless it is empty, holds a `,`, a `"`, a line
/// break, or is `\.`; quotes doubled inside quotes.
fn csv_field(text: &str) -> String {
    let quote = text.is_empty() || text == "\\." || text.contains([',', '"', '\n', '\r']);
    if quote {
        format!("\"{}\"", text.replace('"', "\"\""))
    } else {
        text.to_owned()
    }
}

/// A `text` field: a backslash, tab, line feed and carriage return escaped.
fn text_escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            c => out.push(c),
        }
    }
    out
}

/// Write a schema and its rows as a table directory of `dialect`.
///
/// The directory holds `manifest.json`, the schema as `schema.sql` in the
/// canonical DDL of `emit_ddl`, and one file per table of the schema, in
/// name order, named from the table (see the module documentation), each
/// with a header of the table's columns in position order and its rows in
/// id order. The output is a function of the schema and the instance, and
/// reading it gives the schema and the instance again, its rows numbered
/// as the reader numbers them.
///
/// # Errors
///
/// [`TableDirectoryError::Unwritable`] for a schema with no DDL form, or an
/// instance [`write_data`](super::write_data) would refuse.
pub fn write_table_directory(
    schema: &Schema,
    instance: &FInstance,
    dialect: Dialect,
) -> Result<BTreeMap<String, Vec<u8>>, TableDirectoryError> {
    let ddl = emit_ddl(schema)
        .map_err(|e| TableDirectoryError::Unwritable(SqlDataError::NoDdlForm(e.to_string())))?;
    let rows: BTreeMap<String, Vec<&Row>> = instance_rows(schema, instance)
        .map_err(TableDirectoryError::Unwritable)?
        .into_iter()
        .map(|(table, _, rows)| (table, rows))
        .collect();
    let mut table_names: Vec<&str> = schema
        .vertices
        .values()
        .filter(|v| v.kind == "table")
        .map(|v| v.id.as_str())
        .collect();
    table_names.sort_unstable();
    let mut files = BTreeMap::new();
    let mut listed = Vec::new();
    for table in table_names {
        let table_columns = columns(schema, table).ok_or_else(|| {
            TableDirectoryError::Unwritable(SqlDataError::Unwritable {
                table: table.to_owned(),
                column: None,
                reason: WriteRefusal::UnknownTable,
            })
        })?;
        let mut out = String::new();
        let names: Vec<String> = table_columns
            .iter()
            .map(|c| match dialect {
                Dialect::Csv => csv_field(&c.name),
                Dialect::Text => text_escape(&c.name),
            })
            .collect();
        let separator = match dialect {
            Dialect::Csv => ",",
            Dialect::Text => "\t",
        };
        out.push_str(&names.join(separator));
        out.push('\n');
        for row in rows.get(table).into_iter().flatten() {
            let mut fields = Vec::with_capacity(table_columns.len());
            for column in &table_columns {
                let value = row.get(&column.name).unwrap_or(&Value::Null);
                if *value == Value::Null {
                    fields.push(match dialect {
                        Dialect::Csv => String::new(),
                        Dialect::Text => "\\N".to_owned(),
                    });
                    continue;
                }
                let text = encode(value, &column.ty).map_err(|reason| {
                    TableDirectoryError::Unwritable(SqlDataError::Unwritable {
                        table: table.to_owned(),
                        column: Some(column.name.clone()),
                        reason: WriteRefusal::Value(reason),
                    })
                })?;
                fields.push(match dialect {
                    Dialect::Csv => csv_field(&text),
                    Dialect::Text => text_escape(&text),
                });
            }
            out.push_str(&fields.join(separator));
            out.push('\n');
        }
        let file = table_file_name(table, dialect);
        files.insert(file.clone(), out.into_bytes());
        listed.push(TableFile {
            table: table.to_owned(),
            file,
        });
    }
    let manifest = Manifest {
        format: FORMAT.to_owned(),
        version: MANIFEST_VERSION,
        protocol: "postgresql".to_owned(),
        schema: "schema.sql".to_owned(),
        dialect,
        tables: listed,
    };
    let mut manifest_text = serde_json::to_string_pretty(&manifest)
        .map_err(|e| TableDirectoryError::Manifest(e.to_string()))?;
    manifest_text.push('\n');
    files.insert("schema.sql".to_owned(), ddl.into_bytes());
    files.insert(MANIFEST.to_owned(), manifest_text.into_bytes());
    Ok(files)
}

/// Read the manifest of the directory at `dir` and the files it names.
///
/// # Errors
///
/// [`TableDirectoryError::Io`] when a file cannot be read, or a refusal of
/// [`read_table_directory`]'s manifest checks.
pub fn load_table_directory(dir: &Path) -> Result<BTreeMap<String, Vec<u8>>, TableDirectoryError> {
    let io = |e: std::io::Error| TableDirectoryError::Io(e.to_string());
    let manifest_bytes = match std::fs::read(dir.join(MANIFEST)) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(TableDirectoryError::NoManifest);
        }
        Err(e) => return Err(io(e)),
    };
    let manifest: Manifest = serde_json::from_slice(&manifest_bytes)
        .map_err(|e| TableDirectoryError::Manifest(e.to_string()))?;
    let mut files = BTreeMap::from([(MANIFEST.to_owned(), manifest_bytes)]);
    let names = std::iter::once(&manifest.schema).chain(manifest.tables.iter().map(|t| &t.file));
    for name in names {
        if !plain_file_name(name) || name == MANIFEST {
            return Err(TableDirectoryError::FileName(name.clone()));
        }
        match std::fs::read(dir.join(name)) {
            Ok(bytes) => {
                files.insert(name.clone(), bytes);
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Err(TableDirectoryError::MissingFile(name.clone()));
            }
            Err(e) => return Err(io(e)),
        }
    }
    Ok(files)
}

/// Write the files of a table directory into `dir`, which must exist.
///
/// # Errors
///
/// [`TableDirectoryError::FileName`] for a name that is not a plain file
/// name, or [`TableDirectoryError::Io`] when a file cannot be written.
pub fn store_table_directory(
    dir: &Path,
    files: &BTreeMap<String, Vec<u8>>,
) -> Result<(), TableDirectoryError> {
    for (name, bytes) in files {
        if !plain_file_name(name) {
            return Err(TableDirectoryError::FileName(name.clone()));
        }
        std::fs::write(dir.join(name), bytes)
            .map_err(|e| TableDirectoryError::Io(e.to_string()))?;
    }
    Ok(())
}
