//! A graph edit lens translates each source edit to the view delta whose
//! result is whole-state graph get of the edited source, up to the ids of
//! synthesized elements; it keeps those ids stable, translates view edits
//! back through put, and refuses an edit without changing its state.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::error::Error;

use panproto_expr::{Expr, Literal};
use panproto_gat::Name;
use panproto_inst::graph_edit::GraphEdit;
use panproto_inst::{
    CompiledMigration, EdgeId, EditError, GEdge, GInstance, GraphComplement, GraphDelta, Node,
    SynthKey,
};
use panproto_lens::{
    GraphEditLens, GraphEditLensError, Lens, LensError, PutDomainError, graph_lens,
};
use panproto_schema::{Edge, Protocol, Schema, SchemaBuilder};

type TestResult = Result<(), Box<dyn Error>>;

fn edge(src: &str, tgt: &str, name: &str) -> Edge {
    Edge {
        src: src.into(),
        tgt: tgt.into(),
        kind: "prop".into(),
        name: Some(name.into()),
    }
}

fn schema(vertices: &[&str], edges: &[Edge]) -> Result<Schema, Box<dyn Error>> {
    let protocol = Protocol::default();
    let mut builder = SchemaBuilder::new(&protocol);
    for vertex in vertices {
        builder = builder.vertex(vertex, "object", None)?;
    }
    for e in edges {
        builder = builder.edge(&e.src, &e.tgt, &e.kind, e.name.as_deref())?;
    }
    Ok(builder.build()?)
}

/// The source schema: `a -> h -> b`, `h -> a` (closing a cycle through
/// the hidden vertex) and `a -> b`.
fn source_schema() -> Result<Schema, Box<dyn Error>> {
    schema(
        &["a", "b", "h"],
        &[
            edge("a", "h", "ah"),
            edge("h", "b", "hb"),
            edge("h", "a", "ha"),
            edge("a", "b", "ab"),
        ],
    )
}

/// The lens that drops `h` and contracts each path through hidden nodes
/// to `a -ab-> b` or `a -aa-> a`. The target has two edges from `a` to `b`,
/// so with `resolve` unset the pair `(a, b)` is ambiguous and a realized
/// pair is a get refusal.
fn contraction_lens(resolve: bool) -> Result<Lens, Box<dyn Error>> {
    let src_schema = source_schema()?;
    let tgt_schema = schema(
        &["a", "b"],
        &[
            edge("a", "b", "ab"),
            edge("a", "b", "ab2"),
            edge("a", "a", "aa"),
        ],
    )?;
    let mut resolver = HashMap::new();
    if resolve {
        resolver.insert((Name::from("a"), Name::from("b")), edge("a", "b", "ab"));
        resolver.insert((Name::from("a"), Name::from("a")), edge("a", "a", "aa"));
    }
    let compiled = CompiledMigration {
        surviving_verts: ["a", "b"].into_iter().map(Name::from).collect(),
        surviving_edges: HashSet::from([edge("a", "b", "ab")]),
        resolver,
        ..CompiledMigration::default()
    };
    Ok(Lens {
        compiled,
        src_schema,
        tgt_schema,
    })
}

/// Two contracted pairs: `0 -> 1 -> 2` and `3 -> 4 -> 5`, with a direct
/// edge `0 -> 2` parallel to the first contracted path.
fn source() -> GInstance {
    GInstance::new()
        .with_node(Node::new(0, "a"))
        .with_node(Node::new(1, "h"))
        .with_node(Node::new(2, "b"))
        .with_node(Node::new(3, "a"))
        .with_node(Node::new(4, "h"))
        .with_node(Node::new(5, "b"))
        .with_edge_id(EdgeId(0), 0, 1, edge("a", "h", "ah"))
        .with_edge_id(EdgeId(1), 1, 2, edge("h", "b", "hb"))
        .with_edge_id(EdgeId(2), 0, 2, edge("a", "b", "ab"))
        .with_edge_id(EdgeId(3), 3, 4, edge("a", "h", "ah"))
        .with_edge_id(EdgeId(4), 4, 5, edge("h", "b", "hb"))
}

/// A view and complement with every synthesized id replaced by its key:
/// what the law equivalence of the lens page compares.
#[derive(Debug, PartialEq)]
struct Normal {
    nodes: BTreeMap<String, String>,
    edges: BTreeMap<String, String>,
    residue: String,
    synthesized: Vec<String>,
}

