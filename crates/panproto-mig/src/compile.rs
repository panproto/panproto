//! Migration compilation for fast per-record application.
//!
//! [`compile`] pre-computes surviving vertex/edge sets, remapping
//! tables, and resolver lookups so that `lift_wtype` and `lift_functor`
//! can process each record in O(nodes) time without re-deriving the
//! migration structure.

use std::collections::{HashMap, HashSet};

use panproto_inst::CompiledMigration;
use panproto_schema::Schema;

use crate::error::ExistenceError;
use crate::migration::Migration;

/// Compile a migration specification into a form optimized for
/// per-record application.
///
/// The compilation computes:
/// 1. The surviving vertex set (image of `vertex_map` in the target)
/// 2. The surviving edge set (image of `edge_map`)
/// 3. Vertex and edge remap tables
/// 4. Resolver copies
///
/// # Errors
///
/// Returns `ExistenceError::WellFormedness` if the migration references
/// vertices or edges not present in either schema.
pub fn compile(
    src: &Schema,
    tgt: &Schema,
    migration: &Migration,
) -> Result<CompiledMigration, ExistenceError> {
    // Step 1: Compute surviving vertices (target vertices in the image of vertex_map).
    let mut surviving_verts = HashSet::new();
    let mut vertex_remap = HashMap::new();

    for (src_v, tgt_v) in &migration.vertex_map {
        if !tgt.has_vertex(tgt_v) {
            return Err(ExistenceError::WellFormedness {
                message: format!("vertex_map target {tgt_v} (from {src_v}) not in target schema"),
            });
        }
        surviving_verts.insert(tgt_v.clone());
        vertex_remap.insert(src_v.clone(), tgt_v.clone());
    }

    // Step 2: Compute surviving edges (target edges in the image of edge_map).
    let mut surviving_edges = HashSet::new();
    let mut edge_remap = HashMap::new();

    for (src_e, tgt_e) in &migration.edge_map {
        surviving_edges.insert(tgt_e.clone());
        edge_remap.insert(src_e.clone(), tgt_e.clone());
    }

    // Also include edges in the target that are between surviving vertices
    // and not explicitly remapped. These edges "survive" because both
    // endpoints do.
    for edge in tgt.edges.keys() {
        if surviving_verts.contains(&edge.src) && surviving_verts.contains(&edge.tgt) {
            surviving_edges.insert(edge.clone());
        }
    }

    // Step 3: Generate field_transforms from schema coercions.
    let mut field_transforms = HashMap::new();
    for (src_v, tgt_v) in &migration.vertex_map {
        if let (Some(src_vert), Some(tgt_vert)) = (src.vertex(src_v), tgt.vertex(tgt_v)) {
            if src_vert.kind != tgt_vert.kind {
                if let Some(coercion_spec) = tgt
                    .coercions
                    .get(&(src_vert.kind.clone(), tgt_vert.kind.clone()))
                {
                    field_transforms
                        .entry(src_v.clone())
                        .or_insert_with(Vec::new)
                        .push(panproto_inst::FieldTransform::ApplyExpr {
                            key: "__value__".to_string(),
                            expr: coercion_spec.forward.clone(),
                            inverse: coercion_spec.inverse.clone(),
                            coercion_class: coercion_spec.class,
                        });
                }
            }
        }
    }

    // Step 4: Copy resolver tables.
    let resolver = migration.resolver.clone();

    // Step 5: Build hyper-resolver (convert key format).
    let mut hyper_resolver = HashMap::new();
    for ((he_id, _labels), (tgt_he_id, label_map)) in &migration.hyper_resolver {
        // The inst-level CompiledMigration uses he_id -> (new_id, label_map).
        // We flatten the labels key since the inst crate indexes by he_id.
        hyper_resolver.insert(he_id.clone(), (tgt_he_id.clone(), label_map.clone()));
    }

    Ok(CompiledMigration {
        surviving_verts,
        surviving_edges,
        vertex_remap,
        edge_remap,
        resolver,
        hyper_resolver,
        field_transforms,
        conditional_survival: HashMap::new(),
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use panproto_gat::Name;
    use panproto_schema::{Edge, Vertex};

    fn test_schema(vertices: &[(&str, &str)], edges: &[Edge]) -> Schema {
        let mut vert_map = HashMap::new();
        let mut edge_map = HashMap::new();
        let mut outgoing: HashMap<Name, smallvec::SmallVec<Edge, 4>> = HashMap::new();
        let mut incoming: HashMap<Name, smallvec::SmallVec<Edge, 4>> = HashMap::new();
        let mut between: HashMap<(Name, Name), smallvec::SmallVec<Edge, 2>> = HashMap::new();

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
    fn compile_identity() {
        let edge = Edge {
            src: "a".into(),
            tgt: "b".into(),
            kind: "prop".into(),
            name: Some("x".into()),
        };
        let schema = test_schema(
            &[("a", "object"), ("b", "string")],
            std::slice::from_ref(&edge),
        );

        let mig = Migration::identity(&["a".into(), "b".into()], std::slice::from_ref(&edge));

        let compiled = compile(&schema, &schema, &mig);
        assert!(compiled.is_ok());
        let c = compiled.unwrap_or_else(|_| panic!("compile should succeed"));
        assert_eq!(c.surviving_verts.len(), 2);
        assert!(c.surviving_verts.contains("a"));
        assert!(c.surviving_verts.contains("b"));
        assert!(c.surviving_edges.contains(&edge));
    }

    #[test]
    #[allow(clippy::similar_names)]
    fn compile_projection_drops_vertex() {
        let edge_ab = Edge {
            src: "a".into(),
            tgt: "b".into(),
            kind: "prop".into(),
            name: Some("x".into()),
        };
        let edge_ac = Edge {
            src: "a".into(),
            tgt: "c".into(),
            kind: "prop".into(),
            name: Some("y".into()),
        };

        let src = test_schema(
            &[("a", "object"), ("b", "string"), ("c", "string")],
            &[edge_ab.clone(), edge_ac],
        );
        let tgt = test_schema(
            &[("a", "object"), ("b", "string")],
            std::slice::from_ref(&edge_ab),
        );

        let mig = Migration {
            vertex_map: HashMap::from([("a".into(), "a".into()), ("b".into(), "b".into())]),
            edge_map: HashMap::from([(edge_ab.clone(), edge_ab)]),
            hyper_edge_map: HashMap::new(),
            label_map: HashMap::new(),
            resolver: HashMap::new(),
            hyper_resolver: HashMap::new(),
            expr_resolvers: HashMap::new(),
        };

        let compiled = compile(&src, &tgt, &mig);
        assert!(compiled.is_ok());
        let c = compiled.unwrap_or_else(|_| panic!("compile should succeed"));
        assert_eq!(c.surviving_verts.len(), 2);
        assert!(!c.surviving_verts.contains("c"));
    }
}
