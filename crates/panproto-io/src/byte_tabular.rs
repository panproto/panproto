//! Byte-faithful codec for delimited line-oriented formats.
//!
//! Handles RESP/redis, SWIFT MT, and EDI X12: delimited line syntaxes with no
//! tree-sitter grammar.
//!
//! The legacy [`TabularCodec`](crate::tabular_codec::TabularCodec) routes these
//! formats through a `HashMap`-backed `FInstance`, which reorders columns,
//! assumes a header row, drops `_` sentinels, and canonicalizes line endings on
//! emit. None of those formats actually have a header, and all of them lose
//! their original bytes on a `parse → emit` round-trip.
//!
//! This codec mirrors the CST-complement strategy used for CSV/TSV
//! (`extract_tabular_cst`/`inject_tabular_cst`): parsing records the *exact*
//! original layout (every line's raw bytes, its split into fields, the delimiter
//! and line-ending bytes) as a `ByteTabularComplement`. Emission replays the
//! recorded layout verbatim, splicing in only the field values that the instance
//! actually changed. An unmodified round-trip is therefore byte-identical, and a
//! single-cell edit re-emits exactly that cell while leaving the rest of the file
//! untouched.
//!
//! The instance view is a single `FInstance` table whose rows are the file's
//! content lines (in order), each addressed positionally by `col_0`, `col_1`,
//! ... This keeps the functor semantics the registry exposes while imposing no
//! header/column-name assumption the formats do not have.
//!
//! ## Prop1 waiver list
//!
//! - A scalar whose written text would exceed the default 64 MiB output-byte
//!   allowance is refused with a typed limit error. The byte-faithful route
//!   otherwise writes only cell values already represented by source bytes.

use std::collections::HashMap;

use panproto_inst::value::Value;
use panproto_inst::{FInstance, RowId, ValueSite, WInstance};
use panproto_schema::Schema;

use crate::error::{EmitInstanceError, EmitValueError, EmitValueReason, ParseInstanceError};
use crate::traits::{InstanceEmitter, InstanceParser, NativeRepr};

/// Positional cell-name prefix used in the `FInstance` rows.
const COL_PREFIX: &str = "col_";

/// One physical line of the original input, recorded for byte-faithful replay.
#[derive(Debug, Clone)]
struct LineRecord {
    /// Whether this line carries data fields (and so contributes a row to the
    /// `FInstance`) or is preserved verbatim (blank line, comment line).
    is_data: bool,
    /// The field byte-slices for a data line, in order. Empty for non-data.
    fields: Vec<Vec<u8>>,
    /// The line-ending bytes that followed this line in the original input
    /// (`b"\n"`, `b"\r\n"`, or empty for a final line with no trailing newline).
    line_ending: Vec<u8>,
    /// The exact original bytes of the line *content* (no line ending). Used to
    /// replay non-data lines and unchanged data lines verbatim.
    raw: Vec<u8>,
}

/// The byte-faithful complement for a delimited line-oriented file.
///
/// Records the full original layout so [`emit`](ByteTabularCodec::emit) can
/// reproduce the input exactly, splicing only changed cell values.
#[derive(Debug, Clone)]
pub struct ByteTabularComplement {
    lines: Vec<LineRecord>,
    delimiter: u8,
    protocol: String,
    table_vertex: String,
    comment_prefix: Option<u8>,
}

/// A byte-faithful codec for a single delimiter character.
///
/// Unlike [`TabularCodec`](crate::tabular_codec::TabularCodec), this codec makes
/// no header assumption, preserves comment and blank lines, preserves line
/// endings and the presence/absence of a trailing newline, and never reorders
/// fields.
pub struct ByteTabularCodec {
    protocol: String,
    table_vertex: String,
    delimiter: u8,
    comment_prefix: Option<u8>,
}

impl ByteTabularCodec {
    /// Create a codec with a custom single-byte delimiter.
    ///
    /// `comment_prefix`, if set, marks lines that are preserved verbatim and
    /// not surfaced as data rows.
    #[must_use]
    pub fn new(
        protocol: impl Into<String>,
        table_vertex: impl Into<String>,
        delimiter: u8,
        comment_prefix: Option<u8>,
    ) -> Self {
        Self {
            protocol: protocol.into(),
            table_vertex: table_vertex.into(),
            delimiter,
            comment_prefix,
        }
    }

