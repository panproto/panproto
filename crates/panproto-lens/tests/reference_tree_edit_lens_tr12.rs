//! Differential closure for DESIGN TR12 and E.6 laws (6), (10), and (11).
//!
//! The oracle is the independent finite leaf-tree carrier in
//! `panproto-reference`. Production `TreeEdit` words are lowered through
//! `GraphDelta`, because `TreeEdit` is a free sequence: it neither records
//! delete payloads nor exposes inversion or replacement normalisation.

#![allow(clippy::expect_used, clippy::too_many_lines, clippy::unwrap_used)]

use std::collections::{BTreeMap, BTreeSet, HashMap};

use panproto_inst::value::{FieldPresence, Value};
use panproto_inst::{
    CompiledMigration, EditError, GInstance, GraphDelta, Node, TreeEdit, TreeEmbedding, WInstance,
};
use panproto_lens::{EditLens, Lens, LensShape, TreeShape};
use panproto_reference::pages::restrict_and_contraction::tree_edit_lens as reference;
use panproto_reference::pages::restrict_and_contraction::tree_edit_lens::{
    Atom, Change, Delta, Leaf, Refusal, Visibility,
};
use panproto_schema::{Edge, Protocol, Schema, SchemaBuilder};

const ROOT: u32 = 0;
const KEPT: u32 = 1;
const HIDDEN: u32 = 2;
const FRESH: u32 = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LensKind {
    Identity,
    Projection,
}

#[derive(Clone, Debug)]
struct Plan {
    name: String,
    atoms: Vec<Atom>,
}

#[derive(Debug)]
struct Lowered {
    delta: GraphDelta,
    tree: WInstance,
    embedding: TreeEmbedding,
}

fn edge(visibility: Visibility) -> Edge {
    let target = match visibility {
        Visibility::Visible => "visible",
        Visibility::Hidden => "hidden",
    };
    Edge {
        src: "root".into(),
        tgt: target.into(),
        kind: "prop".into(),
        name: Some(target.into()),
    }
}

const fn leaf(id: u32, visibility: Visibility, payload: u8) -> Leaf {
    Leaf {
        id,
        parent: ROOT,
        visibility,
        payload,
    }
}

fn production_node(leaf: &Leaf) -> Node {
    let anchor = match leaf.visibility {
        Visibility::Visible => "visible",
        Visibility::Hidden => "hidden",
    };
    Node::new(leaf.id, anchor)
        .with_value(FieldPresence::Present(Value::Int(i64::from(leaf.payload))))
}

fn reference_leaf(node: &Node, parent: u32) -> Leaf {
    let visibility = match node.anchor.as_ref() {
        "visible" => Visibility::Visible,
        "hidden" => Visibility::Hidden,
        other => panic!("unexpected leaf anchor {other}"),
    };
    let payload = match &node.value {
        Some(FieldPresence::Present(Value::Int(value))) => u8::try_from(*value).unwrap(),
        other => panic!("unexpected leaf payload {other:?}"),
    };
    Leaf {
        id: node.id,
        parent,
        visibility,
        payload,
    }
}

fn reference_source() -> reference::Tree {
    reference::Tree::new([
        leaf(KEPT, Visibility::Visible, 0),
        leaf(HIDDEN, Visibility::Hidden, 0),
    ])
    .unwrap()
}

fn production_source() -> WInstance {
    let kept = leaf(KEPT, Visibility::Visible, 0);
    let hidden = leaf(HIDDEN, Visibility::Hidden, 0);
    WInstance::new(
        HashMap::from([
            (ROOT, Node::new(ROOT, "root")),
            (KEPT, production_node(&kept)),
            (HIDDEN, production_node(&hidden)),
        ]),
        vec![
            (ROOT, KEPT, edge(Visibility::Visible)),
            (ROOT, HIDDEN, edge(Visibility::Hidden)),
        ],
        Vec::new(),
        ROOT,
        "root".into(),
    )
}

