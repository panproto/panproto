//! A diff and its classification depend on the two schemas and nothing else.
//!
//! Every table of a [`Schema`] is a `HashMap`, which enumerates its entries in
//! an order its own random hash keys decide. A diff built by walking those
//! tables as they come, or carrying a `HashMap` of its own, lists the same
//! changes in a different order every time the schemas are rebuilt, so its
//! encoding, its rendered report and every host comparing either see a change
//! where there is none.
//!
//! Each test rebuilds a pair of schemas that differ in every category the diff
//! records, many times over and on several threads, each rebuild drawing new
//! hash keys, and requires one answer: the same value, the same `Debug`
//! rendering and the same JSON bytes.

use std::collections::{BTreeSet, HashMap};

use panproto_check::{classify, diff, report_json, report_text};
use panproto_gat::Name;
use panproto_schema::{
    Constraint, Edge, EdgeRule, HyperEdge, Protocol, RecursionPoint, Schema, Span, UsageMode,
    Variant, Vertex,
};

/// How many entries each category holds. Enough that an order chosen by the
/// hash keys is overwhelmingly unlikely to repeat by chance.
const WIDTH: usize = 12;

/// How many times each pair is rebuilt in one thread.
const REBUILDS: usize = 24;

/// How many threads rebuild it as well.
const THREADS: usize = 4;

fn name(text: &str) -> Name {
    Name::from(text)
}

fn edge(src: &str, tgt: &str, label: &str) -> Edge {
    Edge {
        src: name(src),
        tgt: name(tgt),
        kind: name("prop"),
        name: Some(name(label)),
    }
}

/// One side of the pair. `side` is 0 for the old schema and 1 for the new;
/// every category gets entries only one side has, entries both have alike,
/// and entries both have that differ.
fn schema(side: usize) -> Schema {
    let mut s = Schema {
        protocol: "determinism".into(),
        ..Default::default()
    };
    let other = 1 - side;
    for i in 0..WIDTH {
        // Vertices: shared ones whose kind flips, and ones only this side has.
        let shared = format!("v{i}");
        let kind = if i % 2 == 0 {
            "object"
        } else {
            ["string", "integer"][side]
        };
        s.vertices.insert(
            name(&shared),
            Vertex {
                id: name(&shared),
                kind: name(kind),
                nsid: None,
            },
        );
        let own = format!("only{side}_{i}");
        s.vertices.insert(
            name(&own),
            Vertex {
                id: name(&own),
                kind: name("object"),
                nsid: None,
            },
        );

        let own_edge = edge(&shared, &own, &format!("e{side}_{i}"));
        s.edges.insert(own_edge.clone(), name("e"));
        let shared_edge = edge(&shared, &format!("v{}", (i + 1) % WIDTH), "next");
        s.edges.insert(shared_edge.clone(), name("e"));

        // Constraints: one added, one removed, one changed on each vertex.
        s.constraints.insert(
            name(&shared),
            vec![
                Constraint {
                    sort: name(&format!("only{side}")),
                    value: "1".into(),
                },
                Constraint {
                    sort: name("maxLength"),
                    value: format!("{}", 10 + side),
                },
            ],
        );

        // Hyper-edges whose signatures gain, lose and retarget labels.
        s.hyper_edges.insert(
            name(&format!("h{i}")),
            HyperEdge {
                id: name(&format!("h{i}")),
                kind: name("rel"),
                signature: (0..4)
                    .map(|j| (name(&format!("l{side}_{j}")), name(&shared)))
                    .chain(std::iter::once((
                        name("moved"),
                        name(&format!("v{}", i + side)),
                    )))
                    .collect(),
                parent_label: name("p"),
            },
        );

        s.required
            .insert(name(&shared), vec![own_edge.clone(), shared_edge.clone()]);
        s.nsids
            .insert(name(&own), name(&format!("com.example.{side}.{i}")));
        s.nsids.insert(
            name(&shared),
            name(&format!("com.example.shared{side}.{i}")),
        );

        s.variants.insert(
            name(&shared),
            vec![
                Variant {
                    id: name(&format!("var{side}_{i}")),
                    parent_vertex: name(&shared),
                    tag: None,
                },
                Variant {
                    id: name("tagged"),
                    parent_vertex: name(&shared),
                    tag: Some(name(&format!("t{side}"))),
                },
            ],
        );

        s.orderings
            .insert(shared_edge.clone(), u32::try_from(i + side).unwrap_or(0));
        s.orderings.insert(own_edge.clone(), 0);

        s.recursion_points.insert(
            name(&format!("mu{side}_{i}")),
            RecursionPoint {
                target_vertex: name(&shared),
            },
        );
        s.recursion_points.insert(
            name(&format!("mu{i}")),
            RecursionPoint {
                target_vertex: name(&format!("v{}", (i + side) % WIDTH)),
            },
        );

        s.usage_modes.insert(
            shared_edge.clone(),
            [UsageMode::Structural, UsageMode::Linear][side].clone(),
        );

        s.spans.insert(
            name(&format!("s{i}")),
            Span {
                id: name(&format!("s{i}")),
                left: name(&shared),
                right: name(&format!("v{}", (i + side) % WIDTH)),
            },
        );
        s.spans.insert(
            name(&format!("s{side}_{i}")),
            Span {
                id: name(&format!("s{side}_{i}")),
                left: name(&shared),
                right: name(&own),
            },
        );

        s.nominal.insert(name(&shared), side == 1);
        s.nominal.insert(name(&format!("only{other}_{i}")), false);
    }
    s
}

