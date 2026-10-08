#![allow(clippy::expect_used, clippy::unwrap_used)]
//! Deterministic schema-v2 path and amendment-law audits.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

use panproto_reference::bytes;
use panproto_reference::canonical::{ComplementShape, IdentityEnvelope, identity};
use panproto_reference::coerce::{
    FidelityReason, LexClass, NumeralDialect, ScalarKind, ValueSpec, coerce_dialect,
};
use panproto_reference::exact::Decimal;
use panproto_reference::keys::{KeyEquality, NumericPrimitive, SqlCollation, SqlPad};
use panproto_reference::model::{GEdge, Graph, Node, NodeShape, SEdge};
use panproto_reference::resolution::{rfc3986, standoff_form, standoff_inverse};
use panproto_reference::schema::{
    Change, Key, KeyPart, KeyRole, KeyScope, Schema, canonical_bytes, classify_change,
};
use panproto_reference::temporal::{TimeUnit, from_epoch_count, to_epoch_count};
use panproto_reference::value::Value;

fn read_dialect(
    text: &str,
    class: impl std::borrow::Borrow<LexClass>,
    kind: ScalarKind,
    numeral: NumeralDialect,
) -> Result<Value, FidelityReason> {
    coerce_dialect(
        text,
        class.borrow(),
        &ValueSpec {
            numeral: Some(numeral),
            ..ValueSpec::of(kind)
        },
    )
    .unwrap()
}

#[test]
fn manifest_reference_paths_resolve_at_compile_time() {
    assert_eq!(
        panproto_reference::theory::quotient(3, &[(0, 1), (1, 2)]),
        Some(vec![0, 0, 0])
    );
}

#[test]
fn identity_envelope_is_the_exact_version_two_layout() {
    let envelope = IdentityEnvelope {
        source_schema: [0x11; 32],
        target_schema: [0x22; 32],
        lens: [0x33; 32],
        shape: ComplementShape::Resolution,
        format_version: 0x5544,
    };
    let bytes = identity(&envelope);
    assert_eq!(bytes.len(), 101);
    assert_eq!(&bytes[..2], &[2, 0]);
    assert_eq!(&bytes[2..34], &[0x11; 32]);
    assert_eq!(&bytes[34..66], &[0x22; 32]);
    assert_eq!(&bytes[66..98], &[0x33; 32]);
    assert_eq!(bytes[98], 3);
    assert_eq!(&bytes[99..], &[0x44, 0x55]);

    for (shape, tag) in [
        (ComplementShape::WType, 0),
        (ComplementShape::Graph, 1),
        (ComplementShape::Functor, 2),
        (ComplementShape::Resolution, 3),
    ] {
        assert_eq!(identity(&IdentityEnvelope { shape, ..envelope })[98], tag);
    }
}

#[test]
fn numeral_dialect_probe_table_is_exact() {
    let int = ScalarKind::Int {
        bits: 64,
        signed: true,
    };
    let float = ScalarKind::Float { bits: 64 };

    assert_eq!(
        read_dialect("1_000", LexClass::Number, int, NumeralDialect::Toml),
        Ok(Value::Int(1_000))
    );
    assert_eq!(
        read_dialect("0x1F", LexClass::Number, int, NumeralDialect::Toml),
        Ok(Value::Int(31))
    );
    assert!(matches!(
        read_dialect("+inf", LexClass::Number, float, NumeralDialect::Toml),
        Ok(Value::Float(bits)) if bits == f64::INFINITY.to_bits()
    ));
    for invalid in ["1__0", "_1", "1_", ".5", "1."] {
        assert_eq!(
            read_dialect(invalid, LexClass::Number, int, NumeralDialect::Toml),
            Err(FidelityReason::NotANumber)
        );
    }
    assert_eq!(
        read_dialect("1.e2", LexClass::Number, float, NumeralDialect::Json),
        Err(FidelityReason::NotANumber)
    );

    assert_eq!(
        read_dialect("017", LexClass::Number, int, NumeralDialect::Yaml12),
        Ok(Value::Int(17))
    );
    assert_eq!(
        read_dialect("0o17", LexClass::Number, int, NumeralDialect::Yaml12),
        Ok(Value::Int(15))
    );
    assert_eq!(
        read_dialect("1_000", LexClass::Number, int, NumeralDialect::Yaml12),
        Err(FidelityReason::NotANumber)
    );
    assert_eq!(
        read_dialect("017", LexClass::Number, int, NumeralDialect::Yaml11),
        Ok(Value::Int(15))
    );
    assert_eq!(
        read_dialect("22:30", LexClass::Number, int, NumeralDialect::Yaml11),
        Ok(Value::Int(1_350))
    );
    assert_eq!(
        read_dialect("1234:30", LexClass::Number, int, NumeralDialect::Yaml11),
        Ok(Value::Int(74_070))
    );
    assert_eq!(
        read_dialect("1_000", LexClass::Number, int, NumeralDialect::Yaml11),
        Ok(Value::Int(1_000))
    );
    assert_eq!(
        read_dialect("1e3", LexClass::Number, float, NumeralDialect::Yaml11),
        Err(FidelityReason::NotANumber)
    );
}

