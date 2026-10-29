//! Supplemental direct witnesses for value codecs and public value-I/O operations.

use std::collections::BTreeMap;

use panproto_reference::coerce::{FidelityReason, LexClass, ScalarKind, ValueSpec};
use panproto_reference::exact::{BigIntValue, Decimal, DecimalValue};
use panproto_reference::float::{Format, Rounded};
use panproto_reference::num::{Int, Nat, Rat};
use panproto_reference::temporal::{Offset, OffsetForm, TemporalKind, ZuluCase};
use panproto_reference::value::Value;
use panproto_reference::{canonical, coerce, float, tabular, temporal, toml, value, value_io, xml};

trait TestUnwrap<T> {
    #[track_caller]
    fn test_unwrap(self, context: &str) -> T;
}

impl<T, E: std::fmt::Debug> TestUnwrap<T> for Result<T, E> {
    fn test_unwrap(self, context: &str) -> T {
        self.unwrap_or_else(|error| panic!("{context}: {error:?}"))
    }
}

impl<T> TestUnwrap<T> for Option<T> {
    fn test_unwrap(self, context: &str) -> T {
        self.unwrap_or_else(|| panic!("{context}"))
    }
}

#[test]
fn v1_decimal_bytes_cover_infinity_and_nan_payload_layouts() {
    assert_eq!(
        canonical::decimal_bytes(&Decimal::Infinite { negative: false }),
        Some(vec![13, 2, 0]),
        "positive infinity has the decimal value tag, infinity arm, and positive sign"
    );
    assert_eq!(
        canonical::decimal_bytes(&Decimal::NaN {
            negative: true,
            signalling: true,
            payload: Nat::from_u64(42),
        }),
        Some(vec![13, 3, 1, 1, 2, 0, 0, 0, 0, 0, 0, 0, b'4', b'2']),
        "NaN sign, signalling bit, payload length, and payload digits are all observable"
    );
}

#[test]
fn v2_decode_value_distinguishes_decimal_and_bigint_tags_and_refuses_bad_bytes() {
    assert_eq!(
        canonical::decode_value(&[13, 2, 0]),
        Some(Value::Decimal(Decimal::Infinite { negative: false })),
        "tag 13 must decode as a special decimal"
    );
    assert_eq!(
        canonical::decode_value(&[
            14, 0, 1, 0, 0, 0, 0, 0, 0, 0, b'1', 2, 0, 0, 0, 0, 0, 0, 0, b'1', b'9',
        ]),
        Some(Value::BigInt(BigIntValue {
            negative: false,
            significand: Nat::from_u64(1),
            zeros: Nat::from_u64(19),
        })),
        "tag 14 must decode the canonical integer 10^19"
    );
    assert_eq!(
        canonical::decode_value(&[
            14, 0, 1, 0, 0, 0, 0, 0, 0, 0, b'1', 1, 0, 0, 0, 0, 0, 0, 0, b'0',
        ]),
        None,
        "a tag-14 value that belongs in i64 is not a canonical BigInt"
    );
    assert_eq!(
        canonical::decode_value(&[13, 2, 0, 99]),
        None,
        "the decoder refuses trailing bytes"
    );
}

#[test]
fn v3_coerce_tagged_uses_the_table_and_preserves_unknown_tags() {
    let table = BTreeMap::from([
        (
            "!int".to_owned(),
            ScalarKind::Int {
                bits: 64,
                signed: true,
            },
        ),
        ("!text".to_owned(), ScalarKind::Str),
    ]);
    let any = ValueSpec::of(ScalarKind::Any);

    assert_eq!(
        coerce::coerce_tagged("12", &LexClass::Number, Some("!int"), &table, &any),
        Some(Ok(Value::Int(12))),
        "a known discriminator selects its declared integer kind"
    );
    assert_eq!(
        coerce::coerce_tagged("12", &LexClass::Number, Some("!other"), &table, &any),
        Some(Ok(Value::Opaque {
            type_: "!other".to_owned(),
            fields: BTreeMap::from([("value".to_owned(), Value::Str("12".to_owned()))]),
        })),
        "an unknown discriminator is preserved as an opaque tagged value"
    );
    assert_eq!(
        coerce::coerce_tagged("12", &LexClass::Number, Some("!text"), &table, &any),
        Some(Err(FidelityReason::NotANumberForKind)),
        "the original lexical class still constrains a table-selected string kind"
    );
}

