//! Keys unique within the nearest ancestor of a scope vertex.
//!
//! Under a `Within(v)` key each keyed element belongs to the partition of
//! its nearest ancestor anchored at `v` (the least such when several are
//! equally near), and an element with no such ancestor is outside the key's
//! domain: it joins no partition, duplicates nothing, and is reported by
//! one advisory `OutsideKeyScope` observation that leaves the status
//! unchanged. The oracle below computes the partition by its own
//! breadth-first walk over the instance, without the validator's scope
//! function, and is compared with the validator on every instance of an
//! exhaustive bounded scope and under proptest.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::{BTreeMap, BTreeSet, HashMap};

use panproto_gat::CheckStatus;
use panproto_inst::validation::{
    CheckKind, ExternalVerdicts, Requirement, ValidationBudget, ValidationOptions, ValidationPlan,
    ValidationReport, Violation,
};
use panproto_inst::{FieldPresence, GInstance, Node, Value, ViewRef, WInstance};
use panproto_schema::{
    Edge, InstanceShapeKind, Key, KeyPart, KeyRole, KeyScope, Protocol, Schema, Vertex,
};
use proptest::prelude::*;

fn edge(src: &str, tgt: &str, name: &str) -> Edge {
    Edge {
        src: src.into(),
        tgt: tgt.into(),
        kind: "prop".into(),
        name: Some(name.into()),
    }
}

/// `map -entries-> entry`.
fn e_entries() -> Edge {
    edge("map", "entry", "entries")
}

/// `entry -value-> map`: an entry's value is a nested map.
fn e_value() -> Edge {
    edge("entry", "map", "value")
}

/// `entry -key-> key`.
fn e_key() -> Edge {
    edge("entry", "key", "key")
}

/// The recursive map-in-map schema: maps hold entries, an entry holds its
/// key and may hold a nested map; each entry's key is unique within its
/// nearest enclosing map.
fn schema() -> Schema {
    let mut schema = Schema::default();
    for (id, kind) in [("map", "object"), ("entry", "object"), ("key", "string")] {
        schema.vertices.insert(
            id.into(),
            Vertex {
                id: id.into(),
                kind: kind.into(),
                nsid: None,
            },
        );
    }
    for e in [e_entries(), e_value(), e_key()] {
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
            .push(e.clone());
    }
    schema.declarations.insert_key(
        "entry".into(),
        Key {
            id: "entry_key".into(),
            parts: vec![KeyPart::Path(vec![e_key()])],
            role: KeyRole::Unique,
            scope: KeyScope::Within("map".into()),
        },
    );
    schema
}

fn plan(shape: InstanceShapeKind) -> ValidationPlan {
    ValidationPlan::compile(
        &schema(),
        &Protocol {
            syntactic_shape: shape,
            semantic_shape: shape,
            ..Protocol::default()
        },
        &ValidationOptions::default(),
    )
}

/// One node of a drawn tree: its parent (an index among the nodes before
/// it), its anchor, and for a key node its value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Shape {
    Map,
    Entry,
    Key(u8),
}

impl Shape {
    const fn anchor(self) -> &'static str {
        match self {
            Self::Map => "map",
            Self::Entry => "entry",
            Self::Key(_) => "key",
        }
    }
}

/// The schema edge from a parent of `parent` shape to a child of `child`
/// shape, if the schema declares one.
fn label(parent: Shape, child: Shape) -> Option<Edge> {
    match (parent, child) {
        (Shape::Map, Shape::Entry) => Some(e_entries()),
        (Shape::Entry, Shape::Map) => Some(e_value()),
        (Shape::Entry, Shape::Key(_)) => Some(e_key()),
        _ => None,
    }
}

/// A drawn tree: the root's shape, then each other node's parent index and
/// shape. A node whose parent's shape has no declared edge to it is not
/// drawn (the enumerators and the generator skip it).
#[derive(Clone, Debug)]
struct Drawn {
    root: Shape,
    nodes: Vec<(usize, Shape)>,
}

