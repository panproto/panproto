//! Differential closure for the tree edit lens laws: get and put
//! functoriality, replacement, minimality and source undo.
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
            (ROOT, HIDDEN, edge(Visibility::Hidden)),
            (ROOT, KEPT, edge(Visibility::Visible)),
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

// Allocation history survives cancellation and undo. Compare every other
// law-observable complement coordinate, then check the floor separately.
fn assert_residue_restored(
    actual: &panproto_lens::Complement,
    original: &panproto_lens::Complement,
) {
    assert!(actual.id_floor >= original.id_floor);
    let mut expected = original.clone();
    expected.id_floor = actual.id_floor;
    assert!(
        <TreeShape as LensShape>::complement_divergence(actual, &expected).is_none(),
        "all complement coordinates except historical allocation agree"
    );
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
) -> (GraphDelta, panproto_inst::Complement, WInstance) {
    let mut lens = edit_lens(kind, source);
    if !sequential {
        let translated = lens.get_edit(production_edit(&plan.atoms)).unwrap();
        let lowered = lower(view, view_embedding, &translated).unwrap();
        return (lowered.delta, lens.complement, lowered.tree);
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
    (net, lens.complement, current)
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
fn tree_edit_lens_get_functoriality_replacement_and_minimality_match_the_reference() {
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

            let (whole, whole_complement, edited_view) =
                translated_get(kind, &source, &view, &view_embedding, &plan, false);
            let (steps, steps_complement, steps_view) =
                translated_get(kind, &source, &view, &view_embedding, &plan, true);

            // Get consistency: the translated edit applied to
            // the view is the view of the edited source, with the residue of
            // the edited source. Reference first, then production.
            let mut expected_view = ref_view.clone();
            reference::Delta::apply(&expected, &mut expected_view).unwrap();
            let (recomputed_view, recomputed_complement) = ref_lens.initialize(&applied);
            assert_eq!(
                expected_view, recomputed_view,
                "reference get consistency: {}",
                plan.name
            );
            assert_eq!(
                expected_complement, recomputed_complement,
                "reference complement consistency: {}",
                plan.name
            );
            let (fresh_view, fresh_complement) =
                panproto_lens::get(&state_lens, &input.tree).unwrap();
            assert_eq!(
                reference_tree(&edited_view),
                reference_tree(&fresh_view),
                "production get consistency: {}",
                plan.name
            );
            assert_eq!(
                reference_tree(&steps_view),
                reference_tree(&fresh_view),
                "production sequential get consistency: {}",
                plan.name
            );
            assert_eq!(
                reference_tree(&fresh_view),
                expected_view,
                "production recomputed view against the reference: {}",
                plan.name
            );
            assert_eq!(
                complement_carrier(&fresh_complement),
                complement_carrier(&whole_complement),
                "production complement consistency: {}",
                plan.name
            );
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
                assert_residue_restored(&whole_complement, &initial_complement);
                assert_eq!(
                    whole_complement.id_floor,
                    initial_complement.id_floor.max(FRESH + 1)
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
fn tree_edit_lens_put_functoriality_minimality_and_source_undo_match_the_reference() {
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

            let floor_before_undo = whole_lens.complement.id_floor;
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
            assert_residue_restored(&whole_lens.complement, &initial_complement);
            assert!(whole_lens.complement.id_floor >= floor_before_undo);
            checked += 1;
        }
    }
    assert_eq!(checked, 27, "nineteen identity plus eight projection puts");
    // Fourteen inverses are tree edits. The inverse of `delete-2` re-adds
    // the hidden leaf's arc under its old id, which is below the edge floor;
    // a tree edit draws every fresh arc id from the floor (L-edits-3), so no
    // tree edit lowers to it and both readers refuse it
    // (`inverse_reusing_an_arc_id_below_the_floor_is_no_tree_edit` in
    // panproto-inst's reference_edits).
    assert_eq!(
        tree_image_undo, 14,
        "the exact TreeEdit inverse subdomain count drifted"
    );
    assert_eq!(
        canonical_only_undo, 13,
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
fn tree_edit_lens_state_reading_lowering_has_the_same_typed_refusals() {
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
fn tree_edit_lens_late_sequence_refusal_is_atomic_for_reference_and_production() {
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

    // The common late word, through each side's source editor: delete the
    // hidden leaf, then a missing id. The production edit lens translates
    // admitted edits only, so the common refusal is the source editor's.
    let late_word = [Atom::Delete { id: HIDDEN }, Atom::Delete { id: 99 }];
    let expected = reference::lower(&ref_source, &late_word).unwrap_err();
    assert_eq!(expected, Refusal::Missing { id: 99 });
    let source = production_source();
    let embedding = initial_embedding(&source);
    let edit = production_edit(&late_word);
    let graph = GInstance::from_wtype_under(&source, &embedding, None).unwrap();
    let mut next_embedding = embedding;
    let mut next_source = source.clone();
    let actual = match GraphDelta::from_tree_edit(&graph, &mut next_embedding, &edit) {
        Err(error) => error,
        Ok(_) => edit.apply(&mut next_source).unwrap_err(),
    };
    assert_eq!(
        production_refusal(&actual),
        reference_refusal(&expected),
        "the common late word refuses with the same typed cause"
    );
    assert!(
        panproto_lens::instances_equivalent(&source, &next_source),
        "the common late word leaves the production source unchanged"
    );
    assert_eq!(
        reference_tree(&source),
        ref_source,
        "the common late word leaves the reference source unchanged"
    );

    // The production edit lens refuses the same word itself: a delete of an
    // id absent from its tracked source is a typed refusal, and the staged
    // first step is discarded with it.
    for kind in [LensKind::Identity, LensKind::Projection] {
        let mut lens = edit_lens(kind, &source);
        let initial_complement = lens.complement.clone();
        let initial_tracked_source = lens.pipeline.as_ref().unwrap().source().clone();
        let refused = lens.get_edit(edit.clone()).unwrap_err();
        assert!(
            matches!(
                refused,
                panproto_lens::EditLensError::EditApply(EditError::NodeNotFound(99))
            ),
            "{refused:?}"
        );
        assert!(
            <TreeShape as LensShape>::complement_divergence(&lens.complement, &initial_complement)
                .is_none(),
            "a refused absent delete must leave the complement unchanged"
        );
        assert!(
            panproto_lens::instances_equivalent(
                lens.pipeline.as_ref().unwrap().source(),
                &initial_tracked_source
            ),
            "a refused absent delete must leave the tracked source unchanged"
        );
    }

    // A production-only late refusal, raised by the restrict stage after the
    // first edit of the sequence has been staged.
    let mut state_lens = production_lens(LensKind::Projection);
    // The transform writes `text` on `visible`, so the target declares it:
    // a transform output is a leaf edge of its vertex.
    state_lens.tgt_schema = SchemaBuilder::new(&Protocol::default())
        .vertex("root", "record", None::<&str>)
        .unwrap()
        .vertex("visible", "scalar", None::<&str>)
        .unwrap()
        .vertex("visible.text", "scalar", None::<&str>)
        .unwrap()
        .edge("root", "visible", "prop", Some("visible"))
        .unwrap()
        .edge("visible", "visible.text", "prop", Some("text"))
        .unwrap()
        .entry("root")
        .build()
        .unwrap();
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

/// A field transform belongs to its declared anchor, including for edits.
#[test]
fn field_edit_translation_uses_only_the_edited_nodes_anchor() {
    let protocol = Protocol::default();
    let source_schema = SchemaBuilder::new(&protocol)
        .vertex("a", "record", None::<&str>)
        .unwrap()
        .vertex("b", "record", None::<&str>)
        .unwrap()
        .entry("a")
        .build()
        .unwrap();
    let target_schema = SchemaBuilder::new(&protocol)
        .vertex("a", "record", None::<&str>)
        .unwrap()
        .vertex("b", "record", None::<&str>)
        .unwrap()
        .vertex("g", "scalar", None::<&str>)
        .unwrap()
        .edge("b", "g", "prop", Some("g"))
        .unwrap()
        .entry("a")
        .build()
        .unwrap();
    let mut compiled = CompiledMigration {
        surviving_verts: source_schema.vertices.keys().cloned().collect(),
        ..CompiledMigration::default()
    };
    compiled.field_transforms.insert(
        "b".into(),
        vec![panproto_inst::FieldTransform::RenameField {
            old_key: "f".to_owned(),
            new_key: "g".to_owned(),
        }],
    );
    let lens = Lens {
        compiled,
        src_schema: source_schema,
        tgt_schema: target_schema,
    };
    let mut node = Node::new(0, "a");
    node.extra_fields.insert("f".to_owned(), Value::Int(0));
    let source = WInstance::new(
        HashMap::from([(0, node)]),
        Vec::new(),
        Vec::new(),
        0,
        "a".into(),
    );
    let edit = TreeEdit::SetField {
        node_id: 0,
        field: "f".into(),
        value: Value::Int(1),
    };
    let (mut translated_view, _) = panproto_lens::get(&lens, &source).unwrap();
    let mut edited_source = source.clone();
    edit.apply(&mut edited_source).unwrap();
    let (expected, _) = panproto_lens::get(&lens, &edited_source).unwrap();
    let mut edit_lens = EditLens::from_lens(
        Lens {
            compiled: lens.compiled.clone(),
            src_schema: lens.src_schema.clone(),
            tgt_schema: lens.tgt_schema.clone(),
        },
        protocol,
    );
    edit_lens.initialize(&source).unwrap();
    edit_lens
        .get_edit(edit)
        .unwrap()
        .apply(&mut translated_view)
        .unwrap();
    assert_eq!(
        translated_view.nodes[&0].extra_fields, expected.nodes[&0].extra_fields,
        "a transform declared at b must not rename a field edit on a"
    );
    let removal = edit_lens
        .get_edit(TreeEdit::RemoveField {
            node_id: 0,
            field: "f".into(),
        })
        .unwrap();
    assert!(matches!(removal, TreeEdit::RemoveField { field, .. } if field == "f"));
    let backward = edit_lens
        .put_edit(TreeEdit::SetField {
            node_id: 0,
            field: "g".into(),
            value: Value::Int(2),
        })
        .unwrap();
    assert!(matches!(backward, TreeEdit::SetField { field, .. } if field == "g"));
    let backward_removal = edit_lens
        .put_edit(TreeEdit::RemoveField {
            node_id: 0,
            field: "g".into(),
        })
        .unwrap();
    assert!(matches!(backward_removal, TreeEdit::RemoveField { field, .. } if field == "g"));
    let before_source = edit_lens.pipeline.as_ref().unwrap().source().clone();
    let before_complement = format!("{:?}", edit_lens.complement);
    let edit = TreeEdit::Sequence(vec![
        TreeEdit::RelabelNode {
            id: 0,
            new_anchor: "b".into(),
        },
        TreeEdit::SetField {
            node_id: 0,
            field: "g".into(),
            value: Value::Int(3),
        },
    ]);
    let (mut edited_view, _) = panproto_lens::get(&lens, &before_source).unwrap();
    edit.apply(&mut edited_view).unwrap();
    let expected = panproto_lens::put(&lens, &edited_view, &edit_lens.complement).unwrap();
    let mut raw_candidate = before_source.clone();
    TreeEdit::Sequence(vec![
        TreeEdit::RelabelNode {
            id: 0,
            new_anchor: "b".into(),
        },
        TreeEdit::SetField {
            node_id: 0,
            field: "f".into(),
            value: Value::Int(3),
        },
    ])
    .apply(&mut raw_candidate)
    .unwrap();
    assert_eq!(raw_candidate.nodes[&0].anchor, expected.nodes[&0].anchor);
    assert_eq!(
        raw_candidate.nodes[&0].extra_fields,
        expected.nodes[&0].extra_fields
    );
    assert_eq!(raw_candidate.schema_root.as_ref(), "a");
    assert_eq!(expected.schema_root.as_ref(), "b");
    let error = edit_lens.put_edit(edit).unwrap_err();
    assert!(
        matches!(error, panproto_lens::EditLensError::TranslationFailed(message)
        if message == "the translated tree edit does not produce the whole-state put source")
    );
    assert_eq!(
        format!("{:?}", edit_lens.pipeline.as_ref().unwrap().source()),
        format!("{before_source:?}")
    );
    assert_eq!(format!("{:?}", edit_lens.complement), before_complement);
}

/// Diagnostic: an attribute loop is identity in the source and replacement in the view.
#[test]
fn conditional_survival_loop_exposes_strict_functoriality_obstruction() {
    let protocol = Protocol::default();
    let schema = SchemaBuilder::new(&protocol)
        .vertex("r", "record", None::<&str>)
        .unwrap()
        .vertex("a", "record", None::<&str>)
        .unwrap()
        .edge("r", "a", "prop", Some("a"))
        .unwrap()
        .entry("r")
        .build()
        .unwrap();
    let mut compiled = CompiledMigration {
        surviving_verts: schema.vertices.keys().cloned().collect(),
        surviving_edges: schema.edges.keys().cloned().collect(),
        ..CompiledMigration::default()
    };
    compiled
        .conditional_survival
        .insert("a".into(), panproto_expr::Expr::Var("visible".into()));
    let make_lens = || Lens {
        compiled: compiled.clone(),
        src_schema: schema.clone(),
        tgt_schema: schema.clone(),
    };
    let mut leaf = Node::new(1, "a");
    leaf.extra_fields
        .insert("visible".to_owned(), Value::Bool(true));
    let arc = Edge {
        src: "r".into(),
        tgt: "a".into(),
        kind: "prop".into(),
        name: Some("a".into()),
    };
    let before = WInstance::new(
        HashMap::from([(0, Node::new(0, "r")), (1, leaf)]),
        vec![(0, 1, arc)],
        vec![],
        0,
        "r".into(),
    );
    let mut middle = before.clone();
    TreeEdit::SetField {
        node_id: 1,
        field: "visible".into(),
        value: Value::Bool(false),
    }
    .apply(&mut middle)
    .unwrap();
    let (g0, se0) = GInstance::from_wtype_embedded(&before, Some(&schema)).unwrap();
    let (g1, se1) = GInstance::from_wtype_embedded(&middle, Some(&schema)).unwrap();
    let down = GraphDelta::diff(&g0, &g1);
    let up = GraphDelta::diff(&g1, &g0);
    assert_eq!(down.compose(&up).unwrap(), GraphDelta::identity());
    let lens = make_lens();
    let (v0, c0) = panproto_lens::get(&lens, &before).unwrap();
    let (v1, c1) = panproto_lens::get(&lens, &middle).unwrap();
    assert!(v0.nodes.contains_key(&1));
    assert!(!v1.nodes.contains_key(&1));
    // Both reached views are admitted to whole-state put, with the threaded complement.
    panproto_lens::put(&lens, &v0, &c0).unwrap();
    panproto_lens::put(&lens, &v1, &c1).unwrap();
    let (gv0, ve0) = GInstance::from_wtype_embedded(&v0, Some(&schema)).unwrap();
    let (_, ve1) = GInstance::from_wtype_embedded(&v1, Some(&schema)).unwrap();
    let mut translator = EditLens::from_lens(make_lens(), protocol.clone());
    translator.initialize(&before).unwrap();
    let first = translator
        .get_graph_edit(&before, &se0, &v0, &ve0, &down)
        .unwrap();
    let second = translator
        .get_graph_edit(&middle, &se1, &v1, &ve1, &up)
        .unwrap();
    let sequential = first.compose(&second).unwrap();
    let mut direct_lens = EditLens::from_lens(make_lens(), protocol);
    direct_lens.initialize(&before).unwrap();
    let direct = direct_lens
        .get_graph_edit(&before, &se0, &v0, &ve0, &GraphDelta::identity())
        .unwrap();
    assert!(sequential.del_nodes().contains_key(&1));
    assert!(sequential.add_nodes().contains_key(&1));
    assert_eq!(direct, GraphDelta::identity());
    assert_ne!(
        sequential, direct,
        "the existing strict general theorem is incompatible with occurrence-sensitive equality"
    );
    let mut result = gv0.clone();
    sequential.apply(&mut result).unwrap();
    assert_eq!(result, gv0);
}

/// Public `get_edit` must agree with `get` when removing a rename's shadowing source.
#[test]
fn field_removal_recovers_a_shadowed_rename_destination() {
    let protocol = Protocol::default();
    let source_schema = SchemaBuilder::new(&protocol)
        .vertex("a", "record", None::<&str>)
        .unwrap()
        .entry("a")
        .build()
        .unwrap();
    let target_schema = SchemaBuilder::new(&protocol)
        .vertex("a", "record", None::<&str>)
        .unwrap()
        .vertex("g", "scalar", None::<&str>)
        .unwrap()
        .edge("a", "g", "prop", Some("g"))
        .unwrap()
        .entry("a")
        .build()
        .unwrap();
    let mut compiled = CompiledMigration {
        surviving_verts: source_schema.vertices.keys().cloned().collect(),
        ..CompiledMigration::default()
    };
    compiled.field_transforms.insert(
        "a".into(),
        vec![panproto_inst::FieldTransform::RenameField {
            old_key: "f".to_owned(),
            new_key: "g".to_owned(),
        }],
    );
    let lens = Lens {
        compiled,
        src_schema: source_schema,
        tgt_schema: target_schema,
    };
    let mut node = Node::new(0, "a");
    node.extra_fields.insert("f".to_owned(), Value::Int(1));
    node.extra_fields.insert("g".to_owned(), Value::Int(2));
    let source = WInstance::new(HashMap::from([(0, node)]), vec![], vec![], 0, "a".into());
    let edit = TreeEdit::RemoveField {
        node_id: 0,
        field: "f".into(),
    };
    let (mut actual, _) = panproto_lens::get(&lens, &source).unwrap();
    assert_eq!(actual.nodes[&0].extra_fields["g"], Value::Int(1));
    let mut edited_source = source.clone();
    edit.apply(&mut edited_source).unwrap();
    let (expected, expected_complement) = panproto_lens::get(&lens, &edited_source).unwrap();
    assert_eq!(expected.nodes[&0].extra_fields["g"], Value::Int(2));
    // Explicitly establish the resulting view's whole-state put domain;
    // a failed assertion here must not be reported as an admitted-law failure.
    panproto_lens::put(&lens, &expected, &expected_complement).unwrap();
    let mut translator = EditLens::from_lens(lens, protocol);
    translator.initialize(&source).unwrap();
    let translated = translator.get_edit(edit).unwrap();
    translated.apply(&mut actual).unwrap();
    assert_eq!(
        actual.nodes[&0].extra_fields,
        expected.nodes[&0].extra_fields
    );
}

/// One generated leaf word is compared with the independent edit lens, both
/// as an atomic word and as its individually admitted steps. This generator
/// stays inside identity/static projection; it asserts no general composition
/// law for conditional survival or synthesized incidence.
fn generated_leaf_word_agrees(kind: LensKind, source: &WInstance, plan: &Plan, put: bool) {
    let ref_source = reference_tree(source);
    let ref_lens = reference_lens(kind);
    let (ref_view, mut expected_complement) = ref_lens.initialize(&ref_source);
    let state_lens = production_lens(kind);
    let (view, _) = panproto_lens::get(&state_lens, source).unwrap();
    assert_eq!(reference_tree(&view), ref_view);
    let source_embedding = initial_embedding(source);
    let view_embedding = initial_embedding(&view);
    let ref_input =
        reference::lower(if put { &ref_view } else { &ref_source }, &plan.atoms).unwrap();
    if put {
        let expected =
            reference::Lens::put_edit(ref_lens, &ref_input, &mut expected_complement).unwrap();
        let (whole, whole_lens) = translated_put(kind, source, &source_embedding, plan, false);
        let (steps, steps_lens) = translated_put(kind, source, &source_embedding, plan, true);
        assert_eq!(whole.delta, steps.delta);
        assert_eq!(carrier(&whole.delta), expected);
        assert_eq!(
            complement_carrier(&whole_lens.complement),
            expected_complement.hidden().clone()
        );
        assert!(
            <TreeShape as LensShape>::complement_divergence(
                &whole_lens.complement,
                &steps_lens.complement
            )
            .is_none()
        );
        let mut expected_source = ref_source.clone();
        reference::Delta::apply(&expected, &mut expected_source).unwrap();
        assert_eq!(reference_tree(&whole.tree), expected_source);
        let mut edited_view = view.clone();
        production_edit(&plan.atoms)
            .apply(&mut edited_view)
            .unwrap();
        let (fresh_view, _) = panproto_lens::get(&state_lens, &whole.tree).unwrap();
        assert_eq!(reference_tree(&fresh_view), reference_tree(&edited_view));
        let original = GInstance::from_wtype_under(source, &source_embedding, None).unwrap();
        let mut restored = original.clone();
        whole.delta.apply(&mut restored).unwrap();
        whole.delta.invert().apply(&mut restored).unwrap();
        assert!(GInstance::identical(&original, &restored));
    } else {
        let expected =
            reference::Lens::get_edit(ref_lens, &ref_input, &mut expected_complement).unwrap();
        let (whole, complement, edited_view) =
            translated_get(kind, source, &view, &view_embedding, plan, false);
        let (steps, steps_complement, steps_view) =
            translated_get(kind, source, &view, &view_embedding, plan, true);
        assert_eq!(whole, steps);
        assert_eq!(carrier(&whole), expected);
        assert_eq!(
            complement_carrier(&complement),
            expected_complement.hidden().clone()
        );
        assert!(
            <TreeShape as LensShape>::complement_divergence(&complement, &steps_complement)
                .is_none()
        );
        let mut expected_view = ref_view;
        reference::Delta::apply(&expected, &mut expected_view).unwrap();
        assert_eq!(reference_tree(&edited_view), expected_view);
        assert_eq!(reference_tree(&steps_view), expected_view);
        let mut edited_source = source.clone();
        production_edit(&plan.atoms)
            .apply(&mut edited_source)
            .unwrap();
        let (fresh_view, fresh_complement) =
            panproto_lens::get(&state_lens, &edited_source).unwrap();
        assert_eq!(reference_tree(&fresh_view), expected_view);
        assert_eq!(
            complement_carrier(&fresh_complement),
            complement_carrier(&complement)
        );
    }
}

proptest::proptest! {
    #![proptest_config(proptest::test_runner::Config::default())]

    #[test]
    fn randomized_leaf_edit_lens_differential(
        visible_payload in proptest::prelude::any::<u8>(),
        hidden_payload in proptest::prelude::any::<u8>(),
        new_payload in proptest::prelude::any::<u8>(),
        fresh in 3_u32..10_000,
        identity in proptest::prelude::any::<bool>(),
        put in proptest::prelude::any::<bool>(),
        hidden in proptest::prelude::any::<bool>(),
        operation in 0_u8..5,
    ) {
        let kind=if identity {LensKind::Identity} else {LensKind::Projection};
        let operation=if put && !identity {if operation==0 {0} else {3}} else {operation};
        let mut source=production_source();
        source.nodes.insert(KEPT,production_node(&leaf(KEPT,Visibility::Visible,visible_payload)));
        source.nodes.insert(HIDDEN,production_node(&leaf(HIDDEN,Visibility::Hidden,hidden_payload)));
        let visibility=if hidden && (!put || identity) {Visibility::Hidden} else {Visibility::Visible};
        let id=if visibility==Visibility::Hidden {HIDDEN} else {KEPT};
        let new=leaf(if operation==1 || operation==2 {fresh} else {id},visibility,new_payload);
        let atoms=match operation {
            0=>Vec::new(),
            1=>vec![Atom::Insert(new)],
            2=>vec![Atom::Insert(new),Atom::Delete{id:fresh}],
            3=>vec![Atom::Delete{id}],
            _=>vec![Atom::Delete{id},Atom::Insert(new)],
        };
        generated_leaf_word_agrees(kind,&source,&Plan{name:"generated".into(),atoms},put);
    }
}

/// The finite admitted carrier fixes hidden-before-visible arc order. The
/// reversed presentation exposes a strict endpoint difference on replacement.
#[test]
fn visible_first_leaf_replacement_is_an_atomic_endpoint_refusal() {
    let original = production_source();
    let mut arcs = original.arcs;
    arcs.reverse();
    let source = WInstance::new(
        original.nodes,
        arcs,
        original.fans,
        original.root,
        original.schema_root,
    );
    let lens = production_lens(LensKind::Projection);
    let (mut edited_view, complement) = panproto_lens::get(&lens, &source).unwrap();
    let edit = production_edit(&[
        Atom::Delete { id: KEPT },
        Atom::Insert(leaf(KEPT, Visibility::Visible, 0)),
    ]);
    edit.apply(&mut edited_view).unwrap();
    let expected = panproto_lens::put(&lens, &edited_view, &complement).unwrap();
    let mut raw_candidate = source.clone();
    edit.apply(&mut raw_candidate).unwrap();
    assert_eq!(reference_tree(&raw_candidate), reference_tree(&expected));
    assert_eq!(
        raw_candidate
            .arcs
            .iter()
            .map(|(_, child, _)| *child)
            .collect::<Vec<_>>(),
        [HIDDEN, KEPT]
    );
    assert_eq!(
        expected
            .arcs
            .iter()
            .map(|(_, child, _)| *child)
            .collect::<Vec<_>>(),
        [KEPT, HIDDEN]
    );
    let mut translator = edit_lens(LensKind::Projection, &source);
    let before_complement = format!("{:?}", translator.complement);
    let error = translator.put_edit(edit).unwrap_err();
    assert!(
        matches!(error, panproto_lens::EditLensError::TranslationFailed(message)
        if message == "the translated tree edit does not produce the whole-state put source")
    );
    assert_eq!(
        format!("{:?}", translator.pipeline.as_ref().unwrap().source()),
        format!("{source:?}")
    );
    assert_eq!(format!("{:?}", translator.complement), before_complement);
}