fn protocol() -> Protocol {
    Protocol {
        name: "determinism".into(),
        schema_theory: "ThGraph".into(),
        instance_theory: "ThWType".into(),
        edge_rules: vec![EdgeRule {
            edge_kind: "prop".into(),
            src_kinds: vec![],
            tgt_kinds: vec![],
        }],
        obj_kinds: vec!["object".into(), "string".into(), "integer".into()],
        constraint_sorts: vec!["maxLength".into()],
        ..Default::default()
    }
}

/// Everything one rebuild answers, rendered to text and bytes.
fn answer() -> (String, Vec<u8>, String, String, String) {
    let old = schema(0);
    let new = schema(1);
    let computed = diff(&old, &new);
    let report = classify(&computed, &protocol());
    let diff_json =
        serde_json::to_vec(&computed).unwrap_or_else(|e| panic!("a diff serializes to JSON: {e}"));
    (
        format!("{computed:?}"),
        diff_json,
        format!("{report:?}"),
        report_text(&report),
        report_json(&report).to_string(),
    )
}

#[test]
fn the_fixture_exercises_every_ordered_category() {
    let computed = diff(&schema(0), &schema(1));
    let sizes = [
        ("added_vertices", computed.added_vertices.len()),
        ("removed_vertices", computed.removed_vertices.len()),
        ("kind_changes", computed.kind_changes.len()),
        ("added_edges", computed.added_edges.len()),
        ("removed_edges", computed.removed_edges.len()),
        ("modified_constraints", computed.modified_constraints.len()),
        ("modified_hyper_edges", computed.modified_hyper_edges.len()),
        ("added_required", computed.added_required.len()),
        ("removed_required", computed.removed_required.len()),
        ("added_nsids", computed.added_nsids.len()),
        ("removed_nsids", computed.removed_nsids.len()),
        ("changed_nsids", computed.changed_nsids.len()),
        ("added_variants", computed.added_variants.len()),
        ("removed_variants", computed.removed_variants.len()),
        ("modified_variants", computed.modified_variants.len()),
        ("order_changes", computed.order_changes.len()),
        (
            "added_recursion_points",
            computed.added_recursion_points.len(),
        ),
        (
            "removed_recursion_points",
            computed.removed_recursion_points.len(),
        ),
        (
            "modified_recursion_points",
            computed.modified_recursion_points.len(),
        ),
        ("usage_mode_changes", computed.usage_mode_changes.len()),
        ("added_spans", computed.added_spans.len()),
        ("removed_spans", computed.removed_spans.len()),
        ("modified_spans", computed.modified_spans.len()),
        ("nominal_changes", computed.nominal_changes.len()),
    ];
    let thin: Vec<_> = sizes.iter().filter(|(_, n)| *n < 2).collect();
    assert!(
        thin.is_empty(),
        "categories with fewer than two entries cannot show an order: {thin:?}"
    );
    let constraints = computed
        .modified_constraints
        .values()
        .next()
        .unwrap_or_else(|| panic!("a constraint diff"));
    assert!(
        !constraints.added.is_empty()
            && !constraints.removed.is_empty()
            && !constraints.changed.is_empty()
    );
    let hyper = &computed.modified_hyper_edges[0];
    assert!(hyper.signature_added.len() > 1 && hyper.signature_removed.len() > 1);
    let required = computed
        .added_required
        .values()
        .next()
        .unwrap_or_else(|| panic!("a required-edge diff"));
    assert!(!required.is_empty());
}

#[test]
fn a_diff_and_its_report_are_the_same_on_every_rebuild() {
    let mut answers: BTreeSet<(String, Vec<u8>, String, String, String)> = BTreeSet::new();
    for _ in 0..REBUILDS {
        answers.insert(answer());
    }
    let threaded: Vec<_> = std::thread::scope(|scope| {
        let workers: Vec<_> = (0..THREADS).map(|_| scope.spawn(answer)).collect();
        workers
            .into_iter()
            .map(|worker| {
                worker
                    .join()
                    .unwrap_or_else(|_| panic!("a worker thread panicked"))
            })
            .collect()
    });
    answers.extend(threaded);
    let distinct = |pick: fn(&(String, Vec<u8>, String, String, String)) -> Vec<u8>| {
        answers.iter().map(pick).collect::<BTreeSet<_>>().len()
    };
    let counts: HashMap<&str, usize> = [
        ("diff (Debug)", distinct(|a| a.0.clone().into_bytes())),
        ("diff (JSON)", distinct(|a| a.1.clone())),
        ("report (Debug)", distinct(|a| a.2.clone().into_bytes())),
        ("report_text", distinct(|a| a.3.clone().into_bytes())),
        ("report_json", distinct(|a| a.4.clone().into_bytes())),
    ]
    .into_iter()
    .collect();
    let mut unstable: Vec<_> = counts.iter().filter(|(_, n)| **n != 1).collect();
    unstable.sort();
    assert!(
        unstable.is_empty(),
        "{} rebuilds gave several answers: {unstable:?}",
        REBUILDS + THREADS
    );
}
