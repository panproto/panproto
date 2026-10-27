//! Stable serialized payload forms used by [`Value`](super::Value).

pub mod float {
    use crate::exact::{CANONICAL_NAN, NEGATIVE_CANONICAL_NAN};
    use serde::de::{self, Visitor};
    use serde::{Deserializer, Serializer};

    const PAYLOAD_PREFIX: &str = "NaN:0x";

    fn spelling(value: f64) -> Option<String> {
        if value.is_nan() {
            let bits = value.to_bits();
            Some(if bits == CANONICAL_NAN.to_bits() {
                "NaN".to_owned()
            } else if bits == NEGATIVE_CANONICAL_NAN.to_bits() {
                "-NaN".to_owned()
            } else {
                format!("{PAYLOAD_PREFIX}{bits:016x}")
            })
        } else if value.is_infinite() {
            Some(if value.is_sign_positive() {
                "Infinity".to_owned()
            } else {
                "-Infinity".to_owned()
            })
        } else {
            None
        }
    }

    fn from_spelling(text: &str) -> Option<f64> {
        match text {
            "NaN" => Some(CANONICAL_NAN),
            "-NaN" => Some(NEGATIVE_CANONICAL_NAN),
            "Infinity" => Some(f64::INFINITY),
            "-Infinity" => Some(f64::NEG_INFINITY),
            _ => {
                let hex = text.strip_prefix(PAYLOAD_PREFIX)?;
                let lowercase_hex =
                    |byte: u8| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte);
                if hex.len() != 16 || !hex.bytes().all(lowercase_hex) {
                    return None;
                }
                let bits = u64::from_str_radix(hex, 16).ok()?;
                let value = f64::from_bits(bits);
                let named =
                    bits == CANONICAL_NAN.to_bits() || bits == NEGATIVE_CANONICAL_NAN.to_bits();
                (value.is_nan() && !named).then_some(value)
            }
        }
    }

    pub fn serialize<S: Serializer, F: Copy + Into<f64>>(
        value: &F,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        let value: f64 = (*value).into();
        match spelling(value) {
            Some(text) if serializer.is_human_readable() => serializer.serialize_str(&text),
            _ => serializer.serialize_f64(value),
        }
    }

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

        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str(
                "a number, a named infinity or NaN, or NaN:0x followed by 16 lowercase hexadecimal digits",
            )
        }

        fn visit_f64<E: de::Error>(self, value: f64) -> Result<f64, E> {
            Ok(value)
        }

        fn visit_f32<E: de::Error>(self, value: f32) -> Result<f64, E> {
            Ok(f64::from(value))
        }

        fn visit_i64<E: de::Error>(self, value: i64) -> Result<f64, E> {
            value.to_string().parse().map_err(E::custom)
        }

        fn visit_u64<E: de::Error>(self, value: u64) -> Result<f64, E> {
            value.to_string().parse().map_err(E::custom)
        }

        fn visit_str<E: de::Error>(self, value: &str) -> Result<f64, E> {
            from_spelling(value).ok_or_else(|| E::invalid_value(de::Unexpected::Str(value), &self))
        }
    }
}

pub mod bytes {
    use serde::de::{self, SeqAccess, Visitor};
    use serde::{Deserializer, Serializer};

    pub fn serialize<S: Serializer>(bytes: &[u8], serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_bytes(bytes)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<u8>, D::Error> {
        deserializer.deserialize_any(BytesVisitor)
    }

    struct BytesVisitor;

    impl<'de> Visitor<'de> for BytesVisitor {
        type Value = Vec<u8>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("a byte string or a sequence of bytes")
        }

        fn visit_bytes<E: de::Error>(self, value: &[u8]) -> Result<Vec<u8>, E> {
            Ok(value.to_vec())
        }

        fn visit_byte_buf<E: de::Error>(self, value: Vec<u8>) -> Result<Vec<u8>, E> {
            Ok(value)
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

pub mod sorted_map {
    use std::collections::HashMap;
    use std::hash::{BuildHasher, Hash};

    use serde::de::Deserializer;
    use serde::ser::{SerializeMap as _, Serializer};
    use serde::{Deserialize, Serialize};

    pub fn serialize<S, K, V, H>(map: &HashMap<K, V, H>, serializer: S) -> Result<S::Ok, S::Error>
    where
        K: Serialize + Eq + Hash + Ord,
        V: Serialize,
        S: Serializer,
    {
        let mut pairs: Vec<_> = map.iter().collect();
        pairs.sort_unstable_by(|left, right| left.0.cmp(right.0));
        let mut entries = serializer.serialize_map(Some(pairs.len()))?;
        for (key, value) in pairs {
            entries.serialize_entry(key, value)?;
        }
        entries.end()
    }

    pub fn deserialize<'de, D, K, V, H>(deserializer: D) -> Result<HashMap<K, V, H>, D::Error>
    where
        K: Deserialize<'de> + Eq + Hash,
        V: Deserialize<'de>,
        D: Deserializer<'de>,
        H: BuildHasher + Default,
    {
        HashMap::deserialize(deserializer)
    }
}