fn normal(view: &GInstance, complement: &GraphComplement) -> Normal {
    let node_name = |id: u32| {
        complement
            .synthesized_nodes
            .get(&id)
            .map_or_else(|| format!("node {id}"), |key| format!("{key:?}"))
    };
    let mut nodes = BTreeMap::new();
    for (id, node) in view.nodes() {
        let mut node = node.clone();
        node.id = 0;
        nodes.insert(node_name(*id), format!("{node:?}"));
    }
    let mut edges = BTreeMap::new();
    for (id, edge) in view.edges() {
        let name = complement
            .synthesized_edges
            .get(id)
            .map_or_else(|| format!("edge {}", id.0), |key| format!("{key:?}"));
        edges.insert(
            name,
            format!(
                "{} -> {} {:?} {:?}",
                node_name(edge.src),
                node_name(edge.tgt),
                edge.edge,
                edge.position
            ),
        );
    }
    let residue = format!(
        "{:?} {:?} {:?} {:?} {:?} {:?} {:?} {:?}",
        complement.dropped_nodes,
        complement.dropped_edges,
        complement.original_edges,
        complement.original_anchors,
        complement.original_extra_fields,
        complement.original_values,
        complement.id_floor,
        complement.identity,
    );
    let mut synthesized: Vec<String> = complement
        .synthesized_nodes
        .values()
        .chain(complement.synthesized_edges.values())
        .map(|key| format!("{key:?}"))
        .collect();
    synthesized.sort();
    Normal {
        nodes,
        edges,
        residue,
        synthesized,
    }
}

/// The held view and complement are whole-state get of the held source up
/// to synthesized ids, and the held view and complement agree on which
/// elements are synthesized.
fn assert_matches_whole_state(lens: &GraphEditLens) -> TestResult {
    let (view, complement) = graph_lens::get(lens.lens(), lens.source())?;
    assert_eq!(
        normal(lens.view(), lens.complement()),
        normal(&view, &complement)
    );
    Ok(())
}

/// Every part of the lens state, for checking that a refusal changed none.
fn snapshot(lens: &GraphEditLens) -> String {
    format!(
        "{:?} {:?} {:?} {}",
        normal(lens.source(), &GraphComplement::empty()),
        normal(lens.view(), lens.complement()),
        lens.complement(),
        lens.sequence()
    )
}

fn synth_edge_id(lens: &GraphEditLens, src: u32, tgt: u32) -> Option<EdgeId> {
    lens.complement()
        .synthesized_edges
        .iter()
        .find(|(_, key)| matches!(key, SynthKey::Contract { src: s, tgt: t, .. } if (*s, *t) == (src, tgt)))
        .map(|(id, _)| *id)
}

#[test]
fn binding_a_source_holds_its_whole_state_get() -> TestResult {
    let lens = GraphEditLens::new(contraction_lens(true)?, source())?;
    assert_matches_whole_state(&lens)?;
    assert!(synth_edge_id(&lens, 0, 2).is_some());
    assert!(synth_edge_id(&lens, 3, 5).is_some());
    Ok(())
}

#[test]
fn deleting_a_hidden_edge_removes_its_contracted_edge_and_nothing_else() -> TestResult {
    let mut lens = GraphEditLens::new(contraction_lens(true)?, source())?;
    let contracted = synth_edge_id(&lens, 0, 2).ok_or("no contracted edge")?;
    let hidden = lens
        .source()
        .edge(EdgeId(1))
        .ok_or("no hidden edge")?
        .clone();
    let step = lens.get_edit(&GraphDelta::identity().with_deleted_edge(EdgeId(1), hidden))?;
    assert_eq!(step.sequence, 1);
    assert_eq!(
        step.view_delta
            .del_edges()
            .keys()
            .copied()
            .collect::<Vec<_>>(),
        vec![contracted]
    );
    assert!(step.view_delta.add_edges().is_empty() && step.view_delta.del_nodes().is_empty());
    assert_matches_whole_state(&lens)?;
    Ok(())
}

