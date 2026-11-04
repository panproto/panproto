//! The declared value and dialect domains of the YAML, TOML, CSV and TSV
//! text routes: what each reads, what each writes back exactly, and what
//! each refuses with a typed error rather than losing.

#![cfg(feature = "tree-sitter")]
#![allow(clippy::expect_used, clippy::unwrap_used)]

use panproto_inst::WInstance;
use panproto_inst::value::{
    CANONICAL_NAN, DecimalValue, FieldPresence, NEGATIVE_CANONICAL_NAN, Temporal, Value,
};
use panproto_io::ParseInstanceError;
use panproto_io::error::{EmitInstanceError, EmitValueReason};
use panproto_io::toml_pathway::emit_toml_bytes;
use panproto_io::unified_codec::UnifiedCodec;
use panproto_schema::{Protocol, Schema, SchemaBuilder};

fn one_vertex_schema(vertex: &str) -> Schema {
    let proto = Protocol {
        name: "test".into(),
        schema_theory: "ThtestSchema".into(),
        instance_theory: "ThtestInstance".into(),
        ..Default::default()
    };
    SchemaBuilder::new(&proto)
        .vertex(vertex, "object", None)
        .expect("vertex")
        .build()
        .expect("schema")
}

/// The value of the node anchored at `anchor`.
fn value_at<'a>(instance: &'a WInstance, anchor: &str) -> &'a Value {
    let Some(FieldPresence::Present(value)) = instance
        .nodes
        .values()
        .find(|node| node.anchor.as_ref() == anchor)
        .and_then(|node| node.value.as_ref())
    else {
        panic!("no present value at {anchor}: {instance:?}");
    };
    value
}

fn set_value(instance: &mut WInstance, anchor: &str, value: Value) {
    let node = instance
        .nodes
        .values_mut()
        .find(|node| node.anchor.as_ref() == anchor)
        .expect("anchored node");
    node.value = Some(FieldPresence::Present(value));
}

fn float_bits(value: &Value) -> u64 {
    let Value::Float(number) = value else {
        panic!("not a float: {value:?}");
    };
    number.to_bits()
}

fn assert_not_utf8(result: Result<impl std::fmt::Debug, ParseInstanceError>, offset: usize) {
    match result {
        Err(ParseInstanceError::Parse { message, .. }) => assert!(
            message.contains("not UTF-8") && message.ends_with(&format!("at byte {offset}")),
            "{message}"
        ),
        other => panic!("expected a typed encoding refusal, got {other:?}"),
    }
}

#[test]
fn text_routes_refuse_a_source_that_is_not_utf8() {
    let tree = one_vertex_schema("root");
    let yaml = UnifiedCodec::yaml("test").unwrap();
    let toml = UnifiedCodec::toml("test").unwrap();
    assert_not_utf8(yaml.parse_wtype_preserving(&tree, b"a: \xff\n"), 3);
    assert_not_utf8(toml.parse_wtype_preserving(&tree, b"a = \"\xff\"\n"), 5);
    // A comment is source text too: its bytes must survive an unedited
    // emission, so it is held to the same encoding.
    assert_not_utf8(toml.parse_wtype_preserving(&tree, b"# \xc3\na = 1\n"), 2);

    let table = one_vertex_schema("rows");
    let csv = UnifiedCodec::csv("test").unwrap();
    let tsv = UnifiedCodec::tsv("test", "rows").unwrap();
    // These cells read as empty strings, and the unedited emission dropped
    // the bytes, before the route refused them.
    assert_not_utf8(csv.parse_functor_preserving(&table, b"a,b\n\xff,\xfe\n"), 4);
    assert_not_utf8(tsv.parse_functor_preserving(&table, b"a\tb\n\xff\t1\n"), 4);
    assert_not_utf8(
        panproto_io::traits::InstanceParser::parse_functor(&csv, &table, b"a\n\x80\n"),
        2,
    );
}

#[test]
fn yaml_and_toml_read_the_sign_of_a_nan() {
    let tree = one_vertex_schema("root");
    let (yaml, _) = UnifiedCodec::yaml("test")
        .unwrap()
        .parse_wtype_preserving(&tree, b"a: .nan\nb: -.nan\n")
        .unwrap();
    assert_eq!(
        float_bits(value_at(&yaml, "root:a")),
        CANONICAL_NAN.to_bits()
    );
    assert_eq!(
        float_bits(value_at(&yaml, "root:b")),
        NEGATIVE_CANONICAL_NAN.to_bits()
    );
    let (toml, _) = UnifiedCodec::toml("test")
        .unwrap()
        .parse_wtype_preserving(&tree, b"a = nan\nb = -nan\nc = +nan\n")
        .unwrap();
    assert_eq!(
        float_bits(value_at(&toml, "root:a")),
        CANONICAL_NAN.to_bits()
    );
    assert_eq!(
        float_bits(value_at(&toml, "root:b")),
        NEGATIVE_CANONICAL_NAN.to_bits()
    );
    assert_eq!(
        float_bits(value_at(&toml, "root:c")),
        CANONICAL_NAN.to_bits()
    );
}

