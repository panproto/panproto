//! Shared tabular pathway for line/field-delimited instance parsing via `memchr`.
//!
//! Provides SIMD-accelerated line and field splitting for protocols whose
//! instance data is tab-separated (CoNLL-U), comma-separated (CSV),
//! or fixed-field (EDI X12, SWIFT MT). Produces `FInstance` (set-valued
//! functor) tables guided by a schema.

use std::collections::HashMap;

use memchr::memchr_iter;

use panproto_inst::FInstance;
use panproto_inst::value::Value;
use panproto_schema::Schema;

use crate::error::{EmitInstanceError, ParseInstanceError};

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

/// Parse tab-separated data into an `FInstance`.
///
/// Assumes the first non-comment, non-blank line is a header row defining
/// column names. Subsequent lines are data rows. Blank lines and lines
/// starting with `comment_prefix` are skipped.
///
/// Each row becomes a row in the `table_vertex` table of the `FInstance`.
///
/// # Errors
///
/// Returns [`ParseInstanceError::Parse`] if the input is malformed.
pub fn parse_tsv(
    _schema: &Schema,
    input: &[u8],
    protocol: &str,
    table_vertex: &str,
    delimiter: u8,
    comment_prefix: Option<u8>,
) -> Result<FInstance, ParseInstanceError> {
    let lines = split_lines(input);
    let mut data_lines: Vec<&[u8]> = Vec::new();

    for line in &lines {
        if line.is_empty() {
            continue;
        }
        if let Some(prefix) = comment_prefix {
            if line.first() == Some(&prefix) {
                continue;
            }
        }
        data_lines.push(line);
    }

    if data_lines.is_empty() {
        return Err(ParseInstanceError::Parse {
            protocol: protocol.to_string(),
            message: "no data lines found".into(),
        });
    }

    // First data line is header.
    let header_fields = split_fields(data_lines[0], delimiter);
    let headers: Vec<String> = header_fields
        .iter()
        .map(|f| String::from_utf8_lossy(f).to_string())
        .collect();

    // Remaining lines are data rows.
    let mut rows: Vec<HashMap<String, Value>> = Vec::new();
    for line in &data_lines[1..] {
        let fields = split_fields(line, delimiter);
        let mut row = HashMap::new();
        for (i, field) in fields.iter().enumerate() {
            let col_name = headers
                .get(i)
                .cloned()
                .unwrap_or_else(|| format!("col_{i}"));
            let val = String::from_utf8_lossy(field).to_string();
            if val != "_" {
                row.insert(col_name, Value::Str(val));
            }
        }
        rows.push(row);
    }

    let instance = FInstance::new().with_table(table_vertex, rows);
    Ok(instance)
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
    _schema: &Schema,
    input: &[u8],
    protocol: &str,
) -> Result<FInstance, ParseInstanceError> {
    let lines = split_lines(input);
    let mut sentence_rows: Vec<HashMap<String, Value>> = Vec::new();
    let mut token_rows: Vec<HashMap<String, Value>> = Vec::new();
    let mut current_sent_id: usize = 0;
    let mut in_sentence = false;
    let mut sent_metadata: HashMap<String, Value> = HashMap::new();

    let conllu_columns = [
        "ID", "FORM", "LEMMA", "UPOS", "XPOS", "FEATS", "HEAD", "DEPREL", "DEPS", "MISC",
    ];

    for line in &lines {
        if line.is_empty() {
            // Sentence boundary.
            if in_sentence {
                sent_metadata.insert(
                    "sent_idx".into(),
                    Value::Int(i64::try_from(current_sent_id).unwrap_or(0)),
                );
                sentence_rows.push(sent_metadata.clone());
                sent_metadata.clear();
                current_sent_id += 1;
                in_sentence = false;
            }
            continue;
        }

        if line.first() == Some(&b'#') {
            // Comment line — extract metadata.
            let comment = String::from_utf8_lossy(line);
            let trimmed = comment.trim_start_matches('#').trim();
            if let Some((key, val)) = trimmed.split_once('=') {
                sent_metadata.insert(
                    key.trim().replace(' ', "_"),
                    Value::Str(val.trim().to_string()),
                );
            }
            in_sentence = true;
            continue;
        }

        in_sentence = true;
        let fields = split_fields(line, b'\t');
        let mut row = HashMap::new();
        row.insert(
            "sent_idx".into(),
            Value::Int(i64::try_from(current_sent_id).unwrap_or(0)),
        );

        for (i, field) in fields.iter().enumerate() {
            if let Some(col_name) = conllu_columns.get(i) {
                let val = String::from_utf8_lossy(field).to_string();
                if val != "_" {
                    row.insert((*col_name).to_string(), Value::Str(val));
                }
            }
        }
        token_rows.push(row);
    }

    // Handle last sentence if no trailing blank line.
    if in_sentence {
        sent_metadata.insert(
            "sent_idx".into(),
            Value::Int(i64::try_from(current_sent_id).unwrap_or(0)),
        );
        sentence_rows.push(sent_metadata);
    }

    if token_rows.is_empty() {
        return Err(ParseInstanceError::Parse {
            protocol: protocol.to_string(),
            message: "no tokens found in CoNLL-U input".into(),
        });
    }

    let instance = FInstance::new()
        .with_table("sentence", sentence_rows)
        .with_table("token", token_rows);

    Ok(instance)
}

