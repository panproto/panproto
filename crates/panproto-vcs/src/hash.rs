//! Content-addressing via canonical serialization and blake3 hashing.
//!
//! Every object in the VCS is identified by a blake3 hash of its canonical
//! `MessagePack` representation. Canonical forms sort all map entries by key
//! (using [`BTreeMap`]) and exclude derived/precomputed fields.

use panproto_gat::Name;
use std::collections::BTreeMap;
use std::fmt;
use std::str::FromStr;

use panproto_mig::Migration;
use panproto_schema::{
    Constraint, Edge, HyperEdge, RecursionPoint, Schema, Span, UsageMode, Variant, Vertex,
};
use serde::{Deserialize, Serialize};

use crate::error::VcsError;
use crate::object::{CommitObject, ComplementObject, DataSetObject, EditLogObject, TagObject};

/// A content-addressed object identifier: a blake3 hash (32 bytes).
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ObjectId([u8; 32]);

impl ObjectId {
    /// The zero object ID (useful as a sentinel).
    pub const ZERO: Self = Self([0u8; 32]);

    /// Create an `ObjectId` from raw bytes.
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Return the raw bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Return the first 7 hex characters (short form for display).
    #[must_use]
    pub fn short(&self) -> String {
        let full = self.to_string();
        full[..7].to_owned()
    }
}

impl fmt::Display for ObjectId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in &self.0 {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

impl fmt::Debug for ObjectId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ObjectId({})", self.short())
    }
}

/// Error parsing a hex string as an `ObjectId`.
#[derive(Debug, thiserror::Error)]
#[error("invalid object id: {reason}")]
pub struct ParseObjectIdError {
    reason: String,
}

impl FromStr for ObjectId {
    type Err = ParseObjectIdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.len() != 64 {
            return Err(ParseObjectIdError {
                reason: format!("expected 64 hex chars, got {}", s.len()),
            });
        }
        let mut bytes = [0u8; 32];
        for (i, byte) in bytes.iter_mut().enumerate() {
            *byte =
                u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).map_err(|e| ParseObjectIdError {
                    reason: e.to_string(),
                })?;
        }
        Ok(Self(bytes))
    }
}

// ---------------------------------------------------------------------------
// Canonical forms — private structs with deterministic field ordering.
// These exist only for hashing; they are never persisted directly.
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct CanonicalVertex {
    id: String,
    kind: String,
    nsid: Option<String>,
}

impl From<&Vertex> for CanonicalVertex {
    fn from(v: &Vertex) -> Self {
        Self {
            id: v.id.to_string(),
            kind: v.kind.to_string(),
            nsid: v.nsid.as_ref().map(Name::to_string),
        }
    }
}

#[derive(Serialize)]
struct CanonicalHyperEdge {
    id: String,
    kind: String,
    signature: BTreeMap<String, String>,
    parent_label: String,
}

impl From<&HyperEdge> for CanonicalHyperEdge {
    fn from(he: &HyperEdge) -> Self {
        Self {
            id: he.id.to_string(),
            kind: he.kind.to_string(),
            signature: he
                .signature
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            parent_label: he.parent_label.to_string(),
        }
    }
}

/// Canonical schema: `BTreeMap` fields, sorted `Vec`s, excludes precomputed indices.
#[derive(Serialize)]
struct CanonicalSchema {
    protocol: String,
    vertices: BTreeMap<String, CanonicalVertex>,
    edges: BTreeMap<Edge, String>,
    hyper_edges: BTreeMap<String, CanonicalHyperEdge>,
    constraints: BTreeMap<String, Vec<Constraint>>,
    required: BTreeMap<String, Vec<Edge>>,
    nsids: BTreeMap<String, String>,
    variants: BTreeMap<String, Vec<Variant>>,
    orderings: BTreeMap<Edge, u32>,
    recursion_points: BTreeMap<String, RecursionPoint>,
    spans: BTreeMap<String, Span>,
    usage_modes: BTreeMap<Edge, UsageMode>,
    nominal: BTreeMap<String, bool>,
    coercions: BTreeMap<(String, String), panproto_expr::Expr>,
    mergers: BTreeMap<String, panproto_expr::Expr>,
    defaults: BTreeMap<String, panproto_expr::Expr>,
    policies: BTreeMap<String, panproto_expr::Expr>,
}