#[test]
fn canonical_toml_writes_a_temporal_exactly_as_it_displays() {
    let tree = one_vertex_schema("root");
    let codec = UnifiedCodec::toml("test").unwrap();
    // Ten fractional digits, a space separator, a lowercase zulu and a
    // negative-zero offset: each is part of the value, and a conversion
    // through a nanosecond date-time kept none of them.
    let input = b"a = 1979-05-27 07:32:00.1234567891z\nb = 07:32:00.0000000001\nc = 1979-05-27T07:32:00-00:00\n";
    let (parsed, _) = codec.parse_wtype_preserving(&tree, input).unwrap();
    let written = emit_toml_bytes(&tree, &parsed, "test").expect("canonical TOML");
    assert_eq!(
        String::from_utf8(written.clone()).unwrap(),
        String::from_utf8(input.to_vec()).unwrap()
    );
    let (reread, _) = codec.parse_wtype_preserving(&tree, &written).unwrap();
    for anchor in ["root:a", "root:b", "root:c"] {
        assert_eq!(
            value_at(&reread, anchor),
            value_at(&parsed, anchor),
            "{anchor}"
        );
    }

    // A temporal TOML has no form for is still refused.
    let mut edited = parsed;
    set_value(
        &mut edited,
        "root:a",
        Value::Temporal(Temporal::parse("1979-05-27T07:32:00+0530").expect("basic offset")),
    );
    match emit_toml_bytes(&tree, &edited, "test") {
        Err(EmitInstanceError::Value(error)) => {
            assert_eq!(error.reason, EmitValueReason::NotRepresentable);
        }
        other => panic!("expected a typed refusal, got {other:?}"),
    }
}

fn yaml_edit(value: Value) -> Result<Vec<u8>, EmitInstanceError> {
    let tree = one_vertex_schema("root");
    let codec = UnifiedCodec::yaml("test").unwrap();
    let (mut instance, complement) = codec.parse_wtype_preserving(&tree, b"a: 1.5\n").unwrap();
    set_value(&mut instance, "root:a", value);
    codec.emit_wtype_preserving(&tree, &instance, &complement)
}

fn assert_yaml_refuses(value: &Value, reason: &EmitValueReason) {
    match yaml_edit(value.clone()) {
        Err(EmitInstanceError::Value(error)) => assert_eq!(&error.reason, reason, "{value:?}"),
        other => panic!("{value:?}: expected a typed refusal, got {other:?}"),
    }
}

#[test]
fn yaml_refuses_a_local_temporal_and_writes_a_zoned_one() {
    // A YAML timestamp without a zone denotes UTC, so no YAML spelling
    // reads back as a local date, time or date-time.
    for local in ["1979-05-27", "07:32:00", "1979-05-27T07:32:00"] {
        assert_yaml_refuses(
            &Value::Temporal(Temporal::parse(local).expect("temporal")),
            &EmitValueReason::NotRepresentable,
        );
    }
    let zoned = Value::Temporal(Temporal::parse("1979-05-27T07:32:00.1234567891Z").unwrap());
    let written = yaml_edit(zoned.clone()).expect("a zoned date-time has a YAML spelling");
    let tree = one_vertex_schema("root");
    let (reread, _) = UnifiedCodec::yaml("test")
        .unwrap()
        .parse_wtype_preserving(&tree, &written)
        .unwrap();
    assert_eq!(value_at(&reread, "root:a"), &zoned);
}

#[test]
fn yaml_refuses_a_special_decimal_as_non_finite() {
    for special in [
        DecimalValue::infinity(false),
        DecimalValue::infinity(true),
        DecimalValue::nan(false, false, ""),
        DecimalValue::nan(true, true, "7"),
    ] {
        assert_yaml_refuses(&Value::Decimal(special), &EmitValueReason::NonFinite);
    }
    let finite = Value::Decimal(DecimalValue::parse("1.10").unwrap());
    let written = yaml_edit(finite.clone()).expect("a finite decimal has a YAML spelling");
    let tree = one_vertex_schema("root");
    let (reread, _) = UnifiedCodec::yaml("test")
        .unwrap()
        .parse_wtype_preserving(&tree, &written)
        .unwrap();
    assert_eq!(value_at(&reread, "root:a"), &finite);
}

