//! The serialized forms of the [`Value`](super::Value) payloads whose form
//! depends on the format: floats and byte strings.

/// A float: a number, or in a human-readable format the string `"NaN"`,
/// `"Infinity"` or `"-Infinity"` for a non-finite one.
pub mod float {
    use serde::de::{self, Visitor};
    use serde::{Deserializer, Serializer};

    /// The spelling of a non-finite float in a human-readable format.
    const fn spelling(f: f64) -> Option<&'static str> {
        if f.is_nan() {
            Some("NaN")
        } else if f.is_infinite() {
            if f.is_sign_positive() {
                Some("Infinity")
            } else {
                Some("-Infinity")
            }
        } else {
            None
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
            Some(text) if serializer.is_human_readable() => serializer.serialize_str(text),
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
            f.write_str("a number or one of \"NaN\", \"Infinity\", \"-Infinity\"")
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
            match v {
                "NaN" => Ok(panproto_expr::exact::CANONICAL_NAN),
                "Infinity" => Ok(f64::INFINITY),
                "-Infinity" => Ok(f64::NEG_INFINITY),
                _ => Err(E::invalid_value(de::Unexpected::Str(v), &self)),
            }
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
