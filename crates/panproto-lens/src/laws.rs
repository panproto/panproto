//! Round-trip law verification for lenses.
//!
//! Two laws characterize well-behaved lenses:
//! - **`GetPut`**: `put(s, get(s)) = s`: round-tripping with an unmodified
//!   view recovers the original source.
//! - **`PutGet`**: `get(put(s, v)) = v`: what you put is what you get back.
//!
//! `PutGet` is checked modulo the view's derived components, the
//! coordinates a field transform materializes and `get` recomputes on every
//! pass. See [`crate::derived`] for why the law cannot be checked strictly
//! against them and what remains under test once they are excluded.

use crate::Lens;
use crate::asymmetric::{Complement, get, put};
use crate::derived::{DerivedMap, collect_derived_fields, extra_fields_equiv_modulo};
use crate::error::LawViolation;

use panproto_inst::WInstance;

/// Verify both `GetPut` and `PutGet` laws on a specific instance.
///
/// # Errors
///
/// Returns [`LawViolation::GetPut`] if the round-trip fails, or
/// [`LawViolation::PutGet`] if the put-get cycle fails, or
/// `LawViolation::Error` if an operational error occurs.
pub fn check_laws(lens: &Lens, instance: &WInstance) -> Result<(), LawViolation> {
    // GetPut: put(s, get(s)) should recover s
    let (view, complement) = get(lens, instance).map_err(LawViolation::Error)?;
    let restored = put(lens, &view, &complement).map_err(LawViolation::Error)?;

    if !instances_equivalent(instance, &restored) {
        return Err(LawViolation::GetPut {
            detail: format!(
                "original has {} nodes and {} arcs, restored has {} nodes and {} arcs",
                instance.node_count(),
                instance.arc_count(),
                restored.node_count(),
                restored.arc_count(),
            ),
        });
    }

    // PutGet: get(put(s, v, c)) should return v (for arbitrary v).
    // Test with original view.
    check_put_get_with_view(lens, &view, &complement)?;

    // Test with a modified view.
    let modified_view = modify_leaf_values(&view);
    if !instances_equivalent(&view, &modified_view) {
        check_put_get_with_view(lens, &modified_view, &complement)?;
    }

    Ok(())
}

/// Check if two instances are structurally equivalent.
///
/// Since `WInstance` does not derive `PartialEq`, this compares structural
/// properties: root, schema root, node and arc counts, and per-node anchors,
/// values, and extra fields (values compared with NaN-tolerant equivalence).
/// It is the instance comparator used by the round-trip lens laws and by the
/// VCS double-category square check.
#[must_use]
pub fn instances_equivalent(a: &WInstance, b: &WInstance) -> bool {
    if a.root != b.root || a.schema_root != b.schema_root {
        return false;
    }

    if a.node_count() != b.node_count() || a.arc_count() != b.arc_count() {
        return false;
    }

    // Check that all node IDs match and anchors are the same.
    // Value comparison delegates to `asymmetric::value_equiv` so NaN
    // payloads compare equal to themselves (the derived `PartialEq` on
    // `Value` would say NaN ≠ NaN and falsely report drift).
    for (&id, node_a) in &a.nodes {
        match b.nodes.get(&id) {
            Some(node_b) => {
                if node_a.anchor != node_b.anchor {
                    return false;
                }
                if !crate::asymmetric::presence_equiv(node_a.value.as_ref(), node_b.value.as_ref())
                {
                    return false;
                }
                if !crate::asymmetric::extra_fields_equiv(
                    &node_a.extra_fields,
                    &node_b.extra_fields,
                ) {
                    return false;
                }
            }
            None => return false,
        }
    }

    // Compare parent maps for structural consistency.
    if a.parent_map != b.parent_map {
        return false;
    }

    // Compare the arc *set*: sort by (parent, child, edge) then compare.
    let mut arcs_a: Vec<_> = a.arcs.clone();
    let mut arcs_b: Vec<_> = b.arcs.clone();
    arcs_a.sort();
    arcs_b.sort();
    if arcs_a != arcs_b {
        return false;
    }

    // And compare the order children appear in under each parent. The
    // children of a collection node are its elements in sequence, so two
    // instances with the same arc set but a different order serialize to
    // different arrays. Comparing only the sorted set reports such a pair
    // as equivalent, which is how a backward pass that permuted every
    // array could satisfy `GetPut` while handing back reordered records.
    if child_sequences(a) != child_sequences(b) {
        return false;
    }

    // Compare fans (order-independent).
    if a.fans.len() != b.fans.len() {
        return false;
    }
    let mut fans_a: Vec<_> = a.fans.clone();
    let mut fans_b: Vec<_> = b.fans.clone();
    fans_a.sort_by(|x, y| (&x.hyper_edge_id, x.parent).cmp(&(&y.hyper_edge_id, y.parent)));
    fans_b.sort_by(|x, y| (&x.hyper_edge_id, x.parent).cmp(&(&y.hyper_edge_id, y.parent)));
    if fans_a != fans_b {
        return false;
    }

    true
}

/// The ordered child list of every parent, keyed by parent id.
///
/// This is the coordinate that array serialization reads: a collection
/// node's children *are* its elements, in arc order.
fn child_sequences(instance: &WInstance) -> std::collections::HashMap<u32, Vec<u32>> {
    let mut seqs: std::collections::HashMap<u32, Vec<u32>> = std::collections::HashMap::new();
    for (parent, child, _) in &instance.arcs {
        seqs.entry(*parent).or_default().push(*child);
    }
    seqs
}

/// Structural equivalence of two [`Complement`]s.
///
/// Complements do not derive `PartialEq` (their node/value payloads need
/// the NaN-reflexive comparison used elsewhere in this crate). This is
/// `true` exactly when [`complement_divergence`] finds no diverging
/// field — dropped nodes (by id, anchor, value, and `extra_fields`),
/// dropped arcs and fans (order-independent), contraction choices,
/// parent maps, arc-edge disambiguators, snapshot `extra_fields`/values,
/// synthesized-node sets, contracted-into fibres, and the source
/// fingerprint. The edit-lens complement-coherence checker
/// ([`crate::edit_laws`]) and the delta-lens functoriality proptests both
/// rely on it.
pub(crate) fn complements_equivalent(a: &Complement, b: &Complement) -> bool {
    complement_divergence(a, b).is_none()
}

/// Compare two `u32`-keyed maps for structural equivalence under `eq`,
/// returning a divergence detail naming `field` (and the node id) on the
/// first mismatch, or `None` when equivalent.
fn map_divergence<V>(
    field: &str,
    a: &std::collections::HashMap<u32, V>,
    b: &std::collections::HashMap<u32, V>,
    eq: impl Fn(&V, &V) -> bool,
) -> Option<String> {
    if a.len() != b.len() {
        return Some(format!("{field} count: {} vs {}", a.len(), b.len()));
    }
    for (id, va) in a {
        match b.get(id) {
            Some(vb) if eq(va, vb) => {}
            Some(_) => return Some(format!("{field}: node {id} differs")),
            None => return Some(format!("{field}: node {id} present on one side only")),
        }
    }
    None
}