#[test]
fn v4_binary16_matches_published_rounding_and_signed_zero_patterns() {
    let one_tenth = Rat::new(Int::from_i64(1), Nat::from_u64(10))
        .test_unwrap("the literal denominator is nonzero");

    assert_eq!(
        float::binary16(&one_tenth, false),
        Rounded::Finite(0x2e66),
        "binary16 rounds exact 1/10 to the published nearest-even pattern"
    );
    assert_eq!(
        float::binary16(&Rat::zero(), true),
        Rounded::Finite(0x8000),
        "the supplied sign is retained for exact zero"
    );
}

#[test]
fn v5_computed_nan_sets_only_the_canonical_quiet_nan_bits() {
    assert_eq!(float::computed_nan(Format::B16), 0x7e00);
    assert_eq!(float::computed_nan(Format::B32), 0x7fc0_0000);
    assert_eq!(float::computed_nan(Format::B64), 0x7ff8_0000_0000_0000);
}

#[test]
fn v6_temporal_canonical_bytes_are_literal_invertible_and_refuse_invalid_complete_values() {
    let valid = temporal::parse(TemporalKind::DateTime, "2024-01-02T03:04:05Z")
        .test_unwrap("the literal is a valid complete date-time");
    let expected = vec![
        15, 2, 232, 7, 0, 0, 0, 0, 0, 0, 1, 2, b'T', 3, 4, 5, 0, 0, 0, 0, 0, 0, 0, 0, 1, b'Z',
    ];

    assert_eq!(
        temporal::canonical_bytes(&valid),
        Some(expected.clone()),
        "the complete legacy date-time layout remains byte-for-byte stable"
    );
    assert_eq!(
        canonical::decode_value(&expected),
        Some(Value::Temporal(valid.clone())),
        "the independent public decoder is a left inverse of the literal temporal bytes"
    );
    let mut invalid_month_bytes = expected.clone();
    invalid_month_bytes[10] = 13;
    assert_eq!(
        canonical::decode_value(&invalid_month_bytes),
        None,
        "the legacy complete-date decoder refuses an out-of-domain month"
    );
    let mut invalid_hour_bytes = expected;
    invalid_hour_bytes[13] = 24;
    assert_eq!(
        canonical::decode_value(&invalid_hour_bytes),
        None,
        "the legacy complete-time decoder refuses an out-of-domain hour"
    );

    let mut nondigit_fraction = valid.clone();
    nondigit_fraction
        .time
        .as_mut()
        .test_unwrap("the literal contains a time")
        .fraction = "x".to_owned();
    assert!(!nondigit_fraction.well_formed());
    assert_eq!(
        temporal::canonical_bytes(&nondigit_fraction),
        None,
        "a complete-layout shape cannot bypass the fractional-digit domain"
    );

    let mut invalid_clock = valid;
    invalid_clock
        .time
        .as_mut()
        .test_unwrap("the literal contains a time")
        .hour = Some(24);
    assert!(!invalid_clock.well_formed());
    assert_eq!(
        temporal::canonical_bytes(&invalid_clock),
        None,
        "a complete-layout shape cannot bypass clock-field bounds"
    );
}

#[test]
fn v7_unwritten_zulu_accepts_only_the_unwritten_zulu_variant() {
    let mut value = temporal::parse(TemporalKind::DateTime, "2024-01-02T03:04:05Z")
        .test_unwrap("the literal is a valid complete date-time");
    assert!(!temporal::unwritten_zulu(&value));

    value.offset = Offset::Zulu(ZuluCase::Unwritten);
    assert!(temporal::unwritten_zulu(&value));

    value.offset = Offset::Fixed(0, OffsetForm::Unwritten);
    assert!(
        !temporal::unwritten_zulu(&value),
        "an unwritten numeric zero offset is not unwritten Zulu"
    );
}

#[test]
fn v8_representation_observes_decimal_scale() {
    let scaled = Value::Decimal(Decimal::Finite(
        DecimalValue::parse("1.50").test_unwrap("the literal is a decimal"),
    ));
    let minimal = Value::Decimal(Decimal::Finite(
        DecimalValue::parse("1.5").test_unwrap("the literal is a decimal"),
    ));

    assert_eq!(value::representation(&scaled, &scaled), Some(true));
    assert_eq!(
        value::representation(&scaled, &minimal),
        Some(false),
        "numeric equality does not erase written decimal scale"
    );
}