#[test]
fn numeral_dialect_special_value_table_is_exact() {
    let int = ScalarKind::Int {
        bits: 64,
        signed: true,
    };
    let float = ScalarKind::Float { bits: 64 };

    assert!(matches!(
        read_dialect("INF", LexClass::String, float, NumeralDialect::Xsd),
        Ok(Value::Float(bits)) if bits == f64::INFINITY.to_bits()
    ));
    assert!(matches!(
        read_dialect("-NaN", LexClass::String, float, NumeralDialect::Xsd),
        Ok(Value::Float(bits)) if bits == 0xfff8_0000_0000_0000
    ));
    assert_eq!(
        read_dialect("inf", LexClass::String, float, NumeralDialect::Xsd),
        Err(FidelityReason::NotANumber)
    );
    assert_eq!(
        read_dialect("+1", LexClass::Number, int, NumeralDialect::Json),
        Err(FidelityReason::NotANumber)
    );
    assert_eq!(
        read_dialect("NaN", LexClass::String, float, NumeralDialect::Json),
        Err(FidelityReason::NotANumber)
    );
    assert!(matches!(
        read_dialect("Infinity", LexClass::String, float, NumeralDialect::Java),
        Ok(Value::Float(bits)) if bits == f64::INFINITY.to_bits()
    ));
    assert_eq!(
        read_dialect("inf", LexClass::String, float, NumeralDialect::Java),
        Err(FidelityReason::NotANumber)
    );
    assert!(matches!(
        read_dialect("NaN", LexClass::String, float, NumeralDialect::ProtoJson),
        Ok(Value::Float(bits)) if bits == f64::NAN.to_bits()
    ));
    assert_eq!(
        read_dialect("1e2", LexClass::String, int, NumeralDialect::ProtoJson),
        Ok(Value::Int(100))
    );
    assert!(matches!(
        read_dialect("Infinity", LexClass::String, float, NumeralDialect::Sql),
        Ok(Value::Float(bits)) if bits == f64::INFINITY.to_bits()
    ));
}

#[test]
fn schema_bytes_are_injective_on_the_labelled_scope() {
    let schemas = panproto_reference::enumerate::schemas(2, &["a", "b"], &["x"], 2);
    let encodings: BTreeSet<_> = schemas.iter().map(canonical_bytes).collect();
    assert_eq!(encodings.len(), schemas.len());
}

fn schema_with_key(role: KeyRole, scope: KeyScope) -> Schema {
    let mut schema = Schema::default()
        .vertex("record", "object")
        .vertex("a", "object")
        .vertex("b", "object")
        .vertex("scalar", "string")
        .edge("record", "scalar", "property", Some("id"));
    schema.keys.insert(
        "record".to_owned(),
        vec![Key {
            id: "key".to_owned(),
            parts: vec![KeyPart::Path(vec![panproto_reference::model::SEdge::new(
                "record",
                "scalar",
                "property",
                Some("id"),
            )])],
            role,
            scope,
        }],
    );
    schema
}

