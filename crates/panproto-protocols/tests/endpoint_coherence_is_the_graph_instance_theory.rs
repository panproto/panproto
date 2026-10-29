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
//! completes: the carriers stay within the default assignment bound. The
//! agreement also holds on a schema whose endpoints have anchor families (a
//! union and its variants, a recursion point, a reference vertex), which
//! the validator accepts anywhere in the family and the model reads by
//! specialising each edge's label to its endpoints.
//!
//! The graphs are enumerated up to renaming their nodes: nodes are numbered
//! by anchor (every `a` node before every `b` node), which reaches a graph
//! isomorphic to each graph, and both the check and the equations are
//! invariant under renaming nodes. Edges are enumerated as multisets of
//! (source, target, label), since parallel edges differ only in their ids.

#![allow(clippy::unwrap_used, clippy::expect_used)]


use panproto_gat::{CheckModelOptions, ModelValue, verify_model};
use panproto_inst::validation::{
    CheckKind, EndpointSide, ExternalVerdicts, ValidationBudget, ValidationOptions, ValidationPlan,
    Violation,
};
use panproto_inst::{GInstance, Node, ViewRef};
use panproto_protocols::graph_instance_model;
use panproto_protocols::theories::th_graph_instance;
use panproto_schema::{Edge, Protocol, RecursionPoint, Schema, Variant, Vertex};

fn assert_reference_validation_on_the_endpoint_carrier() {
    use panproto_reference::lens::ViewRef as ReferenceViewRef;
    use panproto_reference::model::{GEdge as ReferenceEdge, Graph, Node as ReferenceNode, SEdge};
    use panproto_reference::validation::{
        CheckClass as ReferenceClass, CheckKey as ReferenceCheckKey,
        CheckKind as ReferenceCheckKind, CheckStatus as ReferenceStatus,
        Diagnostic as ReferenceDiagnostic, Requirement as ReferenceRequirement,
        SchemaRef as ReferenceSchemaRef, Violation as ReferenceViolation,
    };

    let label = SEdge::new("a", "b", "prop", Some("ab"));
    let schema = panproto_reference::schema::Schema::default()
        .vertex("a", "object")
        .vertex("b", "object")
        .edge("a", "b", "prop", Some("ab"));
    let graph = Graph {
        nodes: [
            (0, ReferenceNode::new(0, "a")),
            (1, ReferenceNode::new(1, "b")),
            (2, ReferenceNode::new(2, "b")),
            (3, ReferenceNode::new(3, "a")),
        ]
        .into(),
        edges: [
            (
                0,
                ReferenceEdge {
                    src: 0,
                    tgt: 1,
                    label: label.clone(),
                    position: None,
                },
            ),
            (
                1,
                ReferenceEdge {
                    src: 2,
                    tgt: 1,
                    label: label.clone(),
                    position: None,
                },
            ),
            (
                2,
                ReferenceEdge {
                    src: 0,
                    tgt: 3,
                    label: label.clone(),
                    position: None,
                },
            ),
        ]
        .into(),
    };
    let report = panproto_reference::validation::validate(&schema, &Default::default(), &graph);
    let diagnostic = |edge, node, source, expected: &str, found: &str| ReferenceDiagnostic {
        check: ReferenceCheckKey {
            kind: ReferenceCheckKind::EndpointCoherence,
            decl: ReferenceSchemaRef::Schema,
        },
        class: ReferenceClass::GraphLocal,
        requirement: ReferenceRequirement::Required,
        subject: Some(ReferenceViewRef::Edge(edge)),
        elements: vec![ReferenceViewRef::Node(node), ReferenceViewRef::Edge(edge)],
        violation: ReferenceViolation::IncoherentEndpoint(
            source,
            expected.to_owned(),
            found.to_owned(),
        ),
    };
    assert_eq!(
        report.diagnostics,
        vec![
            diagnostic(1, 2, true, "a", "b"),
            diagnostic(2, 3, false, "b", "a"),
        ]
    );
    assert!(report.incomplete.is_empty());
    assert_eq!(report.status, ReferenceStatus::Failed);
}

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
    schema_of(
        &[("a", "object"), ("b", "object")],
        &[edge("a", "b", "ab"), edge("a", "a", "aa")],
    )
}

