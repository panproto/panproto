//! Asymmetric lens operations: `get` and `put` with complement tracking.
//!
//! The `get` direction runs the restrict pipeline (forward migration) while
//! capturing everything that was discarded into a [`Complement`]. The `put`
//! direction restores the original source instance from a (possibly modified)
//! view plus the complement.

use std::collections::{HashMap, HashSet};

use panproto_gat::Name;
use panproto_inst::{Fan, Node, WInstance, wtype_restrict};
use panproto_schema::Edge;

use crate::Lens;
use crate::error::LensError;

/// The complement: data discarded by `get`, needed by `put` to restore the
/// original source instance.
///
/// The canonical definition lives in [`panproto_inst::Complement`] and is
/// shared with the restrict pipeline; it is re-exported here because the
/// asymmetric lens is its primary producer and consumer. Composition of
/// complements as a partial monoid is provided by [`ComplementCompose`],
/// which lives in this crate because its conflict semantics are expressed in
/// terms of [`LensError`].
pub use panproto_inst::Complement;

/// Partial-commutative-monoid composition of [`Complement`]s.
///
/// This is an extension trait (rather than inherent methods) because the
/// merge's conflict signalling is expressed with [`LensError`], which lives
/// in `panproto-lens`, while the [`Complement`] type itself lives in
/// `panproto-inst`.
pub trait ComplementCompose: Sized {
    /// Compose two complements as a *partial commutative monoid* (in the
    /// sense of separation-algebra / Clog-style algebras): the merge is
    /// defined exactly when the two complements agree on every shared key,
    /// in which case the result records everything lost by both.
    ///
    /// [`Complement::empty`] is a two-sided identity. On the domain of
    /// definition (pairs satisfying [`is_compatible`](Self::is_compatible)),
    /// composition is associative and commutative; this is verified by the
    /// `complement_partial_monoid_*` proptests.
    ///
    /// # Errors
    ///
    /// Returns [`LensError::ComplementConflict`] when both operands carry
    /// distinct entries on the same key (the partial-monoid's
    /// disjointness/agreement condition fails). Returns
    /// [`LensError::ComplementFingerprintMismatch`] when both fingerprints
    /// are non-zero and differ; complements rooted at different source
    /// schemas cannot be combined.
    fn compose(&self, other: &Self) -> Result<Self, LensError>;

    /// Returns `true` exactly when [`compose`](Self::compose) would succeed.
    ///
    /// Equivalent to running `compose` and discarding the result, but avoids
    /// the allocation when only the predicate is needed.
    fn is_compatible(&self, other: &Self) -> bool;
}

impl ComplementCompose for Complement {
    fn compose(&self, other: &Self) -> Result<Self, LensError> {
        let source_fingerprint =
            compose_fingerprint(self.source_fingerprint, other.source_fingerprint)?;

        let mut dropped_nodes = self.dropped_nodes.clone();
        merge_keyed_with_eq(
            &mut dropped_nodes,
            &other.dropped_nodes,
            "dropped_nodes",
            node_equiv,
        )?;

        let mut dropped_arcs = self.dropped_arcs.clone();
        merge_vec_dedup(&mut dropped_arcs, &other.dropped_arcs);

        let mut dropped_fans = self.dropped_fans.clone();
        merge_vec_dedup(&mut dropped_fans, &other.dropped_fans);

        let mut contraction_choices = self.contraction_choices.clone();
        merge_keyed_with_eq(
            &mut contraction_choices,
            &other.contraction_choices,
            "contraction_choices",
            PartialEq::eq,
        )?;

        let mut original_parent = self.original_parent.clone();
        merge_keyed_with_eq(
            &mut original_parent,
            &other.original_parent,
            "original_parent",
            PartialEq::eq,
        )?;

        let mut original_extra_fields = self.original_extra_fields.clone();
        merge_keyed_with_eq(
            &mut original_extra_fields,
            &other.original_extra_fields,
            "original_extra_fields",
            extra_fields_equiv,
        )?;

        let mut arc_edges = self.arc_edges.clone();
        merge_keyed_with_eq(&mut arc_edges, &other.arc_edges, "arc_edges", PartialEq::eq)?;

        let mut original_values = self.original_values.clone();
        merge_keyed_with_eq(
            &mut original_values,
            &other.original_values,
            "original_values",
            |a, b| presence_equiv(a.as_ref(), b.as_ref()),
        )?;

        let mut synthesized_nodes = self.synthesized_nodes.clone();
        synthesized_nodes.extend(other.synthesized_nodes.iter().copied());

        let mut contracted_into = self.contracted_into.clone();
        merge_keyed_with_eq(
            &mut contracted_into,
            &other.contracted_into,
            "contracted_into",
            PartialEq::eq,
        )?;

        Ok(Self {
            dropped_nodes,
            dropped_arcs,
            dropped_fans,
            contraction_choices,
            original_parent,
            source_fingerprint,
            original_extra_fields,
            arc_edges,
            original_values,
            synthesized_nodes,
            contracted_into,
        })
    }

    fn is_compatible(&self, other: &Self) -> bool {
        self.clone().compose(other).is_ok()
    }
}

fn compose_fingerprint(left: u64, right: u64) -> Result<u64, LensError> {
    match (left, right) {
        (0, _) | (_, 0) => Ok(left.max(right)),
        (l, r) if l == r => Ok(l),
        (l, r) => Err(LensError::ComplementFingerprintMismatch { left: l, right: r }),
    }
}

fn merge_keyed_with_eq<K, V, F>(
    target: &mut HashMap<K, V>,
    source: &HashMap<K, V>,
    kind: &'static str,
    eq: F,
) -> Result<(), LensError>
where
    K: Eq + std::hash::Hash + Clone + std::fmt::Debug,
    V: Clone,
    F: Fn(&V, &V) -> bool,
{
    for (k, v) in source {
        match target.get(k) {
            None => {
                target.insert(k.clone(), v.clone());
            }
            Some(existing) if eq(existing, v) => {}
            Some(_) => {
                return Err(LensError::ComplementConflict {
                    kind,
                    key: format!("{k:?}"),
                });
            }
        }
    }
    Ok(())
}