    /// Parse raw bytes into an `FInstance` plus the byte-faithful complement.
    ///
    /// The instance's `table_vertex` table has one row per content line, with
    /// cells addressed positionally (`col_0`, `col_1`, ...). The complement
    /// records the exact original layout for replay.
    ///
    /// # Errors
    ///
    /// This parse is total over byte input and does not currently fail; the
    /// `Result` is retained for signature symmetry with the other codecs.
    pub fn parse(
        &self,
        input: &[u8],
    ) -> Result<(FInstance, ByteTabularComplement), ParseInstanceError> {
        if matches!(self.delimiter, b'\n' | b'\r') {
            return Err(ParseInstanceError::Parse {
                protocol: self.protocol.clone(),
                message: "a line delimiter cannot also be the field delimiter".to_owned(),
            });
        }
        let lines = split_lines_with_endings(input);
        let mut records = Vec::with_capacity(lines.len());
        let mut rows: Vec<HashMap<String, Value>> = Vec::new();

        for (content, ending) in lines {
            let is_comment = self
                .comment_prefix
                .is_some_and(|prefix| content.first() == Some(&prefix));
            // A blank line carries no data and is preserved verbatim.
            let is_data = !is_comment && !content.is_empty();

            let fields = if is_data {
                split_fields(content, self.delimiter)
            } else {
                Vec::new()
            };

            if is_data {
                let mut row = HashMap::with_capacity(fields.len());
                for (i, field) in fields.iter().enumerate() {
                    row.insert(format!("{COL_PREFIX}{i}"), cell_value(field));
                }
                rows.push(row);
            }

            records.push(LineRecord {
                is_data,
                fields: fields.iter().map(|f| f.to_vec()).collect(),
                line_ending: ending.to_vec(),
                raw: content.to_vec(),
            });
        }

        let instance = FInstance::new()
            .with_table(self.table_vertex.as_str(), rows)
            .map_err(|e| ParseInstanceError::Parse {
                protocol: self.protocol.clone(),
                message: e.to_string(),
            })?;
        let complement = ByteTabularComplement {
            lines: records,
            delimiter: self.delimiter,
            protocol: self.protocol.clone(),
            table_vertex: self.table_vertex.clone(),
            comment_prefix: self.comment_prefix,
        };
        Ok((instance, complement))
    }

    /// Emit bytes by replaying the complement.
    ///
    /// Any cell values the instance changed relative to the parsed original are
    /// spliced in; everything else is reproduced verbatim.
    ///
    /// Lines whose cells are all unchanged (and all non-data lines) are emitted
    /// from their recorded raw bytes, so an unmodified round-trip is
    /// byte-identical. A changed cell rebuilds that line by joining the row's
    /// fields with the recorded delimiter.
    ///
    /// # Errors
    ///
    /// Returns a structured error when rows, row identities, or columns no
    /// longer match the preserved source layout, or when an edited value has
    /// no faithful single-field representation.
    pub fn emit(
        &self,
        instance: &FInstance,
        complement: &ByteTabularComplement,
    ) -> Result<Vec<u8>, EmitInstanceError> {
        if matches!(self.delimiter, b'\n' | b'\r') {
            return Err(EmitInstanceError::Emit {
                protocol: self.protocol.clone(),
                message: "a line delimiter cannot also be the field delimiter".to_owned(),
            });
        }
        if complement.delimiter != self.delimiter
            || complement.protocol != self.protocol
            || complement.table_vertex != self.table_vertex
            || complement.comment_prefix != self.comment_prefix
        {
            return Err(EmitInstanceError::UnsupportedStructuralEdit {
                format: "byte-tabular".to_owned(),
                detail: "the complement belongs to a different codec configuration".to_owned(),
            });
        }
        let rows = instance.rows(&self.table_vertex);
        validate_preserved_structure(instance, &rows, complement, &self.table_vertex)?;
        reject_labeled_nulls(&rows)?;

        let mut output = Vec::with_capacity(complement.lines.len() * 16);
        let mut data_idx = 0usize;

        for line in &complement.lines {
            if !line.is_data {
                output.extend_from_slice(&line.raw);
                output.extend_from_slice(&line.line_ending);
                continue;
            }

            // Pair this data line with the next instance row in row-id order.
            let row = rows.get(data_idx).copied();
            data_idx += 1;

            let updated = match row {
                Some((row_id, row)) => {
                    splice_line(line, row_id, row, complement.delimiter, self.comment_prefix)?
                }
                None => None,
            };
            match updated {
                Some(bytes) => output.extend_from_slice(&bytes),
                None => output.extend_from_slice(&line.raw),
            }
            output.extend_from_slice(&line.line_ending);
        }

        Ok(output)
    }
}