impl From<&Schema> for CanonicalSchema {
    fn from(s: &Schema) -> Self {
        let mut constraints: BTreeMap<String, Vec<Constraint>> = s
            .constraints
            .iter()
            .map(|(k, v)| {
                let mut sorted = v.clone();
                sorted.sort();
                (k.to_string(), sorted)
            })
            .collect();
        // Remove empty constraint lists.
        constraints.retain(|_, v| !v.is_empty());

        let mut required: BTreeMap<String, Vec<Edge>> = s
            .required
            .iter()
            .map(|(k, v)| {
                let mut sorted = v.clone();
                sorted.sort();
                (k.to_string(), sorted)
            })
            .collect();
        required.retain(|_, v| !v.is_empty());

        Self {
            protocol: s.protocol.clone(),
            vertices: s
                .vertices
                .iter()
                .map(|(k, v)| (k.to_string(), CanonicalVertex::from(v)))
                .collect(),
            edges: s
                .edges
                .iter()
                .map(|(k, v)| (k.clone(), v.to_string()))
                .collect(),
            hyper_edges: s
                .hyper_edges
                .iter()
                .map(|(k, v)| (k.to_string(), CanonicalHyperEdge::from(v)))
                .collect(),
            constraints,
            required,
            nsids: s
                .nsids
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            variants: s
                .variants
                .iter()
                .map(|(k, v)| (k.to_string(), v.clone()))
                .collect(),
            orderings: s.orderings.iter().map(|(k, v)| (k.clone(), *v)).collect(),
            recursion_points: s
                .recursion_points
                .iter()
                .map(|(k, v)| (k.to_string(), v.clone()))
                .collect(),
            spans: s
                .spans
                .iter()
                .map(|(k, v)| (k.to_string(), v.clone()))
                .collect(),
            usage_modes: s
                .usage_modes
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
            nominal: s.nominal.iter().map(|(k, v)| (k.to_string(), *v)).collect(),
            coercions: s
                .coercions
                .iter()
                .map(|((k1, k2), v)| ((k1.to_string(), k2.to_string()), v.clone()))
                .collect(),
            mergers: s
                .mergers
                .iter()
                .map(|(k, v)| (k.to_string(), v.clone()))
                .collect(),
            defaults: s
                .defaults
                .iter()
                .map(|(k, v)| (k.to_string(), v.clone()))
                .collect(),
            policies: s
                .policies
                .iter()
                .map(|(k, v)| (k.to_string(), v.clone()))
                .collect(),
        }
    }
}

/// Canonical migration: all `HashMap` fields become `BTreeMap`.
#[derive(Serialize)]
struct CanonicalMigration {
    src: ObjectId,
    tgt: ObjectId,
    vertex_map: BTreeMap<String, String>,
    edge_map: BTreeMap<Edge, Edge>,
    hyper_edge_map: BTreeMap<String, String>,
    label_map: BTreeMap<(String, String), String>,
    resolver: BTreeMap<(String, String), Edge>,
    hyper_resolver: BTreeMap<String, (String, BTreeMap<String, String>)>,
    expr_resolvers: BTreeMap<(String, String), panproto_expr::Expr>,
}

// ---------------------------------------------------------------------------
// Public hashing functions
// ---------------------------------------------------------------------------

/// Compute the content-addressed ID of a schema.
///
/// The hash excludes precomputed indices (`outgoing`, `incoming`, `between`)
/// since those are derived data.
///
/// # Errors
///
/// Returns an error if canonical serialization fails.
pub fn hash_schema(schema: &Schema) -> Result<ObjectId, VcsError> {
    let canonical = CanonicalSchema::from(schema);
    let bytes = rmp_serde::to_vec(&canonical)?;
    Ok(ObjectId(blake3::hash(&bytes).into()))
}

