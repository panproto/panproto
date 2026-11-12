//! The bson reference on the examples and refusals the page states, and
//! the closed-form counts of its scopes.

use panproto_reference::enumerate::bson::{TEXTS, documents, documents_count, values};
use panproto_reference::pages::bson::{
    Bson, Decimal, DecodeRefusal, ReadRefusal, TreeNode, WriteRefusal, canonical_text,
    decimal_text, decode, double_text, encode, read_text, relaxed_text, tree,
};

fn hex(text: &str) -> Vec<u8> {
    let text: String = text.split_whitespace().collect();
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap_or_else(|e| panic!("{e}")))
        .collect()
}

#[test]
fn a_binary_document_reads_by_the_bson_grammar_and_encodes_back() {
    let bytes = hex(
        "2F000000 10 69 00 01000000 12 6C 00 0200000000000000 02 73 00 02000000 61 00 \
         04 61 00 0C000000 08 30 00 01 0A 31 00 00 00",
    );
    let document = decode(&bytes).unwrap_or_else(|e| panic!("{e:?}"));
    assert_eq!(
        document,
        vec![
            ("i".to_owned(), Bson::Int32(1)),
            ("l".to_owned(), Bson::Int64(2)),
            ("s".to_owned(), Bson::String("a".to_owned())),
            (
                "a".to_owned(),
                Bson::Array(vec![Bson::Boolean(true), Bson::Null])
            ),
        ]
    );
    assert_eq!(encode(&document), bytes);
}

#[test]
fn binary_refusals_follow_the_order_the_page_states() {
    let refused = |text: &str| decode(&hex(text)).map(|_| ()).err();
    assert_eq!(
        refused("04000000"),
        Some((0, DecodeRefusal::DocumentLength(4)))
    );
    assert_eq!(refused("050000"), Some((0, DecodeRefusal::Truncated)));
    assert_eq!(
        refused("05000000 01"),
        Some((4, DecodeRefusal::Terminator(1)))
    );
    assert_eq!(
        refused("05000000 00 00"),
        Some((5, DecodeRefusal::TrailingBytes))
    );
    assert_eq!(
        refused("07000000 14 61 00"),
        Some((4, DecodeRefusal::UnknownType(0x14)))
    );
    assert_eq!(
        refused("09000000 08 61 00 02 00"),
        Some((7, DecodeRefusal::Boolean(2)))
    );
    assert_eq!(
        refused("13000000 05 61 00 06000000 02 03000000 FFFF 00"),
        Some((12, DecodeRefusal::OldBinaryLength(3)))
    );
    assert_eq!(
        refused("0E000000 02 61 00 01000000 00 00 00"),
        Some((12, DecodeRefusal::EarlyTerminator))
    );
    assert_eq!(
        refused("18000000 13 64 00 00000000000000000000000000001E78 00"),
        Some((7, DecodeRefusal::NonCanonicalDecimal128))
    );
}

#[test]
fn decimal128_and_double_strings_follow_the_page() {
    let finite = |negative, coefficient, exponent| Decimal::Finite {
        negative,
        coefficient,
        exponent,
    };
    assert_eq!(decimal_text(&finite(false, 150, -2)), "1.50");
    assert_eq!(decimal_text(&finite(true, 0, 0)), "-0");
    assert_eq!(decimal_text(&finite(false, 1, -7)), "1E-7");
    assert_eq!(decimal_text(&finite(false, 10, 6111)), "1.0E+6112");
    assert_eq!(decimal_text(&Decimal::NaN(true, true, 9)), "NaN");
    assert_eq!(double_text(1.0_f64.to_bits()).as_deref(), Some("1.0"));
    assert_eq!(
        double_text(1.234_567_892_123_2e18_f64.to_bits()).as_deref(),
        Some("1.2345678921232E+18")
    );
    assert_eq!(double_text(0.0001_f64.to_bits()).as_deref(), Some("0.0001"));
    assert_eq!(double_text(f64::NAN.to_bits()), None);
}

#[test]
fn extended_json_writes_both_formats_and_refuses_what_does_not_read_back() {
    let document = vec![
        ("n".to_owned(), Bson::Int64(5)),
        ("d".to_owned(), Bson::DateTime(0)),
        ("x".to_owned(), Bson::Double(1.0_f64.to_bits())),
    ];
    assert_eq!(
        canonical_text(&document).as_deref(),
        Ok(
            r#"{"n":{"$numberLong":"5"},"d":{"$date":{"$numberLong":"0"}},"x":{"$numberDouble":"1.0"}}"#
        )
    );
    assert_eq!(
        relaxed_text(&document).as_deref(),
        Ok(r#"{"n":5,"d":{"$date":"1970-01-01T00:00:00Z"},"x":1.0}"#)
    );
    let wrapped = vec![(
        "a".to_owned(),
        Bson::Document(vec![("$date".to_owned(), Bson::Null)]),
    )];
    assert_eq!(
        canonical_text(&wrapped),
        Err(("/a/$date".to_owned(), WriteRefusal::WrapperKey))
    );
}

#[test]
fn extended_json_reads_by_the_conversion_table_and_names_the_place() {
    assert_eq!(
        read_text(r#"{"a":1,"b":3000000000,"c":1.0}"#),
        Ok(vec![
            ("a".to_owned(), Bson::Int32(1)),
            ("b".to_owned(), Bson::Int64(3_000_000_000)),
            ("c".to_owned(), Bson::Double(1.0_f64.to_bits())),
        ])
    );
    assert_eq!(
        read_text(r#"{"a":{"$date":"1970-01-01T01:00:00+01:00"}}"#),
        Ok(vec![("a".to_owned(), Bson::DateTime(0))])
    );
    assert_eq!(
        read_text(r#"{"a":{"$oid":"x","y":1}}"#),
        Err(("/a".to_owned(), ReadRefusal::WrapperKeys("$oid")))
    );
    assert_eq!(
        read_text(r#"{"a":{"$numberDecimal":"1E+6112"}}"#),
        Err((
            "/a/$numberDecimal".to_owned(),
            ReadRefusal::DecimalRepresentation
        ))
    );
    assert_eq!(
        read_text("[]"),
        Err((String::new(), ReadRefusal::NotADocument))
    );
    assert_eq!(
        read_text(r#"{"a":1,"a":1}"#),
        Err((String::new(), ReadRefusal::Json))
    );
}

#[test]
fn the_tree_encoding_numbers_nodes_in_document_order() {
    let document = vec![(
        "c".to_owned(),
        Bson::CodeWithScope("f".to_owned(), vec![("x".to_owned(), Bson::MinKey)]),
    )];
    assert_eq!(
        tree(&document),
        vec![
            TreeNode {
                parent: None,
                anchor: "document",
                held: None
            },
            TreeNode {
                parent: Some((0, "element")),
                anchor: "element",
                held: Some("str c".to_owned())
            },
            TreeNode {
                parent: Some((1, "value")),
                anchor: "code-with-scope",
                held: Some("str f".to_owned())
            },
            TreeNode {
                parent: Some((2, "scope")),
                anchor: "document",
                held: None
            },
            TreeNode {
                parent: Some((3, "element")),
                anchor: "element",
                held: Some("str x".to_owned())
            },
            TreeNode {
                parent: Some((4, "value")),
                anchor: "min-key",
                held: None
            },
        ]
    );
}

#[test]
fn the_bson_scopes_have_their_closed_form_counts() {
    assert_eq!(values().len(), 31);
    assert_eq!(documents().len(), documents_count(31));
    assert_eq!(documents_count(31), 994);
    assert_eq!(TEXTS.len(), 62);
}
