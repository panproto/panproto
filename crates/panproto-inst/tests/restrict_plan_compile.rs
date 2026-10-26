#![allow(clippy::unwrap_used, clippy::expect_used)]
//! The compiled restrict plan: the edge image, the contraction plan (pairs,
//! regions, boundedness, mode, resolution) and the boundary layer.

use std::collections::{BTreeMap, BTreeSet};

use panproto_gat::Name;
use panproto_inst::{
    BoundaryProtocols, CellFate, CompiledMigration, ContractionMode, ContractionPlan, EdgeFate,
    EdgeImage, PairResolution, RestrictError, RestrictPlan, UnresolvedReason, resolve_pair,
};
use panproto_schema::{Edge, Protocol, ScalarKind, Schema, SchemaBuilder};

fn schema_with(protocol: &Protocol, vertices: &[(&str, &str)], edges: &[&Edge]) -> Schema {
    let mut builder = SchemaBuilder::new(protocol);
    for (v, kind) in vertices {
        builder = builder.vertex(v, kind, None).unwrap();
    }
    for edge in edges {
        builder = builder
            .edge(&edge.src, &edge.tgt, &edge.kind, edge.name.as_deref())
            .unwrap();
    }
    builder.build().unwrap()
}

fn schema(vertices: &[&str], edges: &[&Edge]) -> Schema {
    let kinded: Vec<(&str, &str)> = vertices.iter().map(|v| (*v, "record")).collect();
    schema_with(&Protocol::default(), &kinded, edges)
}

fn e(src: &str, tgt: &str, name: &str) -> Edge {
    Edge {
        src: src.into(),
        tgt: tgt.into(),
        kind: "prop".into(),
        name: Some(name.into()),
    }
}

fn keep(vertices: &[&str]) -> CompiledMigration {
    CompiledMigration {
        surviving_verts: vertices.iter().map(|v| Name::from(*v)).collect(),
        ..CompiledMigration::default()
    }
}

fn pair(a: &str, b: &str) -> (Name, Name) {
    (a.into(), b.into())
}

#[test]
fn the_edge_image_has_a_fate_for_every_source_edge() {
    let src = schema(
        &["r", "a", "b", "x"],
        &[&e("r", "a", "a"), &e("r", "b", "b"), &e("r", "x", "x")],
    );
    let tgt = schema(&["r", "a", "b"], &[&e("r", "a", "a")]);
    let image = EdgeImage::compile(&src, &tgt, &keep(&["r", "a", "b"])).unwrap();
    assert_eq!(
        image.fates.keys().collect::<BTreeSet<_>>(),
        src.edges.keys().collect::<BTreeSet<_>>()
    );
    assert_eq!(
        image.fate(&e("r", "a", "a")),
        Some(&EdgeFate::Keep(e("r", "a", "a")))
    );
    // The target has no edge for b: dropped. x does not survive: dropped.
    assert_eq!(image.fate(&e("r", "b", "b")), Some(&EdgeFate::Drop));
    assert_eq!(image.fate(&e("r", "x", "x")), Some(&EdgeFate::Drop));
}

#[test]
fn edge_image_follows_the_tree_arc_decision() {
    // Direct image by remap; by kind and name across a vertex rename; by the
    // resolver for a single source edge; by an expansion path; else dropped.
    let src = schema(
        &["r", "a", "b", "c", "d"],
        &[
            &e("r", "a", "a"),
            &e("r", "b", "b"),
            &e("r", "c", "c"),
            &e("r", "d", "d"),
        ],
    );
    let tgt = schema(
        &["r", "A", "b", "c", "w", "d"],
        &[
            &e("r", "A", "a"),
            &e("r", "b", "bee"),
            &e("r", "w", "w"),
            &e("w", "c", "c"),
        ],
    );
    let mut m = keep(&["r", "A", "b", "c", "w", "d"]);
    m.vertex_remap.insert("a".into(), "A".into());
    m.resolver.insert(pair("r", "b"), e("r", "b", "bee"));
    m.expansion_path.insert(pair("r", "c"), vec!["w".into()]);
    let image = EdgeImage::compile(&src, &tgt, &m).unwrap();
    assert_eq!(
        image.fate(&e("r", "a", "a")),
        Some(&EdgeFate::Keep(e("r", "A", "a")))
    );
    assert_eq!(
        image.fate(&e("r", "b", "b")),
        Some(&EdgeFate::Keep(e("r", "b", "bee")))
    );
    assert_eq!(
        image.fate(&e("r", "c", "c")),
        Some(&EdgeFate::Expand {
            intermediates: vec!["w".into()],
            labels: vec![e("r", "w", "w"), e("w", "c", "c")],
        })
    );
    assert_eq!(image.fate(&e("r", "d", "d")), Some(&EdgeFate::Drop));
}

