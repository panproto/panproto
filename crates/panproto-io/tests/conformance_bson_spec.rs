//! The BSON codec against the original conformance set of
//! `fixtures/conformance/bson-spec`, written from the text of the BSON 1.1,
//! BSON Decimal128 and Extended JSON v2 specifications.
//!
//! Every line of the set's `MANIFEST` is one case with the section of the
//! specification it exercises. A `valid` case decodes, encodes back to its
//! bytes directly and through the BSON tree encoding, writes its canonical
//! Extended JSON, and that text reads back as the bytes; a `relaxed` case
//! writes its relaxed Extended JSON, which reads and writes back as itself;
//! a `read` case is an Extended JSON text that reads as the bytes given; a
//! `decode-error`, `parse-error` or `write-error` case is refused by the
//! decoder, the Extended JSON reader or the canonical Extended JSON writer
//! with the class and the place given. Extended JSON texts are compared as
//! JSON values. The set runs in every build of the tests; the full corpus
//! of the MongoDB specifications is run by `conformance_bson.rs` when it
//! has been fetched.

#![cfg(feature = "tree-sitter")]

use std::collections::BTreeMap;
use std::path::Path;

use panproto_io::config::tree::read_json;
use panproto_io::data_schema::bson::{
    Mode, decode, encode, from_instance, read_extended_json, to_instance, write_extended_json,
};

/// One line of the set: case, check, input, expected and section.
struct Case {
    id: String,
    check: String,
    input: String,
    expected: String,
    section: String,
}

fn cases() -> Vec<Case> {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/conformance/bson-spec/MANIFEST");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{e}"));
    text.lines()
        .filter(|line| !line.starts_with('#') && !line.trim().is_empty())
        .map(|line| {
            let fields: Vec<&str> = line.split('\t').collect();
            let [id, check, input, expected, section] = fields[..] else {
                panic!("not five fields: {line}");
            };
            Case {
                id: id.to_owned(),
                check: check.to_owned(),
                input: input.to_owned(),
                expected: expected.to_owned(),
                section: section.to_owned(),
            }
        })
        .collect()
}

fn hex(text: &str) -> Result<Vec<u8>, String> {
    if !text.len().is_multiple_of(2) {
        return Err(format!("odd hex length {}", text.len()));
    }
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).map_err(|e| format!("hex: {e}")))
        .collect()
}

fn json(text: &str) -> Result<serde_json::Value, String> {
    serde_json::from_str(text).map_err(|e| format!("not JSON {text}: {e}"))
}

/// Two Extended JSON texts as JSON values, which ignores member order and
/// number spelling; texts equal as written are equal without parsing, so a
/// text nested deeper than `serde_json` reads compares when it is written
/// exactly as expected.
fn same_json(left: &str, right: &str) -> Result<(), String> {
    if left == right || json(left)? == json(right)? {
        Ok(())
    } else {
        Err(format!("{left} is not {right}"))
    }
}

fn same_bytes(left: &[u8], right: &[u8], what: &str) -> Result<(), String> {
    if left == right {
        Ok(())
    } else {
        Err(format!("{what}: {} is not {}", hex_of(left), hex_of(right)))
    }
}

fn hex_of(bytes: &[u8]) -> String {
    bytes.iter().fold(String::new(), |mut out, byte| {
        use std::fmt::Write as _;
        let _ = write!(out, "{byte:02x}");
        out
    })
}

/// The pointer of an expected refusal: `class@"pointer"`.
fn pointer_refusal(class: &str, pointer: &str) -> Result<String, String> {
    let quoted = serde_json::to_string(pointer).map_err(|e| e.to_string())?;
    Ok(format!("{class}@{quoted}"))
}

fn valid(case: &Case) -> Result<(), String> {
    let bytes = hex(&case.input)?;
    let document = decode(&bytes).map_err(|e| e.to_string())?;
    same_bytes(
        &encode(&document).map_err(|e| e.to_string())?,
        &bytes,
        "encode",
    )?;
    let instance = to_instance(&document).map_err(|e| e.to_string())?;
    let again = from_instance(&instance).map_err(|e| e.to_string())?;
    same_bytes(
        &encode(&again).map_err(|e| e.to_string())?,
        &bytes,
        "through the tree encoding",
    )?;
    let canonical = write_extended_json(&document, Mode::Canonical).map_err(|e| e.to_string())?;
    same_json(&canonical, &case.expected)?;
    let read = read_extended_json(case.expected.as_bytes()).map_err(|e| e.to_string())?;
    same_bytes(
        &encode(&read).map_err(|e| e.to_string())?,
        &bytes,
        "canonical text read back",
    )
}

fn relaxed(case: &Case) -> Result<(), String> {
    let document = decode(&hex(&case.input)?).map_err(|e| e.to_string())?;
    let text = write_extended_json(&document, Mode::Relaxed).map_err(|e| e.to_string())?;
    same_json(&text, &case.expected)?;
    let read = read_extended_json(case.expected.as_bytes()).map_err(|e| e.to_string())?;
    same_json(
        &write_extended_json(&read, Mode::Relaxed).map_err(|e| e.to_string())?,
        &case.expected,
    )
}

fn read(case: &Case) -> Result<(), String> {
    let document = read_extended_json(case.input.as_bytes()).map_err(|e| e.to_string())?;
    same_bytes(
        &encode(&document).map_err(|e| e.to_string())?,
        &hex(&case.expected)?,
        "read",
    )
}

