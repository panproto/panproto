//! Serde helpers for deterministic serialization.
//!
//! A `HashMap` enumerates its entries in an order the process's hash seed
//! decides, so writing one out as it comes makes the bytes of one and the
//! same value differ from process to process. The helpers here write maps
//! in key order instead. `panproto_schema::serde_helpers` re-exports
//! [`sorted_map`], so one definition serves both crates.

/// Serialize a `HashMap` as a map, written in key order.
///
/// Use with `#[serde(with = "sorted_map")]` on fields whose key type *can* be
/// a JSON object key. The entries are written in key order rather than in the
/// map's own enumeration order, so one value always produces one encoding.
pub mod sorted_map {
    use std::collections::HashMap;
    use std::hash::{BuildHasher, Hash};

    use serde::de::Deserializer;
    use serde::ser::{SerializeMap as _, Serializer};
    use serde::{Deserialize, Serialize};

    /// Serialize a `HashMap` as a map, in key order.
    ///
    /// # Errors
    ///
    /// Returns a serialization error if any key or value fails to serialize.
    pub fn serialize<S, K, V, H>(map: &HashMap<K, V, H>, serializer: S) -> Result<S::Ok, S::Error>
    where
        K: Serialize + Eq + Hash + Ord,
        V: Serialize,
        S: Serializer,
    {
        let mut pairs: Vec<(&K, &V)> = map.iter().collect();
        pairs.sort_unstable_by(|a, b| a.0.cmp(b.0));
        let mut entries = serializer.serialize_map(Some(pairs.len()))?;
        for (key, value) in pairs {
            entries.serialize_entry(key, value)?;
        }
        entries.end()
    }

    /// Deserialize a map into a `HashMap`.
    ///
    /// # Errors
    ///
    /// Returns a deserialization error if the input is not a valid map.
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