#[test]
fn preimages_list_every_source_edge_mapped_to_an_image() {
    let src = schema(&["r", "a"], &[&e("r", "a", "x"), &e("r", "a", "y")]);
    let tgt = schema(&["r", "a"], &[&e("r", "a", "z")]);
    let mut m = keep(&["r", "a"]);
    m.edge_remap.insert(e("r", "a", "x"), e("r", "a", "z"));
    m.edge_remap.insert(e("r", "a", "y"), e("r", "a", "z"));
    let image = EdgeImage::compile(&src, &tgt, &m).unwrap();
    assert_eq!(
        image.preimages(&e("r", "a", "z")),
        vec![&e("r", "a", "x"), &e("r", "a", "y")]
    );
    assert!(image.preimages(&e("r", "a", "x")).is_empty());
}

#[test]
fn a_failing_expansion_hop_is_unresolved_at_compile_and_errors_at_get() {
    // The expansion path's second hop, w -> c, has two target edges.
    let src = schema(&["r", "c"], &[&e("r", "c", "c")]);
    let tgt = schema(
        &["r", "c", "w"],
        &[&e("r", "w", "w"), &e("w", "c", "p"), &e("w", "c", "q")],
    );
    let mut m = keep(&["r", "c", "w"]);
    m.expansion_path.insert(pair("r", "c"), vec!["w".into()]);
    let plan = RestrictPlan::compile(&src, &tgt, &m).unwrap();
    assert_eq!(
        plan.edge_image.fate(&e("r", "c", "c")),
        Some(&EdgeFate::Unresolved(UnresolvedReason::Ambiguous {
            count: 2
        }))
    );
    let graph = panproto_inst::GInstance::new()
        .with_node(panproto_inst::Node::new(0, "r"))
        .with_node(panproto_inst::Node::new(1, "c"))
        .with_edge(0, 1, e("r", "c", "c"));
    let error = panproto_inst::graph_restrict_with_plan(&graph, &plan).unwrap_err();
    assert!(
        matches!(
            error.unlocated(),
            RestrictError::AmbiguousEdge { src, tgt, count: 2 } if src == "w" && tgt == "c"
        ),
        "{error:?}"
    );
}

/// `r -> m -> leaf`, `m` dropped.
fn wrapper() -> (Schema, Schema, CompiledMigration) {
    let src = schema(
        &["r", "m", "leaf"],
        &[&e("r", "m", "m"), &e("m", "leaf", "leaf")],
    );
    let tgt = schema(&["r", "leaf"], &[&e("r", "leaf", "leaf")]);
    (src, tgt, keep(&["r", "leaf"]))
}

#[test]
fn the_plan_lists_every_pair_through_a_dropped_region() {
    // A recursive dropped wrapper: r -> m, m -> m, m -> leaf.
    let src = schema(
        &["r", "m", "leaf"],
        &[
            &e("r", "m", "m"),
            &e("m", "m", "self"),
            &e("m", "leaf", "leaf"),
        ],
    );
    let tgt = schema(&["r", "leaf"], &[&e("r", "leaf", "leaf")]);
    let plan = ContractionPlan::compile(&src, &tgt, &keep(&["r", "leaf"])).unwrap();
    assert_eq!(
        plan.pairs.keys().cloned().collect::<Vec<_>>(),
        vec![pair("r", "leaf")]
    );
    let planned = &plan.pairs[&pair("r", "leaf")];
    assert_eq!(planned.target, pair("r", "leaf"));
    assert!(
        !planned.bounded,
        "the loop on m makes walks of every length"
    );
    assert_eq!(
        planned.resolution,
        PairResolution::Label(e("r", "leaf", "leaf"))
    );
}