fn decode_error(case: &Case) -> Result<(), String> {
    match decode(&hex(&case.input)?) {
        Ok(_) => Err("decoded".to_owned()),
        Err(e) => {
            let got = format!("{}@{}", e.kind.class(), e.offset);
            if got == case.expected {
                Ok(())
            } else {
                Err(format!("refused as {got}"))
            }
        }
    }
}

fn parse_error(case: &Case) -> Result<(), String> {
    let Err(error) = read_extended_json(case.input.as_bytes()) else {
        return Err("read".to_owned());
    };
    let class = error.kind.class();
    if class != "json" && read_json(case.input.as_bytes()).is_err() {
        return Err("the JSON route refuses the text".to_owned());
    }
    let got = pointer_refusal(class, &error.path)?;
    if got == case.expected {
        Ok(())
    } else {
        Err(format!("refused as {got}"))
    }
}

fn write_error(case: &Case) -> Result<(), String> {
    let document = decode(&hex(&case.input)?).map_err(|e| e.to_string())?;
    match write_extended_json(&document, Mode::Canonical) {
        Ok(text) => Err(format!("written as {text}")),
        Err(e) => {
            let got = pointer_refusal(e.kind.class(), &e.path)?;
            if got == case.expected {
                Ok(())
            } else {
                Err(format!("refused as {got}"))
            }
        }
    }
}

fn outcome(case: &Case) -> Result<(), String> {
    match case.check.as_str() {
        "valid" => valid(case),
        "relaxed" => relaxed(case),
        "read" => read(case),
        "decode-error" => decode_error(case),
        "parse-error" => parse_error(case),
        "write-error" => write_error(case),
        other => Err(format!("unknown check {other}")),
    }
}

#[test]
fn every_original_bson_case_has_the_outcome_its_manifest_line_states() {
    let failures: Vec<String> = cases()
        .iter()
        .filter_map(|case| {
            outcome(case)
                .err()
                .map(|why| format!("{} ({}): {why}", case.id, case.check))
        })
        .collect();
    assert_eq!(failures, Vec::<String>::new());
}

#[test]
fn the_original_bson_set_covers_every_type_with_each_check_and_names_its_section() {
    let cases = cases();
    let mut checks: BTreeMap<&str, usize> = BTreeMap::new();
    for case in &cases {
        *checks.entry(case.check.as_str()).or_default() += 1;
    }
    assert_eq!(
        checks,
        BTreeMap::from([
            ("decode-error", 27),
            ("parse-error", 34),
            ("read", 6),
            ("relaxed", 5),
            ("valid", 31),
            ("write-error", 3),
        ])
    );
    let types = [
        "double",
        "string",
        "document",
        "array",
        "binary",
        "undefined",
        "object-id",
        "boolean",
        "datetime",
        "null",
        "regex",
        "db-pointer",
        "code",
        "symbol",
        "code-with-scope",
        "int32",
        "timestamp",
        "int64",
        "decimal128",
        "min-key",
        "max-key",
    ];
    let ids: Vec<&str> = cases.iter().map(|case| case.id.as_str()).collect();
    let missing: Vec<String> = types
        .iter()
        .flat_map(|ty| {
            ["valid", "decode-error", "parse-error"]
                .into_iter()
                .map(move |check| format!("{ty}#{check}"))
        })
        .filter(|id| !ids.contains(&id.as_str()))
        .collect();
    assert_eq!(missing, Vec::<String>::new());
    let unsourced: Vec<&str> = cases
        .iter()
        .filter(|case| {
            !["BSON 1.1 (", "BSON Decimal128 (", "Extended JSON v2 ("]
                .iter()
                .any(|spec| case.section.starts_with(spec))
        })
        .map(|case| case.id.as_str())
        .collect();
    assert_eq!(unsourced, Vec::<&str>::new());
}

#[test]
fn the_original_bson_set_exercises_every_refusal_class_of_each_check() {
    let cases = cases();
    let mut classes: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for case in &cases {
        if case.check.ends_with("-error") {
            let (class, _) = case
                .expected
                .split_once('@')
                .unwrap_or_else(|| panic!("no place: {}", case.id));
            let seen = classes.entry(case.check.as_str()).or_default();
            if !seen.contains(&class) {
                seen.push(class);
            }
        }
    }
    for seen in classes.values_mut() {
        seen.sort_unstable();
    }
    assert_eq!(
        classes,
        BTreeMap::from([
            (
                "decode-error",
                vec![
                    "array-key",
                    "binary-length",
                    "boolean",
                    "code-with-scope-length",
                    "depth",
                    "document-length",
                    "early-terminator",
                    "invalid-utf8",
                    "non-canonical-decimal128",
                    "old-binary-length",
                    "regex-options",
                    "string-length",
                    "string-terminator",
                    "terminator",
                    "trailing-bytes",
                    "truncated",
                    "unknown-type",
                    "unterminated-cstring",
                ]
            ),
            (
                "parse-error",
                vec![
                    "decimal-representation",
                    "inexact-number",
                    "integer-range",
                    "json",
                    "not-a-document",
                    "nul-in-key",
                    "nul-in-regex",
                    "regex-options",
                    "wrapper-keys",
                    "wrapper-value",
                ]
            ),
            (
                "write-error",
                vec!["write-duplicate-key", "write-nan", "write-wrapper-key"]
            ),
        ])
    );
}