/// The schema of `vertices` (each with its kind) and `edges`.
fn schema_of(vertices: &[(&str, &str)], edges: &[Edge]) -> Schema {
    let mut schema = Schema::default();
    for (v, kind) in vertices {
        schema.vertices.insert(
            (*v).into(),
            Vertex {
                id: (*v).into(),
                kind: (*kind).into(),
                nsid: None,
            },
        );
    }
    for e in edges.iter().cloned() {
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

/// One endpoint finding: the edge, the side, the expected anchor (the
/// label's endpoint vertex) and the found anchor (the endpoint node's).
type Finding = (u32, &'static str, String, String);

/// The endpoint findings the validator reports, with multiplicity, in
/// ascending order.
fn reported(plan: &ValidationPlan, graph: &GInstance) -> Vec<Finding> {
    let mut out: Vec<Finding> = plan
        .validate_graph(
            graph,
            &ValidationBudget::default(),
            &ExternalVerdicts::default(),
        )
        .diagnostics
        .iter()
        .filter(|d| d.check.kind == CheckKind::EndpointCoherence)
        .map(|d| match (&d.subject, &d.violation) {
            (
                Some(ViewRef::Edge(e)),
                Violation::IncoherentEndpoint {
                    side,
                    expected,
                    found,
                },
            ) => (
                e.0,
                match side {
                    EndpointSide::Src => "ie_src_anchor",
                    EndpointSide::Tgt => "ie_tgt_anchor",
                },
                expected.to_string(),
                found.to_string(),
            ),
            other => panic!("not an endpoint diagnostic: {other:?}"),
        })
        .collect();
    out.sort();
    out
}

/// The endpoint findings of the reference validator on the same plain
/// schema and graph. The reference has no anchor families, so it is run on
/// the schemas without unions, recursion points or reference vertices.
fn reference_reported(schema: &Schema, graph: &GInstance) -> Vec<Finding> {
    use panproto_reference::model::{GEdge as ReferenceEdge, Graph, Node as ReferenceNode, SEdge};
    use panproto_reference::validation::Violation as ReferenceViolation;
    let mut reference = panproto_reference::schema::Schema::default();
    for (id, vertex) in &schema.vertices {
        reference = reference.vertex(id, &vertex.kind);
    }
    for edge in schema.edges.keys() {
        reference = reference.edge(&edge.src, &edge.tgt, &edge.kind, edge.name.as_deref());
    }
    let model = Graph {
        nodes: graph
            .max_node_id()
            .map_or(0..0, |max| 0..max + 1)
            .filter_map(|id| {
                graph
                    .node(id)
                    .map(|node| (id, ReferenceNode::new(id, &node.anchor)))
            })
            .collect(),
        edges: graph
            .edges_sorted()
            .map(|(id, edge)| {
                (
                    id.0,
                    ReferenceEdge {
                        src: edge.src,
                        tgt: edge.tgt,
                        label: SEdge::new(
                            &edge.edge.src,
                            &edge.edge.tgt,
                            &edge.edge.kind,
                            edge.edge.name.as_deref(),
                        ),
                        position: edge.position,
                    },
                )
            })
            .collect(),
    };
    let mut out: Vec<Finding> =
        panproto_reference::validation::validate(&reference, &Default::default(), &model)
            .diagnostics
            .iter()
            .filter_map(|d| match (&d.subject, &d.violation) {
                (
                    Some(panproto_reference::lens::ViewRef::Edge(e)),
                    ReferenceViolation::IncoherentEndpoint(source, expected, found),
                ) => Some((
                    *e,
                    if *source {
                        "ie_src_anchor"
                    } else {
                        "ie_tgt_anchor"
                    },
                    expected.clone(),
                    found.clone(),
                )),
                _ => None,
            })
            .collect();
    out.sort();
    out
}

/// The endpoint findings `verify_model` refutes, each completed from the
/// graph with the label's endpoint vertex and the endpoint node's anchor,
/// with multiplicity, in ascending order; and whether it completed.
fn refuted(graph: &GInstance, schema: &Schema) -> (Vec<Finding>, bool) {
    let theory = th_graph_instance();
    let report = verify_model(
        &graph_instance_model(graph, schema),
        &theory,
        &CheckModelOptions::default(),
    );
    let mut out: Vec<Finding> = report
        .violations
        .iter()
        .map(|v| {
            let e = match v.assignment.get("e") {
                Some(ModelValue::Int(n)) => u32::try_from(*n).unwrap(),
                other => panic!("an edge assignment, not {other:?}"),
            };
            let edge = graph
                .edge(panproto_inst::EdgeId(e))
                .unwrap_or_else(|| panic!("edge {e} of the graph"));
            let (equation, expected, node) = match v.equation.as_ref() {
                "ie_src_anchor" => ("ie_src_anchor", &edge.edge.src, edge.src),
                "ie_tgt_anchor" => ("ie_tgt_anchor", &edge.edge.tgt, edge.tgt),
                other => panic!("an unexpected equation {other}"),
            };
            let found = graph
                .node(node)
                .unwrap_or_else(|| panic!("node {node} of the graph"))
                .anchor
                .to_string();
            (e, equation, expected.to_string(), found)
        })
        .collect();
    out.sort();
    (out, report.incomplete.is_none())
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
    with_reference: bool,
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
                sweep_one(
                    plan,
                    schema,
                    anchors,
                    labels,
                    (*n, assignment),
                    max_edges,
                    with_reference,
                )
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
    (n, assignment): (u32, &[usize]),
    max_edges: usize,
    with_reference: bool,
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
        if with_reference {
            assert_eq!(expected, reference_reported(schema, &graph), "{graph:?}");
        }
        graphs += 1;
        if !expected.is_empty() {
            incoherent += 1;
        }
    }
    (graphs, incoherent)
}

#[test]
fn v10_endpoint_coherence_agrees_with_the_graph_instance_theory() {
    assert_reference_validation_on_the_endpoint_carrier();
    let schema = schema();
    let (graphs, incoherent) = sweep(
        &schema,
        &["a", "b"],
        &[edge("a", "b", "ab"), edge("a", "a", "aa")],
        4,
        4,
        true,
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
        true,
    );
    assert!(graphs > 10_000, "{graphs} graphs");
    assert!(incoherent > 0);
}

/// A schema whose endpoints have anchor families: `u` is a union of the
/// variants `w` and `x`, `p` is a recursion point unfolding to `a`, and `f`
/// is a reference vertex whose reference edge `fw` points at `w`. The
/// edges `au: a -> u`, `ap: a -> p` and `af: a -> f` end at the
/// placeholders.
fn family_schema() -> Schema {
    let mut schema = schema_of(
        &[
            ("a", "object"),
            ("u", "union"),
            ("w", "object"),
            ("x", "object"),
            ("p", "object"),
            ("f", "ref"),
        ],
        &[
            edge("a", "u", "au"),
            edge("a", "p", "ap"),
            edge("a", "f", "af"),
            ref_edge("f", "w", "fw"),
        ],
    );
    schema.variants.insert(
        "u".into(),
        ["w", "x"]
            .into_iter()
            .map(|v| Variant {
                id: v.into(),
                parent_vertex: "u".into(),
                tag: None,
            })
            .collect(),
    );
    schema.recursion_points.insert(
        "p".into(),
        RecursionPoint {
            target_vertex: "a".into(),
        },
    );
    schema
}

/// A reference edge named `name`.
fn ref_edge(src: &str, tgt: &str, name: &str) -> Edge {
    Edge {
        kind: "ref".into(),
        ..edge(src, tgt, name)
    }
}

/// An edge into a union vertex whose target is anchored at one of the
/// union's variants is coherent for the validator and satisfies the
/// equations of the model.
#[test]
fn an_edge_into_a_union_may_end_at_a_variant() {
    let schema = family_schema();
    let au = edge("a", "u", "au");
    let graph = GInstance::new()
        .with_node(Node::new(0, "a"))
        .with_node(Node::new(1, "w"))
        .with_edge(0, 1, au);
    let plan =
        ValidationPlan::compile(&schema, &Protocol::default(), &ValidationOptions::default());
    assert_eq!(reported(&plan, &graph), Vec::new());
    assert_eq!(refuted(&graph, &schema), (Vec::new(), true));
}

/// The agreement on a schema with anchor families (a union with two
/// variants, a recursion point and a reference vertex), over every graph of
/// up to three nodes and three edges anchored anywhere in the schema.
#[test]
fn v10_endpoint_coherence_agrees_on_anchor_families() {
    let schema = family_schema();
    let (graphs, incoherent) = sweep(
        &schema,
        &["a", "u", "w", "x", "p", "f"],
        &[
            edge("a", "u", "au"),
            edge("a", "p", "ap"),
            edge("a", "f", "af"),
            ref_edge("f", "w", "fw"),
        ],
        3,
        3,
        false,
    );
    assert!(graphs > 100_000, "{graphs} graphs");
    assert!(
        incoherent > graphs / 2,
        "{incoherent} of {graphs} graphs incoherent"
    );
}