fn node(id: u32, shape: Shape) -> Node {
    let node = Node::new(id, shape.anchor());
    match shape {
        Shape::Key(v) => node.with_value(FieldPresence::Present(Value::Str(
            ["a", "b"][usize::from(v % 2)].into(),
        ))),
        _ => node,
    }
}

fn shapes(drawn: &Drawn) -> Vec<Shape> {
    std::iter::once(drawn.root)
        .chain(drawn.nodes.iter().map(|(_, s)| *s))
        .collect()
}

fn id(i: usize) -> u32 {
    u32::try_from(i).expect("small trees")
}

fn tree_of(drawn: &Drawn) -> WInstance {
    let shapes = shapes(drawn);
    let nodes: HashMap<u32, Node> = shapes
        .iter()
        .enumerate()
        .map(|(i, s)| (id(i), node(id(i), *s)))
        .collect();
    let arcs = drawn
        .nodes
        .iter()
        .enumerate()
        .map(|(i, (parent, shape))| {
            let label = label(shapes[*parent], *shape).expect("drawn arcs are declared");
            (id(*parent), id(i + 1), label)
        })
        .collect();
    WInstance::new(nodes, arcs, Vec::new(), 0, drawn.root.anchor().into())
}

/// The graph form of a drawn tree, with `extra` additional declared edges
/// (indices into the nodes, as source and target) that give a node a second
/// parent.
fn graph_of(drawn: &Drawn, extra: &[(usize, usize)]) -> GInstance {
    let shapes = shapes(drawn);
    let mut graph = GInstance::new();
    for (i, s) in shapes.iter().enumerate() {
        graph = graph.with_node(node(id(i), *s));
    }
    for (i, (parent, shape)) in drawn.nodes.iter().enumerate() {
        let label = label(shapes[*parent], *shape).expect("drawn arcs are declared");
        graph = graph.with_edge(id(*parent), id(i + 1), label);
    }
    for (s, t) in extra {
        let (s, t) = (s % shapes.len(), t % shapes.len());
        if let Some(label) = label(shapes[s], shapes[t]) {
            graph = graph.with_edge(id(s), id(t), label);
        }
    }
    graph
}

/// The oracle's reading of an instance: each node's anchor and key value,
/// and its parents.
struct Reading {
    anchors: BTreeMap<u32, String>,
    keys: BTreeMap<u32, Vec<String>>,
    parents: BTreeMap<u32, BTreeSet<u32>>,
}

fn read_tree(tree: &WInstance) -> Reading {
    let mut reading = Reading {
        anchors: BTreeMap::new(),
        keys: BTreeMap::new(),
        parents: BTreeMap::new(),
    };
    for (n, node) in &tree.nodes {
        reading.anchors.insert(*n, node.anchor.to_string());
    }
    for (p, c, e) in &tree.arcs {
        reading.parents.entry(*c).or_default().insert(*p);
        if *e == e_key()
            && let Some(FieldPresence::Present(Value::Str(v))) =
                tree.nodes.get(c).and_then(|n| n.value.as_ref())
        {
            reading.keys.entry(*p).or_default().push(v.clone());
        }
    }
    reading
}

fn read_graph(graph: &GInstance) -> Reading {
    let mut reading = Reading {
        anchors: BTreeMap::new(),
        keys: BTreeMap::new(),
        parents: BTreeMap::new(),
    };
    for (n, node) in graph.nodes() {
        reading.anchors.insert(*n, node.anchor.to_string());
    }
    for (_, e) in graph.edges_sorted() {
        reading.parents.entry(e.tgt).or_default().insert(e.src);
        if e.edge == e_key()
            && let Some(FieldPresence::Present(Value::Str(v))) =
                graph.node(e.tgt).and_then(|n| n.value.as_ref())
        {
            reading.keys.entry(e.src).or_default().push(v.clone());
        }
    }
    reading
}

