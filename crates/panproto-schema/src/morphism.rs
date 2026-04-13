//! Explicit schema morphisms (functors between schema categories).
//!
//! A [`SchemaMorphism`] makes the category structure of schema
//! transformations explicit. It maps vertex IDs to vertex IDs and
//! edge structures to edge structures, recording the site-qualified
//! renames that produced the mapping.
//!
//! Schema morphisms compose associatively and can be lowered to
//! `CompiledMigration` for the restrict pipeline.

use std::collections::HashMap;

use panproto_gat::{Name, SiteRename};
use serde::{Deserialize, Serialize};

use crate::schema::Edge;

/// An explicit schema morphism (functor F: S → T).
///
/// Stores the vertex and edge mappings between a source and target
/// schema, together with the provenance (the site renames that
/// produced this morphism). Composition is sequential: `self ; other`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SchemaMorphism {
    /// Name of this morphism (for display/debugging).
    pub name: String,
    /// Source protocol name.
    pub src_protocol: String,
    /// Target protocol name.
    pub tgt_protocol: String,
    /// Vertex ID mapping: source vertex ID → target vertex ID.
    pub vertex_map: HashMap<Name, Name>,
    /// Edge mapping: source edge → target edge.
    #[serde(with = "crate::serde_helpers::map_as_vec")]
    pub edge_map: HashMap<Edge, Edge>,
    /// Provenance: the site renames that produced this morphism.
    pub renames: Vec<SiteRename>,
}

impl SchemaMorphism {
    /// Compose two schema morphisms: `self ; other`.
    ///
    /// The result maps source vertex/edge IDs from `self.src_protocol`
    /// to target IDs in `other.tgt_protocol`.
    #[must_use]
    pub fn compose(&self, other: &Self) -> Self {
        let mut vertex_map = HashMap::new();
        for (src, mid) in &self.vertex_map {
            if let Some(tgt) = other.vertex_map.get(mid) {
                vertex_map.insert(src.clone(), tgt.clone());
            }
            // Partial-map semantics: if `mid` is not in `other`'s map,
            // the vertex was dropped by `other` and should not appear
            // in the composed morphism.
        }

        let mut edge_map = HashMap::new();
        for (src_e, mid_e) in &self.edge_map {
            if let Some(tgt_e) = other.edge_map.get(mid_e) {
                edge_map.insert(src_e.clone(), tgt_e.clone());
            }
            // Edge dropped by `other`: omit from composed morphism.
        }

        let mut renames = self.renames.clone();
        renames.extend(other.renames.iter().cloned());

        Self {
            name: format!("{};{}", self.name, other.name),
            src_protocol: self.src_protocol.clone(),
            tgt_protocol: other.tgt_protocol.clone(),
            vertex_map,
            edge_map,
            renames,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compose_chains_vertex_maps() {
        let m1 = SchemaMorphism {
            name: "m1".into(),
            src_protocol: "a".into(),
            tgt_protocol: "b".into(),
            vertex_map: HashMap::from([(Name::from("v1"), Name::from("v2"))]),
            edge_map: HashMap::new(),
            renames: vec![],
        };
        let m2 = SchemaMorphism {
            name: "m2".into(),
            src_protocol: "b".into(),
            tgt_protocol: "c".into(),
            vertex_map: HashMap::from([(Name::from("v2"), Name::from("v3"))]),
            edge_map: HashMap::new(),
            renames: vec![],
        };
        let composed = m1.compose(&m2);
        assert_eq!(composed.vertex_map.get("v1").map(AsRef::as_ref), Some("v3"));
        assert_eq!(composed.src_protocol, "a");
        assert_eq!(composed.tgt_protocol, "c");
    }

    #[test]
    fn compose_preserves_renames() {
        let r1 = SiteRename::new(panproto_gat::NameSite::EdgeLabel, "a", "b");
        let r2 = SiteRename::new(panproto_gat::NameSite::VertexKind, "x", "y");
        let m1 = SchemaMorphism {
            name: "m1".into(),
            src_protocol: "p".into(),
            tgt_protocol: "p".into(),
            vertex_map: HashMap::new(),
            edge_map: HashMap::new(),
            renames: vec![r1.clone()],
        };
        let m2 = SchemaMorphism {
            name: "m2".into(),
            src_protocol: "p".into(),
            tgt_protocol: "p".into(),
            vertex_map: HashMap::new(),
            edge_map: HashMap::new(),
            renames: vec![r2.clone()],
        };
        let composed = m1.compose(&m2);
        assert_eq!(composed.renames.len(), 2);
        assert_eq!(composed.renames[0], r1);
        assert_eq!(composed.renames[1], r2);
    }

    #[test]
    fn compose_drops_unmapped_vertices() {
        // If m2 doesn't map a vertex, it should be dropped from composition.
        let m1 = SchemaMorphism {
            name: "m1".into(),
            src_protocol: "a".into(),
            tgt_protocol: "b".into(),
            vertex_map: HashMap::from([
                (Name::from("v1"), Name::from("v2")),
                (Name::from("v3"), Name::from("v4")),
            ]),
            edge_map: HashMap::new(),
            renames: vec![],
        };
        // m2 only maps v2, dropping v4.
        let m2 = SchemaMorphism {
            name: "m2".into(),
            src_protocol: "b".into(),
            tgt_protocol: "c".into(),
            vertex_map: HashMap::from([(Name::from("v2"), Name::from("v5"))]),
            edge_map: HashMap::new(),
            renames: vec![],
        };
        let composed = m1.compose(&m2);
        assert_eq!(composed.vertex_map.len(), 1);
        assert_eq!(composed.vertex_map.get("v1").map(AsRef::as_ref), Some("v5"));
        assert!(!composed.vertex_map.contains_key("v3"));
    }

    #[test]
    fn compose_associativity() {
        let m1 = SchemaMorphism {
            name: "m1".into(),
            src_protocol: "a".into(),
            tgt_protocol: "b".into(),
            vertex_map: HashMap::from([
                (Name::from("v1"), Name::from("v2")),
                (Name::from("v3"), Name::from("v4")),
            ]),
            edge_map: HashMap::new(),
            renames: vec![],
        };
        let m2 = SchemaMorphism {
            name: "m2".into(),
            src_protocol: "b".into(),
            tgt_protocol: "c".into(),
            vertex_map: HashMap::from([
                (Name::from("v2"), Name::from("v5")),
                // v4 dropped by m2
            ]),
            edge_map: HashMap::new(),
            renames: vec![],
        };
        let m3 = SchemaMorphism {
            name: "m3".into(),
            src_protocol: "c".into(),
            tgt_protocol: "d".into(),
            vertex_map: HashMap::from([(Name::from("v5"), Name::from("v6"))]),
            edge_map: HashMap::new(),
            renames: vec![],
        };

        let left = m1.compose(&m2).compose(&m3);
        let right = m1.compose(&m2.compose(&m3));
        assert_eq!(left.vertex_map, right.vertex_map);
    }
}
