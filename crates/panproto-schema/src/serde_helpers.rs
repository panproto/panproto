//! Custom serde helpers for JSON-compatible serialization of complex map keys.
//!
//! `serde_json` cannot serialize `HashMap<K, V>` when `K` is a struct or
//! tuple; it requires string keys for JSON objects. These modules serialize
//! such maps as `Vec<(K, V)>` arrays instead, which round-trip through both
//! JSON and `MessagePack`.
//!
//! The array is written in key order rather than in the map's own iteration
//! order. A `HashMap` enumerates its entries in an order the process's hash
//! seed decides, so writing them out as they come would make the bytes of a
//! serialized schema, morphism, or migration differ from run to run for one
//! and the same value — every write a spurious diff, and every digest taken
//! over those bytes a different digest.

/// Serialize/deserialize `HashMap<K, V>` as `Vec<(K, V)>`.
///
/// Use with `#[serde(with = "map_as_vec")]` on fields where the key type
/// is a struct (like [`crate::Edge`]) or tuple that cannot be a JSON object key.
pub mod map_as_vec {
    use std::collections::HashMap;
    use std::hash::{BuildHasher, Hash};

    use serde::de::Deserializer;
    use serde::ser::Serializer;
    use serde::{Deserialize, Serialize};

    /// Serialize a `HashMap` as a `Vec` of key-value pairs, in key order.
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
        pairs.serialize(serializer)
    }

    /// Deserialize a `Vec` of key-value pairs into a `HashMap`.
    ///
    /// # Errors
    ///
    /// Returns a deserialization error if the input is not a valid array
    /// of `(K, V)` pairs.
    pub fn deserialize<'de, D, K, V, H>(deserializer: D) -> Result<HashMap<K, V, H>, D::Error>
    where
        K: Deserialize<'de> + Eq + Hash,
        V: Deserialize<'de>,
        D: Deserializer<'de>,
        H: BuildHasher + Default,
    {
        let pairs: Vec<(K, V)> = Vec::deserialize(deserializer)?;
        Ok(pairs.into_iter().collect())
    }
}

/// Like [`map_as_vec`] but compatible with `#[serde(default)]`.
///
/// Use with `#[serde(default, with = "map_as_vec_default")]` on optional
/// fields that should default to an empty `HashMap` when absent.
pub mod map_as_vec_default {
    use std::collections::HashMap;
    use std::hash::{BuildHasher, Hash};

    use serde::de::Deserializer;
    use serde::ser::Serializer;
    use serde::{Deserialize, Serialize};

    /// Serialize a `HashMap` as a `Vec` of key-value pairs, in key order.
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
        pairs.serialize(serializer)
    }

    /// Deserialize a `Vec` of key-value pairs into a `HashMap`.
    ///
    /// # Errors
    ///
    /// Returns a deserialization error if the input is not a valid array
    /// of `(K, V)` pairs.
    pub fn deserialize<'de, D, K, V, H>(deserializer: D) -> Result<HashMap<K, V, H>, D::Error>
    where
        K: Deserialize<'de> + Eq + Hash,
        V: Deserialize<'de>,
        D: Deserializer<'de>,
        H: BuildHasher + Default,
    {
        let pairs: Vec<(K, V)> = Vec::deserialize(deserializer)?;
        Ok(pairs.into_iter().collect())
    }
}

/// Serialize a `HashMap` as a map, written in key order.
///
/// Defined in `panproto-gat`, which this crate depends on, so that theory
/// morphisms and schemas share one definition.
pub use panproto_gat::serde_helpers::sorted_map;

/// Serialize a `HashMap` whose values are themselves maps, in key order at
/// both levels.
///
/// Use with `#[serde(with = "sorted_nested_map")]`. Sorting only the outer
/// map would leave the inner ones to their own enumeration order, which is
/// enough on its own to make one value produce many encodings.
pub mod sorted_nested_map {
    use std::collections::HashMap;
    use std::hash::{BuildHasher, Hash};

