//! Edit lens law verification.
//!
//! Checks successful raw edit translation against its whole-state endpoint:
//!
//! - **Consistency**: translating a source edit and applying it to the
//!   view gives the same result as applying the edit to the source and
//!   then doing a whole-state `get`.
//! - **Complement coherence**: the complement state after `get_edit` is
//!   equal to fresh whole-state get except for a retained historical floor,
//!   which must not decrease. Both checkers require an initialized lens
//!   whose tracked source is exactly the supplied source.

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
/// to the source and then doing a whole-state `get`. The lens must be
/// initialized and its tracked source must exactly match `source`, including
/// node metadata and arc/fan order. This checker leaves the lens unchanged.
///
/// # Errors
///
/// Returns [`EditLawViolation`] if the law is violated or an error occurs.
pub fn check_edit_consistency(
    lens: &mut EditLens,
    edit: &TreeEdit,
    source: &WInstance,
) -> Result<(), EditLawViolation> {
    check_tracked_source(lens, source)?;
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
        .map_err(|e| EditLawViolation::Error(crate::edit_lens::whole_state_error(e)))?;
    view_edit
        .apply(&mut view)
        .map_err(|e| EditLawViolation::Error(EditLensError::EditApply(e)))?;

    // Path 2: apply edit to source, then whole-state get.
    let mut edited_source = source.clone();
    edit.apply(&mut edited_source)
        .map_err(|e| EditLawViolation::Error(EditLensError::EditApply(e)))?;
    let (view2, _) = crate::get(&state_lens, &edited_source)
        .map_err(|e| EditLawViolation::Error(crate::edit_lens::whole_state_error(e)))?;

    // Compare full node payloads, roots, and ordered arcs and fans.
    if !crate::laws::tree_identical(&view, &view2) {
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
/// complement from whole-state get of the edited source, with every
/// non-floor coordinate equal. The retained floor must be at least the
/// incoming floor and fresh get's floor. The lens must be initialized with
/// tracked source exactly matching `source`; this checker leaves it unchanged.
///
/// # Errors
///
/// Returns [`EditLawViolation`] if the law is violated or an error occurs.
pub fn check_complement_coherence(
    lens: &mut EditLens,
    edit: &TreeEdit,
    source: &WInstance,
) -> Result<(), EditLawViolation> {
    check_tracked_source(lens, source)?;
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
        .map_err(|e| EditLawViolation::Error(crate::edit_lens::whole_state_error(e)))?;

    if let Some(detail) = retained_complement_divergence(
        &lens_clone.complement,
        &complement2,
        lens.complement.id_floor,
    ) {
        return Err(EditLawViolation::ComplementCoherence { detail });
    }

    Ok(())
}

fn check_tracked_source(lens: &EditLens, source: &WInstance) -> Result<(), EditLawViolation> {
    if lens
        .pipeline
        .as_ref()
        .is_some_and(|pipeline| crate::laws::tree_identical(pipeline.source(), source))
    {
        Ok(())
    } else {
        Err(EditLawViolation::Error(EditLensError::TranslationFailed(
            "law checking requires an initialized lens with the supplied tracked source".to_owned(),
        )))
    }
}

// Extend the legacy residue comparator with each coordinate or ordering it
// intentionally omits. Only the independently checked historical floor varies.
fn retained_complement_divergence(
    actual: &crate::Complement,
    fresh: &crate::Complement,
    incoming_floor: u32,
) -> Option<String> {
    if actual.id_floor < incoming_floor.max(fresh.id_floor) {
        return Some("id_floor is below the incoming or whole-state floor".to_owned());
    }
    if actual.dropped_nodes.len() != fresh.dropped_nodes.len()
        || actual.dropped_nodes.iter().any(|(id, node)| {
            !fresh
                .dropped_nodes
                .get(id)
                .is_some_and(|other| crate::asymmetric::node_equiv(node, other))
        })
    {
        return Some("dropped_nodes differ in full payload".to_owned());
    }
    if actual.dropped_arcs != fresh.dropped_arcs {
        return Some("dropped_arcs differ in payload or order".to_owned());
    }
    if actual.dropped_fans != fresh.dropped_fans {
        return Some("dropped_fans differ in payload or order".to_owned());
    }
    if actual.synthesized_keys != fresh.synthesized_keys {
        return Some("synthesized_keys differ".to_owned());
    }
    if actual.view_positions != fresh.view_positions {
        return Some("view_positions differ".to_owned());
    }
    crate::laws::complement_divergence(actual, fresh)
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

    #[test]
    fn edit_law_checkers_require_the_matching_initialized_source() {
        let schema = three_node_schema();
        let source = three_node_instance();
        let mut lens = EditLens::from_lens(identity_lens(&schema), test_protocol());
        assert!(check_edit_consistency(&mut lens, &TreeEdit::Identity, &source).is_err());
        assert!(check_complement_coherence(&mut lens, &TreeEdit::Identity, &source).is_err());
        lens.initialize(&source).unwrap();
        let mut different = source.clone();
        different
            .nodes
            .get_mut(&1)
            .unwrap()
            .annotations
            .insert("changed".into(), Value::Int(1));
        assert!(check_edit_consistency(&mut lens, &TreeEdit::Identity, &different).is_err());
        assert!(check_complement_coherence(&mut lens, &TreeEdit::Identity, &different).is_err());
        lens.complement.id_floor = 100;
        assert!(check_edit_consistency(&mut lens, &TreeEdit::Identity, &source).is_ok());
        assert!(check_complement_coherence(&mut lens, &TreeEdit::Identity, &source).is_ok());
    }

    #[test]
    fn retained_complement_comparison_checks_floor_metadata_and_bookkeeping() {
        let schema = three_node_schema();
        let source = three_node_instance();
        let lens = crate::tests::projection_lens(&schema, "createdAt");
        let (_, fresh) = crate::get(&lens, &source).unwrap();
        let mut retained = fresh.clone();
        retained.id_floor = 100;
        assert!(retained_complement_divergence(&retained, &fresh, 99).is_none());
        assert!(retained_complement_divergence(&retained, &fresh, 101).is_some());
        let mut changed = retained.clone();
        changed
            .dropped_nodes
            .get_mut(&2)
            .unwrap()
            .annotations
            .insert("changed".into(), Value::Int(1));
        assert!(
            retained_complement_divergence(&changed, &fresh, 99)
                .unwrap()
                .contains("dropped_nodes")
        );
        let mut changed = retained.clone();
        changed.view_positions.clear();
        assert!(!fresh.view_positions.is_empty());
        assert!(
            retained_complement_divergence(&changed, &fresh, 99)
                .unwrap()
                .contains("view_positions")
        );
        let mut changed = retained;
        changed.id_floor = 0;
        assert!(
            retained_complement_divergence(&changed, &fresh, 0)
                .unwrap()
                .contains("id_floor")
        );
    }

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
            ..Default::default()
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

    /// Relabelling a live node onto an anchor the migration drops removes it
    /// from the view, so its payload has to land in the complement: otherwise
    /// it is gone from both sides and `c' = get(es · source).1` fails.
    #[test]
    fn relabel_to_a_non_surviving_anchor_keeps_the_payload() {
        let schema = three_node_schema();
        let lens = crate::tests::projection_lens(&schema, "createdAt");
        let instance = three_node_instance();
        let mut edit_lens = EditLens::from_lens(lens, test_protocol());
        edit_lens.initialize(&instance).unwrap();

        // Node 1 is live in the view; `post:body.createdAt` is not in it.
        let edit = TreeEdit::RelabelNode {
            id: 1,
            new_anchor: Name::from("post:body.createdAt"),
        };

        let result = check_complement_coherence(&mut edit_lens, &edit, &instance);
        assert!(
            result.is_ok(),
            "coherence must hold for a relabel out of the view: {result:?}"
        );

        let translated = edit_lens.get_edit(edit).unwrap();
        assert!(
            matches!(translated, TreeEdit::DeleteNode { id: 1 }),
            "the view sees the node leave, got {translated:?}"
        );
        let kept = edit_lens.complement.dropped_nodes.get(&1).unwrap();
        assert_eq!(
            kept.value, instance.nodes[&1].value,
            "the dropped node keeps the payload it had in the source",
        );
        assert_eq!(&*kept.anchor, "post:body.createdAt");
    }

    /// A field transform attached only to the text anchor. The regression
    /// checks that an identically named field at another anchor is unchanged.
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
    fn consistency_preserves_anchor_scoped_field_transforms() {
        let mut lens = value_corrupting_lens();
        let instance = three_node_instance();
        assert!(matches!(
            crate::get(&lens, &instance),
            Err(crate::LensError::Restrict(
                panproto_inst::RestrictError::UndeclaredTransformField {
                    source_vertex,
                    target_vertex,
                    field,
                }
            )) if source_vertex == "post:body.text"
                && target_vertex == "post:body.text"
                && field == "tag"
        ));

        let edge = panproto_schema::Edge {
            src: "post:body.text".into(),
            tgt: "post:body.text.tag".into(),
            kind: "prop".into(),
            name: Some("tag".into()),
        };
        // The declared `tag` is a string leaf like `post:body.text` itself.
        let mut tag_vertex = lens.tgt_schema.vertices[&edge.src].clone();
        tag_vertex.id = edge.tgt.clone();
        lens.tgt_schema
            .vertices
            .insert(edge.tgt.clone(), tag_vertex);
        lens.tgt_schema
            .edges
            .insert(edge.clone(), edge.kind.clone());
        lens.tgt_schema
            .outgoing
            .entry(edge.src.clone())
            .or_default()
            .push(edge.clone());
        lens.tgt_schema
            .incoming
            .entry(edge.tgt.clone())
            .or_default()
            .push(edge.clone());
        lens.tgt_schema
            .between
            .entry((edge.src.clone(), edge.tgt.clone()))
            .or_default()
            .push(edge);
        let mut edit_lens = EditLens::from_lens(lens, test_protocol());
        edit_lens.initialize(&instance).unwrap();

        // Node 2 has another anchor, so the text transform must not apply.
        let edit = TreeEdit::SetField {
            node_id: 2,
            field: Name::from("tag"),
            value: Value::Str("hello".into()),
        };

        let result = check_edit_consistency(&mut edit_lens, &edit, &instance);
        assert!(
            result.is_ok(),
            "an unrelated anchor must retain its field value: {result:?}"
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
/// results with strict tree identity, including full node metadata and
/// complete arc and fan order.
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

    use crate::laws::tree_identical;
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
                    tree_identical(a, b),
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
                prop_assert!(tree_identical(d, l), "left identity diverged");
            }
            if let (Ok(d), Ok(r)) = (&direct, &right) {
                prop_assert!(tree_identical(d, r), "right identity diverged");
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
                    tree_identical(a, b),
                    "left- and right-associated composites diverged",
                );
            }
        }
    }
}

/// Endpoint and threaded-complement checks for generated field-edit words
/// under identity and static projection. These tests do not establish
/// general strict composition of canonical deltas or prefix-closed admission.
#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod functor_laws {
    use panproto_gat::Name;
    use panproto_inst::{TreeEdit, Value, WInstance};
    use panproto_schema::Protocol;
    use proptest::prelude::*;

    use super::retained_complement_divergence;
    use crate::Lens;
    use crate::edit_lens::EditLens;
    use crate::laws::tree_identical;
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
            ..Default::default()
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
            tree_identical(&view_a, &view_b),
            "functor composition: view edits diverged",
        );
        prop_assert!(
            copy_a.complement.id_floor == copy_b.complement.id_floor
                && retained_complement_divergence(
                    &copy_a.complement,
                    &copy_b.complement,
                    base.complement.id_floor,
                )
                .is_none(),
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
            tree_identical(&view0, &view),
            "identity edit changed the view",
        );
        prop_assert!(
            before.id_floor == copy.complement.id_floor
                && retained_complement_divergence(&copy.complement, &before, before.id_floor)
                    .is_none(),
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