fn reference_tree(tree: &WInstance) -> reference::Tree {
    reference::Tree::new(
        tree.nodes
            .values()
            .filter(|node| node.id != ROOT)
            .map(|node| reference_leaf(node, ROOT)),
    )
    .unwrap()
}

fn schema(include_hidden: bool) -> Schema {
    let protocol = Protocol::default();
    let mut builder = SchemaBuilder::new(&protocol)
        .vertex("root", "record", None::<&str>)
        .unwrap()
        .vertex("visible", "scalar", None::<&str>)
        .unwrap()
        .edge("root", "visible", "prop", Some("visible"))
        .unwrap();
    if include_hidden {
        builder = builder
            .vertex("hidden", "scalar", None::<&str>)
            .unwrap()
            .edge("root", "hidden", "prop", Some("hidden"))
            .unwrap();
    }
    builder.entry("root").build().unwrap()
}

fn production_lens(kind: LensKind) -> Lens {
    let source = schema(true);
    let target = schema(kind == LensKind::Identity);
    let compiled = CompiledMigration {
        surviving_verts: target.vertices.keys().cloned().collect(),
        surviving_edges: target.edges.keys().cloned().collect(),
        ..CompiledMigration::default()
    };
    Lens {
        compiled,
        src_schema: source,
        tgt_schema: target,
    }
}

const fn reference_lens(kind: LensKind) -> reference::Lens {
    match kind {
        LensKind::Identity => reference::Lens::Identity,
        LensKind::Projection => reference::Lens::Projection,
    }
}

fn edit_lens(kind: LensKind, source: &WInstance) -> EditLens {
    let mut lens = EditLens::from_lens(production_lens(kind), Protocol::default());
    lens.initialize(source).unwrap();
    lens
}

fn initial_embedding(tree: &WInstance) -> TreeEmbedding {
    GInstance::from_wtype_embedded(tree, None).unwrap().1
}

fn lower(
    tree: &WInstance,
    embedding: &TreeEmbedding,
    edit: &TreeEdit,
) -> Result<Lowered, EditError> {
    let graph = GInstance::from_wtype_under(tree, embedding, None)
        .map_err(|error| EditError::ApplyFailed(error.to_string()))?;
    let mut next_embedding = embedding.clone();
    let delta = GraphDelta::from_tree_edit(&graph, &mut next_embedding, edit)?;
    let mut next_tree = tree.clone();
    edit.apply(&mut next_tree)?;
    Ok(Lowered {
        delta,
        tree: next_tree,
        embedding: next_embedding,
    })
}

fn production_edit(atoms: &[Atom]) -> TreeEdit {
    atoms
        .iter()
        .map(|atom| match atom {
            Atom::Insert(leaf) => TreeEdit::InsertNode {
                parent: leaf.parent,
                child_id: leaf.id,
                node: production_node(leaf),
                edge: edge(leaf.visibility),
            },
            Atom::Delete { id } => TreeEdit::DeleteNode { id: *id },
        })
        .reduce(TreeEdit::compose)
        .unwrap_or(TreeEdit::Identity)
}

fn carrier(delta: &GraphDelta) -> Delta {
    let ids: BTreeSet<u32> = delta
        .del_nodes()
        .keys()
        .chain(delta.add_nodes().keys())
        .copied()
        .filter(|id| *id != ROOT)
        .collect();
    let mut out = Delta::identity();
    for id in ids {
        let change = match (delta.del_nodes().get(&id), delta.add_nodes().get(&id)) {
            (None, Some(new)) => Change::Add(reference_leaf(new, ROOT)),
            (Some(old), None) => Change::Delete(reference_leaf(old, ROOT)),
            (Some(old), Some(new)) => Change::Replace {
                old: reference_leaf(old, ROOT),
                new: reference_leaf(new, ROOT),
            },
            (None, None) => unreachable!(),
        };
        out = reference::Delta::compose(&out, &Delta::singleton(change)).unwrap();
    }
    assert!(
        delta.attrs().is_empty(),
        "the structural carrier generated no attribute edits: {delta:?}"
    );
    out
}