fn validate_preserved_structure(
    instance: &FInstance,
    rows: &[(RowId, &HashMap<String, Value>)],
    complement: &ByteTabularComplement,
    table_vertex: &str,
) -> Result<(), EmitInstanceError> {
    let data_lines: Vec<_> = complement
        .lines
        .iter()
        .filter(|line| line.is_data)
        .collect();
    let expected_tables: Vec<&str> = if data_lines.is_empty() {
        Vec::new()
    } else {
        vec![table_vertex]
    };
    let actual_tables: Vec<_> = instance
        .table_names()
        .into_iter()
        .map(AsRef::as_ref)
        .collect();
    if actual_tables != expected_tables {
        return Err(EmitInstanceError::UnsupportedStructuralEdit {
            format: "byte-tabular".to_owned(),
            detail: "the set of non-empty tables changed".to_owned(),
        });
    }
    if instance.pair_count() != 0 {
        return Err(EmitInstanceError::UnsupportedStructuralEdit {
            format: "byte-tabular".to_owned(),
            detail: "foreign-key pairs cannot be written to a delimited source".to_owned(),
        });
    }
    if rows.len() != data_lines.len() {
        return Err(EmitInstanceError::UnsupportedStructuralEdit {
            format: "byte-tabular".to_owned(),
            detail: format!(
                "data row count changed from {} to {}",
                data_lines.len(),
                rows.len()
            ),
        });
    }
    for (index, ((row_id, row), line)) in rows.iter().zip(data_lines).enumerate() {
        let expected_id = u32::try_from(index).map(RowId).map_err(|_| {
            EmitInstanceError::UnsupportedStructuralEdit {
                format: "byte-tabular".to_owned(),
                detail: "source has more rows than row ids can address".to_owned(),
            }
        })?;
        if *row_id != expected_id {
            return Err(EmitInstanceError::UnsupportedStructuralEdit {
                format: "byte-tabular".to_owned(),
                detail: format!(
                    "row at source position {index} has id {} rather than {}",
                    row_id.0, expected_id.0
                ),
            });
        }
        for column in row.keys() {
            let supported = column
                .strip_prefix(COL_PREFIX)
                .and_then(|suffix| suffix.parse::<usize>().ok())
                .is_some_and(|column_index| {
                    column == &format!("{COL_PREFIX}{column_index}")
                        && column_index < line.fields.len()
                });
            if !supported {
                return Err(EmitInstanceError::UnsupportedStructuralEdit {
                    format: "byte-tabular".to_owned(),
                    detail: format!("row {} added unaddressed column '{column}'", row_id.0),
                });
            }
        }
    }
    Ok(())
}

/// Rebuild a data line from a possibly-edited instance row, or return `None`
/// when every cell is byte-identical to the recorded original (so the caller
/// can replay the raw bytes verbatim).
fn splice_line(
    line: &LineRecord,
    row_id: RowId,
    row: &HashMap<String, Value>,
    delimiter: u8,
    comment_prefix: Option<u8>,
) -> Result<Option<Vec<u8>>, EmitInstanceError> {
    let mut changed = false;
    let mut out_fields: Vec<Vec<u8>> = Vec::with_capacity(line.fields.len());

    for (i, original) in line.fields.iter().enumerate() {
        let key = format!("{COL_PREFIX}{i}");
        match row.get(&key) {
            Some(value) => {
                if value_matches_original_bytes(value, original) {
                    out_fields.push(original.clone());
                    continue;
                }
                let bytes = value_bytes(value, row_id, &key, delimiter)?;
                if &bytes != original {
                    changed = true;
                }
                out_fields.push(bytes);
            }
            None => {
                return Err(EmitValueError::new(
                    ValueSite::new(key.as_str(), None),
                    "absent",
                    "byte-tabular",
                    EmitValueReason::AbsentNotRepresentable,
                )
                .into());
            }
        }
    }

    if !changed {
        return Ok(None);
    }

    let mut bytes = Vec::new();
    for (i, field) in out_fields.iter().enumerate() {
        if i > 0 {
            bytes.push(delimiter);
        }
        bytes.extend_from_slice(field);
    }
    reject_line_reclassified_as_non_data(&bytes, row, comment_prefix)?;
    Ok(Some(bytes))
}

