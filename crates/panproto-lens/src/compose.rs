//! Lens composition.
//!
//! Two lenses can be composed when the target schema of the first matches
//! the source schema of the second. The resulting lens goes directly from
//! the first source to the second target.

use std::collections::{HashMap, HashSet};

use panproto_expr::Expr;
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

    let compiled = compose_compiled_migrations(&l1.compiled, &l2.compiled, Some(&l1.src_schema))?;

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
///
/// `src_schema` is `m1`'s source schema when the caller has it. It is used
/// only to tell a dropped `extra_fields` key apart from a surviving child
/// edge of the same name; pass `None` to skip that diagnostic entirely.
///
/// # Errors
///
/// Returns [`LensError::ComposeUnboundField`] when a value-level transform
/// of `m2` reads a field that `m1` drops, which would otherwise surface as
/// an `UnboundVariable` at evaluation time.
pub(crate) fn compose_compiled_migrations(
    m1: &CompiledMigration,
    m2: &CompiledMigration,
    src_schema: Option<&panproto_schema::Schema>,
) -> Result<CompiledMigration, LensError> {
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
    // Also include m2 remaps for vertices m1's remap does not reach.
    // The reached set is collected once: asking `values().any(...)` per
    // entry walks all of m1's remap for every entry of m2's.
    let m1_targets: HashSet<&Name> = m1.vertex_remap.values().collect();
    for (mid, tgt) in &m2.vertex_remap {
        if !m1_targets.contains(mid) {
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

    let field_transforms = compose_field_transforms(m1, m2, src_schema)?;
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

    Ok(CompiledMigration {
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
    })
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
///
/// Both coordinates are conjugated, not just the anchor. `m2`'s entries
/// are written against `m1`'s **output** field names, but the composed
/// list is evaluated in one batch against `m1`'s **source** node, so the
/// expressions' free variables are rewritten along the inverse of `m1`'s
/// renames on that anchor. Without this the composite interprets `m2`'s
/// variables in the wrong frame: `get(m2 ∘ m1)` rejects the expression
/// that `get(m2) ∘ get(m1)` accepts, and accepts one naming a field that
/// does not exist in `m2`'s input schema, so composition is not
/// functorial on value-level transforms.
///
/// The anchor coordinate was already conjugated through
/// `m1.vertex_remap`; this is the field coordinate's counterpart.
///
/// # Errors
///
/// Returns [`LensError::ComposeUnboundField`] when an `m2` expression
/// reads a field `m1` drops outright.
fn compose_field_transforms(
    m1: &CompiledMigration,
    m2: &CompiledMigration,
    src_schema: Option<&panproto_schema::Schema>,
) -> Result<HashMap<panproto_gat::Name, Vec<panproto_inst::wtype::FieldTransform>>, LensError> {
    use panproto_gat::Name;
    use panproto_inst::wtype::FieldTransform;

    let mut result = m1.field_transforms.clone();

    // Inject each m2 anchor's entries, conjugated into m1's source frame.
    let mut inject = |m1_anchor: &Name, entries: &[FieldTransform]| -> Result<(), LensError> {
        let renames = field_rename_inverse(m1, m1_anchor);
        let unavailable = unavailable_fields(m1, m1_anchor, src_schema);
        let conjugated = conjugate_transforms(entries, &renames, &unavailable, m1_anchor)?;
        result
            .entry(m1_anchor.clone())
            .or_default()
            .extend(conjugated);
        Ok(())
    };

    for (m2_anchor, m2_entries) in &m2.field_transforms {
        let mut found = false;
        for (m1_src, m1_tgt) in &m1.vertex_remap {
            if m1_tgt == m2_anchor {
                inject(m1_src, m2_entries)?;
                found = true;
            }
        }
        if !found && unchanged_by_m1(m1, m2_anchor) {
            inject(m2_anchor, m2_entries)?;
        }
    }

    Ok(result)
}

/// The child-scalar renames `m1` performs on `anchor`, inverted: a map
/// from the field name visible in `m1`'s **output** back to the name it
/// has in `m1`'s **source**.
///
/// Only schema edge renames contribute. An edge rename
/// is applied via `edge_remap` when output arcs are materialized, and
/// never reaches the `child_scalars` map that
/// `collect_scalar_child_values` projects into the expression
/// environment, so an `m2` expression naming the new edge name finds
/// nothing under it in the merged batch. That is the frame mismatch this
/// map repairs.
///
/// `FieldTransform::RenameField` deliberately does *not* contribute.
/// It rewrites the `extra_fields` key in place, and `m1`'s entries run
/// ahead of `m2`'s in the merged batch, so by the time `m2`'s expression
/// evaluates the field really is under its new name. Conjugating those
/// as well would rewrite a correct reference back to a name that no
/// longer exists. This asymmetry between the two rename routes is the
/// whole of the defect: one is reflected in the value layer's
/// environment and the other is not.
///
/// Renames nested under a `PathTransform` and renames inside a `Case`
/// branch are not included: the first affects nested keys rather than
/// the top-level bindings `free_vars` reports, and the second is
/// value-dependent, so no static rewrite is sound for it.
///
/// `edge_remap`'s entries are simultaneous, one per source edge, so the
/// map is read off directly rather than folded. A chain of renames across
/// several migrations has already been collapsed into a single entry by
/// `compose_relabeling` before it reaches here, which is what makes
/// `a → b` then `b → c` resolve `c` to `a` in one step.
fn field_rename_inverse(
    m1: &CompiledMigration,
    anchor: &panproto_gat::Name,
) -> HashMap<String, String> {
    let mut origin: HashMap<String, String> = HashMap::new();

    for (src_edge, tgt_edge) in &m1.edge_remap {
        if &src_edge.src != anchor {
            continue;
        }
        let old_field = edge_field_name(src_edge);
        let new_field = edge_field_name(tgt_edge);
        if old_field == new_field {
            continue;
        }
        origin.insert(new_field, old_field);
    }

    origin
}

/// The environment key a child edge contributes, matching
/// `collect_scalar_child_values`: the edge's name when it has one, else
/// its target vertex.
fn edge_field_name(edge: &Edge) -> String {
    edge.name.as_deref().unwrap_or(&edge.tgt).to_string()
}

/// Names that do not exist in `m1`'s output frame at `anchor`, so an `m2`
/// expression reading one is written against the wrong frame.
///
/// Two ways a name goes missing:
///
/// * `DropField` removes an `extra_fields` key outright.
/// * A schema edge rename takes a child-scalar name away, leaving nothing
///   bound under the old name in `m1`'s output.
///
/// Both are then forgiven when the name is put back: by another surviving
/// child edge carrying it in the output frame, or by an `m1` transform
/// writing it (`AddField`, `ComputeField`, `ApplyExpr`, or a `RenameField`
/// target).
///
/// `RenameField`'s *old* key is not reported. It ceases to exist in
/// `m1`'s output, but `m1` and `m2` share one evaluation batch, so a
/// stale reference resolves against whatever `m1` left in `extra_fields`
/// rather than failing; reporting it would reject compositions that run
/// correctly today. `KeepFields` is likewise excluded: it filters
/// `extra_fields` only, so a name it omits may still be bound as a child
/// scalar.
///
/// Returns empty without `src_schema`, since a dropped key cannot then be
/// told apart from a child edge of the same name and a false report would
/// reject a sound composition.
fn unavailable_fields(
    m1: &CompiledMigration,
    anchor: &panproto_gat::Name,
    src_schema: Option<&panproto_schema::Schema>,
) -> std::collections::HashSet<String> {
    use panproto_inst::wtype::FieldTransform;

    let mut unavailable = std::collections::HashSet::new();
    let Some(schema) = src_schema else {
        return unavailable;
    };

    // Child-scalar names an edge rename takes away.
    let outgoing = schema.outgoing.get(anchor);
    let mut output_child_names = std::collections::HashSet::new();
    if let Some(edges) = outgoing {
        for edge in edges {
            if !m1.surviving_edges.contains(edge) {
                continue;
            }
            let out_edge = m1.edge_remap.get(edge).unwrap_or(edge);
            output_child_names.insert(edge_field_name(out_edge));
        }
        for edge in edges {
            if !m1.surviving_edges.contains(edge) {
                continue;
            }
            let source_name = edge_field_name(edge);
            if !output_child_names.contains(&source_name) {
                unavailable.insert(source_name);
            }
        }
    }

    if let Some(transforms) = m1.field_transforms.get(anchor) {
        for transform in transforms {
            match transform {
                FieldTransform::DropField { key } => {
                    if !output_child_names.contains(key) {
                        unavailable.insert(key.clone());
                    }
                }
                FieldTransform::AddField { key, .. }
                | FieldTransform::ComputeField {
                    target_key: key, ..
                }
                | FieldTransform::ApplyExpr { key, .. } => {
                    unavailable.remove(key);
                }
                FieldTransform::RenameField { new_key, .. } => {
                    unavailable.remove(new_key);
                }
                _ => {}
            }
        }
    }

    unavailable
}

/// Rewrite each transform's expressions from `m1`'s output frame into
/// `m1`'s source frame.
fn conjugate_transforms(
    entries: &[panproto_inst::wtype::FieldTransform],
    renames: &HashMap<String, String>,
    unavailable: &std::collections::HashSet<String>,
    anchor: &panproto_gat::Name,
) -> Result<Vec<panproto_inst::wtype::FieldTransform>, LensError> {
    use panproto_inst::wtype::FieldTransform;

    // Keys this m2 list writes itself. A read of one of these resolves at
    // evaluation time even if m1 dropped the same name earlier, because
    // m2's own transform runs first in the merged batch.
    let mut produced: std::collections::HashSet<String> = std::collections::HashSet::new();

    let mut out = Vec::with_capacity(entries.len());
    for entry in entries {
        let mut next = entry.clone();
        conjugate_transform_exprs(&mut next, renames, unavailable, &produced, anchor)?;
        match entry {
            FieldTransform::ComputeField { target_key, .. } => {
                produced.insert(target_key.clone());
            }
            FieldTransform::AddField { key, .. } | FieldTransform::ApplyExpr { key, .. } => {
                produced.insert(key.clone());
            }
            FieldTransform::RenameField { new_key, .. } => {
                produced.insert(new_key.clone());
            }
            _ => {}
        }
        out.push(next);
    }
    Ok(out)
}

/// Rewrite the expressions carried by one transform, recursing through
/// `PathTransform` and `Case`.
fn conjugate_transform_exprs(
    transform: &mut panproto_inst::wtype::FieldTransform,
    renames: &HashMap<String, String>,
    unavailable: &std::collections::HashSet<String>,
    produced: &std::collections::HashSet<String>,
    anchor: &panproto_gat::Name,
) -> Result<(), LensError> {
    use panproto_inst::wtype::FieldTransform;

    let check = |expr: &panproto_expr::Expr| -> Result<(), LensError> {
        for var in panproto_expr::free_vars(expr) {
            if unavailable.contains(var.as_ref()) && !produced.contains(var.as_ref()) {
                return Err(LensError::ComposeUnboundField {
                    anchor: anchor.to_string(),
                    field: var.to_string(),
                });
            }
        }
        Ok(())
    };
    let rewrite = |expr: &mut panproto_expr::Expr| -> Result<(), LensError> {
        check(expr)?;
        *expr = rename_free_vars(expr, renames);
        Ok(())
    };

    // `ApplyExpr` reads and writes one and the same key. When `m1` renamed
    // that key the two coordinates part company: the read has to happen
    // under `m1`'s source name, because that is what the child-scalar map
    // is keyed by, while the write has to land on the output name, because
    // that is what the remapped arc will be emitted under. Conjugating the
    // key would move both and leave the composite emitting the original
    // value under the new edge name beside the transformed value under the
    // old one; leaving it alone finds nothing to read and silently does
    // nothing. Neither is `ApplyExpr`-shaped, so it becomes the transform
    // that can hold the two coordinates apart: a `ComputeField` writing
    // the output name from an expression reading the source name. The
    // expression's free variables lie within the read key, which the
    // fiber environment binds, and `inverse` and `coercion_class` carry
    // over unchanged. `ApplyExpr` skips a key it cannot find whereas
    // `ComputeField` reports it, which is the direction this codebase
    // already takes for a transform that cannot evaluate.
    if let FieldTransform::ApplyExpr {
        key,
        expr,
        inverse,
        coercion_class,
    } = &*transform
        && renames.contains_key(key.as_str())
    {
        check(expr)?;
        let replacement = FieldTransform::ComputeField {
            target_key: key.clone(),
            expr: rename_free_vars(expr, renames),
            inverse: inverse.as_ref().map(|i| rename_free_vars(i, renames)),
            coercion_class: *coercion_class,
        };
        *transform = replacement;
        return Ok(());
    }

    match transform {
        FieldTransform::ComputeField { expr, inverse, .. } => {
            rewrite(expr)?;
            if let Some(inv) = inverse {
                *inv = rename_free_vars(inv, renames);
            }
        }
        FieldTransform::ApplyExpr { expr, inverse, .. } => {
            // The renamed-key case is rewritten to a `ComputeField` above;
            // reaching here means the key is a fixed point, so only the
            // body needs conjugating.
            rewrite(expr)?;
            if let Some(inv) = inverse {
                *inv = rename_free_vars(inv, renames);
            }
        }
        FieldTransform::Case { branches } => {
            for branch in branches {
                rewrite(&mut branch.predicate)?;
                for inner in &mut branch.transforms {
                    conjugate_transform_exprs(inner, renames, unavailable, produced, anchor)?;
                }
            }
        }
        FieldTransform::PathTransform { inner, .. } => {
            // A nested transform reads the nested map, not the top-level
            // bindings the renames describe, so only its own recursion
            // applies.
            conjugate_transform_exprs(inner, renames, unavailable, produced, anchor)?;
        }
        FieldTransform::RenameField { old_key, .. } => {
            // The key this renames away is named in m1's output frame.
            if let Some(source) = renames.get(old_key.as_str()) {
                old_key.clone_from(source);
            }
        }
        FieldTransform::DropField { key } => {
            if let Some(source) = renames.get(key.as_str()) {
                key.clone_from(source);
            }
        }
        FieldTransform::KeepFields { keys } => {
            for key in keys {
                if let Some(source) = renames.get(key.as_str()) {
                    key.clone_from(source);
                }
            }
        }
        FieldTransform::AddField { .. } | FieldTransform::MapReferences { .. } => {}
    }
    Ok(())
}

/// Simultaneously rename an expression's free variables.
///
/// `panproto_expr::substitute` replaces one name at a time, so applying a
/// map sequentially would compose the replacements: a swap `{a → b,
/// b → a}` would send every `a` to `b` and then straight back to `a`.
/// Routing through fresh placeholders makes the renaming simultaneous,
/// which is what conjugation into another frame requires.
///
/// Both the bare key and its `attrs.`-qualified alias are rewritten,
/// since `build_env_from_extra_fields` binds a field under both.
fn rename_free_vars(expr: &panproto_expr::Expr, renames: &HashMap<String, String>) -> Expr {
    use std::sync::Arc;

    if renames.is_empty() {
        return expr.clone();
    }

    let free = panproto_expr::free_vars(expr);
    // Pair every applicable rename with a placeholder that appears
    // nowhere in the expression or in the rename map.
    let mut pairs: Vec<(String, String, String)> = Vec::new();
    for (index, (visible, source)) in renames.iter().enumerate() {
        for (from, to) in [
            (visible.clone(), source.clone()),
            (format!("attrs.{visible}"), format!("attrs.{source}")),
        ] {
            if !free.iter().any(|v| v.as_ref() == from) {
                continue;
            }
            let mut placeholder = format!("\u{0}compose{index}\u{0}{from}");
            while free.iter().any(|v| v.as_ref() == placeholder.as_str())
                || renames.contains_key(&placeholder)
            {
                placeholder.push('\u{0}');
            }
            pairs.push((from, placeholder, to));
        }
    }

    let mut out = expr.clone();
    for (from, placeholder, _) in &pairs {
        out = panproto_expr::substitute(&out, from, &Expr::Var(Arc::from(placeholder.as_str())));
    }
    for (_, placeholder, to) in &pairs {
        out = panproto_expr::substitute(&out, placeholder, &Expr::Var(Arc::from(to.as_str())));
    }
    out
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
