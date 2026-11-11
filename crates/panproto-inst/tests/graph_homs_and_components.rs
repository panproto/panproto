#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Graph homomorphisms carry an edge map and check it; the components of a
//! graph come out in one order in every process; a graph restrict's
//! provenance and fibers are read off its complement, and fibers may
//! overlap where a tree's cannot.

use std::collections::{BTreeMap, HashMap};
use std::process::Command;

use panproto_gat::Name;
use panproto_inst::{
    CompiledMigration, EdgeId, GInstance, GInstanceHom, HomError, Node, RestrictPlan,
    graph_components, graph_fiber_at_node, graph_group_by, graph_provenance,
    graph_restrict_with_plan, strongly_connected_components,
};
use panproto_schema::{Edge, Protocol, Schema, SchemaBuilder};

fn schema(vertices: &[&str], edges: &[&Edge]) -> Schema {
    let protocol = Protocol::default();
    let mut builder = SchemaBuilder::new(&protocol);
    for v in vertices {
        builder = builder.vertex(v, "record", None).unwrap();
    }
    for edge in edges {
        builder = builder
            .edge(&edge.src, &edge.tgt, &edge.kind, edge.name.as_deref())
            .unwrap();
    }
    builder.build().unwrap()
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

/// Two `r` nodes joined by two parallel `x` edges (ids 0 and 1).
fn parallel() -> GInstance {
    GInstance::new()
        .with_node(Node::new(0, "r"))
        .with_node(Node::new(1, "r"))
        .with_edge(0, 1, e("r", "r", "x"))
        .with_edge(0, 1, e("r", "r", "x"))
}

#[test]
fn graph_homs_compose_and_check_edges() {
    let g = parallel();
    assert_eq!(g.edges().len(), 2);
    let id = GInstanceHom::identity(&g);
    assert_eq!(id.check(&g, &g), Ok(()));

    // Swapping the parallel edges is a homomorphism the node map alone could
    // not tell from the identity.
    let swap = GInstanceHom {
        node_map: id.node_map.clone(),
        edge_map: BTreeMap::from([(EdgeId(0), EdgeId(1)), (EdgeId(1), EdgeId(0))]),
    };
    assert_eq!(swap.check(&g, &g), Ok(()));
    assert_ne!(swap, id);
    assert_eq!(swap.compose(&swap).unwrap(), id);
    assert_eq!(id.compose(&swap).unwrap(), swap);
    assert_eq!(swap.compose(&id).unwrap(), swap);

    // A missing edge image is refused.
    let partial = GInstanceHom {
        node_map: id.node_map.clone(),
        edge_map: BTreeMap::from([(EdgeId(0), EdgeId(0))]),
    };
    assert_eq!(
        partial.check(&g, &g),
        Err(HomError::EdgeNotMapped(EdgeId(1)))
    );

    // An edge sent to an edge that does not join its endpoints' images is
    // refused: collapsing both nodes onto 0 leaves no loop at 0.
    let collapse = GInstanceHom {
        node_map: BTreeMap::from([(0, 0), (1, 0)]),
        edge_map: id.edge_map.clone(),
    };
    assert_eq!(
        collapse.check(&g, &g),
        Err(HomError::EdgeNotPreserved {
            src: EdgeId(0),
            tgt: EdgeId(0)
        })
    );

    // An edge image outside the codomain is refused.
    let dangling = GInstanceHom {
        node_map: id.node_map.clone(),
        edge_map: BTreeMap::from([(EdgeId(0), EdgeId(0)), (EdgeId(1), EdgeId(7))]),
    };
    assert_eq!(
        dangling.check(&g, &g),
        Err(HomError::EdgeNotPreserved {
            src: EdgeId(1),
            tgt: EdgeId(7)
        })
    );

    // Composing through an edge the second map does not send anywhere is
    // refused.
    assert_eq!(
        dangling.compose(&id),
        Err(HomError::ComposeEdgeMismatch(EdgeId(7)))
    );
}

/// A graph with two cycles and a tail, built from a `HashMap` so that its
/// iteration order varies with the process's hash seed.
fn components_answer() -> String {
    let arcs: HashMap<u32, Vec<u32>> = HashMap::from([
        (5, vec![6]),
        (6, vec![5, 2]),
        (2, vec![3]),
        (3, vec![1]),
        (1, vec![2, 4]),
        (4, vec![]),
        (0, vec![0]),
    ]);
    let pairs: Vec<(u32, u32)> = arcs
        .iter()
        .flat_map(|(&u, vs)| vs.iter().map(move |&v| (u, v)))
        .collect();
    let mut g = GInstance::new();
    for &id in arcs.keys() {
        g = g.with_node(Node::new(id, "r"));
    }
    for &(u, v) in &pairs {
        g = g.with_edge(u, v, e("r", "r", "x"));
    }
    let direct = strongly_connected_components(arcs.keys().copied(), pairs.iter().copied());
    assert_eq!(graph_components(&g), direct);
    format!("{direct:?}")
}

#[test]
fn one_process_components() {
    if std::env::var_os("PP_SCC_DUMP").is_none() {
        return;
    }
    print!("<<<{}>>>", components_answer());
}

#[test]
fn scc_order_is_deterministic() {
    if std::env::var_os("PP_SCC_DUMP").is_some() {
        return;
    }
    let expected = "[[0], [1, 2, 3], [4], [5, 6]]";
    assert_eq!(components_answer(), expected);
    let exe = std::env::current_exe().unwrap();
    for _ in 0..16 {
        let output = Command::new(&exe)
            .args(["one_process_components", "--exact", "--nocapture"])
            .env("PP_SCC_DUMP", "1")
            .output()
            .unwrap();
        let text = String::from_utf8_lossy(&output.stdout).into_owned();
        let start = text.find("<<<").expect("the child printed its answer");
        let end = text.find(">>>").expect("the child closed its answer");
        assert_eq!(&text[start + 3..end], expected);
    }
}

/// `r -m-> m -leaf-> leaf` with `m` dropped: two `r` nodes share one `m`.
#[test]
fn graph_fibers_may_overlap() {
    let src = schema(
        &["r", "m", "leaf"],
        &[&e("r", "m", "m"), &e("m", "leaf", "leaf")],
    );
    let tgt = schema(&["r", "leaf"], &[&e("r", "leaf", "leaf")]);
    let m = keep(&["r", "leaf"]);
    let g = GInstance::new()
        .with_node(Node::new(0, "r"))
        .with_node(Node::new(1, "m"))
        .with_node(Node::new(2, "leaf"))
        .with_node(Node::new(3, "r"))
        .with_edge(0, 1, e("r", "m", "m"))
        .with_edge(3, 1, e("r", "m", "m"))
        .with_edge(1, 2, e("m", "leaf", "leaf"));
    let plan = RestrictPlan::compile(&src, &tgt, &m).unwrap();
    let (view, complement) = graph_restrict_with_plan(&g, &plan).unwrap();

    // The dropped `m` lies in the fiber of both survivors that reach it.
    assert_eq!(graph_fiber_at_node(&g, &complement, 0), vec![1]);
    assert_eq!(graph_fiber_at_node(&g, &complement, 3), vec![1]);
    assert_eq!(graph_fiber_at_node(&g, &complement, 2), Vec::<u32>::new());
    // A dropped node has no fiber of its own.
    assert_eq!(graph_fiber_at_node(&g, &complement, 1), Vec::<u32>::new());

    // Kept nodes are their own source.
    let provenance = graph_provenance(&g, &view, &complement);
    let mut keys: Vec<u32> = provenance.keys().copied().collect();
    keys.sort_unstable();
    assert_eq!(keys, vec![0, 2, 3]);
    for (&id, p) in &provenance {
        assert_eq!(
            p.source_fields
                .iter()
                .map(|f| f.node_id)
                .collect::<Vec<_>>(),
            vec![id]
        );
    }

    // Grouping by view anchor partitions the surviving source nodes.
    let groups = graph_group_by(&m, &g);
    assert_eq!(
        groups,
        BTreeMap::from([(Name::from("leaf"), vec![2]), (Name::from("r"), vec![0, 3])])
    );
}

#[test]
fn an_expanded_node_comes_from_its_source_edge_endpoints() {
    let src = schema(&["r", "c"], &[&e("r", "c", "c")]);
    let tgt = schema(&["r", "c", "u"], &[&e("r", "u", "u"), &e("u", "c", "c")]);
    let mut m = keep(&["r", "c", "u"]);
    m.expansion_path
        .insert(("r".into(), "c".into()), vec!["u".into()]);
    let g = GInstance::new()
        .with_node(Node::new(0, "r"))
        .with_node(Node::new(1, "c"))
        .with_edge(0, 1, e("r", "c", "c"));
    let plan = RestrictPlan::compile(&src, &tgt, &m).unwrap();
    let (view, complement) = graph_restrict_with_plan(&g, &plan).unwrap();
    let provenance = graph_provenance(&g, &view, &complement);
    let synthesized: Vec<u32> = complement.synthesized_nodes.keys().copied().collect();
    assert_eq!(synthesized.len(), 1);
    let sources: Vec<u32> = provenance[&synthesized[0]]
        .source_fields
        .iter()
        .map(|f| f.node_id)
        .collect();
    assert_eq!(sources, vec![0, 1]);
}
