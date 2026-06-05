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

use std::collections::HashMap;

use panproto_inst::value::Value;
use panproto_inst::{FInstance, WInstance};
use panproto_schema::Schema;

use crate::error::{EmitInstanceError, ParseInstanceError};
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
                    row.insert(
                        format!("{COL_PREFIX}{i}"),
                        Value::Str(String::from_utf8_lossy(field).into_owned()),
                    );
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

        let instance = FInstance::new().with_table(&self.table_vertex, rows);
        let complement = ByteTabularComplement {
            lines: records,
            delimiter: self.delimiter,
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
    /// Returns [`EmitInstanceError::Emit`] if the instance lacks the table.
    pub fn emit(
        &self,
        instance: &FInstance,
        complement: &ByteTabularComplement,
    ) -> Result<Vec<u8>, EmitInstanceError> {
        let rows =
            instance
                .tables
                .get(&self.table_vertex)
                .ok_or_else(|| EmitInstanceError::Emit {
                    protocol: self.protocol.clone(),
                    message: format!("table '{}' not found in instance", self.table_vertex),
                })?;

        let mut output = Vec::with_capacity(complement.lines.len() * 16);
        let mut data_idx = 0usize;

        for line in &complement.lines {
            if !line.is_data {
                output.extend_from_slice(&line.raw);
                output.extend_from_slice(&line.line_ending);
                continue;
            }

            // Pair this data line with the next instance row (positional).
            let row = rows.get(data_idx);
            data_idx += 1;

            let updated = row.and_then(|r| splice_line(line, r, complement.delimiter));
            match updated {
                Some(bytes) => output.extend_from_slice(&bytes),
                None => output.extend_from_slice(&line.raw),
            }
            output.extend_from_slice(&line.line_ending);
        }

        Ok(output)
    }
}

/// Rebuild a data line from a possibly-edited instance row, or return `None`
/// when every cell is byte-identical to the recorded original (so the caller
/// can replay the raw bytes verbatim).
fn splice_line(line: &LineRecord, row: &HashMap<String, Value>, delimiter: u8) -> Option<Vec<u8>> {
    let mut changed = false;
    let mut out_fields: Vec<Vec<u8>> = Vec::with_capacity(line.fields.len());

    for (i, original) in line.fields.iter().enumerate() {
        let key = format!("{COL_PREFIX}{i}");
        match row.get(&key) {
            Some(value) => {
                let bytes = value_bytes(value);
                if &bytes != original {
                    changed = true;
                }
                out_fields.push(bytes);
            }
            None => out_fields.push(original.clone()),
        }
    }

    if !changed {
        return None;
    }

    let mut bytes = Vec::new();
    for (i, field) in out_fields.iter().enumerate() {
        if i > 0 {
            bytes.push(delimiter);
        }
        bytes.extend_from_slice(field);
    }
    Some(bytes)
}

/// Render a [`Value`] to its byte form for splicing into a delimited line.
fn value_bytes(value: &Value) -> Vec<u8> {
    match value {
        Value::Str(s) => s.clone().into_bytes(),
        Value::Int(n) => n.to_string().into_bytes(),
        Value::Float(f) => f.to_string().into_bytes(),
        Value::Bool(b) => if *b { "true" } else { "false" }.as_bytes().to_vec(),
        Value::Bytes(b) => b.clone(),
        other => format!("{other:?}").into_bytes(),
    }
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
        NativeRepr::Functor
    }

    fn parse_wtype(
        &self,
        _schema: &Schema,
        _input: &[u8],
    ) -> Result<WInstance, ParseInstanceError> {
        Err(ParseInstanceError::UnsupportedRepresentation {
            protocol: self.protocol.clone(),
            requested: NativeRepr::WType,
            native: NativeRepr::Functor,
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
            requested: NativeRepr::WType,
            native: NativeRepr::Functor,
        })
    }

    fn emit_functor(
        &self,
        _schema: &Schema,
        instance: &FInstance,
    ) -> Result<Vec<u8>, EmitInstanceError> {
        // Without a complement there is nothing to replay; rebuild a canonical
        // delimited file from the positional cells. This is the non-preserving
        // fallback used by the generic `InstanceEmitter` seam; the byte-faithful
        // path is `emit` with a complement.
        let rows =
            instance
                .tables
                .get(&self.table_vertex)
                .ok_or_else(|| EmitInstanceError::Emit {
                    protocol: self.protocol.clone(),
                    message: format!("table '{}' not found in instance", self.table_vertex),
                })?;
        let mut output = Vec::new();
        for row in rows {
            let mut i = 0usize;
            loop {
                let key = format!("{COL_PREFIX}{i}");
                let Some(value) = row.get(&key) else { break };
                if i > 0 {
                    output.push(self.delimiter);
                }
                output.extend_from_slice(&value_bytes(value));
                i += 1;
            }
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
    fn no_trailing_newline_preserved() {
        let input = b"key user:1001\nname Alice";
        let c = codec();
        let (inst, comp) = c.parse(input).unwrap();
        let out = c.emit(&inst, &comp).unwrap();
        assert_eq!(out, input);
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
        assert_eq!(inst.tables.get("t").unwrap().len(), 2);
        let out = c.emit(&inst, &comp).unwrap();
        assert_eq!(out, input);
    }

    #[test]
    fn edit_rewrites_one_cell() {
        let input = b"key user:1001\nname Alice Chen\n";
        let c = codec();
        let (mut inst, comp) = c.parse(input).unwrap();
        // Edit the value of the first record's col_1 (user:1001 -> user:2002).
        let rows = inst.tables.get_mut("entries").unwrap();
        rows[0].insert("col_1".into(), Value::Str("user:2002".into()));
        let out = c.emit(&inst, &comp).unwrap();
        assert_eq!(out, b"key user:2002\nname Alice Chen\n");
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