/// The nearest ancestor of `n` anchored at `map`: the least such node at
/// the first level of the backward breadth-first walk that holds one.
fn nearest_map(reading: &Reading, n: u32) -> Option<u32> {
    let mut seen = BTreeSet::from([n]);
    let mut level = BTreeSet::from([n]);
    while !level.is_empty() {
        let next: BTreeSet<u32> = level
            .iter()
            .flat_map(|x| reading.parents.get(x).into_iter().flatten().copied())
            .filter(|p| seen.insert(*p))
            .collect();
        if let Some(found) = next
            .iter()
            .find(|p| reading.anchors.get(p).is_some_and(|a| a == "map"))
        {
            return Some(*found);
        }
        level = next;
    }
    None
}

/// What the oracle expects of the key: the duplicate groups (each sorted,
/// of two or more entries sharing a key value within one map) and the
/// entries outside every map. An entry whose key is absent takes no part
/// (its key value is NULL).
fn expected(reading: &Reading) -> (BTreeSet<Vec<u32>>, BTreeSet<u32>) {
    let mut groups: BTreeMap<(u32, String), Vec<u32>> = BTreeMap::new();
    let mut outside = BTreeSet::new();
    for (n, anchor) in &reading.anchors {
        if anchor != "entry" {
            continue;
        }
        // The key value is the first key read along the `key` edge.
        let Some(value) = reading.keys.get(n).and_then(|vs| vs.first()) else {
            continue;
        };
        match nearest_map(reading, *n) {
            Some(map) => groups.entry((map, value.clone())).or_default().push(*n),
            None => {
                outside.insert(*n);
            }
        }
    }
    let duplicates = groups
        .into_values()
        .filter(|g| g.len() > 1)
        .map(|mut g| {
            g.sort_unstable();
            g
        })
        .collect();
    (duplicates, outside)
}

/// What the validator reported of the key, in the oracle's terms, after
/// checking that each observation is advisory and names its element.
fn observed(report: &ValidationReport) -> (BTreeSet<Vec<u32>>, BTreeSet<u32>) {
    let node = |r: &ViewRef| match r {
        ViewRef::Node(n) => *n,
        other => panic!("a tree or graph element: {other:?}"),
    };
    let mut duplicates = BTreeSet::new();
    let mut outside = BTreeSet::new();
    for d in report
        .diagnostics
        .iter()
        .filter(|d| d.check.kind == CheckKind::KeyUnique)
    {
        match &d.violation {
            Violation::DuplicateKey { elems, .. } => {
                assert_eq!(d.requirement, Requirement::Required);
                duplicates.insert(elems.iter().map(node).collect());
            }
            Violation::OutsideKeyScope { key, scope } => {
                assert_eq!(d.requirement, Requirement::Advisory);
                assert_eq!((key.as_ref(), scope.as_ref()), ("entry_key", "map"));
                let subject = d
                    .subject
                    .as_ref()
                    .expect("an observation names its element");
                assert_eq!(d.elements, vec![subject.clone()]);
                outside.insert(node(subject));
            }
            other => panic!("unexpected key violation {other:?}"),
        }
    }
    (duplicates, outside)
}

/// Compare the validator with the oracle on one instance, and check that the
/// observations change neither status nor coverage.
fn agree(report: &ValidationReport, reading: &Reading) {
    let (duplicates, outside) = expected(reading);
    assert_eq!(
        observed(report),
        (duplicates.clone(), outside),
        "{reading_anchors:?}",
        reading_anchors = reading.anchors
    );
    let key_coverage = report
        .coverage
        .iter()
        .find(|(k, _)| k.kind == CheckKind::KeyUnique)
        .map(|(_, c)| *c)
        .expect("the key compiles");
    assert_eq!(key_coverage.instances, 1);
    assert_eq!(key_coverage.failed, u64::from(!duplicates.is_empty()));
    assert_eq!(key_coverage.passed, u64::from(duplicates.is_empty()));
    let refuted = report
        .diagnostics
        .iter()
        .any(|d| d.requirement == Requirement::Required);
    assert_eq!(
        report.status == CheckStatus::Failed,
        refuted,
        "{:#?}",
        report.diagnostics
    );
}