fn reject_line_reclassified_as_non_data(
    bytes: &[u8],
    row: &HashMap<String, Value>,
    comment_prefix: Option<u8>,
) -> Result<(), EmitInstanceError> {
    if !bytes.is_empty() && comment_prefix.is_none_or(|prefix| bytes.first() != Some(&prefix)) {
        return Ok(());
    }

    let value = row.get("col_0");
    Err(EmitValueError::new(
        ValueSite::new("col_0", None),
        value.map_or("absent", Value::type_name),
        "byte-tabular",
        EmitValueReason::NotRepresentable,
    )
    .into())
}

fn value_matches_original_bytes(value: &Value, original: &[u8]) -> bool {
    match value {
        Value::Str(value) => value.as_bytes() == original,
        Value::Bytes(value) => std::str::from_utf8(original).is_err() && value == original,
        _ => false,
    }
}

/// Surface one field's bytes as a [`Value`].
///
/// A field that is valid UTF-8 becomes a [`Value::Str`], which is what
/// consumers of the `FInstance` want to read and edit. A field that is not
/// becomes a [`Value::Bytes`], which [`value_bytes`] replays verbatim: EDI
/// X12 and SWIFT MT payloads are latin-1, and decoding them lossily would
/// substitute `U+FFFD` for every non-ASCII byte and destroy the file on an
/// unmodified round-trip.
fn cell_value(field: &[u8]) -> Value {
    std::str::from_utf8(field).map_or_else(
        |_| Value::Bytes(field.to_vec()),
        |s| Value::Str(s.to_owned()),
    )
}