#[test]
fn contraction_region_is_the_set_of_edges_on_interior_walks() {
    // Fixture 1: edges {a -> x, x -> b, a -> b}, x interior.
    let src = schema(
        &["a", "b", "x"],
        &[&e("a", "x", "ax"), &e("x", "b", "xb"), &e("a", "b", "ab")],
    );
    let tgt = schema(&["a", "b"], &[&e("a", "b", "ab")]);
    let plan = ContractionPlan::compile(&src, &tgt, &keep(&["a", "b"])).unwrap();
    assert_eq!(
        plan.pairs[&pair("a", "b")].region,
        BTreeSet::from([e("a", "x", "ax"), e("x", "b", "xb")])
    );
    // Fixture 2: the same with a conditional: a is both endpoint and
    // interior, and its direct edge to b lies on no walk through an
    // interior vertex.
    let mut m = keep(&["a", "b"]);
    m.add_conditional_survival(
        "a",
        panproto_expr::Expr::Lit(panproto_expr::Literal::Bool(true)),
    );
    let plan = ContractionPlan::compile(&src, &tgt, &m).unwrap();
    assert_eq!(
        plan.pairs[&pair("a", "b")].region,
        BTreeSet::from([e("a", "x", "ax"), e("x", "b", "xb")])
    );
}

/// Every edge on some walk `a -> .. -> b` of length at least two whose inner
/// vertices are interior-capable, by enumerating walks up to a length bound:
/// an edge `u -> v` on such a walk lies on one made of a simple walk to `u`,
/// the edge, and a simple walk from `v`, of length at most `2 n + 1`.
fn brute_region(
    edges: &[(usize, usize)],
    interior: &[bool],
    a: usize,
    b: usize,
) -> BTreeSet<usize> {
    let n = interior.len();
    let mut region = BTreeSet::new();
    // States: (current vertex, edges used so far), BFS over walk prefixes
    // whose inner vertices are interior, up to length 2 n + 1.
    let mut frontier: Vec<(usize, Vec<usize>)> = vec![(a, Vec::new())];
    for _ in 0..=2 * n {
        let mut next = Vec::new();
        for (at, used) in &frontier {
            for (i, &(u, v)) in edges.iter().enumerate() {
                if u != *at {
                    continue;
                }
                let mut walk = used.clone();
                walk.push(i);
                if v == b && walk.len() >= 2 {
                    region.extend(walk.iter().copied());
                }
                if interior[v] && walk.len() <= 2 * n {
                    next.push((v, walk));
                }
            }
        }
        frontier = next;
        // Keep the search finite: a prefix only matters by its last vertex
        // and the edges it used, so drop repeated (vertex, edge set) states.
        frontier.sort();
        frontier.dedup();
    }
    region
}

#[test]
fn contraction_regions_agree_with_walk_enumeration() {
    // Every graph on three vertices with up to four edges from a fixed
    // pool, every capability assignment: the compiled region of each
    // planned pair is the set of edges on its interior walks.
    let names = ["v0", "v1", "v2"];
    let pool: Vec<(usize, usize)> = (0..3).flat_map(|u| (0..3).map(move |v| (u, v))).collect();
    let mut checked = 0;
    for mask in 0u32..(1 << pool.len()) {
        if mask.count_ones() > 4 {
            continue;
        }
        let edges: Vec<(usize, usize)> = pool
            .iter()
            .enumerate()
            .filter(|(i, _)| mask & (1 << i) != 0)
            .map(|(_, e)| *e)
            .collect();
        let labelled: Vec<Edge> = edges
            .iter()
            .map(|(u, v)| e(names[*u], names[*v], &format!("{u}{v}")))
            .collect();
        let src = schema(&names, &labelled.iter().collect::<Vec<_>>());
        for capabilities in 0u32..27 {
            // 0: survives, 1: dropped, 2: conditional.
            let caps: Vec<u32> = (0..3).map(|i| (capabilities / 3_u32.pow(i)) % 3).collect();
            let surviving: Vec<&str> = (0..3).filter(|i| caps[*i] != 1).map(|i| names[i]).collect();
            let mut m = keep(&surviving);
            for i in 0..3 {
                if caps[i] == 2 {
                    m.add_conditional_survival(
                        names[i],
                        panproto_expr::Expr::Lit(panproto_expr::Literal::Bool(true)),
                    );
                }
            }
            // The target needs one vertex at least; `sink` is no source vertex.
            let mut target_vertices = surviving.clone();
            target_vertices.push("sink");
            let tgt = schema(&target_vertices, &[]);
            let plan = ContractionPlan::compile(&src, &tgt, &m).unwrap();
            let interior: Vec<bool> = caps.iter().map(|c| *c != 0).collect();
            for a in 0..3 {
                for b in 0..3 {
                    let expected = if caps[a] == 1 || caps[b] == 1 {
                        BTreeSet::new()
                    } else {
                        brute_region(&edges, &interior, a, b)
                    };
                    let compiled = plan
                        .pairs
                        .get(&pair(names[a], names[b]))
                        .map(|p| p.region.clone())
                        .unwrap_or_default();
                    let expected: BTreeSet<Edge> =
                        expected.into_iter().map(|i| labelled[i].clone()).collect();
                    assert_eq!(
                        compiled, expected,
                        "edges {edges:?}, capabilities {caps:?}, pair ({a}, {b})"
                    );
                    checked += 1;
                }
            }
        }
    }
    assert!(checked > 10_000, "{checked} cases");
}

