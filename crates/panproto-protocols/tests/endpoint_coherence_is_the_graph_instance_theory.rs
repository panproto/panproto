//! The validator's `EndpointCoherence` check is the coherence equations of
//! the graph instance theory.
//!
//! `th_graph_instance` states that an instance edge's endpoints are anchored
//! at the endpoints of the schema edge it is labelled with
//! (`src(ie_anchor(e)) = iv_anchor(i_src(e))`, and likewise for targets).
//! `graph_instance_model` reads a graph as a model of the theory, and
//! `verify_model` checks the equations on it. Over every graph of up to four
//! nodes and four edges, the edges and sides `EndpointCoherence` reports are
//! exactly the edges and equations `verify_model` refutes, and the check
//! completes: the carriers stay within the default assignment bound.
//!
//! The graphs are enumerated up to renaming their nodes: nodes are numbered
//! by anchor (every `a` node before every `b` node), which reaches a graph
//! isomorphic to each graph, and both the check and the equations are
//! invariant under renaming nodes. Edges are enumerated as multisets of
//! (source, target, label), since parallel edges differ only in their ids.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeSet;

use panproto_gat::{CheckModelOptions, ModelValue, verify_model};
use panproto_inst::validation::{
    CheckKind, EndpointSide, ExternalVerdicts, ValidationBudget, ValidationOptions, ValidationPlan,
    Violation,
};
use panproto_inst::{GInstance, Node, ViewRef};
use panproto_protocols::graph_instance_model;
use panproto_protocols::theories::th_graph_instance;
use panproto_schema::{Edge, Protocol, Schema, Vertex};

fn edge(src: &str, tgt: &str, name: &str) -> Edge {
    Edge {
        src: src.into(),
        tgt: tgt.into(),
        kind: "prop".into(),
        name: Some(name.into()),
    }
}

/// Vertices `a` and `b`, edges `ab: a -> b` and `aa: a -> a`.
fn schema() -> Schema {
    let mut schema = Schema::default();
    for v in ["a", "b"] {
        schema.vertices.insert(
            v.into(),
            Vertex {
                id: v.into(),
                kind: "object".into(),
                nsid: None,
            },
        );
    }
    for e in [edge("a", "b", "ab"), edge("a", "a", "aa")] {
        schema.edges.insert(e.clone(), e.kind.clone());
        schema
            .outgoing
            .entry(e.src.clone())
            .or_default()
            .push(e.clone());
        schema
            .incoming
            .entry(e.tgt.clone())
            .or_default()
            .push(e.clone());
        schema
            .between
            .entry((e.src.clone(), e.tgt.clone()))
            .or_default()
            .push(e);
    }
    schema
}

/// Every non-decreasing sequence of at most `max` indices below `options`:
/// the multisets of at most `max` options.
fn multisets(options: usize, max: usize) -> Vec<Vec<usize>> {
    let mut out = vec![Vec::new()];
    let mut frontier = vec![Vec::new()];
    for _ in 0..max {
        let mut next = Vec::new();
        for seq in &frontier {
            let from = seq.last().copied().unwrap_or(0);
            for o in from..options {
                let mut s: Vec<usize> = seq.clone();
                s.push(o);
                next.push(s);
            }
        }
        out.extend(next.iter().cloned());
        frontier = next;
    }
    out
}

/// The edges and sides the validator reports incoherent.
fn reported(plan: &ValidationPlan, graph: &GInstance) -> BTreeSet<(u32, &'static str)> {
    plan.validate_graph(
        graph,
        &ValidationBudget::default(),
        &ExternalVerdicts::default(),
    )
    .diagnostics
    .iter()
    .filter(|d| d.check.kind == CheckKind::EndpointCoherence)
    .map(|d| match (&d.subject, &d.violation) {
        (Some(ViewRef::Edge(e)), Violation::IncoherentEndpoint { side, .. }) => (
            e.0,
            match side {
                EndpointSide::Src => "ie_src_anchor",
                EndpointSide::Tgt => "ie_tgt_anchor",
            },
        ),
        other => panic!("not an endpoint diagnostic: {other:?}"),
    })
    .collect()
}