/// Render a [`Value`] to its byte form for splicing into a delimited line.
fn value_bytes(
    value: &Value,
    row: RowId,
    column: &str,
    delimiter: u8,
) -> Result<Vec<u8>, EmitInstanceError> {
    let site = || ValueSite::new(column, None);
    let bytes = match value {
        Value::Str(s) => Ok(s.clone().into_bytes()),
        Value::Float(number) if !number.is_finite() => Err(EmitValueError::new(
            site(),
            value.type_name(),
            "byte-tabular",
            EmitValueReason::NonFinite,
        )
        .into()),
        Value::Bytes(bytes) if std::str::from_utf8(bytes).is_err() => Ok(bytes.clone()),
        Value::Null => Err(EmitValueError::new(
            site(),
            value.type_name(),
            "byte-tabular",
            EmitValueReason::NullNotRepresentable,
        )
        .into()),
        Value::LabeledNull(_) => Err(EmitInstanceError::LabeledNull {
            row,
            column: column.to_owned(),
        }),
        other => Err(EmitValueError::new(
            site(),
            other.type_name(),
            "byte-tabular",
            EmitValueReason::NotRepresentable,
        )
        .into()),
    }?;
    if bytes.contains(&delimiter) || bytes.contains(&b'\r') || bytes.contains(&b'\n') {
        return Err(EmitValueError::new(
            site(),
            value.type_name(),
            "byte-tabular",
            EmitValueReason::NotRepresentable,
        )
        .into());
    }
    Ok(bytes)
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

fn reject_labeled_nulls(
    rows: &[(RowId, &HashMap<String, Value>)],
) -> Result<(), EmitInstanceError> {
    for (row_id, row) in rows {
        let mut columns: Vec<_> = row.keys().collect();
        columns.sort();
        for column in columns {
            if row.get(column).is_some_and(contains_labeled_null) {
                return Err(EmitInstanceError::LabeledNull {
                    row: *row_id,
                    column: column.clone(),
                });
            }
        }
    }
    Ok(())
}

fn canonical_positional_fields(
    row_id: RowId,
    row: &HashMap<String, Value>,
) -> Result<Vec<&Value>, EmitInstanceError> {
    let mut indexed = Vec::with_capacity(row.len());
    for (column, value) in row {
        let Some(index) = column
            .strip_prefix(COL_PREFIX)
            .and_then(|suffix| suffix.parse::<usize>().ok())
            .filter(|index| column == &format!("{COL_PREFIX}{index}"))
        else {
            return Err(EmitInstanceError::UnsupportedStructuralEdit {
                format: "byte-tabular".to_owned(),
                detail: format!("row {} has non-positional column '{column}'", row_id.0),
            });
        };
        indexed.push((index, value));
    }
    indexed.sort_unstable_by_key(|(index, _)| *index);

    let expected_len = indexed.len().max(1);
    for expected in 0..expected_len {
        if indexed
            .get(expected)
            .is_none_or(|(actual, _)| *actual != expected)
        {
            let column = format!("{COL_PREFIX}{expected}");
            return Err(EmitValueError::new(
                ValueSite::new(column.as_str(), None),
                "absent",
                "byte-tabular",
                EmitValueReason::AbsentNotRepresentable,
            )
            .into());
        }
    }
    Ok(indexed.into_iter().map(|(_, value)| value).collect())
}

/// Split input into `(content, line_ending)` pairs.
///
/// `content` excludes the line ending; `line_ending` is the exact bytes
/// (`\n`, `\r\n`, or empty for a final line with no trailing newline). A
/// trailing newline therefore yields no spurious empty final record, and its
/// absence is faithfully recorded.
fn split_lines_with_endings(input: &[u8]) -> Vec<(&[u8], &[u8])> {
    let mut out = Vec::new();
    let mut start = 0usize;
    let mut i = 0usize;
    while i < input.len() {
        if input[i] == b'\n' {
            let (content_end, ending_start) = if i > start && input[i - 1] == b'\r' {
                (i - 1, i - 1)
            } else {
                (i, i)
            };
            out.push((&input[start..content_end], &input[ending_start..=i]));
            start = i + 1;
        }
        i += 1;
    }
    if start < input.len() {
        // Final line with no trailing newline.
        out.push((&input[start..], &input[input.len()..]));
    }
    out
}

/// Split a line into fields by `delimiter`. Exact inverse of joining with the
/// same delimiter (an N-field line has N-1 delimiters).
fn split_fields(line: &[u8], delimiter: u8) -> Vec<&[u8]> {
    let mut fields = Vec::new();
    let mut start = 0usize;
    for (i, &b) in line.iter().enumerate() {
        if b == delimiter {
            fields.push(&line[start..i]);
            start = i + 1;
        }
    }
    fields.push(&line[start..]);
    fields
}

impl InstanceParser for ByteTabularCodec {
    fn protocol_name(&self) -> &str {
        &self.protocol
    }

    fn native_repr(&self) -> NativeRepr {
        NativeRepr::Table
    }

    fn parse_wtype(
        &self,
        _schema: &Schema,
        _input: &[u8],
    ) -> Result<WInstance, ParseInstanceError> {
        Err(ParseInstanceError::UnsupportedRepresentation {
            protocol: self.protocol.clone(),
            requested: NativeRepr::Tree,
            native: NativeRepr::Table,
        })
    }

    fn parse_functor(
        &self,
        _schema: &Schema,
        input: &[u8],
    ) -> Result<FInstance, ParseInstanceError> {
        let (instance, _complement) = self.parse(input)?;
        Ok(instance)
    }
}

impl InstanceEmitter for ByteTabularCodec {
    fn protocol_name(&self) -> &str {
        &self.protocol
    }

    fn emit_wtype(
        &self,
        _schema: &Schema,
        _instance: &WInstance,
    ) -> Result<Vec<u8>, EmitInstanceError> {
        Err(EmitInstanceError::UnsupportedRepresentation {
            protocol: self.protocol.clone(),
            requested: NativeRepr::Tree,
            native: NativeRepr::Table,
        })
    }

    fn emit_functor(
        &self,
        _schema: &Schema,
        instance: &FInstance,
    ) -> Result<Vec<u8>, EmitInstanceError> {
        if matches!(self.delimiter, b'\n' | b'\r') {
            return Err(EmitInstanceError::Emit {
                protocol: self.protocol.clone(),
                message: "a line delimiter cannot also be the field delimiter".to_owned(),
            });
        }
        // Without a complement there is nothing to replay; rebuild a canonical
        // delimited file from the positional cells. This is the non-preserving
        // fallback used by the generic `InstanceEmitter` seam; the byte-faithful
        // path is `emit` with a complement.
        crate::tabular_pathway::validate_canonical_tabular_structure(
            instance,
            &self.table_vertex,
            "byte-tabular",
        )?;
        let rows = instance.rows(&self.table_vertex);
        reject_labeled_nulls(&rows)?;
        let mut output = Vec::new();
        for (row_id, row) in rows {
            let mut line = Vec::new();
            for (i, value) in canonical_positional_fields(row_id, row)?
                .into_iter()
                .enumerate()
            {
                let key = format!("{COL_PREFIX}{i}");
                if i > 0 {
                    line.push(self.delimiter);
                }
                line.extend_from_slice(&value_bytes(value, row_id, &key, self.delimiter)?);
            }
            reject_line_reclassified_as_non_data(&line, row, self.comment_prefix)?;
            output.extend_from_slice(&line);
            output.push(b'\n');
        }
        Ok(output)
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    fn codec() -> ByteTabularCodec {
        ByteTabularCodec::new("redis", "entries", b' ', None)
    }

    #[test]
    fn round_trip_is_byte_identical() {
        let input = b"key user:1001\nname Alice Chen\nscore 94.5\n";
        let c = codec();
        let (inst, comp) = c.parse(input).unwrap();
        let out = c.emit(&inst, &comp).unwrap();
        assert_eq!(out, input);
    }

    #[test]
    fn a_complement_cannot_be_replayed_through_another_codec_configuration() {
        let source = ByteTabularCodec::new("source", "rows", b'|', Some(b'#'));
        let target = ByteTabularCodec::new("target", "rows", b',', None);
        let (instance, complement) = source.parse(b"left|right\n").unwrap();

        assert!(matches!(
            target.emit(&instance, &complement),
            Err(EmitInstanceError::UnsupportedStructuralEdit {
                ref format,
                ..
            }) if format == "byte-tabular"
        ));
    }

    #[test]
    fn a_literal_underscore_is_not_absence() {
        let c = codec();
        let (mut instance, complement) = c.parse(b"key value\n").unwrap();
        let row = instance.table("entries").unwrap().ids()[0];
        instance
            .row_mut(row)
            .unwrap()
            .insert("col_1".into(), Value::Str("_".into()));
        let emitted = c.emit(&instance, &complement).unwrap();
        assert_eq!(emitted, b"key _\n");
        let (reparsed, _) = c.parse(&emitted).unwrap();
        assert_eq!(
            reparsed.rows("entries")[0].1.get("col_1"),
            Some(&Value::Str("_".into()))
        );
    }

    #[test]
    fn a_line_delimiter_cannot_also_split_fields() {
        let codec = ByteTabularCodec::new("redis", "entries", b'\n', None);
        assert!(matches!(
            codec.parse(b"left\nright\n"),
            Err(ParseInstanceError::Parse { ref message, .. })
                if message.contains("line delimiter")
        ));
        let instance = FInstance::new()
            .with_table(
                "entries",
                vec![HashMap::from([
                    ("col_0".to_owned(), Value::Str("left".into())),
                    ("col_1".to_owned(), Value::Str("right".into())),
                ])],
            )
            .expect("table");
        assert!(matches!(
            codec.emit_functor(&Schema::default(), &instance),
            Err(EmitInstanceError::Emit { message, .. })
                if message.contains("line delimiter")
        ));
    }

    #[test]
    fn no_trailing_newline_preserved() {
        let input = b"key user:1001\nname Alice";
        let c = codec();
        let (inst, comp) = c.parse(input).unwrap();
        let out = c.emit(&inst, &comp).unwrap();
        assert_eq!(out, input);
    }

    /// A file with no data line parses to an instance with no rows, and
    /// replays verbatim: a table without rows is no table, and emitting it
    /// emits no rows.
    #[test]
    fn a_file_without_data_lines_round_trips() {
        let input = b"# only a comment\n\n";
        let c = ByteTabularCodec::new("p", "t", b' ', Some(b'#'));
        let (inst, comp) = c.parse(input).unwrap();
        assert_eq!(inst.total_row_count(), 0);
        assert_eq!(c.emit(&inst, &comp).unwrap(), input);
        assert_eq!(c.emit_functor(&Schema::default(), &inst).unwrap(), b"");
    }

    #[test]
    fn crlf_preserved() {
        let input = b"key user:1001\r\nname Alice\r\n";
        let c = codec();
        let (inst, comp) = c.parse(input).unwrap();
        let out = c.emit(&inst, &comp).unwrap();
        assert_eq!(out, input);
    }

    #[test]
    fn blank_and_comment_lines_preserved() {
        let input = b"# header\nkey v\n\nname Alice\n";
        let c = ByteTabularCodec::new("p", "t", b' ', Some(b'#'));
        let (inst, comp) = c.parse(input).unwrap();
        // Only the two data lines surface as rows.
        assert_eq!(inst.row_count("t"), 2);
        let out = c.emit(&inst, &comp).unwrap();
        assert_eq!(out, input);
    }

    #[test]
    fn edit_rewrites_one_cell() {
        let input = b"key user:1001\nname Alice Chen\n";
        let c = codec();
        let (mut inst, comp) = c.parse(input).unwrap();
        // Edit the value of the first record's col_1 (user:1001 -> user:2002).
        let first = inst.table("entries").unwrap().ids()[0];
        inst.row_mut(first)
            .unwrap()
            .insert("col_1".into(), Value::Str("user:2002".into()));
        let out = c.emit(&inst, &comp).unwrap();
        assert_eq!(out, b"key user:2002\nname Alice Chen\n");
    }

    #[test]
    fn edits_cannot_turn_a_data_row_into_a_comment_or_blank_line() {
        let c = ByteTabularCodec::new("p", "t", b' ', Some(b'#'));
        for replacement in ["# lost", ""] {
            let (mut instance, complement) = c.parse(b"value\n").unwrap();
            instance
                .row_mut(RowId(0))
                .unwrap()
                .insert("col_0".into(), Value::Str(replacement.into()));

            assert!(matches!(
                c.emit(&instance, &complement),
                Err(EmitInstanceError::Value(EmitValueError {
                    reason: EmitValueReason::NotRepresentable,
                    ..
                }))
            ));
        }
    }

    #[test]
    fn a_comment_prefix_outside_the_first_field_round_trips_as_data() {
        let c = ByteTabularCodec::new("p", "t", b' ', Some(b'#'));
        let (mut instance, complement) = c.parse(b"key value\n").unwrap();
        instance
            .row_mut(RowId(0))
            .unwrap()
            .insert("col_1".into(), Value::Str("#kept".into()));

        let emitted = c.emit(&instance, &complement).unwrap();
        let (reparsed, _) = c.parse(&emitted).unwrap();
        assert_eq!(reparsed.row_count("t"), 1);
        assert_eq!(
            reparsed.rows("t")[0].1.get("col_1"),
            Some(&Value::Str("#kept".into()))
        );
    }

    fn assert_structural_refusal(
        codec: &ByteTabularCodec,
        instance: &FInstance,
        complement: &ByteTabularComplement,
    ) {
        assert!(matches!(
            codec.emit(instance, complement),
            Err(EmitInstanceError::UnsupportedStructuralEdit {
                ref format,
                ..
            }) if format == "byte-tabular"
        ));
    }

    #[test]
    fn structural_row_and_column_edits_are_refused_instead_of_replayed() {
        let c = codec();
        let (original, complement) = c.parse(b"first a\nmiddle b\nlast c\n").unwrap();

        for row in [RowId(0), RowId(1), RowId(2)] {
            let mut removed = original.clone();
            assert!(removed.remove_row(row).is_some());
            assert_structural_refusal(&c, &removed, &complement);
        }

        let mut added = original.clone();
        added
            .push_row(
                "entries",
                HashMap::from([("col_0".to_owned(), Value::Str("extra".to_owned()))]),
            )
            .unwrap();
        assert_structural_refusal(&c, &added, &complement);

        let mut replaced_row_identity = original.clone();
        let (_, row, _) = replaced_row_identity.remove_row(RowId(0)).unwrap();
        replaced_row_identity.push_row("entries", row).unwrap();
        assert_structural_refusal(&c, &replaced_row_identity, &complement);

        let mut extra_column = original;
        extra_column
            .row_mut(RowId(1))
            .unwrap()
            .insert("col_2".to_owned(), Value::Str("extra".to_owned()));
        assert_structural_refusal(&c, &extra_column, &complement);
    }

    #[test]
    fn canonical_emission_refuses_a_gap_in_positional_columns() {
        let instance = FInstance::new()
            .with_table(
                "entries",
                vec![HashMap::from([
                    ("col_0".to_owned(), Value::Str("kept".to_owned())),
                    (
                        "col_2".to_owned(),
                        Value::Str("must not disappear".to_owned()),
                    ),
                ])],
            )
            .unwrap();

        assert!(matches!(
            codec().emit_functor(&Schema::default(), &instance),
            Err(EmitInstanceError::Value(EmitValueError {
                reason: EmitValueReason::AbsentNotRepresentable,
                ..
            }))
        ));
    }

    #[test]
    fn non_utf8_cells_round_trip_byte_identically() {
        // EDI X12 and SWIFT MT carry latin-1 payloads. A cell that is not
        // valid UTF-8 surfaces as `Value::Bytes` and replays verbatim.
        let input = b"ISA*caf\xe9*00\n";
        let c = ByteTabularCodec::new("edi", "segments", b'*', None);
        let (inst, comp) = c.parse(input).unwrap();
        let out = c.emit(&inst, &comp).unwrap();
        assert_eq!(out, input);
        let rows = inst.rows("segments");
        assert_eq!(
            rows[0].1.get("col_1"),
            Some(&Value::Bytes(b"caf\xe9".to_vec()))
        );
    }

    #[test]
    fn a_non_utf8_cell_can_be_replaced_with_text() {
        let input = b"ISA*caf\xe9*00\n";
        let c = ByteTabularCodec::new("edi", "segments", b'*', None);
        let (mut inst, comp) = c.parse(input).unwrap();
        let first = inst.table("segments").unwrap().ids()[0];
        inst.row_mut(first)
            .unwrap()
            .insert("col_1".into(), Value::Str("cafe".into()));
        let out = c.emit(&inst, &comp).unwrap();
        assert_eq!(out, b"ISA*cafe*00\n");
    }

    #[test]
    fn the_byte_tabular_codec_refuses_a_labelled_null() {
        let input = b"key value\n";
        let c = codec();
        let (mut instance, complement) = c.parse(input).unwrap();
        let row = instance.table("entries").unwrap().ids()[0];
        instance
            .row_mut(row)
            .unwrap()
            .insert("col_1".into(), Value::LabeledNull(3));

        let error = c.emit(&instance, &complement).expect_err("must refuse");
        assert!(matches!(
            error,
            EmitInstanceError::LabeledNull { ref column, .. } if column == "col_1"
        ));
    }

    #[test]
    fn byte_tabular_labelled_null_refusal_uses_row_then_column_order() {
        let c = codec();
        let (mut instance, complement) = c.parse(b"first row\nsecond row\n").unwrap();
        let rows = instance.table("entries").unwrap().ids();
        instance
            .row_mut(rows[1])
            .unwrap()
            .insert("col_0".into(), Value::LabeledNull(1));
        instance
            .row_mut(rows[0])
            .unwrap()
            .insert("col_1".into(), Value::LabeledNull(2));
        instance
            .row_mut(rows[0])
            .unwrap()
            .insert("col_0".into(), Value::LabeledNull(3));

        let error = c.emit(&instance, &complement).expect_err("must refuse");
        assert!(matches!(
            error,
            EmitInstanceError::LabeledNull { row, ref column }
                if row == rows[0] && column == "col_0"
        ));
    }

    #[test]
    fn empty_fields_preserved() {
        // Asterisk-delimited with consecutive delimiters (empty cells).
        let input = b"ISA*00**ZZ*\nGS**X\n";
        let c = ByteTabularCodec::new("edi", "segments", b'*', None);
        let (inst, comp) = c.parse(input).unwrap();
        let out = c.emit(&inst, &comp).unwrap();
        assert_eq!(out, input);
    }
}