#[test]
fn a_cyclic_region_is_unbounded() {
    // A dense strongly connected dropped region between r and leaf.
    let src = schema(
        &["r", "x", "y", "leaf"],
        &[
            &e("r", "x", "rx"),
            &e("x", "y", "xy"),
            &e("y", "x", "yx"),
            &e("y", "leaf", "yl"),
        ],
    );
    let tgt = schema(&["r", "leaf"], &[&e("r", "leaf", "leaf")]);
    let plan = ContractionPlan::compile(&src, &tgt, &keep(&["r", "leaf"])).unwrap();
    assert!(!plan.pairs[&pair("r", "leaf")].bounded);
    assert_eq!(plan.mode(), ContractionMode::Closure);
}

#[test]
fn contraction_mode_follows_the_pairs() {
    let (src, tgt, m) = wrapper();
    assert_eq!(
        ContractionPlan::compile(&src, &tgt, &m).unwrap().mode(),
        ContractionMode::Declared
    );
    let none = ContractionPlan::compile(&src, &src, &keep(&["r", "m", "leaf"])).unwrap();
    assert_eq!(none.mode(), ContractionMode::None);
    let looped = schema(
        &["r", "m", "leaf"],
        &[
            &e("r", "m", "m"),
            &e("m", "m", "self"),
            &e("m", "leaf", "leaf"),
        ],
    );
    assert_eq!(
        ContractionPlan::compile(&looped, &tgt, &m).unwrap().mode(),
        ContractionMode::Closure
    );
}

#[test]
fn a_conditional_vertex_is_endpoint_and_interior() {
    // r -> x -> c -> leaf, x dropped, c conditional: c ends the pair
    // (r, c) through x, and lies inside the pair (r, leaf).
    let src = schema(
        &["r", "x", "c", "leaf"],
        &[
            &e("r", "x", "x"),
            &e("x", "c", "c"),
            &e("c", "leaf", "leaf"),
        ],
    );
    let tgt = schema(
        &["r", "c", "leaf"],
        &[
            &e("r", "c", "c"),
            &e("c", "leaf", "leaf"),
            &e("r", "leaf", "leaf"),
        ],
    );
    let mut m = keep(&["r", "c", "leaf"]);
    m.add_conditional_survival(
        "c",
        panproto_expr::Expr::Lit(panproto_expr::Literal::Bool(true)),
    );
    let plan = ContractionPlan::compile(&src, &tgt, &m).unwrap();
    assert_eq!(
        plan.pairs.keys().cloned().collect::<Vec<_>>(),
        vec![pair("r", "c"), pair("r", "leaf")]
    );
    assert_eq!(
        plan.pairs[&pair("r", "leaf")].region,
        BTreeSet::from([e("r", "x", "x"), e("x", "c", "c"), e("c", "leaf", "leaf")])
    );
}

