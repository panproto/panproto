//! Edit lens law verification.
//!
//! Checks the two Hofmann-Pierce-Wagner edit lens laws:
//!
//! - **Consistency**: translating a source edit and applying it to the
//!   view gives the same result as applying the edit to the source and
//!   then doing a whole-state `get`.
//! - **Complement coherence**: the complement state after `get_edit` is
//!   consistent with the complement that would result from a whole-state
//!   `get` on the edited source.

use std::fmt;

use panproto_inst::{TreeEdit, WInstance};

use crate::Lens;
use crate::edit_error::EditLensError;
use crate::edit_lens::EditLens;

/// A violation of an edit lens law.
#[derive(Debug)]
#[non_exhaustive]
pub enum EditLawViolation {
    /// Consistency law violation.
    Consistency {
        /// Description of the mismatch.
        detail: String,
    },
    /// Complement coherence law violation.
    ComplementCoherence {
        /// Description of the mismatch.
        detail: String,
    },
    /// An error occurred during law checking.
    Error(EditLensError),
}

impl fmt::Display for EditLawViolation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Consistency { detail } => write!(f, "Consistency law violated: {detail}"),
            Self::ComplementCoherence { detail } => {
                write!(f, "Complement coherence violated: {detail}")
            }
            Self::Error(e) => write!(f, "error during law check: {e}"),
        }
    }
}

impl std::error::Error for EditLawViolation {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Error(e) => Some(e),
            _ => None,
        }
    }
}

/// Check the Consistency law for an edit lens.
///
/// Verifies that translating `edit` through the lens and applying the
/// result to the current view produces the same view as applying `edit`
/// to the source and then doing a whole-state `get`.
///
/// # Errors
///
/// Returns [`EditLawViolation`] if the law is violated or an error occurs.
pub fn check_edit_consistency(
    lens: &mut EditLens,
    edit: &TreeEdit,
    source: &WInstance,
) -> Result<(), EditLawViolation> {
    // Path 1: translate edit, apply to view.
    let mut lens_clone = clone_edit_lens(lens);
    let view_edit = lens_clone
        .get_edit(edit.clone())
        .map_err(EditLawViolation::Error)?;

    // Get current view via whole-state get.
    let state_lens = Lens {
        compiled: lens.compiled.clone(),
        src_schema: lens.src_schema.clone(),
        tgt_schema: lens.tgt_schema.clone(),
    };
    let (mut view, _) = crate::get(&state_lens, source)
        .map_err(|e| EditLawViolation::Error(EditLensError::TranslationFailed(e.to_string())))?;
    view_edit
        .apply(&mut view)
        .map_err(|e| EditLawViolation::Error(EditLensError::EditApply(e)))?;

    // Path 2: apply edit to source, then whole-state get.
    let mut edited_source = source.clone();
    edit.apply(&mut edited_source)
        .map_err(|e| EditLawViolation::Error(EditLensError::EditApply(e)))?;
    let (view2, _) = crate::get(&state_lens, &edited_source)
        .map_err(|e| EditLawViolation::Error(EditLensError::TranslationFailed(e.to_string())))?;

    // Compare the two views by full instance structure — node values,
    // `extra_fields`, arcs, parent map, and fans — not merely node counts
    // and anchors. A value-corrupting edit translation preserves the
    // count and anchors but changes a field, so the coarser comparison
    // would pass a genuine consistency-law violation.
    if !crate::laws::instances_equivalent(&view, &view2) {
        return Err(EditLawViolation::Consistency {
            detail: format!(
                "translate-then-apply and apply-then-get views diverge \
                 ({} vs {} nodes, {} vs {} arcs)",
                view.node_count(),
                view2.node_count(),
                view.arc_count(),
                view2.arc_count(),
            ),
        });
    }

    Ok(())
}

