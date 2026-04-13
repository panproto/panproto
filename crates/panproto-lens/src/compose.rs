//! Lens composition.
//!
//! Two lenses can be composed when the target schema of the first matches
//! the source schema of the second. The resulting lens goes directly from
//! the first source to the second target.

use std::collections::HashMap;

use panproto_gat::Name;
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
        // Only the remapped vertex should be checked against m2's surviving set.
        // Checking the original vertex `v` against m2 is incorrect: `v` is in
        // m1's source space, not m2's source space.
        if m2.surviving_verts.contains(remapped) {
            surviving_verts.insert(v.clone());
        }
    }

    // Surviving edges: compose similarly
    let mut surviving_edges = std::collections::HashSet::new();
    for e in &m1.surviving_edges {
        let remapped = m1.edge_remap.get(e).unwrap_or(e);
        if m2.surviving_edges.contains(remapped) {
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

    let field_transforms = compose_field_transforms(m1, m2);
    let conditional_survival = compose_conditional_survival(m1, m2);

    // Compose expansion paths: m1's paths may need to be extended by m2's
    // paths. If m1 expands (src, tgt) through intermediates, and m2 has
    // a further expansion from the remapped tgt, chain them together.
    let mut expansion_path: HashMap<(Name, Name), Vec<Name>> = HashMap::new();
    for ((src, tgt), mids) in &m1.expansion_path {
        let remapped_tgt = m1.vertex_remap.get(tgt).unwrap_or(tgt);
        // Check if m2 extends from remapped_tgt to any further vertex.
        let mut found_chain = false;
        for ((m2_src, m2_tgt), m2_mids) in &m2.expansion_path {
            if m2_src == remapped_tgt {
                let mut combined = mids.clone();
                combined.extend(m2_mids.iter().cloned());
                expansion_path.insert((src.clone(), m2_tgt.clone()), combined);
                found_chain = true;
            }
        }
        if !found_chain {
            expansion_path.insert((src.clone(), tgt.clone()), mids.clone());
        }
    }
    // Include m2 entries for pairs not covered by m1's composition.
    for (k, v) in &m2.expansion_path {
        expansion_path.entry(k.clone()).or_insert_with(|| v.clone());
    }

    CompiledMigration {
        surviving_verts,
        surviving_edges,
        vertex_remap,
        edge_remap,
        resolver,
        hyper_resolver,
        field_transforms,
        conditional_survival,
        expansion_path,
    }
}

/// Compose `field_transforms` from two migrations, re-keying through `vertex_remap`.
fn compose_field_transforms(
    m1: &CompiledMigration,
    m2: &CompiledMigration,
) -> HashMap<panproto_gat::Name, Vec<panproto_inst::wtype::FieldTransform>> {
    let mut result = m1.field_transforms.clone();
    for (m2_anchor, m2_transforms) in &m2.field_transforms {
        let mut found = false;
        for (m1_src, m1_tgt) in &m1.vertex_remap {
            if m1_tgt == m2_anchor {
                result
                    .entry(m1_src.clone())
                    .or_default()
                    .extend(m2_transforms.iter().cloned());
                found = true;
            }
        }
        if !found {
            result
                .entry(m2_anchor.clone())
                .or_default()
                .extend(m2_transforms.iter().cloned());
        }
    }
    result
}

/// Compose `conditional_survival` predicates, AND-ing when both exist.
fn compose_conditional_survival(
    m1: &CompiledMigration,
    m2: &CompiledMigration,
) -> HashMap<panproto_gat::Name, panproto_expr::Expr> {
    let mut result = m1.conditional_survival.clone();
    for (m2_anchor, m2_pred) in &m2.conditional_survival {
        let mut found = false;
        for (m1_src, m1_tgt) in &m1.vertex_remap {
            if m1_tgt == m2_anchor {
                found = true;
                result
                    .entry(m1_src.clone())
                    .and_modify(|existing| {
                        *existing = panproto_expr::Expr::Builtin(
                            panproto_expr::BuiltinOp::And,
                            vec![existing.clone(), m2_pred.clone()],
                        );
                    })
                    .or_insert_with(|| m2_pred.clone());
            }
        }
        if !found {
            result
                .entry(m2_anchor.clone())
                .and_modify(|existing| {
                    *existing = panproto_expr::Expr::Builtin(
                        panproto_expr::BuiltinOp::And,
                        vec![existing.clone(), m2_pred.clone()],
                    );
                })
                .or_insert_with(|| m2_pred.clone());
        }
    }
    result
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