fn check_tree(drawn: &Drawn) {
    let tree = tree_of(drawn);
    let plan = plan(InstanceShapeKind::Tree);
    let report = plan.validate_tree(
        &tree,
        &ValidationBudget::default(),
        &ExternalVerdicts::default(),
    );
    agree(&report, &read_tree(&tree));
    for d in &report.diagnostics {
        assert!(d.verify(&plan, &tree), "{d:#?}");
    }
}

fn check_graph(drawn: &Drawn, extra: &[(usize, usize)]) {
    let graph = graph_of(drawn, extra);
    let plan = plan(InstanceShapeKind::Graph);
    let report = plan.validate_graph(
        &graph,
        &ValidationBudget::default(),
        &ExternalVerdicts::default(),
    );
    agree(&report, &read_graph(&graph));
    for d in &report.diagnostics {
        assert!(d.verify(&plan, &graph), "{d:#?}");
    }
}

const ALL_SHAPES: [Shape; 4] = [Shape::Map, Shape::Entry, Shape::Key(0), Shape::Key(1)];

/// Every drawn tree of `n` nodes whose arcs are declared: a root of any
/// shape and, for each further node, any earlier parent and any shape the
/// parent's shape has a declared edge to.
fn every_tree(n: usize) -> Vec<Drawn> {
    let mut out: Vec<Drawn> = ALL_SHAPES
        .iter()
        .map(|root| Drawn {
            root: *root,
            nodes: Vec::new(),
        })
        .collect();
    for size in 1..n {
        out = out
            .into_iter()
            .flat_map(|drawn| {
                let shapes = shapes(&drawn);
                (0..size)
                    .flat_map(move |parent| ALL_SHAPES.map(move |shape| (parent, shape)))
                    .filter(move |(parent, shape)| label(shapes[*parent], *shape).is_some())
                    .map(move |(parent, shape)| {
                        let mut next = drawn.clone();
                        next.nodes.push((parent, shape));
                        next
                    })
                    .collect::<Vec<_>>()
            })
            .collect();
    }
    out
}

/// The schema of the fixtures, with a map nested in an entry of another
/// map: an equal key in the inner and the outer map is accepted, and twice
/// in one map is rejected; an entry with no enclosing map is outside the
/// key.
#[test]
fn an_element_without_a_within_ancestor_is_outside_the_key() {
    // Two entries with equal keys and no enclosing map.
    let drawn = Drawn {
        root: Shape::Entry,
        nodes: vec![
            (0, Shape::Key(0)),
            (0, Shape::Map),
            (2, Shape::Entry),
            (3, Shape::Key(0)),
        ],
    };
    let mut graph = graph_of(&drawn, &[]);
    // A second entry outside every map, with the same key as the root.
    graph = graph
        .with_node(Node::new(10, "entry"))
        .with_node(node(11, Shape::Key(0)))
        .with_edge(10, 11, e_key());
    let plan = plan(InstanceShapeKind::Graph);
    let report = plan.validate_graph(
        &graph,
        &ValidationBudget::default(),
        &ExternalVerdicts::default(),
    );
    assert_eq!(
        report.status,
        CheckStatus::Passed,
        "{:#?}",
        report.diagnostics
    );
    assert!(
        !report
            .diagnostics
            .iter()
            .any(|d| matches!(d.violation, Violation::DuplicateKey { .. }))
    );
    let outside: Vec<&ViewRef> = report
        .diagnostics
        .iter()
        .filter(|d| {
            d.requirement == Requirement::Advisory
                && matches!(d.violation, Violation::OutsideKeyScope { .. })
        })
        .filter_map(|d| d.subject.as_ref())
        .collect();
    assert_eq!(outside, vec![&ViewRef::Node(0), &ViewRef::Node(10)]);
    check_graph(&drawn, &[]);
    check_tree(&drawn);
}

