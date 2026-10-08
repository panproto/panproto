//! Exact regressions for canonical text-route repairs.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::HashMap;

use panproto_gat::Name;
use panproto_inst::metadata::Node;
use panproto_inst::value::coerce::{LexClass, Lexeme, coerce};
use panproto_inst::value::{DecimalValue, FieldPresence, Value};
use panproto_inst::{FInstance, RowId, WInstance};
use panproto_io::error::{EmitInstanceError, EmitValueReason, LimitExceeded, Resource};
use panproto_io::json_pathway::{emit_json_bytes, parse_json_bytes};
use panproto_io::tabular_pathway::{emit_tsv_with_schema, parse_tsv};
use panproto_io::toml_pathway::emit_toml_bytes;
#[cfg(feature = "tree-sitter")]
use panproto_io::unified_codec::UnifiedCodec;
use panproto_io::xml_pathway::{emit_xml_bytes, parse_xml_bytes};
use panproto_schema::{Edge, ScalarKind, Schema, SchemaBuilder, ValueSpec};

fn one_text_column() -> Schema {
    let protocol = panproto_protocols::database::cassandra::protocol();
    SchemaBuilder::new(&protocol)
        .vertex("rows", "table", None)
        .expect("rows")
        .vertex("value", "text", None)
        .expect("value")
        .edge("rows", "value", "prop", Some("value"))
        .expect("value edge")
        .build()
        .expect("schema")
}

fn one_bigint_column(root: &str) -> Schema {
    let protocol = panproto_protocols::database::cassandra::protocol();
    SchemaBuilder::new(&protocol)
        .vertex(root, "table", None)
        .expect("root")
        .vertex("integer", "varint", None)
        .expect("integer")
        .edge(root, "integer", "prop", Some("integer"))
        .expect("integer edge")
        .build()
        .expect("schema")
}

fn one_decimal_column(root: &str) -> Schema {
    let protocol = panproto_protocols::database::cassandra::protocol();
    SchemaBuilder::new(&protocol)
        .vertex(root, "table", None)
        .expect("root")
        .vertex("decimal", "decimal", None)
        .expect("decimal")
        .edge(root, "decimal", "prop", Some("decimal"))
        .expect("decimal edge")
        .build()
        .expect("schema")
}

const fn extreme_decimal_zeros() -> [(&'static str, &'static str, bool); 3] {
    [
        ("0.0e-3000000000", "-3000000001", false),
        ("0e3000000000", "3000000000", false),
        ("-0e3000000000", "3000000000", false),
    ]
}

fn assert_extreme_decimal_zero(
    actual: &Value,
    expected_exponent: &str,
    expected_negative: bool,
    route: &str,
) {
    assert!(
        matches!(actual, Value::Decimal(value)
            if value.digits == "0"
                && value.negative == expected_negative
                && value.exponent.to_string() == expected_exponent),
        "{route}: {actual:?}"
    );
}

fn tree_value<'a>(instance: &'a WInstance, anchor: &str) -> &'a Value {
    let Some(FieldPresence::Present(value)) = instance
        .nodes
        .values()
        .find(|node| node.anchor.as_ref() == anchor)
        .and_then(|node| node.value.as_ref())
    else {
        panic!("missing present value at {anchor}");
    };
    value
}

fn one_row(value: Option<&str>) -> FInstance {
    let row = value.map_or_else(HashMap::new, |value| {
        HashMap::from([("value".to_owned(), Value::Str(value.to_owned()))])
    });
    FInstance::new()
        .with_table("rows", vec![row])
        .expect("one-row table")
}

fn cell(instance: &FInstance) -> Option<&Value> {
    instance.rows("rows")[0].1.get("value")
}

#[test]
fn tsv_has_no_comment_lines_by_default() {
    let schema = one_text_column();
    let input = b"value\n#ordinary-data\n";
    let parsed =
        parse_tsv(&schema, input, "cassandra", "rows", b'\t', None).expect("parse TSV hash");
    assert_eq!(cell(&parsed), Some(&Value::Str("#ordinary-data".into())));
    assert_eq!(
        emit_tsv_with_schema(&schema, &parsed, "cassandra", "rows", b'\t').expect("emit TSV hash"),
        input
    );
}

