//! Lens composition.
//!
//! Two lenses can be composed when the target schema of the first matches
//! the source schema of the second. The resulting lens goes directly from
//! the first source to the second target.

use std::collections::HashMap;

use panproto_gat::Name;
use panproto_inst::CompiledMigration;
use panproto_mig::{OnMissing, compose_relabeling};
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
    // The middle boundary must agree not only on vertices but on the
    // relational structure carried over them: the edge set and the
    // hyper-edge set. Two schemas with identical vertices but a differing
    // edge, edge kind, or fan would compose into a lens whose declared
    // intermediate schema does not exist, silently dropping or misrouting
    // arcs. Compare the vertex, edge, and hyper-edge key sets exactly.
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
    if l1
        .tgt_schema
        .edges
        .keys()
        .collect::<std::collections::BTreeSet<_>>()
        != l2
            .src_schema
            .edges
            .keys()
            .collect::<std::collections::BTreeSet<_>>()
    {
        return Err(LensError::CompositionMismatch);
    }
    if l1
        .tgt_schema
        .hyper_edges
        .keys()
        .collect::<std::collections::BTreeSet<_>>()
        != l2
            .src_schema
            .hyper_edges
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

    // Compose vertex remaps through the shared relabeling kernel: apply m1's
    // remap, then m2's remap, keeping m1's target where m2 does not remap it.
    let mut vertex_remap = compose_relabeling(
        &m1.vertex_remap,
        &m2.vertex_remap,
        OnMissing::KeepIntermediate,
    )
    .map;
    // Also include m2 remaps for vertices not in m1's remap.
    for (mid, tgt) in &m2.vertex_remap {
        if !m1.vertex_remap.values().any(|v| v == mid) {
            vertex_remap
                .entry(mid.clone())
                .or_insert_with(|| tgt.clone());
        }
    }

    // Compose edge remaps through the same kernel.
    let edge_remap: HashMap<Edge, Edge> =
        compose_relabeling(&m1.edge_remap, &m2.edge_remap, OnMissing::KeepIntermediate).map;

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
    let op_term_assignments = compose_op_term_assignments(m1, m2);
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
        op_term_assignments,
        expansion_path,
    }
}

/// Returns `true` iff `name` is a fixed point under `m1`: it lives in
/// both `m1`'s source and target spaces with the same name. This is
/// the predicate that lets us re-key per-anchor maps from `m2`'s
/// source space (= `m1`'s target space) into the composed migration's
/// source space (= `m1`'s source space) without name-space mixing.
fn unchanged_by_m1(m1: &CompiledMigration, name: &panproto_gat::Name) -> bool {
    m1.vertex_remap.get(name).map_or_else(
        || m1.surviving_verts.contains(name),
        |remapped| remapped == name,
    )
}

/// Compose `field_transforms` from two migrations, re-keying through
/// `vertex_remap`. The composed map is keyed by `m1`-source anchors
/// throughout. An `m2`-anchor that is neither the image of an
/// `m1`-source anchor under `m1.vertex_remap` nor a fixed point under
/// `m1` lives only in `m1`'s target space; its transforms cannot be
/// expressed against `m1`'s source and are dropped from the composed
/// map (they would otherwise corrupt the keyspace invariant).
fn compose_field_transforms(
    m1: &CompiledMigration,
    m2: &CompiledMigration,
) -> HashMap<panproto_gat::Name, Vec<panproto_inst::wtype::FieldTransform>> {
    compose_anchor_keyed_lists(m1, &m1.field_transforms, &m2.field_transforms)
}

/// Compose `op_term_assignments` from two migrations, re-keying through
/// `vertex_remap` with the same fixed-point discipline as
/// [`compose_field_transforms`].
fn compose_op_term_assignments(
    m1: &CompiledMigration,
    m2: &CompiledMigration,
) -> HashMap<panproto_gat::Name, Vec<panproto_inst::wtype::TermAssignment>> {
    compose_anchor_keyed_lists(m1, &m1.op_term_assignments, &m2.op_term_assignments)
}