#[test]
fn schema_change_classification_has_the_key_boundaries() {
    let unique = schema_with_key(KeyRole::Unique, KeyScope::Document);
    let primary = schema_with_key(KeyRole::Primary, KeyScope::Document);
    assert_eq!(classify_change(&unique, &primary), Change::Breaking);
    assert_eq!(
        classify_change(&primary, &unique),
        Change::BackwardCompatible
    );

    let within_a = schema_with_key(KeyRole::Unique, KeyScope::Within("a".to_owned()));
    let within_b = schema_with_key(KeyRole::Unique, KeyScope::Within("b".to_owned()));
    assert_eq!(classify_change(&within_a, &within_b), Change::Breaking);
    assert_eq!(classify_change(&within_b, &within_a), Change::Breaking);

    let mut no_key = unique.clone();
    no_key.keys.clear();
    assert_eq!(classify_change(&no_key, &unique), Change::Breaking);
    assert_eq!(
        classify_change(&unique, &no_key),
        Change::BackwardCompatible
    );
}

#[test]
fn latin1_and_base64_laws_hold_on_the_fast_exhaustive_scope() {
    let strings = panproto_reference::enumerate::byte_strings(2).unwrap();
    assert_eq!(strings.len(), 65_793);
    for bytes in strings {
        assert!(bytes::latin1_decode_encode(&bytes));
        assert!(bytes::padded_bytes_decode_encode(&bytes));
        let padded = bytes::encode_base64(&bytes);
        assert!(bytes::padded_text_encode_decode(&padded));
        let unpadded = padded.trim_end_matches('=');
        assert_eq!(bytes::decode_base64(unpadded, false), Ok(bytes));
    }
    assert!(!bytes::latin1_encode_decode("Ā"));
    assert!(bytes::latin1_encode_decode("\u{0}\u{ff}"));
}

#[test]
#[ignore = "full O12 A6 audit: all 16,843,009 byte strings through length three"]
fn padded_base64_laws_hold_on_the_contract_scope() {
    for bytes in panproto_reference::enumerate::byte_strings(3).unwrap() {
        assert!(bytes::padded_bytes_decode_encode(&bytes));
        let padded = bytes::encode_base64(&bytes);
        assert!(bytes::padded_text_encode_decode(&padded));
        assert_eq!(
            bytes::decode_base64(padded.trim_end_matches('='), false),
            Ok(bytes)
        );
    }
}

#[test]
fn epoch_count_conversions_are_two_sided_on_the_contract_centre() {
    for unit in [
        TimeUnit::Seconds,
        TimeUnit::Millis,
        TimeUnit::Micros,
        TimeUnit::Nanos,
    ] {
        for instant in [false, true] {
            for count in -10_000..=10_000 {
                let temporal = from_epoch_count(count, unit, instant).unwrap();
                assert_eq!(to_epoch_count(&temporal, unit, instant), Some(count));
            }
        }
    }
}

#[test]
fn rfc3986_matches_the_normal_and_abnormal_examples() {
    let base = "http://a/b/c/d;p?q";
    let examples = [
        ("g:h", "g:h"),
        ("g", "http://a/b/c/g"),
        ("./g", "http://a/b/c/g"),
        ("g/", "http://a/b/c/g/"),
        ("/g", "http://a/g"),
        ("//g", "http://g"),
        ("?y", "http://a/b/c/d;p?y"),
        ("g?y", "http://a/b/c/g?y"),
        ("#s", "http://a/b/c/d;p?q#s"),
        ("g#s", "http://a/b/c/g#s"),
        ("g?y#s", "http://a/b/c/g?y#s"),
        (";x", "http://a/b/c/;x"),
        ("g;x", "http://a/b/c/g;x"),
        ("g;x?y#s", "http://a/b/c/g;x?y#s"),
        ("", "http://a/b/c/d;p?q"),
        (".", "http://a/b/c/"),
        ("./", "http://a/b/c/"),
        ("..", "http://a/b/"),
        ("../", "http://a/b/"),
        ("../g", "http://a/b/g"),
        ("../..", "http://a/"),
        ("../../", "http://a/"),
        ("../../g", "http://a/g"),
        ("../../../g", "http://a/g"),
        ("../../../../g", "http://a/g"),
        ("/./g", "http://a/g"),
        ("/../g", "http://a/g"),
        ("g.", "http://a/b/c/g."),
        (".g", "http://a/b/c/.g"),
        ("g..", "http://a/b/c/g.."),
        ("..g", "http://a/b/c/..g"),
        ("./../g", "http://a/b/g"),
        ("./g/.", "http://a/b/c/g/"),
        ("g/./h", "http://a/b/c/g/h"),
        ("g/../h", "http://a/b/c/h"),
        ("g;x=1/./y", "http://a/b/c/g;x=1/y"),
        ("g;x=1/../y", "http://a/b/c/y"),
        ("g?y/./x", "http://a/b/c/g?y/./x"),
        ("g?y/../x", "http://a/b/c/g?y/../x"),
        ("g#s/./x", "http://a/b/c/g#s/./x"),
        ("g#s/../x", "http://a/b/c/g#s/../x"),
        ("http:g", "http:g"),
        ("g//h", "http://a/b/c/g//h"),
        ("//g//h", "http://g//h"),
    ];
    for (reference, expected) in examples {
        assert_eq!(
            rfc3986(base, reference).as_deref(),
            Some(expected),
            "{reference}"
        );
    }
}