#[test]
fn resolve_pair_follows_the_tree_order() {
    let tgt = schema(
        &["a", "b", "c", "w"],
        &[
            &e("a", "b", "one"),
            &e("a", "c", "p"),
            &e("a", "c", "q"),
            &e("a", "w", "w"),
            &e("w", "c", "c"),
        ],
    );
    // A resolver entry wins.
    let mut m = keep(&["a", "b", "c", "w"]);
    m.resolver.insert(pair("a", "c"), e("a", "c", "q"));
    assert_eq!(
        resolve_pair(&tgt, &m, &"a".into(), &"c".into()),
        PairResolution::Label(e("a", "c", "q"))
    );
    // Else the unique edge.
    let m = keep(&["a", "b", "c", "w"]);
    assert_eq!(
        resolve_pair(&tgt, &m, &"a".into(), &"b".into()),
        PairResolution::Label(e("a", "b", "one"))
    );
    // Else, with several edges, the expansion path.
    let mut m = keep(&["a", "b", "c", "w"]);
    m.expansion_path.insert(pair("a", "c"), vec!["w".into()]);
    assert_eq!(
        resolve_pair(&tgt, &m, &"a".into(), &"c".into()),
        PairResolution::Expand {
            intermediates: vec!["w".into()],
            labels: vec![e("a", "w", "w"), e("w", "c", "c")],
        }
    );
    // Else unresolved, by count.
    let m = keep(&["a", "b", "c", "w"]);
    assert_eq!(
        resolve_pair(&tgt, &m, &"a".into(), &"c".into()),
        PairResolution::Unresolved(UnresolvedReason::Ambiguous { count: 2 })
    );
    assert_eq!(
        resolve_pair(&tgt, &m, &"b".into(), &"a".into()),
        PairResolution::Unresolved(UnresolvedReason::NoEdge)
    );
    // A cyclic expansion path resolves its hops without following another
    // path: a -> w is resolved by the edge, never by w's own path.
    let mut m = keep(&["a", "b", "c", "w"]);
    m.expansion_path.insert(pair("b", "c"), vec!["b".into()]);
    m.expansion_path.insert(pair("b", "b"), vec!["c".into()]);
    assert_eq!(
        resolve_pair(&tgt, &m, &"b".into(), &"c".into()),
        PairResolution::Unresolved(UnresolvedReason::NoEdge)
    );
}

#[test]
fn contraction_types_serde_round_trip() {
    let (src, tgt, m) = wrapper();
    let plan = ContractionPlan::compile(&src, &tgt, &m).unwrap();
    let json = serde_json::to_string(&plan).unwrap();
    assert_eq!(
        serde_json::from_str::<ContractionPlan>(&json).unwrap(),
        plan
    );
    let msgpack = rmp_serde::to_vec_named(&plan).unwrap();
    assert_eq!(
        rmp_serde::from_slice::<ContractionPlan>(&msgpack).unwrap(),
        plan
    );
    let mut cbor = Vec::new();
    ciborium::into_writer(&plan, &mut cbor).unwrap();
    assert_eq!(
        ciborium::from_reader::<ContractionPlan, _>(cbor.as_slice()).unwrap(),
        plan
    );
    for mode in [
        ContractionMode::None,
        ContractionMode::Declared,
        ContractionMode::Closure,
    ] {
        let json = serde_json::to_string(&mode).unwrap();
        assert_eq!(
            serde_json::from_str::<ContractionMode>(&json).unwrap(),
            mode
        );
    }
    assert_eq!(
        serde_json::to_string(&ContractionMode::Declared).unwrap(),
        r#""declared""#
    );
    assert_eq!(
        serde_json::to_string(&PairResolution::Unresolved(UnresolvedReason::Ambiguous {
            count: 2
        }))
        .unwrap(),
        r#"{"unresolved":{"ambiguous":{"count":2}}}"#
    );
}

/// A protocol that declares `record` a table kind and `string` a value kind.
fn table_protocol() -> Protocol {
    Protocol {
        value_kinds: BTreeMap::from([("string".to_owned(), ScalarKind::Str)]),
        non_value_kinds: BTreeSet::from(["record".to_owned()]),
        ..Protocol::default()
    }
}