#[test]
fn an_underscore_cell_is_text() {
    let schema = one_text_column();
    for (delimiter, label) in [(b',', "csv"), (b'\t', "tsv")] {
        let parsed = parse_tsv(&schema, b"value\n_\n", "cassandra", "rows", delimiter, None)
            .unwrap_or_else(|error| panic!("parse {label}: {error}"));
        assert_eq!(cell(&parsed), Some(&Value::Str("_".into())));
        assert_eq!(
            emit_tsv_with_schema(&schema, &parsed, "cassandra", "rows", delimiter)
                .unwrap_or_else(|error| panic!("emit {label}: {error}")),
            b"value\n_\n"
        );

        let error = emit_tsv_with_schema(&schema, &one_row(None), "cassandra", "rows", delimiter)
            .expect_err("absence has no implicit delimited-text token");
        assert!(matches!(
            error,
            EmitInstanceError::Value(ref value)
                if value.reason == EmitValueReason::AbsentNotRepresentable
                    && value.value_type == "absent"
        ));
    }
}

#[test]
fn one_column_empty_rows_and_tsv_quotes_round_trip() {
    let schema = one_text_column();
    for delimiter in *b",\t" {
        let parsed = parse_tsv(&schema, b"value\n\n", "cassandra", "rows", delimiter, None)
            .expect("parse one-column empty row");
        assert_eq!(cell(&parsed), Some(&Value::Str(String::new())));
        assert_eq!(
            emit_tsv_with_schema(&schema, &parsed, "cassandra", "rows", delimiter)
                .expect("emit one-column empty row"),
            b"value\n\n"
        );
    }

    let quoted = one_row(Some("x\"y"));
    let tsv = emit_tsv_with_schema(&schema, &quoted, "cassandra", "rows", b'\t')
        .expect("a quote is ordinary TSV data");
    assert_eq!(tsv, b"value\nx\"y\n");
    assert_eq!(
        cell(
            &parse_tsv(&schema, &tsv, "cassandra", "rows", b'\t', None).expect("reparse TSV quote")
        ),
        Some(&Value::Str("x\"y".into()))
    );
}

#[test]
fn absent_fixture_uses_a_real_row_id() {
    // Guard the test fixture itself: `with_table` must allocate the absent row
    // rather than silently discarding an empty map.
    assert_eq!(one_row(None).rows("rows")[0].0, RowId(0));
}

#[test]
fn toml_writes_exact_decimals_and_big_integers() {
    let protocol = panproto_protocols::database::cassandra::protocol();
    let schema = SchemaBuilder::new(&protocol)
        .vertex("root", "table", None)
        .expect("root")
        .vertex("decimal", "column", None)
        .expect("decimal")
        .vertex("bigint", "column", None)
        .expect("bigint")
        .vertex("huge", "column", None)
        .expect("huge")
        .edge("root", "decimal", "prop", Some("decimal"))
        .expect("decimal edge")
        .edge("root", "bigint", "prop", Some("bigint"))
        .expect("bigint edge")
        .edge("root", "huge", "prop", Some("huge"))
        .expect("huge edge")
        .build()
        .expect("schema");
    let values = [
        (
            "decimal",
            Value::Decimal(
                DecimalValue::parse("0.1000000000000000055511151231257827").expect("exact decimal"),
            ),
        ),
        (
            "bigint",
            Value::parse_integer("99999999999999999999").expect("exact bigint"),
        ),
        (
            "huge",
            Value::Decimal(DecimalValue::parse("1e400").expect("huge decimal")),
        ),
    ];
    let mut nodes = HashMap::from([(0, Node::new(0, "root"))]);
    let mut arcs = Vec::new();
    for (index, (anchor, value)) in values.into_iter().enumerate() {
        let id = u32::try_from(index + 1).expect("small fixture");
        nodes.insert(
            id,
            Node::new(id, anchor).with_value(FieldPresence::Present(value)),
        );
        arcs.push((
            0,
            id,
            Edge {
                src: Name::from("root"),
                tgt: Name::from(anchor),
                kind: Name::from("prop"),
                name: Some(Name::from(anchor)),
            },
        ));
    }
    let instance = WInstance::new(nodes, arcs, Vec::new(), 0, "root".into());
    let emitted = emit_toml_bytes(&schema, &instance, "cassandra").expect("emit exact TOML");
    let text = std::str::from_utf8(&emitted).expect("TOML UTF-8");
    for line in [
        "decimal = 0.1000000000000000055511151231257827",
        "bigint = 99999999999999999999",
        "huge = 1e400",
    ] {
        assert!(
            text.lines().any(|actual| actual == line),
            "missing {line:?} in {text:?}"
        );
    }
    for (lexeme, expected) in [
        (
            "0.1000000000000000055511151231257827",
            instance.node(1).and_then(|node| node.value.as_ref()),
        ),
        (
            "99999999999999999999",
            instance.node(2).and_then(|node| node.value.as_ref()),
        ),
        (
            "1e400",
            instance.node(3).and_then(|node| node.value.as_ref()),
        ),
    ] {
        let actual = coerce(
            &Lexeme::new(lexeme, LexClass::Number),
            &ValueSpec::new(ScalarKind::Any),
        )
        .expect("read emitted token");
        let Some(FieldPresence::Present(expected)) = expected else {
            panic!("missing expected scalar for {lexeme}");
        };
        assert_eq!(&actual, expected);
    }
}

