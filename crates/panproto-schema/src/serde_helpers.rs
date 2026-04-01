//! Custom serde helpers for JSON-compatible serialization of complex map keys.
//!
//! `serde_json` cannot serialize `HashMap<K, V>` when `K` is a struct or
//! tuple; it requires string keys for JSON objects. These modules serialize
//! such maps as `Vec<(K, V)>` arrays instead, which round-trip through both
//! JSON and `MessagePack`.

/// Serialize/deserialize `HashMap<K, V>` as `Vec<(K, V)>`.
///
/// Use with `#[serde(with = "map_as_vec")]` on fields where the key type
/// is a struct (like [`crate::Edge`]) or tuple that cannot be a JSON object key.
pub mod map_as_vec {
    use std::collections::HashMap;
    use std::hash::Hash;

    use serde::de::Deserializer;
    use serde::ser::Serializer;
    use serde::{Deserialize, Serialize};

    /// Serialize a `HashMap` as a `Vec` of key-value pairs.
    ///
    /// # Errors
    ///
    /// Returns a serialization error if any key or value fails to serialize.
    #[allow(clippy::implicit_hasher)]
    pub fn serialize<S, K, V>(map: &HashMap<K, V>, serializer: S) -> Result<S::Ok, S::Error>
    where
        K: Serialize + Eq + Hash,
        V: Serialize,
        S: Serializer,
    {
        let pairs: Vec<(&K, &V)> = map.iter().collect();
        pairs.serialize(serializer)
    }

    /// Deserialize a `Vec` of key-value pairs into a `HashMap`.
    ///
    /// # Errors
    ///
    /// Returns a deserialization error if the input is not a valid array
    /// of `(K, V)` pairs.
    pub fn deserialize<'de, D, K, V>(deserializer: D) -> Result<HashMap<K, V>, D::Error>
    where
        K: Deserialize<'de> + Eq + Hash,
        V: Deserialize<'de>,
        D: Deserializer<'de>,
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
    use std::hash::Hash;

    use serde::de::Deserializer;
    use serde::ser::Serializer;
    use serde::{Deserialize, Serialize};

    /// Serialize a `HashMap` as a `Vec` of key-value pairs.
    ///
    /// # Errors
    ///
    /// Returns a serialization error if any key or value fails to serialize.
    #[allow(clippy::implicit_hasher)]
    pub fn serialize<S, K, V>(map: &HashMap<K, V>, serializer: S) -> Result<S::Ok, S::Error>
    where
        K: Serialize + Eq + Hash,
        V: Serialize,
        S: Serializer,
    {
        let pairs: Vec<(&K, &V)> = map.iter().collect();
        pairs.serialize(serializer)
    }

    /// Deserialize a `Vec` of key-value pairs into a `HashMap`.
    ///
    /// # Errors
    ///
    /// Returns a deserialization error if the input is not a valid array
    /// of `(K, V)` pairs.
    pub fn deserialize<'de, D, K, V>(deserializer: D) -> Result<HashMap<K, V>, D::Error>
    where
        K: Deserialize<'de> + Eq + Hash,
        V: Deserialize<'de>,
        D: Deserializer<'de>,
    {
        let pairs: Vec<(K, V)> = Vec::deserialize(deserializer)?;
        Ok(pairs.into_iter().collect())
    }
}

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
}