#[test]
fn i1_tabular_missing_paths_recognize_only_the_literal_missing_token() {
    assert!(value_io::tabular_missing("_"));
    assert!(!value_io::tabular_missing(""));
    assert!(!value_io::tabular_missing("__"));
    assert!(tabular::missing_token_cst("_"));
    assert!(!tabular::missing_token_cst(""));
    assert!(!tabular::missing_token_cst("__"));
}

#[test]
fn i2_tabular_empty_row_paths_require_one_column_and_an_empty_line() {
    assert!(value_io::tabular_empty_row(1, ""));
    assert!(!value_io::tabular_empty_row(2, ""));
    assert!(!value_io::tabular_empty_row(1, "x"));
    assert!(tabular::one_column_empty_cst(1, ""));
    assert!(!tabular::one_column_empty_cst(2, ""));
    assert!(!tabular::one_column_empty_cst(1, "x"));
}

#[test]
fn i3_toml_scalar_paths_preserve_decimal_scale_and_bigint_digits() {
    let decimal = panproto_reference::coerce::ScalarKind::Decimal;
    let big = panproto_reference::coerce::ScalarKind::BigInt;
    assert!(value_io::toml_scalar(&decimal, "1.50", "1.50"));
    assert!(!value_io::toml_scalar(&decimal, "1.50", "1.5"));
    assert!(value_io::toml_scalar(&decimal, "0e-1", "0.0"));
    assert!(!value_io::toml_scalar(&decimal, "10e-1", "1e0"));
    assert!(!value_io::toml_scalar(&decimal, "-0e-1", "0e0"));
    assert!(value_io::toml_scalar(&big, "1e20", "100000000000000000000"));
    assert!(toml::decimal_bigint_cst(
        "9223372036854775808",
        "9223372036854775808"
    ));
    assert!(!toml::decimal_bigint_cst(
        "9223372036854775808",
        "922337203685477580"
    ));
    assert!(!toml::decimal_bigint_cst("0e-1", "0.0"));
}

#[test]
fn i4_byte_paths_compare_both_directions_and_blob_layout_is_exact() {
    assert!(value_io::bytes(b"left", b"left"));
    assert!(!value_io::bytes(b"left", b"right"));
    assert!(!value_io::bytes(b"right", b"left"));
    assert_eq!(
        value_io::blob_writer("bafyreia", "image/png", 42),
        br#"{"$type":"blob","mimeType":"image/png","ref":{"$link":"bafyreia"},"size":42}"#
    );
    assert_eq!(
        String::from_utf8(value_io::blob_writer("\u{7f}", "\u{80}\u{9f}", 2))
            .test_unwrap("blob JSON is UTF-8"),
        "{\"$type\":\"blob\",\"mimeType\":\"\u{80}\u{9f}\",\"ref\":{\"$link\":\"\u{7f}\"},\"size\":2}"
    );
}

#[test]
fn i5_decimal_route_agreement_is_exact_in_both_directions() {
    assert!(value_io::decimal("1.50", "1.50"));
    assert!(!value_io::decimal("1.50", "1.5"));
    assert!(!value_io::decimal("1.5", "1.50"));
}

#[test]
fn i6_duplicate_member_paths_find_nonadjacent_duplicates_only() {
    let unique = vec![("a".to_owned(), 1), ("b".to_owned(), 2)];
    let duplicate = vec![
        ("a".to_owned(), 1),
        ("b".to_owned(), 2),
        ("a".to_owned(), 3),
    ];

    assert!(!value_io::duplicate_member(&unique));
    assert!(value_io::duplicate_member(&duplicate));
    assert!(!value_io::duplicate_member_cst(&unique));
    assert!(value_io::duplicate_member_cst(&duplicate));
}