#[test]
fn bigint_text_routes_observe_the_output_allowance() {
    const TWENTY: &str = "100000000000000000000";

    let json_schema = one_bigint_column("root");
    let json = parse_json_bytes(
        &json_schema,
        "root",
        br#"{"integer":1e100000000}"#,
        "cassandra",
    )
    .expect("parse compact JSON bigint");
    assert_eq!(
        emit_json_bytes(&json_schema, &json, "cassandra").expect("emit compact JSON bigint"),
        br#"{"integer":1e100000000}"#
    );

    let xml_schema = one_bigint_column("doc");
    let xml = parse_xml_bytes(
        &xml_schema,
        format!("<doc><integer>{TWENTY}</integer></doc>").as_bytes(),
        "cassandra",
    )
    .expect("parse bounded XML bigint");
    let written_xml = emit_xml_bytes(&xml_schema, &xml, "cassandra").expect("emit XML bigint");
    assert!(
        std::str::from_utf8(&written_xml)
            .expect("XML UTF-8")
            .contains(TWENTY)
    );

    let huge_xml = parse_xml_bytes(
        &xml_schema,
        b"<doc><integer>1e100000000</integer></doc>",
        "cassandra",
    )
    .expect("parse compact XML bigint");
    let error = emit_xml_bytes(&xml_schema, &huge_xml, "cassandra")
        .expect_err("expanded XML bigint exceeds the allowance");
    assert!(matches!(
        error,
        EmitInstanceError::Value(ref error)
            if error.reason
                == EmitValueReason::LimitExceeded(LimitExceeded {
                    resource: Resource::OutputBytes,
                    limit: 67_108_864,
                })
    ));

    let csv_schema = one_bigint_column("rows");
    let csv = parse_tsv(
        &csv_schema,
        format!("integer\n{TWENTY}\n").as_bytes(),
        "cassandra",
        "rows",
        b',',
        None,
    )
    .expect("parse CSV bigint");
    assert_eq!(
        emit_tsv_with_schema(&csv_schema, &csv, "cassandra", "rows", b',')
            .expect("emit CSV bigint"),
        format!("integer\n{TWENTY}\n").as_bytes()
    );
}