fn merge_vec_dedup<T: Clone + PartialEq>(target: &mut Vec<T>, source: &[T]) {
    for item in source {
        if !target.contains(item) {
            target.push(item.clone());
        }
    }
}

/// Total structural equivalence on [`Node`].
///
/// `Node` does not derive `PartialEq` (its `Value` field carries
/// `f64`/`HashMap` content). We compare each field directly,
/// delegating to [`value_equiv`] for `Value`-bearing positions so that
/// `f64::NaN` compares equal to itself (bit-equality) and `HashMap`
/// values compare order-independently.
fn node_equiv(a: &Node, b: &Node) -> bool {
    a.id == b.id
        && a.anchor == b.anchor
        && a.discriminator == b.discriminator
        && a.position == b.position
        && a.shape == b.shape
        && presence_equiv(a.value.as_ref(), b.value.as_ref())
        && extra_fields_equiv(&a.extra_fields, &b.extra_fields)
        && extra_fields_equiv(&a.annotations, &b.annotations)
}

/// Total structural equivalence on a `HashMap<String, Value>`.
pub(crate) fn extra_fields_equiv(
    a: &HashMap<String, panproto_inst::value::Value>,
    b: &HashMap<String, panproto_inst::value::Value>,
) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter()
        .all(|(k, v)| b.get(k).is_some_and(|w| value_equiv(v, w)))
}

/// Total structural equivalence on `Option<FieldPresence>`.
pub(crate) fn presence_equiv(
    a: Option<&panproto_inst::value::FieldPresence>,
    b: Option<&panproto_inst::value::FieldPresence>,
) -> bool {
    use panproto_inst::value::FieldPresence;
    match (a, b) {
        (None, None)
        | (Some(FieldPresence::Absent), Some(FieldPresence::Absent))
        | (Some(FieldPresence::Null), Some(FieldPresence::Null)) => true,
        (Some(FieldPresence::Present(x)), Some(FieldPresence::Present(y))) => value_equiv(x, y),
        _ => false,
    }
}

/// IEEE-754 equality with NaN reflexive: `+0.0 == -0.0`, every NaN
/// equals every NaN. Used by [`value_equiv`] so that complement
/// composition treats a NaN-bearing node as self-equal (the derived
/// `PartialEq` on `f64` would say NaN ≠ NaN and falsely report a
/// conflict).
///
/// Implemented via `partial_cmp`: it returns `None` for NaN
/// arguments (caught explicitly above) and `Some(Ordering::Equal)`
/// for `+0.0 == -0.0` and every other IEEE-754-equal pair.
fn float_equiv(x: f64, y: f64) -> bool {
    if x.is_nan() && y.is_nan() {
        return true;
    }
    x.partial_cmp(&y) == Some(std::cmp::Ordering::Equal)
}

/// Total structural equivalence on [`Value`](panproto_inst::value::Value).
///
/// Differs from `Value`'s derived `PartialEq` in exactly one place:
/// `Float(NaN)` compares equal to `Float(NaN)`, so complement
/// composition does not spuriously reject a node against itself.
/// All other floats fall through to ordinary `==`, which means
/// `+0.0` and `-0.0` compare equal (matching IEEE-754 numeric
/// equality) — using bit-equality there would over-discriminate.
///
/// Distinct numeric variants (`Int(1)` vs `Float(1.0)`) remain
/// distinct: panproto preserves the source-format distinction by
/// design, and conflating them would lose round-trip fidelity.
///
/// `Unknown`/`Opaque` field-bag comparisons recurse, so NaN-equality
/// propagates through nesting.
pub(crate) fn value_equiv(
    a: &panproto_inst::value::Value,
    b: &panproto_inst::value::Value,
) -> bool {
    use panproto_inst::value::Value;
    match (a, b) {
        (Value::Float(x), Value::Float(y)) => float_equiv(*x, *y),
        (Value::List(xs), Value::List(ys)) => {
            xs.len() == ys.len() && xs.iter().zip(ys).all(|(x, y)| value_equiv(x, y))
        }
        (Value::Unknown(m1), Value::Unknown(m2)) => extra_fields_equiv(m1, m2),
        (
            Value::Opaque {
                type_: t1,
                fields: f1,
            },
            Value::Opaque {
                type_: t2,
                fields: f2,
            },
        ) => t1 == t2 && extra_fields_equiv(f1, f2),
        _ => a == b,
    }
}

/// Compute a fingerprint of a schema for complement provenance validation.
fn schema_fingerprint(schema: &panproto_schema::Schema) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    let mut vert_names: Vec<&str> = schema.vertices.keys().map(|n| &**n).collect();
    vert_names.sort_unstable();
    for v in &vert_names {
        v.hash(&mut hasher);
    }
    schema.edges.len().hash(&mut hasher);
    hasher.finish()
}