    use serde::de::Deserializer;
    use serde::ser::{SerializeMap as _, Serializer};
    use serde::{Deserialize, Serialize};

    /// A map of maps: the shape this module reads and writes.
    type Nested<K, IK, IV, H, IH> = HashMap<K, HashMap<IK, IV, IH>, H>;

    /// One inner map, written in key order.
    struct Inner<'a, K, V, H>(&'a HashMap<K, V, H>);

    impl<K, V, H> Serialize for Inner<'_, K, V, H>
    where
        K: Serialize + Eq + Hash + Ord,
        V: Serialize,
    {
        fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            super::sorted_map::serialize(self.0, serializer)
        }
    }

    /// Serialize a map of maps, in key order at both levels.
    ///
    /// # Errors
    ///
    /// Returns a serialization error if any key or value fails to serialize.
    pub fn serialize<S, K, IK, IV, H, IH>(
        map: &Nested<K, IK, IV, H, IH>,
        serializer: S,
    ) -> Result<S::Ok, S::Error>
    where
        K: Serialize + Eq + Hash + Ord,
        IK: Serialize + Eq + Hash + Ord,
        IV: Serialize,
        S: Serializer,
    {
        let mut pairs: Vec<(&K, &HashMap<IK, IV, IH>)> = map.iter().collect();
        pairs.sort_unstable_by(|a, b| a.0.cmp(b.0));
        let mut entries = serializer.serialize_map(Some(pairs.len()))?;
        for (key, inner) in pairs {
            entries.serialize_entry(key, &Inner(inner))?;
        }
        entries.end()
    }

    /// Deserialize a map of maps.
    ///
    /// # Errors
    ///
    /// Returns a deserialization error if the input is not a valid map.
    pub fn deserialize<'de, D, K, IK, IV, H, IH>(
        deserializer: D,
    ) -> Result<Nested<K, IK, IV, H, IH>, D::Error>
    where
        K: Deserialize<'de> + Eq + Hash,
        IK: Deserialize<'de> + Eq + Hash,
        IV: Deserialize<'de>,
        D: Deserializer<'de>,
        H: BuildHasher + Default,
        IH: BuildHasher + Default,
    {
        HashMap::deserialize(deserializer)
    }
}

/// Serialize a `HashSet` as a sequence, written in member order.
///
/// Use with `#[serde(with = "sorted_set")]`. A `HashSet` enumerates its
/// members in an order the process's hash seed decides, so writing them out
/// as they come would give one set many encodings.
pub mod sorted_set {
    use std::collections::HashSet;
    use std::hash::{BuildHasher, Hash};

    use serde::de::Deserializer;
    use serde::ser::{SerializeSeq as _, Serializer};
    use serde::{Deserialize, Serialize};

    /// Serialize a `HashSet` as a sequence, in member order.
    ///
    /// # Errors
    ///
    /// Returns a serialization error if any member fails to serialize.
    pub fn serialize<S, T, H>(set: &HashSet<T, H>, serializer: S) -> Result<S::Ok, S::Error>
    where
        T: Serialize + Eq + Hash + Ord,
        S: Serializer,
    {
        let mut members: Vec<&T> = set.iter().collect();
        members.sort_unstable();
        let mut seq = serializer.serialize_seq(Some(members.len()))?;
        for member in members {
            seq.serialize_element(member)?;
        }
        seq.end()
    }

    /// Deserialize a sequence into a `HashSet`.
    ///
    /// # Errors
    ///
    /// Returns a deserialization error if the input is not a valid sequence.
    pub fn deserialize<'de, D, T, H>(deserializer: D) -> Result<HashSet<T, H>, D::Error>
    where
        T: Deserialize<'de> + Eq + Hash,
        D: Deserializer<'de>,
        H: BuildHasher + Default,
    {
        HashSet::deserialize(deserializer)
    }
}