/// Compute the content-addressed ID of a migration.
///
/// The hash includes the source and target schema object IDs so that the
/// same morphism applied between different schema pairs produces distinct
/// migration IDs.
///
/// # Errors
///
/// Returns an error if canonical serialization fails.
pub fn hash_migration(
    src: ObjectId,
    tgt: ObjectId,
    migration: &Migration,
) -> Result<ObjectId, VcsError> {
    // Flatten hyper_resolver to BTreeMap with sorted inner maps.
    let hyper_resolver: BTreeMap<String, (String, BTreeMap<String, String>)> = migration
        .hyper_resolver
        .iter()
        .map(|((he_id, _labels), (tgt_he, remap))| {
            let sorted_remap: BTreeMap<String, String> = remap
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect();
            (he_id.to_string(), (tgt_he.to_string(), sorted_remap))
        })
        .collect();

    let canonical = CanonicalMigration {
        src,
        tgt,
        vertex_map: migration
            .vertex_map
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
        edge_map: migration
            .edge_map
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect(),
        hyper_edge_map: migration
            .hyper_edge_map
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
        label_map: migration
            .label_map
            .iter()
            .map(|((k1, k2), v)| ((k1.to_string(), k2.to_string()), v.to_string()))
            .collect(),
        resolver: migration
            .resolver
            .iter()
            .map(|((k1, k2), v)| ((k1.to_string(), k2.to_string()), v.clone()))
            .collect(),
        hyper_resolver,
        expr_resolvers: migration
            .expr_resolvers
            .iter()
            .map(|((k1, k2), v)| ((k1.to_string(), k2.to_string()), v.clone()))
            .collect(),
    };
    let bytes = rmp_serde::to_vec(&canonical)?;
    Ok(ObjectId(blake3::hash(&bytes).into()))
}

/// Compute the content-addressed ID of a commit.
///
/// # Errors
///
/// Returns an error if serialization fails.
pub fn hash_commit(commit: &CommitObject) -> Result<ObjectId, VcsError> {
    let bytes = rmp_serde::to_vec(commit)?;
    Ok(ObjectId(blake3::hash(&bytes).into()))
}

/// Hash an annotated tag object.
///
/// # Errors
///
/// Returns an error if serialization fails.
pub fn hash_tag(tag: &TagObject) -> Result<ObjectId, VcsError> {
    let bytes = rmp_serde::to_vec(tag)?;
    Ok(ObjectId(blake3::hash(&bytes).into()))
}

/// Compute the content-addressed ID of a data set.
///
/// Uses a canonical `BTreeMap` form to ensure deterministic hashing
/// regardless of field ordering.
///
/// # Errors
///
/// Returns an error if serialization fails.
pub fn hash_dataset(dataset: &DataSetObject) -> Result<ObjectId, VcsError> {
    let canonical: BTreeMap<&str, Vec<u8>> = BTreeMap::from([
        ("schema_id", rmp_serde::to_vec(&dataset.schema_id)?),
        ("data", rmp_serde::to_vec(&dataset.data)?),
        ("record_count", rmp_serde::to_vec(&dataset.record_count)?),
    ]);
    let bytes = rmp_serde::to_vec(&canonical)?;
    Ok(ObjectId(blake3::hash(&bytes).into()))
}

/// Compute the content-addressed ID of a complement.
///
/// Uses a canonical `BTreeMap` form to ensure deterministic hashing.
///
/// # Errors
///
/// Returns an error if serialization fails.
pub fn hash_complement(complement: &ComplementObject) -> Result<ObjectId, VcsError> {
    let canonical: BTreeMap<&str, Vec<u8>> = BTreeMap::from([
        ("migration_id", rmp_serde::to_vec(&complement.migration_id)?),
        ("data_id", rmp_serde::to_vec(&complement.data_id)?),
        ("complement", rmp_serde::to_vec(&complement.complement)?),
    ]);
    let bytes = rmp_serde::to_vec(&canonical)?;
    Ok(ObjectId(blake3::hash(&bytes).into()))
}

/// Compute the content-addressed ID of an expression.
///
/// The hash is computed from the canonical `MessagePack` serialization
/// of the expression AST.
///
/// # Errors
///
/// Returns an error if serialization fails.
pub fn hash_expr(expr: &panproto_expr::Expr) -> Result<ObjectId, VcsError> {
    let bytes = rmp_serde::to_vec(expr)?;
    Ok(ObjectId(blake3::hash(&bytes).into()))
}