/// Compose two anchor-keyed transform lists, re-keying `m2`'s entries into
/// `m1`'s source frame via `m1.vertex_remap`. An `m2`-anchor that is
/// neither the image of an `m1`-source anchor nor a fixed point under `m1`
/// lives only in `m1`'s target space; its entries have no representation
/// in `m1`'s source and are dropped to preserve the keyspace invariant.
fn compose_anchor_keyed_lists<T: Clone>(
    m1: &CompiledMigration,
    m1_map: &HashMap<panproto_gat::Name, Vec<T>>,
    m2_map: &HashMap<panproto_gat::Name, Vec<T>>,
) -> HashMap<panproto_gat::Name, Vec<T>> {
    let mut result = m1_map.clone();
    for (m2_anchor, m2_entries) in m2_map {
        let mut found = false;
        for (m1_src, m1_tgt) in &m1.vertex_remap {
            if m1_tgt == m2_anchor {
                result
                    .entry(m1_src.clone())
                    .or_default()
                    .extend(m2_entries.iter().cloned());
                found = true;
            }
        }
        if !found && unchanged_by_m1(m1, m2_anchor) {
            result
                .entry(m2_anchor.clone())
                .or_default()
                .extend(m2_entries.iter().cloned());
        }
    }
    result
}

/// Compose `conditional_survival` predicates, AND-ing when both exist.
/// Re-keys via the same fixed-point discipline as
/// [`compose_field_transforms`]: predicates whose anchor lives only in
/// `m1`'s target space are dropped rather than injected with a foreign
/// key. The AND-conjunction is taken in the composed-source frame, so
/// `m2_pred`'s free variables are interpreted against the schema
/// presented to `m1`'s output (= `m2`'s input).
///
/// Free-variable scope: when `m1` explicitly drops or renames a field
/// on `anchor` whose name is also free in `m2_pred`, the AND-merged
/// predicate would reference a variable that does not exist at
/// evaluation time. We detect that statically: any `m2_pred` whose
/// free-variable set intersects the keys dropped or renamed-away by
/// `m1`'s field transforms on the corresponding anchor is
/// conservatively rewritten to `false` on its own anchor (the
/// variable cannot be present, so the predicate cannot legitimately
/// be evaluated; refusing to keep the row is the safe default and
/// matches the audit's "default fail" recommendation).
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
                let scoped = scope_check_predicate(m2_pred, m1, m1_src);
                result
                    .entry(m1_src.clone())
                    .and_modify(|existing| {
                        *existing = panproto_expr::Expr::Builtin(
                            panproto_expr::BuiltinOp::And,
                            vec![existing.clone(), scoped.clone()],
                        );
                    })
                    .or_insert(scoped);
            }
        }
        if !found && unchanged_by_m1(m1, m2_anchor) {
            let scoped = scope_check_predicate(m2_pred, m1, m2_anchor);
            result
                .entry(m2_anchor.clone())
                .and_modify(|existing| {
                    *existing = panproto_expr::Expr::Builtin(
                        panproto_expr::BuiltinOp::And,
                        vec![existing.clone(), scoped.clone()],
                    );
                })
                .or_insert(scoped);
        }
    }
    result
}

/// Returns `m2_pred` unchanged when every free top-level variable
/// still exists at the composed evaluation site, or the constant
/// `false` when `m1` drops, renames-away, or filters-out a field
/// referenced in `m2_pred`.
///
/// Detection rules (top-level keys only — nested-path access through
/// `Field`/`Index` is not analysed because `panproto_expr::free_vars`
/// returns only top-level `Var` names; a `PathTransform` on `attrs`
/// affects only nested keys and so is not relevant here):
///
/// * `DropField { key }` removes `key`.
/// * `RenameField { old_key, .. }` removes `old_key`.
/// * `KeepFields { keys }` removes any top-level field not in `keys`;
///   we conservatively flag every free variable not in the
///   intersection of all `KeepFields` retain sets on this anchor.
/// * `Case { branches }` would require all branches to drop a key
///   for that key to be statically certain-dropped; the conservative
///   approximation here is to skip `Case` entirely (no false
///   positives on conditional drops, with a known soundness gap if
///   every branch happens to drop the same key).
/// * `PathTransform`, `AddField`, `ApplyExpr`, `ComputeField`,
///   `MapReferences`: do not remove top-level bindings.
fn scope_check_predicate(
    pred: &panproto_expr::Expr,
    m1: &CompiledMigration,
    anchor: &panproto_gat::Name,
) -> panproto_expr::Expr {
    let Some(m1_xforms) = m1.field_transforms.get(anchor) else {
        return pred.clone();
    };
    let analysis = analyse_field_transforms(m1_xforms);
    if analysis.dropped.is_empty() && analysis.keep_intersection.is_none() {
        return pred.clone();
    }
    let free = panproto_expr::free_vars(pred);
    let dropped_hit = free.iter().any(|v| analysis.dropped.contains(v.as_ref()));
    let keep_violation = analysis
        .keep_intersection
        .as_ref()
        .is_some_and(|keep| free.iter().any(|v| !keep.contains(v.as_ref())));
    if dropped_hit || keep_violation {
        // Conservative: refuse to keep the row when the predicate
        // depends on a field that no longer exists.
        return panproto_expr::Expr::Lit(panproto_expr::Literal::Bool(false));
    }
    pred.clone()
}

