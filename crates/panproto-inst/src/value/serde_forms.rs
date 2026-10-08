//! The serialized forms of the [`Value`](super::Value) payloads whose form
//! depends on the format: floats and byte strings.

/// A float: a number, or in a human-readable format a string for a
/// non-finite one.
///
/// The strings are `"Infinity"` and `"-Infinity"` for the infinities,
/// `"NaN"` for [`CANONICAL_NAN`](panproto_expr::exact::CANONICAL_NAN),
/// `"-NaN"` for
/// [`NEGATIVE_CANONICAL_NAN`](panproto_expr::exact::NEGATIVE_CANONICAL_NAN),
/// and, for every other NaN, `"NaN:0x"` followed by the 16 lowercase
/// hexadecimal digits of its bits, so a NaN keeps its sign and payload. Each
/// NaN has exactly one spelling: a `"NaN:0x"` string that is not 16
/// lowercase hexadecimal digits, whose bits are not a NaN, or whose bits
/// are those of one of the two named NaNs is refused. Compact formats
/// write the native float.
pub mod float {
    use panproto_expr::exact::{CANONICAL_NAN, NEGATIVE_CANONICAL_NAN};
    use serde::de::{self, Visitor};
    use serde::{Deserializer, Serializer};

    /// The prefix of the spelling of a NaN other than the two named ones.
    const PAYLOAD_PREFIX: &str = "NaN:0x";

    /// The spelling of a non-finite float in a human-readable format, or
    /// `None` for a finite one.
    fn spelling(f: f64) -> Option<String> {
        if f.is_nan() {
            let bits = f.to_bits();
            Some(if bits == CANONICAL_NAN.to_bits() {
                "NaN".to_owned()
            } else if bits == NEGATIVE_CANONICAL_NAN.to_bits() {
                "-NaN".to_owned()
            } else {
                format!("{PAYLOAD_PREFIX}{bits:016x}")
            })
        } else if f.is_infinite() {
            Some(if f.is_sign_positive() {
                "Infinity".to_owned()
            } else {
                "-Infinity".to_owned()
            })
        } else {
            None
        }
    }

    /// The non-finite float `text` spells, or `None` when it spells none:
    /// the inverse of [`spelling`] on non-finite floats, bit for bit.
    fn from_spelling(text: &str) -> Option<f64> {
        match text {
            "NaN" => Some(CANONICAL_NAN),
            "-NaN" => Some(NEGATIVE_CANONICAL_NAN),
            "Infinity" => Some(f64::INFINITY),
            "-Infinity" => Some(f64::NEG_INFINITY),
            _ => {
                let hex = text.strip_prefix(PAYLOAD_PREFIX)?;
                let lowercase_hex = |b: u8| b.is_ascii_digit() || (b'a'..=b'f').contains(&b);
                if hex.len() != 16 || !hex.bytes().all(lowercase_hex) {
                    return None;
                }
                let bits = u64::from_str_radix(hex, 16).ok()?;
                let f = f64::from_bits(bits);
                let named =
                    bits == CANONICAL_NAN.to_bits() || bits == NEGATIVE_CANONICAL_NAN.to_bits();
                (f.is_nan() && !named).then_some(f)
            }
        }
    }

    /// Write `f`: its spelling when it is non-finite and the format is
    /// human-readable, else the native float.
    ///
    /// # Errors
    ///
    /// Whatever the serializer returns.
    pub fn serialize<S: Serializer, F: Copy + Into<f64>>(
        f: &F,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        let f: f64 = (*f).into();
        match spelling(f) {
            Some(text) if serializer.is_human_readable() => serializer.serialize_str(&text),
            _ => serializer.serialize_f64(f),
        }
    }

    /// Read a float from a number, or from a non-finite spelling.
    ///
    /// # Errors
    ///
    /// When the input is neither a number nor a non-finite spelling.
    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<f64, D::Error> {
        if deserializer.is_human_readable() {
            deserializer.deserialize_any(FloatVisitor)
        } else {
            deserializer.deserialize_f64(FloatVisitor)
        }
    }

    struct FloatVisitor;

    impl Visitor<'_> for FloatVisitor {
        type Value = f64;

        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str(
                "a number or one of \"NaN\", \"-NaN\", \"Infinity\", \"-Infinity\" or \"NaN:0x\" with 16 lowercase hexadecimal digits",
            )
        }

        fn visit_f64<E: de::Error>(self, v: f64) -> Result<f64, E> {
            Ok(v)
        }

        fn visit_f32<E: de::Error>(self, v: f32) -> Result<f64, E> {
            Ok(f64::from(v))
        }

        // An integer reads as the nearest float, rounded once, through its
        // decimal text (which Rust parses correctly rounded).
        fn visit_i64<E: de::Error>(self, v: i64) -> Result<f64, E> {
            v.to_string().parse().map_err(E::custom)
        }

        fn visit_u64<E: de::Error>(self, v: u64) -> Result<f64, E> {
            v.to_string().parse().map_err(E::custom)
        }

        fn visit_str<E: de::Error>(self, v: &str) -> Result<f64, E> {
            from_spelling(v).ok_or_else(|| E::invalid_value(de::Unexpected::Str(v), &self))
        }
    }
}

/// A byte string: written as one (a msgpack bin, a CBOR byte string, an
/// integer array in JSON), read from a byte string or an integer sequence.
pub mod bytes {
    use serde::de::{self, SeqAccess, Visitor};
    use serde::{Deserializer, Serializer};

    /// Write `bytes` as a byte string.
    ///
    /// # Errors
    ///
    /// Whatever the serializer returns.
    pub fn serialize<S: Serializer>(bytes: &[u8], serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_bytes(bytes)
    }

    /// Read a byte string, or a sequence of integers each in `0..=255`.
    ///
    /// # Errors
    ///
    /// When the input is neither, or an integer is out of range.
    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<u8>, D::Error> {
        deserializer.deserialize_any(BytesVisitor)
    }

    struct BytesVisitor;

    impl<'de> Visitor<'de> for BytesVisitor {
        type Value = Vec<u8>;

        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("a byte string or a sequence of bytes")
        }

        fn visit_bytes<E: de::Error>(self, v: &[u8]) -> Result<Vec<u8>, E> {
            Ok(v.to_vec())
        }

        fn visit_byte_buf<E: de::Error>(self, v: Vec<u8>) -> Result<Vec<u8>, E> {
            Ok(v)
        }

        fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Vec<u8>, A::Error> {
            let mut out = Vec::with_capacity(seq.size_hint().unwrap_or(0).min(1 << 16));
            while let Some(byte) = seq.next_element::<u8>()? {
                out.push(byte);
            }
            Ok(out)
        }
    }
}