fn complement_carrier(complement: &panproto_inst::Complement) -> BTreeMap<u32, Leaf> {
    complement
        .dropped_nodes
        .iter()
        .map(|(&id, node)| {
            let parent = complement
                .dropped_arcs
                .iter()
                .find_map(|(parent, child, _)| (*child == id).then_some(*parent))
                .or_else(|| complement.original_parent.get(&id).copied())
                .expect("every hidden non-root leaf has a recorded parent");
            (id, reference_leaf(node, parent))
        })
        .collect()
}

fn all_new_leaves(id: u32) -> Vec<Leaf> {
    [Visibility::Visible, Visibility::Hidden]
        .into_iter()
        .flat_map(|visibility| (0_u8..=1).map(move |payload| leaf(id, visibility, payload)))
        .collect()
}

/// The full source-word scope has exactly 19 valid points:
/// 1 identity + 2 deletes + 4 inserts + 4 insert/delete cancellations +
/// 8 delete/reinsert replacements.
fn source_plans() -> Vec<Plan> {
    let mut plans = vec![Plan {
        name: "identity".to_owned(),
        atoms: Vec::new(),
    }];
    for id in [KEPT, HIDDEN] {
        plans.push(Plan {
            name: format!("delete-{id}"),
            atoms: vec![Atom::Delete { id }],
        });
    }
    for new in all_new_leaves(FRESH) {
        plans.push(Plan {
            name: format!("add-{:?}-{}", new.visibility, new.payload),
            atoms: vec![Atom::Insert(new.clone())],
        });
        plans.push(Plan {
            name: format!("add-delete-{:?}-{}", new.visibility, new.payload),
            atoms: vec![Atom::Insert(new), Atom::Delete { id: FRESH }],
        });
    }
    for id in [KEPT, HIDDEN] {
        for new in all_new_leaves(id) {
            plans.push(Plan {
                name: format!("delete-reinsert-{id}-{:?}-{}", new.visibility, new.payload),
                atoms: vec![Atom::Delete { id }, Atom::Insert(new)],
            });
        }
    }
    assert_eq!(plans.len(), 19, "finite source-word count drifted");
    assert_eq!(
        plans
            .iter()
            .map(|plan| plan.name.as_str())
            .collect::<BTreeSet<_>>()
            .len(),
        19,
        "finite source-word labels must be unique"
    );
    plans
}

/// The projection view has exactly eight admitted words: identity, one
/// delete, two visible inserts, their two cancellations, and two visible
/// replacements of the kept leaf.
fn projection_put_plans() -> Vec<Plan> {
    let mut plans = vec![
        Plan {
            name: "identity".to_owned(),
            atoms: Vec::new(),
        },
        Plan {
            name: "delete-kept".to_owned(),
            atoms: vec![Atom::Delete { id: KEPT }],
        },
    ];
    for payload in 0_u8..=1 {
        let fresh = leaf(FRESH, Visibility::Visible, payload);
        plans.push(Plan {
            name: format!("add-visible-{payload}"),
            atoms: vec![Atom::Insert(fresh.clone())],
        });
        plans.push(Plan {
            name: format!("add-delete-visible-{payload}"),
            atoms: vec![Atom::Insert(fresh), Atom::Delete { id: FRESH }],
        });
        plans.push(Plan {
            name: format!("replace-kept-visible-{payload}"),
            atoms: vec![
                Atom::Delete { id: KEPT },
                Atom::Insert(leaf(KEPT, Visibility::Visible, payload)),
            ],
        });
    }
    assert_eq!(plans.len(), 8, "finite projection-put count drifted");
    plans
}

fn put_plans(kind: LensKind) -> Vec<Plan> {
    match kind {
        LensKind::Identity => source_plans(),
        LensKind::Projection => projection_put_plans(),
    }
}