#[test]
fn an_equal_key_in_an_inner_and_an_outer_map_is_accepted_and_twice_in_one_map_is_not() {
    // map 0 holds entry 1 (key a) whose value is map 3, which holds entry 4
    // (key a): accepted.
    let nested = Drawn {
        root: Shape::Map,
        nodes: vec![
            (0, Shape::Entry),
            (1, Shape::Key(0)),
            (1, Shape::Map),
            (3, Shape::Entry),
            (4, Shape::Key(0)),
        ],
    };
    let tree = tree_of(&nested);
    let report = plan(InstanceShapeKind::Tree).validate_tree(
        &tree,
        &ValidationBudget::default(),
        &ExternalVerdicts::default(),
    );
    assert_eq!(
        report.status,
        CheckStatus::Passed,
        "{:#?}",
        report.diagnostics
    );
    // The same key twice in map 0: rejected.
    let twice = Drawn {
        root: Shape::Map,
        nodes: vec![
            (0, Shape::Entry),
            (1, Shape::Key(0)),
            (0, Shape::Entry),
            (3, Shape::Key(0)),
        ],
    };
    let tree = tree_of(&twice);
    let report = plan(InstanceShapeKind::Tree).validate_tree(
        &tree,
        &ValidationBudget::default(),
        &ExternalVerdicts::default(),
    );
    assert_eq!(report.status, CheckStatus::Failed);
    assert!(report.diagnostics.iter().any(|d| matches!(
        &d.violation,
        Violation::DuplicateKey { elems, .. } if *elems == vec![ViewRef::Node(1), ViewRef::Node(3)]
    )));
    check_tree(&nested);
    check_tree(&twice);
}

/// Every declared tree of up to six nodes (2770 trees), as a tree and as a
/// graph.
#[test]
fn within_scopes_agree_with_an_independent_partition_oracle() {
    let mut checked = 0_usize;
    for n in 1..=6 {
        for drawn in every_tree(n) {
            check_tree(&drawn);
            check_graph(&drawn, &[]);
            checked += 1;
        }
    }
    assert_eq!(checked, 2770);
}

/// Every declared tree of up to five nodes with every one extra declared
/// edge (9406 graphs), so that an entry or a map may have a second parent
/// and an entry several nearest maps (the least is its scope).
#[test]
fn within_scopes_agree_with_the_oracle_on_every_bounded_two_parent_graph() {
    let mut checked = 0_usize;
    for n in 1..=5 {
        for drawn in every_tree(n) {
            for s in 0..n {
                for t in 0..n {
                    check_graph(&drawn, &[(s, t)]);
                    checked += 1;
                }
            }
        }
    }
    assert_eq!(checked, 9406);
}

fn arb_drawn(max: usize) -> impl Strategy<Value = Drawn> {
    (
        proptest::sample::select(ALL_SHAPES.to_vec()),
        proptest::collection::vec((any::<usize>(), 0..ALL_SHAPES.len()), 0..max),
    )
        .prop_map(|(root, picks)| {
            let mut drawn = Drawn {
                root,
                nodes: Vec::new(),
            };
            for (parent, shape) in picks {
                let shapes = shapes(&drawn);
                let parent = parent % shapes.len();
                let shape = ALL_SHAPES[shape];
                if label(shapes[parent], shape).is_some() {
                    drawn.nodes.push((parent, shape));
                }
            }
            drawn
        })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    #[test]
    fn within_scopes_agree_with_the_oracle_on_generated_instances(
        drawn in arb_drawn(24),
        extra in proptest::collection::vec((any::<usize>(), any::<usize>()), 0..4),
    ) {
        check_tree(&drawn);
        check_graph(&drawn, &extra);
    }
}