/// Check the Complement coherence law for an edit lens.
///
/// Verifies that the complement state after `get_edit` matches the
/// complement that would result from a whole-state `get` on the edited
/// source.
///
/// # Errors
///
/// Returns [`EditLawViolation`] if the law is violated or an error occurs.
pub fn check_complement_coherence(
    lens: &mut EditLens,
    edit: &TreeEdit,
    source: &WInstance,
) -> Result<(), EditLawViolation> {
    // Path 1: get_edit on the lens.
    let mut lens_clone = clone_edit_lens(lens);
    let _ = lens_clone
        .get_edit(edit.clone())
        .map_err(EditLawViolation::Error)?;

    // Path 2: apply edit to source, then whole-state get.
    let mut edited_source = source.clone();
    edit.apply(&mut edited_source)
        .map_err(|e| EditLawViolation::Error(EditLensError::EditApply(e)))?;

    let state_lens = Lens {
        compiled: lens.compiled.clone(),
        src_schema: lens.src_schema.clone(),
        tgt_schema: lens.tgt_schema.clone(),
    };
    let (_, complement2) = crate::get(&state_lens, &edited_source)
        .map_err(|e| EditLawViolation::Error(EditLensError::TranslationFailed(e.to_string())))?;

    // Compare the two complements by full structure — dropped node
    // contents, dropped arcs and fans, contraction choices, original
    // parent map, arc-edge disambiguators, snapshot `extra_fields` and
    // values, synthesized-node set, and source fingerprint — not merely
    // the dropped-node count. Complements with the same count but
    // different dropped contents or contraction choices are a genuine
    // coherence-law violation the count comparison would pass.
    let c1 = &lens_clone.complement;
    if !crate::laws::complements_equivalent(c1, &complement2) {
        let detail = crate::laws::complement_divergence(c1, &complement2)
            .unwrap_or_else(|| "complement structure diverges".to_owned());
        return Err(EditLawViolation::ComplementCoherence { detail });
    }

    Ok(())
}