fn assert_production_minimal(source: &WInstance, input: &GraphDelta, output: &GraphDelta) {
    let named = input.touched_nodes();
    assert!(
        output
            .touched_nodes()
            .iter()
            .all(|id| { source.nodes.contains_key(id) || named.contains(id) })
    );
    for id in output
        .del_nodes()
        .keys()
        .filter(|id| output.add_nodes().contains_key(id))
    {
        assert!(
            input.del_nodes().contains_key(id) && input.add_nodes().contains_key(id),
            "output replacement {id} has no input replacement"
        );
    }
    assert!(
        output
            .attrs()
            .values()
            .all(|change| change.old != change.new)
    );
}

fn translated_get(
    kind: LensKind,
    source: &WInstance,
    view: &WInstance,
    view_embedding: &TreeEmbedding,
    plan: &Plan,
    sequential: bool,
) -> (GraphDelta, panproto_inst::Complement) {
    let mut lens = edit_lens(kind, source);
    if !sequential {
        let translated = lens.get_edit(production_edit(&plan.atoms)).unwrap();
        let lowered = lower(view, view_embedding, &translated).unwrap();
        return (lowered.delta, lens.complement);
    }

    let mut current = view.clone();
    let mut embedding = view_embedding.clone();
    let mut net = GraphDelta::identity();
    for atom in &plan.atoms {
        let translated = lens
            .get_edit(production_edit(std::slice::from_ref(atom)))
            .unwrap();
        let lowered = lower(&current, &embedding, &translated).unwrap();
        net = net.compose(&lowered.delta).unwrap();
        current = lowered.tree;
        embedding = lowered.embedding;
    }
    (net, lens.complement)
}

fn translated_put(
    kind: LensKind,
    source: &WInstance,
    source_embedding: &TreeEmbedding,
    plan: &Plan,
    sequential: bool,
) -> (Lowered, EditLens) {
    let mut lens = edit_lens(kind, source);
    if !sequential {
        let translated = lens.put_edit(production_edit(&plan.atoms)).unwrap();
        return (lower(source, source_embedding, &translated).unwrap(), lens);
    }

    let mut current = source.clone();
    let mut embedding = source_embedding.clone();
    let mut net = GraphDelta::identity();
    for atom in &plan.atoms {
        let translated = lens
            .put_edit(production_edit(std::slice::from_ref(atom)))
            .unwrap();
        let lowered = lower(&current, &embedding, &translated).unwrap();
        net = net.compose(&lowered.delta).unwrap();
        current = lowered.tree;
        embedding = lowered.embedding;
    }
    (
        Lowered {
            delta: net,
            tree: current,
            embedding,
        },
        lens,
    )
}

#[test]
fn tr12_get_functoriality_replacement_and_minimality_match_the_reference() {
    let ref_source = reference_source();
    let source = production_source();
    let source_embedding = initial_embedding(&source);
    let mut checked = 0_usize;

    for kind in [LensKind::Identity, LensKind::Projection] {
        let ref_lens = reference_lens(kind);
        let (ref_view, initial_ref_complement) = ref_lens.initialize(&ref_source);
        let state_lens = production_lens(kind);
        let (view, initial_complement) = panproto_lens::get(&state_lens, &source).unwrap();
        let view_embedding = initial_embedding(&view);
        assert_eq!(
            complement_carrier(&initial_complement),
            initial_ref_complement.hidden().clone()
        );
        assert_eq!(
            view.nodes.len() - 1,
            ref_view.leaves().len(),
            "reference and production views have the same leaf carrier"
        );

        for plan in source_plans() {
            let ref_input = reference::lower(&ref_source, &plan.atoms).unwrap();
            let input = lower(&source, &source_embedding, &production_edit(&plan.atoms)).unwrap();
            assert_eq!(carrier(&input.delta), ref_input, "input: {}", plan.name);
            let mut applied = ref_source.clone();
            reference::Delta::apply(&ref_input, &mut applied).unwrap();
            assert_eq!(applied, reference_tree(&input.tree), "state: {}", plan.name);

            let mut expected_complement = initial_ref_complement.clone();
            let expected =
                reference::Lens::get_edit(ref_lens, &ref_input, &mut expected_complement).unwrap();
            assert!(reference::is_minimal(&ref_input, &expected));

            let (whole, whole_complement) =
                translated_get(kind, &source, &view, &view_embedding, &plan, false);
            let (steps, steps_complement) =
                translated_get(kind, &source, &view, &view_embedding, &plan, true);
            assert_eq!(whole, steps, "strict get composition: {}", plan.name);
            assert!(
                <TreeShape as LensShape>::complement_divergence(
                    &whole_complement,
                    &steps_complement
                )
                .is_none(),
                "threaded get complement: {}",
                plan.name
            );
            assert_eq!(carrier(&whole), expected, "get oracle: {}", plan.name);
            assert_eq!(
                complement_carrier(&whole_complement),
                expected_complement.hidden().clone(),
                "get residue: {}",
                plan.name
            );
            assert_production_minimal(&source, &input.delta, &whole);

            if plan.name.starts_with("add-delete") {
                assert!(
                    ref_input.is_identity(),
                    "reference cancellation: {}",
                    plan.name
                );
                assert!(
                    whole.is_identity(),
                    "production cancellation: {}",
                    plan.name
                );
                assert!(
                    <TreeShape as LensShape>::complement_divergence(
                        &initial_complement,
                        &whole_complement
                    )
                    .is_none(),
                    "add/delete restores residue: {}",
                    plan.name
                );
            }
            if plan.name.starts_with("delete-reinsert") {
                assert!(
                    matches!(
                        ref_input.changes().values().next(),
                        Some(Change::Replace { .. })
                    ),
                    "delete/reinsert remains replacement: {}",
                    plan.name
                );
            }
            checked += 1;
        }
    }
    assert_eq!(checked, 38, "two lenses times nineteen source words");
}