/// Forward lens direction: restrict the source instance to the target view
/// and capture the complement.
///
/// This runs `wtype_restrict` and then computes the set difference between
/// the source and result to populate the complement.
///
/// # Errors
///
/// Returns `LensError::Restrict` if the underlying restrict pipeline fails.
pub fn get(lens: &Lens, instance: &WInstance) -> Result<(WInstance, Complement), LensError> {
    let view = wtype_restrict(instance, &lens.src_schema, &lens.tgt_schema, &lens.compiled)?;

    // Compute complement: everything in source not in view
    let mut dropped_nodes = HashMap::new();
    for (&id, node) in &instance.nodes {
        if !view.nodes.contains_key(&id) {
            dropped_nodes.insert(id, node.clone());
        }
    }

    let mut dropped_arcs = Vec::new();
    for arc in &instance.arcs {
        let (parent, child, _) = arc;
        if !view.nodes.contains_key(parent) || !view.nodes.contains_key(child) {
            dropped_arcs.push(arc.clone());
        }
    }

    // Capture contraction choices: for each arc in the view that connects
    // nodes that were not directly connected in the source, record the
    // resolver decision.
    let mut contraction_choices = HashMap::new();
    for (parent, child, edge) in &view.arcs {
        // Check if this arc existed in the original
        let was_direct = instance
            .arcs
            .iter()
            .any(|(p, c, _)| p == parent && c == child);
        if !was_direct {
            contraction_choices.insert((*parent, *child), edge.clone());
        }
    }

    // Capture original parent mapping for all surviving nodes
    let mut original_parent = HashMap::new();
    for &id in view.nodes.keys() {
        if let Some(&parent) = instance.parent_map.get(&id) {
            original_parent.insert(id, parent);
        }
    }

    // Capture fans whose parent or any children were dropped
    let mut dropped_fans = Vec::new();
    for fan in &instance.fans {
        let parent_survives = view.nodes.contains_key(&fan.parent);
        let all_children_survive = fan
            .children
            .values()
            .all(|node_id| view.nodes.contains_key(node_id));
        if !parent_survives || !all_children_survive {
            dropped_fans.push(fan.clone());
        }
    }

    // Capture pre-transform extra_fields for nodes that had field_transforms
    let mut original_extra_fields = HashMap::new();
    for &id in view.nodes.keys() {
        if let Some(source_node) = instance.nodes.get(&id) {
            if lens
                .compiled
                .field_transforms
                .contains_key(&source_node.anchor)
            {
                original_extra_fields.insert(id, source_node.extra_fields.clone());
            }
        }
    }

    // Capture pre-coercion node.value for nodes that had __value__ field transforms
    let mut original_values = HashMap::new();
    for &id in view.nodes.keys() {
        if let Some(source_node) = instance.nodes.get(&id) {
            if lens
                .compiled
                .field_transforms
                .get(&source_node.anchor)
                .is_some_and(|ts| {
                    ts.iter().any(|t| {
                        matches!(t, panproto_inst::FieldTransform::ApplyExpr { key, .. } if key == "__value__")
                    })
                })
            {
                original_values.insert(id, source_node.value.clone());
            }
        }
    }

    // Record the exact edge for every arc in the source instance whose
    // parent and child both survive. This makes `put` deterministic when
    // the source schema has parallel edges between the same vertex pair.
    let mut arc_edges = HashMap::new();
    for (parent, child, edge) in &instance.arcs {
        if view.nodes.contains_key(parent) && view.nodes.contains_key(child) {
            arc_edges.insert((*parent, *child), edge.clone());
        }
    }

    let source_fingerprint = schema_fingerprint(&lens.src_schema);

    // Capture node ids that appear in the view but not in the source
    // instance. These were synthesized by `wtype_restrict` to satisfy a
    // nest-style `expansion_path` in the compiled migration (a direct
    // source arc expanded into a multi-hop target path). `put` drops
    // them when collapsing back to the source.
    let synthesized_nodes: std::collections::HashSet<u32> = view
        .nodes
        .keys()
        .copied()
        .filter(|id| !instance.nodes.contains_key(id))
        .collect();

    let complement = Complement {
        dropped_nodes,
        dropped_arcs,
        dropped_fans,
        contraction_choices,
        original_parent,
        source_fingerprint,
        original_extra_fields,
        arc_edges,
        original_values,
        synthesized_nodes,
        // The lens `get` path records no ancestor-contraction fibre; that
        // field is populated only by the restrict pipeline.
        contracted_into: HashMap::new(),
    };

    Ok((view, complement))
}

