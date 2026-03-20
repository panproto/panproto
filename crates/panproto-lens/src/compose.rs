//! Lens composition.
//!
//! Two lenses can be composed when the target schema of the first matches
//! the source schema of the second. The resulting lens goes directly from
//! the first source to the second target.

use std::collections::HashMap;

use panproto_inst::CompiledMigration;
use panproto_schema::Edge;

use crate::Lens;
use crate::error::LensError;

/// Compose two lenses: the result goes from `l1.src_schema` to `l2.tgt_schema`.
///
/// The target schema of `l1` must be compatible with the source schema of `l2`.
///
/// # Errors
///
/// Returns `LensError::CompositionMismatch` if the schemas don't align.
pub fn compose(l1: &Lens, l2: &Lens) -> Result<Lens, LensError> {
    // Verify compatibility: l1's target should match l2's source
    if l1.tgt_schema.vertex_count() != l2.src_schema.vertex_count()
        || l1.tgt_schema.protocol != l2.src_schema.protocol
    {
        return Err(LensError::CompositionMismatch);
    }
    // Check that vertex IDs match exactly
    if l1
        .tgt_schema
        .vertices
        .keys()
        .collect::<std::collections::BTreeSet<_>>()
        != l2
            .src_schema
            .vertices
            .keys()
            .collect::<std::collections::BTreeSet<_>>()
    {
        return Err(LensError::CompositionMismatch);
    }

    let compiled = compose_compiled_migrations(&l1.compiled, &l2.compiled);

    Ok(Lens {
        compiled,
        src_schema: l1.src_schema.clone(),
        tgt_schema: l2.tgt_schema.clone(),
    })
}

/// Compose two compiled migrations.
///
/// The surviving sets are intersected (a vertex/edge must survive both),
/// and remaps are composed (l1's output feeds into l2's input).
pub(crate) fn compose_compiled_migrations(
    m1: &CompiledMigration,
    m2: &CompiledMigration,
) -> CompiledMigration {
    // Surviving verts: a vertex from the source must survive both migrations.
    // After m1, the vertex might be remapped; the remapped version must survive m2.
    let mut surviving_verts = std::collections::HashSet::new();
    for v in &m1.surviving_verts {
        let remapped = m1.vertex_remap.get(v).unwrap_or(v);
        if m2.surviving_verts.contains(remapped) || m2.surviving_verts.contains(v) {
            surviving_verts.insert(v.clone());
        }
    }

    // Surviving edges: compose similarly
    let mut surviving_edges = std::collections::HashSet::new();
    for e in &m1.surviving_edges {
        let remapped = m1.edge_remap.get(e).unwrap_or(e);
        if m2.surviving_edges.contains(remapped) || m2.surviving_edges.contains(e) {
            surviving_edges.insert(e.clone());
        }
    }

    // Compose vertex remaps: apply m1's remap, then m2's remap
    let mut vertex_remap = HashMap::new();
    for (src, mid) in &m1.vertex_remap {
        let final_v = m2.vertex_remap.get(mid).unwrap_or(mid).clone();
        vertex_remap.insert(src.clone(), final_v);
    }
    // Also include m2 remaps for vertices not in m1's remap
    for (mid, tgt) in &m2.vertex_remap {
        if !m1.vertex_remap.values().any(|v| v == mid) {
            vertex_remap
                .entry(mid.clone())
                .or_insert_with(|| tgt.clone());
        }
    }

    // Compose edge remaps
    let mut edge_remap: HashMap<Edge, Edge> = HashMap::new();
    for (src_e, mid_e) in &m1.edge_remap {
        let final_e = m2.edge_remap.get(mid_e).unwrap_or(mid_e).clone();
        edge_remap.insert(src_e.clone(), final_e);
    }

    // Compose resolvers
    let mut resolver = m1.resolver.clone();
    for (k, v) in &m2.resolver {
        resolver.entry(k.clone()).or_insert_with(|| v.clone());
    }

    // Compose hyper resolvers
    let mut hyper_resolver = m1.hyper_resolver.clone();
    for (k, v) in &m2.hyper_resolver {
        hyper_resolver.entry(k.clone()).or_insert_with(|| v.clone());
    }

    CompiledMigration {
        surviving_verts,
        surviving_edges,
        vertex_remap,
        edge_remap,
        resolver,
        hyper_resolver,
        field_transforms: HashMap::new(),
        conditional_survival: HashMap::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::{identity_lens, three_node_schema};

    #[test]
    fn compose_identity_with_identity() {
        let schema = three_node_schema();
        let l1 = identity_lens(&schema);
        let l2 = identity_lens(&schema);

        let composed = compose(&l1, &l2);
        assert!(composed.is_ok(), "composing identity lenses should succeed");

        let lens = composed.unwrap_or_else(|e| panic!("compose failed: {e}"));
        assert_eq!(
            lens.src_schema.vertex_count(),
            schema.vertex_count(),
            "composed src schema should match original"
        );
        assert_eq!(
            lens.tgt_schema.vertex_count(),
            schema.vertex_count(),
            "composed tgt schema should match original"
        );
    }
}
