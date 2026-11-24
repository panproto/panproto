//! The CSV and TSV preserving routes against the vendored csv-spectrum
//! corpus.
//!
//! Each csv-spectrum case is an RFC 4180 file with a header record and the
//! JSON array of records it denotes. The CSV route passes a case when it
//! reads exactly those records, every field as text, and an unedited
//! emission reproduces the file. The TSV route is checked against the same
//! records written as tab-separated text, which a case can only be when no
//! field holds a tab or a line break; the TSV dialect has no quoting, so a
//! record holding one is not writable as TSV and is skipped by construction.

#![cfg(feature = "tree-sitter")]
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::{Path, PathBuf};

use panproto_inst::FInstance;
use panproto_inst::value::Value;
use panproto_io::unified_codec::UnifiedCodec;
use panproto_schema::{Protocol, Schema, SchemaBuilder};
use serde_json::Value as Json;

fn corpus() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/conformance/csv-spectrum")
}

fn table_schema() -> Schema {
    let proto = Protocol {
        name: "csv".into(),
        schema_theory: "ThCsvSchema".into(),
        instance_theory: "ThCsvInstance".into(),
        ..Default::default()
    };
    SchemaBuilder::new(&proto)
        .vertex("rows", "table", None)
        .expect("table")
        .entry("rows")
        .build()
        .expect("schema")
}

/// The records of a table as JSON objects of strings, in row order.
fn records(instance: &FInstance) -> Vec<Json> {
    instance
        .rows("rows")
        .into_iter()
        .map(|(_, row)| {
            Json::Object(
                row.iter()
                    .map(|(column, value)| {
                        let text = match value {
                            Value::Str(text) => text.clone(),
                            other => format!("not text: {other:?}"),
                        };
                        (column.clone(), Json::String(text))
                    })
                    .collect(),
            )
        })
        .collect()
}

fn case_names() -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(corpus().join("csvs"))
        .unwrap()
        .map(|entry| {
            entry
                .unwrap()
                .path()
                .file_stem()
                .unwrap()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    names.sort();
    names
}

fn expected(name: &str) -> Vec<Json> {
    let text = std::fs::read(corpus().join("json").join(format!("{name}.json"))).unwrap();
    match serde_json::from_slice(&text).unwrap() {
        Json::Array(items) => items,
        record @ Json::Object(_) => vec![record],
        other => panic!("{name}: not a record set: {other}"),
    }
}

/// The records a case's file holds. The upstream `location_coordinates`
/// JSON gives the contact number `1234567890` where its CSV file holds
/// `2095257564`; the file is the case, so its own number is expected.
fn expected_records(name: &str) -> Vec<Json> {
    let mut records = expected(name);
    if name == "location_coordinates" {
        assert_eq!(records[0]["Contact Phone Number"], "1234567890");
        records[0]["Contact Phone Number"] = Json::String("2095257564".to_owned());
    }
    records
}

#[test]
fn every_csv_spectrum_case_reads_its_records_and_writes_its_bytes() {
    let codec = UnifiedCodec::csv("csv").expect("CSV codec");
    let schema = table_schema();
    let names = case_names();
    assert_eq!(names.len(), 12);
    for name in &names {
        let bytes = std::fs::read(corpus().join("csvs").join(format!("{name}.csv"))).unwrap();
        let (instance, complement) = codec
            .parse_functor_preserving(&schema, &bytes)
            .unwrap_or_else(|error| panic!("{name} was refused: {error}"));
        assert_eq!(records(&instance), expected_records(name), "{name}");
        assert_eq!(
            codec
                .emit_functor_preserving(&schema, &instance, &complement)
                .unwrap(),
            bytes,
            "{name}: unedited emission"
        );
    }
}

#[test]
fn every_tab_free_csv_spectrum_record_set_reads_back_through_tsv() {
    let codec = UnifiedCodec::new(
        "tsv",
        panproto_io::cst_extract::FormatKind::Tsv,
        panproto_io::NativeRepr::Table,
    )
    .expect("TSV codec");
    let schema = table_schema();
    let mut checked = 0;
    for name in case_names() {
        let records_expected = expected_records(&name);
        let csv =
            std::fs::read_to_string(corpus().join("csvs").join(format!("{name}.csv"))).unwrap();
        let header_line = csv.lines().next().unwrap_or_default();
        let header: Vec<String> = header_line.split(',').map(str::to_owned).collect();
        let writable = !records_expected.is_empty()
            && header.iter().all(|column| !column.contains(['"', '\t']))
            && records_expected.iter().all(|record| {
                record
                    .as_object()
                    .unwrap()
                    .values()
                    .all(|value| !value.as_str().unwrap().contains(['\t', '\n', '\r']))
            });
        if !writable {
            continue;
        }
        let mut tsv = header.join("\t");
        tsv.push('\n');
        for record in &records_expected {
            let fields: Vec<&str> = header
                .iter()
                .map(|column| record[column.as_str()].as_str().unwrap())
                .collect();
            tsv.push_str(&fields.join("\t"));
            tsv.push('\n');
        }
        let (instance, complement) = codec
            .parse_functor_preserving(&schema, tsv.as_bytes())
            .unwrap_or_else(|error| panic!("{name} as TSV was refused: {error}"));
        assert_eq!(records(&instance), records_expected, "{name} as TSV");
        assert_eq!(
            codec
                .emit_functor_preserving(&schema, &instance, &complement)
                .unwrap(),
            tsv.as_bytes(),
            "{name} as TSV: unedited emission"
        );
        checked += 1;
    }
    assert_eq!(checked, 9, "the tab-free cases");
}