#[test]
fn restrict_plan_compiles_edge_image_contraction_and_cells() {
    let protocol = table_protocol();
    let src = schema_with(
        &protocol,
        &[("t", "record"), ("t.a", "string"), ("t.b", "string")],
        &[&e("t", "t.a", "a"), &e("t", "t.b", "b")],
    );
    let tgt = schema_with(
        &protocol,
        &[("t", "record"), ("t.a", "string")],
        &[&e("t", "t.a", "renamed")],
    );
    let mut m = keep(&["t", "t.a"]);
    m.edge_remap
        .insert(e("t", "t.a", "a"), e("t", "t.a", "renamed"));
    let protocols = BoundaryProtocols {
        src: &protocol,
        tgt: &protocol,
    };
    let plan = RestrictPlan::compile_with_protocols(&src, &tgt, &m, protocols).unwrap();
    assert_eq!(
        plan.src_schema,
        panproto_inst::identity::schema_digest(&src)
    );
    assert_eq!(
        plan.tgt_schema,
        panproto_inst::identity::schema_digest(&tgt)
    );
    let cells = plan
        .boundary
        .as_ref()
        .and_then(|b| b.cells.as_ref())
        .unwrap();
    assert_eq!(
        cells.fates.get("t"),
        Some(&vec![
            CellFate::Rename("a".into(), "renamed".into()),
            CellFate::Drop("b".into()),
        ])
    );
    assert_eq!(
        cells
            .specs
            .get(&(Name::from("t"), "renamed".to_owned()))
            .map(|s| &s.kind),
        Some(&ScalarKind::Str)
    );
    assert!(
        plan.boundary
            .as_ref()
            .and_then(|b| b.table.as_ref())
            .is_none()
    );
}

#[test]
fn a_protocol_free_plan_has_no_boundary() {
    let (src, tgt, m) = wrapper();
    assert!(
        RestrictPlan::compile(&src, &tgt, &m)
            .unwrap()
            .boundary
            .is_none()
    );
}

#[test]
fn an_unclassified_protocol_leaves_graphs_without_a_cell_layer() {
    let (src, tgt, m) = wrapper();
    let protocol = Protocol::default();
    let protocols = BoundaryProtocols {
        src: &protocol,
        tgt: &protocol,
    };
    let plan = RestrictPlan::compile_with_protocols(&src, &tgt, &m, protocols).unwrap();
    let boundary = plan.boundary.as_ref().unwrap();
    assert!(boundary.cells.is_none());
    assert!(boundary.table.is_none());
}

#[test]
fn table_plan_refusals_come_in_their_stated_order() {
    let protocol = table_protocol();
    let unclassified = Protocol::default();
    let src = schema_with(
        &protocol,
        &[("t", "record"), ("t.a", "string")],
        &[&e("t", "t.a", "a")],
    );
    // The source classifies under no protocol first.
    let error = RestrictPlan::compile_for_tables(
        &src,
        &src,
        &keep(&["t", "t.a"]),
        BoundaryProtocols {
            src: &unclassified,
            tgt: &unclassified,
        },
    )
    .unwrap_err();
    assert!(matches!(error, RestrictError::RelSchema(_)), "{error:?}");
    let protocols = BoundaryProtocols {
        src: &protocol,
        tgt: &protocol,
    };
    // A functor error: a table sent to a column vertex.
    let mut bad = keep(&["t.a"]);
    bad.vertex_remap.insert("t".into(), "t.a".into());
    let error = RestrictPlan::compile_for_tables(&src, &src, &bad, protocols).unwrap_err();
    assert!(
        matches!(error, RestrictError::VertexImageNotATable { .. }),
        "{error:?}"
    );
    // An expansion is not representable on tables.
    let wide = schema_with(
        &protocol,
        &[("t", "record"), ("u", "record"), ("w", "record")],
        &[&e("t", "u", "fk")],
    );
    let path = schema_with(
        &protocol,
        &[("t", "record"), ("u", "record"), ("w", "record")],
        &[&e("t", "w", "fk1"), &e("w", "u", "fk2")],
    );
    let mut expand = keep(&["t", "u", "w"]);
    expand.surviving_edges.insert(e("t", "u", "fk"));
    expand
        .expansion_path
        .insert(("t".into(), "u".into()), vec!["w".into()]);
    let error = RestrictPlan::compile_for_tables(&wide, &path, &expand, protocols).unwrap_err();
    assert!(
        matches!(
            error,
            RestrictError::NotTableRepresentable { .. } | RestrictError::IncoherentEdgeImage { .. }
        ),
        "{error:?}"
    );
}