/// Emit an `FInstance` to tab-separated bytes.
///
/// Writes a header row followed by data rows. The `table_vertex` specifies
/// which table to emit. Columns are sorted alphabetically.
///
/// # Errors
///
/// Returns [`EmitInstanceError::Emit`] if the table is not found.
pub fn emit_tsv(
    instance: &FInstance,
    protocol: &str,
    table_vertex: &str,
    delimiter: u8,
) -> Result<Vec<u8>, EmitInstanceError> {
    let rows = instance
        .tables
        .get(table_vertex)
        .ok_or_else(|| EmitInstanceError::Emit {
            protocol: protocol.to_string(),
            message: format!("table '{table_vertex}' not found in instance"),
        })?;

    if rows.is_empty() {
        return Ok(Vec::new());
    }

    // Collect all column names from all rows, sorted.
    let mut columns: Vec<String> = rows
        .iter()
        .flat_map(|r| r.keys().cloned())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    columns.sort();

    let mut output = Vec::new();

    // Header.
    for (i, col) in columns.iter().enumerate() {
        if i > 0 {
            output.push(delimiter);
        }
        output.extend_from_slice(col.as_bytes());
    }
    output.push(b'\n');

    // Data rows.
    for row in rows {
        for (i, col) in columns.iter().enumerate() {
            if i > 0 {
                output.push(delimiter);
            }
            if let Some(val) = row.get(col) {
                match val {
                    Value::Str(s) => output.extend_from_slice(s.as_bytes()),
                    Value::Int(n) => output.extend_from_slice(n.to_string().as_bytes()),
                    Value::Float(f) => output.extend_from_slice(f.to_string().as_bytes()),
                    Value::Bool(b) => output.extend_from_slice(if *b { b"true" } else { b"false" }),
                    _ => output.extend_from_slice(b"_"),
                }
            } else {
                output.push(b'_');
            }
        }
        output.push(b'\n');
    }

    Ok(output)
}

/// Emit an `FInstance` to CoNLL-U format.
///
/// # Errors
///
/// Returns [`EmitInstanceError::Emit`] if required tables are missing.
pub fn emit_conllu(instance: &FInstance, protocol: &str) -> Result<Vec<u8>, EmitInstanceError> {
    let token_rows = instance
        .tables
        .get("token")
        .ok_or_else(|| EmitInstanceError::Emit {
            protocol: protocol.to_string(),
            message: "token table not found".into(),
        })?;

    let sentence_rows = instance
        .tables
        .get("sentence")
        .unwrap_or(&Vec::new())
        .clone();

    let conllu_columns = [
        "ID", "FORM", "LEMMA", "UPOS", "XPOS", "FEATS", "HEAD", "DEPREL", "DEPS", "MISC",
    ];

    let mut output = Vec::new();
    let mut current_sent: i64 = -1;

    for row in token_rows {
        let sent_idx = match row.get("sent_idx") {
            Some(Value::Int(n)) => *n,
            _ => 0,
        };

        // Emit sentence boundary + metadata.
        if sent_idx != current_sent {
            if current_sent >= 0 {
                output.push(b'\n'); // Blank line between sentences.
            }
            current_sent = sent_idx;

            // Emit sentence metadata comments.
            if let Some(sent_row) = sentence_rows
                .iter()
                .find(|r| matches!(r.get("sent_idx"), Some(Value::Int(n)) if *n == sent_idx))
            {
                for (key, val) in sent_row {
                    if key == "sent_idx" {
                        continue;
                    }
                    if let Value::Str(s) = val {
                        output.extend_from_slice(format!("# {key} = {s}\n").as_bytes());
                    }
                }
            }
        }

        // Emit token line.
        for (i, col) in conllu_columns.iter().enumerate() {
            if i > 0 {
                output.push(b'\t');
            }
            if let Some(val) = row.get(*col) {
                match val {
                    Value::Str(s) => output.extend_from_slice(s.as_bytes()),
                    Value::Int(n) => output.extend_from_slice(n.to_string().as_bytes()),
                    _ => output.push(b'_'),
                }
            } else {
                output.push(b'_');
            }
        }
        output.push(b'\n');
    }

    // Trailing blank line.
    output.push(b'\n');

    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

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
            variants: HashMap::new(),
            orderings: HashMap::new(),
            recursion_points: HashMap::new(),
            spans: HashMap::new(),
            usage_modes: HashMap::new(),
            nominal: HashMap::new(),
            outgoing: HashMap::new(),
            incoming: HashMap::new(),
            between: HashMap::new(),
        };

        let instance = parse_conllu(&schema, input, "conllu").expect("parse");
        let tokens = instance.tables.get("token").expect("token table");
        assert_eq!(tokens.len(), 3, "should have 3 tokens");

        let sentences = instance.tables.get("sentence").expect("sentence table");
        assert_eq!(sentences.len(), 1, "should have 1 sentence");
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
            variants: HashMap::new(),
            orderings: HashMap::new(),
            recursion_points: HashMap::new(),
            spans: HashMap::new(),
            usage_modes: HashMap::new(),
            nominal: HashMap::new(),
            outgoing: HashMap::new(),
            incoming: HashMap::new(),
            between: HashMap::new(),
        };

        let instance = parse_conllu(&schema, input, "conllu").expect("parse");
        let emitted = emit_conllu(&instance, "conllu").expect("emit");
        let instance2 = parse_conllu(&schema, &emitted, "conllu").expect("re-parse");

        let t1 = instance.tables.get("token").expect("tokens");
        let t2 = instance2.tables.get("token").expect("tokens");
        assert_eq!(
            t1.len(),
            t2.len(),
            "token count should match after round-trip"
        );
    }
}
