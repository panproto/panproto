//! An instance [`Value`] survives every serde format the workspace writes
//! values in, exactly.
//!
//! Human-readable formats (JSON, and through it pythonize and
//! serde-wasm-bindgen) write a non-finite float as a tagged string, since
//! JSON has no literal for it; msgpack and CBOR keep the native float.
//! Bytes go out as a byte string (msgpack bin, CBOR byte string) and read
//! back from either a byte string or the integer sequence older writers
//! produced.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use panproto_inst::Value;
use panproto_inst::value::{CANONICAL_NAN, NEGATIVE_CANONICAL_NAN};

fn json_round_trip(value: &Value) -> Value {
    let text = serde_json::to_string(value).expect("serializes to JSON");
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{text} reads back: {e}"))
}

fn msgpack_round_trip(value: &Value) -> Value {
    let bytes = rmp_serde::to_vec(value).expect("serializes to msgpack");
    rmp_serde::from_slice(&bytes).expect("msgpack reads back")
}

fn named_msgpack_round_trip(value: &Value) -> Value {
    let bytes = rmp_serde::to_vec_named(value).expect("serializes to named msgpack");
    rmp_serde::from_slice(&bytes).expect("named msgpack reads back")
}

fn cbor_round_trip(value: &Value) -> Value {
    let mut bytes = Vec::new();
    ciborium::into_writer(value, &mut bytes).expect("serializes to CBOR");
    ciborium::from_reader(bytes.as_slice()).expect("CBOR reads back")
}

/// Equality that reads a NaN as equal to itself.
fn same(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Float(x), Value::Float(y)) => x.to_bits() == y.to_bits(),
        _ => a == b,
    }
}

#[test]
fn non_finite_floats_round_trip_in_every_serde_format() {
    for f in [CANONICAL_NAN, f64::INFINITY, f64::NEG_INFINITY, 1.5, -0.0] {
        let value = Value::Float(f);
        for (format, back) in [
            ("json", json_round_trip(&value)),
            ("msgpack", msgpack_round_trip(&value)),
            ("named msgpack", named_msgpack_round_trip(&value)),
            ("cbor", cbor_round_trip(&value)),
        ] {
            assert!(
                same(&value, &back),
                "{format}: {value:?} came back as {back:?}"
            );
        }
    }
    assert_eq!(
        serde_json::to_string(&Value::Float(CANONICAL_NAN)).unwrap(),
        r#"{"Float":"NaN"}"#
    );
    assert_eq!(
        serde_json::to_string(&Value::Float(f64::INFINITY)).unwrap(),
        r#"{"Float":"Infinity"}"#
    );
    assert_eq!(
        serde_json::to_string(&Value::Float(f64::NEG_INFINITY)).unwrap(),
        r#"{"Float":"-Infinity"}"#
    );
    assert_eq!(
        serde_json::to_string(&Value::Float(2.5)).unwrap(),
        r#"{"Float":2.5}"#
    );
    // msgpack and CBOR keep the native float: the encoded NaN is a float,
    // not a string.
    let packed = rmp_serde::to_vec(&Value::Float(f64::INFINITY)).unwrap();
    assert!(
        !packed.windows(8).any(|w| w == b"Infinity"),
        "msgpack writes a native float: {packed:?}"
    );
    let mut cbor = Vec::new();
    ciborium::into_writer(&Value::Float(f64::INFINITY), &mut cbor).unwrap();
    assert!(
        !cbor.windows(8).any(|w| w == b"Infinity"),
        "CBOR writes a native float: {cbor:?}"
    );
}

