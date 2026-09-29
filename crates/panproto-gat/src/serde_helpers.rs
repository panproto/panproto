//! Serde helpers for deterministic serialization.
//!
//! A `HashMap` enumerates its entries in an order the process's hash seed
//! decides, so writing one out as it comes makes the bytes of one and the
//! same value differ from process to process. The helpers here write maps
//! in key order instead. `panproto_schema::serde_helpers` re-exports
//! [`sorted_map`], so one definition serves both crates.
//!
//! [`btree_map_as_vec`] writes a `BTreeMap` whose key is a struct (which
//! JSON cannot use as an object key) as a sequence of pairs in key order,
//! for the composition provenance maps keyed by node-qualified symbol.

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

/// Serialize a `BTreeMap<K, V>` as a sequence of `(K, V)` pairs, in key
/// order, rejecting a duplicate key on decode.
///
/// Use with `#[serde(with = "btree_map_as_vec")]` on maps whose key type
/// cannot be a JSON object key.
pub mod btree_map_as_vec {
    use std::collections::BTreeMap;
    use std::collections::btree_map::Entry;
    use std::fmt;
    use std::marker::PhantomData;

    use serde::de::{Deserializer, Error as _, SeqAccess, Visitor};
    use serde::ser::{SerializeSeq as _, Serializer};
    use serde::{Deserialize, Serialize};

    /// Serialize a `BTreeMap` as a sequence of key-value pairs, in key order.
    ///
    /// # Errors
    ///
    /// Returns a serialization error if any key or value fails to serialize.
    pub fn serialize<S, K, V>(map: &BTreeMap<K, V>, serializer: S) -> Result<S::Ok, S::Error>
    where
        K: Serialize,
        V: Serialize,
        S: Serializer,
    {
        let mut seq = serializer.serialize_seq(Some(map.len()))?;
        for pair in map {
            seq.serialize_element(&pair)?;
        }
        seq.end()
    }

    /// Deserialize a sequence of key-value pairs into a `BTreeMap`.
    ///
    /// # Errors
    ///
    /// Returns a deserialization error if the input is not a sequence of
    /// `(K, V)` pairs, or if a key occurs more than once.
    pub fn deserialize<'de, D, K, V>(deserializer: D) -> Result<BTreeMap<K, V>, D::Error>
    where
        K: Deserialize<'de> + Ord + fmt::Debug,
        V: Deserialize<'de>,
        D: Deserializer<'de>,
    {
        deserializer.deserialize_seq(PairsVisitor(PhantomData))
    }

    /// Reads the pairs one at a time so a duplicate key is caught as it
    /// arrives.
    struct PairsVisitor<K, V>(PhantomData<fn() -> (K, V)>);

    impl<'de, K, V> Visitor<'de> for PairsVisitor<K, V>
    where
        K: Deserialize<'de> + Ord + fmt::Debug,
        V: Deserialize<'de>,
    {
        type Value = BTreeMap<K, V>;

        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("a sequence of [key, value] pairs with distinct keys")
        }

        fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
        where
            A: SeqAccess<'de>,
        {
            let mut map = BTreeMap::new();
            while let Some((key, value)) = seq.next_element::<(K, V)>()? {
                match map.entry(key) {
                    Entry::Vacant(slot) => {
                        slot.insert(value);
                    }
                    Entry::Occupied(slot) => {
                        return Err(A::Error::custom(format_args!(
                            "duplicate map key {:?}",
                            slot.key()
                        )));
                    }
                }
            }
            Ok(map)
        }
    }
}