#[test]
fn text_routes_write_back_a_leading_byte_order_mark() {
    let tree = one_vertex_schema("root");
    for (codec, input, anchor) in [
        (
            UnifiedCodec::yaml("test").unwrap(),
            "\u{feff}a: 1\n",
            "root:a",
        ),
        (
            UnifiedCodec::toml("test").unwrap(),
            "\u{feff}a = 1\n",
            "root:a",
        ),
    ] {
        let (mut instance, complement) = codec
            .parse_wtype_preserving(&tree, input.as_bytes())
            .unwrap();
        assert!(complement.byte_order_mark, "{input:?}");
        assert_eq!(value_at(&instance, anchor), &Value::Int(1));
        let unedited = codec
            .emit_wtype_preserving(&tree, &instance, &complement)
            .unwrap();
        assert_eq!(unedited, input.as_bytes());
        set_value(&mut instance, anchor, Value::Int(2));
        let edited = codec
            .emit_wtype_preserving(&tree, &instance, &complement)
            .unwrap();
        assert_eq!(edited, input.replace('1', "2").as_bytes());
    }

    let table = one_vertex_schema("rows");
    for (codec, input) in [
        (UnifiedCodec::csv("test").unwrap(), "\u{feff}a,b\r\n1,2\r\n"),
        (
            UnifiedCodec::tsv("test", "rows").unwrap(),
            "\u{feff}a\tb\r\n1\t2\r\n",
        ),
    ] {
        let (instance, complement) = codec
            .parse_functor_preserving(&table, input.as_bytes())
            .unwrap();
        assert!(complement.byte_order_mark);
        let header: Vec<&String> = {
            let mut keys: Vec<&String> = instance.rows("rows")[0].1.keys().collect();
            keys.sort();
            keys
        };
        assert_eq!(
            header,
            ["a", "b"],
            "the mark is not part of the first header"
        );
        assert_eq!(
            codec
                .emit_functor_preserving(&table, &instance, &complement)
                .unwrap(),
            input.as_bytes()
        );
    }

    // A source without a mark gains none, and its complement carrier is
    // the one written before the flag existed.
    let (instance, complement) = UnifiedCodec::yaml("test")
        .unwrap()
        .parse_wtype_preserving(&tree, b"a: 1\n")
        .unwrap();
    assert!(!complement.byte_order_mark);
    assert!(
        !serde_json::to_string(&complement)
            .unwrap()
            .contains("byte_order_mark")
    );
    assert_eq!(
        UnifiedCodec::yaml("test")
            .unwrap()
            .emit_wtype_preserving(&tree, &instance, &complement)
            .unwrap(),
        b"a: 1\n"
    );
}

/// The cells of the only row of `rows`, by column name.
fn only_row(instance: &panproto_inst::FInstance) -> Vec<(String, Value)> {
    let rows = instance.rows("rows");
    assert_eq!(rows.len(), 1, "{rows:?}");
    let mut cells: Vec<(String, Value)> = rows[0]
        .1
        .iter()
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();
    cells.sort_by(|a, b| a.0.cmp(&b.0));
    cells
}

fn text(value: &str) -> Value {
    Value::Str(value.to_owned())
}

#[test]
fn csv_and_tsv_read_their_declared_dialects() {
    let table = one_vertex_schema("rows");
    let csv = UnifiedCodec::csv("test").unwrap();
    let tsv = UnifiedCodec::tsv("test", "rows").unwrap();
    let read = |codec: &UnifiedCodec, input: &[u8]| {
        let (instance, complement) = codec.parse_functor_preserving(&table, input).unwrap();
        assert_eq!(
            codec
                .emit_functor_preserving(&table, &instance, &complement)
                .unwrap(),
            input,
            "unedited emission changed {:?}",
            String::from_utf8_lossy(input)
        );
        only_row(&instance)
    };

    assert_eq!(
        read(&csv, b"a,b,c\r\n\"x,\r\ny\",\"say \"\"hi\"\"\",\r\n"),
        [
            ("a".to_owned(), text("x,\r\ny")),
            ("b".to_owned(), text("say \"hi\"")),
            ("c".to_owned(), text("")),
        ]
    );
    // No null, missing or comment token, no trimming, no final line end.
    assert_eq!(
        read(&csv, b"#a, b,c\n_, null,NaN"),
        [
            (" b".to_owned(), text(" null")),
            ("#a".to_owned(), text("_")),
            ("c".to_owned(), text("NaN")),
        ]
    );
    // No delimiter detection: a `;` file is one column.
    assert_eq!(read(&csv, b"a;b\n1;2\n"), [("a;b".to_owned(), text("1;2"))]);
    assert_eq!(
        read(&tsv, b"a\tb\r\n\"x\"\t\"y,z\"\r\n"),
        [
            ("a".to_owned(), text("\"x\"")),
            ("b".to_owned(), text("\"y,z\""))
        ]
    );

    for (codec, input) in [
        (&csv, &b"a,b\n1\n"[..]),
        (&csv, b"a,b\n1,2,3\n"),
        (&csv, b"a,a\n1,2\n"),
        (&tsv, b"a\tb\n1\n"),
        (&tsv, b"a\ta\n1\t2\n"),
    ] {
        assert!(
            matches!(
                codec.parse_functor_preserving(&table, input),
                Err(ParseInstanceError::Parse { .. })
            ),
            "{:?} was not refused",
            String::from_utf8_lossy(input)
        );
    }
}
