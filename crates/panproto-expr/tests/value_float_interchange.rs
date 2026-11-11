//! Float interchange keeps numeric inputs and every non-finite bit pattern.

use panproto_expr::{CANONICAL_NAN, NEGATIVE_CANONICAL_NAN, Value};

fn float_bits(value: Value) -> u64 {
    match value {
        Value::Float(number) => number.to_bits(),
        other => panic!("expected a float, got {other:?}"),
    }
}

#[test]
fn value_json_float_preserves_nonfinite_sign_and_payload() -> Result<(), Box<dyn std::error::Error>>
{
    for (bits, spelling) in [
        (CANONICAL_NAN.to_bits(), "NaN"),
        (NEGATIVE_CANONICAL_NAN.to_bits(), "-NaN"),
        (0x7ff0_0000_0000_0001, "NaN:0x7ff0000000000001"),
        (0xfff8_0000_0000_0042, "NaN:0xfff8000000000042"),
        (f64::INFINITY.to_bits(), "Infinity"),
        (f64::NEG_INFINITY.to_bits(), "-Infinity"),
    ] {
        let text = serde_json::to_string(&Value::Float(f64::from_bits(bits)))?;
        let expected = format!("{{\"Float\":\"{spelling}\"}}");
        assert_eq!(text, expected, "the human-readable spelling is canonical");
        assert_eq!(float_bits(serde_json::from_str(&text)?), bits);
    }
    Ok(())
}

#[test]
fn value_json_float_reads_signed_integer_tokens_numerically()
-> Result<(), Box<dyn std::error::Error>> {
    for (text, expected) in [
        (r#"{"Float":-7}"#, -7.0_f64),
        (
            r#"{"Float":-9223372036854775808}"#,
            -9_223_372_036_854_775_808.0_f64,
        ),
    ] {
        assert_eq!(
            float_bits(serde_json::from_str(text)?),
            expected.to_bits(),
            "integer tokens retain their numeric value in a Float payload"
        );
    }
    Ok(())
}

#[test]
fn value_messagepack_float_uses_native_bits_for_nonfinite_values()
-> Result<(), Box<dyn std::error::Error>> {
    for bits in [
        CANONICAL_NAN.to_bits(),
        NEGATIVE_CANONICAL_NAN.to_bits(),
        0x7ff0_0000_0000_0001,
        0xfff8_0000_0000_0042,
        f64::INFINITY.to_bits(),
        f64::NEG_INFINITY.to_bits(),
        (-0.0_f64).to_bits(),
    ] {
        // A one-entry map, the enum tag Float, then the native f64 marker.
        let mut expected = vec![0x81, 0xa5, b'F', b'l', b'o', b'a', b't', 0xcb];
        expected.extend_from_slice(&bits.to_be_bytes());
        let actual = rmp_serde::to_vec_named(&Value::Float(f64::from_bits(bits)))?;
        assert_eq!(
            actual, expected,
            "compact floats are numbers, including NaNs"
        );
        assert_eq!(float_bits(rmp_serde::from_slice(&actual)?), bits);
    }
    Ok(())
}

#[test]
fn value_messagepack_float_reads_binary32_tokens_by_exact_widening()
-> Result<(), Box<dyn std::error::Error>> {
    for value in [-3.5_f32, 0.25_f32, -0.0_f32, f32::MIN_POSITIVE] {
        let mut wire = vec![0x81, 0xa5, b'F', b'l', b'o', b'a', b't', 0xca];
        wire.extend_from_slice(&value.to_bits().to_be_bytes());
        assert_eq!(
            float_bits(rmp_serde::from_slice(&wire)?),
            f64::from(value).to_bits(),
            "a native f32 payload widens without rounding"
        );
    }
    Ok(())
}