/// Clone an `EditLens` for law checking (needs all fields).
fn clone_edit_lens(lens: &EditLens) -> EditLens {
    EditLens {
        compiled: lens.compiled.clone(),
        src_schema: lens.src_schema.clone(),
        tgt_schema: lens.tgt_schema.clone(),
        complement: lens.complement.clone(),
        protocol: lens.protocol.clone(),
        reverse_vertex_remap: lens.reverse_vertex_remap.clone(),
        reverse_edge_remap: lens.reverse_edge_remap.clone(),
        pipeline: lens.pipeline.clone(),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use panproto_gat::Name;
    use panproto_inst::{TreeEdit, Value};
    use panproto_schema::Protocol;

    use crate::edit_lens::EditLens;
    use crate::tests::{identity_lens, three_node_instance, three_node_schema};

    use super::*;

    fn test_protocol() -> Protocol {
        Protocol {
            name: "test".into(),
            schema_theory: "ThTest".into(),
            instance_theory: "ThWType".into(),
            schema_composition: None,
            instance_composition: None,
            edge_rules: vec![],
            obj_kinds: vec![],
            constraint_sorts: vec![],
            has_order: false,
            has_coproducts: false,
            has_recursion: false,
            has_causal: false,
            nominal_identity: false,
            has_defaults: false,
            has_coercions: false,
            has_mergers: false,
            has_policies: false,
        }
    }

    #[test]
    fn consistency_identity_lens() {
        let schema = three_node_schema();
        let lens = identity_lens(&schema);
        let instance = three_node_instance();
        let mut edit_lens = EditLens::from_lens(lens, test_protocol());
        edit_lens.initialize(&instance).unwrap();

        let edit = TreeEdit::SetField {
            node_id: 1,
            field: Name::from("text"),
            value: Value::Str("changed".into()),
        };

        let result = check_edit_consistency(&mut edit_lens, &edit, &instance);
        assert!(result.is_ok(), "consistency should hold: {result:?}");
    }

    #[test]
    fn complement_coherence_identity_lens() {
        let schema = three_node_schema();
        let lens = identity_lens(&schema);
        let instance = three_node_instance();
        let mut edit_lens = EditLens::from_lens(lens, test_protocol());
        edit_lens.initialize(&instance).unwrap();

        let edit = TreeEdit::SetField {
            node_id: 1,
            field: Name::from("text"),
            value: Value::Str("changed".into()),
        };

        let result = check_complement_coherence(&mut edit_lens, &edit, &instance);
        assert!(result.is_ok(), "coherence should hold: {result:?}");
    }

    /// Build an identity-shaped lens carrying a field transform keyed
    /// under the vertex kind `post:body.text` that upper-cases a field
    /// named `tag`. The whole-state `get` applies field transforms per
    /// node anchor, whereas the edit translation applies any
    /// name-matching transform regardless of anchor; a `SetField` of
    /// `tag` on a *different-anchored* node is therefore upper-cased in
    /// the translated view but left untouched by apply-then-get — a
    /// value corruption with unchanged node count and anchors.
    fn value_corrupting_lens() -> crate::Lens {
        use panproto_expr::{BuiltinOp, Expr};
        use panproto_inst::{CompiledMigration, FieldTransform};
        use panproto_schema::Edge;
        use std::collections::{HashMap, HashSet};
        use std::sync::Arc;

        let schema = three_node_schema();
        let surviving_verts: HashSet<Name> = schema.vertices.keys().cloned().collect();
        let surviving_edges: HashSet<Edge> = schema.edges.keys().cloned().collect();

        let mut field_transforms: HashMap<Name, Vec<FieldTransform>> = HashMap::new();
        field_transforms.insert(
            Name::from("post:body.text"),
            vec![FieldTransform::ApplyExpr {
                key: "tag".to_owned(),
                expr: Expr::Builtin(BuiltinOp::Upper, vec![Expr::Var(Arc::from("tag"))]),
                inverse: None,
                coercion_class: panproto_gat::CoercionClass::Retraction,
            }],
        );

        crate::Lens {
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
        }
    }

    #[test]
    fn consistency_detects_value_corruption() {
        let lens = value_corrupting_lens();
        let instance = three_node_instance();
        let mut edit_lens = EditLens::from_lens(lens, test_protocol());
        edit_lens.initialize(&instance).unwrap();

        // `tag` set on node 2 (anchor `post:body.createdAt`). The
        // transform is keyed under `post:body.text`, so apply-then-get
        // leaves `tag` verbatim while the edit translation upper-cases
        // it. Node count and anchors are unchanged, so only the
        // structural value comparison catches the divergence.
        let edit = TreeEdit::SetField {
            node_id: 2,
            field: Name::from("tag"),
            value: Value::Str("hello".into()),
        };

        let result = check_edit_consistency(&mut edit_lens, &edit, &instance);
        assert!(
            matches!(result, Err(EditLawViolation::Consistency { .. })),
            "value corruption must be flagged as a consistency violation: {result:?}"
        );
    }

    #[test]
    fn coherence_detects_content_drift() {
        use crate::asymmetric::Complement;
        use crate::laws::{complement_divergence, complements_equivalent};
        use panproto_inst::Node;

        // Both complements drop exactly one node under id 5, so their
        // dropped-node counts are equal by construction; only the dropped
        // node's content (its anchor) differs — the count-only check would
        // pass this.
        let mut c1 = Complement::empty();
        c1.dropped_nodes.insert(5, Node::new(5, "alpha"));
        let mut c2 = Complement::empty();
        c2.dropped_nodes.insert(5, Node::new(5, "beta"));

        assert!(
            !complements_equivalent(&c1, &c2),
            "complements differing only in dropped-node content must not compare equal"
        );
        let detail = complement_divergence(&c1, &c2).unwrap();
        assert!(
            detail.contains("dropped_nodes"),
            "divergence detail should name the diverging field: {detail}"
        );
    }
}

/// Property tests for the `TreeEdit` partial-monoid action.
///
/// [`TreeEdit`] declares the partial monoid-action law
/// `apply(compose(e1, e2), s) = apply(e2, apply(e1, s))` and the identity
/// laws `apply(compose(id, e), s) = apply(e, s) = apply(compose(e, id), s)`.
/// This module exercises those laws — and the associativity of `compose`
/// under the action — over *generated* edit words and instances, comparing
/// results with the full structural comparator
/// [`crate::laws::instances_equivalent`] (nodes with values and
/// `extra_fields`, order-independent arcs, `parent_map`, and fans) rather
/// than by node counts and anchors.
///
/// Because `apply` is a *partial* action, each property compares the
/// success/failure outcome of the two evaluation paths, not only the
/// success path.
#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod action_laws {
    use panproto_gat::Name;
    use panproto_inst::value::{FieldPresence, Value};
    use panproto_inst::{Fan, Node, TreeEdit, WInstance};
    use panproto_schema::Edge;
    use proptest::prelude::*;
    use std::collections::HashMap;

    use crate::laws::instances_equivalent;
    use panproto_inst::EditError;

    /// Build a `prop`-kind edge with matching label.
    fn edge(src: &str, tgt: &str, label: &str) -> Edge {
        Edge {
            src: Name::from(src),
            tgt: Name::from(tgt),
            kind: Name::from("prop"),
            name: Some(Name::from(label)),
        }
    }

    /// Build a base instance: `root → branch → leaf` (a grandchild), plus
    /// `extra` leaves directly under the root, and optionally a fan.
    fn build_base(extra: usize, with_fan: bool) -> WInstance {
        let mut nodes = HashMap::new();
        nodes.insert(0, Node::new(0, "root"));
        nodes.insert(1, Node::new(1, "branch"));
        let mut leaf =
            Node::new(2, "leaf").with_value(FieldPresence::Present(Value::Str("g".to_owned())));
        leaf.extra_fields.insert("f".to_owned(), Value::Int(1));
        nodes.insert(2, leaf);

        let mut arcs = vec![
            (0, 1, edge("root", "branch", "branch")),
            (1, 2, edge("branch", "leaf", "leaf")),
        ];
        for i in 0..extra {
            let id = u32::try_from(3 + i).unwrap();
            let name = format!("extra{i}");
            nodes.insert(
                id,
                Node::new(id, name.as_str())
                    .with_value(FieldPresence::Present(Value::Str("v".to_owned()))),
            );
            arcs.push((0, id, edge("root", name.as_str(), name.as_str())));
        }
        let fans = if with_fan {
            vec![Fan::new("he", 0).with_child("a", 1).with_child("b", 2)]
        } else {
            Vec::new()
        };
        WInstance::new(nodes, arcs, fans, 0, Name::from("root"))
    }

    /// Generate a base instance with a small, varied shape.
    fn arb_base_instance() -> impl Strategy<Value = WInstance> {
        (0usize..=3, any::<bool>()).prop_map(|(extra, with_fan)| build_base(extra, with_fan))
    }

    /// Generate a single non-`Sequence`, non-`JoinFeatures` `TreeEdit`
    /// referencing existing node ids (and fresh ids for insertion).
    fn arb_simple_edit(ids: &[u32]) -> impl Strategy<Value = TreeEdit> + use<> {
        prop_oneof![
            1 => Just(TreeEdit::Identity),
            3 => (prop::sample::select(ids.to_vec()), 1000u32..5000u32).prop_map(
                |(parent, fresh)| TreeEdit::InsertNode {
                    parent,
                    child_id: fresh,
                    node: Node::new(fresh, "inserted"),
                    edge: edge("inserted_src", "inserted", "ins"),
                }
            ),
            2 => prop::sample::select(ids.to_vec())
                .prop_map(|id| TreeEdit::DeleteNode { id }),
            2 => prop::sample::select(ids.to_vec())
                .prop_map(|id| TreeEdit::ContractNode { id }),
            2 => prop::sample::select(ids.to_vec())
                .prop_map(|id| TreeEdit::RelabelNode { id, new_anchor: Name::from("relabeled") }),
            3 => (prop::sample::select(ids.to_vec()), "[a-z]{1,4}", any::<i64>()).prop_map(
                |(id, field, v)| TreeEdit::SetField {
                    node_id: id,
                    field: Name::from(field.as_str()),
                    value: Value::Int(v),
                }
            ),
            2 => (prop::sample::select(ids.to_vec()), "[a-z]{1,4}").prop_map(
                |(id, field)| TreeEdit::RemoveField {
                    node_id: id,
                    field: Name::from(field.as_str()),
                }
            ),
            2 => (prop::sample::select(ids.to_vec()), prop::sample::select(ids.to_vec())).prop_map(
                |(node_id, new_parent)| TreeEdit::MoveSubtree {
                    node_id,
                    new_parent,
                    edge: edge("root", "inserted", "moved"),
                }
            ),
            1 => (prop::sample::select(ids.to_vec()), prop::sample::select(ids.to_vec())).prop_map(
                |(parent, child)| TreeEdit::InsertFan {
                    fan: Fan::new("fan_new", parent).with_child("x", child),
                }
            ),
            1 => Just(TreeEdit::DeleteFan { hyper_edge_id: Name::from("he") }),
        ]
    }

    /// Generate a `TreeEdit`, occasionally wrapping two simple edits in a
    /// `Sequence` so that variant is covered directly.
    fn arb_edit(ids: &[u32]) -> impl Strategy<Value = TreeEdit> + use<> {
        prop_oneof![
            6 => arb_simple_edit(ids),
            1 => (arb_simple_edit(ids), arb_simple_edit(ids))
                .prop_map(|(a, b)| TreeEdit::Sequence(vec![a, b])),
        ]
    }

    /// Generate `(instance, e1, e2, e3)` with edits scoped to the
    /// instance's node ids.
    fn arb_scenario() -> impl Strategy<Value = (WInstance, TreeEdit, TreeEdit, TreeEdit)> {
        arb_base_instance().prop_flat_map(|inst| {
            let mut ids: Vec<u32> = inst.nodes.keys().copied().collect();
            ids.sort_unstable();
            (Just(inst), arb_edit(&ids), arb_edit(&ids), arb_edit(&ids))
        })
    }

    /// Apply an edit to a fresh clone of `s`.
    fn apply_to_clone(edit: &TreeEdit, s: &WInstance) -> Result<WInstance, EditError> {
        let mut st = s.clone();
        edit.apply(&mut st)?;
        Ok(st)
    }

    /// Apply `e1` then `e2` sequentially to a fresh clone of `s`
    /// (`apply(e2, apply(e1, s))`).
    fn apply_sequential(
        e1: &TreeEdit,
        e2: &TreeEdit,
        s: &WInstance,
    ) -> Result<WInstance, EditError> {
        let mut st = s.clone();
        e1.apply(&mut st)?;
        e2.apply(&mut st)?;
        Ok(st)
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(24))]

        /// Monoid-action coherence:
        /// `apply(compose(e1, e2), s)` succeeds iff `apply(e2, apply(e1, s))`
        /// succeeds, and the two agree structurally when both succeed.
        #[test]
        fn action_coherence(
            (s, e1, e2, _e3) in arb_scenario()
        ) {
            let composed = apply_to_clone(&e1.clone().compose(e2.clone()), &s);
            let sequential = apply_sequential(&e1, &e2, &s);
            prop_assert_eq!(
                composed.is_ok(),
                sequential.is_ok(),
                "compose/sequential outcome disagreement",
            );
            if let (Ok(a), Ok(b)) = (&composed, &sequential) {
                prop_assert!(
                    instances_equivalent(a, b),
                    "compose and sequential application diverged structurally",
                );
            }
        }

        /// Identity action: `compose(id, e)` and `compose(e, id)` act on the
        /// view exactly as `e` does (same outcome, same structure).
        #[test]
        fn identity_action(
            (s, e, _e2, _e3) in arb_scenario()
        ) {
            let direct = apply_to_clone(&e, &s);
            let left = apply_to_clone(&TreeEdit::identity().compose(e.clone()), &s);
            let right = apply_to_clone(&e.compose(TreeEdit::identity()), &s);

            prop_assert_eq!(direct.is_ok(), left.is_ok(), "left-identity outcome");
            prop_assert_eq!(direct.is_ok(), right.is_ok(), "right-identity outcome");
            if let (Ok(d), Ok(l)) = (&direct, &left) {
                prop_assert!(instances_equivalent(d, l), "left identity diverged");
            }
            if let (Ok(d), Ok(r)) = (&direct, &right) {
                prop_assert!(instances_equivalent(d, r), "right identity diverged");
            }
        }

        /// Associativity of `compose` under the action:
        /// `(e1 . e2) . e3` and `e1 . (e2 . e3)` act identically.
        #[test]
        fn compose_associativity(
            (s, e1, e2, e3) in arb_scenario()
        ) {
            let left = e1.clone().compose(e2.clone()).compose(e3.clone());
            let right = e1.compose(e2.compose(e3));
            let rl = apply_to_clone(&left, &s);
            let rr = apply_to_clone(&right, &s);
            prop_assert_eq!(rl.is_ok(), rr.is_ok(), "associativity outcome disagreement");
            if let (Ok(a), Ok(b)) = (&rl, &rr) {
                prop_assert!(
                    instances_equivalent(a, b),
                    "left- and right-associated composites diverged",
                );
            }
        }
    }
}

