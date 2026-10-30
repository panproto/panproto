//! Shared tabular pathway for line/field-delimited instance parsing via `memchr`.
//!
//! Provides SIMD-accelerated line and field splitting for protocols whose
//! instance data is tab-separated (CoNLL-U), comma-separated (CSV),
//! or fixed-field (EDI X12, SWIFT MT). Produces `FInstance` (set-valued
//! functor) tables guided by a schema.

use std::borrow::Cow;
use std::collections::{HashMap, HashSet};

use memchr::memchr_iter;

use panproto_gat::Name;
use panproto_inst::value::coerce::{LexClass, Lexeme};
use panproto_inst::value::{BigIntValue, Value};
use panproto_inst::{FInstance, Row, RowId, ValueSite};
use panproto_schema::value_kinds::NumeralDialect;
use panproto_schema::{Protocol, ScalarKind, Schema, ValueSpec};

use crate::error::{EmitInstanceError, EmitValueError, EmitValueReason, ParseInstanceError};

const CONLLU_COLUMNS: [&str; 10] = [
    "ID", "FORM", "LEMMA", "UPOS", "XPOS", "FEATS", "HEAD", "DEPREL", "DEPS", "MISC",
];

/// Split input bytes into lines using SIMD-accelerated newline scanning.
#[must_use]
pub fn split_lines(input: &[u8]) -> Vec<&[u8]> {
    let mut lines = Vec::new();
    let mut start = 0;
    for pos in memchr_iter(b'\n', input) {
        let end = if pos > 0 && input[pos - 1] == b'\r' {
            pos - 1
        } else {
            pos
        };
        lines.push(&input[start..end]);
        start = pos + 1;
    }
    if start < input.len() {
        lines.push(&input[start..]);
    }
    lines
}

/// Split a line into fields by a delimiter byte using SIMD scanning.
#[must_use]
pub fn split_fields(line: &[u8], delimiter: u8) -> Vec<&[u8]> {
    let mut fields = Vec::new();
    let mut start = 0;
    for pos in memchr_iter(delimiter, line) {
        fields.push(&line[start..pos]);
        start = pos + 1;
    }
    fields.push(&line[start..]);
    fields
}

fn tabular_data_lines(input: &[u8], comment_prefix: Option<u8>) -> Vec<&[u8]> {
    let mut saw_header = false;
    split_lines(input)
        .into_iter()
        .filter(|line| {
            if comment_prefix.is_some_and(|prefix| line.first() == Some(&prefix))
                || (line.is_empty() && !saw_header)
            {
                return false;
            }
            saw_header = true;
            true
        })
        .collect()
}

struct ParsedCell<'input> {
    bytes: Cow<'input, [u8]>,
    source_offset: Option<u64>,
}

struct CsvRecord<'input> {
    cells: Vec<ParsedCell<'input>>,
    source_start: usize,
    source_end: usize,
}

fn csv_parse_error(protocol: &str, message: &str) -> ParseInstanceError {
    ParseInstanceError::Parse {
        protocol: protocol.to_owned(),
        message: message.to_owned(),
    }
}

fn finish_csv_cell(cells: &mut Vec<ParsedCell<'_>>, bytes: &mut Vec<u8>, source_start: usize) {
    cells.push(ParsedCell {
        bytes: Cow::Owned(std::mem::take(bytes)),
        source_offset: u64::try_from(source_start).ok(),
    });
}

/// Read the CSV records of `input` from byte `start`, every offset counted
/// from byte 0 of `input`.
fn parse_csv_records<'input>(
    input: &'input [u8],
    start: usize,
    protocol: &str,
) -> Result<Vec<CsvRecord<'input>>, ParseInstanceError> {
    let mut records = Vec::new();
    let mut cells = Vec::new();
    let mut cell = Vec::new();
    let mut record_start = start;
    let mut cell_start = start;
    let mut index = start;
    let mut in_quotes = false;
    let mut after_quote = false;

    while index < input.len() {
        let byte = input[index];
        if in_quotes {
            if byte != b'"' {
                cell.push(byte);
                index += 1;
            } else if input.get(index + 1) == Some(&b'"') {
                cell.push(b'"');
                index += 2;
            } else {
                in_quotes = false;
                after_quote = true;
                index += 1;
            }
            continue;
        }

        match byte {
            b',' => {
                finish_csv_cell(&mut cells, &mut cell, cell_start);
                index += 1;
                cell_start = index;
                after_quote = false;
            }
            b'\n' => {
                finish_csv_cell(&mut cells, &mut cell, cell_start);
                records.push(CsvRecord {
                    cells: std::mem::take(&mut cells),
                    source_start: record_start,
                    source_end: index,
                });
                index += 1;
                record_start = index;
                cell_start = index;
                after_quote = false;
            }
            b'\r' if input.get(index + 1) == Some(&b'\n') => {
                finish_csv_cell(&mut cells, &mut cell, cell_start);
                records.push(CsvRecord {
                    cells: std::mem::take(&mut cells),
                    source_start: record_start,
                    source_end: index,
                });
                index += 2;
                record_start = index;
                cell_start = index;
                after_quote = false;
            }
            b'"' if index == cell_start && cell.is_empty() && !after_quote => {
                in_quotes = true;
                index += 1;
            }
            b'"' => return Err(csv_parse_error(protocol, "misplaced quote in CSV field")),
            b'\r' => {
                return Err(csv_parse_error(
                    protocol,
                    "bare carriage return in CSV record",
                ));
            }
            _ if after_quote => {
                return Err(csv_parse_error(
                    protocol,
                    "bytes follow the closing quote in a CSV field",
                ));
            }
            _ => {
                cell.push(byte);
                index += 1;
            }
        }
    }

    if in_quotes {
        return Err(csv_parse_error(protocol, "unterminated quoted CSV field"));
    }
    if cell_start < input.len() || !cells.is_empty() || after_quote {
        finish_csv_cell(&mut cells, &mut cell, cell_start);
        records.push(CsvRecord {
            cells,
            source_start: record_start,
            source_end: input.len(),
        });
    }
    Ok(records)
}

/// The UTF-8 encoding of U+FEFF. At the start of a delimited text it is an
/// encoding signature, not content: the canonical reader starts after it,
/// with every offset still counted from byte 0, and the writer never adds it.
const UTF8_BYTE_ORDER_MARK: &[u8] = &[0xEF, 0xBB, 0xBF];

fn tabular_rows<'input>(
    input: &'input [u8],
    protocol: &str,
    delimiter: u8,
    comment_prefix: Option<u8>,
) -> Result<Vec<Vec<ParsedCell<'input>>>, ParseInstanceError> {
    let start = if input.starts_with(UTF8_BYTE_ORDER_MARK) {
        UTF8_BYTE_ORDER_MARK.len()
    } else {
        0
    };
    if delimiter == b',' {
        let mut saw_header = false;
        return parse_csv_records(input, start, protocol).map(|records| {
            records
                .into_iter()
                .filter(|record| {
                    if comment_prefix
                        .is_some_and(|prefix| input.get(record.source_start) == Some(&prefix))
                        || (record.source_start == record.source_end && !saw_header)
                    {
                        return false;
                    }
                    saw_header = true;
                    true
                })
                .map(|record| record.cells)
                .collect()
        });
    }

    Ok(tabular_data_lines(&input[start..], comment_prefix)
        .into_iter()
        .map(|line| {
            split_fields(line, delimiter)
                .into_iter()
                .map(|field| ParsedCell {
                    bytes: Cow::Borrowed(field),
                    source_offset: byte_offset(input, field),
                })
                .collect()
        })
        .collect())
}