#[test]
fn i7_canonical_encoding_normalizes_while_cst_encoding_is_exact() {
    let with_bom = b"\xef\xbb\xbf{}";
    let without_bom = b"{}";

    assert!(value_io::encoding(without_bom));
    assert!(!value_io::encoding(with_bom));
    assert!(!value_io::encoding(b"\xff"));
    assert!(value_io::xml_encoding(
        br#"<?xml version="1.0" encoding="UTF-8"?><root/>"#
    ));
    assert!(value_io::xml_encoding(b"<root/>"));
    assert!(!value_io::xml_encoding(
        br#"<?xml version="1.0" encoding="UTF-16"?><root/>"#
    ));
    assert!(!value_io::xml_encoding(
        b"\xef\xbb\xbf<?xml version=\"1.0\"?><root/>"
    ));
    assert!(value_io::encoding_bom_cst(with_bom, with_bom));
    assert!(!value_io::encoding_bom_cst(with_bom, without_bom));
}

#[test]
fn i12_default_routes_distinguish_null_from_absence() {
    use value_io::{Presence, PresenceOutcome, Route};

    for route in [Route::Json, Route::Yaml, Route::Xml] {
        assert_eq!(
            value_io::presence(route, Presence::Null),
            PresenceOutcome::NullSyntax
        );
    }
    for route in [Route::Json, Route::Toml, Route::Yaml, Route::Xml] {
        assert_eq!(
            value_io::presence(route, Presence::Absent),
            PresenceOutcome::Omitted
        );
    }
    for route in [Route::Toml, Route::Csv, Route::Tsv] {
        assert_eq!(
            value_io::presence(route, Presence::Null),
            PresenceOutcome::NullNotRepresentable
        );
    }
    for route in [Route::Csv, Route::Tsv] {
        assert_eq!(
            value_io::presence(route, Presence::Absent),
            PresenceOutcome::AbsentNotRepresentable
        );
    }
}

#[test]
fn i8_preservation_paths_accept_exact_bytes_and_reject_changed_bytes() {
    assert!(value_io::preserve(b"source", b"source"));
    assert!(!value_io::preserve(b"source", b"changed"));
    assert!(!value_io::preserve(b"changed", b"source"));
    assert!(value_io::preserve_cst(b"source", b"source"));
    assert!(!value_io::preserve_cst(b"source", b"changed"));
    assert!(!value_io::preserve_cst(b"changed", b"source"));
    assert!(xml::round_trip_cst(b"source", b"source"));
    assert!(!xml::round_trip_cst(b"source", b"changed"));
    assert!(!xml::round_trip_cst(b"changed", b"source"));
}

#[test]
fn i9_raw_numeric_preserves_the_written_lexeme_not_only_its_value() {
    assert!(value_io::raw_numeric("1.00e+2", "1.00e+2"));
    assert!(!value_io::raw_numeric("1.00e+2", "100"));
    assert!(!value_io::raw_numeric("100", "1.00e+2"));
}

#[test]
fn i10_xml_entity_paths_expand_declarations_and_enforce_the_byte_budget() {
    let declarations = BTreeMap::from([("x".to_owned(), "ok".to_owned())]);
    assert_eq!(
        value_io::xml_entity("&x;", &declarations),
        Ok("ok".to_owned())
    );
    assert_eq!(
        xml::expand_entities("&x;", &declarations),
        Ok("ok".to_owned())
    );
    assert_eq!(
        xml::bounded_expansion_cst("&x;", &declarations),
        Ok("ok".to_owned())
    );

    let over_budget = BTreeMap::from([("x".to_owned(), "x".repeat(1_048_577))]);
    assert_eq!(
        value_io::xml_entity("&x;", &over_budget),
        Err(xml::EntityError::Bytes)
    );
    assert_eq!(
        xml::expand_entities("&x;", &over_budget),
        Err(xml::EntityError::Bytes)
    );
    assert_eq!(
        xml::bounded_expansion_cst("&x;", &over_budget),
        Err(xml::EntityError::Bytes)
    );
}

#[test]
fn i11_xml_expanded_name_paths_preserve_namespace_then_local_order() {
    assert_eq!(
        value_io::xml_expanded_name("urn:a", "local"),
        ("urn:a".to_owned(), "local".to_owned())
    );
    assert_eq!(
        xml::expanded_name_cst("urn:a", "local"),
        ("urn:a".to_owned(), "local".to_owned())
    );
    assert_eq!(
        value_io::xml_expanded_name("", "local"),
        (String::new(), "local".to_owned())
    );
    assert_eq!(
        xml::expanded_name_cst("", "local"),
        (String::new(), "local".to_owned())
    );
}