/// `get_edit` functoriality (Johnson-Rosebrugh delta-lens
/// formulation).
///
/// In the delta-lens view, `get_edit` is the action of a functor between
/// edit categories, so it must preserve identity and composition. Over a
/// generated word of source edits `[e1, …, en]`:
///
/// - **Composition**: translating the composed word `e1 · … · en` in one
///   call must act on the view exactly as translating each `eᵢ` in order
///   and threading the complement, and must leave the *same* complement.
/// - **Identity**: `get_edit(Identity)` yields an edit that leaves both
///   the view and the complement unchanged.
///
/// Both properties compare the full instance structure via
/// [`crate::laws::instances_equivalent`] and the full complement structure
/// via [`crate::laws::complements_equivalent`], not counts, and are run
/// over both an identity lens and a drop-last-child projection lens.
#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod functor_laws {
    use panproto_gat::Name;
    use panproto_inst::{TreeEdit, Value, WInstance};
    use panproto_schema::Protocol;
    use proptest::prelude::*;

    use crate::Lens;
    use crate::edit_lens::EditLens;
    use crate::laws::{complements_equivalent, instances_equivalent};
    use crate::tests::{identity_lens, projection_lens, three_node_instance, three_node_schema};

    use super::clone_edit_lens;

    fn test_protocol() -> Protocol {
        Protocol {
            name: "test".into(),
            schema_theory: "ThTest".into(),
            instance_theory: "ThWType".into(),
            schema_composition: None,
            instance_composition: None,
            edge_rules: vec![],
            obj_kinds: vec![],
            constraint_sorts: vec![],
            has_order: false,
            has_coproducts: false,
            has_recursion: false,
            has_causal: false,
            nominal_identity: false,
            has_defaults: false,
            has_coercions: false,
            has_mergers: false,
            has_policies: false,
        }
    }

    /// A single source edit applicable to the three-node instance:
    /// `SetField`/`RemoveField` on one of node ids `{0, 1, 2}`. These
    /// always translate (surviving nodes → view edits; dropped nodes →
    /// complement updates absorbed to `Identity`) and always apply.
    fn arb_source_edit() -> impl Strategy<Value = TreeEdit> {
        prop_oneof![
            (0u32..=2, "[a-z]{1,5}", "[a-z]{1,6}").prop_map(|(id, field, v)| TreeEdit::SetField {
                node_id: id,
                field: Name::from(field.as_str()),
                value: Value::Str(v),
            }),
            (0u32..=2, "[a-z]{1,5}").prop_map(|(id, field)| TreeEdit::RemoveField {
                node_id: id,
                field: Name::from(field.as_str()),
            }),
        ]
    }

    /// A word of 1-5 applicable source edits.
    fn arb_source_word() -> impl Strategy<Value = Vec<TreeEdit>> {
        prop::collection::vec(arb_source_edit(), 1..=5)
    }

    /// Fold a slice of edits into a single composed edit.
    fn compose_all(edits: impl IntoIterator<Item = TreeEdit>) -> TreeEdit {
        edits
            .into_iter()
            .reduce(TreeEdit::compose)
            .unwrap_or(TreeEdit::Identity)
    }

    fn init_edit_lens(lens: Lens, source: &WInstance) -> EditLens {
        let mut el = EditLens::from_lens(lens, test_protocol());
        el.initialize(source).unwrap();
        el
    }

    fn state_lens(el: &EditLens) -> Lens {
        Lens {
            compiled: el.compiled.clone(),
            src_schema: el.src_schema.clone(),
            tgt_schema: el.tgt_schema.clone(),
        }
    }

    /// The composition half of functoriality.
    fn check_composition(
        lens: Lens,
        source: &WInstance,
        word: &[TreeEdit],
    ) -> Result<(), TestCaseError> {
        let base = init_edit_lens(lens, source);
        let (view0, _) = crate::get(&state_lens(&base), source).unwrap();

        // Copy A: translate the composed word in one call.
        let mut copy_a = clone_edit_lens(&base);
        let view_edit_a = copy_a.get_edit(compose_all(word.iter().cloned())).unwrap();
        let mut view_a = view0.clone();
        view_edit_a.apply(&mut view_a).unwrap();

        // Copy B: translate each edit in order, threading the complement.
        let mut copy_b = clone_edit_lens(&base);
        let mut translated_b = Vec::with_capacity(word.len());
        for e in word {
            translated_b.push(copy_b.get_edit(e.clone()).unwrap());
        }
        let mut view_b = view0;
        compose_all(translated_b).apply(&mut view_b).unwrap();

        prop_assert!(
            instances_equivalent(&view_a, &view_b),
            "functor composition: view edits diverged",
        );
        prop_assert!(
            complements_equivalent(&copy_a.complement, &copy_b.complement),
            "functor composition: threaded complements diverged",
        );
        Ok(())
    }

    /// The identity half of functoriality.
    fn check_identity(lens: Lens, source: &WInstance) -> Result<(), TestCaseError> {
        let base = init_edit_lens(lens, source);
        let (view0, _) = crate::get(&state_lens(&base), source).unwrap();
        let before = base.complement.clone();

        let mut copy = clone_edit_lens(&base);
        let translated = copy.get_edit(TreeEdit::Identity).unwrap();
        let mut view = view0.clone();
        translated.apply(&mut view).unwrap();

        prop_assert!(
            instances_equivalent(&view0, &view),
            "identity edit changed the view",
        );
        prop_assert!(
            complements_equivalent(&before, &copy.complement),
            "identity edit changed the complement",
        );
        Ok(())
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(128))]

        #[test]
        fn functor_preserves_composition_identity(word in arb_source_word()) {
            let schema = three_node_schema();
            check_composition(identity_lens(&schema), &three_node_instance(), &word)?;
        }

        #[test]
        fn functor_preserves_composition_projection(word in arb_source_word()) {
            let schema = three_node_schema();
            check_composition(
                projection_lens(&schema, "createdAt"),
                &three_node_instance(),
                &word,
            )?;
        }
    }

    #[test]
    fn functor_preserves_identity_identity() {
        let schema = three_node_schema();
        check_identity(identity_lens(&schema), &three_node_instance()).unwrap();
    }

    #[test]
    fn functor_preserves_identity_projection() {
        let schema = three_node_schema();
        check_identity(
            projection_lens(&schema, "createdAt"),
            &three_node_instance(),
        )
        .unwrap();
    }
}