/// The first structural field on which two [`Complement`]s diverge, or
/// `None` when they are structurally equivalent.
///
/// On divergence the returned string names the field (and, where a
/// per-node map diverges, the node id) so callers can report *which*
/// component drifted rather than merely that the complements differ.
/// The field ordering matches [`complements_equivalent`].
pub(crate) fn complement_divergence(a: &Complement, b: &Complement) -> Option<String> {
    if a.source_fingerprint != b.source_fingerprint {
        return Some(format!(
            "source_fingerprint: {} vs {}",
            a.source_fingerprint, b.source_fingerprint
        ));
    }

    // Dropped nodes: same ids, structurally equal payloads.
    if let Some(detail) = map_divergence(
        "dropped_nodes",
        &a.dropped_nodes,
        &b.dropped_nodes,
        dropped_node_equiv,
    ) {
        return Some(detail);
    }

    // Dropped arcs (order-independent).
    let mut arcs_a: Vec<_> = a.dropped_arcs.clone();
    let mut arcs_b: Vec<_> = b.dropped_arcs.clone();
    arcs_a.sort();
    arcs_b.sort();
    if arcs_a != arcs_b {
        return Some("dropped_arcs differ".to_owned());
    }

    // Dropped fans (order-independent).
    if a.dropped_fans.len() != b.dropped_fans.len() {
        return Some(format!(
            "dropped_fans count: {} vs {}",
            a.dropped_fans.len(),
            b.dropped_fans.len()
        ));
    }
    let mut fans_a: Vec<_> = a.dropped_fans.clone();
    let mut fans_b: Vec<_> = b.dropped_fans.clone();
    fans_a.sort_by(|x, y| (&x.hyper_edge_id, x.parent).cmp(&(&y.hyper_edge_id, y.parent)));
    fans_b.sort_by(|x, y| (&x.hyper_edge_id, x.parent).cmp(&(&y.hyper_edge_id, y.parent)));
    if fans_a != fans_b {
        return Some("dropped_fans differ".to_owned());
    }

    // Maps with directly comparable (Edge / u32) values.
    if a.contraction_choices != b.contraction_choices {
        return Some("contraction_choices differ".to_owned());
    }
    if a.original_parent != b.original_parent {
        return Some("original_parent differ".to_owned());
    }
    if a.arc_edges != b.arc_edges {
        return Some("arc_edges differ".to_owned());
    }
    if a.synthesized_nodes != b.synthesized_nodes {
        return Some("synthesized_nodes differ".to_owned());
    }
    if a.contracted_into != b.contracted_into {
        return Some("contracted_into differ".to_owned());
    }

    // Snapshot extra_fields and node values: NaN-reflexive comparison.
    if let Some(detail) = map_divergence(
        "original_extra_fields",
        &a.original_extra_fields,
        &b.original_extra_fields,
        crate::asymmetric::extra_fields_equiv,
    ) {
        return Some(detail);
    }
    if let Some(detail) = map_divergence(
        "original_values",
        &a.original_values,
        &b.original_values,
        |x, y| crate::asymmetric::presence_equiv(x.as_ref(), y.as_ref()),
    ) {
        return Some(detail);
    }

    None
}

/// Structural equivalence of two dropped-complement nodes.
fn dropped_node_equiv(a: &panproto_inst::Node, b: &panproto_inst::Node) -> bool {
    a.anchor == b.anchor
        && crate::asymmetric::presence_equiv(a.value.as_ref(), b.value.as_ref())
        && crate::asymmetric::extra_fields_equiv(&a.extra_fields, &b.extra_fields)
}

/// Verify only the `GetPut` law.
///
/// # Errors
///
/// Returns [`LawViolation::GetPut`] or [`LawViolation::Error`].
pub fn check_get_put(lens: &Lens, instance: &WInstance) -> Result<(), LawViolation> {
    let (view, complement) = get(lens, instance).map_err(LawViolation::Error)?;
    let restored = put(lens, &view, &complement).map_err(LawViolation::Error)?;

    if !instances_equivalent(instance, &restored) {
        return Err(LawViolation::GetPut {
            detail: format!(
                "original has {} nodes, restored has {} nodes",
                instance.node_count(),
                restored.node_count(),
            ),
        });
    }
    Ok(())
}

/// Verify the `PutGet` law: `get(put(s, v, c)) = v`.
///
/// This is a deterministic **smoke check**, not a sampler: it exercises
/// the law on exactly two views, the original (unmodified) view and one
/// canned mutation that perturbs every scalar leaf. Passing this function
/// does *not* establish `PutGet` for arbitrary `v`; it only catches gross
/// regressions on a fixed pair. Broad, generated-view coverage lives in
/// the `property` proptests below, which drive `check_put_get_with_view`
/// with mutations produced by a proptest strategy
/// (`property::arb_view_mutation`).
///
/// The comparison excludes derived view components; see
/// [`crate::derived`] for which components those are and why the law
/// cannot be checked strictly against them.
///
/// # Errors
///
/// Returns [`LawViolation::PutGet`] or [`LawViolation::Error`].
pub fn check_put_get(lens: &Lens, instance: &WInstance) -> Result<(), LawViolation> {
    let (view, complement) = get(lens, instance).map_err(LawViolation::Error)?;

    // Test with original view (identity case).
    check_put_get_with_view(lens, &view, &complement)?;

    // Test with a modified view: perturb the scalar leaves to exercise
    // the law with a genuinely different view.
    let modified_view = modify_leaf_values(&view);
    if !instances_equivalent(&view, &modified_view) {
        check_put_get_with_view(lens, &modified_view, &complement)?;
    }

    Ok(())
}

/// Check the `PutGet` law for a specific view: `get(put(s, v, c)) = v`.
///
/// The comparison excludes the view's derived components, which `get`
/// recomputes from the independent ones on every pass; see
/// [`crate::derived`]. Every independent coordinate must agree exactly.
///
/// Exposed to the crate so the `property` proptests can drive it with
/// generated view mutations (see [`property::arb_view_mutation`]).
///
/// # Errors
///
/// Returns [`LawViolation::PutGet`] or [`LawViolation::Error`].
pub(crate) fn check_put_get_with_view(
    lens: &Lens,
    view: &WInstance,
    complement: &Complement,
) -> Result<(), LawViolation> {
    let restored = put(lens, view, complement).map_err(LawViolation::Error)?;
    let (view2, _) = get(lens, &restored).map_err(LawViolation::Error)?;

    let derived = collect_derived_fields(&lens.compiled);
    if let Some(detail) = instance_divergence_modulo_derived(view, &view2, &derived) {
        return Err(LawViolation::PutGet { detail });
    }
    Ok(())
}

/// Describe the first way `view` and `re_get` disagree on an independent
/// coordinate, or `None` when they agree.
///
/// Structure (roots, node and arc counts, anchors, arcs, fans) is compared
/// exactly. Node values and `extra_fields` are compared modulo the derived
/// coordinates recorded for that node's anchor.
fn instance_divergence_modulo_derived(
    view: &WInstance,
    re_get: &WInstance,
    derived: &DerivedMap,
) -> Option<String> {
    use crate::derived::DerivedFiber;

    if view.root != re_get.root {
        return Some(format!(
            "root node differs: view has {}, re-get has {}",
            view.root, re_get.root
        ));
    }
    if view.schema_root != re_get.schema_root {
        return Some(format!(
            "schema root differs: view has `{}`, re-get has `{}`",
            view.schema_root, re_get.schema_root
        ));
    }
    if view.node_count() != re_get.node_count() {
        return Some(format!(
            "node count differs: view has {}, re-get has {}",
            view.node_count(),
            re_get.node_count()
        ));
    }
    if view.arc_count() != re_get.arc_count() {
        return Some(format!(
            "arc count differs: view has {}, re-get has {}",
            view.arc_count(),
            re_get.arc_count()
        ));
    }

    let empty = DerivedFiber::default();
    for (&id, node_a) in &view.nodes {
        let Some(node_b) = re_get.nodes.get(&id) else {
            return Some(format!("node {id} present in view, absent after re-get"));
        };
        if node_a.anchor != node_b.anchor {
            return Some(format!(
                "node {id} anchor differs: view has `{}`, re-get has `{}`",
                node_a.anchor, node_b.anchor
            ));
        }
        let fiber = derived.get(&node_a.anchor).unwrap_or(&empty);
        if !fiber.value_is_derived()
            && !crate::asymmetric::presence_equiv(node_a.value.as_ref(), node_b.value.as_ref())
        {
            return Some(format!(
                "node {id} (`{}`) value differs: view has {:?}, re-get has {:?}",
                node_a.anchor, node_a.value, node_b.value
            ));
        }
        if let Some(detail) =
            extra_fields_equiv_modulo(&node_a.extra_fields, &node_b.extra_fields, fiber, &[])
        {
            return Some(format!("node {id} (`{}`): {detail}", node_a.anchor));
        }
    }

    if view.parent_map != re_get.parent_map {
        return Some("parent maps differ".to_string());
    }

    let mut arcs_a: Vec<_> = view.arcs.clone();
    let mut arcs_b: Vec<_> = re_get.arcs.clone();
    arcs_a.sort();
    arcs_b.sort();
    if arcs_a != arcs_b {
        return Some("arc sets differ".to_string());
    }

    if view.fans.len() != re_get.fans.len() {
        return Some(format!(
            "fan count differs: view has {}, re-get has {}",
            view.fans.len(),
            re_get.fans.len()
        ));
    }
    let mut fans_a: Vec<_> = view.fans.clone();
    let mut fans_b: Vec<_> = re_get.fans.clone();
    fans_a.sort_by(|x, y| (&x.hyper_edge_id, x.parent).cmp(&(&y.hyper_edge_id, y.parent)));
    fans_b.sort_by(|x, y| (&x.hyper_edge_id, x.parent).cmp(&(&y.hyper_edge_id, y.parent)));
    if fans_a != fans_b {
        return Some("fan sets differ".to_string());
    }

    None
}