#[test]
fn an_unrelated_contracted_edge_keeps_its_id_when_whole_state_get_would_renumber_it() -> TestResult
{
    let mut lens = GraphEditLens::new(contraction_lens(true)?, source())?;
    let kept = synth_edge_id(&lens, 3, 5).ok_or("no contracted edge")?;
    let hidden = lens
        .source()
        .edge(EdgeId(1))
        .ok_or("no hidden edge")?
        .clone();
    lens.get_edit(&GraphDelta::identity().with_deleted_edge(EdgeId(1), hidden))?;
    let (_, whole) = graph_lens::get(lens.lens(), lens.source())?;
    let renumbered = whole
        .synthesized_edges
        .iter()
        .find(|(_, key)| matches!(key, SynthKey::Contract { src: 3, tgt: 5, .. }))
        .map(|(id, _)| *id);
    assert_ne!(renumbered, Some(kept), "whole-state get renumbers the edge");
    assert_eq!(synth_edge_id(&lens, 3, 5), Some(kept));
    assert_matches_whole_state(&lens)?;
    Ok(())
}

#[test]
fn reinserting_a_hidden_edge_restores_the_contracted_edge_at_the_least_free_id() -> TestResult {
    let mut lens = GraphEditLens::new(contraction_lens(true)?, source())?;
    let hidden = lens
        .source()
        .edge(EdgeId(1))
        .ok_or("no hidden edge")?
        .clone();
    lens.get_edit(&GraphDelta::identity().with_deleted_edge(EdgeId(1), hidden.clone()))?;
    let step = lens.get_edit(&GraphDelta::identity().with_added_edge(EdgeId(1), hidden))?;
    let added: Vec<EdgeId> = step.view_delta.add_edges().keys().copied().collect();
    assert_eq!(added.len(), 1);
    let floor = lens.complement().id_floor.edge;
    let occupied: Vec<u32> = lens
        .complement()
        .synthesized_edges
        .keys()
        .map(|id| id.0)
        .filter(|id| *id != added[0].0)
        .collect();
    let least = (floor..)
        .find(|id| !occupied.contains(id))
        .ok_or("no free id")?;
    assert_eq!(added[0].0, least);
    assert_matches_whole_state(&lens)?;
    Ok(())
}

#[test]
fn a_cycle_through_a_hidden_node_contracts_to_a_loop_and_back() -> TestResult {
    let mut lens = GraphEditLens::new(contraction_lens(true)?, source())?;
    let back =
        GraphDelta::identity().with_added_edge(EdgeId(20), GEdge::new(1, 0, edge("h", "a", "ha")));
    let step = lens.get_edit(&back)?;
    let added: Vec<&GEdge> = step.view_delta.add_edges().values().collect();
    assert_eq!(added.len(), 1);
    assert_eq!((added[0].src, added[0].tgt), (0, 0));
    assert_matches_whole_state(&lens)?;
    let inverse = back.invert();
    let step = lens.get_edit(&inverse)?;
    assert_eq!(step.view_delta.del_edges().len(), 1);
    assert_matches_whole_state(&lens)?;
    Ok(())
}

#[test]
fn a_source_edge_that_takes_a_synthesized_id_moves_the_synthesized_edge() -> TestResult {
    let mut lens = GraphEditLens::new(contraction_lens(true)?, source())?;
    let taken = synth_edge_id(&lens, 0, 2).ok_or("no contracted edge")?;
    let parallel =
        GraphDelta::identity().with_added_edge(taken, GEdge::new(0, 2, edge("a", "b", "ab")));
    let step = lens.get_edit(&parallel)?;
    let moved = synth_edge_id(&lens, 0, 2).ok_or("no contracted edge")?;
    assert_ne!(moved, taken);
    // The new source edge has the payload the synthesized edge had, so the
    // view keeps an equal edge at that id and gains the moved one.
    assert!(step.view_delta.del_edges().is_empty());
    assert_eq!(
        step.view_delta
            .add_edges()
            .keys()
            .copied()
            .collect::<Vec<_>>(),
        vec![moved]
    );
    assert!(lens.complement().synthesized_edges.get(&taken).is_none());
    assert_matches_whole_state(&lens)?;
    Ok(())
}

