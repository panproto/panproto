//! BSON documents read from their binary form and from Extended JSON, the
//! refusals of each reader and writer with the place they name, and the
//! BSON tree encoding.

#![cfg(feature = "tree-sitter")]

use panproto_expr::exact::DecimalValue;
use panproto_inst::{FieldPresence, Node, Value};
use panproto_io::data_schema::bson::{
    BsonCodec, BsonDecodeErrorKind, BsonDocument, BsonInstanceErrorKind, BsonValue,
    BsonWriteErrorKind, ExtJsonErrorKind, Mode, decode, encode, from_instance, read_extended_json,
    to_instance, write_extended_json,
};
use panproto_io::{InstanceEmitter, InstanceParser};

fn hex(text: &str) -> Vec<u8> {
    let text: String = text.split_whitespace().collect();
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap_or_else(|e| panic!("{e}")))
        .collect()
}

/// The error of a result that must be a refusal.
fn refusal_of<T: std::fmt::Debug, E>(result: Result<T, E>) -> E {
    match result {
        Ok(value) => panic!("accepted as {value:?}"),
        Err(error) => error,
    }
}

fn document(elements: Vec<(&str, BsonValue)>) -> BsonDocument {
    BsonDocument(
        elements
            .into_iter()
            .map(|(k, v)| (k.to_owned(), v))
            .collect(),
    )
}

/// A document holding a value of every BSON 1.1 type.
fn every_type() -> BsonDocument {
    let scope = document(vec![("x", BsonValue::Int32(1))]);
    document(vec![
        ("double", BsonValue::Double(-1.5)),
        ("string", BsonValue::String("a\u{0}b é".to_owned())),
        ("document", BsonValue::Document(scope.clone())),
        (
            "array",
            BsonValue::Array(vec![BsonValue::Null, BsonValue::Boolean(true)]),
        ),
        (
            "binary",
            BsonValue::Binary {
                subtype: 0x80,
                bytes: vec![1, 2, 3],
            },
        ),
        (
            "old binary",
            BsonValue::Binary {
                subtype: 2,
                bytes: vec![0xFF, 0xFF],
            },
        ),
        ("undefined", BsonValue::Undefined),
        ("oid", BsonValue::ObjectId([7; 12])),
        ("bool", BsonValue::Boolean(false)),
        ("date", BsonValue::DateTime(-284_643_869_501)),
        ("null", BsonValue::Null),
        (
            "regex",
            BsonValue::Regex {
                pattern: "a/b".to_owned(),
                options: "imx".to_owned(),
            },
        ),
        (
            "pointer",
            BsonValue::DbPointer {
                namespace: "db.c".to_owned(),
                id: [9; 12],
            },
        ),
        ("code", BsonValue::Code("f()".to_owned())),
        ("symbol", BsonValue::Symbol("s".to_owned())),
        (
            "scoped",
            BsonValue::CodeWithScope {
                code: "g()".to_owned(),
                scope,
            },
        ),
        ("int32", BsonValue::Int32(-7)),
        (
            "timestamp",
            BsonValue::Timestamp {
                time: 4_000_000_000,
                increment: 42,
            },
        ),
        ("int64", BsonValue::Int64(1 << 40)),
        (
            "decimal",
            BsonValue::Decimal128(
                DecimalValue::parse("-1.50E-3").unwrap_or_else(|e| panic!("{e}")),
            ),
        ),
        ("min", BsonValue::MinKey),
        ("max", BsonValue::MaxKey),
        ("max", BsonValue::Int32(2)),
    ])
}

#[test]
fn a_document_of_every_type_round_trips_through_bytes_and_the_tree_encoding() {
    let original = every_type();
    let bytes = encode(&original).unwrap_or_else(|e| panic!("{e}"));
    let decoded = decode(&bytes).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(decoded, original);
    let instance = to_instance(&decoded).unwrap_or_else(|e| panic!("{e}"));
    let back = from_instance(&instance).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(back, original);
    let schema =
        panproto_protocols::data_schema::bson::tree_schema().unwrap_or_else(|e| panic!("{e}"));
    let codec = BsonCodec;
    let parsed = codec
        .parse_wtype(&schema, &bytes)
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(
        codec
            .emit_wtype(&schema, &parsed)
            .unwrap_or_else(|e| panic!("{e}")),
        bytes
    );
}