#[test]
fn canonical_routes_keep_extreme_decimal_zero_scale_and_sign() {
    for (text, exponent, negative) in extreme_decimal_zeros() {
        let json_schema = one_decimal_column("root");
        let json_input = format!(r#"{{"decimal":{text}}}"#);
        let json = parse_json_bytes(&json_schema, "root", json_input.as_bytes(), "cassandra")
            .unwrap_or_else(|error| panic!("canonical JSON {text}: {error}"));
        assert_extreme_decimal_zero(tree_value(&json, "decimal"), exponent, negative, "JSON");

        let xml_schema = one_decimal_column("doc");
        let xml_input = format!("<doc><decimal>{text}</decimal></doc>");
        let xml = parse_xml_bytes(&xml_schema, xml_input.as_bytes(), "cassandra")
            .unwrap_or_else(|error| panic!("canonical XML {text}: {error}"));
        assert_extreme_decimal_zero(tree_value(&xml, "decimal"), exponent, negative, "XML");

        let tabular_schema = one_decimal_column("rows");
        for (delimiter, route) in [(b',', "CSV"), (b'\t', "TSV")] {
            let input = format!("decimal\n{text}\n");
            let table = parse_tsv(
                &tabular_schema,
                input.as_bytes(),
                "cassandra",
                "rows",
                delimiter,
                None,
            )
            .unwrap_or_else(|error| panic!("canonical {route} {text}: {error}"));
            let actual = table.rows("rows")[0]
                .1
                .get("decimal")
                .expect("decimal cell");
            assert_extreme_decimal_zero(actual, exponent, negative, route);
        }
    }
}

#[cfg(feature = "tree-sitter")]
#[test]
fn preserving_routes_keep_extreme_decimal_zero_scale_and_sign() {
    let tree_schema = one_decimal_column("root");
    let tree_routes = [
        (
            "JSON",
            UnifiedCodec::json("cassandra").expect("JSON CST codec"),
        ),
        (
            "TOML",
            UnifiedCodec::toml("cassandra").expect("TOML CST codec"),
        ),
        (
            "XML",
            UnifiedCodec::xml("cassandra").expect("XML CST codec"),
        ),
        (
            "YAML",
            UnifiedCodec::yaml("cassandra").expect("YAML CST codec"),
        ),
    ];
    for (text, exponent, negative) in extreme_decimal_zeros() {
        for (route, codec) in &tree_routes {
            let input = match *route {
                "JSON" => format!(r#"{{"decimal":{text}}}"#),
                "TOML" => format!("decimal = {text}\n"),
                "XML" => format!("<root><decimal>{text}</decimal></root>"),
                "YAML" => format!("decimal: {text}\n"),
                _ => unreachable!(),
            };
            let (instance, _) = codec
                .parse_wtype_preserving(&tree_schema, input.as_bytes())
                .unwrap_or_else(|error| panic!("preserving {route} {text}: {error}"));
            assert_extreme_decimal_zero(
                tree_value(&instance, "decimal"),
                exponent,
                negative,
                route,
            );
        }

        let table_schema = one_decimal_column("rows");
        for (route, codec) in [
            (
                "CSV",
                UnifiedCodec::csv("cassandra").expect("CSV CST codec"),
            ),
            (
                "TSV",
                UnifiedCodec::tsv("cassandra", "rows").expect("TSV CST codec"),
            ),
        ] {
            let input = format!("decimal\n{text}\n");
            let (table, _) = codec
                .parse_functor_preserving(&table_schema, input.as_bytes())
                .unwrap_or_else(|error| panic!("preserving {route} {text}: {error}"));
            let actual = table.rows("rows")[0]
                .1
                .get("decimal")
                .expect("decimal cell");
            assert_extreme_decimal_zero(actual, exponent, negative, route);
        }
    }
}

#[cfg(feature = "tree-sitter")]
#[test]
fn toml_cst_writes_exact_decimals_and_big_integers() {
    let protocol = panproto_protocols::database::cassandra::protocol();
    let schema = SchemaBuilder::new(&protocol)
        .vertex("root", "table", None)
        .expect("root")
        .vertex("x", "column", None)
        .expect("x")
        .edge("root", "x", "prop", Some("x"))
        .expect("x edge")
        .build()
        .expect("schema");
    let codec = UnifiedCodec::toml("cassandra").expect("TOML CST codec");
    let cases = [
        (
            b"x = 0.1000000000000000055511151231257827\n".as_slice(),
            Value::Decimal(
                DecimalValue::parse("0.1000000000000000055511151231257827").expect("exact decimal"),
            ),
        ),
        (
            b"x = 99999999999999999999\n".as_slice(),
            Value::parse_integer("99999999999999999999").expect("exact bigint"),
        ),
        (
            b"x = 1e400\n".as_slice(),
            Value::Decimal(DecimalValue::parse("1e400").expect("huge decimal")),
        ),
    ];
    for (input, expected) in cases {
        let (instance, complement) = codec
            .parse_wtype_preserving(&schema, input)
            .expect("parse exact TOML token");
        assert_eq!(
            codec
                .emit_wtype_preserving(&schema, &instance, &complement)
                .expect("replay exact TOML token"),
            input
        );

        let (mut edited, edit_complement) = codec
            .parse_wtype_preserving(&schema, b"x = 1\n")
            .expect("parse editable TOML token");
        edited
            .nodes
            .values_mut()
            .find(|node| node.anchor.as_ref() == "x")
            .expect("x node")
            .value = Some(FieldPresence::Present(expected.clone()));
        let emitted = codec
            .emit_wtype_preserving(&schema, &edited, &edit_complement)
            .expect("write exact edited TOML token");
        let (reparsed, _) = codec
            .parse_wtype_preserving(&schema, &emitted)
            .expect("reparse exact edited TOML token");
        let actual = reparsed
            .nodes
            .values()
            .find(|node| node.anchor.as_ref() == "x")
            .and_then(|node| node.value.as_ref());
        assert_eq!(actual, Some(&FieldPresence::Present(expected)));
    }
}

#[cfg(feature = "tree-sitter")]
#[test]
fn delimited_cst_cells_are_text() {
    let schema = one_text_column();
    for (codec, delimiter, label) in [
        (
            UnifiedCodec::csv("cassandra").expect("CSV CST codec"),
            b',',
            "CSV",
        ),
        (
            UnifiedCodec::tsv("cassandra", "rows").expect("TSV CST codec"),
            b'\t',
            "TSV",
        ),
    ] {
        let (underscore, complement) = codec
            .parse_functor_preserving(&schema, b"value\n_\n")
            .unwrap_or_else(|error| panic!("parse {label} underscore: {error}"));
        assert_eq!(cell(&underscore), Some(&Value::Str("_".into())));
        assert_eq!(
            codec
                .emit_functor_preserving(&schema, &underscore, &complement)
                .unwrap_or_else(|error| panic!("emit {label} underscore: {error}")),
            b"value\n_\n"
        );

        let (mut edited, edit_complement) = codec
            .parse_functor_preserving(&schema, b"value\nx\n")
            .unwrap_or_else(|error| panic!("parse editable {label}: {error}"));
        let row = edited.table("rows").expect("rows").ids()[0];
        edited
            .row_mut(row)
            .expect("row")
            .insert("value".into(), Value::Str("_".into()));
        let emitted = codec
            .emit_functor_preserving(&schema, &edited, &edit_complement)
            .unwrap_or_else(|error| panic!("emit edited {label}: {error}"));
        let (reparsed, _) = codec
            .parse_functor_preserving(&schema, &emitted)
            .unwrap_or_else(|error| panic!("reparse edited {label}: {error}"));
        assert_eq!(cell(&reparsed), Some(&Value::Str("_".into())));

        edited.row_mut(row).expect("row").remove("value");
        let error = codec
            .emit_functor_preserving(&schema, &edited, &edit_complement)
            .expect_err("absence has no delimited CST token");
        assert!(matches!(
            error,
            EmitInstanceError::Value(ref error)
                if error.reason == EmitValueReason::AbsentNotRepresentable
        ));

        let (empty, empty_complement) = codec
            .parse_functor_preserving(&schema, b"value\n\n")
            .unwrap_or_else(|error| panic!("parse empty {label} cell: {error}"));
        assert_eq!(cell(&empty), Some(&Value::Str(String::new())));
        assert_eq!(
            codec
                .emit_functor_preserving(&schema, &empty, &empty_complement)
                .unwrap_or_else(|error| panic!("emit empty {label} cell: {error}")),
            b"value\n\n"
        );

        if delimiter == b'\t' {
            let (mut quoted, quoted_complement) = codec
                .parse_functor_preserving(&schema, b"value\nx\n")
                .expect("parse TSV quote edit base");
            let row = quoted.table("rows").expect("rows").ids()[0];
            quoted
                .row_mut(row)
                .expect("row")
                .insert("value".into(), Value::Str("x\"y".into()));
            let emitted = codec
                .emit_functor_preserving(&schema, &quoted, &quoted_complement)
                .expect("quote is ordinary TSV CST data");
            let (reparsed, _) = codec
                .parse_functor_preserving(&schema, &emitted)
                .expect("reparse TSV quote edit");
            assert_eq!(cell(&reparsed), Some(&Value::Str("x\"y".into())));
        }
    }
}