#[test]
fn declared_key_relations_and_sort_order_have_the_stated_boundaries() {
    let one = Value::Int(1);
    let one_float = Value::Float(1.0f64.to_bits());
    assert_eq!(
        panproto_reference::keys::key_equivalence(&KeyEquality::NumericValue, &one, &one_float),
        Some(true)
    );
    assert_eq!(
        panproto_reference::keys::key_equivalence(
            &KeyEquality::NumericValueWithinKind(NumericPrimitive::Any),
            &one,
            &one_float,
        ),
        Some(false)
    );
    let nocase = KeyEquality::Sql {
        collation: SqlCollation::AsciiCaseInsensitive,
        pad: SqlPad::PadSpace,
    };
    assert_eq!(
        panproto_reference::keys::key_equivalence(
            &nocase,
            &Value::Str("A  ".into()),
            &Value::Str("a".into()),
        ),
        Some(true)
    );
    assert_eq!(
        panproto_reference::keys::sort_order(&Value::Int(1), &Value::Int(2)),
        Some(Ordering::Less)
    );
    assert_eq!(
        panproto_reference::keys::key_order(&[Value::Int(1), Value::Int(1), Value::Int(2)]),
        Some(true)
    );
    assert_eq!(
        panproto_reference::keys::key_order(&[Value::Int(2), Value::Int(1)]),
        Some(false)
    );

    let numeric_classes = [
        vec![
            Value::Float(f64::NEG_INFINITY.to_bits()),
            Value::Decimal(Decimal::Infinite { negative: true }),
        ],
        vec![Value::Int(0), Value::Float(0.0f64.to_bits())],
        vec![
            Value::Float(f64::INFINITY.to_bits()),
            Value::Decimal(Decimal::Infinite { negative: false }),
        ],
    ];
    for (left_class, left_values) in numeric_classes.iter().enumerate() {
        for (right_class, right_values) in numeric_classes.iter().enumerate() {
            let expected = left_class.cmp(&right_class);
            for left in left_values {
                for right in right_values {
                    assert_eq!(
                        panproto_reference::keys::sort_order(left, right),
                        Some(expected),
                        "{left:?} versus {right:?}"
                    );
                }
            }
        }
    }
    let nan = Value::Float(f64::NAN.to_bits());
    for numeric in numeric_classes.iter().flatten() {
        assert_eq!(panproto_reference::keys::sort_order(&nan, numeric), None);
        assert_eq!(panproto_reference::keys::sort_order(numeric, &nan), None);
    }
    assert_eq!(panproto_reference::keys::sort_order(&nan, &nan), None);
    assert_eq!(
        panproto_reference::keys::sort_order(&Value::Int(0), &Value::Str("0".into())),
        None
    );
}