/// Verify the `PutPut` law for two views over a shared complement:
/// `put(put(s, v1, c), v2, c) ≡ put(s, v2, c)`. The well-behaved-lens
/// law requiring sequential puts to be subsumed by the latter.
///
/// # Errors
///
/// Returns [`LawViolation::PutGet`] (re-used to signal a put-stage
/// disagreement) or [`LawViolation::Error`] on operational failure.
pub fn check_put_put(
    lens: &Lens,
    instance: &WInstance,
    second_view: &WInstance,
) -> Result<(), LawViolation> {
    let (first_view, complement) = get(lens, instance).map_err(LawViolation::Error)?;
    let after_first = put(lens, &first_view, &complement).map_err(LawViolation::Error)?;
    let (_, complement2) = get(lens, &after_first).map_err(LawViolation::Error)?;
    let after_second_via_chain =
        put(lens, second_view, &complement2).map_err(LawViolation::Error)?;
    let after_second_direct = put(lens, second_view, &complement).map_err(LawViolation::Error)?;

    if !instances_equivalent(&after_second_via_chain, &after_second_direct) {
        return Err(LawViolation::PutGet {
            detail: format!(
                "PutPut violated: chained put has {} nodes, direct put has {} nodes",
                after_second_via_chain.node_count(),
                after_second_direct.node_count(),
            ),
        });
    }
    Ok(())
}