#[test]
fn an_expanding_migration_is_not_table_representable() {
    let protocol = table_protocol();
    let protocols = BoundaryProtocols {
        src: &protocol,
        tgt: &protocol,
    };
    let src = schema_with(
        &protocol,
        &[("t", "record"), ("u", "record")],
        &[&e("t", "u", "fk")],
    );
    let tgt = schema_with(
        &protocol,
        &[("t", "record"), ("u", "record"), ("w", "record")],
        &[&e("t", "w", "fk1"), &e("w", "u", "fk2")],
    );
    let mut m = keep(&["t", "u", "w"]);
    m.expansion_path
        .insert(("t".into(), "u".into()), vec!["w".into()]);
    let error = RestrictPlan::compile_for_tables(&src, &tgt, &m, protocols).unwrap_err();
    assert!(
        matches!(
            &error,
            RestrictError::NotTableRepresentable { reason } if reason.contains("expands")
        ),
        "{error:?}"
    );
    // A hyper-edge resolution is not representable either.
    let mut hyper = keep(&["t", "u"]);
    hyper.surviving_edges.insert(e("t", "u", "fk"));
    hyper.hyper_resolver.insert(
        ("h".into(), vec![]),
        ("h2".into(), std::collections::HashMap::new()),
    );
    let error = RestrictPlan::compile_for_tables(&src, &src, &hyper, protocols).unwrap_err();
    assert!(
        matches!(
            &error,
            RestrictError::NotTableRepresentable { reason } if reason.contains("hyper")
        ),
        "{error:?}"
    );
}

#[test]
fn a_table_merged_into_an_unmapped_tables_name_is_refused_on_tables() {
    // Probe B: tables a, b, target table b, vertex_remap {a -> b},
    // surviving {b}: the functor drops b (it is an image), the restrict
    // keeps it.
    let protocol = table_protocol();
    let protocols = BoundaryProtocols {
        src: &protocol,
        tgt: &protocol,
    };
    let src = schema_with(&protocol, &[("a", "record"), ("b", "record")], &[]);
    let tgt = schema_with(&protocol, &[("b", "record")], &[]);
    let mut m = keep(&["b"]);
    m.vertex_remap.insert("a".into(), "b".into());
    let error = RestrictPlan::compile_for_tables(&src, &tgt, &m, protocols).unwrap_err();
    assert!(
        matches!(&error, RestrictError::SurvivalRulesDisagree { element }
            if **element == panproto_inst::SurvivalElement::Table("b".into())),
        "{error:?}"
    );
    // The graph restrict keeps the node anchored at b.
    let graph = panproto_inst::GInstance::new()
        .with_node(panproto_inst::Node::new(0, "a"))
        .with_node(panproto_inst::Node::new(1, "b"));
    let plan = RestrictPlan::compile(&src, &tgt, &m).unwrap();
    let (view, _) = panproto_inst::graph_restrict_with_plan(&graph, &plan).unwrap();
    assert!(view.node(1).is_some_and(|n| n.anchor == "b"));
}

#[test]
fn a_relation_carried_only_by_name_is_refused_on_tables() {
    // Probe C: fk a -f-> b, target A -f-> b, vertex_remap {a -> A},
    // surviving {A, b}: the functor drops the relation (not remapped, not
    // listed), the restrict keeps it as A -f-> b.
    let protocol = table_protocol();
    let protocols = BoundaryProtocols {
        src: &protocol,
        tgt: &protocol,
    };
    let f = |s: &str| e(s, "b", "f");
    let src = schema_with(&protocol, &[("a", "record"), ("b", "record")], &[&f("a")]);
    let tgt = schema_with(&protocol, &[("A", "record"), ("b", "record")], &[&f("A")]);
    let mut m = keep(&["A", "b"]);
    m.vertex_remap.insert("a".into(), "A".into());
    let error = RestrictPlan::compile_for_tables(&src, &tgt, &m, protocols).unwrap_err();
    assert!(
        matches!(&error, RestrictError::SurvivalRulesDisagree { element }
            if **element == panproto_inst::SurvivalElement::Edge(f("a"))),
        "{error:?}"
    );
    let plan = RestrictPlan::compile(&src, &tgt, &m).unwrap();
    assert_eq!(plan.edge_image.fate(&f("a")), Some(&EdgeFate::Keep(f("A"))));
}