/// The edges and equations `verify_model` refutes, and whether it
/// completed.
fn refuted(graph: &GInstance, schema: &Schema) -> (BTreeSet<(u32, &'static str)>, bool) {
    let theory = th_graph_instance();
    let report = verify_model(
        &graph_instance_model(graph, schema),
        &theory,
        &CheckModelOptions::default(),
    );
    let set = report
        .violations
        .iter()
        .map(|v| {
            let e = match v.assignment.get("e") {
                Some(ModelValue::Int(n)) => u32::try_from(*n).unwrap(),
                other => panic!("an edge assignment, not {other:?}"),
            };
            let equation = match v.equation.as_ref() {
                "ie_src_anchor" => "ie_src_anchor",
                "ie_tgt_anchor" => "ie_tgt_anchor",
                other => panic!("an unexpected equation {other}"),
            };
            (e, equation)
        })
        .collect();
    (set, report.incomplete.is_none())
}

/// Check every graph with nodes anchored by `anchors` (ascending by node
/// id), up to `max_nodes` nodes and `max_edges` edges labelled from
/// `labels`; returns the number of graphs and of incoherent ones.
fn sweep(
    schema: &Schema,
    anchors: &[&str],
    labels: &[Edge],
    max_nodes: u32,
    max_edges: usize,
) -> (usize, usize) {
    let plan = ValidationPlan::compile(schema, &Protocol::default(), &ValidationOptions::default());
    let mut work = Vec::new();
    for n in 0..=max_nodes {
        // Anchor assignments up to renaming nodes: a non-decreasing
        // sequence of anchors.
        for assignment in multisets(anchors.len(), n as usize)
            .into_iter()
            .filter(|a| a.len() == n as usize)
        {
            work.push((n, assignment));
        }
    }
    // Each anchor assignment is swept on its own thread; every thread is
    // started before any is joined.
    std::thread::scope(|scope| {
        let plan = &plan;
        let mut handles = Vec::with_capacity(work.len());
        for (n, assignment) in &work {
            handles.push(scope.spawn(move || {
                sweep_one(plan, schema, anchors, labels, *n, assignment, max_edges)
            }));
        }
        let (mut graphs, mut incoherent) = (0, 0);
        for handle in handles {
            let (g, i) = handle.join().expect("a sweep thread finished");
            graphs += g;
            incoherent += i;
        }
        (graphs, incoherent)
    })
}

/// [`sweep`] over the graphs of `n` nodes anchored by `assignment`.
fn sweep_one(
    plan: &ValidationPlan,
    schema: &Schema,
    anchors: &[&str],
    labels: &[Edge],
    n: u32,
    assignment: &[usize],
    max_edges: usize,
) -> (usize, usize) {
    let (mut graphs, mut incoherent) = (0, 0);
    let options: Vec<(u32, u32, &Edge)> = (0..n)
        .flat_map(|s| (0..n).flat_map(move |t| labels.iter().map(move |l| (s, t, l))))
        .collect();
    for chosen in multisets(options.len(), max_edges) {
        let mut graph = GInstance::new();
        for (id, a) in (0..n).zip(assignment) {
            graph = graph.with_node(Node::new(id, anchors[*a]));
        }
        for o in &chosen {
            let (s, t, l) = options[*o];
            graph = graph.with_edge(s, t, l.clone());
        }
        let expected = reported(plan, &graph);
        let (found, complete) = refuted(&graph, schema);
        assert!(complete, "verify_model did not complete on {graph:?}");
        assert_eq!(expected, found, "{graph:?}");
        graphs += 1;
        if !expected.is_empty() {
            incoherent += 1;
        }
    }
    (graphs, incoherent)
}

#[test]
fn v10_endpoint_coherence_agrees_with_the_graph_instance_theory() {
    let schema = schema();
    let (graphs, incoherent) = sweep(
        &schema,
        &["a", "b"],
        &[edge("a", "b", "ab"), edge("a", "a", "aa")],
        4,
        4,
    );
    assert!(graphs > 300_000, "{graphs} graphs");
    assert!(
        incoherent > graphs / 2,
        "{incoherent} of {graphs} graphs incoherent"
    );
}

/// The same agreement with an anchor the schema does not declare and a
/// label it does not declare (`zb: z -> b`), on every graph of up to three
/// nodes and three edges.
#[test]
fn v10_endpoint_coherence_agrees_on_undeclared_anchors_and_labels() {
    let schema = schema();
    let (graphs, incoherent) = sweep(
        &schema,
        &["a", "b", "z"],
        &[
            edge("a", "b", "ab"),
            edge("a", "a", "aa"),
            edge("z", "b", "zb"),
        ],
        3,
        3,
    );
    assert!(graphs > 10_000, "{graphs} graphs");
    assert!(incoherent > 0);
}