#[test]
fn tr12_put_functoriality_minimality_and_source_undo_match_the_reference() {
    let ref_source = reference_source();
    let source = production_source();
    let source_embedding = initial_embedding(&source);
    let mut checked = 0_usize;
    let mut tree_image_undo = 0_usize;
    let mut canonical_only_undo = 0_usize;

    for kind in [LensKind::Identity, LensKind::Projection] {
        let ref_lens = reference_lens(kind);
        let (ref_view, initial_ref_complement) = ref_lens.initialize(&ref_source);
        let state_lens = production_lens(kind);
        let (view, initial_complement) = panproto_lens::get(&state_lens, &source).unwrap();
        let view_embedding = initial_embedding(&view);

        for plan in put_plans(kind) {
            let ref_input = reference::lower(&ref_view, &plan.atoms).unwrap();
            let view_input = lower(&view, &view_embedding, &production_edit(&plan.atoms)).unwrap();
            assert_eq!(
                carrier(&view_input.delta),
                ref_input,
                "put input: {}",
                plan.name
            );

            let mut expected_after_put = initial_ref_complement.clone();
            let expected_source =
                reference::Lens::put_edit(ref_lens, &ref_input, &mut expected_after_put).unwrap();
            assert!(reference::is_minimal(&ref_input, &expected_source));

            let (whole, mut whole_lens) =
                translated_put(kind, &source, &source_embedding, &plan, false);
            let (steps, steps_lens) = translated_put(kind, &source, &source_embedding, &plan, true);
            assert_eq!(
                whole.delta, steps.delta,
                "strict put composition: {}",
                plan.name
            );
            assert!(
                <TreeShape as LensShape>::complement_divergence(
                    &whole_lens.complement,
                    &steps_lens.complement
                )
                .is_none(),
                "threaded put complement: {}",
                plan.name
            );
            assert_eq!(
                carrier(&whole.delta),
                expected_source,
                "put oracle: {}",
                plan.name
            );
            assert_eq!(
                complement_carrier(&whole_lens.complement),
                expected_after_put.hidden().clone(),
                "put residue: {}",
                plan.name
            );
            assert_production_minimal(&source, &view_input.delta, &whole.delta);

            // Canonical graph inversion proves exact source restoration,
            // including edge ids and full payloads.
            let source_graph =
                GInstance::from_wtype_under(&source, &source_embedding, None).unwrap();
            let mut after_graph = source_graph.clone();
            whole.delta.apply(&mut after_graph).unwrap();
            let inverse_source = whole.delta.invert();
            inverse_source.apply(&mut after_graph).unwrap();
            assert!(
                GInstance::identical(&source_graph, &after_graph),
                "canonical source undo: {}",
                plan.name
            );

            // The tree surface cannot always spell the canonical inverse:
            // reinserting a deleted arc may reuse an edge id below the
            // embedding's allocation floor. Count that boundary explicitly,
            // but feed the actual canonical inverse through the production
            // graph-edit entry point in every case.
            let exact_tree_inverse =
                TreeEdit::from_graph_delta(&whole.tree, &whole.embedding, &inverse_source).ok();
            if exact_tree_inverse.is_some() {
                tree_image_undo += 1;
            } else {
                canonical_only_undo += 1;
            }

            let ref_inverse_source = reference::Delta::invert(&expected_source);
            let mut expected_after_undo = expected_after_put.clone();
            let expected_inverse_view =
                reference::Lens::get_edit(ref_lens, &ref_inverse_source, &mut expected_after_undo)
                    .unwrap();
            assert_eq!(expected_inverse_view, reference::Delta::invert(&ref_input));
            assert_eq!(expected_after_undo, initial_ref_complement);

            let translated_inverse = whole_lens
                .get_graph_edit(
                    &whole.tree,
                    &whole.embedding,
                    &view_input.tree,
                    &view_input.embedding,
                    &inverse_source,
                )
                .unwrap();
            assert_eq!(
                carrier(&translated_inverse),
                expected_inverse_view,
                "get_graph_edit of the actual inverse put delta: {}",
                plan.name
            );
            assert_eq!(
                translated_inverse,
                view_input.delta.invert(),
                "strict canonical inverse translation: {}",
                plan.name
            );
            if plan.name == "delete-reinsert-1-Visible-0" {
                assert!(
                    translated_inverse.del_nodes().contains_key(&KEPT)
                        && translated_inverse.add_nodes().contains_key(&KEPT),
                    "equal-payload visible replacement must retain replacement provenance"
                );
                let old = translated_inverse.del_nodes().get(&KEPT).unwrap();
                let new = translated_inverse.add_nodes().get(&KEPT).unwrap();
                assert_eq!(
                    reference_leaf(old, ROOT),
                    reference_leaf(new, ROOT),
                    "the named regression replaces an equal payload, not merely an equal id"
                );
            }
            let mut restored_view =
                GInstance::from_wtype_under(&view_input.tree, &view_input.embedding, None).unwrap();
            translated_inverse.apply(&mut restored_view).unwrap();
            let original_view = GInstance::from_wtype_under(&view, &view_embedding, None).unwrap();
            assert!(
                GInstance::identical(&restored_view, &original_view),
                "exact canonical view undo: {}",
                plan.name
            );
            assert!(
                <TreeShape as LensShape>::complement_divergence(
                    &whole_lens.complement,
                    &initial_complement
                )
                .is_none(),
                "source undo restores complement: {}",
                plan.name
            );
            checked += 1;
        }
    }
    assert_eq!(checked, 27, "nineteen identity plus eight projection puts");
    assert_eq!(
        tree_image_undo, 15,
        "the exact TreeEdit inverse subdomain count drifted"
    );
    assert_eq!(
        canonical_only_undo, 12,
        "the canonical GraphDelta-only inverse subdomain count drifted"
    );
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RefusalKind {
    Missing(u32),
    Duplicate(u32),
}

fn reference_refusal(error: &Refusal) -> RefusalKind {
    match error {
        Refusal::Missing { id } => RefusalKind::Missing(*id),
        Refusal::Duplicate { id } => RefusalKind::Duplicate(*id),
        other => panic!("unexpected reference refusal {other:?}"),
    }
}

fn production_refusal(error: &EditError) -> RefusalKind {
    match error {
        EditError::NodeNotFound(id) => RefusalKind::Missing(*id),
        EditError::DuplicateNodeId(id) => RefusalKind::Duplicate(*id),
        other => panic!("unexpected production refusal {other:?}"),
    }
}

#[test]
fn tr12_state_reading_lowering_has_the_same_typed_refusals() {
    let ref_source = reference_source();
    let source = production_source();
    let embedding = initial_embedding(&source);
    let cases = [
        Plan {
            name: "missing-delete".to_owned(),
            atoms: vec![Atom::Delete { id: 99 }],
        },
        Plan {
            name: "duplicate-insert".to_owned(),
            atoms: vec![Atom::Insert(leaf(KEPT, Visibility::Visible, 1))],
        },
    ];
    for plan in cases {
        let expected = reference::lower(&ref_source, &plan.atoms).unwrap_err();
        let edit = production_edit(&plan.atoms);
        let graph = GInstance::from_wtype_under(&source, &embedding, None).unwrap();
        let mut next_embedding = embedding.clone();
        let mut next_source = source.clone();
        let actual = match GraphDelta::from_tree_edit(&graph, &mut next_embedding, &edit) {
            Err(error) => error,
            Ok(_) => edit.apply(&mut next_source).unwrap_err(),
        };
        assert_eq!(
            production_refusal(&actual),
            reference_refusal(&expected),
            "typed refusal: {}",
            plan.name
        );
        assert!(
            panproto_lens::instances_equivalent(&source, &next_source),
            "a lowering refusal must not change production state: {}",
            plan.name
        );
    }
}

#[test]
fn tr12_late_sequence_refusal_is_atomic_for_reference_and_production() {
    let ref_source = reference_source();
    let ref_lens = reference::Lens::Projection;
    let (_, mut ref_complement) = ref_lens.initialize(&ref_source);
    let initial_ref_complement = ref_complement.clone();
    let late_missing = reference::Delta::compose(
        &Delta::singleton(Change::Delete(leaf(HIDDEN, Visibility::Hidden, 0))),
        &Delta::singleton(Change::Delete(leaf(99, Visibility::Hidden, 0))),
    )
    .unwrap();
    assert_eq!(
        reference::Lens::get_edit(ref_lens, &late_missing, &mut ref_complement).unwrap_err(),
        Refusal::Missing { id: 99 }
    );
    assert_eq!(
        ref_complement, initial_ref_complement,
        "a late reference refusal must not retain the first id's mutation"
    );

    let source = production_source();
    let mut state_lens = production_lens(LensKind::Projection);
    state_lens.compiled.field_transforms.insert(
        panproto_gat::Name::from("visible"),
        vec![panproto_inst::FieldTransform::ApplyExpr {
            key: "text".to_owned(),
            expr: panproto_expr::Expr::Var(std::sync::Arc::from("text")),
            inverse: Some(panproto_expr::Expr::Var(std::sync::Arc::from("text"))),
            coercion_class: panproto_gat::CoercionClass::Iso,
        }],
    );
    let mut lens = EditLens::from_lens(state_lens, Protocol::default());
    lens.initialize(&source).unwrap();
    let initial_complement = lens.complement.clone();
    let initial_tracked_source = lens.pipeline.as_ref().unwrap().source().clone();
    let refused = lens
        .get_edit(TreeEdit::Sequence(vec![
            TreeEdit::DeleteNode { id: HIDDEN },
            TreeEdit::SetField {
                node_id: KEPT,
                field: "text".into(),
                value: Value::Token("not-an-expression-literal".into()),
            },
        ]))
        .unwrap_err();
    assert!(matches!(
        refused,
        panproto_lens::EditLensError::Restrict(panproto_inst::RestrictError::ValueNotExpressible {
            node: KEPT,
            ..
        })
    ));
    assert!(
        <TreeShape as LensShape>::complement_divergence(&lens.complement, &initial_complement)
            .is_none(),
        "a late production refusal must leave the complement unchanged"
    );
    assert!(
        panproto_lens::instances_equivalent(
            lens.pipeline.as_ref().unwrap().source(),
            &initial_tracked_source
        ),
        "a late production refusal must leave the tracked source unchanged"
    );
}