#[test]
fn canonical_extended_json_reads_back_as_the_same_document() {
    let mut original = every_type();
    original.0.pop();
    let text = write_extended_json(&original, Mode::Canonical).unwrap_or_else(|e| panic!("{e}"));
    assert!(
        text.starts_with(r#"{"double":{"$numberDouble":"-1.5"},"string":"a\u0000b é","#),
        "{text}"
    );
    let read = read_extended_json(text.as_bytes()).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(read, original);
}

#[test]
fn relaxed_extended_json_writes_numbers_and_dates_as_json_and_reads_by_range() {
    let original = document(vec![
        ("a", BsonValue::Int32(1)),
        ("b", BsonValue::Int64(3_000_000_000)),
        ("c", BsonValue::Int64(5)),
        ("d", BsonValue::Double(1.0)),
        ("e", BsonValue::DateTime(1_356_351_330_501)),
        ("f", BsonValue::DateTime(-1)),
    ]);
    let text = write_extended_json(&original, Mode::Relaxed).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(
        text,
        r#"{"a":1,"b":3000000000,"c":5,"d":1.0,"e":{"$date":"2012-12-24T12:15:30.501Z"},"f":{"$date":{"$numberLong":"-1"}}}"#
    );
    let read = read_extended_json(text.as_bytes()).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(read.0[2].1, BsonValue::Int32(5));
    let offset = read_extended_json(br#"{"e":{"$date":"2012-12-24T13:15:30.501+01:00"}}"#)
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(offset.0[0].1, BsonValue::DateTime(1_356_351_330_501));
}

#[test]
fn binary_refusals_name_the_byte_offset_and_the_rule() {
    let refused = |text: &str| {
        let error = refusal_of(decode(&hex(text)));
        (error.offset, error.kind)
    };
    assert_eq!(
        refused("05000000 01"),
        (4, BsonDecodeErrorKind::Terminator { byte: 1 })
    );
    assert_eq!(
        refused("05000000 00 00"),
        (5, BsonDecodeErrorKind::TrailingBytes)
    );
    assert_eq!(
        refused("07000000 14 61 00"),
        (4, BsonDecodeErrorKind::UnknownType { byte: 0x14 })
    );
    assert_eq!(
        refused("0D000000 10 61 FF 00 01000000 00"),
        (6, BsonDecodeErrorKind::InvalidUtf8)
    );
    assert_eq!(
        refused("09000000 08 61 00 02 00"),
        (7, BsonDecodeErrorKind::Boolean { byte: 2 })
    );
    assert_eq!(
        refused("14000000 04 61 00 0C000000 10 31 00 0A000000 00 00"),
        (
            12,
            BsonDecodeErrorKind::ArrayKey {
                expected: "0".to_owned()
            }
        )
    );
    assert_eq!(
        refused("0E000000 02 61 00 01000000 00 00 00"),
        (12, BsonDecodeErrorKind::EarlyTerminator)
    );
    assert_eq!(
        refused("0C000000 02 61 00 FFFFFFFF 00"),
        (7, BsonDecodeErrorKind::StringLength { declared: -1 })
    );
    assert_eq!(
        refused("0D000000 0B 61 00 61 00 6D 69 00 00"),
        (9, BsonDecodeErrorKind::RegexOptions)
    );
    assert_eq!(
        refused("18000000 13 64 00 0000000000000000 000000000000106C 00"),
        (7, BsonDecodeErrorKind::NonCanonicalDecimal128)
    );
    assert_eq!(
        refused("04000000"),
        (0, BsonDecodeErrorKind::DocumentLength { declared: 4 })
    );
}

#[test]
fn deep_nesting_is_refused_at_the_level_past_the_bound() {
    let mut bytes = vec![0x05, 0, 0, 0, 0];
    for _ in 0..128 {
        let mut outer = vec![0, 0, 0, 0, 0x03, b'a', 0];
        outer.extend_from_slice(&bytes);
        outer.push(0);
        let length = i32::try_from(outer.len()).unwrap_or_else(|e| panic!("{e}"));
        outer[..4].copy_from_slice(&length.to_le_bytes());
        bytes = outer;
    }
    let error = refusal_of(decode(&bytes));
    assert_eq!(error.kind, BsonDecodeErrorKind::Depth);
    assert_eq!(error.offset, 128 * 7);
}

#[test]
fn extended_json_refusals_name_the_path_offset_and_rule() {
    let refused = |text: &str| {
        let error = refusal_of(read_extended_json(text.as_bytes()));
        (error.path, error.offset, error.kind)
    };
    assert_eq!(
        refused(r#"{"a":{"$numberInt":42}}"#),
        (
            "/a/$numberInt".to_owned(),
            19,
            ExtJsonErrorKind::WrapperValue("$numberInt")
        )
    );
    assert_eq!(
        refused(r#"{"a":[{"$oid":"56e1fc72e0c917e9c4714161","x":1}]}"#),
        ("/a/0".to_owned(), 6, ExtJsonErrorKind::WrapperKeys("$oid"))
    );
    assert_eq!(
        refused("[1]"),
        (String::new(), 0, ExtJsonErrorKind::NotADocument)
    );
    assert_eq!(
        refused(r#"{"a":12345678901234567890}"#),
        ("/a".to_owned(), 5, ExtJsonErrorKind::IntegerRange)
    );
    assert_eq!(
        refused(r#"{"a":0.10000000000000000001}"#),
        ("/a".to_owned(), 5, ExtJsonErrorKind::InexactNumber)
    );
    assert_eq!(
        refused(r#"{"a":{"$numberDecimal":"1E+6112"}}"#),
        (
            "/a/$numberDecimal".to_owned(),
            23,
            ExtJsonErrorKind::DecimalRepresentation("1E+6112".to_owned())
        )
    );
    assert_eq!(
        refused(r#"{"a":{"$regularExpression":{"pattern":"p","options":"mi"}}}"#),
        (
            "/a/$regularExpression".to_owned(),
            27,
            ExtJsonErrorKind::RegexOptions
        )
    );
    assert_eq!(
        refused(r#"{"a":{"$date":42}}"#),
        (
            "/a/$date".to_owned(),
            14,
            ExtJsonErrorKind::WrapperValue("$date")
        )
    );
}

#[test]
fn ordinary_dollar_keys_read_as_document_members() {
    let read = read_extended_json(
        br#"{"$key":1,"q":{"$regex":"^a","$options":"i"},"t":{"$type":"string"},"r":{"$ref":"c","$id":2}}"#,
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let keys: Vec<&str> = read.0.iter().map(|(k, _)| k.as_str()).collect();
    assert_eq!(keys, ["$key", "q", "t", "r"]);
    assert_eq!(
        read.0[2].1,
        BsonValue::Document(document(vec![(
            "$type",
            BsonValue::String("string".to_owned())
        )]))
    );
}

#[test]
fn writers_refuse_documents_no_text_or_bytes_read_back_as() {
    let refusal = |document: &BsonDocument, mode: Mode| {
        let error = refusal_of(write_extended_json(document, mode));
        (error.path, error.kind)
    };
    let twice = document(vec![("a", BsonValue::Null), ("a", BsonValue::Null)]);
    assert_eq!(
        refusal(&twice, Mode::Canonical),
        (
            "/a".to_owned(),
            BsonWriteErrorKind::DuplicateKey("a".to_owned())
        )
    );
    assert!(encode(&twice).is_ok());
    let wrapper = document(vec![(
        "a",
        BsonValue::Document(document(vec![("$oid", BsonValue::Null)])),
    )]);
    assert_eq!(
        refusal(&wrapper, Mode::Relaxed),
        (
            "/a/$oid".to_owned(),
            BsonWriteErrorKind::WrapperKey("$oid".to_owned())
        )
    );
    let payload = document(vec![(
        "d",
        BsonValue::Double(f64::from_bits(0x7FF8_0000_0000_0012)),
    )]);
    assert_eq!(
        refusal(&payload, Mode::Canonical),
        ("/d".to_owned(), BsonWriteErrorKind::NaN)
    );
    let nul = document(vec![("a\u{0}", BsonValue::Null)]);
    assert_eq!(refusal_of(encode(&nul)).kind, BsonWriteErrorKind::NulInKey);
    let unsorted = document(vec![(
        "r",
        BsonValue::Regex {
            pattern: String::new(),
            options: "xi".to_owned(),
        },
    )]);
    assert_eq!(
        refusal_of(encode(&unsorted)).kind,
        BsonWriteErrorKind::RegexOptions
    );
}

#[test]
fn instances_outside_the_encoding_are_refused_at_the_node() {
    let original = document(vec![("a", BsonValue::Int32(1))]);
    let instance = to_instance(&original).unwrap_or_else(|e| panic!("{e}"));
    let refused = |instance: &panproto_inst::WInstance| {
        let error = refusal_of(from_instance(instance));
        (error.node, error.kind)
    };

    let mut wide = instance.clone();
    if let Some(node) = wide.nodes.get_mut(&2) {
        node.value = Some(FieldPresence::Present(Value::Int(1 << 40)));
    }
    assert_eq!(refused(&wide), (2, BsonInstanceErrorKind::Value));

    let mut orphan = instance.clone();
    orphan.nodes.insert(9, Node::new(9, "null"));
    assert_eq!(refused(&orphan), (9, BsonInstanceErrorKind::Unreachable));

    let mut foreign = instance.clone();
    if let Some(node) = foreign.nodes.get_mut(&1) {
        node.anchor = "field".into();
    }
    assert_eq!(
        refused(&foreign),
        (
            0,
            BsonInstanceErrorKind::UnexpectedEdge {
                name: "element".to_owned()
            }
        )
    );

    let mut positioned = instance;
    if let Some(node) = positioned.nodes.get_mut(&1) {
        node.position = Some(0);
    }
    assert_eq!(
        refused(&positioned),
        (1, BsonInstanceErrorKind::Presentation)
    );
}

#[test]
fn the_codec_refuses_a_schema_without_the_tree_encoding() {
    let protocol = panproto_protocols::data_schema::bson::protocol();
    let schema = panproto_schema::SchemaBuilder::new(&protocol)
        .vertex("document", "document", None)
        .and_then(panproto_schema::SchemaBuilder::build)
        .unwrap_or_else(|e| panic!("{e}"));
    let error = refusal_of(BsonCodec.parse_wtype(&schema, &hex("05000000 00")));
    assert!(
        error
            .to_string()
            .contains("does not hold the BSON tree encoding"),
        "{error}"
    );
}