/// Parse tab-separated data into an `FInstance`.
///
/// Assumes the first non-comment, non-blank line is a header row defining
/// column names. Subsequent lines are data rows. A blank line is one empty
/// field when the header has one column; it is otherwise skipped. Lines
/// starting with `comment_prefix` are skipped only when a caller explicitly
/// enables that nonstandard convention.
///
/// Each row becomes a row in the `table_vertex` table of the `FInstance`.
///
/// # Errors
///
/// Returns [`ParseInstanceError::Parse`] if the input is malformed.
pub fn parse_tsv(
    schema: &Schema,
    input: &[u8],
    protocol: &str,
    table_vertex: &str,
    delimiter: u8,
    comment_prefix: Option<u8>,
) -> Result<FInstance, ParseInstanceError> {
    if matches!(delimiter, b'\n' | b'\r') {
        return Err(ParseInstanceError::Parse {
            protocol: protocol.to_owned(),
            message: "a line delimiter cannot also be the field delimiter".to_owned(),
        });
    }
    let protocol_definition = crate::json_pathway::protocol_definition(protocol);
    let data_rows = tabular_rows(input, protocol, delimiter, comment_prefix)?;

    if data_rows.is_empty() {
        return Ok(FInstance::new());
    }

    // First data line is header.
    let headers = data_rows[0]
        .iter()
        .map(|field| parse_utf8_field(field.bytes.as_ref(), protocol))
        .collect::<Result<Vec<_>, _>>()?;
    let mut distinct_headers = HashSet::with_capacity(headers.len());
    if let Some(duplicate) = headers
        .iter()
        .find(|header| !distinct_headers.insert(header.as_str()))
    {
        return Err(ParseInstanceError::Parse {
            protocol: protocol.to_owned(),
            message: format!("duplicate header name '{duplicate}'"),
        });
    }

    // Remaining lines are data rows.
    let mut rows: Vec<HashMap<String, Value>> = Vec::new();
    let mut fidelity = Vec::new();
    for (row_idx, fields) in data_rows[1..].iter().enumerate() {
        if fields.len() == 1 && fields[0].bytes.is_empty() && headers.len() != 1 {
            continue;
        }
        if fields.len() != headers.len() {
            return Err(ParseInstanceError::Parse {
                protocol: protocol.to_owned(),
                message: format!(
                    "data row {} has {} fields but {} header fields are declared",
                    row_idx + 1,
                    fields.len(),
                    headers.len()
                ),
            });
        }
        let mut row = HashMap::new();
        for (field, col_name) in fields.iter().zip(&headers) {
            let col_name = col_name.clone();
            let text = parse_utf8_field(field.bytes.as_ref(), protocol)?;
            let (vertex, spec) =
                delimited_cell_spec(schema, &protocol_definition, table_vertex, &col_name);
            match crate::json_pathway::coerce_exact_lexeme(
                &Lexeme::new(&text, LexClass::String),
                &spec,
            ) {
                Ok(value) => {
                    row.insert(col_name, value);
                }
                Err(error) => {
                    fidelity.push(error.at(ValueSite::new(vertex, field.source_offset)));
                }
            }
        }
        rows.push(row);
    }

    if !fidelity.is_empty() {
        return Err(ParseInstanceError::Fidelity(fidelity));
    }

    FInstance::new()
        .with_table(table_vertex, rows)
        .map_err(|e| ParseInstanceError::Parse {
            protocol: protocol.to_string(),
            message: e.to_string(),
        })
}

/// Parse CoNLL-U formatted data into an `FInstance`.
///
/// CoNLL-U uses tab-separated fields with `#` comments and blank-line
/// sentence boundaries. Each sentence becomes a set of rows in the
/// "sentence" table, with token rows in a "token" table.
///
/// # Errors
///
/// Returns [`ParseInstanceError::Parse`] if the input is malformed.
pub fn parse_conllu(
    schema: &Schema,
    input: &[u8],
    protocol: &str,
) -> Result<FInstance, ParseInstanceError> {
    let protocol_definition = crate::json_pathway::protocol_definition(protocol);
    let lines = split_lines(input);
    let mut sentence_rows: Vec<HashMap<String, Value>> = Vec::new();
    let mut token_rows: Vec<HashMap<String, Value>> = Vec::new();
    let mut current_sent_id: usize = 0;
    let mut in_sentence = false;
    let mut sent_metadata: HashMap<String, Value> = HashMap::new();
    let mut fidelity = Vec::new();

    for line in &lines {
        if line.is_empty() {
            if in_sentence {
                finish_conllu_sentence(current_sent_id, &mut sent_metadata, &mut sentence_rows);
                current_sent_id += 1;
                in_sentence = false;
            }
            continue;
        }

        if line.first() == Some(&b'#') {
            parse_conllu_metadata(
                schema,
                &protocol_definition,
                input,
                line,
                protocol,
                &mut sent_metadata,
                &mut fidelity,
            )?;
            in_sentence = true;
            continue;
        }

        in_sentence = true;
        token_rows.push(parse_conllu_token_row(
            schema,
            &protocol_definition,
            input,
            line,
            protocol,
            current_sent_id,
            &mut fidelity,
        )?);
    }

    if in_sentence {
        finish_conllu_sentence(current_sent_id, &mut sent_metadata, &mut sentence_rows);
    }

    if token_rows.is_empty() {
        return Err(ParseInstanceError::Parse {
            protocol: protocol.to_string(),
            message: "no tokens found in CoNLL-U input".into(),
        });
    }
    if !fidelity.is_empty() {
        return Err(ParseInstanceError::Fidelity(fidelity));
    }

    FInstance::new()
        .with_table("sentence", sentence_rows)
        .and_then(|f| f.with_table("token", token_rows))
        .map_err(|e| ParseInstanceError::Parse {
            protocol: protocol.to_string(),
            message: e.to_string(),
        })
}

fn finish_conllu_sentence(
    sentence_index: usize,
    metadata: &mut HashMap<String, Value>,
    sentences: &mut Vec<HashMap<String, Value>>,
) {
    metadata.insert(
        "sent_idx".into(),
        Value::Int(i64::try_from(sentence_index).unwrap_or(0)),
    );
    sentences.push(std::mem::take(metadata));
}

fn parse_conllu_metadata(
    schema: &Schema,
    protocol_definition: &Protocol,
    input: &[u8],
    line: &[u8],
    protocol: &str,
    metadata: &mut HashMap<String, Value>,
    fidelity: &mut Vec<panproto_inst::ValueFidelityError>,
) -> Result<(), ParseInstanceError> {
    let comment = parse_utf8_field(line, protocol)?;
    let Some((key, value)) = comment.trim_start_matches('#').trim().split_once('=') else {
        return Ok(());
    };
    let column = key.trim().replace(' ', "_");
    let text = value.trim();
    let (vertex, spec) = cell_spec(schema, protocol_definition, "sentence", &column);
    match crate::json_pathway::coerce_exact_lexeme(&Lexeme::new(text, LexClass::String), &spec) {
        Ok(value) => {
            metadata.insert(column, value);
        }
        Err(error) => fidelity.push(error.at(ValueSite::new(vertex, byte_offset(input, line)))),
    }
    Ok(())
}

fn parse_conllu_token_row(
    schema: &Schema,
    protocol_definition: &Protocol,
    input: &[u8],
    line: &[u8],
    protocol: &str,
    sentence_index: usize,
    fidelity: &mut Vec<panproto_inst::ValueFidelityError>,
) -> Result<HashMap<String, Value>, ParseInstanceError> {
    let fields = split_fields(line, b'\t');
    if fields.len() != CONLLU_COLUMNS.len() {
        return Err(ParseInstanceError::Parse {
            protocol: protocol.to_owned(),
            message: format!(
                "CoNLL-U token line at byte {} has {} fields; expected exactly {}",
                byte_offset(input, line).unwrap_or(0),
                fields.len(),
                CONLLU_COLUMNS.len()
            ),
        });
    }

    let mut row = HashMap::new();
    row.insert(
        "sent_idx".into(),
        Value::Int(i64::try_from(sentence_index).unwrap_or(0)),
    );
    for (field, column) in fields.iter().zip(CONLLU_COLUMNS) {
        let text = parse_utf8_field(field, protocol)?;
        if text == "_" {
            continue;
        }
        let (vertex, spec) = cell_spec(schema, protocol_definition, "token", column);
        match crate::json_pathway::coerce_exact_lexeme(&Lexeme::new(&text, LexClass::String), &spec)
        {
            Ok(value) => {
                row.insert(column.to_owned(), value);
            }
            Err(error) => {
                fidelity.push(error.at(ValueSite::new(vertex, byte_offset(input, field))));
            }
        }
    }
    Ok(row)
}

fn parse_utf8_field(field: &[u8], protocol: &str) -> Result<String, ParseInstanceError> {
    std::str::from_utf8(field)
        .map(str::to_owned)
        .map_err(|error| ParseInstanceError::Parse {
            protocol: protocol.to_owned(),
            message: format!("tabular field is not UTF-8: {error}"),
        })
}

fn byte_offset(input: &[u8], field: &[u8]) -> Option<u64> {
    let input_start = input.as_ptr() as usize;
    let field_start = field.as_ptr() as usize;
    field_start
        .checked_sub(input_start)
        .and_then(|offset| u64::try_from(offset).ok())
}