/// Backward lens direction: restore a source instance from a (possibly
/// modified) view and the complement.
///
/// The complement provides the dropped nodes and arcs; the view provides
/// the (potentially modified) surviving data. Together they reconstruct
/// the full source instance.
///
/// # Errors
///
/// Returns `LensError::ComplementMismatch` if the complement is inconsistent
/// with the view.
pub fn put(lens: &Lens, view: &WInstance, complement: &Complement) -> Result<WInstance, LensError> {
    // Validate complement provenance: the complement must have been produced
    // from the same source schema.
    if complement.source_fingerprint != 0 {
        let expected = schema_fingerprint(&lens.src_schema);
        if complement.source_fingerprint != expected {
            return Err(LensError::ComplementMismatch {
                detail: format!(
                    "source fingerprint mismatch: complement has {}, lens expects {}",
                    complement.source_fingerprint, expected
                ),
            });
        }
    }

    // Start with all nodes from the view (un-remap anchors back to source).
    // Skip nodes that were synthesized during forward eval by the
    // nest-style `expansion_path` mechanism: they exist only in the view,
    // not in the source, so collapsing them back means dropping them.
    let mut nodes = HashMap::new();
    let reverse_remap = build_reverse_remap(&lens.compiled.vertex_remap);

    for (&id, node) in &view.nodes {
        if complement.synthesized_nodes.contains(&id) {
            continue;
        }
        let mut restored_node = node.clone();
        if let Some(original_anchor) = reverse_remap.get(&node.anchor) {
            restored_node.anchor.clone_from(original_anchor);
        }
        // Restore original extra_fields.
        //
        // When a complement snapshot is present, start from the snapshot
        // (so fields dropped by the forward pass and lossy computed
        // fields are restored exactly), then propagate any view edits
        // back through the inverse of each forward transform. This
        // preserves user edits in the view while still honoring the
        // snapshot as the source of truth for information that the
        // forward pass threw away.
        //
        // When there is no snapshot (externally constructed complements
        // or composed lenses that did not propagate one), fall back to
        // applying inverse transforms to the view's fields directly.
        let src_anchor = reverse_remap.get(&node.anchor).unwrap_or(&node.anchor);
        let transforms = lens.compiled.field_transforms.get(src_anchor);
        if let Some(original_fields) = complement.original_extra_fields.get(&id) {
            let view_fields = restored_node.extra_fields.clone();
            restored_node.extra_fields.clone_from(original_fields);
            if let Some(transforms) = transforms {
                propagate_view_edits_through_inverse(&mut restored_node, &view_fields, transforms);
            }
        } else if let Some(transforms) = transforms {
            apply_inverse_transforms(&mut restored_node, transforms);
        }
        // Restore original node.value for __value__ coercions
        if let Some(original_val) = complement.original_values.get(&id) {
            restored_node.value.clone_from(original_val);
        }
        nodes.insert(id, restored_node);
    }

    // Re-insert dropped nodes from complement
    for (&id, node) in &complement.dropped_nodes {
        nodes.insert(id, node.clone());
    }

    // Rebuild arcs: start with original parent relationships for view nodes,
    // then add back dropped arcs.
    //
    // A given child can be reached through two distinct restoration paths:
    // its `original_parent` entry (restored below for surviving view nodes)
    // and a `dropped_arcs` entry (restored when a sort containing that arc
    // was dropped). When a combinator both drops a sort and keeps that
    // sort's child in the view (e.g. `hoist_field`, which drops the
    // intermediate `profile` sort while hoisting its `name` child), the same
    // `(parent, child)` arc is produced by both paths. Emitting it twice
    // gives the parent two identical outgoing arcs, which the JSON encoder
    // reads as a repeated-edge list signal and serializes as a duplicated
    // `Value::List` instead of the original record. We therefore dedupe by
    // `(parent, child)`: a W-type instance has at most one arc per
    // parent/child pair, and genuine list nodes are distinguished by having
    // multiple arcs to *distinct* children, which this key preserves.
    let mut arcs = Vec::new();
    let mut seen_pairs: HashSet<(u32, u32)> = HashSet::new();

    // For view nodes, use the original parent mapping to restore arcs
    for (&child_id, &original_parent) in &complement.original_parent {
        if !nodes.contains_key(&child_id) || child_id == view.root {
            continue;
        }
        // Find the original arc for this parent-child pair, consulting
        // contraction_choices for disambiguation when multiple edges exist.
        if let Some(arc) = find_original_arc(
            &lens.src_schema,
            &nodes,
            original_parent,
            child_id,
            &complement.contraction_choices,
            &complement.arc_edges,
        ) && seen_pairs.insert((arc.0, arc.1))
        {
            arcs.push(arc);
        }
    }

    // Add dropped arcs back (they connect dropped nodes)
    for arc in &complement.dropped_arcs {
        let (parent, child, _) = arc;
        if nodes.contains_key(parent)
            && nodes.contains_key(child)
            && seen_pairs.insert((*parent, *child))
        {
            arcs.push(arc.clone());
        }
    }

    // Reconstruct fans: start with the view's fans (un-remapping vertex
    // references), then re-insert dropped fans from the complement whose
    // parent and children are all present in the restored node set.
    let mut fans: Vec<Fan> = view
        .fans
        .iter()
        .map(|fan| {
            let mut restored_fan = fan.clone();
            // Un-remap the hyper-edge ID if needed
            if let Some(original_he) = reverse_remap.get(fan.hyper_edge_id.as_str()) {
                restored_fan.hyper_edge_id = original_he.to_string();
            }
            restored_fan
        })
        .collect();

    // Re-insert dropped fans whose nodes are all present after restoration
    for fan in &complement.dropped_fans {
        let parent_present = nodes.contains_key(&fan.parent);
        let all_children_present = fan
            .children
            .values()
            .all(|node_id| nodes.contains_key(node_id));
        if parent_present && all_children_present {
            fans.push(fan.clone());
        }
    }

    let schema_root = reverse_remap
        .get(&view.schema_root)
        .cloned()
        .unwrap_or_else(|| view.schema_root.clone());
    // schema_root is Name, which WInstance::new accepts via Into<Name>

    Ok(WInstance::new(nodes, arcs, fans, view.root, schema_root))
}

/// Propagate user edits from a view's `extra_fields` back into a node
/// pre-populated from the complement snapshot.
///
/// The snapshot already contains the pre-transform values, so we only
/// need to overwrite entries for which the view's (possibly edited)
/// value differs. For each forward transform whose output key is still
/// meaningful (`RenameField`, `ApplyExpr`, `ComputeField`), we read the
/// view's value at the forward-target key and, when the transform has a
/// usable inverse, write the inverted value to the corresponding
/// source-side key. For `AddField` we simply drop the added key because
/// the source never had it.
///
/// Transforms are processed in reverse of their forward order so that
/// when multiple transforms chain (e.g. rename-then-compute) the later
/// forward transform is undone first.
fn propagate_view_edits_through_inverse(
    node: &mut Node,
    view_fields: &std::collections::HashMap<String, panproto_inst::value::Value>,
    transforms: &[panproto_inst::FieldTransform],
) {
    use panproto_inst::FieldTransform;

    // Mutable working copy so ApplyExpr lookups see prior rewrites.
    let mut working: std::collections::HashMap<String, panproto_inst::value::Value> =
        view_fields.clone();

    for transform in transforms.iter().rev() {
        match transform {
            FieldTransform::RenameField { old_key, new_key } => {
                if let Some(val) = working.remove(new_key) {
                    node.extra_fields.insert(old_key.clone(), val.clone());
                    working.insert(old_key.clone(), val);
                }
            }
            FieldTransform::ApplyExpr {
                key,
                inverse: Some(inv_expr),
                ..
            } => {
                if key == "__value__" {
                    // Handled via complement.original_values elsewhere.
                    continue;
                }
                if working.contains_key(key) {
                    let env = panproto_inst::build_env_from_extra_fields(&working);
                    let config = panproto_expr::EvalConfig::default();
                    if let Ok(result) = panproto_expr::eval(inv_expr, &env, &config) {
                        let inverted = panproto_inst::expr_literal_to_value(&result);
                        node.extra_fields.insert(key.clone(), inverted.clone());
                        working.insert(key.clone(), inverted);
                    }
                }
            }
            FieldTransform::ComputeField {
                target_key,
                inverse: Some(inv_expr),
                ..
            } if working.contains_key(target_key) => {
                let env = panproto_inst::build_env_from_extra_fields(&working);
                let config = panproto_expr::EvalConfig::default();
                if let Ok(result) = panproto_expr::eval(inv_expr, &env, &config) {
                    let inverted = panproto_inst::expr_literal_to_value(&result);
                    node.extra_fields.insert(target_key.clone(), inverted);
                }
            }
            FieldTransform::AddField { key, .. } => {
                // Snapshot has no entry for the added key; make sure
                // nothing else slipped it in.
                node.extra_fields.remove(key);
            }
            FieldTransform::PathTransform { path, inner } if path.is_empty() => {
                propagate_view_edits_through_inverse(
                    node,
                    view_fields,
                    std::slice::from_ref(inner),
                );
            }
            // DropField / KeepFields / Case / MapReferences / ComputeField
            // without inverse / ApplyExpr without inverse: the snapshot
            // is authoritative, nothing to propagate.
            _ => {}
        }
    }
}