/// The canonical bytes of an `OutsideKeyScope` observation: the variant's
/// tag, 20, after the nineteen of `ExternalRejected`, then the key's and the
/// scope's names, each as a little-endian length and its bytes. The report
/// digest is pinned with them.
#[test]
fn an_outside_key_scope_observation_has_pinned_canonical_bytes() {
    use panproto_inst::Digest;
    use panproto_inst::validation::{
        CheckClass, CheckCoverage, CheckKey, Diagnostic, ReportScope, SchemaRef,
    };
    let check = CheckKey::new(
        CheckKind::KeyUnique,
        SchemaRef::Key {
            vertex: "entry".into(),
            key: "k".into(),
        },
    );
    let report = ValidationReport::new(
        InstanceShapeKind::Graph,
        ReportScope::Instance,
        Digest([0; 32]),
        vec![Diagnostic {
            check: check.clone(),
            class: CheckClass::MaintainedGlobal,
            requirement: Requirement::Advisory,
            subject: Some(ViewRef::Node(3)),
            elements: vec![ViewRef::Node(3)],
            violation: Violation::OutsideKeyScope {
                key: "k".into(),
                scope: "list".into(),
            },
            source: None,
        }],
        vec![],
        BTreeMap::from([(
            check,
            CheckCoverage {
                class: CheckClass::MaintainedGlobal,
                requirement: Requirement::Required,
                instances: 1,
                passed: 1,
                failed: 0,
                incomplete: 0,
            },
        )]),
    );
    assert_eq!(report.status, CheckStatus::Passed);
    let bytes = report.canonical_bytes();
    let mut tail = vec![20_u8];
    tail.extend(1_u64.to_le_bytes());
    tail.extend(b"k");
    tail.extend(4_u64.to_le_bytes());
    tail.extend(b"list");
    assert!(
        bytes.windows(tail.len()).any(|w| w == tail.as_slice()),
        "{bytes:?}"
    );
    assert_eq!(
        report.digest(),
        Digest::derive(panproto_inst::identity::VALIDATION_REPORT_CONTEXT, &bytes)
    );
    assert_eq!(report.digest().to_hex(), PINNED_OUTSIDE_REPORT_DIGEST);
}

/// The digest of the report above, as observed when the variant was added.
const PINNED_OUTSIDE_REPORT_DIGEST: &str =
    "cde1cdda6030e6ae74c74c1510c19e70e5d09b389b4423fa4bd7ae4353643218";