fn cell_spec(
    schema: &Schema,
    protocol: &Protocol,
    table_vertex: &str,
    column: &str,
) -> (Name, ValueSpec) {
    let target = schema
        .outgoing_edges(table_vertex)
        .iter()
        .find(|edge| edge.name.as_deref() == Some(column) || edge.tgt.as_ref() == column)
        .map_or_else(|| Name::from(column), |edge| edge.tgt.clone());
    let spec = if schema.vertices.contains_key(&target) {
        ValueSpec::for_vertex(schema, protocol, &target)
    } else {
        ValueSpec::new(ScalarKind::Any)
    };
    (target, spec)
}

/// The spec of a CSV or TSV cell: the column's spec in the JSON numeral
/// grammar, which has no non-finite spelling and no special decimal.
fn delimited_cell_spec(
    schema: &Schema,
    protocol: &Protocol,
    table_vertex: &str,
    column: &str,
) -> (Name, ValueSpec) {
    let (vertex, spec) = cell_spec(schema, protocol, table_vertex, column);
    (
        vertex,
        ValueSpec {
            numeral: Some(NumeralDialect::Json),
            ..spec
        },
    )
}

/// Emit an `FInstance` to tab-separated bytes.
///
/// Writes a header row followed by data rows. The `table_vertex` specifies
/// which table to emit. Columns are sorted alphabetically.
///
/// A table the instance does not hold has no rows, so it emits no bytes.
///
/// # Errors
///
/// Returns a typed value error when a cell has no faithful delimited-text form.
pub fn emit_tsv(
    instance: &FInstance,
    protocol: &str,
    table_vertex: &str,
    delimiter: u8,
) -> Result<Vec<u8>, EmitInstanceError> {
    emit_tsv_impl(None, instance, protocol, table_vertex, delimiter, None)
}

/// Emit a schema-guided `FInstance` to delimited text.
///
/// Unlike [`emit_tsv`], this form can encode [`Value::Bytes`] according to
/// the column's declared [`ScalarKind::Bytes`] representation. The
/// schema-less form returns a typed refusal for bytes because writing their
/// raw octets would not round-trip through a textual parser.
///
/// # Errors
///
/// Returns a typed value error when a cell has no faithful representation
/// under its column's [`ValueSpec`].
pub fn emit_tsv_with_schema(
    schema: &Schema,
    instance: &FInstance,
    protocol: &str,
    table_vertex: &str,
    delimiter: u8,
) -> Result<Vec<u8>, EmitInstanceError> {
    emit_tsv_impl(
        Some(schema),
        instance,
        protocol,
        table_vertex,
        delimiter,
        None,
    )
}

pub(crate) fn emit_tsv_with_schema_and_comments(
    schema: &Schema,
    instance: &FInstance,
    protocol: &str,
    table_vertex: &str,
    delimiter: u8,
    comment_prefix: Option<u8>,
) -> Result<Vec<u8>, EmitInstanceError> {
    emit_tsv_impl(
        Some(schema),
        instance,
        protocol,
        table_vertex,
        delimiter,
        comment_prefix,
    )
}

fn emit_tsv_impl(
    schema: Option<&Schema>,
    instance: &FInstance,
    protocol: &str,
    table_vertex: &str,
    delimiter: u8,
    comment_prefix: Option<u8>,
) -> Result<Vec<u8>, EmitInstanceError> {
    let format = match delimiter {
        b',' => "csv",
        b'\t' => "tsv",
        _ => "tabular",
    };
    if matches!(delimiter, b'\n' | b'\r') {
        return Err(EmitInstanceError::Emit {
            protocol: protocol.to_owned(),
            message: "a line delimiter cannot also be the field delimiter".to_owned(),
        });
    }
    validate_canonical_tabular_structure(instance, table_vertex, format)?;
    let rows = instance.rows(table_vertex);
    reject_labeled_nulls(rows.iter().map(|&(row_id, row)| (row_id, row)))?;

    if rows.is_empty() && schema.is_none() {
        return Ok(Vec::new());
    }

    let columns = canonical_tabular_columns(
        schema,
        &rows,
        protocol,
        table_vertex,
        format,
        delimiter,
        comment_prefix,
    )?;
    let mut output = Vec::new();
    let protocol_definition = schema.map(|_| crate::json_pathway::protocol_definition(protocol));
    write_tabular_header(&mut output, &columns, delimiter);
    let context = TabularEmitContext {
        schema,
        protocol_definition: protocol_definition.as_ref(),
        columns: &columns,
        table_vertex,
        format,
        delimiter,
        comment_prefix,
    };
    for (row_id, row) in rows {
        output.extend_from_slice(&emit_tabular_row(&context, row_id, row)?);
        output.push(b'\n');
    }

    Ok(output)
}

fn canonical_tabular_columns(
    schema: Option<&Schema>,
    rows: &[(RowId, &Row)],
    protocol: &str,
    table_vertex: &str,
    format: &str,
    delimiter: u8,
    comment_prefix: Option<u8>,
) -> Result<Vec<String>, EmitInstanceError> {
    let mut columns: std::collections::BTreeSet<String> = rows
        .iter()
        .flat_map(|(_, row)| row.keys().cloned())
        .collect();
    if columns.is_empty()
        && let Some(schema) = schema
    {
        columns.extend(
            schema
                .outgoing_edges(table_vertex)
                .iter()
                .map(|edge| edge.name.as_deref().unwrap_or(&edge.tgt).to_string()),
        );
    }
    let columns: Vec<_> = columns.into_iter().collect();
    if columns.is_empty() {
        return Err(EmitInstanceError::Emit {
            protocol: protocol.to_owned(),
            message: format!("{format} cannot represent a row with no columns"),
        });
    }
    for column in &columns {
        if column.is_empty()
            || column.as_bytes().contains(&delimiter)
            || column.as_bytes().contains(&b'\r')
            || column.as_bytes().contains(&b'\n')
            || (delimiter == b',' && column.as_bytes().contains(&b'"'))
        {
            return Err(EmitInstanceError::Emit {
                protocol: protocol.to_owned(),
                message: format!(
                    "{format} header {column:?} contains a reserved row or field delimiter"
                ),
            });
        }
    }
    if comment_prefix.is_some_and(|prefix| {
        columns
            .first()
            .is_some_and(|column| column.as_bytes().first() == Some(&prefix))
    }) {
        return Err(EmitInstanceError::Emit {
            protocol: protocol.to_owned(),
            message: format!("{format} header would be classified as a comment"),
        });
    }
    Ok(columns)
}

fn write_tabular_header(output: &mut Vec<u8>, columns: &[String], delimiter: u8) {
    for (index, column) in columns.iter().enumerate() {
        if index > 0 {
            output.push(delimiter);
        }
        output.extend_from_slice(column.as_bytes());
    }
    output.push(b'\n');
}

struct TabularEmitContext<'a> {
    schema: Option<&'a Schema>,
    protocol_definition: Option<&'a Protocol>,
    columns: &'a [String],
    table_vertex: &'a str,
    format: &'a str,
    delimiter: u8,
    comment_prefix: Option<u8>,
}

fn emit_tabular_row(
    context: &TabularEmitContext<'_>,
    row_id: RowId,
    row: &Row,
) -> Result<Vec<u8>, EmitInstanceError> {
    let mut output = Vec::new();
    for (index, column) in context.columns.iter().enumerate() {
        if index > 0 {
            output.push(context.delimiter);
        }
        let Some(value) = row.get(column) else {
            let (site_vertex, _) = context.schema.zip(context.protocol_definition).map_or_else(
                || (Name::from(column.as_str()), None),
                |(schema, protocol)| {
                    let (vertex, spec) =
                        delimited_cell_spec(schema, protocol, context.table_vertex, column);
                    (vertex, Some(spec))
                },
            );
            return Err(EmitValueError::new(
                ValueSite::new(site_vertex, None),
                "absent",
                context.format,
                EmitValueReason::AbsentNotRepresentable,
            )
            .into());
        };
        let (site_vertex, spec) = context.schema.zip(context.protocol_definition).map_or_else(
            || (Name::from(column.as_str()), None),
            |(schema, protocol)| {
                let (vertex, spec) =
                    delimited_cell_spec(schema, protocol, context.table_vertex, column);
                (vertex, Some(spec))
            },
        );
        if spec.is_none() && !matches!(value, Value::Str(_) | Value::Null) {
            return Err(EmitValueError::new(
                ValueSite::new(site_vertex, None),
                value.type_name(),
                context.format,
                EmitValueReason::NotRepresentable,
            )
            .into());
        }
        output.extend_from_slice(&tabular_value_bytes(
            value,
            row_id,
            column,
            &site_vertex,
            context.format,
            context.delimiter,
            spec.as_ref(),
        )?);
    }
    if context
        .comment_prefix
        .is_some_and(|prefix| output.first() == Some(&prefix))
    {
        return Err(tabular_row_reclassification_error(
            context.schema,
            context.protocol_definition,
            row,
            context.columns,
            context.table_vertex,
            context.format,
        ));
    }
    Ok(output)
}