#[test]
fn a_parallel_kept_edge_translates_alone() -> TestResult {
    let mut lens = GraphEditLens::new(contraction_lens(true)?, source())?;
    let parallel =
        GraphDelta::identity().with_added_edge(EdgeId(9), GEdge::new(0, 2, edge("a", "b", "ab")));
    let step = lens.get_edit(&parallel)?;
    assert_eq!(step.view_delta, parallel);
    assert_matches_whole_state(&lens)?;
    Ok(())
}

#[test]
fn deleting_a_node_with_its_incident_edges_cascades_into_the_view() -> TestResult {
    let mut lens = GraphEditLens::new(contraction_lens(true)?, source())?;
    let cascade = GraphEdit::remove_node_cascade(lens.source(), 2)?.to_delta()?;
    let step = lens.get_edit(&cascade)?;
    assert!(step.view_delta.del_nodes().contains_key(&2));
    assert!(step.view_delta.del_edges().contains_key(&EdgeId(2)));
    assert!(lens.view().node(2).is_none());
    assert_matches_whole_state(&lens)?;
    Ok(())
}

#[test]
fn reconnecting_a_hidden_edge_moves_the_contracted_pair() -> TestResult {
    let mut lens = GraphEditLens::new(contraction_lens(true)?, source())?;
    let reconnect = GraphEdit::reconnect(lens.source(), EdgeId(1), 1, 5)?.to_delta()?;
    lens.get_edit(&reconnect)?;
    assert!(synth_edge_id(&lens, 0, 2).is_none());
    assert!(synth_edge_id(&lens, 0, 5).is_some());
    assert_matches_whole_state(&lens)?;
    Ok(())
}

#[test]
fn replacing_a_node_with_an_equal_payload_is_a_view_replacement() -> TestResult {
    let mut lens = GraphEditLens::new(contraction_lens(true)?, source())?;
    let cascade = GraphEdit::remove_node_cascade(lens.source(), 2)?.to_delta()?;
    let replacement = cascade.compose(&cascade.invert())?;
    let step = lens.get_edit(&replacement)?;
    assert!(step.view_delta.del_nodes().contains_key(&2));
    assert!(step.view_delta.add_nodes().contains_key(&2));
    assert_matches_whole_state(&lens)?;
    Ok(())
}

#[test]
fn an_edit_that_does_not_apply_is_refused_and_changes_nothing() -> TestResult {
    let mut lens = GraphEditLens::new(contraction_lens(true)?, source())?;
    let before = snapshot(&lens);
    let dangling = GraphDelta::identity().with_deleted_node(Node::new(2, "b"));
    let refusal = lens.get_edit(&dangling);
    assert!(matches!(
        refusal,
        Err(GraphEditLensError::Apply(EditError::DanglingEdges { .. }))
    ));
    let stale = GraphDelta::identity().with_deleted_node(Node::new(0, "b"));
    assert!(matches!(
        lens.get_edit(&stale),
        Err(GraphEditLensError::Apply(EditError::StaleNode { node: 0 }))
    ));
    let missing =
        GraphDelta::identity().with_added_edge(EdgeId(9), GEdge::new(0, 77, edge("a", "b", "ab")));
    assert!(matches!(
        lens.get_edit(&missing),
        Err(GraphEditLensError::Apply(EditError::MissingEndpoint { .. }))
    ));
    assert_eq!(snapshot(&lens), before);
    Ok(())
}

#[test]
fn an_edit_outside_the_get_domain_is_refused_like_whole_state_get() -> TestResult {
    let start = GInstance::new()
        .with_node(Node::new(0, "a"))
        .with_node(Node::new(2, "b"));
    let mut lens = GraphEditLens::new(contraction_lens(false)?, start)?;
    let before = snapshot(&lens);
    let path = GraphDelta::identity()
        .with_added_node(Node::new(1, "h"))
        .with_added_edge(EdgeId(0), GEdge::new(0, 1, edge("a", "h", "ah")))
        .with_added_edge(EdgeId(1), GEdge::new(1, 2, edge("h", "b", "hb")));
    let mut edited = lens.source().clone();
    path.apply(&mut edited)?;
    let whole = graph_lens::get(lens.lens(), &edited)
        .err()
        .ok_or("whole-state get accepts")?;
    match lens.get_edit(&path) {
        Err(GraphEditLensError::Get(refusal)) => {
            assert_eq!(format!("{refusal:?}"), format!("{whole:?}"));
        }
        other => return Err(format!("expected a get refusal, found {other:?}").into()),
    }
    assert_eq!(snapshot(&lens), before);
    Ok(())
}