/// Apply inverse field transforms to a node, undoing the forward coercion.
///
/// For each `ApplyExpr` or `ComputeField` with an inverse expression, evaluate
/// the inverse on the node's current value to recover the pre-coercion value.
/// Transforms without inverses are skipped (the view's value is kept as-is).
///
/// This is a fallback path used when the complement does not contain a
/// snapshot of the original `extra_fields`. The primary `put` path restores
/// from `complement.original_extra_fields`, which is always captured by
/// `get`. This function is only reached for externally constructed
/// complements or composed lenses where snapshots were not propagated.
fn apply_inverse_transforms(node: &mut Node, transforms: &[panproto_inst::FieldTransform]) {
    use panproto_inst::FieldTransform;

    // Apply in reverse order (undo last transform first).
    for transform in transforms.iter().rev() {
        match transform {
            FieldTransform::ApplyExpr {
                key,
                inverse: Some(inv_expr),
                ..
            } => {
                // Handle "__value__" specially: target node.value, not extra_fields.
                if key == "__value__" {
                    if let Some(panproto_inst::value::FieldPresence::Present(val)) = &node.value {
                        let input = panproto_inst::value_to_expr_literal(val);
                        let env = panproto_expr::Env::new()
                            .extend(std::sync::Arc::from("v"), input.clone())
                            .extend(std::sync::Arc::from("__value__"), input);
                        let config = panproto_expr::EvalConfig::default();
                        if let Ok(result) = panproto_expr::eval(inv_expr, &env, &config) {
                            node.value = Some(panproto_inst::value::FieldPresence::Present(
                                panproto_inst::expr_literal_to_value(&result),
                            ));
                        }
                    }
                } else if node.extra_fields.contains_key(key) {
                    let env = panproto_inst::build_env_from_extra_fields(&node.extra_fields);
                    let config = panproto_expr::EvalConfig::default();
                    if let Ok(result) = panproto_expr::eval(inv_expr, &env, &config) {
                        node.extra_fields
                            .insert(key.clone(), panproto_inst::expr_literal_to_value(&result));
                    }
                }
            }
            FieldTransform::ComputeField {
                target_key,
                inverse: Some(inv_expr),
                ..
            } if node.extra_fields.contains_key(target_key) => {
                let env = panproto_inst::build_env_from_extra_fields(&node.extra_fields);
                let config = panproto_expr::EvalConfig::default();
                if let Ok(result) = panproto_expr::eval(inv_expr, &env, &config) {
                    node.extra_fields.insert(
                        target_key.clone(),
                        panproto_inst::expr_literal_to_value(&result),
                    );
                }
            }
            FieldTransform::RenameField { old_key, new_key } => {
                // Reverse: rename new_key back to old_key.
                if let Some(val) = node.extra_fields.remove(new_key) {
                    node.extra_fields.insert(old_key.clone(), val);
                }
            }
            FieldTransform::AddField { key, .. } => {
                // Reverse of adding: remove the field.
                node.extra_fields.remove(key);
            }
            FieldTransform::PathTransform { path, inner } if path.is_empty() => {
                // Recurse into nested structure. Non-empty paths into
                // nested Value structures are not inverted (would
                // require traversing Value::Unknown maps).
                apply_inverse_transforms(node, std::slice::from_ref(inner));
            }
            // DropField, KeepFields, Case, MapReferences: data is lost, cannot invert.
            _ => {}
        }
    }
}

/// Build a reverse mapping from target vertex IDs back to source vertex IDs.
fn build_reverse_remap(forward: &HashMap<Name, Name>) -> HashMap<Name, Name> {
    forward
        .iter()
        .map(|(k, v)| (v.clone(), k.clone()))
        .collect()
}