fn tabular_row_reclassification_error(
    schema: Option<&Schema>,
    protocol: Option<&Protocol>,
    row: &Row,
    columns: &[String],
    table_vertex: &str,
    format: &str,
) -> EmitInstanceError {
    let column = columns.first().map_or("", String::as_str);
    let value = row.get(column);
    let vertex = schema.zip(protocol).map_or_else(
        || Name::from(column),
        |(schema, protocol)| cell_spec(schema, protocol, table_vertex, column).0,
    );
    EmitValueError::new(
        ValueSite::new(vertex, None),
        value.map_or("absent", Value::type_name),
        format,
        EmitValueReason::NotRepresentable,
    )
    .into()
}

/// Emit an `FInstance` to CoNLL-U format.
///
/// A table the instance does not hold has no rows: without tokens the
/// document has no token lines.
///
/// # Errors
///
/// Returns a typed value error when a cell has no faithful CoNLL-U form.
pub fn emit_conllu(instance: &FInstance, protocol: &str) -> Result<Vec<u8>, EmitInstanceError> {
    emit_conllu_impl(None, instance, protocol)
}

/// Emit CoNLL-U using the same schema-directed scalar contract as parsing.
///
/// # Errors
///
/// Returns a typed value or structural error when emission would not reparse
/// to the same representable instance.
pub fn emit_conllu_with_schema(
    schema: &Schema,
    instance: &FInstance,
    protocol: &str,
) -> Result<Vec<u8>, EmitInstanceError> {
    emit_conllu_impl(Some(schema), instance, protocol)
}

fn emit_conllu_impl(
    schema: Option<&Schema>,
    instance: &FInstance,
    protocol: &str,
) -> Result<Vec<u8>, EmitInstanceError> {
    let token_rows = instance.rows("token");
    let sentence_rows = instance.rows("sentence");
    reject_labeled_nulls(
        token_rows
            .iter()
            .chain(&sentence_rows)
            .map(|&(row_id, row)| (row_id, row)),
    )?;

    validate_canonical_conllu_structure(
        instance,
        &token_rows,
        &sentence_rows,
        &CONLLU_COLUMNS,
        protocol,
    )?;
    if token_rows.is_empty() {
        return Ok(Vec::new());
    }
    let protocol_definition = schema.map(|_| crate::json_pathway::protocol_definition(protocol));
    let context = ConlluEmitContext {
        schema,
        protocol_definition: protocol_definition.as_ref(),
    };
    let mut output = Vec::new();
    let mut current_sent: i64 = -1;

    for (row_id, row) in token_rows {
        let sent_idx = match row.get("sent_idx") {
            Some(Value::Int(n)) => *n,
            _ => 0,
        };

        // Emit sentence boundary + metadata.
        if sent_idx != current_sent {
            if current_sent >= 0 {
                output.push(b'\n');
            }
            current_sent = sent_idx;
            emit_conllu_sentence_metadata(&mut output, &context, &sentence_rows, sent_idx)?;
        }
        emit_conllu_token_line(&mut output, &context, row_id, row)?;
    }
    output.push(b'\n');

    Ok(output)
}

struct ConlluEmitContext<'a> {
    schema: Option<&'a Schema>,
    protocol_definition: Option<&'a Protocol>,
}

fn conllu_cell_spec(
    context: &ConlluEmitContext<'_>,
    table: &str,
    column: &str,
) -> (Name, Option<ValueSpec>) {
    context.schema.zip(context.protocol_definition).map_or_else(
        || (Name::from(column), None),
        |(schema, definition)| {
            let (vertex, spec) = cell_spec(schema, definition, table, column);
            (vertex, Some(spec))
        },
    )
}

fn emit_conllu_sentence_metadata(
    output: &mut Vec<u8>,
    context: &ConlluEmitContext<'_>,
    sentence_rows: &[(RowId, &Row)],
    sentence_index: i64,
) -> Result<(), EmitInstanceError> {
    let Some((row_id, row)) = sentence_rows.iter().find(
        |(_, row)| matches!(row.get("sent_idx"), Some(Value::Int(index)) if *index == sentence_index),
    ) else {
        return Ok(());
    };
    let mut metadata: Vec<_> = row.iter().collect();
    metadata.sort_by_key(|(key, _)| *key);
    for (key, value) in metadata {
        if key == "sent_idx" {
            continue;
        }
        let (site_vertex, spec) = conllu_cell_spec(context, "sentence", key);
        if spec.is_none() && !matches!(value, Value::Str(_)) {
            return Err(EmitValueError::new(
                ValueSite::new(site_vertex, None),
                value.type_name(),
                "conllu-metadata",
                EmitValueReason::NotRepresentable,
            )
            .into());
        }
        let rendered = tabular_value_bytes(
            value,
            *row_id,
            key,
            &site_vertex,
            "conllu-metadata",
            b'\t',
            spec.as_ref(),
        )?;
        output.extend_from_slice(b"# ");
        output.extend_from_slice(key.as_bytes());
        output.extend_from_slice(b" = ");
        output.extend_from_slice(&rendered);
        output.push(b'\n');
    }
    Ok(())
}

fn emit_conllu_token_line(
    output: &mut Vec<u8>,
    context: &ConlluEmitContext<'_>,
    row_id: RowId,
    row: &Row,
) -> Result<(), EmitInstanceError> {
    for (index, column) in CONLLU_COLUMNS.iter().enumerate() {
        if index > 0 {
            output.push(b'\t');
        }
        let Some(value) = row.get(*column) else {
            output.push(b'_');
            continue;
        };
        let (site_vertex, spec) = conllu_cell_spec(context, "token", column);
        if spec.is_none() && !matches!(value, Value::Str(_)) {
            return Err(EmitValueError::new(
                ValueSite::new(site_vertex, None),
                value.type_name(),
                "conllu",
                EmitValueReason::NotRepresentable,
            )
            .into());
        }
        let rendered = tabular_value_bytes(
            value,
            row_id,
            column,
            &site_vertex,
            "conllu",
            b'\t',
            spec.as_ref(),
        )?;
        if index == 0 && rendered.first() == Some(&b'#') {
            return Err(EmitValueError::new(
                ValueSite::new(site_vertex, None),
                value.type_name(),
                "conllu",
                EmitValueReason::NotRepresentable,
            )
            .into());
        }
        output.extend_from_slice(&rendered);
    }
    output.push(b'\n');
    Ok(())
}

fn validate_canonical_conllu_structure(
    instance: &FInstance,
    token_rows: &[(RowId, &Row)],
    sentence_rows: &[(RowId, &Row)],
    columns: &[&str; 10],
    protocol: &str,
) -> Result<(), EmitInstanceError> {
    validate_conllu_containers(instance, token_rows, sentence_rows, protocol)?;
    if token_rows.is_empty() {
        return Ok(());
    }
    validate_conllu_row_ids(token_rows, sentence_rows, protocol)?;
    let used_sentence_indices = validate_conllu_token_rows(token_rows, columns, protocol)?;
    let declared_sentence_indices = validate_conllu_sentence_rows(sentence_rows, protocol)?;
    if declared_sentence_indices != used_sentence_indices {
        let mut declared: Vec<_> = declared_sentence_indices.into_iter().collect();
        declared.sort_unstable();
        let mut used: Vec<_> = used_sentence_indices.into_iter().collect();
        used.sort_unstable();
        return Err(conllu_structural_error(
            protocol,
            format!(
                "sentence sent_idx values {declared:?} do not exactly match token sent_idx values {used:?}"
            ),
        ));
    }
    Ok(())
}

fn conllu_structural_error(protocol: &str, detail: impl Into<String>) -> EmitInstanceError {
    EmitInstanceError::UnsupportedStructuralEdit {
        format: protocol.to_owned(),
        detail: detail.into(),
    }
}