/// Serialize a `BTreeMap<K, V>` as `Vec<(K, V)>`, in key order.
///
/// Use with `#[serde(with = "panproto_schema::serde_helpers::btree_map_as_vec")]`
/// on every serialized `BTreeMap` whose key is neither a string nor an
/// integer: a struct, a data-carrying enum or a tuple. `serde_json` and
/// `pythonize` reject such a key as a map key, while a sequence of pairs is
/// accepted by every self-describing format.
///
/// A `BTreeMap` already enumerates in key order, so the pairs are written as
/// they come. Decoding rejects a key that occurs twice, since a sequence of
/// pairs, unlike a map, can carry one, and keeping either occurrence silently
/// would make two different encodings decode to one value.
///
/// ```
/// use std::collections::BTreeMap;
///
/// use serde::{Deserialize, Serialize};
///
/// #[derive(Debug, PartialEq, Serialize, Deserialize)]
/// struct Wrapper {
///     #[serde(with = "panproto_schema::serde_helpers::btree_map_as_vec")]
///     map: BTreeMap<(u32, u32), String>,
/// }
///
/// let w = Wrapper { map: BTreeMap::from([((2, 0), "b".to_owned()), ((1, 5), "a".to_owned())]) };
/// let json = serde_json::to_string(&w).unwrap();
/// assert_eq!(json, r#"{"map":[[[1,5],"a"],[[2,0],"b"]]}"#);
/// assert_eq!(serde_json::from_str::<Wrapper>(&json).unwrap(), w);
///
/// let dup = r#"{"map":[[[1,5],"a"],[[1,5],"b"]]}"#;
/// assert!(serde_json::from_str::<Wrapper>(dup).is_err());
/// ```
///
/// Defined in `panproto-gat`, which this crate depends on, so that the
/// composition provenance and schemas share one definition.
pub use panproto_gat::serde_helpers::btree_map_as_vec;

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::collections::HashMap;

    use crate::Edge;

    #[test]
    fn schema_with_edges_json_roundtrip() {
        let mut schema = crate::Schema {
            protocol: "test".into(),
            vertices: HashMap::from([
                (
                    "root".into(),
                    crate::Vertex {
                        id: "root".into(),
                        kind: "object".into(),
                        nsid: None,
                    },
                ),
                (
                    "root.name".into(),
                    crate::Vertex {
                        id: "root.name".into(),
                        kind: "string".into(),
                        nsid: None,
                    },
                ),
            ]),
            edges: HashMap::new(),
            hyper_edges: HashMap::new(),
            constraints: HashMap::new(),
            required: HashMap::new(),
            nsids: HashMap::new(),
            entries: Vec::new(),
            variants: HashMap::new(),
            orderings: HashMap::new(),
            recursion_points: HashMap::new(),
            spans: HashMap::new(),
            usage_modes: HashMap::new(),
            nominal: HashMap::new(),
            coercions: HashMap::new(),
            mergers: HashMap::new(),
            defaults: HashMap::new(),
            policies: HashMap::new(),
            outgoing: HashMap::new(),
            incoming: HashMap::new(),
            between: HashMap::new(),
            ..Default::default()
        };

        let edge = Edge {
            src: "root".into(),
            tgt: "root.name".into(),
            kind: "prop".into(),
            name: Some("name".into()),
        };
        schema.edges.insert(edge, "prop".into());

        let json = serde_json::to_string_pretty(&schema).unwrap();
        let recovered: crate::Schema = serde_json::from_str(&json).unwrap();

        assert_eq!(schema.edges.len(), recovered.edges.len());
        assert_eq!(schema.vertices.len(), recovered.vertices.len());
    }

    #[test]
    fn schema_with_edges_msgpack_roundtrip() {
        let mut schema = crate::Schema {
            protocol: "test".into(),
            vertices: HashMap::new(),
            edges: HashMap::new(),
            hyper_edges: HashMap::new(),
            constraints: HashMap::new(),
            required: HashMap::new(),
            nsids: HashMap::new(),
            entries: Vec::new(),
            variants: HashMap::new(),
            orderings: HashMap::new(),
            recursion_points: HashMap::new(),
            spans: HashMap::new(),
            usage_modes: HashMap::new(),
            nominal: HashMap::new(),
            coercions: HashMap::new(),
            mergers: HashMap::new(),
            defaults: HashMap::new(),
            policies: HashMap::new(),
            outgoing: HashMap::new(),
            incoming: HashMap::new(),
            between: HashMap::new(),
            ..Default::default()
        };

        let edge = Edge {
            src: "a".into(),
            tgt: "b".into(),
            kind: "prop".into(),
            name: None,
        };
        schema.edges.insert(edge, "prop".into());

        let bytes = rmp_serde::to_vec(&schema).unwrap();
        let recovered: crate::Schema = rmp_serde::from_slice(&bytes).unwrap();

        assert_eq!(schema.edges.len(), recovered.edges.len());
    }

    /// A key type that no map-keyed format accepts: a data-carrying enum.
    #[derive(
        Clone, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
    )]
    #[serde(rename_all = "snake_case")]
    enum Key {
        Node(u32),
        Pair { edge: Edge, src: u32 },
    }

    #[derive(Debug, PartialEq, serde::Serialize, serde::Deserialize)]
    struct Keyed {
        #[serde(with = "super::btree_map_as_vec")]
        map: std::collections::BTreeMap<Key, String>,
    }

    /// The pairs of a [`Keyed`] written without the duplicate check.
    #[derive(serde::Serialize)]
    struct Raw {
        map: Vec<(Key, String)>,
    }

    fn keyed() -> Keyed {
        let edge = Edge {
            src: "a".into(),
            tgt: "b".into(),
            kind: "prop".into(),
            name: Some("x".into()),
        };
        Keyed {
            map: std::collections::BTreeMap::from([
                (Key::Pair { edge, src: 1 }, "pair".to_owned()),
                (Key::Node(7), "seven".to_owned()),
                (Key::Node(3), "three".to_owned()),
            ]),
        }
    }

    #[test]
    fn btree_map_as_vec_round_trips_json_and_msgpack() {
        let value = keyed();
        let json = serde_json::to_string(&value).unwrap();
        assert!(json.starts_with(r#"{"map":[[{"node":3},"three"],[{"node":7},"seven"],"#));
        assert_eq!(serde_json::from_str::<Keyed>(&json).unwrap(), value);

        let bytes = rmp_serde::to_vec_named(&value).unwrap();
        assert_eq!(rmp_serde::from_slice::<Keyed>(&bytes).unwrap(), value);

        let empty = Keyed {
            map: std::collections::BTreeMap::new(),
        };
        let json = serde_json::to_string(&empty).unwrap();
        assert_eq!(json, r#"{"map":[]}"#);
        assert_eq!(serde_json::from_str::<Keyed>(&json).unwrap(), empty);
    }

    #[test]
    fn btree_map_as_vec_writes_pairs_in_key_order() {
        let value = keyed();
        let json: serde_json::Value = serde_json::to_value(&value).unwrap();
        let pairs = json["map"].as_array().unwrap();
        let keys: Vec<Key> = pairs
            .iter()
            .map(|p| serde_json::from_value(p[0].clone()).unwrap())
            .collect();
        let expected: Vec<Key> = value.map.keys().cloned().collect();
        assert_eq!(keys, expected);
        assert!(keys.windows(2).all(|w| w[0] < w[1]));
    }

    #[test]
    fn btree_map_as_vec_rejects_duplicate_keys() {
        let json = r#"{"map":[[{"node":3},"a"],[{"node":3},"b"]]}"#;
        let err = serde_json::from_str::<Keyed>(json).unwrap_err();
        assert!(err.to_string().contains("duplicate map key"), "{err}");

        let pairs: Vec<(Key, String)> = vec![
            (Key::Node(3), "a".to_owned()),
            (Key::Node(3), "b".to_owned()),
        ];
        let bytes = rmp_serde::to_vec_named(&Raw { map: pairs }).unwrap();
        let err = rmp_serde::from_slice::<Keyed>(&bytes).unwrap_err();
        assert!(err.to_string().contains("duplicate map key"), "{err}");
    }
}