/// The source edge `r -x-> leaf`, and a target that joins `r` to `leaf`
/// only through a chain of `k` intermediates `m0 .. m(k-1)`, with the
/// migration that keeps both ends and expands the edge along the chain.
fn long_expansion(k: usize) -> (Schema, Schema, CompiledMigration) {
    let src = schema(&["r", "leaf"], &[&e("r", "leaf", "x")]);
    let names: Vec<String> = (0..k).map(|i| format!("m{i}")).collect();
    let mut anchors: Vec<&str> = vec!["r"];
    anchors.extend(names.iter().map(String::as_str));
    anchors.push("leaf");
    let mut builder = SchemaBuilder::new(&Protocol::default());
    for v in &anchors {
        builder = builder.vertex(v, "record", None).unwrap();
    }
    for hop in anchors.windows(2) {
        builder = builder.edge(hop[0], hop[1], "prop", Some("h")).unwrap();
    }
    let tgt = builder.build().unwrap();
    let mut m = keep(&["r", "leaf"]);
    m.expansion_path.insert(
        pair("r", "leaf"),
        names.iter().map(|n| Name::from(n.as_str())).collect(),
    );
    (src, tgt, m)
}

#[test]
fn an_expansion_longer_than_a_synthesized_key_can_number_is_refused() {
    // A synthesized key numbers an expansion's hops with 16 bits: 65536
    // labels (65535 intermediates) fit, 65537 do not.
    let (src, tgt, m) = long_expansion(65_535);
    let image = EdgeImage::compile(&src, &tgt, &m).unwrap();
    assert!(matches!(
        image.fate(&e("r", "leaf", "x")),
        Some(EdgeFate::Expand { labels, .. }) if labels.len() == 65_536
    ));
    let (src, tgt, m) = long_expansion(65_536);
    let error = RestrictPlan::compile(&src, &tgt, &m).unwrap_err();
    assert!(
        matches!(
            &error,
            RestrictError::ExpansionTooLong { src, tgt, hops }
                if src == "r" && tgt == "leaf" && *hops == 65_537
        ),
        "{error:?}"
    );
}

#[test]
fn an_engine_refuses_an_expansion_too_long_to_number() {
    // A plan whose fields were changed after compilation to hold a longer
    // expansion is refused by the engines too, at the edge, rather than
    // numbered wrongly.
    let (src, tgt, m) = long_expansion(1);
    let mut plan = RestrictPlan::compile(&src, &tgt, &m).unwrap();
    let hop = e("r", "m0", "h");
    plan.edge_image.fates.insert(
        e("r", "leaf", "x"),
        EdgeFate::Expand {
            intermediates: vec![Name::from("m0"); 65_536],
            labels: vec![hop; 65_537],
        },
    );
    let tree = panproto_inst::WInstance::new(
        [
            (0, panproto_inst::Node::new(0, "r")),
            (1, panproto_inst::Node::new(1, "leaf")),
        ]
        .into(),
        vec![(0, 1, e("r", "leaf", "x"))],
        vec![],
        0,
        "r".into(),
    );
    let too_long = |error: &RestrictError| {
        matches!(
            error.unlocated(),
            RestrictError::ExpansionTooLong { hops: 65_537, .. }
        )
    };
    let tree_error = panproto_inst::wtype_restrict_with_plan(&tree, &plan).unwrap_err();
    assert!(too_long(&tree_error), "{tree_error:?}");
    assert_eq!(tree_error.key(), None);
    let graph = panproto_inst::GInstance::from_wtype(&tree).unwrap();
    let graph_error = panproto_inst::graph_restrict_with_plan(&graph, &plan).unwrap_err();
    assert!(too_long(&graph_error), "{graph_error:?}");
    assert_eq!(graph_error.key().map(|k| (k.stage, k.id)), Some((1, 0)));
}