fn validate_conllu_containers(
    instance: &FInstance,
    token_rows: &[(RowId, &Row)],
    sentence_rows: &[(RowId, &Row)],
    protocol: &str,
) -> Result<(), EmitInstanceError> {
    let tables = instance.table_names();
    if tables
        .iter()
        .any(|table| !matches!(table.as_ref(), "sentence" | "token"))
    {
        return Err(conllu_structural_error(
            protocol,
            format!(
                "CoNLL-U can carry only sentence and token tables, not {:?}",
                tables.iter().map(AsRef::as_ref).collect::<Vec<_>>()
            ),
        ));
    }
    if instance.pair_count() != 0 {
        return Err(conllu_structural_error(
            protocol,
            "CoNLL-U canonical emission cannot carry foreign-key pairs",
        ));
    }
    if token_rows.is_empty() && !sentence_rows.is_empty() {
        return Err(conllu_structural_error(
            protocol,
            "sentence rows without token rows would be omitted",
        ));
    }
    Ok(())
}

fn validate_conllu_row_ids(
    token_rows: &[(RowId, &Row)],
    sentence_rows: &[(RowId, &Row)],
    protocol: &str,
) -> Result<(), EmitInstanceError> {
    for (expected, (actual, _)) in sentence_rows.iter().enumerate() {
        if usize::try_from(actual.0).ok() != Some(expected) {
            return Err(conllu_structural_error(
                protocol,
                format!(
                    "sentence row ids must be contiguous from 0; found row {} at position {expected}",
                    actual.0
                ),
            ));
        }
    }
    for (offset, (actual, _)) in token_rows.iter().enumerate() {
        let expected = sentence_rows.len() + offset;
        if usize::try_from(actual.0).ok() != Some(expected) {
            return Err(conllu_structural_error(
                protocol,
                format!(
                    "token row ids must follow sentence rows contiguously; found row {} at position {expected}",
                    actual.0
                ),
            ));
        }
    }
    Ok(())
}

fn validate_conllu_token_rows(
    token_rows: &[(RowId, &Row)],
    columns: &[&str; 10],
    protocol: &str,
) -> Result<HashSet<i64>, EmitInstanceError> {
    let allowed_token_keys: HashSet<&str> = columns
        .iter()
        .copied()
        .chain(std::iter::once("sent_idx"))
        .collect();
    let mut used_sentence_indices = HashSet::new();
    let mut prior_index = None;
    for (row_id, row) in token_rows {
        if let Some(key) = row
            .keys()
            .filter(|key| !allowed_token_keys.contains(key.as_str()))
            .min()
        {
            return Err(conllu_structural_error(
                protocol,
                format!("token row {} has unsupported column {key:?}", row_id.0),
            ));
        }
        let Some(Value::Int(sent_idx)) = row.get("sent_idx") else {
            return Err(conllu_structural_error(
                protocol,
                format!("token row {} requires an integer sent_idx", row_id.0),
            ));
        };
        if *sent_idx < 0 {
            return Err(conllu_structural_error(
                protocol,
                format!("token row {} has negative sent_idx {sent_idx}", row_id.0),
            ));
        }
        if let Some(prior) = prior_index
            && (*sent_idx < prior || *sent_idx > prior.saturating_add(1))
        {
            return Err(conllu_structural_error(
                protocol,
                format!(
                    "token row {} has non-contiguous sent_idx {sent_idx} after {prior}",
                    row_id.0
                ),
            ));
        }
        if prior_index.is_none() && *sent_idx != 0 {
            return Err(conllu_structural_error(
                protocol,
                format!(
                    "the first token row has sent_idx {sent_idx}; canonical CoNLL-U numbering starts at 0"
                ),
            ));
        }
        prior_index = Some(*sent_idx);
        used_sentence_indices.insert(*sent_idx);
    }
    Ok(used_sentence_indices)
}

fn validate_conllu_sentence_rows(
    sentence_rows: &[(RowId, &Row)],
    protocol: &str,
) -> Result<HashSet<i64>, EmitInstanceError> {
    let mut declared_sentence_indices = HashSet::new();
    for (row_id, row) in sentence_rows {
        let Some(Value::Int(sent_idx)) = row.get("sent_idx") else {
            return Err(conllu_structural_error(
                protocol,
                format!("sentence row {} requires an integer sent_idx", row_id.0),
            ));
        };
        if *sent_idx < 0 || !declared_sentence_indices.insert(*sent_idx) {
            return Err(conllu_structural_error(
                protocol,
                format!(
                    "sentence row {} has invalid or duplicate sent_idx {sent_idx}",
                    row_id.0
                ),
            ));
        }
        for key in row.keys().filter(|key| key.as_str() != "sent_idx") {
            if key.is_empty()
                || key.contains('=')
                || key.chars().any(char::is_whitespace)
                || key.trim() != key
            {
                return Err(conllu_structural_error(
                    protocol,
                    format!(
                        "sentence row {} has metadata key {key:?} that reparses differently",
                        row_id.0
                    ),
                ));
            }
            if let Some(Value::Str(text) | Value::Token(text)) = row.get(key)
                && text.trim() != text
            {
                return Err(conllu_structural_error(
                    protocol,
                    format!(
                        "sentence row {} has metadata value for {key:?} whose surrounding whitespace would be trimmed",
                        row_id.0
                    ),
                ));
            }
        }
    }
    Ok(declared_sentence_indices)
}

/// Refuse functor structure that a single-table canonical delimited emitter
/// cannot serialize. Empty instances are representable by an empty document;
/// otherwise the sole non-empty table must be the selected table and no graph
/// edges may be present.
pub(crate) fn validate_canonical_tabular_structure(
    instance: &FInstance,
    table_vertex: &str,
    format: &str,
) -> Result<(), EmitInstanceError> {
    let tables = instance.table_names();
    if tables.len() > 1
        || tables
            .first()
            .is_some_and(|table| table.as_ref() != table_vertex)
    {
        return Err(EmitInstanceError::UnsupportedStructuralEdit {
            format: format.to_owned(),
            detail: format!(
                "canonical emission selects table '{table_vertex}', but the non-empty table set is {:?}",
                tables.iter().map(AsRef::as_ref).collect::<Vec<_>>()
            ),
        });
    }
    if instance.pair_count() != 0 {
        return Err(EmitInstanceError::UnsupportedStructuralEdit {
            format: format.to_owned(),
            detail: "foreign-key pairs cannot be written to a delimited document".to_owned(),
        });
    }
    for (expected, (actual, _)) in instance.rows(table_vertex).iter().enumerate() {
        if usize::try_from(actual.0).ok() != Some(expected) {
            return Err(EmitInstanceError::UnsupportedStructuralEdit {
                format: format.to_owned(),
                detail: format!(
                    "row ids must be contiguous in file order from 0; found row {} at position {expected}",
                    actual.0
                ),
            });
        }
    }
    Ok(())
}

fn tabular_value_bytes(
    value: &Value,
    row: RowId,
    column: &str,
    site_vertex: &str,
    format: &str,
    delimiter: u8,
    spec: Option<&ValueSpec>,
) -> Result<Vec<u8>, EmitInstanceError> {
    let refuse = |reason| -> EmitInstanceError {
        EmitValueError::new(
            ValueSite::new(site_vertex, None),
            value.type_name(),
            format,
            reason,
        )
        .into()
    };
    // A CSV or TSV cell reads numerals in the JSON grammar, which spells no
    // infinity, NaN or special decimal.
    if matches!(format, "csv" | "tsv" | "tabular")
        && (matches!(value, Value::Float(number) if !number.is_finite())
            || matches!(value, Value::Decimal(number) if !number.is_finite()))
    {
        return Err(refuse(EmitValueReason::NonFinite));
    }
    let bigint_bytes = if let Value::BigInt(number) = value {
        Some(tabular_bigint_bytes(number, site_vertex, format)?)
    } else {
        None
    };
    if let Some(spec) = spec
        && !matches!(value, Value::Null)
        && !tabular_value_roundtrips(value, bigint_bytes.as_deref(), spec)
    {
        return Err(refuse(EmitValueReason::NotRepresentable));
    }
    let bytes = tabular_cell_bytes(value, row, column, bigint_bytes, spec, &refuse)?;
    if bytes.contains(&delimiter)
        || bytes.contains(&b'\n')
        || bytes.contains(&b'\r')
        || (delimiter == b',' && bytes.contains(&b'"'))
    {
        return Err(refuse(EmitValueReason::NotRepresentable));
    }
    Ok(bytes)
}

