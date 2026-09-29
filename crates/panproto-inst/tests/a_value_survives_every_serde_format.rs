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
    for f in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, 1.5, -0.0] {
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
        serde_json::to_string(&Value::Float(f64::NAN)).unwrap(),
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