/// The map-in-map schema with references: a map also holds `ref` elements,
/// each naming an entry of its map by key through its `wanted` value.
fn schema_with_references() -> Schema {
    use panproto_schema::{Cardinality, OnUnresolved, RefSource, Reference, ResolveMode};
    let mut schema = schema();
    let refs = edge("map", "ref", "refs");
    let wanted = edge("ref", "key", "wanted");
    let resolves = Edge {
        kind: "ref".into(),
        ..edge("ref", "entry", "resolves")
    };
    schema.vertices.insert(
        "ref".into(),
        Vertex {
            id: "ref".into(),
            kind: "object".into(),
            nsid: None,
        },
    );
    for e in [refs, wanted.clone(), resolves.clone()] {
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
    schema.declarations.insert_reference(Reference {
        id: "pick".into(),
        holder: "ref".into(),
        source: RefSource::KeyValue {
            parts: vec![vec![wanted]],
        },
        target_vertex: "entry".into(),
        target_key: Some("entry_key".into()),
        edge: Some(resolves),
        cardinality: Cardinality::One,
        nullable: true,
        on_unresolved: OnUnresolved::Error,
        mode: ResolveMode::AddEdge,
    });
    schema
}

/// The nested-context instance, as a tree and as a graph.
fn nested_context_instance() -> (WInstance, GInstance) {
    let text = |id: u32, value: &str| {
        Node::new(id, "key").with_value(FieldPresence::Present(Value::Str(value.into())))
    };
    let nodes = [
        Node::new(0, "map"),
        Node::new(1, "entry"),
        text(2, "a"),
        Node::new(3, "map"),
        Node::new(4, "entry"),
        text(5, "a"),
        Node::new(6, "entry"),
        text(7, "b"),
        Node::new(8, "ref"),
        text(9, "b"),
        Node::new(10, "ref"),
        text(11, "a"),
    ];
    let arcs = [
        (0, 1, e_entries()),
        (1, 2, e_key()),
        (1, 3, e_value()),
        (3, 4, e_entries()),
        (4, 5, e_key()),
        (3, 6, e_entries()),
        (6, 7, e_key()),
        (0, 8, edge("map", "ref", "refs")),
        (8, 9, edge("ref", "key", "wanted")),
        (0, 10, edge("map", "ref", "refs")),
        (10, 11, edge("ref", "key", "wanted")),
    ];
    let tree = WInstance::new(
        nodes.iter().map(|n| (n.id, n.clone())).collect(),
        arcs.to_vec(),
        Vec::new(),
        0,
        "map".into(),
    );
    let mut graph = GInstance::new();
    for n in &nodes {
        graph = graph.with_node(n.clone());
    }
    for (p, c, e) in &arcs {
        graph = graph.with_edge(*p, *c, e.clone());
    }
    (tree, graph)
}

/// A map 0 holds an entry 1 keyed `a` whose value is a nested map 3 holding
/// an entry 4 keyed `a` and an entry 6 keyed `b`, and a reference 8 held
/// directly by the outer map wanting `b`, which only the inner map's entry
/// carries. The two `a` entries are in the buckets of their own maps, so
/// there is no duplicate; the reference looks in its own map's bucket and
/// does not resolve; a second reference 10 in the outer map wanting `a`
/// resolves to entry 1 alone.
#[test]
fn a_reference_to_a_target_only_in_a_nested_context_does_not_resolve() {
    let (tree, graph) = nested_context_instance();
    for (shape, report) in [
        (
            InstanceShapeKind::Tree,
            plan_of(&schema_with_references(), InstanceShapeKind::Tree).validate_tree(
                &tree,
                &ValidationBudget::default(),
                &ExternalVerdicts::default(),
            ),
        ),
        (
            InstanceShapeKind::Graph,
            plan_of(&schema_with_references(), InstanceShapeKind::Graph).validate_graph(
                &graph,
                &ValidationBudget::default(),
                &ExternalVerdicts::default(),
            ),
        ),
    ] {
        assert!(
            !report
                .diagnostics
                .iter()
                .any(|d| matches!(d.violation, Violation::DuplicateKey { .. })),
            "{shape:?}: {:#?}",
            report.diagnostics
        );
        let references: Vec<(Option<ViewRef>, &Violation)> = report
            .diagnostics
            .iter()
            .filter(|d| d.check.kind == CheckKind::ReferenceResolves)
            .map(|d| (d.subject.clone(), &d.violation))
            .collect();
        assert_eq!(
            references,
            vec![(
                Some(ViewRef::Node(8)),
                &Violation::DanglingReference {
                    reference: "pick".into(),
                    key: vec![Value::Str("b".into())],
                }
            )],
            "{shape:?}"
        );
        assert_eq!(report.status, CheckStatus::Failed, "{shape:?}");
    }
    let plan = plan_of(&schema_with_references(), InstanceShapeKind::Tree);
    let report = plan.validate_tree(
        &tree,
        &ValidationBudget::default(),
        &ExternalVerdicts::default(),
    );
    for d in &report.diagnostics {
        assert!(d.verify(&plan, &tree), "{d:#?}");
    }
    // The oracle's partition of the keyed entries: each in its own map.
    let reading = read_tree(&tree);
    assert_eq!(nearest_map(&reading, 1), Some(0));
    assert_eq!(nearest_map(&reading, 4), Some(3));
    assert_eq!(nearest_map(&reading, 6), Some(3));
    assert_eq!(nearest_map(&reading, 8), Some(0));
    assert_eq!(expected(&reading), (BTreeSet::new(), BTreeSet::new()));
}

/// The plan of `schema` read in `shape`.
fn plan_of(schema: &Schema, shape: InstanceShapeKind) -> ValidationPlan {
    ValidationPlan::compile(
        schema,
        &Protocol {
            syntactic_shape: shape,
            semantic_shape: shape,
            ..Protocol::default()
        },
        &ValidationOptions::default(),
    )
}