/// Whether the plain-text spelling of `value` (its precomputed big-integer
/// lexeme when it has one) reads back as `value` under `spec`.
fn tabular_value_roundtrips(value: &Value, bigint_bytes: Option<&[u8]>, spec: &ValueSpec) -> bool {
    bigint_bytes.map_or_else(
        || crate::json_pathway::plain_text_value_roundtrips(value, spec),
        |bytes| {
            std::str::from_utf8(bytes)
                .is_ok_and(|text| crate::json_pathway::plain_text_reparses_as(value, text, spec))
        },
    )
}

/// The unquoted cell bytes of `value`, before the delimiter check;
/// `refuse` builds the value refusal at the cell's site.
fn tabular_cell_bytes(
    value: &Value,
    row: RowId,
    column: &str,
    bigint_bytes: Option<Vec<u8>>,
    spec: Option<&ValueSpec>,
    refuse: &dyn Fn(EmitValueReason) -> EmitInstanceError,
) -> Result<Vec<u8>, EmitInstanceError> {
    match value {
        Value::Str(text) | Value::Token(text) => Ok(text.as_bytes().to_vec()),
        Value::Int(number) => Ok(number.to_string().into_bytes()),
        Value::Float(number) => crate::json_pathway::plain_text_float_lexeme(*number)
            .map(String::into_bytes)
            .ok_or_else(|| refuse(EmitValueReason::NonFinite)),
        Value::Bool(value) => Ok(value.to_string().into_bytes()),
        Value::Bytes(bytes) => {
            let Some(ScalarKind::Bytes(encoding)) = spec.map(|spec| &spec.kind) else {
                return Err(refuse(EmitValueReason::NotRepresentable));
            };
            crate::json_pathway::encode_bytes(bytes, encoding)
                .map(String::into_bytes)
                .ok_or_else(|| refuse(EmitValueReason::NotRepresentable))
        }
        Value::Decimal(number) => Ok(number.to_string().into_bytes()),
        Value::BigInt(_) => bigint_bytes.ok_or_else(|| refuse(EmitValueReason::NotRepresentable)),
        Value::Temporal(value) => Ok(value.to_string().into_bytes()),
        Value::Null => Err(refuse(EmitValueReason::NullNotRepresentable)),
        Value::LabeledNull(_) => Err(EmitInstanceError::LabeledNull {
            row,
            column: column.to_owned(),
        }),
        _ => Err(refuse(EmitValueReason::NotRepresentable)),
    }
}

fn tabular_bigint_bytes(
    value: &BigIntValue,
    site_vertex: &str,
    format: &str,
) -> Result<Vec<u8>, EmitInstanceError> {
    crate::json_pathway::bigint_lexeme(value, false)
        .map(String::into_bytes)
        .map_err(|limit| {
            EmitValueError::new(
                ValueSite::new(site_vertex, None),
                "bigint",
                format,
                EmitValueReason::LimitExceeded(limit),
            )
            .into()
        })
}

fn contains_labeled_null(value: &Value) -> bool {
    let mut pending = vec![value];
    while let Some(value) = pending.pop() {
        match value {
            Value::LabeledNull(_) => return true,
            Value::Opaque { fields, .. } | Value::Unknown(fields) => {
                pending.extend(fields.values());
            }
            Value::List(items) => pending.extend(items),
            _ => {}
        }
    }
    false
}