#[test]
fn a_batch_is_admitted_whole_or_not_at_all() -> TestResult {
    let mut lens = GraphEditLens::new(contraction_lens(true)?, source())?;
    let hidden = lens
        .source()
        .edge(EdgeId(1))
        .ok_or("no hidden edge")?
        .clone();
    let first = GraphDelta::identity().with_deleted_edge(EdgeId(1), hidden);
    let stale = GraphDelta::identity().with_deleted_node(Node::new(0, "b"));
    let before = snapshot(&lens);
    assert!(lens.get_batch(&[first.clone(), stale]).is_err());
    assert_eq!(snapshot(&lens), before);

    let second = GraphDelta::identity().with_deleted_node(Node::new(1, "h"));
    let cut = GraphDelta::identity().with_deleted_edge(
        EdgeId(0),
        lens.source().edge(EdgeId(0)).ok_or("no edge")?.clone(),
    );
    let mut sequential = lens.clone();
    sequential.get_edit(&first)?;
    sequential.get_edit(&cut)?;
    sequential.get_edit(&second)?;
    lens.get_batch(&[first, cut, second])?;
    assert_eq!(
        normal(lens.view(), lens.complement()),
        normal(sequential.view(), sequential.complement())
    );
    assert_matches_whole_state(&lens)?;
    Ok(())
}

#[test]
fn a_view_deletion_puts_back_through_the_hidden_path() -> TestResult {
    let mut lens = GraphEditLens::new(contraction_lens(true)?, source())?;
    let view_cascade = GraphEdit::remove_node_cascade(lens.view(), 5)?.to_delta()?;
    let step = lens.put_edit(&view_cascade)?;
    assert!(step.source_delta.del_nodes().contains_key(&5));
    assert!(step.source_delta.del_edges().contains_key(&EdgeId(4)));
    assert!(lens.source().node(5).is_none());
    assert_eq!(step.view_delta, view_cascade);
    assert_matches_whole_state(&lens)?;
    Ok(())
}

#[test]
fn a_view_node_added_puts_back_as_a_source_node() -> TestResult {
    let mut lens = GraphEditLens::new(contraction_lens(true)?, source())?;
    let add = GraphDelta::identity().with_added_node(Node::new(20, "a"));
    let step = lens.put_edit(&add)?;
    assert_eq!(step.source_delta, add);
    assert_matches_whole_state(&lens)?;
    Ok(())
}

#[test]
fn deleting_a_contracted_edge_in_the_view_is_refused_by_put_and_changes_nothing() -> TestResult {
    let mut lens = GraphEditLens::new(contraction_lens(true)?, source())?;
    let contracted = synth_edge_id(&lens, 3, 5).ok_or("no contracted edge")?;
    let payload = lens.view().edge(contracted).ok_or("no view edge")?.clone();
    let before = snapshot(&lens);
    match lens.put_edit(&GraphDelta::identity().with_deleted_edge(contracted, payload)) {
        Err(GraphEditLensError::Put(refusal)) => assert!(matches!(
            *refusal,
            LensError::PutDomain(ref violation)
                if matches!(**violation, PutDomainError::SynthesizedEdited { .. })
        )),
        other => return Err(format!("expected a put refusal, found {other:?}").into()),
    }
    assert_eq!(snapshot(&lens), before);
    Ok(())
}

#[test]
fn a_lens_with_a_survival_predicate_translates_over_the_whole_source() -> TestResult {
    let mut lens = contraction_lens(true)?;
    lens.compiled
        .conditional_survival
        .insert(Name::from("b"), Expr::Lit(Literal::Bool(true)));
    let mut lens = GraphEditLens::new(lens, source())?;
    let hidden = lens
        .source()
        .edge(EdgeId(4))
        .ok_or("no hidden edge")?
        .clone();
    lens.get_edit(&GraphDelta::identity().with_deleted_edge(EdgeId(4), hidden.clone()))?;
    assert_matches_whole_state(&lens)?;
    lens.get_edit(&GraphDelta::identity().with_added_edge(EdgeId(4), hidden))?;
    assert_matches_whole_state(&lens)?;
    Ok(())
}