/// Compute the content-addressed ID of a protocol definition.
///
/// The hash includes all protocol fields via direct serialization.
///
/// # Errors
///
/// Returns an error if serialization fails.
pub fn hash_protocol(protocol: &panproto_schema::Protocol) -> Result<ObjectId, VcsError> {
    let bytes = rmp_serde::to_vec(protocol)?;
    Ok(ObjectId(blake3::hash(&bytes).into()))
}

/// Compute the content-addressed ID of an edit log.
///
/// Uses a canonical `BTreeMap` form to ensure deterministic hashing.
///
/// # Errors
///
/// Returns an error if serialization fails.
pub fn hash_edit_log(edit_log: &EditLogObject) -> Result<ObjectId, VcsError> {
    let canonical: BTreeMap<&str, Vec<u8>> = BTreeMap::from([
        ("schema_id", rmp_serde::to_vec(&edit_log.schema_id)?),
        ("data_id", rmp_serde::to_vec(&edit_log.data_id)?),
        ("edits", rmp_serde::to_vec(&edit_log.edits)?),
        ("edit_count", rmp_serde::to_vec(&edit_log.edit_count)?),
        (
            "final_complement",
            rmp_serde::to_vec(&edit_log.final_complement)?,
        ),
    ]);
    let bytes = rmp_serde::to_vec(&canonical)?;
    Ok(ObjectId(blake3::hash(&bytes).into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use panproto_schema::Vertex;
    use smallvec::SmallVec;
    use std::collections::HashMap;

    fn make_schema(vertices: &[(&str, &str)], edges: &[Edge]) -> Schema {
        let mut vert_map = HashMap::new();
        let mut edge_map = HashMap::new();
        let mut outgoing: HashMap<Name, SmallVec<Edge, 4>> = HashMap::new();
        let mut incoming: HashMap<Name, SmallVec<Edge, 4>> = HashMap::new();
        let mut between: HashMap<(Name, Name), SmallVec<Edge, 2>> = HashMap::new();

        for (id, kind) in vertices {
            vert_map.insert(
                Name::from(*id),
                Vertex {
                    id: Name::from(*id),
                    kind: Name::from(*kind),
                    nsid: None,
                },
            );
        }
        for edge in edges {
            edge_map.insert(edge.clone(), edge.kind.clone());
            outgoing
                .entry(edge.src.clone())
                .or_default()
                .push(edge.clone());
            incoming
                .entry(edge.tgt.clone())
                .or_default()
                .push(edge.clone());
            between
                .entry((edge.src.clone(), edge.tgt.clone()))
                .or_default()
                .push(edge.clone());
        }

        Schema {
            protocol: "test".into(),
            vertices: vert_map,
            edges: edge_map,
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
            outgoing,
            incoming,
            between,
        }
    }

    #[test]
    fn hash_stability_same_schema() -> Result<(), Box<dyn std::error::Error>> {
        let s = make_schema(&[("a", "object"), ("b", "string")], &[]);
        let h1 = hash_schema(&s)?;
        let h2 = hash_schema(&s)?;
        assert_eq!(h1, h2);
        Ok(())
    }

    #[test]
    fn hash_differs_for_different_schemas() -> Result<(), Box<dyn std::error::Error>> {
        let s1 = make_schema(&[("a", "object")], &[]);
        let s2 = make_schema(&[("a", "object"), ("b", "string")], &[]);
        let h1 = hash_schema(&s1)?;
        let h2 = hash_schema(&s2)?;
        assert_ne!(h1, h2);
        Ok(())
    }

    #[test]
    fn hash_ignores_precomputed_indices() -> Result<(), Box<dyn std::error::Error>> {
        let edge = Edge {
            src: "a".into(),
            tgt: "b".into(),
            kind: "prop".into(),
            name: None,
        };
        let s1 = make_schema(&[("a", "object"), ("b", "string")], &[edge]);

        // Create the same schema but with empty precomputed indices.
        let mut s2 = s1.clone();
        s2.outgoing.clear();
        s2.incoming.clear();
        s2.between.clear();

        let h1 = hash_schema(&s1)?;
        let h2 = hash_schema(&s2)?;
        assert_eq!(h1, h2, "hash should not depend on precomputed indices");
        Ok(())
    }

    #[test]
    fn object_id_display_and_parse() -> Result<(), Box<dyn std::error::Error>> {
        let id = ObjectId::ZERO;
        let hex = id.to_string();
        assert_eq!(hex.len(), 64);
        assert!(hex.chars().all(|c| c == '0'));

        let parsed: ObjectId = hex.parse()?;
        assert_eq!(parsed, id);
        Ok(())
    }

    #[test]
    fn object_id_short() {
        let id = ObjectId::from_bytes([0xab; 32]);
        assert_eq!(id.short(), "abababa");
    }

    #[test]
    fn hash_commit_deterministic() -> Result<(), Box<dyn std::error::Error>> {
        let commit = CommitObject {
            schema_id: ObjectId::ZERO,
            parents: vec![],
            migration_id: None,
            protocol: "test".into(),
            author: "test-author".into(),
            timestamp: 1_234_567_890,
            message: "initial commit".into(),
            renames: vec![],
            protocol_id: None,
            data_ids: vec![],
            complement_ids: vec![],
            edit_log_ids: vec![],
        };
        let h1 = hash_commit(&commit)?;
        let h2 = hash_commit(&commit)?;
        assert_eq!(h1, h2);
        Ok(())
    }

    #[test]
    fn hash_migration_includes_endpoints() -> Result<(), Box<dyn std::error::Error>> {
        let mig = Migration::empty();
        let src1 = ObjectId::from_bytes([1; 32]);
        let src2 = ObjectId::from_bytes([2; 32]);
        let tgt = ObjectId::from_bytes([3; 32]);

        let h1 = hash_migration(src1, tgt, &mig)?;
        let h2 = hash_migration(src2, tgt, &mig)?;
        assert_ne!(
            h1, h2,
            "different source schemas should produce different migration IDs"
        );
        Ok(())
    }

    #[test]
    fn hash_dataset_stability() -> Result<(), Box<dyn std::error::Error>> {
        let ds = crate::object::DataSetObject {
            schema_id: ObjectId::from_bytes([1; 32]),
            data: vec![10, 20, 30],
            record_count: 3,
        };
        let h1 = hash_dataset(&ds)?;
        let h2 = hash_dataset(&ds)?;
        assert_eq!(h1, h2, "same dataset should produce the same hash");
        Ok(())
    }

    #[test]
    fn hash_complement_stability() -> Result<(), Box<dyn std::error::Error>> {
        let comp = crate::object::ComplementObject {
            migration_id: ObjectId::from_bytes([1; 32]),
            data_id: ObjectId::from_bytes([2; 32]),
            complement: vec![42],
        };
        let h1 = hash_complement(&comp)?;
        let h2 = hash_complement(&comp)?;
        assert_eq!(h1, h2, "same complement should produce the same hash");
        Ok(())
    }

    #[test]
    fn hash_protocol_stability() -> Result<(), Box<dyn std::error::Error>> {
        let proto = panproto_schema::Protocol {
            name: "test-proto".into(),
            ..Default::default()
        };
        let h1 = hash_protocol(&proto)?;
        let h2 = hash_protocol(&proto)?;
        assert_eq!(h1, h2, "same protocol should produce the same hash");
        Ok(())
    }

    #[test]
    fn hash_expr_stability() -> Result<(), Box<dyn std::error::Error>> {
        let expr = panproto_expr::Expr::Lit(panproto_expr::Literal::Int(42));
        let h1 = hash_expr(&expr)?;
        let h2 = hash_expr(&expr)?;
        assert_eq!(h1, h2, "same expression should produce the same hash");
        Ok(())
    }

    #[test]
    fn hash_expr_differs_for_different_values() -> Result<(), Box<dyn std::error::Error>> {
        let e1 = panproto_expr::Expr::Lit(panproto_expr::Literal::Int(1));
        let e2 = panproto_expr::Expr::Lit(panproto_expr::Literal::Int(2));
        let h1 = hash_expr(&e1)?;
        let h2 = hash_expr(&e2)?;
        assert_ne!(
            h1, h2,
            "different expressions should produce different hashes"
        );
        Ok(())
    }
}