fn reject_labeled_nulls<'row>(
    rows: impl IntoIterator<Item = (RowId, &'row Row)>,
) -> Result<(), EmitInstanceError> {
    let first = rows
        .into_iter()
        .flat_map(|(row, cells)| {
            cells
                .iter()
                .filter(|(_, value)| contains_labeled_null(value))
                .map(move |(column, _)| (row, column.clone()))
        })
        .min_by(|(left_row, left_column), (right_row, right_column)| {
            (left_row, left_column).cmp(&(right_row, right_column))
        });
    match first {
        Some((row, column)) => Err(EmitInstanceError::LabeledNull { row, column }),
        None => Ok(()),
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;

    fn table_with(value: Value) -> FInstance {
        FInstance::new()
            .with_table("rows", vec![HashMap::from([("value".to_owned(), value)])])
            .expect("table")
    }

    fn typed_table_schema() -> Schema {
        let protocol = crate::json_pathway::protocol_definition("sql");
        panproto_schema::SchemaBuilder::new(&protocol)
            .vertex("rows", "table", None)
            .expect("table vertex")
            .vertex("amount", "integer", None)
            .expect("integer vertex")
            .edge("rows", "amount", "prop", Some("amount"))
            .expect("column edge")
            .build()
            .expect("schema")
    }

    fn double_table_schema(kind: &str) -> Schema {
        let protocol = crate::json_pathway::protocol_definition("cassandra");
        panproto_schema::SchemaBuilder::new(&protocol)
            .vertex("rows", "table", None)
            .expect("table vertex")
            .vertex("value", kind, None)
            .expect("value vertex")
            .edge("rows", "value", "prop", Some("value"))
            .expect("column edge")
            .build()
            .expect("schema")
    }

    #[test]
    fn delimited_cells_use_the_json_numeral_dialect() {
        let double = double_table_schema("double");
        let decimal = double_table_schema("decimal");
        for delimiter in [b',', b'\t'] {
            for value in [f64::INFINITY, f64::NEG_INFINITY, f64::NAN, -f64::NAN] {
                let error = emit_tsv_with_schema(
                    &double,
                    &table_with(Value::Float(value)),
                    "cassandra",
                    "rows",
                    delimiter,
                )
                .expect_err("JSON numerals have no non-finite spelling");
                assert!(matches!(
                    error,
                    EmitInstanceError::Value(ref error) if error.reason == EmitValueReason::NonFinite
                ));
            }
            for lexeme in ["Infinity", "-Infinity", "NaN", "sNaN"] {
                let special = panproto_inst::value::DecimalValue::parse(lexeme).expect("special");
                let error = emit_tsv_with_schema(
                    &decimal,
                    &table_with(Value::Decimal(special)),
                    "cassandra",
                    "rows",
                    delimiter,
                )
                .expect_err("JSON numerals have no special decimal");
                assert!(matches!(
                    error,
                    EmitInstanceError::Value(ref error) if error.reason == EmitValueReason::NonFinite
                ));
            }
            for (schema, cell) in [
                (&double, "Infinity"),
                (&double, "NaN"),
                (&double, "+1"),
                (&decimal, "Infinity"),
                (&decimal, ".5"),
            ] {
                let input = format!("value\n{cell}\n");
                assert!(
                    matches!(
                        parse_tsv(
                            schema,
                            input.as_bytes(),
                            "cassandra",
                            "rows",
                            delimiter,
                            None
                        ),
                        Err(ParseInstanceError::Fidelity(_))
                    ),
                    "{cell} is not a JSON numeral"
                );
            }
        }
    }

    #[test]
    fn a_leading_byte_order_mark_is_read_past_and_never_written() {
        let schema = typed_table_schema();
        for (delimiter, text) in [(b'\t', b"amount\n1\n".as_slice()), (b',', b"amount\n1\n")] {
            let mut input = vec![0xEF, 0xBB, 0xBF];
            input.extend_from_slice(text);
            let parsed = parse_tsv(&schema, &input, "sql", "rows", delimiter, None)
                .expect("a leading byte order mark is an encoding signature");
            let rows = parsed.rows("rows");
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0].1.get("amount"), Some(&Value::Int(1)));
            assert_eq!(
                emit_tsv_with_schema(&schema, &parsed, "sql", "rows", delimiter).expect("emit"),
                text
            );
            // Offsets count from byte 0 of the input, the mark included.
            let mut malformed = vec![0xEF, 0xBB, 0xBF];
            malformed.extend_from_slice(b"amount\nx\n");
            assert!(matches!(
                parse_tsv(&schema, &malformed, "sql", "rows", delimiter, None),
                Err(ParseInstanceError::Fidelity(ref errors))
                    if errors.len() == 1 && errors[0].site.byte_offset == Some(10)
            ));
        }
        let mut two_columns = vec![0xEF, 0xBB, 0xBF];
        two_columns.extend_from_slice(b"a\tb\n1\t2\n");
        let parsed = parse_tsv(&Schema::default(), &two_columns, "tsv", "rows", b'\t', None)
            .expect("two-column table after a mark");
        let mut names: Vec<_> = parsed.rows("rows")[0].1.keys().cloned().collect();
        names.sort();
        assert_eq!(names, ["a", "b"]);
        // A mark after the start is content of the first cell.
        let inner = b"amount\n\xEF\xBB\xBF1\n";
        assert!(parse_tsv(&schema, inner, "sql", "rows", b'\t', None).is_err());
    }

    #[test]
    fn explicit_comment_mode_keeps_only_noncomment_data_lines() {
        assert_eq!(
            tabular_data_lines(b"\nname\n# skipped\nkept\n", Some(b'#')),
            vec![b"name".as_slice(), b"kept".as_slice()]
        );
    }

    /// An instance without the table emits as one whose table has no rows.
    #[test]
    fn an_absent_table_emits_no_rows() {
        let empty = FInstance::new();
        let emitted = emit_tsv(&empty, "tsv", "rows", b'\t').expect("emits");
        assert_eq!(emitted, b"");
        assert_eq!(
            parse_tsv(&Schema::default(), &emitted, "tsv", "rows", b'\t', None,)
                .expect("reparse empty canonical table"),
            empty
        );
        assert!(emit_conllu(&empty, "conllu").is_ok());
    }

    #[test]
    fn duplicate_headers_are_refused_before_cells_can_collapse() {
        let error = parse_tsv(&Schema::default(), b"x,x\na,b\n", "csv", "rows", b',', None)
            .expect_err("duplicate headers must be refused");
        assert!(matches!(
            error,
            ParseInstanceError::Parse { ref message, .. }
                if message == "duplicate header name 'x'"
        ));
    }

    #[test]
    fn rows_wider_than_the_header_are_refused_before_cells_can_collapse() {
        let error = parse_tsv(
            &Schema::default(),
            b"col_1\nleft,right\n",
            "csv",
            "rows",
            b',',
            None,
        )
        .expect_err("a synthesized column name must not overwrite a header cell");
        assert!(matches!(
            error,
            ParseInstanceError::Parse { ref message, .. }
                if message.contains("data row 1")
                    && message.contains("2 fields")
                    && message.contains("1 header")
        ));
    }

    #[test]
    fn canonical_csv_decodes_strict_doubled_quote_fields() {
        for (input, expected) in [
            (b"value\n\"\"\n".as_slice(), ""),
            (b"value\n\"a,b\"\n".as_slice(), "a,b"),
            (b"value\n\"a\"\"b\"\n".as_slice(), "a\"b"),
            (b"value\n\"a\nb\"\n".as_slice(), "a\nb"),
            (b"value\n\"a\r\nb\"\n".as_slice(), "a\r\nb"),
        ] {
            let parsed = parse_tsv(&Schema::default(), input, "csv", "rows", b',', None)
                .expect("valid quoted CSV field");
            assert_eq!(
                parsed
                    .rows("rows")
                    .first()
                    .and_then(|(_, row)| row.get("value")),
                Some(&Value::Str(expected.to_owned())),
                "input={input:?}"
            );
        }

        let parsed = parse_tsv(
            &Schema::default(),
            b"left,right\n\"x,y\",z\n",
            "csv",
            "rows",
            b',',
            None,
        )
        .expect("quoted delimiter stays in one field");
        let (_, row) = parsed.rows("rows")[0];
        assert_eq!(row.get("left"), Some(&Value::Str("x,y".to_owned())));
        assert_eq!(row.get("right"), Some(&Value::Str("z".to_owned())));
    }

    #[test]
    fn canonical_csv_refuses_malformed_quotes_and_wrong_field_counts() {
        for input in [
            b"value\n\"abc\n".as_slice(),
            b"value\na\"b\n".as_slice(),
            b"value\n\"a\"x\n".as_slice(),
            b"left,right\n\"x,y\"\n".as_slice(),
            b"left,right\nonly-left\n".as_slice(),
        ] {
            assert!(matches!(
                parse_tsv(&Schema::default(), input, "csv", "rows", b',', None),
                Err(ParseInstanceError::Parse { .. })
            ));
        }
    }

    #[test]
    fn canonical_delimited_parse_refuses_line_delimiters_as_field_delimiters() {
        assert!(matches!(
            parse_tsv(
                &Schema::default(),
                b"a\nb\n",
                "tabular",
                "rows",
                b'\n',
                None,
            ),
            Err(ParseInstanceError::Parse { ref message, .. })
                if message.contains("line delimiter")
        ));
        assert!(matches!(
            emit_tsv(&table_with(Value::Str("value".into())), "tabular", "rows", b'\n'),
            Err(EmitInstanceError::Emit { message, .. })
                if message.contains("line delimiter")
        ));
    }

    #[test]
    fn the_tsv_pathway_refuses_a_labelled_null() {
        let instance = table_with(Value::LabeledNull(7));
        let error = emit_tsv(&instance, "tsv", "rows", b'\t').expect_err("must refuse");
        assert!(matches!(
            error,
            EmitInstanceError::LabeledNull { ref column, .. } if column == "value"
        ));
    }

    #[test]
    fn direct_tsv_emitters_refuse_unselected_tables_pairs_and_row_renumbering() {
        let instance = FInstance::new()
            .with_table(
                "unrelated",
                vec![HashMap::from([("first".to_owned(), Value::LabeledNull(7))])],
            )
            .expect("unrelated table")
            .with_table(
                "rows",
                vec![HashMap::from([(
                    "value".to_owned(),
                    Value::Str("kept".into()),
                )])],
            )
            .expect("selected table");

        for result in [
            emit_tsv(&instance, "tsv", "rows", b'\t'),
            emit_tsv_with_schema(&Schema::default(), &instance, "tsv", "rows", b'\t'),
        ] {
            assert!(matches!(
                result,
                Err(EmitInstanceError::UnsupportedStructuralEdit { .. })
            ));
        }

        let renumbered = FInstance::new()
            .with_rows(
                "rows",
                [(
                    RowId(7),
                    HashMap::from([("value".into(), Value::Str("kept".into()))]),
                )],
            )
            .expect("renumbered row");
        assert!(matches!(
            emit_tsv(&renumbered, "tsv", "rows", b'\t'),
            Err(EmitInstanceError::UnsupportedStructuralEdit { .. })
        ));

        let relation = panproto_schema::Edge {
            src: "rows".into(),
            tgt: "rows".into(),
            kind: "link".into(),
            name: Some("next".into()),
        };
        let related = FInstance::new()
            .with_rows(
                "rows",
                [
                    (
                        RowId(0),
                        HashMap::from([("value".into(), Value::Str("a".into()))]),
                    ),
                    (
                        RowId(1),
                        HashMap::from([("value".into(), Value::Str("b".into()))]),
                    ),
                ],
            )
            .expect("rows")
            .with_foreign_key(relation, [(RowId(0), RowId(1))])
            .expect("pair");
        assert!(matches!(
            emit_tsv(&related, "tsv", "rows", b'\t'),
            Err(EmitInstanceError::UnsupportedStructuralEdit { .. })
        ));
    }

    #[test]
    fn the_conllu_pathway_refuses_a_labelled_null() {
        let instance = FInstance::new()
            .with_table(
                "token",
                vec![HashMap::from([("FORM".to_owned(), Value::LabeledNull(11))])],
            )
            .expect("token table");
        let error = emit_conllu(&instance, "conllu").expect_err("must refuse");
        assert!(matches!(
            error,
            EmitInstanceError::LabeledNull { ref column, .. } if column == "FORM"
        ));
    }

    #[test]
    fn conllu_refuses_token_ids_that_reparse_as_comments() {
        let instance = FInstance::new()
            .with_table(
                "sentence",
                vec![HashMap::from([("sent_idx".to_owned(), Value::Int(0))])],
            )
            .expect("sentence table")
            .with_table(
                "token",
                vec![HashMap::from([
                    ("sent_idx".to_owned(), Value::Int(0)),
                    ("ID".to_owned(), Value::Str("#id".into())),
                ])],
            )
            .expect("token table");

        assert!(matches!(
            emit_conllu(&instance, "conllu"),
            Err(EmitInstanceError::Value(EmitValueError {
                reason: EmitValueReason::NotRepresentable,
                ..
            }))
        ));
    }

    #[test]
    fn nested_labelled_nulls_are_also_refused() {
        let instance = table_with(Value::List(vec![Value::Unknown(HashMap::from([(
            "nested".to_owned(),
            Value::LabeledNull(19),
        )]))]));
        let error = emit_tsv(&instance, "tsv", "rows", b'\t').expect_err("must refuse");
        assert!(matches!(
            error,
            EmitInstanceError::LabeledNull { ref column, .. } if column == "value"
        ));
    }

    #[test]
    fn labelled_null_refusal_uses_row_then_column_order() {
        let instance = FInstance::new()
            .with_table(
                "rows",
                vec![
                    HashMap::from([
                        ("z".to_owned(), Value::LabeledNull(1)),
                        ("a".to_owned(), Value::LabeledNull(2)),
                    ]),
                    HashMap::from([("a".to_owned(), Value::LabeledNull(3))]),
                ],
            )
            .expect("table");
        let first_row = instance.table("rows").expect("rows").ids()[0];
        let error = emit_tsv(&instance, "tsv", "rows", b'\t').expect_err("must refuse");
        assert!(matches!(
            error,
            EmitInstanceError::LabeledNull { row, ref column }
                if row == first_row && column == "a"
        ));
    }

    #[test]
    fn row_delimiters_are_not_emitted_as_strings() {
        for value in [
            Value::Str("two\nrows".into()),
            Value::Str("two\tfields".into()),
        ] {
            let error = emit_tsv(&table_with(value), "tsv", "rows", b'\t')
                .expect_err("ambiguous cell must be refused");
            assert!(matches!(error, EmitInstanceError::Value(_)));
        }
        for text in ["_", "\"wrapped\""] {
            let emitted = emit_tsv(&table_with(Value::Str(text.into())), "tsv", "rows", b'\t')
                .expect("underscore and quote are ordinary TSV text");
            assert!(std::str::from_utf8(&emitted).is_ok_and(|output| output.contains(text)));
        }
    }

    #[test]
    fn reserved_delimiters_are_not_emitted_in_headers() {
        for column in ["two,columns", "two\nrows", "carriage\rreturn", "\"quoted\""] {
            let instance = FInstance::new()
                .with_table(
                    "rows",
                    vec![HashMap::from([(
                        column.to_owned(),
                        Value::Str("kept".into()),
                    )])],
                )
                .expect("table");
            let error = emit_tsv(&instance, "csv", "rows", b',')
                .expect_err("ambiguous header must be refused");
            assert!(matches!(
                error,
                EmitInstanceError::Emit { message, .. }
                    if message.contains("header") && message.contains("reserved")
            ));
        }
    }

    #[test]
    fn bytes_need_a_schema_declared_text_encoding() {
        let instance = table_with(Value::Bytes(vec![0x00, 0x01, 0x02, 0xff]));
        let error = emit_tsv(&instance, "sql", "rows", b'\t')
            .expect_err("schema-less text cannot choose a byte encoding");
        assert!(matches!(
            error,
            EmitInstanceError::Value(EmitValueError {
                reason: EmitValueReason::NotRepresentable,
                ..
            })
        ));

        let schema = typed_bytes_table_schema();
        assert_eq!(
            emit_tsv_with_schema(&schema, &instance, "sql", "rows", b'\t')
                .expect("SQL declares hexadecimal bytes"),
            b"value\n000102ff\n"
        );
    }

    #[test]
    fn schema_less_tabular_emit_refuses_typed_non_string_values() {
        for value in [
            Value::Token("token".into()),
            Value::Int(1),
            Value::Float(1.0),
            Value::Bool(true),
        ] {
            let error = emit_tsv(&table_with(value), "csv", "rows", b',')
                .expect_err("schema-less text has no inverse for the value variant");
            assert!(matches!(
                error,
                EmitInstanceError::Value(EmitValueError {
                    reason: EmitValueReason::NotRepresentable,
                    ..
                })
            ));
        }
    }

    #[test]
    fn tsv_cells_are_coerced_by_the_column_value_spec() {
        let schema = typed_table_schema();
        let instance = parse_tsv(&schema, b"amount\n42\n", "sql", "rows", b'\t', None)
            .expect("parse typed cell");
        assert!(matches!(
            instance.rows("rows")[0].1.get("amount"),
            Some(Value::Int(42))
        ));

        let error = parse_tsv(
            &schema,
            b"amount\n999999999999999999999999\n",
            "sql",
            "rows",
            b'\t',
            None,
        )
        .expect_err("out-of-range integer must be a fidelity error");
        assert!(matches!(error, ParseInstanceError::Fidelity(errors) if errors.len() == 1));
    }

    fn typed_bytes_table_schema() -> Schema {
        let protocol = crate::json_pathway::protocol_definition("sql");
        panproto_schema::SchemaBuilder::new(&protocol)
            .vertex("rows", "table", None)
            .expect("table vertex")
            .vertex("value", "bytes", None)
            .expect("bytes vertex")
            .edge("rows", "value", "prop", Some("value"))
            .expect("column edge")
            .build()
            .expect("schema")
    }

    #[test]
    fn split_lines_basic() {
        let input = b"line1\nline2\nline3";
        let lines = split_lines(input);
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0], b"line1");
        assert_eq!(lines[2], b"line3");
    }

    #[test]
    fn split_lines_crlf() {
        let input = b"line1\r\nline2\r\n";
        let lines = split_lines(input);
        assert_eq!(lines[0], b"line1");
        assert_eq!(lines[1], b"line2");
    }

    #[test]
    fn split_fields_tab() {
        let line = b"a\tb\tc";
        let fields = split_fields(line, b'\t');
        assert_eq!(fields.len(), 3);
        assert_eq!(fields[1], b"b");
    }

    #[test]
    fn parse_conllu_basic() {
        let input = b"\
# sent_id = test-01
# text = The cat sat.
1\tThe\tthe\tDET\tDT\tDefinite=Def\t2\tdet\t_\t_
2\tcat\tcat\tNOUN\tNN\tNumber=Sing\t3\tnsubj\t_\t_
3\tsat\tsit\tVERB\tVBD\tMood=Ind\t0\troot\t_\tSpaceAfter=No

";
        let schema = panproto_schema::Schema {
            protocol: "conllu".into(),
            vertices: HashMap::new(),
            edges: HashMap::new(),
            hyper_edges: HashMap::new(),
            constraints: HashMap::new(),
            required: HashMap::new(),
            nsids: HashMap::new(),
            entries: Vec::new(),
            variants: HashMap::new(),
            orderings: HashMap::new(),
            recursion_points: HashMap::new(),
            spans: HashMap::new(),
            usage_modes: HashMap::new(),
            nominal: HashMap::new(),
            coercions: HashMap::new(),
            mergers: HashMap::new(),
            defaults: HashMap::new(),
            policies: HashMap::new(),
            outgoing: HashMap::new(),
            incoming: HashMap::new(),
            between: HashMap::new(),
            ..Default::default()
        };

        let instance = parse_conllu(&schema, input, "conllu").expect("parse");
        assert_eq!(instance.row_count("token"), 3, "should have 3 tokens");
        assert_eq!(instance.row_count("sentence"), 1, "should have 1 sentence");
    }

    #[test]
    fn conllu_roundtrip() {
        let input = b"\
# sent_id = s1
# text = Hello world
1\tHello\thello\tINTJ\tUH\t_\t0\troot\t_\t_
2\tworld\tworld\tNOUN\tNN\t_\t1\tvocative\t_\t_

";
        let schema = panproto_schema::Schema {
            protocol: "conllu".into(),
            vertices: HashMap::new(),
            edges: HashMap::new(),
            hyper_edges: HashMap::new(),
            constraints: HashMap::new(),
            required: HashMap::new(),
            nsids: HashMap::new(),
            entries: Vec::new(),
            variants: HashMap::new(),
            orderings: HashMap::new(),
            recursion_points: HashMap::new(),
            spans: HashMap::new(),
            usage_modes: HashMap::new(),
            nominal: HashMap::new(),
            coercions: HashMap::new(),
            mergers: HashMap::new(),
            defaults: HashMap::new(),
            policies: HashMap::new(),
            outgoing: HashMap::new(),
            incoming: HashMap::new(),
            between: HashMap::new(),
            ..Default::default()
        };

        let instance = parse_conllu(&schema, input, "conllu").expect("parse");
        let emitted = emit_conllu(&instance, "conllu").expect("emit");
        let instance2 = parse_conllu(&schema, &emitted, "conllu").expect("re-parse");

        let t1 = instance.rows("token");
        let t2 = instance2.rows("token");
        assert_eq!(
            t1.len(),
            t2.len(),
            "token count should match after round-trip"
        );
    }
}