/// Find the original arc between a parent and child in the source schema.
///
/// Consults `arc_edges` (exact edge recorded during `get`) first, then
/// `contraction_choices`, then falls back to the source schema. When the
/// complement was produced by this module's `get`, `arc_edges` will always
/// contain the exact edge, making this function deterministic even when
/// the source schema has parallel edges.
fn find_original_arc(
    src_schema: &panproto_schema::Schema,
    nodes: &HashMap<u32, Node>,
    parent_id: u32,
    child_id: u32,
    contraction_choices: &HashMap<(u32, u32), Edge>,
    arc_edges: &HashMap<(u32, u32), Edge>,
) -> Option<(u32, u32, Edge)> {
    // Exact edge recorded during get: deterministic.
    if let Some(edge) = arc_edges.get(&(parent_id, child_id)) {
        return Some((parent_id, child_id, edge.clone()));
    }

    // Contraction choice: edges created by ancestor contraction.
    if let Some(edge) = contraction_choices.get(&(parent_id, child_id)) {
        return Some((parent_id, child_id, edge.clone()));
    }

    // Fallback: look up in the source schema (backward compat for old complements).
    let parent_node = nodes.get(&parent_id)?;
    let child_node = nodes.get(&child_id)?;

    let edges = src_schema.edges_between(&parent_node.anchor, &child_node.anchor);
    if edges.len() == 1 {
        Some((parent_id, child_id, edges[0].clone()))
    } else {
        edges.first().map(|e| (parent_id, child_id, e.clone()))
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::tests::{identity_lens, three_node_instance, three_node_schema};

    #[test]
    fn get_with_identity_lens_produces_empty_complement() {
        let schema = three_node_schema();
        let lens = identity_lens(&schema);
        let instance = three_node_instance();

        let (view, complement) =
            get(&lens, &instance).unwrap_or_else(|e| panic!("get failed: {e}"));
        assert_eq!(view.node_count(), instance.node_count());
        assert!(
            complement.dropped_nodes.is_empty(),
            "no nodes should be dropped by identity lens"
        );
    }

    #[test]
    fn get_then_put_round_trips_identity() {
        let schema = three_node_schema();
        let lens = identity_lens(&schema);
        let instance = three_node_instance();

        let (view, complement) =
            get(&lens, &instance).unwrap_or_else(|e| panic!("get failed: {e}"));
        let restored = put(&lens, &view, &complement).unwrap_or_else(|e| panic!("put failed: {e}"));

        assert_eq!(
            restored.node_count(),
            instance.node_count(),
            "restored should have same node count"
        );
        assert_eq!(restored.root, instance.root, "restored root should match");
    }

    #[test]
    fn complement_is_empty_for_identity() {
        let complement = Complement::empty();
        assert!(complement.is_empty());
    }

    /// Partial-monoid laws: identity, idempotence, associativity, and
    /// commutativity on the domain of definition.
    mod partial_monoid {
        use super::*;
        use crate::asymmetric::ComplementCompose;
        use panproto_inst::Node as InstNode;
        use panproto_schema::Edge;

        fn mk_node(id: u32, anchor: &str) -> InstNode {
            InstNode::new(id, anchor)
        }

        fn mk_edge(src: &str, tgt: &str, kind: &str) -> Edge {
            Edge {
                src: src.into(),
                tgt: tgt.into(),
                kind: kind.into(),
                name: None,
            }
        }

        fn singleton_dropped_node(id: u32, anchor: &str) -> Complement {
            let mut c = Complement::empty();
            c.dropped_nodes.insert(id, mk_node(id, anchor));
            c
        }

        fn singleton_arc_edge(parent: u32, child: u32, kind: &str) -> Complement {
            let mut c = Complement::empty();
            c.arc_edges.insert((parent, child), mk_edge("a", "b", kind));
            c
        }

        #[test]
        fn empty_is_left_identity() {
            let c = singleton_dropped_node(1, "x");
            let composed = Complement::empty().compose(&c).expect("compatible");
            assert_eq!(composed.dropped_nodes.len(), 1);
            assert!(composed.dropped_nodes.contains_key(&1));
        }

        #[test]
        fn empty_is_right_identity() {
            let c = singleton_dropped_node(1, "x");
            let composed = c.compose(&Complement::empty()).expect("compatible");
            assert_eq!(composed.dropped_nodes.len(), 1);
        }

        #[test]
        fn idempotent_on_equal_entries() {
            let c = singleton_dropped_node(7, "v7");
            let composed = c.compose(&c).expect("equal entries idempotent");
            assert_eq!(composed.dropped_nodes.len(), 1);
        }

        #[test]
        fn rejects_distinct_entries_on_same_key() {
            let a = singleton_dropped_node(3, "left");
            let b = singleton_dropped_node(3, "right");
            assert!(matches!(
                a.compose(&b),
                Err(LensError::ComplementConflict {
                    kind: "dropped_nodes",
                    ..
                })
            ));
            assert!(!a.is_compatible(&b));
        }

        #[test]
        fn rejects_cross_fingerprint_when_both_set() {
            let mut a = Complement::empty();
            a.source_fingerprint = 0xAAAA;
            let mut b = Complement::empty();
            b.source_fingerprint = 0xBBBB;
            assert!(matches!(
                a.compose(&b),
                Err(LensError::ComplementFingerprintMismatch { .. })
            ));
        }

        #[test]
        fn propagates_fingerprint_when_only_one_set() {
            let mut a = Complement::empty();
            a.source_fingerprint = 0xC0DE;
            let b = Complement::empty();
            let r = a.compose(&b).expect("compatible");
            assert_eq!(r.source_fingerprint, 0xC0DE);
            let r2 = b.compose(&a).expect("compatible");
            assert_eq!(r2.source_fingerprint, 0xC0DE);
        }

        #[test]
        fn associative_on_disjoint_keys() {
            let a = singleton_dropped_node(1, "v1");
            let b = singleton_dropped_node(2, "v2");
            let c = singleton_arc_edge(3, 4, "prop");

            let left = a
                .compose(&b)
                .expect("ab compatible")
                .compose(&c)
                .expect("(ab)c compatible");
            let right = a
                .compose(&b.compose(&c).expect("bc compatible"))
                .expect("a(bc) compatible");
            assert_eq!(left.dropped_nodes.len(), right.dropped_nodes.len());
            assert_eq!(left.arc_edges.len(), right.arc_edges.len());
            for (k, v) in &left.dropped_nodes {
                let other = right.dropped_nodes.get(k).expect("present on right");
                assert!(node_equiv(v, other));
            }
            for (k, v) in &left.arc_edges {
                assert_eq!(right.arc_edges.get(k), Some(v));
            }
        }

        #[test]
        fn commutative_on_disjoint_keys() {
            let a = singleton_dropped_node(10, "x");
            let b = singleton_dropped_node(20, "y");
            let ab = a.compose(&b).expect("compatible");
            let ba = b.compose(&a).expect("compatible");
            assert_eq!(ab.dropped_nodes.len(), ba.dropped_nodes.len());
            for (k, v) in &ab.dropped_nodes {
                let other = ba.dropped_nodes.get(k).expect("present");
                assert!(node_equiv(v, other));
            }
        }

        /// Property-based partial-monoid laws for [`Complement::compose`].
        ///
        /// These are the `complement_partial_monoid_*` proptests that the
        /// `Complement::compose` doc comment cites. Each draws complements
        /// with every field independently populated from a shared key pool,
        /// so distinct draws mix disjoint keys with agreeing shared keys. On
        /// the domain of definition the merge is an idempotent partial
        /// commutative monoid with `empty()` as identity; off it, `compose`
        /// reports a precise error.
        mod props {
            use super::*;
            use crate::asymmetric::ComplementCompose;
            use panproto_inst::value::{FieldPresence, Value};
            use proptest::prelude::*;
            use std::collections::{HashMap, HashSet};

            /// Size of the shared key pool. Small enough that independently
            /// drawn subsets overlap often (so shared keys are genuinely
            /// exercised) yet large enough to also yield disjoint keys.
            const KEY_POOL: u32 = 6;

            /// A non-zero fingerprint used alongside `0`. Any two complements
            /// drawn from `{0, FP}` are fingerprint-compatible, keeping the
            /// algebraic-law generators inside the domain of definition.
            const FP: u64 = 0xC0FF_EE00;

            // Canonical value for each key. Because every field's value is a
            // pure function of its key, two independently generated
            // complements agree on every shared key by construction. Hence
            // `compose` is always defined between them and the partial monoid
            // restricted to these inputs is total, which is what lets the
            // commutativity and associativity properties assert an
            // unconditional equality.
            fn canon_node(k: u32) -> InstNode {
                mk_node(k, &format!("anchor-{}", k % 3))
            }
            fn canon_edge(p: u32, c: u32) -> Edge {
                mk_edge(&format!("s{p}"), &format!("t{c}"), "prop")
            }
            fn canon_arc(p: u32, c: u32) -> (u32, u32, Edge) {
                (p, c, canon_edge(p, c))
            }
            fn canon_fan(k: u32) -> Fan {
                Fan::new(format!("he-{k}"), k).with_child("child", k + 100)
            }
            fn canon_extra_fields(k: u32) -> HashMap<String, Value> {
                let mut m = HashMap::new();
                m.insert("payload".to_owned(), Value::Int(i64::from(k)));
                m
            }
            fn canon_presence(k: u32) -> Option<FieldPresence> {
                // Vary across the `Option<FieldPresence>` variants so the
                // merge's `presence_equiv` comparison exercises absent, null,
                // and present-with-value keys.
                match k % 3 {
                    0 => None,
                    1 => Some(FieldPresence::Null),
                    _ => Some(FieldPresence::Present(Value::Str(format!("v{k}")))),
                }
            }
            fn canon_parent(k: u32) -> u32 {
                k + 100
            }

            type IdSet = HashSet<u32>;
            type PairSet = HashSet<(u32, u32)>;

            fn arb_id_set() -> impl Strategy<Value = IdSet> {
                prop::collection::hash_set(0u32..KEY_POOL, 0..=4)
            }
            fn arb_pair_set() -> impl Strategy<Value = PairSet> {
                prop::collection::hash_set((0u32..KEY_POOL, 0u32..KEY_POOL), 0..=4)
            }
            fn arb_fingerprint() -> impl Strategy<Value = u64> {
                prop_oneof![Just(0u64), Just(FP)]
            }

            /// Strategy generating a [`Complement`] with `dropped_nodes`,
            /// `dropped_arcs`, `dropped_fans`, `contraction_choices`,
            /// `original_parent`, `original_extra_fields`, `arc_edges`,
            /// `original_values`, `synthesized_nodes`, and a fingerprint all
            /// independently populated from the shared key pool.
            fn arb_complement() -> impl Strategy<Value = Complement> {
                (
                    arb_id_set(),
                    arb_pair_set(),
                    arb_id_set(),
                    arb_pair_set(),
                    arb_id_set(),
                    arb_id_set(),
                    arb_pair_set(),
                    arb_id_set(),
                    arb_id_set(),
                    arb_fingerprint(),
                )
                    .prop_map(
                        |(
                            nodes,
                            arcs,
                            fans,
                            contraction,
                            parents,
                            extra,
                            arc_edges,
                            values,
                            synth,
                            fp,
                        )| {
                            let mut c = Complement::empty();
                            for k in nodes {
                                c.dropped_nodes.insert(k, canon_node(k));
                            }
                            for (p, ch) in arcs {
                                c.dropped_arcs.push(canon_arc(p, ch));
                            }
                            for k in fans {
                                c.dropped_fans.push(canon_fan(k));
                            }
                            for (p, ch) in contraction {
                                c.contraction_choices.insert((p, ch), canon_edge(p, ch));
                            }
                            for k in parents {
                                c.original_parent.insert(k, canon_parent(k));
                            }
                            for k in extra {
                                c.original_extra_fields.insert(k, canon_extra_fields(k));
                            }
                            for (p, ch) in arc_edges {
                                c.arc_edges.insert((p, ch), canon_edge(p, ch));
                            }
                            for k in values {
                                c.original_values.insert(k, canon_presence(k));
                            }
                            c.synthesized_nodes = synth;
                            c.source_fingerprint = fp;
                            c
                        },
                    )
            }

            // --- order-independent structural equality on Complement ---

            fn map_equiv<K, V, F>(a: &HashMap<K, V>, b: &HashMap<K, V>, eq: F) -> bool
            where
                K: Eq + std::hash::Hash,
                F: Fn(&V, &V) -> bool,
            {
                a.len() == b.len() && a.iter().all(|(k, v)| b.get(k).is_some_and(|w| eq(v, w)))
            }

            // Composition dedups these vectors, so equal length plus one-way
            // containment is full set equality.
            fn vec_set_equiv<T: PartialEq>(a: &[T], b: &[T]) -> bool {
                a.len() == b.len() && a.iter().all(|x| b.contains(x))
            }

            fn complement_equiv(a: &Complement, b: &Complement) -> bool {
                a.source_fingerprint == b.source_fingerprint
                    && map_equiv(&a.dropped_nodes, &b.dropped_nodes, node_equiv)
                    && vec_set_equiv(&a.dropped_arcs, &b.dropped_arcs)
                    && vec_set_equiv(&a.dropped_fans, &b.dropped_fans)
                    && map_equiv(
                        &a.contraction_choices,
                        &b.contraction_choices,
                        PartialEq::eq,
                    )
                    && a.original_parent == b.original_parent
                    && map_equiv(
                        &a.original_extra_fields,
                        &b.original_extra_fields,
                        extra_fields_equiv,
                    )
                    && map_equiv(&a.arc_edges, &b.arc_edges, PartialEq::eq)
                    && map_equiv(&a.original_values, &b.original_values, |x, y| {
                        presence_equiv(x.as_ref(), y.as_ref())
                    })
                    && a.synthesized_nodes == b.synthesized_nodes
            }

            proptest! {
                #![proptest_config(ProptestConfig::with_cases(128))]

                /// `empty()` is a two-sided identity for `compose` on every
                /// generated complement. The composite is always defined
                /// because `empty()` carries fingerprint `0` and no keys.
                #[test]
                fn complement_partial_monoid_identity(c in arb_complement()) {
                    let e = Complement::empty();
                    let left = e.compose(&c).expect("empty is left-compatible");
                    let right = c.compose(&e).expect("empty is right-compatible");
                    prop_assert!(complement_equiv(&left, &c), "left identity");
                    prop_assert!(complement_equiv(&right, &c), "right identity");
                }

                /// On the domain of definition `compose` is commutative:
                /// `a.compose(b)` and `b.compose(a)` agree. The generator makes
                /// every pair compatible by construction, so both sides are
                /// defined and equality is asserted unconditionally.
                #[test]
                fn complement_partial_monoid_commutative(
                    a in arb_complement(),
                    b in arb_complement(),
                ) {
                    prop_assert!(a.is_compatible(&b), "compatible by construction");
                    let ab = a.compose(&b).expect("ab defined");
                    let ba = b.compose(&a).expect("ba defined");
                    prop_assert!(complement_equiv(&ab, &ba));
                }

                /// On the domain of definition `compose` is associative:
                /// `(a.b).c == a.(b.c)`. Every triple drawn from the shared
                /// key pool is pairwise compatible, so both bracketings are
                /// defined.
                #[test]
                fn complement_partial_monoid_associative(
                    a in arb_complement(),
                    b in arb_complement(),
                    c in arb_complement(),
                ) {
                    let left = a
                        .compose(&b)
                        .and_then(|ab| ab.compose(&c))
                        .expect("(ab)c defined");
                    let right = b
                        .compose(&c)
                        .and_then(|bc| a.compose(&bc))
                        .expect("a(bc) defined");
                    prop_assert!(complement_equiv(&left, &right));
                }

                /// Off the domain of definition `compose` reports a precise
                /// error: a shared key with distinct values yields
                /// [`LensError::ComplementConflict`], and two distinct
                /// non-zero fingerprints yield
                /// [`LensError::ComplementFingerprintMismatch`].
                #[test]
                fn complement_partial_monoid_conflict(
                    key in 0u32..KEY_POOL,
                    left_tag in 0u32..3,
                    right_tag in 0u32..3,
                    fp_a in 1u64..0x1_0000,
                    fp_b in 1u64..0x1_0000,
                ) {
                    // Distinct dropped-node values on a shared key -> conflict.
                    if left_tag != right_tag {
                        let mut a = Complement::empty();
                        a.dropped_nodes.insert(key, mk_node(key, &format!("L{left_tag}")));
                        let mut b = Complement::empty();
                        b.dropped_nodes.insert(key, mk_node(key, &format!("R{right_tag}")));
                        let node_conflict = matches!(
                            a.compose(&b),
                            Err(LensError::ComplementConflict {
                                kind: "dropped_nodes",
                                ..
                            })
                        );
                        prop_assert!(node_conflict, "dropped_nodes conflict expected");
                        prop_assert!(!a.is_compatible(&b));

                        // The same clash surfaces through an edge-keyed field.
                        let mut a2 = Complement::empty();
                        a2.arc_edges
                            .insert((0, 1), mk_edge("s", "t", &format!("L{left_tag}")));
                        let mut b2 = Complement::empty();
                        b2.arc_edges
                            .insert((0, 1), mk_edge("s", "t", &format!("R{right_tag}")));
                        let edge_conflict = matches!(
                            a2.compose(&b2),
                            Err(LensError::ComplementConflict {
                                kind: "arc_edges",
                                ..
                            })
                        );
                        prop_assert!(edge_conflict, "arc_edges conflict expected");
                    }

                    // Two distinct non-zero fingerprints -> mismatch.
                    if fp_a != fp_b {
                        let mut a = Complement::empty();
                        a.source_fingerprint = fp_a;
                        let mut b = Complement::empty();
                        b.source_fingerprint = fp_b;
                        let fp_mismatch = matches!(
                            a.compose(&b),
                            Err(LensError::ComplementFingerprintMismatch { .. })
                        );
                        prop_assert!(fp_mismatch, "fingerprint mismatch expected");
                    }
                }
            }
        }
    }
}