/// A NaN keeps its bits in every format: a human-readable one writes the
/// canonical NaN as `"NaN"`, its negative as `"-NaN"` and every other NaN
/// as `"NaN:0x"` with its 16 lowercase hexadecimal digits, and reads each
/// back to exactly those bits; the compact formats keep the native float.
/// A `"NaN:0x"` string that is not 16 lowercase hexadecimal digits, whose
/// bits are not a NaN, or whose bits have a spelling of their own is
/// refused, so every NaN has one spelling.
#[test]
fn a_nan_keeps_its_bits_in_every_serde_format() {
    let nans = [
        CANONICAL_NAN,
        NEGATIVE_CANONICAL_NAN,
        f64::from_bits(0x7ff8_0000_0000_1234),
        f64::from_bits(0xfff0_0000_0000_0001),
        f64::from_bits(0x7ff0_0000_2000_0000),
    ];
    for f in nans {
        let value = Value::Float(f);
        for (format, back) in [
            ("json", json_round_trip(&value)),
            ("msgpack", msgpack_round_trip(&value)),
            ("named msgpack", named_msgpack_round_trip(&value)),
            ("cbor", cbor_round_trip(&value)),
        ] {
            let Value::Float(g) = back else {
                panic!("{format}: {value:?} came back as {back:?}");
            };
            assert_eq!(
                g.to_bits(),
                f.to_bits(),
                "{format}: {:#018x} came back as {:#018x}",
                f.to_bits(),
                g.to_bits()
            );
        }
    }
    for (f, json) in [
        (CANONICAL_NAN, r#"{"Float":"NaN"}"#),
        (NEGATIVE_CANONICAL_NAN, r#"{"Float":"-NaN"}"#),
        (
            f64::from_bits(0x7ff8_0000_0000_1234),
            r#"{"Float":"NaN:0x7ff8000000001234"}"#,
        ),
        (
            f64::from_bits(0xfff0_0000_0000_0001),
            r#"{"Float":"NaN:0xfff0000000000001"}"#,
        ),
    ] {
        assert_eq!(serde_json::to_string(&Value::Float(f)).unwrap(), json);
    }
    for json in [
        r#"{"Float":"NaN:0x7ff8000000000000"}"#,
        r#"{"Float":"NaN:0xfff8000000000000"}"#,
        r#"{"Float":"NaN:0x3ff0000000000000"}"#,
        r#"{"Float":"NaN:0x7ff0000000000000"}"#,
        r#"{"Float":"NaN:0x7FF8000000001234"}"#,
        r#"{"Float":"NaN:0x7ff800000000123"}"#,
        r#"{"Float":"NaN:0x7ff80000000012345"}"#,
        r#"{"Float":"NaN:0x+ff8000000001234"}"#,
        r#"{"Float":"NaN:7ff8000000001234"}"#,
        r#"{"Float":"nan"}"#,
        r#"{"Float":"-nan"}"#,
    ] {
        let error = serde_json::from_str::<Value>(json).expect_err("refused");
        // The refusal names every spelling a float may take.
        assert!(
            error.to_string().contains(
                r#"expected a number or one of "NaN", "-NaN", "Infinity", "-Infinity" or "NaN:0x" with 16 lowercase hexadecimal digits"#
            ),
            "{json}: {error}"
        );
    }
}

#[test]
fn bytes_values_read_both_byte_strings_and_integer_sequences() {
    let value = Value::Bytes(vec![0x00, 0x01, 0x02, 0xff]);

    // msgpack writes a bin (0xc4, length 4), not an array of integers.
    let packed = rmp_serde::to_vec(&value).unwrap();
    assert!(
        packed
            .windows(6)
            .any(|w| w == [0xc4, 4, 0x00, 0x01, 0x02, 0xff]),
        "msgpack bin: {packed:02x?}"
    );
    // CBOR writes a byte string (major type 2, length 4).
    let mut cbor = Vec::new();
    ciborium::into_writer(&value, &mut cbor).unwrap();
    assert!(
        cbor.windows(5).any(|w| w == [0x44, 0x00, 0x01, 0x02, 0xff]),
        "CBOR byte string: {cbor:02x?}"
    );

    for (format, back) in [
        ("json", json_round_trip(&value)),
        ("msgpack", msgpack_round_trip(&value)),
        ("named msgpack", named_msgpack_round_trip(&value)),
        ("cbor", cbor_round_trip(&value)),
    ] {
        assert_eq!(back, value, "{format}");
    }

    // The integer sequence older writers produced still reads, in every
    // format: a map `{"Bytes": [0, 1, 2, 255]}` in each encoding.
    let legacy_json = r#"{"Bytes":[0,1,2,255]}"#;
    assert_eq!(serde_json::from_str::<Value>(legacy_json).unwrap(), value);
    let legacy: serde_json::Value = serde_json::from_str(legacy_json).unwrap();
    let legacy_msgpack = rmp_serde::to_vec(&legacy).unwrap();
    assert_eq!(
        rmp_serde::from_slice::<Value>(&legacy_msgpack).unwrap(),
        value
    );
    let mut legacy_cbor = Vec::new();
    ciborium::into_writer(&legacy, &mut legacy_cbor).unwrap();
    assert_eq!(
        ciborium::from_reader::<Value, _>(legacy_cbor.as_slice()).unwrap(),
        value
    );
}