/// Create a copy of the instance with every scalar leaf value perturbed.
///
/// Each scalar kind is moved off its current value so that the canned
/// `PutGet` mutation is non-vacuous whatever the leaf type is. Mutating only
/// strings would leave an integer-, float-, or boolean-leaved instance
/// equal to its own mutation, and `check_put_get` skips the mutated view
/// when it matches the original, so those leaf types would never exercise
/// the mutated branch at all, and the law would appear to hold for them
/// without having been tested.
fn modify_leaf_values(instance: &WInstance) -> WInstance {
    use panproto_inst::value::{FieldPresence, Value};

    let mut modified = instance.clone();
    for node in modified.nodes.values_mut() {
        if let Some(FieldPresence::Present(ref mut value)) = node.value {
            match value {
                Value::Str(s) => s.push_str("_modified"),
                Value::Int(i) => *i = i.wrapping_add(1),
                // NaN is its own perturbation: any finite value differs from
                // it, and `value_equiv` treats NaN as self-equal, so the
                // mutated view compares unequal to the original either way.
                Value::Float(f) => *f = if f.is_nan() { 0.0 } else { *f + 1.0 },
                Value::Bool(b) => *b = !*b,
                _ => {}
            }
        }
    }
    modified
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::{identity_lens, three_node_instance, three_node_schema};

    #[test]
    fn identity_lens_satisfies_laws() {
        let schema = three_node_schema();
        let lens = identity_lens(&schema);
        let instance = three_node_instance();

        let result = check_laws(&lens, &instance);
        assert!(
            result.is_ok(),
            "identity lens should satisfy all laws: {result:?}"
        );
    }

    #[test]
    fn identity_lens_satisfies_get_put() {
        let schema = three_node_schema();
        let lens = identity_lens(&schema);
        let instance = three_node_instance();

        let result = check_get_put(&lens, &instance);
        assert!(result.is_ok(), "identity lens should satisfy GetPut");
    }

    #[test]
    fn identity_lens_satisfies_put_get() {
        let schema = three_node_schema();
        let lens = identity_lens(&schema);
        let instance = three_node_instance();

        let result = check_put_get(&lens, &instance);
        assert!(result.is_ok(), "identity lens should satisfy PutGet");
    }

    #[test]
    fn different_arcs_are_not_equivalent() {
        use panproto_schema::Edge;

        let a = three_node_instance();
        let mut b = a.clone();

        // Swap an arc's edge kind in b so arcs differ
        if let Some(arc) = b.arcs.first_mut() {
            arc.2 = Edge {
                src: arc.2.src.clone(),
                tgt: arc.2.tgt.clone(),
                kind: "different_kind".into(),
                name: arc.2.name.clone(),
            };
        }

        assert!(
            !instances_equivalent(&a, &b),
            "instances with different arcs should not be equivalent"
        );
    }

    // --- proptest strategies and property tests ---

    #[allow(clippy::unwrap_used, clippy::expect_used)]
    mod property {
        use super::*;
        use panproto_gat::Name;
        use panproto_inst::value::{FieldPresence, Value};
        use panproto_inst::{CompiledMigration, Node, WInstance};
        use panproto_schema::{Edge, Schema, Vertex};
        use proptest::prelude::*;
        use smallvec::SmallVec;
        use std::collections::{HashMap, HashSet};

        const LEAF_KINDS: &[&str] = &["string", "integer", "boolean"];

        fn make_schema(verts: &[(&str, &str)], edge_list: &[Edge]) -> Schema {
            let mut vertices = HashMap::new();
            let mut edges = HashMap::new();
            let mut outgoing: HashMap<Name, SmallVec<Edge, 4>> = HashMap::new();
            let mut incoming: HashMap<Name, SmallVec<Edge, 4>> = HashMap::new();
            let mut between: HashMap<(Name, Name), SmallVec<Edge, 2>> = HashMap::new();

            for (id, kind) in verts {
                vertices.insert(
                    Name::from(*id),
                    Vertex {
                        id: Name::from(*id),
                        kind: Name::from(*kind),
                        nsid: None,
                    },
                );
            }
            for e in edge_list {
                edges.insert(e.clone(), e.kind.clone());
                outgoing.entry(e.src.clone()).or_default().push(e.clone());
                incoming.entry(e.tgt.clone()).or_default().push(e.clone());
                between
                    .entry((e.src.clone(), e.tgt.clone()))
                    .or_default()
                    .push(e.clone());
            }

            Schema {
                protocol: "test".into(),
                vertices,
                edges,
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
                outgoing,
                incoming,
                between,
            }
        }

        /// Generate a random schema + instance + identity lens.
        fn arb_identity_lens_scenario() -> impl Strategy<Value = (Lens, WInstance)> {
            // 1-4 leaf children under a root object.
            (1..=4usize).prop_flat_map(|n_children| {
                prop::collection::vec(
                    prop::sample::select(LEAF_KINDS).prop_map(ToOwned::to_owned),
                    n_children..=n_children,
                )
                .prop_flat_map(move |kinds| {
                    // Generate random string values for each leaf.
                    prop::collection::vec(
                        "[a-z]{1,8}".prop_map(String::from),
                        n_children..=n_children,
                    )
                    .prop_map(move |values| {
                        let kinds = kinds.clone();
                        let root_name = "root";
                        let child_names: Vec<String> =
                            (0..kinds.len()).map(|i| format!("child{i}")).collect();

                        // Build schema.
                        let mut vert_specs: Vec<(String, String)> =
                            vec![(root_name.to_owned(), "object".to_owned())];
                        let mut edges = Vec::new();
                        for (i, kind) in kinds.iter().enumerate() {
                            vert_specs.push((child_names[i].clone(), kind.clone()));
                            edges.push(Edge {
                                src: root_name.into(),
                                tgt: Name::from(child_names[i].as_str()),
                                kind: "prop".into(),
                                name: Some(Name::from(child_names[i].as_str())),
                            });
                        }
                        let vert_refs: Vec<(&str, &str)> = vert_specs
                            .iter()
                            .map(|(a, b)| (a.as_str(), b.as_str()))
                            .collect();
                        let schema = make_schema(&vert_refs, &edges);

                        // Build instance.
                        let mut nodes = HashMap::new();
                        nodes.insert(0, Node::new(0, root_name));
                        for (i, val) in values.iter().enumerate() {
                            let node_id = u32::try_from(i + 1).unwrap();
                            nodes.insert(
                                node_id,
                                Node::new(node_id, child_names[i].as_str())
                                    .with_value(FieldPresence::Present(Value::Str(val.clone()))),
                            );
                        }
                        let arcs: Vec<(u32, u32, Edge)> = edges
                            .iter()
                            .enumerate()
                            .map(|(i, e)| (0, u32::try_from(i + 1).unwrap(), e.clone()))
                            .collect();
                        let instance = WInstance::new(nodes, arcs, vec![], 0, root_name.into());

                        // Build identity lens.
                        let surviving_verts: HashSet<Name> =
                            schema.vertices.keys().cloned().collect();
                        let surviving_edges: HashSet<Edge> = schema.edges.keys().cloned().collect();
                        let lens = Lens {
                            compiled: CompiledMigration {
                                surviving_verts,
                                surviving_edges,
                                vertex_remap: HashMap::new(),
                                edge_remap: HashMap::new(),
                                resolver: HashMap::new(),
                                hyper_resolver: HashMap::new(),
                                field_transforms: HashMap::new(),
                                conditional_survival: HashMap::new(),
                                op_term_assignments: HashMap::new(),
                                expansion_path: HashMap::new(),
                            },
                            src_schema: schema.clone(),
                            tgt_schema: schema,
                        };

                        (lens, instance)
                    })
                })
            })
        }

        /// Generate a projection lens scenario: schema with root + N children,
        /// lens drops one child.
        fn arb_projection_lens_scenario() -> impl Strategy<Value = (Lens, WInstance)> {
            // 2-4 leaf children; we'll drop the last one.
            (2..=4usize).prop_flat_map(|n_children| {
                prop::collection::vec("[a-z]{1,8}".prop_map(String::from), n_children..=n_children)
                    .prop_map(move |values| {
                        let root_name = "root";
                        let child_names: Vec<String> =
                            (0..n_children).map(|i| format!("child{i}")).collect();

                        // Build full schema.
                        let mut vert_specs: Vec<(String, String)> =
                            vec![(root_name.to_owned(), "object".to_owned())];
                        let mut all_edges = Vec::new();
                        for name in &child_names {
                            vert_specs.push((name.clone(), "string".to_owned()));
                            all_edges.push(Edge {
                                src: root_name.into(),
                                tgt: Name::from(name.as_str()),
                                kind: "prop".into(),
                                name: Some(Name::from(name.as_str())),
                            });
                        }
                        let vert_refs: Vec<(&str, &str)> = vert_specs
                            .iter()
                            .map(|(a, b)| (a.as_str(), b.as_str()))
                            .collect();
                        let src_schema = make_schema(&vert_refs, &all_edges);

                        // Target schema: drop last child.
                        let tgt_vert_refs: Vec<(&str, &str)> =
                            vert_refs[..vert_refs.len() - 1].to_vec();
                        let tgt_edges: Vec<Edge> = all_edges[..all_edges.len() - 1].to_vec();
                        let tgt_schema = make_schema(&tgt_vert_refs, &tgt_edges);

                        // Build instance.
                        let mut nodes = HashMap::new();
                        nodes.insert(0, Node::new(0, root_name));
                        for (i, val) in values.iter().enumerate() {
                            let node_id = u32::try_from(i + 1).unwrap();
                            nodes.insert(
                                node_id,
                                Node::new(node_id, child_names[i].as_str())
                                    .with_value(FieldPresence::Present(Value::Str(val.clone()))),
                            );
                        }
                        let arcs: Vec<(u32, u32, Edge)> = all_edges
                            .iter()
                            .enumerate()
                            .map(|(i, e)| (0, u32::try_from(i + 1).unwrap(), e.clone()))
                            .collect();
                        let instance = WInstance::new(nodes, arcs, vec![], 0, root_name.into());

                        // Build projection lens.
                        let surviving_verts: HashSet<Name> =
                            tgt_schema.vertices.keys().cloned().collect();
                        let surviving_edges: HashSet<Edge> =
                            tgt_schema.edges.keys().cloned().collect();
                        let lens = Lens {
                            compiled: CompiledMigration {
                                surviving_verts,
                                surviving_edges,
                                vertex_remap: HashMap::new(),
                                edge_remap: HashMap::new(),
                                resolver: HashMap::new(),
                                hyper_resolver: HashMap::new(),
                                field_transforms: HashMap::new(),
                                conditional_survival: HashMap::new(),
                                op_term_assignments: HashMap::new(),
                                expansion_path: HashMap::new(),
                            },
                            src_schema,
                            tgt_schema,
                        };

                        (lens, instance)
                    })
            })
        }

        proptest! {
            #![proptest_config(ProptestConfig::with_cases(64))]

            #[test]
            fn identity_lens_satisfies_laws_proptest(
                (lens, instance) in arb_identity_lens_scenario()
            ) {
                prop_assert!(
                    check_laws(&lens, &instance).is_ok(),
                    "identity lens should satisfy all laws",
                );
            }

            #[test]
            fn projection_lens_satisfies_get_put_proptest(
                (lens, instance) in arb_projection_lens_scenario()
            ) {
                prop_assert!(
                    check_get_put(&lens, &instance).is_ok(),
                    "projection lens should satisfy GetPut",
                );
            }

            #[test]
            fn projection_lens_satisfies_put_get_proptest(
                (lens, instance) in arb_projection_lens_scenario()
            ) {
                prop_assert!(
                    check_put_get(&lens, &instance).is_ok(),
                    "projection lens should satisfy PutGet",
                );
            }

            #[test]
            fn projection_lens_satisfies_full_laws_proptest(
                (lens, instance) in arb_projection_lens_scenario()
            ) {
                prop_assert!(
                    check_laws(&lens, &instance).is_ok(),
                    "projection lens should satisfy all laws",
                );
            }

            /// Verify GetPut holds when ComputeField transforms access
            /// child scalar values. This tests the formal property that
            /// the complement mechanism correctly handles computed
            /// extra_fields derived from the dependent-sum projection.
            ///
            /// The correctness argument: ComputeField writes derived
            /// data to extra_fields. The complement captures
            /// `original_extra_fields` (pre-transform), which does NOT
            /// contain the computed field. `put` restores
            /// `original_extra_fields`, discarding the computed field.
            /// Child scalar nodes survive via tree structure. So GetPut
            /// holds: the restored instance equals the original.
            #[test]
            fn identity_lens_with_compute_field_satisfies_getput(
                (lens, instance) in arb_identity_lens_with_compute_field()
            ) {
                prop_assert!(
                    check_get_put(&lens, &instance).is_ok(),
                    "identity lens with ComputeField should satisfy GetPut",
                );
            }

            /// PutPut: a second put subsumes the first. `put(put(s, v1, c), v2, c) ≡ put(s, v2, c)`.
            /// Exercised over identity lenses (where the law follows trivially) and over
            /// projections (where it constrains the round-trip).
            #[test]
            fn identity_lens_satisfies_put_put_proptest(
                (lens, instance) in arb_identity_lens_scenario()
            ) {
                let (view, _) = get(&lens, &instance).unwrap();
                let perturbed = perturb_view_leaves(&view);
                prop_assert!(
                    check_put_put(&lens, &instance, &perturbed).is_ok(),
                    "identity lens should satisfy PutPut",
                );
            }

            #[test]
            fn projection_lens_satisfies_put_put_proptest(
                (lens, instance) in arb_projection_lens_scenario()
            ) {
                let (view, _) = get(&lens, &instance).unwrap();
                let perturbed = perturb_view_leaves(&view);
                prop_assert!(
                    check_put_put(&lens, &instance, &perturbed).is_ok(),
                    "projection lens should satisfy PutPut",
                );
            }
        }

        /// Perturb every leaf-string value in `view` so that it differs
        /// from the original. Used by `PutPut` tests to obtain a second
        /// view structurally compatible with the schema but distinct
        /// from the round-tripped first view.
        fn perturb_view_leaves(view: &WInstance) -> WInstance {
            let mut perturbed = view.clone();
            for node in perturbed.nodes.values_mut() {
                if let Some(FieldPresence::Present(Value::Str(ref mut s))) = node.value {
                    s.push_str("_v2");
                } else if let Some(FieldPresence::Present(Value::Int(ref mut i))) = node.value {
                    *i = i.wrapping_add(1);
                }
            }
            perturbed
        }

        /// Generate an identity lens scenario WITH a `ComputeField` that
        /// copies a child scalar to a new key. This tests that the
        /// complement mechanism handles the new computed `extra_fields`
        /// correctly.
        fn arb_identity_lens_with_compute_field() -> impl Strategy<Value = (Lens, WInstance)> {
            (2..=4usize).prop_flat_map(|n_children| {
                prop::collection::vec("[a-z]{1,8}".prop_map(String::from), n_children..=n_children)
                    .prop_map(move |values| {
                        let root_name = "root";
                        let child_names: Vec<String> =
                            (0..n_children).map(|i| format!("child{i}")).collect();

                        let mut vert_specs: Vec<(String, String)> =
                            vec![(root_name.to_owned(), "object".to_owned())];
                        let mut edges = Vec::new();
                        for name in &child_names {
                            vert_specs.push((name.clone(), "string".to_owned()));
                            edges.push(Edge {
                                src: root_name.into(),
                                tgt: Name::from(name.as_str()),
                                kind: "prop".into(),
                                name: Some(Name::from(name.as_str())),
                            });
                        }
                        let vert_refs: Vec<(&str, &str)> = vert_specs
                            .iter()
                            .map(|(a, b)| (a.as_str(), b.as_str()))
                            .collect();
                        let schema = make_schema(&vert_refs, &edges);

                        let mut nodes = HashMap::new();
                        nodes.insert(0, Node::new(0, root_name));
                        for (i, val) in values.iter().enumerate() {
                            let node_id = u32::try_from(i + 1).unwrap();
                            nodes.insert(
                                node_id,
                                Node::new(node_id, child_names[i].as_str())
                                    .with_value(FieldPresence::Present(Value::Str(val.clone()))),
                            );
                        }
                        let arcs: Vec<(u32, u32, Edge)> = edges
                            .iter()
                            .enumerate()
                            .map(|(i, e)| (0, u32::try_from(i + 1).unwrap(), e.clone()))
                            .collect();
                        let instance = WInstance::new(nodes, arcs, vec![], 0, root_name.into());

                        // Add a ComputeField that copies child0 to a new key.
                        // This exercises the child scalar access path.
                        let compute = panproto_inst::FieldTransform::ComputeField {
                            target_key: "derived_from_child0".into(),
                            expr: panproto_expr::Expr::Var(std::sync::Arc::from("child0")),
                            inverse: None,
                            coercion_class: panproto_gat::CoercionClass::Projection,
                        };

                        let mut field_transforms = HashMap::new();
                        field_transforms.insert(Name::from(root_name), vec![compute]);

                        let surviving_verts: HashSet<Name> =
                            schema.vertices.keys().cloned().collect();
                        let surviving_edges: HashSet<Edge> = schema.edges.keys().cloned().collect();
                        let lens = Lens {
                            compiled: CompiledMigration {
                                surviving_verts,
                                surviving_edges,
                                vertex_remap: HashMap::new(),
                                edge_remap: HashMap::new(),
                                resolver: HashMap::new(),
                                hyper_resolver: HashMap::new(),
                                field_transforms,
                                conditional_survival: HashMap::new(),
                                op_term_assignments: HashMap::new(),
                                expansion_path: HashMap::new(),
                            },
                            src_schema: schema.clone(),
                            tgt_schema: schema,
                        };

                        (lens, instance)
                    })
            })
        }

        // -------------------------------------------------------------------
        // Widened generators — nested trees, vertex/edge remaps,
        // and interior field transforms.
        // -------------------------------------------------------------------

        /// Generate one leaf child spec: `(kind, value-seed)`.
        fn arb_leaf() -> impl Strategy<Value = (String, String)> {
            (
                prop::sample::select(LEAF_KINDS).prop_map(ToOwned::to_owned),
                "[a-z]{1,6}".prop_map(String::from),
            )
        }

        /// Materialize a leaf value of the given kind from a string seed.
        ///
        /// Keeps the value type aligned with the schema's vertex kind so
        /// that mutation and round-tripping stay schema-compatible.
        fn leaf_value(kind: &str, seed: &str) -> Value {
            match kind {
                "integer" => Value::Int(i64::try_from(seed.len()).unwrap_or(0)),
                "boolean" => Value::Bool(seed.len() % 2 == 0),
                // "string" and anything else fall back to a string carrier.
                _ => Value::Str(seed.to_owned()),
            }
        }

        /// Generate a depth-3 nested tree (root → object mids → leaves) with
        /// an identity lens. Leaves are grandchildren of the root (their
        /// parent is a mid-level object, not the root), and the root and each
        /// mid carry `extra_fields` so the view-mutation strategy has
        /// non-scalar payloads to perturb.
        fn arb_nested_tree_scenario() -> impl Strategy<Value = (Lens, WInstance)> {
            // 1-3 mid-level objects, each with 1-2 leaf children.
            prop::collection::vec(prop::collection::vec(arb_leaf(), 1..=2), 1..=3)
                .prop_map(|mids| build_nested_scenario(&mids))
        }

        /// Build the nested-tree scenario from a per-mid list of leaf specs.
        fn build_nested_scenario(mids: &[Vec<(String, String)>]) -> (Lens, WInstance) {
            let root_name = "root";
            let mut vert_specs: Vec<(String, String)> =
                vec![(root_name.to_owned(), "object".to_owned())];
            let mut edges: Vec<Edge> = Vec::new();
            let mut nodes: HashMap<u32, Node> = HashMap::new();
            let mut arcs: Vec<(u32, u32, Edge)> = Vec::new();

            // Root carries an extra field.
            let mut root_node = Node::new(0, root_name);
            root_node
                .extra_fields
                .insert("kind_tag".to_owned(), Value::Str("object".to_owned()));
            nodes.insert(0, root_node);

            let mut next_id: u32 = 1;
            for (i, leaves) in mids.iter().enumerate() {
                let mid_name = format!("mid{i}");
                vert_specs.push((mid_name.clone(), "object".to_owned()));
                let mid_edge = Edge {
                    src: root_name.into(),
                    tgt: Name::from(mid_name.as_str()),
                    kind: "prop".into(),
                    name: Some(Name::from(mid_name.as_str())),
                };
                edges.push(mid_edge.clone());
                let mid_id = next_id;
                next_id += 1;
                let mut mid_node = Node::new(mid_id, mid_name.as_str());
                mid_node
                    .extra_fields
                    .insert("depth".to_owned(), Value::Int(1));
                nodes.insert(mid_id, mid_node);
                arcs.push((0, mid_id, mid_edge));

                for (j, (kind, seed)) in leaves.iter().enumerate() {
                    let leaf_name = format!("leaf{i}_{j}");
                    vert_specs.push((leaf_name.clone(), kind.clone()));
                    let leaf_edge = Edge {
                        src: Name::from(mid_name.as_str()),
                        tgt: Name::from(leaf_name.as_str()),
                        kind: "prop".into(),
                        name: Some(Name::from(leaf_name.as_str())),
                    };
                    edges.push(leaf_edge.clone());
                    let leaf_id = next_id;
                    next_id += 1;
                    let leaf_node = Node::new(leaf_id, leaf_name.as_str())
                        .with_value(FieldPresence::Present(leaf_value(kind, seed)));
                    nodes.insert(leaf_id, leaf_node);
                    // Grandchild arc: parent is the mid node, not the root.
                    arcs.push((mid_id, leaf_id, leaf_edge));
                }
            }

            let vert_refs: Vec<(&str, &str)> = vert_specs
                .iter()
                .map(|(a, b)| (a.as_str(), b.as_str()))
                .collect();
            let schema = make_schema(&vert_refs, &edges);
            let instance = WInstance::new(nodes, arcs, vec![], 0, root_name.into());

            let surviving_verts: HashSet<Name> = schema.vertices.keys().cloned().collect();
            let surviving_edges: HashSet<Edge> = schema.edges.keys().cloned().collect();
            let lens = Lens {
                compiled: CompiledMigration {
                    surviving_verts,
                    surviving_edges,
                    vertex_remap: HashMap::new(),
                    edge_remap: HashMap::new(),
                    resolver: HashMap::new(),
                    hyper_resolver: HashMap::new(),
                    field_transforms: HashMap::new(),
                    conditional_survival: HashMap::new(),
                    op_term_assignments: HashMap::new(),
                    expansion_path: HashMap::new(),
                },
                src_schema: schema.clone(),
                tgt_schema: schema,
            };
            (lens, instance)
        }

        /// Generate a lossless *rename remap* lens between two distinct
        /// schemas: every child vertex `src_childᵢ` is renamed to
        /// `tgt_childᵢ`, with `vertex_remap` and `edge_remap` populated
        /// accordingly. The remap is a schema isomorphism, so the lens is
        /// well-behaved (all laws hold).
        fn arb_remap_lens_scenario() -> impl Strategy<Value = (Lens, WInstance)> {
            (1..=4usize).prop_flat_map(|n| {
                prop::collection::vec("[a-z]{1,6}".prop_map(String::from), n..=n)
                    .prop_map(|values| build_remap_scenario(&values))
            })
        }

        /// Build the rename-remap scenario from a list of leaf string values.
        fn build_remap_scenario(values: &[String]) -> (Lens, WInstance) {
            let root_name = "root";
            let n = values.len();

            let mut src_vert_specs: Vec<(String, String)> =
                vec![(root_name.to_owned(), "object".to_owned())];
            let mut tgt_vert_specs: Vec<(String, String)> =
                vec![(root_name.to_owned(), "object".to_owned())];
            let mut src_edges: Vec<Edge> = Vec::new();
            let mut tgt_edges: Vec<Edge> = Vec::new();

            let mut vertex_remap: HashMap<Name, Name> = HashMap::new();
            let mut edge_remap: HashMap<Edge, Edge> = HashMap::new();

            for i in 0..n {
                let src_child = format!("src_child{i}");
                let tgt_child = format!("tgt_child{i}");
                let key = format!("key{i}");
                src_vert_specs.push((src_child.clone(), "string".to_owned()));
                tgt_vert_specs.push((tgt_child.clone(), "string".to_owned()));

                let src_edge = Edge {
                    src: root_name.into(),
                    tgt: Name::from(src_child.as_str()),
                    kind: "prop".into(),
                    name: Some(Name::from(key.as_str())),
                };
                let tgt_edge = Edge {
                    src: root_name.into(),
                    tgt: Name::from(tgt_child.as_str()),
                    kind: "prop".into(),
                    name: Some(Name::from(key.as_str())),
                };
                src_edges.push(src_edge.clone());
                tgt_edges.push(tgt_edge.clone());

                // Non-empty vertex_remap and edge_remap: the migration
                // renames every child vertex and its incoming edge.
                vertex_remap.insert(
                    Name::from(src_child.as_str()),
                    Name::from(tgt_child.as_str()),
                );
                edge_remap.insert(src_edge, tgt_edge);
            }

            let src_vert_refs: Vec<(&str, &str)> = src_vert_specs
                .iter()
                .map(|(a, b)| (a.as_str(), b.as_str()))
                .collect();
            let tgt_vert_refs: Vec<(&str, &str)> = tgt_vert_specs
                .iter()
                .map(|(a, b)| (a.as_str(), b.as_str()))
                .collect();
            let src_schema = make_schema(&src_vert_refs, &src_edges);
            let tgt_schema = make_schema(&tgt_vert_refs, &tgt_edges);

            // Build the source instance over the source (un-renamed) schema.
            let mut nodes = HashMap::new();
            nodes.insert(0, Node::new(0, root_name));
            let mut arcs: Vec<(u32, u32, Edge)> = Vec::new();
            for (i, val) in values.iter().enumerate() {
                let node_id = u32::try_from(i + 1).unwrap();
                let src_child = format!("src_child{i}");
                nodes.insert(
                    node_id,
                    Node::new(node_id, src_child.as_str())
                        .with_value(FieldPresence::Present(Value::Str(val.clone()))),
                );
                arcs.push((0, node_id, src_edges[i].clone()));
            }
            let instance = WInstance::new(nodes, arcs, vec![], 0, root_name.into());

            // Surviving vertices/edges are keyed by *target* anchors, since
            // `wtype_restrict` checks survival against the remapped anchor.
            let surviving_verts: HashSet<Name> = tgt_schema.vertices.keys().cloned().collect();
            let surviving_edges: HashSet<Edge> = tgt_schema.edges.keys().cloned().collect();
            let lens = Lens {
                compiled: CompiledMigration {
                    surviving_verts,
                    surviving_edges,
                    vertex_remap,
                    edge_remap,
                    resolver: HashMap::new(),
                    hyper_resolver: HashMap::new(),
                    field_transforms: HashMap::new(),
                    conditional_survival: HashMap::new(),
                    op_term_assignments: HashMap::new(),
                    expansion_path: HashMap::new(),
                },
                src_schema,
                tgt_schema,
            };
            (lens, instance)
        }

        /// Generate an identity-structure lens carrying interior
        /// `FieldTransform`s (`RenameField`, `DropField`, `AddField`) on the
        /// root object's `extra_fields`. Each transform snapshots the
        /// pre-transform fields into the complement, so `GetPut` holds.
        fn arb_field_transform_scenario() -> impl Strategy<Value = (Lens, WInstance)> {
            (
                "[a-z]{1,6}".prop_map(String::from),
                "[a-z]{1,6}".prop_map(String::from),
                "[a-z]{1,6}".prop_map(String::from),
                prop::collection::vec("[a-z]{1,6}".prop_map(String::from), 1..=3),
            )
                .prop_map(|(a_val, b_val, add_val, leaf_vals)| {
                    build_field_transform_scenario(&a_val, &b_val, &add_val, &leaf_vals)
                })
        }

        /// Build the interior-field-transform scenario.
        fn build_field_transform_scenario(
            a_val: &str,
            b_val: &str,
            add_val: &str,
            leaf_vals: &[String],
        ) -> (Lens, WInstance) {
            use panproto_inst::FieldTransform;

            let root_name = "root";
            let mut vert_specs: Vec<(String, String)> =
                vec![(root_name.to_owned(), "object".to_owned())];
            let mut edges = Vec::new();
            let child_names: Vec<String> =
                (0..leaf_vals.len()).map(|i| format!("child{i}")).collect();
            for name in &child_names {
                vert_specs.push((name.clone(), "string".to_owned()));
                edges.push(Edge {
                    src: root_name.into(),
                    tgt: Name::from(name.as_str()),
                    kind: "prop".into(),
                    name: Some(Name::from(name.as_str())),
                });
            }
            let vert_refs: Vec<(&str, &str)> = vert_specs
                .iter()
                .map(|(a, b)| (a.as_str(), b.as_str()))
                .collect();
            let schema = make_schema(&vert_refs, &edges);

            let mut root = Node::new(0, root_name);
            root.extra_fields
                .insert("field_a".to_owned(), Value::Str(a_val.to_owned()));
            root.extra_fields
                .insert("field_b".to_owned(), Value::Str(b_val.to_owned()));
            root.extra_fields
                .insert("keep".to_owned(), Value::Str("kept".to_owned()));
            let mut nodes = HashMap::new();
            nodes.insert(0, root);
            let mut arcs = Vec::new();
            for (i, val) in leaf_vals.iter().enumerate() {
                let node_id = u32::try_from(i + 1).unwrap();
                nodes.insert(
                    node_id,
                    Node::new(node_id, child_names[i].as_str())
                        .with_value(FieldPresence::Present(Value::Str(val.clone()))),
                );
                arcs.push((0, node_id, edges[i].clone()));
            }
            let instance = WInstance::new(nodes, arcs, vec![], 0, root_name.into());

            // Interior transforms on the root object (an interior anchor:
            // it has children). Rename one field, drop another, add a third.
            let transforms = vec![
                FieldTransform::RenameField {
                    old_key: "field_a".to_owned(),
                    new_key: "field_a_renamed".to_owned(),
                },
                FieldTransform::DropField {
                    key: "field_b".to_owned(),
                },
                FieldTransform::AddField {
                    key: "field_c".to_owned(),
                    value: Value::Str(add_val.to_owned()),
                },
            ];
            let mut field_transforms: HashMap<Name, Vec<FieldTransform>> = HashMap::new();
            field_transforms.insert(Name::from(root_name), transforms);

            let surviving_verts: HashSet<Name> = schema.vertices.keys().cloned().collect();
            let surviving_edges: HashSet<Edge> = schema.edges.keys().cloned().collect();
            let lens = Lens {
                compiled: CompiledMigration {
                    surviving_verts,
                    surviving_edges,
                    vertex_remap: HashMap::new(),
                    edge_remap: HashMap::new(),
                    resolver: HashMap::new(),
                    hyper_resolver: HashMap::new(),
                    field_transforms,
                    conditional_survival: HashMap::new(),
                    op_term_assignments: HashMap::new(),
                    expansion_path: HashMap::new(),
                },
                src_schema: schema.clone(),
                tgt_schema: schema,
            };
            (lens, instance)
        }

        /// Generate an identity-structure lens carrying an interior
        /// `FieldTransform::ApplyExpr` on the root object's `extra_fields`
        /// (the DSL `ApplyExpr` step's compile target). The forward expr
        /// uppercases a field and the inverse lowercases it; the complement
        /// snapshots the pre-transform fields, so `GetPut` holds regardless
        /// of the expression's own round-trip fidelity.
        fn arb_apply_expr_scenario() -> impl Strategy<Value = (Lens, WInstance)> {
            (
                "[a-z]{1,6}".prop_map(String::from),
                prop::collection::vec("[a-z]{1,6}".prop_map(String::from), 1..=3),
            )
                .prop_map(|(field_val, leaf_vals)| {
                    build_apply_expr_scenario(&field_val, &leaf_vals)
                })
        }

        /// Build the `ApplyExpr` scenario.
        fn build_apply_expr_scenario(field_val: &str, leaf_vals: &[String]) -> (Lens, WInstance) {
            use panproto_expr::{BuiltinOp, Expr};
            use panproto_inst::FieldTransform;
            use std::sync::Arc;

            let root_name = "root";
            let mut vert_specs: Vec<(String, String)> =
                vec![(root_name.to_owned(), "object".to_owned())];
            let mut edges = Vec::new();
            let child_names: Vec<String> =
                (0..leaf_vals.len()).map(|i| format!("child{i}")).collect();
            for name in &child_names {
                vert_specs.push((name.clone(), "string".to_owned()));
                edges.push(Edge {
                    src: root_name.into(),
                    tgt: Name::from(name.as_str()),
                    kind: "prop".into(),
                    name: Some(Name::from(name.as_str())),
                });
            }
            let vert_refs: Vec<(&str, &str)> = vert_specs
                .iter()
                .map(|(a, b)| (a.as_str(), b.as_str()))
                .collect();
            let schema = make_schema(&vert_refs, &edges);

            let mut root = Node::new(0, root_name);
            root.extra_fields
                .insert("label".to_owned(), Value::Str(field_val.to_owned()));
            let mut nodes = HashMap::new();
            nodes.insert(0, root);
            let mut arcs = Vec::new();
            for (i, val) in leaf_vals.iter().enumerate() {
                let node_id = u32::try_from(i + 1).unwrap();
                nodes.insert(
                    node_id,
                    Node::new(node_id, child_names[i].as_str())
                        .with_value(FieldPresence::Present(Value::Str(val.clone()))),
                );
                arcs.push((0, node_id, edges[i].clone()));
            }
            let instance = WInstance::new(nodes, arcs, vec![], 0, root_name.into());

            let transform = FieldTransform::ApplyExpr {
                key: "label".to_owned(),
                expr: Expr::Builtin(BuiltinOp::Upper, vec![Expr::Var(Arc::from("label"))]),
                inverse: Some(Expr::Builtin(
                    BuiltinOp::Lower,
                    vec![Expr::Var(Arc::from("label"))],
                )),
                coercion_class: panproto_gat::CoercionClass::Retraction,
            };
            let mut field_transforms: HashMap<Name, Vec<FieldTransform>> = HashMap::new();
            field_transforms.insert(Name::from(root_name), vec![transform]);

            let surviving_verts: HashSet<Name> = schema.vertices.keys().cloned().collect();
            let surviving_edges: HashSet<Edge> = schema.edges.keys().cloned().collect();
            let lens = Lens {
                compiled: CompiledMigration {
                    surviving_verts,
                    surviving_edges,
                    vertex_remap: HashMap::new(),
                    edge_remap: HashMap::new(),
                    resolver: HashMap::new(),
                    hyper_resolver: HashMap::new(),
                    field_transforms,
                    conditional_survival: HashMap::new(),
                    op_term_assignments: HashMap::new(),
                    expansion_path: HashMap::new(),
                },
                src_schema: schema.clone(),
                tgt_schema: schema,
            };
            (lens, instance)
        }

        // -------------------------------------------------------------------
        // Generated view mutations.
        // -------------------------------------------------------------------

        /// A generated plan for mutating a view. Applying it produces a
        /// schema-compatible mutant that differs from the original view in
        /// (a random subset of) node scalar values and `extra_fields`.
        #[derive(Debug, Clone)]
        pub(super) struct ViewMutation {
            /// Bitmask selecting which view nodes (in sorted-id order) to
            /// mutate. Mutating a subset rather than every node is the point:
            /// the canned mutator touches every scalar leaf uniformly.
            node_mask: u64,
            /// Suffix appended to selected string carriers (guaranteed
            /// non-empty, so the mutant genuinely differs).
            string_suffix: String,
            /// Delta added (wrapping) to selected integer carriers.
            int_delta: i64,
            /// Whether to flip selected boolean carriers.
            flip_bool: bool,
            /// Whether to also mutate `extra_fields` values (not only the
            /// node's scalar `value`).
            mutate_extra_fields: bool,
        }

        /// Strategy producing arbitrary [`ViewMutation`] plans.
        pub(super) fn arb_view_mutation() -> impl Strategy<Value = ViewMutation> {
            (
                any::<u64>(),
                "[a-z]{1,4}".prop_map(String::from),
                -5i64..=5i64,
                any::<bool>(),
                any::<bool>(),
            )
                .prop_map(
                    |(node_mask, string_suffix, int_delta, flip_bool, mutate_extra_fields)| {
                        ViewMutation {
                            node_mask,
                            string_suffix,
                            int_delta,
                            flip_bool,
                            mutate_extra_fields,
                        }
                    },
                )
        }

        /// Apply a [`ViewMutation`] plan to `view`, returning a new instance.
        ///
        /// The mutation is type-preserving: strings stay strings, integers
        /// stay integers, booleans stay booleans. Only the selected nodes
        /// (per `node_mask`) are touched.
        pub(super) fn apply_view_mutation(view: &WInstance, m: &ViewMutation) -> WInstance {
            let mut mutant = view.clone();
            let mut ids: Vec<u32> = mutant.nodes.keys().copied().collect();
            ids.sort_unstable();
            for (idx, id) in ids.iter().enumerate() {
                let selected = idx < 64 && (m.node_mask >> idx) & 1 == 1;
                if !selected {
                    continue;
                }
                let Some(node) = mutant.nodes.get_mut(id) else {
                    continue;
                };
                match node.value {
                    Some(FieldPresence::Present(Value::Str(ref mut s))) => {
                        s.push_str(&m.string_suffix);
                    }
                    Some(FieldPresence::Present(Value::Int(ref mut i))) => {
                        *i = i.wrapping_add(m.int_delta);
                    }
                    Some(FieldPresence::Present(Value::Bool(ref mut b))) if m.flip_bool => {
                        *b = !*b;
                    }
                    _ => {}
                }
                if m.mutate_extra_fields {
                    for v in node.extra_fields.values_mut() {
                        match v {
                            Value::Str(s) => s.push_str(&m.string_suffix),
                            Value::Int(i) => *i = i.wrapping_add(m.int_delta),
                            Value::Bool(b) if m.flip_bool => *b = !*b,
                            _ => {}
                        }
                    }
                }
            }
            mutant
        }

        proptest! {
            #![proptest_config(ProptestConfig::with_cases(128))]

            /// A depth-3 nested tree with grandchild nodes round-trips under
            /// the identity lens.
            #[test]
            fn nested_tree_satisfies_laws(
                (lens, instance) in arb_nested_tree_scenario()
            ) {
                prop_assert!(
                    check_laws(&lens, &instance).is_ok(),
                    "nested identity lens should satisfy all laws: {:?}",
                    check_laws(&lens, &instance),
                );
            }

            /// A lossless rename remap (non-empty vertex_remap + edge_remap)
            /// is well-behaved.
            #[test]
            fn remap_lens_satisfies_laws(
                (lens, instance) in arb_remap_lens_scenario()
            ) {
                prop_assert!(
                    check_laws(&lens, &instance).is_ok(),
                    "rename remap lens should satisfy all laws: {:?}",
                    check_laws(&lens, &instance),
                );
                prop_assert!(check_get_put(&lens, &instance).is_ok());
            }

            /// Interior field transforms (rename/drop/add) preserve GetPut,
            /// since the complement snapshots the pre-transform fields.
            ///
            /// Together with [`arb_apply_expr_scenario`] below and the
            /// existing `identity_lens_with_compute_field` proptest, this
            /// covers the compile targets of the DSL field-level steps
            /// (RemoveField/DropField, RenameField, AddField, ApplyExpr,
            /// ComputeField) over generated instances.
            #[test]
            fn field_transform_satisfies_get_put(
                (lens, instance) in arb_field_transform_scenario()
            ) {
                prop_assert!(
                    check_get_put(&lens, &instance).is_ok(),
                    "interior field transforms should satisfy GetPut: {:?}",
                    check_get_put(&lens, &instance),
                );
            }

            /// The `ApplyExpr` field step preserves GetPut over generated
            /// instances.
            #[test]
            fn field_apply_expr_satisfies_get_put(
                (lens, instance) in arb_apply_expr_scenario()
            ) {
                prop_assert!(
                    check_get_put(&lens, &instance).is_ok(),
                    "ApplyExpr field transform should satisfy GetPut: {:?}",
                    check_get_put(&lens, &instance),
                );
            }

            /// PutGet over generated view mutations (identity lens).
            #[test]
            fn identity_put_get_generated_mutation(
                (lens, instance) in arb_identity_lens_scenario(),
                mutation in arb_view_mutation(),
            ) {
                let (view, complement) = get(&lens, &instance).unwrap();
                let mutant = apply_view_mutation(&view, &mutation);
                prop_assert!(
                    check_put_get_with_view(&lens, &mutant, &complement).is_ok(),
                    "identity lens PutGet should hold for generated mutant",
                );
            }

            /// PutGet over generated view mutations (projection lens).
            #[test]
            fn projection_put_get_generated_mutation(
                (lens, instance) in arb_projection_lens_scenario(),
                mutation in arb_view_mutation(),
            ) {
                let (view, complement) = get(&lens, &instance).unwrap();
                let mutant = apply_view_mutation(&view, &mutation);
                prop_assert!(
                    check_put_get_with_view(&lens, &mutant, &complement).is_ok(),
                    "projection lens PutGet should hold for generated mutant",
                );
            }

            /// PutGet over generated view mutations on a nested tree, where
            /// the mutation also perturbs interior `extra_fields`.
            #[test]
            fn nested_put_get_generated_mutation(
                (lens, instance) in arb_nested_tree_scenario(),
                mutation in arb_view_mutation(),
            ) {
                let (view, complement) = get(&lens, &instance).unwrap();
                let mutant = apply_view_mutation(&view, &mutation);
                prop_assert!(
                    check_put_get_with_view(&lens, &mutant, &complement).is_ok(),
                    "nested identity lens PutGet should hold for generated mutant",
                );
            }

            /// PutPut driven by a generated second view (identity lens).
            #[test]
            fn identity_put_put_generated_mutation(
                (lens, instance) in arb_identity_lens_scenario(),
                mutation in arb_view_mutation(),
            ) {
                let (view, _) = get(&lens, &instance).unwrap();
                let second = apply_view_mutation(&view, &mutation);
                prop_assert!(
                    check_put_put(&lens, &instance, &second).is_ok(),
                    "identity lens PutPut should hold for generated second view",
                );
            }

            /// PutPut driven by a generated second view (projection lens).
            #[test]
            fn projection_put_put_generated_mutation(
                (lens, instance) in arb_projection_lens_scenario(),
                mutation in arb_view_mutation(),
            ) {
                let (view, _) = get(&lens, &instance).unwrap();
                let second = apply_view_mutation(&view, &mutation);
                prop_assert!(
                    check_put_put(&lens, &instance, &second).is_ok(),
                    "projection lens PutPut should hold for generated second view",
                );
            }
        }
    }
}