/// Static analysis result for a single anchor's transform list.
struct FieldDropAnalysis {
    /// Keys explicitly dropped or renamed-away.
    dropped: std::collections::HashSet<String>,
    /// If any `KeepFields` is present, the intersection of all its
    /// retain sets — every free variable outside this set has been
    /// dropped by the filter.
    keep_intersection: Option<std::collections::HashSet<String>>,
}

fn analyse_field_transforms(xforms: &[panproto_inst::wtype::FieldTransform]) -> FieldDropAnalysis {
    use panproto_inst::wtype::FieldTransform;
    let mut dropped = std::collections::HashSet::new();
    let mut keep_intersection: Option<std::collections::HashSet<String>> = None;
    for x in xforms {
        match x {
            FieldTransform::DropField { key } => {
                dropped.insert(key.clone());
            }
            FieldTransform::RenameField { old_key, .. } => {
                dropped.insert(old_key.clone());
            }
            FieldTransform::KeepFields { keys } => {
                let next: std::collections::HashSet<String> = keys.iter().cloned().collect();
                keep_intersection = Some(match keep_intersection {
                    None => next,
                    Some(prev) => prev.intersection(&next).cloned().collect(),
                });
            }
            // PathTransform on `path = ["attrs"]` drops nested
            // `attrs.x`, not top-level `x`; free_vars sees only
            // top-level Var names, so PathTransform is not relevant
            // to this analysis.
            //
            // Case branches are conditional; static drop-detection
            // would require all branches to drop the same key. We
            // skip Case rather than return spurious false-rewrites.
            //
            // AddField / ApplyExpr / ComputeField / MapReferences:
            // do not remove a free-name binding.
            _ => {}
        }
    }
    FieldDropAnalysis {
        dropped,
        keep_intersection,
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

    #[test]
    fn compose_rejects_edge_mismatch() {
        use std::collections::BTreeSet;
        let schema = three_node_schema();
        // Identical vertices, but the middle boundary differs by one edge.
        let mut altered = schema.clone();
        let victim = altered.edges.keys().next().cloned();
        if let Some(victim) = victim {
            altered.edges.remove(&victim);
        }

        // Precondition of the test: vertex sets still match exactly, so
        // the vertex-only check would have let this through.
        assert_eq!(
            schema.vertices.keys().collect::<BTreeSet<_>>(),
            altered.vertices.keys().collect::<BTreeSet<_>>(),
            "vertex sets must be identical for this test to exercise the edge check"
        );

        let l1 = identity_lens(&schema);
        let l2 = identity_lens(&altered);
        let result = compose(&l1, &l2);
        assert!(
            matches!(result, Err(LensError::CompositionMismatch)),
            "composition across an edge mismatch must be rejected: {result:?}"
        );
    }

    #[test]
    fn compose_rejects_hyper_edge_mismatch() {
        use panproto_gat::Name;
        use panproto_schema::HyperEdge;
        use std::collections::HashMap;

        let schema = three_node_schema();
        // Identical vertices and edges, but add a hyper-edge to one side.
        let mut altered = schema.clone();
        altered.hyper_edges.insert(
            Name::from("fan"),
            HyperEdge {
                id: Name::from("fan"),
                kind: Name::from("fan"),
                signature: HashMap::new(),
                parent_label: Name::from("post:body"),
            },
        );

        let l1 = identity_lens(&schema);
        let l2 = identity_lens(&altered);
        let result = compose(&l1, &l2);
        assert!(
            matches!(result, Err(LensError::CompositionMismatch)),
            "composition across a hyper-edge mismatch must be rejected: {result:?}"
        );
    }
}