#[test]
fn fan_incidence_is_neither_ancestry_nor_a_position_parent() {
    let within = SEdge::new("parent", "item", "ordered", Some("items"));
    let mut graph = Graph {
        nodes: [
            (0, Node::new(0, "scope")),
            (1, Node::new(1, "fan")),
            (2, Node::new(2, "item")),
            (3, Node::new(3, "parent")),
            (4, Node::new(4, "item")),
        ]
        .into(),
        edges: BTreeMap::new(),
    };
    graph.nodes.get_mut(&1).unwrap().shape = NodeShape::HyperEdge;
    graph.edges.insert(
        0,
        GEdge {
            src: 0,
            tgt: 1,
            label: SEdge::new("scope", "fan", "foreign", None),
            position: None,
        },
    );
    graph.edges.insert(
        1,
        GEdge {
            src: 1,
            tgt: 2,
            label: within.clone(),
            position: Some(0),
        },
    );
    graph.edges.insert(
        2,
        GEdge {
            src: 3,
            tgt: 4,
            label: within.clone(),
            position: Some(0),
        },
    );
    assert_eq!(
        panproto_reference::keys::nearest_ancestor(&graph, 2, "scope"),
        None
    );
    let position_key = Key {
        id: "position".to_owned(),
        parts: vec![KeyPart::Position { within }],
        role: KeyRole::Unique,
        scope: KeyScope::Global,
    };
    assert_eq!(
        panproto_reference::keys::key_value(&graph, 2, &position_key),
        None
    );
    assert_eq!(
        panproto_reference::keys::key_value(&graph, 4, &position_key),
        Some(vec![Value::Int(0)])
    );
}

#[test]
fn an_index_key_never_derives_a_resolution_pair() {
    for index in [None, Some(0), Some(1)] {
        for target in [None, Some(0), Some(1)] {
            assert!(!panproto_reference::resolution::index_target(index, target));
        }
    }
}

#[test]
fn standoff_forms_are_left_inverses_and_entities_are_bounded() {
    for origin in [0, 1] {
        for length_form in [false, true] {
            for start in 0..=4 {
                for end in start..=4 {
                    let written = standoff_inverse(start, end, origin, length_form).unwrap();
                    assert_eq!(
                        standoff_form(written.0, written.1, origin, length_form),
                        Some((start, end))
                    );
                }
            }
        }
    }
    let declarations = BTreeMap::from([
        ("a".to_owned(), "A".to_owned()),
        ("b".to_owned(), "&a;B".to_owned()),
    ]);
    assert_eq!(
        panproto_reference::xml::expand_entities("x&b;y", &declarations).as_deref(),
        Ok("xABy")
    );
    assert_eq!(
        panproto_reference::xml::expand_entities(
            "&amp;&lt;&gt;&apos;&quot; &b; &amp;b;",
            &declarations,
        )
        .as_deref(),
        Ok("&<>'\" AB &b;")
    );
    assert_eq!(
        panproto_reference::xml::expand_entities(
            "&#65;&#x41;&#9;&#xA;&#13;&#x10FFFF;",
            &declarations,
        )
        .as_deref(),
        Ok("AA\t\n\r\u{10ffff}")
    );
    for malformed in [
        "&#;",
        "&#x;",
        "&#xg;",
        "&#0;",
        "&#1;",
        "&#xD800;",
        "&#xFFFE;",
        "&#xFFFF;",
        "&#999999999999;",
        "&;",
        "&1x;",
        "&x y;",
    ] {
        assert_eq!(
            panproto_reference::xml::expand_entities(malformed, &declarations),
            Err(panproto_reference::xml::EntityError::Malformed),
            "{malformed}",
        );
    }
    let recursive = BTreeMap::from([("r".to_owned(), "&r;".to_owned())]);
    assert_eq!(
        panproto_reference::xml::expand_entities("&r;", &recursive),
        Err(panproto_reference::xml::EntityError::Recursive(
            "r".to_owned()
        ))
    );

    let mut sixteen = BTreeMap::new();
    for depth in 0..16 {
        let replacement = if depth == 15 {
            "x".to_owned()
        } else {
            format!("&e{};", depth + 1)
        };
        sixteen.insert(format!("e{depth}"), replacement);
    }
    assert_eq!(
        panproto_reference::xml::expand_entities("&e0;", &sixteen).as_deref(),
        Ok("x")
    );
    sixteen.insert("e15".to_owned(), "&e16;".to_owned());
    sixteen.insert("e16".to_owned(), "x".to_owned());
    assert_eq!(
        panproto_reference::xml::expand_entities("&e0;", &sixteen),
        Err(panproto_reference::xml::EntityError::Depth)
    );
}

#[test]
fn tabular_edge_cases_are_literal() {
    assert!(panproto_reference::value_io::tsv_comment("#data"));
    assert!(panproto_reference::value_io::tabular_missing("_"));
    assert!(panproto_reference::value_io::tabular_empty_row(1, ""));
    assert!(panproto_reference::value_io::tsv_quote("x\"y"));
    assert!(!panproto_reference::value_io::tsv_quote("x\ty"));
}
